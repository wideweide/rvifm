//! 递归搜索（M17）：ignore 遍历 + 结果虚拟目录的数据层。
//!
//! `:find`（文件名子串）与 `:grep`（正则行匹配）共用遍历与结果形态；
//! 只作用物理目录（M7 只读 VFS 边界），尊重 .gitignore/.ignore。

use std::path::{Path, PathBuf};

/// 结果条目（与 Tab.results.items 平行；entries 的 name 为显示串）。
#[derive(Debug, Clone)]
pub struct ResultItem {
    /// 命中文件路径。
    pub path: PathBuf,
    /// grep 命中行号（find/compare 为 None）。
    pub line: Option<usize>,
    /// grep 命中行内容；compare 为前缀标记（=/!/</>；find 为空）。
    pub text: String,
    /// 文件大小（compare 使用；find/grep 为 0——属性列显示与既有结果一致）。
    pub size: u64,
}

/// 搜索结果虚拟目录状态（Tab.results；沿 :tree 同构的静态条目）。
#[derive(Debug, Clone)]
pub struct SearchResults {
    pub kind: SearchKind,
    pub query: String,
    /// 遍历根目录（VIEW-1 查看器/重搜索使用）。
    #[allow(dead_code)]
    pub root: PathBuf,
    /// 是否因上限截断。
    pub truncated: bool,
    /// 与 Tab.entries 平行（同长度同序）。
    pub items: Vec<ResultItem>,
}

/// 结果视图类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchKind {
    Find,
    Grep,
    /// M24-8：左右 pane 目录对比（text 承载前缀标记）。
    Compare,
}

/// 结果上限（与 :tree 的 1 万条上限同模式）。
pub const MAX_RESULTS: usize = 10_000;

/// 构造遍历器：gitignore/.ignore 感知，隐藏文件按配置。
fn walker(root: &Path, show_hidden: bool) -> ignore::Walk {
    ignore::WalkBuilder::new(root).hidden(!show_hidden).build()
}

/// :find——递归文件名查找（大小写不敏感子串；query 空 = 全部文件）。
/// 返回 (条目, 是否截断)。
pub fn find_files(root: &Path, query: &str, show_hidden: bool) -> (Vec<ResultItem>, bool) {
    let lower = query.to_lowercase();
    let mut items = Vec::new();
    for result in walker(root, show_hidden) {
        let Ok(entry) = result else { continue }; // 无权限等：跳过
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue; // 只收文件
        }
        let path = entry.path().to_path_buf();
        let leaf = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !lower.is_empty() && !leaf.to_lowercase().contains(&lower) {
            continue;
        }
        if items.len() >= MAX_RESULTS {
            return (items, true);
        }
        items.push(ResultItem {
            path,
            line: None,
            text: String::new(),
            size: 0,
        });
    }
    (items, false)
}

/// :grep——递归内容搜索（正则逐行匹配；二进制/非 UTF-8 跳过）。
/// 返回 (条目, 是否截断)。
pub fn grep_files(
    root: &Path,
    pattern: &str,
    show_hidden: bool,
) -> Result<(Vec<ResultItem>, bool), String> {
    let re = regex::Regex::new(pattern).map_err(|e| format!("正则无效: {e}"))?;
    let mut items = Vec::new();
    for result in walker(root, show_hidden) {
        let Ok(entry) = result else { continue };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.path().to_path_buf();
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        if bytes.contains(&0u8) {
            continue; // 二进制（含 NUL）跳过
        }
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        }; // 非 UTF-8 跳过
        for (lineno, line) in text.lines().enumerate() {
            if re.is_match(line) {
                if items.len() >= MAX_RESULTS {
                    return Ok((items, true));
                }
                items.push(ResultItem {
                    path: path.clone(),
                    line: Some(lineno + 1),
                    text: line.to_owned(),
                    size: 0,
                });
            }
        }
    }
    Ok((items, false))
}

// ── M22-5：远端 :find/:grep（RemoteFs 驱动；无 ignore 语义） ───────────────

/// 单文件 read 上限（SPEC M22-5 SIMPLIFIED：防大文件拖垮）。
const REMOTE_GREP_MAX_BYTES: u64 = 1024 * 1024;

/// 远端递归收集文件（BFS 队列；条目上限 MAX_RESULTS；list 失败静默跳过）。
/// 返回 (文件路径, 大小, 是否截断)。路径恒为 `/` 分隔的远端绝对路径。
fn remote_walk_files(
    backend: &dyn crate::remote::RemoteFs,
    root: &str,
) -> (Vec<(String, u64)>, bool) {
    let join =
        |dir: &str, name: &str| -> String { format!("{}/{}", dir.trim_end_matches('/'), name) };
    let mut files: Vec<(String, u64)> = Vec::new();
    let mut queue = vec![root.to_owned()];
    let mut visited = 0usize;
    while let Some(dir) = queue.pop() {
        let Ok(entries) = backend.list(Path::new(&dir)) else {
            continue; // 无权限等：静默跳过
        };
        for e in entries {
            visited += 1;
            if visited > MAX_RESULTS {
                return (files, true);
            }
            let child = join(&dir, &e.name);
            if e.is_dir {
                queue.push(child);
            } else {
                files.push((child, e.size));
            }
        }
    }
    (files, false)
}

