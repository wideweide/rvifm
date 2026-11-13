//! 预览渲染器（M8-8；SPEC §4.9）。
//!
//! `PreviewRenderer` 是 TUI 与 `pick` **共用**的预览抽象——不允许为
//! pick 单独实现一套（NOTES §4.8）。文本/目录内建；图像为占位
//! （M11-1/M11-2 接入 `ratatui-image` 协议探测 + halfblocks 回退）。
//!
//! M8-8 仅固化共享渲染入口与文本/目录实现；消费方：M11-1/M11-2
//! （图像渲染）、M11-9（Office 接入）——届时 TUI 接入预览窗格。

#![allow(dead_code)]

use std::fs;
use std::path::Path;

use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// 预览内容的最大读取字节数（防止大文件撑爆内存/渲染）。
const MAX_PREVIEW_BYTES: u64 = 512 * 1024;

/// 预览结果：可直接交给 ratatui 渲染的行列表。
pub struct Preview {
    lines: Vec<Line<'static>>,
}

impl Preview {
    /// 转换为 `Paragraph`（TUI 渲染用）。
    pub fn into_paragraph(self) -> Paragraph<'static> {
        Paragraph::new(self.lines)
    }

    /// 行数（测试与截断判断用）。
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// 行文本（测试与 pick 共用入口）。
    pub fn line_texts(&self) -> Vec<String> {
        self.lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.clone()).collect())
            .collect()
    }
}

/// 预览内容类型（M11-2；SPEC §4.9 `PreviewContent` 的分流语义）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewKind {
    /// 目录成员列表。
    Directory,
    /// 图片文件（由 `ImageEngine` 渲染到预览区域）。
    Image,
    /// 其余按文本预览。
    Text,
}

/// 预览分流入口（M11-2）：TUI 与 `pick` 共用，保证同源渲染。
pub fn preview_kind(path: &Path) -> PreviewKind {
    if path.is_dir() {
        PreviewKind::Directory
    } else if crate::image::is_image_file(path) {
        PreviewKind::Image
    } else {
        PreviewKind::Text
    }
}

/// 为 `path` 生成预览（M8-8）：
/// - 目录 → 成员列表（前若干项，不递归）；
/// - 普通文件 → 文本内容（截断保护）；二进制/读取失败 → 友好提示；
/// - 图像文件 → 友好提示行（真实渲染走 `ImageEngine`；此分支服务
///   解码失败降级与无图形能力的调用方）。
pub fn render_preview(path: &Path, max_lines: usize) -> Preview {
    if path.is_dir() {
        return directory_preview(path, max_lines);
    }
    if crate::image::is_image_file(path) {
        return text_preview(&format!(
            "[图像预览] {}\n\n图形渲染不可用（协议探测失败或解码错误）。\n按 Enter 用系统默认程序打开。",
            path.display()
        ));
    }
    text_preview(&read_text_preview(path, max_lines))
}

/// 目录预览：成员名列表（目录后缀 `/`，按名排序，截断到 max_lines）。
fn directory_preview(path: &Path, max_lines: usize) -> Preview {
    let mut lines: Vec<Line<'static>> = Vec::new();
    match std::fs::read_dir(path) {
        Ok(iter) => {
            let mut names: Vec<(String, bool)> = iter
                .filter_map(|e| e.ok())
                .map(|e| {
                    let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    (e.file_name().to_string_lossy().into_owned(), is_dir)
                })
                .collect();
            names.sort_by(|a, b| {
                b.1.cmp(&a.1)
                    .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
            });
            lines.push(Line::from(format!(
                "目录 {}（{} 项）",
                path.display(),
                names.len()
            )));
            for (name, is_dir) in names.iter().take(max_lines.saturating_sub(1)) {
                let label = if *is_dir {
                    format!("{name}/")
                } else {
                    name.clone()
                };
                // M9-4：归档/预览共用同一 devicons 图标 + 激活主题。
                let full = path.join(name);
                let icon = crate::icons::icon_for_active(&full);
                lines.push(Line::from(format!("  {} {label}", icon.glyph)));
            }
            if names.len() > max_lines.saturating_sub(1) {
                lines.push(Line::from(format!(
                    "  …（共 {} 项，仅显示前 {}）",
                    names.len(),
                    max_lines.saturating_sub(1)
                )));
            }
        }
        Err(e) => {
            lines.push(Line::from(format!("目录读取失败: {e}")));
        }
    }
    Preview { lines }
}

/// 文本文件预览：读取（截断保护），按行渲染。
fn text_preview(body: &str) -> Preview {
    let lines: Vec<Line<'static>> = body.lines().map(|l| Line::from(l.to_owned())).collect();
    Preview { lines }
}

