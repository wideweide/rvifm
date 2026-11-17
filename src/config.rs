//! TOML 配置读取（M5-1）。
//!
//! 配置缺失时使用默认值（不报错退出，ACCEPTANCE §7）；语法错误返回可理解
//! 提示（状态栏展示，不 panic）。基本选项在 M5-2 生效。

use serde::Deserialize;
use std::path::PathBuf;

/// 用户配置。TOML 字段缺省时逐项回退默认值（`#[serde(default)]`）。
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    /// 显示以 `.` 开头的隐藏文件（默认不显示）。
    pub show_hidden: bool,
    /// 目录排在文件之前（默认开）。
    pub sort_dirs_first: bool,
    /// 排序时不区分大小写（默认开）。
    pub ignore_case_sort: bool,
    /// M18-4：界面行为选项（[ui] 表）。
    #[serde(default)]
    pub ui: UiConfig,
    /// M23-4：frecency 选项（[frecency] 表）。
    #[serde(default)]
    pub frecency: FrecencyConfig,
}

/// M23-4：[frecency] 表——record 忽略模式（命中路径不写入事件）。
/// M23-5：per-topic λ 与容量上限（None = 默认 0.01/0.01/0.005 与 1500）。
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct FrecencyConfig {
    /// glob 模式列表（如 `*/.git`、`*/node_modules/*`）；命中静默跳过。
    pub ignore_patterns: Vec<String>,
    /// per-topic 衰减系数。
    pub lambda_dir: Option<f64>,
    pub lambda_file: Option<f64>,
    pub lambda_unified: Option<f64>,
    /// 容量上限（M23-3 清理阈值）。
    pub max_entries: Option<usize>,
}

/// M18-4：[ui] 表——外部命令/shell 在新终端窗口执行（检测 $TMUX/$STY；
/// 默认开；false = 始终当前 pane）。
/// `attrs_column`（M21-E1）：文件列表显示大小/修改时间列，默认开；
/// 窄终端自动隐藏（文件名优先）。
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct UiConfig {
    pub terminal_in_new_window: bool,
    pub attrs_column: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            terminal_in_new_window: true,
            attrs_column: true,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            show_hidden: false,
            sort_dirs_first: true,
            ignore_case_sort: true,
            ui: UiConfig::default(),
            frecency: FrecencyConfig::default(),
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

/// M19-4：`~/.ssh/config` 路径（远端别名来源）。
pub fn ssh_config_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".ssh").join("config")
}

/// M22-2：`~/.ssh/known_hosts` 路径（主机密钥校验）。
pub fn known_hosts_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".ssh").join("known_hosts")
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

/// M16 NAV-1：char→keyword 快速映射（config.toml `[quickjump]` 表）。
/// 独立于 `Config` 解析（表可选、缺省为空），不存路径——跳转目标由
/// frecency keyword 检索决定。
pub fn load_quickjump_from(path: &std::path::Path) -> std::collections::BTreeMap<String, String> {
    let mut map = std::collections::BTreeMap::new();
    let Ok(text) = std::fs::read_to_string(path) else {
        return map;
    };
    // toml 1.x：`Value::from_str` 只解析单值；文档解析用 `Table`。
    let value = match text.parse::<toml::Table>() {
        Ok(v) => v,
        Err(_) => return map,
    };
    #[cfg(test)]
    eprintln!("DBG parsed: {value:?}");
    if let Some(table) = value.get("quickjump").and_then(|v| v.as_table()) {
        for (k, v) in table {
            if let Some(kw) = v.as_str()
                && let Some(ch) = k.chars().next()
                && k.chars().count() == 1
            {
                map.insert(ch.to_string(), kw.to_owned());
            }
        }
    }
    map
}

static QUICKJUMP: std::sync::OnceLock<std::collections::BTreeMap<String, String>> =
    std::sync::OnceLock::new();

/// M10-5 GAP-1：键位分层（prepend/append/replace），绑定已完成
/// TOML → (Key 序列, Action) 的解析；解析失败的条目跳过。
#[derive(Debug, Default, Clone)]
pub struct KeymapLayers {
    pub prepend: Vec<(Vec<crate::keymap::Key>, crate::keymap::Action)>,
    pub append: Vec<(Vec<crate::keymap::Key>, crate::keymap::Action)>,
    pub replace: Option<Vec<(Vec<crate::keymap::Key>, crate::keymap::Action)>>,
}

/// 解析一条 `{ on = ..., run = ... }` 键位绑定。
fn parse_binding(
    value: &toml::Value,
    into: &mut Vec<(Vec<crate::keymap::Key>, crate::keymap::Action)>,
) {
    let on = value.get("on");
    let Some(run) = value.get("run").and_then(|v| v.as_str()) else {
        return;
    };
    let Some(action) = crate::keymap::Action::from_name(run) else {
        return; // 未知动作名：跳过（不 panic）
    };
    let keys: Vec<crate::keymap::Key> = match on {
        Some(toml::Value::String(spec)) => match crate::keymap::parse_key(spec) {
            Some(k) => vec![k],
            None => return,
        },
        Some(toml::Value::Array(arr)) => {
            let mut keys = Vec::new();
            for item in arr {
                match item.as_str().and_then(crate::keymap::parse_key) {
                    Some(k) => keys.push(k),
                    None => return, // 任一键解析失败：整条跳过
                }
            }
            keys
        }
        _ => return,
    };
    if keys.is_empty() {
        return;
    }
    into.push((keys, action));
}

