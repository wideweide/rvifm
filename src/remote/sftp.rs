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
#[derive(Debug, Clone, Default)]
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
    /// M22-2：跳过 known_hosts 校验（安全默认 false = 校验）。
    pub skip_host_key_check: bool,
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
                move || actor_thread(params, cmd_rx, init_tx)
            })
            .map_err(|e| RemoteError::Other(format!("线程启动失败: {e}")))?;
        // 连接结果（actor 侧已含 CONNECT_TIMEOUT）。
        match init_rx.recv_timeout(CONNECT_TIMEOUT + OP_TIMEOUT) {
            Ok(Ok(())) => Ok(Self { cmd_tx }),
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
    let session = match rt
        .block_on(async { tokio::time::timeout(CONNECT_TIMEOUT, connect_async(&params)).await })
    {
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

async fn connect_async(params: &SftpParams) -> Result<SshSession, RemoteError> {
    let config = client::Config::default();
    let key = load_user_key(&params.key_path)?;
    let key = PrivateKeyWithHashAlg::new(std::sync::Arc::new(key), None);
    // M22-2：known_hosts 校验——错误经共享槽带回（russh connect 失败时不
    // 归还 handler，用 Arc<Mutex> 跨越）。
    let key_error: std::sync::Arc<std::sync::Mutex<Option<String>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    let known_hosts_path = if params.skip_host_key_check {
        None
    } else {
        Some(crate::config::known_hosts_path())
    };
    let handler = ClientHandler {
        host: params.host.clone(),
        port: params.port,
        known_hosts_path,
        key_error: std::sync::Arc::clone(&key_error),
    };
    let mut handle = client::connect(
        std::sync::Arc::new(config),
        (params.host.as_str(), params.port),
        handler,
    )
    .await
    .map_err(|e| map_connect_err(e, &key_error))?;
    let auth = handle
        .authenticate_publickey(&params.user, key)
        .await
        .map_err(map_russh_err)?;
    if !auth.success() {
        return Err(RemoteError::Auth(format!(
            "公钥认证被拒（user={}）",
            params.user
        )));
    }
    let channel = handle.channel_open_session().await.map_err(map_russh_err)?;
    // spike 教训：必须先请求 sftp 子系统，否则会话握手超时。
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

/// M22-2：known_hosts 校验的 SSH 客户端 handler（原 M19-3b 空结构升级）。
struct ClientHandler {
    host: String,
    port: u16,
    /// None = 用户显式跳过校验（`known_hosts = false`）。
    known_hosts_path: Option<std::path::PathBuf>,
    /// 安全拒绝的可读错误（russh connect Err 时不归还 handler，经共享槽带回）。
    key_error: std::sync::Arc<std::sync::Mutex<Option<String>>>,
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

    /// 连接本机 sshd；CI 无 authorized_keys → None（测试跳过）。
    fn try_backend() -> Option<SftpBackend> {
        let mut p = SftpParams::new("127.0.0.1");
        p.user = crate::remote::sftp::default_user();
        SftpBackend::connect(&p).ok()
    }

    fn backend() -> SftpBackend {
        try_backend().expect("127.0.0.1 sshd + 密钥认证不可用")
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
}
