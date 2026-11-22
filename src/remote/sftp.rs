//! M19-3b：SFTP 后端（russh + russh-sftp，spike 锁定方案）。
//!
//! 架构：专用后台线程持有 current-thread tokio runtime 与 SSH/SFTP 会话；
//! 同步接口经 `std::sync::mpsc` 提交命令、等待带超时的回复——tokio 语义
//! 不跨线程，调用线程（UI）绝不运行 reactor。
//!
//! 认证：密钥（显式 key → ~/.ssh/id_ed25519 → id_rsa）；口令后续增强。
//! 错误映射：russh/sftp → [`RemoteError`] 分类（连接/认证/不存在/权限/超时）。

use std::path::{Path, PathBuf};
use std::sync::mpsc as std_mpsc;
use std::time::Duration;

use russh::client::{self, Handle};
use russh::keys::{PrivateKeyWithHashAlg, load_secret_key};
use russh_sftp::client::SftpSession;

use super::{RemoteEntry, RemoteError, RemoteFs, RemoteMeta};

/// 连接超时（含认证与 SFTP 握手）。
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// 单次操作超时（UI 侧等待上限）。
const OP_TIMEOUT: Duration = Duration::from_secs(30);
/// 默认 SSH 端口。
const DEFAULT_PORT: u16 = 22;

/// 连接参数（M19-4：来源 remotes.toml / ssh config / 默认推断）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SftpParams {
    pub host: String,
    pub port: u16,
    pub user: String,
    /// 密钥路径；None 时按 id_ed25519 → id_rsa 顺序探测 ~/.ssh/。
    pub key_path: Option<PathBuf>,
    /// 密钥路径原始串（供 :file 展示；不影响连接）。
    pub key_path_raw: Option<String>,
    /// 会话名（:file 展示 remote:/path 用）。
    pub name: String,
    /// 连接后的初始路径（remotes.toml root；缺省 /）。
    pub root: Option<PathBuf>,
    /// M22-2：跳过 known_hosts 校验（安全默认 false = 校验；M39-4 `no` 等效）。
    pub skip_host_key_check: bool,
    /// M39-4：首连主机密钥策略（ask 缺省——TOFU 改为交互确认）。
    pub strict_host_key: StrictHostKey,
    /// M39-4：指纹确认后重连标志（ask 路径置 true；首连写入放行）。
    pub accept_new_host_key: bool,
    /// M39-4：ProxyJump 跳板（`host[:port]`；ssh config 透传）。
    pub proxy_jump: Option<String>,
}

/// M39-4：首连主机密钥策略——`strict_host_key = ask|accept|no`（缺省 ask：
/// 首连交互确认指纹，接受后写入 known_hosts；accept = 旧 TOFU 自动写入；
/// no = 跳过校验，等效 `known_hosts = false`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StrictHostKey {
    #[default]
    Ask,
    Accept,
    No,
}

impl StrictHostKey {
    /// 解析配置串（非法值回落缺省 ask）。
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "ask" => Some(Self::Ask),
            "accept" => Some(Self::Accept),
            "no" => Some(Self::No),
            _ => None,
        }
    }
}

/// 缺省用户名（USER/USERNAME/root）。
pub fn default_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "root".to_owned())
}

impl SftpParams {
    /// M19-4 辅助：仅指定主机（其余默认）。
    pub fn new(host: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            port: DEFAULT_PORT,
            user: default_user(),
            key_path: None,
            key_path_raw: None,
            name: String::new(),
            root: None,
            skip_host_key_check: false,
            strict_host_key: StrictHostKey::default(),
            accept_new_host_key: false,
            proxy_jump: None,
        }
    }
}

/// actor 命令（回复通道随命令携带）。
enum SftpCmd {
    List {
        path: String,
        resp: std_mpsc::Sender<Result<Vec<RemoteEntry>, RemoteError>>,
    },
    Read {
        path: String,
        resp: std_mpsc::Sender<Result<Vec<u8>, RemoteError>>,
    },
    Metadata {
        path: String,
        resp: std_mpsc::Sender<Result<RemoteMeta, RemoteError>>,
    },
    Write {
        path: String,
        data: Vec<u8>,
        resp: std_mpsc::Sender<Result<(), RemoteError>>,
    },
    /// M36-3：范围读（offset 起 len 字节）。
    ReadRange {
        path: String,
        offset: u64,
        len: u64,
        resp: std_mpsc::Sender<Result<Vec<u8>, RemoteError>>,
    },
    /// M36-3：范围写（offset 处写入 data）。
    WriteRange {
        path: String,
        offset: u64,
        data: Vec<u8>,
        resp: std_mpsc::Sender<Result<(), RemoteError>>,
    },
    RemoveFile {
        path: String,
        resp: std_mpsc::Sender<Result<(), RemoteError>>,
    },
    RemoveDir {
        path: String,
        resp: std_mpsc::Sender<Result<(), RemoteError>>,
    },
    Rename {
        from: String,
        to: String,
        resp: std_mpsc::Sender<Result<(), RemoteError>>,
    },
    CreateDir {
        path: String,
        resp: std_mpsc::Sender<Result<(), RemoteError>>,
    },
    MoveToTrash {
        path: String,
        resp: std_mpsc::Sender<Result<String, RemoteError>>,
    },
    RestoreFromTrash {
        trash_path: String,
        orig: String,
        resp: std_mpsc::Sender<Result<(), RemoteError>>,
    },
}

