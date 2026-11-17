//! OpenSSH known_hosts 主机密钥校验（M22-2；SPEC M22 硬约束）。
//!
//! 解析 `~/.ssh/known_hosts`：明文 hostname 与 `|1|` hashed（HMAC-SHA1）
//! 两种形式；`@revoked`/`@cert-authority` 等标记行跳过不解析。端口非 22
//! 的主机按 OpenSSH 惯例记作 `[host]:port`。指纹变更拒绝连接（安全默认），
//! 跳过校验须由用户显式配置（SPEC M22-2）。
//!
//! 解析/匹配逻辑为纯函数（可测）；文件读写（追加首连条目、0600 权限）
//! 由 sftp actor 调用。

use std::path::Path;

/// 主机匹配器：明文（含 `[host]:port` 形式）或 `|1|salt|hash` hashed。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostMatcher {
    Plain(String),
    Hashed { salt: Vec<u8>, hash: Vec<u8> },
}

/// 解析后的 known_hosts 行：匹配器列表 + "算法 base64" 公钥串。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostEntry {
    pub matchers: Vec<HostMatcher>,
    pub key: String,
}

/// 校验结论（M22-2）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyVerdict {
    Match,
    FirstUse,
    Mismatch { existing: Vec<String> },
}

/// OpenSSH hashed hostname：`HMAC-SHA1(key=base64salt 解码, msg=hostname)`。
fn hashed_match(salt: &[u8], hash: &[u8], host: &str) -> bool {
    use hmac::{Hmac, KeyInit, Mac};
    type HmacSha1 = Hmac<sha1::Sha1>;
    let Ok(mut mac) = HmacSha1::new_from_slice(salt) else {
        return false;
    };
    mac.update(host.as_bytes());
    mac.verify_slice(hash).is_ok()
}

/// 解析单行 known_hosts（注释/空行/标记行返回 None；坏行返回 None 不阻断）。
pub fn parse_line(line: &str) -> Option<HostEntry> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') || line.starts_with('@') {
        return None;
    }
    let mut parts = line.split_whitespace();
    let hosts = parts.next()?;
    let keytype = parts.next()?;
    let b64 = parts.next()?;
    let matchers: Vec<HostMatcher> = hosts
        .split(',')
        .filter(|h| !h.is_empty())
        .filter_map(|h| {
            if let Some(rest) = h.strip_prefix("|1|") {
                let mut seg = rest.split('|');
                let salt = seg.next()?;
                let hash = seg.next()?;
                use base64ct::Encoding as _;
                let salt = base64ct::Base64::decode_vec(salt).ok()?;
                let hash = base64ct::Base64::decode_vec(hash).ok()?;
                Some(HostMatcher::Hashed { salt, hash })
            } else {
                Some(HostMatcher::Plain(h.to_owned()))
            }
        })
        .collect();
    if matchers.is_empty() {
        return None;
    }
    Some(HostEntry {
        matchers,
        key: format!("{keytype} {b64}"),
    })
}

/// 读入整个 known_hosts（文件缺失/坏行静默跳过——不阻断连接）。
pub fn load(path: &Path) -> Vec<HostEntry> {
    match std::fs::read_to_string(path) {
        Ok(text) => text.lines().filter_map(parse_line).collect(),
        Err(_) => Vec::new(),
    }
}

/// 主机-端口的匹配形式集合：22 → `host`；非 22 → `[host]:port`（OpenSSH
/// 惯例）。hashed 条目按同一形式哈希。
pub fn host_forms(host: &str, port: u16) -> Vec<String> {
    if port == 22 {
        vec![host.to_owned()]
    } else {
        vec![format!("[{host}]:{port}")]
    }
}

fn entry_matches(entry: &HostEntry, host: &str, port: u16) -> bool {
    host_forms(host, port).iter().any(|form| {
        entry.matchers.iter().any(|m| match m {
            HostMatcher::Plain(p) => p == form,
            HostMatcher::Hashed { salt, hash } => hashed_match(salt, hash, form),
        })
    })
}

/// 纯校验（M22-2 核心）：比对 server 公钥串（"算法 base64"）。
pub fn check(entries: &[HostEntry], host: &str, port: u16, server_key: &str) -> KeyVerdict {
    let matching: Vec<&str> = entries
        .iter()
        .filter(|e| entry_matches(e, host, port))
        .map(|e| e.key.as_str())
        .collect();
    if matching.is_empty() {
        KeyVerdict::FirstUse
    } else if matching.contains(&server_key) {
        KeyVerdict::Match
    } else {
        KeyVerdict::Mismatch {
            existing: matching.iter().map(|k| (*k).to_owned()).collect(),
        }
    }
}

