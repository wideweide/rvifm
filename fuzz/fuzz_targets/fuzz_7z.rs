#![no_main]
//! M40-2：7z 解析 fuzz（sevenz-rust 解码路径）。
use libfuzzer_sys::fuzz_target;

mod common;

fuzz_target!(|data: &[u8]| {
    common::drive_archive(".7z", data);
});