/// SFTP 后端（实现 [`RemoteFs`]）。Clone 安全（内部为 mpsc Sender）。
#[derive(Clone)]
pub struct SftpBackend {
    cmd_tx: std_mpsc::Sender<SftpCmd>,
    /// M32-1：连接参数副本（自动重连用；口令认证的会话 password 不落盘
    /// ——重连仅密钥路径可行，口令会话重连失败给可读提示）。
    pub(crate) params: SftpParams,
    /// M32-1：口令（仅口令认证会话持有；密钥会话 None。进程内存驻留，
    /// 不写任何文件——M22-3 口令不落盘边界不变）。
    pub(crate) password: Option<String>,
}

impl SftpBackend {
    /// M32-1：连接类错误自动重连一次——按原参数（含口令，若会话以口令
    /// 认证）重建后端；成功返回新后端，失败返回原始错误。非连接类错误
    /// 原样返回 Err（is_connection_class 口径）。
    /// M32-1：&self 形态（downcast 后调用）——用 params/password 副本
    /// 重建后端；成功返回新后端（password 副本随迁）。
    pub fn reconnect_from_params(&self) -> Result<Self, RemoteError> {
        let r = if let Some(pw) = &self.password {
            SftpBackend::connect_with_password(&self.params, pw)
        } else {
            SftpBackend::connect(&self.params)
        };
        r.map(|mut b| {
            b.password = self.password.clone();
            b
        })
    }
}

impl std::fmt::Debug for SftpBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SftpBackend").finish_non_exhaustive()
    }
}

impl SftpBackend {
    /// 连接远端并启动 actor 线程（阻塞调用线程至连接完成/超时）。
    pub fn connect(params: &SftpParams) -> Result<Self, RemoteError> {
        let (cmd_tx, cmd_rx) = std_mpsc::channel::<SftpCmd>();
        let (init_tx, init_rx) = std_mpsc::channel::<Result<(), RemoteError>>();
        std::thread::Builder::new()
            .name("rvifm-sftp".to_owned())
            .spawn({
                let params = params.clone();
                move || actor_thread(params, None, cmd_rx, init_tx)
            })
            .map_err(|e| RemoteError::Other(format!("线程启动失败: {e}")))?;
        // 连接结果（actor 侧已含 CONNECT_TIMEOUT）。
        match init_rx.recv_timeout(CONNECT_TIMEOUT + OP_TIMEOUT) {
            Ok(Ok(())) => Ok(Self {
                cmd_tx,
                params: params.clone(),
                password: None,
            }),
            Ok(Err(e)) => Err(e),
            Err(std_mpsc::RecvTimeoutError::Timeout) => {
                Err(RemoteError::Timeout("连接超时".to_owned()))
            }
            Err(std_mpsc::RecvTimeoutError::Disconnected) => {
                Err(RemoteError::Other("SFTP actor 提前退出".to_owned()))
            }
        }
    }

    /// M22-3：口令认证入口（公钥失败回退；口令仅内存传递不落盘）。
    pub fn connect_with_password(params: &SftpParams, password: &str) -> Result<Self, RemoteError> {
        let (cmd_tx, cmd_rx) = std_mpsc::channel::<SftpCmd>();
        let (init_tx, init_rx) = std_mpsc::channel::<Result<(), RemoteError>>();
        std::thread::Builder::new()
            .name("rvifm-sftp".to_owned())
            .spawn({
                let params = params.clone();
                let password = password.to_owned();
                move || actor_thread(params, Some(password), cmd_rx, init_tx)
            })
            .map_err(|e| RemoteError::Other(format!("线程启动失败: {e}")))?;
        match init_rx.recv_timeout(CONNECT_TIMEOUT + OP_TIMEOUT) {
            Ok(Ok(())) => Ok(Self {
                cmd_tx,
                params: params.clone(),
                password: Some(password.to_owned()),
            }),
            Ok(Err(e)) => Err(e),
            Err(std_mpsc::RecvTimeoutError::Timeout) => {
                Err(RemoteError::Timeout("连接超时".to_owned()))
            }
            Err(std_mpsc::RecvTimeoutError::Disconnected) => {
                Err(RemoteError::Other("SFTP actor 提前退出".to_owned()))
            }
        }
    }

    /// 提交命令并等待带超时的回复（UI 线程仅阻塞此一次等待）。
    fn call<R>(
        &self,
        make: impl FnOnce(std_mpsc::Sender<Result<R, RemoteError>>) -> SftpCmd,
    ) -> Result<R, RemoteError> {
        let (resp_tx, resp_rx) = std_mpsc::channel();
        self.cmd_tx
            .send(make(resp_tx))
            .map_err(|_| RemoteError::Other("SFTP actor 已退出".to_owned()))?;
        match resp_rx.recv_timeout(OP_TIMEOUT) {
            Ok(r) => r,
            Err(std_mpsc::RecvTimeoutError::Timeout) => {
                Err(RemoteError::Timeout("操作超时".to_owned()))
            }
            Err(std_mpsc::RecvTimeoutError::Disconnected) => {
                Err(RemoteError::Other("SFTP actor 已退出".to_owned()))
            }
        }
    }
}

impl RemoteFs for SftpBackend {
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    /// M36-3：真范围读（SFTP read + seek——不整读）。
    fn read_range(&self, path: &Path, offset: u64, len: u64) -> Result<Vec<u8>, RemoteError> {
        let path = path_to_string(path);
        self.call(|resp| SftpCmd::ReadRange {
            path,
            offset,
            len,
            resp,
        })
    }

    /// M36-3：真范围写（SFTP open WRITE|CREATE 不截断 + seek 写）。
    fn write_range(&self, path: &Path, offset: u64, data: &[u8]) -> Result<(), RemoteError> {
        let path = path_to_string(path);
        self.call(|resp| SftpCmd::WriteRange {
            path,
            offset,
            data: data.to_vec(),
            resp,
        })
    }

    fn list(&self, path: &Path) -> Result<Vec<RemoteEntry>, RemoteError> {
        let path = path_to_string(path);
        self.call(|resp| SftpCmd::List { path, resp })
    }

