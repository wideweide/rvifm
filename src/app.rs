//! 事件循环：绘制目录列表与状态栏 → 阻塞等待事件 → 分发按键。
//!
//! M1-3 起支持多键序列（`g` 前缀：`gg`；`gt`/`gT` 在 M1-5 接入）与
//! `Ctrl-u`/`Ctrl-d` 半页翻页。

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

/// 分发按键；返回 `true` 表示请求退出。
///
/// `viewport_h` 为列表区高度（行数），用于 `Ctrl-u`/`Ctrl-d` 的半页步进。
/// 已接入：`j/k`、`l`/`Enter`、`h`、`gg`、`G`、`Ctrl-u/d`、`q`/`Ctrl-C`。
fn handle_key(state: &mut AppState, pending: &mut Pending, key: KeyEvent, viewport_h: u16) -> bool {
    if key.kind != KeyEventKind::Press {
        return false; // Release 先过滤，避免误消费待决序列。
    }

    // `g` 前缀序列：待决态优先于退出与普通分发；任意第二键结束序列
    // （未知第二键整体丢弃，包括 `q`/`Ctrl-C`）。
    if *pending == Pending::G {
        *pending = Pending::None;
        if key.code == KeyCode::Char('g') {
            state.active_tab_mut().cursor_top();
        }
        // `gt` / `gT` 在 M1-5 接入多标签切换。
        return false;
    }

    if is_quit_key(key) {
        return true;
    }

    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let half = (viewport_h.saturating_sub(1) / 2).max(1) as isize;
    match key.code {
        KeyCode::Char('g') => *pending = Pending::G,
        KeyCode::Char('G') => state.active_tab_mut().cursor_bottom(),
        KeyCode::Char('d') if ctrl => state.active_tab_mut().move_cursor(half),
        KeyCode::Char('u') if ctrl => state.active_tab_mut().move_cursor(-half),
        KeyCode::Char('j') => state.active_tab_mut().move_cursor(1),
        KeyCode::Char('k') => state.active_tab_mut().move_cursor(-1),
        KeyCode::Char('l') | KeyCode::Enter => state.active_tab_mut().enter_selected(),
        KeyCode::Char('h') => state.active_tab_mut().return_parent(),
        _ => {}
    }
    false
}

/// 事件循环：进入时终端已由 `ratatui::init` 处于 raw mode + 备用屏。
pub fn run(mut terminal: ratatui::DefaultTerminal) -> std::io::Result<()> {
    let mut state = AppState::new(Tab::from_physical(&std::env::current_dir()?)?);
    let mut pending = Pending::default();
    loop {
        terminal.draw(|frame| render(frame, &state))?;
        // 列表区高度 = 终端高度 - 状态栏 1 行。
        let viewport_h = terminal.size()?.height.saturating_sub(1);
        if let Event::Key(key) = event::read()?
            && handle_key(&mut state, &mut pending, key, viewport_h)
        {
            return Ok(());
        }
    }
}

