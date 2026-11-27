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
    /// 书签 keyword 设置（M16 NAV-1）：元数据事件，重放时只应用 keyword
    /// 字段，不推进时钟、不加分。
    Keyword,
    /// 容量清理墓碑（M23-3）：重放时删除该条目（带 keyword 条目永不清理）。
    Remove,
    /// M30-1：`:` 命令执行成功（cmd topic；path 承载命令文本）。
    Command,
}

/// 访问事件记录（append-only；M8-2 起持久化到 redb `events` 表，
/// key 为全局单调递增序号）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EntryRecord {
    /// 路径（内部统一 `/`）。
    pub path: String,
    /// 主题：`dir`（目录跳转）/ `file`（文件打开）。
    pub topic: String,
    pub kind: EntryKind,
    /// Unix 时间戳（秒）。
    pub ts: u64,
    /// 发生时该事件**类型时钟**的值（M8-10：dir/file 各自独立推进）。
    /// 旧格式无此字段 → 0（重放侧以全局序号兜底）。
    #[serde(default)]
    pub clock: u64,
    /// keyword 事件携带的书签字符（M16 NAV-1）；其余事件为空串。
    /// 旧格式无此字段 → 空（重放侧跳过）。
    #[serde(default)]
    pub keyword: String,
}

/// snapshot 条目（compaction 产物；redb `snapshot` 表，key = `path + topic`）。
/// `keyword` 非空 = 书签；`score` 为最后一次重放后的 EMS 分数；
/// `clock` = 最后一次应用到该条目的**类型时钟**值（M8-10）；`tick` 为
/// 全局事件序号（兼容字段，仅在 `clock == 0` 的旧数据下参与 aging 兜底）。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SnapshotEntry {
    pub path: String,
    pub topic: String,
    /// 书签关键字；空串 = 非书签。
    pub keyword: String,
    pub score: f64,
    pub tick: u64,
    #[serde(default)]
    pub clock: u64,
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
    // M75-2：空查询不置顶书签（上游无查询只按分数排序）；filter_jump 会直接返回全部。
    if query.is_empty() {
        return Grade::None;
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
    // M75-2：删除 BookmarkOnly 兜底——书签也必须与查询真实匹配才显示，
    // 与上游 frecency 的「书签不置顶、不靠兜底混入」对齐。
    let _ = is_bookmark;
    Grade::None
}

/// M23-1：多词查询匹配分级——每词独立 grade_for 取最低档；词按序出现在
/// 路径（ci 顺序扫描）→ 保持档位，乱序全命中 → 降为 Substring 上限；
/// 单词/空查询退化为 grade_for。
pub fn grade_for_multi(entry: &SnapshotEntry, query: &str) -> Grade {
    let words: Vec<&str> = query.split_whitespace().collect();
    if words.len() <= 1 {
        return grade_for(entry, query);
    }
    let mut min = Grade::BookmarkKeyword;
    for w in &words {
        let g = grade_for(entry, w);
        if g == Grade::None {
            return Grade::None;
        }
        if g.rank() < min.rank() {
            min = g;
        }
    }
    // ordered 判定：词按序出现在路径（ci 顺序扫描，命中点后移）。
    let mut rest = entry.path.as_str();
    let mut ordered = true;
    for w in &words {
        let (w_l, r_l) = (w.to_lowercase(), rest.to_lowercase());
        match r_l.find(&w_l) {
            Some(pos) => {
                rest = &rest[pos + w.len()..];
            }
            None => {
                ordered = false;
                break;
            }
        }
    }
    if !ordered && min.rank() > Grade::Substring.rank() {
        min = Grade::Substring;
    }
    min
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
    /// 最后一次访问的 Unix 时间戳（M16 NAV-4：近期视角排序用；
    /// snapshot-only 条目为 0，排序时靠后）。
    pub last_visit: u64,
}

