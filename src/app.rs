//! 事件循环：绘制目录列表与状态栏 → 阻塞等待事件 → 分发按键。
//!
//! M1-3 起支持多键序列（`g` 前缀：`gg`）；M1-5 起支持多标签
//! （`gt`/`gT`/`N gt`）与最小命令行（`:tabnew` / `:tabclose` / `:tabmove`；
//! 统一命令解析在 M5-4 接入）。

use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

use std::path::PathBuf;

use crate::keymap::Action;
use crate::state::AppState;
use crate::tab::{Location, SearchState, Tab};
use crate::{config, ops};

/// 判断按键是否请求退出：`q` 或 `Ctrl-C`。
fn is_quit_key(key: KeyEvent) -> bool {
    key.kind == KeyEventKind::Press
        && (key.code == KeyCode::Char('q')
            || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)))
}

/// Normal 模式输入状态：待决序列 + 数字计数（`N gt` 用，M1-5）。
/// `seq` 由 Keymap 驱动解析（M10-3 接线）。
#[derive(Debug)]
struct InputState {
    seq: Vec<crate::keymap::Key>,
    count: Option<usize>,
    keymap: crate::keymap::Keymap,
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            seq: Vec::new(),
            count: None,
            keymap: crate::keymap::Keymap::default_map(),
        }
    }
}

/// Normal 模式按键的处理结果。
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    /// 无状态变化。
    Continue,
    /// 请求退出应用。
    Quit,
    /// 请求进入命令行模式（`:`）。
    EnterCommand,
    /// 请求进入搜索输入模式（`/`，M1-7）。
    EnterSearch,
    /// 请求进入过滤输入模式（`=`，M4-2）。
    EnterFilter,
    /// 请求确认删除（`dd`，M2-2）：携带待删完整路径。
    ConfirmDelete(PathBuf),
}

/// 输入模式（M5-4 统一模式系统前先持有最小集合）。
#[derive(Debug, Default, PartialEq, Eq)]
enum InputMode {
    #[default]
    Normal,
    /// 命令行模式：正在输入 `:` 后的内容。
    Command { input: String },
    /// 搜索输入模式：正在输入 `/` 后的内容（M1-7）。
    Search { input: String },
    /// 过滤输入模式：正在输入 `=` 后的内容（M4-2）。
    Filter { input: String },
    /// 删除确认模式：`y` 执行，其余取消（M2-2）。
    Confirm { path: PathBuf },
    /// 标记批量删除确认模式（`:tag delete`，M3-3）。
    ConfirmTagDelete,
    /// 帮助覆盖层（`:help`，M5-3；任意键返回）。
    Help,
}

/// 命令行单键处理结果。
#[derive(Debug, PartialEq, Eq)]
enum CmdInput {
    Continue,
    Submit,
    Cancel,
}

