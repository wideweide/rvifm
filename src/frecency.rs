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

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(path: &str, keyword: &str, score: f64) -> SnapshotEntry {
        SnapshotEntry {
            path: path.to_owned(),
            topic: "dir".to_owned(),
            keyword: keyword.to_owned(),
            score,
            tick: 100,
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
