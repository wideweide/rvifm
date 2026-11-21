use super::*;

/// FEAT-3：菜单浮窗矩形——固定 34×8 居中（含边框）。
pub(crate) fn context_menu_rect(area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    use ratatui::layout::Rect;
    let w = 34u16.min(area.width);
    let h = ((CONTEXT_MENU.len() as u16) + 2).min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(w) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}

/// FEAT-1：列表视口偏移——保证光标可见（offset ≤ cursor < offset+height）。
/// 渲染（ListState.with_offset）与鼠标命中测试共用同一推导，避免偏差；
/// 与 ratatui List 从 0 起自动调整的结果一致。
pub(crate) fn list_offset(cursor: usize, height: usize) -> usize {
    if height == 0 || cursor < height {
        0
    } else {
        cursor + 1 - height
    }
}

/// M21-F1：整体几何——标签栏 1 行 + 双 pane（居中分开，间隔 1 列）+ 状态栏 1 行。
/// 渲染与鼠标命中测试共用同一推导，避免两处几何漂移。
pub(crate) fn pane_layout(area: ratatui::layout::Rect) -> [ratatui::layout::Rect; 4] {
    let [tab_bar, main, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);
    let [left, _gap, right] = Layout::horizontal([
        Constraint::Ratio(1, 2),
        Constraint::Length(1),
        Constraint::Ratio(1, 2),
    ])
    .areas(main);
    [tab_bar, left, right, status]
}

/// M21-F1：左 pane 列表内容区（边框内减去 `..` 行）。
/// 渲染与鼠标命中测试共用；`has_parent` 由调用方决定是否再减 1 行。
/// M27-2：`borders=false` 时为 pane 区减去顶部路径行（vifm TopLine）。
pub(crate) fn left_list_area(borders: bool, area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    if borders {
        ratatui::widgets::Block::bordered().inner(area)
    } else {
        let [_topline, rest] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
        rest
    }
}

/// M27-2：pane 间隔列（1 列；vifm Border 分隔线渲染区）。
pub(crate) fn pane_gap_area(area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    let [_, main, _] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);
    let [_, gap, _] = Layout::horizontal([
        Constraint::Ratio(1, 2),
        Constraint::Length(1),
        Constraint::Ratio(1, 2),
    ])
    .areas(main);
    gap
}

/// M27-2：vifm 命名样式（TabLine/TopLine/StatusLine/Border/Selected…）——
/// 主题提供该命名样式时优先；缺失时按标准 token 推导（M34-1）；未激活/
/// 缺 token 时回退 vifm Default 风格的 16 色（vifm_fallback）。
pub(crate) fn vifm_style(name: &str) -> Style {
    let s = crate::theme::style(name);
    if s.fg.is_some() || s.bg.is_some() {
        return s;
    }
    // M34-1：主题缺该命名样式时按标准 token 通用推导（gruvbox 等内置主题
    // 的高亮组映射面）；推导不可用（无主题/缺 token/未知组）回退 16 色。
    if let Some(derived) = crate::theme::derive_vifm_group(name) {
        return derived;
    }
    vifm_fallback(name)
}

/// M27-2：vifm Default 风格回退值（NOTES M27-2 调研口径）。
fn vifm_fallback(name: &str) -> Style {
    match name {
        // 非焦点路径行 / 标签底 / 分隔线 / 状态行：黑字白底。
        "TopLine" | "TabLine" | "Border" | "StatusLine" => {
            Style::new().fg(Color::Black).bg(Color::White)
        }
        // 焦点路径行 / 激活标签：黑字默认底加粗。
        "TopLineSel" | "TabLineSel" => Style::new().fg(Color::Black).add_modifier(Modifier::BOLD),
        // 选中项：品红加粗。
        "Selected" => Style::new().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        _ => Style::new(),
    }
}

/// M21-F1：pane 边框焦点态——主题命名样式（focused_border/unfocused_border）
/// 优先，未激活主题或样式缺失时以 BOLD/DIM 区分（保证无边框主题下仍可见）。
pub(crate) fn pane_border_style(focused: bool) -> Style {
    let s = crate::theme::style(if focused {
        "focused_border"
    } else {
        "unfocused_border"
    });
    if s.fg.is_some() {
        s
    } else if focused {
        Style::new().add_modifier(Modifier::BOLD)
    } else {
        Style::new().add_modifier(Modifier::DIM)
    }
}

/// M21-E1/M24-1：属性列布局——大小列 10 列右对齐 + 分隔 2 列 + 时间列
/// 16 列（`yyyy-mm-dd HH:mm`，分钟精度）。
pub(crate) const ATTRS_WIDTH: usize = 10 + 2 + 16;
/// 属性列显示所需的最小 pane 内区宽度（窄终端隐藏属性列，文件名优先）。
pub(crate) const MIN_ATTRS_INNER_WIDTH: usize = 55;

/// M25-5：git 状态列宽（状态字符 + 2 空格间隔）。
pub(crate) const GIT_STATUS_WIDTH: usize = 3;

/// M25-5：git 状态显示 span（着色；M/D 红、A 绿、R/T 黄、? 青、U 品红）。
pub(crate) fn git_status_span(st: crate::git::GitStatus) -> Span<'static> {
    let color = match st {
        crate::git::GitStatus::Modified | crate::git::GitStatus::Deleted => Color::Red,
        crate::git::GitStatus::Added => Color::Green,
        crate::git::GitStatus::Renamed | crate::git::GitStatus::Typechange => Color::Yellow,
        crate::git::GitStatus::Untracked => Color::Cyan,
        crate::git::GitStatus::Conflicted => Color::Magenta,
    };
    Span::styled(format!("{}  ", st.marker()), Style::new().fg(color))
}

/// M21-E1：按显示预算截断名称（字符数近似；CJK 全角按 1 计——与查看器
/// 显示口径一致，SIMPLIFIED）。搜索/定位逻辑不受影响（仅显示层裁剪）。
pub(crate) fn clip_name(name: &str, budget: usize) -> String {
    if budget == 0 {
        return String::new();
    }
    name.chars().take(budget).collect()
}

