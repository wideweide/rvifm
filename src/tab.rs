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
/// `cursor`/`scroll` 由 M1-1/M1-3 读写，`search` 由 M1-7 读写，
/// `history` 由 M1-2 读写；M1-0 仅落地形状。
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
}
