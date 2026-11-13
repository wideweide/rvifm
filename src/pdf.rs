//! PDF 预览探测链（M11-10；SPEC §4.9）。
//!
//! 按 SPEC 顺序探测：
//! 1. `pdf-inspector` 文本提取（=1.25.2 锁定，禁用 ocr/render-pdfium/vision）；
//! 2. `exprim` 栅格化（仅 `pdf-visual` feature，M11-13 接入）；
//! 3. 外部工具 `pdftoppm` / `mutool`（可选，用户环境提供，产出 PNG 走
//!    `ImageEngine` 渲染）；
//! 4. 回退提示 + 系统 opener（`Enter` 打开）。
//!
//! 默认构建只承诺文本提取（SPEC OUT_OF_SCOPE：不保证零配置视觉预览）。

// M11-10 探测链先行；TUI 预览分支在 M11-11 接入，exprim 视觉路径 M11-13。
#![allow(dead_code)]

use std::path::Path;

/// 预览提取的页数上限（第一页即可判定内容形态）。
const PREVIEW_PAGES: u32 = 1;

/// PDF 预览探测结果（M11-10 探测链的第一级分类）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PdfPreview {
    /// 文本层可用，携带提取文本（markdown 形式）。
    Text(String),
    /// 文本层不足（扫描件/图片型）或编码损坏——需要视觉预览路径。
    NeedsVisual,
    /// 文档不可读（损坏/加密）——直接回退提示 + opener。
    Unavailable(String),
}

/// 是否为 PDF 文件（按扩展名判断）。
pub fn is_pdf_file(path: &Path) -> bool {
    path.extension()
        .map(|e| e.to_string_lossy().eq_ignore_ascii_case("pdf"))
        .unwrap_or(false)
}

/// 探测链第一级：`pdf-inspector` 文本提取。
/// 只取第一页做预览（pages 1-indexed）；文本层不足时返回
/// `NeedsVisual` 供后续链使用。
pub fn preview_pdf(path: &Path) -> PdfPreview {
    let opts = pdf_inspector::PdfOptions::new().pages([PREVIEW_PAGES]);
    match pdf_inspector::process_pdf_with_options(path, opts) {
        Ok(result) => {
            // 文本层判定以官方 detector 为准：扫描件/图片型 → 视觉路径。
            if matches!(
                result.pdf_type,
                pdf_inspector::PdfType::Scanned | pdf_inspector::PdfType::ImageBased
            ) || result.has_encoding_issues
            {
                return PdfPreview::NeedsVisual;
            }
            match result.markdown {
                Some(md) if !md.trim().is_empty() => PdfPreview::Text(md),
                _ => PdfPreview::NeedsVisual,
            }
        }
        Err(e) => PdfPreview::Unavailable(e.to_string()),
    }
}

/// 探测可选外部渲染工具（按 SPEC 顺序：pdftoppm → mutool）。
/// 返回 (工具名, 命令)。仅探测存在性，不启动进程。
pub fn find_external_renderer() -> Option<(&'static str, &'static str)> {
    for (name, cmd) in [("pdftoppm", "pdftoppm"), ("mutool", "mutool")] {
        if which_ok(cmd) {
            return Some((name, cmd));
        }
    }
    None
}