impl ScoredEntry {
    /// 从 snapshot 打分（M8-1；M23-1 起经 grade_for_multi 支持多词）。
    pub fn from_snapshot(entry: &SnapshotEntry, query: &str) -> Self {
        Self {
            path: entry.path.clone(),
            topic: entry.topic.clone(),
            keyword: entry.keyword.clone(),
            score: entry.score,
            grade: grade_for_multi(entry, query),
            last_visit: 0,
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

/// M23-5：per-topic frecency 设置（OnceLock；run/pick/record 启动时初始化，
/// 未初始化用默认值——旧配置缺省兼容）。
#[derive(Debug, Clone, Copy)]
pub struct FrecencySettings {
    pub lambda_dir: f64,
    pub lambda_file: f64,
    pub lambda_unified: f64,
    /// M30-1：cmd topic 衰减速率（缺省 0.01，与 dir/file 同）。
    pub lambda_cmd: f64,
    pub max_entries: usize,
}

impl Default for FrecencySettings {
    fn default() -> Self {
        Self {
            lambda_dir: AGING_LAMBDA,
            lambda_file: AGING_LAMBDA,
            lambda_unified: 0.005, // unified 时钟混合计数 dir+file 事件
            lambda_cmd: AGING_LAMBDA,
            max_entries: MAX_ENTRIES_DEFAULT,
        }
    }
}

static SETTINGS: std::sync::Mutex<FrecencySettings> = std::sync::Mutex::new(FrecencySettings {
    lambda_dir: AGING_LAMBDA,
    lambda_file: AGING_LAMBDA,
    lambda_unified: 0.005,
    lambda_cmd: AGING_LAMBDA,
    max_entries: MAX_ENTRIES_DEFAULT,
});

/// M23-5：初始化 per-topic 设置（TUI run / pick / record 启动时调用；
/// None 字段取默认值，旧配置缺省兼容）。AUDIT-8：锁毒化时静默跳过
/// （保留原设置，不 panic）。
pub fn init_settings(cfg: &crate::config::FrecencyConfig) {
    let d = FrecencySettings::default();
    if let Ok(mut s) = SETTINGS.lock() {
        *s = FrecencySettings {
            lambda_dir: cfg.lambda_dir.unwrap_or(d.lambda_dir),
            lambda_file: cfg.lambda_file.unwrap_or(d.lambda_file),
            lambda_unified: cfg.lambda_unified.unwrap_or(d.lambda_unified),
            lambda_cmd: cfg.lambda_cmd.unwrap_or(d.lambda_cmd),
            max_entries: cfg.max_entries.unwrap_or(d.max_entries),
        };
    }
}

fn settings() -> FrecencySettings {
    // AUDIT-8：锁毒化时回退默认设置（不 panic）。
    SETTINGS.lock().map(|s| *s).unwrap_or_default()
}

/// EMS 老化：`score * exp(-λ * Δtick)`。
fn aged(score: f64, lambda: f64, delta_ticks: u64) -> f64 {
    score * (-lambda * delta_ticks as f64).exp()
}

/// 重放器（M8-4 / M8-10）：`snapshot`（compaction 产物）+ `events`（自
/// last_compaction 起的增量）→ 内存评分（`now_dir`/`now_file` 为当前
/// **类型时钟**值，读取时按条目 topic 对齐 aging——冷启动偏差修正）。
///
/// 规则：每条事件使对应条目 `score = score * exp(-λ·Δclock) + 1` 并推进
/// 其类型时钟；事件流中首次出现的路径从 0 分起步（新条目）。
/// 兼容：事件 `clock == 0`（旧格式）以全局序号兜底，snapshot `clock == 0`
/// 以 `tick` 兜底——旧数据 aging 行为与旧版一致。
/// **record 是唯一写入入口；本函数纯只读**。
pub fn replay(
    snapshot: &[SnapshotEntry],
    events: &[(u64, EntryRecord)],
    now_dir: u64,
    now_file: u64,
    now_cmd: u64,
) -> Vec<ScoredEntry> {
    replay_inner(
        snapshot,
        events,
        ReplayClocks::PerType {
            now_dir,
            now_file,
            now_cmd,
        },
    )
}

/// 统一视角（M8-12）：全部条目按**全局事件序号** aging（dir + file 混合
/// 排名）——保留合一表/全局时钟的旧语义，供  合一跳转选择器使用。
pub fn replay_unified(
    snapshot: &[SnapshotEntry],
    events: &[(u64, EntryRecord)],
    now_seq: u64,
) -> Vec<ScoredEntry> {
    replay_inner(snapshot, events, ReplayClocks::Unified(now_seq))
}

/// 时钟视角（M8-12）：PerType = dir/file 各自独立时钟；Unified = 全局序号。
#[derive(Debug, Clone, Copy)]
enum ReplayClocks {
    PerType {
        now_dir: u64,
        now_file: u64,
        /// M30-1：cmd 类型时钟（命令历史独立衰减）。
        now_cmd: u64,
    },
    Unified(u64),
}

fn replay_inner(
    snapshot: &[SnapshotEntry],
    events: &[(u64, EntryRecord)],
    clocks: ReplayClocks,
) -> Vec<ScoredEntry> {
    // M23-5：per-topic λ（PerType 按事件 topic 取；Unified 用统一 λ）。
    let st = settings();
    let unified = matches!(clocks, ReplayClocks::Unified(_));
    let lambda_for_topic = |topic: &str| {
        if unified {
            st.lambda_unified
        } else if topic == "file" {
            st.lambda_file
        } else if topic == "cmd" {
            st.lambda_cmd
        } else {
            st.lambda_dir
        }
    };
    let mut map: std::collections::BTreeMap<(String, String), SnapshotEntry> =
        std::collections::BTreeMap::new();
    for s in snapshot {
        map.insert((s.topic.clone(), s.path.clone()), s.clone());
    }
    // M16 NAV-4：每条目最后访问的墙钟时间（事件 ts；snapshot-only 为 0）。
    let mut last_visit: std::collections::BTreeMap<(String, String), u64> =
        std::collections::BTreeMap::new();
    for (seq, ev) in events {
        let lv = last_visit
            .entry((ev.topic.clone(), ev.path.clone()))
            .or_default();
        *lv = (*lv).max(ev.ts);
        let key = (ev.topic.clone(), ev.path.clone());
        // 统一视角一律用全局序号；分型视角用事件携带的类型时钟（旧格式兜底）。
        let clock = match clocks {
            ReplayClocks::Unified(_) => *seq,
            ReplayClocks::PerType { .. } => {
                if ev.clock == 0 {
                    *seq
                } else {
                    ev.clock
                }
            }
        };
        // M23-3：容量清理墓碑——删除该条目（其后访问可重新创建）。
        if matches!(ev.kind, EntryKind::Remove) {
            map.remove(&key);
            continue;
        }
        let entry = map.entry(key).or_insert_with(|| SnapshotEntry {
            path: ev.path.clone(),
            topic: ev.topic.clone(),
            keyword: String::new(),
            score: 0.0,
            tick: *seq,
            clock,
        });
        // M16 NAV-1：keyword 事件是元数据——只应用 keyword，不加分、
        // 不推进时钟（书签不因设置动作本身获得访问权重）。
        if matches!(ev.kind, EntryKind::Keyword) {
            entry.keyword = ev.keyword.clone();
            continue;
        }
        let delta = clock.saturating_sub(entry.clock);
        entry.score = aged(entry.score, lambda_for_topic(&ev.topic), delta) + 1.0;
        entry.tick = *seq;
        entry.clock = clock;
    }
    let mut out: Vec<ScoredEntry> = map
        .values()
        .map(|s| {
            // 统一视角：now = 全局序号，clock 兜底 tick（全局序号语义）。
            let (now, clock) = match clocks {
                ReplayClocks::Unified(now) => (now, s.tick),
                ReplayClocks::PerType {
                    now_dir,
                    now_file,
                    now_cmd,
                } => {
                    let now = match s.topic.as_str() {
                        "dir" => now_dir,
                        "cmd" => now_cmd,
                        _ => now_file,
                    };
                    let clock = if s.clock == 0 { s.tick } else { s.clock };
                    (now, clock)
                }
            };
            let delta = now.saturating_sub(clock);
            ScoredEntry {
                path: s.path.clone(),
                topic: s.topic.clone(),
                keyword: s.keyword.clone(),
                score: aged(s.score, lambda_for_topic(&s.topic), delta),
                grade: grade_for(s, ""),
                last_visit: last_visit
                    .get(&(s.topic.clone(), s.path.clone()))
                    .copied()
                    .unwrap_or(0),
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

/// 只读打开 frecency 存储（AUDIT-1）：不建表、不合并 pending、不写任何事务。
/// `pick`（红线：纯只读）与 TUI 跳转选择器共用。
/// 库缺失 → `Ok(None)`（调用方按空库降级：全 0 分 / 空选择器）。
pub fn open_store_readonly() -> std::io::Result<Option<Store>> {
    open_store_readonly_at(&crate::config::frecency_db_path())
}

/// 注入路径形态（测试用）；语义同 `open_store_readonly`。
fn open_store_readonly_at(db_path: &Path) -> std::io::Result<Option<Store>> {
    // redb 单写多读；并发写事务/另一进程短暂持锁时有限重试（10ms×3），
    // 避免并行测试或双开 rvifm 时偶发 "Database already open"。
    let mut last = None;
    for _ in 0..10 {
        match Store::open_readonly(db_path) {
            Ok(store) => return Ok(Some(store)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                last = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("frecency 只读打开失败")))
}

// ── M38-4：frecency 数据管理（:frecency stats / export / purge / prune） ──

/// M38-4：frecency 数据库统计快照（只读）。
#[derive(Debug, serde::Serialize)]
pub struct FrecencyStats {
    pub db_path: String,
    pub db_size_bytes: u64,
    pub events: usize,
    /// topic → snapshot 条目数（`dir`/`file`/`cmd`；升序）。
    pub snapshots_by_topic: Vec<(String, usize)>,
    /// meta 全部键值（时钟 / last_compaction）。
    pub meta: Vec<(String, u64)>,
}

/// M38-4：导出数据（JSON 序列化目标；测试侧 round-trip 反序列化）。
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct FrecencyExport {
    pub meta: Vec<(String, u64)>,
    pub snapshot: Vec<SnapshotEntry>,
    pub events: Vec<(u64, EntryRecord)>,
}

/// M38-4：`YYYY-MM-DD` → Unix 秒（UTC 0 点）。非法输入返回 None。
/// 纯 std 实现（Hinnant days_from_civil），不引日期 crate。
pub(crate) fn parse_date_ts(s: &str) -> Option<u64> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let y: i64 = parts[0].parse().ok()?;
    let m: i64 = parts[1].parse().ok()?;
    let d: i64 = parts[2].parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let y_adj = if m <= 2 { y - 1 } else { y };
    let era = y_adj.div_euclid(400);
    let yoe = y_adj - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    u64::try_from(days * 86400).ok()
}

/// M38-4：统计（默认库路径；库缺失降级为全 0 统计）。
pub fn frecency_stats() -> std::io::Result<FrecencyStats> {
    frecency_stats_at(&crate::config::frecency_db_path())
}

/// 注入路径形态（测试用）。
fn frecency_stats_at(db_path: &Path) -> std::io::Result<FrecencyStats> {
    let zero = FrecencyStats {
        db_path: db_path.display().to_string(),
        db_size_bytes: std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0),
        events: 0,
        snapshots_by_topic: Vec::new(),
        meta: Vec::new(),
    };
    let Some(store) = open_store_readonly_at(db_path)? else {
        return Ok(zero);
    };
    let events = store.events_count()?;
    let snapshot = store.snapshot_all()?;
    let mut by_topic: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for e in &snapshot {
        *by_topic.entry(e.topic.clone()).or_default() += 1;
    }
    Ok(FrecencyStats {
        db_path: db_path.display().to_string(),
        db_size_bytes: std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0),
        events,
        snapshots_by_topic: by_topic.into_iter().collect(),
        meta: store.meta_all()?,
    })
}

/// M38-4：导出 JSON 到文件（默认库路径）。返回导出的事件条数。
/// 库缺失 → 空数据文件（meta/snapshot/events 全空）。
pub fn frecency_export(file: &str) -> std::io::Result<usize> {
    frecency_export_at(&crate::config::frecency_db_path(), file)
}

/// 注入路径形态（测试用）。
fn frecency_export_at(db_path: &Path, file: &str) -> std::io::Result<usize> {
    let data = match open_store_readonly_at(db_path)? {
        Some(store) => FrecencyExport {
            meta: store.meta_all()?,
            snapshot: store.snapshot_all()?,
            events: store.events_since(0)?,
        },
        None => FrecencyExport {
            meta: Vec::new(),
            snapshot: Vec::new(),
            events: Vec::new(),
        },
    };
    let n = data.events.len();
    let json = serde_json::to_vec_pretty(&data).map_err(std::io::Error::other)?;
    std::fs::write(file, json)?;
    Ok(n)
}

/// M38-4：全清（写路径；pending 降级文件一并删除）。返回 (events, snapshot)
/// 删除数。库缺失 → 先建空库再清（结果 (0, 0)）。
pub fn frecency_purge() -> std::io::Result<(usize, usize)> {
    let counts = frecency_purge_at(&crate::config::frecency_db_path())?;
    let _ = std::fs::remove_file(pending_path());
    Ok(counts)
}

/// 注入路径形态（测试用；仅库内三表，不动全局 pending 文件）。
fn frecency_purge_at(db_path: &Path) -> std::io::Result<(usize, usize)> {
    let store = Store::open(db_path)?;
    store.purge_all()
}

/// M38-4：按时间裁剪事件（写路径）——删除 `ts < before` 的事件；snapshot
/// 不动（分数经 EMS 衰减与容量清理自然收敛，见 NOTES M38-4）。返回删除数。
pub fn frecency_prune_before(before: u64) -> std::io::Result<usize> {
    frecency_prune_before_at(&crate::config::frecency_db_path(), before)
}

/// 注入路径形态（测试用）。
fn frecency_prune_before_at(db_path: &Path, before: u64) -> std::io::Result<usize> {
    let store = Store::open(db_path)?;
    store.prune_events_before(before)
}

/// TUI 启动合并钩子（AUDIT-1）：把上次降级的 `events.pending` 合并进库。
/// 合并只发生在 `record` 写路径与 TUI 启动；`pick` / 跳转选择器纯只读。
/// 打开失败静默跳过（保留 pending，下次启动或 record 再试）。
pub fn merge_pending_startup() {
    let _ = merge_pending_at(&crate::config::frecency_db_path(), &pending_path());
}

/// 注入路径形态的启动合并（`merge_pending_startup` 与测试共用）。
fn merge_pending_at(db_path: &Path, pending: &Path) -> std::io::Result<()> {
    let store = Store::open(db_path)?;
    merge_pending(&store, pending)
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
    // M23-5：加载 [frecency] 设置（λ/容量上限）。
    init_settings(&crate::config::load().unwrap_or_default().frecency);
    let mut topic = "dir".to_owned();
    let mut path: Option<String> = None;
    let mut keyword: Option<String> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--topic" => {
                topic = it
                    .next()
                    .ok_or_else(|| arg_err("record: --topic 缺少值"))?
                    .clone();
            }
            "--keyword" => {
                keyword = Some(
                    it.next()
                        .ok_or_else(|| arg_err("record: --keyword 缺少值"))?
                        .clone(),
                );
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
    let is_cmd = topic == "cmd";
    let record = EntryRecord {
        path: path.replace('\\', "/"),
        topic,
        // M16 NAV-1：--keyword → 纯元数据事件（不记访问）。
        // M30-1：cmd topic 访问 → Command kind（命令历史）。
        kind: if keyword.is_some() {
            EntryKind::Keyword
        } else if is_cmd {
            EntryKind::Command
        } else {
            EntryKind::Navigate
        },
        clock: 0, // M8-10：写入时由 store 注入类型时钟
        keyword: keyword.unwrap_or_default(),
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
    // M23-4：IGNORE_PATTERNS——命中路径静默跳过（不追加事件）。
    let cfg = crate::config::load().unwrap_or_default();
    if crate::frecency::ignored(&record.path, &cfg.frecency.ignore_patterns) {
        return Ok(());
    }
    record_impl(&db_path, &pending, &record)
}

/// M30-1：命令文本记录（TUI `:` 执行成功后调用；cmd topic）。
/// IGNORE_PATTERNS 命中命令文本 → 静默跳过（模式由调用方传入——TUI 侧
/// 经 state.config 传递，测试可直接注入，避免宿主配置依赖）；写失败
/// 降级 events.pending（与路径记录同语义）。`line` 为原始命令行。
pub fn record_command_line(
    db_path: &Path,
    pending: &Path,
    line: &str,
    ignore_patterns: &[String],
) -> std::io::Result<()> {
    init_settings(&crate::config::load().unwrap_or_default().frecency);
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    if crate::frecency::ignored(trimmed, ignore_patterns) {
        return Ok(());
    }
    let record = EntryRecord {
        path: trimmed.to_owned(),
        topic: "cmd".to_owned(),
        kind: EntryKind::Command,
        clock: 0,
        keyword: String::new(),
        ts: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };
    record_impl(db_path, pending, &record)
}

/// M30-4：命令书签查找——cmd topic 中 keyword 精确匹配 `tag` 的最高
/// 分条目的命令文本；无命中返回 None。只读（open_store_readonly）。
/// 多字符 keyword 在 `o` 搜索享高优先级由 M23-1 既有 grade_for 保证。
pub fn cmd_bookmark_at(db_path: &Path, tag: &str) -> Option<String> {
    let store = open_store_readonly_at(db_path).ok().flatten()?;
    let snapshot = store.snapshot_all().ok()?;
    let events = store
        .last_compaction()
        .and_then(|s| store.events_since(s))
        .unwrap_or_default();
    let now_dir = store.type_tick("dir").ok()?;
    let now_file = store.type_tick("file").ok()?;
    let now_cmd = store.type_tick("cmd").ok()?;
    replay(&snapshot, &events, now_dir, now_file, now_cmd)
        .into_iter()
        .filter(|e| e.topic == "cmd" && e.keyword == tag)
        .max_by(|a, b| a.score.total_cmp(&b.score))
        .map(|e| e.path)
}

/// M30-4：全局路径形态（TUI `:cmdmark` 消费用）。
pub fn cmd_bookmark(tag: &str) -> Option<String> {
    cmd_bookmark_at(&crate::config::frecency_db_path(), tag)
}

/// M30-3：dir/file 条目分数映射（`:` Tab 补全加权用）——只读
///（open_store_readonly，AUDIT-1 口径：不合并 pending、不写库）；
/// 库缺失 → 空表（调用方零成本字典序兜底）。
pub fn path_scores() -> std::collections::HashMap<(String, String), f64> {
    path_scores_at(&crate::config::frecency_db_path())
}

/// M30-3：注入 DB 路径形态（测试用；语义同 [`path_scores`]）。
pub fn path_scores_at(db_path: &Path) -> std::collections::HashMap<(String, String), f64> {
    let mut map = std::collections::HashMap::new();
    let store = match open_store_readonly_at(db_path) {
        Ok(Some(s)) => s,
        _ => return map,
    };
    let snapshot = store.snapshot_all().unwrap_or_default();
    let events = store
        .last_compaction()
        .and_then(|s| store.events_since(s))
        .unwrap_or_default();
    let now_dir = store.type_tick("dir").unwrap_or(0);
    let now_file = store.type_tick("file").unwrap_or(0);
    let now_cmd = store.type_tick("cmd").unwrap_or(0);
    for e in replay(&snapshot, &events, now_dir, now_file, now_cmd) {
        if e.topic == "dir" || e.topic == "file" {
            map.insert((e.topic, e.path), e.score);
        }
    }
    map
}

/// M30-2：命令历史排序（`:` 模式 ↑/↓ 数据源）——cmd topic replay 顶层
/// `limit` 条（分数降序 = 常用优先；并列按 last_visit 新在前），会话内
/// 未落盘命令并入（列表头部 = 最不常用位）。返回**升序**（最常用在
/// 末尾）——`↑`（hist_prev 首跳 `len-1`）先命中最常用条目（M25-7
/// "常用命令置顶"口径）；store 缺失/为空时仅会话列表（行为退化同
/// M24-6 FIFO）。无 pending 合并（AUDIT-1：合并只在 record/TUI 启动）。
/// M50-1：内部命令补全用的只读分数表（cmd topic，key = 命令名）。
/// M58-5：cmd topic 中已入库的**完整命令行**（只读；补全按整行去重用）。
pub fn cmd_full_lines() -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    let Ok(Some(store)) = open_store_readonly() else {
        return out;
    };
    let snapshot = store.snapshot_all().unwrap_or_default();
    let events = store
        .last_compaction()
        .and_then(|s| store.events_since(s))
        .unwrap_or_default();
    let replayed = replay(
        &snapshot,
        &events,
        store.type_tick("dir").unwrap_or(0),
        store.type_tick("file").unwrap_or(0),
        store.type_tick("cmd").unwrap_or(0),
    );
    for entry in replayed.into_iter().filter(|e| e.topic == "cmd") {
        out.insert(entry.path);
    }
    out
}

pub fn cmd_scores() -> std::collections::HashMap<String, f64> {
    let mut out = std::collections::HashMap::new();
    // 只读打开（AUDIT-1 红线：补全不得写库；也避免并发测试争写锁）。
    let Ok(Some(store)) = open_store_readonly() else {
        return out;
    };
    let snapshot = store.snapshot_all().unwrap_or_default();
    let events = store
        .last_compaction()
        .and_then(|s| store.events_since(s))
        .unwrap_or_default();
    let replayed = replay(
        &snapshot,
        &events,
        store.type_tick("dir").unwrap_or(0),
        store.type_tick("file").unwrap_or(0),
        store.type_tick("cmd").unwrap_or(0),
    );
    for entry in replayed.into_iter().filter(|e| e.topic == "cmd") {
        // 补全排序只取命令名（首 token），参数变化不稀释分数。
        let name = entry
            .path
            .split_whitespace()
            .next()
            .unwrap_or(&entry.path)
            .to_owned();
        out.insert(name, entry.score);
    }
    out
}

pub fn cmd_history_ranked(session: &[String], limit: usize) -> Vec<String> {
    cmd_history_ranked_at(&crate::config::frecency_db_path(), session, limit)
}

/// M30-2：注入 DB 路径形态（测试用；语义同 [`cmd_history_ranked`]）。
pub fn cmd_history_ranked_at(db_path: &Path, session: &[String], limit: usize) -> Vec<String> {
    let mut ranked: Vec<ScoredEntry> = Vec::new();
    if let Ok(store) = Store::open(db_path) {
        let snapshot = store.snapshot_all().unwrap_or_default();
        let events = store
            .last_compaction()
            .and_then(|s| store.events_since(s))
            .unwrap_or_default();
        let now_dir = store.type_tick("dir").unwrap_or(0);
        let now_file = store.type_tick("file").unwrap_or(0);
        let now_cmd = store.type_tick("cmd").unwrap_or(0);
        ranked = replay(&snapshot, &events, now_dir, now_file, now_cmd)
            .into_iter()
            .filter(|e| e.topic == "cmd")
            .collect();
        ranked.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| b.last_visit.cmp(&a.last_visit))
        });
    }
    // 升序输出（最不常用在前、最常用在末尾）——↑ 首跳 len-1 命中最常用。
    let mut out: Vec<String> = ranked.into_iter().take(limit).map(|e| e.path).collect();
    out.reverse();
    // 会话未落盘命令并入头部（保持会话顺序；insert(0) 逐条会逆序，
    // 故先收集再 splice）。
    let missing: Vec<String> = session
        .iter()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty() && !out.iter().any(|s| s == t)
        })
        .map(|l| l.trim().to_owned())
        .collect();
    out.splice(0..0, missing);
    out
}

