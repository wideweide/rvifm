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
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
struct SessionFile {
    active: usize,
    tabs: Vec<SessionTab>,
}

/// 单个 tab 的恢复数据（左 pane 语义）。
/// M32-3：`kind` 扩展——`physical`（缺省，向后兼容）/ `archive`
/// （host:inner）/ `remote`（remote:path）；归档恢复 host 存在即进入，
/// 远端恢复重连（失败跳过该 tab）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct SessionTab {
    path: String,
    cursor: usize,
    on_parent_row: bool,
    /// "left" / "right"（M14-1 焦点 pane）。
    focus: String,
    /// M32-3：tab 类型（缺省 physical = 旧文件零迁移兼容）。
    #[serde(default)]
    kind: String,
    /// M53-2：`:tabname` 命名（持久；None = 显示路径）。
    #[serde(default)]
    name: Option<String>,
    /// M53-2：右栏活动项（location + cursor；缺省 None = 旧会话兼容）。
    #[serde(default)]
    right_path: Option<String>,
    #[serde(default)]
    right_kind: String,
    #[serde(default)]
    right_cursor: usize,
}

/// 会话文件路径（与 config.toml 同目录）。
pub fn session_path() -> PathBuf {
    let cfg = crate::config::config_path();
    match cfg.parent() {
        Some(dir) => dir.join("session.toml"),
        None => PathBuf::from("session.toml"),
    }
}

/// Location → (kind, 可序列化路径串)；左右栏共用（M53-2）。
fn location_parts(loc: &Location) -> (String, String) {
    match loc {
        Location::Physical(p) => ("physical".to_owned(), p.to_string_lossy().into_owned()),
        Location::Archive { host, inner } => (
            "archive".to_owned(),
            format!(
                "{}:{}",
                host.to_string_lossy().replace('\\', "/"),
                inner.to_string_lossy().replace('\\', "/")
            ),
        ),
        Location::Remote { remote, path } => (
            "remote".to_owned(),
            format!("{}:{}", remote, path.to_string_lossy().replace('\\', "/")),
        ),
    }
}

/// 收集可持久化 tab 并序列化为 TOML 文本（仅物理 Location）。
pub fn serialize_session(state: &AppState) -> String {
    let file = SessionFile {
        active: state.active,
        tabs: state
            .tabs
            .iter()
            .map(|tab| {
                // M32-3：三种 Location 均入会话（归档/远端恢复失败静默跳过）。
                let (kind, path) = location_parts(&tab.location);
                let (right_kind, right_path) = location_parts(&tab.right.location);
                SessionTab {
                    path,
                    cursor: tab.cursor,
                    on_parent_row: tab.on_parent_row,
                    focus: match tab.focus {
                        crate::tab::Focus::Right => "right".to_owned(),
                        crate::tab::Focus::Left => "left".to_owned(),
                    },
                    kind,
                    name: tab.name.clone(),
                    right_kind,
                    right_path: Some(right_path),
                    right_cursor: tab.right.cursor,
                }
            })
            .collect(),
    };
    toml::to_string(&file).unwrap_or_default()
}

/// 退出保存（GAP-2 同模式：失败 eprintln 不阻塞退出）；
/// `session_restore = false` 时不写。
pub fn save_session(state: &AppState) {
    if let Err(e) = save_session_with_path(state, &session_path()) {
        eprintln!("rvifm: 会话保存失败: {e}");
    }
}

