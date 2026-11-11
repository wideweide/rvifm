//! 事件循环：绘制目录列表与状态栏 → 阻塞等待事件 → 分发按键。
//!
//! M1-0 起渲染基于 `AppState` / `Tab`；导航与光标移动在 M1-1/M1-2 接入。

use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Paragraph};

use crate::state::AppState;
use crate::tab::Tab;

/// 判断按键是否请求退出：`q` 或 `Ctrl-C`。
fn is_quit_key(key: KeyEvent) -> bool {
    key.kind == KeyEventKind::Press
        && (key.code == KeyCode::Char('q')
            || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)))
}

/// 事件循环：进入时终端已由 `ratatui::init` 处于 raw mode + 备用屏。
pub fn run(mut terminal: ratatui::DefaultTerminal) -> std::io::Result<()> {
    let state = AppState::new(Tab::from_physical(&std::env::current_dir()?)?);
    loop {
        terminal.draw(|frame| render(frame, &state))?;
        if let Event::Key(key) = event::read()?
            && is_quit_key(key)
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
    frame.render_widget(List::new(list_items), list_area);

    let status = Line::from(vec![
        Span::styled(
            tab.location.display_path(),
            Style::new().add_modifier(Modifier::BOLD),
        ),
        Span::from(format!(" | {} items", tab.entries.len())),
    ]);
    frame.render_widget(Paragraph::new(status), status_area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

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
