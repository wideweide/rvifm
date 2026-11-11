//! 归档只读 VFS 抽象（M7-1；SPEC §4.5、NOTES §4.4）。
//!
//! `ArchiveFs` 抽象 list/read/metadata；`ZipFs`/`TarFs` 在 M7-2/M7-3 分别实现。
//! 归档内路径（`inner`）统一用 `/` 分隔，根为空串；导航层（M7-4）通过
//! `Location::Archive` 分派，不把 zip/tar 分支塞进导航核心。

// M7-1 仅固化抽象形状；消费方：M7-2（ZipFs 实现）与 M7-4（导航接入）。
// M7-2 的 ZipFs 公开但尚未接入导航。
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

/// 归档内条目（M7-1）。`path` 以 `/` 分隔、不带前导 `/`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry {
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
}

/// 归档内路径规范化（M7-1）：统一 `/`、去前导与尾随 `/`、折叠重复 `/`、
/// 解析 `.` 与 `..`（不得逃逸出归档根）。渲染、比较、索引前必须规范化。
pub fn normalize_inner(inner: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for part in inner.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            p => out.push(p),
        }
    }
    out.join("/")
}

/// 归档只读 VFS（M7-1）。实现方在打开归档时建立成员索引；读取按需解码，
/// 内存占用受最大单成员约束。所有 `dir`/`path` 参数必须先经 `normalize_inner`。
pub trait ArchiveFs {
    /// 归档宿主文件路径（`Location::Archive.host` 同源）。
    /// 列出 `dir` 下的直接子项（目录条目聚合，不递归）。
    fn list(&self, dir: &str) -> std::io::Result<Vec<ArchiveEntry>>;
    /// 读取文件内容（目录返回 NotFound）。需要可变借用（zip 随机访问 API）。
    fn read(&mut self, path: &str) -> std::io::Result<Vec<u8>>;
    /// 查询单个条目元数据（不存在返回 NotFound）。
    fn metadata(&self, path: &str) -> std::io::Result<ArchiveEntry>;
    /// 归档宿主文件路径（`Location::Archive.host` 同源）。
    fn host(&self) -> &Path;
}

/// 拆出 `(父目录, 叶子名)`；根级条目父目录为空串。
fn split_parent(path: &str) -> Option<(&str, &str)> {
    path.rsplit_once('/')
}

/// zip 归档实现（M7-2，基于 `zip` crate）。打开时建立成员索引与隐式目录
/// 集合；`ZipArchive` 持有文件句柄，读取按需解码。
pub struct ZipFs {
    archive: zip::ZipArchive<std::fs::File>,
    /// 规范化文件名 → (条目索引, 解压后大小)。
    index: BTreeMap<String, (usize, u64)>,
    /// 显式与隐式目录集合（规范化路径）。
    dirs: BTreeSet<String>,
    host: PathBuf,
}

impl ZipFs {
    /// 打开 zip 归档并建立索引。损坏/加密归档返回 InvalidData（M7-7 细化提示）。
    pub fn open(host: &Path) -> std::io::Result<Self> {
        let file = std::fs::File::open(host)?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let mut index = BTreeMap::new();
        let mut dirs = BTreeSet::new();
        for i in 0..archive.len() {
            let entry = archive
                .by_index(i)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            let name = entry
                .name()
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?
                .into_owned();
            if name.ends_with('/') {
                dirs.insert(normalize_inner(&name));
                continue;
            }
            let norm = normalize_inner(&name);
            index.insert(norm.clone(), (i, entry.size()));
            // 隐式父目录（zip 不保证显式目录条目存在）：只收集祖先前缀，
            // 不把文件自身路径插入 dirs。
            let parts: Vec<&str> = norm.split('/').collect();
            for i in 0..parts.len().saturating_sub(1) {
                dirs.insert(parts[..=i].join("/"));
            }
        }
        Ok(Self {
            archive,
            index,
            dirs,
            host: host.to_path_buf(),
        })
    }

