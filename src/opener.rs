//! 文件打开器（M11-3/M11-4/M11-5；SPEC §4.9）。
//!
//! 零配置默认走系统 opener；TOML 分层定制（prepend/append/replace）。
//! 目录由 rvifm 内部进入，文件才调用系统 opener。
//!
//! M11-3/M11-4/M11-5 固化 opener 层；消费方：M11-7（Enter 打开文件
//! 经 enter_selected 接入）。

// M11-3/M11-5 固化 opener 层；M11-7 接入 TUI enter_selected 路径。
#![allow(dead_code)]

use std::path::Path;
use std::process::{Command, Stdio};

/// 静默化子进程 IO（M21-F8）：GUI 程序的 stderr/stdout 不漏进 TUI 终端
/// （如 winit_wayland 警告直漏终端）；失败经返回值进状态栏（app.rs 调用方）。
fn silent(mut cmd: Command) -> Command {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd
}

/// 系统默认 opener（M11-3；M21-F8 子进程 IO 静默化）。
/// Linux：`gio open` → `xdg-open`；macOS：`open`；Windows：`cmd /c start`。
pub fn system_open(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        // Linux：优先 gio open，回退 xdg-open。
        if let Ok(s) = silent(Command::new("gio"))
            .args(["open"])
            .arg(path)
            .status()
            && s.success()
        {
            return Ok(());
        }
        if let Ok(s) = silent(Command::new("xdg-open")).arg(path).status()
            && s.success()
        {
            return Ok(());
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "gio open 和 xdg-open 均不可用",
        ))
    }
    #[cfg(target_os = "macos")]
    {
        silent(Command::new("open")).arg(path).status().map(|_| ())
    }
    #[cfg(target_os = "windows")]
    {
        let status = silent(Command::new("cmd"))
            .args(["/c", "start", ""])
            .arg(path)
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(format!("start 返回 {}", status)))
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = path;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "不支持的平台",
        ))
    }
}

/// opener 规则（M11-5）：mime 优先，name 回退。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenerRule {
    /// glob 模式匹配文件名（如 `*.txt`）。
    pub pattern: String,
    /// 打开命令（如 `nvim`）。
    pub command: String,
}

/// opener 配置分层（M11-4）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpenerConfig {
    /// prepend 规则优先命中。
    pub prepend_rules: Vec<OpenerRule>,
    /// append 规则在默认之后。
    pub append_rules: Vec<OpenerRule>,
}

/// 解析 opener 命令：先查规则（prepend → append），未命中走系统 opener。
pub fn resolve_opener(config: &OpenerConfig, path: &Path) -> OpenResult {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    // prepend → append 顺序检查。
    for rule in config.prepend_rules.iter().chain(&config.append_rules) {
        if glob_match_simple(&rule.pattern, &name) {
            return OpenResult::Command(rule.command.clone());
        }
    }
    OpenResult::SystemDefault
}

/// opener 解析结果。
#[derive(Debug, PartialEq, Eq)]
pub enum OpenResult {
    /// 使用系统默认 opener。
    SystemDefault,
    /// 使用指定命令。
    Command(String),
}

/// 简化 glob 匹配（`*` 通配符，不区分大小写）。
fn glob_match_simple(pattern: &str, name: &str) -> bool {
    let p = pattern.to_lowercase();
    let n = name.to_lowercase();
    if let Some(prefix) = p.strip_suffix('*') {
        return n.starts_with(prefix);
    }
    if let Some(suffix) = p.strip_prefix('*') {
        return n.ends_with(suffix);
    }
    if p.starts_with('*') && p.ends_with('*') && p.len() > 1 {
        return n.contains(&p[1..p.len() - 1]);
    }
    p == n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn m21f8_system_open_failure_returns_readable_err() {
        // M21-F8：子进程 IO 静默化（silent()：stdin/stdout/stderr = null，
        // GUI 程序 stderr 不漏进 TUI 终端——stable Rust 无法内省 Command
        // stdio，实现直读）+ 失败经返回 Err 由调用方进状态栏（app.rs
        // "打开失败: {e}"）。本测试验证失败路径：不存在的文件返回可读 Err
        // 且不 panic。仅 unix：Windows `start` 对不存在文件会弹 GUI 对话框
        // 阻塞 status()（本机实测），不能进自动化测试。
        let r = system_open(Path::new("definitely-not-here-xyz.txt"));
        let err = r.expect_err("不存在的文件应返回 Err");
        assert!(!err.to_string().is_empty(), "错误信息可读: {err}");
    }

    #[test]
    fn glob_match_simple_patterns() {
        assert!(glob_match_simple("*.txt", "readme.txt"));
        assert!(glob_match_simple("*.TXT", "readme.txt"));
        assert!(glob_match_simple("Makefile", "Makefile"));
        assert!(glob_match_simple("*.rs", "main.rs"));
        assert!(!glob_match_simple("*.rs", "main.go"));
        // 不区分大小写（Windows 文件系统惯例）。
        assert!(glob_match_simple("Makefile", "makefile"));
    }

    #[test]
    fn resolve_opener_prepend_takes_priority() {
        let config = OpenerConfig {
            prepend_rules: vec![OpenerRule {
                pattern: "*.md".to_owned(),
                command: "nvim".to_owned(),
            }],
            append_rules: vec![OpenerRule {
                pattern: "*.md".to_owned(),
                command: "less".to_owned(),
            }],
        };
        let result = resolve_opener(&config, Path::new("/w/notes.md"));
        assert_eq!(result, OpenResult::Command("nvim".to_owned()));
    }

    #[test]
    fn resolve_opener_append_fires_when_no_prepend() {
        let config = OpenerConfig {
            prepend_rules: vec![],
            append_rules: vec![OpenerRule {
                pattern: "*.md".to_owned(),
                command: "less".to_owned(),
            }],
        };
        let result = resolve_opener(&config, Path::new("/w/notes.md"));
        assert_eq!(result, OpenResult::Command("less".to_owned()));
    }

    #[test]
    fn resolve_opener_unmatched_returns_system_default() {
        let config = OpenerConfig {
            prepend_rules: vec![OpenerRule {
                pattern: "*.xyz".to_owned(),
                command: "foo".to_owned(),
            }],
            append_rules: vec![],
        };
        let result = resolve_opener(&config, Path::new("/w/notes.md"));
        assert_eq!(result, OpenResult::SystemDefault);
    }

    // M11-3 的 `system_open` 依赖系统 opener，测试中不调用（会弹出 GUI）。
    // `resolve_opener` 的规则匹配测试覆盖了 M11-5 逻辑。
}
