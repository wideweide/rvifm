#![no_main]
//! M40-2：路径规范化 fuzz（归档 inner 规范化 + :cd 词法规范化）。
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = rvifm::archive::normalize_inner(s);
        let _ = rvifm::app::exec::normalize_lexical(std::path::Path::new(s));
    }
});
