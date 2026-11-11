//! rvifm —— Rust 编写的 vim 风格 TUI 文件管理器。
//!
//! 参考 vifm 的交互范式，非 fork、非行为级兼容（见 README.md）。
//!
//! M0-6 起支持 `--version` / `--help`；无参数时进入 TUI（SPEC §5 M0）。
//! M8 将扩展 `pick` / `record` 子命令。

mod app;
mod archive;
mod config;
mod frecency;
mod ops;
mod state;
mod store;
mod tab;

fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--version" | "-V") => {
            println!("rvifm {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--help" | "-h") => {
            print_help();
            return Ok(());
        }
        // M8-3：frecency 写入入口——record 是唯一写入入口（红线）。
        Some("record") => {
            return frecency::cmd_record(args.collect());
        }
        // M8-6：只读模糊选择器；M8-5：compaction 子命令。
        Some("pick") => {
            return frecency::cmd_pick(args.collect());
        }
        Some("maintenance") => {
            return match args.next().as_deref() {
                Some("compact") => frecency::cmd_compact(),
                _ => {
                    eprintln!("rvifm: maintenance 需要 compact 子命令");
                    std::process::exit(2);
                }
            };
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
         用法: rvifm [子命令] [选项]\n\n\
         选项:\n  \
         -h, --help     显示帮助\n  \
         -V, --version  显示版本\n\n\
         子命令:\n  \
         record --topic dir <path>  记录一次已发生的访问（仅 cd/打开成功后调用）\n  \
         pick                       只读模糊选择器（stdin 候选 → stdout 选中）\n  \
         maintenance compact        合并事件到 snapshot（手动 compaction）\n\n\
         不带子命令启动 TUI 文件管理器。",
        env!("CARGO_PKG_VERSION")
    );
}
