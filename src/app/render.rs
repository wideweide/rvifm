use super::*;

/// M47-9：列表项左侧留白（对齐 vifm 的栏内间距观感）。前导空白属于列表项
/// 本身，整行高亮（List highlight_style / 右栏 item 级 REVERSED）自然覆盖它，
/// 不留彩色断层。
pub(crate) const LIST_LEADING_BLANK: &str = " ";

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

/// M21-F1：整体几何——标签栏 1 行 + 主区 + 状态栏 1 行。M39-6：主区按
/// `[ui] layout`——Single = 左全宽（右零宽）；PreviewBottom 且 view 模式 =
/// 上下半分（列表上半、查看器下半经 `viewer_rect`）；其余左右半分。
/// 渲染与鼠标命中测试共用同一推导，避免两处几何漂移。
pub(crate) fn pane_layout(
    area: ratatui::layout::Rect,
    layout: crate::config::LayoutMode,
    viewing: bool,
) -> [ratatui::layout::Rect; 4] {
    let [tab_bar, main, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);
    let (left, right) = match (layout, viewing) {
        (crate::config::LayoutMode::Single, _) => (
            main,
            ratatui::layout::Rect {
                x: main.x + main.width,
                y: main.y,
                width: 0,
                height: main.height,
            },
        ),
        (crate::config::LayoutMode::PreviewBottom, true) => {
            let [top, _bottom] =
                Layout::vertical([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).areas(main);
            let [l, _gap, r] = Layout::horizontal([
                Constraint::Ratio(1, 2),
                Constraint::Length(1),
                Constraint::Ratio(1, 2),
            ])
            .areas(top);
            (l, r)
        }
        _ => {
            let [l, _gap, r] = Layout::horizontal([
                Constraint::Ratio(1, 2),
                Constraint::Length(1),
                Constraint::Ratio(1, 2),
            ])
            .areas(main);
            (l, r)
        }
    };
    [tab_bar, left, right, status]
}

/// M52-1：frecency 专用「预览区位置」语义。默认 `fixed_side=false` → 焦点栏
/// 搜索/列表、对侧预览；显式 `true` 时预览恒在右栏。`:view` 不走本函数（恒对侧）。
/// 返回 (列表区, 预览区)，几何与 `pane_layout(Dual)` 同源。
pub(crate) fn preview_layout_areas(
    area: ratatui::layout::Rect,
    focus: crate::tab::Focus,
    fixed_side: bool,
) -> (ratatui::layout::Rect, ratatui::layout::Rect) {
    let [_, left, right, _] = pane_layout(area, crate::config::LayoutMode::Dual, false);
    if fixed_side || focus == crate::tab::Focus::Left {
        (left, right)
    } else {
        (right, left)
    }
}

/// M39-6 / M46-1：view 模式查看器占位（Single = 全宽主区 / PreviewBottom =
/// 主区下半；Dual/PreviewRight 经 pane 分派，返回 None）。参数 `main` 必须是
/// `pane_layout` 产出的主区（左右 pane 的并集），与列表区同源——避免再用
/// 整帧重算导致查看器与（被 tutor 面板收缩后的）pane 几何错位、图片外溢。
pub(crate) fn viewer_rect(
    main: ratatui::layout::Rect,
    layout: crate::config::LayoutMode,
    viewing: bool,
) -> Option<ratatui::layout::Rect> {
    if !viewing {
        return None;
    }
    match layout {
        crate::config::LayoutMode::Single => Some(main),
        crate::config::LayoutMode::PreviewBottom => {
            let [_top, bottom] =
                Layout::vertical([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).areas(main);
            Some(bottom)
        }
        _ => None,
    }
}

/// M39-6：pane 是否被查看器覆盖（命中测试守卫）——Single 全覆盖；
/// Dual 非焦点侧（M34-4 对侧预览）；PreviewRight 右侧；PreviewBottom
/// 两 pane 可见（下半为查看器，不在 pane 区域内）。
pub(crate) fn pane_covered_by_viewer(
    layout: crate::config::LayoutMode,
    viewing: bool,
    focus: crate::tab::Focus,
    pane: crate::tab::Focus,
) -> bool {
    if !viewing {
        return false;
    }
    match layout {
        crate::config::LayoutMode::Single => true,
        crate::config::LayoutMode::PreviewRight => pane == crate::tab::Focus::Right,
        crate::config::LayoutMode::PreviewBottom => false,
        crate::config::LayoutMode::Dual => pane != focus,
    }
}

/// M45-1c：标签样式——激活标签灰白底反色字（与 pane 路径色 `TopLineSel` 解耦，
/// 不再使用 accent 色），非激活标签深灰底浅色字。borders 模式保留原 REVERSED
/// 语义（M27-2）。纯函数便于测试（不读全局主题）。
pub(crate) fn tab_label_style(active: bool, borders: bool) -> Style {
    if borders {
        return if active {
            Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            Style::new()
        };
    }
    if active {
        Style::new()
            .bg(Color::White)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD)
    } else {
        // M81：非焦点标签无背景色，文字统一白色。
        Style::new().fg(Color::White)
    }
}

/// M45-1b：标签栏文本与可见窗口——render 与鼠标命中共用（同源，避免分叉）。
/// 返回 (labels, visible)；宽按字符数近似，CJK 全宽忽略（SIMPLIFIED，同 M39-6）。
pub(crate) fn tab_bar_labels_and_visible(
    state: &AppState,
    width: u16,
) -> (Vec<String>, Vec<usize>) {
    let labels: Vec<String> = state
        .tabs
        .iter()
        .enumerate()
        .map(|(idx, t)| {
            // M53-1：标签显示**焦点栏全路径**，中括号包裹；随焦点切换同步变化。
            let path = match t.focused_location() {
                crate::tab::Location::Physical(p) => p.to_string_lossy().replace('\\', "/"),
                other => other.display_path().replace('\\', "/"),
            };
            let label = t.name.clone().unwrap_or(path);
            format!("[{}:{}] ", idx + 1, label)
        })
        .collect();
    let widths: Vec<usize> = labels.iter().map(|l| l.chars().count()).collect();
    let total: usize = widths.iter().sum();
    let budget = width as usize;
    let mut visible: Vec<usize> = (0..labels.len()).collect();
    if total > budget && !labels.is_empty() {
        // 溢出：自活动标签向两侧交替扩展，直至预算用尽。
        let active = state.active;
        visible.clear();
        let mut used = 0usize;
        let mut lo = active.min(labels.len().saturating_sub(1));
        let mut hi = lo;
        // 活动标签必须可见（自身超宽时独占——后续条目截断由渲染层裁剪）。
        used += widths[active];
        let mut push_left = lo > 0;
        let mut push_right = hi + 1 < labels.len();
        while used < budget && (push_left || push_right) {
            if push_right {
                hi += 1;
                used += widths[hi];
                push_right = hi + 1 < labels.len();
            }
            if used < budget && push_left {
                lo = lo.saturating_sub(1);
                used += widths[lo];
                push_left = lo > 0;
            }
            if !push_left && !push_right {
                break;
            }
        }
        visible = (lo..=hi).collect();
    }
    (labels, visible)
}

/// M45-1b：鼠标点击标签栏列 → tab 索引（render 同源 geometry）。
/// 点击命中某可见标签文本区间（含左侧 "‹ " 指示偏移）→ 该 tab 索引；
/// 点击空白/指示符/溢出区 → None。
pub(crate) fn tab_bar_hit_test(state: &AppState, width: u16, col: u16) -> Option<usize> {
    let (labels, visible) = tab_bar_labels_and_visible(state, width);
    if visible.is_empty() {
        return None;
    }
    let mut x = 0usize;
    // 左侧溢出指示 "‹ " 占 2 列（与 render 同源）。
    if visible[0] > 0 {
        x += 2;
    }
    let c = col as usize;
    for &idx in &visible {
        let w = labels[idx].chars().count();
        if c >= x && c < x + w {
            return Some(idx);
        }
        x += w;
    }
    None
}

/// M39-6：标签栏 spans——命名标签（Tab.name 覆盖目录名）+ 溢出滚动
/// （宽度不足时显示活动标签附近窗口，两侧 ‹/› 指示；宽度按字符数近似，
/// CJK 全宽忽略——SIMPLIFIED）。
/// M81：焦点标签白色背景 + 反色文字。
fn focused_tab_style() -> Style {
    Style::new()
        .bg(Color::White)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD)
}

pub(crate) fn tab_bar_spans(state: &AppState, width: u16, borders: bool) -> Vec<Span<'static>> {
    let (labels, visible) = tab_bar_labels_and_visible(state, width);
    let mut out: Vec<Span> = Vec::new();
    if !visible.is_empty() && visible[0] > 0 {
        out.push(Span::styled("‹ ", Style::new().add_modifier(Modifier::DIM)));
    }
    for &idx in &visible {
        let label = labels[idx].clone();
        // M45-1c：标签样式独立于路径行（灰白底反色字），不再复用 TopLine/
        // TabLine 的主题 accent 色；borders 模式仍走 REVERSED。
        let style = if !borders && idx == state.active {
            focused_tab_style()
        } else {
            tab_label_style(idx == state.active, borders)
        };
        out.push(Span::styled(label, style));
    }
    if let Some(last) = visible.last()
        && *last + 1 < labels.len()
    {
        out.push(Span::styled(" ›", Style::new().add_modifier(Modifier::DIM)));
    }
    out
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

