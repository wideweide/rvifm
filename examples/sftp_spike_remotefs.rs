//! M19-3a spike（候选 B）：remotefs 1.0 + remotefs-ssh 1.1（默认 libssh2
//! 后端，C 绑定；同步 API）。连接 127.0.0.1（密钥认证），list /，验证写操作。

use remotefs::fs::{ReadOptions, RemoteFs, UnixPex, WriteOptions, WriteStream};
use remotefs_ssh::{SftpFs, SshKeyStorage, SshOpts};
use std::io::Write;
use std::path::PathBuf;

/// spike：从 ~/.ssh/ 解析密钥（正式实现可复用该逻辑）。
struct HomeKeyStorage;

impl SshKeyStorage for HomeKeyStorage {
    fn resolve(&self, _host: &str, username: &str) -> Option<PathBuf> {
        let home = std::env::var("HOME").ok()?;
        for name in ["id_ed25519", "id_rsa"] {
            let p = PathBuf::from(format!("{home}/.ssh/{name}"));
            if p.exists() {
                eprintln!("[B] key: {}", p.display());
                return Some(p);
            }
        }
        eprintln!("[B] no key for {username}");
        None
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let t0 = std::time::Instant::now();
    let opts = SshOpts::new("127.0.0.1")
        .port(22)
        .username(std::env::var("USER").unwrap_or_else(|_| "dlt".to_owned()))
        .key_storage(Box::new(HomeKeyStorage))
        .connection_timeout(std::time::Duration::from_secs(10));
    let mut client: SftpFs<remotefs_ssh::LibSsh2Session> = opts.into();
    client.connect()?;
    eprintln!("[B] connect+auth: {:?}", t0.elapsed());

    // list /
    let entries = client.list_dir(std::path::Path::new("/"))?;
    eprintln!("[B] list /: {} 项", entries.len());
    for e in entries.iter().take(3) {
        eprintln!("  - {} (dir={})", e.path().display(), e.is_dir());
    }

    // 写操作验证：/tmp/rvifm-spike-b
    let dir = std::path::Path::new("/tmp/rvifm-spike-b");
    let _ = client.remove_dir_all(dir); // 清残留
    client.create_dir(dir, Some(UnixPex::from(0o755)))?;
    eprintln!("[B] create_dir OK");
    let f = dir.join("spike.txt");
    let mut stream: WriteStream = client.create(
        &f,
        &WriteOptions::default().size_hint(11).mode(UnixPex::from(0o644)),
    )?;
    stream.write_all(b"rvifm spike")?;
    drop(stream);
    eprintln!("[B] write OK");
    let mut buf = Vec::new();
    use std::io::Read as _;
    let mut rs = client.open(&f, &ReadOptions::default())?;
    rs.read_to_end(&mut buf)?;
    eprintln!("[B] read back: {} bytes", buf.len());
    let f2 = dir.join("spike2.txt");
    client.rename(&f, &f2)?;
    eprintln!("[B] rename OK");
    client.remove_file(&f2)?;
    eprintln!("[B] remove_file OK");
    client.remove_dir_all(dir)?;
    eprintln!("[B] remove_dir_all OK（写操作全支持）");

    client.disconnect()?;
    Ok(())
}