/// M30-1：record_command_line 的便捷封装（数据目录/降级路径按配置推导；
/// TUI 调用用——写失败静默，不阻塞 UI）。
pub fn record_command_line_now(line: &str, ignore_patterns: &[String]) {
    let db_path = crate::config::frecency_db_path();
    let pending = db_path
        .parent()
        .unwrap_or(Path::new("."))
        .join("events.pending");
    let _ = record_command_line(&db_path, &pending, line, ignore_patterns);
}

/// M50 性能修复：批量记录命令名（shell 历史增量导入用）。单次打开 store、
/// 单次 merge_pending、单次容量清理；失败整批降级 pending，避免 N 次事务。
pub fn record_command_lines_bulk(names: &[String], ignore_patterns: &[String]) -> usize {
    let db_path = crate::config::frecency_db_path();
    let pending = db_path
        .parent()
        .unwrap_or(Path::new("."))
        .join("events.pending");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let records: Vec<EntryRecord> = names
        .iter()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !ignored(line, ignore_patterns))
        .map(|line| EntryRecord {
            path: line.to_owned(),
            topic: "cmd".to_owned(),
            kind: EntryKind::Command,
            clock: 0,
            keyword: String::new(),
            ts: now,
        })
        .collect();
    if records.is_empty() {
        return 0;
    }
    if std::env::var_os("RVIFM_STARTUP_TRACE").is_some() {
        eprintln!("history bulk: opening store, {} records", records.len());
    }
    let Ok(store) = Store::open(&db_path) else {
        let mut written = 0;
        for record in &records {
            if degraded_write_at(&pending, record, "数据库打开失败").is_ok() {
                written += 1;
            }
        }
        return written;
    };
    if std::env::var_os("RVIFM_STARTUP_TRACE").is_some() {
        eprintln!("history bulk: store opened, merging pending");
    }
    if merge_pending(&store, &pending).is_err() {
        return 0;
    }
    if std::env::var_os("RVIFM_STARTUP_TRACE").is_some() {
        eprintln!("history bulk: appending events");
    }
    let mut written = 0;
    for _ in 0..WRITE_RETRIES {
        match store.append_events_batch(&records) {
            Ok(n) => {
                written = n;
                break;
            }
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(WRITE_BACKOFF_MS)),
        }
    }
    if std::env::var_os("RVIFM_STARTUP_TRACE").is_some() {
        eprintln!("history bulk: appended, cleanup capacity");
    }
    let _ = cleanup_capacity(&store, "cmd");
    if std::env::var_os("RVIFM_STARTUP_TRACE").is_some() {
        eprintln!("history bulk: done");
    }
    written
}

/// M16 NAV-1：设置/清除书签 keyword（可注入路径，测试用）。
/// `keyword` 为空 = 清除书签（保留累积分数正常衰减，SPEC §4.6）。
pub fn set_keyword_at(
    db_path: &Path,
    pending: &Path,
    path: &str,
    keyword: &str,
) -> std::io::Result<()> {
    set_keyword_topic_at(db_path, pending, path, "dir", keyword)
}

/// M25-2：topic 感知形态——marks 支持 file 条目（dir/file 各自评分链）。
/// `keyword` 为空 = 清除书签（保留累积分数正常衰减，SPEC §4.6）。
pub fn set_keyword_topic_at(
    db_path: &Path,
    pending: &Path,
    path: &str,
    topic: &str,
    keyword: &str,
) -> std::io::Result<()> {
    let record = EntryRecord {
        path: path.replace('\\', "/"),
        topic: topic.to_owned(),
        kind: EntryKind::Keyword,
        clock: 0,
        keyword: keyword.trim().to_owned(),
        ts: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };
    record_impl(db_path, pending, &record)
}

/// M16 NAV-1：按平台默认存储路径设置 keyword（TUI `:keyword` 入口）。
pub fn set_keyword_now(path: &str, keyword: &str) -> std::io::Result<()> {
    set_keyword_topic_now(path, "dir", keyword)
}

/// M25-2：全局路径 + topic 形态（`m<char>` / `:bmark` TUI 入口）。
pub fn set_keyword_topic_now(path: &str, topic: &str, keyword: &str) -> std::io::Result<()> {
    let db_path = crate::config::frecency_db_path();
    let pending = db_path
        .parent()
        .unwrap_or(Path::new("."))
        .join("events.pending");
    set_keyword_topic_at(&db_path, &pending, path, topic, keyword)
}

/// M8 收尾（GAP-2）：TUI 退出时批量追加会话访问事件——与 record 同链
/// （Store 短事务 + 重试；打开失败整批降级到 events.pending）。返回写入数。
pub fn flush_visits_at(
    db_path: &Path,
    pending: &Path,
    records: &[EntryRecord],
) -> std::io::Result<usize> {
    if records.is_empty() {
        return Ok(0);
    }
    let store = match Store::open(db_path) {
        Ok(s) => s,
        Err(e) => {
            // 整批降级：逐条追加到 pending（record 同款降级文件）。
            for record in records {
                let _ = degraded_write_at(pending, record, &e);
            }
            return Ok(0);
        }
    };
    merge_pending(&store, pending)?;
    let mut written = 0usize;
    for record in records {
        let mut ok = false;
        for _ in 0..WRITE_RETRIES {
            if store.append_event(record).is_ok() {
                ok = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(WRITE_BACKOFF_MS));
        }
        if ok {
            written += 1;
        } else {
            let _ = degraded_write_at(pending, record, "数据库写入连续失败");
        }
    }
    Ok(written)
}

/// M8 收尾（GAP-2）：按平台默认存储路径刷写会话访问。
pub fn flush_visits(records: &[EntryRecord]) -> std::io::Result<usize> {
    let db_path = crate::config::frecency_db_path();
    let pending = db_path
        .parent()
        .unwrap_or(Path::new("."))
        .join("events.pending");
    flush_visits_at(&db_path, &pending, records)
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
            Ok(_) => {
                // M23-3：容量清理（dir/file topic；写事务末尾检查）。
                if matches!(
                    record.kind,
                    EntryKind::Navigate | EntryKind::Open | EntryKind::Command
                ) {
                    let _ = cleanup_capacity(&store, &record.topic);
                }
                return Ok(()); // 成功静默（不污染 stdout）
            }
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(WRITE_BACKOFF_MS)),
        }
    }
    degraded_write_at(pending, record, "数据库写入连续失败")
}

/// M23-3：per-topic 容量上限（M23-5 起可配置；默认与参考项目一致）。
pub const MAX_ENTRIES_DEFAULT: usize = 1500;

/// M23-8：删除单条 frecency 条目（Remove 墓碑事件持久化）。
pub fn remove_entry(topic: &str, path: &str) -> std::io::Result<()> {
    let db_path = crate::config::frecency_db_path();
    let pending = db_path
        .parent()
        .unwrap_or(Path::new("."))
        .join("events.pending");
    let store = match Store::open(&db_path) {
        Ok(s) => s,
        Err(e) => return degraded_write_at(&pending, &tombstone_record(topic, path), e),
    };
    merge_pending(&store, &pending)?;
    let tomb = tombstone_record(topic, path);
    for _ in 0..WRITE_RETRIES {
        match store.append_event(&tomb) {
            Ok(_) => return Ok(()),
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(WRITE_BACKOFF_MS)),
        }
    }
    degraded_write_at(&pending, &tomb, "数据库写入连续失败")
}

fn tombstone_record(topic: &str, path: &str) -> EntryRecord {
    EntryRecord {
        path: path.replace('\\', "/").to_string(),
        topic: topic.to_owned(),
        kind: EntryKind::Remove,
        clock: 0,
        keyword: String::new(),
        ts: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    }
}
/// M49-15：恢复被 `dd` 删除的 frecency 记录——追加一条 Navigate 事件，
/// 重放时重建该条目（与 M23-8 原承诺的 `u` 恢复对齐；不写归档内容）。
pub fn restore_entry(topic: &str, path: &str) -> std::io::Result<()> {
    let record = EntryRecord {
        path: path.replace('\\', "/").to_string(),
        topic: topic.to_owned(),
        kind: EntryKind::Navigate,
        clock: 0,
        keyword: String::new(),
        ts: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };
    // 只写 events.pending（不打开 redb）——避免与并发测试/记录事务争锁；
    // 下次 record/TUI 启动按既有降级合并链写回三表（AUDIT-1 口径不变）。
    degraded_write_at(&pending_path(), &record, "frecency 记录恢复")
}