/// M21-F1：类型着色——主题命名样式（`ft_dir` 等）优先，缺失时回落 16 色
/// 近似（yazi filetype + vifm 配色语义：目录青/可执行绿/链接紫/归档红/
/// 图片亮蓝/视频黄/代码蓝；Other 不着色）。
pub(crate) fn filetype_style(class: crate::tab::FileTypeClass) -> Style {
    use crate::tab::FileTypeClass as C;
    let (name, fallback) = match class {
        C::Directory => ("ft_dir", Color::Cyan),
        C::Executable => ("ft_exec", Color::Green),
        C::Symlink => ("ft_link", Color::Magenta),
        C::Archive => ("ft_archive", Color::Red),
        C::Image => ("ft_image", Color::LightBlue),
        C::Video => ("ft_video", Color::Yellow),
        C::Code => ("ft_code", Color::Blue),
        C::Other => return Style::new(),
    };
    let s = crate::theme::style(name);
    if s.fg.is_some() {
        s
    } else {
        Style::new().fg(fallback)
    }
}

/// FEAT-4：帮助文本（内嵌单文件，`include_str!` 便于维护）。
pub(crate) const HELP_TEXT: &str = include_str!("help.txt");

/// FEAT-4：帮助文本按行展开（渲染与搜索共用）。
pub(crate) fn help_lines() -> Vec<&'static str> {
    HELP_TEXT.lines().collect()
}

/// FEAT-4：帮助页搜索——从 `start` 行起找包含 `query`（大小写不敏感）
/// 的行，环绕列表首尾；找不到返回 None。
pub(crate) fn help_find_match(
    lines: &[&str],
    query: &str,
    start: usize,
    forward: bool,
) -> Option<usize> {
    if lines.is_empty() || query.is_empty() {
        return None;
    }
    let len = lines.len();
    for i in 0..len {
        let idx = if forward {
            (start + i) % len
        } else {
            (start + len - i % len) % len
        };
        if lines[idx].to_lowercase().contains(&query.to_lowercase()) {
            return Some(idx);
        }
    }
    None
}

/// FEAT-1/FEAT-3：条目行命中测试——鼠标行 → 条目索引；不在条目区
/// （边框/`..` 行/右 pane/标签栏/状态栏/越界）返回 None。
/// 几何与 render 同源（M21-F1 pane_layout + 左 pane 边框内区）。
pub(crate) fn hit_entry_index(
    state: &AppState,
    mouse: &MouseEvent,
    area: ratatui::layout::Rect,
) -> Option<usize> {
    let [_, left, _, _] = pane_layout(area);
    let inner = left_list_area(state.config.ui.pane_borders, left);
    let has_parent = state.active_tab().has_parent_row() as usize;
    let view_h = inner.height.saturating_sub(has_parent as u16) as usize;
    if mouse.column < inner.x
        || mouse.column >= inner.x + inner.width
        || mouse.row < inner.y + has_parent as u16
        || mouse.row >= inner.y + inner.height
    {
        return None;
    }
    let tab = state.active_tab();
    let offset = list_offset(if tab.on_parent_row { 0 } else { tab.cursor }, view_h);
    let idx = (mouse.row - inner.y - has_parent as u16) as usize;
    idx.checked_add(offset)?;
    Some(idx.min(tab.entries.len().saturating_sub(1)))
}

/// FIX-4：居中浮窗矩形——按终端自适应（60% 宽 / 50% 高），上限 80×20，
/// 小终端保底 30×8（不足时取终端本身），在区域内水平垂直居中。
pub(crate) fn centered_rect(area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    use ratatui::layout::Rect;
    let w = (area.width * 60 / 100).clamp(30.min(area.width), 80.min(area.width));
    let h = (area.height * 50 / 100).clamp(8.min(area.height), 20.min(area.height));
    Rect {
        x: area.x + area.width.saturating_sub(w) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}

/// M21-E8：跳转选择器区域——80%×80% 居中（无全屏遮罩，双 pane 可见）。
pub(crate) fn jump_rect(area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    use ratatui::layout::Rect;
    let w = (area.width * 80 / 100).max(1);
    let h = (area.height * 80 / 100).max(1);
    Rect {
        x: area.x + area.width.saturating_sub(w) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}

/// M23-6：Frecency Pane 渲染——焦点 pane 显示搜索+结果，对侧显示光标
/// 条目的快速预览（PreviewRenderer，文本 100 行截断 + 标记）。
pub(crate) fn render_frecency_panes(frame: &mut Frame, state: &AppState) {
    let tab = state.active_tab();
    let pane = match &tab.pane_frecency {
        Some(p) => p,
        None => return,
    };
    let [tab_bar_area, left_area, right_area, status_area] = pane_layout(frame.area());
    // 标签栏照常渲染。
    let tab_spans: Vec<Span> = state
        .tabs
        .iter()
        .enumerate()
        .map(|(idx, t)| {
            let dir = t
                .location
                .base_dir()
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| t.location.display_path());
            let label = format!("{}:{} ", idx + 1, dir);
            if idx == state.active {
                Span::styled(
                    label,
                    Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD),
                )
            } else {
                Span::from(label)
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(Line::from(tab_spans)), tab_bar_area);

    let focused_is_left = tab.focus == crate::tab::Focus::Left;
    let frec_area = if focused_is_left {
        left_area
    } else {
        right_area
    };
    let other_area = if focused_is_left {
        right_area
    } else {
        left_area
    };

    // 焦点 pane：Frecency 搜索 UI。
    let visible = filter_jump(&pane.entries, &pane.query);
    // M34-5：标题固定文案（原 `Frecency · Normal（N 项）`——计数移至底部提示行）。
    let title = "Frecency - 高频目录/文件搜索";
    let frec_block = ratatui::widgets::Block::bordered()
        .title(title)
        .border_style(pane_border_style(true));
    let inner = frec_block.inner(frec_area);
    frame.render_widget(frec_block, frec_area);
    let frec_items: Vec<ListItem> = visible
        .iter()
        .enumerate()
        .map(|(idx, e)| {
            // M34-5：devicons 图标 + 全路径（替换 [目录]/[文件] 文本标记；
            // 目录条目路径补尾斜杠触发 devicons 文件夹图标；✦keyword 保留）。
            let icon_path = if e.topic == "dir" {
                std::path::PathBuf::from(format!("{}/", e.path))
            } else {
                std::path::PathBuf::from(&e.path)
            };
            let icon = crate::icons::icon_for_active(&icon_path);
            let bm = if e.keyword.is_empty() {
                String::new()
            } else {
                format!(" ✦{}", e.keyword)
            };
            ListItem::new(Line::from(format!("{} {}{}", icon.glyph, e.path, bm))).style(
                if idx == pane.cursor {
                    Style::new().add_modifier(Modifier::REVERSED)
                } else {
                    Style::new()
                },
            )
        })
        .collect();
    let mut list_state = ListState::default().with_selected(Some(pane.cursor));
    frame.render_stateful_widget(
        ratatui::widgets::List::new(frec_items)
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED)),
        inner,
        &mut list_state,
    );

    // 对侧：光标条目的快速预览（PreviewRenderer，100 行截断）。
    let other_block = ratatui::widgets::Block::bordered()
        .title("预览")
        .border_style(pane_border_style(false));
    let other_inner = other_block.inner(other_area);
    frame.render_widget(other_block, other_area);
    if let Some(entry) = visible.get(pane.cursor) {
        let path = std::path::PathBuf::from(&entry.path);
        let preview = crate::preview::render_preview(&path, 100);
        frame.render_widget(preview.into_paragraph(), other_inner);
    }
    // M34-5：搜索模式底部实时回显当前 query（键入即时更新、清空同步）；
    // Normal 显示操作提示。计数自标题移至两处提示行。
    let hint = if pane.normal {
        format!(
            "j/k/gg/G=移动 l/Enter=跳转 y=复制 e=编辑 w=view Esc=关闭（{} 项）",
            visible.len()
        )
    } else {
        format!(
            "/{}  输入=筛选 Enter=跳转 Esc=Normal（{} 项）",
            pane.query,
            visible.len()
        )
    };
    frame.render_widget(Paragraph::new(hint), status_area);
}

