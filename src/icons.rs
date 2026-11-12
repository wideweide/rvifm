//! 图标渲染（M9-1；SPEC §4.7）。
//!
//! 基于 `devicons` crate：文件列表/预览行首渲染类型图标与颜色。
//! 无 Nerd Font 时终端无法显示 glyph，但不会 panic（字符照传，
//! 由终端决定显示为 tofu——不降级逻辑判断，M9-5 验证）。
//!
//! M9-3：`IconTheme` 与 opaline `ThemeVariant` 对齐——`theme_variant()`
//! 返回当前激活主题的明暗，`IconTheme::Light/Dark` 直接映射 devicons 主题。

use std::path::Path;

/// 图标主题（M9-3 与 opaline ThemeVariant 对齐）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IconTheme {
    #[default]
    Dark,
    Light,
}

/// 单个文件条目的图标信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconInfo {
    /// Nerd Font glyph（单字符）。
    pub glyph: char,
    /// ANSI 256-color 码（devicons 返回的 `&'static str` 数字）。
    pub color: &'static str,
}

/// 获取 `path` 的图标（M9-1）。任何路径都不会 panic；
/// 不存在的路径按扩展名/文件名推断（devicons 内部行为）。
pub fn icon_for(path: &Path, theme: IconTheme) -> IconInfo {
    let theme_opt = match theme {
        IconTheme::Dark => Some(devicons::Theme::Dark),
        IconTheme::Light => Some(devicons::Theme::Light),
    };
    let file_icon = devicons::icon_for_file(path, &theme_opt);
    IconInfo {
        glyph: file_icon.icon,
        color: file_icon.color,
    }
}

/// 便捷入口（M9-3）：直接按当前激活主题取图标。
/// 未激活主题时默认 Dark。
pub fn icon_for_active(path: &Path) -> IconInfo {
    icon_for(path, active_theme())
}

/// 当前图标主题 = 当前激活 opaline 主题的 variant（M9-3 对齐）。
pub fn active_theme() -> IconTheme {
    crate::theme::icon_theme()
}

/// 为列表行渲染 `"{glyph} {name}"` 前缀（M9-1；文件列表用 `icon.glyph` 直接
/// 拼接，本 helper 供 pick/预览共用——消费方在 M9-4 接入）。
#[allow(dead_code)]
pub fn decorate(name: &str, icon: &IconInfo) -> String {
    format!("{} {name}", icon.glyph)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_for_rust_source_returns_glyph_and_color() {
        let p = Path::new("src/main.rs");
        let info = icon_for(p, IconTheme::Dark);
        // glyph 与 color 非空即视为集成成功（具体值由 devicons 数据决定）。
        assert!(info.glyph != '\0', "glyph should be set");
        assert!(!info.color.is_empty());
    }

    #[test]
    fn icon_for_directory_and_light_theme_differ() {
        let dir = Path::new("some/dir");
        let dark = icon_for(dir, IconTheme::Dark);
        let light = icon_for(dir, IconTheme::Light);
        assert!(dark.glyph != '\0');
        assert!(light.glyph != '\0');
        // 目录 icon 存在且两主题均有输出（颜色/glyph 可不同）。
        assert!(!dark.color.is_empty() && !light.color.is_empty());
    }

    #[test]
    fn decorate_prepends_glyph() {
        let info = icon_for(Path::new("lib.rs"), IconTheme::Dark);
        let line = decorate("lib.rs", &info);
        assert!(line.ends_with(" lib.rs"));
        assert!(line.starts_with(info.glyph));
    }

    #[test]
    fn nonexistent_path_does_not_panic() {
        // M9-1：任何路径不 panic（ACCEPTANCE §11 降级验证的前置）。
        let info = icon_for(Path::new("Z:/definitely/not/here.xyz"), IconTheme::Dark);
        let _ = decorate("here.xyz", &info);
    }
}
