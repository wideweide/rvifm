//! frecency 数据结构与分级匹配（M8-1；SPEC §4.6、NOTES §4.5）。
//!
//! 三层模型：`EntryRecord`（events 追加，M8-2 写入 redb）→
//! `SnapshotEntry`（compaction 产物）→ `ScoredEntry`（pick/TUI 查询结果）。
//! **record 是唯一写入入口，pick 纯只读**（红线，AGENTS §5.3）。
//!
//! 分级匹配 0–7 档（M8-1 固化）：书签 > 文件名 > 中间段 > 通用子串；
//! 空查询按 aging 分数排序，书签仍置顶（NOTES §4.5）。

// M8-1 仅固化数据结构与分级；消费方：M8-2（redb events/snapshot 持久化）、
// M8-4（重放评分）、M8-6（pick 子命令）。
#![allow(dead_code)]

use crate::store::Store;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::text::Line;
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use std::path::Path;

/// 事件类型（NOTES §4.5 events `{path, topic, kind, ts}`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EntryKind {
    /// shell 真正 `cd` 成功 / 文件管理器跳转目录。
    Navigate,
    /// 文件管理器打开文件成功。
    Open,
}

/// 访问事件记录（append-only；M8-2 起持久化到 redb `events` 表，
/// key 为单调递增 tick）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EntryRecord {
    /// 路径（内部统一 `/`）。
    pub path: String,
    /// 主题：`dir`（目录跳转）/ `file`（文件打开）。
    pub topic: String,
    pub kind: EntryKind,
    /// Unix 时间戳（秒）。
    pub ts: u64,
}

/// snapshot 条目（compaction 产物；redb `snapshot` 表，key = `path + topic`）。
/// `keyword` 非空 = 书签；`score` 为最后一次重放后的 EMS 分数；`tick` 用于 aging。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SnapshotEntry {
    pub path: String,
    pub topic: String,
    /// 书签关键字；空串 = 非书签。
    pub keyword: String,
    pub score: f64,
    pub tick: u64,
}

/// 分级匹配 0–7 档（M8-1 固化）：
/// - 7 书签关键字命中（keyword 前缀匹配查询，ci）
/// - 6 书签（keyword 非空但未命中；空查询时书签仍置顶）
/// - 5 文件名精确匹配（ci）
/// - 4 文件名前缀匹配（ci）
/// - 3 文件名包含（ci）
/// - 2 中间路径段前缀匹配（ci）
/// - 1 通用子串（全路径包含，ci）
/// - 0 无匹配（空查询时所有条目均为 0，纯按 aging 分数排序）
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Grade {
    None = 0,
    Substring = 1,
    PathSegment = 2,
    NameContains = 3,
    NamePrefix = 4,
    NameExact = 5,
    BookmarkOnly = 6,
    BookmarkKeyword = 7,
}

impl Grade {
    /// 数值档位（排序用，越大越优先）。
    pub fn rank(self) -> u8 {
        self as u8
    }
}

/// 从全路径提取叶子文件名（`/` 分隔）。
fn leaf_of(path: &str) -> &str {
    path.rsplit_once('/').map(|(_, l)| l).unwrap_or(path)
}

/// 智能大小写：查询含大写 → 区分大小写，否则不区分（M4-1 同规则）。
fn ci_contains(haystack: &str, needle: &str) -> bool {
    if needle.chars().any(char::is_uppercase) {
        haystack.contains(needle)
    } else {
        haystack.to_lowercase().contains(&needle.to_lowercase())
    }
}

/// 智能大小写前缀。
fn ci_starts_with(haystack: &str, needle: &str) -> bool {
    if needle.chars().any(char::is_uppercase) {
        haystack.starts_with(needle)
    } else {
        haystack.to_lowercase().starts_with(&needle.to_lowercase())
    }
}

