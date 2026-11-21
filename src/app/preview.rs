use super::*;

/// M27-3：pages=2 但未进入两页布局时，底部 1 行可读说明的文案。
/// - `pages < 2`：无说明（None）；
/// - 两页均已生成（`pngs_len >= 2`）→ 预览区高度不足，仅显示第 1 页；
/// - 仅生成第 1 页 → 第 2 页未生成（文档仅 1 页或渲染失败）。
pub(crate) fn pdf_overflow_note(pages: u32, pngs_len: usize) -> Option<&'static str> {
    if pages < 2 {
        return None;
    }
    if pngs_len >= 2 {
        Some("（预览区高度不足，仅显示第 1 页）")
    } else {
        Some("（第 2 页未生成：文档仅 1 页或第 2 页渲染失败）")
    }
}

/// M34-4：查看器内容渲染（M17 VIEW-1 文本 + 图片/目录/PDF/Office 分流）——
/// 对侧 pane 复用：render_right_pane（焦点左）与 render()（焦点右）分派到
/// 非焦点 pane。预览目标 = preview_path()（M34-4 起焦点感知）。
pub(crate) fn render_view_content(
    frame: &mut Frame,
    state: &AppState,
    area: ratatui::layout::Rect,
) {
    let tab = state.active_tab();
    // M17 SRCH：预览源抽象——结果模式取命中文件，物理视图取 base.join。
    let Some(path) = tab.preview_path() else {
        let pane = ratatui::widgets::Block::bordered()
            .title("view · 预览")
            .border_style(pane_border_style(true));
        let inner = pane.inner(area);
        frame.render_widget(pane, area);
        frame.render_widget(Paragraph::new("归档内预览暂不支持"), inner);
        return;
    };
    // 图片仍走 ImageEngine（查看器针对文本）。
    if matches!(
        crate::preview::preview_kind(&path),
        crate::preview::PreviewKind::Image
    ) {
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
            let label = if item.is_dir {
                format!("{}{}/", "  ".repeat(depth), leaf)
            } else {
                format!("{}{}", "  ".repeat(depth), leaf)
            };
            let spans = vec![
                Span::from(format!("{} ", item.icon.glyph)),
                Span::styled(label, filetype_style(item.filetype_class())),
            ];
            lines.push(Line::from(spans));
        }
        frame.render_widget(Paragraph::new(lines), inner);
        return;
    }
    let viewer = tab.viewer.clone().unwrap_or_default();
    let shown = path.display().to_string().replace('\\', "/");
    // M21-E3：大文件截断提示前置（路径长会被 pane 宽度裁掉，放行尾不可见）。
    const VIEWER_MAX_LINES: usize = 100_000;
    let trunc_hint = if viewer.total_lines > viewer.line_count {
        format!("〔已截断 共 {} 行〕", viewer.total_lines)
    } else {
        String::new()
    };
    let title = match &viewer.query {
        Some(q) => format!(
            "{trunc_hint}{shown} · 行 {}/{}  /{}（n/N 跳转）",
            viewer.scroll + 1,
            viewer.line_count,
            q
        ),
        None => format!(
            "{trunc_hint}{shown} · 行 {}/{}（w/Esc 退出 / 搜索）",
            viewer.scroll + 1,
            viewer.line_count
        ),
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
        crate::preview::PreviewKind::Pdf => {
            // AUDIT-2 + M27-3：`[preview] pdf_image_pages`（0 关/1 第 1 页/
            // 2 前两页）与 `pdf_prefer_image`（true = 文本先行后仍优先展示
            // 图像）。产出（结果缓存）经 ImageEngine 渲染；两页在预览区
            // 上下排列（高度不足仅显示第 1 页并提示）；失败回退文本/提示
            // 路径（文本提取先行不跳过，AGENTS §5.5）。
            let pages = state.config.preview.pdf_image_pages.min(2);
            let prefer = state.config.preview.pdf_prefer_image;
            if let Some(pngs) = crate::pdf::cached_visual_pngs(&path, pages, prefer) {
                let page_note = if pngs.len() >= 2 {
                    "第 1–2 页图像"
                } else {
                    "第 1 页图像"
                };
                let pane = ratatui::widgets::Block::bordered()
                    .title(format!("{shown} · PDF 预览（{page_note}）"))
                    .border_style(pane_border_style(true));
                let inner = pane.inner(area);
                frame.render_widget(pane, area);
                if pngs.len() >= 2 && inner.height >= 12 {
                    // 两页上下排列（预览区足够高）。
                    let [top, bottom] = ratatui::layout::Layout::vertical([
                        ratatui::layout::Constraint::Ratio(1, 2),
                        ratatui::layout::Constraint::Ratio(1, 2),
                    ])
                    .areas(inner);
                    let r1 = state.image_engine.render_file(frame, top, &pngs[0]);
                    let r2 = state.image_engine.render_file(frame, bottom, &pngs[1]);
                    if r1.is_err() || r2.is_err() {
                        frame.render_widget(
                            Paragraph::new("PDF 图像渲染失败（协议探测失败或解码错误）"),
                            inner,
                        );
                    }
                } else {
                    // 单页展示（pages=1 / 第 2 页生成失败 / 高度不足只
                    // 显示第 1 页）。pages=2 时底部 1 行可读说明——文案
                    // 区分"高度不足省略第 2 页"与"第 2 页未生成"。
                    let note = pdf_overflow_note(pages, pngs.len());
                    let (img_area, note_area) = if note.is_some() {
                        let [img, n] = ratatui::layout::Layout::vertical([
                            ratatui::layout::Constraint::Min(0),
                            ratatui::layout::Constraint::Length(1),
                        ])
                        .areas(inner);
                        (img, Some(n))
                    } else {
                        (inner, None)
                    };
                    if state
                        .image_engine
                        .render_file(frame, img_area, &pngs[0])
                        .is_err()
                    {
                        frame.render_widget(
                            Paragraph::new("PDF 图像渲染失败（协议探测失败或解码错误）"),
                            img_area,
                        );
                    }
                    if let (Some(text), Some(narea)) = (note, note_area) {
                        frame.render_widget(Paragraph::new(ratatui::text::Line::from(text)), narea);
                    }
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
    let Ok(text) = std::fs::read_to_string(&path) else {
        // M34-7：读取失败/非 UTF-8 → 结构化信息面板（类型/大小/修改）。
        frame.render_widget(Paragraph::new(crate::preview::file_info_text(&path)), inner);
        return;
    };
    let height = inner.height as usize;
    let width = inner.width as usize;
    let lines: Vec<&str> = text.lines().take(VIEWER_MAX_LINES).collect();
    // M21-E3：语法高亮——扩展名/shebang 推断，未命中或搜索模式下
    // 降级纯文本（查询黄底叠加与语法着色互斥，SIMPLIFIED）。
    let dark = crate::theme::active_variant() != opaline::schema::ThemeVariant::Light;
    let mut highlighter = if viewer.query.is_none() {
        crate::highlight::Highlighter::for_path(&path, lines.first().copied().unwrap_or(""), dark)
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
                    let toks = h.highlight_line(line);
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
    if tab.view_mode && tab.focus == crate::tab::Focus::Left {
        render_view_content(frame, state, area);
        return;
    }

    // 默认：右 pane 目录列表（独立导航；M21-F1 路径标题 + 类型着色）。
    // M27-2：vifm 模式（pane_borders=false）——顶部路径行（TopLine/
    // TopLineSel 背景色区分焦点），无整框边框。
    let borders = state.config.ui.pane_borders;
    let inner = if borders {
        let pane = ratatui::widgets::Block::bordered()
            .title(tab.right.location.display_path().replace('\\', "/"))
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
            Paragraph::new(Line::from(
                tab.right.location.display_path().replace('\\', "/"),
            ))
            .style(path_style),
            topline,
        );
        rest
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
    let items: Vec<ListItem> = tab
        .right
        .entries
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            // M21-E1：属性列自适应（与左 pane 同口径）。
            let show_attrs =
                state.config.ui.attrs_column && inner.width as usize >= MIN_ATTRS_INNER_WIDTH;
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
                (inner.width as usize).saturating_sub(
                    2 + ATTRS_WIDTH + 2 + if git_col { GIT_STATUS_WIDTH } else { 0 },
                )
            } else {
                (inner.width as usize).saturating_sub(4)
            };
            let display_name = clip_name(&item.name, name_budget);
            let ft = filetype_style(item.filetype_class());
            let mut spans = vec![Span::from(format!("{} ", item.icon.glyph))];
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
            if idx == tab.right.cursor {
                // M21-F2：焦点侧反显，非焦点侧 DIM 灰化。
                li = li.style(if focused {
                    Style::new().add_modifier(Modifier::REVERSED)
                } else {
                    Style::new().add_modifier(Modifier::DIM)
                });
            }
            li
        })
        .collect();
    // M41-4：偏移有状态（Pane.scroll，每帧 sync_viewport_scroll 落盘）；
    // 不再依赖 ListState 从 0 起的自动调整（会把光标钉在视口底行）。
    let mut list_state = ListState::default()
        .with_offset(tab.right.scroll)
        .with_selected(Some(tab.right.cursor));
    frame.render_stateful_widget(ratatui::widgets::List::new(items), inner, &mut list_state);
}
