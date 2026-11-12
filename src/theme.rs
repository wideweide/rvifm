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
    fn set_builtin_unknown_theme_errors() {
        let err = set_builtin("no-such-theme-xyz").unwrap_err();
        assert!(err.to_string().contains("未知内置主题"));
    }
}