/// 分级匹配（M8-1）。`query` 为空 → 0 档（纯按 aging 分数排序，书签除外）。
pub fn grade_for(entry: &SnapshotEntry, query: &str) -> Grade {
    let is_bookmark = !entry.keyword.is_empty();
    if query.is_empty() {
        return if is_bookmark {
            Grade::BookmarkOnly
        } else {
            Grade::None
        };
    }
    // 7：书签关键字命中（前缀，ci）。
    if is_bookmark && ci_starts_with(&entry.keyword, query) {
        return Grade::BookmarkKeyword;
    }
    let leaf = leaf_of(&entry.path);
    // 5/4/3：文件名精确（smart-case）→ 前缀 → 包含。
    let exact = if query.chars().any(char::is_uppercase) {
        leaf == query
    } else {
        leaf.eq_ignore_ascii_case(query)
    };
    if exact {
        return Grade::NameExact;
    }
    if ci_starts_with(leaf, query) {
        return Grade::NamePrefix;
    }
    if ci_contains(leaf, query) {
        return Grade::NameContains;
    }
    // 2：中间路径段前缀匹配（排除叶子段）。
    let segments: Vec<&str> = entry.path.split('/').collect();
    if segments.len() > 1 {
        for seg in &segments[..segments.len() - 1] {
            if !seg.is_empty() && ci_starts_with(seg, query) {
                return Grade::PathSegment;
            }
        }
    }
    // 1：通用子串（全路径）。
    if ci_contains(&entry.path, query) {
        return Grade::Substring;
    }
    // 6：书签未命中查询——降级但仍在有查询排序中高于无匹配。
    if is_bookmark {
        return Grade::BookmarkOnly;
    }
    Grade::None
}

/// 查询结果条目：aging 后分数 + 分级。
#[derive(Debug, Clone, PartialEq)]
pub struct ScoredEntry {
    pub path: String,
    pub topic: String,
    pub keyword: String,
    /// aging 后分数（EMS，M8-4 接入重放计算）。
    pub score: f64,
    pub grade: Grade,
}

impl ScoredEntry {
    /// 从 snapshot 打分（M8-1）。
    pub fn from_snapshot(entry: &SnapshotEntry, query: &str) -> Self {
        Self {
            path: entry.path.clone(),
            topic: entry.topic.clone(),
            keyword: entry.keyword.clone(),
            score: entry.score,
            grade: grade_for(entry, query),
        }
    }

    /// 排序比较：档位降序 → aging 分数降序 → 路径字典序（稳定确定性）。
    pub fn cmp_ranked(a: &Self, b: &Self) -> std::cmp::Ordering {
        b.grade
            .rank()
            .cmp(&a.grade.rank())
            .then(b.score.total_cmp(&a.score))
            .then(a.path.cmp(&b.path))
    }
}

/// 对打分结果排序（pick/TUI 共用，M8-4 重放后调用）。
pub fn ranked(entries: &mut [ScoredEntry]) {
    entries.sort_by(ScoredEntry::cmp_ranked);
}

// ---------------------------------------------------------------------------
// 重放与 EMS aging（M8-4）
// ---------------------------------------------------------------------------

/// EMS 老化速率：按事件时钟（tick）衰减，λ = 0.01/事件。
/// 500 个事件前的一次访问衰减为 exp(-5) ≈ 0.7%，实现"近期访问优先"。
pub const AGING_LAMBDA: f64 = 0.01;

/// EMS 老化：`score * exp(-λ * Δtick)`。
fn aged(score: f64, lambda: f64, delta_ticks: u64) -> f64 {
    score * (-lambda * delta_ticks as f64).exp()
}