    fn read(&self, path: &Path) -> Result<Vec<u8>, RemoteError> {
        let path = path_to_string(path);
        self.call(|resp| SftpCmd::Read { path, resp })
    }

    fn metadata(&self, path: &Path) -> Result<RemoteMeta, RemoteError> {
        let path = path_to_string(path);
        self.call(|resp| SftpCmd::Metadata { path, resp })
    }

    fn write(&self, path: &Path, data: &[u8]) -> Result<(), RemoteError> {
        let path = path_to_string(path);
        let data = data.to_vec();
        self.call(|resp| SftpCmd::Write { path, data, resp })
    }

    fn remove(&self, path: &Path) -> Result<(), RemoteError> {
        let path = path_to_string(path);
        self.call(|resp| SftpCmd::RemoveFile { path, resp })
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<(), RemoteError> {
        let from = path_to_string(from);
        let to = path_to_string(to);
        self.call(|resp| SftpCmd::Rename { from, to, resp })
    }

    fn create_dir(&self, path: &Path) -> Result<(), RemoteError> {
        let path = path_to_string(path);
        self.call(|resp| SftpCmd::CreateDir { path, resp })
    }

    fn move_to_trash(&self, path: &Path) -> Result<PathBuf, RemoteError> {
        let path = path_to_string(path);
        self.call(|resp| SftpCmd::MoveToTrash { path, resp })
            .map(PathBuf::from)
    }

    fn restore_from_trash(&self, trash_path: &Path, orig: &Path) -> Result<(), RemoteError> {
        let trash_path = path_to_string(trash_path);
        let orig = path_to_string(orig);
        self.call(|resp| SftpCmd::RestoreFromTrash {
            trash_path,
            orig,
            resp,
        })
    }

    fn remove_permanent(&self, path: &Path) -> Result<(), RemoteError> {
        let path = path_to_string(path);
        self.call(|resp| SftpCmd::RemoveFile { path, resp })
    }

    fn remove_dir(&self, path: &Path) -> Result<(), RemoteError> {
        let path = path_to_string(path);
        self.call(|resp| SftpCmd::RemoveDir { path, resp })
    }
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

// ── actor 线程 ───────────────────────────────────────────────────────

/// 后台线程主体：建 runtime → 连接 → 顺序执行命令（每命令独立超时）。
fn actor_thread(
    params: SftpParams,
    password: Option<String>,
    cmd_rx: std_mpsc::Receiver<SftpCmd>,
    init_tx: std_mpsc::Sender<Result<(), RemoteError>>,
) {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            let _ = init_tx.send(Err(RemoteError::Other(e.to_string())));
            return;
        }
    };
    // 连接（含认证与 SFTP 握手，整体超时）。
    let session = match rt.block_on(async {
        tokio::time::timeout(CONNECT_TIMEOUT, connect_async(&params, password.as_deref())).await
    }) {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            let _ = init_tx.send(Err(e));
            return;
        }
        Err(_) => {
            let _ = init_tx.send(Err(RemoteError::Timeout("连接超时".to_owned())));
            return;
        }
    };
    if init_tx.send(Ok(())).is_err() {
        return; // 调用方已放弃
    }
    // 命令循环（顺序执行；UI 侧另有等待超时兜底）。
    while let Ok(cmd) = cmd_rx.recv() {
        rt.block_on(handle_cmd(&session, cmd));
    }
    // 会话收尾（尽力而为）。
    let _ = rt.block_on(session.handle.disconnect(
        russh::Disconnect::ByApplication,
        "rvifm session closed",
        "en",
    ));
}

