//! 事件循环骨架：绘制一帧 → 阻塞等待事件 → 分发按键。
//!
//! M0-3 只建立循环与终端生命周期管理；目录列表、状态栏与 `q` 退出在 M0-4 接入。

use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::widgets::Paragraph;

/// 判断按键是否请求退出。骨架阶段仅 `Ctrl-C`；`q` 在 M0-4 开放。
fn is_quit_key(key: KeyEvent) -> bool {
    key.kind == KeyEventKind::Press
        && key.code == KeyCode::Char('c')
        && key.modifiers.contains(KeyModifiers::CONTROL)
}

/// 事件循环：进入时终端已由 `ratatui::init` 处于 raw mode + 备用屏。
pub fn run(mut terminal: ratatui::DefaultTerminal) -> std::io::Result<()> {
    loop {
        terminal.draw(render)?;
        if let Event::Key(key) = event::read()?
            && is_quit_key(key)
        {
            return Ok(());
        }
    }
}

fn render(frame: &mut Frame) {
    // M0-4 将替换为目录列表 + 状态栏。
    frame.render_widget(Paragraph::new("rvifm（骨架）— Ctrl-C 退出"), frame.area());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_c_requests_quit() {
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(is_quit_key(key));
    }

    #[test]
    fn plain_keys_do_not_quit() {
        let q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        let x = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
        assert!(!is_quit_key(q));
        assert!(!is_quit_key(x));
    }

    #[test]
    fn renders_placeholder_into_test_backend() {
        let backend = ratatui::backend::TestBackend::new(40, 3);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(render).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let line: String = (0..40).map(|x| buffer[(x, 0)].symbol()).collect();
        assert!(line.contains("rvifm"));
    }
}