/// M47-2/M65-补：pane 顶部路径行文本——路径后追加当前 pane 的过滤状态；
/// 按可用宽度从左侧省略路径，保证过滤标记不会被挤出视口。
pub(crate) fn path_line_text_fit(path: String, filter: Option<&str>, width: u16) -> String {
    let Some(f) = filter else {
        return path;
    };
    let suffix = format!("  [filter: {f}]");
    let max = (width as usize).saturating_sub(suffix.chars().count());
    let chars: Vec<char> = path.chars().collect();
    if chars.len() <= max {
        return format!("{path}{suffix}");
    }
    let keep = max.saturating_sub(1);
    let tail: String = chars[chars.len().saturating_sub(keep)..].iter().collect();
    format!("…{tail}{suffix}")
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

/// M80-2：选中项背景同源——焦点用主题选中背景，失焦取更浅一档。
pub(crate) fn selected_row_style(focused: bool) -> Style {
    let base = crate::theme::style("selected")
        .bg
        .or_else(|| vifm_style("TopLineSel").bg)
        .unwrap_or(Color::White);
    if focused {
        // 焦点侧保留反显语义（与既有列表/测试一致）。
        return Style::new().add_modifier(Modifier::REVERSED);
    }
    let bg = match base {
        Color::Rgb(r, g, b) => Color::Rgb(
            r.saturating_add(255) / 2,
            g.saturating_add(255) / 2,
            b.saturating_add(255) / 2,
        ),
        Color::Black | Color::DarkGray => Color::Gray,
        Color::Gray => Color::White,
        other => other,
    };
    Style::new().bg(bg)
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
        // M81：失焦边框用灰色（与 :diff 弹出框同源）。
        Style::new().fg(Color::Gray)
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
    use unicode_width::UnicodeWidthChar;
    if budget == 0 {
        return String::new();
    }
    let chars: Vec<char> = name.chars().collect();
    let width = |cs: &[char]| -> usize { cs.iter().map(|c| c.width().unwrap_or(0)).sum() };
    if width(&chars) <= budget {
        return name.to_owned();
    }
    if budget <= 1 {
        return chars.into_iter().take(budget).collect();
    }
    // M88：保留尾部最多两段扩展名链（.pdf.md5 / .tar.gz）。
    let n = chars.len();
    let mut ext_start = n;
    let mut pos = n;
    for _ in 0..2 {
        let mut j = pos;
        while j > 0 && chars[j - 1].is_ascii_alphanumeric() {
            j -= 1;
        }
        if j == pos || j == 0 || chars[j - 1] != '.' {
            break;
        }
        ext_start = j - 1;
        pos = j - 1;
    }
    let ext: Vec<char> = chars[ext_start..].to_vec();
    let ext_w = width(&ext);
    if ext_w + 1 >= budget {
        // 扩展名本身放不下 → 从右往左按宽度保留尾部字符。
        let mut out = Vec::new();
        let mut used = 0usize;
        for c in chars.iter().rev() {
            let w = c.width().unwrap_or(0);
            if used + w > budget {
                break;
            }
            out.push(*c);
            used += w;
        }
        out.reverse();
        return out.into_iter().collect();
    }
    let head_budget = budget - 1 - ext_w;
    let mut head = Vec::new();
    let mut used = 0usize;
    for c in &chars[..ext_start] {
        let w = c.width().unwrap_or(0);
        if used + w > head_budget {
            break;
        }
        head.push(*c);
        used += w;
    }
    let mut out: String = head.into_iter().collect();
    out.push('…');
    out.extend(ext);
    out
}

/// M99-4：图标降级——关闭 glyph 时用纯文本标记，终端缺 Nerd Font 不显示豆腐块。
pub(crate) fn icon_prefix(icon: char, is_dir: bool, enabled: bool) -> String {
    if enabled {
        format!("{icon} ")
    } else if is_dir {
        "[D] ".to_owned()
    } else {
        "[F] ".to_owned()
    }
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

/// M46-7：不透明帮助覆盖层（View 与 Search 共用）。`input = Some(...)` 时
/// 在可视区底部渲染 `/输入` 行，保证 `/` 搜索过程可见且内容不被清空。
fn render_help_overlay(
    frame: &mut Frame,
    scroll: usize,
    search: Option<&str>,
    input: Option<&String>,
) {
    let area = frame.area();
    frame.render_widget(ratatui::widgets::Clear, area);
    // 显式铺一层不透明背景（Paragraph 的 style 不保证填满空白单元）。
    frame.render_widget(
        ratatui::widgets::Block::default().style(Style::new().bg(Color::Black)),
        area,
    );
    let lines = help_lines();
    let title = match (input, search) {
        (Some(q), _) => format!("帮助  /{q}（Enter 搜索  Esc 返回）"),
        (_, Some(q)) => format!("帮助  /{q}（n/N 跳转）"),
        _ => "帮助（j/k 滚动  / 搜索  n/N 跳转  q 返回）".to_owned(),
    };
    // 搜索输入期预留末行给输入框，避免挤压帮助正文。
    let capacity = area.height.saturating_sub(2) as usize;
    let body = capacity.saturating_sub(input.is_some() as usize);
    let mut visible: Vec<Line> = lines
        .iter()
        .skip(scroll)
        .take(body)
        .map(|l| {
            // M48-12：查看器式高亮——`== 段名 ==` 标题行着色，正文保持原样。
            if l.starts_with("==") && l.ends_with("==") {
                Line::from(Span::styled(
                    (*l).to_owned(),
                    Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from((*l).to_owned())
            }
        })
        .collect();
    if let Some(q) = input {
        visible.push(Line::from(vec![
            Span::styled("/".to_owned(), Style::new().fg(Color::Yellow)),
            Span::from(q.clone()),
            Span::from("█".to_owned()),
        ]));
    }
    frame.render_widget(
        Paragraph::new(visible)
            .block(ratatui::widgets::Block::bordered().title(title))
            .style(Style::new().bg(Color::Black)),
        area,
    );
}

/// FEAT-1/FEAT-3：条目行命中测试——鼠标行 → 条目索引；不在条目区
/// （边框/`..` 行/右 pane/标签栏/状态栏/越界）返回 None。
/// 几何与 render 同源（M21-F1 pane_layout + 左 pane 边框内区）。
pub(crate) fn hit_entry_index(
    state: &AppState,
    mouse: &MouseEvent,
    area: ratatui::layout::Rect,
) -> Option<usize> {
    let tab = state.active_tab();
    // M39-6：查看器覆盖的 pane 不响应命中（覆盖判定与渲染同源）。
    if pane_covered_by_viewer(
        state.config.ui.layout(),
        tab.view_mode,
        tab.focus,
        crate::tab::Focus::Left,
    ) {
        return None;
    }
    let [_, left, _, _] = pane_layout(area, state.config.ui.layout(), tab.view_mode);
    let inner = left_list_area(state.config.ui.pane_borders, left);
    let has_parent = tab.has_parent_row() as usize;
    if mouse.column < inner.x
        || mouse.column >= inner.x + inner.width
        || mouse.row < inner.y + has_parent as u16
        || mouse.row >= inner.y + inner.height
    {
        return None;
    }
    let offset = tab.scroll;
    let idx = (mouse.row - inner.y - has_parent as u16) as usize;
    idx.checked_add(offset)?;
    Some(idx.min(tab.entries.len().saturating_sub(1)))
}

/// M41-3：右 pane 命中索引——与左 pane 一样计入可见的 `..` 行，条目从其后
/// 开始；offset 取 `Pane.scroll`（与渲染 ListState 同源，M41-4 有状态）。
/// M39-6：布局感知 + 查看器覆盖守卫。空列表返回 None。
pub(crate) fn hit_right_index(
    state: &AppState,
    mouse: &MouseEvent,
    area: ratatui::layout::Rect,
) -> Option<usize> {
    let tab = state.active_tab();
    if pane_covered_by_viewer(
        state.config.ui.layout(),
        tab.view_mode,
        tab.focus,
        crate::tab::Focus::Right,
    ) {
        return None;
    }
    let [_, _, right, _] = pane_layout(area, state.config.ui.layout(), tab.view_mode);
    let inner = left_list_area(state.config.ui.pane_borders, right);
    if inner.width == 0
        || mouse.column < inner.x
        || mouse.column >= inner.x + inner.width
        || mouse.row < inner.y
        || mouse.row >= inner.y + inner.height
    {
        return None;
    }
    let parent = tab.right.has_parent_row();
    if parent && mouse.row == inner.y {
        return None;
    }
    if tab.right.entries.is_empty() {
        return None;
    }
    let offset = tab.right.scroll;
    let idx = (mouse.row - inner.y - parent as u16) as usize;
    idx.checked_add(offset)?;
    Some(idx.min(tab.right.entries.len().saturating_sub(1)))
}

/// M45-9c：归档创建对话框矩形——比通用右键菜单更宽（64 列上限），纯函数
/// 便于测试（小终端退化为终端宽度）。
pub(crate) fn archive_dialog_rect(area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    let w = area.width.saturating_sub(4).clamp(area.width.min(40), 64);
    let h = 6u16.min(area.height);
    ratatui::layout::Rect {
        x: area.x + area.width.saturating_sub(w) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width: w.min(area.width),
        height: h,
    }
}

/// M45-9c：名称字段显示去后缀（后缀由格式字段右侧自动展示）。仅剥除已知
/// 归档扩展名，避免误删普通文件名中的点。纯函数便于测试。
pub(crate) fn strip_archive_ext(name: &str) -> &str {
    const EXTS: &[&str] = &[
        ".tar.gz", ".tar.bz2", ".tar.zst", ".tar.xz", ".zip", ".tar", ".tgz", ".7z", ".rar",
        ".cab", ".lzh", ".deb", ".apk",
    ];
    let lower = name.to_ascii_lowercase();
    for ext in EXTS {
        if lower.ends_with(ext) {
            return &name[..name.len() - ext.len()];
        }
    }
    name
}

/// M45-8d：向导模块列表左侧进度标签——`掌握数/总数` 百分比。纯函数便于
/// 测试（不读宿主进度文件）。
pub(crate) fn tutor_progress_label(total: usize, mastered: usize) -> String {
    match mastered.min(total).saturating_mul(100).checked_div(total) {
        Some(pct) => format!("{pct:>3}%"),
        None => " --%".to_owned(),
    }
}

/// M45-8c：Tutor 底部面板高度——render 与输入循环视口同步共用（纯函数），
/// 列表渲染区与 `sync_viewport_scroll` 都须扣除它，避免 `G` 跳到底部条目
/// 被面板遮住。
pub(crate) fn tutor_panel_height(total_height: u16) -> u16 {
    // M74-1：给换行后的长提示更多行空间。
    (total_height / 4).clamp(8, 14)
}

/// M56-3：向导对话框几何——居中的 80%×80%；极小终端（≤40 列或 ≤10 行）
/// 降级为全屏，避免对话框过小无法阅读。纯函数便于测试。
pub(crate) fn tutor_dialog_rect(area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    use ratatui::layout::Rect;
    if area.width <= 40 || area.height <= 10 {
        return area;
    }
    let w = (area.width * 80 / 100).max(1);
    let h = (area.height * 80 / 100).max(1);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

/// M100-1：窄屏对话框用全宽（保留垂直比例），宽屏走原 80% 居中逻辑。
pub(crate) fn tutor_dialog_rect_for(
    area: ratatui::layout::Rect,
    narrow: bool,
) -> ratatui::layout::Rect {
    use ratatui::layout::Rect;
    if !narrow {
        return tutor_dialog_rect(area);
    }
    if area.height <= 10 {
        return area;
    }
    let h = (area.height * 80 / 100).max(1);
    Rect {
        x: area.x,
        y: area.y + (area.height - h) / 2,
        width: area.width,
        height: h,
    }
}

/// M56-4：总体学习进度 `(已完成模块, 模块总数, 已掌握功能点, 功能点总数, 百分比)`。
/// 按稳定模块 ID 从进度表聚合，不依赖 rvifm 版本。
pub(crate) fn tutor_overall_progress(
    progress: &crate::tutor::TutorProgress,
) -> (usize, usize, usize, usize, usize) {
    let modules = crate::tutor::builtin_modules();
    let mut total_items = 0usize;
    let mut mastered_items = 0usize;
    let mut done_modules = 0usize;
    for m in &modules {
        let total = m.items.len();
        let mastered = progress
            .get(&m.id)
            .map(|p| {
                p.mastered
                    .iter()
                    .filter(|id| m.items.iter().any(|i| &i.id == *id))
                    .count()
            })
            .unwrap_or(0);
        total_items += total;
        mastered_items += mastered;
        if total > 0 && mastered >= total {
            done_modules += 1;
        }
    }
    let pct = mastered_items
        .saturating_mul(100)
        .checked_div(total_items)
        .unwrap_or(0);
    (
        done_modules,
        modules.len(),
        mastered_items,
        total_items,
        pct,
    )
}

/// M64-1：修仙境界（符号 + 中文 + 小阶段）。纯函数，不落盘。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TutorRank {
    pub symbol: &'static str,
    pub name: &'static str,
    pub stage: &'static str,
}

impl TutorRank {
    #[allow(dead_code)]
    pub fn label(&self) -> String {
        if self.stage.is_empty() {
            format!("{} {}", self.symbol, self.name)
        } else {
            format!("{} {}{}", self.symbol, self.name, self.stage)
        }
    }
}

/// M86-4：境界图标按级别着色（低阶冷色 → 高阶暖色），不引入新依赖。
pub(crate) fn tutor_rank_color(name: &str) -> Color {
    match name {
        "入门" | "炼气" => Color::DarkGray,
        "筑基" | "金丹" => Color::LightCyan,
        "元婴" | "化神" => Color::LightGreen,
        "炼虚" | "合体" => Color::LightYellow,
        "大乘" | "渡劫" => Color::LightRed,
        "真仙" => Color::LightMagenta,
        _ => Color::Reset,
    }
}

/// 境界表：(符号, 中文, 最低掌握率%)。真仙的阈值 101 表示只能由「100% 且零错题」达成。
const TUTOR_RANKS: &[(&str, &str, usize)] = &[
    ("○", "入门", 0),
    ("⚗", "炼气", 1),
    ("▤", "筑基", 5),
    ("●", "金丹", 15),
    ("◉", "元婴", 30),
    ("✧", "化神", 45),
    ("◇", "炼虚", 60),
    ("◆", "合体", 75),
    ("✦", "大乘", 90),
    ("⚡", "渡劫", 100),
    ("★", "真仙", 101),
];

/// M64-1：按掌握率推算境界；筑基严格绑定 `core_nav` 全部掌握；
/// 100% 且有错题记录封顶渡劫，100% 且零错题才为真仙。
pub(crate) fn tutor_rank(progress: &crate::tutor::TutorProgress) -> TutorRank {
    let (_, _, mastered, total, pct) = tutor_overall_progress(progress);
    if total == 0 || mastered == 0 {
        return TutorRank {
            symbol: TUTOR_RANKS[0].0,
            name: TUTOR_RANKS[0].1,
            stage: "",
        };
    }
    let mut idx = 0;
    for (i, (_, _, min)) in TUTOR_RANKS.iter().enumerate() {
        if pct >= *min {
            idx = i;
        }
    }
    // 筑基门槛：核心导航模块必须全部掌握（即使用户从其它模块凑够百分比）。
    let core_done = crate::tutor::module_by_id("core_nav").is_some_and(|module| {
        progress.get("core_nav").is_some_and(|entry| {
            !module.items.is_empty()
                && module
                    .items
                    .iter()
                    .all(|i| entry.mastered.iter().any(|id| id == &i.id))
        })
    });
    // 首项掌握即入炼气（102 项时 1/102 的整数百分比仍是 0）。
    if mastered > 0 && idx < 1 {
        idx = 1;
    }
    if core_done && idx < 2 {
        idx = 2;
    }
    let any_wrong = progress.values().any(|entry| !entry.skipped.is_empty());
    if pct >= 100 {
        idx = if any_wrong { 9 } else { 10 };
    }
    // 四小阶段：在当前境界百分比区间内取四分位。
    let stage = if (1..=8).contains(&idx) {
        let lo = TUTOR_RANKS[idx].2;
        let hi = TUTOR_RANKS[idx + 1].2;
        let step = ((pct.saturating_sub(lo)) * 4 / hi.saturating_sub(lo).max(1)).min(3);
        ["初期", "中期", "后期", "圆满"][step]
    } else {
        ""
    };
    TutorRank {
        symbol: TUTOR_RANKS[idx].0,
        name: TUTOR_RANKS[idx].1,
        stage,
    }
}

/// M56-11：成就徽章——完成首模块 🏅 / 全模块 100% 🏆 / 零错题通过 ⭐。
/// 纯函数：按稳定 item id 判定，与进度文件版本无关。
/// M77-2/M86-4：满宽进度条色块。M86-4 起 UI 不再渲染进度条，保留纯函数供
/// 历史测试与主题对照使用。
#[allow(dead_code)]
pub(crate) fn tutor_bar_spans(pct: usize, width: usize) -> Vec<Span<'static>> {
    let filled = width.saturating_mul(pct.min(100)) / 100;
    let mut spans = Vec::with_capacity(2);
    if filled > 0 {
        spans.push(Span::styled(
            " ".repeat(filled),
            Style::new().bg(Color::DarkGray),
        ));
    }
    if filled < width {
        spans.push(Span::styled(
            " ".repeat(width - filled),
            Style::new().bg(Color::Black),
        ));
    }
    spans
}

/// M45-3c：删除确认居中弹窗矩形——宽取终端减边距（24–72），高按正文行数
/// +2 边框，小终端取终端本身。
pub(crate) fn confirm_rect(
    area: ratatui::layout::Rect,
    body_lines: usize,
) -> ratatui::layout::Rect {
    use ratatui::layout::Rect;
    let w = area.width.saturating_sub(4).clamp(area.width.min(24), 72);
    let h = (body_lines as u16 + 2).min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(w) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width: w.min(area.width),
        height: h,
    }
}

/// M46-10：删除确认列表固定高度（不全显示时右侧滚动条 + j/k 浏览）。
pub(crate) const CONFIRM_LIST_HEIGHT: usize = 6;

/// M46-10：yazi 风格删除确认弹窗内容——标题居中带 ⚠ 图标，标题下不再显示
/// 提示文字；待删除文件列表样式与双栏一致（图标 + 类型色 + 当前行反显），
/// 固定高度 + 右侧滚动条。纯函数便于测试。
pub(crate) fn confirm_dialog(
    permanent: bool,
    paths: &[std::path::PathBuf],
    remote: Option<&str>,
    cursor: usize,
    scroll: usize,
    width: u16,
) -> (String, Line<'static>, Vec<Line<'static>>) {
    let n = paths.len().max(1);
    let title = if permanent {
        format!("⚠ 是否永久删除选定的 {n} 个文件")
    } else {
        format!("⚠ 是否删除选定的 {n} 个文件")
    };
    let mut lines: Vec<Line<'static>> = Vec::new();
    let start = scroll.min(n.saturating_sub(CONFIRM_LIST_HEIGHT));
    let thumb = (start * CONFIRM_LIST_HEIGHT / n).min(CONFIRM_LIST_HEIGHT - 1);
    // M48-11：滚动条列与文件名列分离，严格落在窗口右侧内缘。
    let name_width = (width as usize).saturating_sub(6).max(8);
    for row in 0..CONFIRM_LIST_HEIGHT {
        let idx = start + row;
        let mut spans: Vec<Span<'static>> = Vec::new();
        let bar = if n > CONFIRM_LIST_HEIGHT && row == thumb {
            "▮"
        } else if n > CONFIRM_LIST_HEIGHT {
            "│"
        } else {
            " "
        };
        if let Some(path) = paths.get(idx) {
            let icon = crate::icons::icon_for_active(path);
            let label = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            let class = if path.is_dir() {
                crate::tab::FileTypeClass::Directory
            } else {
                crate::tab::FileTypeClass::Other
            };
            spans.push(Span::from(format!("{} ", icon.glyph)));
            let used = label.chars().count() + 2;
            spans.push(Span::styled(label, filetype_style(class)));
            spans.push(Span::from(" ".repeat(name_width.saturating_sub(used))));
        } else {
            spans.push(Span::from(" ".repeat(name_width + 2)));
        }
        spans.push(Span::from(bar.to_owned()));
        let line = Line::from(spans);
        lines.push(if idx == cursor {
            line.style(Style::new().add_modifier(Modifier::REVERSED))
        } else {
            line
        });
    }
    if let Some(r) = remote {
        lines.push(Line::from(format!("远端: {r}")));
    }
    // M48-11：状态栏是窗口自己的底栏（独立 widget，不是列表最后一行）。
    let key_yes = Span::styled(
        " y ",
        Style::new()
            .bg(Color::Red)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    );
    let key_no = Span::styled(" n / Esc ", Style::new().add_modifier(Modifier::REVERSED));
    let footer = Line::from(vec![
        key_yes,
        Span::from(" 确认   "),
        key_no,
        Span::from(" 取消   j/k 浏览"),
    ])
    .style(Style::new().bg(Color::DarkGray).fg(Color::White));
    (title, footer, lines)
}

fn render_confirm(
    frame: &mut Frame,
    paths: &[std::path::PathBuf],
    permanent: bool,
    remote: Option<&str>,
    cursor: usize,
    scroll: usize,
) {
    let rect = confirm_rect(frame.area(), CONFIRM_LIST_HEIGHT + 3);
    let (title, footer, lines) =
        confirm_dialog(permanent, paths, remote, cursor, scroll, rect.width);
    frame.render_widget(ratatui::widgets::Clear, rect);
    let border = Style::new().fg(Color::Red).add_modifier(Modifier::BOLD);
    // ratatui 0.30：Title 结构体退役，标题居中用 Line::centered()。
    let centered = ratatui::text::Line::from(title).style(border).centered();
    let block = ratatui::widgets::Block::bordered()
        .title(centered)
        .border_style(border);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    // M48-11：底部状态栏与右侧滚动条都落在窗口内缘，形成完整窗口观感。
    let [body, foot] = ratatui::layout::Layout::vertical([
        ratatui::layout::Constraint::Min(1),
        ratatui::layout::Constraint::Length(1),
    ])
    .areas(inner);
    frame.render_widget(Paragraph::new(lines), body);
    frame.render_widget(Paragraph::new(footer), foot);
}

/// M50-1/M58-4：命令补全浮层——满宽、底部对齐；在可用空间内尽量显示全部候选，
/// 超出部分由滚动条指示（不再固定 ≤40% / 15 行）。
pub(crate) fn command_completion_rect(
    area: ratatui::layout::Rect,
    count: usize,
) -> ratatui::layout::Rect {
    let max_h = area.height.saturating_sub(3) as usize;
    let h = count
        .saturating_add(2)
        .clamp(5, max_h.max(5))
        .min(area.height as usize);
    ratatui::layout::Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(h as u16),
        width: area.width,
        height: h as u16,
    }
}

/// M58-6：多行外部命令预览行（仅含换行的候选才显示）。纯函数便于测试。
pub(crate) fn command_preview_lines(candidate: &str) -> Vec<String> {
    candidate.lines().map(str::to_owned).collect()
}

/// M58-4：选中索引对应的视口首行（自动滚动使其可见）。纯函数便于测试。
pub(crate) fn completion_offset(index: usize, total: usize, visible: usize) -> usize {
    index
        .saturating_sub(visible.saturating_sub(1))
        .min(total.saturating_sub(visible))
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

/// M45-5b：跳转选择器三段区域（输入行 / 列表 / 提示行）——render 与鼠标
/// 命中同源，避免分叉。列表段供点击选中测试。
pub(crate) fn jump_areas(area: ratatui::layout::Rect) -> [ratatui::layout::Rect; 3] {
    let popup = jump_rect(area);
    let inner = ratatui::widgets::Block::bordered().inner(popup);
    Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(inner)
}

/// M45-5b：选择器鼠标点击 → 可见项下标（仅列表段内命中，标题/输入/提示行
/// 与框外返回 None）。纯函数便于测试。
pub(crate) fn jump_click_index(area: ratatui::layout::Rect, col: u16, row: u16) -> Option<usize> {
    let list = jump_areas(area)[1];
    if col >= list.x && col < list.x + list.width && row >= list.y && row < list.y + list.height {
        Some((row - list.y) as usize)
    } else {
        None
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

/// FIX-8：与 frecency 搜索区渲染同源的列表区域和纵向偏移。
pub(crate) fn frecency_list_offset(cursor: usize, height: usize) -> usize {
    if height == 0 || cursor < height {
        0
    } else {
        cursor + 1 - height
    }
}

pub(crate) fn frecency_click_index(
    area: ratatui::layout::Rect,
    focus: crate::tab::Focus,
    fixed_side: bool,
    cursor: usize,
    col: u16,
    row: u16,
) -> Option<usize> {
    let (focused, _) = preview_layout_areas(area, focus, fixed_side);
    let _ = (focus, fixed_side);
    // M47-5：frecency 面板与焦点栏目录区同几何（无边框，顶部 1 行路径/标题）。
    let list = left_list_area(false, focused);
    if col < list.x
        || col >= list.x.saturating_add(list.width)
        || row < list.y
        || row >= list.y.saturating_add(list.height)
    {
        return None;
    }
    Some(frecency_list_offset(cursor, list.height as usize) + (row - list.y) as usize)
}

/// FIX-8：按 frecency 搜索词分段高亮所有路径匹配，复用文件列表搜索的
/// smart-case byte range 语义；多词命中片段合并，避免重复覆盖。
pub(crate) fn highlight_query_spans(value: &str, query: &str) -> Vec<Span<'static>> {
    let mut ranges = Vec::new();
    for term in query.split_whitespace() {
        let mut offset = 0;
        while offset < value.len() {
            let rest = &value[offset..];
            let Some((start, end)) = crate::tab::match_range(rest, term) else {
                break;
            };
            let start = offset + start;
            let end = offset + end;
            ranges.push((start, end));
            offset = end.max(offset + 1);
        }
    }
    ranges.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in ranges {
        if let Some(last) = merged.last_mut()
            && start <= last.1
        {
            last.1 = last.1.max(end);
        } else {
            merged.push((start, end));
        }
    }
    let mut spans = Vec::new();
    let mut pos = 0;
    for (start, end) in merged {
        if start > pos {
            spans.push(Span::from(value[pos..start].to_owned()));
        }
        spans.push(Span::styled(
            value[start..end].to_owned(),
            Style::new().fg(Color::Black).bg(Color::LightYellow),
        ));
        pos = end;
    }
    if pos < value.len() {
        spans.push(Span::from(value[pos..].to_owned()));
    }
    if spans.is_empty() {
        spans.push(Span::from(value.to_owned()));
    }
    spans
}

/// M55-1/3：单行带边框输入框 + 可见光标 + 右侧提示（frecency 搜索与实时递归共用）。
fn render_input_line(frame: &mut Frame, area: ratatui::layout::Rect, query: &str, hint: String) {
    frame.render_widget(ratatui::widgets::Clear, area);
    if area.width < 12 {
        frame.render_widget(Paragraph::new(hint), area);
        return;
    }
    let hint_w = (hint.chars().count() as u16).min(area.width / 2);
    let input_w = area.width.saturating_sub(hint_w).max(12).min(area.width);
    let input_area = ratatui::layout::Rect {
        x: area.x,
        y: area.y,
        width: input_w,
        height: area.height,
    };
    let hint_area = ratatui::layout::Rect {
        x: area.x.saturating_add(input_w),
        y: area.y,
        width: area.width.saturating_sub(input_w),
        height: area.height,
    };
    let block = ratatui::widgets::Block::new()
        .borders(ratatui::widgets::Borders::LEFT | ratatui::widgets::Borders::RIGHT)
        .border_style(pane_border_style(true));
    let spans = vec![
        Span::styled("/", Style::new().fg(Color::LightYellow)),
        Span::from(query.to_owned()),
        Span::styled(
            "▋",
            Style::new()
                .fg(Color::LightYellow)
                .add_modifier(Modifier::BOLD),
        ),
    ];
    frame.render_widget(Paragraph::new(Line::from(spans)).block(block), input_area);
    if hint_area.width > 0 {
        frame.render_widget(Paragraph::new(hint), hint_area);
    }
}

/// M55-2：`O` 实时递归查找渲染——复用 frecency 的列表几何/图标/高亮，
/// 状态栏带边框输入框与 scanning 计数。
pub(crate) fn render_live_find(frame: &mut Frame, state: &AppState, msg: Option<&str>) {
    let Some(lf) = state.live_find.as_ref() else {
        return;
    };
    let [tab_bar_area, main, _right, status_area] =
        pane_layout(frame.area(), crate::config::LayoutMode::Dual, false);
    let tab_spans = tab_bar_spans(state, tab_bar_area.width, false);
    frame.render_widget(Paragraph::new(Line::from(tab_spans)), tab_bar_area);
    let [topline, inner] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(main);
    frame.render_widget(ratatui::widgets::Clear, topline);
    let title = if lf.scanning {
        format!(
            "实时递归查找  /{}  [scanning {} 项{}]",
            lf.query,
            lf.all.len(),
            if lf.truncated { "，已截断" } else { "" }
        )
    } else {
        format!(
            "实时递归查找  /{}  [{} 项{}]",
            lf.query,
            lf.filtered.len(),
            if lf.truncated { "，已截断" } else { "" }
        )
    };
    frame.render_widget(
        Paragraph::new(title).style(pane_border_style(true).add_modifier(Modifier::BOLD)),
        topline,
    );
    let items: Vec<ListItem> = lf
        .filtered
        .iter()
        .enumerate()
        .map(|(idx, i)| {
            let item = &lf.all[*i];
            let icon = crate::icons::icon_for_active(&item.path);
            let mut spans = vec![
                Span::from(LIST_LEADING_BLANK),
                Span::from(icon_prefix(icon.glyph, false, state.config.ui.icons)),
            ];
            spans.extend(highlight_query_spans(
                &item.path.to_string_lossy().replace('\\', "/"),
                &lf.query,
            ));
            ListItem::new(Line::from(spans)).style(if idx == lf.cursor {
                Style::new().add_modifier(Modifier::REVERSED)
            } else {
                Style::new()
            })
        })
        .collect();
    let offset = frecency_list_offset(lf.cursor, inner.height as usize);
    let mut list_state = ListState::default()
        .with_selected(Some(lf.cursor))
        .with_offset(offset);
    frame.render_stateful_widget(
        List::new(items).highlight_style(Style::new().add_modifier(Modifier::REVERSED)),
        inner,
        &mut list_state,
    );
    let hint = format!(
        "↑↓/Ctrl+J/K=移动（j/k 为输入） Enter=跳转 Esc=退出  匹配 {}/{} 项{}",
        lf.filtered.len(),
        lf.all.len(),
        msg.map(|m| format!("  {m}")).unwrap_or_default()
    );
    render_input_line(frame, status_area, &lf.query, hint);
}

/// M23-6：Frecency Pane 渲染——焦点 pane 显示搜索+结果，对侧显示光标
/// 条目的快速预览（PreviewRenderer，文本 100 行截断 + 标记）。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn render_frecency_panes(frame: &mut Frame, state: &AppState) {
    render_frecency_panes_with_msg(frame, state, None);
}

pub(crate) fn render_frecency_panes_with_msg(
    frame: &mut Frame,
    state: &AppState,
    msg: Option<&str>,
) {
    let tab = state.active_tab();
    let pane = match &tab.pane_frecency {
        Some(p) => p,
        None => return,
    };
    // M49-1：预览固定右侧（默认）；动态对侧仅在 fixed_side=false 时保留。
    let [tab_bar_area, _left, _right, status_area] =
        pane_layout(frame.area(), crate::config::LayoutMode::Dual, false);
    let (frec_area, preview_area) =
        preview_layout_areas(frame.area(), tab.focus, state.config.preview.fixed_side);
    // M105-1：先清焦点侧整块区域，原列表条目的焦点高亮背景不得从
    // 搜索/预览层缝隙透出；对侧不清除，保持原目录面板可见（仅供查看）。
    frame.render_widget(ratatui::widgets::Clear, frec_area);
    // 标签栏照常渲染（M39-6 共享 spans：命名标签 + 溢出滚动）。
    let tab_spans = tab_bar_spans(state, tab_bar_area.width, false);
    frame.render_widget(Paragraph::new(Line::from(tab_spans)), tab_bar_area);

    // 焦点 pane：Frecency 搜索 UI。
    let visible = filter_jump(&pane.entries, &pane.query);
    // M105-2：标题栏即输入框——与 Normal 模式同尺寸（1 行），不再在底部预留
    // 3 行。单行空间不够画完整边框，退化为底横线表示输入态。
    let [topline, inner] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(frec_area);
    frame.render_widget(ratatui::widgets::Clear, topline);
    if pane.normal {
        let title_spans = vec![Span::from("Frecency - 高频目录/文件搜索 - NORMAL 模式")];
        frame.render_widget(
            Paragraph::new(Line::from(title_spans)).style(vifm_style("TopLineSel")),
            topline,
        );
    } else {
        let blink = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() % 1000 < 500)
            .unwrap_or(true);
        let caret = if blink { "▋" } else { " " };
        // 预览获得焦点时输入失焦：颜色与其它组件焦点样式同源同步。
        // 单行空间无法容纳完整边框，改用下划线（标题行下方横线）表示输入态。
        let focused = !pane.preview_focus;
        let input_fg = if focused {
            Color::LightYellow
        } else {
            Color::DarkGray
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("/", Style::new().fg(input_fg)),
                Span::styled(
                    pane.query.clone(),
                    Style::new().fg(input_fg).add_modifier(Modifier::UNDERLINED),
                ),
                Span::styled(
                    caret,
                    Style::new()
                        .fg(input_fg)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ),
            ])),
            topline,
        );
    }
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
            let mut spans = vec![
                Span::from(LIST_LEADING_BLANK),
                Span::from(icon_prefix(icon.glyph, false, state.config.ui.icons)),
            ];
            spans.extend(highlight_query_spans(&e.path, &pane.query));
            spans.extend(highlight_query_spans(&bm, &pane.query));
            // M75-3：v 可视选择范围与当前光标同样反显。
            let in_visual = pane
                .visual
                .is_some_and(|a| (a.min(pane.cursor)..=a.max(pane.cursor)).contains(&idx));
            let selected = idx == pane.cursor || in_visual;
            // M105-3：选中行整行反显，原 DarkGray 得分在反色背景上几乎不可见；
            // 选中行用黑字、未选中行用亮灰，保证首条（默认光标）得分可读。
            spans.push(Span::styled(
                format!("{:>7.1}", e.score),
                Style::new().fg(if selected { Color::Black } else { Color::Gray }),
            ));
            ListItem::new(Line::from(spans)).style(if selected {
                Style::new().add_modifier(Modifier::REVERSED)
            } else {
                Style::new()
            })
        })
        .collect();
    let list_offset = frecency_list_offset(pane.cursor, inner.height as usize);
    let mut list_state = ListState::default()
        .with_selected(Some(pane.cursor))
        .with_offset(list_offset);
    frame.render_stateful_widget(
        ratatui::widgets::List::new(frec_items)
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED)),
        inner,
        &mut list_state,
    );

    // M76-3：默认不启动 frecency 预览，仅 w 开启后渲染。
    if (state.config.preview.frecency_preview || pane.preview_focus)
        && let Some(entry) = visible.get(pane.cursor)
    {
        // M105-1：预览区完整覆盖对侧（含原 pane 高亮），不留透明缝隙。
        frame.render_widget(ratatui::widgets::Clear, preview_area);
        let path = std::path::PathBuf::from(&entry.path);
        let viewer = pane.preview_focus.then(|| crate::tab::ViewerState {
            scroll: pane.preview_scroll,
            line_count: 0,
            total_lines: 0,
            query: pane.preview_query.clone(),
            match_line: pane.preview_match,
            hex: false,
            page: 0,
            path: Some(path.clone()),
        });
        crate::app::preview::render_view_path(frame, state, &path, preview_area, viewer.as_ref());
    }
    // M105-2：输入框已上移到标题行；状态行只给模式提示，不再画第二套输入框。
    // M105-7：r→y 等确认提示此前被这里的状态提示覆盖，改为优先显示 msg。
    frame.render_widget(ratatui::widgets::Clear, status_area);
    if let Some(m) = msg.filter(|m| !m.is_empty()) {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("⚠ {m}"),
                Style::new()
                    .fg(Color::LightYellow)
                    .add_modifier(Modifier::BOLD),
            ))),
            status_area,
        );
    } else if pane.normal {
        let hint = format!(
            "NORMAL 模式｜j/k/gg/G=移动 l/Enter=跳转 d=批量删 dd=删当前 u=恢复 r=清书签(y确认) yf/yn/yc=复制 Tab=预览焦点 w=view Esc=关闭（{} 项）",
            visible.len()
        );
        frame.render_widget(Paragraph::new(hint), status_area);
    } else {
        frame.render_widget(
            Paragraph::new(format!(
                "搜索模式｜关键词实时过滤，Esc 回 NORMAL，Enter 跳转（{} 项）",
                visible.len()
            )),
            status_area,
        );
    }
}