/// 单命令执行（async 侧）。
async fn handle_cmd(session: &SshSession, cmd: SftpCmd) {
    match cmd {
        SftpCmd::List { path, resp } => {
            let r = tokio::time::timeout(OP_TIMEOUT, session.sftp.read_dir(&path))
                .await
                .map_err(|_| RemoteError::Timeout("list 超时".to_owned()))
                .and_then(|dir| {
                    dir.map_err(sftp_err).map(|dir| {
                        let mut out = Vec::new();
                        for e in dir {
                            out.push(RemoteEntry {
                                name: e.file_name(),
                                is_dir: e.metadata().is_dir(),
                                size: e.metadata().len(),
                                mtime: mtime_of(&e.metadata()),
                            });
                        }
                        out.sort_by(|a, b| a.name.cmp(&b.name));
                        out
                    })
                });
            let _ = resp.send(r);
        }
        SftpCmd::Read { path, resp } => {
            let r = tokio::time::timeout(OP_TIMEOUT, session.sftp.read(&path))
                .await
                .map_err(|_| RemoteError::Timeout("read 超时".to_owned()))
                .and_then(|r| r.map_err(sftp_err));
            let _ = resp.send(r);
        }
        SftpCmd::ReadRange {
            path,
            offset,
            len,
            resp,
        } => {
            let r = tokio::time::timeout(OP_TIMEOUT, async {
                use tokio::io::{AsyncReadExt, AsyncSeekExt};
                let mut file = session.sftp.open(&path).await.map_err(sftp_err)?;
                if offset > 0 {
                    file.seek(std::io::SeekFrom::Start(offset))
                        .await
                        .map_err(|e| RemoteError::Other(format!("seek io: {e}")))?;
                }
                let mut buf = vec![0u8; len as usize];
                let mut read_total = 0usize;
                loop {
                    let n = file
                        .read(&mut buf[read_total..])
                        .await
                        .map_err(|e| RemoteError::Other(format!("read_range io: {e}")))?;
                    if n == 0 {
                        break;
                    }
                    read_total += n;
                    if read_total == buf.len() {
                        break;
                    }
                }
                buf.truncate(read_total);
                Ok::<_, RemoteError>(buf)
            })
            .await
            .map_err(|_| RemoteError::Timeout("read_range 超时".to_owned()))
            .and_then(|r| r);
            let _ = resp.send(r);
        }
        SftpCmd::WriteRange {
            path,
            offset,
            data,
            resp,
        } => {
            let r = tokio::time::timeout(OP_TIMEOUT, async {
                use tokio::io::{AsyncSeekExt, AsyncWriteExt};
                // WRITE|CREATE 不 TRUNCATE——保留已有前缀（偏移续传写）。
                let mut file = session
                    .sftp
                    .open_with_flags(
                        &path,
                        russh_sftp::protocol::OpenFlags::WRITE
                            | russh_sftp::protocol::OpenFlags::CREATE,
                    )
                    .await
                    .map_err(sftp_err)?;
                if offset > 0 {
                    file.seek(std::io::SeekFrom::Start(offset))
                        .await
                        .map_err(|e| RemoteError::Other(format!("seek io: {e}")))?;
                }
                file.write_all(&data)
                    .await
                    .map_err(|e| RemoteError::Other(format!("write_range io: {e}")))?;
                file.shutdown()
                    .await
                    .map_err(|e| RemoteError::Other(format!("close io: {e}")))?;
                Ok::<(), RemoteError>(())
            })
            .await
            .map_err(|_| RemoteError::Timeout("write_range 超时".to_owned()))
            .and_then(|r| r);
            let _ = resp.send(r);
        }
        SftpCmd::Metadata { path, resp } => {
            let r = tokio::time::timeout(OP_TIMEOUT, session.sftp.metadata(&path))
                .await
                .map_err(|_| RemoteError::Timeout("metadata 超时".to_owned()))
                .and_then(|m| {
                    m.map_err(sftp_err).map(|m| RemoteMeta {
                        size: m.len(),
                        mtime: mtime_of(&m),
                        is_dir: m.is_dir(),
                    })
                });
            let _ = resp.send(r);
        }
        SftpCmd::Write { path, data, resp } => {
            let r = tokio::time::timeout(OP_TIMEOUT, async {
                let mut file = session.sftp.create(&path).await.map_err(sftp_err)?;
                use tokio::io::AsyncWriteExt;
                file.write_all(&data)
                    .await
                    .map_err(|e| RemoteError::Other(format!("write io: {}", e)))?;
                file.shutdown()
                    .await
                    .map_err(|e| RemoteError::Other(format!("close io: {}", e)))?;
                Ok::<(), RemoteError>(())
            })
            .await
            .map_err(|_| RemoteError::Timeout("write timeout".to_owned()))
            .and_then(|r| r);
            let _ = resp.send(r);
        }
        SftpCmd::RemoveFile { path, resp } => {
            let r = tokio::time::timeout(OP_TIMEOUT, session.sftp.remove_file(&path))
                .await
                .map_err(|_| RemoteError::Timeout("remove timeout".to_owned()))
                .and_then(|r| r.map_err(sftp_err));
            let _ = resp.send(r);
        }
        SftpCmd::RemoveDir { path, resp } => {
            let r = tokio::time::timeout(OP_TIMEOUT, session.sftp.remove_dir(&path))
                .await
                .map_err(|_| RemoteError::Timeout("remove_dir timeout".to_owned()))
                .and_then(|r| r.map_err(sftp_err));
            let _ = resp.send(r);
        }
        SftpCmd::Rename { from, to, resp } => {
            let r = tokio::time::timeout(OP_TIMEOUT, session.sftp.rename(&from, &to))
                .await
                .map_err(|_| RemoteError::Timeout("rename timeout".to_owned()))
                .and_then(|r| r.map_err(sftp_err));
            let _ = resp.send(r);
        }
        SftpCmd::CreateDir { path, resp } => {
            let r = tokio::time::timeout(OP_TIMEOUT, session.sftp.create_dir(&path))
                .await
                .map_err(|_| RemoteError::Timeout("mkdir timeout".to_owned()))
                .and_then(|r| r.map_err(sftp_err));
            let _ = resp.send(r);
        }
        SftpCmd::MoveToTrash { path, resp } => {
            let r = tokio::time::timeout(OP_TIMEOUT, async {
                const TRASH_DIR: &str = "/tmp/rvifm-trash";
                let _ = session.sftp.create_dir(TRASH_DIR.to_owned()).await;
                let base = path.rsplit('/').next().unwrap_or("file");
                for i in 0..1000u32 {
                    let name = if i == 0 {
                        format!("{}/{}", TRASH_DIR, base)
                    } else {
                        format!("{}/{}.rvt{}", TRASH_DIR, base, i)
                    };
                    if session
                        .sftp
                        .try_exists(name.clone())
                        .await
                        .map_err(sftp_err)?
                    {
                        continue;
                    }
                    session
                        .sftp
                        .rename(path.clone(), name.clone())
                        .await
                        .map_err(sftp_err)?;
                    return Ok(name);
                }
                Err(RemoteError::Other("trash name collision".to_owned()))
            })
            .await
            .map_err(|_| RemoteError::Timeout("trash timeout".to_owned()))
            .and_then(|r| r);
            let _ = resp.send(r);
        }
        SftpCmd::RestoreFromTrash {
            trash_path,
            orig,
            resp,
        } => {
            let r = tokio::time::timeout(OP_TIMEOUT, async {
                if session
                    .sftp
                    .try_exists(orig.to_owned())
                    .await
                    .map_err(sftp_err)?
                {
                    return Err(RemoteError::Other(format!("undo target exists: {}", orig)));
                }
                session
                    .sftp
                    .rename(trash_path, orig)
                    .await
                    .map_err(sftp_err)?;
                Ok(())
            })
            .await
            .map_err(|_| RemoteError::Timeout("restore timeout".to_owned()))
            .and_then(|r| r);
            let _ = resp.send(r);
        }
    }
}