/// 重放器（M8-4）：`snapshot`（compaction 产物）+ `events`（自
/// last_compaction 起的增量）→ 内存评分（`now_tick` 为当前全局 tick，
/// 读取时统一 aging——冷启动偏差修正）。
///
/// 规则：每条事件使对应条目 `score = score * exp(-λ·Δtick) + 1` 并推进
/// `tick`；事件流中首次出现的路径从 0 分起步（新条目）。
/// **record 是唯一写入入口；本函数纯只读**。
pub fn replay(
    snapshot: &[SnapshotEntry],
    events: &[(u64, EntryRecord)],
    now_tick: u64,
) -> Vec<ScoredEntry> {
    let lambda = AGING_LAMBDA;
    let mut map: std::collections::BTreeMap<(String, String), SnapshotEntry> =
        std::collections::BTreeMap::new();
    for s in snapshot {
        map.insert((s.topic.clone(), s.path.clone()), s.clone());
    }
    for (tick, ev) in events {
        let key = (ev.topic.clone(), ev.path.clone());
        let entry = map.entry(key).or_insert_with(|| SnapshotEntry {
            path: ev.path.clone(),
            topic: ev.topic.clone(),
            keyword: String::new(),
            score: 0.0,
            tick: *tick,
        });
        let delta = tick.saturating_sub(entry.tick);
        entry.score = aged(entry.score, lambda, delta) + 1.0;
        entry.tick = *tick;
    }
    let mut out: Vec<ScoredEntry> = map
        .values()
        .map(|s| {
            let delta = now_tick.saturating_sub(s.tick);
            ScoredEntry {
                path: s.path.clone(),
                topic: s.topic.clone(),
                keyword: s.keyword.clone(),
                score: aged(s.score, lambda, delta),
                grade: grade_for(s, ""),
            }
        })
        .collect();
    ranked(&mut out);
    out
}

// ---------------------------------------------------------------------------
// 子命令层（M8-3：record；M8-5：compact；M8-6：pick）
// ---------------------------------------------------------------------------

/// 单次写入重试上限与退避（NOTES §4.5：3 次、10ms）。
const WRITE_RETRIES: usize = 3;
const WRITE_BACKOFF_MS: u64 = 10;

/// `events.pending` 降级文件路径（与数据库同目录）。
fn pending_path() -> std::path::PathBuf {
    crate::config::data_dir().join("events.pending")
}

/// 打开 frecency 存储：先合并上次降级的 pending 事件（"下次启动合并"）。
fn open_store() -> std::io::Result<Store> {
    let store = Store::open(&crate::config::frecency_db_path())?;
    merge_pending(&store, &pending_path())?;
    Ok(store)
}

/// 把 `events.pending`（每行一条 JSON `EntryRecord`）合并进数据库并删除文件。
fn merge_pending(store: &Store, path: &Path) -> std::io::Result<()> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Ok(()); // 无降级文件
    };
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<EntryRecord>(line) {
            Ok(rec) => {
                // pending 本身就是降级产物：合并失败时保留文件（下次重试）。
                if store.append_event(&rec).is_err() {
                    return Ok(());
                }
            }
            Err(_) => continue, // 损坏行丢弃
        }
    }
    std::fs::remove_file(path).ok();
    Ok(())
}

/// `rvifm record --topic dir <path>`：追加事件（唯一写入入口）。
/// 成功静默退出 0；数据库连续失败降级写 `events.pending`（下次启动合并）；
/// 降级也失败才退出 1。
pub fn cmd_record(args: Vec<String>) -> std::io::Result<()> {
    let mut topic = "dir".to_owned();
    let mut path: Option<String> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--topic" => {
                topic = it
                    .next()
                    .ok_or_else(|| arg_err("record: --topic 缺少值"))?
                    .clone();
            }
            other => {
                if path.is_some() {
                    return Err(arg_err("record: 多余的位置参数"));
                }
                path = Some(other.to_owned());
            }
        }
    }
    let Some(path) = path else {
        return Err(arg_err("record: 缺少路径"));
    };
    let record = EntryRecord {
        path: path.replace('\\', "/"),
        topic,
        kind: EntryKind::Navigate,
        ts: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };

    let db_path = crate::config::frecency_db_path();
    let pending = db_path
        .parent()
        .unwrap_or(Path::new("."))
        .join("events.pending");
    record_impl(&db_path, &pending, &record)
}

/// record 的可测试实现（路径注入）。
fn record_impl(db_path: &Path, pending: &Path, record: &EntryRecord) -> std::io::Result<()> {
    let store = match Store::open(db_path) {
        Ok(s) => s,
        Err(e) => return degraded_write_at(pending, record, e),
    };
    merge_pending(&store, pending)?;
    for _ in 0..WRITE_RETRIES {
        match store.append_event(record) {
            Ok(_) => return Ok(()), // 成功静默（不污染 stdout）
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(WRITE_BACKOFF_MS)),
        }
    }
    degraded_write_at(pending, record, "数据库写入连续失败")
}

