//! 语法高亮（M21-E3）：syntect 5（fancy-regex 纯 Rust 后端，无 onig C 依赖）。
//!
//! 预览查看器按**可见行**逐行解析（每帧新建 `HighlightLines`——视口起点
//! 可能处于多行注释中间，跨行状态从头重建是 SIMPLIFIED 取舍）。语法按
//! 扩展名 → 首行 shebang 链查找（syntect 内建 100+ 语言与 Plain Text），
//! 未命中返回 `None` 由调用方降级纯文本。主题随 opaline 明暗对齐。
//!
//! 消费方：TUI 查看器（render_right_pane 文本分支）；搜索模式跳过高亮
//! （查询子串黄底叠加与语法着色互斥，SIMPLIFIED）。

use std::path::Path;
use std::sync::OnceLock;

use ratatui::style::{Color as RtColor, Modifier as RtModifier, Style as RtStyle};
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

static SYNTAX: OnceLock<SyntaxSet> = OnceLock::new();
static THEMES: OnceLock<ThemeSet> = OnceLock::new();

fn syntax_set() -> &'static SyntaxSet {
    SYNTAX.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme_set() -> &'static ThemeSet {
    THEMES.get_or_init(ThemeSet::load_defaults)
}

/// 主题随 opaline 明暗对齐（内置默认主题集；空集不可达——load_defaults
/// 恒含 base16-ocean 系列）。AUDIT-8：去 expect——理论不可达（空集）
/// 兜底为泄漏的空主题（无着色，不 panic）。
fn theme(dark: bool) -> &'static Theme {
    let ts = theme_set();
    ts.themes
        .get(if dark {
            "base16-ocean.dark"
        } else {
            "base16-ocean.light"
        })
        .or_else(|| ts.themes.values().next())
        .unwrap_or_else(|| {
            // 空主题集兜底：无着色的空主题（泄漏一次，进程级静态）。
            Box::leak(Box::new(Theme {
                name: None,
                author: None,
                settings: syntect::highlighting::ThemeSettings::default(),
                scopes: Vec::new(),
            }))
        })
}

/// 查找语法：首行 shebang 优先（`.txt` 内容推断，对齐 syntect
/// `find_syntax_for_file` 语义），扩展名兜底；未命中 None。
fn find_syntax(path: &Path, first_line: &str) -> Option<&'static SyntaxReference> {
    let ss = syntax_set();
    ss.find_syntax_by_first_line(first_line).or_else(|| {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default();
        ss.find_syntax_by_extension(&ext)
    })
}

/// 逐行高亮器（每帧新建；无语法时 `for_path` 返回 None → 调用方纯文本）。
pub struct Highlighter {
    hl: HighlightLines<'static>,
}

impl Highlighter {
    /// 按路径推断语法并创建高亮器；推断失败返回 None（纯文本降级）。
    pub fn for_path(path: &Path, first_line: &str, dark: bool) -> Option<Self> {
        let syntax = find_syntax(path, first_line)?;
        Some(Self {
            hl: HighlightLines::new(syntax, theme(dark)),
        })
    }

    /// 高亮一行 → (片段文本, ratatui Style) 列表。行内容不含行尾换行
    /// （内部补 `\n` 解析并从末片段裁掉）；跨行状态由内部逐行推进。
    pub fn highlight_line(&mut self, line: &str) -> Vec<(String, RtStyle)> {
        let ss = syntax_set();
        let owned = format!("{line}\n");
        // fancy-regex 后端解析失败 → 空列表（调用方降级纯文本行）。
        let Ok(regions) = self.hl.highlight_line(&owned, ss) else {
            return Vec::new();
        };
        regions
            .into_iter()
            .filter_map(|(style, text)| {
                let text = text.trim_end_matches('\n');
                if text.is_empty() {
                    return None;
                }
                Some((text.to_owned(), to_rt_style(style)))
            })
            .collect()
    }
}

/// 共享文本高亮入口：路径推断语法 → 逐行着色，供 TUI 查看器与 `rvifm pick`
/// 预览共用（避免 pick 出现"有预览但无语法高亮"的分叉）。无语法返回 None。
pub fn styled_lines(
    path: &Path,
    lines: &[String],
    dark: bool,
) -> Option<Vec<ratatui::text::Line<'static>>> {
    let first = lines.first().map(String::as_str).unwrap_or("");
    let mut highlighter = Highlighter::for_path(path, first, dark)?;
    Some(
        lines
            .iter()
            .map(|line| {
                let spans: Vec<_> = highlighter
                    .highlight_line(line)
                    .into_iter()
                    .map(|(text, style)| ratatui::text::Span::styled(text, style))
                    .collect();
                ratatui::text::Line::from(spans)
            })
            .collect(),
    )
}

/// syntect Style → ratatui Style（fg 着色 + 粗体/斜体）。
fn to_rt_style(style: syntect::highlighting::Style) -> RtStyle {
    let c = style.foreground;
    let mut rt = RtStyle::new().fg(RtColor::Rgb(c.r, c.g, c.b));
    if style.font_style.contains(FontStyle::BOLD) {
        rt = rt.add_modifier(RtModifier::BOLD);
    }
    if style.font_style.contains(FontStyle::ITALIC) {
        rt = rt.add_modifier(RtModifier::ITALIC);
    }
    rt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_syntax_by_extension_and_shebang() {
        // .rs 按扩展名命中；.txt 首行 shebang 推断为 shell；纯文本降级。
        assert!(find_syntax(Path::new("a/main.rs"), "").is_some(), "rs 命中");
        let py = find_syntax(Path::new("a/script.py"), "").unwrap();
        let sh = find_syntax(Path::new("a/notes.txt"), "#!/bin/sh\necho hello").unwrap();
        assert_ne!(py.name, sh.name, "shebang 推断与扩展名推断不同源");
        assert!(find_syntax(Path::new("a/data.bincode"), "randombytes").is_none());
    }

    #[test]
    fn highlight_line_colors_rust_tokens() {
        let mut hl = Highlighter::for_path(Path::new("m.rs"), "fn main() {}", true).unwrap();
        let spans = hl.highlight_line("fn main() { let x = 1; }");
        assert!(!spans.is_empty(), "产生着色片段");
        // 至少一个片段带非默认前景色（Rust 关键字/函数着色）。
        assert!(
            spans
                .iter()
                .any(|(_, s)| matches!(s.fg, Some(RtColor::Rgb(_, _, _)))),
            "存在 RGB 着色片段"
        );
        // 拼接还原原文（无换行残留）。
        let joined: String = spans.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(joined, "fn main() { let x = 1; }");
    }

    #[test]
    fn m591_shared_styled_lines_for_pick_and_tui() {
        // M59-1：pick 与 TUI 共享的着色入口返回 ratatui 行（非纯文本）。
        let lines = vec!["fn main() {".to_owned(), "}".to_owned()];
        let styled = styled_lines(Path::new("demo.rs"), &lines, true).expect("rust 语法应命中");
        assert_eq!(styled.len(), 2);
        let text: String = styled[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "fn main() {");
    }

    #[test]
    fn highlight_line_multiline_state_progresses() {
        // 块注释跨行：状态随行推进，注释行整体着色。
        let mut hl = Highlighter::for_path(Path::new("m.rs"), "// x", true).unwrap();
        let spans = hl.highlight_line("/* start");
        assert!(!spans.is_empty());
        let spans = hl.highlight_line("end */ fn a() {}");
        assert!(!spans.is_empty());
    }
}