/// FileAttributes 修改时间 → Unix 秒（缺失/早于纪元 → 0）。
fn mtime_of(m: &russh_sftp::protocol::FileAttributes) -> u64 {
    m.modified()
        .map(|t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        })
        .unwrap_or(0)
}

/// 已建立的 SSH+SFTP 会话（actor 线程持有）。
struct SshSession {
    handle: Handle<ClientHandler>,
    sftp: SftpSession,
}

async fn connect_async(
    params: &SftpParams,
    password: Option<&str>,
) -> Result<SshSession, RemoteError> {
    // M39-4：ProxyJump——先连跳板（复用同一认证材料，SIMPLIFIED），再经
    // direct-tcpip 通道连接目标（ssh config 透传）。
    if let Some(jump) = params.proxy_jump.clone() {
        return connect_via_jump(params, &jump, password).await;
    }
    connect_single_hop(params, password).await
}

/// M39-4：keepalive + 客户端配置（30s 间隔，3 次无响应断开）。
fn ssh_client_config() -> std::sync::Arc<client::Config> {
    std::sync::Arc::new(client::Config {
        keepalive_interval: Some(std::time::Duration::from_secs(30)),
        keepalive_max: 3,
        ..Default::default()
    })
}

/// M39-4：认证序列（共享：单跳与 ProxyJump 跳板/目标一致）——密钥文件 →
/// ssh-agent（SSH_AUTH_SOCK 在且含可用身份；失败静默跳过）→ 口令回退。
async fn authenticate_session(
    handle: &mut client::Handle<ClientHandler>,
    params: &SftpParams,
    password: Option<&str>,
) -> Result<(), RemoteError> {
    // M22-3：密钥加载失败不再直接报错——回退后续方式。
    if let Ok(k) = load_user_key(&params.key_path) {
        let key = PrivateKeyWithHashAlg::new(std::sync::Arc::new(k), None);
        let auth = handle
            .authenticate_publickey(&params.user, key)
            .await
            .map_err(map_russh_err)?;
        if auth.success() {
            return Ok(());
        }
    }
    // M39-4：ssh-agent 认证（SSH_AUTH_SOCK；不可用静默跳过）。
    match try_agent_auth(handle, &params.user).await {
        Ok(true) => return Ok(()),
        Ok(false) => {}
        Err(_) => {} // agent 不可用/出错：跳过（回退口令）
    }
    let Some(pw) = password else {
        return Err(RemoteError::Auth(format!(
            "公钥认证失败（user={}）；可用口令认证重试",
            params.user
        )));
    };
    let auth = handle
        .authenticate_password(&params.user, pw)
        .await
        .map_err(map_russh_err)?;
    if !auth.success() {
        return Err(RemoteError::Auth(format!(
            "口令认证被拒（user={}）",
            params.user
        )));
    }
    Ok(())
}

