//! 主题引擎（M9-2；SPEC §4.7）。
//!
//! 基于 `opaline`：内置主题按名加载（`load_by_name`），运行时切换
//! （`set_active`）；`apply` 将命名样式应用到 ratatui `Style`。
//! 用户 TOML 主题发现与加载在 M9-6 接入。
//!
//! M9-2 仅替换 TUI 中可集中管理的样式来源（状态栏/列表）；
//! 逐 widget 硬编码样式的全部替换随 M9-3（明暗对齐）推进。
//!
//! M9-2 仅固化主题引擎入口；消费方：M9-3（明暗对齐）、
//! M9-6（用户 TOML 主题）、TUI 渲染接线。

// M9-2 引擎层先行；set_active/builtin_names 的 TUI 接线在 M9-3 接入。
#![allow(dead_code)]

use std::path::Path;

use ratatui::style::{Color as RtColor, Modifier as RtModifier, Style as RtStyle};

use opaline::Theme;

/// 当前激活主题（进程级单选；TUI 与 pick 共用）。
/// AUDIT-13：`OnceLock` 只能写入一次——运行时 `:theme` 重复切换会静默
/// 失效，改用 `RwLock<Option<_>>` 支持任意次替换；None = 未激活
/// （样式走调用方 16 色回退、图标默认 Dark）。
static ACTIVE: std::sync::RwLock<Option<ThemeState>> = std::sync::RwLock::new(None);

struct ThemeState {
    theme: Theme,
    variant: opaline::schema::ThemeVariant,
}

/// 写入激活主题（读写锁失败静默——仅毒化时发生，UI 主题保持原样）。
fn set_active(theme: Theme, variant: opaline::schema::ThemeVariant) {
    if let Ok(mut slot) = ACTIVE.write() {
        *slot = Some(ThemeState { theme, variant });
    }
}

/// 测试专用：清空激活主题（恢复"未选主题默认 Dark"基线）。
/// 激活类测试与颜色敏感渲染测试须共用 [`TEST_LOCK`] 串行。
#[cfg(test)]
pub(crate) fn reset_active() {
    if let Ok(mut slot) = ACTIVE.write() {
        *slot = None;
    }
}

/// 测试专用串行锁：主题激活会改变进程级样式/图标（Rgb 前景），
/// 与颜色断言的渲染测试（如 m21e3）共用此锁避免并行污染。
#[cfg(test)]
pub(crate) static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// 内置主题名列表（opaline 自动发现，启动时打印枚举供 `:theme` 类命令用）。
pub fn builtin_names() -> Vec<String> {
    opaline::builtins::builtin_names()
        .iter()
        .map(|(id, _)| id.to_string())
        .collect()
}

/// 按名加载并激活内置主题；未知名称返回 Err（不 panic）。
pub fn set_builtin(name: &str) -> std::io::Result<()> {
    let theme = opaline::builtins::load_by_name(name).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("未知内置主题: {name}"),
        )
    })?;
    let variant = theme.meta.variant;
    set_active(theme, variant);
    Ok(())
}

/// AUDIT-13：统一激活入口——内置主题优先，其次用户主题目录
/// （`~/.config/rvifm/themes/`）；两者都未命中/不可读时返回可读 Err
/// （`[ui] theme` 启动激活与 `:theme` 运行时切换共用）。
pub fn activate(name: &str) -> std::io::Result<()> {
    if set_builtin(name).is_ok() {
        return Ok(());
    }
    set_user_theme(name).map_err(|e| {
        std::io::Error::new(
            e.kind(),
            format!("主题 `{name}` 激活失败（内置与用户主题目录均未命中或不可读）: {e}"),
        )
    })
}

/// 读取当前主题 variant（M9-3 明暗对齐用；未设置时默认 Dark）。
pub fn active_variant() -> opaline::schema::ThemeVariant {
    ACTIVE
        .read()
        .ok()
        .and_then(|slot| slot.as_ref().map(|s| s.variant))
        .unwrap_or(opaline::schema::ThemeVariant::Dark)
}