/// M23-4：IGNORE_PATTERNS——路径命中任一 glob 模式则不记录（静默跳过）。
/// glob `*` 跨 `/`（tab::glob_match 语义），`*/.git` 命中任意深度的 .git。
pub fn ignored(path: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|p| crate::tab::glob_match(p, path))
}

#[cfg(test)]
mod ignore_tests {
    use super::*;

    #[test]
    fn m23_4_ignored_glob_semantics() {
        // M23-4：glob `*` 跨 `/`——`*/.git` 命中任意深度的 .git；
        // `*/node_modules/*` 命中子文件；未命中路径不拦。
        let pats = vec!["*/.git".to_owned(), "*/node_modules/*".to_owned()];
        assert!(ignored("/home/u/.git", &pats));
        assert!(ignored("/a/b/c/.git", &pats));
        assert!(ignored("/p/node_modules/x.js", &pats));
        assert!(!ignored("/home/u/src", &pats));
        assert!(!ignored("/p/node_modules", &pats), "尾段模式需子路径");
    }
}

/// M23-3：容量清理——条目数超过 `MAX_ENTRIES_DEFAULT` 时删除最低分的
/// **未书签**条目至 90%（带 keyword 条目永不清理）。返回删除数。
pub fn cleanup_capacity(store: &Store, topic: &str) -> std::io::Result<usize> {
    cleanup_capacity_with(store, topic, settings().max_entries)
}

/// M23-3：可注入上限的清理本体（测试用小上限加速）。
pub fn cleanup_capacity_with(
    store: &Store,
    topic: &str,
    max_entries: usize,
) -> std::io::Result<usize> {
    let snapshot = store.snapshot_all()?;
    let since = store.last_compaction()?;
    let events = store.events_since(since)?;
    let dir_tick = store.type_tick("dir")?;
    let file_tick = store.type_tick("file")?;
    let cmd_tick = store.type_tick("cmd")?;
    let entries = replay(&snapshot, &events, dir_tick, file_tick, cmd_tick);
    let topic_entries: Vec<&ScoredEntry> = entries.iter().filter(|e| e.topic == topic).collect();
    if topic_entries.len() <= max_entries {
        return Ok(0);
    }
    let target = max_entries * 90 / 100;
    let mut victims: Vec<&ScoredEntry> = topic_entries
        .iter()
        .copied()
        .filter(|e| e.keyword.is_empty())
        .collect();
    victims.sort_by(|a, b| a.score.total_cmp(&b.score));
    let mut removed = 0usize;
    for v in victims.into_iter().take(topic_entries.len() - target) {
        let tomb = EntryRecord {
            path: v.path.clone(),
            topic: topic.to_owned(),
            kind: EntryKind::Remove,
            clock: 0,
            keyword: String::new(),
            ts: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };
        store.append_event(&tomb)?;
        removed += 1;
    }
    Ok(removed)
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
    // 维护写命令：读写打开（负责建表）+ 先合并降级 pending，再压实。
    let store = Store::open(&crate::config::frecency_db_path())?;
    merge_pending(&store, &pending_path())?;
    compact_store(&store).map(|_| ())
}

/// compaction 核心（AUDIT-4 自 `cmd_compact` 抽出，手动命令与 TUI 启动
/// 钩子共用）：合并 `last_compaction` 之后的事件进 snapshot、推进标记并
/// 裁剪。返回合并条数（0 = 无新事件 no-op）。
pub(crate) fn compact_store(store: &Store) -> std::io::Result<usize> {
    let since = store.last_compaction()?;
    let now = store.global_tick()?;
    if now <= since {
        return Ok(0); // 无新事件：no-op
    }
    let events = store.events_since(since)?;
    let snapshot = store.snapshot_all()?;
    let merged = replay_merge(&snapshot, &events, since);
    for entry in &merged {
        store.snapshot_upsert(entry)?;
    }
    store.set_last_compaction(now)?;
    store.prune_events_le(now)?;
    Ok(merged.len())
}

/// AUDIT-4：TUI 启动惰性 compaction 阈值（events 自上次压实起的增量；
/// 2026-11-19 定稿 5000 条，见 SPEC §4.6）。
pub const COMPACTION_THRESHOLD: u64 = 5000;

/// AUDIT-4：TUI 启动惰性 compaction——events 超阈值时写 snapshot + 截断；
/// `pick` / 跳转选择器不触发（只读红线）。打开失败静默（下次启动再试）。
/// 调用顺序：在 `merge_pending_startup` 之后（pending 先落 events 再压实）。
pub fn lazy_compact_on_startup() {
    let _ = lazy_compact_at(&crate::config::frecency_db_path(), COMPACTION_THRESHOLD);
}

/// 注入路径形态（测试用）；语义同 [`lazy_compact_on_startup`]。
/// 返回合并条数（0 = 阈值内 no-op）。
fn lazy_compact_at(db_path: &Path, threshold: u64) -> std::io::Result<usize> {
    let store = Store::open(db_path)?;
    let since = store.last_compaction()?;
    let now = store.global_tick()?;
    if now.saturating_sub(since) <= threshold {
        return Ok(0);
    }
    compact_store(&store)
}

/// compaction 合并（与 `replay` 同规则，但返回更新后的 snapshot 表）。
fn replay_merge(
    snapshot: &[SnapshotEntry],
    events: &[(u64, EntryRecord)],
    _since: u64,
) -> Vec<SnapshotEntry> {
    // AUDIT-16：per-topic λ（与 replay_inner 同语义——分型时钟按事件
    // topic 取 λ；不得硬编码 AGING_LAMBDA，否则配置 per-topic λ 后
    // compaction 前后评分漂移、排序不一致）。
    let st = settings();
    let lambda_for_topic = |topic: &str| {
        if topic == "file" {
            st.lambda_file
        } else if topic == "cmd" {
            st.lambda_cmd
        } else {
            st.lambda_dir
        }
    };
    let mut map: std::collections::BTreeMap<(String, String), SnapshotEntry> =
        std::collections::BTreeMap::new();
    for s in snapshot {
        map.insert((s.topic.clone(), s.path.clone()), s.clone());
    }
    // M16 NAV-4：每条目最后访问的墙钟时间（事件 ts；snapshot-only 为 0）。
    let mut last_visit: std::collections::BTreeMap<(String, String), u64> =
        std::collections::BTreeMap::new();
    for (seq, ev) in events {
        let lv = last_visit
            .entry((ev.topic.clone(), ev.path.clone()))
            .or_default();
        *lv = (*lv).max(ev.ts);
        let clock = if ev.clock == 0 { *seq } else { ev.clock };
        let entry = map
            .entry((ev.topic.clone(), ev.path.clone()))
            .or_insert_with(|| SnapshotEntry {
                path: ev.path.clone(),
                topic: ev.topic.clone(),
                keyword: String::new(),
                score: 0.0,
                tick: *seq,
                clock,
            });
        let delta = clock.saturating_sub(entry.clock);
        entry.score = aged(entry.score, lambda_for_topic(&ev.topic), delta) + 1.0;
        entry.tick = *seq;
        entry.clock = clock;
    }
    map.into_values().collect()
}

/// AUDIT-15：`pick` 参数解析——`--topic dir|file`（与 `record` 对称，
/// 缺省 `dir`）+ 可选位置查询词；多余位置参数报错。
/// M38-2：fzf 兼容参数集（常用子集；未知参数可读错误）。
#[derive(Debug, Default)]
pub struct PickOptions {
    pub topic: String,
    pub query: String,
    pub multi: bool,
    pub print0: bool,
    pub no_sort: bool,
    pub exit_0: bool,
    pub select_1: bool,
    /// --preview / --height：接受并忽略（预览由交互 UI 自带；高度自适应）。
    pub accepted_ignored: Vec<String>,
}

fn parse_pick_args(args: &[String]) -> std::io::Result<(String, String)> {
    parse_pick_options(args).map(|o| (o.topic, o.query))
}

/// M38-2：完整参数解析（fzf 兼容子集）。
pub fn parse_pick_options(args: &[String]) -> std::io::Result<PickOptions> {
    let mut o = PickOptions {
        topic: "dir".to_owned(),
        ..Default::default()
    };
    let mut it = args.iter();
    let mut query_set = false;
    while let Some(a) = it.next() {
        match a.as_str() {
            "--topic" => {
                o.topic = it
                    .next()
                    .ok_or_else(|| arg_err("pick: --topic 缺少值"))?
                    .clone();
            }
            "--query" => {
                o.query = it
                    .next()
                    .ok_or_else(|| arg_err("pick: --query 缺少值"))?
                    .clone();
                query_set = true;
            }
            "--multi" => o.multi = true,
            "--print0" => o.print0 = true,
            "--no-sort" => o.no_sort = true,
            "--exit-0" => o.exit_0 = true,
            "--select-1" => o.select_1 = true,
            "--preview" | "--height" => {
                // 接受 + 忽略值（预览内置；高度自适应）。
                let _ = it.next();
                o.accepted_ignored.push(a.clone());
            }
            other => {
                if query_set {
                    return Err(arg_err("pick: 多余的位置参数"));
                }
                o.query = other.to_owned();
                query_set = true;
            }
        }
    }
    Ok(o)
}

/// AUDIT-15：只读加载评分表——库缺失（数据目录不存在）→ 空表
/// （全 0 分降级），不裸 `NotFound` 失败。`pick` 纯只读（AUDIT-1）。
fn pick_ranked_entries_at(db_path: &Path) -> std::io::Result<Vec<ScoredEntry>> {
    match open_store_readonly_at(db_path)? {
        Some(store) => {
            let snapshot = store.snapshot_all()?;
            let since = store.last_compaction()?;
            // M8-10：读方分别取类型时钟（dir/file/cmd 各自独立 aging）。
            let now_dir = store.type_tick("dir")?;
            let now_file = store.type_tick("file")?;
            let now_cmd = store.type_tick("cmd")?;
            let events = store.events_since(since)?;
            Ok(replay(&snapshot, &events, now_dir, now_file, now_cmd))
        }
        None => Ok(Vec::new()),
    }
}

/// AUDIT-15：候选评分（纯函数，测试用）——每行候选按 `topic` 查分，
/// 无对应条目 → 0.0，**不得以其他 topic 兜底**。
fn pick_score_candidates(
    ranked: &[ScoredEntry],
    candidates: &str,
    topic: &str,
) -> Vec<(String, String, f64)> {
    let score_of = |path: &str, topic: &str| -> f64 {
        ranked
            .iter()
            .find(|e| e.path == path && e.topic == topic)
            .map(|e| e.score)
            .unwrap_or(0.0)
    };
    candidates
        .lines()
        .filter_map(|line| {
            let path = line.trim().replace('\\', "/");
            if path.is_empty() {
                None
            } else {
                Some((path.clone(), topic.to_owned(), score_of(&path, topic)))
            }
        })
        .collect()
}