/// M39-4：ssh-agent 认证尝试——连接 SSH_AUTH_SOCK（仅 unix；russh agent
/// 客户端不支持 Windows 命名管道），请求身份逐个尝试；`Ok(true)` = 已认证，
/// `Ok(false)` = agent 可用但身份全被拒。
#[cfg(unix)]
async fn try_agent_auth(
    handle: &mut client::Handle<ClientHandler>,
    user: &str,
) -> Result<bool, RemoteError> {
    if std::env::var("SSH_AUTH_SOCK").is_err() {
        return Ok(false);
    }
    // M40-2 CI 修复：agent 客户端返回 keys::Error（非 russh::Error）——
    // map_russh_err 签名不匹配（E0631，ubuntu CI 首次编译 cfg(unix) 暴露）。
    let mut agent = russh::keys::agent::client::AgentClient::connect_env()
        .await
        .map_err(|e| RemoteError::Auth(format!("ssh-agent 不可用: {e}")))?;
    let identities = agent
        .request_identities()
        .await
        .map_err(|e| RemoteError::Auth(format!("ssh-agent 身份获取失败: {e}")))?;
    for identity in identities {
        let pubkey = identity.public_key().into_owned();
        let auth = handle
            .authenticate_publickey_with(user, pubkey, None, &mut agent)
            .await
            .map_err(|e| RemoteError::Auth(format!("ssh-agent 认证失败: {e}")))?;
        if auth.success() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// 非 unix（Windows）：russh agent 客户端不支持命名管道——直接跳过。
#[cfg(not(unix))]
async fn try_agent_auth(
    _handle: &mut client::Handle<ClientHandler>,
    _user: &str,
) -> Result<bool, RemoteError> {
    Ok(false)
}

/// 单跳连接（M19-3b 主体 + M39-4 keepalive/agent/ask 门控/代理判断抽出）。
async fn connect_single_hop(
    params: &SftpParams,
    password: Option<&str>,
) -> Result<SshSession, RemoteError> {
    let config = ssh_client_config();
    let known_hosts_path = if params.skip_host_key_check {
        None
    } else {
        Some(crate::config::known_hosts_path())
    };
    let key_error: std::sync::Arc<std::sync::Mutex<Option<String>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    let confirm_fp: std::sync::Arc<std::sync::Mutex<Option<String>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    let handler = ClientHandler {
        host: params.host.clone(),
        port: params.port,
        known_hosts_path,
        strict: params.strict_host_key,
        accept_new: params.accept_new_host_key,
        key_error: std::sync::Arc::clone(&key_error),
        confirm_fp: std::sync::Arc::clone(&confirm_fp),
    };
    let mut handle = client::connect(config, (params.host.as_str(), params.port), handler)
        .await
        .map_err(|e| {
            // M39-4：ask 首连 → 指纹确认错误（TUI 交互确认后重连）。
            if let Ok(mut g) = confirm_fp.lock()
                && let Some(fp) = g.take()
            {
                return RemoteError::FingerprintConfirm {
                    host: params.host.clone(),
                    port: params.port,
                    fp,
                };
            }
            map_connect_err(e, &key_error)
        })?;
    authenticate_session(&mut handle, params, password).await?;
    open_sftp_session(handle).await
}

/// M39-4：ProxyJump 连接——跳板（host[:port]；认证材料同目标，SIMPLIFIED）
/// → direct-tcpip 通道 → 目标 SSH 会话（connect_stream）。跳板/目标各自
/// 走完整指纹门控（known_hosts 按 host:port 区分）。
async fn connect_via_jump(
    params: &SftpParams,
    jump: &str,
    password: Option<&str>,
) -> Result<SshSession, RemoteError> {
    let (jh, jport) = match jump.rsplit_once(':') {
        Some((h, p)) => (h.to_owned(), p.parse::<u16>().unwrap_or(22)),
        None => (jump.to_owned(), 22),
    };
    let mut jump_params = params.clone();
    jump_params.host = jh;
    jump_params.port = jport;
    jump_params.proxy_jump = None;
    // 跳板握手（复用单跳；目标 known_hosts 与跳板分开记录）。
    let jump_session = connect_single_hop(&jump_params, password).await?;
    let channel = jump_session
        .handle
        .channel_open_direct_tcpip(
            params.host.as_str(),
            u32::from(params.port),
            "127.0.0.1",
            0u32,
        )
        .await
        .map_err(map_russh_err)?;
    let stream = channel.into_stream();
    let known_hosts_path = if params.skip_host_key_check {
        None
    } else {
        Some(crate::config::known_hosts_path())
    };
    let key_error: std::sync::Arc<std::sync::Mutex<Option<String>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    let confirm_fp: std::sync::Arc<std::sync::Mutex<Option<String>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    let handler = ClientHandler {
        host: params.host.clone(),
        port: params.port,
        known_hosts_path,
        strict: params.strict_host_key,
        accept_new: params.accept_new_host_key,
        key_error: std::sync::Arc::clone(&key_error),
        confirm_fp: std::sync::Arc::clone(&confirm_fp),
    };
    let mut handle = client::connect_stream(ssh_client_config(), stream, handler)
        .await
        .map_err(|e| {
            if let Ok(mut g) = confirm_fp.lock()
                && let Some(fp) = g.take()
            {
                return RemoteError::FingerprintConfirm {
                    host: params.host.clone(),
                    port: params.port,
                    fp,
                };
            }
            map_connect_err(e, &key_error)
        })?;
    authenticate_session(&mut handle, params, password).await?;
    open_sftp_session(handle).await
}

/// SFTP 会话开启（session channel + 子系统；spike 教训：先请求子系统）。
async fn open_sftp_session(
    handle: client::Handle<ClientHandler>,
) -> Result<SshSession, RemoteError> {
    let channel = handle.channel_open_session().await.map_err(map_russh_err)?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(map_russh_err)?;
    let sftp = SftpSession::new(channel.into_stream())
        .await
        .map_err(sftp_err)?;
    Ok(SshSession { handle, sftp })
}

/// M22-2：连接错误映射——known_hosts 指纹变更等安全拒绝带可读错误。
fn map_connect_err(e: russh::Error, key_error: &std::sync::Mutex<Option<String>>) -> RemoteError {
    if let Some(msg) = key_error.lock().ok().and_then(|mut g| g.take()) {
        return RemoteError::Other(msg);
    }
    map_russh_err(e)
}

/// M39-4：首连写入放行判定（纯函数，可测）——accept（旧 TOFU）或指纹
/// 已确认（accept_new）放行；ask 且未确认 → 交互确认流程。
fn first_use_allowed(strict: StrictHostKey, accept_new: bool) -> bool {
    matches!(strict, StrictHostKey::Accept) || accept_new
}

/// M22-2：known_hosts 校验的 SSH 客户端 handler（原 M19-3b 空结构升级）。
/// M39-4：strict_host_key 三态门控——FirstUse 时 ask = 填 confirm_fp 槽并
/// 拒绝本次连接（TUI 交互确认后带 accept_new 重连）；accept = 旧 TOFU。
struct ClientHandler {
    host: String,
    port: u16,
    /// None = 用户显式跳过校验（`known_hosts = false` / `no`）。
    known_hosts_path: Option<std::path::PathBuf>,
    /// M39-4：首连策略（ask/accept/no）。
    strict: StrictHostKey,
    /// M39-4：确认后重连标志（ask 路径首连写入放行）。
    accept_new: bool,
    /// 安全拒绝的可读错误（russh connect Err 时不归还 handler，经共享槽带回）。
    key_error: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    /// M39-4：ask 首连待确认指纹（connect 失败时取出转 FingerprintConfirm）。
    confirm_fp: std::sync::Arc<std::sync::Mutex<Option<String>>>,
}

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let Some(path) = &self.known_hosts_path else {
            return Ok(true); // 用户显式跳过（known_hosts = false）
        };
        // 公钥串（"算法 base64"，与 known_hosts 行格式一致）+ 指纹。
        let (key_str, fp): (String, String) = match server_public_key {
            russh::keys::PublicKeyOrCertificate::PublicKey { key, .. } => {
                let fp = key.fingerprint(russh::keys::HashAlg::Sha256).to_string();
                let ok = key
                    .to_openssh()
                    .map(|s| s.trim().to_owned())
                    .unwrap_or_default();
                (ok, fp)
            }
            russh::keys::PublicKeyOrCertificate::Certificate(c) => {
                // 证书场景（SIMPLIFIED）：按证书公钥串接受并提示。
                let fp = c
                    .public_key()
                    .fingerprint(russh::keys::HashAlg::Sha256)
                    .to_string();
                let ok = c
                    .to_openssh()
                    .map(|s| s.trim().to_owned())
                    .unwrap_or_default();
                (ok, fp)
            }
        };
        if key_str.is_empty() {
            return Ok(true);
        }
        let entries = crate::remote::known_hosts::load(path);
        match crate::remote::known_hosts::check(&entries, &self.host, self.port, &key_str) {
            crate::remote::known_hosts::KeyVerdict::Match => Ok(true),
            crate::remote::known_hosts::KeyVerdict::FirstUse => {
                // M39-4：ask（缺省）——首连不自动写入：填确认槽、拒绝本次
                // 连接（TUI 交互确认后带 accept_new_host_key 重连放行）；
                // accept（旧 TOFU）或已确认重连 → 写入并放行。
                if !first_use_allowed(self.strict, self.accept_new) {
                    if let Ok(mut g) = self.confirm_fp.lock() {
                        *g = Some(fp);
                    }
                    return Ok(false);
                }
                match crate::remote::known_hosts::append(path, &self.host, self.port, &key_str) {
                    Ok(()) => {
                        eprintln!(
                            "rvifm: 首连 {}:{}，主机密钥已写入 known_hosts（指纹 {}）",
                            self.host, self.port, fp
                        );
                        Ok(true)
                    }
                    Err(e) => {
                        eprintln!(
                            "rvifm: 首连 {}:{} 指纹 {}（known_hosts 写入失败: {e}，本次接受）",
                            self.host, self.port, fp
                        );
                        Ok(true)
                    }
                }
            }
            crate::remote::known_hosts::KeyVerdict::Mismatch { existing } => {
                let msg = format!(
                    "主机密钥校验失败（{}:{}）：known_hosts 记录的指纹与服务器不符。\n已记录: {}\n本次: {}\n如确认服务器已更换密钥，请手动删除 known_hosts 中该主机条目，或在 remotes.toml 为该远端设置 known_hosts = false。",
                    self.host,
                    self.port,
                    existing.join(" | "),
                    fp
                );
                eprintln!("rvifm: {msg}");
                if let Ok(mut g) = self.key_error.lock() {
                    *g = Some(msg);
                }
                Ok(false)
            }
        }
    }
}

