//! M19-4：远端配置来源——remotes.toml（优先）与 ~/.ssh/config Host 别名。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::sftp::SftpParams;

/// remotes.toml 中单个远端的连接参数。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemoteEntry {
    pub host: String,
    pub port: Option<u16>,
    pub user: Option<String>,
    /// 密钥路径（~ 展开为 HOME）。
    pub key: Option<PathBuf>,
    /// 连接后的初始路径（缺省 /）。
    pub root: Option<PathBuf>,
    /// M22-2：跳过 known_hosts 校验（安全默认 false = 校验）。
    /// remotes.toml 里显式 `known_hosts = false` 才跳过。
    pub known_hosts: Option<bool>,
    /// M39-4：首连主机密钥策略（`strict_host_key = ask|accept|no`；缺省
    /// ask——TOFU 改为交互确认）。
    pub strict_host_key: Option<String>,
    /// M39-4：ProxyJump 跳板（`host[:port]`；仅 ssh config 透传，SIMPLIFIED）。
    pub proxy_jump: Option<String>,
}

impl RemoteEntry {
    /// 构造 SFTP 连接参数（user 缺省当前用户；key 展开 ~）。
    /// M39-4：strict_host_key 三态 + ProxyJump 透传；`known_hosts = false`
    /// 与 `strict_host_key = "no"` 等效（均跳过校验）。
    pub fn to_params(&self, name: &str) -> super::sftp::SftpParams {
        let strict = self
            .strict_host_key
            .as_deref()
            .and_then(crate::remote::sftp::StrictHostKey::parse)
            .unwrap_or_default();
        SftpParams {
            host: self.host.clone(),
            port: self.port.unwrap_or(22),
            user: self.user.clone().unwrap_or_else(super::sftp::default_user),
            key_path: self.key.as_ref().map(|k| expand_tilde(k)),
            key_path_raw: Some(
                self.key
                    .clone()
                    .map(|k| k.display().to_string())
                    .unwrap_or_default(),
            ),
            name: name.to_owned(),
            root: self.root.clone(),
            skip_host_key_check: self.known_hosts == Some(false)
                || strict == crate::remote::sftp::StrictHostKey::No,
            strict_host_key: strict,
            accept_new_host_key: false,
            proxy_jump: self.proxy_jump.clone(),
        }
    }
}

/// ~ 展开为 HOME（其余原样）。
pub fn expand_tilde(p: &Path) -> PathBuf {
    if let Some(rest) = p.to_str().and_then(|s| s.strip_prefix("~/"))
        && let Ok(home) = std::env::var("HOME")
    {
        return PathBuf::from(format!("{home}/{rest}"));
    }
    p.to_path_buf()
}

/// remotes.toml：`[<name>]` 表（host 必填；port/user/key/root 可选）。
pub fn load_remotes_toml(path: &Path) -> BTreeMap<String, RemoteEntry> {
    let mut out = BTreeMap::new();
    let Ok(text) = std::fs::read_to_string(path) else {
        return out;
    };
    let Ok(value) = text.parse::<toml::Table>() else {
        return out;
    };
    for (name, item) in value {
        let Some(table) = item.as_table() else {
            continue;
        };
        let Some(host) = table.get("host").and_then(|v| v.as_str()) else {
            continue; // 无 host 的条目跳过
        };
        out.insert(
            name.clone(),
            RemoteEntry {
                host: host.to_owned(),
                port: table
                    .get("port")
                    .and_then(|v| v.as_integer())
                    .map(|p| p as u16),
                user: table.get("user").and_then(|v| v.as_str()).map(String::from),
                key: table.get("key").and_then(|v| v.as_str()).map(PathBuf::from),
                root: table
                    .get("root")
                    .and_then(|v| v.as_str())
                    .map(PathBuf::from),
                known_hosts: table.get("known_hosts").and_then(|v| v.as_bool()),
                strict_host_key: table
                    .get("strict_host_key")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                proxy_jump: None, // 仅 ssh config 透传（SIMPLIFIED）
            },
        );
    }
    out
}