/// `rvifm pick [--topic dir|file] [query]`（M8-6；AUDIT-15 补 --topic）：
/// stdin 读候选（每行一个路径）。
/// stdout 为 TTY → 交互式选择器（ratatui 自绘，Esc/Ctrl-C 取消退出码 1）；
/// stdout 非 TTY → fzf filter 语义（输出全部命中候选）。
/// **纯只读：不写事件、不推进 tick。**
pub fn cmd_pick(args: Vec<String>) -> std::io::Result<()> {
    // AUDIT-16：加载 [frecency] 设置（λ/容量上限）——与 record/TUI 一致，
    // 否则 pick 的评分衰减永远用缺省 λ，与 TUI 排序漂移。
    init_settings(&crate::config::load().unwrap_or_default().frecency);
    let opts = parse_pick_options(&args)?;
    let topic = opts.topic.clone();
    let initial_query = opts.query.clone();
    use std::io::{IsTerminal, Read as _, Write as _};
    let mut candidates = String::new();
    std::io::stdin().read_to_string(&mut candidates)?;

    let ranked_entries = pick_ranked_entries_at(&crate::config::frecency_db_path())?;
    let out = pick_score_candidates(&ranked_entries, &candidates, &topic);

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
        if !opts.no_sort {
            hit.sort_by(|a, b| b.2.total_cmp(&a.2).then(a.0.cmp(&b.0)));
        }
        if hit.is_empty() {
            // --exit-0：空匹配退出 0（fzf 语义）。
            if opts.exit_0 {
                return Ok(());
            }
            std::process::exit(1);
        }
        // --select-1：唯一命中直接输出（fzf -1 语义）。
        if opts.select_1 && hit.len() == 1 {
            println!("{}", hit[0].0);
            return Ok(());
        }
        let stdout = std::io::stdout();
        let mut w = std::io::BufWriter::new(stdout.lock());
        // filter 语义：输出全部命中（fzf filter 同款）；--print0 NUL 分隔。
        // --multi 仅交互 UI 多选有意义（交互 UI 侧）。
        for (path, _, _) in hit.into_iter() {
            if opts.print0 {
                write!(w, "{path}\0")?;
            } else {
                writeln!(w, "{path}")?;
            }
        }
        return Ok(());
    }

    pick_interactive(&out, &initial_query)
}