/// 密钥探测：显式 key_path → ~/.ssh/id_ed25519 → ~/.ssh/id_rsa。
fn load_user_key(key_path: &Option<PathBuf>) -> Result<russh::keys::PrivateKey, RemoteError> {
    let home =
        std::env::var("HOME").map_err(|_| RemoteError::Auth("无法确定 HOME 目录".to_owned()))?;
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(p) = key_path {
        candidates.push(p.clone());
    }
    candidates.push(PathBuf::from(format!("{home}/.ssh/id_ed25519")));
    candidates.push(PathBuf::from(format!("{home}/.ssh/id_rsa")));
    let mut last: Option<PathBuf> = None;
    for path in candidates {
        match load_secret_key(&path, None) {
            Ok(key) => return Ok(key),
            Err(_) => last = Some(path),
        }
    }
    Err(RemoteError::Auth(format!(
        "无可用密钥（最后探测: {:?}）",
        last.map(|p| p.display().to_string())
    )))
}

/// russh 错误 → RemoteError 分类。
fn map_russh_err(e: russh::Error) -> RemoteError {
    match e {
        russh::Error::UnknownKey => RemoteError::Auth("未知主机密钥".to_owned()),
        russh::Error::NoAuthMethod => RemoteError::Auth("无可用认证方式".to_owned()),
        russh::Error::ConnectionTimeout
        | russh::Error::KeepaliveTimeout
        | russh::Error::InactivityTimeout => RemoteError::Timeout("连接超时".to_owned()),
        russh::Error::Disconnect => RemoteError::Connect("连接被断开".to_owned()),
        other => RemoteError::Connect(other.to_string()),
    }
}

/// russh-sftp 错误/状态 → RemoteError 分类（SFTPv3 状态码：2=不存在、3=权限）。
fn sftp_err(e: russh_sftp::client::error::Error) -> RemoteError {
    use russh_sftp::client::error::Error as E;
    use russh_sftp::protocol::StatusCode;
    match e {
        E::Status(status) => {
            let msg = status.error_message;
            match status.status_code {
                StatusCode::NoSuchFile => RemoteError::NotFound(msg),
                StatusCode::PermissionDenied => RemoteError::PermissionDenied(msg),
                _ => RemoteError::Other(format!("SFTP 状态错误: {msg}")),
            }
        }
        E::Timeout => RemoteError::Timeout("SFTP 操作超时".to_owned()),
        other => RemoteError::Other(other.to_string()),
    }
}

