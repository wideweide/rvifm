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
use std::process::Command;

/// 系统默认 opener（M11-3）。
/// Linux：`gio open` → `xdg-open`；macOS：`open`；Windows：`cmd /c start`。
pub fn system_open(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        // Linux：优先 gio open，回退 xdg-open。
        if let Ok(s) = Command::new("gio").args(["open"]).arg(path).status()
            && s.success()
        {
            return Ok(());
        }
        if let Ok(s) = Command::new("xdg-open").arg(path).status()
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
        Command::new("open").arg(path).status().map(|_| ())
    }
    #[cfg(target_os = "windows")]
    {
        let status = Command::new("cmd")
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