/// M56-3/4/5：向导模块列表 + 80% 欢迎对话框 + 进度状态栏。
/// `editing=true` 表示 `/` 输入态：列表按实时输入过滤，顶行显示光标。
fn render_tutor_list(
    frame: &mut Frame,
    state: &AppState,
    cursor: usize,
    search: Option<&str>,
    editing: bool,
) {
    // M100-1：窄屏 tutor 全宽面板/短提示（复用 M99 `narrow_threshold`）。
    let narrow = state.config.ui.narrow_threshold > 0
        && frame.area().width < state.config.ui.narrow_threshold;
    // M74-2：只清对话框矩形，并用不透明底色覆盖原文字，双栏上下文仍可见。
    let dialog = tutor_dialog_rect_for(frame.area(), narrow);
    frame.render_widget(ratatui::widgets::Clear, dialog);
    let modules = crate::tutor::builtin_modules();
    let filtered: Vec<_> = modules
        .iter()
        .filter(|m| {
            search.is_none_or(|q| {
                let q = q.to_lowercase();
                m.id.to_lowercase().contains(&q) || m.name.to_lowercase().contains(&q)
            })
        })
        .collect();
    let items: Vec<Line> = filtered
        .iter()
        .map(|m| {
            let total = m.items.len();
            let mastered = state
                .tutor_progress
                .get(&m.id)
                .map(|p| p.mastered.len())
                .unwrap_or(0);
            let label = tutor_progress_label(total, mastered);
            Line::from(if narrow {
                format!("{label} {:<14} {}", m.id, m.name)
            } else {
                format!("{label} {:<14} {} — {}", m.id, m.name, m.description)
            })
        })
        .collect();
    let block = ratatui::widgets::Block::bordered()
        .style(Style::new().bg(Color::Black))
        .title("🎓 向导模式 (:tutor)")
        .title_bottom(
            ratatui::text::Line::from(if editing {
                if narrow {
                    " ↑↓ Enter Esc "
                } else {
                    " ↑↓=选择 Enter=应用 Esc=清除 "
                }
            } else if narrow {
                " j/k Enter / q "
            } else {
                " j/k=选择 Enter/l=进入 /=搜索 d=重置 q/Esc=关闭 "
            })
            .centered(),
        );
    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);
    let status_h = (if narrow { 3u16 } else { 5u16 }).min(inner.height.saturating_sub(2));
    let [mut list_area, status_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(status_h)]).areas(inner);
    if let Some(q) = search {
        let [top, rest] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(list_area);
        let label = if editing {
            format!("/{q}▋  Enter=应用 Esc=清除")
        } else {
            format!("/{q}  Esc=清除")
        };
        frame.render_widget(Paragraph::new(label), top);
        list_area = rest;
    }
    let list = ratatui::widgets::List::new(items)
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▶ ");
    let mut list_state = ListState::default().with_selected(Some(cursor));
    frame.render_stateful_widget(list, list_area, &mut list_state);
    if status_area.height >= 2 {
        let (done, mods, mastered, total, pct) = tutor_overall_progress(&state.tutor_progress);
        let rank = tutor_rank(&state.tutor_progress);
        // M86-4：去掉方框与「·」小圆点，顶部一条分割线与上方分隔；
        // 获奖标题放在分割线下方左对齐，境界图标按级别着色。
        frame.render_widget(ratatui::widgets::Clear, status_area);
        let divider = ratatui::layout::Rect {
            height: 1,
            ..status_area
        };
        frame.render_widget(
            Paragraph::new("─".repeat(status_area.width as usize)),
            divider,
        );
        let body = ratatui::layout::Rect {
            y: status_area.y + 1,
            height: status_area.height.saturating_sub(1),
            ..status_area
        };
        let title = Line::from(vec![
            Span::from("学习进度  "),
            Span::styled(
                rank.symbol,
                Style::new()
                    .fg(tutor_rank_color(rank.name))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::from(format!(" {}  ", rank.name)),
            Span::from(rank.stage.to_owned()),
        ]);
        let lines = vec![
            title,
            Line::from(format!(
                "模块 {done}/{mods}  功能点 {mastered}/{total}  已掌握本系统 {pct}% 的功能"
            )),
        ];
        frame.render_widget(Paragraph::new(lines), body);
    }
}

