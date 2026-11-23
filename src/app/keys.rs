use super::*;

/// AUDIT-6：待决序列截止时刻推导——seq 清空 → 无截止；seq 长度变化
/// （新键入/前缀推进）→ 重置截止；同一段待决 → 保持原截止（避免
/// 每帧顺延导致永不超时）。
pub(crate) fn seq_deadline_after(
    seq_len: usize,
    prev_len: usize,
    prev_deadline: Option<std::time::Instant>,
    now: std::time::Instant,
    timeout_ms: u64,
) -> Option<std::time::Instant> {
    if seq_len == 0 {
        return None;
    }
    if seq_len != prev_len {
        return Some(now + std::time::Duration::from_millis(timeout_ms));
    }
    prev_deadline
}

/// AUDIT-6：poll 预算与到期判定——有待决序列时预算收缩到超时余量
/// （不超过 100ms 空转粒度），到期返回 `(0, true)`（调用方清空 seq）。
pub(crate) fn seq_poll_plan(
    deadline: Option<std::time::Instant>,
    now: std::time::Instant,
) -> (u64, bool) {
    match deadline {
        None => (100, false),
        Some(d) => {
            let remaining = d.saturating_duration_since(now);
            if remaining.is_zero() {
                (0, true)
            } else {
                (remaining.as_millis().min(100) as u64, false)
            }
        }
    }
}

pub(crate) fn is_quit_key(key: KeyEvent) -> bool {
    key.kind == KeyEventKind::Press
        && (key.code == KeyCode::Char('q')
            || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)))
}

/// 命令行模式按键（M1-5 最小集）。
pub(crate) fn command_input(input: &mut String, key: KeyEvent) -> CmdInput {
    if key.kind != KeyEventKind::Press {
        return CmdInput::Continue;
    }
    match key.code {
        KeyCode::Enter => CmdInput::Submit,
        KeyCode::Esc => CmdInput::Cancel,
        KeyCode::Backspace => {
            input.pop();
            CmdInput::Continue
        }
        KeyCode::Char(c) => {
            input.push(c);
            CmdInput::Continue
        }
        _ => CmdInput::Continue,
    }
}

/// M24-6：↑ 移到更旧的历史条目；首次进入保存当前草稿输入。空历史无操作。
pub(crate) fn hist_prev(
    hist: &[String],
    pos: &mut Option<usize>,
    input: &mut String,
    draft: &mut String,
) {
    if hist.is_empty() {
        return;
    }
    let i = match *pos {
        None => {
            *draft = std::mem::take(input);
            hist.len() - 1
        }
        Some(i) => i.saturating_sub(1),
    };
    *pos = Some(i);
    *input = hist[i].clone();
}

/// M24-6：↓ 移到更新的历史条目；越过最新一条回到草稿输入（游标清空）。
pub(crate) fn hist_next(
    hist: &[String],
    pos: &mut Option<usize>,
    input: &mut String,
    draft: &mut String,
) {
    let i = match *pos {
        None => return,
        Some(i) => i + 1,
    };
    if i >= hist.len() {
        *pos = None;
        *input = std::mem::take(draft);
    } else {
        *pos = Some(i);
        *input = hist[i].clone();
    }
}

/// 取输入最后一个空白分隔词；返回（词前部分, 词）。字符边界按 char_indices
/// 处理，多字节安全。
pub(crate) fn split_last_word(s: &str) -> (String, String) {
    match s.char_indices().rfind(|&(_, c)| c.is_whitespace()) {
        Some((i, _)) => (s[..=i].to_owned(), s[i + 1..].to_owned()),
        None => (String::new(), s.to_owned()),
    }
}

/// M24-7：Tab 路径补全——输入最后一个空白分隔词按前缀匹配候选名
/// （焦点 pane 条目，M21-E1 同源）；输入自上次补全后未改动则循环到下一
/// 候选（尾→首环绕），否则重建候选列表。返回候选总数（调用方超 10 个
/// 时显示计数）。无候选时不改输入。
pub(crate) fn tab_complete(
    input: &mut String,
    comp: &mut Option<CmdComp>,
    names: &[String],
) -> usize {
    let unchanged = matches!(
        comp.as_ref(),
        Some(c) if *input == format!("{}{}", c.head, c.candidates[c.index])
    );
    if unchanged {
        // AUDIT-8：不变量（unchanged ⇒ comp 存在）用 filter 表达，去 expect。
        if let Some(c) = comp
            .as_mut()
            .filter(|c| *input == format!("{}{}", c.head, c.candidates[c.index]))
        {
            c.index = (c.index + 1) % c.candidates.len();
        }
    } else {
        let (head, token) = split_last_word(input);
        let candidates: Vec<String> = names
            .iter()
            .filter(|n| n.starts_with(token.as_str()))
            .cloned()
            .collect();
        if candidates.is_empty() {
            *comp = None;
            return 0;
        }
        *comp = Some(CmdComp {
            head,
            candidates,
            index: 0,
        });
    }
    // AUDIT-8：去 expect——comp 缺失（不可达）时安全返回 0（无候选补全）。
    let Some(c) = comp.as_ref() else {
        return 0;
    };
    *input = format!("{}{}", c.head, c.candidates[c.index]);
    c.candidates.len()
}

/// M24-7：`~` 展开为 HOME（裸 `~` → HOME，`~/…` → HOME/…；其余原样）。
/// 复用 M19-4 的 expand_tilde（HOME 缺失时原样返回）。
pub(crate) fn expand_home_arg(arg: &str) -> String {
    if arg == "~"
        && let Ok(home) = std::env::var("HOME")
    {
        return home;
    }
    crate::remote::config::expand_tilde(std::path::Path::new(arg))
        .to_string_lossy()
        .into_owned()
}

/// M24-8：左右 pane 条目对比（文件名 + 大小；目录跳过——目录大小无比较
/// 意义）。输出序：左 pane 遍历（`=` 同名同大小 / `!` 同名异大小 /
/// `<` 仅左），随后右 pane 独有（`>` 仅右）。路径取实际所在侧
/// （=/!/< 左，> 右），size 透传供属性列展示。
/// M29-1：bycontents 单文件读取上限（沿用 M11/AUDIT-17 护栏口径
/// 64MB）——超限不读内容、按"不同（未知）"计并在完成消息中标注。
pub(crate) const COMPARE_MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// M29-1：SHA-256 分块哈希（1MB 块，不整读入内存）。
fn sha256_file(path: &Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read as _;
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    // digest 0.11 的 GenericArray 不再实现 LowerHex——手写小写 hex（零新增依赖）。
    Ok(digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>())
}

pub(crate) fn compare_panes(
    left_base: &Path,
    left: &[crate::tab::FileItem],
    right_base: &Path,
    right: &[crate::tab::FileItem],
    by_contents: bool,
) -> Vec<crate::search::ResultItem> {
    let right_sizes: std::collections::HashMap<&str, u64> = right
        .iter()
        .filter(|i| !i.is_dir)
        .map(|i| (i.name.as_str(), i.size))
        .collect();
    let mut matched: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for item in left.iter().filter(|i| !i.is_dir) {
        match right_sizes.get(item.name.as_str()) {
            Some(&rsize) => {
                matched.insert(item.name.as_str());
                let marker = if rsize != item.size {
                    "!"
                } else if !by_contents {
                    "="
                } else {
                    // M29-1：同尺寸 → 内容判定；超限/读取失败按不同（未知）计。
                    let lp = left_base.join(&item.name);
                    let rp = right_base.join(&item.name);
                    let oversized = rsize > COMPARE_MAX_FILE_BYTES;
                    let same = if oversized {
                        None
                    } else {
                        sha256_file(&lp)
                            .ok()
                            .zip(sha256_file(&rp).ok())
                            .map(|(a, b)| a == b)
                    };
                    match same {
                        Some(true) => "=",
                        Some(false) => "!",
                        None => "!",
                    }
                };
                out.push(compare_item(left_base, item, marker));
            }
            None => out.push(compare_item(left_base, item, "<")),
        }
    }
    for item in right.iter().filter(|i| !i.is_dir) {
        if !matched.contains(item.name.as_str()) {
            out.push(compare_item(right_base, item, ">"));
        }
    }
    out
}

/// M24-8：单条对比结果构造（text 承载前缀标记）。
pub(crate) fn compare_item(
    base: &Path,
    item: &crate::tab::FileItem,
    marker: &str,
) -> crate::search::ResultItem {
    crate::search::ResultItem {
        path: base.join(&item.name),
        line: None,
        text: marker.to_owned(),
        size: item.size,
    }
}

/// AskPass 模式单键结果（M12-3；独立函数便于单测）。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AskPassResult {
    /// 继续输入。
    Continue,
    /// Enter：提交密码重试提权。
    Retry { password: String },
    /// Esc：取消（M12-4 干净中止）。
    Cancel,
}

