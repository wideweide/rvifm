//! M25-5：git 状态（`git` CLI 子进程，零编译期依赖——M23-12 决策路线）。
//!
//! 异步模型（gitui 同款体感策略，SPEC §25 M25-5）：查询在后台线程执行，
//! UI 不等待——列表先渲染，状态列就绪后填充；结果经 mpsc 回传主线程写入
//! per-目录缓存。查询用 pathspec 限定当前目录（只扫当前子树，比全仓库
//! status 便宜一个量级）+ `--no-optional-locks`（浏览不写 index、不与用户
//! 其他 git 操作抢锁）。超大仓库建议用户侧开启 `core.fsmonitor` +
//! `core.untrackedCache`（README 提示，rvifm 不代改配置）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 条目 git 状态（列表列显示单字符）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Typechange,
    Untracked,
    Conflicted,
}

impl GitStatus {
    /// 显示字符（渲染列）。
    pub fn marker(self) -> &'static str {
        match self {
            GitStatus::Modified => "M",
            GitStatus::Added => "A",
            GitStatus::Deleted => "D",
            GitStatus::Renamed => "R",
            GitStatus::Typechange => "T",
            GitStatus::Untracked => "?",
            GitStatus::Conflicted => "U",
        }
    }

    /// porcelain 状态字母 → 枚举（'!' 忽略项 → None）。
    fn from_code(c: u8) -> Option<GitStatus> {
        match c {
            b'M' => Some(GitStatus::Modified),
            b'A' => Some(GitStatus::Added),
            b'D' => Some(GitStatus::Deleted),
            b'R' => Some(GitStatus::Renamed),
            b'C' => Some(GitStatus::Typechange),
            b'T' => Some(GitStatus::Typechange),
            b'U' => Some(GitStatus::Conflicted),
            b'?' => Some(GitStatus::Untracked),
            _ => None,
        }
    }
}

/// 一个目录的 git 状态快照（直接子项映射；子目录聚合标记）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GitDirStatus {
    /// 当前分支（`-b` 头行；M25-6 状态栏复用）。
    pub branch: Option<String>,
    /// 直接子项名 → 状态（子目录取其内部条目聚合）。
    pub entries: HashMap<String, GitStatus>,
}

/// 逐级向上探测仓库根（`.git` 目录或 worktree 文件）。
pub fn repo_root(dir: &Path) -> Option<PathBuf> {
    let mut cur = Some(dir);
    while let Some(p) = cur {
        if p.join(".git").exists() {
            return Some(p.to_path_buf());
        }
        cur = p.parent();
    }
    None
}

/// 同步查询（后台线程内执行）：`git --no-optional-locks status
/// --porcelain -z -b --untracked-files=normal -- <dir>`。
pub fn query_status(root: &Path, dir: &Path) -> std::io::Result<GitDirStatus> {
    let out = std::process::Command::new("git")
        .current_dir(root)
        .args([
            "--no-optional-locks",
            "status",
            "--porcelain",
            "-z",
            "-b",
            "--untracked-files=normal",
            "--",
        ])
        .arg(dir)
        .output()?;
    if !out.status.success() {
        return Err(std::io::Error::other(
            String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        ));
    }
    // porcelain 路径为仓库根相对路径 → 换算前缀供解析。
    let rel_dir = dir
        .strip_prefix(root)
        .unwrap_or(dir)
        .to_string_lossy()
        .replace('\\', "/");
    let rel_dir = rel_dir.trim_matches('/').to_owned();
    Ok(parse_status(&rel_dir, &out.stdout))
}

