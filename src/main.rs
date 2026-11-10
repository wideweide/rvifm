//! rvifm —— Rust 编写的 vim 风格 TUI 文件管理器。
//!
//! 参考 vifm 的交互范式，非 fork、非行为级兼容（见 README.md）。
//!
//! M0-6 起支持 `--version` / `--help`；无参数时进入 TUI（SPEC §5 M0）。
//! M8 将扩展 `pick` / `record` 子命令。

mod app;

fn main() -> std::io::Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("--version" | "-V") => {
            println!("rvifm {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--help" | "-h") => {
            print_help();
            return Ok(());
        }
        Some(other) => {
            eprintln!("rvifm: 未知参数 `{other}`（使用 --help 查看用法）");
            std::process::exit(2);
        }
        None => {}
    }

    let terminal = ratatui::init();
    let result = app::run(terminal);
    ratatui::restore();
    result
}

fn print_help() {
    println!(
        "rvifm {} — Rust 编写的 vim 风格 TUI 文件管理器\n\n\
         用法: rvifm [选项]\n\n\
         选项:\n  \
         -h, --help     显示帮助\n  \
         -V, --version  显示版本\n\n\
         不带参数启动 TUI 文件管理器。",
        env!("CARGO_PKG_VERSION")
    );
}