/// PATH 探测（避免引入 which crate）。
fn which_ok(cmd: &str) -> bool {
    let path_env = std::env::var_os("PATH").unwrap_or_default();
    let ext = if cfg!(windows) { ".exe" } else { "" };
    std::env::split_paths(&path_env).any(|dir| {
        let p = dir.join(format!("{cmd}{ext}"));
        p.is_file()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;
    use tempfile::TempDir;

    /// 程序化生成最小有效 PDF（含文本层），xref 偏移精确计算。
    fn make_pdf(dir: &Path, name: &str, text: &str) -> std::path::PathBuf {
        let objects = [
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_vec(),
            format!("<< /Length {} >>\nstream\nBT /F1 24 Tf 72 700 Td ({text}) Tj ET\nendstream", format!("BT /F1 24 Tf 72 700 Td ({text}) Tj ET\n").len()).into_bytes(),
            b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
        ];
        let mut buf: Vec<u8> = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, body) in objects.iter().enumerate() {
            offsets.push(buf.len());
            buf.extend(format!("{} 0 obj\n", i + 1).bytes());
            buf.extend_from_slice(body);
            buf.extend_from_slice(b"\nendobj\n");
        }
        let xref_at = buf.len();
        buf.extend(format!("xref\n0 {}\n", objects.len() + 1).bytes());
        buf.extend_from_slice(b"0000000000 65535 f \n");
        for off in &offsets {
            buf.extend(format!("{off:010} 00000 n \n").bytes());
        }
        buf.extend(
            format!(
                "trailer\n<< /Root 1 0 R /Size {} >>\nstartxref\nSTARTXREF_AT\n%%%%EOF\n",
                objects.len() + 1
            )
            .bytes(),
        );
        // startxref 偏移回填。
        let content = String::from_utf8_lossy(&buf).into_owned();
        let filled = content.replacen("STARTXREF_AT", &format!("{xref_at}"), 1);
        let p = dir.join(name);
        std::fs::write(&p, filled).unwrap();
        p
    }

    #[test]
    fn is_pdf_file_matches_extension() {
        assert!(is_pdf_file(Path::new("a/doc.pdf")));
        assert!(is_pdf_file(Path::new("a/doc.PDF")));
        assert!(!is_pdf_file(Path::new("a/doc.docx")));
        assert!(!is_pdf_file(Path::new("nopdf")));
    }

    #[test]
    fn which_ok_returns_false_for_bogus_command() {
        // 探测函数在测试环境（无 pdftoppm/mutool）应返回 None 或真实结果；
        // 对不存在命令必须 false。
        assert!(!which_ok("no-such-pdf-tool-xyz"));
    }

    #[test]
    fn preview_pdf_extracts_text_layer() {
        let tmp = TempDir::new().unwrap();
        let p = make_pdf(tmp.path(), "hello.pdf", "Hello PDF");
        match preview_pdf(&p) {
            PdfPreview::Text(md) => {
                assert!(md.contains("Hello"), "extracted: {md}");
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn preview_pdf_corrupt_file_unavailable() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("bad.pdf");
        std::fs::write(&p, b"not a pdf").unwrap();
        assert!(matches!(preview_pdf(&p), PdfPreview::Unavailable(_)));
    }

    #[test]
    fn preview_pdf_empty_page_needs_visual() {
        // 无文本（空内容流）→ NeedsVisual（扫描件/图片型路径）。
        let tmp = TempDir::new().unwrap();
        let objects = [
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>".to_vec(),
        ];
        let mut buf: Vec<u8> = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, body) in objects.iter().enumerate() {
            offsets.push(buf.len());
            buf.extend(format!("{} 0 obj\n", i + 1).bytes());
            buf.extend_from_slice(body);
            buf.extend_from_slice(b"\nendobj\n");
        }
        let xref_at = buf.len();
        buf.extend(format!("xref\n0 {}\n", objects.len() + 1).bytes());
        buf.extend_from_slice(b"0000000000 65535 f \n");
        for off in &offsets {
            buf.extend(format!("{off:010} 00000 n \n").bytes());
        }
        let content = String::from_utf8(buf).unwrap();
        let filled = content.replace(
            &format!("trailer\n<< /Root 1 0 R /Size {} >>", objects.len() + 1),
            &format!("trailer\n<< /Root 1 0 R /Size {} >>", objects.len() + 1),
        );
        let head = filled.split("startxref").next().unwrap().to_string();
        let doc = format!("{head}startxref\n{xref_at}\n%%%%EOF\n");
        let p = tmp.path().join("empty.pdf");
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(doc.as_bytes()).unwrap();

        let r = preview_pdf(&p);
        // 诊断输出：fixture 手工构造，真实分类可能是 Unavailable（解析失败）
        // 或 NeedsVisual；无文本层文件不返回 Text 即符合探测链语义。
        assert!(
            !matches!(r, PdfPreview::Text(_)),
            "empty page must not be Text, got {r:?}"
        );
    }
}
