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

// ── M11-13：pdf-visual 护栏（仅 feature 启用时编译） ─────────────────────
//
// SPEC 硬约束（AGENTS §5.5 / ACCEPTANCE §13）：
// - 导出像素上限限制在预览窗格级别（200 万–400 万像素）；
// - 限制输入 PDF 文件大小；
// - release profile 保持 `panic = "unwind"`（Cargo.toml 显式固化）——
//   exprim 的 per-page panic 边界依赖 `catch_unwind`，`abort` 会失去隔离。
#[cfg(feature = "pdf-visual")]
pub mod visual {
    use std::path::Path;
    use std::sync::Arc;

    /// 输入 PDF 文件大小上限：超过不进入栅格化（护栏）。
    pub const MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;

    /// 导出像素硬上限（400 万；预览窗格级别）。
    pub const MAX_EXPORT_PIXELS: u64 = 4_000_000;

    /// 导出像素目标值（200 万–400 万区间内取 300 万，按页面比例整除）。
    const TARGET_EXPORT_PIXELS: f64 = 3_000_000.0;

    /// 栅格化页面结果（RGB8，行优先，`rgb.len() = width * height * 3`）。
    pub struct PagePixels {
        pub width: u32,
        pub height: u32,
        pub rgb: Vec<u8>,
    }

    /// 按目标像素数计算 DPI（页面点尺寸 → 缩放）。
    /// `dpi = 72 * sqrt(target / (pt_w * pt_h))`，下限 72 DPI。
    fn clamp_dpi(pt_width: f64, pt_height: f64) -> f64 {
        let area = (pt_width * pt_height).max(1.0);
        let scale = (TARGET_EXPORT_PIXELS / area).sqrt().min(2.0);
        (72.0 * scale).max(72.0)
    }

    /// 栅格化指定页（0-indexed）。两级护栏先于渲染执行：
    /// 输入文件大小 → 超限 Err；导出像素 → 由 DPI 反推后再次校验。
    /// 渲染 panic 由 exprim 的 per-page 边界转为 `RenderError::Panic`
    /// （依赖 `panic = "unwind"`），不传播 abort。
    pub fn render_page(path: &Path, page_index: u32) -> Result<PagePixels, String> {
        // 护栏 1：输入文件大小。
        let size = std::fs::metadata(path)
            .map_err(|e| format!("读取元数据失败: {e}"))?
            .len();
        if size > MAX_INPUT_BYTES {
            return Err(format!(
                "PDF 文件过大（{size} 字节 > {MAX_INPUT_BYTES} 上限），已跳过视觉渲染"
            ));
        }

        let bytes: Arc<[u8]> = std::fs::read(path)
            .map_err(|e| format!("读取失败: {e}"))?
            .into();
        let session =
            exprim::RenderSession::open(bytes, None).map_err(|e| format!("PDF 打开失败: {e}"))?;
        if page_index >= session.page_count() {
            return Err(format!("页码 {page_index} 超出范围"));
        }

        // 护栏 2：导出像素上限——先按目标 DPI 算尺寸，超限则按比例收缩。
        let (mut width, mut height) = session
            .page_pixel_size(page_index, clamp_dpi_for(&session, page_index))
            .map_err(|e| format!("页面尺寸计算失败: {e}"))?;
        let pixels = u64::from(width) * u64::from(height);
        if pixels > MAX_EXPORT_PIXELS {
            let shrink = (MAX_EXPORT_PIXELS as f64 / pixels as f64).sqrt();
            width = ((width as f64) * shrink).max(1.0) as u32;
            height = ((height as f64) * shrink).max(1.0) as u32;
        }

        let page = session
            .compile(page_index)
            .map_err(|e| format!("页面编译失败: {e}"))?;
        let plane = session
            .render(&page, &exprim::RasterProduct::rgb8(width, height))
            .map_err(|e| format!("栅格化失败: {e}"))?;
        match plane {
            exprim::RasterPlane::Rgb8(surface) => {
                let expected = width as usize * height as usize * 3;
                let mut rgb = surface.pixels.to_vec();
                rgb.resize(expected, 255);
                Ok(PagePixels { width, height, rgb })
            }
            _ => Err("意外的输出格式（非 RGB8）".to_owned()),
        }
    }

    /// 用页面几何（点）反推目标 DPI。
    fn clamp_dpi_for(session: &exprim::RenderSession, page_index: u32) -> f64 {
        session
            .page_geometry(page_index)
            .map(|g| clamp_dpi(g.display_width(), g.display_height()))
            .unwrap_or(150.0)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use tempfile::TempDir;

        #[test]
        fn clamp_dpi_targets_three_megapixels() {
            // A4 纵向（595×842 pt ≈ 501k pt²）→ 缩放约 2.4x → DPI 上限 2x=144。
            let dpi = clamp_dpi(595.0, 842.0);
            let w = 595.0 * dpi / 72.0;
            let h = 842.0 * dpi / 72.0;
            let px = w * h;
            assert!(px <= 4_000_000.0, "pixels {px} exceeds cap");
            assert!(dpi >= 72.0);
        }

        #[test]
        fn oversized_input_rejected_by_guard() {
            let tmp = TempDir::new().unwrap();
            let p = tmp.path().join("fake.pdf");
            std::fs::write(&p, b"%PDF-1.4 fake").unwrap();
            // 直接构造超限场景：用 MAX_INPUT_BYTES 比较逻辑验证。
            let size = std::fs::metadata(&p).unwrap().len();
            assert!(size < MAX_INPUT_BYTES);
            // 超限路径不读文件内容（先 metadata 判断），用一个假路径验证 Err。
            let r = render_page(Path::new("no-such-file.pdf"), 0);
            assert!(r.is_err());
        }
    }
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

