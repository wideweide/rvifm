use super::*;
use std::fs;

/// M34-4：查看器内容渲染（M17 VIEW-1 文本 + 图片/目录/PDF/Office 分流）——
/// 对侧 pane 复用：render_right_pane（焦点左）与 render()（焦点右）分派到
/// 非焦点 pane。预览目标 = preview_path()（M34-4 起焦点感知）。
/// M59-5：快速预览行数——至少 36 行，且不低于视口高度 +10（保证充满预览区）。
pub(crate) fn quick_preview_line_count(height: usize) -> usize {
    36.max(height.saturating_add(10))
}

pub(crate) fn render_view_content(
    frame: &mut Frame,
    state: &AppState,
    area: ratatui::layout::Rect,
) {
    let tab = state.active_tab();
    // M52-1：`:view` 恒对侧预览，目标取焦点栏光标文件（M34-4 焦点感知语义）。
    // M53-3：归档成员走临时只读副本（由事件循环按光标解出）。
    // M59-6：查看器已提交的路径优先——避免 150ms 防抖期间用新路径渲染旧
    // `ViewerState`（典型症状：从二进制切到 Markdown 先闪出 hex）。
    let path = if tab.view_mode && tab.viewer.is_some() {
        tab.viewer.as_ref().and_then(|v| v.path.clone())
    } else {
        None
    }
    .or_else(|| {
        tab.preview_path()
            .or_else(|| tab.archive_preview_tmp.clone())
    });
    let Some(path) = path else {
        let pane = ratatui::widgets::Block::bordered()
            .title("view · 预览")
            .border_style(pane_border_style(true));
        let inner = pane.inner(area);
        frame.render_widget(pane, area);
        frame.render_widget(Paragraph::new("归档内预览暂不支持"), inner);
        return;
    };
    render_view_path(frame, state, &path, area, tab.viewer.as_ref());
}