/// 保存核心（路径注入，供测试与生产共用）。FIX-6：写前创建父目录——
/// 与 `tutor::save_progress_to` 同模式；全新安装无 config 目录时不再静默失败。
/// `session_restore = false` 时直接返回 Ok 不写（既有语义不变）。
pub(crate) fn save_session_with_path(
    state: &AppState,
    path: &std::path::Path,
) -> std::io::Result<()> {
    if !state.config.ui.session_restore {
        return Ok(());
    }
    let text = serialize_session(state);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, text)
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
/// 把会话数据应用到 state（纯逻辑，供测试直接驱动）。
/// 单个 tab 构建失败即跳过；cursor/active 钳制到合法范围。
fn apply_session(state: &mut AppState, file: &SessionFile) -> Vec<String> {
    let cfg = state.config.clone();
    let mut restored = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    for tab in &file.tabs {
        // M32-3：kind 分派——archive/remote 恢复失败跳过该 tab
        //（不 panic、不阻塞其余 tab）；physical 行为与 M25-1 一致。
        let mut built = match tab.kind.as_str() {
            "archive" => {
                // Windows 盘符（C:/...）含冒号——rsplit_once 以最后的冒号
                // 切分（inner 无冒号语义；host 保留完整盘符路径）。
                let (host, inner) = match tab.path.rsplit_once(':') {
                    Some((h, i)) => (PathBuf::from(h), PathBuf::from(i)),
                    None => continue,
                };
                if !host.is_file() {
                    skipped.push(format!("归档文件缺失: {}，已跳过", host.display()));
                    continue;
                }
                // 基座 tab 用临时目录（host 是归档文件，from_physical 对
                // 文件路径 read_dir 失败）——navigate_to 加载归档条目。
                match crate::tab::Tab::from_physical(&std::env::temp_dir(), cfg.clone()) {
                    Ok(mut t) => {
                        if t.navigate_to(
                            crate::tab::Location::Archive {
                                host: host.clone(),
                                inner: inner.clone(),
                            },
                            cfg.clone(),
                        )
                        .is_err()
                        {
                            skipped.push(format!("归档 {} 打开失败，已跳过", host.display()));
                            continue;
                        }
                        t
                    }
                    Err(_) => continue,
                }
            }
            "remote" => {
                let (name, rpath) = match tab.path.split_once(':') {
                    Some((n, pth)) => (n.to_owned(), PathBuf::from(pth)),
                    None => continue,
                };
                // 远端恢复：重连会话 + 加载条目；失败跳过该 tab。
                if crate::app::remote_reconnect_session(state, &name).is_err() {
                    skipped.push(format!("远端 {} 重连失败，已跳过", name));
                    continue;
                }
                let Ok(items) = crate::app::remote_load_entries(state, &name, &rpath) else {
                    continue;
                };
                let Ok(mut t) = crate::tab::Tab::from_physical(&std::env::temp_dir(), cfg.clone())
                else {
                    continue;
                };
                t.location = crate::tab::Location::Remote {
                    remote: name,
                    path: rpath,
                };
                t.entries = items;
                t
            }
            _ => {
                let path = PathBuf::from(&tab.path);
                match crate::tab::Tab::from_physical(&path, cfg.clone()) {
                    Ok(t) => t,
                    Err(_) => continue, // 目录不存在/读取失败：跳过
                }
            }
        };
        built.cursor = tab.cursor.min(built.entries.len().saturating_sub(1));
        built.on_parent_row = tab.on_parent_row && built.has_parent_row();
        built.focus = if tab.focus == "right" {
            crate::tab::Focus::Right
        } else {
            crate::tab::Focus::Left
        };
        // M53-2：`:tabname` 持久化恢复。
        built.name = tab.name.clone();
        // M53-2：右栏活动项恢复（物理路径为主；归档/远端失败静默降级）。
        if tab.right_kind == "physical"
            && let Some(rpath) = &tab.right_path
            && let Ok(mut pane) =
                crate::tab::Pane::from_physical(&PathBuf::from(rpath), cfg.clone())
        {
            pane.cursor = tab.right_cursor.min(pane.entries.len().saturating_sub(1));
            built.right = pane;
        }
        restored.push(built);
    }
    if restored.is_empty() {
        return skipped; // 全部恢复失败 → 保持初始会话（仍返回跳过原因）
    }
    state.tabs = restored;
    state.active = file.active.min(state.tabs.len() - 1);
    skipped
}