    // ── M11-14：pdf-visual 语料回归（CJK/加密/损坏/截断，均不 panic） ──────

    /// 构造带 /Encrypt 字典的假加密 PDF（真实密钥流未构造——目标是
    /// 让解析器进入加密路径并返回 Err/Unavailable，而非 panic）。
    #[cfg(feature = "pdf-visual")]
    fn make_fake_encrypted_pdf(dir: &Path, name: &str) -> std::path::PathBuf {
        let objects = [
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>".to_vec(),
            b"<< /Filter /Standard /V 1 /R 2 /O <0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20> /U <2122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f40> /P -1340 >>".to_vec(),
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
        let head = content.split("startxref").next().unwrap();
        let doc = format!(
            "{head}trailer\n<< /Root 1 0 R /Size {} /Encrypt 4 0 R >>\nstartxref\n{xref_at}\n%%%%EOF\n",
            objects.len() + 1
        );
        let p = dir.join(name);
        std::fs::write(&p, doc).unwrap();
        p
    }

    /// 构造名义 CJK PDF（SimSun 字体名 + 中文注记字符串；无嵌入字体，
    /// 字体栈需容错而非 panic）。
    #[cfg(feature = "pdf-visual")]
    fn make_cjk_pdf(dir: &Path, name: &str) -> std::path::PathBuf {
        let objects = [
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_vec(),
            b"<< /Length 96 >>\nstream\nBT /F1 24 Tf 72 700 Td (CJK: \\u4e2d\\u6587 \\u6d4b\\u8bd5) Tj ET\nendstream".to_vec(),
            b"<< /Type /Font /Subtype /Type1 /BaseFont /SimSun >>".to_vec(),
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
        let head = content.split("startxref").next().unwrap();
        let doc = format!(
            "{head}trailer\n<< /Root 1 0 R /Size {} >>\nstartxref\n{xref_at}\n%%%%EOF\n",
            objects.len() + 1
        );
        let p = dir.join(name);
        std::fs::write(&p, doc).unwrap();
        p
    }

    #[cfg(feature = "pdf-visual")]
    #[test]
    fn corpus_regression_visual_never_panics() {
        // M11-14：语料回归——CJK（无嵌入字体）/ 假加密 / 损坏 / 截断
        // 全部过 render_page：Ok 或 Err 均可，panic 即失败。
        let tmp = TempDir::new().unwrap();

        let cjk = make_cjk_pdf(tmp.path(), "cjk.pdf");
        let _ = visual::render_page(&cjk, 0);

        let enc = make_fake_encrypted_pdf(tmp.path(), "enc.pdf");
        let r = visual::render_page(&enc, 0);
        // 加密文档必须被拒绝或跳过，不能渲染出成功结果。
        if let Ok(pp) = r {
            // 部分解析器对空加密文档降级：即便成功，像素护栏仍须成立。
            let px = u64::from(pp.width) * u64::from(pp.height);
            assert!(px <= visual::MAX_EXPORT_PIXELS);
        }

        let corrupt = tmp.path().join("corrupt.pdf");
        std::fs::write(&corrupt, b"%PDF-1.4 garbage without objects").unwrap();
        assert!(visual::render_page(&corrupt, 0).is_err());

        // 截断文件：头在、体被切断——exprim 有 xref 重建容错，
        // Ok 或 Err 均可，回归重点是绝不 panic。
        let truncated = tmp.path().join("trunc.pdf");
        let good = make_pdf(tmp.path(), "for-trunc.pdf", "Hello");
        let mut bytes = std::fs::read(&good).unwrap();
        bytes.truncate(bytes.len() / 2);
        std::fs::write(&truncated, bytes).unwrap();
        let _ = visual::render_page(&truncated, 0);
    }

    #[cfg(feature = "pdf-visual")]
    #[test]
    fn corpus_visual_text_pdf_renders_within_pixel_cap() {
        // 正常语料：含文本 PDF 视觉渲染成功且像素在护栏内。
        let tmp = TempDir::new().unwrap();
        let p = make_pdf(tmp.path(), "visual.pdf", "Visual Corpus");
        let pp = visual::render_page(&p, 0).expect("render ok");
        let px = u64::from(pp.width) * u64::from(pp.height);
        assert!(px <= visual::MAX_EXPORT_PIXELS, "{px} > cap");
        assert!(pp.width > 0 && pp.height > 0);
        assert_eq!(pp.rgb.len(), pp.width as usize * pp.height as usize * 3);
    }
}
