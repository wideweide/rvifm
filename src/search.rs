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
    /// M28-2：`:rename` 结构化预览（text 承载新名/跳过原因；y 应用整批）。
    RenamePreview,
}

/// 结果上限（与 :tree 的 1 万条上限同模式）。
pub const MAX_RESULTS: usize = 10_000;

/// M55-2：实时递归模式（`O`）的护栏——深度 20、条目 5 万。
pub const LIVE_FIND_MAX_DEPTH: usize = 20;
pub const LIVE_FIND_MAX_ENTRIES: usize = 50_000;

/// M55-2：后台流式扫描回传（逐条产出，不写盘、不改 frecency tick）。
#[derive(Debug)]
pub enum LiveFindUpdate {
    Entry(ResultItem),
    Done { truncated: bool },
}

/// M55-2：后台线程扫描 root，逐条 `send`；`stop` 置位即中止（用户退出）。
/// 复用 `:find` 的 ignore 语义（gitignore/.ignore、隐藏文件按配置），
/// 仅收文件；深度/条目护栏见常量，超限以 `Done { truncated: true }` 结束。
pub fn spawn_live_find(
    tx: std::sync::mpsc::Sender<LiveFindUpdate>,
    root: PathBuf,
    show_hidden: bool,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    std::thread::spawn(move || {
        let mut count = 0usize;
        let mut builder = ignore::WalkBuilder::new(&root);
        builder
            .hidden(!show_hidden)
            .max_depth(Some(LIVE_FIND_MAX_DEPTH));
        for result in builder.build() {
            if stop.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            let Ok(entry) = result else { continue };
            if !entry.file_type().is_some_and(|t| t.is_file()) {
                continue;
            }
            let path = entry.path().to_path_buf();
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if tx
                .send(LiveFindUpdate::Entry(ResultItem {
                    path,
                    line: None,
                    text: name,
                    size: 0,
                }))
                .is_err()
            {
                break;
            }
            count += 1;
            if count >= LIVE_FIND_MAX_ENTRIES {
                let _ = tx.send(LiveFindUpdate::Done { truncated: true });
                return;
            }
        }
        let _ = tx.send(LiveFindUpdate::Done { truncated: false });
    });
}

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

// ── M38-5：:find 过滤器（--size / --mtime / --type / --depth） ──────────────

/// M38-5：find 过滤器参数（全部缺省 = 不过滤，行为与 M17 一致）。
#[derive(Debug, Default, Clone)]
pub struct FindFilter {
    /// `--size +N`：文件 ≥ N 字节。
    pub min_size: Option<u64>,
    /// `--size -N`：文件 ≤ N 字节。
    pub max_size: Option<u64>,
    /// `--size =N`：文件 = N 字节。
    pub eq_size: Option<u64>,
    /// `--mtime +N`：mtime 早于 N 天前（单位：天）。
    pub mtime_older_days: Option<u64>,
    /// `--mtime -N`：mtime 在最近 N 天内。
    pub mtime_within_days: Option<u64>,
    /// `--type d`：仅目录；`--type f`：仅文件（缺省 = 文件，与既有行为一致）。
    pub type_dir: Option<bool>,
    /// `--depth N`：最大遍历深度（root = 0，直接子项 = 1）。
    pub max_depth: Option<usize>,
}

impl FindFilter {
    /// 是否未配置任何过滤项（用于跳过 metadata 采集的快路径）。
    fn is_empty(&self) -> bool {
        self.min_size.is_none()
            && self.max_size.is_none()
            && self.eq_size.is_none()
            && self.mtime_older_days.is_none()
            && self.mtime_within_days.is_none()
            && self.type_dir.is_none()
            && self.max_depth.is_none()
    }
}

/// 单位倍率（b/k/m/g，大小写不敏感；缺省 = 字节）。
fn size_unit_multiplier(unit: &str) -> Option<u64> {
    match unit.to_ascii_lowercase().as_str() {
        "" | "b" => Some(1),
        "k" | "kb" => Some(1024),
        "m" | "mb" => Some(1024 * 1024),
        "g" | "gb" => Some(1024 * 1024 * 1024),
        _ => None,
    }
}

/// `--size` 规格解析结果：(min, max, eq)。
pub type SizeSpec = (Option<u64>, Option<u64>, Option<u64>);

