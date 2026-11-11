//! `Location` 与 `Tab`：浏览位置模型与每标签独立状态（SPEC §4.1 / §4.2）。
//!
//! 归档浏览行为在 M7 接入；`Location::Archive` 的数据形状现在固定，
//! 避免后期改动所有 tab 相关代码。

use std::path::{Path, PathBuf};

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
}

/// 单个目录项。M1-4 扩展大小/权限等元数据。
#[derive(Debug, Clone)]
pub struct FileItem {
    pub name: String,
    pub is_dir: bool,
}

/// 搜索状态。`query` 由 M1-7 的 `/` 与 `n/N` 读取。
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SearchState {
    pub query: String,
}

/// 每标签独立状态：位置、条目、光标、滚动、搜索、历史（SPEC §4.2）。
/// `scroll` 由 M1-3 翻页读写，`search` 由 M1-7 读写，`history` 由 M1-2 写入；
/// `cursor`/`location`/`entries` 已接线。
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Tab {
    pub location: Location,
    pub entries: Vec<FileItem>,
    pub cursor: usize,
    pub scroll: usize,
    pub search: Option<SearchState>,
    pub history: Vec<Location>,
}

impl Tab {
    /// 从物理目录构建标签：加载并排序条目（目录在前、名称不区分大小写）。
    pub fn from_physical(dir: &Path) -> std::io::Result<Self> {
        Ok(Self {
            location: Location::Physical(dir.to_path_buf()),
            entries: load_entries(dir)?,
            cursor: 0,
            scroll: 0,
            search: None,
            history: Vec::new(),
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

    /// 进入光标选中的目录（M1-2，`l`/`Enter`）。目录重读即刷新；
    /// 读取失败时不导航、不产生副作用（错误提示在 M6-1 接入）。
    /// 归档内导航在 M7 接入，当前仅物理目录。
    pub fn enter_selected(&mut self) {
        let Some(item) = self.entries.get(self.cursor) else {
            return;
        };
        if !item.is_dir {
            return;
        }
        let Location::Physical(base) = &self.location else {
            return;
        };
        let target = base.join(&item.name);
        let Ok(entries) = load_entries(&target) else {
            return;
        };
        self.history.push(self.location.clone());
        self.location = Location::Physical(target);
        self.entries = entries;
        self.cursor = 0;
        self.scroll = 0;
    }

    /// 返回父目录（M1-2，`h`）。已在文件系统根时为 no-op（ACCEPTANCE §3）。
    pub fn return_parent(&mut self) {
        let Location::Physical(base) = &self.location else {
            return;
        };
        let Some(parent) = base.parent() else {
            return;
        };
        if parent == base {
            return;
        }
        let Ok(entries) = load_entries(parent) else {
            return;
        };
        self.history.push(self.location.clone());
        self.location = Location::Physical(parent.to_path_buf());
        self.entries = entries;
        self.cursor = 0;
        self.scroll = 0;
    }
}

/// 读取并排序目录条目：目录在前，名称不区分大小写。
fn load_entries(dir: &Path) -> std::io::Result<Vec<FileItem>> {
    let mut items = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let is_dir = entry.file_type()?.is_dir();
        let name = entry.file_name().to_string_lossy().into_owned();
        items.push(FileItem { name, is_dir });
    }
    items.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

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

        let tab = Tab::from_physical(tmp.path()).unwrap();
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
        let mut tab = Tab::from_physical(tmp.path()).unwrap();
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
        let mut tab = Tab::from_physical(tmp.path()).unwrap();
        tab.move_cursor(1);
        tab.move_cursor(-1);
        assert_eq!(tab.cursor, 0);
    }

    #[test]
    fn enter_selected_navigates_and_records_history() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("sub").join("inner.txt"), "").unwrap();
        fs::write(tmp.path().join("file.txt"), "").unwrap();
        let mut tab = Tab::from_physical(tmp.path()).unwrap();

        // 排序后 "sub/" 在第 0 位（目录优先）。
        tab.enter_selected();
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
        let mut tab = Tab::from_physical(tmp.path()).unwrap();

        // 光标落在文件上：no-op。
        tab.cursor = 1; // "file.txt"（"gone" 为目录在前）
        tab.enter_selected();
        assert_eq!(tab.location, Location::Physical(tmp.path().to_path_buf()));
        assert!(tab.history.is_empty());

        // 目录在加载后被删除：读取失败，不导航。
        fs::remove_dir(tmp.path().join("gone")).unwrap();
        tab.cursor = 0;
        tab.enter_selected();
        assert_eq!(tab.location, Location::Physical(tmp.path().to_path_buf()));
        assert!(tab.history.is_empty());
    }

    #[test]
    fn return_parent_goes_up_and_noop_at_root() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        let mut tab = Tab::from_physical(&tmp.path().join("sub")).unwrap();

        tab.return_parent();
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
        };
        root_tab.return_parent();
        assert_eq!(root_tab.location, Location::Physical(PathBuf::from("C:\\")));
        assert!(root_tab.history.is_empty());
    }
}
