//! `Location` 与 `Tab`：浏览位置模型与每标签独立状态（SPEC §4.1 / §4.2）。
//!
//! 归档浏览行为在 M7 接入；`Location::Archive` 的数据形状现在固定，
//! 避免后期改动所有 tab 相关代码。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::config::Config;

/// 浏览位置：物理目录或归档内虚拟路径（只读）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    Physical(PathBuf),
    /// `host` 为归档文件的物理路径，`inner` 为归档内路径；构造与导航在 M7 接入
    /// （形状由 SPEC §4.1 现在固定）。
    #[allow(dead_code)]
    Archive {
        host: PathBuf,
        inner: PathBuf,
    },
}

impl Location {
    /// 状态栏/面包屑显示。归档位置显示为 `host:/inner/path`（SPEC §4.1）。
    pub fn display_path(&self) -> String {
        match self {
            Location::Physical(p) => p.to_string_lossy().into_owned(),
            Location::Archive { host, inner } => format!(
                "{}:/{}",
                host.display(),
                inner.to_string_lossy().replace('\\', "/")
            ),
        }
    }

    /// `:tabnew` 的落点目录：Physical 为该目录；Archive 为宿主目录
    /// （归档内新建标签复用宿主，M7 再细化）。
    pub fn base_dir(&self) -> &Path {
        match self {
            Location::Physical(p) => p,
            Location::Archive { host, .. } => host,
        }
    }
}

/// 单个目录项。M1-4 起携带大小与权限摘要。
#[derive(Debug, Clone)]
pub struct FileItem {
    /// 类型图标（M9-1 devicons）。
    pub icon: crate::icons::IconInfo,
    pub name: String,
    pub is_dir: bool,
    /// 文件字节数（目录同样记录底层值，状态栏对目录省略显示）。
    pub size: u64,
    /// 平台相关权限摘要：unix 为 3 位八进制（如 `644`），Windows 为 `ro`/`rw`。
    pub perm: String,
}

/// 搜索状态。`query` 由 `/` 设置、`n/N` 消费（M1-7）。
#[derive(Debug, Clone)]
pub struct SearchState {
    pub query: String,
}

/// 每标签独立状态：位置、条目、光标、滚动、搜索、历史（SPEC §4.2）。
/// `visual` 为可视选择锚点（M3-1）；`tags` 为文件标记（M3-2）；`scroll` 为
/// 显式偏移预留（视口滚动当前由 `ListState` 从 cursor 派生，显式存储待 M10）。
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Tab {
    pub location: Location,
    pub entries: Vec<FileItem>,
    pub cursor: usize,
    pub scroll: usize,
    pub search: Option<SearchState>,
    pub history: Vec<Location>,
    /// 可视选择锚点（`v` 进入时的 cursor；None = 非可视模式，M3-1）。
    pub visual: Option<usize>,
    /// glob 过滤模式（M4-2，`=` 设置；None = 不过滤）。
    pub filter: Option<String>,
    /// `t` 标记的文件名集合（tag：文件级标记，供 M3 批量操作。
    /// **tag ≠ mark**——mark 是路径跳转标记，本项目不实现，见 SPEC OUT_OF_SCOPE）。
    pub tags: HashSet<String>,
}

impl Tab {
    /// 从物理目录构建标签：按配置加载并排序条目（M5-2 起 options 生效）。
    pub fn from_physical(dir: &Path, config: Config) -> std::io::Result<Self> {
        Ok(Self {
            location: Location::Physical(dir.to_path_buf()),
            entries: load_entries(dir, &config)?,
            cursor: 0,
            scroll: 0,
            search: None,
            history: Vec::new(),
            visual: None,
            filter: None,
            tags: HashSet::new(),
        })
    }

    /// 光标移动（M1-1）：`delta` 为正向下。空目录为 no-op，两端钳制不越界。
    pub fn move_cursor(&mut self, delta: isize) {
        if self.entries.is_empty() {
            return;
        }
        let max = self.entries.len() as isize - 1;
        let next = self.cursor as isize + delta;
        self.cursor = next.clamp(0, max) as usize;
    }

    /// 跳到首项（M1-3，`gg`）。
    pub fn cursor_top(&mut self) {
        self.cursor = 0;
    }