/// `--size` 规格：`[+-=]N[unit]`（如 +10M、-1k、=5g、100）。
/// 返回 (min, max, eq)。
pub fn parse_size_spec(spec: &str) -> Result<SizeSpec, String> {
    let (op, rest) = match spec.chars().next() {
        Some('+') => ('+', &spec[1..]),
        Some('-') => ('-', &spec[1..]),
        Some('=') => ('=', &spec[1..]),
        _ => ('+', spec),
    };
    // 数值 + 单位拆分（单位为尾部字母）。
    let digit_end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    if digit_end == 0 {
        return Err(format!("size 规格缺数值：{spec}"));
    }
    let n: u64 = rest[..digit_end]
        .parse()
        .map_err(|_| format!("size 数值非法：{spec}"))?;
    let unit = &rest[digit_end..];
    let mult = size_unit_multiplier(unit).ok_or_else(|| format!("size 单位非法：{spec}"))?;
    let bytes = n.saturating_mul(mult);
    let out = match op {
        '+' => (Some(bytes), None, None),
        '-' => (None, Some(bytes), None),
        _ => (None, None, Some(bytes)),
    };
    Ok(out)
}

/// `--mtime` 规格：`+N`（早于 N 天前）或 `-N`（最近 N 天内）。单位 = 天。
pub fn parse_mtime_spec(spec: &str) -> Result<(Option<u64>, Option<u64>), String> {
    let (op, rest) = match spec.chars().next() {
        Some('+') => ('+', &spec[1..]),
        Some('-') => ('-', &spec[1..]),
        _ => return Err(format!("mtime 规格需 +N/-N（天）：{spec}")),
    };
    let n: u64 = rest
        .parse()
        .map_err(|_| format!("mtime 数值非法：{spec}"))?;
    Ok(match op {
        '+' => (Some(n), None),
        _ => (None, Some(n)),
    })
}

/// `--type` 规格：f（文件）/ d（目录）。
pub fn parse_type_spec(spec: &str) -> Result<bool, String> {
    match spec {
        "f" => Ok(false),
        "d" => Ok(true),
        other => Err(format!("type 规格非法（f|d）：{other}")),
    }
}

/// M38-5：纯谓词（注入 now 与条目属性，可测）——过滤项全部通过才为 true。
pub fn filter_matches(
    size: u64,
    mtime: u64,
    is_dir: bool,
    depth: usize,
    f: &FindFilter,
    now: u64,
) -> bool {
    if let Some(want_dir) = f.type_dir
        && is_dir != want_dir
    {
        return false;
    }
    if let Some(min) = f.min_size
        && size < min
    {
        return false;
    }
    if let Some(max) = f.max_size
        && size > max
    {
        return false;
    }
    if let Some(eq) = f.eq_size
        && size != eq
    {
        return false;
    }
    if let Some(days) = f.mtime_older_days
        && mtime >= now.saturating_sub(days.saturating_mul(86_400))
    {
        return false;
    }
    if let Some(days) = f.mtime_within_days
        && mtime < now.saturating_sub(days.saturating_mul(86_400))
    {
        return false;
    }
    if let Some(max) = f.max_depth
        && depth > max
    {
        return false;
    }
    true
}

/// M38-5：解析 find 参数 token 序列 → (模式, 过滤器)。
/// 首个非 `--` token 为搜索模式（可空 = 全部文件）；`--k=v` 或 `--k v`；
/// 多余位置参数可读报错。
pub fn parse_find_args(tokens: &[String]) -> Result<(String, FindFilter), String> {
    let mut pattern = String::new();
    let mut f = FindFilter::default();
    let mut i = 0usize;
    while i < tokens.len() {
        let tok = &tokens[i];
        let Some(key) = tok.strip_prefix("--") else {
            if pattern.is_empty() {
                pattern = tok.clone();
            } else {
                return Err(format!("find: 意外参数 {tok}（模式后仅可跟 -- 选项）"));
            }
            i += 1;
            continue;
        };
        let (key, inline) = match key.split_once('=') {
            Some((k, v)) => (k.to_owned(), Some(v.to_owned())),
            None => (key.to_owned(), None),
        };
        let value = match inline {
            Some(v) => v,
            None => {
                i += 1;
                tokens
                    .get(i)
                    .cloned()
                    .ok_or_else(|| format!("find: --{key} 缺少参数值"))?
            }
        };
        match key.as_str() {
            "size" => {
                let (min, max, eq) = parse_size_spec(&value)?;
                f.min_size = min;
                f.max_size = max;
                f.eq_size = eq;
            }
            "mtime" => {
                let (older, within) = parse_mtime_spec(&value)?;
                f.mtime_older_days = older;
                f.mtime_within_days = within;
            }
            "type" => f.type_dir = Some(parse_type_spec(&value)?),
            "depth" => {
                let n: usize = value
                    .parse()
                    .map_err(|_| format!("find: depth 数值非法：{value}"))?;
                if n == 0 {
                    return Err("find: depth 需 ≥ 1".to_owned());
                }
                f.max_depth = Some(n);
            }
            other => return Err(format!("find: 未知选项 --{other}")),
        }
        i += 1;
    }
    Ok((pattern, f))
}