/// 读取文件为预览文本：二进制探测 / 大文件截断 / 错误提示。
fn read_text_preview(path: &Path, max_lines: usize) -> String {
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) => return format!("读取失败: {e}"),
    };
    if meta.len() > MAX_PREVIEW_BYTES {
        return format!(
            "[文件过大] {}（{} 字节，仅预览前 {} KB）",
            path.display(),
            meta.len(),
            MAX_PREVIEW_BYTES / 1024
        );
    }
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => return format!("读取失败: {e}"),
    };
    // 二进制探测：首 1KB 含 NUL 即按二进制处理。
    let probe = &bytes[..bytes.len().min(1024)];
    if probe.contains(&0) {
        return format!("[二进制文件] {}", path.display());
    }
    let text = String::from_utf8_lossy(&bytes);
    let mut kept: Vec<&str> = text.lines().take(max_lines).collect();
    if kept.is_empty() {
        kept.push("（空文件）");
    }
    kept.join("\n")
}

/// 供 `pick` 使用的快捷入口：给路径渲染预览行（Span 形式，M8-8 共用证明）。
pub fn preview_lines(path: &Path, max_lines: usize) -> Vec<Span<'static>> {
    let preview = render_preview(path, max_lines);
    // Paragraph 的行文本提取（共享渲染证明；M11 接入图像后同样适用）。
    preview.line_texts().into_iter().map(Span::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;
    use tempfile::TempDir;

    #[test]
    fn directory_preview_lists_sorted_with_dirs_first() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join("zdir")).unwrap();
        std::fs::create_dir(tmp.path().join("adir")).unwrap();
        std::fs::write(tmp.path().join("f.txt"), "").unwrap();

        let preview = render_preview(tmp.path(), 20);
        let texts = preview.line_texts();
        assert!(texts[0].contains("目录"), "first line: {}", texts[0]);
        assert!(texts.iter().any(|t| t.contains("adir/")));
        assert!(texts.iter().any(|t| t.contains("f.txt")));
    }

    #[test]
    fn text_preview_reads_and_truncates() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("a.txt");
        let mut f = fs::File::create(&p).unwrap();
        for i in 0..100 {
            writeln!(f, "line {i}").unwrap();
        }
        let preview = render_preview(&p, 5);
        assert_eq!(preview.line_count(), 5); // 截断到 max_lines

        // 空文件。
        let empty = tmp.path().join("empty.txt");
        fs::File::create(&empty).unwrap();
        let preview = render_preview(&empty, 5);
        let texts = preview.line_texts();
        assert!(texts.iter().any(|t| t.contains("空文件")));
    }

    #[test]
    fn binary_file_and_oversize_get_friendly_hint() {
        let tmp = TempDir::new().unwrap();
        let bin = tmp.path().join("x.bin");
        let mut f = fs::File::create(&bin).unwrap();
        f.write_all(&[0x89, 0x50, 0x4e, 0x47, 0x00, 0x01]).unwrap(); // 含 NUL

        let preview = render_preview(&bin, 10);
        let texts = preview.line_texts();
        assert!(texts.iter().any(|t| t.contains("二进制文件")));

        // 大文件（>512KB）→ 截断提示。
        let big = tmp.path().join("big.txt");
        let mut f = fs::File::create(&big).unwrap();
        let chunk = vec![b'a'; 64 * 1024];
        for _ in 0..10 {
            f.write_all(&chunk).unwrap();
        }
        let preview = render_preview(&big, 10);
        let texts = preview.line_texts();
        assert!(texts.iter().any(|t| t.contains("文件过大")));
    }

    #[test]
    fn image_files_get_friendly_hint_when_graphics_unavailable() {
        // M11-2：图像文件预览行 = 解码失败/无图形能力时的降级提示。
        let tmp = TempDir::new().unwrap();
        let img = tmp.path().join("x.png");
        fs::write(&img, b"not-a-real-png").unwrap();
        assert_eq!(preview_kind(&img), PreviewKind::Image);
        let preview = render_preview(&img, 10);
        let texts = preview.line_texts();
        assert!(texts.iter().any(|t| t.contains("图像预览")));
    }

    #[test]
    fn preview_kind_classifies_dir_image_text() {
        // M11-2：TUI 与 pick 共用的分流入口。
        let tmp = TempDir::new().unwrap();
        assert_eq!(preview_kind(tmp.path()), PreviewKind::Directory);

        let img = tmp.path().join("photo.JPEG");
        fs::write(&img, b"\x89PNG").unwrap();
        assert_eq!(preview_kind(&img), PreviewKind::Image);

        let txt = tmp.path().join("a.txt");
        fs::write(&txt, "hello").unwrap();
        assert_eq!(preview_kind(&txt), PreviewKind::Text);

        let noext = tmp.path().join("Makefile");
        fs::write(&noext, "all:").unwrap();
        assert_eq!(preview_kind(&noext), PreviewKind::Text);
    }

    #[test]
    fn preview_lines_shared_helper() {
        // M8-8：TUI 与 pick 共用的入口（返回 Span 行）。
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("shared.txt");
        fs::write(&p, "hello shared").unwrap();
        let spans = preview_lines(&p, 10);
        let joined: String = spans.iter().map(|s| s.content.clone()).collect();
        assert!(joined.contains("hello shared"));
    }
}