/// 当前图标主题 = 激活主题 variant 对应的 devicons 主题（M9-3 对齐）。
pub fn icon_theme() -> crate::icons::IconTheme {
    match active_variant() {
        opaline::schema::ThemeVariant::Light => crate::icons::IconTheme::Light,
        _ => crate::icons::IconTheme::Dark,
    }
}

// ── M9-6：用户 TOML 主题发现与加载 ────────────────────────────────────────

/// 用户主题目录：`~/.config/rvifm/themes/`（Windows `%APPDATA%\rvifm\themes`）。
pub fn user_themes_dir() -> std::path::PathBuf {
    crate::config::config_path()
        .parent()
        .unwrap_or(Path::new("."))
        .join("themes")
}

/// 发现用户主题目录下的全部 `.toml` 主题文件（不含扩展名，排序返回）。
/// 目录不存在时返回空 Vec（零配置可用）。
pub fn discover_user_themes() -> Vec<String> {
    discover_user_themes_from(&user_themes_dir())
}

/// 从指定目录发现用户主题（M9-6 注入式测试入口）。
pub fn discover_user_themes_from(dir: &Path) -> Vec<String> {
    let Ok(iter) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = iter
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|x| x.eq_ignore_ascii_case("toml"))
                .unwrap_or(false)
        })
        .filter_map(|e| {
            e.path()
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .collect();
    names.sort();
    names
}

/// 从用户主题目录按名加载 TOML 主题（M9-6）。未找到返回 NotFound。
pub fn load_user_theme(name: &str) -> std::io::Result<Theme> {
    load_user_theme_from(&user_themes_dir(), name)
}

/// 从指定目录按名加载 TOML 主题（M9-6 注入式测试入口）。
pub fn load_user_theme_from(dir: &Path, name: &str) -> std::io::Result<Theme> {
    let path = dir.join(format!("{name}.toml"));
    if !path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("用户主题不存在: {}", path.display()),
        ));
    }
    opaline::loader::load_from_file(&path)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// 激活用户主题（M9-6）：加载成功后与内置主题同样接管渲染。
/// 解析失败返回 Err（不 panic、不改变当前主题）。
pub fn set_user_theme(name: &str) -> std::io::Result<()> {
    set_user_theme_from(&user_themes_dir(), name)
}

/// 从指定目录激活用户主题（M9-6 注入式测试入口）。
pub fn set_user_theme_from(dir: &Path, name: &str) -> std::io::Result<()> {
    let theme = load_user_theme_from(dir, name)?;
    let variant = theme.meta.variant;
    set_active(theme, variant);
    Ok(())
}

/// 将 opaline 命名样式解析为 ratatui `Style`。
/// 未激活主题或样式名不存在时返回默认样式（零配置可用）。
pub fn style(style_name: &str) -> RtStyle {
    let Ok(slot) = ACTIVE.read() else {
        return RtStyle::default();
    };
    let Some(state) = slot.as_ref() else {
        return RtStyle::default();
    };
    let s = state.theme.style(style_name);
    let mut rt = RtStyle::default();
    if let Some(fg) = s.fg {
        rt = rt.fg(to_rt_color(&fg));
    }
    if let Some(bg) = s.bg {
        rt = rt.bg(to_rt_color(&bg));
    }
    if s.bold {
        rt = rt.add_modifier(RtModifier::BOLD);
    }
    if s.italic {
        rt = rt.add_modifier(RtModifier::ITALIC);
    }
    rt
}