/// porcelain -z 解析（纯函数）。`rel_dir` 为查询目录相对仓库根的路径
/// （根目录 = ""）。字节流：`## branch...\0`（-b 头行）+ `XY <根相对路径>\0`
/// （rename 记录在路径后追加 orig\0；untracked 目录路径带尾 `/`）。
pub fn parse_status(rel_dir: &str, raw: &[u8]) -> GitDirStatus {
    let mut status = GitDirStatus::default();
    for rec in raw.split(|b| *b == 0) {
        if rec.is_empty() {
            continue;
        }
        if rec.starts_with(b"## ") {
            // `## main...origin/main [ahead 1]` / `## HEAD (no branch)`。
            let head = &rec[3..];
            let text = String::from_utf8_lossy(head);
            let branch = text.split("...").next().unwrap_or("").trim().to_owned();
            if !branch.is_empty() {
                status.branch = Some(branch);
            }
            continue;
        }
        if rec.len() < 4 {
            continue; // 至少 "XY p"
        }
        let (x, y) = (rec[0], rec[1]);
        if rec[2] != b' ' {
            continue;
        }
        // 单字符归并：未跟踪 → 冲突优先 → 工作区（Y）优先、暂存区（X）兜底；
        // 双侧均无映射（如 '!!' 忽略）跳过。
        let Some(st) = (if x == b'?' {
            Some(GitStatus::Untracked)
        } else if x == b'U' || y == b'U' {
            Some(GitStatus::Conflicted)
        } else {
            GitStatus::from_code(y).or_else(|| GitStatus::from_code(x))
        }) else {
            continue;
        };
        let raw_path = String::from_utf8_lossy(&rec[3..]).into_owned();
        let root_rel = raw_path.trim_end_matches('/');
        // 根相对 → 查询目录相对（直接子项）。
        let dir_rel = if rel_dir.is_empty() {
            root_rel.to_owned()
        } else {
            root_rel
                .strip_prefix(&format!("{rel_dir}/"))
                .unwrap_or(root_rel)
                .to_owned()
        };
        if dir_rel.is_empty() || dir_rel.starts_with('/') {
            continue;
        }
        // 直接子项 = 首段；子目录内部条目聚合到目录（Simple: 标记 Modified）。
        let child = dir_rel.split('/').next().unwrap_or(&dir_rel).to_owned();
        let child_status = if dir_rel.contains('/') && st != GitStatus::Untracked {
            GitStatus::Modified
        } else {
            st
        };
        if status.entries.get(&child) == Some(&GitStatus::Conflicted) {
            continue; // 冲突优先保留
        }
        status.entries.insert(child, child_status);
    }
    status
}

/// 异步查询（gitui 同款：后台线程 + mpsc 回传；UI 不等待）。
pub fn spawn_status_query(
    tx: &std::sync::mpsc::Sender<(PathBuf, std::io::Result<GitDirStatus>)>,
    root: PathBuf,
    dir: PathBuf,
) {
    let tx = tx.clone();
    std::thread::spawn(move || {
        let result = query_status(&root, &dir);
        let _ = tx.send((dir, result));
    });
}