/// AskPass 模式单键处理（M12-3）：只维护密码缓冲，模式状态由调用方迁移。
pub(crate) fn askpass_key(input: &mut String, code: KeyCode) -> AskPassResult {
    match code {
        KeyCode::Enter => AskPassResult::Retry {
            password: std::mem::take(input),
        },
        KeyCode::Esc => {
            input.clear();
            AskPassResult::Cancel
        }
        KeyCode::Backspace => {
            input.pop();
            AskPassResult::Continue
        }
        KeyCode::Char(c) => {
            input.push(c);
            AskPassResult::Continue
        }
        _ => AskPassResult::Continue,
    }
}

/// M18-1：排序对话框按键处理（字母=直接应用；空格=换向；回车=应用
/// 高亮字段当前方向；j/k 移动；Esc=取消）。
pub(crate) fn handle_sort_dialog_key(
    _state: &mut AppState,
    key: KeyEvent,
    cursor: &mut usize,
    desc: &mut bool,
    msg: &mut Option<String>,
) -> SortDialogOutcome {
    let field_of = |code: KeyCode| -> Option<(crate::tab::SortField, bool)> {
        SORT_FIELDS.iter().find_map(|(lo, hi, _, f)| {
            if *lo == code {
                Some((*f, false))
            } else if *hi == code {
                Some((*f, true))
            } else {
                None
            }
        })
    };
    if let Some((field, d)) = field_of(key.code) {
        let _ = msg;
        return SortDialogOutcome::Apply(crate::tab::SortSpec { field, desc: d });
    }
    match key.code {
        KeyCode::Esc => SortDialogOutcome::Close,
        KeyCode::Down | KeyCode::Char('j') => {
            *cursor = (*cursor + 1).min(SORT_FIELDS.len() - 1);
            SortDialogOutcome::None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            *cursor = cursor.saturating_sub(1);
            SortDialogOutcome::None
        }
        KeyCode::Char(' ') => {
            *desc = !*desc;
            SortDialogOutcome::None
        }
        KeyCode::Enter => {
            let field = SORT_FIELDS[*cursor].3;
            SortDialogOutcome::Apply(crate::tab::SortSpec { field, desc: *desc })
        }
        _ => SortDialogOutcome::None,
    }
}

