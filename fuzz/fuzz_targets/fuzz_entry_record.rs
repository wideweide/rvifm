#![no_main]
//! M40-2：EntryRecord 反序列化 fuzz（redb events 载荷同形态）。
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = serde_json::from_slice::<rvifm::frecency::EntryRecord>(data);
});
