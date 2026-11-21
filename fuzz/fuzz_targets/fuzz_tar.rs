#![no_main]
//! M40-2：tar.gz 解析 fuzz（flate2 解压 + tar 条目遍历）。
use libfuzzer_sys::fuzz_target;

mod common;

fuzz_target!(|data: &[u8]| {
    common::drive_archive(".tar.gz", data);
});