/// FIX-8：按给定路径复用 `:view` 的图片、Office/PDF、目录、语法高亮与
/// 二进制信息面板渲染，供 frecency 对侧预览使用。
pub(crate) fn render_view_path(
    frame: &mut Frame,
    state: &AppState,
    path: &std::path::Path,
    area: ratatui::layout::Rect,
    viewer_state: Option<&crate::tab::ViewerState>,
) {
    let tab = state.active_tab();
    // M49-9/10：快速切换时只做轻量预览；预览获焦（Tab 进入）后才全量加载。
    let quick = !tab.preview_focus && tab.pane_frecency.as_ref().is_none_or(|p| !p.preview_focus);
    let path = path.to_path_buf();
    // 图片：快速态只显示元数据；获焦后仍走 ImageEngine 全量渲染。
    if matches!(
        crate::preview::preview_kind(&path),
        crate::preview::PreviewKind::Image
    ) {
        if quick {
            let pane = ratatui::widgets::Block::bordered()
                .title(format!(
                    "{} · 图片（快速预览）",
                    path.display().to_string().replace('\\', "/")
                ))
                .border_style(pane_border_style(true));
            let inner = pane.inner(area);
            frame.render_widget(pane, area);
            let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from("图片预览（快速模式：仅元数据）"),
                    Line::from(format!("大小: {} 字节", size)),
                    Line::from("按 Tab 进入预览加载完整图像"),
                ]),
                inner,
            );
            return;
        }
        let pane = ratatui::widgets::Block::bordered()
            .title(format!(
                "{} · 图片预览",
                path.display().to_string().replace('\\', "/")
            ))
            .border_style(pane_border_style(true));
        let inner = pane.inner(area);
        frame.render_widget(pane, area);
        if state.image_engine.render_file(frame, inner, &path).is_err() {
            frame.render_widget(Paragraph::new("图片渲染失败（损坏/超大）"), inner);
        }
        return;
    }
    // M21-F6：光标在目录上 → 浅层 tree（复用 M13-7 构建器，深度 2、
    // 只读快照）——不走文本读取路径，杜绝"无法读取/非 UTF-8"误报。
    if path.is_dir() {
        let pane = ratatui::widgets::Block::bordered()
            .title(format!(
                "{} · 目录",
                path.display().to_string().replace('\\', "/")
            ))
            .border_style(pane_border_style(true));
        let inner = pane.inner(area);
        frame.render_widget(pane, area);
        let items = match crate::tab::Tab::build_tree_entries(&path, 2) {
            Ok(items) => items,
            Err(e) => {
                frame.render_widget(Paragraph::new(format!("目录读取失败: {e}")), inner);
                return;
            }
        };
        let height = inner.height as usize;
        let mut lines: Vec<Line> = Vec::with_capacity(height);
        if items.is_empty() {
            lines.push(Line::from("（空目录）"));
        }
        for item in items.iter().take(height) {
            let depth = item.name.matches('/').count();
            let leaf = item
                .name
                .rsplit('/')
                .next()
                .unwrap_or(&item.name)
                .to_owned();
            // M47-1：缩进放在图标之前（原实现只缩进文件名，图标全部贴左边缘）。
            let indent = "  ".repeat(depth + 1);
            let label = if item.is_dir {
                format!("{leaf}/")
            } else {
                leaf
            };
            let spans = vec![
                Span::from(format!("{indent}{} ", item.icon.glyph)),
                Span::styled(label, filetype_style(item.filetype_class())),
            ];
            lines.push(Line::from(spans));
        }
        frame.render_widget(Paragraph::new(lines), inner);
        return;
    }
    let viewer = viewer_state.cloned().unwrap_or_default();
    let shown = path.display().to_string().replace('\\', "/");
    // M21-E3：大文件截断提示前置（路径长会被 pane 宽度裁掉，放行尾不可见）。
    const VIEWER_MAX_LINES: usize = 100_000;
    let trunc_hint = if viewer.total_lines > viewer.line_count {
        format!("〔已截断 共 {} 行〕", viewer.total_lines)
    } else {
        String::new()
    };
    let quick_mark = if quick { "快速 " } else { "" };
    let title = if viewer_state.is_none() {
        format!("{quick_mark}{shown} · 预览")
    } else {
        match &viewer.query {
            Some(q) => format!(
                "{quick_mark}{trunc_hint}{shown} · 行 {}/{}  /{}（n/N 跳转）",
                viewer.scroll + 1,
                viewer.line_count,
                q
            ),
            None => format!(
                "{quick_mark}{trunc_hint}{shown} · 行 {}/{}（Tab 进入 / w/Esc 退出 / 搜索）",
                viewer.scroll + 1,
                viewer.line_count
            ),
        }
    };
    let pane = ratatui::widgets::Block::bordered()
        .title(title)
        .border_style(pane_border_style(true));
    let inner = pane.inner(area);
    frame.render_widget(pane, area);
    // M21-F7：PDF/Office 分流——走共享 PreviewRenderer（pdf-inspector
    // 文本提取先行 / office_oxide 提取），失败按类型提示；不落文本读取
    // 通用路径（杜绝"无法读取（非 UTF-8 或已删除）"误报）。
    match crate::preview::preview_kind(&path) {
        crate::preview::PreviewKind::Office => {
            let pane = ratatui::widgets::Block::bordered()
                .title(format!("{shown} · Office 预览"))
                .border_style(pane_border_style(true));
            let inner = pane.inner(area);
            frame.render_widget(pane, area);
            let preview = crate::preview::render_preview(&path, inner.height.max(1) as usize);
            frame.render_widget(preview.into_paragraph(), inner);
            return;
        }
        crate::preview::PreviewKind::Archive => {
            // M45-4b + M46-8：归档成员列表（Office/PDF 已在 preview_kind 中
            // 优先命中）——缓存版同源渲染，损坏/.rar 回退信息面板。
            let pane = ratatui::widgets::Block::bordered()
                .title(format!("{shown} · 归档成员（只读）"))
                .border_style(pane_border_style(true));
            let inner = pane.inner(area);
            frame.render_widget(pane, area);
            // M46-8：按 (路径, mtime, size) 缓存成员列表，同一归档预览不再
            // 每帧重复打开/全量解压（firefox tarball 等 xz/bz2 秒出提示）。
            let preview = crate::preview::archive_preview_cached_with_config(
                &path,
                inner.height.max(1) as usize,
                &state.archive_preview_cache,
                state.config.preview.archive_external_listing,
            );
            frame.render_widget(preview.into_paragraph(), inner);
            return;
        }
        crate::preview::PreviewKind::Pdf => {
            // AUDIT-2 + M27-3：`[preview] pdf_image_pages`（0 关）与
            // `pdf_prefer_image` 门控；M39-2：查看器改为**单页展示 + `[`/`]`
            // 翻页（任意页，按需渲染缓存）——不再 1–2 页网格。
            let pages_cfg = state.config.preview.pdf_image_pages.min(2);
            let prefer = state.config.preview.pdf_prefer_image;
            if pages_cfg > 0 && !quick {
                let page = viewer.page;
                let pane = ratatui::widgets::Block::bordered()
                    .title(format!("{shown} · PDF 第 {} 页（[ ] 翻页）", page + 1))
                    .border_style(pane_border_style(true));
                let inner = pane.inner(area);
                frame.render_widget(pane, area);
                if let Some(png) = crate::pdf::cached_visual_page_png(&path, page, prefer) {
                    if state.image_engine.render_file(frame, inner, &png).is_err() {
                        frame.render_widget(
                            Paragraph::new("PDF 图像渲染失败（协议探测失败或解码错误）"),
                            inner,
                        );
                    }
                } else {
                    // 视觉不可用（文本层足够且无 prefer / 渲染失败）→ 文本回退。
                    let preview =
                        crate::preview::render_preview(&path, inner.height.max(1) as usize);
                    frame.render_widget(preview.into_paragraph(), inner);
                }
                return;
            }
            let pane = ratatui::widgets::Block::bordered()
                .title(format!("{shown} · PDF 预览"))
                .border_style(pane_border_style(true));
            let inner = pane.inner(area);
            frame.render_widget(pane, area);
            let preview = crate::preview::render_preview(&path, inner.height.max(1) as usize);
            frame.render_widget(preview.into_paragraph(), inner);
            return;
        }
        _ => {}
    }
    // 内容行：行号 gutter + 查询匹配高亮；视口取 viewer.scroll 起。
    // M39-1：hex 模式——二进制文件 16 字节/行（分块读取；无搜索/高亮）。
    if viewer.hex {
        let height = inner.height as usize;
        let lines = tab.viewer_hex_lines(height).unwrap_or_default();
        let out: Vec<ratatui::text::Line> =
            lines.into_iter().map(ratatui::text::Line::from).collect();
        frame.render_widget(Paragraph::new(out), inner);
        return;
    }
    let height = inner.height as usize;
    let width = inner.width as usize;
    // M53-5：按 (path, mtime, size) 缓存行，避免快速 j/k 每帧重读/重解析。
    let meta = std::fs::metadata(&path).ok();
    let key = (
        path.clone(),
        meta.as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0),
        meta.as_ref().map(|m| m.len()).unwrap_or(0),
    );
    let quick_n = quick_preview_line_count(height);
    // M59-6：命中缓存只克隆当前需要的行（旧实现克隆最多 10 万行，小文件也会卡）。
    let needed = if quick {
        quick_n
    } else {
        viewer.scroll.saturating_add(height)
    };
    let cached: Option<Vec<String>> =
        state
            .text_preview_cache
            .try_borrow()
            .ok()
            .and_then(|cache| {
                cache
                    .get(&key)
                    .map(|lines| lines.iter().take(needed).cloned().collect())
            });
    let all_lines: Vec<String> = if let Some(hit) = cached {
        hit
    } else {
        let Ok(text) = std::fs::read_to_string(&path) else {
            frame.render_widget(Paragraph::new(crate::preview::file_info_text(&path)), inner);
            return;
        };
        // NUL 即判二进制（M45-4b）。
        if text.contains('\0') {
            frame.render_widget(Paragraph::new(crate::preview::file_info_text(&path)), inner);
            return;
        }
        let parsed: Vec<String> = text
            .lines()
            .take(VIEWER_MAX_LINES)
            .map(str::to_owned)
            .collect();
        if let Ok(mut cache) = state.text_preview_cache.try_borrow_mut() {
            if cache.len() >= 16 {
                cache.clear();
            }
            cache.insert(key, parsed.clone());
        }
        parsed
    };
    // M53-5：快速态截取至少 36 行且充满预览区（替代 M49-9 的 300 字）。
    // M59-5：至少 36 行，且按视口实际高度 +10 行扩展，保证充满预览区。
    let lines: Vec<String> = if quick && all_lines.len() > quick_n {
        let n = quick_n;
        let mut short: Vec<String> = all_lines.iter().take(n).cloned().collect();
        short.push("…（快速预览仅前若干行，Tab 进入预览加载完整内容）".to_owned());
        short
    } else {
        all_lines
    };
    // M21-E3：语法高亮——扩展名/shebang 推断，未命中或搜索模式下
    // 降级纯文本（查询黄底叠加与语法着色互斥，SIMPLIFIED）。
    let dark = crate::theme::active_variant() != opaline::schema::ThemeVariant::Light;
    let mut highlighter = if viewer.query.is_none() {
        crate::highlight::Highlighter::for_path(
            &path,
            lines.first().map(String::as_str).unwrap_or(""),
            dark,
        )
    } else {
        None
    };
    let mut out: Vec<Line> = Vec::new();
    for (i, line) in lines.iter().skip(viewer.scroll).take(height).enumerate() {
        let lineno = viewer.scroll + i + 1;
        let is_current = viewer.query.is_some() && viewer.scroll + i == viewer.match_line;
        let gutter = if is_current {
            format!("{lineno:>4} ▶ ")
        } else {
            format!("{lineno:>4} │ ")
        };
        let mut spans = vec![Span::styled(
            gutter,
            Style::new().add_modifier(Modifier::DIM),
        )];
        // 行内容按查询子串分段高亮（超宽截断由渲染器裁剪）。
        let display: String = line.chars().take(width).collect();
        match viewer
            .query
            .as_deref()
            .and_then(|q| crate::tab::match_range(&display, q))
        {
            Some((start, end)) => {
                if start > 0 {
                    spans.push(Span::from(display[..start].to_owned()));
                }
                spans.push(Span::styled(
                    display[start..end].to_owned(),
                    Style::new().fg(Color::Black).bg(Color::LightYellow),
                ));
                spans.push(Span::from(display[end..].to_owned()));
            }
            None => {
                if let Some(h) = highlighter.as_mut() {
                    // 语法着色片段（宽超界由渲染器裁剪；解析失败降级纯文本）。
                    let toks = h.highlight_line(line.as_str());
                    if toks.is_empty() {
                        spans.push(Span::from(display));
                    } else {
                        for (tok, st) in toks {
                            spans.push(Span::styled(tok, st));
                        }
                    }
                } else {
                    spans.push(Span::from(display));
                }
            }
        }
        out.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(out), inner);
}