/// 命令行模式按键（M1-5 最小集）。
fn command_input(input: &mut String, key: KeyEvent) -> CmdInput {
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

/// `:` 命令执行结果（M2-6）：退出请求或完成（可携带提示）。
#[derive(Debug, PartialEq, Eq)]
enum ExecOutcome {
    /// 关闭最后一个 tab，应用退出。
    Quit,
    /// 打开帮助覆盖层（`:help`，M5-3；完整帮助页在 M6-2）。
    EnterHelp,
    /// 命令完成，可携带状态栏提示。
    Done { msg: Option<String> },
    /// 请求确认批量删除标记文件（`:tag delete`，M3-3）。
    ConfirmTagDelete,
}

/// 测试兼容包装：沿用 `Ok(true)` = 退出的旧契约（丢弃提示）。
#[cfg(test)]
fn exec_command(state: &mut AppState, line: &str) -> Result<bool, String> {
    exec_command_impl(state, line).map(|o| matches!(o, ExecOutcome::Quit))
}

/// 取当前 tab 的物理操作基准目录；归档视图只读（M7 边界），返回错误提示。
/// （归档内条目不可改——SPEC §4.5"归档内不出现删除/重命名/移动入口"。）
fn physical_base(state: &AppState) -> Result<PathBuf, String> {
    match &state.active_tab().location {
        Location::Physical(p) => Ok(p.clone()),
        Location::Archive { .. } => Err("归档视图只读（解压后可修改）".to_owned()),
    }
}

/// 执行最小 `:` 命令（M1-5/M2-6）。
fn exec_command_impl(state: &mut AppState, line: &str) -> Result<ExecOutcome, String> {
    let mut parts = line.split_whitespace();
    match parts.next() {
        None => Ok(ExecOutcome::Done { msg: None }),
        Some("q" | "quit") => Ok(ExecOutcome::Quit),
        Some("cd") => {
            // M5-3：切换当前 tab 到目标目录（保留原名导航语义）。
            let dir_arg = parts.next().ok_or_else(|| "cd: 缺少目录".to_owned())?;
            let target = PathBuf::from(dir_arg);
            if !target.is_dir() {
                return Err(format!("cd: 目录不存在: {dir_arg}"));
            }
            let cfg = state.config;
            let tab = state.active_tab_mut();
            tab.history.push(tab.location.clone());
            tab.location = Location::Physical(target);
            tab.cursor = 0;
            tab.scroll = 0;
            let _ = tab.refresh(cfg);
            Ok(ExecOutcome::Done { msg: None })
        }
        Some("help") => Ok(ExecOutcome::EnterHelp),
        Some("tabnew") => {
            if state.new_tab_here() {
                Ok(ExecOutcome::Done { msg: None })
            } else {
                Err("tabnew: 目录读取失败".to_owned())
            }
        }
        Some("tabclose") => {
            // 关闭标签不可撤销：明确提示（M2-6）。
            if state.close_tab() {
                Ok(ExecOutcome::Quit)
            } else {
                Ok(ExecOutcome::Done {
                    msg: Some("标签已关闭（不可撤销）".to_owned()),
                })
            }
        }
        Some("tabmove") => {
            let n = parts
                .next()
                .map(str::parse::<usize>)
                .transpose()
                .map_err(|_| "tabmove: 位置必须是数字".to_owned())?;
            state.move_tab(n);
            Ok(ExecOutcome::Done { msg: None })
        }
        Some("rename") => {
            // M2-3：重命名选中条目（新名字不含空白；含空格的名字在 M5-4 统一解析）。
            let new_name = parts
                .next()
                .ok_or_else(|| "rename: 缺少新名字".to_owned())?;
            let base = physical_base(state)?;
            let tab = state.active_tab();
            let path = tab
                .entries
                .get(tab.cursor)
                .map(|item| base.join(&item.name))
                .ok_or_else(|| "rename: 无选中条目".to_owned())?;
            let op = ops::rename_to(state.active, &path, new_name)
                .map_err(|e| format!("rename: {e}"))?;
            state.undo_stack.push(op);
            let cfg = state.config;
            if let Some(tab) = state.tabs.get_mut(state.active) {
                let _ = tab.refresh(cfg);
            }
            Ok(ExecOutcome::Done { msg: None })
        }
        Some("move") => {
            // M2-4：移动选中条目到目标目录（保留原名；`yy`/`P` 键位在 M10 接入）。
            let dir_arg = parts
                .next()
                .ok_or_else(|| "move: 缺少目标目录".to_owned())?;
            let target = PathBuf::from(dir_arg);
            let base = physical_base(state)?;
            let tab = state.active_tab();
            let path = tab
                .entries
                .get(tab.cursor)
                .map(|item| base.join(&item.name))
                .ok_or_else(|| "move: 无选中条目".to_owned())?;
            let op =
                ops::move_to(state.active, &path, &target).map_err(|e| format!("move: {e}"))?;
            state.undo_stack.push(op);
            let cfg = state.config;
            if let Some(tab) = state.tabs.get_mut(state.active) {
                let _ = tab.refresh(cfg);
            }
            Ok(ExecOutcome::Done { msg: None })
        }
        Some("copy") => {
            // M2-4b（SPEC §5 M2 MUST）：复制选中条目到目标目录（保留原名）；
            // undo = 删除生成的副本；`p`（PutCopy）键位在 M10 接入。
            let dir_arg = parts
                .next()
                .ok_or_else(|| "copy: 缺少目标目录".to_owned())?;
            let target = PathBuf::from(dir_arg);
            let base = physical_base(state)?;
            let tab = state.active_tab();
            let path = tab
                .entries
                .get(tab.cursor)
                .map(|item| base.join(&item.name))
                .ok_or_else(|| "copy: 无选中条目".to_owned())?;
            let op =
                ops::copy_to(state.active, &path, &target).map_err(|e| format!("copy: {e}"))?;
            state.undo_stack.push(op);
            let cfg = state.config;
            if let Some(tab) = state.tabs.get_mut(state.active) {
                let _ = tab.refresh(cfg);
            }
            Ok(ExecOutcome::Done { msg: None })
        }
        Some("tag") => {
            // M3-3：对标记集合执行批量复制/移动/删除。
            // 逐文件入撤销栈（LIFO 逆序还原）；已消失的标记条目跳过。
            let action = parts
                .next()
                .ok_or_else(|| "tag: 缺少动作（delete / copy|move <目录>）".to_owned())?;
            match action {
                "delete" => {
                    if state.active_tab().tagged_count() == 0 {
                        Ok(ExecOutcome::Done {
                            msg: Some("tag: 无标记文件".to_owned()),
                        })
                    } else {
                        Ok(ExecOutcome::ConfirmTagDelete)
                    }
                }
                "copy" | "move" => {
                    let dir_arg = parts
                        .next()
                        .ok_or_else(|| format!("tag {action}: 缺少目标目录"))?;
                    let target = PathBuf::from(dir_arg);
                    let base = physical_base(state)?;
                    let names: Vec<String> = state.active_tab().tags.iter().cloned().collect();
                    let mut done = 0;
                    let mut skipped = 0;
                    let mut first_err = None;
                    for name in &names {
                        let path = base.join(name);
                        if !path.exists() {
                            skipped += 1;
                            continue;
                        }
                        let result = if action == "copy" {
                            ops::copy_to(state.active, &path, &target)
                        } else {
                            ops::move_to(state.active, &path, &target)
                        };
                        match result {
                            Ok(op) => {
                                state.undo_stack.push(op);
                                done += 1;
                            }
                            Err(e) => {
                                if first_err.is_none() {
                                    first_err = Some(format!("{action} {name}: {e}"));
                                }
                            }
                        }
                    }
                    let cfg = state.config;
                    if let Some(tab) = state.tabs.get_mut(state.active) {
                        let _ = tab.refresh(cfg);
                        // M3-4：批量操作完成后清空标记状态。
                        tab.clear_tags();
                    }
                    let mut msg = format!("tag {action}: {done} 完成");
                    if skipped > 0 {
                        msg.push_str(&format!(", {skipped} 跳过"));
                    }
                    if let Some(e) = first_err {
                        msg.push_str(&format!("; 错误: {e}"));
                    }
                    Ok(ExecOutcome::Done { msg: Some(msg) })
                }
                other => Err(format!(
                    "tag: 未知动作 {other}（delete / copy|move <目录>）"
                )),
            }
        }
        Some(cmd @ ("mkdir" | "touch")) => {
            // M2-5：在当前目录新建目录/空文件。
            let name = parts.next().ok_or_else(|| format!("{cmd}: 缺少名字"))?;
            let base = physical_base(state)?;
            let result = if cmd == "mkdir" {
                ops::create_dir(state.active, &base, name)
            } else {
                ops::create_file(state.active, &base, name)
            };
            let op = result.map_err(|e| format!("{cmd}: {e}"))?;
            state.undo_stack.push(op);
            let cfg = state.config;
            if let Some(tab) = state.tabs.get_mut(state.active) {
                let _ = tab.refresh(cfg);
            }
            Ok(ExecOutcome::Done { msg: None })
        }
        Some(other) => Err(format!("未知命令: {other}（完整命令解析在 M5-4 接入）")),
    }
}

/// `n`/`N`：沿当前方向移动到下一个匹配；未设置搜索词时提示（M1-7）。
fn run_search(state: &mut AppState, msg: &mut Option<String>, forward: bool) {
    let Some(query) = state.active_tab().search.as_ref().map(|s| s.query.clone()) else {
        *msg = Some("未设置搜索词（先用 / 搜索）".to_owned());
        return;
    };
    if state.active_tab_mut().search_move(&query, forward) {
        *msg = None;
    } else {
        *msg = Some("无匹配".to_owned());
    }
}

/// 提交搜索词（M1-7）：设置搜索状态并移动到首个匹配（从光标下一位环绕）。
fn submit_search(state: &mut AppState, line: &str, msg: &mut Option<String>) {
    let query = line.trim().to_owned();
    if query.is_empty() {
        *msg = None;
        return;
    }
    let tab = state.active_tab_mut();
    let start = tab.cursor + 1;
    match tab.find_match(&query, start, true) {
        Some(idx) => {
            tab.cursor = idx;
            tab.search = Some(SearchState { query });
            *msg = None;
        }
        None => {
            tab.search = Some(SearchState { query });
            *msg = Some("无匹配".to_owned());
        }
    }
}

/// 分发 Normal 模式按键。
///
/// `viewport_h` 为列表区高度（行数），用于 `Ctrl-u`/`Ctrl-d` 的半页步进。
/// 已接入：`j/k`、`l`/`Enter`、`h`、`gg`、`G`、`Ctrl-u/d`、`gt`/`gT`/`N gt`、
/// `/`、`n`/`N`、`dd`（删除确认）、`u`（撤销）、`v`（可视选择）、Esc（退出选择）、
/// `t`（tag 标记）、`q`/`Ctrl-C`、`:`。
fn handle_key(
    state: &mut AppState,
    input: &mut InputState,
    msg: &mut Option<String>,
    key: KeyEvent,
    viewport_h: u16,
) -> Outcome {
    if key.kind != KeyEventKind::Press {
        return Outcome::Continue; // Release 先过滤，避免误消费待决序列。
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
        input.seq.push(crate::keymap::Key::plain(key.code));
        let count = input.count.take();
        input.count = None;
        match input.keymap.lookup(&input.seq) {
            Some(Action::ToTop) => state.active_tab_mut().cursor_top(),
            Some(Action::NextTab) => match count {
                Some(n) => state.goto_tab(n),
                None => state.next_tab(),
            },
            Some(Action::PrevTab) => match count {
                Some(n) => state.goto_tab(n),
                None => state.prev_tab(),
            },
            Some(Action::DeleteSelected) => {
                let tab = state.active_tab();
                if let (Location::Physical(base), Some(item)) =
                    (&tab.location, tab.entries.get(tab.cursor))
                {
                    return Outcome::ConfirmDelete(base.join(&item.name));
                }
            }
            _ => {} // 未知序列整体丢弃
        }
        input.seq.clear();
        return Outcome::Continue;
    }

    if is_quit_key(key) {
        return Outcome::Quit;
    }

    // Esc：退出可视选择（M3-1）；非可视模式忽略。
    if key.code == KeyCode::Esc {
        input.count = None;
        state.active_tab_mut().exit_visual();
        return Outcome::Continue;
    }

    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let half = (viewport_h.saturating_sub(1) / 2).max(1) as isize;
    match key.code {
        KeyCode::Char('G') => state.active_tab_mut().cursor_bottom(),
        KeyCode::Char('d') if ctrl => state.active_tab_mut().move_cursor(half),
        KeyCode::Char('u') if ctrl => state.active_tab_mut().move_cursor(-half),
        KeyCode::Char('u') => {
            input.count = None;
            undo_last(state, msg);
        }
        KeyCode::Char('j') => {
            input.count = None;
            state.active_tab_mut().move_cursor(1);
        }
        KeyCode::Char('k') => {
            input.count = None;
            state.active_tab_mut().move_cursor(-1);
        }
        KeyCode::Char('l') | KeyCode::Enter => {
            input.count = None;
            let cfg = state.config;
            // M11-7：目录/归档 → enter_selected 导航；普通文件 → 系统 opener。
            let tab = state.active_tab();
            let is_file = tab
                .entries
                .get(tab.cursor)
                .map(|item| !item.is_dir)
                .unwrap_or(false);
            let is_physical = matches!(tab.location, crate::tab::Location::Physical(_));
            if is_file && is_physical {
                let path = tab.location.base_dir().join(&tab.entries[tab.cursor].name);
                match crate::opener::system_open(&path) {
                    Ok(()) => *msg = None,
                    Err(e) => *msg = Some(format!("打开失败: {e}")),
                }
                return Outcome::Continue;
            }
            if let Err(e) = state.active_tab_mut().enter_selected(cfg) {
                *msg = Some(format!("进入失败: {e}"));
            }
        }
        KeyCode::Char('h') => {
            input.count = None;
            let cfg = state.config;
            if let Err(e) = state.active_tab_mut().return_parent(cfg) {
                *msg = Some(format!("返回失败: {e}"));
            }
        }
        KeyCode::Char('/') => {
            input.count = None;
            return Outcome::EnterSearch;
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
        // M10-3：`g`/`d`/`v`/`t` 前缀检测经 Keymap；单键动作经 dispatch。
        KeyCode::Char('g') | KeyCode::Char('d') => {
            let k = crate::keymap::Key::plain(key.code);
            if input.keymap.has_prefix(&[k]) {
                input.seq.push(k);
            }
        }
        KeyCode::Char('v') => {
            input.count = None;
            state.active_tab_mut().toggle_visual();
        }
        KeyCode::Char('t') => {
            input.count = None;
            state.active_tab_mut().toggle_tag();
        }
        KeyCode::Char('p') | KeyCode::Char('P') => {
            // M10-1：PutCopy/PutMove 已入 Keymap；yy 缓冲区在 M10-7 接入。
            input.count = None;
        }
        _ => input.count = None,
    }
    Outcome::Continue
}

/// 执行确认后的删除（M2-2）：移入应用内暂存、记录撤销栈、刷新所在 tab。
fn execute_delete(state: &mut AppState, msg: &mut Option<String>, path: PathBuf) {
    let tab_id = state.active;
    let cfg = state.config;
    match ops::delete_to_trash(tab_id, &path) {
        Ok(op) => {
            state.undo_stack.push(op);
            *msg = None;
            if let Some(tab) = state.tabs.get_mut(tab_id)
                && let Err(e) = tab.refresh(cfg)
            {
                *msg = Some(format!("刷新失败: {e}"));
            }
        }
        Err(e) => *msg = Some(format!("删除失败: {e}")),
    }
}

/// 执行确认后的标记批量删除（M3-3）：逐文件入暂存并各生成撤销记录；
/// 已消失的标记条目跳过。归档视图只读（M7 边界）。
fn execute_tag_delete(state: &mut AppState, msg: &mut Option<String>) {
    let Ok(base) = physical_base(state) else {
        *msg = Some("归档视图只读（解压后可修改）".to_owned());
        return;
    };
    let names: Vec<String> = state.active_tab().tags.iter().cloned().collect();
    let mut done = 0;
    let mut first_err = None;
    for name in &names {
        let path = base.join(name);
        if !path.exists() {
            continue; // 标记后条目已消失：跳过
        }
        match ops::delete_to_trash(state.active, &path) {
            Ok(op) => {
                state.undo_stack.push(op);
                done += 1;
            }
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some(format!("删除 {name}: {e}"));
                }
            }
        }
    }
    let cfg = state.config;
    if let Some(tab) = state.tabs.get_mut(state.active) {
        let _ = tab.refresh(cfg);
        // M3-4：批量删除完成后清空标记状态。
        tab.clear_tags();
    }
    let mut m = format!("tag delete: {done} 完成");
    if let Some(e) = first_err {
        m.push_str(&format!("; 错误: {e}"));
    }
    *msg = Some(m);
}