    /// 跳到末项（M1-3，`G`）。空目录停在 0。
    pub fn cursor_bottom(&mut self) {
        self.cursor = self.entries.len().saturating_sub(1);
    }

    /// 环绕查找匹配条目（M1-7）：大小写不敏感子串；`start` 为起始扫描索引。
    pub fn find_match(&self, query: &str, start: usize, forward: bool) -> Option<usize> {
        if self.entries.is_empty() || query.is_empty() {
            return None;
        }
        let len = self.entries.len();
        for i in 0..len {
            let idx = if forward {
                (start + i) % len
            } else {
                (start + len - i % len) % len
            };
            if query_matches(&self.entries[idx].name, query) {
                return Some(idx);
            }
        }
        None
    }

    /// 从当前位置沿 `forward` 方向移动到下一个匹配（M1-7，`n`/`N`）。
    /// 未找到时 cursor 不变并返回 `false`。
    pub fn search_move(&mut self, query: &str, forward: bool) -> bool {
        let start = if forward {
            self.cursor + 1
        } else {
            self.cursor + self.entries.len().saturating_sub(1)
        };
        match self.find_match(query, start, forward) {
            Some(idx) => {
                self.cursor = idx;
                true
            }
            None => false,
        }
    }

    /// 重读当前物理目录条目并钳制光标（删除/撤销后刷新，M2-2）。
    /// 归档位置为 no-op（M7 接入）。
    pub fn refresh(&mut self, config: Config) -> std::io::Result<()> {
        let Location::Physical(dir) = &self.location else {
            return Ok(());
        };
        let mut entries = load_entries(dir, &config)?;
        if let Some(pat) = &self.filter {
            entries.retain(|item| glob_match(pat, &item.name));
        }
        self.entries = entries;
        self.cursor = self.cursor.min(self.entries.len().saturating_sub(1));
        self.visual = self
            .visual
            .map(|a| a.min(self.entries.len().saturating_sub(1)));
        Ok(())
    }

    /// 应用/清除 glob 过滤（M4-2，`=`）。空 pattern 清除过滤；应用后光标与选区归零。
    pub fn apply_filter(&mut self, pattern: &str, config: Config) {
        let pat = pattern.trim().to_owned();
        self.filter = if pat.is_empty() { None } else { Some(pat) };
        if self.refresh(config).is_ok() {
            self.cursor = 0;
            self.visual = None;
        }
    }

    /// 进入/退出可视选择（M3-1，`v`）。空目录为 no-op。
    pub fn toggle_visual(&mut self) {
        self.visual = match self.visual {
            Some(_) => None,
            None if !self.entries.is_empty() => Some(self.cursor),
            None => None,
        };
    }

    /// 退出可视选择（M3-1，Esc）。
    pub fn exit_visual(&mut self) {
        self.visual = None;
    }

    /// 可视选择范围（含锚点与光标的闭区间）；非可视模式返回 None。
    pub fn visual_range(&self) -> Option<(usize, usize)> {
        self.visual
            .map(|a| (a.min(self.cursor), a.max(self.cursor)))
    }

    /// 切换光标所在条目的 tag 标记（M3-2，`t`）。空目录为 no-op。
    /// tag 按文件名记录；目录重读后仍按名匹配（改名/删除的条目在批量操作时跳过）。
    pub fn toggle_tag(&mut self) {
        if let Some(item) = self.entries.get(self.cursor) {
            let name = item.name.clone();
            if !self.tags.remove(&name) {
                self.tags.insert(name);
            }
        }
    }

    /// 已标记数量（M3-2 状态栏与 M3-3 批量操作用）。
    pub fn tagged_count(&self) -> usize {
        self.tags.len()
    }

    /// 清空全部标记（M3-4：批量操作完成后调用；此前仅测试使用）。
    #[allow(dead_code)]
    pub fn clear_tags(&mut self) {
        self.tags.clear();
    }

