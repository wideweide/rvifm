//! 事件循环：绘制目录列表与状态栏 → 阻塞等待事件 → 分发按键。
//!
//! M0 阶段只显示当前目录（SPEC §5 M0 SIMPLIFIED），不做导航与文件操作；
//! 导航/刷新/多标签在 M1 接入。

use std::path::{Path, PathBuf};

use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Paragraph};

/// 单个目录项。M0 只需要名字与是否目录；元数据在 M1-4 扩展。
struct FileItem {
    name: String,
    is_dir: bool,
}

/// M0 的应用状态：当前目录与启动时一次性加载的条目。
struct App {
    cwd: PathBuf,
    items: Vec<FileItem>,
}

impl App {
    fn new(cwd: PathBuf) -> std::io::Result<Self> {
        let items = load_entries(&cwd)?;
        Ok(Self { cwd, items })
    }
}

/// 读取并排序目录条目：目录在前，名称不区分大小写。
fn load_entries(dir: &Path) -> std::io::Result<Vec<FileItem>> {
    let mut items = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let is_dir = entry.file_type()?.is_dir();
        let name = entry.file_name().to_string_lossy().into_owned();
        items.push(FileItem { name, is_dir });
    }
    items.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(items)
}

/// 判断按键是否请求退出：`q` 或 `Ctrl-C`。
fn is_quit_key(key: KeyEvent) -> bool {
    key.kind == KeyEventKind::Press
        && (key.code == KeyCode::Char('q')
            || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)))
}

/// 事件循环：进入时终端已由 `ratatui::init` 处于 raw mode + 备用屏。
pub fn run(mut terminal: ratatui::DefaultTerminal) -> std::io::Result<()> {
    let app = App::new(std::env::current_dir()?)?;
    loop {
        terminal.draw(|frame| render(frame, &app))?;
        if let Event::Key(key) = event::read()?
            && is_quit_key(key)
        {
            return Ok(());
        }
    }
}

fn render(frame: &mut Frame, app: &App) {
    let [list_area, status_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(frame.area());

    let list_items: Vec<ListItem> = app
        .items
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
            app.cwd.to_string_lossy().into_owned(),
            Style::new().add_modifier(Modifier::BOLD),
        ),
        Span::from(format!(" | {} items", app.items.len())),
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
    fn entries_sorted_dirs_first_case_insensitive() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("zdir")).unwrap();
        fs::create_dir(tmp.path().join("adir")).unwrap();
        fs::write(tmp.path().join("Banana.txt"), "").unwrap();
        fs::write(tmp.path().join("apple.txt"), "").unwrap();

        let items = load_entries(tmp.path()).unwrap();
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["adir", "zdir", "apple.txt", "Banana.txt"]);
        assert!(items[0].is_dir);
        assert!(!items[2].is_dir);
    }

    #[test]
    fn renders_list_and_status_bar() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("hello.txt"), "").unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        let app = App::new(tmp.path().to_path_buf()).unwrap();

        let backend = ratatui::backend::TestBackend::new(80, 10);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let row = |y: u16| {
            (0..80u16)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        };

        // 目录在前并带 `/` 后缀。
        assert!(row(0).contains("sub/"));
        assert!(row(1).contains("hello.txt"));
        // 状态栏显示当前目录与条目数。
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
