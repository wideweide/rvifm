//! M25-1：会话持久化——TUI 退出保存、启动恢复（TOML）。
//!
//! 仅持久化**物理 Location** 的 tab（左 pane Location + 光标 + `..` 行焦点 +
//! 焦点 pane）与活动 tab 索引；归档/远端视图不保存（SPEC SIMPLIFIED——远端
//! 会话需重认证、归档属临时浏览态）。文件缺失/损坏/空 → 静默新会话；
//! `[ui] session_restore = false` 时不读不写。无备份/迁移机制（会话文件
//! 属可丢状态）。

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::state::AppState;
use crate::tab::Location;

/// 会话文件结构（配置目录 `session.toml`）。
#[derive(Debug, Serialize, Deserialize, Default, PartialEq)]
struct SessionFile {
    active: usize,
    tabs: Vec<SessionTab>,
}

/// 单个 tab 的恢复数据（左 pane 语义）。
#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct SessionTab {
    path: String,
    cursor: usize,
    on_parent_row: bool,
    /// "left" / "right"（M14-1 焦点 pane）。
    focus: String,
}

/// 会话文件路径（与 config.toml 同目录）。
pub fn session_path() -> PathBuf {
    let cfg = crate::config::config_path();
    match cfg.parent() {
        Some(dir) => dir.join("session.toml"),
        None => PathBuf::from("session.toml"),
    }
}

/// 收集可持久化 tab 并序列化为 TOML 文本（仅物理 Location）。
pub fn serialize_session(state: &AppState) -> String {
    let file = SessionFile {
        active: state.active,
        tabs: state
            .tabs
            .iter()
            .filter_map(|tab| {
                let Location::Physical(p) = &tab.location else {
                    return None; // 归档/远端不保存
                };
                Some(SessionTab {
                    path: p.to_string_lossy().into_owned(),
                    cursor: tab.cursor,
                    on_parent_row: tab.on_parent_row,
                    focus: match tab.focus {
                        crate::tab::Focus::Right => "right".to_owned(),
                        crate::tab::Focus::Left => "left".to_owned(),
                    },
                })
            })
            .collect(),
    };
    toml::to_string(&file).unwrap_or_default()
}

/// 退出保存（GAP-2 同模式：失败 eprintln 不阻塞退出）；
/// `session_restore = false` 时不写。
pub fn save_session(state: &AppState) {
    if !state.config.ui.session_restore {
        return;
    }
    let text = serialize_session(state);
    if let Err(e) = fs::write(session_path(), text) {
        eprintln!("rvifm: 会话保存失败: {e}");
    }
}

/// 解析会话文本（缺失/损坏/空 → None）。
fn parse_session(text: &str) -> Option<SessionFile> {
    match toml::from_str::<SessionFile>(text) {
        Ok(file) if !file.tabs.is_empty() => Some(file),
        _ => None,
    }
}

/// 启动恢复：`session_restore = false` 或文件缺失/损坏/空时保持初始状态。
/// 恢复失败的 tab（目录不存在等）跳过；恢复后 tab 数为 0 → 保持初始 tab。
pub fn restore_session(state: &mut AppState) {
    if !state.config.ui.session_restore {
        return;
    }
    let Ok(text) = fs::read_to_string(session_path()) else {
        return;
    };
    let Some(file) = parse_session(&text) else {
        return;
    };
    apply_session(state, &file);
}

/// 把会话数据应用到 state（纯逻辑，供测试直接驱动）。
/// 单个 tab 构建失败即跳过；cursor/active 钳制到合法范围。
fn apply_session(state: &mut AppState, file: &SessionFile) {
    let cfg = state.config.clone();
    let mut restored = Vec::new();
    for tab in &file.tabs {
        let path = PathBuf::from(&tab.path);
        let Ok(mut built) = crate::tab::Tab::from_physical(&path, cfg.clone()) else {
            continue; // 目录不存在/读取失败：跳过
        };
        built.cursor = tab.cursor.min(built.entries.len().saturating_sub(1));
        built.on_parent_row = tab.on_parent_row && built.has_parent_row();
        built.focus = if tab.focus == "right" {
            crate::tab::Focus::Right
        } else {
            crate::tab::Focus::Left
        };
        restored.push(built);
    }
    if restored.is_empty() {
        return; // 全部恢复失败 → 保持初始会话
    }
    state.tabs = restored;
    state.active = file.active.min(state.tabs.len() - 1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::tab::Tab;
    use tempfile::TempDir;

    #[test]
    fn m251_roundtrip_and_clamp() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "A").unwrap();
        fs::write(tmp.path().join("b.txt"), "B").unwrap();
        let other = TempDir::new().unwrap();
        fs::write(other.path().join("x.txt"), "X").unwrap();
        fs::write(other.path().join("y.txt"), "Y").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut t2 = Tab::from_physical(other.path(), Config::default()).unwrap();
        t2.cursor = 1;
        t2.focus = crate::tab::Focus::Right;
        state.tabs.push(t2);
        state.active = 1;

        // 序列化 → 解析 → 应用：全部字段还原。
        let text = serialize_session(&state);
        assert!(text.contains("active = 1"), "text: {text}");
        let file = parse_session(&text).expect("应可解析");
        assert_eq!(file.tabs.len(), 2);

        let mut fresh = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        apply_session(&mut fresh, &file);
        assert_eq!(fresh.tabs.len(), 2);
        assert_eq!(fresh.active, 1);
        assert_eq!(fresh.tabs[1].cursor, 1);
        assert_eq!(fresh.tabs[1].focus, crate::tab::Focus::Right);
        assert_eq!(fresh.tabs[0].location.base_dir(), tmp.path());
    }

    #[test]
    fn m251_corrupt_and_nonphysical_dropped() {
        // 损坏文本 → None。
        assert_eq!(parse_session("!!!not toml{{{"), None);
        assert_eq!(parse_session(""), None);
        // 空 tabs → None（保持初始会话）。
        assert_eq!(parse_session("active = 0\ntabs = []"), None);

        // 远端 tab 不保存。
        let tmp = TempDir::new().unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut remote_tab = Tab::from_physical(tmp.path(), Config::default()).unwrap();
        remote_tab.location = Location::Remote {
            remote: "srv".to_owned(),
            path: PathBuf::from("/root"),
        };
        state.tabs.push(remote_tab);
        let text = serialize_session(&state);
        let file = parse_session(&text).unwrap();
        assert_eq!(file.tabs.len(), 1, "仅物理 Location 入会话");

        // cursor 越界钳制；恢复失败目录跳过；active 钳制。
        let file2 = SessionFile {
            active: 5,
            tabs: vec![
                SessionTab {
                    path: tmp.path().to_string_lossy().into_owned(),
                    cursor: 99,
                    on_parent_row: true,
                    focus: "left".to_owned(),
                },
                SessionTab {
                    path: "/nonexistent-dir-xyz-12345".to_owned(),
                    cursor: 0,
                    on_parent_row: false,
                    focus: "left".to_owned(),
                },
            ],
        };
        let mut fresh = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        apply_session(&mut fresh, &file2);
        assert_eq!(fresh.tabs.len(), 1, "失败目录跳过");
        assert_eq!(fresh.active, 0, "active 钳制到 tab 数内");
        assert_eq!(fresh.tabs[0].cursor, 0, "cursor 钳制到条目数内");
    }
}
