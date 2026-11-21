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
    /// M25-4：归档创建选项（[archive] 表）。
    #[serde(default)]
    pub archive: ArchiveConfig,
    /// M27-3：预览选项（[preview] 表——PDF 图像页数/展示优先级）。
    #[serde(default)]
    pub preview: PreviewConfig,
    /// M36-5：传输选项（[transfer] 表——冲突策略）。
    #[serde(default)]
    pub transfer: TransferConfig,
    /// AUDIT-7：[opener] 表——命名 opener 定义（run/block/desc）。
    #[serde(default)]
    pub opener: std::collections::BTreeMap<String, crate::opener::OpenerDef>,
    /// AUDIT-7：[open] 表——打开规则分层（prepend/append/rules）。
    #[serde(default)]
    pub open: crate::opener::OpenLayers,
    /// AUDIT-6：[keymap] 表——键位解析选项（多键序列超时）。
    #[serde(default)]
    pub keymap: KeymapConfig,
}

/// M25-4：[archive] 表——归档后校验文件类型（md5 默认 / sha256 / none）。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ArchiveConfig {
    pub checksum: String,
}

impl Default for ArchiveConfig {
    fn default() -> Self {
        Self {
            checksum: "md5".to_owned(),
        }
    }
}

/// M27-3：[preview] 表——PDF 图像预览配置。
/// `pdf_image_pages`：0=关闭，1=第 1 页（默认），2=前两页（预览区上下排列）。
/// `pdf_prefer_image`：true = 文本提取先行后仍优先展示图像（展示优先级，
/// 探测顺序不跳过文本提取——AGENTS §5.5）；false = 仅文本层不足
/// （NeedsVisual）时走图像。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PreviewConfig {
    pub pdf_image_pages: u32,
    pub pdf_prefer_image: bool,
}

impl Default for PreviewConfig {
    fn default() -> Self {
        Self {
            pdf_image_pages: 1,
            pdf_prefer_image: false,
        }
    }
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
    /// M30-1：cmd topic 衰减速率（缺省 0.01）。
    pub lambda_cmd: Option<f64>,
    /// 容量上限（M23-3 清理阈值）。
    pub max_entries: Option<usize>,
}

/// M18-4：[ui] 表——外部命令/shell 在新终端窗口执行（检测 $TMUX/$STY；
/// 默认开；false = 始终当前 pane）。
/// `attrs_column`（M21-E1）：文件列表显示大小/修改时间列，默认开；
/// 窄终端自动隐藏（文件名优先）。
/// `session_restore`（M25-1）：退出保存/启动恢复会话（tabs），默认开；
/// false = 不读不写。
/// `git_column`（M25-5）：文件列表 git 状态字符列，默认开；非仓库隐藏。
/// `theme`（AUDIT-13；M34-1 默认 `gruvbox-dark`）：opaline 内置主题名或
/// 用户主题文件名；None = 不激活（图标默认 Dark、样式走 16 色回退）。
/// `pane_borders`（M27-2）：true = 旧整框边框外观；false（默认）= vifm
/// 风格布局（无整框、pane 顶部路径行 TopLine/TopLineSel、间隔列分隔线）。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct UiConfig {
    pub terminal_in_new_window: bool,
    pub attrs_column: bool,
    pub session_restore: bool,
    pub git_column: bool,
    pub theme: Option<String>,
    pub pane_borders: bool,
    /// M39-6：主区布局（`layout = single|dual|preview-right|preview-bottom`；
    /// None/未知 = dual 缺省）。
    pub layout: Option<String>,
}

/// M39-6：主区布局模式——`[ui] layout`。
/// - Single：单栏（列表全宽；view 模式查看器占全宽）；
/// - Dual（缺省）：双栏（查看器按焦点占对侧，M34-4）；
/// - PreviewRight：view 模式查看器恒在右半；
/// - PreviewBottom：view 模式查看器占下半全宽（列表上半双栏）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutMode {
    #[default]
    Dual,
    Single,
    PreviewRight,
    PreviewBottom,
}

impl LayoutMode {
    /// 解析配置串（非法值回落 dual）。
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "single" => Some(Self::Single),
            "dual" => Some(Self::Dual),
            "preview-right" => Some(Self::PreviewRight),
            "preview-bottom" => Some(Self::PreviewBottom),
            _ => None,
        }
    }
}

impl UiConfig {
    /// M39-6：布局模式（None/未知回落 Dual）。
    pub fn layout(&self) -> LayoutMode {
        self.layout
            .as_deref()
            .and_then(LayoutMode::parse)
            .unwrap_or_default()
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            terminal_in_new_window: true,
            attrs_column: true,
            session_restore: true,
            git_column: true,
            // M34-1：默认主题 gruvbox-dark（视觉等价 vifm gruvbox；显式配置覆盖）。
            theme: Some("gruvbox-dark".to_string()),
            pane_borders: false,
            layout: None,
        }
    }
}

