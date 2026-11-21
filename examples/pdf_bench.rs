//! M31-5：pdf-visual 护栏实测（语料耗时/内存数据）——本机 rustc ≥ 1.97
//! 解锁后执行；结果入 docs/NOTES.md。运行：
//!   cargo run --release --example pdf_bench --features pdf-visual
//!
//! 语料：单页大文本 PDF×5 + CJK（无嵌入字体）+ 损坏文件；每页测
//! `render_page` 耗时与像素量（内存 ≈ 像素×3B RGB）。

use rvifm::config::Config;
use rvifm::tab::load_entries;
use std::path::Path;
use std::time::Instant;

/// 最小单页 PDF 构造（make_pdf 测试助手同构，examples 可见副本）。
fn make_pdf(dir: &Path, name: &str, text: &str) -> std::path::PathBuf {
    let objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_vec(),
        format!(
            "<< /Length {} >>\nstream\nBT /F1 24 Tf 72 700 Td ({text}) Tj ET\nendstream",
            format!("BT /F1 24 Tf 72 700 Td ({text}) Tj ET\n").len()
        )
        .into_bytes(),
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
            "trailer\n<< /Root 1 0 R /Size {} >>\nstartxref\n{}\n%%%%EOF\n",
            objects.len() + 1,
            xref_at
        )
        .bytes(),
    );
    let p = dir.join(name);
    std::fs::write(&p, &buf).unwrap();
    p
}

fn main() {
    let tmp = tempfile::tempdir().unwrap();
    // 语料 1：大文本单页 ×5（每页 ~1.6KB 文本）。
    let mut corpus: Vec<(String, std::path::PathBuf)> = Vec::new();
    for i in 0..5 {
        let p = make_pdf(
            tmp.path(),
            &format!("big{i}.pdf"),
            &format!("PAGE {i} {}", "lorem ipsum corpus ".repeat(80)),
        );
        corpus.push((format!("big{i}.pdf p0"), p));
    }
    corpus.push((
        "cjk.pdf p0".to_owned(),
        make_pdf(tmp.path(), "cjk.pdf", "中文测试内容 漢字テスト 韓国語"),
    ));
    let corrupt = tmp.path().join("corrupt.pdf");
    std::fs::write(&corrupt, b"%PDF-1.4 broken junk no xref").unwrap();

    println!("=== M31-5 pdf-visual 护栏实测 ===");
    println!("环境: Windows Server 2016 / rustc 1.99.1 / release / exprim");
    println!(
        "护栏: 导出像素 ≤ {}（超限自动缩放）",
        rvifm::pdf::visual::MAX_EXPORT_PIXELS
    );

    let mut total_ms = 0u128;
    for (label, path) in &corpus {
        let t = Instant::now();
        match rvifm::pdf::visual::render_page(path, 0) {
            Ok(pp) => {
                let ms = t.elapsed().as_millis();
                total_ms += ms;
                println!(
                    "{label}: {ms} ms  {}x{}px  内存≈{} B",
                    pp.width,
                    pp.height,
                    pp.width as u64 * pp.height as u64 * 3
                );
            }
            Err(e) => println!("{label}: {} ms  ERR {e}", t.elapsed().as_millis()),
        }
    }
    let t = Instant::now();
    let r = rvifm::pdf::visual::render_page(&corrupt, 0);
    let ms = t.elapsed().as_millis();
    println!(
        "corrupt.pdf p0: {ms} ms  {}",
        match r {
            Ok(pp) => format!("{}x{}px（不应发生）", pp.width, pp.height),
            Err(e) => format!("拒绝（不 panic）: {e}"),
        }
    );
    println!("正常语料合计: {total_ms} ms（6 页）");
    let _ = load_entries(Path::new("."), &Config::default());
}