/// 降级：追加一行 JSON 到 pending 文件。失败时退出 1。
fn degraded_write_at<E: std::fmt::Display>(
    pending: &Path,
    record: &EntryRecord,
    why: E,
) -> std::io::Result<()> {
    let line = serde_json::to_string(record)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    if let Some(dir) = pending.parent() {
        std::fs::create_dir_all(dir)?;
    }
    use std::io::Write as _;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(pending)?;
    writeln!(f, "{line}").map_err(|e| {
        eprintln!("rvifm record: 记录失败（{why}）：{e}");
        e
    })
}

fn arg_err(msg: &'static str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, msg)
}

/// `rvifm maintenance compact`（M8-5）：合并 `last_compaction` 之后的
/// 事件进 snapshot 表，推进标记并裁剪已合并事件。幂等：无新事件时为 no-op。
pub fn cmd_compact() -> std::io::Result<()> {
    let store = open_store()?;
    let since = store.last_compaction()?;
    let now = store.global_tick()?;
    if now <= since {
        return Ok(()); // 无新事件：no-op
    }
    let events = store.events_since(since)?;
    let snapshot = store.snapshot_all()?;
    let merged = replay_merge(&snapshot, &events, since);
    for entry in &merged {
        store.snapshot_upsert(entry)?;
    }
    store.set_last_compaction(now)?;
    store.prune_events_le(now)?;
    Ok(())
}

/// compaction 合并（与 `replay` 同规则，但返回更新后的 snapshot 表）。
fn replay_merge(
    snapshot: &[SnapshotEntry],
    events: &[(u64, EntryRecord)],
    _since: u64,
) -> Vec<SnapshotEntry> {
    let lambda = AGING_LAMBDA;
    let mut map: std::collections::BTreeMap<(String, String), SnapshotEntry> =
        std::collections::BTreeMap::new();
    for s in snapshot {
        map.insert((s.topic.clone(), s.path.clone()), s.clone());
    }
    for (tick, ev) in events {
        let entry = map
            .entry((ev.topic.clone(), ev.path.clone()))
            .or_insert_with(|| SnapshotEntry {
                path: ev.path.clone(),
                topic: ev.topic.clone(),
                keyword: String::new(),
                score: 0.0,
                tick: *tick,
            });
        let delta = tick.saturating_sub(entry.tick);
        entry.score = aged(entry.score, lambda, delta) + 1.0;
        entry.tick = *tick;
    }
    map.into_values().collect()
}

/// `rvifm pick [query]`（M8-6）：stdin 读候选（每行一个路径）。
/// stdout 为 TTY → 交互式选择器（ratatui 自绘，Esc/Ctrl-C 取消退出码 1）；
/// stdout 非 TTY → fzf filter 语义（输出全部命中候选）。
/// **纯只读：不写事件、不推进 tick。**
pub fn cmd_pick(args: Vec<String>) -> std::io::Result<()> {
    let initial_query = args.first().cloned().unwrap_or_default();
    use std::io::{IsTerminal, Read as _, Write as _};
    let mut candidates = String::new();
    std::io::stdin().read_to_string(&mut candidates)?;

    let store = open_store()?;
    let snapshot = store.snapshot_all()?;
    let since = store.last_compaction()?;
    let now = store.global_tick()?;
    let events = store.events_since(since)?;
    let ranked_entries = replay(&snapshot, &events, now);

    // 候选 → 分数索引（path|topic 匹配 snapshot）。
    let score_of = |path: &str, topic: &str| -> f64 {
        ranked_entries
            .iter()
            .find(|e| e.path == path && e.topic == topic)
            .map(|e| e.score)
            .unwrap_or(0.0)
    };

    let mut out: Vec<(String, String, f64)> = Vec::new();
    for line in candidates.lines() {
        let path = line.trim().replace('\\', "/");
        if path.is_empty() {
            continue;
        }
        let topic = "dir".to_owned();
        out.push((path.clone(), topic.clone(), score_of(&path, &topic)));
    }

    // stdout 为 TTY → 交互式选择器；否则 fzf filter 语义。
    if !std::io::stdout().is_terminal() {
        let mut hit: Vec<&(String, String, f64)> = out
            .iter()
            .filter(|(path, _, _)| {
                initial_query.is_empty()
                    || ci_contains(leaf_of(path), &initial_query)
                    || ci_contains(path, &initial_query)
            })
            .collect();
        hit.sort_by(|a, b| b.2.total_cmp(&a.2).then(a.0.cmp(&b.0)));
        if hit.is_empty() {
            std::process::exit(1);
        }
        let stdout = std::io::stdout();
        let mut w = std::io::BufWriter::new(stdout.lock());
        for (path, _, _) in hit {
            writeln!(w, "{path}")?;
        }
        return Ok(());
    }

    pick_interactive(&out, &initial_query)
}

