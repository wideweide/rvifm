//! rvifm 库入口（M40-1）：模块树对二进制与 criterion 基准共用——
//! `benches/` 通过本 lib 导入内部函数做性能基准（冷启动路径/10 万项
//! 加载/自然排序/frecency 重放/1GB 复制吞吐，见 SPEC M40-1）。
//!
//! 模块清单与原 main.rs 的 `mod` 声明一致（lib 化是纯搬移：模块内
//! `crate::` 路径在 lib 根同样成立；二进制 `main.rs` 改为 `use rvifm::`）。

pub mod app;
pub mod archive;
pub mod config;
pub mod frecency;
pub mod git;
pub mod gitdiff;
pub mod highlight;
pub mod icons;
pub mod image;
pub mod ime;
pub mod keymap;
pub mod office;
pub mod opener;
pub mod ops;
pub mod pdf;
pub mod preview;
pub mod privilege;
pub mod shell_history;
pub mod shell_init;
pub mod viewport;
// M19-2/M19-3b 接入导航与后端；本阶段仅 trait 层（测试使用）。
pub mod remote;
pub mod search;
pub mod session;
pub mod state;
pub mod store;
pub mod tab;
pub mod theme;
pub mod tutor;
