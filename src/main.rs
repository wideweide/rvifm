//! rvifm —— Rust 编写的 vim 风格 TUI 文件管理器。
//!
//! 参考 vifm 的交互范式，非 fork、非行为级兼容（见 README.md）。
//!
//! M0-1 仅初始化 Cargo 项目骨架；`ratatui + crossterm` 事件循环在 M0-3 接入，
//! 目录列表与状态栏在 M0-4 实现（见 docs/SPEC.md §5 M0）。

fn main() {
    // TODO(M0-3): 接入 ratatui + crossterm 事件循环。
}