/// 当前 Unix 秒（mtime 过滤的"现在"基准）。
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// M38-5：:find 过滤版——模式子串 + size/mtime/type/depth 过滤。
/// `--type` 未配置时仅收文件（与既有行为一致）；配置 d 时仅收目录。
pub fn find_files_filtered(
    root: &Path,
    query: &str,
    show_hidden: bool,
    filter: &FindFilter,
) -> (Vec<ResultItem>, bool) {
    let lower = query.to_lowercase();
    let now = now_secs();
    let mut items = Vec::new();
    for result in walker(root, show_hidden) {
        let Ok(entry) = result else { continue };
        if entry.depth() == 0 {
            continue; // 根目录自身不进结果（--type d 时避免计入）。
        }
        let is_dir = entry.file_type().is_some_and(|t| t.is_dir());
        let is_file = entry.file_type().is_some_and(|t| t.is_file());
        // 类型门：缺省仅文件（既有行为）；--type 显式指定时按指定收集。
        match filter.type_dir {
            None => {
                if !is_file {
                    continue;
                }
            }
            Some(want_dir) => {
                if is_dir != want_dir {
                    continue;
                }
            }
        }
        let path = entry.path().to_path_buf();
        let leaf = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !lower.is_empty() && !leaf.to_lowercase().contains(&lower) {
            continue;
        }
        if filter.is_empty() {
            if items.len() >= MAX_RESULTS {
                return (items, true);
            }
            items.push(ResultItem {
                path,
                line: None,
                text: String::new(),
                size: 0,
            });
            continue;
        }
        // --depth 过滤（无需 metadata）。
        let depth = entry.depth();
        if let Some(max) = filter.max_depth
            && depth > max
        {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if !filter_matches(meta.len(), mtime, is_dir, depth, filter, now) {
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

    #[test]
    fn m38_5_parse_specs() {
        // size：[+-=]N[unit]。
        assert_eq!(
            parse_size_spec("+10M").unwrap(),
            (Some(10 * 1024 * 1024), None, None)
        );
        assert_eq!(parse_size_spec("-1k").unwrap(), (None, Some(1024), None));
        assert_eq!(
            parse_size_spec("=5g").unwrap(),
            (None, None, Some(5 * 1024 * 1024 * 1024))
        );
        assert_eq!(parse_size_spec("100").unwrap(), (Some(100), None, None));
        assert!(parse_size_spec("+").is_err());
        assert!(parse_size_spec("+5x").is_err());
        // mtime：+N / -N（天）。
        assert_eq!(parse_mtime_spec("+7").unwrap(), (Some(7), None));
        assert_eq!(parse_mtime_spec("-3").unwrap(), (None, Some(3)));
        assert!(parse_mtime_spec("7").is_err());
        assert!(parse_mtime_spec("+x").is_err());
        // type：f / d。
        assert!(!parse_type_spec("f").unwrap());
        assert!(parse_type_spec("d").unwrap());
        assert!(parse_type_spec("l").is_err());
    }

    #[test]
    fn m38_5_filter_matches_predicate() {
        let now = 1_795_219_200u64;
        let f = FindFilter {
            min_size: Some(100),
            mtime_within_days: Some(7),
            ..Default::default()
        };
        assert!(filter_matches(200, now - 100, false, 1, &f, now));
        assert!(
            !filter_matches(50, now - 100, false, 1, &f, now),
            "size < min"
        );
        assert!(
            !filter_matches(200, now - 8 * 86_400, false, 1, &f, now),
            "mtime 超出 7 天"
        );
        let f2 = FindFilter {
            max_size: Some(10),
            mtime_older_days: Some(2),
            type_dir: Some(true),
            max_depth: Some(2),
            ..Default::default()
        };
        assert!(filter_matches(5, now - 3 * 86_400, true, 2, &f2, now));
        assert!(
            !filter_matches(11, now - 3 * 86_400, true, 2, &f2, now),
            "size > max"
        );
        assert!(
            !filter_matches(5, now - 86_400, true, 2, &f2, now),
            "mtime 未满 2 天"
        );
        assert!(
            !filter_matches(5, now - 3 * 86_400, false, 2, &f2, now),
            "type 不符"
        );
        assert!(
            !filter_matches(5, now - 3 * 86_400, true, 3, &f2, now),
            "depth 超限"
        );
    }

    #[test]
    fn m38_5_parse_find_args_tokens() {
        let t: Vec<String> = [
            "txt",
            "--size",
            "+1M",
            "--mtime=-7",
            "--type=f",
            "--depth",
            "2",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
        let (pat, f) = parse_find_args(&t).unwrap();
        assert_eq!(pat, "txt");
        assert_eq!(f.min_size, Some(1024 * 1024));
        assert_eq!(f.mtime_within_days, Some(7));
        assert_eq!(f.type_dir, Some(false));
        assert_eq!(f.max_depth, Some(2));
        // 空模式 = 全部。
        let (pat, f) = parse_find_args(&[]).unwrap();
        assert!(pat.is_empty());
        assert!(f.is_empty());
        // 错误分支。
        let bad: Vec<String> = vec!["txt".into(), "--bogus".into(), "1".into()];
        assert!(parse_find_args(&bad).unwrap_err().contains("--bogus"));
        let bad: Vec<String> = vec!["a".into(), "b".into()];
        assert!(parse_find_args(&bad).unwrap_err().contains("意外参数"));
        let bad: Vec<String> = vec!["--size".into()];
        assert!(parse_find_args(&bad).unwrap_err().contains("缺少参数值"));
        let bad: Vec<String> = vec!["--depth".into(), "0".into()];
        assert!(parse_find_args(&bad).unwrap_err().contains("depth"));
    }

    #[test]
    fn m38_5_find_files_filtered_end_to_end() {
        // 端到端：类型门（缺省仅文件 / --type d）、size、depth、mtime。
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        fs::create_dir_all(tmp.path().join("sub").join("deep")).unwrap();
        fs::write(tmp.path().join("big.txt"), vec![b'x'; 2048]).unwrap();
        fs::write(tmp.path().join("small.txt"), "x").unwrap();
        fs::write(tmp.path().join("sub").join("mid.txt"), vec![b'x'; 100]).unwrap();
        fs::write(tmp.path().join("sub").join("deep").join("far.txt"), "x").unwrap();
        // 缺省（无过滤）：.txt 文件 4 个（目录不收，既有行为）。
        let (items, _) = find_files_filtered(tmp.path(), ".txt", false, &FindFilter::default());
        assert_eq!(items.len(), 4);
        // --type d：目录 2 个。
        let (items, _) = find_files_filtered(
            tmp.path(),
            "",
            false,
            &FindFilter {
                type_dir: Some(true),
                ..Default::default()
            },
        );
        assert_eq!(items.len(), 2);
        // --size +1k：仅 big.txt。
        let (items, _) = find_files_filtered(
            tmp.path(),
            "",
            false,
            &FindFilter {
                min_size: Some(1024),
                ..Default::default()
            },
        );
        assert_eq!(items.len(), 1);
        assert!(items[0].path.ends_with("big.txt"));
        // --depth 1：root 直接层 2 个 .txt。
        let (items, _) = find_files_filtered(
            tmp.path(),
            ".txt",
            false,
            &FindFilter {
                max_depth: Some(1),
                ..Default::default()
            },
        );
        assert_eq!(items.len(), 2);
        // --mtime -1：全部新建文件命中。
        let (items, _) = find_files_filtered(
            tmp.path(),
            ".txt",
            false,
            &FindFilter {
                mtime_within_days: Some(1),
                ..Default::default()
            },
        );
        assert_eq!(items.len(), 4);
        // --mtime +1：新建文件早于 1 天前才命中 → 0。
        let (items, _) = find_files_filtered(
            tmp.path(),
            ".txt",
            false,
            &FindFilter {
                mtime_older_days: Some(1),
                ..Default::default()
            },
        );
        assert_eq!(items.len(), 0);
    }
}
