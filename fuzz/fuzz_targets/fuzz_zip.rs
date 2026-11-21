#![no_main]
//! M40-2：zip 解析 fuzz（含 .jar/.docx 同类容器路径）。
use libfuzzer_sys::fuzz_target;

mod common;

fuzz_target!(|data: &[u8]| {
    common::drive_archive(".zip", data);
});