/// 集成测试：本机 sshd（127.0.0.1:22）可用时全链路验证；不可达自动跳过。
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::net::TcpStream;

    fn sshd_available() -> bool {
        TcpStream::connect_timeout(&"127.0.0.1:22".parse().unwrap(), Duration::from_secs(1)).is_ok()
    }

    /// M40-7：指纹门全链路测试前置——首连门只对 unknown host 触发。
    /// 宿主 known_hosts 已含 127.0.0.1（明文条目，或 russh 可匹配的哈希
    /// `|1|` 条目——本机日常 known_hosts 实测）时门控不返回
    /// FingerprintConfirm；CI runner 该文件缺失/为空 → 前置成立，
    /// 全链路覆盖不缩水。哈希条目无法在测试内复算匹配 → 一律跳过。
    fn first_use_gate_precondition() -> bool {
        let Ok(text) = std::fs::read_to_string(crate::config::known_hosts_path()) else {
            return true;
        };
        if text.trim().is_empty() {
            return true;
        }
        for line in text.lines() {
            if line.starts_with("|1|") {
                return false;
            }
            let host = line.split_whitespace().next().unwrap_or("");
            if matches!(host, "127.0.0.1" | "localhost" | "[127.0.0.1]:22") {
                return false;
            }
        }
        true
    }

    /// 连接本机 sshd；CI 无 authorized_keys → None（测试跳过）。
    fn try_backend() -> Option<SftpBackend> {
        let mut p = SftpParams::new("127.0.0.1");
        p.user = crate::remote::sftp::default_user();
        SftpBackend::connect(&p).ok()
    }

    #[test]
    fn connect_list_read_metadata_roundtrip() {
        if !sshd_available() {
            eprintln!("skip: 127.0.0.1:22 不可达");
            return;
        }
        let Some(be) = try_backend() else {
            eprintln!("skip: sshd/认证不可用");
            return;
        };
        let entries = be.list(Path::new("/tmp")).unwrap();
        assert!(!entries.is_empty());
        let data = be.read(Path::new("/etc/hostname")).unwrap();
        assert!(!data.is_empty(), "hostname 非空");
        let meta = be.metadata(Path::new("/etc/hostname")).unwrap();
        assert!(!meta.is_dir);
    }

    #[test]
    fn remote_errors_are_classified() {
        if !sshd_available() {
            eprintln!("skip: 127.0.0.1:22 不可达");
            return;
        }
        let Some(be) = try_backend() else {
            eprintln!("skip: sshd/认证不可用");
            return;
        };
        match be.list(Path::new("/definitely-not-exist-rvifm")) {
            Err(RemoteError::NotFound(_)) => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
        // openssh 对目录 read 返回 SSH_FX_FAILURE（非 NotFound/PermissionDenied）
        // ——实测行为，归 Other 分类即可（不 panic、可读）。
        match be.read(Path::new("/tmp")) {
            Err(RemoteError::NotFound(_)) => {}
            Err(RemoteError::Other(msg)) => {
                assert!(msg.contains("I/O") || msg.contains("No such"), "msg: {msg}");
            }
            other => panic!("expected classified error, got {other:?}"),
        }
    }

    #[test]
    fn connect_failure_maps_to_connect_error() {
        // 不可达端口 → 分类错误（不 panic；CI 环境差异接受任意分类）。
        let params = SftpParams {
            host: "127.0.0.1".to_owned(),
            port: 1,
            user: default_user(),
            ..SftpParams::new("127.0.0.1")
        };
        match SftpBackend::connect(&params) {
            Err(_) => {} // 任何分类错误均可（Connect/Timeout/其他）
            Ok(_) => panic!("不可达端口不应连接成功"),
        }
    }

    #[test]
    fn m394_first_use_allowed_matrix() {
        // M39-4：首连放行判定——accept 恒放行；ask 仅确认后放行。
        assert!(
            !first_use_allowed(StrictHostKey::Ask, false),
            "ask 未确认拒绝"
        );
        assert!(
            first_use_allowed(StrictHostKey::Ask, true),
            "ask 确认后放行"
        );
        assert!(
            first_use_allowed(StrictHostKey::Accept, false),
            "accept TOFU 放行"
        );
        assert!(first_use_allowed(StrictHostKey::Accept, true));
    }

    #[test]
    fn m394_strict_host_key_parse() {
        assert_eq!(StrictHostKey::parse("ask"), Some(StrictHostKey::Ask));
        assert_eq!(StrictHostKey::parse("accept"), Some(StrictHostKey::Accept));
        assert_eq!(StrictHostKey::parse("no"), Some(StrictHostKey::No));
        assert_eq!(
            StrictHostKey::parse("NO"),
            Some(StrictHostKey::No),
            "大小写不敏感"
        );
        assert_eq!(StrictHostKey::parse("bogus"), None);
        assert_eq!(StrictHostKey::default(), StrictHostKey::Ask, "缺省 ask");
    }

    #[test]
    fn m394_ask_gates_first_use_then_accepts_on_confirm() {
        // 全链路（本机 sshd 可用时；CI ubuntu 覆盖）：ask 首连 →
        // FingerprintConfirm（带 host:port + 指纹）→ 确认重连成功。
        if !sshd_available() {
            return;
        }
        if !first_use_gate_precondition() {
            eprintln!("skip: known_hosts 已含 127.0.0.1（哈希/明文），首连门不触发");
            return;
        }
        let first = SftpParams {
            strict_host_key: StrictHostKey::Ask,
            accept_new_host_key: false,
            skip_host_key_check: false,
            ..SftpParams::new("127.0.0.1")
        };
        let err = SftpBackend::connect(&first).unwrap_err();
        let (fp, host, port) = match err {
            RemoteError::FingerprintConfirm { host, port, fp } => (fp, host, port),
            other => panic!("ask 首连应返回 FingerprintConfirm: {other:?}"),
        };
        assert!(!fp.is_empty());
        assert_eq!(host, "127.0.0.1");
        assert_eq!(port, 22);
        // 确认重连：accept_new = true → 写入 known_hosts + 连接成功。
        let confirmed = SftpParams {
            accept_new_host_key: true,
            ..first
        };
        // 指纹门（本测目标）已验证；认证结果取决于 runner 密钥凭据有无
        // （CI sshd 无 authorized_keys → Auth 可接受，非认证类才算失败）。
        match SftpBackend::connect(&confirmed) {
            Ok(_) => {}
            Err(RemoteError::Auth(_)) => {}
            Err(e) => panic!("确认后重连失败（非认证类）: {e:?}"),
        }
    }
}
