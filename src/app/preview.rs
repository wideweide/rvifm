use super::*;
use std::fs;

/// M34-4：查看器内容渲染（M17 VIEW-1 文本 + 图片/目录/PDF/Office 分流）——
/// 对侧 pane 复用：render_right_pane（焦点左）与 render()（焦点右）分派到
/// 非焦点 pane。预览目标 = preview_path()（M34-4 起焦点感知）。
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
    // M81：标题栏与目录栏同源；下方完整方框边框用 :diff 弹窗同一套焦点样式。
    let focused = !quick;
    let title_style = if focused {
        crate::app::render::vifm_style("TopLineSel")
    } else {
        crate::app::render::vifm_style("TopLine")
    };
    let title_label = format!("{} · 预览", path.display().to_string().replace('\\', "/"));
    let [title_area, box_area] = ratatui::layout::Layout::vertical([
        ratatui::layout::Constraint::Length(1),
        ratatui::layout::Constraint::Min(0),
    ])
    .areas(area);
    frame.render_widget(ratatui::widgets::Clear, title_area);
    frame.render_widget(
        ratatui::widgets::Paragraph::new(ratatui::text::Line::from(title_label)).style(title_style),
        title_area,
    );
    let box_block = ratatui::widgets::Block::bordered()
        .border_style(crate::app::render::pane_border_style(focused));
    let inner_box = box_block.inner(box_area);
    frame.render_widget(box_block, box_area);
    let area = inner_box;
    let path = path.to_path_buf();
    // 图片：快速态只显示元数据；获焦后仍走 ImageEngine 全量渲染。
    if matches!(
        crate::preview::preview_kind(&path),
        crate::preview::PreviewKind::Image
    ) {
        if quick {
            let inner = area;
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
        let inner = area.intersection(frame.area());
        if inner.width == 0 || inner.height == 0 {
            return;
        }
        // M85：每张图独立 worker 通道——切换图片时旧发送端随协议一起丢弃，
        // 在途的旧 id 请求无法串进新图（H3）；编码失败节流重试（H2 风险）。
        let mut guard = match state.thread_preview.try_borrow_mut() {
            Ok(g) => g,
            Err(_) => return,
        };
        let now = std::time::Instant::now();
        let rebuild = guard
            .as_ref()
            .is_none_or(|p| p.path.as_path() != path.as_path())
            || guard
                .as_ref()
                .and_then(|p| p.retry_after)
                .is_some_and(|t| t <= now);
        if rebuild {
            *guard = None;
            if let Some((_, img)) = state.decoded_preview.as_ref().filter(|(p, _)| p == &path) {
                let (req_tx, req_rx): (
                    std::sync::mpsc::Sender<ratatui_image::thread::ResizeRequest>,
                    std::sync::mpsc::Receiver<ratatui_image::thread::ResizeRequest>,
                ) = std::sync::mpsc::channel();
                let (resp_tx, resp_rx): (
                    std::sync::mpsc::Sender<crate::state::ResizeResult>,
                    std::sync::mpsc::Receiver<crate::state::ResizeResult>,
                ) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    while let Ok(request) = req_rx.recv() {
                        let result = request.resize_encode().map_err(|_| ());
                        if resp_tx.send(result).is_err() {
                            break;
                        }
                    }
                });
                let proto = ratatui_image::thread::ThreadProtocol::new(
                    req_tx,
                    Some(state.image_engine.resize_protocol(img.clone())),
                );
                *guard = Some(crate::state::ThreadPreview {
                    path: path.clone(),
                    rect: inner,
                    proto,
                    rx: resp_rx,
                    retry_after: None,
                });
            }
        }
        if let Some(preview_state) = guard.as_mut() {
            if preview_state.retry_after.is_some_and(|t| t > now) {
                frame.render_widget(Paragraph::new("图片编码失败，稍后重试…"), inner);
            } else {
                // 记录完整矩形（含坐标），布局变化由 ThreadProtocol 自动发 resize。
                preview_state.rect = inner;
                frame.render_stateful_widget(
                    ratatui_image::StatefulImage::new().resize(ratatui_image::Resize::Fit(None)),
                    inner,
                    &mut preview_state.proto,
                );
            }
        } else {
            frame.render_widget(Paragraph::new("图片解码中…（后台加载）"), inner);
        }
        return;
    }
    // M21-F6：光标在目录上 → 浅层 tree（复用 M13-7 构建器，深度 2、
    // 只读快照）——不走文本读取路径，杜绝"无法读取/非 UTF-8"误报。
    if path.is_dir() {
        let inner = area;
        // M63-1：按 (目录, mtime) 缓存浅层树，超大目录不再每帧重扫。
        let stamp = std::fs::metadata(&path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let key = (path.clone(), stamp);
        let cached = state.directory_preview_cache.borrow().get(&key).cloned();
        let items = match cached {
            Some(items) => items,
            None => match crate::tab::Tab::build_tree_entries(&path, 2) {
                Ok(items) => {
                    if let Ok(mut cache) = state.directory_preview_cache.try_borrow_mut() {
                        if cache.len() >= 16 {
                            cache.clear();
                        }
                        cache.insert(key, items.clone());
                    }
                    items
                }
                Err(e) => {
                    frame.render_widget(Paragraph::new(format!("目录读取失败: {e}")), inner);
                    return;
                }
            },
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
    // M86-1：`:view` 标题改清爽模式——只显示「路径 · 预览」，不再塞入
    // 行号计数/「已截断」（二进制 zip 的 size/80 或 size/16 估算会给出
    // `1/11692098` 这类无意义数字）。
    let title = format!("{shown} · 预览");
    frame.render_widget(
        ratatui::widgets::Paragraph::new(ratatui::text::Line::from(title)).style(title_style),
        title_area,
    );
    let inner = area;
    // M96-1：乐观占位——后台 viewer 载荷未就绪时立即给出可读反馈，
    // 不让用户盯着旧内容或空白等待。
    if state
        .active_tab()
        .viewer_pending
        .as_ref()
        .is_some_and(|(p, _)| p == &path)
        && tab.viewer.as_ref().and_then(|v| v.path.as_ref()) != Some(&path)
    {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(format!(
                    "预览生成中… {}",
                    path.display().to_string().replace('\\', "/")
                )),
                Line::from("（后台加载；移动光标或按 Esc 可取消等待）"),
            ]),
            inner,
        );
        return;
    }
    // M97：Markdown 样式行（后台渲染，复用 viewer 的滚动/检索）。
    let is_md = path.extension().is_some_and(|e| {
        matches!(
            e.to_string_lossy().to_ascii_lowercase().as_str(),
            "md" | "markdown"
        )
    });
    if is_md && state.config.preview.markdown_render {
        if let Some((_, lines)) = state.md_preview.peek(&path) {
            let shown: Vec<Line> = lines
                .iter()
                .skip(viewer.scroll)
                .take(inner.height.max(1) as usize)
                .cloned()
                .collect();
            frame.render_widget(Paragraph::new(shown), inner);
        } else {
            frame.render_widget(Paragraph::new("Markdown 渲染中…"), inner);
        }
        return;
    }
    // M21-F7：PDF/Office 分流——走共享 PreviewRenderer（pdf-inspector
    // 文本提取先行 / office_oxide 提取），失败按类型提示；不落文本读取
    // 通用路径（杜绝"无法读取（非 UTF-8 或已删除）"误报）。
    match crate::preview::preview_kind(&path) {
        crate::preview::PreviewKind::Office => {
            let inner = area;
            if let Some((_, lines)) = state.special_preview.peek(&path) {
                let rendered = lines
                    .iter()
                    .take(inner.height.max(1) as usize)
                    .map(|l| Line::from(l.clone()))
                    .collect::<Vec<_>>();
                frame.render_widget(Paragraph::new(rendered), inner);
            } else {
                let preview = crate::preview::render_preview(&path, inner.height.max(1) as usize);
                frame.render_widget(preview.into_paragraph(), inner);
            }
            return;
        }
        crate::preview::PreviewKind::Archive => {
            // M45-4b + M46-8：归档成员列表（Office/PDF 已在 preview_kind 中
            // 优先命中）——缓存版同源渲染，损坏/.rar 回退信息面板。
            let inner = area;
            // M46-8：按 (路径, mtime, size) 缓存成员列表，同一归档预览不再
            // 每帧重复打开/全量解压（firefox tarball 等 xz/bz2 秒出提示）。
            if let Some((_, lines)) = state.special_preview.peek(&path) {
                let rendered = lines
                    .iter()
                    .take(inner.height.max(1) as usize)
                    .map(|l| Line::from(l.clone()))
                    .collect::<Vec<_>>();
                frame.render_widget(Paragraph::new(rendered), inner);
            } else {
                let preview = crate::preview::archive_preview_cached_with_config(
                    &path,
                    inner.height.max(1) as usize,
                    &state.archive_preview_cache,
                    state.config.preview.archive_external_listing,
                );
                frame.render_widget(preview.into_paragraph(), inner);
            }
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
                let inner = area;
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
            let inner = area;
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
    // M53-5/M83：按 (path, mtime, size) 缓存行，`Arc` 命中零拷贝。
    let meta = std::fs::metadata(&path).ok();
    let key: crate::state::HighlightCacheKey = (
        path.clone(),
        meta.as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0),
        meta.as_ref().map(|m| m.len()).unwrap_or(0),
    );
    // M83-1：文本一律视口渲染，不再「快速截断 → 获焦全量」。
    let (lines, base_line): (Vec<String>, usize) = if let Some(view) = state
        .viewport_text
        .as_ref()
        .filter(|v| v.path() == path.as_path())
    {
        (
            view.lines(viewer.scroll, height.saturating_add(1), width),
            viewer.scroll,
        )
    } else {
        let cached: Option<std::sync::Arc<Vec<String>>> = state
            .text_preview_cache
            .try_borrow_mut()
            .ok()
            .and_then(|mut cache| cache.get_cloned(&key));
        let parsed = if let Some(hit) = cached {
            hit
        } else {
            let text = std::fs::read(&path)
                .ok()
                .and_then(|bytes| crate::preview::decode_preview_text(&path, bytes));
            let Some(text) = text else {
                render_fallback_lines(frame, &path, inner);
                return;
            };
            let parsed: std::sync::Arc<Vec<String>> = std::sync::Arc::new(
                text.lines()
                    .take(VIEWER_MAX_LINES)
                    .map(str::to_owned)
                    .collect(),
            );
            if let Ok(mut cache) = state.text_preview_cache.try_borrow_mut() {
                cache.insert(key.clone(), parsed.clone());
            }
            parsed
        };
        // 只拷贝当前视口行（最多 height 行），不再整文件 clone。
        let start = viewer.scroll.min(parsed.len());
        let end = start.saturating_add(height).min(parsed.len());
        (parsed[start..end].to_vec(), viewer.scroll)
    };
    // M83：语法高亮始终启用；查询命中改黄底叠加（不再互斥）。
    // H4：每帧新建高亮器（从当前视口首行推进），避免跨帧复用导致 syntect
    // 状态单调前进、块注释/字符串在第二帧起错色。构造本身很便宜。
    let dark = crate::theme::active_variant() != opaline::schema::ThemeVariant::Light;
    let first = lines.first().map(String::as_str).unwrap_or("");
    let mut highlighter = crate::highlight::Highlighter::for_path(&path, first, dark);
    let mut out: Vec<Line> = Vec::with_capacity(height);
    for (i, line) in lines.iter().take(height).enumerate() {
        let lineno = base_line + i + 1;
        let is_current = viewer.query.is_some() && base_line + i == viewer.match_line;
        let gutter = if is_current {
            format!("{lineno:>4} ▶ ")
        } else {
            format!("{lineno:>4} │ ")
        };
        let mut spans = vec![Span::styled(
            gutter,
            Style::new().add_modifier(Modifier::DIM),
        )];
        let display: String = line.chars().take(width).collect();
        let query_range = viewer
            .query
            .as_deref()
            .and_then(|q| crate::tab::match_range(&display, q));
        let mut content_spans: Vec<Span<'static>> = match highlighter.as_mut() {
            Some(h) => {
                let toks = h.highlight_line(line.as_str());
                if toks.is_empty() {
                    vec![Span::from(display)]
                } else {
                    toks.into_iter()
                        .map(|(tok, st)| Span::styled(tok, st))
                        .collect()
                }
            }
            None => vec![Span::from(display)],
        };
        if let Some((start, end)) = query_range {
            content_spans = overlay_query_spans(content_spans, start, end);
        }
        spans.extend(content_spans);
        out.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(out), inner);
}

