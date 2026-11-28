//! rvifm —— Rust 编写的 vim 风格 TUI 文件管理器。
//!
//! 参考 vifm 的交互范式，非 fork、非行为级兼容（见 README.md）。
//!
//! M0-6 起支持 `--version` / `--help`；无参数时进入 TUI（SPEC §5 M0）。
//! M8 将扩展 `pick` / `record` 子命令。
//!
//! M40-1：模块树移入 `lib.rs`（bench 共用）；本文件只保留 CLI 分派与
//! TUI 启动（`use rvifm::` 引入 lib 项）。

use ratatui::crossterm::tty::IsTty;
use rvifm::{app, frecency, privilege};

fn main() -> std::io::Result<()> {
    // M95-6：panic hook 保留默认回溯，并提示用 `rvifm doctor` 自证环境。
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        eprintln!("rvifm 发生 panic；请运行 `rvifm doctor` 并附上输出以便诊断。");
        default_hook(info);
    }));
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
        // M57：帮助只在应用内 `:help` 执行，不提供 `rvifm help` 外部子命令。
        // M8-3：frecency 写入入口——record 是唯一写入入口（红线）。
        Some("record") => {
            return frecency::cmd_record(args.collect());
        }
        // M8-6：只读模糊选择器；M8-5：compaction 子命令。
        Some("pick") => {
            return frecency::cmd_pick(args.collect());
        }
        // M58-8：只读 shell 历史命令选择器（fzf Ctrl+R 同款）。
        Some("cmd") => {
            return frecency::cmd_cmd(args.collect());
        }
        // M38-3：shell 集成初始化脚本（一行接入）。
        Some("init") => {
            return match args.next().as_deref() {
                Some(shell @ ("bash" | "zsh" | "fish" | "powershell")) => {
                    print!("{}", rvifm::shell_init::script(shell));
                    Ok(())
                }
                other => {
                    eprintln!(
                        "init: 未知 shell {}（支持 bash/zsh/fish/powershell）",
                        other.unwrap_or("<缺省>")
                    );
                    std::process::exit(2);
                }
            };
        }
        Some("maintenance") => {
            return match args.next().as_deref() {
                Some("compact") => frecency::cmd_compact(),
                // M76-1/M84：清理 frecency 中临时目录前缀记录（不删本地文件）。
                Some("clean-tmp") => frecency::cmd_clean_tmp(),
                _ => {
                    eprintln!("rvifm: maintenance 需要 compact 子命令");
                    std::process::exit(2);
                }
            };
        }
        // M95-6：一键自证式诊断（终端 / 配置 / 数据 / 外部工具）。
        Some("doctor") => {
            let cfg_path = rvifm::config::config_path();
            let data_dir = rvifm::config::data_dir();
            let tty = std::io::stdout().is_tty();
            let tool = |name: &str| -> &'static str {
                std::process::Command::new(name)
                    .arg("--version")
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .map(|s| if s.success() { "可用" } else { "异常" })
                    .unwrap_or("缺失")
            };
            println!("rvifm doctor");
            println!("  版本: {}", env!("CARGO_PKG_VERSION"));
            println!("  TTY: {}", if tty { "是" } else { "否（CI/管道）" });
            println!(
                "  配置: {} ({})",
                cfg_path.display(),
                if cfg_path.exists() {
                    "存在"
                } else {
                    "未生成"
                }
            );
            println!("  数据目录: {}", data_dir.display());
            println!("  git: {}", tool("git"));
            println!("  tmux: {}", tool("tmux"));
            println!("  ziggity: {}", tool("ziggity"));
            return Ok(());
        }
        // M12-2：sudo -A 的 askpass 辅助模式（读 FIFO 密码 → stdout）。
        Some("--askpass") => {
            return privilege::cmd_askpass(args.collect());
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
    // FEAT-1：退出时恢复鼠标捕获状态（含错误退出路径）。
    let _ = ratatui::crossterm::execute!(
        std::io::stdout(),
        ratatui::crossterm::event::DisableMouseCapture
    );
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
         cmd [query]                只读 shell 历史命令选择（fzf Ctrl+R 同款）\n  \
         maintenance compact        合并事件到 snapshot（手动 compaction）\n  \
         maintenance clean-tmp      清理 frecency 中临时目录前缀记录（含 Windows Temp）\n\n\
         不带子命令启动 TUI 文件管理器。",
        env!("CARGO_PKG_VERSION")
    );
}
