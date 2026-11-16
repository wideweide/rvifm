//! 远端文件系统抽象（M19-1）：协议无关 trait + 可分类错误。
//!
//! 与 `ArchiveFs` 同构（list/read/metadata）；SFTP 后端按 M19-3a spike
//! 结论实现（M19-3b），写操作由 M20 填充（本阶段默认"未实现"）。
//! 不引入具体协议依赖。

use std::path::Path;

/// 远端操作错误（M19-1：可分类，UI 据此给出针对性提示）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteError {
    /// 连接失败（网络不可达/DNS/握手）。
    Connect(String),
    /// 认证失败（密钥被拒/密码错误）。
    Auth(String),
    /// 路径不存在。
    NotFound(String),
    /// 权限不足。
    PermissionDenied(String),
    /// 超时。
    Timeout(String),
    /// 功能未实现（M20 前的写操作占位）。
    Unimplemented(&'static str),
    /// 其他（携带原始描述）。
    Other(String),
}

impl std::fmt::Display for RemoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RemoteError::Connect(d) => write!(f, "连接失败: {d}"),
            RemoteError::Auth(d) => write!(f, "认证失败: {d}"),
            RemoteError::NotFound(d) => write!(f, "远端路径不存在: {d}"),
            RemoteError::PermissionDenied(d) => write!(f, "权限不足: {d}"),
            RemoteError::Timeout(d) => write!(f, "超时: {d}"),
            RemoteError::Unimplemented(op) => write!(f, "远端操作未实现: {op}"),
            RemoteError::Other(d) => write!(f, "远端错误: {d}"),
        }
    }
}

impl std::error::Error for RemoteError {}

/// 远端条目（list 结果；与本地 FileItem 对齐的字段子集）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteEntry {
    /// 条目名（不含路径）。
    pub name: String,
    pub is_dir: bool,
    /// 字节大小（目录为 0）。
    pub size: u64,
    /// 修改时间（Unix 秒；后端不可得时 0）。
    pub mtime: u64,
}

/// 远端元数据（metadata 结果）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteMeta {
    pub size: u64,
    pub mtime: u64,
    pub is_dir: bool,
}

/// 协议无关的远端文件系统 trait（M19-1，与 ArchiveFs 同构）。
///
/// 路径参数为远端**绝对路径**（`/` 分隔，如 `/home/user/a.txt`）。
/// 网络实现必须保证不阻塞调用线程（M19-3b：超时/异步由后端负责）。
pub trait RemoteFs {
    /// 列出 `path` 目录下的直接子项（不递归）。
    fn list(&self, path: &Path) -> Result<Vec<RemoteEntry>, RemoteError>;
    /// 读取文件内容（目录返回 NotFound）。
    fn read(&self, path: &Path) -> Result<Vec<u8>, RemoteError>;
    /// 查询单个条目元数据。
    fn metadata(&self, path: &Path) -> Result<RemoteMeta, RemoteError>;

    // ── M20：写操作（M19 阶段默认"未实现"，后端按需覆盖）──

    /// 写入文件（整文件覆盖；父目录须存在）。
    fn write(&self, path: &Path, data: &[u8]) -> Result<(), RemoteError> {
        let _ = (path, data);
        Err(RemoteError::Unimplemented("write"))
    }
    /// 删除文件或空目录（非空目录由调用方先清空）。
    fn remove(&self, path: &Path) -> Result<(), RemoteError> {
        let _ = path;
        Err(RemoteError::Unimplemented("remove"))
    }
    /// 重命名/移动。
    fn rename(&self, from: &Path, to: &Path) -> Result<(), RemoteError> {
        let _ = (from, to);
        Err(RemoteError::Unimplemented("rename"))
    }
    /// 创建目录。
    fn create_dir(&self, path: &Path) -> Result<(), RemoteError> {
        let _ = path;
        Err(RemoteError::Unimplemented("create_dir"))
    }
}

pub mod sftp;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// 内存 mock：验证 trait 人体工学与默认写操作"未实现"。
    struct MockFs {
        entries: Vec<RemoteEntry>,
    }

    impl RemoteFs for MockFs {
        fn list(&self, _path: &Path) -> Result<Vec<RemoteEntry>, RemoteError> {
            Ok(self.entries.clone())
        }
        fn read(&self, path: &Path) -> Result<Vec<u8>, RemoteError> {
            if path == Path::new("/a.txt") {
                Ok(b"hello".to_vec())
            } else {
                Err(RemoteError::NotFound(path.display().to_string()))
            }
        }
        fn metadata(&self, path: &Path) -> Result<RemoteMeta, RemoteError> {
            if path == Path::new("/a.txt") {
                Ok(RemoteMeta {
                    size: 5,
                    mtime: 100,
                    is_dir: false,
                })
            } else {
                Err(RemoteError::NotFound(path.display().to_string()))
            }
        }
    }

    fn mock() -> MockFs {
        MockFs {
            entries: vec![RemoteEntry {
                name: "a.txt".to_owned(),
                is_dir: false,
                size: 5,
                mtime: 100,
            }],
        }
    }

    #[test]
    fn mock_list_read_metadata_roundtrip() {
        let fs = mock();
        let items = fs.list(Path::new("/")).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "a.txt");
        assert_eq!(fs.read(Path::new("/a.txt")).unwrap(), b"hello");
        let meta = fs.metadata(Path::new("/a.txt")).unwrap();
        assert_eq!(meta.size, 5);
        assert!(!meta.is_dir);
        // 未命中路径 → NotFound 分类。
        assert_eq!(
            fs.read(Path::new("/missing")),
            Err(RemoteError::NotFound("/missing".to_owned()))
        );
    }

    #[test]
    fn default_write_ops_are_unimplemented() {
        let fs = mock();
        assert_eq!(
            fs.write(Path::new("/a.txt"), b"x"),
            Err(RemoteError::Unimplemented("write"))
        );
        assert_eq!(
            fs.remove(Path::new("/a.txt")),
            Err(RemoteError::Unimplemented("remove"))
        );
        assert_eq!(
            fs.rename(Path::new("/a"), Path::new("/b")),
            Err(RemoteError::Unimplemented("rename"))
        );
        assert_eq!(
            fs.create_dir(Path::new("/d")),
            Err(RemoteError::Unimplemented("create_dir"))
        );
    }

    #[test]
    fn error_display_is_readable() {
        assert_eq!(
            RemoteError::Connect(" refused ".trim().to_owned()).to_string(),
            "连接失败: refused"
        );
        assert_eq!(
            RemoteError::Auth("密钥被拒".to_owned()).to_string(),
            "认证失败: 密钥被拒"
        );
        assert_eq!(
            RemoteError::Timeout("10s".to_owned()).to_string(),
            "超时: 10s"
        );
        assert_eq!(
            RemoteError::Unimplemented("write").to_string(),
            "远端操作未实现: write"
        );
    }

    #[test]
    fn trait_is_object_safe_for_dyn_use() {
        // M19-2/M19-3b 将以 Box<dyn RemoteFs> 持有后端——此处锁定对象安全。
        let fs: Box<dyn RemoteFs> = Box::new(mock());
        assert_eq!(fs.read(Path::new("/a.txt")).unwrap(), b"hello");
        let _path: Option<PathBuf> = None;
    }
}
