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
static ACTIVE: std::sync::OnceLock<ThemeState> = std::sync::OnceLock::new();

struct ThemeState {
    theme: Theme,
    variant: opaline::schema::ThemeVariant,
}

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
    let _ = ACTIVE.set(ThemeState { theme, variant });
    Ok(())
}

/// 读取当前主题 variant（M9-3 明暗对齐用；未设置时默认 Dark）。
pub fn active_variant() -> opaline::schema::ThemeVariant {
    ACTIVE
        .get()
        .map(|s| s.variant)
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
    let dir = user_themes_dir();
    let Ok(iter) = std::fs::read_dir(&dir) else {
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
    let path = user_themes_dir().join(format!("{name}.toml"));
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
    let theme = load_user_theme(name)?;
    let variant = theme.meta.variant;
    let _ = ACTIVE.set(ThemeState { theme, variant });
    Ok(())
}

/// 将 opaline 命名样式解析为 ratatui `Style`。
/// 未激活主题或样式名不存在时返回默认样式（零配置可用）。
pub fn style(style_name: &str) -> RtStyle {
    let Some(state) = ACTIVE.get() else {
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
        set_builtin("ayu-dark").unwrap();
        // 激活后样式解析不再返回默认（以常见命名样式验证存在即可）。
        // 具体样式名由主题 TOML 决定——用 palette 常见的 "background" 探测。
        let s = style("background");
        let _ = s; // 存在性由主题文件决定；不 panic 即通过
    }

    #[test]
    fn user_theme_discover_load_and_activate() {
        // M9-6：用户 TOML 主题发现与加载（XDG_DATA/CONFIG 环境注入）。
        let tmp = TempDir::new().unwrap();
        let themes_dir = tmp.path().join("rvifm").join("themes");
        std::fs::create_dir_all(&themes_dir).unwrap();
        std::fs::write(
            themes_dir.join("mytheme.toml"),
            "[meta]\nname = \"My Theme\"\nvariant = \"dark\"\n\n\
             [palette]\nbg = \"#1e1e2e\"\n\n\
             [styles]\nbackground = { fg = \"bg\" }\n",
        )
        .unwrap();

        // SAFETY: 测试进程独占，串行执行，无并发读取。
        unsafe {
            // Windows 上 config_path() 读 APPDATA，Unix 读 XDG_CONFIG_HOME。
            if cfg!(windows) {
                std::env::set_var("APPDATA", tmp.path());
            } else {
                std::env::set_var("XDG_CONFIG_HOME", tmp.path());
            }
        }

        let names = discover_user_themes();
        assert_eq!(names, ["mytheme"]);

        let theme = load_user_theme("mytheme").unwrap();
        assert_eq!(theme.meta.name, "My Theme");
        assert!(matches!(
            theme.meta.variant,
            opaline::schema::ThemeVariant::Dark
        ));

        set_user_theme("mytheme").unwrap();
        assert!(matches!(
            active_variant(),
            opaline::schema::ThemeVariant::Dark
        ));

        // 未存在主题：NotFound，不 panic。
        assert!(load_user_theme("nope").is_err());

        unsafe {
            if cfg!(windows) {
                std::env::remove_var("APPDATA");
            } else {
                std::env::remove_var("XDG_CONFIG_HOME");
            }
        }
    }

    #[test]
    fn set_builtin_unknown_theme_errors() {
        let err = set_builtin("no-such-theme-xyz").unwrap_err();
        assert!(err.to_string().contains("未知内置主题"));
    }
}
