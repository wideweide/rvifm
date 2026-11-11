//! TOML 配置读取（M5-1）。
//!
//! 配置缺失时使用默认值（不报错退出，ACCEPTANCE §7）；语法错误返回可理解
//! 提示（状态栏展示，不 panic）。基本选项在 M5-2 生效。

use serde::Deserialize;
use std::path::PathBuf;

/// 用户配置。TOML 字段缺省时逐项回退默认值（`#[serde(default)]`）。
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    /// 显示以 `.` 开头的隐藏文件（默认不显示）。
    pub show_hidden: bool,
    /// 目录排在文件之前（默认开）。
    pub sort_dirs_first: bool,
    /// 排序时不区分大小写（默认开）。
    pub ignore_case_sort: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            show_hidden: false,
            sort_dirs_first: true,
            ignore_case_sort: true,
        }
    }
}

/// 用户配置文件路径：Windows `%APPDATA%\rvifm\config.toml`；
/// Unix `$XDG_CONFIG_HOME/rvifm/config.toml`，缺省 `~/.config/rvifm/config.toml`。
pub fn config_path() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."))
    };
    base.join("rvifm").join("config.toml")
}

/// 数据目录（frecency 数据库与降级文件）：Windows `%LOCALAPPDATA%\rvifm`；
/// Unix `$XDG_DATA_HOME/rvifm`，缺省 `~/.local/share/rvifm`（M8-3）。
pub fn data_dir() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share"))
            })
            .unwrap_or_else(|| PathBuf::from("."))
    };
    base.join("rvifm")
}

/// frecency 数据库文件路径（M8-3）。
pub fn frecency_db_path() -> PathBuf {
    data_dir().join("frecency.redb")
}

/// 从路径加载配置：缺失 → 默认值；读取/解析失败 → 可理解的错误信息。
pub fn load_from(path: &std::path::Path) -> Result<Config, String> {
    match std::fs::read_to_string(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(format!("读取配置失败: {e}")),
        Ok(text) => toml::from_str(&text).map_err(|e| format!("配置解析失败: {e}")),
    }
}

/// 按平台默认位置加载配置（启动入口用）。
pub fn load() -> Result<Config, String> {
    load_from(&config_path())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn missing_file_yields_default() {
        let tmp = TempDir::new().unwrap();
        let cfg = load_from(&tmp.path().join("nope.toml")).unwrap();
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn valid_toml_overrides_options() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            "show_hidden = true\nsort_dirs_first = false\nignore_case_sort = true\n",
        )
        .unwrap();
        let cfg = load_from(&path).unwrap();
        assert!(cfg.show_hidden);
        assert!(!cfg.sort_dirs_first);
        assert!(cfg.ignore_case_sort);
    }

    #[test]
    fn partial_toml_falls_back_per_key() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "show_hidden = true\n").unwrap();
        let cfg = load_from(&path).unwrap();
        assert!(cfg.show_hidden);
        assert!(cfg.sort_dirs_first); // 缺省键回退默认
    }

    #[test]
    fn syntax_error_returns_readable_message() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "show_hidden = [[[").unwrap();
        let err = load_from(&path).unwrap_err();
        assert!(err.starts_with("配置解析失败"), "err: {err}");
    }
}
