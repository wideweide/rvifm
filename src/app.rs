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

use crate::state::AppState;
use crate::tab::Tab;

/// 判断按键是否请求退出：`q` 或 `Ctrl-C`。
fn is_quit_key(key: KeyEvent) -> bool {
    key.kind == KeyEventKind::Press
        && (key.code == KeyCode::Char('q')
            || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)))
}

/// `g` 前缀多键序列的待决状态（超时可配置在 M10 键位系统接入）。
#[derive(Debug, Default, PartialEq, Eq)]
enum Pending {
    #[default]
    None,
    /// 已按下 `g`，等待第二键。
    G,
}

/// Normal 模式输入状态：`g` 待决 + 数字计数（`N gt` 用，M1-5）。
#[derive(Debug, Default)]
struct InputState {
    pending: Pending,
    count: Option<usize>,
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
}

/// 输入模式（M5-4 统一模式系统前先持有最小两个）。
#[derive(Debug, Default, PartialEq, Eq)]
enum InputMode {
    #[default]
    Normal,
    /// 命令行模式：正在输入 `:` 后的内容。
    Command { input: String },
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

/// 执行最小 `:` 命令（M1-5）。返回 `Ok(true)` 表示应用退出（关闭最后一个 tab）。
fn exec_command(state: &mut AppState, line: &str) -> Result<bool, String> {
    let mut parts = line.split_whitespace();
    match parts.next() {
        None => Ok(false),
        Some("tabnew") => {
            if state.new_tab_here() {
                Ok(false)
            } else {
                Err("tabnew: 目录读取失败".to_owned())
            }
        }
        Some("tabclose") => Ok(state.close_tab()),
        Some("tabmove") => {
            let n = parts
                .next()
                .map(str::parse::<usize>)
                .transpose()
                .map_err(|_| "tabmove: 位置必须是数字".to_owned())?;
            state.move_tab(n);
            Ok(false)
        }
        Some(other) => Err(format!("未知命令: {other}（完整命令解析在 M5-4 接入）")),
    }
}

/// 分发 Normal 模式按键。
///
/// `viewport_h` 为列表区高度（行数），用于 `Ctrl-u`/`Ctrl-d` 的半页步进。
/// 已接入：`j/k`、`l`/`Enter`、`h`、`gg`、`G`、`Ctrl-u/d`、`gt`/`gT`/`N gt`、
/// `q`/`Ctrl-C`、`:`。
fn handle_key(
    state: &mut AppState,
    input: &mut InputState,
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

    // `g` 前缀序列：待决态优先于退出与普通分发；任意第二键结束序列
    // （未知第二键整体丢弃，包括 `q`/`Ctrl-C`）。
    if input.pending == Pending::G {
        input.pending = Pending::None;
        let count = input.count.take();
        match key.code {
            KeyCode::Char('g') => state.active_tab_mut().cursor_top(),
            KeyCode::Char('t') => match count {
                Some(n) => state.goto_tab(n),
                None => state.next_tab(),
            },
            KeyCode::Char('T') => match count {
                Some(n) => state.goto_tab(n),
                None => state.prev_tab(),
            },
            _ => {}
        }
        return Outcome::Continue;
    }

    if is_quit_key(key) {
        return Outcome::Quit;
    }

    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let half = (viewport_h.saturating_sub(1) / 2).max(1) as isize;
    match key.code {
        KeyCode::Char('g') => input.pending = Pending::G,
        KeyCode::Char('G') => state.active_tab_mut().cursor_bottom(),
        KeyCode::Char('d') if ctrl => state.active_tab_mut().move_cursor(half),
        KeyCode::Char('u') if ctrl => state.active_tab_mut().move_cursor(-half),
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
            state.active_tab_mut().enter_selected();
        }
        KeyCode::Char('h') => {
            input.count = None;
            state.active_tab_mut().return_parent();
        }
        KeyCode::Char(':') => {
            input.count = None;
            return Outcome::EnterCommand;
        }
        _ => input.count = None,
    }
    Outcome::Continue
}

/// 事件循环：进入时终端已由 `ratatui::init` 处于 raw mode + 备用屏。
pub fn run(mut terminal: ratatui::DefaultTerminal) -> std::io::Result<()> {
    let mut state = AppState::new(Tab::from_physical(&std::env::current_dir()?)?);
    let mut input = InputState::default();
    let mut mode = InputMode::default();
    let mut msg: Option<String> = None;
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
                        match exec_command(&mut state, line.trim()) {
                            Ok(true) => return Ok(()),
                            Ok(false) => msg = None,
                            Err(m) => msg = Some(m),
                        }
                    }
                },
                InputMode::Normal => match handle_key(&mut state, &mut input, key, viewport_h) {
                    Outcome::Continue => {}
                    Outcome::Quit => return Ok(()),
                    Outcome::EnterCommand => {
                        mode = InputMode::Command {
                            input: String::new(),
                        };
                    }
                },
            }
        }
    }
}

fn render(frame: &mut Frame, state: &AppState, mode: &InputMode, msg: Option<&str>) {
    let tab = state.active_tab();
    let [list_area, status_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(frame.area());

    let list_items: Vec<ListItem> = tab
        .entries
        .iter()
        .map(|item| {
            let label = if item.is_dir {
                format!("{}/", item.name)
            } else {
                item.name.clone()
            };
            ListItem::new(Line::from(label))
        })
        .collect();
    let mut list_state = ListState::default().with_selected(Some(tab.cursor));
    frame.render_stateful_widget(
        List::new(list_items).highlight_style(Style::new().add_modifier(Modifier::REVERSED)),
        list_area,
        &mut list_state,
    );

    let status = match mode {
        InputMode::Command { input } => Line::from(format!(":{input}")),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tab::Location;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// 测试辅助：viewport 高度 10（半页步进 4）。
    fn press(
        state: &mut AppState,
        input: &mut InputState,
        code: KeyCode,
        mods: KeyModifiers,
    ) -> Outcome {
        handle_key(state, input, KeyEvent::new(code, mods), 10)
    }

    fn make_state(n: usize) -> (TempDir, AppState) {
        let tmp = TempDir::new().unwrap();
        for i in 0..n {
            fs::write(tmp.path().join(format!("f{i:02}")), "").unwrap();
        }
        let state = AppState::new(Tab::from_physical(tmp.path()).unwrap());
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
        assert_eq!(input.pending, Pending::G); // 等待第二键
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('g'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(input.pending, Pending::None);
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
        assert_eq!(input.pending, Pending::None);
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
        let state = AppState::new(Tab::from_physical(tmp.path()).unwrap());

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