/// 右 pane 焦点下的按键分发（M14-1）：精简导航集。
/// SIMPLIFIED：可视选择（v）、标记（t）、树（:tree 仍可退出）、提权（写
/// 命令 EACCES 时提示切回左 pane）仅左 pane；搜索（/、n/N）自 M34-3 起
/// 解除——经上方分派进入与左 pane 共用的搜索链路（作用对象 = 焦点 pane）。
pub(crate) fn handle_right_key(
    state: &mut AppState,
    msg: &mut Option<String>,
    key: KeyEvent,
) -> Outcome {
    if key.kind != KeyEventKind::Press {
        return Outcome::Continue;
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let cfg = state.config.clone();
    let tab = state.active_tab_mut();
    let half = 8isize; // 右 pane 半页步进（固定近似值，视口高度不可得）
    match key.code {
        KeyCode::Tab | KeyCode::Char(' ') => {
            // M21-F2：焦点经边框/当前行样式呈现，状态栏不再提示焦点文字。
            tab.focus = crate::tab::Focus::Left;
        }
        KeyCode::Char('j') | KeyCode::Down if !ctrl => tab.right.move_cursor(1),
        KeyCode::Char('k') | KeyCode::Up if !ctrl => tab.right.move_cursor(-1),
        KeyCode::Char('d') if ctrl => tab.right.move_cursor(half),
        KeyCode::Char('u') if ctrl => tab.right.move_cursor(-half),
        // M41-1：`g` 序列（gt/gT/Ngt/gg）已由 handle_key 右 pane 分支拦截，
        // 此处不再有直跳顶部的单键 `g`（与左 pane gg 序列语义统一）。
        KeyCode::Char('G') if !ctrl => tab.right.cursor_bottom(),
        KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right if !ctrl => {
            if let Err(e) = tab.right.enter_selected(cfg) {
                *msg = Some(format!("进入失败: {e}"));
            } else {
                // M21-F9：右 pane 进入成功同样实时记录（侧别 'R'）。
                let tab_id = state.active;
                record_live_visit(state, tab_id, 'R');
            }
        }
        KeyCode::Char('h') | KeyCode::Left if !ctrl => {
            if let Err(e) = tab.right.return_parent(cfg) {
                *msg = Some(format!("返回失败: {e}"));
            }
        }
        KeyCode::Char('g') if ctrl => {
            // Ctrl-G：右 pane 光标文件的详情对话框。
            if let (Location::Physical(base), Some(item)) =
                (&tab.right.location, tab.right.entries.get(tab.right.cursor))
            {
                let path = base.join(&item.name);
                let text = file_info_text(&path, &item.name);
                return Outcome::FileInfo { path, text };
            }
        }
        KeyCode::Char('w') if !ctrl => {
            // view 切换：右 pane 目录列表 ↔ 预览视图（M13-9 语义保持）。
            let on = tab.toggle_view();
            *msg = Some(if on {
                "view: 预览已开启（Tab 进入预览）".to_owned()
            } else {
                "view: 已关闭预览".to_owned()
            });
        }
        KeyCode::Char('d' | 'D' | 'c') if !ctrl => {
            *msg = Some("右 pane 仅导航（文件操作请 Tab 切回左 pane）".to_owned());
        }
        _ => {}
    }
    Outcome::Continue
}

/// 分发 Normal 模式按键。
///
/// `viewport_h` 为列表区高度（行数），用于 `Ctrl-u`/`Ctrl-d` 的半页步进。
/// 已接入：`j/k`、`l`/`Enter`、`h`、`gg`、`G`、`Ctrl-u/d`、`gt`/`gT`/`N gt`、
/// `/`、`n`/`N`、`dd`（删除确认）、`u`（撤销）、`v`（可视选择）、Esc（退出选择）、
/// `t`（tag 标记）、`q`/`Ctrl-C`、`:`。
/// M18-1：排序对话框字段表（小写键, 大写键, 说明, 字段）——字母直接应用；
/// 对话框内 `s`/`S` 是"大小"字段键，不触发 shell（模态）。
pub(crate) const SORT_FIELDS: [(KeyCode, KeyCode, &str, crate::tab::SortField); 5] = [
    (
        KeyCode::Char('n'),
        KeyCode::Char('N'),
        "名称 name",
        crate::tab::SortField::Name,
    ),
    (
        KeyCode::Char('t'),
        KeyCode::Char('T'),
        "自然 natural",
        crate::tab::SortField::Natural,
    ),
    (
        KeyCode::Char('m'),
        KeyCode::Char('M'),
        "修改时间 mtime",
        crate::tab::SortField::Mtime,
    ),
    (
        KeyCode::Char('s'),
        KeyCode::Char('S'),
        "大小 size",
        crate::tab::SortField::Size,
    ),
    (
        KeyCode::Char('e'),
        KeyCode::Char('E'),
        "扩展名 ext",
        crate::tab::SortField::Ext,
    ),
];

/// M23-6：Frecency Pane 按键处理（可测抽取）。
/// 搜索态：输入即搜（Char 入 query），方向键移动，Esc 清 query。
/// Normal 态：j/k/gg/G 移动，l/Enter 跳转，y/n/f/c 复制，e 编辑，w view，
/// Esc 退出 pane 模式，Tab 切换焦点。
pub(crate) fn frec_pane_key(
    state: &mut AppState,
    input: &mut InputState,
    msg: &mut Option<String>,
    key: KeyEvent,
) -> Outcome {
    let Some(pane) = state.active_tab_mut().pane_frecency.as_mut() else {
        return Outcome::Continue;
    };
    if key.kind != KeyEventKind::Press {
        return Outcome::Continue;
    }
    let visible_len = filter_jump(&pane.entries, &pane.query).len();
    let bottom = visible_len.saturating_sub(1);
    // M23-7（修订）：Normal 态键位与 Normal 一致（作用于光标结果条目）。
    if pane.normal {
        match key.code {
            KeyCode::Esc => {
                // 退出 pane 模式回目录。
                state.active_tab_mut().pane_frecency = None;
                *msg = None;
                return Outcome::Continue;
            }
            KeyCode::Enter | KeyCode::Char('l') => {
                let visible = filter_jump(&pane.entries, &pane.query);
                let picked = visible.get(pane.cursor).cloned();
                state.active_tab_mut().pane_frecency = None;
                match picked {
                    Some(entry) => match jump_to(state, &entry) {
                        Ok(m) => *msg = Some(m),
                        Err(e) => *msg = Some(e),
                    },
                    None => *msg = Some("跳转: 无选定条目".to_owned()),
                }
                return Outcome::Continue;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                pane.cursor = (pane.cursor + 1).min(bottom);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                pane.cursor = pane.cursor.saturating_sub(1);
            }
            KeyCode::Char('g') => {
                if pane.g_pending {
                    pane.cursor = 0;
                    pane.g_pending = false;
                } else {
                    pane.g_pending = true;
                }
            }
            KeyCode::Char('G') => {
                pane.cursor = bottom;
                pane.g_pending = false;
            }
            KeyCode::Tab => {
                // Tab 切换焦点 pane（frecency 保持激活）。
                let tab = state.active_tab_mut();
                tab.focus = match tab.focus {
                    crate::tab::Focus::Left => crate::tab::Focus::Right,
                    crate::tab::Focus::Right => crate::tab::Focus::Left,
                };
            }
            KeyCode::Char('y') => {
                // y 前缀：y-n 名称 / y-f 路径 / y-c 内容（作用于光标结果）。
                // 用 seq 暂存等待第二键。
                input
                    .seq
                    .push(crate::keymap::Key::plain(KeyCode::Char('y')));
                return Outcome::Continue;
            }
            KeyCode::Char('e') => {
                // e：$EDITOR 打开光标结果文件。
                let visible = filter_jump(&pane.entries, &pane.query);
                if let Some(entry) = visible.get(pane.cursor) {
                    let path = std::path::PathBuf::from(&entry.path);
                    if path.is_file() {
                        state.active_tab_mut().pane_frecency = None;
                        return Outcome::EditFile { path };
                    }
                    *msg = Some("目录不支持编辑".to_owned());
                }
            }
            KeyCode::Char('d') => {
                // M23-8：d 删除光标 frecency 条目（Remove 墓碑 + pane 内移除）。
                let visible = filter_jump(&pane.entries, &pane.query);
                if let Some(entry) = visible.get(pane.cursor).cloned() {
                    let topic = entry.topic.clone();
                    let path = entry.path.clone();
                    // 追加 Remove 墓碑事件（持久化删除）。
                    let _ = crate::frecency::remove_entry(&topic, &path);
                    // pane 内移除（从 entries 和可见列表）。
                    pane.entries.retain(|e| e.path != entry.path);
                    pane.cursor = pane.cursor.min(
                        filter_jump(&pane.entries, &pane.query)
                            .len()
                            .saturating_sub(1),
                    );
                    *msg = Some(format!("已删除 frecency 条目: {path}"));
                }
            }
            KeyCode::Char('w') => {
                // w：对侧进入 View 模式（预览/查看光标条目）。
                state.active_tab_mut().view_mode = !state.active_tab_mut().view_mode;
                let tab = state.active_tab_mut();
                tab.focus = match tab.focus {
                    crate::tab::Focus::Left => crate::tab::Focus::Right,
                    crate::tab::Focus::Right => crate::tab::Focus::Left,
                };
            }
            KeyCode::Char(c) => {
                // 可打印字符回搜索态（携带字符重新筛选）。
                pane.normal = false;
                pane.query.clear();
                pane.query.push(c);
                pane.cursor = 0;
                pane.g_pending = false;
            }
            _ => {}
        }
        return Outcome::Continue;
    }
    // 搜索态
    match key.code {
        KeyCode::Esc => {
            // 搜索态 Esc → Normal（清 query）。
            pane.normal = true;
            pane.query.clear();
            pane.cursor = pane.cursor.min(bottom);
            pane.g_pending = false;
        }
        KeyCode::Enter => {
            let visible = filter_jump(&pane.entries, &pane.query);
            let picked = visible.get(pane.cursor).cloned();
            state.active_tab_mut().pane_frecency = None;
            match picked {
                Some(entry) => match jump_to(state, &entry) {
                    Ok(m) => *msg = Some(m),
                    Err(e) => *msg = Some(e),
                },
                None => *msg = Some("跳转: 无选定条目".to_owned()),
            }
        }
        KeyCode::Backspace => {
            pane.query.pop();
            pane.cursor = 0;
        }
        KeyCode::Down => {
            pane.cursor = (pane.cursor + 1).min(bottom);
        }
        KeyCode::Up => {
            pane.cursor = pane.cursor.saturating_sub(1);
        }
        KeyCode::Tab => {
            let tab = state.active_tab_mut();
            tab.focus = match tab.focus {
                crate::tab::Focus::Left => crate::tab::Focus::Right,
                crate::tab::Focus::Right => crate::tab::Focus::Left,
            };
        }
        KeyCode::Char(c) => {
            pane.query.push(c);
            pane.cursor = 0;
        }
        _ => {}
    }
    Outcome::Continue
}

pub(crate) fn handle_key(
    state: &mut AppState,
    input: &mut InputState,
    msg: &mut Option<String>,
    key: KeyEvent,
    viewport_h: u16,
) -> Outcome {
    // M23-6：Frecency Pane 模式——全部键路由到 pane 键处理。
    if state.active_tab().pane_frecency.is_some() {
        return frec_pane_key(state, input, msg, key);
    }
    if key.kind != KeyEventKind::Press {
        return Outcome::Continue; // Release 先过滤，避免误消费待决序列。
    }
    // M24-4：可视模式 d/y/x 直接对选中区域执行（不再需要 dd/yy 序列）。
    if state.active_tab().visual.is_some() {
        match key.code {
            KeyCode::Char('d') | KeyCode::Char('x') => {
                input.count = None;
                // M24-4：可视范围条目加 tag → 触发批量删除确认（复用 M3-3 管线）。
                if let Some((lo, hi)) = state.active_tab().visual_range() {
                    let tab = state.active_tab_mut();
                    for i in lo..=hi.min(tab.entries.len().saturating_sub(1)) {
                        if let Some(item) = tab.entries.get(i) {
                            tab.tags.insert(item.name.clone());
                        }
                    }
                    tab.visual = None;
                    // M24-4：直接批量删除（tag 复用 M3-3，撤销栈可逆）。
                    execute_tag_delete(state, msg);
                }
                return Outcome::Continue;
            }
            KeyCode::Char('y') => {
                input.count = None;
                let range = state.active_tab().visual_range();
                if let Some((lo, hi)) = range {
                    let base = state.active_tab().location.base_dir().to_path_buf();
                    let paths: Vec<String> = state
                        .active_tab()
                        .entries
                        .get(lo..=hi.min(state.active_tab().entries.len().saturating_sub(1)))
                        .map(|slice| {
                            slice
                                .iter()
                                .map(|i| base.join(&i.name).to_string_lossy().replace('\\', "/"))
                                .collect()
                        })
                        .unwrap_or_default();
                    let text = paths.join("\n");
                    copy_to_clipboard(&text);
                    *msg = Some(format!("copied {} 项", paths.len()));
                }
                return Outcome::Continue;
            }
            _ => {}
        }
    }

    // 数字计数：`N gt`（M1-5）。其他动作键会丢弃未消费的计数。
    if let KeyCode::Char(c) = key.code
        && c.is_ascii_digit()
    {
        let d = c as u8 - b'0';
        input.count = Some(input.count.unwrap_or(0) * 10 + usize::from(d));
        return Outcome::Continue;
    }

    // M10-3：序列续接——待决非空时经 Keymap 解析；任意第二键结束序列
    // （未知第二键整体丢弃，包括 `q`/`Ctrl-C`）。
    if !input.seq.is_empty() {
        // M13-1：f/F 序列的第二键 = 搜索字符（通配序列，Keymap 无法表达）。
        if input.seq.len() == 1
            && matches!(input.seq[0].code, KeyCode::Char('f') | KeyCode::Char('F'))
        {
            let forward = input.seq[0].code == KeyCode::Char('f');
            input.seq.clear();
            input.count = None;
            if let KeyCode::Char(c) = key.code
                && !state.active_tab_mut().jump_first_char(c, forward)
            {
                *msg = Some(format!("无首字符 {c} 的条目"));
            }
            return Outcome::Continue;
        }
        // M16 NAV-1：' 序列的第二键 = keyword 字符（快速跳转）。
        if input.seq.len() == 1 && input.seq[0].code == KeyCode::Char('\'') {
            input.seq.clear();
            input.count = None;
            if let KeyCode::Char(c) = key.code {
                quickjump(state, c, msg);
            }
            return Outcome::Continue;
        }
        // M25-2：m 序列的第二键 = marks 字符（写入 frecency keyword）。
        if input.seq.len() == 1 && input.seq[0].code == KeyCode::Char('m') {
            input.seq.clear();
            input.count = None;
            if let KeyCode::Char(c) = key.code {
                mark_set(state, c, msg);
            }
            return Outcome::Continue;
        }
        input.seq.push(crate::keymap::Key::plain(key.code));
        let count = input.count.take();
        input.count = None;
        let action = input.keymap.lookup(&input.seq);
        let result = execute_action(state, input, msg, action, count, viewport_h);
        input.seq.clear();
        if let Some(outcome) = result {
            return outcome;
        }
        return Outcome::Continue;
    }
    /// M10-5 GAP-1：Action 执行（seq 完成与单键用户绑定共用）。返回
    /// `Some(Outcome)` 表示模式迁移/退出请求；`None` 表示动作完成、继续循环。
    /// 序列清理由调用方统一处理。
    fn execute_action(
        state: &mut AppState,
        _input: &mut InputState,
        msg: &mut Option<String>,
        action: Option<crate::keymap::Action>,
        count: Option<usize>,
        _viewport_h: u16,
    ) -> Option<Outcome> {
        match action {
            Some(Action::ToTop) => {
                // M24-5：Ngg → 光标定位到第 N 行（1-based）。
                // M41-1：焦点分派——右 pane 焦点下 gg/Ngg 作用于右 pane 列表。
                if let Some(n) = count {
                    let tab = state.active_tab_mut();
                    if tab.focus == crate::tab::Focus::Right {
                        tab.right.cursor = (n - 1).min(tab.right.entries.len().saturating_sub(1));
                    } else {
                        tab.cursor = (n - 1).min(tab.entries.len().saturating_sub(1));
                        tab.scroll = tab.cursor;
                    }
                } else if state.active_tab().focus == crate::tab::Focus::Right {
                    state.active_tab_mut().right.cursor_top();
                } else {
                    state.active_tab_mut().cursor_top();
                }
            }
            Some(Action::NextTab) => match count {
                Some(n) => state.goto_tab(n),
                None => state.next_tab(),
            },
            Some(Action::PrevTab) => match count {
                Some(n) => state.goto_tab(n),
                None => state.prev_tab(),
            },
            Some(Action::DiffPut) | Some(Action::DiffObtain) => {
                // M29-2：对比结果视图同步（dp=取回另一侧到当前侧；
                // do=删除当前侧多出条目）——仅 Compare 结果视图生效。
                compare_sync(state, msg, matches!(action, Some(Action::DiffPut)));
            }
            Some(Action::DeleteSelected) => {
                if state.active_tab().in_results() && !state.active_tab().results_actions_enabled()
                {
                    *msg = Some("搜索结果视图只读".to_owned());
                    return None;
                }
                // M38-1：find/grep 结果视图 Delete → 删除命中文件（真实路径）。
                if state.active_tab().results_actions_enabled() {
                    let tab = state.active_tab();
                    if let Some(path) = tab.result_path_at(tab.cursor) {
                        let mut m = None;
                        if execute_delete(state, &mut m, path, false, None).is_some() {
                            *msg = Some("权限不足（可 l 定位后操作）".to_owned());
                        } else {
                            *msg = m;
                        }
                        return None;
                    }
                }
                if state.active_tab().on_parent_row {
                    // FEAT-2：`..` 行仅导航。
                    *msg = Some("`..` 行仅导航".to_owned());
                    return None;
                }
                let tab = state.active_tab();
                // M20-1：远端视图 → 远端删除（trash 可恢复）。
                if let Location::Remote { remote, path } = &tab.location
                    && let Some(item) = tab.entries.get(tab.cursor)
                {
                    let target = if path == std::path::Path::new("/") {
                        PathBuf::from(format!("/{}", item.name))
                    } else {
                        path.join(&item.name)
                    };
                    return Some(Outcome::ConfirmDelete(target, false, Some(remote.clone())));
                }
                if let (Location::Physical(base), Some(item)) =
                    (&tab.location, tab.entries.get(tab.cursor))
                {
                    return Some(Outcome::ConfirmDelete(base.join(&item.name), false, None));
                }
            }
            Some(action @ (Action::CopyName | Action::CopyPath | Action::CopyContent)) => {
                // M23-10：y 复制序列——选集（标记/可视/光标）逐条目取目标文本，
                // OSC52 写入剪贴板（零依赖），状态栏 `copied` 反馈。
                if state.active_tab().on_parent_row {
                    *msg = Some("`..` 行仅导航".to_owned());
                    return None;
                }
                let names = selected_names(state);
                if names.is_empty() {
                    *msg = Some("无选中条目".to_owned());
                    return None;
                }
                let is_physical = matches!(
                    state.active_tab().location,
                    crate::tab::Location::Physical(_)
                );
                // AUDIT-8：外层模式已限定 Copy 系列，去 unwrap。
                let text = match action {
                    Action::CopyName => names.join("\n"),
                    Action::CopyPath => {
                        let base = state.active_tab().location.base_dir().to_path_buf();
                        names
                            .iter()
                            .map(|n| base.join(n).to_string_lossy().replace('\\', "/"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    }
                    Action::CopyContent => {
                        if !is_physical {
                            *msg = Some("当前视图不支持复制内容".to_owned());
                            return None;
                        }
                        let base = state.active_tab().location.base_dir().to_path_buf();
                        let mut parts: Vec<String> = Vec::new();
                        for n in &names {
                            let p = base.join(n);
                            if p.is_dir() {
                                let mut listing: Vec<String> = match std::fs::read_dir(&p) {
                                    Ok(rd) => rd
                                        .filter_map(|e| e.ok())
                                        .map(|e| e.file_name().to_string_lossy().into_owned())
                                        .collect(),
                                    Err(_) => Vec::new(),
                                };
                                listing.sort();
                                parts.push(listing.join("\n"));
                            } else {
                                match std::fs::read_to_string(&p) {
                                    Ok(t) => parts.push(t),
                                    Err(_) => {
                                        *msg = Some(format!(
                                            "复制内容失败: {n}（非 UTF-8 或读取失败）"
                                        ));
                                        return None;
                                    }
                                }
                            }
                        }
                        parts.join("\n")
                    }
                    // AUDIT-8：去 unreachable!——外层模式已限定 Copy 系列，
                    // 此臂防御性兜底（与"不命中即忽略"语义一致）。
                    _ => return None,
                };
                let what = match action {
                    Action::CopyPath => text.lines().next().unwrap_or("").to_owned(),
                    _ => names[0].clone(), // 名称/内容：反馈条目名
                };
                let what = if names.len() > 1 {
                    format!("{} 项", names.len())
                } else {
                    what
                };
                copy_to_clipboard(&text);
                *msg = Some(format!("copied {what}"));
            }
            Some(Action::OpenEditor) => {
                // M23-10：e——$EDITOR 打开光标文件（目录提示不支持）。
                if state.active_tab().on_parent_row {
                    *msg = Some("`..` 行仅导航".to_owned());
                    return None;
                }
                if !matches!(
                    state.active_tab().location,
                    crate::tab::Location::Physical(_)
                ) {
                    *msg = Some("当前视图不支持编辑".to_owned());
                    return None;
                }
                let tab = state.active_tab();
                let is_dir = tab
                    .entries
                    .get(tab.cursor)
                    .map(|i| i.is_dir)
                    .unwrap_or(true);
                if is_dir {
                    *msg = Some("目录不支持编辑".to_owned());
                    return None;
                }
                let base = tab.location.base_dir().to_path_buf();
                let name = tab.entries[tab.cursor].name.clone();
                return Some(Outcome::EditFile {
                    path: base.join(name),
                });
            }
            Some(Action::RunCommand(index)) => {
                return Some(run_user_command(state, index as usize, msg));
            }
            Some(Action::GitUi) => {
                // M25-6：gi——PATH 优先级探测 gitui→ziggity→lazygit；复用
                // M18-4 分流（terminal_in_new_window 语义与 :! / 自定义命令一致）。
                return Some(match crate::git::find_git_tui() {
                    Some(bin) => {
                        let ui = state.config.ui.clone();
                        match run_external_target(&ui, bin) {
                            ExternalRun::Multiplexer(m) => {
                                *msg = Some(m);
                                Outcome::Continue
                            }
                            ExternalRun::CurrentPane(cmd) => Outcome::RunUserCommand { cmd },
                        }
                    }
                    None => {
                        *msg = Some(
                            "gi: 未找到 git TUI（PATH 探测 gitui / ziggity / lazygit 均失败）"
                                .to_owned(),
                        );
                        Outcome::Continue
                    }
                });
            }
            Some(Action::RenameStart) | Some(Action::RenameClear) | Some(Action::RenameAppend) => {
                // M21-E4：I/A 保留原名（光标首/尾），cc 清空（光标归零）。
                if state.active_tab().in_results() {
                    *msg = Some("搜索结果视图只读".to_owned());
                    return None;
                }
                if state.active_tab().on_parent_row {
                    *msg = Some("`..` 行仅导航".to_owned());
                    return None;
                }
                let tab = state.active_tab();
                if let (Location::Physical(base), Some(item)) =
                    (&tab.location, tab.entries.get(tab.cursor))
                {
                    let name = item.name.clone();
                    let (input, pos) = if matches!(action, Some(Action::RenameClear)) {
                        (String::new(), 0)
                    } else if matches!(action, Some(Action::RenameAppend)) {
                        (name.clone(), name.chars().count())
                    } else {
                        (name.clone(), 0)
                    };
                    return Some(Outcome::EnterRename {
                        path: base.join(&name),
                        input,
                        pos,
                    });
                }
            }
            Some(Action::RenameBatch) => {
                // M13-6：cw 批量重命名——收集标记（或光标单条）名单。
                // M38-1：find/grep 结果视图可操作（映射真实路径）。
                if state.active_tab().in_results() && !state.active_tab().results_actions_enabled()
                {
                    *msg = Some("搜索结果视图只读".to_owned());
                    return None;
                }
                if state.active_tab().on_parent_row {
                    // FEAT-2：`..` 行仅导航。
                    *msg = Some("`..` 行仅导航".to_owned());
                    return None;
                }
                let tab = state.active_tab();
                if let Location::Physical(base) = &tab.location {
                    let names: Vec<String> = if tab.tags.is_empty() {
                        tab.entries
                            .get(tab.cursor)
                            .map(|i| i.name.clone())
                            .into_iter()
                            .collect()
                    } else {
                        // 按列表顺序收集标记条目（确定性）。
                        tab.entries
                            .iter()
                            .filter(|i| tab.tags.contains(&i.name))
                            .map(|i| i.name.clone())
                            .collect()
                    };
                    if !names.is_empty() {
                        return Some(Outcome::BatchRename {
                            base: base.clone(),
                            names,
                        });
                    }
                }
            }
            Some(Action::DeletePermanent) => {
                // M13-3：DD 永久删除——确认提示后跳过暂存（不可逆强提示）。
                if state.active_tab().in_results() {
                    *msg = Some("搜索结果视图只读".to_owned());
                    return None;
                }
                if state.active_tab().on_parent_row {
                    // FEAT-2：`..` 行仅导航。
                    *msg = Some("`..` 行仅导航".to_owned());
                    return None;
                }
                let tab = state.active_tab();
                if let (Location::Physical(base), Some(item)) =
                    (&tab.location, tab.entries.get(tab.cursor))
                {
                    return Some(Outcome::ConfirmDelete(base.join(&item.name), true, None));
                }
            }
            Some(Action::HistPrev) | Some(Action::HistNext) => {
                // M16 NAV-3/M19-2：目录历史后退/前进（远端位置走会话恢复）。
                let forward = matches!(action, Some(Action::HistNext));
                if hist_move(state, forward, msg).is_err() {
                    return None; // 错误已入 msg
                }
            }
            Some(Action::Yank) => {
                // FIX-2：yy——可视选择 → 标记 → 光标起 N 条（count 前缀），
                // 放入复制缓冲区（cut=false；dd 的 cut 缓冲接线待后续）。
                if state.active_tab().on_parent_row {
                    // FEAT-2：`..` 行仅导航。
                    *msg = Some("`..` 行仅导航".to_owned());
                    return None;
                }
                // M17 SRCH：结果视图 yy = 命中文件路径本身。
                if let Some(r) = state.active_tab().results.as_ref() {
                    let count = count.unwrap_or(1).max(1);
                    let start = state.active_tab().cursor.min(r.items.len());
                    let end = (start + count).min(r.items.len());
                    let paths: Vec<PathBuf> =
                        r.items[start..end].iter().map(|i| i.path.clone()).collect();
                    let n = paths.len();
                    state.yank = Some(crate::state::YankBuffer {
                        paths,
                        cut: false,
                        source: crate::state::YankSource::Local,
                    });
                    *msg = Some(format!("Yanked {n} item(s)"));
                    return None;
                }
                // M20-1：远端视图 yy = 远端路径（YankSource::Remote）。
                if let Some(remote) = state.active_tab().location.remote_name() {
                    let count = count.unwrap_or(1).max(1);
                    let names: Vec<PathBuf> = {
                        let tab = state.active_tab();
                        let base = match &tab.location {
                            Location::Remote { path, .. } => path.clone(),
                            _ => PathBuf::from("/"),
                        };
                        let start = tab.cursor.min(tab.entries.len());
                        let end = (start + count).min(tab.entries.len());
                        tab.entries[start..end]
                            .iter()
                            .map(|i| {
                                if base == std::path::Path::new("/") {
                                    PathBuf::from(format!("/{}", i.name))
                                } else {
                                    base.join(&i.name)
                                }
                            })
                            .collect()
                    };
                    let n = names.len();
                    if n == 0 {
                        *msg = Some("yy: 无可复制的条目".to_owned());
                        return None;
                    }
                    state.yank = Some(crate::state::YankBuffer {
                        paths: names,
                        cut: false,
                        source: crate::state::YankSource::Remote(remote),
                    });
                    *msg = Some(format!("Yanked {n} item(s)"));
                    return None;
                }
                let count = count.unwrap_or(1).max(1);
                let names: Vec<String> = {
                    let tab = state.active_tab();
                    if matches!(tab.location, Location::Archive { .. }) {
                        Vec::new()
                    } else if let Some((lo, hi)) = tab.visual_range() {
                        tab.entries[lo..=hi]
                            .iter()
                            .map(|i| i.name.clone())
                            .collect()
                    } else if !tab.tags.is_empty() {
                        tab.entries
                            .iter()
                            .filter(|i| tab.tags.contains(&i.name))
                            .map(|i| i.name.clone())
                            .collect()
                    } else {
                        let start = tab.cursor.min(tab.entries.len());
                        let end = (start + count).min(tab.entries.len());
                        tab.entries[start..end]
                            .iter()
                            .map(|i| i.name.clone())
                            .collect()
                    }
                };
                if names.is_empty() {
                    *msg = Some("yy: 无可复制的条目".to_owned());
                } else {
                    let base = state.active_tab().location.base_dir().to_path_buf();
                    let paths: Vec<PathBuf> = names.iter().map(|n| base.join(n)).collect();
                    let n = paths.len();
                    state.yank = Some(crate::state::YankBuffer {
                        paths,
                        cut: false,
                        source: crate::state::YankSource::Local,
                    });
                    *msg = Some(format!("Yanked {n} item(s)"));
                }
            }
            _ => {}
        }
        None
    }

    // M42：只有显式 `Tab` 进入预览窗格（preview_focus）时，按键才路由给
    // 查看器（滚动/搜索）；焦点留在原 pane 时按键照常作用于目录列表，
    // 对侧预览由 refresh_viewer_follow 跟随光标。
    //
    // M42：view 模式开着、焦点仍在原 pane 时，Tab/Space 显式进入预览窗格
    // （按键随后路由给查看器）；预览窗格内的 Tab 由下方分支返回。
    if state.active_tab().view_mode
        && state.active_tab().viewer.is_some()
        && !state.active_tab().preview_focus
        && matches!(key.code, KeyCode::Tab | KeyCode::Char(' '))
    {
        input.count = None;
        state.active_tab_mut().preview_focus = true;
        input.switch_mode_map(Some("view"));
        *msg = Some("view: 焦点进入预览（Tab 返回）".to_owned());
        return Outcome::Continue;
    }

    // M17 VIEW-1：查看器内滚动/搜索/退出全部作用于查看器（列表光标不动）；
    // `:` 放行进入命令模式。
    if state.active_tab().viewer.is_some() && state.active_tab().preview_focus {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let half = (viewport_h.saturating_sub(1) / 2).max(1) as usize;
        match key.code {
            KeyCode::Tab => {
                // M42：预览窗格内 Tab → 焦点返回原 pane（预览保持开启）。
                input.count = None;
                state.active_tab_mut().preview_focus = false;
                input.switch_mode_map(None);
                *msg = Some("view: 焦点返回目录列表（Tab 再次进入预览）".to_owned());
                return Outcome::Continue;
            }
            KeyCode::Char('w') | KeyCode::Esc | KeyCode::Char('q') => {
                input.count = None;
                input.switch_mode_map(None);
                let tab = state.active_tab_mut();
                tab.viewer = None;
                tab.view_mode = false;
                tab.preview_focus = false;
                *msg = Some("已退出查看器".to_owned());
                return Outcome::Continue;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                input.count = None;
                state.active_tab_mut().viewer_scroll(1, half);
                return Outcome::Continue;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                input.count = None;
                state.active_tab_mut().viewer_scroll(-1, half);
                return Outcome::Continue;
            }
            KeyCode::Char('d') if ctrl => {
                input.count = None;
                state.active_tab_mut().viewer_scroll(2, half);
                return Outcome::Continue;
            }
            KeyCode::Char('u') if ctrl => {
                input.count = None;
                state.active_tab_mut().viewer_scroll(-2, half);
                return Outcome::Continue;
            }
            KeyCode::Char('G') => {
                input.count = None;
                state.active_tab_mut().viewer_goto(false);
                return Outcome::Continue;
            }
            KeyCode::Char('g') => {
                input.count = None;
                state.active_tab_mut().viewer_goto(true);
                return Outcome::Continue;
            }
            KeyCode::Char('/') => {
                input.count = None;
                return Outcome::EnterViewerSearch;
            }
            KeyCode::Char('n') => {
                input.count = None;
                if !state.active_tab_mut().viewer_jump_match(true) {
                    *msg = Some("无匹配".to_owned());
                }
                return Outcome::Continue;
            }
            KeyCode::Char('N') => {
                input.count = None;
                if !state.active_tab_mut().viewer_jump_match(false) {
                    *msg = Some("无匹配".to_owned());
                }
                return Outcome::Continue;
            }
            KeyCode::Char('[') => {
                // M39-2：PDF 查看器翻页（上一页；第 1 页停留；其余查看器 no-op）。
                input.count = None;
                state.active_tab_mut().viewer_prev_page();
                return Outcome::Continue;
            }
            KeyCode::Char(']') => {
                // M39-2：PDF 查看器翻页（下一页；越界页渲染失败可见，[ 退回）。
                input.count = None;
                state.active_tab_mut().viewer_next_page();
                return Outcome::Continue;
            }
            KeyCode::Char(':') => {} // 放行：进入命令模式
            _ => {
                input.count = None;

                return Outcome::Continue; // 查看器消费其余按键（列表不动）
            }
        }
    }

    if is_quit_key(key) {
        return Outcome::Quit;
    }

    // M14-1：右 pane 焦点——精简导航分支（SIMPLIFIED：可视选择/标记/树/
    // 选择器/提权仅左 pane；写命令经 exec 焦点化仍可用）。
    // M34-3：搜索解除 SIMPLIFIED——`/`/n/N 与左 pane 共用 Search 链路，
    // 作用对象为焦点 pane（Tab::search_move/status 已 focus-aware）。
    // M41-1：`:` 与 gt/gT/Ngt/gg 解除 SIMPLIFIED——命令模式经 focus_base
    // 作用于焦点 pane；`g` 起始待决序列复用左 pane seq 机制（gt/gT/Ngt 由
    // 默认 keymap 命中，gg 由 ToTop 臂焦点化；未知第二键整体丢弃）。
    if state.active_tab().focus == crate::tab::Focus::Right {
        match key.code {
            KeyCode::Char('/') => {
                input.count = None;
                return Outcome::EnterSearch;
            }
            KeyCode::Char(':') => {
                input.count = None;
                return Outcome::EnterCommand;
            }
            KeyCode::Char('g') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                input
                    .seq
                    .push(crate::keymap::Key::plain(KeyCode::Char('g')));
                return Outcome::Continue;
            }
            KeyCode::Char('n') => {
                input.count = None;
                run_search(state, msg, true);
                return Outcome::Continue;
            }
            KeyCode::Char('N') => {
                input.count = None;
                run_search(state, msg, false);
                return Outcome::Continue;
            }
            _ => {}
        }
        return handle_right_key(state, msg, key);
    }

    // M19-2：远端视图——l/Enter=进入子目录或打开、h=上级/返回本地。
    if matches!(state.active_tab().location, Location::Remote { .. }) {
        match key.code {
            KeyCode::Char('h') => {
                input.count = None;
                if let Err(e) = remote_up(state) {
                    *msg = Some(e);
                } else {
                    *msg = None;
                }
                return Outcome::Continue;
            }
            KeyCode::Char('l') | KeyCode::Enter => {
                input.count = None;
                let tab = state.active_tab();
                let (path, is_dir, name) = match (&tab.location, tab.entries.get(tab.cursor)) {
                    (Location::Remote { path, .. }, Some(item)) => {
                        (path.clone(), item.is_dir, item.name.clone())
                    }
                    _ => return Outcome::Continue,
                };
                if is_dir {
                    let child = if path == std::path::Path::new("/") {
                        PathBuf::from(format!("/{name}"))
                    } else {
                        path.join(&name)
                    };
                    if let Err(e) = remote_navigate(state, &child) {
                        *msg = Some(e);
                    } else {
                        *msg = None;
                    }
                } else {
                    // M22-6：远端文件打开 = read → 临时副本 → system opener；
                    // 副本保留至进程退出统一清理（Windows opener 可能锁定文件）。
                    let full = if path == std::path::Path::new("/") {
                        PathBuf::from(format!("/{name}"))
                    } else {
                        path.join(&name)
                    };
                    let remote = state
                        .active_tab()
                        .location
                        .remote_name()
                        .unwrap_or_default();
                    match remote_open_file(state, &remote, &full) {
                        Ok(m) => *msg = m,
                        Err(e) => *msg = Some(e),
                    }
                }
                return Outcome::Continue;
            }
            _ => {}
        }
    }

    // M17 SRCH：结果虚拟目录——l/Enter=定位、Esc/h=退出、v/t=只读拒绝；
    // 其余导航/yy/预览键落回既有处理（目录语义）。
    if state.active_tab().in_results() {
        match key.code {
            // M28-2：rename 预览——y 应用整批；其余类别落到既有处理（yy 等）。
            KeyCode::Char('y')
                if state
                    .active_tab()
                    .results
                    .as_ref()
                    .map(|r| r.kind == crate::search::SearchKind::RenamePreview)
                    .unwrap_or(false) =>
            {
                input.count = None;
                apply_rename_preview(state, msg);
                return Outcome::Continue;
            }
            KeyCode::Esc | KeyCode::Char('h') => {
                input.count = None;
                let cfg = state.config.clone();
                state.active_tab_mut().exit_results(cfg);
                *msg = Some("已退出搜索结果".to_owned());
                return Outcome::Continue;
            }
            KeyCode::Char('l') | KeyCode::Enter => {
                input.count = None;
                // M22-5：远端结果定位——退出结果视图 + remote_navigate 父目录
                // + 光标停在该文件（navigate_to 语义对远端不可用）。
                if matches!(state.active_tab().location, Location::Remote { .. }) {
                    match remote_locate_result(state) {
                        Ok(()) => *msg = None,
                        Err(e) => *msg = Some(format!("定位失败: {e}")),
                    }
                    return Outcome::Continue;
                }
                let cfg = state.config.clone();
                if let Err(e) = state.active_tab_mut().locate_result(cfg) {
                    *msg = Some(format!("定位失败: {e}"));
                } else {
                    *msg = None;
                }
                return Outcome::Continue;
            }
            KeyCode::Char('t') | KeyCode::Char('v') => {
                input.count = None;
                *msg = Some("搜索结果视图只读".to_owned());
                return Outcome::Continue;
            }
            _ => {}
        }
    }

    // M14-1/M18-4：Tab 或 Space 切换焦点 pane（左 → 右；vifm 语义）。
    // M21-F2：焦点经边框/当前行样式呈现，状态栏不再提示焦点文字。
    if (key.code == KeyCode::Tab || key.code == KeyCode::Char(' '))
        && key.kind == KeyEventKind::Press
    {
        input.count = None;
        state.active_tab_mut().focus = crate::tab::Focus::Right;
        return Outcome::Continue;
    }

    // Esc：退出可视选择（M3-1）；非可视模式忽略。
    if key.code == KeyCode::Esc {
        input.count = None;
        if state.active_tab().visual.is_some() {
            state.active_tab_mut().exit_visual();
            // M33-2：退出 visual → 还原 normal 主表。
            input.switch_mode_map(None);
        }
        return Outcome::Continue;
    }

    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let half = (viewport_h.saturating_sub(1) / 2).max(1) as isize;

    // M10-5 GAP-1：用户显式绑定优先于内建单键处理（prepend 覆盖默认）。
    {
        let probe = crate::keymap::Key {
            code: key.code,
            ctrl,
        };
        if input.keymap.is_explicit(&[probe]) {
            let action = input.keymap.lookup(&[probe]);
            let count = input.count.take();
            input.count = None;
            if let Some(outcome) = execute_action(state, input, msg, action, count, viewport_h) {
                return outcome;
            }
            return Outcome::Continue;
        }
    }

    // M13-5：Ctrl-G → 文件详情对话框（Physical 视图；arm 顺序在
    // Char('g') 前缀分支之前，避免被前缀捕获）。
    if ctrl && let KeyCode::Char('g') = key.code {
        input.count = None;
        if state.active_tab().on_parent_row {
            // FEAT-2：`..` 行仅导航。
            *msg = Some("`..` 行仅导航".to_owned());
            return Outcome::Continue;
        }
        let tab = state.active_tab();
        if let (Location::Physical(base), Some(item)) = (&tab.location, tab.entries.get(tab.cursor))
        {
            let path = base.join(&item.name);
            let text = file_info_text(&path, &item.name);
            return Outcome::FileInfo { path, text };
        }
        return Outcome::Continue;
    }

    // M13-7：树视图只读——写操作与导航入口拒绝（l/Enter/h/dd/DD/cw 前缀/u）。
    // Ctrl-d/u 半页滚动放行（ctrl 排除）。M41-2：←/→ 与 h/l 同受守卫。
    if state.active_tab().in_tree()
        && !ctrl
        && matches!(
            key.code,
            KeyCode::Char('l')
                | KeyCode::Enter
                | KeyCode::Char('h')
                | KeyCode::Right
                | KeyCode::Left
                | KeyCode::Char('d')
                | KeyCode::Char('D')
                | KeyCode::Char('c')
                | KeyCode::Char('u')
        )
    {
        input.count = None;
        *msg = Some("树视图为只读浏览（:tree 退出）".to_owned());
        return Outcome::Continue;
    }

    // M41-2：方向键 ↑↓←→ 与 k/j/l/h 别名进同款臂——环绕、计数前缀、
    // `..` 行、M21-F4 返回定位等行为完全一致。
    match key.code {
        KeyCode::Char('G') => state.active_tab_mut().cursor_bottom(),
        KeyCode::Char('d') if ctrl => state.active_tab_mut().move_cursor(half),
        KeyCode::Char('u') if ctrl => state.active_tab_mut().move_cursor(-half),
        KeyCode::Char('u') => {
            input.count = None;
            undo_last(state, msg);
        }
        KeyCode::Char('j') | KeyCode::Down => {
            // M13-4/M24-5：j/k 环绕 + 计数前缀 N 消耗。
            let n = input.count.take().unwrap_or(1).max(1);
            input.count = None;
            for _ in 0..n {
                state.active_tab_mut().move_cursor_wrap(1);
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            let n = input.count.take().unwrap_or(1).max(1);
            input.count = None;
            for _ in 0..n {
                state.active_tab_mut().move_cursor_wrap(-1);
            }
        }
        KeyCode::Char('l') | KeyCode::Enter | KeyCode::Right => {
            input.count = None;
            open_selected(state, msg);
        }
        KeyCode::Char('h') | KeyCode::Left => {
            let n = input.count.take().unwrap_or(1).max(1);
            input.count = None;
            for _ in 0..n {
                let cfg = state.config.clone();
                if let Err(e) = state.active_tab_mut().return_parent(cfg) {
                    *msg = Some(format!("返回失败: {e}"));
                    break;
                }
            }
        }
        KeyCode::Char('/') => {
            input.count = None;
            return Outcome::EnterSearch;
        }
        KeyCode::Char('f') | KeyCode::Char('F') => {
            // M13-1：f/F 进入首字母搜索序列（第二键为搜索字符）。
            input.count = None;
            input.seq.push(crate::keymap::Key::plain(key.code));
        }
        KeyCode::Char('\'') => {
            // M16 NAV-1：' 进入快速跳转序列（第二键为 keyword 字符）。
            input.count = None;
            input.seq.push(crate::keymap::Key::plain(key.code));
        }
        KeyCode::Char('m') => {
            // M25-2：m 进入标记序列（第二键为 marks 字符，字母与数字）。
            input.count = None;
            input.seq.push(crate::keymap::Key::plain(key.code));
        }
        KeyCode::Char(';') | KeyCode::Char(',') => {
            // M13-1：重复上次首字母搜索（; 同向 / , 反向）。
            input.count = None;
            let tab = state.active_tab();
            if let Some((c, fwd)) = tab.char_search {
                let forward = if key.code == KeyCode::Char(';') {
                    fwd
                } else {
                    !fwd
                };
                if !state.active_tab_mut().jump_first_char(c, forward) {
                    *msg = Some(format!("无首字符 {c} 的条目"));
                }
            } else {
                *msg = Some("无上次首字母搜索".to_owned());
            }
        }
        KeyCode::Char('=') => {
            input.count = None;
            return Outcome::EnterFilter;
        }
        KeyCode::Char('n') => {
            input.count = None;
            run_search(state, msg, true);
        }
        KeyCode::Char('N') => {
            input.count = None;
            run_search(state, msg, false);
        }
        KeyCode::Char(':') => {
            input.count = None;
            return Outcome::EnterCommand;
        }
        KeyCode::Char('o') => {
            // M23-6：Frecency Pane 模式——焦点 pane 进入内嵌搜索（非浮层）。
            input.count = None;
            match load_jump_entries(state, JumpKind::Unified) {
                Ok(entries) => {
                    return Outcome::OpenFrecencyPane { entries };
                }
                Err(e) => *msg = Some(e),
            }
        }
        // M10-3：`g`/`d`/`y`/`v`/`t` 前缀检测经 Keymap；单键动作经 dispatch。
        KeyCode::Char('g')
        | KeyCode::Char('d')
        | KeyCode::Char('D')
        | KeyCode::Char('c')
        | KeyCode::Char('y') => {
            let k = crate::keymap::Key::plain(key.code);
            if input.keymap.has_prefix(&[k]) {
                input.seq.push(k);
            }
        }
        KeyCode::Char('v') => {
            input.count = None;
            let entering = state.active_tab().visual.is_none();
            state.active_tab_mut().toggle_visual();
            // M33-2：进入/退出 visual → 切换 per-mode 键位表（未配置不换）。
            input.switch_mode_map(if entering { Some("visual") } else { None });
        }
        KeyCode::Char('V') => {
            // M24-3：行可视模式 V——与 v 同语义（选中条目），互斥自动。
            input.count = None;
            let entering = state.active_tab().visual.is_none();
            state.active_tab_mut().toggle_visual();
            input.switch_mode_map(if entering { Some("visual") } else { None });
        }
        KeyCode::Char('t') => {
            input.count = None;
            state.active_tab_mut().toggle_tag();
        }
        KeyCode::Char('w') => {
            // M13-9：view 模式切换（预览窗格完整内容视图 ↔ 摘要预览）。
            // M42：焦点保持在当前 pane；Tab 才进入预览窗格（per-mode 键位表
            // 随之切换，未配置时不换）。
            input.count = None;
            let on = state.active_tab_mut().toggle_view();
            *msg = Some(if on {
                "view: 预览已开启（Tab 进入预览）".to_owned()
            } else {
                "view: 已关闭预览".to_owned()
            });
        }
        KeyCode::Char('p') | KeyCode::Char('P') => {
            // FIX-2：PutCopy/PutMove——消费 yank 缓冲区（复制/移动到当前目录）。
            input.count = None;
            paste_yanked(state, key.code == KeyCode::Char('P'), msg);
        }
        KeyCode::Char('s') => {
            // M18-3：挂起 TUI 进入 shell（vifm 既有习惯；M18-4 接入
            // terminal_in_new_window 配置后按配置分流）。
            input.count = None;
            return Outcome::EnterShell;
        }
        KeyCode::Char('S') => {
            // M18-1：排序对话框（唯一排序入口；`s` 仍为 shell，见 M18-3）。
            input.count = None;
            return Outcome::EnterSortDialog;
        }
        _ => {
            // GAP-1：非 Char 键（F 键/Home 等）的单键用户绑定兜底。
            let plain = crate::keymap::Key::plain(key.code);
            let with_ctrl = crate::keymap::Key {
                code: key.code,
                ctrl,
            };
            let action = input
                .keymap
                .lookup(&[plain])
                .or_else(|| input.keymap.lookup(&[with_ctrl]));
            input.count = None;
            if let Some(action) = action
                && !input.keymap.is_explicit(&[with_ctrl])
            {
                // 非显式（默认表绑定同键）才在此兜底；显式已在上方拦截。
                if let Some(outcome) =
                    execute_action(state, input, msg, Some(action), None, viewport_h)
                {
                    return outcome;
                }
            }
        }
    }
    Outcome::Continue
}

/// FEAT-1/M41-3：鼠标事件处理——左键单击切换焦点 pane 并移动该 pane 光标
/// （高亮随焦点呈现；已聚焦行再次单击 = 打开，双击的第二击自然落到此路径；
/// 右 pane 打开 = M14-1 进入目录语义，文件 no-op）、滚轮逐行滚动作用于
/// 焦点 pane（每次精确移动一行，不跳变）。仅 Normal 模式响应（输入模式
/// 忽略）；标签栏/状态栏不响应；右键上下文菜单保持左 pane（SPEC M41
/// SIMPLIFIED）。Visual 选择由 tab.visual 锚点 + 光标移动自然延伸，鼠标
/// 只移动光标不破坏选区。
pub(crate) fn handle_mouse(
    state: &mut AppState,
    mode: &InputMode,
    mouse: &MouseEvent,
    msg: &mut Option<String>,
    area: ratatui::layout::Rect,
) {
    if !matches!(mode, InputMode::Normal) {
        return; // Command/Search 等输入模式忽略鼠标（SPEC M15 SIMPLIFIED）。
    }
    // 列表几何：与 render 同源（M21-F1）——M39-6 布局感知（Single 右零宽、
    // PreviewBottom 上半双栏；查看器覆盖 pane 的命中在 hit_*_index 守卫）。
    // 左 pane `..` 行占内区首行（FEAT-2，有父行时条目从第 2 行开始），
    // 右 pane 无 `..` 行（M14-1，条目从内区首行开始）。
    let [_, left, right, _] =
        pane_layout(area, state.config.ui.layout(), state.active_tab().view_mode);
    let borders = state.config.ui.pane_borders;
    let left_inner = left_list_area(borders, left);
    let right_inner = left_list_area(borders, right);
    let in_left = mouse.column >= left_inner.x
        && mouse.column < left_inner.x + left_inner.width
        && mouse.row >= left_inner.y
        && mouse.row < left_inner.y + left_inner.height;
    let in_right = mouse.column >= right_inner.x
        && mouse.column < right_inner.x + right_inner.width
        && mouse.row >= right_inner.y
        && mouse.row < right_inner.y + right_inner.height;
    match mouse.kind {
        // M41-3：滚轮作用于焦点 pane（左右内区均可发起）。
        MouseEventKind::ScrollUp if in_left || in_right => {
            if state.active_tab().focus == crate::tab::Focus::Right {
                state.active_tab_mut().right.move_cursor(-1);
            } else {
                state.active_tab_mut().move_cursor(-1);
            }
        }
        MouseEventKind::ScrollDown if in_left || in_right => {
            if state.active_tab().focus == crate::tab::Focus::Right {
                state.active_tab_mut().right.move_cursor(1);
            } else {
                state.active_tab_mut().move_cursor(1);
            }
        }
        MouseEventKind::Down(MouseButton::Left) if in_left => {
            let has_parent = state.active_tab().has_parent_row();
            let on_parent = has_parent && mouse.row == left_inner.y;
            if on_parent {
                let focused = {
                    let tab = state.active_tab();
                    tab.focus == crate::tab::Focus::Left && tab.on_parent_row
                };
                {
                    let tab = state.active_tab_mut();
                    tab.focus = crate::tab::Focus::Left;
                    if !focused {
                        // 单击聚焦 `..` 行。
                        tab.on_parent_row = true;
                    }
                }
                if focused {
                    // 已聚焦 `..` 行再次单击（含双击第二击）→ 返回上级。
                    open_selected(state, msg);
                }
            } else {
                let Some(idx) = hit_entry_index(state, mouse, area) else {
                    return;
                };
                let was_focused_row = {
                    let tab = state.active_tab();
                    tab.focus == crate::tab::Focus::Left && !tab.on_parent_row && idx == tab.cursor
                };
                {
                    let tab = state.active_tab_mut();
                    tab.focus = crate::tab::Focus::Left;
                    tab.on_parent_row = false;
                    if !was_focused_row {
                        // 单击切焦点 + 移光标，不打开。
                        tab.cursor = idx;
                    }
                }
                if was_focused_row {
                    // 已聚焦行再次单击（含双击第二击）→ 打开。
                    open_selected(state, msg);
                }
            }
        }
        MouseEventKind::Down(MouseButton::Left) if in_right => {
            // 右 pane（M41-3）：单击切焦点 + 移光标；已聚焦行再击 = 进入
            // 目录（M14-1 语义，文件 no-op）。
            let Some(idx) = hit_right_index(state, mouse, area) else {
                return;
            };
            let was_focused_row = {
                let tab = state.active_tab();
                tab.focus == crate::tab::Focus::Right && idx == tab.right.cursor
            };
            if was_focused_row {
                let cfg = state.config.clone();
                {
                    let tab = state.active_tab_mut();
                    tab.focus = crate::tab::Focus::Right;
                    if let Err(e) = tab.right.enter_selected(cfg) {
                        *msg = Some(format!("进入失败: {e}"));
                    }
                }
                // M21-F9：进入成功实时记录（与右 pane Enter 臂同款）。
                let tab_id = state.active;
                record_live_visit(state, tab_id, 'R');
            } else {
                let tab = state.active_tab_mut();
                tab.focus = crate::tab::Focus::Right;
                tab.right.cursor = idx;
            }
        }
        _ => {}
    }
}

/// M21-E8：跳转选择器按键结果。
pub(crate) enum JumpKey {
    Continue,
    Select,
    Close,
}

/// M21-E8：选择器按键处理（可测抽取；run 循环消费 Select/Close）。
/// 搜索态：输入即搜（Char 入筛选），方向键移动，Esc→Normal；
/// Normal 态：j/k/gg/G 移动、l/Enter 选定、可打印字符回搜索态、Esc 关闭。
pub(crate) fn jump_key(
    normal: &mut bool,
    g_pending: &mut bool,
    input: &mut String,
    cursor: &mut usize,
    visible_len: usize,
    key: &KeyEvent,
) -> JumpKey {
    if key.kind != KeyEventKind::Press {
        return JumpKey::Continue;
    }
    let bottom = visible_len.saturating_sub(1);
    match key.code {
        KeyCode::Esc => {
            if *normal {
                JumpKey::Close
            } else {
                *normal = true;
                input.clear();
                *g_pending = false;
                *cursor = (*cursor).min(bottom);
                JumpKey::Continue
            }
        }
        KeyCode::Enter => JumpKey::Select,
        KeyCode::Char('l') if *normal => JumpKey::Select,
        KeyCode::Down => {
            *cursor = (*cursor).saturating_add(1).min(bottom);
            JumpKey::Continue
        }
        KeyCode::Up => {
            *cursor = (*cursor).saturating_sub(1);
            JumpKey::Continue
        }
        KeyCode::Char('j') if *normal => {
            *cursor = (*cursor).saturating_add(1).min(bottom);
            JumpKey::Continue
        }
        KeyCode::Char('k') if *normal => {
            *cursor = (*cursor).saturating_sub(1);
            JumpKey::Continue
        }
        KeyCode::Char('g') if *normal => {
            // gg 顶：'g' 待决，第二键 'g' 落顶（单按不动作）。
            if *g_pending {
                *cursor = 0;
                *g_pending = false;
            } else {
                *g_pending = true;
            }
            JumpKey::Continue
        }
        KeyCode::Char('G') if *normal => {
            *cursor = bottom;
            *g_pending = false;
            JumpKey::Continue
        }
        KeyCode::Backspace if !*normal => {
            input.pop();
            *cursor = 0;
            JumpKey::Continue
        }
        KeyCode::Char(c) => {
            if *normal {
                // 可打印字符回搜索态（携带该字符重新筛选）。
                *normal = false;
                input.clear();
                input.push(c);
            } else {
                input.push(c);
            }
            *cursor = 0;
            *g_pending = false;
            JumpKey::Continue
        }
        _ => JumpKey::Continue,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn seq_timeout_deadline_and_expiry() {
        // AUDIT-6：多键序列可配置超时——纯函数层验证。超时后 run 循环
        // 清空 seq（不吞后续按键）；seq 清空 → 无截止。
        let now = std::time::Instant::now();
        // 无待决 → 预算 100（空转粒度）、不超时。
        assert_eq!(seq_poll_plan(None, now), (100, false));
        // 新待决（g）→ 截止 = now + timeout。
        let d1 = seq_deadline_after(1, 0, None, now, 500);
        assert_eq!(d1, Some(now + Duration::from_millis(500)));
        // 同一段待决（长度不变）→ 保持截止（不每帧顺延，否则永不超时）。
        assert_eq!(seq_deadline_after(1, 1, d1, now, 500), d1);
        // 前缀推进（g → gg）→ 重置截止。
        let later = now + Duration::from_millis(100);
        assert_eq!(
            seq_deadline_after(2, 1, d1, later, 500),
            Some(later + Duration::from_millis(500))
        );
        // 未到期 → 预算收缩（499ms 余量 → 预算封顶 100）。
        assert_eq!(
            seq_poll_plan(d1, now + Duration::from_millis(1)),
            (100, false)
        );
        // 到期 → (0, true)（调用方清空 seq，后续按键不被吞）。
        assert_eq!(
            seq_poll_plan(d1, now + Duration::from_millis(500)),
            (0, true)
        );
        // seq 清空 → 无截止。
        assert_eq!(seq_deadline_after(0, 0, d1, now, 500), None);
    }
}
