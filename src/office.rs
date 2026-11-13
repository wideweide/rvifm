//! Office 预览提取（M11-8；SPEC §4.9）。
//!
//! 纯 Rust 解析库 `office_oxide`（~0.1.16 锁定，ACCEPTANCE §13）：
//! DOCX/XLSX/PPTX 及 legacy DOC/XLS/PPT 的文本提取，零外部工具依赖。
//! Office 预览只做文本提取，不保证格式还原（SPEC SIMPLIFIED）；
//! 复杂对象（图表/SmartArt/OLE）被库跳过且不 panic。
//!
//! M11-8 固化提取层；消费方：M11-9（PreviewRenderer 分支接入）。

// M11-8 提取层先行；PreviewRenderer 分支在 M11-9 接入。
#![allow(dead_code)]

use std::path::Path;

/// Office 文件扩展名（小写比较；含 legacy 二进制格式）。
const OFFICE_EXTENSIONS: &[&str] = &["docx", "xlsx", "pptx", "doc", "xls", "ppt"];

/// 预览文本的最大提取字符数（保护超大文档的渲染）。
const MAX_PREVIEW_CHARS: usize = 64 * 1024;

/// 是否为可提取文本的 Office 文件（按扩展名判断）。
pub fn is_office_file(path: &Path) -> bool {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .map(|e| OFFICE_EXTENSIONS.contains(&e.as_str()))
        .unwrap_or(false)
}

/// 提取 Office 文档的预览文本（按行截断到 `max_lines`）。
/// 解析失败返回 Err，由调用方降级为提示行，不 panic。
pub fn extract_preview_text(path: &Path, max_lines: usize) -> std::io::Result<String> {
    let text = office_oxide::extract_text(path)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    // 超大文档截断（字符级），避免预览窗格渲染被撑爆。
    let text: String = text.chars().take(MAX_PREVIEW_CHARS).collect();
    let mut kept: Vec<&str> = text.lines().take(max_lines).collect();
    if kept.is_empty() {
        kept.push("（空文档）");
    }
    Ok(kept.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// 程序化生成最小 Office fixture（office_oxide 自身 API，无二进制 fixture）。
    fn make_doc(dir: &Path, name: &str, md: &str) -> std::path::PathBuf {
        let fmt = match name.rsplit('.').next().unwrap() {
            "xlsx" => office_oxide::format::DocumentFormat::Xlsx,
            "pptx" => office_oxide::format::DocumentFormat::Pptx,
            _ => office_oxide::format::DocumentFormat::Docx,
        };
        office_oxide::create::create_from_markdown(md, fmt, dir.join(name)).unwrap();
        dir.join(name)
    }

    #[test]
    fn is_office_file_matches_extension_case_insensitive() {
        assert!(is_office_file(Path::new("a/report.docx")));
        assert!(is_office_file(Path::new("book.XLSX")));
        assert!(is_office_file(Path::new("old.doc")));
        assert!(is_office_file(Path::new("slides.pptx")));
        assert!(!is_office_file(Path::new("notes.txt")));
        assert!(!is_office_file(Path::new("photo.png")));
        assert!(!is_office_file(Path::new("dir.zip")));
    }

    #[test]
    fn extract_preview_text_reads_docx() {
        let tmp = TempDir::new().unwrap();
        let p = make_doc(
            tmp.path(),
            "hello.docx",
            "# 标题\n\n正文第一行。\n正文第二行。",
        );
        let text = extract_preview_text(&p, 20).unwrap();
        assert!(text.contains("标题"), "docx text: {text}");
        assert!(text.contains("正文第一行"));
    }

    #[test]
    fn extract_preview_text_reads_xlsx_and_pptx() {
        let tmp = TempDir::new().unwrap();
        let xlsx = make_doc(tmp.path(), "sheet.xlsx", "Hello Cell");
        let t1 = extract_preview_text(&xlsx, 20).unwrap();
        assert!(t1.contains("Hello Cell"), "xlsx text: {t1}");

        let pptx = make_doc(tmp.path(), "deck.pptx", "# 幻灯片\n\n要点内容");
        let t2 = extract_preview_text(&pptx, 20).unwrap();
        assert!(t2.contains("幻灯片"), "pptx text: {t2}");
    }

    #[test]
    fn extract_preview_text_truncates_lines() {
        let tmp = TempDir::new().unwrap();
        // 多段落文档（空行分隔才是独立段落，软换行会被合并）截断到 max_lines。
        let md: String = (0..100).map(|i| format!("line {i}\n\n")).collect();
        let p = make_doc(tmp.path(), "long.docx", &md);
        let text = extract_preview_text(&p, 5).unwrap();
        assert_eq!(text.lines().count(), 5);
    }

    #[test]
    fn extract_preview_text_corrupt_file_errors_without_panic() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("bad.docx");
        std::fs::write(&p, b"not a real docx").unwrap();
        assert!(extract_preview_text(&p, 10).is_err());
    }
}