/// 解析一个模式表（如 `[normal]`）中的三层键位。
fn parse_mode_layers(table: &toml::value::Table, layers: &mut KeymapLayers) {
    let mut replace: Option<Vec<(Vec<crate::keymap::Key>, crate::keymap::Action)>> = None;
    if let Some(arr) = table.get("prepend_keymap").and_then(|v| v.as_array()) {
        for item in arr {
            parse_binding(item, &mut layers.prepend);
        }
    }
    if let Some(arr) = table.get("append_keymap").and_then(|v| v.as_array()) {
        for item in arr {
            parse_binding(item, &mut layers.append);
        }
    }
    if let Some(arr) = table.get("keymap").and_then(|v| v.as_array()) {
        let mut parsed = Vec::new();
        for item in arr {
            parse_binding(item, &mut parsed);
        }
        replace = Some(parsed);
    }
    layers.replace = match (layers.replace.clone(), replace) {
        (None, r) => r,
        (Some(existing), Some(new)) => {
            let mut merged = existing;
            merged.extend(new);
            Some(merged)
        }
        (some, None) => some,
    };
}

/// M10-5 GAP-1：从配置文件解析键位分层（当前支持 `[normal]`；其余模式
/// 表保留解析框架，模式扩展时接入）。
pub fn load_keymap_layers_from(path: &std::path::Path) -> KeymapLayers {
    let mut layers = KeymapLayers::default();
    let Ok(text) = std::fs::read_to_string(path) else {
        return layers;
    };
    let Ok(value) = text.parse::<toml::Table>() else {
        return layers;
    };
    if let Some(mode) = value.get("normal").and_then(|v| v.as_table()) {
        parse_mode_layers(mode, &mut layers);
    }
    layers
}

static KEYMAP_LAYERS: std::sync::OnceLock<KeymapLayers> = std::sync::OnceLock::new();

/// TUI 启动时加载一次（run 入口调用）。
pub fn init_keymap_layers() {
    let _ = KEYMAP_LAYERS.set(load_keymap_layers_from(&config_path()));
}

/// 已加载的键位分层（未初始化时为空 = 纯默认表）。
pub fn keymap_layers() -> KeymapLayers {
    KEYMAP_LAYERS.get().cloned().unwrap_or_default()
}

/// TUI 启动时加载一次（run 入口调用）。
pub fn init_quickjump() {
    let _ = QUICKJUMP.set(load_quickjump_from(&config_path()));
}

/// `'`+字符 的 keyword 别名（未配置返回 None，调用方回退到字符本身）。
pub fn quickjump_alias(c: char) -> Option<String> {
    QUICKJUMP.get().and_then(|m| m.get(&c.to_string()).cloned())
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
    fn keymap_layers_parse_and_skip_invalid() {
        // M10-5 GAP-1：[normal] 三层解析；未知 run/非法键整条跳过。
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[normal]
prepend_keymap = [
  { on = "P", run = "put_move" },
  { on = ["C-o"], run = "histprev" },
  { on = "x", run = "unknown_action" },
  { on = ["bogus!!"], run = "undo" },
]
append_keymap = [
  { on = ["Z", "Z"], run = "delete_permanent" },
]
"#,
        )
        .unwrap();
        let layers = load_keymap_layers_from(&path);
        assert_eq!(layers.prepend.len(), 2, "无效条目跳过");
        assert_eq!(layers.prepend[0].1, crate::keymap::Action::PutMove);
        assert_eq!(layers.prepend[1].1, crate::keymap::Action::HistPrev);
        assert_eq!(layers.append.len(), 1);
        assert!(layers.replace.is_none());
        // 缺文件 → 空。
        assert!(
            load_keymap_layers_from(&tmp.path().join("nope.toml"))
                .prepend
                .is_empty()
        );
    }

    #[test]
    fn quickjump_table_parses_single_char_keys_only() {
        // M16 NAV-1：[quickjump] char→keyword；多字符键忽略。
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            r#"[quickjump]
d = "dl"
c = "code"
dl = "bad"
"#,
        )
        .unwrap();
        let m = load_quickjump_from(&path);
        assert_eq!(m.get("d").map(String::as_str), Some("dl"));
        assert_eq!(m.get("c").map(String::as_str), Some("code"));
        assert!(!m.contains_key("dl"), "多字符键不进快速层");
        // 缺文件 → 空。
        assert!(load_quickjump_from(&tmp.path().join("nope.toml")).is_empty());
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