/// ~/.ssh/config 的 Host 别名（跳过通配模式 `*`/`?`；Match 后的 Host 行
/// 视为条件块，SIMPLIFIED：不做 Match 语义，仅收集裸 Host 行的别名）。
pub fn parse_ssh_config_aliases(path: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut aliases = Vec::new();
    let mut in_match = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let Some((key, value)) = line
            .split_once(' ')
            .or_else(|| line.split_once('\t').map(|(k, v)| (k, v.trim_start())))
        else {
            if line.eq_ignore_ascii_case("match") {
                in_match = true;
            }
            continue;
        };
        let key_l = key.to_lowercase();
        if key_l == "match" {
            in_match = true;
            continue;
        }
        if key_l == "host" {
            if !in_match {
                for alias in value.split_whitespace() {
                    if !alias.contains('*') && !alias.contains('?') {
                        aliases.push(alias.to_owned());
                    }
                }
            }
            in_match = false;
            continue;
        }
        // 其他指令重置 Match 状态（Match 块只到下一个指令）。
        in_match = false;
    }
    aliases
}

/// M39-4：~/.ssh/config 单 Host 块关键字段（透传解析；SIMPLIFIED——不做
/// Match 通配语义，取首个含该别名的裸 Host 块）。
pub struct SshHostFields {
    pub hostname: Option<String>,
    pub port: Option<u16>,
    pub user: Option<String>,
    pub proxy_jump: Option<String>,
}

/// 解析 `~/.ssh/config` 中 `alias` 所在块的字段（`ProxyJump` 仅透传，
/// `ProxyCommand` 不支持——SIMPLIFIED）。
pub fn ssh_config_host_fields(path: &Path, alias: &str) -> Option<SshHostFields> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut fields = SshHostFields {
        hostname: None,
        port: None,
        user: None,
        proxy_jump: None,
    };
    let mut hit = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let Some((key, value)) = line
            .split_once(' ')
            .or_else(|| line.split_once('\t').map(|(k, v)| (k, v.trim_start())))
        else {
            continue;
        };
        let key_l = key.to_lowercase();
        if key_l == "host" {
            if hit {
                break; // 已命中的块结束（下一个 Host 行）
            }
            hit = value.split_whitespace().any(|a| a == alias);
            continue;
        }
        if hit && let Some(v) = Some(value.trim().to_owned()) {
            match key_l.as_str() {
                "hostname" => fields.hostname = Some(v),
                "port" => fields.port = v.parse().ok(),
                "user" => fields.user = Some(v),
                "proxyjump" => fields.proxy_jump = Some(v),
                _ => {}
            }
        }
    }
    hit.then_some(fields)
}

