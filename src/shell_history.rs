//! M50-3：shell 历史读取（命令补全数据源）。
//!
//! - 完整命令行仅作内存补全候选；只有命令名（首个 token）写入 frecency `cmd` topic。
//! - 增量：记录每个历史文件的 (len, mtime) 签名，只在变化时对新行记账；
//!   签名不变时跳过记录（候选列表仍从文件内容读取，保持可用）。
//! - 不扫描 PATH、不监控 execve、不读取命令参数入库。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
struct FileSig {
    len: u64,
    mtime: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct HistoryState {
    files: BTreeMap<String, FileSig>,
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// 历史文件路径（bash / zsh）；缺失或不存在的路径可安全忽略。
pub fn history_paths() -> Vec<PathBuf> {
    let Some(home) = home_dir() else {
        return Vec::new();
    };
    [".bash_history", ".zsh_history"]
        .iter()
        .map(|name| home.join(name))
        .filter(|p| p.exists())
        .collect()
}

/// bash 历史：逐行命令；跳过空行、注释与 zsh 扩展时间戳行。
pub fn parse_bash(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter(|line| !is_zsh_meta(line))
        .map(str::to_owned)
        .collect()
}

fn is_zsh_meta(line: &str) -> bool {
    line.starts_with(':')
        && line[1..]
            .trim_start()
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
}

/// zsh 扩展历史：`: <epoch>:<duration>;<command>` → 取命令部分。
pub fn parse_zsh(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter_map(|line| {
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let command = if is_zsh_meta(line) {
                line.split_once(';').map(|(_, cmd)| cmd).unwrap_or(line)
            } else {
                line
            };
            let command = command.trim();
            (!command.is_empty()).then(|| command.to_owned())
        })
        .collect()
}

/// 读取全部候选（完整命令行，内存态）。
pub fn read_all_commands() -> Vec<String> {
    let mut out = Vec::new();
    for path in history_paths() {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let lines = if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.contains("zsh"))
        {
            parse_zsh(&text)
        } else {
            parse_bash(&text)
        };
        out.extend(lines);
    }
    out
}

fn sig(path: &Path) -> Option<FileSig> {
    let meta = std::fs::metadata(path).ok()?;
    Some(FileSig {
        len: meta.len(),
        mtime: meta
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    })
}

fn state_path() -> PathBuf {
    crate::config::data_dir().join("shell_history.json")
}

/// 增量记账：签名变化的文件，其命令行首 token 写入 frecency cmd topic；
/// 未变化则跳过。文件变小/被截断时重新全量记账（简单且安全）。
pub fn record_new_commands(enabled: bool, ignore_patterns: &[String]) {
    if !enabled {
        return;
    }
    let mut state: HistoryState = std::fs::read_to_string(state_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let mut changed = false;
    for path in history_paths() {
        let Some(current) = sig(&path) else {
            continue;
        };
        let key = path.to_string_lossy().into_owned();
        let previous = state.files.get(&key);
        let is_new = previous.is_none_or(|old| {
            current.len < old.len || current.len != old.len || current.mtime != old.mtime
        });
        if !is_new {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&path) {
            let lines = if key.contains("zsh") {
                parse_zsh(&text)
            } else {
                parse_bash(&text)
            };
            for line in lines {
                let Some(name) = line.split_whitespace().next() else {
                    continue;
                };
                if name.starts_with('#') || name.starts_with(':') {
                    continue;
                }
                crate::frecency::record_command_line_now(name, ignore_patterns);
            }
        }
        state.files.insert(key, current);
        changed = true;
    }
    if changed {
        let _ = std::fs::create_dir_all(state_path().parent().unwrap_or(Path::new(".")));
        if let Ok(json) = serde_json::to_string(&state) {
            let _ = std::fs::write(state_path(), json);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bash_skips_comments_and_meta() {
        let text = "ls -la\n#comment\n: 1700000000\n\ngit status\n";
        assert_eq!(parse_bash(text), vec!["ls -la", "git status"]);
    }

    #[test]
    fn parse_zsh_strips_extended_history_prefix() {
        let text = ": 1700000000:0;git status\n: 1700000001:3;ls\nplain\n";
        assert_eq!(parse_zsh(text), vec!["git status", "ls", "plain"]);
    }
}
