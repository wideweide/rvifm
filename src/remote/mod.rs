//! 远端文件系统抽象（M19-1）：协议无关 trait + 可分类错误。
//!
//! 与 `ArchiveFs` 同构（list/read/metadata）；SFTP 后端按 M19-3a spike
//! 结论实现（M19-3b），写操作由 M20 填充（本阶段默认"未实现"）。
//! 不引入具体协议依赖。

use std::path::{Path, PathBuf};

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

impl RemoteError {
    /// M32-1：连接类错误判定（重连候选）——Connect/Timeout 属传输层
    /// 失效（网络断/握手超时），重连可能恢复；Auth（凭据问题重连同样
    /// 失败，且 M22-3 有独立口令流程）、NotFound/PermissionDenied（远端
    /// 语义错误）、Other（不明）不触发自动重连——避免无意义重试与
    /// 重连循环。
    pub fn is_connection_class(&self) -> bool {
        matches!(self, RemoteError::Connect(_) | RemoteError::Timeout(_))
    }
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
    /// M32-1：downcast 支持（SftpBackend 自动重连取连接参数副本用；
    /// 默认实现空 Any——具体后端按需覆盖）。
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        None
    }

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
    /// M36-3：范围读（从 `offset` 起读 `len` 字节；默认实现 = read +
    /// 偏移切片——兼容 mock 与未覆盖后端；SftpBackend 覆盖为真范围请求）。
    fn read_range(&self, path: &Path, offset: u64, len: u64) -> Result<Vec<u8>, RemoteError> {
        let data = self.read(path)?;
        let start = (offset as usize).min(data.len());
        let end = (start + len as usize).min(data.len());
        Ok(data[start..end].to_vec())
    }

    /// M36-3：范围写（`offset` 处写入 `data`——偏移超长时行为 = 追加/截断
    /// 由后端定义；默认实现 = read 全量 + 拼接 + 整写）。
    fn write_range(&self, path: &Path, offset: u64, data: &[u8]) -> Result<(), RemoteError> {
        let mut cur = if offset == 0 {
            Vec::new()
        } else {
            self.read(path).unwrap_or_default()
        };
        let start = (offset as usize).min(cur.len());
        cur.resize(start.max(cur.len()), 0);
        let end = start + data.len();
        if end > cur.len() {
            cur.resize(end, 0);
        }
        cur[start..end].copy_from_slice(data);
        self.write(path, &cur)
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

    /// M20-1：可恢复删除——rename 至后端 trash 目录，返回 trash 路径。
    fn move_to_trash(&self, path: &Path) -> Result<PathBuf, RemoteError> {
        let _ = path;
        Err(RemoteError::Unimplemented("move_to_trash"))
    }

    /// M20-1：从 trash 恢复（撤销删除）。
    fn restore_from_trash(&self, trash_path: &Path, orig: &Path) -> Result<(), RemoteError> {
        let _ = (trash_path, orig);
        Err(RemoteError::Unimplemented("restore_from_trash"))
    }

    /// M20-1：永久删除（Create 撤销用；绕过 trash）。
    fn remove_permanent(&self, path: &Path) -> Result<(), RemoteError> {
        let _ = path;
        Err(RemoteError::Unimplemented("remove_permanent"))
    }

    /// M20-1：删除空目录（Create 目录撤销用；绕过 trash）。
    fn remove_dir(&self, path: &Path) -> Result<(), RemoteError> {
        let _ = path;
        Err(RemoteError::Unimplemented("remove_dir"))
    }
}

pub mod config;
pub mod known_hosts;
pub mod sftp;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn m363_read_range_write_range_default_impl() {
        // M36-3：默认实现（mock 未覆盖）——read_range 对 NotFound 路径
        // Err 不 panic；write_range offset=0 走默认 write（mock → Unimplemented）。
        let mock = MockFs {
            entries: Vec::new(),
        };
        let r = mock.read_range(Path::new("/x"), 0, 10);
        assert!(r.is_err(), "mock read 失败路径不 panic");
        let r = mock.write_range(Path::new("/x"), 0, b"data");
        assert!(matches!(r, Err(RemoteError::Unimplemented("write"))));
    }

    #[test]
    fn m321_connection_class_classification() {
        // M32-1：连接类判定——Connect/Timeout true；Auth/NotFound/
        // PermissionDenied/Other false（无意义重试与重连循环防护）。
        assert!(RemoteError::Connect("x".to_owned()).is_connection_class());
        assert!(RemoteError::Timeout("t".to_owned()).is_connection_class());
        assert!(!RemoteError::Auth("a".to_owned()).is_connection_class());
        assert!(!RemoteError::NotFound("n".to_owned()).is_connection_class());
        assert!(!RemoteError::PermissionDenied("p".to_owned()).is_connection_class());
        assert!(!RemoteError::Other("o".to_owned()).is_connection_class());
        assert!(!RemoteError::Unimplemented("u").is_connection_class());
    }

    #[test]
    fn m321_as_any_default_none_and_override() {
        // M32-1：as_any 默认 None（Mock 无重连语义）；SftpBackend 覆盖
        // 返回 Some（downcast 可达）。
        let mock = MockFs {
            entries: Vec::new(),
        };
        assert!(mock.as_any().is_none(), "默认实现 None");
    }

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