/// AUDIT-6：[keymap] 表——多键序列待决超时（毫秒）。超时清空待决 seq，
/// 后续按键按全新序列解析（不被旧前缀吞掉）。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct KeymapConfig {
    pub seq_timeout_ms: u64,
}

/// M36-5：传输选项（[transfer] 表——冲突策略）。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct TransferConfig {
    /// 冲突策略：ask（缺省）/overwrite/skip/rename/newer。
    pub conflict_policy: String,
    /// M36-7：粘贴入队（true = 多选传输排队顺序执行；缺省 false = 同步直传）。
    pub queue: bool,
}

impl Default for TransferConfig {
    fn default() -> Self {
        Self {
            conflict_policy: "ask".to_owned(),
            queue: false,
        }
    }
}

impl Default for KeymapConfig {
    fn default() -> Self {
        Self {
            seq_timeout_ms: 500,
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
            archive: ArchiveConfig::default(),
            preview: PreviewConfig::default(),
            transfer: TransferConfig::default(),
            opener: std::collections::BTreeMap::new(),
            open: crate::opener::OpenLayers::default(),
            keymap: KeymapConfig::default(),
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
    /// M23-11：自定义外部命令模板（run = "cmd:<模板>" 注册；索引 = 数组序号）。
    pub commands: Vec<String>,
    /// M33-1：per-mode 分层（[visual]/[view]/[command]；缺省空 = 未配置
    /// 模式行为不变——消费端回退内建默认）。
    pub visual: ModeLayers,
    pub view: ModeLayers,
    pub command: ModeLayers,
}

/// M33-1：单模式三层（不含自定义命令表——command 模式绑定共用全局表）。
#[derive(Debug, Default, Clone)]
pub struct ModeLayers {
    pub prepend: Vec<(Vec<crate::keymap::Key>, crate::keymap::Action)>,
    pub append: Vec<(Vec<crate::keymap::Key>, crate::keymap::Action)>,
    pub replace: Option<Vec<(Vec<crate::keymap::Key>, crate::keymap::Action)>>,
}

/// 解析一条 `{ on = ..., run = ... }` 键位绑定。
fn parse_binding(
    value: &toml::Value,
    into: &mut Vec<(Vec<crate::keymap::Key>, crate::keymap::Action)>,
    commands: &mut Vec<String>,
) {
    let on = value.get("on");
    let Some(run) = value.get("run").and_then(|v| v.as_str()) else {
        return;
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
    // M23-11：run = "cmd:<模板>" → 注册自定义命令（去重）+ RunCommand(索引)。
    if let Some(tpl) = run.strip_prefix("cmd:") {
        let idx = if let Some(pos) = commands.iter().position(|c| c == tpl) {
            pos
        } else {
            commands.push(tpl.to_owned());
            commands.len() - 1
        };
        into.push((keys, crate::keymap::Action::RunCommand(idx as u16)));
        return;
    }
    let Some(action) = crate::keymap::Action::from_name(run) else {
        return; // 未知动作名：跳过（不 panic）
    };
    into.push((keys, action));
}

/// M33-1：解析单模式三层（无 KeymapLayers 依赖——供 [visual]/[view]/
/// [command] 的 ModeLayers 解析复用）。
fn parse_mode_layers_standalone(table: &toml::value::Table) -> ModeLayers {
    let mut out = ModeLayers::default();
    let mut commands = Vec::new();
    if let Some(arr) = table.get("prepend_keymap").and_then(|v| v.as_array()) {
        for item in arr {
            parse_binding(item, &mut out.prepend, &mut commands);
        }
    }
    if let Some(arr) = table.get("append_keymap").and_then(|v| v.as_array()) {
        for item in arr {
            parse_binding(item, &mut out.append, &mut commands);
        }
    }
    if let Some(arr) = table.get("keymap").and_then(|v| v.as_array()) {
        let mut parsed = Vec::new();
        for item in arr {
            parse_binding(item, &mut parsed, &mut commands);
        }
        out.replace = Some(parsed);
    }
    out
}

/// 解析一个模式表（如 `[normal]`）中的三层键位。
fn parse_mode_layers(table: &toml::value::Table, layers: &mut KeymapLayers) {
    let mut replace: Option<Vec<(Vec<crate::keymap::Key>, crate::keymap::Action)>> = None;
    if let Some(arr) = table.get("prepend_keymap").and_then(|v| v.as_array()) {
        for item in arr {
            parse_binding(item, &mut layers.prepend, &mut layers.commands);
        }
    }
    if let Some(arr) = table.get("append_keymap").and_then(|v| v.as_array()) {
        for item in arr {
            parse_binding(item, &mut layers.append, &mut layers.commands);
        }
    }
    if let Some(arr) = table.get("keymap").and_then(|v| v.as_array()) {
        let mut parsed = Vec::new();
        for item in arr {
            parse_binding(item, &mut parsed, &mut layers.commands);
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
    // M33-1：[visual]/[view]/[command] 三模式分层（缺省空 = 未配置）。
    for (key, slot) in [
        ("visual", &mut layers.visual),
        ("view", &mut layers.view),
        ("command", &mut layers.command),
    ] {
        if let Some(mode) = value.get(key).and_then(|v| v.as_table()) {
            *slot = parse_mode_layers_standalone(mode);
        }
    }
    layers
}

static KEYMAP_LAYERS: std::sync::OnceLock<KeymapLayers> = std::sync::OnceLock::new();

/// M23-11：自定义外部命令模板（run = "cmd:<模板>" 注册；索引 = 数组序号）。
/// OnceLock 借 init_keymap_layers 一次性写入；handle_key 经 keymap_commands() 读取。
static KEYMAP_COMMANDS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();

/// M23-11：读取自定义命令表（run 循环 / handle_key 消费）。
pub fn keymap_commands() -> Vec<String> {
    KEYMAP_COMMANDS.get().cloned().unwrap_or_default()
}

/// TUI 启动时加载一次（run 入口调用）。
pub fn init_keymap_layers() {
    let layers = load_keymap_layers_from(&config_path());
    let _ = KEYMAP_COMMANDS.set(layers.commands.clone());
    let _ = KEYMAP_LAYERS.set(layers);
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
    fn opener_and_open_tables_parse() {
        // AUDIT-7：SPEC §4.9 示例形态——[opener] 定义 + [open] 三层规则
        // （`use` 字段 rename、block 默认 false、desc 可选）。
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[opener]
preview_image = { run = "imv %s", desc = "用 imv 打开图片" }
edit_text = { run = "nvim %s", block = true, desc = "用 nvim 编辑" }

[open]
prepend_rules = [
    { mime = "image/*", use = ["preview_image"] },
    { mime = "text/*", use = ["edit_text", "open"] },
]
append_rules = [
    { name = "*.md", use = ["edit_text"] },
]
"#,
        )
        .unwrap();
        let cfg = load_from(&path).unwrap();
        assert_eq!(cfg.opener.len(), 2);
        let edit = &cfg.opener["edit_text"];
        assert_eq!(edit.run, "nvim %s");
        assert!(edit.block, "block 字段生效");
        assert_eq!(edit.desc.as_deref(), Some("用 nvim 编辑"));
        let img = &cfg.opener["preview_image"];
        assert!(!img.block, "block 缺省 false");
        assert_eq!(cfg.open.prepend_rules.len(), 2);
        assert_eq!(cfg.open.append_rules.len(), 1);
        assert!(cfg.open.rules.is_none());
        let r0 = &cfg.open.prepend_rules[0];
        assert_eq!(r0.mime.as_deref(), Some("image/*"));
        assert!(r0.name.is_none());
        assert_eq!(r0.use_rules, vec!["preview_image".to_owned()]);
        let r1 = &cfg.open.prepend_rules[1];
        assert_eq!(
            r1.use_rules,
            vec!["edit_text".to_owned(), "open".to_owned()]
        );
        assert_eq!(cfg.open.append_rules[0].name.as_deref(), Some("*.md"));
    }

    #[test]
    fn opener_missing_tables_default_empty() {
        // AUDIT-7：无 [opener]/[open] → 空定义/空分层（SystemDefault 兜底）。
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "[ui]\nterminal_in_new_window = false\n").unwrap();
        let cfg = load_from(&path).unwrap();
        assert!(cfg.opener.is_empty());
        assert!(cfg.open.prepend_rules.is_empty());
        assert!(cfg.open.append_rules.is_empty());
        assert!(cfg.open.rules.is_none());
        assert_eq!(
            cfg.ui.theme.as_deref(),
            Some("gruvbox-dark"),
            "未配置主题 → 默认 gruvbox-dark（M34-1）"
        );
    }

    #[test]
    fn ui_theme_option_parses() {
        // AUDIT-13：[ui] theme = "ayu-dark" → Option<String>。
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "[ui]\ntheme = \"ayu-dark\"\n").unwrap();
        let cfg = load_from(&path).unwrap();
        assert_eq!(cfg.ui.theme.as_deref(), Some("ayu-dark"));
    }

    #[test]
    fn frecency_lambda_cmd_parses_with_default() {
        // M30-1：[frecency] lambda_cmd 可配；缺省 None（frecency 内取 0.01）。
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "[frecency]\nlambda_cmd = 0.05\n").unwrap();
        let cfg = load_from(&path).unwrap();
        assert_eq!(cfg.frecency.lambda_cmd, Some(0.05));
        let cfg2 = load_from(&tmp.path().join("nope.toml")).unwrap();
        assert_eq!(cfg2.frecency.lambda_cmd, None);
    }

    #[test]
    fn preview_table_parse_and_defaults() {
        // M27-3：[preview] 表解析——pdf_image_pages / pdf_prefer_image；
        // 缺省 1 / false（NeedsVisual 才走图像）。
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            "[preview]\npdf_image_pages = 2\npdf_prefer_image = true\n",
        )
        .unwrap();
        let cfg = load_from(&path).unwrap();
        assert_eq!(cfg.preview.pdf_image_pages, 2);
        assert!(cfg.preview.pdf_prefer_image);

        let tmp2 = TempDir::new().unwrap();
        let path2 = tmp2.path().join("config.toml");
        std::fs::write(&path2, "[ui]\nterminal_in_new_window = false\n").unwrap();
        let cfg2 = load_from(&path2).unwrap();
        assert_eq!(cfg2.preview, PreviewConfig::default());
        assert_eq!(cfg2.preview.pdf_image_pages, 1);
        assert!(!cfg2.preview.pdf_prefer_image);
    }