/// M32-3：恢复 + 跳过提示（restore_session 的增强形态；调用方接状态栏）。
pub fn restore_session_with_hints(state: &mut AppState) -> Vec<String> {
    if !state.config.ui.session_restore {
        return Vec::new();
    }
    let Ok(text) = fs::read_to_string(session_path()) else {
        return Vec::new();
    };
    let Some(file) = parse_session(&text) else {
        return Vec::new();
    };
    apply_session(state, &file)
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
    fn fix6_save_creates_missing_config_dir_and_keeps_all_tabs() {
        // FIX-6：注入不存在的配置目录——save 自动建目录并写入全部 tab。
        let tmp = TempDir::new().unwrap();
        let other = TempDir::new().unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        state
            .tabs
            .push(Tab::from_physical(other.path(), Config::default()).unwrap());
        let missing = tmp.path().join("no").join("such").join("rvifm");
        let path = missing.join("session.toml");
        assert!(!missing.exists(), "前置：目录应不存在");
        save_session_with_path(&state, &path).unwrap();
        assert!(missing.exists(), "父目录应被创建");
        let file = parse_session(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(file.tabs.len(), 2, "多 tab 全部入会话");
    }

    #[test]
    fn fix6_save_disabled_does_not_write() {
        // FIX-6：session_restore=false 时不写（既有语义不变）。
        let tmp = TempDir::new().unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        state.config.ui.session_restore = false;
        let path = tmp.path().join("session.toml");
        save_session_with_path(&state, &path).unwrap();
        assert!(!path.exists(), "禁用时不得写会话文件");
    }

    #[test]
    fn m323_archive_session_roundtrip_and_missing_host() {
        // M32-3：归档 tab 入会话（host:inner 格式）+ 恢复（host 存在 →
        // 进入 Archive Location；host 缺失 → 跳过 + 提示）。
        let tmp = TempDir::new().unwrap();
        let zip = tmp.path().join("a.zip");
        // 真实 zip fixture（M26 pack 保证 navigate 可进）。
        let src = tmp.path().join("payload.txt");
        fs::write(&src, b"PAYLOAD").unwrap();
        let _ = crate::ops::create_archive(
            0,
            tmp.path(),
            "a.zip",
            crate::ops::ArchiveFormat::Zip,
            &["payload.txt".to_owned()],
        )
        .unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut arch = Tab::from_physical(tmp.path(), Config::default()).unwrap();
        arch.location = Location::Archive {
            host: zip.clone(),
            inner: PathBuf::from("inner"),
        };
        state.tabs.push(arch);
        let text = serialize_session(&state);
        let file = parse_session(&text).unwrap();
        assert_eq!(file.tabs[1].kind, "archive");
        assert!(file.tabs[1].path.contains("a.zip:inner"));

        let mut fresh = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let skipped = apply_session(&mut fresh, &file);
        assert_eq!(skipped, Vec::<String>::new(), "有效归档无跳过");
        assert_eq!(fresh.tabs.len(), 2);
        assert!(
            matches!(fresh.tabs[1].location, Location::Archive { .. }),
            "归档恢复进入 Archive Location"
        );

        // host 缺失 → 跳过 + 原因。
        let mut file2 = file.clone();
        file2.tabs[1].path = "/nonexistent-xyz.zip:inner".to_owned();
        let mut fresh2 = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let skipped = apply_session(&mut fresh2, &file2);
        assert!(skipped.iter().any(|s| s.contains("归档文件缺失")));
    }

    #[test]
    fn m323_remote_session_serialize_and_restore_skip() {
        // M32-3：远端 tab 入会话（remote:path）；恢复重连失败（mock 会话
        // 不在会话表）→ 跳过 + 提示。
        let tmp = TempDir::new().unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut r = Tab::from_physical(tmp.path(), Config::default()).unwrap();
        r.location = Location::Remote {
            remote: "srv".to_owned(),
            path: PathBuf::from("/data"),
        };
        state.tabs.push(r);
        let text = serialize_session(&state);
        let file = parse_session(&text).unwrap();
        assert_eq!(file.tabs[1].kind, "remote");

        let mut fresh = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let skipped = apply_session(&mut fresh, &file);
        // remote 恢复经 remote_reconnect_session（会话表无 srv → "会话已断开"）。
        assert!(
            skipped.iter().any(|s| s.contains("重连失败")),
            "skipped: {skipped:?}"
        );
        assert_eq!(fresh.tabs.len(), 1, "失败 tab 跳过");
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
        assert_eq!(file.tabs.len(), 2, "M32-3：远端 tab 也入会话");
        assert_eq!(file.tabs[1].kind, "remote");
        assert!(
            file.tabs[1].path.starts_with("srv:"),
            "远端路径格式 remote:path"
        );

        // cursor 越界钳制；恢复失败目录跳过；active 钳制。
        let file2 = SessionFile {
            active: 5,
            tabs: vec![
                SessionTab {
                    path: tmp.path().to_string_lossy().into_owned(),
                    cursor: 99,
                    on_parent_row: true,
                    focus: "left".to_owned(),
                    kind: "physical".to_owned(),
                    name: None,
                    right_path: None,
                    right_kind: String::new(),
                    right_cursor: 0,
                },
                SessionTab {
                    path: "/nonexistent-dir-xyz-12345".to_owned(),
                    cursor: 0,
                    on_parent_row: false,
                    focus: "left".to_owned(),
                    kind: "physical".to_owned(),
                    name: None,
                    right_path: None,
                    right_kind: String::new(),
                    right_cursor: 0,
                },
            ],
        };
        let mut fresh = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        apply_session(&mut fresh, &file2);
        assert_eq!(fresh.tabs.len(), 1, "失败目录跳过");
        assert_eq!(fresh.active, 0, "active 钳制到 tab 数内");
        assert_eq!(fresh.tabs[0].cursor, 0, "cursor 钳制到条目数内");
    }

    #[test]
    fn m532_tabname_and_right_pane_survive_session_roundtrip() {
        let left = tempfile::TempDir::new().unwrap();
        let right = tempfile::TempDir::new().unwrap();
        fs::write(left.path().join("a"), "").unwrap();
        fs::write(left.path().join("b"), "").unwrap();
        fs::write(right.path().join("x"), "").unwrap();
        fs::write(right.path().join("y"), "").unwrap();
        let mut state = AppState::new(Tab::from_physical(left.path(), Config::default()).unwrap());
        state.active_tab_mut().right =
            crate::tab::Pane::from_physical(right.path(), Config::default()).unwrap();
        state.active_tab_mut().name = Some("工作".to_owned());
        state.active_tab_mut().right.cursor = 1;
        let text = serialize_session(&state);
        let file = parse_session(&text).unwrap();
        let mut restored =
            AppState::new(Tab::from_physical(left.path(), Config::default()).unwrap());
        apply_session(&mut restored, &file);
        assert_eq!(restored.tabs[0].name.as_deref(), Some("工作"));
        assert_eq!(restored.tabs[0].right.cursor, 1);
        assert_eq!(
            restored.tabs[0].right.location.base_dir(),
            right.path(),
            "右栏路径恢复"
        );

        // 旧会话（无新字段）仍可解析（serde default）。
        let legacy = format!(
            "active = 0\n[[tabs]]\npath = \"{}\"\ncursor = 0\non_parent_row = false\nfocus = \"left\"\nkind = \"physical\"\n",
            // M65-补：TOML 基本字符串中的 `\` 是转义符，统一写 `/`（Windows 亦可解析）。
            left.path().to_string_lossy().replace('\\', "/")
        );
        assert!(parse_session(&legacy).is_some());
    }
}