/// M25-5：按需触发 git 状态查询（左右 pane 物理目录；缓存命中/在途跳过；
/// 非仓库记空缓存避免重复探测）。
pub fn sync_status_trigger(state: &mut crate::state::AppState) {
    if !state.config.ui.git_column {
        return;
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    for loc in [
        state.active_tab().location.clone(),
        state.active_tab().right.location.clone(),
    ] {
        if let crate::tab::Location::Physical(p) = loc
            && !dirs.contains(&p)
        {
            dirs.push(p);
        }
    }
    for dir in dirs {
        if state.git_cache.contains_key(&dir) || state.git_pending.contains(&dir) {
            continue;
        }
        let Some(root) = repo_root(&dir) else {
            state.git_cache.insert(dir, None); // 非仓库：无状态列
            continue;
        };
        match &state.git_tx {
            Some(tx) => {
                state.git_pending.insert(dir.clone());
                spawn_status_query(tx, root, dir);
            }
            None => {
                state.git_cache.insert(dir, None); // 无通道（测试环境）：不查询
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    /// 临时仓库 fixture（git 不可用 → None 跳过）。
    fn make_repo() -> Option<(tempfile::TempDir, PathBuf)> {
        let tmp = tempfile::TempDir::new().ok()?;
        let git = |args: &[&str]| -> bool {
            Command::new("git")
                .current_dir(tmp.path())
                .args(["-c", "user.email=t@t", "-c", "user.name=t"])
                .args(args)
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        };
        // 显式 -b main（CI git ≥2.28 支持；失败 → 跳过测试）。
        if !git(&["init", "-q", "-b", "main"]) {
            return None;
        }
        let root = tmp.path().to_path_buf();
        Some((tmp, root))
    }

    #[test]
    fn m255_parse_status_codes_and_aggregation() {
        let mut raw = Vec::new();
        raw.extend_from_slice(b"## main...origin/main\0");
        raw.extend_from_slice(b" M modified.txt\0");
        raw.extend_from_slice(b"A  staged.txt\0");
        raw.extend_from_slice(b"?? new.txt\0");
        raw.extend_from_slice(b"?? newdir/\0");
        raw.extend_from_slice(b" M sub/deep.txt\0");
        raw.extend_from_slice(b"UU conflict.txt\0");
        raw.extend_from_slice(b"R  renamed.txt\0old.txt\0");
        let st = parse_status("", &raw);
        assert_eq!(st.branch.as_deref(), Some("main"));
        assert_eq!(st.entries.get("modified.txt"), Some(&GitStatus::Modified));
        assert_eq!(st.entries.get("staged.txt"), Some(&GitStatus::Added));
        assert_eq!(st.entries.get("new.txt"), Some(&GitStatus::Untracked));
        assert_eq!(st.entries.get("newdir"), Some(&GitStatus::Untracked));
        assert_eq!(
            st.entries.get("sub"),
            Some(&GitStatus::Modified),
            "子目录聚合"
        );
        assert_eq!(st.entries.get("conflict.txt"), Some(&GitStatus::Conflicted));
        assert_eq!(st.entries.get("renamed.txt"), Some(&GitStatus::Renamed));
    }

    #[test]
    fn m255_parse_status_subdir_prefix_and_marker() {
        let mut raw = Vec::new();
        raw.extend_from_slice(b"## main\0");
        raw.extend_from_slice(b" M docs/a.md\0");
        raw.extend_from_slice(b"?? docs/new/\0");
        let st = parse_status("docs", &raw);
        assert_eq!(st.entries.get("a.md"), Some(&GitStatus::Modified));
        assert_eq!(st.entries.get("new"), Some(&GitStatus::Untracked));
        assert!(!st.entries.contains_key("docs"), "根相对前缀被剥离");
        assert_eq!(GitStatus::Modified.marker(), "M");
        assert_eq!(GitStatus::Untracked.marker(), "?");
    }

    #[test]
    fn m255_repo_root_detection() {
        let Some((tmp, root)) = make_repo() else {
            return; // git 不可用跳过
        };
        let sub = tmp.path().join("deep/nested");
        std::fs::create_dir_all(&sub).unwrap();
        assert_eq!(repo_root(&sub), Some(root.clone()));
        assert_eq!(repo_root(&root), Some(root));
        // 非仓库目录 → None。
        let outside = tempfile::TempDir::new().unwrap();
        assert_eq!(repo_root(outside.path()), None);
    }

    #[test]
    fn m255_query_status_live_repo() {
        let Some((tmp, _root)) = make_repo() else {
            return;
        };
        let git = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .current_dir(tmp.path())
                    .args(["-c", "user.email=t@t", "-c", "user.name=t"])
                    .args(args)
                    .status()
                    .unwrap()
                    .success()
            );
        };
        std::fs::write(tmp.path().join("tracked.txt"), "v1").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "init"]);
        // 工作区修改 + 未跟踪 + 删除。
        std::fs::write(tmp.path().join("tracked.txt"), "v2").unwrap();
        std::fs::write(tmp.path().join("untracked.txt"), "u").unwrap();
        let st = query_status(tmp.path(), tmp.path()).expect("查询成功");
        assert_eq!(st.branch.as_deref(), Some("main"));
        assert_eq!(st.entries.get("tracked.txt"), Some(&GitStatus::Modified));
        assert_eq!(st.entries.get("untracked.txt"), Some(&GitStatus::Untracked));
        // 暂存新文件。
        std::fs::write(tmp.path().join("added.txt"), "a").unwrap();
        git(&["add", "added.txt"]);
        let st = query_status(tmp.path(), tmp.path()).unwrap();
        assert_eq!(st.entries.get("added.txt"), Some(&GitStatus::Added));
    }
}
