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
    /// 已处理到的字节偏移（只解析新增行）。
    offset: u64,
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

/// 读取全部候选（完整命令行，内存态；上限 5000 行防止异常大文件拖慢启动）。
pub fn read_all_commands() -> Vec<String> {
    const MAX_CANDIDATES: usize = 5000;
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
        if out.len() >= MAX_CANDIDATES {
            out.truncate(MAX_CANDIDATES);
            break;
        }
    }
    out
}

fn sig(path: &Path, offset: u64) -> Option<FileSig> {
    let meta = std::fs::metadata(path).ok()?;
    Some(FileSig {
        len: meta.len(),
        mtime: meta
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        offset,
    })
}

/// 从 `offset` 读取到 EOF，只返回完整行（不含尾部半行）及其新的偏移。
fn read_new_lines(path: &Path, offset: u64) -> Option<(Vec<String>, u64)> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).ok()?;
    let complete = buf
        .iter()
        .rposition(|b| *b == b'\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let text = String::from_utf8_lossy(&buf[..complete]);
    let names: Vec<String> = text
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            (!line.is_empty() && !line.starts_with('#') && !is_zsh_meta(line))
                .then(|| line.to_owned())
        })
        .collect();
    Some((names, offset + complete as u64))
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
    let mut names: Vec<String> = Vec::new();
    for path in history_paths() {
        if std::env::var_os("RVIFM_STARTUP_TRACE").is_some() {
            eprintln!("history file: {}", path.display());
        }
        let key = path.to_string_lossy().into_owned();
        let previous = state.files.get(&key);
        let start = previous
            .filter(|old| {
                // 文件被截断/轮转或同大小原地重写 → 从头再来。
                std::fs::metadata(&path)
                    .map(|m| m.len() >= old.offset && mtime(&path) == Some(old.mtime))
                    .unwrap_or(false)
            })
            .map(|old| old.offset)
            .unwrap_or(0);
        let Some((new_lines, next_offset)) = read_new_lines(&path, start) else {
            continue;
        };
        if std::env::var_os("RVIFM_STARTUP_TRACE").is_some() {
            eprintln!(
                "history new lines: {} offset={}",
                new_lines.len(),
                next_offset
            );
        }
        if next_offset == start && previous.is_some() {
            continue;
        }
        for line in new_lines {
            if let Some(name) = line.split_whitespace().next() {
                names.push(name.to_owned());
            }
        }
        if let Some(current) = sig(&path, next_offset) {
            state.files.insert(key, current);
            changed = true;
        }
    }
    // 只为「尚未入库的命令名」补冷启动数据，且设上限——避免每次启动把
    // 全部历史重复写入 frecency（原逐条写库/无去重是启动慢的主因）。
    if !names.is_empty() {
        let existing = crate::frecency::cmd_scores();
        names.retain(|name| !existing.contains_key(name));
        names.sort();
        names.dedup();
        names.truncate(500);
    }
    // M50 性能修复：一次事务批量写入（原逐条写库，2000 行历史拖慢启动）。
    let t0 = std::time::Instant::now();
    if !names.is_empty() {
        let n = names.len();
        crate::frecency::record_command_names_bulk(&names, ignore_patterns);
        if std::env::var_os("RVIFM_STARTUP_TRACE").is_some() {
            eprintln!("history bulk {n} names: {:?}", t0.elapsed());
        }
    }
    if changed {
        let _ = std::fs::create_dir_all(state_path().parent().unwrap_or(Path::new(".")));
        if let Ok(json) = serde_json::to_string(&state) {
            let _ = std::fs::write(state_path(), json);
        }
    }
}

fn mtime(path: &Path) -> Option<u64> {
    std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
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