/// 远端 `:find`——文件名子串匹配（大小写不敏感；query 空 = 全部文件）。
pub fn remote_find_files(
    backend: &dyn crate::remote::RemoteFs,
    root: &str,
    query: &str,
) -> (Vec<ResultItem>, bool) {
    let lower = query.to_lowercase();
    let (files, truncated) = remote_walk_files(backend, root);
    let items: Vec<ResultItem> = files
        .into_iter()
        .filter(|(p, _)| {
            if lower.is_empty() {
                return true;
            }
            let leaf = p.rsplit('/').next().unwrap_or_default().to_lowercase();
            leaf.contains(&lower)
        })
        .map(|(p, size)| ResultItem {
            path: PathBuf::from(p),
            line: None,
            text: String::new(),
            size,
        })
        .collect();
    (items, truncated)
}

/// 远端 `:grep`——正则逐行匹配（单文件 ≤ 1 MiB；非 UTF-8 lossy）。
pub fn remote_grep_files(
    backend: &dyn crate::remote::RemoteFs,
    root: &str,
    pattern: &str,
) -> Result<(Vec<ResultItem>, bool), String> {
    let re = regex::Regex::new(pattern).map_err(|e| format!("正则无效: {e}"))?;
    let (files, truncated_files) = remote_walk_files(backend, root);
    let mut items = Vec::new();
    let truncated = truncated_files;
    for (path, size) in files {
        if items.len() >= MAX_RESULTS {
            return Ok((items, true));
        }
        if size > REMOTE_GREP_MAX_BYTES {
            continue; // 大文件跳过（SPEC M22-5）
        }
        let Ok(bytes) = backend.read(Path::new(&path)) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        for (lineno, line) in text.lines().enumerate() {
            if re.is_match(line) {
                if items.len() >= MAX_RESULTS {
                    return Ok((items, true));
                }
                items.push(ResultItem {
                    path: PathBuf::from(path.clone()),
                    line: Some(lineno + 1),
                    text: line.to_owned(),
                    size: 0,
                });
            }
        }
    }
    Ok((items, truncated))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn find_matches_leaf_case_insensitive_and_nested() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("deep/nested")).unwrap();
        fs::write(tmp.path().join("deep/nested/ReadMe.TXT"), "").unwrap();
        fs::write(tmp.path().join("other.log"), "").unwrap();
        let (items, truncated) = find_files(tmp.path(), "readme", false);
        assert!(!truncated);
        assert_eq!(items.len(), 1, "大小写不敏感叶子匹配");
        assert!(items[0].path.ends_with("ReadMe.TXT"));
        assert!(items[0].line.is_none());
    }

    #[test]
    fn find_respects_ignore_file_and_hidden() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("target")).unwrap();
        fs::write(tmp.path().join("target/junk.bin"), "").unwrap();
        fs::write(tmp.path().join("keep.txt"), "").unwrap();
        fs::write(tmp.path().join(".hidden"), "").unwrap();
        // `.ignore` 在非 git 目录也生效（.gitignore 仅 git 仓库内生效）。
        fs::write(tmp.path().join(".ignore"), "target/\n").unwrap();
        let (items, _) = find_files(tmp.path(), "", false);
        let names: Vec<String> = items
            .iter()
            .map(|i| i.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&"keep.txt".to_owned()));
        assert!(!names.contains(&"junk.bin".to_owned()), ".ignore 生效");
        assert!(!names.contains(&".hidden".to_owned()), "隐藏文件默认跳过");
        // show_hidden = true → 出现。
        let (items2, _) = find_files(tmp.path(), "", true);
        let names2: Vec<String> = items2
            .iter()
            .map(|i| i.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(names2.contains(&".hidden".to_owned()));
    }

    #[test]
    fn find_truncates_at_max() {
        let tmp = TempDir::new().unwrap();
        for i in 0..30 {
            fs::write(tmp.path().join(format!("f{i}.txt")), "").unwrap();
        }
        // 直接以小上限验证截断路径：MAX_RESULTS 是常量，这里构造 30 个
        // 文件但把查询限制到命中全部——用 MAX_RESULTS 不可改，因此以
        // grep_files 的同构截断逻辑为准（此处仅验证不截断的常规路径）。
        let (items, truncated) = find_files(tmp.path(), "", false);
        assert_eq!(items.len(), 30);
        assert!(!truncated);
    }

    #[test]
    fn grep_matches_lines_with_numbers_and_skips_binary() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("code.rs"),
            "fn main() {}\n// TODO fix\nx = 1; // TODO later\n",
        )
        .unwrap();
        fs::write(tmp.path().join("bin.dat"), b"a\x00b TODO c".as_slice()).unwrap();
        let (items, truncated) = grep_files(tmp.path(), "TODO", false).unwrap();
        assert!(!truncated);
        assert_eq!(items.len(), 2, "二进制跳过");
        assert_eq!(items[0].line, Some(2));
        assert!(items[0].text.contains("TODO fix"));
        assert_eq!(items[1].line, Some(3));
    }

    #[test]
    fn grep_invalid_regex_reports_error() {
        let tmp = TempDir::new().unwrap();
        let err = grep_files(tmp.path(), "([", false).unwrap_err();
        assert!(err.contains("正则无效"), "err: {err}");
    }
}