pub(crate) fn render(frame: &mut Frame, state: &AppState, mode: &InputMode, msg: Option<&str>) {
    let tab = state.active_tab();
    // M14-2：标签栏（1 行）——每个 tab 的索引与目录名，当前 tab 高亮。
    let [tab_bar_area, left_area, right_area, status_area] = pane_layout(frame.area());
    // M23-6：Frecency Pane 模式——焦点 pane 显示搜索 UI，对侧显示快速预览。
    if state.active_tab().pane_frecency.is_some() {
        render_frecency_panes(frame, state);
        return;
    }
    // M27-2：vifm 风格（pane_borders=false，默认）与旧整框外观（true）双模式。
    let borders = state.config.ui.pane_borders;
    let tab_spans: Vec<Span> = state
        .tabs
        .iter()
        .enumerate()
        .map(|(idx, t)| {
            let dir = t
                .location
                .base_dir()
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| t.location.display_path());
            let label = format!("{}:{} ", idx + 1, dir);
            if idx == state.active {
                if borders {
                    Span::styled(
                        label,
                        Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD),
                    )
                } else {
                    // TabLineSel：激活标签黑字默认底加粗（16 色回退）。
                    Span::styled(label, vifm_style("TabLineSel"))
                }
            } else if borders {
                Span::from(label)
            } else {
                // TabLine：非激活标签黑字白底。
                Span::styled(label, vifm_style("TabLine"))
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(Line::from(tab_spans)), tab_bar_area);
    // M21-F1：左右 pane——pane_borders=true 走旧整框外观（焦点边框高亮）；
    // false（默认，M27-2）为 vifm 式：无整框、pane 顶部路径行（TopLine/
    // TopLineSel 背景色区分焦点）、间隔列 Border 分隔线。
    let left_focused = tab.focus == crate::tab::Focus::Left;
    // M3-1：可视范围（状态栏指示 + 列表高亮共用；M34-4 提升至 pane 分派
    // 之前——查看器占左 pane 时其后置公共段仍在作用域内）。
    let visual_range = tab.visual_range();
    // M34-4：预览渲染在非焦点 pane——焦点右 + view 模式时查看器占左 pane，
    // 右 pane 保持目录列表导航（与焦点左时"查看器在右"对称，共用同一渲染）。
    let view_in_left = tab.view_mode && tab.focus == crate::tab::Focus::Right;
    if view_in_left {
        crate::app::preview::render_view_content(frame, state, left_area);
    } else {
        let left_inner = if borders {
            let left_pane = ratatui::widgets::Block::bordered()
                .title(tab.location.display_path().replace('\\', "/"))
                .border_style(pane_border_style(left_focused));
            let inner = left_pane.inner(left_area);
            frame.render_widget(left_pane, left_area);
            inner
        } else {
            let [topline, rest] =
                Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(left_area);
            let path_style = if left_focused {
                vifm_style("TopLineSel")
            } else {
                vifm_style("TopLine")
            };
            frame.render_widget(
                Paragraph::new(Line::from(tab.location.display_path().replace('\\', "/")))
                    .style(path_style),
                topline,
            );
            rest
        };
        // M27-2：pane 间隔列——Border 分隔线（vifm 模式）。
        if !borders {
            let gap = pane_gap_area(frame.area());
            let lines: Vec<Line> = (0..gap.height).map(|_| Line::from("│")).collect();
            frame.render_widget(Paragraph::new(lines).style(vifm_style("Border")), gap);
        }

        // FEAT-2：`..` 行——列表顶部独立 1 行（渲染层虚拟条目，不进 entries）。
        // 物理根目录无父目录 → 不渲染；归档视图恒渲染。
        // M21-F2：非焦点侧的 `..` 行置灰（vifm TopLine 语义——光标行 DIM）。
        let parent_available = tab.has_parent_row() && !tab.in_results();
        let parent_area = if parent_available {
            let [p, rest] =
                Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(left_inner);
            let parent_style = if tab.on_parent_row && left_focused {
                Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD)
            } else {
                // 视觉上与普通目录项区分（置灰）。
                Style::new().add_modifier(Modifier::DIM)
            };
            frame.render_widget(Paragraph::new(Line::from("../")).style(parent_style), p);
            rest
        } else {
            left_inner
        };

        // FIX-1：当前生效的搜索查询——搜索输入期用实时输入（不回退旧查询，
        // 清空即无高亮），其余用已提交的 tab.search。
        let active_query: Option<String> = match mode {
            InputMode::Search { input, .. } => {
                let query = input.trim();
                (!query.is_empty()).then(|| query.to_owned())
            }
            _ => tab
                .search
                .as_ref()
                .map(|s| s.query.clone())
                .filter(|query| !query.is_empty()),
        };
        // M21-E1：属性列自适应——配置开关 + 窄终端隐藏（文件名优先）。
        let show_attrs = state.config.ui.attrs_column
            && left_inner.width as usize >= MIN_ATTRS_INNER_WIDTH
            && tab.tree.is_none(); // 树视图相对路径较长，不挤属性列
        // M25-5：git 状态列（属性区内；开关 + 就绪数据，否则占位对齐）。
        let git_col = show_attrs && state.config.ui.git_column;
        let git_map = if git_col {
            match &tab.location {
                Location::Physical(p) => state.git_cache.get(p).and_then(|o| o.as_ref()),
                _ => None,
            }
        } else {
            None
        };
        let name_budget = if show_attrs {
            (left_inner.width as usize)
                .saturating_sub(2 + ATTRS_WIDTH + 2 + if git_col { GIT_STATUS_WIDTH } else { 0 })
        } else {
            (left_inner.width as usize).saturating_sub(4)
        };
        let list_items: Vec<ListItem> = tab
            .entries
            .iter()
            .enumerate()
            .map(|(idx, item)| {
                // M21-F1：名称按文件类型着色（目录/可执行/链接/归档/图片/视频/代码）。
                let ft = filetype_style(item.filetype_class());
                // FIX-1：匹配子串分段高亮（黄底黑字，与光标行反显区分）。
                // 图标/树缩进进前缀 span，目录后缀 `/` 跟在名称之后。
                let mut spans: Vec<Span> = vec![Span::from(format!("{} ", item.icon.glyph))];
                let mut budget = name_budget;
                if tab.tree.is_some() {
                    let depth = item.name.matches('/').count();
                    spans.push(Span::from("  ".repeat(depth)));
                    budget = budget.saturating_sub(2 * depth);
                }
                // M21-E1：显示名按预算截断（搜索/定位仍用全名，仅显示层裁剪）。
                let display_name = clip_name(&item.name, budget);
                match active_query
                    .as_deref()
                    .and_then(|query| crate::tab::match_range(&display_name, query))
                {
                    Some((start, end)) => {
                        let (before, mid, after) = (
                            &display_name[..start],
                            &display_name[start..end],
                            &display_name[end..],
                        );
                        if !before.is_empty() {
                            spans.push(Span::styled(before.to_owned(), ft));
                        }
                        spans.push(Span::styled(
                            mid.to_owned(),
                            Style::new().fg(Color::Black).bg(Color::LightYellow),
                        ));
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
                // M24-1：名称填充至固定列宽（属性列对齐）+ 属性列（分钟精度日期）。
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
                    let attr =
                        format!("{:>10}  {}", size, crate::tab::format_mtime_min(item.mtime));
                    spans.push(Span::styled(attr, Style::new().add_modifier(Modifier::DIM)));
                }
                // M9-2：图标颜色经 opaline 样式染色（命名样式缺失时回落默认）。
                let base_style = crate::theme::style("file_icon");
                // 可视选择范围内的行反显（M3-1）；tag 标记行加粗（M3-2）；
                // 光标行由 highlight_style 叠加。
                let mut li = ListItem::new(Line::from(spans)).style(base_style);
                let mut mods = Modifier::empty();
                if visual_range.is_some_and(|(lo, hi)| idx >= lo && idx <= hi) {
                    mods |= Modifier::REVERSED;
                }
                if tab.tags.contains(&item.name) {
                    mods |= Modifier::BOLD;
                }
                if mods != Modifier::empty() {
                    li = li.style(Style::new().add_modifier(mods));
                }
                li
            })
            .collect();
        // FEAT-2：焦点在 `..` 行时列表无高亮选中项。
        let mut list_state = ListState::default().with_offset(list_offset(
            if tab.on_parent_row { 0 } else { tab.cursor },
            parent_area.height as usize,
        ));
        if !tab.on_parent_row {
            list_state = list_state.with_selected(Some(tab.cursor));
        }
        frame.render_stateful_widget(
            List::new(list_items).highlight_style(
                // M21-F2：焦点 pane 当前行反显，非焦点侧当前行 DIM 灰化
                // （vifm TopLineSel/TopLine 语义）。
                if left_focused {
                    Style::new().add_modifier(Modifier::REVERSED)
                } else {
                    Style::new().add_modifier(Modifier::DIM)
                },
            ),
            parent_area,
            &mut list_state,
        );
    }

    // M11-2→M14-1→M21-F1→M34-4：右 pane——默认独立目录列表（带路径标题/
    // 焦点边框）；view 模式且焦点左时显示查看器/图片预览（焦点右时查看器
    // 已由上方分派到左 pane，右 pane 保持列表导航）。
    render_right_pane(frame, state, mode, right_area);

    if let InputMode::HelpView { scroll, search } = mode {
        // FEAT-4：全屏帮助视图（内容来自内嵌 help.txt，滚动状态独立）。
        let lines = help_lines();
        let title = match search {
            Some(q) => format!("帮助  /{q}（n/N 跳转）"),
            None => "帮助（j/k 滚动  / 搜索  n/N 跳转  q 返回）".to_owned(),
        };
        let area = frame.area();
        let visible: Vec<Line> = lines
            .iter()
            .skip(*scroll)
            .take(area.height.saturating_sub(2) as usize)
            .map(|l| Line::from((*l).to_owned()))
            .collect();
        frame.render_widget(
            Paragraph::new(visible).block(ratatui::widgets::Block::bordered().title(title)),
            area,
        );
        return;
    }
    if let InputMode::FileInfo { path, text } = mode {
        // M13-5：文件详情对话框覆盖层（任意键关闭）。
        let _ = path;
        frame.render_widget(
            Paragraph::new(text.clone())
                .block(ratatui::widgets::Block::bordered().title("文件详情（任意键关闭）")),
            frame.area(),
        );
        return;
    }
    if let InputMode::TrashMenu { cursor, items } = mode {
        // M13-10：暂存区菜单覆盖层（staged → orig，Enter 恢复）。
        let mut lines: Vec<Line> = Vec::new();
        if items.is_empty() {
            lines.push(Line::from("（暂存区为空）"));
        } else {
            for (idx, e) in items.iter().enumerate() {
                let mark = if idx == *cursor { "▶ " } else { "  " };
                lines.push(Line::from(format!("{mark}{} → {}", e.staged, e.orig)));
            }
        }
        frame.render_widget(
            Paragraph::new(lines)
                .block(ratatui::widgets::Block::bordered().title("暂存区（Enter=恢复 Esc=关闭）")),
            frame.area(),
        );
        return;
    }
    if let InputMode::Queue { cursor } = mode {
        // M36-7：传输队列覆盖层（j/k 选择、x 取消条目、Esc 关闭）。
        let mut lines: Vec<Line> = Vec::new();
        if state.transfer_queue.is_empty() {
            lines.push(Line::from("（队列为空）"));
        } else {
            for (i, e) in state.transfer_queue.iter().enumerate() {
                let dir_label = match e.direction {
                    crate::app::jobs::TransferDirection::Upload => "↑",
                    crate::app::jobs::TransferDirection::Download => "↓",
                    crate::app::jobs::TransferDirection::RecurseUpload => "↑↑",
                    crate::app::jobs::TransferDirection::RecurseDownload => "↓↓",
                };
                let sel = if i == *cursor {
                    Style::new().add_modifier(Modifier::REVERSED)
                } else {
                    Style::new()
                };
                lines.push(Line::styled(
                    format!(
                        "{dir_label} {src} → {dest}",
                        src = e.src.display().to_string().replace('\\', "/"),
                        dest = e.dest.display().to_string().replace('\\', "/")
                    ),
                    sel,
                ));
            }
        }
        frame.render_widget(
            Paragraph::new(lines).block(
                ratatui::widgets::Block::bordered().title("传输队列（j/k=选择 x=取消 Esc=关闭）"),
            ),
            frame.area(),
        );
        return;
    }
    if let InputMode::JobDetail { index } = mode {
        // M28-3：作业详情覆盖层——类型/源/目标/状态/进度/速率/时长。
        let mut lines: Vec<Line> = Vec::new();
        if let Some(j) = state.jobs.get(*index) {
            let status = match &j.status {
                crate::state::JobStatus::Running => {
                    if j.cancel_requested
                        .load(std::sync::atomic::Ordering::Relaxed)
                    {
                        "取消中…"
                    } else if j.pause.load(std::sync::atomic::Ordering::Relaxed) {
                        "已暂停（p 恢复）"
                    } else {
                        "运行中"
                    }
                }
                crate::state::JobStatus::Done { summary } => summary,
                crate::state::JobStatus::Cancelled { summary } => summary,
                crate::state::JobStatus::Failed { err } => err,
            };
            let elapsed = j.started.elapsed().as_secs();
            let rate = j
                .progress
                .rate_bytes_per_sec()
                .map(|r| format!("{}/s", crate::tab::format_size(r as u64)))
                .unwrap_or_else(|| "—".to_owned());
            lines.push(Line::from(format!("作业: #{id}", id = j.id)));
            lines.push(Line::from(format!("类型: {}", j.kind)));
            lines.push(Line::from(format!("来源: {}", j.src.replace('\\', "/"))));
            lines.push(Line::from(format!("目标: {}", j.target.replace('\\', "/"))));
            lines.push(Line::from(format!(
                "条目: {}/{} 项",
                j.progress.processed(),
                j.src_count
            )));
            lines.push(Line::from(format!(
                "已传输: {}",
                crate::tab::format_size(j.progress.bytes())
            )));
            lines.push(Line::from(format!("速率（5s 滑动平均）: {rate}")));
            lines.push(Line::from(format!("已运行: {elapsed}s")));
            lines.push(Line::from(format!("状态: {status}")));
            lines.push(Line::from(String::new()));
            lines.push(Line::from("任意键返回 :jobs"));
        } else {
            lines.push(Line::from("（作业不存在）"));
        }
        frame.render_widget(
            Paragraph::new(lines)
                .block(ratatui::widgets::Block::bordered().title("作业详情（Enter）")),
            frame.area(),
        );
        return;
    }
    if let InputMode::Jobs { cursor } = mode {
        // M13-8 → M24-9 → M28-3：后台作业菜单覆盖层
        //（j/k 选择、x 取消、p 暂停/恢复、Enter 详情、Esc/q 关闭）。
        let mut lines: Vec<Line> = Vec::new();
        if state.jobs.is_empty() {
            lines.push(Line::from("（无后台作业）"));
        } else {
            for (i, j) in state.jobs.iter().enumerate() {
                let status = match &j.status {
                    crate::state::JobStatus::Running => {
                        if j.cancel_requested
                            .load(std::sync::atomic::Ordering::Relaxed)
                        {
                            "取消中…".to_owned()
                        } else if j.pause.load(std::sync::atomic::Ordering::Relaxed) {
                            "已暂停".to_owned()
                        } else {
                            "运行中".to_owned()
                        }
                    }
                    crate::state::JobStatus::Done { summary } => summary.clone(),
                    crate::state::JobStatus::Cancelled { summary } => summary.clone(),
                    crate::state::JobStatus::Failed { err } => format!("失败: {err}"),
                };
                let sel = if i == *cursor {
                    Style::new().add_modifier(Modifier::REVERSED)
                } else {
                    Style::new()
                };
                lines.push(Line::styled(
                    format!(
                        "#{id} {kind} → {target}（{} 项）: {status}",
                        j.src_count,
                        id = j.id,
                        kind = j.kind,
                        target = j.target
                    ),
                    sel,
                ));
            }
        }
        frame.render_widget(
            Paragraph::new(lines).block(
                ratatui::widgets::Block::bordered()
                    .title("后台作业（j/k=选择 x=取消 p=暂停/恢复 Enter=详情 Esc=关闭）"),
            ),
            frame.area(),
        );
        return;
    }
    if let InputMode::SortDialog { cursor, desc } = mode {
        // M18-1：排序对话框（固定尺寸居中遮罩）。
        let area = frame.area();
        frame.render_widget(ratatui::widgets::Clear, area);
        frame.render_widget(
            ratatui::widgets::Block::new().style(Style::new().bg(Color::Black)),
            area,
        );
        let popup = context_menu_rect(area);
        let cur_sort = state.active_tab().sort.clone();
        let lines: Vec<Line> = SORT_FIELDS
            .iter()
            .enumerate()
            .map(|(idx, (_lo, _hi, label, field))| {
                let current = cur_sort
                    .as_ref()
                    .and_then(|v| v.first())
                    .filter(|s| s.field == *field)
                    .map(|s| if s.desc { "desc" } else { "asc" });
                let pending = if idx == *cursor {
                    Some(if *desc { "desc" } else { "asc" })
                } else {
                    None
                };
                let text = match (current, pending) {
                    (Some(cur), Some(pend)) => {
                        format!("{label}  [{cur} → {pend}]")
                    }
                    (Some(cur), None) => format!("{label}  [{cur}]"),
                    (None, Some(pend)) => format!("{label}  [→ {pend}]"),
                    (None, None) => (*label).to_owned(),
                };
                if idx == *cursor {
                    Line::from(text).style(Style::new().add_modifier(Modifier::REVERSED))
                } else {
                    Line::from(text)
                }
            })
            .collect();
        frame.render_widget(
            Paragraph::new(lines).block(
                ratatui::widgets::Block::bordered()
                    .title("排序（字母=直接应用 空格=换向 回车=应用 Esc=取消）"),
            ),
            popup,
        );
        return;
    }
    if let InputMode::ArchiveDialog {
        input,
        field,
        format_idx,
        checksum_idx,
    } = mode
    {
        // M25-4：归档创建对话框（居中遮罩，M18-1 SortDialog 同款）。
        let area = frame.area();
        frame.render_widget(ratatui::widgets::Clear, area);
        frame.render_widget(
            ratatui::widgets::Block::new().style(Style::new().bg(Color::Black)),
            area,
        );
        let popup = context_menu_rect(area);
        let fmt = ARCHIVE_FORMATS
            .iter()
            .enumerate()
            .map(|(i, f)| {
                if i == *format_idx {
                    f.ext().trim_start_matches('.').to_owned()
                } else {
                    " ".to_owned() + f.ext().trim_start_matches('.')
                }
            })
            .collect::<Vec<_>>()
            .join("  ");
        let cks = ARCHIVE_CHECKSUMS
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let label = match c {
                    ops::Checksum::Md5 => "md5",
                    ops::Checksum::Sha256 => "sha256",
                    ops::Checksum::None => "none",
                };
                if i == *checksum_idx {
                    label.to_owned()
                } else {
                    format!(" {label}")
                }
            })
            .collect::<Vec<_>>()
            .join("  ");
        let line_name = if *field == 0 {
            Line::from(format!("名称: {input}▏"))
                .style(Style::new().add_modifier(Modifier::REVERSED))
        } else {
            Line::from(format!("名称: {input}"))
        };
        let line_fmt = if *field == 1 {
            Line::from(format!("格式: {fmt}")).style(Style::new().add_modifier(Modifier::REVERSED))
        } else {
            Line::from(format!("格式: {fmt}"))
        };
        let line_cks = if *field == 2 {
            Line::from(format!("校验: {cks}")).style(Style::new().add_modifier(Modifier::REVERSED))
        } else {
            Line::from(format!("校验: {cks}"))
        };
        let lines = vec![
            line_name,
            line_fmt,
            line_cks,
            Line::from("Tab=切字段 ←→/空格=换值 回车=创建 Esc=取消"),
        ];
        frame.render_widget(
            Paragraph::new(lines).block(ratatui::widgets::Block::bordered().title("归档创建")),
            popup,
        );
        return;
    }
    if let InputMode::HistoryMenu { cursor } = mode {
        // M16 NAV-2：历史菜单浮窗（居中遮罩；条目 live 取自 history[..=hist_pos]）。
        let area = frame.area();
        frame.render_widget(ratatui::widgets::Clear, area);
        frame.render_widget(
            ratatui::widgets::Block::new().style(Style::new().bg(Color::Black)),
            area,
        );
        let popup = centered_rect(area);
        let tab_hist = state.active_tab();
        let lines: Vec<Line> = tab_hist
            .history
            .iter()
            .take(tab_hist.hist_pos + 1)
            .enumerate()
            .map(|(idx, loc)| {
                // 标记放行首：长路径被浮窗宽度截断时标记仍可见（Windows
                // 路径可达数十列，行尾标记会被截掉）。
                let mut text = if idx == tab_hist.hist_pos {
                    "● ".to_owned()
                } else {
                    String::new()
                };
                text.push_str(&loc.display_path());
                if idx == *cursor {
                    Line::from(text).style(Style::new().add_modifier(Modifier::REVERSED))
                } else {
                    Line::from(text)
                }
            })
            .collect();
        frame.render_widget(
            Paragraph::new(lines).block(
                ratatui::widgets::Block::bordered()
                    .title("目录历史（Enter=导航 dd=移除 Esc=关闭）"),
            ),
            popup,
        );
        return;
    }
    if let InputMode::ContextMenu { cursor } = mode {
        // FEAT-3：右键菜单浮窗（固定尺寸居中，遮罩背景）。
        let area = frame.area();
        frame.render_widget(ratatui::widgets::Clear, area);
        frame.render_widget(
            ratatui::widgets::Block::new().style(Style::new().bg(Color::Black)),
            area,
        );
        let popup = context_menu_rect(area);
        let lines: Vec<Line> = CONTEXT_MENU
            .iter()
            .enumerate()
            .map(|(idx, label)| {
                if idx == *cursor {
                    Line::from((*label).to_owned())
                        .style(Style::new().add_modifier(Modifier::REVERSED))
                } else {
                    Line::from((*label).to_owned())
                }
            })
            .collect();
        frame.render_widget(
            Paragraph::new(lines).block(
                ratatui::widgets::Block::bordered().title("操作菜单（Enter=执行 Esc=关闭）"),
            ),
            popup,
        );
        return;
    }
    if let InputMode::JumpSelect {
        input,
        cursor,
        entries,
        kind,
        normal,
        g_pending: _,
    } = mode
    {
        // M8-12：跳转选择器覆盖层（纯只读；分级与 pick 一致）。
        // M21-E8：80%×80% 居中、无全屏遮罩（双 pane 可见）；标题标示
        // 搜索/Normal 子态。
        let area = frame.area();
        let popup = jump_rect(area);
        let base = match kind {
            JumpKind::Unified => "跳转（合一 · 全局时钟）",
            JumpKind::Dir => "跳转（仅目录 · dir 时钟）",
            JumpKind::File => "跳转（仅文件 · file 时钟）",
            JumpKind::Recent => "跳转（近期 · 时间倒序）",
        };
        let mode_mark = if *normal {
            " · Normal（j/k/gg/G/l）"
        } else {
            " · /搜索"
        };
        let title = format!("{base}{mode_mark}");
        let visible = filter_jump(entries, input);
        let items: Vec<ListItem> = visible
            .iter()
            .enumerate()
            .map(|(idx, e)| {
                let topic_mark = if e.topic == "dir" {
                    "[目录]"
                } else {
                    "[文件]"
                };
                let bm = if e.keyword.is_empty() {
                    String::new()
                } else {
                    format!(" ✦{}", e.keyword)
                };
                ListItem::new(Line::from(format!("{topic_mark}{bm} {}", e.path))).style(
                    if idx == *cursor {
                        Style::new().add_modifier(Modifier::REVERSED)
                    } else {
                        Style::new()
                    },
                )
            })
            .collect();
        let inner: [ratatui::prelude::Rect; 3] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .areas(
            ratatui::widgets::Block::bordered()
                .title(title.clone())
                .inner(popup),
        );
        frame.render_widget(ratatui::widgets::Block::bordered().title(title), popup);
        frame.render_widget(Paragraph::new(Line::from(format!("/{}", input))), inner[0]);
        let list = ratatui::widgets::List::new(items)
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
        let mut list_state = ratatui::widgets::ListState::default().with_selected(Some(*cursor));
        frame.render_stateful_widget(list, inner[1], &mut list_state);
        frame.render_widget(
            Paragraph::new(if *normal {
                "j/k=移动 gg/G=顶尾 l/Enter=选定 输入字符=搜索 Esc=关闭"
            } else {
                "输入=筛选 方向键=移动 Enter=选定 Esc=Normal"
            }),
            inner[2],
        );
        return;
    }
    let status = match mode {
        InputMode::JobDetail { .. } => {
            Line::from(String::new()) // 详情覆盖层提前 return，不可达（M28-3）
        }
        InputMode::Queue { .. } => {
            Line::from(String::new()) // 队列覆盖层提前 return，不可达（M36-7）
        }
        InputMode::ViewerSearch { input } => Line::from(format!("/{}（查看器）", input)),
        InputMode::ArchiveDialog { input, field, .. } => {
            // M25-4：对话框激活字段指示（覆盖层提前 return，实际不可达）。
            let f = match field {
                1 => "格式",
                2 => "校验",
                _ => "名称",
            };
            Line::from(format!(":{input}（{f}）"))
        }
        InputMode::RemotePass { name, input, .. } => {
            // M22-3：masked 渲染（`*` 逐字符替换，同 AskPass）。
            let masked = "*".repeat(input.chars().count());
            Line::from(format!(
                "远端口令（{name}）: {masked} [Enter=连接 Esc=取消]"
            ))
        }
        InputMode::Rename { input, pos, .. } => {
            // M21-E4：插入光标可视化（'▏' 标记当前插入位）。
            let chars: Vec<char> = input.chars().collect();
            let mut s = String::new();
            for (i, c) in chars.iter().enumerate() {
                if i == *pos {
                    s.push('▏');
                }
                s.push(*c);
            }
            if *pos >= chars.len() {
                s.push('▏');
            }
            Line::from(format!("重命名: {s} [Enter=应用 Esc=取消]"))
        }
        InputMode::SortDialog { .. } => Line::from(String::new()), // 排序对话框提前 return，不可达
        InputMode::ContextMenu { .. } => Line::from(String::new()), // 菜单覆盖层提前 return，不可达
        InputMode::HistoryMenu { .. } => Line::from(String::new()), // 历史菜单提前 return，不可达
        InputMode::Command { input, .. } => Line::from(format!(":{input}")),
        InputMode::Search { input, .. } => {
            // FIX-1：输入期实时匹配计数；无匹配 0/0 并提示。
            let query = input.trim();
            if query.is_empty() {
                Line::from(format!("/{input}"))
            } else {
                let (cur, total) = tab.search_status(query);
                let mut text = format!("/{input}  [{cur}/{total}]");
                if total == 0 {
                    text.push_str("  无匹配");
                }
                Line::from(text)
            }
        }
        InputMode::Filter { input } => Line::from(format!("={input}")),
        InputMode::HelpView { .. } | InputMode::HelpSearch { .. } => {
            Line::from(String::new()) // 帮助视图提前 return，不可达
        }
        InputMode::JumpSelect { .. } => Line::from(String::new()), // 选择器覆盖层提前 return，不可达
        InputMode::Jobs { .. } => Line::from(String::new()), // 作业菜单覆盖层提前 return，不可达
        InputMode::TrashMenu { .. } => Line::from(String::new()), // 暂存区覆盖层提前 return，不可达
        InputMode::FileInfo { .. } => Line::from(String::new()), // 详情覆盖层提前 return，不可达
        InputMode::Confirm {
            path,
            permanent,
            remote,
        } => {
            let prefix = if remote.is_some() { "远端" } else { "" };
            if *permanent {
                // M13-3：DD 永久删除强提示（不可恢复）。
                Line::from(format!(
                    "**{prefix}永久删除** {}（不可恢复）? [y/N]",
                    path.display()
                ))
            } else {
                Line::from(format!("删除{prefix} {}? [y/N]", path.display()))
            }
        }
        InputMode::ConfirmTagDelete => {
            let n = tab.tagged_count();
            Line::from(format!("删除 {n} 个标记文件? [y/N]"))
        }
        InputMode::AskPass { input, op } => {
            // M12-3：密码 masked 渲染（`*` 逐字符替换），带操作描述。
            let masked = "*".repeat(input.chars().count());
            Line::from(format!(
                "sudo 密码: {masked} [Enter=确认 Esc=取消] {}",
                op.describe()
            ))
        }
        InputMode::Normal => {
            // M21-F1：状态栏不再显示路径（路径移到 pane 顶部），底部显示
            // 选中项详情——名称 [类型] 大小 权限 时间。
            let selected = match tab.entries.get(tab.cursor) {
                Some(item) => {
                    let class = item.filetype_class();
                    let size = if item.is_dir {
                        String::new()
                    } else {
                        format!(" {}", crate::tab::format_size(item.size))
                    };
                    format!(
                        " {} [{}]{} {} {}",
                        item.name,
                        class.label(),
                        size,
                        item.perm,
                        crate::tab::format_mtime(item.mtime)
                    )
                }
                None => String::new(),
            };
            let mut spans = vec![
                Span::styled(selected, Style::new().add_modifier(Modifier::BOLD)),
                Span::from(format!(" | {} items", tab.entries.len())),
            ];
            if let Some(r) = &tab.results {
                // M17 SRCH：结果视图指示（类别 + 查询 + 条数 + 截断）。
                let kind = match r.kind {
                    crate::search::SearchKind::Find => "find",
                    crate::search::SearchKind::Grep => "grep",
                    crate::search::SearchKind::Compare => "compare",
                    crate::search::SearchKind::RenamePreview => "rename-preview",
                };
                // M24-8：compare 无查询词，状态栏只显示条数。
                // M28-2：rename 预览——追加剧键提示（y 应用整批 / Esc 取消）。
                if r.kind == crate::search::SearchKind::RenamePreview {
                    spans.push(Span::from("  [y=应用整批 / Esc=取消]"));
                }
                spans.push(if r.query.is_empty() {
                    Span::from(format!(
                        "  [{}: {} 项{}]",
                        kind,
                        r.items.len(),
                        if r.truncated { " 已截断" } else { "" }
                    ))
                } else {
                    Span::from(format!(
                        "  [{}: {} — {} 项{}]",
                        kind,
                        r.query,
                        r.items.len(),
                        if r.truncated { " 已截断" } else { "" }
                    ))
                });
            }
            // M25-6：git 分支 + diff 摘要（per-dir 异步缓存；非仓库/未就绪隐藏）。
            if state.config.ui.git_column
                && let Location::Physical(dir) = &tab.location
                && let Some(Some(st)) = state.git_cache.get(dir)
                && let Some(branch) = &st.branch
            {
                let text = match st.diff_summary.as_deref() {
                    Some(diff) if !diff.is_empty() => format!("  [{branch} {diff}]"),
                    _ => format!("  [{branch}]"),
                };
                spans.push(Span::from(text).style(Style::new().add_modifier(Modifier::DIM)));
            }
            if let Some(specs) = &tab.sort {
                // M18-1/M18-2：排序指示（多字段 join）。
                let text = specs
                    .iter()
                    .map(|s| {
                        format!(
                            "{} {}",
                            match s.field {
                                crate::tab::SortField::Name => "name",
                                crate::tab::SortField::Natural => "natural",
                                crate::tab::SortField::Mtime => "mtime",
                                crate::tab::SortField::Size => "size",
                                crate::tab::SortField::Ext => "ext",
                            },
                            if s.desc { "desc" } else { "asc" }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                spans.push(Span::from(format!("  [sort: {text}]")));
            }
            if let Some(s) = &tab.search {
                if s.query.is_empty() {
                    spans.push(Span::from(format!("  /{}", s.query)));
                } else {
                    // FIX-1：已提交搜索的匹配计数（n/N 移动后随光标同步）。
                    let (cur, total) = tab.search_status(&s.query);
                    let mut text = format!("  /{}  [{cur}/{total}]", s.query);
                    if total == 0 {
                        text.push_str("  无匹配");
                    }
                    spans.push(Span::from(text));
                }
            }
            if let Some(f) = &tab.filter {
                // M4-2：过滤指示。
                spans.push(Span::from(format!("  [filter: {f}]")));
            }
            if let Some((lo, hi)) = visual_range {
                // M3-1：可视选择指示（含行数，闭区间）。
                spans.push(Span::from(format!("  VISUAL ({} selected)", hi - lo + 1)));
            }
            let tagged = tab.tagged_count();
            if tagged > 0 {
                // M3-2：tag 计数指示。
                spans.push(Span::from(format!("  {tagged} tagged")));
            }
            if let Some(m) = msg {
                spans.push(Span::styled(
                    format!("  [{m}]"),
                    Style::new().add_modifier(Modifier::BOLD),
                ));
            }
            Line::from(spans)
        }
    };
    // M27-2：状态行——vifm 模式套 StatusLine 样式（黑字白底回退）。
    let status_para = if state.config.ui.pane_borders {
        Paragraph::new(status)
    } else {
        Paragraph::new(status).style(vifm_style("StatusLine"))
    };
    frame.render_widget(status_para, status_area);
}