    /// 列出 `dir` 下直接子项：目录条目（聚合，不递归）与文件条目。
    /// 大小取自开档时缓存的索引，不触碰 `ZipArchive`（避免可变借用）。
    fn list_children(&self, dir: &str) -> BTreeMap<String, (bool, u64, String)> {
        let mut out: BTreeMap<String, (bool, u64, String)> = BTreeMap::new();
        for name in &self.dirs {
            // 根级目录无 `/`，父目录为空串。
            let (parent, leaf) = split_parent(name).unwrap_or(("", name.as_str()));
            if parent == dir {
                out.insert(leaf.to_owned(), (true, 0, name.clone()));
            }
        }
        for (name, &(_idx, size)) in &self.index {
            let (parent, leaf) = match split_parent(name) {
                Some((p, l)) => (p, l),
                None => ("", name.as_str()),
            };
            if parent == dir {
                out.insert(leaf.to_owned(), (false, size, name.clone()));
            }
        }
        out
    }
}

impl std::fmt::Debug for ZipFs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ZipFs").field("host", &self.host).finish()
    }
}

impl ArchiveFs for ZipFs {
    fn list(&self, dir: &str) -> std::io::Result<Vec<ArchiveEntry>> {
        let dir = normalize_inner(dir);
        Ok(self
            .list_children(&dir)
            .into_values()
            .map(|(is_dir, size, path)| ArchiveEntry { path, is_dir, size })
            .collect())
    }

