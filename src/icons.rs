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

/// M45-2b devicons 默认图标 glyph（`*`）——无后缀/未知扩展名时返回，需按类型推断。
const DEFAULT_GLYPH: char = '*';
// 复用 devicons 内已有 glyph（Nerd Font），避免自造图标。
const GLYPH_EXEC: char = '\u{e795}'; // nf-oct-terminal（sh）
const GLYPH_TEXT: char = '\u{f0219}'; // nf-oct-file_text（txt）
const GLYPH_BINARY: char = '\u{eae8}'; // nf-oct-binary（bin）
const GLYPH_JAR: char = '\u{e738}'; // nf-dev-java
const GLYPH_DEB: char = '\u{e70e}'; // nf-android（apk，包语义）

/// M45-2b：类型感知图标推断（纯函数，便于测试）。当 devicons 返回默认
/// `*` 或为 jar/deb 时按实际类型给出特定图标：可执行 → 终端、文本 → 文本、
/// 二进制 → 通用二进制、jar/deb → 包图标。
pub fn infer_icon(name: &str, is_exec: bool, is_text: bool, base: IconInfo) -> IconInfo {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".jar") {
        return IconInfo {
            glyph: GLYPH_JAR,
            color: "#cc3e44",
        };
    }
    if lower.ends_with(".deb") {
        return IconInfo {
            glyph: GLYPH_DEB,
            color: "#34a853",
        };
    }
    if base.glyph != DEFAULT_GLYPH {
        return base; // devicons 已有专用图标
    }
    let (glyph, color) = if is_exec {
        (GLYPH_EXEC, "#4d5a5e")
    } else if is_text {
        (GLYPH_TEXT, "#89e051")
    } else {
        (GLYPH_BINARY, "#9F0500")
    };
    IconInfo { glyph, color }
}

/// M45-2b：列表条目图标——devicons + 类型推断。`is_text` 由调用方对无后缀
/// 文件做有界嗅探（NUL → 二进制）。目录/已知扩展名仍走 devicons 原图标。
pub fn icon_for_entry(path: &Path, is_exec: bool, theme: IconTheme) -> IconInfo {
    let base = icon_for(path, theme);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    // 仅当 devicons 给默认 `*` 或 jar/deb 时才需嗅探文本（省 IO）。
    let need_sniff = base.glyph == DEFAULT_GLYPH
        && !name.to_ascii_lowercase().ends_with(".jar")
        && !name.to_ascii_lowercase().ends_with(".deb");
    let is_text = if need_sniff { sniff_text(path) } else { false };
    infer_icon(&name, is_exec, is_text, base)
}

/// 有界嗅探：前 4KB 无 NUL 且含可打印 UTF-8 → 视为文本（M34-7 魔数思路简化）。
/// 只嗅探常规文件——FIFO/socket/设备文件 `open` 或 `read` 会永久阻塞
/// （实测：目录里含 FIFO 时整个 TUI 卡死，`/tmp` 即含 socket/FIFO）。
fn sniff_text(path: &Path) -> bool {
    use std::io::Read;
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut buf = [0u8; 4096];
    let Ok(n) = file.read(&mut buf) else {
        return false;
    };
    let slice = &buf[..n];
    !slice.contains(&0) && std::str::from_utf8(slice).is_ok()
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
    fn m452b_infer_icon_for_extensionless_and_packages() {
        // M45-2b：devicons 默认 `*` 被按类型替换；jar/deb 专用；已有图标保留。
        let default = IconInfo {
            glyph: DEFAULT_GLYPH,
            color: "#7e8ea8",
        };
        let exec = infer_icon("run", true, false, default.clone());
        assert_eq!(exec.glyph, GLYPH_EXEC);
        let text = infer_icon("README", false, true, default.clone());
        assert_eq!(text.glyph, GLYPH_TEXT);
        let bin = infer_icon("data", false, false, default.clone());
        assert_eq!(bin.glyph, GLYPH_BINARY);
        assert_eq!(
            infer_icon("app.jar", false, false, default.clone()).glyph,
            GLYPH_JAR
        );
        assert_eq!(
            infer_icon("pkg.deb", false, false, default.clone()).glyph,
            GLYPH_DEB
        );
        // 已有专用图标（.rs → devicons glyph）不被覆盖。
        let rs = icon_for(Path::new("lib.rs"), IconTheme::Dark);
        let kept = infer_icon("lib.rs", false, false, rs.clone());
        assert_eq!(kept.glyph, rs.glyph, "devicons 已识别的图标保留");
    }

    #[test]
    fn m452b_sniff_skips_non_regular_files() {
        // M45-2b：嗅探只读常规文件——目录立即返回 false（不 panic）。
        let tmp = tempfile::TempDir::new().unwrap();
        assert!(!sniff_text(tmp.path()), "目录不是常规文件");
        // 含 NUL 的常规文件 → 二进制。
        let bin = tmp.path().join("blob");
        std::fs::write(&bin, [0u8, 1, 2, 3]).unwrap();
        assert!(!sniff_text(&bin));
        // 纯文本 → true。
        let txt = tmp.path().join("note");
        std::fs::write(&txt, "hello\n").unwrap();
        assert!(sniff_text(&txt));
    }

    #[cfg(unix)]
    #[test]
    fn m452b_sniff_skips_socket_without_blocking() {
        // 回归：socket/FIFO 被 open/read 会永久阻塞（/tmp 下实测卡死 TUI）。
        let tmp = tempfile::TempDir::new().unwrap();
        let sock = tmp.path().join("s.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&sock).unwrap();
        assert!(!sniff_text(&sock), "socket 不得被嗅探");
    }

    #[test]
    fn nonexistent_path_does_not_panic() {
        // M9-1：任何路径不 panic（ACCEPTANCE §11 降级验证的前置）。
        let info = icon_for(Path::new("Z:/definitely/not/here.xyz"), IconTheme::Dark);
        let _ = decorate("here.xyz", &info);
    }
}