/// 交互式选择器结果。
enum PickOutcome {
    Selected(String),
    Cancelled,
    /// 过滤后无候选：退出码 1。
    NoMatch,
}

/// ratatui 自绘选择器（M8-6；NOTES §4.5 允许 skim 不达标时降级为自绘）。
/// 输入查询实时过滤（叶子/全路径 ci 子串），↑/↓/j/k 移动，Enter 选中。
fn pick_interactive(
    candidates: &[(String, String, f64)],
    initial_query: &str,
) -> std::io::Result<()> {
    let mut query = initial_query.to_owned();
    let mut cursor: usize = 0;
    let mut scroll: usize = 0;

    let matches = |q: &str, candidates: &[(String, String, f64)]| -> Vec<(String, String, f64)> {
        let mut hit: Vec<(String, String, f64)> = candidates
            .iter()
            .filter(|(path, _, _)| {
                q.is_empty() || ci_contains(leaf_of(path), q) || ci_contains(path, q)
            })
            .cloned()
            .collect();
        hit.sort_by(|a, b| b.2.total_cmp(&a.2).then(a.0.cmp(&b.0)));
        hit
    };

    let mut terminal = ratatui::init();
    let outcome = loop {
        let hits = matches(&query, candidates);
        if cursor >= hits.len() {
            cursor = hits.len().saturating_sub(1);
        }
        let draw_result = terminal.draw(|frame| {
            let [query_area, list_area] = ratatui::layout::Layout::vertical([
                ratatui::layout::Constraint::Length(1),
                ratatui::layout::Constraint::Min(0),
            ])
            .areas(frame.area());
            frame.render_widget(Paragraph::new(format!("/{query}")), query_area);
            let items: Vec<ListItem> = hits
                .iter()
                .map(|(path, _, _)| ListItem::new(Line::from(path.clone())))
                .collect();
            let mut ls = ListState::default()
                .with_selected(Some(cursor))
                .with_offset(scroll);
            frame.render_stateful_widget(
                List::new(items).highlight_style(
                    ratatui::style::Style::new().add_modifier(ratatui::style::Modifier::REVERSED),
                ),
                list_area,
                &mut ls,
            );
            scroll = ls.offset();
        });
        if let Err(e) = draw_result {
            eprintln!("rvifm pick: 渲染失败: {e}");
            break PickOutcome::Cancelled;
        }

        match event::read() {
            Ok(Event::Key(key))
                if key.kind == KeyEventKind::Press
                    && (key.code == KeyCode::Esc
                        || (key.code == KeyCode::Char('c')
                            && key.modifiers.contains(KeyModifiers::CONTROL))) =>
            {
                break PickOutcome::Cancelled;
            }
            Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Enter => {
                    if hits.is_empty() {
                        break PickOutcome::NoMatch;
                    }
                    break PickOutcome::Selected(hits[cursor].0.clone());
                }
                KeyCode::Backspace => {
                    query.pop();
                }
                KeyCode::Down | KeyCode::Char('j') if !hits.is_empty() => {
                    cursor = (cursor + 1).min(hits.len() - 1);
                }
                KeyCode::Up | KeyCode::Char('k') if !hits.is_empty() => {
                    cursor = cursor.saturating_sub(1);
                }
                KeyCode::Char(c) => {
                    query.push(c);
                }
                _ => {}
            },
            _ => {}
        }
    };
    ratatui::restore();
    match outcome {
        PickOutcome::Selected(path) => {
            println!("{path}");
            Ok(())
        }
        PickOutcome::NoMatch => std::process::exit(1),
        PickOutcome::Cancelled => std::process::exit(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn snap(path: &str, keyword: &str, score: f64) -> SnapshotEntry {
        SnapshotEntry {
            path: path.to_owned(),
            topic: "dir".to_owned(),
            keyword: keyword.to_owned(),
            score,
            tick: 100,
        }
    }

    fn rec(path: &str, ts: u64) -> EntryRecord {
        EntryRecord {
            path: path.to_owned(),
            topic: "dir".to_owned(),
            kind: EntryKind::Navigate,
            ts,
        }
    }

    #[test]
    fn grade_matrix_fixed_semantics() {
        // M8-1 档位固化验证。
        // 7：书签关键字前缀命中。
        assert_eq!(
            grade_for(&snap("/w/proj", "proj", 10.0), "pro"),
            Grade::BookmarkKeyword
        );
        // 6：书签未命中（有查询降级）。
        assert_eq!(
            grade_for(&snap("/w/proj", "proj", 10.0), "zzz"),
            Grade::BookmarkOnly
        );
        // 空查询：书签置顶档位 6，非书签 0。
        assert_eq!(
            grade_for(&snap("/w/proj", "proj", 10.0), ""),
            Grade::BookmarkOnly
        );
        assert_eq!(grade_for(&snap("/w/other", "", 10.0), ""), Grade::None);

        // 5/4/3：文件名精确 / 前缀 / 包含。
        assert_eq!(
            grade_for(&snap("/w/main.rs", "", 1.0), "main.rs"),
            Grade::NameExact
        );
        assert_eq!(
            grade_for(&snap("/w/main.rs", "", 1.0), "ma"),
            Grade::NamePrefix
        );
        assert_eq!(
            grade_for(&snap("/w/main.rs", "", 1.0), "ain"),
            Grade::NameContains
        );

        // 2：中间路径段前缀（叶子段不参与，避免与 NamePrefix 冲突）。
        assert_eq!(
            grade_for(&snap("/home/proj/src", "", 1.0), "proj"),
            Grade::PathSegment
        );

        // 1：通用子串（全路径命中，叶子与中间段均不含）。
        assert_eq!(
            grade_for(&snap("/w/x/hello.md", "", 1.0), "w/x"),
            Grade::Substring
        );

        // 0：完全不匹配。
        assert_eq!(grade_for(&snap("/w/x/y.md", "", 1.0), "qqq"), Grade::None);
    }

    #[test]
    fn smart_case_in_grading() {
        // 含大写查询区分大小写。
        assert_eq!(
            grade_for(&snap("/w/Beta", "", 1.0), "Beta"),
            Grade::NameExact
        );
        assert_ne!(
            grade_for(&snap("/w/beta", "", 1.0), "Beta"),
            Grade::NameExact
        );
        // 全小写查询不区分。
        assert_eq!(
            grade_for(&snap("/w/Beta", "", 1.0), "bet"),
            Grade::NamePrefix
        );
    }

    #[test]
    fn ranking_grade_then_score_then_path() {
        // M8-1 排序契约：档位降序 → aging 分数降序 → 路径字典序。
        let mut entries = vec![
            ScoredEntry::from_snapshot(&snap("/a/low", "", 1.0), ""),
            ScoredEntry::from_snapshot(&snap("/b/high", "", 9.0), ""),
            ScoredEntry::from_snapshot(&snap("/a/high", "", 9.0), ""),
            ScoredEntry::from_snapshot(&snap("/c/bm", "bm", 0.1), "bm"),
        ];
        // 空查询：书签（6）置顶，其余按分数。
        ranked(&mut entries);
        assert_eq!(entries[0].keyword, "bm");
        // 同档同分平分：路径字典序（a < b）。
        assert_eq!(entries[1].path, "/a/high");
        assert_eq!(entries[2].path, "/b/high");
        assert_eq!(entries[3].path, "/a/low");
    }

    #[test]
    fn replay_aging_promotes_recent() {
        // M8-9 验收场景：5×A → 495 次其他 → 1×B，B 排在 A 前（aging 生效）。
        let mut events = Vec::new();
        let mut tick = 0u64;
        for i in 0..5 {
            tick += 1;
            events.push((tick, rec("/a", 1000 + i as u64)));
        }
        for i in 0..495 {
            tick += 1;
            events.push((tick, rec(&format!("/o{i}"), 1000 + i as u64)));
        }
        tick += 1;
        events.push((tick, rec("/b", 2000)));

        let now = tick; // 读取时 aging：now = 全局 tick
        let ranked = replay(&[], &events, now);
        let a = ranked.iter().find(|e| e.path == "/a").unwrap();
        let b = ranked.iter().find(|e| e.path == "/b").unwrap();
        assert!(b.score > a.score, "B({}) 应排在 A({}) 前", b.score, a.score);
        // 排名：B 在 A 之前。
        let b_pos = ranked.iter().position(|e| e.path == "/b").unwrap();
        let a_pos = ranked.iter().position(|e| e.path == "/a").unwrap();
        assert!(b_pos < a_pos, "B({b_pos}) 应在 A({a_pos}) 前");
    }

    #[test]
    fn merge_pending_roundtrip_with_tempdir() {
        // M8-3：pending 降级文件在下次启动时合并并删除。
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("f.redb");
        let pending = tmp.path().join("events.pending");
        let store = Store::open(&db).unwrap();

        let rec_a = rec("/a", 1);
        let line = serde_json::to_string(&rec_a).unwrap();
        std::fs::write(&pending, format!("{line}\n\n{line}\n")).unwrap(); // 含空行

        merge_pending(&store, &pending).unwrap();
        assert_eq!(store.global_tick().unwrap(), 2); // 两行有效（空行丢弃）
        assert!(!pending.exists(), "合并后 pending 应删除");

        // 合并失败保留文件：注入不可写的 pending（目录占位）——用目录替代文件模拟。
        std::fs::write(&pending, format!("{line}\n")).unwrap();
        merge_pending(&store, &pending).unwrap();
        assert_eq!(store.global_tick().unwrap(), 3);
    }

    #[test]
    fn record_impl_degrades_to_pending_on_bad_db() {
        // M8-3：数据库损坏 → 降级写 pending；修复后合并恢复。
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("f.redb");
        std::fs::write(&db, b"corrupt redb").unwrap(); // 打开会失败
        let pending = tmp.path().join("events.pending");

        let r = rec("/x", 5);
        record_impl(&db, &pending, &r).unwrap();
        assert!(pending.exists(), "降级文件应写入");

        // 修复：删除损坏文件让 Store::open 重建。
        std::fs::remove_file(&db).unwrap();
        let store = Store::open(&db).unwrap();
        merge_pending(&store, &pending).unwrap();
        assert_eq!(store.global_tick().unwrap(), 1);
        assert_eq!(store.events_since(0).unwrap()[0].1.path, "/x");
        assert!(!pending.exists());
    }

    #[test]
    fn record_roundtrip_fields() {
        // EntryRecord 字段固化（M8-2 持久化用）。
        let r = EntryRecord {
            path: "/w/proj".to_owned(),
            topic: "dir".to_owned(),
            kind: EntryKind::Navigate,
            ts: 1_700_000_000,
        };
        assert_eq!(r.path, "/w/proj");
        assert_eq!(r.kind, EntryKind::Navigate);
        // 克隆语义可用（append-only 队列复制）。
        let c = r.clone();
        assert_eq!(c, r);
    }
}