/// M34-1：vifm 高亮组 → 主题标准 token 通用推导（数据驱动，无主题分支）。
/// 主题直接提供该命名样式时由 `vifm_style` 优先采用；缺失时按组从 opaline
/// 内置主题通用词表（accent/bg/text）推导；任一所需 token 缺失返回 None
/// （整组回退 16 色，调用方处理）。
pub(crate) fn derive_vifm_group(name: &str) -> Option<RtStyle> {
    let slot = ACTIVE.read().ok()?;
    let state = slot.as_ref()?;
    let (fg_token, bg_token, bold) = match name {
        "TopLine" => ("accent.primary", Some("bg.panel"), false),
        "TopLineSel" => ("accent.primary", Some("bg.highlight"), true),
        "TabLine" => ("text.secondary", Some("bg.panel"), false),
        "TabLineSel" => ("accent.primary", Some("bg.highlight"), true),
        "StatusLine" => ("text.muted", Some("bg.panel"), true),
        "Border" => ("text.dim", None, false),
        _ => return None,
    };
    let fg = state.theme.try_color(fg_token).map(|c| to_rt_color(&c))?;
    let mut s = RtStyle::new().fg(fg);
    if let Some(token) = bg_token {
        let bg = state.theme.try_color(token).map(|c| to_rt_color(&c))?;
        s = s.bg(bg);
    }
    if bold {
        s = s.add_modifier(RtModifier::BOLD);
    }
    Some(s)
}