    #[test]
    fn m331_visual_view_command_layers_parse() {
        // M33-1：三模式分层解析——prepend/append/replace 独立生效；
        // 未配置模式为空（零行为变化）；未知动作名条目跳过。
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[visual]
prepend_keymap = [
    { on = "d", run = "delete_selected" },
]
append_keymap = [
    { on = "x", run = "unknown_action_xyz" },
]

[view]
keymap = [
    { on = "q", run = "toggle_view" },
]

[command]
prepend_keymap = [
    { on = "C-a", run = "to_top" },
]
"#,
        )
        .unwrap();
        let layers = load_keymap_layers_from(&path);
        assert_eq!(layers.visual.prepend.len(), 1, "visual prepend");
        assert!(matches!(
            layers.visual.prepend[0].1,
            crate::keymap::Action::DeleteSelected
        ));
        assert_eq!(layers.visual.append.len(), 0, "未知动作整条跳过");
        assert!(layers.view.replace.is_some(), "view replace 层");
        assert!(matches!(
            layers.view.replace.as_ref().unwrap()[0].1,
            crate::keymap::Action::ToggleView
        ));
        assert_eq!(layers.command.prepend.len(), 1, "command prepend");

        // 未配置 → 全空（默认 Config 形态）。
        let tmp2 = TempDir::new().unwrap();
        let layers2 = load_keymap_layers_from(&tmp2.path().join("nope.toml"));
        assert!(layers2.visual.prepend.is_empty());
        assert!(layers2.visual.replace.is_none());
        assert!(layers2.command.prepend.is_empty());
    }

    #[test]
    fn keymap_seq_timeout_parses_with_default() {
        // AUDIT-6：[keymap] seq_timeout_ms 可配置；缺省 500。
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "[keymap]\nseq_timeout_ms = 200\n").unwrap();
        let cfg = load_from(&path).unwrap();
        assert_eq!(cfg.keymap.seq_timeout_ms, 200);
        let cfg2 = load_from(&tmp.path().join("nope.toml")).unwrap();
        assert_eq!(cfg2.keymap.seq_timeout_ms, 500, "缺省 500ms");
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

    #[test]
    fn m23_11_parse_binding_cmd_prefix() {
        // M23-11：run = "cmd:<模板>" → RunCommand(索引) 注册到 commands 表（去重）。
        let mut layers = KeymapLayers::default();
        let mut prepend = Vec::new();
        let mut table = toml::value::Table::new();
        table.insert(
            "on".to_owned(),
            toml::Value::Array(vec![
                toml::Value::String(",".to_owned()),
                toml::Value::String("g".to_owned()),
            ]),
        );
        table.insert(
            "run".to_owned(),
            toml::Value::String("cmd:lazygit %g".to_owned()),
        );
        parse_binding(
            &toml::Value::Table(table),
            &mut prepend,
            &mut layers.commands,
        );
        assert_eq!(prepend.len(), 1);
        assert!(matches!(prepend[0].1, crate::keymap::Action::RunCommand(0)));
        assert_eq!(layers.commands, vec!["lazygit %g".to_owned()]);
        // 重复模板 → 同索引（去重）。
        let mut table2 = toml::value::Table::new();
        table2.insert("on".to_owned(), toml::Value::String(",x".to_owned()));
        table2.insert(
            "run".to_owned(),
            toml::Value::String("cmd:lazygit %g".to_owned()),
        );
        parse_binding(
            &toml::Value::Table(table2),
            &mut prepend,
            &mut layers.commands,
        );
        assert_eq!(layers.commands.len(), 1, "命令模板去重");
    }
}