/// 右 pane 渲染（M14-1；M21-F1 带路径标题与焦点边框）：M34-4 起预览渲染在
/// 非焦点 pane——焦点左 + view 模式时此函数渲染查看器/图片预览（M13-9 现状），
/// 焦点右时保持右 pane 独立目录列表（查看器由 render() 分派到左 pane）。
pub(crate) fn render_right_pane(
    frame: &mut Frame,
    state: &AppState,
    mode: &InputMode,
    area: ratatui::layout::Rect,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let tab = state.active_tab();
    let focused = tab.focus == crate::tab::Focus::Right;
    // M34-4：预览渲染在非焦点 pane——焦点左时查看器占右 pane（M13-9 现状）；
    // 焦点右时右 pane 保持目录列表导航，查看器由左侧渲染（render() 分派）。
    // M39-6：PreviewRight 布局——查看器恒在右侧（不看焦点）。
    let layout = state.config.ui.layout();
    if tab.view_mode
        && (layout == crate::config::LayoutMode::PreviewRight
            || (layout == crate::config::LayoutMode::Dual && tab.focus == crate::tab::Focus::Left))
    {
        render_view_content(frame, state, area);
        return;
    }

    // 默认：右 pane 目录列表（独立导航；M21-F1 路径标题 + 类型着色）。
    // M27-2：vifm 模式（pane_borders=false）——顶部路径行（TopLine/
    // TopLineSel 背景色区分焦点），无整框边框。
    let borders = state.config.ui.pane_borders;
    let inner = if borders {
        let pane = ratatui::widgets::Block::bordered()
            .title(crate::app::render::path_line_text(
                tab.right.location.display_path().replace('\\', "/"),
                tab.right.filter.as_deref(),
            ))
            .border_style(pane_border_style(focused));
        let inner = pane.inner(area);
        frame.render_widget(pane, area);
        inner
    } else {
        let [topline, rest] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
        let path_style = if focused {
            vifm_style("TopLineSel")
        } else {
            vifm_style("TopLine")
        };
        frame.render_widget(
            Paragraph::new(Line::from(crate::app::render::path_line_text(
                tab.right.location.display_path().replace('\\', "/"),
                tab.right.filter.as_deref(),
            )))
            .style(path_style),
            topline,
        );
        rest
    };
    let parent_available = tab.right.has_parent_row();
    let list_area = if parent_available {
        let [parent, rest] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);
        let style = if tab.right.on_parent_row && focused {
            Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            Style::new().add_modifier(Modifier::DIM)
        };
        frame.render_widget(Paragraph::new(Line::from("../")).style(style), parent);
        rest
    } else {
        inner
    };
    // M34-3：右 pane 搜索高亮——焦点在右时生效（搜索输入期用实时输入，
    // 其余用已提交 tab.search；与左 pane FIX-1 同款式）。
    let active_query: Option<String> = if tab.focus != crate::tab::Focus::Right {
        None
    } else {
        match mode {
            InputMode::Search { input, .. } => {
                let q = input.trim();
                (!q.is_empty()).then(|| q.to_owned())
            }
            _ => tab
                .search
                .as_ref()
                .map(|s| s.query.clone())
                .filter(|q| !q.is_empty()),
        }
    };
    // M47-8：右栏选择状态独立——不再读 Tab 级（左栏）visual/tags。
    let visual_range = tab
        .right
        .visual
        .map(|anchor| (anchor.min(tab.right.cursor), anchor.max(tab.right.cursor)));
    let items: Vec<ListItem> = tab
        .right
        .entries
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            // M21-E1：属性列自适应（与左 pane 同口径）。
            let show_attrs =
                state.config.ui.attrs_column && list_area.width as usize >= MIN_ATTRS_INNER_WIDTH;
            // M25-5：git 状态列（与左 pane 同口径；右 pane 独立目录缓存）。
            let git_col = show_attrs && state.config.ui.git_column;
            let git_map = if git_col {
                match &tab.right.location {
                    Location::Physical(p) => state.git_cache.get(p).and_then(|o| o.as_ref()),
                    _ => None,
                }
            } else {
                None
            };
            let name_budget = if show_attrs {
                (list_area.width as usize).saturating_sub(
                    crate::app::render::LIST_LEADING_BLANK.len()
                        + 2
                        + ATTRS_WIDTH
                        + 2
                        + if git_col { GIT_STATUS_WIDTH } else { 0 },
                )
            } else {
                (list_area.width as usize)
                    .saturating_sub(4 + crate::app::render::LIST_LEADING_BLANK.len())
            };
            let display_name = clip_name(&item.name, name_budget);
            // M46-3：与左 pane M45-2a 同款守卫——当前行/可视选择行去除文件
            // 类型前景色，避免反显行上"文字部分单独着色"（整行背景一致）。
            let is_visual = visual_range.is_some_and(|(lo, hi)| idx >= lo && idx <= hi);
            let row_reversed = (!tab.right.on_parent_row && idx == tab.right.cursor) || is_visual;
            let ft = if row_reversed {
                Style::new()
            } else {
                filetype_style(item.filetype_class())
            };
            let mut spans = vec![
                Span::from(crate::app::render::LIST_LEADING_BLANK),
                Span::from(format!("{} ", item.icon.glyph)),
            ];
            match active_query
                .as_deref()
                .and_then(|query| crate::tab::match_range(&display_name, query))
            {
                // FIX-1 同款：匹配子串黄底黑字（M34-3：右 pane 同等高亮）。
                Some((start, end)) => {
                    if !display_name[..start].is_empty() {
                        spans.push(Span::styled(display_name[..start].to_owned(), ft));
                    }
                    spans.push(Span::styled(
                        display_name[start..end].to_owned(),
                        Style::new().fg(Color::Black).bg(Color::LightYellow),
                    ));
                    let after = &display_name[end..];
                    let suffix = if item.is_dir {
                        format!("{after}/")
                    } else {
                        after.to_owned()
                    };
                    if !suffix.is_empty() {
                        spans.push(Span::styled(suffix, ft));
                    }
                }
                None => {
                    let label = if item.is_dir {
                        format!("{display_name}/")
                    } else {
                        display_name.clone()
                    };
                    spans.push(Span::styled(label, ft));
                }
            }
            if show_attrs {
                let name_width = display_name.chars().count() + (item.is_dir as usize);
                let pad = name_budget.saturating_sub(name_width).max(1);
                spans.push(Span::from(" ".repeat(pad)));
                // M25-5：git 状态字符列（占位空格保持列对齐）。
                match git_map.and_then(|m| m.entries.get(&item.name)) {
                    Some(st) => spans.push(git_status_span(*st)),
                    None => spans.push(Span::from(" ".repeat(GIT_STATUS_WIDTH))),
                }
                let size = if item.is_dir {
                    "<DIR>".to_owned()
                } else {
                    crate::tab::format_size(item.size)
                };
                let attr = format!("{:>10}  {}", size, crate::tab::format_mtime_min(item.mtime));
                spans.push(Span::styled(attr, Style::new().add_modifier(Modifier::DIM)));
            }
            let mut li = ListItem::new(Line::from(spans));
            let mut modifiers = Modifier::empty();
            // M47-7：复用 row_reversed（含 !on_parent_row 守卫）——顶部 `..` 行
            // 高亮时第一条真实条目不得同时反显（原无条件 idx==cursor）。
            if row_reversed {
                // M21-F2：焦点侧反显，非焦点侧 DIM 灰化。
                modifiers |= if focused {
                    Modifier::REVERSED
                } else {
                    Modifier::DIM
                };
            }
            if tab.right.tags.contains(&item.name) {
                modifiers |= Modifier::BOLD;
            }
            if modifiers != Modifier::empty() {
                li = li.style(Style::new().add_modifier(modifiers));
            }
            li
        })
        .collect();
    // M41-4：偏移有状态（Pane.scroll，每帧 sync_viewport_scroll 落盘）；
    // 不再依赖 ListState 从 0 起的自动调整（会把光标钉在视口底行）。
    let mut list_state = ListState::default().with_offset(tab.right.scroll);
    if !tab.right.on_parent_row {
        list_state = list_state.with_selected(Some(tab.right.cursor));
    }
    frame.render_stateful_widget(
        ratatui::widgets::List::new(items),
        list_area,
        &mut list_state,
    );
}