/// M83-2：缺失文件首行只显示「文件不存在」，其余情况沿用结构化信息面板。
fn render_fallback_lines(frame: &mut Frame, path: &std::path::Path, area: ratatui::layout::Rect) {
    let lines: Vec<Line<'static>> = if !path.exists() {
        vec![Line::from(format!(
            "文件不存在: {}",
            path.display().to_string().replace('\\', "/")
        ))]
    } else {
        crate::preview::file_info_text(path)
            .lines()
            .map(|l| Line::from(l.to_owned()))
            .collect()
    };
    frame.render_widget(Paragraph::new(lines), area);
}

/// M83：查询命中字节区间叠加黄底到语法着色片段（保留前景色）。
fn overlay_query_spans(spans: Vec<Span<'static>>, start: usize, end: usize) -> Vec<Span<'static>> {
    let mut out = Vec::with_capacity(spans.len() + 2);
    let mut pos = 0usize;
    for span in spans {
        let len = span.content.len();
        let s = pos;
        let e = pos + len;
        pos = e;
        if len == 0 || e <= start || s >= end {
            out.push(span);
            continue;
        }
        let a = start.saturating_sub(s).min(len);
        let b = end.saturating_sub(s).min(len);
        if a > 0 {
            out.push(Span::styled(span.content[..a].to_owned(), span.style));
        }
        if b > a {
            out.push(Span::styled(
                span.content[a..b].to_owned(),
                span.style.patch(Style::new().bg(Color::LightYellow)),
            ));
        }
        if b < len {
            out.push(Span::styled(span.content[b..].to_owned(), span.style));
        }
    }
    out
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
    let preview_focused =
        tab.preview_focus || tab.pane_frecency.as_ref().is_some_and(|p| p.preview_focus);
    let focused = tab.focus == crate::tab::Focus::Right && !preview_focused;
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
            .title(crate::app::render::path_line_text_fit(
                tab.right.location.display_path().replace('\\', "/"),
                tab.right.filter.as_deref(),
                area.width,
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
            Paragraph::new(Line::from(crate::app::render::path_line_text_fit(
                tab.right.location.display_path().replace('\\', "/"),
                tab.right.filter.as_deref(),
                area.width,
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
            // M89-3：右栏 v 选区与 t 标记同背景样式（与左栏一致）。
            if row_reversed || tab.right.tags.contains(&item.name) {
                li = li.style(crate::app::render::selected_row_style(focused));
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