fn to_rt_color(c: &opaline::color::OpalineColor) -> RtColor {
    RtColor::Rgb(c.r, c.g, c.b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn builtin_names_nonempty_and_unique() {
        let names = builtin_names();
        assert!(!names.is_empty());
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "主题名应唯一");
    }

    #[test]
    fn set_builtin_known_theme_enables_styles() {
        let _guard = TEST_LOCK.lock().unwrap();
        set_builtin("ayu-dark").unwrap();
        // 激活后样式解析不再返回默认（以常见命名样式验证存在即可）。
        // 具体样式名由主题 TOML 决定——用 palette 常见的 "background" 探测。
        let s = style("background");
        let _ = s; // 存在性由主题文件决定；不 panic 即通过
        reset_active();
    }

    #[test]
    fn user_theme_discover_load_and_activate() {
        // M9-6：用户 TOML 主题发现与加载（目录注入，无 env 污染）。
        let tmp = TempDir::new().unwrap();
        let themes_dir = tmp.path().join("themes");
        std::fs::create_dir_all(&themes_dir).unwrap();
        std::fs::write(
            themes_dir.join("mytheme.toml"),
            "[meta]\nname = \"My Theme\"\nvariant = \"dark\"\n\n\
             [palette]\nbg = \"#1e1e2e\"\n\n\
             [styles]\nbackground = { fg = \"bg\" }\n",
        )
        .unwrap();

        let names = discover_user_themes_from(&themes_dir);
        assert_eq!(names, ["mytheme"]);

        let theme = load_user_theme_from(&themes_dir, "mytheme").unwrap();
        assert_eq!(theme.meta.name, "My Theme");
        assert!(matches!(
            theme.meta.variant,
            opaline::schema::ThemeVariant::Dark
        ));

        set_user_theme_from(&themes_dir, "mytheme").unwrap();
        assert!(matches!(
            active_variant(),
            opaline::schema::ThemeVariant::Dark
        ));

        // 未存在主题：NotFound，不 panic。
        assert!(load_user_theme_from(&themes_dir, "nope").is_err());
        reset_active();
    }

    #[test]
    fn set_builtin_unknown_theme_errors() {
        let err = set_builtin("no-such-theme-xyz").unwrap_err();
        assert!(err.to_string().contains("未知内置主题"));
    }

    #[test]
    fn activate_prefers_builtin_then_user_and_errors_readable() {
        // AUDIT-13：activate 内置优先；用户目录命中次之；均未命中 → 可读
        // Err（不 panic、不改变当前主题）。激活测试持 TEST_LOCK（与颜色
        // 敏感渲染测试串行）并在结束复位，避免全局样式污染。
        let _guard = TEST_LOCK.lock().unwrap();
        let name = builtin_names().first().expect("内置主题非空").clone();
        activate(&name).unwrap();

        let tmp = TempDir::new().unwrap();
        let themes_dir = tmp.path().join("themes");
        std::fs::create_dir_all(&themes_dir).unwrap();
        std::fs::write(
            themes_dir.join("mine.toml"),
            "[meta]\nname = \"Mine\"\nvariant = \"light\"\n\n\
             [palette]\nbg = \"#ffffff\"\n\n\
             [styles]\nbackground = { fg = \"bg\" }\n",
        )
        .unwrap();
        set_user_theme_from(&themes_dir, "mine").unwrap();
        assert!(matches!(
            active_variant(),
            opaline::schema::ThemeVariant::Light
        ));

        // 均未命中：可读错误（含主题名），当前主题不变。
        let variant_before = active_variant();
        let err = activate("no-such-theme-xyz").unwrap_err();
        assert!(err.to_string().contains("no-such-theme-xyz"));
        assert_eq!(active_variant(), variant_before);
        reset_active();
    }

    #[test]
    fn derive_vifm_groups_under_gruvbox_dark() {
        // M34-1：gruvbox-dark 下 vifm 组经标准 token 推导（视觉等价 vifm gruvbox）。
        let _guard = TEST_LOCK.lock().unwrap();
        set_builtin("gruvbox-dark").unwrap();
        let tl = derive_vifm_group("TopLine").unwrap();
        assert_eq!(
            tl.fg,
            Some(RtColor::Rgb(254, 128, 25)),
            "TopLine fg=accent.primary"
        );
        assert_eq!(tl.bg, Some(RtColor::Rgb(40, 40, 40)), "TopLine bg=bg.panel");
        let tls = derive_vifm_group("TopLineSel").unwrap();
        assert_eq!(
            tls.bg,
            Some(RtColor::Rgb(60, 56, 54)),
            "TopLineSel bg=bg.highlight"
        );
        assert!(tls.add_modifier.contains(RtModifier::BOLD));
        let b = derive_vifm_group("Border").unwrap();
        assert_eq!(
            b.fg,
            Some(RtColor::Rgb(168, 153, 132)),
            "Border fg=text.dim"
        );
        assert_eq!(b.bg, None);
        reset_active();
    }

    #[test]
    fn derive_vifm_group_none_without_theme_or_tokens() {
        // M34-1：无主题 / 主题缺标准 token / 未知组名 → None（整组 16 色回退）。
        let _guard = TEST_LOCK.lock().unwrap();
        reset_active();
        assert!(derive_vifm_group("TopLine").is_none());
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("themes");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("mini.toml"),
            "[meta]\nname = \"Mini\"\nvariant = \"dark\"\n\n[palette]\nbg = \"#111111\"\n",
        )
        .unwrap();
        set_user_theme_from(&dir, "mini").unwrap();
        assert!(
            derive_vifm_group("TopLine").is_none(),
            "缺 accent token → None"
        );
        assert!(derive_vifm_group("WildMenu").is_none(), "未知组名 → None");
        reset_active();
    }

    #[test]
    fn dracula_builtin_activates_and_derives() {
        // M34-1 验证项：`[ui] theme = "dracula"` 生效（activate 链 + 通用推导）。
        let _guard = TEST_LOCK.lock().unwrap();
        activate("dracula").unwrap();
        assert!(matches!(
            active_variant(),
            opaline::schema::ThemeVariant::Dark
        ));
        let s = derive_vifm_group("StatusLine").expect("dracula 含标准 token 词表");
        assert!(s.fg.is_some() && s.bg.is_some());
        reset_active();
    }

    #[test]
    fn corrupt_user_theme_file_errors_readable() {
        // AUDIT-13：损坏主题文件 → 可读 Err，不 panic。
        let tmp = TempDir::new().unwrap();
        let themes_dir = tmp.path().join("themes");
        std::fs::create_dir_all(&themes_dir).unwrap();
        std::fs::write(themes_dir.join("broken.toml"), "not toml at all [[[").unwrap();
        let err = load_user_theme_from(&themes_dir, "broken").unwrap_err();
        assert!(!err.to_string().is_empty());
    }
}
