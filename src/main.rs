//! rvifm —— Rust 编写的 vim 风格 TUI 文件管理器。
//!
//! 参考 vifm 的交互范式，非 fork、非行为级兼容（见 README.md）。
//!
//! M0-3 接入 `ratatui + crossterm` 事件循环骨架（见 docs/SPEC.md §5 M0）；
//! 目录列表与状态栏在 M0-4 实现。

mod app;

fn main() -> std::io::Result<()> {
    let terminal = ratatui::init();
    let result = app::run(terminal);
    ratatui::restore();
    result
}