/// `rvifm cmd [query]`（M58-8）：从 shell 历史中选择完整命令行（fzf Ctrl+R 同款）。
/// **纯只读**：不写 redb/events.pending、不更新 global_tick；与 `pick` 共用交互链。
pub fn cmd_cmd(args: Vec<String>) -> std::io::Result<()> {
    use std::io::{IsTerminal, Write as _};
    let query = args
        .into_iter()
        .find(|a| !a.starts_with('-'))
        .unwrap_or_default();
    let scores = cmd_scores();
    let mut candidates: Vec<(String, String, f64)> = crate::shell_history::read_all_commands()
        .into_iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let name = line.split_whitespace().next().unwrap_or("").to_owned();
            (
                line,
                "cmd".to_owned(),
                scores.get(&name).copied().unwrap_or(0.0),
            )
        })
        .collect();
    candidates.sort_by(|a, b| b.2.total_cmp(&a.2).then(a.0.cmp(&b.0)));
    if !std::io::stdout().is_terminal() {
        let hits: Vec<_> = candidates
            .into_iter()
            .filter(|(line, _, _)| query.is_empty() || ci_contains(line, &query))
            .collect();
        if hits.is_empty() {
            std::process::exit(1);
        }
        let stdout = std::io::stdout();
        let mut w = std::io::BufWriter::new(stdout.lock());
        for (line, _, _) in hits {
            writeln!(w, "{line}")?;
        }
        return Ok(());
    }
    pick_interactive(&candidates, &query)
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
    // AUDIT-3：预览与 TUI 同源——图像经 ImageEngine（与 TUI 预览一致）。
    let mut image_engine = crate::image::ImageEngine::detect();
    let outcome = loop {
        let hits = matches(&query, candidates);
        if cursor >= hits.len() {
            cursor = hits.len().saturating_sub(1);
        }
        let draw_result = terminal.draw(|frame| {
            draw_pick_ui(frame, &query, cursor, &hits, &mut scroll, &mut image_engine);
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

/// AUDIT-3：pick 选择器 UI——查询行 + 候选列表 + 右侧预览窗格。
/// 预览与 TUI **同源**（SPEC §4.9）：文本/目录/Office/PDF 文本走共享
/// `render_preview`，图像走 `ImageEngine`；同一进程内完成，不 fork/exec
/// 外部命令。宽终端（≥60 列）双栏，窄终端降级单栏（预览隐藏）。
fn draw_pick_ui(
    frame: &mut ratatui::Frame,
    query: &str,
    cursor: usize,
    hits: &[(String, String, f64)],
    scroll: &mut usize,
    image_engine: &mut crate::image::ImageEngine,
) {
    use ratatui::layout::Constraint;
    let [query_area, body] =
        ratatui::layout::Layout::vertical([Constraint::Length(1), Constraint::Min(0)])
            .areas(frame.area());
    frame.render_widget(Paragraph::new(format!("/{query}")), query_area);
    let (list_area, preview_area) = if body.width >= 60 {
        let [l, p] = ratatui::layout::Layout::horizontal([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .areas(body);
        (l, p)
    } else {
        (body, ratatui::layout::Rect::default())
    };
    let items: Vec<ListItem> = hits
        .iter()
        .map(|(path, _, _)| ListItem::new(Line::from(path.clone())))
        .collect();
    let mut ls = ListState::default()
        .with_selected(Some(cursor))
        .with_offset(*scroll);
    frame.render_stateful_widget(
        List::new(items).highlight_style(
            ratatui::style::Style::new().add_modifier(ratatui::style::Modifier::REVERSED),
        ),
        list_area,
        &mut ls,
    );
    *scroll = ls.offset();

    if preview_area.width == 0 || preview_area.height == 0 {
        return;
    }
    let pane = ratatui::widgets::Block::bordered().title("预览");
    let inner = pane.inner(preview_area);
    frame.render_widget(pane, preview_area);
    let Some((path, _, _)) = hits.get(cursor) else {
        frame.render_widget(Paragraph::new("（无候选）"), inner);
        return;
    };
    let p = std::path::Path::new(path);
    if crate::preview::preview_kind(p) == crate::preview::PreviewKind::Image {
        // 图像与 TUI 同源：ImageEngine 协议探测 + halfblocks 回退。
        if image_engine.render_file(frame, inner, p).is_err() {
            frame.render_widget(Paragraph::new("图片渲染失败（损坏/超大）"), inner);
        }
        return;
    }
    // M59-1：文本预览与 TUI 查看器共用 `highlight::styled_lines`（同源高亮）。
    let text = crate::preview::read_text_preview(p, inner.height.max(1) as usize);
    let lines: Vec<String> = text
        .lines()
        .take(inner.height.max(1) as usize)
        .map(str::to_owned)
        .collect();
    let dark = crate::theme::active_variant() != opaline::schema::ThemeVariant::Light;
    if let Some(styled) = crate::highlight::styled_lines(p, &lines, dark) {
        frame.render_widget(ratatui::widgets::Paragraph::new(styled), inner);
    } else {
        let preview = crate::preview::render_preview(p, inner.height.max(1) as usize);
        frame.render_widget(preview.into_paragraph(), inner);
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
            clock: 100,
        }
    }

    fn rec(path: &str, ts: u64) -> EntryRecord {
        EntryRecord {
            path: path.to_owned(),
            topic: "dir".to_owned(),
            kind: EntryKind::Navigate,
            ts,
            clock: 0, // 0 = 旧格式兜底（replay 以全局序号回退）
            keyword: String::new(),
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
        // M75-2：删除 BookmarkOnly 兜底——未命中查询的书签不再混入结果。
        assert_eq!(
            grade_for(&snap("/w/proj", "proj", 10.0), "zzz"),
            Grade::None
        );
        assert_eq!(grade_for(&snap("/w/proj", "proj", 10.0), ""), Grade::None);
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

        let now = tick; // 读取时 aging：旧格式 clock=0 兜底全局序号，两类 now 相同
        let ranked = replay(&[], &events, now, now, now);
        let a = ranked.iter().find(|e| e.path == "/a").unwrap();
        let b = ranked.iter().find(|e| e.path == "/b").unwrap();
        assert!(b.score > a.score, "B({}) 应排在 A({}) 前", b.score, a.score);
        // 排名：B 在 A 之前。
        let b_pos = ranked.iter().position(|e| e.path == "/b").unwrap();
        let a_pos = ranked.iter().position(|e| e.path == "/a").unwrap();
        assert!(b_pos < a_pos, "B({b_pos}) 应在 A({a_pos}) 前");
    }

    #[test]
    fn replay_records_last_visit_from_event_ts() {
        // M16 NAV-4：last_visit = 该条目最近事件的 ts（snapshot-only 为 0）。
        let events: Vec<(u64, EntryRecord)> = vec![
            (1, rec("/a", 100)),
            (2, rec("/b", 200)),
            (3, rec("/a", 300)),
        ];
        let entries = replay_inner(&[], &events, ReplayClocks::Unified(10));
        let a = entries.iter().find(|e| e.path == "/a").unwrap();
        let b = entries.iter().find(|e| e.path == "/b").unwrap();
        assert_eq!(a.last_visit, 300, "取最近事件 ts");
        assert_eq!(b.last_visit, 200);
    }

    #[test]
    fn replay_keyword_event_applies_metadata_without_score() {
        // M16 NAV-1：keyword 事件只应用 keyword——不加分、不推进时钟。
        let snap = vec![];
        let events: Vec<(u64, EntryRecord)> = vec![
            (1, rec("/a", 100)),
            (
                2,
                EntryRecord {
                    path: "/a".into(),
                    topic: "dir".into(),
                    kind: EntryKind::Keyword,
                    ts: 100,
                    clock: 2,
                    keyword: "dl".into(),
                },
            ),
            (3, rec("/b", 100)),
        ];
        let entries = replay_inner(&snap, &events, ReplayClocks::Unified(10));
        let a = entries.iter().find(|e| e.path == "/a").unwrap();
        assert_eq!(a.keyword, "dl", "keyword 已应用");

        // 隔离断言：同一访问序列 ± keyword 事件，分数必须完全一致
        // （keyword 事件既不加分也不影响 aging 基线）。
        let without_kw: Vec<(u64, EntryRecord)> = vec![(1, rec("/a", 100)), (3, rec("/b", 100))];
        let base = replay_inner(&snap, &without_kw, ReplayClocks::Unified(10));
        let base_a = base.iter().find(|e| e.path == "/a").unwrap();
        assert!(
            (a.score - base_a.score).abs() < 1e-12,
            "keyword 事件不应改变分数: a={} base={}",
            a.score,
            base_a.score
        );
        // 清除：空 keyword 事件覆盖。
        let events_clear: Vec<(u64, EntryRecord)> = vec![
            (1, rec("/a", 100)),
            (
                2,
                EntryRecord {
                    path: "/a".into(),
                    topic: "dir".into(),
                    kind: EntryKind::Keyword,
                    ts: 100,
                    clock: 2,
                    keyword: String::new(),
                },
            ),
        ];
        let entries2 = replay_inner(&snap, &events_clear, ReplayClocks::Unified(10));
        let a2 = entries2.iter().find(|e| e.path == "/a").unwrap();
        assert!(a2.keyword.is_empty(), "空 keyword = 清除书签");
    }

    #[test]
    fn replay_per_type_clocks_are_independent() {
        // M8-10：file 条目的 aging 只受 file 时钟影响——dir 时钟推进
        // （目录高频访问）不稀释文件条目的新鲜度。
        let file_ev = |clock: u64| EntryRecord {
            path: "/f/notes.txt".to_owned(),
            topic: "file".to_owned(),
            kind: EntryKind::Open,
            ts: 1000,
            clock,
            keyword: String::new(),
        };
        let events = [(1u64, file_ev(1))]; // file 时钟 1 时打开一次
        // 场景 1：dir/file 时钟都停在 2（近期）。
        let r1 = replay(&[], &events, 2, 2, 2);
        // 场景 2：dir 时钟推进到 1000（目录重度使用），file 时钟仍 2。
        let r2 = replay(&[], &events, 1000, 2, 2);
        let s1 = r1.iter().find(|e| e.path == "/f/notes.txt").unwrap();
        let s2 = r2.iter().find(|e| e.path == "/f/notes.txt").unwrap();
        assert!(
            (s1.score - s2.score).abs() < 1e-9,
            "dir 时钟推进不得影响 file 条目分数：{} vs {}",
            s1.score,
            s2.score
        );
        // 反向对照：file 时钟真实推进确实衰减（排除假阳性）。
        let r3 = replay(&[], &events, 2, 1000, 1000);
        let s3 = r3.iter().find(|e| e.path == "/f/notes.txt").unwrap();
        assert!(s3.score < s1.score, "file 时钟推进应衰减 file 分数");
    }

    /// M23-5 测试互斥：SETTINGS 为进程级 OnceLock，并行测试会互相覆盖。
    static SETTINGS_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn m304_cmd_bookmark_set_lookup_unbook() {
        // M30-4：命令书签——keyword 事件（cmd topic）设置/查找/清除
        //（unbook 保留历史）；无命中返回 None。
        let _g = SETTINGS_TEST_LOCK.lock().unwrap();
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("f.redb");
        let pending = tmp.path().join("events.pending");
        // 先记录命令（Command 事件）再设书签。
        record_command_line(&db, &pending, "sort name desc", &[]).unwrap();
        set_keyword_topic_at(&db, &pending, "sort name desc", "cmd", "sd").unwrap();
        assert_eq!(
            cmd_bookmark_at(&db, "sd").as_deref(),
            Some("sort name desc"),
            "书签命中返回命令文本"
        );
        assert_eq!(cmd_bookmark_at(&db, "zz"), None, "未命中 None");
        // 清除（空 keyword 事件）→ 查找 None，历史（Command 事件）保留。
        set_keyword_topic_at(&db, &pending, "sort name desc", "cmd", "").unwrap();
        assert_eq!(cmd_bookmark_at(&db, "sd"), None, "清除后无命中");
        let store = Store::open(&db).unwrap();
        let cmds: Vec<_> = store
            .events_since(0)
            .unwrap()
            .into_iter()
            .filter(|(_, r)| matches!(r.kind, EntryKind::Command))
            .collect();
        assert_eq!(cmds.len(), 1, "unbook 保留 Command 历史");
        assert_eq!(cmds[0].1.path, "sort name desc");
    }

    #[test]
    fn m302_cmd_history_ranked_order_and_merge() {
        // M30-2：ranked 排序（分数升序输出、最常用在末尾——↑ 首跳命中）
        // + 会话未落盘命令并入头部 + 空 store 退化 FIFO + limit 截断。
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("f.redb");
        let store = Store::open(&db).unwrap();
        // 三个命令：高频 / 中频 / 低频（各 3/2/1 次）。
        for _ in 0..3 {
            store
                .append_event(&EntryRecord {
                    path: "hot".to_owned(),
                    topic: "cmd".to_owned(),
                    kind: EntryKind::Command,
                    clock: 0,
                    keyword: String::new(),
                    ts: 0,
                })
                .unwrap();
        }
        for _ in 0..2 {
            store
                .append_event(&EntryRecord {
                    path: "warm".to_owned(),
                    topic: "cmd".to_owned(),
                    kind: EntryKind::Command,
                    clock: 0,
                    keyword: String::new(),
                    ts: 0,
                })
                .unwrap();
        }
        store
            .append_event(&EntryRecord {
                path: "cold".to_owned(),
                topic: "cmd".to_owned(),
                kind: EntryKind::Command,
                clock: 0,
                keyword: String::new(),
                ts: 0,
            })
            .unwrap();
        drop(store); // redb 文件锁：重开前释放句柄
        let ranked = cmd_history_ranked_at(&db, &["session-only".to_owned()], 100);
        assert_eq!(
            ranked.last().map(|s| s.as_str()),
            Some("hot"),
            "最常用在末尾（↑ 首跳）"
        );
        assert!(ranked.contains(&"warm".to_owned()));
        assert!(ranked.contains(&"cold".to_owned()));
        assert_eq!(
            ranked.first().map(|s| s.as_str()),
            Some("session-only"),
            "会话并入头部"
        );
        // limit 截断：保留最常用 top-2（升序输出，hot 在末尾）。
        let top2 = cmd_history_ranked_at(&db, &[], 2);
        assert_eq!(top2, vec!["warm".to_owned(), "hot".to_owned()]);
        // 空 store + 会话 → FIFO（↑ 首跳最新会话命令）。
        let empty_db = tmp.path().join("empty.redb");
        let fifo = cmd_history_ranked_at(&empty_db, &["a".to_owned(), "b".to_owned()], 100);
        assert_eq!(fifo, vec!["a".to_owned(), "b".to_owned()]);
    }

    #[test]
    fn m301_cmd_clock_independent_from_dir_file() {
        // M30-1：cmd 类型时钟独立推进——cmd 事件不推进 dir/file 时钟，
        // dir/file 事件不推进 cmd 时钟。
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("f.redb");
        let store = Store::open(&db).unwrap();
        store
            .append_event(&EntryRecord {
                path: "cd /tmp".to_owned(),
                topic: "cmd".to_owned(),
                kind: EntryKind::Command,
                clock: 0,
                keyword: String::new(),
                ts: 0,
            })
            .unwrap();
        assert_eq!(store.type_tick("cmd").unwrap(), 1);
        // 旧库回退语义：缺失时钟读 global_tick（=1）——独立性的判定是
        // 「自有时钟 ≠ 全局时钟」（dir 时钟缺失时回退不可区分）。
        store
            .append_event(&EntryRecord {
                path: "/d".to_owned(),
                topic: "dir".to_owned(),
                kind: EntryKind::Navigate,
                clock: 0,
                keyword: String::new(),
                ts: 0,
            })
            .unwrap();
        // dir 时钟缺失 → 自当前全局（1）分叉为 2。
        assert_eq!(store.type_tick("dir").unwrap(), 2);
        assert_eq!(store.type_tick("cmd").unwrap(), 1);
        store
            .append_event(&EntryRecord {
                path: "q2".to_owned(),
                topic: "cmd".to_owned(),
                kind: EntryKind::Command,
                clock: 0,
                keyword: String::new(),
                ts: 0,
            })
            .unwrap();
        assert_eq!(store.type_tick("cmd").unwrap(), 2, "cmd 自有时钟推进");
        assert_ne!(
            store.type_tick("cmd").unwrap(),
            store.global_tick().unwrap(),
            "cmd 时钟独立于全局时钟"
        );
        assert_ne!(
            store.type_tick("dir").unwrap(),
            store.global_tick().unwrap(),
            "dir 时钟独立于全局时钟"
        );
    }

    #[test]
    fn m301_record_command_line_writes_and_ignores() {
        // M30-1：record_command_line——写入 cmd 事件；IGNORE_PATTERNS 命中
        // 静默跳过；空行跳过。
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("f.redb");
        let pending = tmp.path().join("events.pending");
        let _ = &pending;
        let _g = SETTINGS_TEST_LOCK.lock().unwrap();
        init_settings(&crate::config::FrecencyConfig::default());
        record_command_line(&db, &pending, "sort name", &[]).unwrap();
        let store = Store::open(&db).unwrap();
        let events = store.events_since(0).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].1.topic, "cmd");
        assert!(matches!(events[0].1.kind, EntryKind::Command));
        assert_eq!(events[0].1.path, "sort name");

        // IGNORE_PATTERNS：命令文本命中 → 跳过。
        let db2 = tmp.path().join("f2.redb");
        let pending2 = tmp.path().join("p2.pending");
        init_settings(&crate::config::FrecencyConfig {
            ignore_patterns: vec!["secret*".to_owned()],
            ..Default::default()
        });
        let pats = vec!["secret*".to_owned()];
        record_command_line(&db2, &pending2, "secret-cmd --key", &pats).unwrap();
        record_command_line(&db2, &pending2, "   ", &pats).unwrap();
        record_command_line(&db2, &pending2, "sort name", &pats).unwrap();
        let store2 = Store::open(&db2).unwrap();
        let evs = store2.events_since(0).unwrap();
        assert_eq!(evs.len(), 1, "命中与空行跳过、未命中照常记录");
        assert_eq!(evs[0].1.path, "sort name");
        init_settings(&crate::config::FrecencyConfig::default());
    }

    #[test]
    fn m301_cmd_topic_ages_with_lambda_cmd() {
        // M30-1：cmd 条目衰减使用 lambda_cmd（与 dir/file 档位独立）。
        let _g = SETTINGS_TEST_LOCK.lock().unwrap();
        init_settings(&crate::config::FrecencyConfig {
            lambda_cmd: Some(0.5),
            lambda_dir: None,
            lambda_file: None,
            lambda_unified: None,
            max_entries: None,
            ignore_patterns: Vec::new(),
        });
        let events = [(
            1u64,
            EntryRecord {
                path: "sort name".to_owned(),
                topic: "cmd".to_owned(),
                kind: EntryKind::Command,
                clock: 0,
                keyword: String::new(),
                ts: 0,
            },
        )];
        // λ=0.5、时钟推进 4 → score = 4*exp(-2) ≈ 0.54（若误用 0.01 → ≈3.84）。
        let r = replay(&[], &events, 0, 0, 4);
        let e = r.iter().find(|e| e.topic == "cmd").unwrap();
        assert!(
            e.score < 1.0,
            "lambda_cmd=0.5 应显著衰减: score={}",
            e.score
        );
        init_settings(&crate::config::FrecencyConfig::default());
    }

    #[test]
    fn m301_entry_kind_command_serde_roundtrip_and_compat() {
        // M30-1：serde 新变体——序列化/反序列化往返；旧 JSON（无 Command
        // 变体）反序列化不受影响。
        let rec = EntryRecord {
            path: "q".to_owned(),
            topic: "cmd".to_owned(),
            kind: EntryKind::Command,
            clock: 3,
            keyword: String::new(),
            ts: 9,
        };
        let json = serde_json::to_string(&rec).unwrap();
        assert!(
            json.contains("\"Command\"") || json.contains("Command"),
            "{json}"
        );
        let back: EntryRecord = serde_json::from_str(&json).unwrap();
        assert!(matches!(back.kind, EntryKind::Command));
        // 旧数据（Navigate）照常反序列化。
        let old = r#"{"path":"/d","topic":"dir","kind":"Navigate","clock":1,"keyword":"","ts":5}"#;
        let back_old: EntryRecord = serde_json::from_str(old).unwrap();
        assert!(matches!(back_old.kind, EntryKind::Navigate));
    }

    #[test]
    fn m23_5_per_topic_lambda_dir_decays_faster_than_file() {
        let _g = SETTINGS_TEST_LOCK.lock().unwrap();
        // ACCEPTANCE §22 M23-5：lambda_dir=0.5 → dir 条目衰减显著快于
        // file 条目（同 clock 推进量）；lambda_file 保持默认不互相影响。
        init_settings(&crate::config::FrecencyConfig {
            lambda_dir: Some(0.5),
            lambda_file: Some(0.001),
            lambda_unified: None,
            lambda_cmd: None,
            max_entries: None,
            ignore_patterns: Vec::new(),
        });
        let events = [
            (1u64, rec("/d/x", 1)),
            (2, {
                let mut r = rec("/f/y", 2);
                r.topic = "file".to_owned();
                r
            }),
        ];
        let r = replay(&[], &events, 100, 100, 100);
        let d = r.iter().find(|e| e.topic == "dir").unwrap();
        let fl = r.iter().find(|e| e.topic == "file").unwrap();
        assert!(
            d.score < fl.score,
            "λ=0.5 的 dir 衰减快于 λ=0.001 的 file: dir={} file={}",
            d.score,
            fl.score
        );
        // 恢复默认（后续测试不受污染）。
        init_settings(&crate::config::FrecencyConfig::default());
    }

    #[test]
    fn m23_5_max_entries_respected_by_cleanup() {
        let _g = SETTINGS_TEST_LOCK.lock().unwrap();
        // max_entries=10：15 条 dir 条目 → 清理至 9（90%）。
        init_settings(&crate::config::FrecencyConfig {
            lambda_dir: None,
            lambda_file: None,
            lambda_unified: None,
            lambda_cmd: None,
            max_entries: Some(10),
            ignore_patterns: Vec::new(),
        });
        let tmp = TempDir::new().unwrap();
        let store = Store::open(&tmp.path().join("f.redb")).unwrap();
        for i in 0..15u64 {
            store.append_event(&rec(&format!("/d/{i}"), i + 1)).unwrap();
        }
        let removed = cleanup_capacity(&store, "dir").unwrap();
        assert!(removed > 0, "清理触发: removed={removed}");
        let entries = replay(
            &[],
            &store.events_since(0).unwrap(),
            store.type_tick("dir").unwrap(),
            store.type_tick("file").unwrap(),
            store.type_tick("cmd").unwrap(),
        );
        let dir_n = entries.iter().filter(|e| e.topic == "dir").count();
        assert_eq!(dir_n, 9, "清理至 90%（上限 10）");
        init_settings(&crate::config::FrecencyConfig::default());
    }

    #[test]
    fn m23_2_unbook_preserves_accumulated_score() {
        // ACCEPTANCE §22 M23-2：unbook（`:keyword -`）只清 keyword 元数据，
        // 累积分数/排名不变（对照：带 unbook 与不带的 replay 分数全等）。
        let kw = |ts: u64, kw: &str| EntryRecord {
            path: "/w/proj".to_owned(),
            topic: "dir".to_owned(),
            kind: EntryKind::Keyword,
            ts,
            clock: 0,
            keyword: kw.to_owned(),
        };
        // 带书签生命周期：访问 → 设书签 → 访问 → unbook。
        let events_with = [
            (1u64, rec("/w/proj", 1)),
            (2, kw(2, "proj")),
            (3, rec("/w/proj", 3)),
            (4, kw(4, "")),
        ];
        // 不设书签的同访问序列。
        let events_without = [(1u64, rec("/w/proj", 1)), (3, rec("/w/proj", 3))];
        let r_with = replay(&[], &events_with, 10, 10, 10);
        let r_without = replay(&[], &events_without, 10, 10, 10);
        let e_with = r_with.iter().find(|e| e.path == "/w/proj").unwrap();
        let e_without = r_without.iter().find(|e| e.path == "/w/proj").unwrap();
        assert_eq!(e_with.keyword, "", "unbook 后 keyword 清空");
        assert!(
            (e_with.score - e_without.score).abs() < 1e-9,
            "unbook 保留累积分数: {} vs {}",
            e_with.score,
            e_without.score
        );
    }

    #[test]
    fn store_type_clocks_advance_independently() {
        // M8-10：dir/file 时钟各自推进；global_tick 只作为全局序号。
        let tmp = TempDir::new().unwrap();
        let store = Store::open(&tmp.path().join("f.redb")).unwrap();

        let mut dir_ev = rec("/d1", 1);
        dir_ev.topic = "dir".to_owned();
        let mut file_ev = rec("/f1", 2);
        file_ev.topic = "file".to_owned();
        file_ev.kind = EntryKind::Open;

        store.append_event(&dir_ev).unwrap();
        store.append_event(&dir_ev).unwrap();
        store.append_event(&file_ev).unwrap();

        // 全局序号 3 条全记。
        assert_eq!(store.global_tick().unwrap(), 3);
        // 类型时钟独立：dir 推进 2 次、file 1 次。
        assert_eq!(store.type_tick("dir").unwrap(), 2);
        assert_eq!(store.type_tick("file").unwrap(), 3); // 旧库无 file 时钟：首笔 = 全局序号分叉(2)+1
        // 事件 JSON 携带各自类型时钟。
        let events = store.events_since(0).unwrap();
        assert_eq!(events[0].1.clock, 1); // dir #1
        assert_eq!(events[1].1.clock, 2); // dir #2
        assert_eq!(events[2].1.clock, 3); // file #1（分叉点 2 + 1；此后与 dir 分叉）
    }

    #[test]
    fn m23_3_capacity_cleanup_protects_bookmarks() {
        // ACCEPTANCE §22 M23-3：30 未书签 + 10 书签（共 40 > 上限 30）→ 清理至
        // 27（90%）；书签条目全部存活。小上限经 cleanup_capacity_with 注入。
        let tmp = TempDir::new().unwrap();
        let store = Store::open(&tmp.path().join("f.redb")).unwrap();
        for i in 0..30u64 {
            store.append_event(&rec(&format!("/d/{i}"), i + 1)).unwrap();
        }
        for i in 0..10u64 {
            store
                .append_event(&EntryRecord {
                    path: format!("/b/{i}"),
                    topic: "dir".to_owned(),
                    kind: EntryKind::Keyword,
                    ts: 10000 + i,
                    clock: 0,
                    keyword: format!("kw{i}"),
                })
                .unwrap();
        }
        let removed = cleanup_capacity_with(&store, "dir", 30).unwrap();
        assert!(removed > 0, "触发清理: removed={removed}");

        let dir_tick = store.type_tick("dir").unwrap();
        let file_tick = store.type_tick("file").unwrap();
        let entries = replay(
            &[],
            &store.events_since(0).unwrap(),
            dir_tick,
            file_tick,
            file_tick,
        );
        let dir_entries: Vec<&ScoredEntry> = entries.iter().filter(|e| e.topic == "dir").collect();
        assert!(
            dir_entries.len() <= 30,
            "清理后 ≤ 上限: {}",
            dir_entries.len()
        );
        assert_eq!(dir_entries.len(), 27, "降至 90%（上限 30）");
        let bookmarks = dir_entries.iter().filter(|e| !e.keyword.is_empty()).count();
        assert_eq!(bookmarks, 10, "书签条目全部存活");
        // 被清理的 3 条 = 未书签条目（17 存活 + 3 删除 = 30 2192 配额 27）。
        assert_eq!(dir_entries.len() - bookmarks, 17, "未书签条目被清至配额");
    }

    #[test]
    fn store_type_clocks_initialize_from_global_for_legacy_db() {
        // M8-10 兼容：旧库（只有 global_tick）首次写入时，类型时钟自
        // 全局值连续分叉——新旧 aging 行为衔接。
        let tmp = TempDir::new().unwrap();
        let store = Store::open(&tmp.path().join("f.redb")).unwrap();
        // 模拟旧库已有 5 条事件（旧代码只推进 global）。
        for i in 0..5 {
            let mut ev = rec(&format!("/legacy{i}"), i);
            ev.clock = 0;
            store.append_event(&ev).unwrap();
        }
        assert_eq!(store.global_tick().unwrap(), 5);
        // 旧事件（新代码写入的 clock=0 被覆盖为类型时钟……注意：新代码
        // 无条件注入 clock）——这里验证的是：dir 时钟初始 = global(5)。
        assert_eq!(store.type_tick("dir").unwrap(), 5);
        // 下一笔 dir 事件 → 6。
        let mut ev = rec("/d", 9);
        ev.topic = "dir".to_owned();
        store.append_event(&ev).unwrap();
        assert_eq!(store.type_tick("dir").unwrap(), 6);
        assert_eq!(store.type_tick("file").unwrap(), 6); // file 时钟仍缺失：读取兜底 = 当前 global(6)
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
    fn pick_readonly_path_leaves_pending_and_db_untouched() {
        // AUDIT-1（ACCEPTANCE §26）：存在 events.pending 时，pick 的只读
        // 读取路径不得写库（mtime/global_tick 不变）、不得合并/删除 pending。
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("f.redb");
        let pending = tmp.path().join("events.pending");
        {
            let store = Store::open(&db).unwrap();
            store.append_event(&rec("/existing", 1)).unwrap();
        }
        let pending_line = serde_json::to_string(&rec("/pending/dir", 2)).unwrap();
        std::fs::write(&pending, format!("{pending_line}\n")).unwrap();

        let mtime_before = std::fs::metadata(&db).unwrap().modified().unwrap();
        // 与 cmd_pick 一致的只读加载（open_store_readonly + 同款读序列）。
        let store = open_store_readonly_at(&db).unwrap().unwrap();
        let tick_before = store.global_tick().unwrap();
        let snapshot = store.snapshot_all().unwrap();
        let since = store.last_compaction().unwrap();
        let now_dir = store.type_tick("dir").unwrap();
        let now_file = store.type_tick("file").unwrap();
        let events = store.events_since(since).unwrap();
        let ranked = replay(&snapshot, &events, now_dir, now_file, now_file);
        drop(store);

        assert!(
            ranked.iter().any(|e| e.path == "/existing"),
            "已有条目参与评分"
        );
        assert!(
            !ranked.iter().any(|e| e.path == "/pending/dir"),
            "pending 事件不得被只读路径合并评分"
        );
        assert_eq!(
            std::fs::metadata(&db).unwrap().modified().unwrap(),
            mtime_before,
            "pick 读取路径不得改数据库 mtime"
        );
        // tick 复核：重开只读句柄，meta 未被推进。
        let store = open_store_readonly_at(&db).unwrap().unwrap();
        assert_eq!(store.global_tick().unwrap(), tick_before, "tick 不得推进");
        assert!(pending.exists(), "pending 不得被 pick 合并/删除");
        drop(store); // 释放只读共享锁：RW 重开（redb 互斥）需要句柄先退出

        // TUI 启动钩子（merge_pending_startup 的注入形态）执行后：pending
        // 合并入库、文件删除、tick 推进——合并职责归启动/record 路径。
        merge_pending_at(&db, &pending).unwrap();
        assert!(!pending.exists(), "启动合并后 pending 应删除");
        let store = open_store_readonly_at(&db).unwrap().unwrap();
        assert_eq!(store.global_tick().unwrap(), tick_before + 1);
        assert!(
            store
                .events_since(0)
                .unwrap()
                .iter()
                .any(|(_, r)| r.path == "/pending/dir")
        );
    }

    #[test]
    fn pick_scores_by_requested_topic_without_dir_fallback() {
        // AUDIT-15：候选按 --topic 评分；file 候选不得以 dir 分数兜底。
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("f.redb");
        {
            let store = Store::open(&db).unwrap();
            store.append_event(&rec("/data/proj", 1)).unwrap(); // dir
            let mut f = rec("/data/proj/main.rs", 2);
            f.topic = "file".to_owned();
            f.kind = EntryKind::Open;
            store.append_event(&f).unwrap();
        }
        let ranked = pick_ranked_entries_at(&db).unwrap();
        let candidates = "/data/proj\n/data/proj/main.rs\n";
        let as_dir = pick_score_candidates(&ranked, candidates, "dir");
        let as_file = pick_score_candidates(&ranked, candidates, "file");
        let dir_proj = as_dir.iter().find(|(p, ..)| p == "/data/proj").unwrap().2;
        let dir_main = as_dir
            .iter()
            .find(|(p, ..)| p == "/data/proj/main.rs")
            .unwrap()
            .2;
        let file_proj = as_file.iter().find(|(p, ..)| p == "/data/proj").unwrap().2;
        let file_main = as_file
            .iter()
            .find(|(p, ..)| p == "/data/proj/main.rs")
            .unwrap()
            .2;
        assert!(
            dir_proj > 0.0 && dir_main == 0.0,
            "dir 视角：目录有分、文件 0（{dir_proj}/{dir_main}）"
        );
        assert!(
            file_main > 0.0 && file_proj == 0.0,
            "file 视角：文件有分、目录 0（{file_main}/{file_proj}）"
        );
        assert!(as_dir.iter().all(|(_, t, _)| t == "dir"));
        assert!(as_file.iter().all(|(_, t, _)| t == "file"));
    }

    #[test]
    fn pick_missing_db_yields_empty_scores() {
        // AUDIT-15：数据目录/库缺失 → 空评分表（全 0），不裸 NotFound。
        let tmp = TempDir::new().unwrap();
        let ranked = pick_ranked_entries_at(&tmp.path().join("none.redb")).unwrap();
        assert!(ranked.is_empty());
        let out = pick_score_candidates(&ranked, "/a\n/b\n", "dir");
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|(.., s)| *s == 0.0));
    }

    #[test]
    fn compaction_per_topic_lambda_consistent_with_live_replay() {
        // AUDIT-16：配置差异化 per-topic λ（dir 慢衰减 / file 快衰减）后，
        // 手动 compaction 的合并结果（replay_merge）与实时重放（replay，
        // TUI/pick 同款）对相同 snapshot+events 的评分一致——排序不漂移。
        init_settings(&crate::config::FrecencyConfig {
            lambda_dir: Some(0.001),
            lambda_file: Some(0.5),
            ..Default::default()
        });
        // snapshot：dir 条目（clock 90, score 5）+ file 条目（clock 50, score 8）。
        let snapshot = vec![
            SnapshotEntry {
                path: "/d/alpha".to_owned(),
                topic: "dir".to_owned(),
                keyword: String::new(),
                score: 5.0,
                tick: 90,
                clock: 90,
            },
            SnapshotEntry {
                path: "/f/beta.rs".to_owned(),
                topic: "file".to_owned(),
                keyword: String::new(),
                score: 8.0,
                tick: 50,
                clock: 50,
            },
        ];
        // 事件：dir@100、file@60（与 snapshot 时钟各差 10 tick）。
        let mut ev_dir = rec("/d/alpha", 1_000);
        ev_dir.clock = 100;
        let mut ev_file = rec("/f/beta.rs", 1_100);
        ev_file.topic = "file".to_owned();
        ev_file.kind = EntryKind::Open;
        ev_file.clock = 60;
        let events = vec![(100u64, ev_dir), (60u64, ev_file)];

        let live = replay(&snapshot, &events, 100, 60, 60);
        let merged = replay_merge(&snapshot, &events, 0);
        for m in &merged {
            let l = live
                .iter()
                .find(|e| e.path == m.path && e.topic == m.topic)
                .unwrap_or_else(|| panic!("live 缺条目 {}", m.path));
            assert!(
                (l.score - m.score).abs() < 1e-9,
                "{} live={} merged={}（per-topic λ 不一致）",
                m.path,
                l.score,
                m.score
            );
        }
        // 差异化 λ 生效的 sanity：file 条目经快衰减后分值远低于慢衰减 dir。
        let d = merged.iter().find(|m| m.topic == "dir").unwrap();
        let f = merged.iter().find(|m| m.topic == "file").unwrap();
        assert!(
            d.score > f.score,
            "dir={} file={}（λ 差异应拉开分差）",
            d.score,
            f.score
        );
        // 复位缺省（避免污染并行测试的 SETTINGS）。
        init_settings(&crate::config::FrecencyConfig::default());
    }

    #[test]
    fn lazy_compact_threshold_and_pick_never_triggers() {
        // AUDIT-4：超阈值 → 写 snapshot + 截断 events + 推进游标；
        // 阈值内 no-op（游标不动）。pick/跳转选择器不触发 compaction——
        // 结构性保证：cached_visual_png/cmd_pick 无任何写调用（AUDIT-1
        // 只读 open），compaction 只挂在 TUI 启动钩子与本命令。
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("f.redb");
        {
            let store = Store::open(&db).unwrap();
            for i in 0..6 {
                store.append_event(&rec(&format!("/d{i}"), i)).unwrap();
            }
        }
        // 阈值 5：6 条 → 触发。
        let merged = lazy_compact_at(&db, 5).unwrap();
        assert_eq!(merged, 6, "全部事件合并进 snapshot");
        {
            let ro = Store::open_readonly(&db).unwrap();
            assert!(!ro.snapshot_all().unwrap().is_empty(), "snapshot 已写");
            assert!(ro.events_since(6).unwrap().is_empty(), "≤ 游标事件已裁剪");
            assert_eq!(ro.last_compaction().unwrap(), 6, "游标推进");
        }
        // 阈值内：no-op（游标保持）。
        {
            let store = Store::open(&db).unwrap();
            store.append_event(&rec("/more", 9)).unwrap();
        }
        assert_eq!(lazy_compact_at(&db, 5).unwrap(), 0, "1 ≤ 5 → no-op");
        let ro = Store::open_readonly(&db).unwrap();
        assert_eq!(ro.last_compaction().unwrap(), 6, "游标未动");
    }

    #[test]
    fn pick_args_parse_topic_and_query() {
        // AUDIT-15：--topic 与位置查询解析；缺省 dir；缺值/多参报错。
        let a = |v: &[&str]| {
            parse_pick_args(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap()
        };
        assert_eq!(a(&[]), ("dir".to_owned(), String::new()));
        assert_eq!(a(&["proj"]), ("dir".to_owned(), "proj".to_owned()));
        assert_eq!(a(&["--topic", "file"]), ("file".to_owned(), String::new()));
        assert_eq!(
            a(&["--topic", "file", "proj"]),
            ("file".to_owned(), "proj".to_owned())
        );
        assert_eq!(
            a(&["proj", "--topic", "file"]),
            ("file".to_owned(), "proj".to_owned())
        );
        assert!(
            parse_pick_args(&["--topic".to_owned()]).is_err(),
            "--topic 缺值"
        );
        assert!(
            parse_pick_args(&["a".to_owned(), "b".to_owned()]).is_err(),
            "多余位置参数"
        );
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
            clock: 0,
            ts: 1_700_000_000,
            keyword: String::new(),
        };
        assert_eq!(r.path, "/w/proj");
        assert_eq!(r.kind, EntryKind::Navigate);
        // 克隆语义可用（append-only 队列复制）。
        let c = r.clone();
        assert_eq!(c, r);
    }

    #[test]
    fn pick_ui_preview_same_source_as_tui() {
        // AUDIT-3：pick 预览与 TUI 同源——共享 render_preview 内容直接可见；
        // 宽终端双栏（≥60 列），窄终端降级单栏（预览隐藏）。
        // CJK 全角在 TestBackend 渲染为"字+空格"——断言前去空白（M21-E3 坑）。
        let compact = |t: &ratatui::backend::TestBackend, w: u16, h: u16| -> String {
            (0..h)
                .flat_map(|y| (0..w).map(move |x| (x, y)))
                .map(|(x, y)| t.buffer()[(x, y)].symbol().to_owned())
                .collect::<String>()
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect()
        };
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("hello.txt"), "line1\nline2").unwrap();
        let hits = vec![(
            tmp.path().join("hello.txt").to_string_lossy().into_owned(),
            "dir".to_owned(),
            1.0,
        )];
        let mut scroll = 0usize;
        let mut engine = crate::image::ImageEngine::halfblocks();
        // 200 列：列表列 100——候选全路径（宿主临时目录长度不定）不截断
        // 丢叶子名（AUDIT-18 同款可移植性考量）。
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(200, 20)).unwrap();
        terminal
            .draw(|f| draw_pick_ui(f, "", 0, &hits, &mut scroll, &mut engine))
            .unwrap();
        let text = compact(terminal.backend(), 200, 20);
        assert!(text.contains("预览"), "预览窗格标题: {text}");
        assert!(
            text.contains("line1"),
            "与 TUI 同源的 render_preview 内容: {text}"
        );
        assert!(text.contains("hello.txt"), "候选列表可见: {text}");
        // 窄终端：预览隐藏（单栏降级）。
        let mut narrow =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(40, 12)).unwrap();
        narrow
            .draw(|f| draw_pick_ui(f, "", 0, &hits, &mut scroll, &mut engine))
            .unwrap();
        let text2 = compact(narrow.backend(), 40, 12);
        assert!(!text2.contains("预览"), "窄终端单栏无预览: {text2}");
        // 列表可见性不在此断言——候选全路径长度随宿主变化（AUDIT-18 坑），
        // 40 列必然截断；列表渲染已由宽终端用例覆盖。
    }

    #[test]
    fn m38_4_parse_date_ts() {
        // M38-4：YYYY-MM-DD → Unix 秒（UTC 0 点）。
        assert_eq!(parse_date_ts("1970-01-01"), Some(0));
        assert_eq!(parse_date_ts("2026-01-01"), Some(1_767_225_600));
        assert_eq!(parse_date_ts("2026-11-21"), Some(1_795_219_200));
        assert_eq!(parse_date_ts("2026-13-01"), None);
        assert_eq!(parse_date_ts("2026-00-10"), None);
        assert_eq!(parse_date_ts("20260101"), None);
        assert_eq!(parse_date_ts("abc"), None);
    }

    #[test]
    fn m38_4_stats_export_purge_prune_roundtrip() {
        // M38-4：stats/export/purge/prune 全链路（注入路径，不触真实库）。
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("f.redb");
        {
            let store = Store::open(&db).unwrap();
            store.append_event(&rec("/a", 1000)).unwrap();
            store.append_event(&rec("/b", 2000)).unwrap();
            store.append_event(&rec("/c", 3000)).unwrap();
            store.snapshot_upsert(&snap("/a", "", 1.0)).unwrap();
            store
                .snapshot_upsert(&SnapshotEntry {
                    path: "/cmd1".to_owned(),
                    topic: "cmd".to_owned(),
                    keyword: String::new(),
                    score: 1.0,
                    tick: 3,
                    clock: 3,
                })
                .unwrap();
        }
        // 统计：events=3，snapshot 按 topic 分组 dir=1、cmd=1。
        let st = frecency_stats_at(&db).unwrap();
        assert_eq!(st.events, 3);
        assert!(st.db_size_bytes > 0);
        assert_eq!(
            st.snapshots_by_topic,
            vec![("cmd".to_owned(), 1), ("dir".to_owned(), 1)]
        );
        assert!(st.meta.iter().any(|(k, _)| k == "global_tick"));
        // 导出：JSON 含全部事件与快照（round-trip 校验）。
        let out = tmp.path().join("export.json");
        let n = frecency_export_at(&db, out.to_str().unwrap()).unwrap();
        assert_eq!(n, 3);
        let text = std::fs::read_to_string(&out).unwrap();
        assert!(text.contains("\"/a\"") && text.contains("\"cmd\""));
        let parsed: FrecencyExport = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed.events.len(), 3);
        assert_eq!(parsed.snapshot.len(), 2);
        // 按时间裁剪：before=2000 → 仅 /a（ts=1000）满足 ts<before 被删；
        // snapshot 不动（分数经衰减/容量清理收敛）。
        let removed = frecency_prune_before_at(&db, 2000).unwrap();
        assert_eq!(removed, 1, "仅 /a（ts=1000）被裁剪");
        let st = frecency_stats_at(&db).unwrap();
        assert_eq!(st.events, 2);
        assert_eq!(st.snapshots_by_topic.len(), 2, "snapshot 不受 prune 影响");
        // 全清：events+snapshot 归零，meta 复位（pending 降级文件删除在
        // 公开包装 frecency_purge() 内——全局路径，测试不触碰）。
        let (ev, sn) = frecency_purge_at(&db).unwrap();
        assert_eq!((ev, sn), (2, 2));
        let st = frecency_stats_at(&db).unwrap();
        assert_eq!(st.events, 0);
        assert_eq!(st.snapshots_by_topic.len(), 0);
        assert_eq!(st.meta.len(), 0, "meta 时钟复位");
        // 缺失库：stats 全 0、export 空文件。
        let missing = tmp.path().join("missing.redb");
        let st = frecency_stats_at(&missing).unwrap();
        assert_eq!(st.events, 0);
        assert_eq!(st.db_size_bytes, 0);
        let out2 = tmp.path().join("empty.json");
        frecency_export_at(&missing, out2.to_str().unwrap()).unwrap();
        let parsed: FrecencyExport =
            serde_json::from_str(&std::fs::read_to_string(out2).unwrap()).unwrap();
        assert!(parsed.events.is_empty() && parsed.snapshot.is_empty());
    }
}