/// `u` 撤销（M2-2）：弹出最近操作并执行逆操作；失败回退入栈保留可重试。
fn undo_last(state: &mut AppState, msg: &mut Option<String>) {
    let Some(op) = state.undo_stack.pop() else {
        *msg = Some("无可撤销操作".to_owned());
        return;
    };
    match ops::undo_op(&op) {
        Ok(()) => {
            // 撤销成功给出反馈（M2-6）。
            *msg = Some(format!("已撤销: {}", op.describe()));
            // 刷新受影响 tab（M2-7：跨 tab 撤销刷新原 tab；tab 已关闭则跳过，
            // 磁盘恢复不受影响；设计取向为"当前 tab 不变"，ACCEPTANCE §4）。
            let cfg = state.config;
            if let Some(tab) = state.tabs.get_mut(op.tab_id())
                && let Err(e) = tab.refresh(cfg)
            {
                *msg = Some(format!("刷新失败: {e}"));
            }
        }
        Err(e) => {
            state.undo_stack.push(op);
            *msg = Some(format!("撤销失败: {e}"));
        }
    }
}
/// 事件循环：进入时终端已由 `ratatui::init` 处于 raw mode + 备用屏。
pub fn run(mut terminal: ratatui::DefaultTerminal) -> std::io::Result<()> {
    // M5-1：启动加载用户配置；缺失用默认值，语法错误在状态栏提示（不退出）。
    let mut msg: Option<String> = None;
    let config = match config::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            msg = Some(e);
            config::Config::default()
        }
    };
    let mut state = AppState::new(Tab::from_physical(&std::env::current_dir()?, config)?);
    state.config = config;
    let mut input = InputState::default();
    let mut mode = InputMode::default();
    loop {
        terminal.draw(|frame| render(frame, &state, &mode, msg.as_deref()))?;
        // 列表区高度 = 终端高度 - 状态栏 1 行。
        let viewport_h = terminal.size()?.height.saturating_sub(1);
        if let Event::Key(key) = event::read()? {
            match &mut mode {
                InputMode::Command { input } => match command_input(input, key) {
                    CmdInput::Continue => {}
                    CmdInput::Cancel => mode = InputMode::Normal,
                    CmdInput::Submit => {
                        let line = std::mem::take(input);
                        mode = InputMode::Normal;
                        match exec_command_impl(&mut state, line.trim()) {
                            Ok(ExecOutcome::Quit) => return Ok(()),
                            Ok(ExecOutcome::EnterHelp) => mode = InputMode::Help,
                            Ok(ExecOutcome::Done { msg: m }) => msg = m,
                            Ok(ExecOutcome::ConfirmTagDelete) => {
                                mode = InputMode::ConfirmTagDelete;
                            }
                            Err(m) => msg = Some(m),
                        }
                    }
                },
                InputMode::Search { input } => match command_input(input, key) {
                    CmdInput::Continue => {}
                    CmdInput::Cancel => mode = InputMode::Normal,
                    CmdInput::Submit => {
                        let line = std::mem::take(input);
                        mode = InputMode::Normal;
                        submit_search(&mut state, &line, &mut msg);
                    }
                },
                InputMode::Filter { input } => match command_input(input, key) {
                    CmdInput::Continue => {}
                    CmdInput::Cancel => mode = InputMode::Normal,
                    CmdInput::Submit => {
                        let line = std::mem::take(input);
                        mode = InputMode::Normal;
                        let cfg = state.config;
                        state.active_tab_mut().apply_filter(&line, cfg);
                        msg = None;
                    }
                },
                InputMode::Confirm { path } => {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            // y / Enter 确认删除；n / Esc / 其他 = 取消（无副作用）。
                            KeyCode::Char('y') | KeyCode::Enter => {
                                let path = std::mem::take(path);
                                mode = InputMode::Normal;
                                execute_delete(&mut state, &mut msg, path);
                            }
                            _ => mode = InputMode::Normal,
                        }
                    }
                }
                InputMode::Help => {
                    // 任意键返回（M5-3 最小帮助覆盖层；完整帮助页在 M6-2）。
                    mode = InputMode::Normal;
                }
                InputMode::ConfirmTagDelete => {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            // y / Enter 确认批量删除；n / Esc / 其他 = 取消（无副作用）。
                            KeyCode::Char('y') | KeyCode::Enter => {
                                mode = InputMode::Normal;
                                execute_tag_delete(&mut state, &mut msg);
                            }
                            _ => mode = InputMode::Normal,
                        }
                    }
                }
                InputMode::Normal => {
                    match handle_key(&mut state, &mut input, &mut msg, key, viewport_h) {
                        Outcome::Continue => {}
                        Outcome::Quit => return Ok(()),
                        Outcome::EnterCommand => {
                            mode = InputMode::Command {
                                input: String::new(),
                            };
                        }
                        Outcome::EnterSearch => {
                            mode = InputMode::Search {
                                input: String::new(),
                            };
                        }
                        Outcome::EnterFilter => {
                            mode = InputMode::Filter {
                                input: String::new(),
                            };
                        }
                        Outcome::ConfirmDelete(path) => {
                            mode = InputMode::Confirm { path };
                        }
                    }
                }
            }
        }
    }
}