    /// 进入光标选中的条目（M1-2 目录；M7-4 归档虚拟目录）。
    /// `l`/`Enter`。目录重读即刷新；读取失败时返回 Err 且不产生副作用
    /// （错误经 M6-1 提示进状态栏）。普通文件 no-op（打开走 M11 opener）。
    pub fn enter_selected(&mut self, config: Config) -> std::io::Result<()> {
        let Some(item) = self.entries.get(self.cursor) else {
            return Ok(());
        };
        let name = item.name.clone();
        let is_dir = item.is_dir;
        match self.location.clone() {
            Location::Physical(base) => {
                let target = base.join(&name);
                if is_dir {
                    let entries = load_entries(&target, &config)?;
                    self.history.push(self.location.clone());
                    self.location = Location::Physical(target);
                    self.entries = entries;
                    self.cursor = 0;
                    self.scroll = 0;
                    return Ok(());
                }
                // 归档文件：进入虚拟根目录（M7-4）；不支持的格式给出解压提示（M7-9）。
                if crate::archive::is_unsupported_archive(&name) {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Unsupported,
                        "该归档格式不支持虚拟浏览，请解压后浏览",
                    ));
                }
                if crate::archive::is_archive_file(&name) {
                    let items = load_archive_entries(&target, "")?;
                    self.history.push(self.location.clone());
                    self.location = Location::Archive {
                        host: target,
                        inner: PathBuf::new(),
                    };
                    self.entries = items;
                    self.cursor = 0;
                    self.scroll = 0;
                }
                Ok(())
            }
            Location::Archive { host, inner } => {
                if !is_dir {
                    return Ok(()); // 归档内文件：只读，打开在 M11 接入
                }
                let inner_str = inner.to_string_lossy().into_owned();
                let new_inner = if inner_str.is_empty() {
                    name
                } else {
                    format!("{inner_str}/{name}")
                };
                let items = load_archive_entries(&host, &new_inner)?;
                self.history.push(self.location.clone());
                self.location = Location::Archive {
                    host,
                    inner: PathBuf::from(crate::archive::normalize_inner(&new_inner)),
                };
                self.entries = items;
                self.cursor = 0;
                self.scroll = 0;
                Ok(())
            }
        }
    }

    /// 返回父目录（M1-2，`h`）。
    /// 物理目录：已在文件系统根时为 no-op（ACCEPTANCE §3）。
    /// 归档内（M7-4）：上一层虚拟目录；归档根处返回宿主目录。
    /// 读取失败时返回 Err 且不产生副作用（提示经 M6-1 进状态栏）。
    pub fn return_parent(&mut self, config: Config) -> std::io::Result<()> {
        match self.location.clone() {
            Location::Archive { host, inner } => {
                let inner_str = inner.to_string_lossy().into_owned();
                if inner_str.is_empty() {
                    // 归档根：`h` 返回归档文件所在的物理目录（host 的父目录）。
                    let host_dir = host.parent().unwrap_or(host.as_path()).to_path_buf();
                    let entries = load_entries(&host_dir, &config)?;
                    self.history.push(self.location.clone());
                    self.location = Location::Physical(host_dir);
                    self.entries = entries;
                    self.cursor = 0;
                    self.scroll = 0;
                    return Ok(());
                }
                // 归档内：上一层虚拟目录（parent 为空串 = 归档根，仍留在归档内）。
                let parent = crate::archive::normalize_inner(
                    inner_str.rsplit_once('/').map(|(p, _)| p).unwrap_or(""),
                );
                let entries = load_archive_entries(&host, &parent)?;
                self.history.push(self.location.clone());
                self.location = Location::Archive {
                    host,
                    inner: PathBuf::from(parent),
                };
                self.entries = entries;
                self.cursor = 0;
                self.scroll = 0;
                Ok(())
            }
            Location::Physical(base) => {
                let Some(parent) = base.parent() else {
                    return Ok(());
                };
                if parent == base {
                    return Ok(());
                }
                let entries = load_entries(parent, &config)?;
                self.history.push(self.location.clone());
                self.location = Location::Physical(parent.to_path_buf());
                self.entries = entries;
                self.cursor = 0;
                self.scroll = 0;
                Ok(())
            }
        }
    }
}

