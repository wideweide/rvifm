//! M19-3a spike（候选 A）：russh 0.65 + russh-sftp 3.0（纯 Rust，async/tokio）。
//! 连接 127.0.0.1（密钥认证），list /，并在 /tmp/rvifm-spike 验证
//! write/remove/rename/create_dir。仅 spike，不进主程序。

use std::path::PathBuf;
use tokio_stream::StreamExt;

use russh::client::{self, Handle};
use russh::keys::{load_secret_key, PrivateKeyWithHashAlg};

/// spike：接受任意服务端主机密钥（正式实现需 known_hosts 校验）。
struct ClientHandler;

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        Ok(true) // spike：接受全部；正式实现需 known_hosts 校验
    }
}

async fn connect_sftp() -> Result<Handle<ClientHandler>, Box<dyn std::error::Error>> {
    let config = client::Config::default();
    let key_path = format!("{}/.ssh/id_ed25519", std::env::var("HOME")?);
    let key = load_secret_key(&key_path, None)?;
    eprintln!("[A] key loaded: algorithm={:?}", key.algorithm());
    // ed25519 无 hash 协商问题——显式 None（PrivateKeyWithHashAlg 内部推断）。
    let key = PrivateKeyWithHashAlg::new(std::sync::Arc::new(key), None);

    let mut handle = client::connect(
        std::sync::Arc::new(config),
        ("127.0.0.1", 22),
        ClientHandler,
    )
    .await?;
    let user = std::env::var("USER").unwrap_or_else(|_| "dlt".to_owned());
    eprintln!("[A] authenticating as {user}...");
    let auth = handle.authenticate_publickey(&user, key).await?;
    eprintln!("[A] auth result: {auth:?}");
    if !auth.success() {
        return Err("公钥认证被拒".into());
    }
    Ok(handle)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let t0 = std::time::Instant::now();
    let mut handle = connect_sftp().await?;
    eprintln!("[A] connect+auth: {:?}", t0.elapsed());

    // SFTP 子通道。
    let channel = handle.channel_open_session().await?;
    // 必须先请求 sftp 子系统（否则 SFTP 会话握手超时）。
    channel.request_subsystem(true, "sftp").await?;
    let sftp = russh_sftp::client::SftpSession::new(channel.into_stream()).await?;

    // list /（ReadDir 是异步流，收集后计数）
    let entries = sftp.read_dir("/").await?;
    let mut names = Vec::new();
    for e in entries {
        names.push(e.file_name());
    }
    eprintln!("[A] list /: {} 项", names.len());
    for n in names.iter().take(3) {
        eprintln!("  - {n}");
    }

    // 写操作验证：/tmp/rvifm-spike
    let dir = "/tmp/rvifm-spike".to_owned();
    // 清残留（remove_dir 对不存在目录返回 NoSuchFile——忽略）。
    let _ = sftp.remove_dir(dir.clone()).await;
    sftp.create_dir(dir.clone()).await?;
    eprintln!("[A] create_dir OK");
    // write() 仅 WRITE flag（不 CREATE）——新文件用 create + write_all。
    let f = format!("{dir}/spike.txt");
    let file = sftp.create(f.clone()).await?;
    let mut file = file;
    use tokio::io::AsyncWriteExt;
    file.write_all(b"rvifm spike").await?;
    file.shutdown().await?;
    eprintln!("[A] write OK");
    let data = sftp.read(f.clone()).await?;
    eprintln!("[A] read back: {} bytes", data.len());
    let f2 = format!("{dir}/spike2.txt");
    sftp.rename(f, f2.clone()).await?;
    eprintln!("[A] rename OK");
    let exists = sftp.metadata(f2.clone()).await.is_ok();
    eprintln!("[A] metadata after rename: {exists}");
    sftp.remove_file(f2).await?;
    sftp.remove_dir(dir).await?;
    eprintln!("[A] remove OK（写操作全部支持）");

    handle
        .disconnect(russh::Disconnect::ByApplication, "spike done", "en")
        .await?;
    Ok(())
}
