//! M40-2：归档 fuzz 公共驱动——fuzzed bytes 落临时文件（指定扩展名决定
//! open_dispatch 分派）→ 虚拟浏览（list 有界 + 首成员读取；AUDIT-17 成员
//! 上限护栏内）。崩溃/挂起/OOM 即 fuzz 发现。

use std::io::Write;

/// 归档解析 fuzz 驱动（ext 决定分派；bounded = list 条目上限与读取数）。
pub fn drive_archive(ext: &str, data: &[u8]) {
    let dir = std::env::temp_dir().join(format!(
        "rvifm-fuzz-{}-{}",
        std::process::id(),
        ext.trim_start_matches('.')
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(format!("f{ext}"));
    let mut f = std::fs::File::create(&p).unwrap();
    f.write_all(data).unwrap();
    drop(f);
    if let Ok(mut fsx) = rvifm::archive::open_dispatch(&p) {
        if let Ok(entries) = fsx.list("/") {
            // 有界：仅读取前 1 个非目录成员（read 走 AUDIT-17 上限护栏）。
            for e in entries.iter().take(1) {
                if !e.is_dir {
                    let _ = fsx.read(&format!("/{}", e.path));
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