/// 列出归档虚拟目录条目（M7-4）：`ArchiveEntry` → `FileItem`
/// （名字取叶子段；perm 在归档视图留空）。打开失败（损坏/加密）原样上抛。
fn load_archive_entries(host: &Path, inner: &str) -> std::io::Result<Vec<FileItem>> {
    let fs = crate::archive::open_dispatch(host)?;
    let entries = fs.list(&crate::archive::normalize_inner(inner))?;
    Ok(entries
        .into_iter()
        .map(|e| {
            let name = e
                .path
                .rsplit_once('/')
                .map(|(_, l)| l.to_owned())
                .unwrap_or(e.path.clone());
            // 归档内路径为虚拟路径：icon 按叶子名/目录推断。
            let icon_path = if e.is_dir {
                format!("/{}/", name)
            } else {
                format!("/{name}")
            };
            let icon = crate::icons::icon_for(Path::new(&icon_path), crate::icons::IconTheme::Dark);
            FileItem {
                icon,
                name,
                is_dir: e.is_dir,
                size: e.size,
                perm: String::new(),
            }
        })
        .collect())
}

/// 智能大小写匹配（M4-1）：查询含大写字母时区分大小写，否则不区分（vim/vifm smartcase）。
fn query_matches(name: &str, query: &str) -> bool {
    if query.chars().any(char::is_uppercase) {
        name.contains(query)
    } else {
        name.to_lowercase().contains(query)
    }
}

/// 简单 glob 匹配（M4-2）：支持 `*`（任意序列）与 `?`（单字符），不区分大小写。
pub(crate) fn glob_match(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().flat_map(char::to_lowercase).collect();
    let s: Vec<char> = name.chars().flat_map(char::to_lowercase).collect();
    fn m(p: &[char], s: &[char]) -> bool {
        if p.is_empty() {
            return s.is_empty();
        }
        match p[0] {
            '*' => (0..=s.len()).any(|i| m(&p[1..], &s[i..])),
            '?' => !s.is_empty() && m(&p[1..], &s[1..]),
            c => !s.is_empty() && s[0] == c && m(&p[1..], &s[1..]),
        }
    }
    m(&p, &s)
}

/// 读取并排序目录条目：按配置决定隐藏文件与排序（M5-2）。
fn load_entries(dir: &Path, config: &Config) -> std::io::Result<Vec<FileItem>> {
    let mut items = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let is_dir = entry.file_type()?.is_dir();
        let name = entry.file_name().to_string_lossy().into_owned();
        // show_hidden = false 时跳过 `.` 开头的隐藏条目。
        if !config.show_hidden && name.starts_with('.') {
            continue;
        }
        let meta = entry.metadata()?;
        let size = meta.len();
        let perm = perm_string(&meta);
        let icon = crate::icons::icon_for(&entry.path(), crate::icons::IconTheme::Dark);
        items.push(FileItem {
            icon,
            name,
            is_dir,
            size,
            perm,
        });
    }
    items.sort_by(|a, b| {
        let dir_cmp = if config.sort_dirs_first {
            b.is_dir.cmp(&a.is_dir)
        } else {
            std::cmp::Ordering::Equal
        };
        let name_cmp = if config.ignore_case_sort {
            a.name.to_lowercase().cmp(&b.name.to_lowercase())
        } else {
            a.name.cmp(&b.name)
        };
        dir_cmp.then(name_cmp)
    });
    Ok(items)
}

/// 权限摘要：unix 用 3 位八进制；Windows 仅区分只读。
#[cfg(unix)]
fn perm_string(meta: &std::fs::Metadata) -> String {
    use std::os::unix::fs::PermissionsExt;
    format!("{:03o}", meta.permissions().mode() & 0o777)
}

/// 权限摘要：Windows 仅区分只读。
#[cfg(windows)]
fn perm_string(meta: &std::fs::Metadata) -> String {
    if meta.permissions().readonly() {
        "ro".to_owned()
    } else {
        "rw".to_owned()
    }
}