    fn read(&mut self, path: &str) -> std::io::Result<Vec<u8>> {
        let path = normalize_inner(path);
        let &(idx, size) = self
            .index
            .get(&path)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "条目不存在"))?;
        let mut file = self
            .archive
            .by_index(idx)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let mut buf = Vec::with_capacity(size as usize);
        file.read_to_end(&mut buf)?;
        Ok(buf)
    }

    fn metadata(&self, path: &str) -> std::io::Result<ArchiveEntry> {
        let path = normalize_inner(path);
        if self.dirs.contains(&path) {
            return Ok(ArchiveEntry {
                path,
                is_dir: true,
                size: 0,
            });
        }
        let &(_idx, size) = self
            .index
            .get(&path)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "条目不存在"))?;
        Ok(ArchiveEntry {
            path,
            is_dir: false,
            size,
        })
    }

    fn host(&self) -> &Path {
        &self.host
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::io::Write as _;
    use std::sync::Mutex;
    use tempfile::TempDir;

    /// 测试用内存实现：验证 trait 形状与导航所需语义（M7-2 换 zip 实现）。
    struct MockFs {
        entries: Mutex<BTreeMap<String, (bool, u64, Vec<u8>)>>,
        host: PathBuf,
    }

    impl MockFs {
        fn new(host: &Path) -> Self {
            Self {
                entries: Mutex::new(BTreeMap::new()),
                host: host.to_path_buf(),
            }
        }

        fn add_dir(&self, path: &str) {
            self.entries
                .lock()
                .unwrap()
                .insert(normalize_inner(path), (true, 0, Vec::new()));
        }

        fn add_file(&self, path: &str, data: &[u8]) {
            self.entries.lock().unwrap().insert(
                normalize_inner(path),
                (false, data.len() as u64, data.to_vec()),
            );
        }
    }

    impl ArchiveFs for MockFs {
        fn list(&self, dir: &str) -> std::io::Result<Vec<ArchiveEntry>> {
            let dir = normalize_inner(dir);
            let map = self.entries.lock().unwrap();
            let mut out: Vec<ArchiveEntry> = Vec::new();
            let mut seen = BTreeMap::new();
            for (path, (is_dir, size, _)) in map.iter() {
                let (parent, name) = match path.rsplit_once('/') {
                    Some((p, n)) => (p.to_owned(), n.to_owned()),
                    None => (String::new(), path.clone()),
                };
                if parent != dir {
                    continue;
                }
                seen.insert(
                    name,
                    ArchiveEntry {
                        path: path.clone(),
                        is_dir: *is_dir,
                        size: *size,
                    },
                );
            }
            out.extend(seen.into_values());
            Ok(out)
        }

        fn read(&mut self, path: &str) -> std::io::Result<Vec<u8>> {
            let path = normalize_inner(path);
            let map = self.entries.lock().unwrap();
            match map.get(&path) {
                Some((false, _, data)) => Ok(data.clone()),
                Some((true, _, _)) => Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "目录不可读",
                )),
                None => Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "条目不存在",
                )),
            }
        }

        fn metadata(&self, path: &str) -> std::io::Result<ArchiveEntry> {
            let path = normalize_inner(path);
            let map = self.entries.lock().unwrap();
            match map.get(&path) {
                Some((is_dir, size, _)) => Ok(ArchiveEntry {
                    path,
                    is_dir: *is_dir,
                    size: *size,
                }),
                None => Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "条目不存在",
                )),
            }
        }

        fn host(&self) -> &Path {
            &self.host
        }
    }

    use std::path::PathBuf;

    fn make_test_zip(path: &Path) {
        // M7-2：构造含显式目录、嵌套文件与隐式父目录的测试 zip。
        let file = std::fs::File::create(path).unwrap();
        let mut w = zip::ZipWriter::new(file);
        let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();
        w.add_directory("src", opts).unwrap();
        w.start_file("src/main.rs", opts).unwrap();
        w.write_all(b"fn main() {}").unwrap();
        w.start_file("readme.md", opts).unwrap();
        w.write_all(b"hi").unwrap();
        w.start_file("deep/nested/blob.bin", opts).unwrap();
        w.write_all(b"ABC").unwrap(); // 隐式父目录 deep/ 与 deep/nested/
        w.finish().unwrap();
    }

    #[test]
    fn zip_fs_list_read_metadata_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("t.zip");
        make_test_zip(&zip_path);
        let mut fs = ZipFs::open(&zip_path).unwrap();

        // 根列表：显式目录 src、文件 readme.md、隐式目录 deep。
        let root = fs.list("").unwrap();
        let names: Vec<(&str, bool)> = root.iter().map(|e| (e.path.as_str(), e.is_dir)).collect();
        assert_eq!(names, [("deep", true), ("readme.md", false), ("src", true)]);

        // 子目录列表 + 读取 + 元数据。
        let src = fs.list("src/").unwrap();
        assert_eq!(src.len(), 1);
        assert_eq!(src[0].path, "src/main.rs");
        assert_eq!(fs.read("src/main.rs").unwrap(), b"fn main() {}");
        assert_eq!(fs.read("/src/main.rs").unwrap(), b"fn main() {}");
        assert_eq!(fs.read("deep/nested/blob.bin").unwrap(), b"ABC");
        assert!(fs.read("src").is_err()); // 目录不可读
        assert_eq!(fs.metadata("readme.md").unwrap().size, 2);
        assert!(fs.metadata("deep").unwrap().is_dir);
        assert!(fs.metadata("nope").is_err());
    }

    #[test]
    fn zip_fs_corrupt_archive_returns_error_not_panic() {
        // M7-2/M7-7 前置：损坏 zip 返回 InvalidData 而非 panic。
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("bad.zip");
        std::fs::write(&zip_path, b"PKgarbage").unwrap();
        let err = ZipFs::open(&zip_path).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn normalize_inner_unifies_separators_and_dots() {
        assert_eq!(normalize_inner(""), "");
        assert_eq!(normalize_inner("/"), "");
        assert_eq!(normalize_inner("src//main//"), "src/main");
        assert_eq!(normalize_inner("/src/./main/"), "src/main");
        assert_eq!(normalize_inner("src/../docs"), "docs");
        // `..` 不得逃逸根：多出的 `..` 被丢弃。
        assert_eq!(normalize_inner("../../etc"), "etc");
    }

    #[test]
    fn mock_fs_list_read_metadata_roundtrip() {
        let mut fs = MockFs::new(Path::new("a.zip"));
        fs.add_dir("src");
        fs.add_file("src/main.rs", b"fn main() {}");
        fs.add_file("readme.md", b"hi");

        // 根列表只含直接子项。
        let root = fs.list("").unwrap();
        let names: Vec<&str> = root.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(names, ["readme.md", "src"]);

        // 子目录列表。
        let src = fs.list("src/").unwrap();
        assert_eq!(src.len(), 1);
        assert_eq!(src[0].path, "src/main.rs");
        assert!(!src[0].is_dir);

        // 读取与元数据。
        assert_eq!(fs.read("src/main.rs").unwrap(), b"fn main() {}");
        assert_eq!(fs.read("/src/main.rs").unwrap(), b"fn main() {}"); // 规范化
        assert!(fs.read("src").is_err()); // 目录不可读
        assert_eq!(fs.metadata("readme.md").unwrap().size, 2);
        assert!(fs.metadata("nope").is_err());

        // host 同源。
        assert_eq!(fs.host(), Path::new("a.zip"));
    }
}
