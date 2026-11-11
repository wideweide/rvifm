//! 归档只读 VFS 抽象（M7-1；SPEC §4.5、NOTES §4.4）。
//!
//! `ArchiveFs` 抽象 list/read/metadata；`ZipFs`/`TarFs` 在 M7-2/M7-3 分别实现。
//! 归档内路径（`inner`）统一用 `/` 分隔，根为空串；导航层（M7-4）通过
//! `Location::Archive` 分派，不把 zip/tar 分支塞进导航核心。

// M7-1 仅固化抽象形状；消费方：M7-2（ZipFs 实现）与 M7-4（导航接入）。
#![allow(dead_code)]

use std::path::Path;

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
    /// 列出 `dir` 下的直接子项（目录条目聚合，不递归）。
    fn list(&self, dir: &str) -> std::io::Result<Vec<ArchiveEntry>>;
    /// 读取文件内容（目录返回 NotFound）。
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>>;
    /// 查询单个条目元数据（不存在返回 NotFound）。
    fn metadata(&self, path: &str) -> std::io::Result<ArchiveEntry>;
    /// 归档宿主文件路径（`Location::Archive.host` 同源）。
    fn host(&self) -> &Path;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::Mutex;

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

        fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
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
        let fs = MockFs::new(Path::new("a.zip"));
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