/// 字节数格式化：`B` / `KiB` / `MiB` / `GiB`（1 位小数）。
pub(crate) fn format_size(size: u64) -> String {
    const KIB: f64 = 1024.0;
    let s = size as f64;
    if size < 1024 {
        format!("{size} B")
    } else if size < 1024_u64.pow(2) {
        format!("{:.1} KiB", s / KIB)
    } else if size < 1024_u64.pow(3) {
        format!("{:.1} MiB", s / KIB / KIB)
    } else {
        format!("{:.1} GiB", s / KIB / KIB / KIB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn archive_navigation_roundtrip() {
        // M7-4：l 进 zip 虚拟目录 → 进子目录 → h 逐层返回 → 根处 h 回宿主目录。
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("t.zip");
        crate::archive::make_test_zip(&zip_path);
        fs::write(tmp.path().join("host.txt"), "H").unwrap();
        let mut tab = Tab::from_physical(tmp.path(), Config::default()).unwrap();

        // 排序：host.txt(0) / t.zip(1)。
        tab.cursor = 1;
        tab.enter_selected(Config::default()).unwrap();
        eprintln!(
            "DBG loc={:?} names={:?}",
            tab.location,
            tab.entries
                .iter()
                .map(|i| (&i.name, i.is_dir))
                .collect::<Vec<_>>()
        );
        match &tab.location {
            Location::Archive { host, inner } => {
                assert_eq!(host, &zip_path);
                assert!(inner.as_os_str().is_empty(), "根 inner 为空");
            }
            other => panic!("expected Archive, got {other:?}"),
        }
        let names: Vec<&str> = tab.entries.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["deep", "readme.md", "src"]);

        // 进入 src/ 虚拟子目录。
        tab.cursor = 2;
        tab.enter_selected(Config::default()).unwrap();
        match &tab.location {
            Location::Archive { inner, .. } => {
                assert_eq!(inner, &PathBuf::from("src"))
            }
            other => panic!("expected Archive, got {other:?}"),
        }
        let names: Vec<&str> = tab.entries.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["main.rs"]);

        // h → 回归档根；h → 回宿主目录。
        tab.return_parent(Config::default()).unwrap();
        assert_eq!(
            tab.location,
            Location::Archive {
                host: zip_path.clone(),
                inner: PathBuf::new()
            }
        );
        tab.return_parent(Config::default()).unwrap();
        assert_eq!(tab.location, Location::Physical(tmp.path().to_path_buf()));
        assert!(tab.entries.iter().any(|i| i.name == "host.txt"));
    }

    #[test]
    fn corrupt_archive_enter_returns_error() {
        // M7-4：损坏归档进入返回 Err（M6-1 提示进状态栏），位置不变。
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("bad.zip");
        fs::write(&p, b"PK junk bytes").unwrap();
        let mut tab = Tab::from_physical(tmp.path(), Config::default()).unwrap();
        tab.cursor = 0; // bad.zip
        assert!(tab.enter_selected(Config::default()).is_err());
        assert_eq!(tab.location, Location::Physical(tmp.path().to_path_buf()));
    }

    #[test]
    fn physical_display_is_path() {
        let loc = Location::Physical(PathBuf::from("C:/tmp"));
        assert_eq!(loc.display_path(), "C:/tmp");
    }

    #[test]
    fn archive_display_uses_host_inner_form() {
        let loc = Location::Archive {
            host: PathBuf::from("host.zip"),
            inner: PathBuf::from("src/main"),
        };
        assert_eq!(loc.display_path(), "host.zip:/src/main");
    }

    #[test]
    fn from_physical_loads_sorted_entries() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("zdir")).unwrap();
        fs::write(tmp.path().join("apple.txt"), "").unwrap();

        let tab = Tab::from_physical(tmp.path(), Config::default()).unwrap();
        assert_eq!(tab.cursor, 0);
        assert!(tab.search.is_none());
        assert!(tab.history.is_empty());
        assert_eq!(tab.location, Location::Physical(tmp.path().to_path_buf()));
        let names: Vec<&str> = tab.entries.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["zdir", "apple.txt"]);
    }

    #[test]
    fn move_cursor_clamps_at_bounds() {
        let tmp = TempDir::new().unwrap();
        for name in ["a", "b", "c"] {
            fs::write(tmp.path().join(name), "").unwrap();
        }
        let mut tab = Tab::from_physical(tmp.path(), Config::default()).unwrap();
        assert_eq!(tab.cursor, 0);
        tab.move_cursor(1);
        assert_eq!(tab.cursor, 1);
        tab.move_cursor(10);
        assert_eq!(tab.cursor, 2); // 末尾钳制
        tab.move_cursor(-10);
        assert_eq!(tab.cursor, 0); // 顶部钳制
        tab.move_cursor(-1);
        assert_eq!(tab.cursor, 0);
    }

    #[test]
    fn move_cursor_is_noop_on_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let mut tab = Tab::from_physical(tmp.path(), Config::default()).unwrap();
        tab.move_cursor(1);
        tab.move_cursor(-1);
        assert_eq!(tab.cursor, 0);
    }

    #[test]
    fn query_matches_smart_case() {
        // 全小写查询：不区分大小写。
        assert!(query_matches("Beta2.txt", "beta"));
        assert!(query_matches("beta.txt", "beta"));
        // 含大写查询：区分大小写。
        assert!(query_matches("Beta2.txt", "Beta"));
        assert!(!query_matches("beta.txt", "Beta"));
        assert!(!query_matches("BETA.txt", "Beta"));
        // 子串语义保持。
        assert!(query_matches("report-2026.txt", "2026"));
    }

    #[test]
    fn format_size_units() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(2048), "2.0 KiB");
        assert_eq!(format_size(1024 * 1024), "1.0 MiB");
        assert_eq!(format_size(3 * 1024 * 1024 * 1024), "3.0 GiB");
    }

    #[test]
    fn entries_carry_size_and_perm() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("hello.txt"), "hello").unwrap(); // 5 字节
        let tab = Tab::from_physical(tmp.path(), Config::default()).unwrap();
        let f = &tab.entries[0];
        assert_eq!(f.name, "hello.txt");
        assert_eq!(f.size, 5);
        if cfg!(windows) {
            assert_eq!(f.perm, "rw");
        } else {
            assert_eq!(f.perm, "644");
        }
    }

    #[test]
    fn enter_selected_navigates_and_records_history() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("sub").join("inner.txt"), "").unwrap();
        fs::write(tmp.path().join("file.txt"), "").unwrap();
        let mut tab = Tab::from_physical(tmp.path(), Config::default()).unwrap();

        // 排序后 "sub/" 在第 0 位（目录优先）。
        tab.enter_selected(Config::default()).unwrap();
        assert_eq!(tab.location, Location::Physical(tmp.path().join("sub")));
        let names: Vec<&str> = tab.entries.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["inner.txt"]);
        assert_eq!(tab.cursor, 0);
        assert_eq!(
            tab.history,
            vec![Location::Physical(tmp.path().to_path_buf())]
        );
    }

    #[test]
    fn enter_selected_ignores_files_and_missing_dirs() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("file.txt"), "").unwrap();
        fs::create_dir(tmp.path().join("gone")).unwrap();
        let mut tab = Tab::from_physical(tmp.path(), Config::default()).unwrap();

        // 光标落在文件上：no-op。
        tab.cursor = 1; // "file.txt"（"gone" 为目录在前）
        tab.enter_selected(Config::default()).unwrap();
        assert_eq!(tab.location, Location::Physical(tmp.path().to_path_buf()));
        assert!(tab.history.is_empty());

        // 目录在加载后被删除：读取失败返回 Err，不导航（M6-1 错误提示接入）。
        fs::remove_dir(tmp.path().join("gone")).unwrap();
        tab.cursor = 0;
        assert!(tab.enter_selected(Config::default()).is_err());
        assert_eq!(tab.location, Location::Physical(tmp.path().to_path_buf()));
        assert!(tab.history.is_empty());
    }

    #[test]
    fn return_parent_goes_up_and_noop_at_root() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        let mut tab = Tab::from_physical(&tmp.path().join("sub"), Config::default()).unwrap();

        tab.return_parent(Config::default()).unwrap();
        assert_eq!(tab.location, Location::Physical(tmp.path().to_path_buf()));
        assert_eq!(tab.history.len(), 1);

        // 根目录：parent() 为 None（Windows 盘根同理），no-op。
        let mut root_tab = Tab {
            location: Location::Physical(PathBuf::from("C:\\")),
            entries: Vec::new(),
            cursor: 0,
            scroll: 0,
            search: None,
            history: Vec::new(),
            visual: None,
            filter: None,
            tags: HashSet::new(),
        };
        let _ = root_tab.return_parent(Config::default()); // 根目录 no-op 返回 Ok
        assert_eq!(root_tab.location, Location::Physical(PathBuf::from("C:\\")));
        assert!(root_tab.history.is_empty());
    }
}
