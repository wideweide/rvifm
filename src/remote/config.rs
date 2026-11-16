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
}

impl RemoteEntry {
    /// 构造 SFTP 连接参数（user 缺省当前用户；key 展开 ~）。
    pub fn to_params(&self, name: &str) -> super::sftp::SftpParams {
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

/// 合并配置：remotes.toml 覆盖同名 ssh config 别名；ssh config 别名缺省
/// host=别名本身（sshd 按配置解析），root 缺省 /。返回按名排序的表。
pub fn resolve_all(remotes_path: &Path, ssh_config_path: &Path) -> BTreeMap<String, RemoteEntry> {
    let mut out = load_remotes_toml(remotes_path);
    for alias in parse_ssh_config_aliases(ssh_config_path) {
        out.entry(alias.clone()).or_insert_with(|| RemoteEntry {
            host: alias,
            port: None,
            user: None,
            key: None,
            root: None,
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
}