/// M70：git 写操作确认框（y/Enter 确认，其它键取消）。
fn render_git_confirm(frame: &mut Frame, op: &crate::state::GitOp, area: ratatui::layout::Rect) {
    let body = match op {
        crate::state::GitOp::StageAll => "暂存/取消暂存全部变更？".to_owned(),
        crate::state::GitOp::Amend => "修补上一次提交（--amend）？".to_owned(),
        crate::state::GitOp::DiscardFile(p) => format!("放弃 {p} 的未暂存修改？此操作不可恢复。"),
        crate::state::GitOp::Commit(m) => format!("提交暂存区，信息：{m}？"),
        crate::state::GitOp::Pull => "从远端拉取（git pull）？".to_owned(),
        crate::state::GitOp::Push => "推送到远端（git push）？".to_owned(),
        crate::state::GitOp::DiscardHunk(path, _, preview) => {
            format!("放弃 {path} 的当前差异块（实验性，可 u 撤销）？\n{preview}")
        }
    };
    let h = area
        .height
        .min(if matches!(op, crate::state::GitOp::DiscardHunk(..)) {
            12
        } else {
            4
        });
    let mut lines: Vec<Line> = body
        .lines()
        .take(h.saturating_sub(2) as usize)
        .map(Line::from)
        .collect();
    lines.push(Line::from("y/Enter=确认    其它键=取消"));
    let rect = ratatui::layout::Rect {
        x: area.x + area.width.saturating_sub(60) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width: area.width.min(60),
        height: h,
    };
    frame.render_widget(
        Paragraph::new(lines).block(ratatui::widgets::Block::bordered().title("git 操作确认")),
        rect,
    );
}