fn render(frame: &mut Frame, state: &AppState, mode: &InputMode, msg: Option<&str>) {
    let tab = state.active_tab();
    let [main_area, status_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(frame.area());
    // M11-2：vifm 式双栏——左列表、右预览（SPEC SIMPLIFIED 允许单栏，
    // 统一预览窗格自 M11-2 起落地）。
    let [list_area, preview_area] =
        Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)])
            .areas(main_area);

    let visual_range = tab.visual_range();
    let list_items: Vec<ListItem> = tab
        .entries
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let label = if item.is_dir {
                format!("{}/", item.name)
            } else {
                item.name.clone()
            };
            // M9-1：行首渲染 devicons 类型图标。
            let label = format!("{} {label}", item.icon.glyph);
            // M9-2：图标颜色经 opaline 样式染色（命名样式缺失时回落默认）。
            let base_style = crate::theme::style("file_icon");
            // 可视选择范围内的行反显（M3-1）；tag 标记行加粗（M3-2）；
            // 光标行由 highlight_style 叠加。
            let mut li = ListItem::new(Line::from(label)).style(base_style);
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
    let mut list_state = ListState::default().with_selected(Some(tab.cursor));
    frame.render_stateful_widget(
        List::new(list_items).highlight_style(Style::new().add_modifier(Modifier::REVERSED)),
        list_area,
        &mut list_state,
    );

    // M11-2：预览窗格——光标项按 `preview_kind` 分流；图片走
    // ImageEngine（解码失败降级为提示行），其余走共享 PreviewRenderer。
    render_preview_pane(frame, state, preview_area);

    if matches!(mode, InputMode::Help) {
        // 帮助覆盖层（M5-3）。
        let text = "rvifm 按键（任意键返回）\n\
            \n  j/k 移动    l/Enter 进入    h 返回\n\
            gg/G 首/末  Ctrl-u/d 半页\n\
            / 搜索    n/N 下/上一匹配    = 过滤\n\
            v 可视选择    t 标记    :tag delete|copy|move <目录>\n\
            dd 删除    u 撤销\n\
            :rename/:move/:copy/:mkdir/:touch/:cd/:q/:help\n\
            :tabnew/:tabclose/:tabmove    gt/gT/N gt 切换标签";
        frame.render_widget(
            Paragraph::new(text).block(ratatui::widgets::Block::bordered().title("帮助")),
            frame.area(),
        );
        return;
    }
    let status = match mode {
        InputMode::Command { input } => Line::from(format!(":{input}")),
        InputMode::Search { input } => Line::from(format!("/{input}")),
        InputMode::Filter { input } => Line::from(format!("={input}")),
        InputMode::Help => Line::from(String::new()), // 帮助覆盖层提前 return，不可达
        InputMode::Confirm { path } => Line::from(format!("删除 {}? [y/N]", path.display())),
        InputMode::ConfirmTagDelete => {
            let n = tab.tagged_count();
            Line::from(format!("删除 {n} 个标记文件? [y/N]"))
        }
        InputMode::Normal => {
            let selected = match tab.entries.get(tab.cursor) {
                Some(item) => {
                    let size = if item.is_dir {
                        String::new()
                    } else {
                        format!(" {}", crate::tab::format_size(item.size))
                    };
                    format!(" | {}{} {}", item.name, size, item.perm)
                }
                None => String::new(),
            };
            let mut spans = vec![
                Span::styled(
                    tab.location.display_path(),
                    Style::new().add_modifier(Modifier::BOLD),
                ),
                Span::from(selected),
                Span::from(format!(" | {} items", tab.entries.len())),
            ];
            if let Some(s) = &tab.search {
                spans.push(Span::from(format!("  /{}", s.query)));
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
    frame.render_widget(Paragraph::new(status), status_area);
}

/// 预览窗格（M11-2）：与 `pick` 共用同一分流与渲染入口。
/// 归档视图条目暂不预览（M7 只读 VFS 的文本读取未接入预览）。
fn render_preview_pane(frame: &mut Frame, state: &AppState, area: ratatui::layout::Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let pane = ratatui::widgets::Block::bordered().title("预览");
    let inner = pane.inner(area);
    frame.render_widget(pane, area);
    let path = match physical_base(state) {
        Ok(base) => base.join(
            state
                .active_tab()
                .entries
                .get(state.active_tab().cursor)
                .map(|i| i.name.as_str())
                .unwrap_or_default(),
        ),
        Err(_) => {
            frame.render_widget(Paragraph::new("归档内预览暂不支持"), inner);
            return;
        }
    };
    match crate::preview::preview_kind(&path) {
        crate::preview::PreviewKind::Image => {
            // 图片渲染失败（损坏/超大）→ 降级为共享提示行。
            if state.image_engine.render_file(frame, inner, &path).is_err() {
                let p = crate::preview::render_preview(&path, inner.height as usize);
                frame.render_widget(p.into_paragraph(), inner);
            }
        }
        _ => {
            let p = crate::preview::render_preview(&path, inner.height as usize);
            frame.render_widget(p.into_paragraph(), inner);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::keymap::Key;
    use crate::tab::{FileItem, Location};
    use std::collections::HashSet;
    use std::fs;
    use std::path::Path;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// 测试辅助：viewport 高度 10（半页步进 4）；丢弃提示消息。
    fn press(
        state: &mut AppState,
        input: &mut InputState,
        code: KeyCode,
        mods: KeyModifiers,
    ) -> Outcome {
        let mut msg = None;
        press_msg(state, input, &mut msg, code, mods)
    }

    /// 测试辅助：保留提示消息（搜索相关测试用）。
    fn press_msg(
        state: &mut AppState,
        input: &mut InputState,
        msg: &mut Option<String>,
        code: KeyCode,
        mods: KeyModifiers,
    ) -> Outcome {
        handle_key(state, input, msg, KeyEvent::new(code, mods), 10)
    }

    fn make_state(n: usize) -> (TempDir, AppState) {
        let tmp = TempDir::new().unwrap();
        for i in 0..n {
            fs::write(tmp.path().join(format!("f{i:02}")), "").unwrap();
        }
        let state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        (tmp, state)
    }

    fn tab_at(path: &str) -> Tab {
        Tab {
            location: Location::Physical(PathBuf::from(path)),
            entries: Vec::new(),
            cursor: 0,
            scroll: 0,
            search: None,
            history: Vec::new(),
            visual: None,
            filter: None,
            tags: HashSet::new(),
        }
    }

    #[test]
    fn q_and_ctrl_c_quit() {
        assert!(is_quit_key(KeyEvent::new(
            KeyCode::Char('q'),
            KeyModifiers::NONE
        )));
        assert!(is_quit_key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL
        )));
    }

    #[test]
    fn other_keys_do_not_quit() {
        assert!(!is_quit_key(KeyEvent::new(
            KeyCode::Char('x'),
            KeyModifiers::NONE
        )));
        assert!(!is_quit_key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::NONE
        )));
        // Windows 会额外发送 Release 事件，必须忽略。
        let release = KeyEvent {
            kind: KeyEventKind::Release,
            ..KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)
        };
        assert!(!is_quit_key(release));
    }

    #[test]
    fn j_k_move_cursor() {
        let (_tmp, mut state) = make_state(3);
        let mut input = InputState::default();
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('j'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(state.active_tab().cursor, 1);
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('k'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(state.active_tab().cursor, 0);
    }

    #[test]
    fn gg_and_g_jump_top_and_bottom() {
        let (_tmp, mut state) = make_state(5);
        let mut input = InputState::default();

        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('G'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(state.active_tab().cursor, 4);

        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('g'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(input.seq, [Key::plain(KeyCode::Char('g'))]); // 等待第二键
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('g'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert!(input.seq.is_empty()); // 序列已清空
        assert_eq!(state.active_tab().cursor, 0);
    }

    #[test]
    fn unknown_g_sequence_is_discarded() {
        let (_tmp, mut state) = make_state(3);
        let mut input = InputState::default();
        // `g` 后跟未知第二键：序列丢弃，不产生移动，也不退出。
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('g'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('x'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert!(input.seq.is_empty()); // 序列已清空
        assert_eq!(state.active_tab().cursor, 0);
        // 序列内的 `q` 被当作第二键丢弃，之后的 `q` 才退出。
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('g'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('q'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('q'),
                KeyModifiers::NONE
            ),
            Outcome::Quit
        );
    }

    #[test]
    fn ctrl_u_d_half_page() {
        let (_tmp, mut state) = make_state(20);
        let mut input = InputState::default();

        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('d'),
                KeyModifiers::CONTROL
            ),
            Outcome::Continue
        );
        assert_eq!(state.active_tab().cursor, 4); // (10-1)/2 = 4
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('d'),
                KeyModifiers::CONTROL
            ),
            Outcome::Continue
        );
        assert_eq!(state.active_tab().cursor, 8);
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('u'),
                KeyModifiers::CONTROL
            ),
            Outcome::Continue
        );
        assert_eq!(state.active_tab().cursor, 4);
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('u'),
                KeyModifiers::CONTROL
            ),
            Outcome::Continue
        );
        assert_eq!(state.active_tab().cursor, 0);
        // 顶部继续 Ctrl-u：钳制不越界。
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('u'),
                KeyModifiers::CONTROL
            ),
            Outcome::Continue
        );
        assert_eq!(state.active_tab().cursor, 0);
    }

    #[test]
    fn gt_gt_and_counted_gt() {
        let mut state = AppState::new(tab_at("C:/a"));
        state.push_tab(tab_at("C:/b"));
        state.push_tab(tab_at("C:/c"));
        state.active = 0; // push_tab 会切到新 tab，这里归位到第一个
        let mut input = InputState::default();

        // gt 循环：a → b → c → a。
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('g'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('t'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(state.active, 1);
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('g'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('t'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(state.active, 2);
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('g'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('t'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(state.active, 0);

        // gT 反向循环：a → c。
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('g'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('T'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(state.active, 2);

        // `2gt` 跳到第 2 个 tab（1 起始）。
        for code in [KeyCode::Char('2'), KeyCode::Char('g'), KeyCode::Char('t')] {
            press(&mut state, &mut input, code, KeyModifiers::NONE);
        }
        assert_eq!(state.active, 1);
        assert_eq!(input.count, None);
    }

    #[test]
    fn count_is_dropped_by_unrelated_keys() {
        let (_tmp, mut state) = make_state(3);
        let mut input = InputState::default();
        press(
            &mut state,
            &mut input,
            KeyCode::Char('3'),
            KeyModifiers::NONE,
        );
        assert_eq!(input.count, Some(3));
        press(
            &mut state,
            &mut input,
            KeyCode::Char('j'),
            KeyModifiers::NONE,
        );
        assert_eq!(input.count, None); // 计数被动作键丢弃
        assert_eq!(state.active_tab().cursor, 1); // j 只移动 1 行
    }

    #[test]
    fn colon_enters_command_mode() {
        let (_tmp, mut state) = make_state(1);
        let mut input = InputState::default();
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char(':'),
                KeyModifiers::NONE
            ),
            Outcome::EnterCommand
        );
    }

    #[test]
    fn command_input_typing_and_submit() {
        let mut buf = String::new();
        for c in "tabnew".chars() {
            assert_eq!(
                command_input(
                    &mut buf,
                    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
                ),
                CmdInput::Continue
            );
        }
        assert_eq!(buf, "tabnew");
        // Backspace 删除。
        assert_eq!(
            command_input(
                &mut buf,
                KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)
            ),
            CmdInput::Continue
        );
        assert_eq!(buf, "tabne");
        // Esc 取消。
        assert_eq!(
            command_input(&mut buf, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            CmdInput::Cancel
        );
        // Enter 提交。
        assert_eq!(
            command_input(&mut buf, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            CmdInput::Submit
        );
    }

    #[test]
    fn exec_tab_commands() {
        let (_tmp, mut state) = make_state(1);
        state.active_tab_mut().cursor = 0;

        assert_eq!(exec_command(&mut state, "tabnew"), Ok(false));
        assert_eq!(state.tabs.len(), 2);
        assert_eq!(state.active, 1);
        // 新 tab 与原 tab 同目录。
        assert_eq!(state.tabs[1].location, state.tabs[0].location);

        assert_eq!(exec_command(&mut state, "tabmove 1"), Ok(false));
        assert_eq!(state.active, 0); // 移动后 active 跟随被移动的 tab
        assert_eq!(state.tabs[0].location, state.tabs[1].location);

        assert_eq!(
            exec_command(&mut state, "tabmove x"),
            Err("tabmove: 位置必须是数字".to_owned())
        );
        assert_eq!(
            exec_command(&mut state, "nosuchcmd"),
            Err("未知命令: nosuchcmd（完整命令解析在 M5-4 接入）".to_owned())
        );

        // 关闭中间 tab：剩余 tab active 正确收敛。
        assert_eq!(exec_command(&mut state, "tabclose"), Ok(false));
        assert_eq!(state.tabs.len(), 1);
        assert_eq!(state.active, 0);
        // 关闭最后一个 tab → 退出。
        assert_eq!(exec_command(&mut state, "tabclose"), Ok(true));
    }

    #[test]
    fn search_submit_moves_to_first_match() {
        let tmp = TempDir::new().unwrap();
        for name in ["alpha", "beta", "Beta2", "gamma"] {
            fs::write(tmp.path().join(name), "").unwrap();
        }
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut msg = None;

        submit_search(&mut state, "beta", &mut msg);
        assert!(msg.is_none());
        assert_eq!(state.active_tab().cursor, 1); // 从 0 向后命中 beta
        assert_eq!(state.active_tab().search.as_ref().unwrap().query, "beta");
    }

    #[test]
    fn search_n_and_n_wrap_case_insensitive() {
        let tmp = TempDir::new().unwrap();
        for name in ["alpha", "beta", "Beta2", "gamma"] {
            fs::write(tmp.path().join(name), "").unwrap();
        }
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut input = InputState::default();
        let mut msg = None;

        submit_search(&mut state, "beta", &mut msg); // cursor → 1

        // n → 下一个匹配 Beta2（大小写不敏感）。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('n'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().cursor, 2);
        // n 环绕 → 回到 beta。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('n'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().cursor, 1);
        // N → 反向：回到 Beta2。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('N'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().cursor, 2);
        assert!(msg.is_none());
    }

    #[test]
    fn search_without_query_prompts_and_no_match_reports() {
        let (_tmp, mut state) = make_state(2);
        let mut input = InputState::default();
        let mut msg = None;

        // 未设置搜索词。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('n'),
            KeyModifiers::NONE,
        );
        assert_eq!(msg.as_deref(), Some("未设置搜索词（先用 / 搜索）"));

        // 设置后搜索不存在的词：无匹配提示，cursor 不变。
        submit_search(&mut state, "zzz", &mut msg);
        assert_eq!(msg.as_deref(), Some("无匹配"));
        assert_eq!(state.active_tab().cursor, 0);
        // 再次 n：无匹配提示保持。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('n'),
            KeyModifiers::NONE,
        );
        assert_eq!(msg.as_deref(), Some("无匹配"));
    }

    #[test]
    fn slash_enters_search_mode() {
        let (_tmp, mut state) = make_state(1);
        let mut input = InputState::default();
        let mut msg = None;
        assert_eq!(
            press_msg(
                &mut state,
                &mut input,
                &mut msg,
                KeyCode::Char('/'),
                KeyModifiers::NONE
            ),
            Outcome::EnterSearch
        );
    }

    #[test]
    fn dd_requests_confirm_and_discards_other_second_key() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("target.txt"), "T").unwrap();
        fs::write(tmp.path().join("keep.txt"), "K").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut input = InputState::default();

        // 排序后 keep.txt(0) / target.txt(1)，把光标放到 target.txt。
        state.active_tab_mut().cursor = 1;
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('d'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(input.seq, [Key::plain(KeyCode::Char('d'))]);
        let outcome = press(
            &mut state,
            &mut input,
            KeyCode::Char('d'),
            KeyModifiers::NONE,
        );
        match outcome {
            Outcome::ConfirmDelete(p) => assert_eq!(p, tmp.path().join("target.txt")),
            other => panic!("expected ConfirmDelete, got {other:?}"),
        }

        // `d` 后跟其他键：序列整体丢弃，不产生确认请求。
        let mut input2 = InputState::default();
        assert_eq!(
            press(
                &mut state,
                &mut input2,
                KeyCode::Char('d'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(
            press(
                &mut state,
                &mut input2,
                KeyCode::Char('x'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert!(input2.seq.is_empty());
        assert!(state.undo_stack.is_empty());
    }

    #[test]
    fn execute_delete_then_undo_restores() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("gone.txt");
        fs::write(&file, "G").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut msg = None;

        execute_delete(&mut state, &mut msg, file.clone());
        assert!(msg.is_none());
        assert!(!file.exists(), "file should be staged out of the dir");
        assert_eq!(state.undo_stack.len(), 1);

        undo_last(&mut state, &mut msg);
        assert!(
            msg.as_deref().unwrap().starts_with("已撤销: "),
            "msg: {msg:?}"
        );
        assert!(file.exists());
        assert_eq!(fs::read_to_string(&file).unwrap(), "G");
        assert!(state.undo_stack.is_empty());

        // 空栈再按 u：明确提示，无 panic。
        undo_last(&mut state, &mut msg);
        assert_eq!(msg.as_deref(), Some("无可撤销操作"));
    }

    #[test]
    fn exec_rename_and_undo() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("old.txt"), "O").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut msg = None;

        // 重命名选中条目（cursor 0 = old.txt）。
        assert_eq!(exec_command(&mut state, "rename new.txt"), Ok(false));
        assert!(!tmp.path().join("old.txt").exists());
        assert!(tmp.path().join("new.txt").exists());
        assert_eq!(state.undo_stack.len(), 1);
        // 列表已刷新。
        assert!(
            state
                .active_tab()
                .entries
                .iter()
                .all(|i| i.name != "old.txt")
        );

        // `u` 撤销 → 回到 old.txt。
        undo_last(&mut state, &mut msg);
        assert!(
            msg.as_deref().unwrap().starts_with("已撤销: "),
            "msg: {msg:?}"
        );
        assert!(tmp.path().join("old.txt").exists());
        assert!(!tmp.path().join("new.txt").exists());

        // 错误路径：缺参数、目标已存在（均有提示，不 panic）。
        assert_eq!(
            exec_command(&mut state, "rename"),
            Err("rename: 缺少新名字".to_owned())
        );
        let err = exec_command(&mut state, "rename old.txt").unwrap_err();
        assert!(err.starts_with("rename: 目标已存在"), "err: {err}");
    }

    #[test]
    fn exec_move_and_undo() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(tmp.path().join("m.txt"), "M").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut msg = None;

        // 排序后 sub/(0) 在前，m.txt 在 1：把光标放到 m.txt。
        state.active_tab_mut().cursor = 1;
        let target = sub.display().to_string();
        assert_eq!(
            exec_command(&mut state, &format!("move {target}")),
            Ok(false)
        );
        assert!(!tmp.path().join("m.txt").exists());
        assert!(sub.join("m.txt").exists());
        assert_eq!(state.undo_stack.len(), 1);

        // `u` 撤销 → 移回原目录。
        undo_last(&mut state, &mut msg);
        assert!(
            msg.as_deref().unwrap().starts_with("已撤销: "),
            "msg: {msg:?}"
        );
        assert!(tmp.path().join("m.txt").exists());
        assert!(!sub.join("m.txt").exists());

        // 错误路径：缺参数、目录不存在。
        assert_eq!(
            exec_command(&mut state, "move"),
            Err("move: 缺少目标目录".to_owned())
        );
        let err = exec_command(&mut state, "move Z:/nope").unwrap_err();
        assert!(err.starts_with("move: 目标目录不存在"), "err: {err}");

        // 防自嵌套：刷新后只剩 sub/（cursor 钳制到 0），把目录移入自身内部被拒绝。
        state.active_tab_mut().refresh(Config::default()).unwrap();
        let err = exec_command(&mut state, &format!("move {target}")).unwrap_err();
        assert!(err.contains("不能把目录移动到其自身内部"), "err: {err}");
    }

    #[test]
    fn exec_mkdir_touch_and_undo() {
        let tmp = TempDir::new().unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut msg = None;

        // mkdir → 目录出现并入栈；u 移除。
        assert_eq!(exec_command(&mut state, "mkdir newdir"), Ok(false));
        assert!(tmp.path().join("newdir").is_dir());
        undo_last(&mut state, &mut msg);
        assert!(
            msg.as_deref().unwrap().starts_with("已撤销: "),
            "msg: {msg:?}"
        );
        assert!(!tmp.path().join("newdir").exists());

        // touch → 文件出现并入栈；u 移除。
        assert_eq!(exec_command(&mut state, "touch t.txt"), Ok(false));
        assert!(tmp.path().join("t.txt").is_file());
        undo_last(&mut state, &mut msg);
        assert!(
            msg.as_deref().unwrap().starts_with("已撤销: "),
            "msg: {msg:?}"
        );
        assert!(!tmp.path().join("t.txt").exists());

        // 错误路径：缺名字、非法名字、已存在。
        assert_eq!(
            exec_command(&mut state, "mkdir"),
            Err("mkdir: 缺少名字".to_owned())
        );
        assert!(exec_command(&mut state, "mkdir a/b").is_err());
        assert_eq!(exec_command(&mut state, "touch t.txt"), Ok(false));
        let err = exec_command(&mut state, "touch t.txt").unwrap_err();
        assert!(err.starts_with("touch: 目标已存在"), "err: {err}");
    }

    #[test]
    fn consecutive_ops_undo_in_reverse() {
        // M2-6：mkdir → touch → rename 连续三步，三次 `u` 逆序还原。
        let tmp = TempDir::new().unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut msg = None;

        assert_eq!(exec_command(&mut state, "mkdir d"), Ok(false));
        assert_eq!(exec_command(&mut state, "touch t.txt"), Ok(false));
        // 排序后 d/ 在 0、t.txt 在 1：把光标放到 t.txt 再重命名。
        state.active_tab_mut().cursor = 1;
        assert_eq!(exec_command(&mut state, "rename d2.txt"), Ok(false)); // 重命名 t.txt

        assert!(tmp.path().join("d").is_dir());
        assert!(tmp.path().join("d2.txt").is_file());
        assert_eq!(state.undo_stack.len(), 3);

        // 第 1 次 u：撤销重命名 → d2.txt 回到 t.txt。
        undo_last(&mut state, &mut msg);
        assert!(msg.as_deref().unwrap().starts_with("已撤销: 重命名"));
        assert!(tmp.path().join("t.txt").is_file());
        assert!(!tmp.path().join("d2.txt").exists());

        // 第 2 次 u：撤销 touch → t.txt 移除。
        undo_last(&mut state, &mut msg);
        assert!(msg.as_deref().unwrap().starts_with("已撤销: 新建"));
        assert!(!tmp.path().join("t.txt").exists());

        // 第 3 次 u：撤销 mkdir → d 移除；栈空。
        undo_last(&mut state, &mut msg);
        assert!(msg.as_deref().unwrap().starts_with("已撤销: 新建"));
        assert!(!tmp.path().join("d").exists());
        assert!(state.undo_stack.is_empty());
    }

    #[test]
    fn tabclose_reports_irreversible() {
        let (_tmp, mut state) = make_state(1);
        state.push_tab(Tab::from_physical(&std::env::temp_dir(), Config::default()).unwrap());

        // 非最后一个 tab：关闭成功并提示不可撤销。
        match exec_command_impl(&mut state, "tabclose") {
            Ok(ExecOutcome::Done { msg }) => {
                assert_eq!(msg.as_deref(), Some("标签已关闭（不可撤销）"))
            }
            other => panic!("expected Done, got {other:?}"),
        }

        // 最后一个 tab：退出请求。
        assert_eq!(
            exec_command_impl(&mut state, "tabclose"),
            Ok(ExecOutcome::Quit)
        );
    }

    #[test]
    fn cross_tab_undo_restores_origin_and_keeps_current() {
        // M2-7：tab 1 删除 → 切到 tab 2 → `u` → tab 1 文件恢复、当前 tab 不变。
        let dir_a = TempDir::new().unwrap(); // tab 1（index 0）
        let dir_b = TempDir::new().unwrap(); // tab 2（index 1）
        let file = dir_a.path().join("x.txt");
        fs::write(&file, "X").unwrap();

        let mut state = AppState::new(Tab::from_physical(dir_a.path(), Config::default()).unwrap());
        state.push_tab(Tab::from_physical(dir_b.path(), Config::default()).unwrap());
        state.active = 0;

        // 在 tab 1 删除 x.txt（entries = [x.txt]，cursor 0）。
        let mut msg = None;
        execute_delete(&mut state, &mut msg, file.clone());
        assert!(!file.exists());
        assert_eq!(state.undo_stack.len(), 1);

        // 切到 tab 2。
        state.next_tab();
        assert_eq!(state.active, 1);

        // `u`：撤销发生在 tab 1 的删除。
        undo_last(&mut state, &mut msg);
        assert!(msg.as_deref().unwrap().starts_with("已撤销: 删除"));
        assert!(file.exists(), "tab 1 的文件应恢复");
        assert_eq!(state.active, 1, "当前 tab 不变");
        assert_eq!(
            state.active_tab().location,
            Location::Physical(dir_b.path().to_path_buf())
        );
        // tab 1 的列表恢复包含 x.txt。
        assert!(state.tabs[0].entries.iter().any(|i| i.name == "x.txt"));
    }

    #[test]
    fn exec_copy_and_undo_removes_copy() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(tmp.path().join("c.txt"), "C").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut msg = None;

        // 排序后 sub/(0) 在前，c.txt 在 1。
        state.active_tab_mut().cursor = 1;
        let target = sub.display().to_string();
        assert_eq!(
            exec_command(&mut state, &format!("copy {target}")),
            Ok(false)
        );
        // 源保留，副本出现，入栈。
        assert!(tmp.path().join("c.txt").exists());
        assert!(sub.join("c.txt").exists());
        assert_eq!(state.undo_stack.len(), 1);

        // `u` = 删除生成的副本（Create 语义），源不受影响。
        undo_last(&mut state, &mut msg);
        assert!(msg.as_deref().unwrap().starts_with("已撤销: 新建"));
        assert!(!sub.join("c.txt").exists());
        assert!(tmp.path().join("c.txt").exists());
    }

    #[test]
    fn visual_selection_flow() {
        // M3-1：v 进入 → j 扩大选区 → Esc 退出，状态正确。
        let (_tmp, mut state) = make_state(4);
        let mut input = InputState::default();

        // 空目录 no-op。
        let empty_tmp = TempDir::new().unwrap();
        let mut empty = Tab::from_physical(empty_tmp.path(), Config::default()).unwrap();
        empty.toggle_visual();
        assert!(empty.visual.is_none());

        // v：进入，锚点 = 进入时光标位置。
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('v'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(state.active_tab().visual, Some(0));
        assert_eq!(state.active_tab().visual_range(), Some((0, 0)));

        // j j：选区扩大。
        press(
            &mut state,
            &mut input,
            KeyCode::Char('j'),
            KeyModifiers::NONE,
        );
        press(
            &mut state,
            &mut input,
            KeyCode::Char('j'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().visual_range(), Some((0, 2)));

        // k：反向收缩。
        press(
            &mut state,
            &mut input,
            KeyCode::Char('k'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().visual_range(), Some((0, 1)));

        // Esc 退出。
        assert_eq!(
            press(&mut state, &mut input, KeyCode::Esc, KeyModifiers::NONE),
            Outcome::Continue
        );
        assert!(state.active_tab().visual.is_none());

        // 再次 v：锚点 = 当前光标（1）；反向移动仍有序。
        press(
            &mut state,
            &mut input,
            KeyCode::Char('v'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().visual, Some(1));
        press(
            &mut state,
            &mut input,
            KeyCode::Char('k'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().visual_range(), Some((0, 1)));

        // 每 tab 独立（第二个 tab 无选区；tab0 锚点保持 1）。
        state.push_tab(Tab::from_physical(&std::env::temp_dir(), Config::default()).unwrap());
        assert!(state.active_tab().visual.is_none());
        assert_eq!(state.tabs[0].visual, Some(1));
    }

    #[test]
    fn render_highlights_visual_range() {
        let (_tmp, mut state) = make_state(5);
        state.active_tab_mut().cursor = 2;
        state.active_tab_mut().visual = Some(1); // 选区 1..=2

        let backend = ratatui::backend::TestBackend::new(120, 8);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &state, &InputMode::Normal, None))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();

        // 选区内反显（行 1、2），选区外不反显（行 0、3）。
        assert!(buffer[(0, 1)].modifier.contains(Modifier::REVERSED));
        assert!(buffer[(0, 2)].modifier.contains(Modifier::REVERSED));
        assert!(!buffer[(0, 0)].modifier.contains(Modifier::REVERSED));
        assert!(!buffer[(0, 3)].modifier.contains(Modifier::REVERSED));

        // 状态栏 VISUAL 指示。
        let status = (0..120u16)
            .map(|x| buffer[(x, 7)].symbol())
            .collect::<String>();
        assert!(status.contains("VISUAL (2 selected)"), "status: {status}");
    }

    #[test]
    fn tag_toggle_flow() {
        // M3-2：t 标记多个文件，再次 t 取消；空目录 no-op；每 tab 独立。
        let (_tmp, mut state) = make_state(3);
        let mut input = InputState::default();

        // 标记 cursor 0（f00），j 后标记 f01。
        press(
            &mut state,
            &mut input,
            KeyCode::Char('t'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().tagged_count(), 1);
        press(
            &mut state,
            &mut input,
            KeyCode::Char('j'),
            KeyModifiers::NONE,
        );
        press(
            &mut state,
            &mut input,
            KeyCode::Char('t'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().tagged_count(), 2);
        assert!(state.active_tab().tags.contains("f00"));
        assert!(state.active_tab().tags.contains("f01"));

        // 再次 t 取消 f01。
        press(
            &mut state,
            &mut input,
            KeyCode::Char('t'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().tagged_count(), 1);

        // 每 tab 独立。
        state.push_tab(Tab::from_physical(&std::env::temp_dir(), Config::default()).unwrap());
        assert_eq!(state.active_tab().tagged_count(), 0);
        assert_eq!(state.tabs[0].tagged_count(), 1);

        // 空目录 no-op。
        let empty_tmp = TempDir::new().unwrap();
        let mut empty = Tab::from_physical(empty_tmp.path(), Config::default()).unwrap();
        empty.toggle_tag();
        assert_eq!(empty.tagged_count(), 0);
    }

    #[test]
    fn no_color_renders_without_panic() {
        // M9-5：NO_COLOR 降级验证——TestBackend 无终端颜色，渲染不 panic 即通过。
        // TestBackend 下不存在真实颜色输出，本测试覆盖"降级路径可跑通"。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("f.txt"), "content").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        state.active_tab_mut().tags.insert("f.txt".to_owned());

        let backend = ratatui::backend::TestBackend::new(120, 8);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let draw = terminal.draw(|frame| render(frame, &state, &InputMode::Normal, None));
        assert!(draw.is_ok(), "渲染不应失败");

        // 预览渲染同样不 panic。
        let p = tmp.path().join("f.txt");
        let preview = crate::preview::render_preview(&p, 10);
        let texts = preview.line_texts();
        assert!(texts.iter().any(|t| t.contains("content")));
    }

    #[test]
    fn render_shows_tag_bold_and_count() {
        let (_tmp, mut state) = make_state(3);
        state.active_tab_mut().tags.insert("f01".to_owned());

        let backend = ratatui::backend::TestBackend::new(120, 8);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &state, &InputMode::Normal, None))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();

        // 标记行加粗，未标记行不加粗。
        assert!(buffer[(0, 1)].modifier.contains(Modifier::BOLD));
        assert!(!buffer[(0, 0)].modifier.contains(Modifier::BOLD));

        // 状态栏计数。
        let status = (0..120u16)
            .map(|x| buffer[(x, 7)].symbol())
            .collect::<String>();
        assert!(status.contains("1 tagged"), "status: {status}");
    }

    #[test]
    fn tag_batch_operations() {
        // M3-3：:tag copy / move / delete 对标记集合生效，未标记不受影响。
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(tmp.path().join("a.txt"), "A").unwrap();
        fs::write(tmp.path().join("b.txt"), "B").unwrap();
        fs::write(tmp.path().join("c.txt"), "C").unwrap(); // 不标记
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());

        // 标记 a.txt、b.txt（排序后 a(0) b(1) c(2)）。
        state.active_tab_mut().tags.insert("a.txt".to_owned());
        state.active_tab_mut().tags.insert("b.txt".to_owned());

        // 批量复制到 sub/。
        let target = sub.display().to_string();
        match exec_command_impl(&mut state, &format!("tag copy {target}")) {
            Ok(ExecOutcome::Done { msg }) => {
                let m = msg.unwrap();
                assert!(m.contains("2 完成"), "msg: {m}");
            }
            other => panic!("expected Done, got {other:?}"),
        }
        assert!(sub.join("a.txt").exists() && sub.join("b.txt").exists());
        assert!(!sub.join("c.txt").exists(), "未标记文件不受影响");
        assert!(tmp.path().join("a.txt").exists(), "copy 源保留");
        // M3-4：批量操作完成后标记自动清空。
        assert_eq!(state.active_tab().tagged_count(), 0);

        // 重新标记后再批量移动。
        state.active_tab_mut().tags.insert("a.txt".to_owned());
        state.active_tab_mut().tags.insert("b.txt".to_owned());

        // 两次复制各一条撤销记录，逆序撤销还原副本。
        assert_eq!(state.undo_stack.len(), 2);
        let mut msg = None;
        undo_last(&mut state, &mut msg);
        undo_last(&mut state, &mut msg);
        assert!(!sub.join("a.txt").exists() && !sub.join("b.txt").exists());

        // 批量移动。
        assert!(matches!(
            exec_command_impl(&mut state, &format!("tag move {target}")),
            Ok(ExecOutcome::Done { .. })
        ));
        assert!(!tmp.path().join("a.txt").exists() && sub.join("a.txt").exists());

        // 批量删除：tag move 后旧标记按名解析失效（跳过），
        // 在当前目录新建 d.txt / e.txt 并标记，验证批量删除 + 逐条 u。
        fs::write(tmp.path().join("d.txt"), "D").unwrap();
        fs::write(tmp.path().join("e.txt"), "E").unwrap();
        state.active_tab_mut().refresh(Config::default()).unwrap();
        state.active_tab_mut().clear_tags();
        state.active_tab_mut().tags.insert("d.txt".to_owned());
        state.active_tab_mut().tags.insert("e.txt".to_owned());
        assert_eq!(
            exec_command_impl(&mut state, "tag delete"),
            Ok(ExecOutcome::ConfirmTagDelete)
        );
        let mut msg0 = None;
        execute_tag_delete(&mut state, &mut msg0);
        assert!(msg0.as_deref().unwrap().contains("2 完成"));
        assert!(!tmp.path().join("d.txt").exists());
        assert!(!tmp.path().join("e.txt").exists());
        assert!(tmp.path().join("c.txt").exists(), "未标记不受影响");
        // M3-4：批量删除完成后标记自动清空。
        assert_eq!(state.active_tab().tagged_count(), 0);

        // 逐条 u 恢复被删的两个文件（LIFO）。
        let mut msg = None;
        undo_last(&mut state, &mut msg);
        undo_last(&mut state, &mut msg);
        assert!(tmp.path().join("d.txt").exists() && tmp.path().join("e.txt").exists());

        // 空标记提示 + 未知动作。
        state.active_tab_mut().clear_tags();
        match exec_command_impl(&mut state, "tag delete") {
            Ok(ExecOutcome::Done { msg }) => assert_eq!(msg.as_deref(), Some("tag: 无标记文件")),
            other => panic!("expected Done, got {other:?}"),
        }
        assert!(exec_command_impl(&mut state, "tag snort").is_err());
    }

    #[test]
    fn glob_match_patterns() {
        // M4-2：`*` 与 `?` 语义，大小写不敏感。
        assert!(crate::tab::glob_match("*.txt", "a.txt"));
        assert!(crate::tab::glob_match("*.TXT", "a.txt"));
        assert!(crate::tab::glob_match("a*", "a.txt"));
        assert!(crate::tab::glob_match("a*b", "ab"));
        assert!(crate::tab::glob_match("a*b", "axxb"));
        assert!(!crate::tab::glob_match("a*b", "axxbyy")); // 模式耗尽后仍有剩余字符
        assert!(crate::tab::glob_match("a*b*", "axxbyy"));
        assert!(crate::tab::glob_match("a?c", "abc"));
        assert!(!crate::tab::glob_match("*.txt", "a.rs"));
        assert!(!crate::tab::glob_match("a?c", "ac"));
        assert!(!crate::tab::glob_match("a?c", "abbc"));
        assert!(!crate::tab::glob_match("a*b", "ba"));
    }

    #[test]
    fn filter_apply_and_clear() {
        // M4-2：= 应用 glob 过滤，空 pattern 清除；光标归零。
        let tmp = TempDir::new().unwrap();
        for name in ["a.txt", "b.txt", "c.rs", "sub"] {
            if name == "sub" {
                fs::create_dir(tmp.path().join(name)).unwrap();
            } else {
                fs::write(tmp.path().join(name), "").unwrap();
            }
        }
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut input = InputState::default();

        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('='),
                KeyModifiers::NONE
            ),
            Outcome::EnterFilter
        );

        // 提交 "*.txt" → 只剩两个 txt（目录 sub 被滤掉）。
        state
            .active_tab_mut()
            .apply_filter("*.txt", Config::default());
        let names: Vec<&str> = state
            .active_tab()
            .entries
            .iter()
            .map(|i| i.name.as_str())
            .collect();
        assert_eq!(names, ["a.txt", "b.txt"]);
        assert_eq!(state.active_tab().cursor, 0);

        // 过滤视图内导航 + 再过滤不叠加异常。
        press(
            &mut state,
            &mut input,
            KeyCode::Char('j'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().cursor, 1);
        state.active_tab_mut().apply_filter("a*", Config::default());
        assert_eq!(state.active_tab().entries.len(), 1);

        // 空 pattern 清除过滤 → refresh 恢复全部条目。
        state.active_tab_mut().apply_filter("", Config::default());
        assert!(state.active_tab().filter.is_none());
        assert_eq!(state.active_tab().entries.len(), 4);
    }

    #[test]
    fn render_shows_filter_indicator() {
        let (_tmp, mut state) = make_state(3);
        state.active_tab_mut().filter = Some("*.txt".to_owned());

        let backend = ratatui::backend::TestBackend::new(120, 8);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &state, &InputMode::Normal, None))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let status = (0..120u16)
            .map(|x| buffer[(x, 7)].symbol())
            .collect::<String>();
        assert!(status.contains("[filter: *.txt]"), "status: {status}");
    }

    #[test]
    fn exec_q_cd_help_unified_parse() {
        // M5-3 + M5-4：最小命令子集与多标签命令在同一解析器共存。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("x.txt"), "X").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());

        // :cd 到不存在目录 → 错误；存在 → 导航。
        assert!(exec_command_impl(&mut state, "cd Z:/nope").is_err());
        let sub = tmp.path().join("sub");
        fs::create_dir(&sub).unwrap();
        assert!(matches!(
            exec_command_impl(&mut state, &format!("cd {}", sub.display())),
            Ok(ExecOutcome::Done { msg: None })
        ));
        assert_eq!(state.active_tab().location, Location::Physical(sub.clone()));

        // :q 请求退出。
        assert_eq!(exec_command_impl(&mut state, "q"), Ok(ExecOutcome::Quit));

        // :help 请求帮助层。
        assert_eq!(
            exec_command_impl(&mut state, "help"),
            Ok(ExecOutcome::EnterHelp)
        );

        // M5-4：多标签命令与文件命令共存不冲突（tabnew → touch → tabmove）。
        assert!(matches!(
            exec_command_impl(&mut state, "tabnew"),
            Ok(ExecOutcome::Done { .. })
        ));
        assert!(matches!(
            exec_command_impl(&mut state, "touch y.txt"),
            Ok(ExecOutcome::Done { .. })
        ));
        assert!(matches!(
            exec_command_impl(&mut state, "tabmove 1"),
            Ok(ExecOutcome::Done { .. })
        ));
        assert_eq!(state.tabs.len(), 2);
    }

    #[test]
    fn large_directory_load_and_render_fast() {
        // M6-4：>10k 项大目录——加载与单帧渲染有界，退出不经过批量渲染（逐键响应）。
        let tmp = TempDir::new().unwrap();
        for i in 0..10_500u32 {
            std::fs::write(tmp.path().join(format!("f{i}.txt")), "").unwrap();
        }

        let t0 = std::time::Instant::now();
        let state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let load = t0.elapsed();
        assert_eq!(state.active_tab().entries.len(), 10_500);

        let t1 = std::time::Instant::now();
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &state, &InputMode::Normal, None))
            .unwrap();
        let draw = t1.elapsed();

        // 宽松上限（CI 速度波动安全）：加载与单帧渲染均 < 2s。
        assert!(load.as_secs() < 2, "load too slow: {load:?}");
        assert!(draw.as_secs() < 2, "draw too slow: {draw:?}");
    }

    #[test]
    fn archive_status_bar_shows_host_inner_breadcrumb() {
        // M7-5：状态栏/面包屑显示 `host:/inner/path`。
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("t.zip");
        crate::archive::make_test_zip(&zip_path);
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        // 直接构造归档内 src 层的位置。
        state.active_tab_mut().location = Location::Archive {
            host: zip_path.clone(),
            inner: PathBuf::from("src"),
        };
        state.active_tab_mut().entries = vec![FileItem {
            icon: crate::icons::icon_for(Path::new("main.rs"), crate::icons::IconTheme::Dark),
            name: "main.rs".into(),
            is_dir: false,
            size: 12,
            perm: String::new(),
        }];

        let backend = ratatui::backend::TestBackend::new(120, 8);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &state, &InputMode::Normal, None))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let status = (0..120u16)
            .map(|x| buffer[(x, 7)].symbol())
            .collect::<String>();
        let leaf = zip_path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            status.contains(&format!("{leaf}:/src")),
            "breadcrumb missing: {status}"
        );
    }

    #[test]
    fn archive_search_reuses_m4_chain() {
        // M7-6：归档内 `/` 搜索复用 M4 链路（find_match/search_move 作用于当前条目）。
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("t.zip");
        crate::archive::make_test_zip(&zip_path);
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        state.active_tab_mut().location = Location::Archive {
            host: zip_path,
            inner: PathBuf::new(),
        };
        state.active_tab_mut().entries = vec![
            FileItem {
                icon: crate::icons::icon_for(Path::new("deep/"), crate::icons::IconTheme::Dark),
                name: "deep".into(),
                is_dir: true,
                size: 0,
                perm: String::new(),
            },
            FileItem {
                icon: crate::icons::icon_for(Path::new("readme.md"), crate::icons::IconTheme::Dark),
                name: "readme.md".into(),
                is_dir: false,
                size: 2,
                perm: String::new(),
            },
            FileItem {
                icon: crate::icons::icon_for(Path::new("src/"), crate::icons::IconTheme::Dark),
                name: "src".into(),
                is_dir: true,
                size: 0,
                perm: String::new(),
            },
        ];

        // /main → 命中? 名单里只有 src/main.rs 在子目录，根列表不递归。
        let mut msg = None;
        submit_search(&mut state, "readme", &mut msg);
        assert!(msg.is_none());
        assert_eq!(state.active_tab().cursor, 1);
        // n 环绕回 0（deep 是目录名，不匹配 readme）。
        let mut input = InputState::default();
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('n'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().cursor, 1); // 无第二匹配则停在同一处
    }

    #[test]
    fn archive_view_is_readonly_for_commands() {
        // M7 验收：归档视图内文件操作命令被拒绝（不触碰宿主目录）。
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("t.zip");
        crate::archive::make_test_zip(&zip_path);
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        state.active_tab_mut().location = Location::Archive {
            host: zip_path,
            inner: PathBuf::new(),
        };

        for cmd in [
            "rename x",
            "move Z:/tmp",
            "copy Z:/tmp",
            "mkdir d",
            "touch t",
        ] {
            let err = exec_command_impl(&mut state, cmd).unwrap_err();
            assert!(err.contains("归档视图只读"), "cmd={cmd} err={err}");
        }

        // 标记批量删除：静默拒绝进状态栏。
        state.active_tab_mut().tags.insert("main.rs".to_owned());
        let mut msg = None;
        execute_tag_delete(&mut state, &mut msg);
        assert_eq!(msg.as_deref(), Some("归档视图只读（解压后可修改）"));
    }

    #[test]
    fn render_highlights_cursor_row() {
        let (_tmp, mut state) = make_state(3);
        state.active_tab_mut().cursor = 1;

        let backend = ratatui::backend::TestBackend::new(40, 6);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &state, &InputMode::Normal, None))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        assert!(buffer[(0, 1)].modifier.contains(Modifier::REVERSED));
        assert!(!buffer[(0, 0)].modifier.contains(Modifier::REVERSED));
        assert!(!buffer[(0, 2)].modifier.contains(Modifier::REVERSED));
    }

    #[test]
    fn renders_active_tab_list_and_status_bar() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("hello.txt"), "").unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        let state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());

        let backend = ratatui::backend::TestBackend::new(80, 10);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &state, &InputMode::Normal, None))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let row = |y: u16| {
            (0..80u16)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        };

        // 目录在前并带 `/` 后缀。
        assert!(row(0).contains("sub/"));
        assert!(row(1).contains("hello.txt"));
        // 状态栏显示当前位置与条目数。
        let dir_name = tmp
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let status = row(9).trim().to_owned();
        assert!(
            status.contains(&dir_name),
            "status bar missing cwd: {status}"
        );
        assert!(
            status.contains("2 items"),
            "status bar missing count: {status}"
        );
    }

    #[test]
    fn preview_pane_renders_image_and_text_by_kind() {
        // M11-2：光标在图片文件上 → 预览窗格 halfblocks 渲染非空白；
        // 光标在文本文件上 → 预览窗格显示文本内容。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("notes.txt"), "hello preview").unwrap();
        let img = image::DynamicImage::new_rgba8(16, 16).into_rgba8();
        let mut png = std::io::Cursor::new(Vec::new());
        img.write_to(&mut png, image::ImageFormat::Png).unwrap();
        fs::write(tmp.path().join("pic.png"), png.into_inner()).unwrap();
        let state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());

        let backend = ratatui::backend::TestBackend::new(80, 10);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();

        // 条目按排序：notes.txt < pic.png。光标 0 = notes.txt → 文本预览。
        terminal
            .draw(|frame| render(frame, &state, &InputMode::Normal, None))
            .unwrap();
        let row_text = |buf: &ratatui::buffer::Buffer, y: u16, x0: u16, x1: u16| {
            (x0..x1).map(|x| buf[(x, y)].symbol()).collect::<String>()
        };
        let buf = terminal.backend().buffer().clone();
        // 右侧 40% 窗格：x 48..80。
        let pane_txt: String = (1..9).map(|y| row_text(&buf, y, 48, 80)).collect();
        assert!(
            pane_txt.contains("hello preview"),
            "text preview: {pane_txt}"
        );

        // 光标移到 pic.png → 图片预览（halfblocks 非空白）。
        let mut st = state;
        st.active_tab_mut().cursor = 1;
        terminal
            .draw(|frame| render(frame, &st, &InputMode::Normal, None))
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let pane_img: String = (1..9).map(|y| row_text(&buf, y, 48, 80)).collect();
        let has_blocks = (1..9).any(|y| {
            (48..80).any(|x| {
                let c = &buf[(x, y)];
                c.symbol() != " " || c.fg != ratatui::style::Color::Reset
            })
        });
        assert!(has_blocks, "image preview pane empty: {pane_img}");
    }

    #[test]
    fn renders_command_line_and_message() {
        let (_tmp, state) = make_state(1);
        let backend = ratatui::backend::TestBackend::new(80, 10);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mode = InputMode::Command {
            input: "tabne".to_owned(),
        };
        terminal
            .draw(|frame| render(frame, &state, &mode, Some("未知命令: x")))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let row = |y: u16| {
            (0..80u16)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        };
        assert!(row(9).contains(":tabne"), "command line: {}", row(9));
    }
}