fn render(frame: &mut Frame, state: &AppState) {
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

    // 状态栏：路径 | 选中项 大小 权限 | 条目数（M1-4）。目录省略大小。
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
    let status = Line::from(vec![
        Span::styled(
            tab.location.display_path(),
            Style::new().add_modifier(Modifier::BOLD),
        ),
        Span::from(selected),
        Span::from(format!(" | {} items", tab.entries.len())),
    ]);
    frame.render_widget(Paragraph::new(status), status_area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// 测试辅助：viewport 高度 10（半页步进 4）。
    fn press(
        state: &mut AppState,
        pending: &mut Pending,
        code: KeyCode,
        mods: KeyModifiers,
    ) -> bool {
        handle_key(state, pending, KeyEvent::new(code, mods), 10)
    }

    fn make_state(n: usize) -> (TempDir, AppState) {
        let tmp = TempDir::new().unwrap();
        for i in 0..n {
            fs::write(tmp.path().join(format!("f{i:02}")), "").unwrap();
        }
        let state = AppState::new(Tab::from_physical(tmp.path()).unwrap());
        (tmp, state)
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
        let mut pending = Pending::default();
        assert!(!press(
            &mut state,
            &mut pending,
            KeyCode::Char('j'),
            KeyModifiers::NONE
        ));
        assert_eq!(state.active_tab().cursor, 1);
        assert!(!press(
            &mut state,
            &mut pending,
            KeyCode::Char('k'),
            KeyModifiers::NONE
        ));
        assert_eq!(state.active_tab().cursor, 0);
    }

    #[test]
    fn gg_and_g_jump_top_and_bottom() {
        let (_tmp, mut state) = make_state(5);
        let mut pending = Pending::default();

        assert!(!press(
            &mut state,
            &mut pending,
            KeyCode::Char('G'),
            KeyModifiers::NONE
        ));
        assert_eq!(state.active_tab().cursor, 4);

        assert!(!press(
            &mut state,
            &mut pending,
            KeyCode::Char('g'),
            KeyModifiers::NONE
        ));
        assert_eq!(pending, Pending::G); // 等待第二键
        assert!(!press(
            &mut state,
            &mut pending,
            KeyCode::Char('g'),
            KeyModifiers::NONE
        ));
        assert_eq!(pending, Pending::None);
        assert_eq!(state.active_tab().cursor, 0);
    }

    #[test]
    fn unknown_g_sequence_is_discarded() {
        let (_tmp, mut state) = make_state(3);
        let mut pending = Pending::default();
        // `g` 后跟未知第二键：序列丢弃，不产生移动，也不退出。
        assert!(!press(
            &mut state,
            &mut pending,
            KeyCode::Char('g'),
            KeyModifiers::NONE
        ));
        assert!(!press(
            &mut state,
            &mut pending,
            KeyCode::Char('x'),
            KeyModifiers::NONE
        ));
        assert_eq!(pending, Pending::None);
        assert_eq!(state.active_tab().cursor, 0);
        // 序列内的 `q` 被当作第二键丢弃，之后的 `q` 才退出。
        assert!(!press(
            &mut state,
            &mut pending,
            KeyCode::Char('g'),
            KeyModifiers::NONE
        ));
        assert!(!press(
            &mut state,
            &mut pending,
            KeyCode::Char('q'),
            KeyModifiers::NONE
        ));
        assert!(press(
            &mut state,
            &mut pending,
            KeyCode::Char('q'),
            KeyModifiers::NONE
        ));
    }

    #[test]
    fn ctrl_u_d_half_page() {
        let (_tmp, mut state) = make_state(20);
        let mut pending = Pending::default();

        assert!(!press(
            &mut state,
            &mut pending,
            KeyCode::Char('d'),
            KeyModifiers::CONTROL
        ));
        assert_eq!(state.active_tab().cursor, 4); // (10-1)/2 = 4
        assert!(!press(
            &mut state,
            &mut pending,
            KeyCode::Char('d'),
            KeyModifiers::CONTROL
        ));
        assert_eq!(state.active_tab().cursor, 8);
        assert!(!press(
            &mut state,
            &mut pending,
            KeyCode::Char('u'),
            KeyModifiers::CONTROL
        ));
        assert_eq!(state.active_tab().cursor, 4);
        assert!(!press(
            &mut state,
            &mut pending,
            KeyCode::Char('u'),
            KeyModifiers::CONTROL
        ));
        assert_eq!(state.active_tab().cursor, 0);
        // 顶部继续 Ctrl-u：钳制不越界。
        assert!(!press(
            &mut state,
            &mut pending,
            KeyCode::Char('u'),
            KeyModifiers::CONTROL
        ));
        assert_eq!(state.active_tab().cursor, 0);
    }

    #[test]
    fn render_highlights_cursor_row() {
        let (_tmp, mut state) = make_state(3);
        state.active_tab_mut().cursor = 1;

        let backend = ratatui::backend::TestBackend::new(40, 6);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &state)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        assert!(buffer[(0, 1)].modifier.contains(Modifier::REVERSED));
        assert!(!buffer[(0, 0)].modifier.contains(Modifier::REVERSED));
        assert!(!buffer[(0, 2)].modifier.contains(Modifier::REVERSED));
    }

    #[test]
    fn status_bar_shows_selected_item_metadata() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("hello.txt"), "hello").unwrap(); // 5 字节
        let state = AppState::new(Tab::from_physical(tmp.path()).unwrap());

        let backend = ratatui::backend::TestBackend::new(80, 10);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &state)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let status = (0..80u16)
            .map(|x| buffer[(x, 9)].symbol())
            .collect::<String>();

        assert!(status.contains("hello.txt 5 B"), "status: {status}");
        let perm = &state.active_tab().entries[0].perm;
        assert!(status.contains(perm.as_str()), "status: {status}");
    }

    #[test]
    fn renders_active_tab_list_and_status_bar() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("hello.txt"), "").unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        let state = AppState::new(Tab::from_physical(tmp.path()).unwrap());

        let backend = ratatui::backend::TestBackend::new(80, 10);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &state)).unwrap();
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
}