/// 合并配置：remotes.toml 覆盖同名 ssh config 别名；ssh config 别名缺省
/// host=别名本身（sshd 按配置解析），root 缺省 /。M39-4：ssh config 块的
/// Hostname/Port/User/ProxyJump 透传。返回按名排序的表。
pub fn resolve_all(remotes_path: &Path, ssh_config_path: &Path) -> BTreeMap<String, RemoteEntry> {
    let mut out = load_remotes_toml(remotes_path);
    for alias in parse_ssh_config_aliases(ssh_config_path) {
        out.entry(alias.clone()).or_insert_with(|| {
            let f = ssh_config_host_fields(ssh_config_path, &alias);
            RemoteEntry {
                host: f
                    .as_ref()
                    .and_then(|x| x.hostname.clone())
                    .unwrap_or_else(|| alias.clone()),
                port: f.as_ref().and_then(|x| x.port),
                user: f.as_ref().and_then(|x| x.user.clone()),
                key: None,
                root: None,
                known_hosts: None,
                strict_host_key: None,
                proxy_jump: f.and_then(|x| x.proxy_jump),
            }
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remotes_toml_full_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("remotes.toml");
        std::fs::write(
            &path,
            r#"[dev]
host = "10.0.0.5"
port = 2222
user = "deploy"
key = "~/.ssh/deploy_key"
root = "/srv/app"
"#,
        )
        .unwrap();
        let map = load_remotes_toml(&path);
        let e = map.get("dev").unwrap();
        assert_eq!(e.host, "10.0.0.5");
        assert_eq!(e.port, Some(2222));
        assert_eq!(e.user.as_deref(), Some("deploy"));
        assert_eq!(e.root.as_deref(), Some(std::path::Path::new("/srv/app")));
    }

    #[test]
    fn ssh_config_aliases_skip_wildcards_and_match() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config");
        std::fs::write(
            &path,
            r#"# 注释
Host dev
  HostName 10.0.0.5

Host prod backup
  HostName 10.0.0.6

Host *.internal
  User deploy

Match host prod
  HostName 10.0.0.9
"#,
        )
        .unwrap();
        let aliases = parse_ssh_config_aliases(&path);
        assert_eq!(aliases, ["dev", "prod", "backup"], "通配与 Match 内跳过");
    }

    #[test]
    fn resolve_all_remotes_toml_overrides_ssh_config() {
        let tmp = tempfile::tempdir().unwrap();
        let remotes = tmp.path().join("remotes.toml");
        std::fs::write(
            &remotes,
            r#"[dev]
host = "10.9.9.9"
"#,
        )
        .unwrap();
        let ssh = tmp.path().join("config");
        std::fs::write(&ssh, "Host dev\nHost prod").unwrap();
        let map = resolve_all(&remotes, &ssh);
        assert_eq!(map.get("dev").unwrap().host, "10.9.9.9", "toml 覆盖");
        assert_eq!(map.get("prod").unwrap().host, "prod", "ssh 别名兜底");
    }

    #[test]
    fn to_params_defaults() {
        let e = RemoteEntry {
            host: "h".to_owned(),
            ..Default::default()
        };
        let p = e.to_params("dev");
        assert_eq!(p.host, "h");
        assert_eq!(p.port, 22);
        assert_eq!(p.root, None);
    }

    #[test]
    fn m394_strict_host_key_and_proxy_jump_params() {
        // M39-4：remotes.toml strict_host_key 三态 + ssh config ProxyJump 透传。
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("remotes.toml");
        std::fs::write(
            &path,
            r#"[a]
host = "10.0.0.5"
strict_host_key = "accept"

[b]
host = "10.0.0.6"
known_hosts = false

[c]
host = "10.0.0.7"
"#,
        )
        .unwrap();
        let map = load_remotes_toml(&path);
        let pa = map.get("a").unwrap().to_params("a");
        assert_eq!(
            pa.strict_host_key,
            crate::remote::sftp::StrictHostKey::Accept
        );
        assert!(!pa.skip_host_key_check, "accept ≠ 跳过校验");
        let pb = map.get("b").unwrap().to_params("b");
        assert!(
            pb.skip_host_key_check,
            "known_hosts=false 与 strict=no 等效跳过"
        );
        let pc = map.get("c").unwrap().to_params("c");
        assert_eq!(
            pc.strict_host_key,
            crate::remote::sftp::StrictHostKey::Ask,
            "缺省 ask"
        );
        // ssh config：Hostname/Port/User/ProxyJump 透传。
        let ssh = tmp.path().join("config");
        std::fs::write(
            &ssh,
            r#"Host jumpme
  HostName 10.1.1.1
  Port 2200
  User ops
  ProxyJump bastion.example.com:2222
"#,
        )
        .unwrap();
        let map = resolve_all(&tmp.path().join("none.toml"), &ssh);
        let e = map.get("jumpme").unwrap();
        assert_eq!(e.host, "10.1.1.1", "Hostname 透传");
        assert_eq!(e.port, Some(2200));
        assert_eq!(e.user.as_deref(), Some("ops"));
        assert_eq!(e.proxy_jump.as_deref(), Some("bastion.example.com:2222"));
        let pj = e.to_params("jumpme");
        assert_eq!(pj.proxy_jump.as_deref(), Some("bastion.example.com:2222"));
        // ssh_config_host_fields 直查。
        let f = ssh_config_host_fields(&ssh, "jumpme").unwrap();
        assert_eq!(f.hostname.as_deref(), Some("10.1.1.1"));
        assert_eq!(f.proxy_jump.as_deref(), Some("bastion.example.com:2222"));
        assert!(ssh_config_host_fields(&ssh, "absent").is_none());
    }
}