/// 首连追加写入（文件不存在则创建，unix 下权限 0600）。
pub fn append(path: &Path, host: &str, port: u16, key: &str) -> std::io::Result<()> {
    use std::io::Write;
    let forms = host_forms(host, port);
    let line = format!("{} {key}\n", forms.join(","));
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    #[allow(unused_variables)]
    let newly = !path.exists(); // unix 0600 权限仅新建时设置（Windows 无该语义）
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    f.write_all(line.as_bytes())?;
    #[cfg(unix)]
    if newly {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // OpenSSH hashed known_hosts 测试向量：salt="foo"，host="192.168.1.61"
    // （hash 由本模块同规格 HMAC-SHA1 计算，往返验证格式一致性）。
    fn hashed_line(salt_b64: &str, host: &str, key: &str) -> String {
        use base64ct::Encoding as _;
        use hmac::{Hmac, KeyInit, Mac};
        type HmacSha1 = Hmac<sha1::Sha1>;
        let salt = base64ct::Base64::decode_vec(salt_b64).unwrap();
        let mut mac = HmacSha1::new_from_slice(&salt).unwrap();
        mac.update(host.as_bytes());
        let hash = base64ct::Base64::encode_string(&mac.finalize().into_bytes());
        format!("|1|{salt_b64}|{hash} {key}")
    }

    const KEY_A: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIHXWzLcKvXWdRlW9xPY2fB6oKZ3xGrk2FqS2dVZM6b9A";
    const KEY_B: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJl0KqzZbVXmQ6lB7cOe1RqWw0PnMkS4dLvTqY9c2E";

    #[test]
    fn parse_plain_and_marked_lines() {
        // 明文行解析；注释/空行/@标记行跳过。
        assert_eq!(
            parse_line("host1,host2 ssh-ed25519 AAAA"),
            Some(HostEntry {
                matchers: vec![
                    HostMatcher::Plain("host1".to_owned()),
                    HostMatcher::Plain("host2".to_owned())
                ],
                key: "ssh-ed25519 AAAA".to_owned(),
            })
        );
        assert!(parse_line("# comment").is_none());
        assert!(parse_line("").is_none());
        assert!(parse_line("@revoked host1 ssh-rsa AAAA").is_none());
    }

    #[test]
    fn check_match_first_use_and_mismatch() {
        let entries = vec![
            parse_line("host1 ssh-ed25519 AAAA").unwrap(),
            parse_line("other ssh-rsa BBBB").unwrap(),
        ];
        assert!(matches!(
            check(&entries, "host1", 22, "ssh-ed25519 AAAA"),
            KeyVerdict::Match
        ));
        assert!(matches!(
            check(&entries, "host1", 22, "ssh-rsa XXXX"),
            KeyVerdict::Mismatch { .. }
        ));
        assert!(matches!(
            check(&entries, "unknown", 22, KEY_A),
            KeyVerdict::FirstUse
        ));
    }

    #[test]
    fn check_port_bracket_form() {
        // 非 22 端口按 `[host]:port` 匹配；22 端口不匹配 bracket 形式条目。
        let entries = vec![parse_line("[host1]:2222 ssh-ed25519 AAAA").unwrap()];
        assert!(matches!(
            check(&entries, "host1", 2222, "ssh-ed25519 AAAA"),
            KeyVerdict::Match
        ));
        assert!(matches!(
            check(&entries, "host1", 22, "ssh-ed25519 AAAA"),
            KeyVerdict::FirstUse
        ));
    }

    #[test]
    fn check_hashed_hostname() {
        // hashed 条目按端口形式哈希匹配（192.168.1.61:22 → 形式 "192.168.1.61"）。
        let line = hashed_line("Zm9v", "192.168.1.61", KEY_A);
        let entries = vec![parse_line(&line).unwrap()];
        assert!(matches!(
            check(&entries, "192.168.1.61", 22, KEY_A),
            KeyVerdict::Match
        ));
        // 其他主机不命中。
        assert!(matches!(
            check(&entries, "10.0.0.1", 22, KEY_A),
            KeyVerdict::FirstUse
        ));
    }

    #[test]
    fn append_creates_file_and_reloads() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("known_hosts");
        append(&path, "newhost", 22, KEY_A).unwrap();
        let entries = load(&path);
        assert!(matches!(
            check(&entries, "newhost", 22, KEY_A),
            KeyVerdict::Match
        ));
        // 追加不覆盖：再写一条不同主机，两条都在。
        append(&path, "second", 22, KEY_B).unwrap();
        let entries = load(&path);
        assert!(matches!(
            check(&entries, "second", 22, KEY_B),
            KeyVerdict::Match
        ));
        assert!(matches!(
            check(&entries, "newhost", 22, KEY_A),
            KeyVerdict::Match
        ));
    }
}