/// M69：git diff 只读 overlay——左栏变更文件，右栏统一补丁/左右对照。
fn render_git_diff(frame: &mut Frame, state: &AppState, msg: Option<&str>) {
    frame.render_widget(ratatui::widgets::Clear, frame.area());
    let Some(diff) = state.git_diff.as_ref() else {
        return;
    };
    // M72-1/M82-2：复用 pane_layout；全屏对照时文件列表让位。
    let [tab_bar_area, mut files_area, mut patch_area, status_area] =
        pane_layout(frame.area(), crate::config::LayoutMode::Dual, false);
    if diff.fullscreen {
        // M86-3：补丁区必须**移动**到文件列表的起点 x，而不是只扩宽——
        // 否则右半栏从原 x 起绘制并溢出屏幕，左半侧留白（用户实测）。
        patch_area.x = files_area.x;
        patch_area.width = files_area.width.saturating_add(patch_area.width);
        files_area.width = 0;
    }
    let tab_spans = tab_bar_spans(state, tab_bar_area.width, false);
    frame.render_widget(Paragraph::new(Line::from(tab_spans)), tab_bar_area);
    if files_area.width > 0 {
        let files_focused = diff.focus == crate::state::DiffFocus::Files;
        // M106-1：标题带项目名与分支，多仓库/多工作区下可辨识。
        let files_title = if diff.project.is_empty() {
            "diff · 变更文件".to_owned()
        } else {
            format!("diff · {} [{}] · 变更文件", diff.project, diff.branch)
        };
        let files_block = ratatui::widgets::Block::bordered()
            .title(files_title)
            .border_style(if files_focused {
                pane_border_style(true)
            } else {
                pane_border_style(false)
            });
        let files_inner = files_block.inner(files_area);
        frame.render_widget(files_block, files_area);
        let items: Vec<ListItem> = diff
            .files
            .iter()
            .map(|(path, x, y)| {
                ListItem::new(Line::from(format!("{x}{y} {path}"))).style(if files_focused {
                    vifm_style("TopLineSel")
                } else {
                    Style::new()
                })
            })
            .collect();
        let mut file_state = ListState::default().with_selected(Some(diff.cursor));
        frame.render_stateful_widget(
            List::new(items).highlight_style(Style::new().add_modifier(Modifier::REVERSED)),
            files_inner,
            &mut file_state,
        );
    }

    let flags = format!(
        "{}{}{}{}",
        if diff.side_by_side {
            "对照"
        } else {
            "补丁"
        },
        if diff.fullscreen { "·全屏" } else { "" },
        if diff.full { "·完整" } else { "" },
        if diff.wrap { "·换行" } else { "·不换行" },
    );
    let title = format!(
        "diff · {} · {}",
        diff.selected_path().unwrap_or("（无文件）"),
        flags
    );
    let border = if diff.focus == crate::state::DiffFocus::Patch {
        pane_border_style(true)
    } else {
        pane_border_style(false)
    };
    if diff.side_by_side {
        let [left_area, right_area] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(patch_area);
        let left_block = ratatui::widgets::Block::bordered()
            .title(format!("{title} · 旧"))
            .border_style(border);
        let right_block = ratatui::widgets::Block::bordered()
            .title(format!("{title} · 新"))
            .border_style(border);
        let left_inner = left_block.inner(left_area);
        let right_inner = right_block.inner(right_area);
        frame.render_widget(left_block, left_area);
        frame.render_widget(right_block, right_area);
        let (rows, _) = match diff.selected() {
            Some(data) => crate::gitdiff::render_side_by_side_styled(
                data,
                diff.full,
                diff.wrap,
                left_inner.width as usize,
            ),
            None => (Vec::new(), Vec::new()),
        };
        let shown: Vec<(Line, Line)> = rows
            .into_iter()
            .skip(diff.patch_scroll)
            .take(left_inner.height as usize)
            .map(|(l, r)| {
                let skip = diff.hscroll;
                let l2 = l.spans.into_iter().skip(skip).collect::<Vec<_>>();
                let r2 = r.spans.into_iter().skip(skip).collect::<Vec<_>>();
                (Line::from(l2).style(l.style), Line::from(r2).style(r.style))
            })
            .collect();
        let (left_lines, right_lines): (Vec<_>, Vec<_>) = shown.into_iter().unzip();
        frame.render_widget(Paragraph::new(left_lines), left_inner);
        frame.render_widget(Paragraph::new(right_lines), right_inner);
    } else {
        let patch_block = ratatui::widgets::Block::bordered()
            .title(title)
            .border_style(border);
        let patch_inner = patch_block.inner(patch_area);
        frame.render_widget(patch_block, patch_area);
        let rows = match diff.selected() {
            Some(data) => {
                crate::gitdiff::render_unified_styled(
                    &data.hunks,
                    &data.path,
                    diff.wrap,
                    patch_inner.width as usize,
                )
                .0
            }
            None => vec![Line::from("正在读取 diff…")],
        };
        let shown: Vec<Line> = rows
            .into_iter()
            .skip(diff.patch_scroll)
            .take(patch_inner.height as usize)
            .map(|line| {
                let spans = line
                    .spans
                    .into_iter()
                    .skip(diff.hscroll)
                    .collect::<Vec<_>>();
                Line::from(spans).style(line.style)
            })
            .collect();
        frame.render_widget(Paragraph::new(shown), patch_inner);
    }
    let hint = format!(
        "j/k · J/K · / n/N · | 对照 · = 完整 · ^w 换行 · Tab · Esc/q{}",
        msg.map(|m| format!("  {m}")).unwrap_or_default()
    );
    frame.render_widget(Paragraph::new(hint), status_area);
}

pub(crate) fn render(frame: &mut Frame, state: &AppState, mode: &InputMode, msg: Option<&str>) {
    // M99-3：极小终端直接给出可读提示，不渲染破碎布局。
    if frame.area().width < 30 || frame.area().height < 6 {
        frame.render_widget(ratatui::widgets::Clear, frame.area());
        frame.render_widget(
            Paragraph::new(vec![
                Line::from("终端过小（至少需要 30 列 × 6 行）"),
                Line::from("请放大终端窗口或旋转手机屏幕后重试"),
            ]),
            frame.area(),
        );
        return;
    }
    // M69：git diff overlay（早于普通目录列表渲染）。
    if matches!(
        mode,
        InputMode::GitDiff
            | InputMode::GitDiffSearch { .. }
            | InputMode::GitDiffConfirm { .. }
            | InputMode::GitCommitInput { .. }
    ) {
        render_git_diff(frame, state, msg);
        if let InputMode::GitDiffConfirm { op } = mode {
            render_git_confirm(frame, op, frame.area());
        }
        if let InputMode::GitCommitInput { input } = mode {
            let area = ratatui::layout::Rect {
                x: frame.area().x,
                y: frame.area().y + frame.area().height.saturating_sub(3),
                width: frame.area().width,
                height: 3,
            };
            frame.render_widget(
                Paragraph::new(format!("git commit -m \"{input}\""))
                    .block(ratatui::widgets::Block::bordered().title("提交信息（Enter 确认）")),
                area,
            );
        }
        return;
    }
    // M55-2：`O` 实时递归查找覆盖整屏（与目录列表共用几何/图标/主题）。
    if matches!(mode, InputMode::LiveFind) && state.live_find.is_some() {
        render_live_find(frame, state, msg);
        return;
    }
    let tab = state.active_tab();
    // M14-2：标签栏（1 行）——M39-6：命名标签 + 溢出滚动（共享 spans）。
    // M99-1：窄屏宽度断点覆盖静态 layout（旋转即时生效，无设备特判）。
    let narrow = state.config.ui.narrow_threshold > 0
        && frame.area().width < state.config.ui.narrow_threshold;
    let layout = if narrow {
        crate::config::LayoutMode::Single
    } else {
        state.config.ui.layout()
    };
    let viewing = tab.view_mode;
    let [tab_bar_area, left_area, right_area, status_area] =
        pane_layout(frame.area(), layout, viewing);
    // M45-8c：Tutor 面板覆盖 pane 底部——列表渲染区扣除面板高度（`G` 跳末尾
    // 不被遮挡）；状态栏在面板下不受影响。
    // M46-1：查看器几何与左右 pane 同源——用收缩前的左右 pane 并集作为主区，
    // 不再对整帧重算（PreviewBottom 的 bottom 与列表主区严格对半）。
    let main_area = ratatui::layout::Rect {
        x: left_area.x,
        y: left_area.y,
        width: right_area.right().saturating_sub(left_area.x),
        height: left_area.height,
    };
    let panel_h = if state.tutor.is_some() {
        crate::app::render::tutor_panel_height(frame.area().height)
    } else {
        0
    };
    let left_area = ratatui::layout::Rect {
        height: left_area.height.saturating_sub(panel_h),
        ..left_area
    };
    let right_area = ratatui::layout::Rect {
        height: right_area.height.saturating_sub(panel_h),
        ..right_area
    };
    // M27-2：vifm 风格（pane_borders=false，默认）与旧整框外观（true）双模式。
    let borders = state.config.ui.pane_borders;
    // M39-6：标签栏 spans 共享（命名标签 + 溢出滚动）。
    let tab_spans = tab_bar_spans(state, tab_bar_area.width, borders);
    frame.render_widget(Paragraph::new(Line::from(tab_spans)), tab_bar_area);
    // M21-F1：左右 pane——pane_borders=true 走旧整框外观（焦点边框高亮）；
    // false（默认，M27-2）为 vifm 式：无整框、pane 顶部路径行（TopLine/
    // TopLineSel 背景色区分焦点）、间隔列 Border 分隔线。
    // M81：预览获焦时左右目录栏都不是焦点，路径标题统一走失焦样式。
    let preview_focused =
        tab.preview_focus || tab.pane_frecency.as_ref().is_some_and(|p| p.preview_focus);
    let left_focused = tab.focus == crate::tab::Focus::Left && !preview_focused;
    // M3-1：可视范围（状态栏指示 + 列表高亮共用；M34-4 提升至 pane 分派
    // 之前——查看器占左 pane 时其后置公共段仍在作用域内）。
    // M47-8：左栏始终展示自己的可视选区（切焦点后仍保持，非焦点侧 DIM）。
    let visual_range = tab.visual_range();
    // M34-4：预览渲染在非焦点 pane——焦点右 + view 模式时查看器占左 pane，
    // 右 pane 保持目录列表导航（与焦点左时"查看器在右"对称，共用同一渲染）。
    // M39-6：布局感知——Single 的查看器占全宽（列表隐藏）；PreviewRight
    // 恒右侧（经 render_right_pane 内部分派）；PreviewBottom 列表上半可见
    // （查看器下半由 viewer_rect 渲染）。
    // M52-1：`:view` 恒对侧预览（焦点右 → 查看器占左栏），不再读 fixed_side。
    let view_in_left = viewing
        && layout == crate::config::LayoutMode::Dual
        && tab.focus == crate::tab::Focus::Right;
    let single_viewer = viewing && layout == crate::config::LayoutMode::Single;
    // M39-6：PreviewBottom——查看器占主区下半（全宽）。
    if viewing
        && layout == crate::config::LayoutMode::PreviewBottom
        && let Some(bottom) = viewer_rect(main_area, layout, viewing)
    {
        crate::app::preview::render_view_content(frame, state, bottom);
    }
    // M39-6：Single——查看器占全宽（列表隐藏）；Dual——对侧预览。
    if narrow && tab.focus == crate::tab::Focus::Right && !viewing {
        // M99-1：单栏下直接渲染焦点右栏（左栏区域复用为唯一视图）。
        crate::app::preview::render_right_pane(frame, state, mode, left_area);
    } else if single_viewer || view_in_left {
        crate::app::preview::render_view_content(frame, state, left_area);
    } else {
        let left_inner = if borders {
            let left_pane = ratatui::widgets::Block::bordered()
                .title(path_line_text_fit(
                    format!(
                        "{}{}",
                        if narrow { "[左] " } else { "" },
                        tab.location.display_path().replace('\\', "/")
                    ),
                    tab.filter.as_deref(),
                    left_area.width,
                ))
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
                Paragraph::new(Line::from(path_line_text_fit(
                    format!(
                        "{}{}",
                        if narrow { "[左] " } else { "" },
                        tab.location.display_path().replace('\\', "/")
                    ),
                    tab.filter.as_deref(),
                    left_area.width,
                )))
                .style(path_style),
                topline,
            );
            rest
        };
        // M27-2：pane 间隔列——Border 分隔线（vifm 模式）。
        // M39-6：Single（单栏全宽）无间隔列。
        // M45-1d：`[ui] pane_separator` 默认 false 不显示（vifm 对齐）。
        if !borders && state.config.ui.pane_separator && layout != crate::config::LayoutMode::Single
        {
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

        // FIX-1 / M46-2：当前生效的搜索查询——搜索输入期用实时输入（不回退
        // 旧查询，清空即无高亮），其余用已提交的 tab.search。
        // M46-2：与右 pane（render_right_pane）对称，只在焦点位于左 pane 时
        // 高亮——一个 Tab 级共享查询不得同时点亮两侧（vifm 对齐）。
        let active_query: Option<String> = if !left_focused {
            None
        } else {
            match mode {
                InputMode::Search { input, .. } => {
                    let query = input.trim();
                    (!query.is_empty()).then(|| query.to_owned())
                }
                _ => tab
                    .search
                    .as_ref()
                    .map(|s| s.query.clone())
                    .filter(|query| !query.is_empty()),
            }
        };
        // M21-E1：属性列自适应——配置开关 + 窄终端隐藏（文件名优先）。
        // M99-2：窄屏不挤占水平空间；detail 模式用第二行显示元数据。
        let narrow_detail = narrow && state.config.ui.narrow_list_mode == "detail";
        let show_attrs = state.config.ui.attrs_column
            && !narrow
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
            (left_inner.width as usize).saturating_sub(
                LIST_LEADING_BLANK.len()
                    + 2
                    + ATTRS_WIDTH
                    + 2
                    + if git_col { GIT_STATUS_WIDTH } else { 0 },
            )
        } else {
            (left_inner.width as usize).saturating_sub(4 + LIST_LEADING_BLANK.len())
        };
        let list_items: Vec<ListItem> = tab
            .entries
            .iter()
            .enumerate()
            .map(|(idx, item)| {
                // M21-F1：名称按文件类型着色（目录/可执行/链接/归档/图片/视频/代码）。
                // M45-2a：当前反显行（光标行 / 可视选区）不用文件类型着色——
                // REVERSED 叠加 span 级 fg 会让文字部分呈现不同底色，用户实测
                // "文字部分单独高亮"；反显行统一为整体背景色。非反显行仍着色。
                let row_reversed = (!tab.on_parent_row && idx == tab.cursor)
                    || visual_range.is_some_and(|(lo, hi)| idx >= lo && idx <= hi);
                let ft = if row_reversed {
                    Style::new()
                } else {
                    filetype_style(item.filetype_class())
                };
                // FIX-1：匹配子串分段高亮（黄底黑字，与光标行反显区分）。
                // 图标/树缩进进前缀 span，目录后缀 `/` 跟在名称之后。
                let mut spans: Vec<Span> = vec![
                    Span::from(LIST_LEADING_BLANK),
                    Span::from(icon_prefix(
                        item.icon.glyph,
                        item.is_dir,
                        state.config.ui.icons,
                    )),
                ];
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
                let mut li = if narrow_detail {
                    let size = if item.is_dir {
                        "<DIR>".to_owned()
                    } else {
                        crate::tab::format_size(item.size)
                    };
                    let detail = Line::from(Span::styled(
                        format!(
                            "    {} · {}",
                            size,
                            crate::tab::format_mtime_min(item.mtime)
                        ),
                        Style::new().fg(Color::DarkGray),
                    ));
                    ListItem::new(vec![Line::from(spans), detail]).style(base_style)
                } else {
                    ListItem::new(Line::from(spans)).style(base_style)
                };
                // M89-3：v 连续选区与 t 持久标记统一为同一背景样式，视觉上
                // 「临时选择 → t 提交为持久标记」不跳变；状态栏 N tagged 区分来源。
                if visual_range.is_some_and(|(lo, hi)| idx >= lo && idx <= hi)
                    || tab.tags.contains(&item.name)
                {
                    li = li.style(selected_row_style(left_focused));
                }
                li
            })
            .collect();
        // FEAT-2：焦点在 `..` 行时列表无高亮选中项。
        // M41-4：偏移有状态（Tab.scroll，每帧 sync_viewport_scroll 落盘）。
        let mut list_state = ListState::default().with_offset(tab.scroll);
        if !tab.on_parent_row {
            list_state = list_state.with_selected(Some(tab.cursor));
        }
        frame.render_stateful_widget(
            List::new(list_items).highlight_style(
                // M21-F2：焦点 pane 当前行反显，非焦点侧当前行 DIM 灰化
                // （vifm TopLineSel/TopLine 语义）。
                selected_row_style(left_focused),
            ),
            parent_area,
            &mut list_state,
        );
    }

    // M11-2→M14-1→M21-F1→M34-4：右 pane——默认独立目录列表（带路径标题/
    // 焦点边框）；view 模式且焦点左时显示查看器/图片预览（焦点右时查看器
    // 已由上方分派到左 pane，右 pane 保持列表导航）。
    render_right_pane(frame, state, mode, right_area);

    // M45-3c：删除确认——居中阻塞弹窗（⚠ 图标 + 红色高亮 + y 键反显），
    // 替代原状态栏 `[y/N]` 单行提示；按键仍由 run 循环按模式处理。
    match mode {
        InputMode::Confirm {
            path,
            permanent,
            remote,
            cursor,
            scroll,
        } => {
            render_confirm(
                frame,
                std::slice::from_ref(path),
                *permanent,
                remote.as_deref(),
                *cursor,
                *scroll,
            );
            return;
        }
        InputMode::ConfirmPaste { cursor, scroll } => {
            // M86-8：渲染可选择的冲突处理选项（同路径/异路径选项集不同）。
            let options: Vec<String> = state
                .paste_conflict_options()
                .into_iter()
                .enumerate()
                .map(|(i, opt)| format!("{} {}", if i == *cursor { "▶" } else { " " }, opt))
                .collect();
            let rect = confirm_rect(frame.area(), CONFIRM_LIST_HEIGHT + 3);
            let footer = Line::from("j/k=选择  y/Enter=应用  Esc=取消");
            let lines: Vec<Line> = options
                .iter()
                .enumerate()
                .skip(*scroll)
                .take(CONFIRM_LIST_HEIGHT)
                .map(|(i, opt)| {
                    Line::from(opt.clone()).style(if i == *cursor {
                        Style::new().add_modifier(Modifier::REVERSED)
                    } else {
                        Style::new()
                    })
                })
                .collect();
            frame.render_widget(ratatui::widgets::Clear, rect);
            let centered = Line::from("⚠ 同名冲突：请选择处理方式")
                .style(Style::new().fg(Color::Red).add_modifier(Modifier::BOLD))
                .centered();
            let border = Style::new().fg(Color::Red).add_modifier(Modifier::BOLD);
            let block = ratatui::widgets::Block::bordered()
                .title(centered)
                .border_style(border);
            let inner = block.inner(rect);
            frame.render_widget(block, rect);
            let [body, foot] = ratatui::layout::Layout::vertical([
                ratatui::layout::Constraint::Min(1),
                ratatui::layout::Constraint::Length(1),
            ])
            .areas(inner);
            frame.render_widget(Paragraph::new(lines), body);
            frame.render_widget(Paragraph::new(footer), foot);
            return;
        }
        InputMode::ConfirmTagDelete { cursor, scroll } => {
            // 标记是名字集合——按焦点 pane 的 base_dir 还原为完整路径并排序。
            let base = if tab.focus == crate::tab::Focus::Right {
                tab.right.location.base_dir()
            } else {
                tab.location.base_dir()
            };
            let mut paths: Vec<std::path::PathBuf> =
                tab.tags.iter().map(|name| base.join(name)).collect();
            paths.sort();
            render_confirm(
                frame,
                &paths,
                state.pending_permanent_delete,
                None,
                *cursor,
                *scroll,
            );
            return;
        }
        _ => {}
    }

    // M46-7：帮助页与帮助搜索共用不透明覆盖层——Clear + 背景色，杜绝透出
    // 下层双栏内容；HelpSearch 增加可见输入框（旧实现无渲染分支，按下 `/`
    // 后落到普通双栏，表现为"内容被清空"）。
    if let InputMode::HelpView { scroll, search } = mode {
        render_help_overlay(frame, *scroll, search.as_deref(), None);
        return;
    }
    if let InputMode::HelpSearch {
        input,
        scroll,
        search,
    } = mode
    {
        render_help_overlay(frame, *scroll, search.as_deref(), Some(input));
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
    if let InputMode::DupView { cursor } = mode {
        let lines: Vec<Line> = if let Some(result) = &state.dup_result {
            let mut out = vec![Line::from(format!(
                "{} 组重复（扫描 {} 项）",
                result.entries.len(),
                result.scanned
            ))];
            for (i, (label, count)) in result.entries.iter().enumerate() {
                out.push(Line::styled(
                    format!("{label}  ×{count}"),
                    if i == *cursor {
                        Style::new().add_modifier(Modifier::REVERSED)
                    } else {
                        Style::new()
                    },
                ));
            }
            out
        } else {
            vec![Line::from("扫描中…")]
        };
        frame.render_widget(
            Paragraph::new(lines)
                .block(ratatui::widgets::Block::bordered().title("查重结果（j/k 选择 Esc 关闭）")),
            frame.area(),
        );
        return;
    }
    if let InputMode::DuView { cursor } = mode {
        let lines: Vec<Line> = if let Some(result) = &state.du_result {
            let mut out = vec![Line::from(format!(
                "总计 {}（扫描 {} 项）",
                crate::tab::format_size(result.total),
                result.scanned
            ))];
            for (i, (name, size)) in result.entries.iter().enumerate() {
                let shown = if result.tree {
                    format!(
                        "{}{}",
                        "  ".repeat(name.matches('/').count()),
                        name.rsplit('/').next().unwrap_or(name)
                    )
                } else {
                    name.clone()
                };
                out.push(Line::styled(
                    format!("{:<60} {}", shown, crate::tab::format_size(*size)),
                    if i == *cursor {
                        Style::new().add_modifier(Modifier::REVERSED)
                    } else {
                        Style::new()
                    },
                ));
            }
            out
        } else {
            vec![Line::from("扫描中…")]
        };
        frame.render_widget(
            Paragraph::new(lines).block(
                ratatui::widgets::Block::bordered().title("磁盘占用（j/k 选择 dd 删除 Esc 关闭）"),
            ),
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
                let status = match e.state {
                    crate::state::TransferState::Pending => "待处理".to_owned(),
                    crate::state::TransferState::Running => "传输中".to_owned(),
                    crate::state::TransferState::Paused => "已暂停".to_owned(),
                    crate::state::TransferState::Done => "完成".to_owned(),
                    crate::state::TransferState::Failed => {
                        format!(
                            "失败(重试{}){}",
                            e.retries,
                            e.error.clone().unwrap_or_default()
                        )
                    }
                    crate::state::TransferState::Cancelled => "已取消".to_owned(),
                };
                lines.push(Line::styled(
                    format!(
                        "{dir_label} [{status}] {}/{} {src} → {dest}",
                        e.transferred,
                        if e.transferred > 0 {
                            "已传"
                        } else {
                            "字节"
                        },
                        src = e.src.display().to_string().replace('\\', "/"),
                        dest = e.dest.display().to_string().replace('\\', "/")
                    ),
                    sel,
                ));
            }
        }
        frame.render_widget(
            Paragraph::new(lines).block(
                ratatui::widgets::Block::bordered()
                    .title("传输队列（j/k=选择 p=暂停/恢复 r=重试 x=取消 Esc=关闭）"),
            ),
            frame.area(),
        );
        return;
    }
    if let InputMode::TutorResetConfirm { module_id, .. } = mode {
        // M74-2：确认框不整帧清除，双栏上下文保留。

        let lines = vec![
            Line::from("本模块已全部掌握，重置后将重新显示所有条目。是否重置？"),
            Line::from(format!("模块：{module_id}")),
            Line::from("y/Enter=重置并重新学习   n/Esc=取消"),
        ];
        let area = confirm_rect(frame.area(), lines.len());
        frame.render_widget(
            Paragraph::new(lines).block(
                ratatui::widgets::Block::bordered()
                    .style(Style::new().bg(Color::Black))
                    .title("向导模块重置"),
            ),
            area,
        );
        return;
    }
    if let InputMode::TutorList { cursor, search } = mode {
        render_tutor_list(frame, state, *cursor, search.as_deref(), false);
        return;
    }
    if let InputMode::TutorListSearch { input, cursor } = mode {
        // M56-5：键入即实时过滤——复用同一列表渲染，输入行带光标。
        render_tutor_list(frame, state, *cursor, Some(input.as_str()), true);
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
        // M48-8：只清空/绘制对话框自身矩形，不再遮罩整个界面。
        let area = frame.area();
        let popup = crate::app::render::archive_dialog_rect(area);
        frame.render_widget(ratatui::widgets::Clear, popup);
        // M45-9c：格式/校验仅高亮选中项——用 spans 逐项着色，不再整行 REVERSED。
        let fmt_spans: Vec<Span> = ARCHIVE_FORMATS
            .iter()
            .enumerate()
            .flat_map(|(i, f)| {
                let label = format!(" {}  ", f.ext().trim_start_matches('.'));
                if i == *format_idx {
                    vec![Span::styled(
                        label,
                        Style::new().add_modifier(Modifier::REVERSED),
                    )]
                } else {
                    vec![Span::from(label)]
                }
            })
            .collect();
        let cks_spans: Vec<Span> = ARCHIVE_CHECKSUMS
            .iter()
            .enumerate()
            .flat_map(|(i, c)| {
                let label = format!(
                    " {}  ",
                    match c {
                        ops::Checksum::Md5 => "md5",
                        ops::Checksum::Sha256 => "sha256",
                        ops::Checksum::None => "none",
                    }
                );
                if i == *checksum_idx {
                    vec![Span::styled(
                        label,
                        Style::new().add_modifier(Modifier::REVERSED),
                    )]
                } else {
                    vec![Span::from(label)]
                }
            })
            .collect();
        // M45-9c：名称字段去后缀显示（后缀在格式行右侧已显式给出）。
        let shown_name = crate::app::render::strip_archive_ext(input);
        let line_name = if *field == 0 {
            Line::from(vec![
                Span::from("名称: "),
                Span::styled(
                    format!("{shown_name}▏"),
                    Style::new().add_modifier(Modifier::REVERSED),
                ),
            ])
        } else {
            Line::from(format!("名称: {shown_name}"))
        };
        // M48-8：当前字段整行背景高亮（Tab 切换后可辨），选中值仍反显。
        let field_style = Style::new().bg(Color::DarkGray).fg(Color::White);
        let mut fmt_line = vec![Span::from("格式: ")];
        fmt_line.extend(fmt_spans);
        let line_fmt = if *field == 1 {
            Line::from(fmt_line).style(field_style)
        } else {
            Line::from(fmt_line)
        };
        let mut cks_line = vec![Span::from("校验: ")];
        cks_line.extend(cks_spans);
        let line_cks = if *field == 2 {
            Line::from(cks_line).style(field_style)
        } else {
            Line::from(cks_line)
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
        // M45-5b：与鼠标命中共用同一 geometry（jump_areas）。
        let inner = crate::app::render::jump_areas(area);
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
    if let Some(session) = &state.tutor {
        let crate::tutor::TutorSession {
            module,
            idx,
            marks,
            done,
            review,
        } = session;
        // M42-1：底部 Tutor Panel——主 TUI 背景保持可见（操作上下文），
        // 面板占底部约 1/4（状态栏其下不受影响）。左列 prompt/expected，
        // 右列进度与按键提示；done 为结算 Summary。
        // M100-1：窄屏面板上限降到 8 行，给软键盘弹出后留更多列表空间。
        let panel_h = (frame.area().height / 4).clamp(6, if narrow { 8 } else { 10 });
        let panel_y = frame.area().height.saturating_sub(panel_h + 1);
        let panel_area = ratatui::layout::Rect {
            x: 0,
            y: panel_y,
            width: frame.area().width,
            height: panel_h,
        };
        // FIX-5-2：仅清除面板区域的下层内容——目录列表文字不得从面板空白
        // 行透出干扰 prompt/expected 可读性（面板之外主 TUI 仍可见）。
        frame.render_widget(ratatui::widgets::Clear, panel_area);
        // M77-3：面板计数用完整模块的持久化进度，不再用本次 session 子集/本次作答。
        let base = crate::tutor::module_by_id(&module.id);
        let total = base
            .as_ref()
            .map(|b| b.items.len())
            .unwrap_or(module.items.len());
        let session_mastered = marks.iter().filter(|(_, ok)| *ok).count();
        let progress_entry = state.tutor_progress.get(&module.id);
        let mastered = progress_entry
            .map(|p| {
                base.as_ref().map_or(p.mastered.len(), |b| {
                    p.mastered
                        .iter()
                        .filter(|id| b.items.iter().any(|i| &i.id == *id))
                        .count()
                })
            })
            .unwrap_or(session_mastered);
        let skipped = progress_entry
            .map(|p| p.skipped.len())
            .unwrap_or(marks.len().saturating_sub(session_mastered));
        let display_total = base.as_ref().map(|b| b.items.len()).unwrap_or(total);
        let display_idx = module
            .items
            .get(*idx)
            .and_then(|item| {
                base.as_ref()
                    .and_then(|b| b.items.iter().position(|i| i.id == item.id))
            })
            .map(|p| p + 1)
            .unwrap_or_else(|| (*idx + 1).min(display_total));
        let title = format!(
            "🎓 向导模式: {}{review} ({}/{})",
            module.name,
            display_idx,
            display_total,
            review = if *review { "（复习）" } else { "" }
        );
        // M86-4：tutor 面板外框统一用焦点样式（与其它 pane 同源）。
        let block = ratatui::widgets::Block::bordered()
            .title(title)
            .border_style(pane_border_style(true));
        if *done {
            let lines = vec![
                Line::from(format!(
                    "模块完成！掌握 {mastered}/{total}，未掌握 {skipped}。"
                )),
                Line::from("未掌握项已记录（M42-2 起本地持久化，下次进入优先复习）。"),
                Line::from(if narrow {
                    "Enter=其它模块  Ctrl+Q 退出（已保存）"
                } else {
                    "Enter=继续学习其它模块（:tutor）  Ctrl+Q 退出（作答已即时保存）。"
                }),
            ];
            frame.render_widget(Paragraph::new(lines).block(block), panel_area);
        } else if let Some(item) = module.items.get(*idx) {
            let inner = block.inner(panel_area);
            let [body, hint_line] = ratatui::layout::Layout::vertical([
                ratatui::layout::Constraint::Min(0),
                ratatui::layout::Constraint::Length(1),
            ])
            .areas(inner);
            let (left, separator, right) = if narrow {
                // M100-1：窄屏单列——去掉竖线与右侧计数列。
                (
                    body,
                    ratatui::layout::Rect::default(),
                    ratatui::layout::Rect::default(),
                )
            } else {
                let [l, sp, r] = ratatui::layout::Layout::horizontal([
                    ratatui::layout::Constraint::Percentage(68),
                    ratatui::layout::Constraint::Length(1),
                    ratatui::layout::Constraint::Min(0),
                ])
                .areas(body);
                (l, sp, r)
            };
            let mut left_lines = vec![
                Line::from(Span::styled(
                    format!("💡 {}", item.prompt),
                    Style::new().add_modifier(Modifier::BOLD),
                )),
                Line::from(String::new()),
                Line::from(format!("预期: {}", item.expected)),
            ];
            if *review {
                left_lines.push(Line::from("🔁 复习模式：未掌握项已排前"));
            }
            if let Some(req) = &item.requires {
                left_lines.push(Line::from(format!(
                    "🔧 环境依赖（{req}）——无此环境可 Ctrl+N 跳过"
                )));
            }
            // M86-4：去掉进度条；右侧仅保留计数，按键提示移到面板底部，
            // 提示与操作区之间用竖线分隔，长提示换行不再压住按键提示。
            let right_lines = vec![
                Line::from(format!("掌握 {mastered}/{total}")),
                Line::from(format!("未掌握 {skipped}")),
            ];
            let sep_lines: Vec<Line> = (0..body.height).map(|_| Line::from("│")).collect();
            frame.render_widget(block, panel_area);
            frame.render_widget(
                Paragraph::new(left_lines).wrap(ratatui::widgets::Wrap { trim: true }),
                left,
            );
            if !narrow {
                frame.render_widget(Paragraph::new(sep_lines), separator);
                frame.render_widget(
                    Paragraph::new(right_lines).wrap(ratatui::widgets::Wrap { trim: true }),
                    right,
                );
            }
            let (hint, align) = if narrow {
                (
                    format!("掌握 {mastered}/{total} · Ctrl+Y 掌握 / Ctrl+N 未掌握 / Ctrl+Q 退出"),
                    ratatui::layout::Alignment::Left,
                )
            } else {
                (
                    "[Ctrl+Y] 掌握   [Ctrl+N] 未掌握   [Ctrl+Q] 退出".to_owned(),
                    ratatui::layout::Alignment::Right,
                )
            };
            frame.render_widget(Paragraph::new(Line::from(hint)).alignment(align), hint_line);
        } else {
            // idx 越界兜底（正常不可达：done 先于越界成立）。
            frame.render_widget(
                Paragraph::new(vec![Line::from("（条目越界）")]).block(block),
                panel_area,
            );
        }
    }
    let status = match mode {
        InputMode::LiveFind => Line::from(String::new()), // 覆盖层提前 return，不可达（M55-2）
        InputMode::DuView { .. }
        | InputMode::DupView { .. }
        | InputMode::GitDiff
        | InputMode::GitDiffSearch { .. }
        | InputMode::GitDiffConfirm { .. }
        | InputMode::GitCommitInput { .. } => {
            Line::from(String::new()) // diff overlay 提前 return（M69/M70）
        }
        InputMode::TutorResetConfirm { .. } => {
            Line::from(String::new()) // 确认框提前 return，不可达（M56-8）
        }
        InputMode::JobDetail { .. } => {
            Line::from(String::new()) // 详情覆盖层提前 return，不可达（M28-3）
        }
        InputMode::Queue { .. } => {
            Line::from(String::new()) // 队列覆盖层提前 return，不可达（M36-7）
        }
        InputMode::TutorList { .. } | InputMode::TutorListSearch { .. } => {
            Line::from(String::new()) // 列表覆盖层提前 return，不可达
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
        InputMode::RemoteFingerprint { fp, .. } => {
            // M39-4：首连指纹确认——指纹展示 + y/n。
            Line::from(format!("主机指纹 {fp} [y=接受并写入 known_hosts n=拒绝]"))
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
        InputMode::Command { input, comp, .. } => {
            // M58-2：500ms 翻转的可见光标；M87-1 标注 SEARCH/NORMAL/EDIT。
            let blink = (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                / 500)
                .is_multiple_of(2);
            let phase = match comp {
                Some(c) if c.normal => "NORMAL",
                Some(_) => "SEARCH",
                None => "EDIT",
            };
            Line::from(format!(
                ":{input} [{phase}]{}",
                if blink { "▋" } else { " " }
            ))
        }
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
        InputMode::FrecencyPreviewSearch { input } => Line::from(format!("预览搜索 /{input}")),
        InputMode::HelpView { .. } => Line::from(String::new()), // 帮助覆盖层提前 return
        InputMode::HelpSearch { input, .. } => Line::from(format!("/{input}")), // 同上，保持语义一致
        InputMode::JumpSelect { .. } => Line::from(String::new()), // 选择器覆盖层提前 return，不可达
        InputMode::Jobs { .. } => Line::from(String::new()), // 作业菜单覆盖层提前 return，不可达
        InputMode::TrashMenu { .. } => Line::from(String::new()), // 暂存区覆盖层提前 return，不可达
        InputMode::FileInfo { .. } => Line::from(String::new()), // 详情覆盖层提前 return，不可达
        InputMode::Confirm { .. }
        | InputMode::ConfirmTagDelete { .. }
        | InputMode::ConfirmPaste { .. } => {
            Line::from(String::new()) // M46-10：居中弹窗提前 return，不可达
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
            let mut spans = Vec::new();
            if let Some(text) = &state.statusline_text {
                spans.push(Span::from(format!("{text} | ")));
            }
            spans.push(Span::styled(
                selected,
                Style::new().add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::from(format!(" | {} items", tab.entries.len())));
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
            // M47-2：过滤状态移到对应 pane 的顶部路径行，状态栏不再显示。
            if let Some((lo, hi)) = tab.focused_visual_range() {
                // M3-1：可视选择指示（含行数，闭区间）；M47-8 取焦点栏。
                spans.push(Span::from(format!("  VISUAL ({} selected)", hi - lo + 1)));
            }
            let tagged = tab.tagged_count_focused();
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
    // M86-2：frecency 覆盖层最后绘制——对侧目录面板保持原样，不被空白遮罩覆盖；
    // 焦点栏由搜索 UI 接管，popup 始终置顶。
    if state.active_tab().pane_frecency.is_some() {
        render_frecency_panes_with_msg(frame, state, msg);
    }
    // M50-1：命令补全浮层（复用 frecency overlay 的 List/选中样式语义）。
    if let InputMode::Command {
        comp: Some(comp), ..
    } = mode
        && !comp.candidates.is_empty()
    {
        // M58-6：多行外部命令在顶部预览（不截断为单行）。
        if let Some(candidate) = comp.candidates.get(comp.index) {
            let preview_lines = command_preview_lines(candidate);
            if preview_lines.len() > 1 {
                let h = (preview_lines.len() + 2)
                    .min(frame.area().height.saturating_sub(4) as usize)
                    .max(3) as u16;
                let preview = ratatui::layout::Rect {
                    x: frame.area().x,
                    y: frame.area().y.saturating_add(1),
                    width: frame.area().width,
                    height: h,
                };
                frame.render_widget(ratatui::widgets::Clear, preview);
                frame.render_widget(
                    Paragraph::new(
                        preview_lines
                            .into_iter()
                            .map(Line::from)
                            .collect::<Vec<_>>(),
                    )
                    .block(
                        ratatui::widgets::Block::bordered()
                            .title("命令补全（Enter 填充 / Ctrl+Enter 执行）"),
                    ),
                    preview,
                );
            }
        }
        let rect = command_completion_rect(frame.area(), comp.candidates.len());
        frame.render_widget(ratatui::widgets::Clear, rect);
        let visible = rect.height.saturating_sub(2) as usize;
        let total = comp.candidates.len();
        // M58-4：选中项始终滚入可见区（与索引同源推导，不在多处保存状态）。
        let offset = completion_offset(comp.index, total, visible);
        let thumb_lo = offset.saturating_mul(visible) / total.max(1);
        let thumb_hi =
            ((offset + visible).saturating_mul(visible) / total.max(1)).max(thumb_lo + 1);
        let lines: Vec<Line> = comp
            .candidates
            .iter()
            .enumerate()
            .skip(offset)
            .take(visible)
            .enumerate()
            .map(|(row, (i, name))| {
                // M58-1：描述常显（路径候选无专有说明时给通用描述）。
                let desc = comp
                    .descs
                    .get(i)
                    .filter(|d| !d.is_empty())
                    .cloned()
                    .unwrap_or_else(|| "文件/路径".to_owned());
                let style = if i == comp.index {
                    vifm_style("Selected")
                } else {
                    Style::new()
                };
                let marker = if row >= thumb_lo && row <= thumb_hi {
                    "█"
                } else {
                    "░"
                };
                Line::from(vec![
                    Span::from(format!(" {:<20}", name)),
                    Span::from(desc),
                    Span::from(format!(" {marker}")),
                ])
                .style(style)
            })
            .collect();
        frame.render_widget(
            Paragraph::new(lines).block(
                ratatui::widgets::Block::bordered()
                    .title("命令补全（Tab 循环 / Space 选定到输入栏 / Enter 执行）")
                    .border_style(vifm_style("Border")),
            ),
            rect,
        );
    }
}
