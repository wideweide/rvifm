//! redb 持久化存储（M8-2；SPEC §4.6、NOTES §4.5）。
//!
//! 三表 schema：
//! - `events`（key: tick u64，value: `EntryRecord` JSON）——append-only；
//! - `snapshot`（key: `topic\x1fpath`，value: `SnapshotEntry` JSON）——compaction 产物；
//! - `meta`（key: 字符串，value: u64）——`global_tick` / `last_compaction`。
//!
//! tick 语义：每次 `append_event` 在事务内读取 `global_tick` 并 +1 写入，
//! 事件 key 即该 tick；snapshot 条目的 `tick` = 最后一次应用到该条目的事件
//! tick。**record 是唯一写入入口**（M8-3 封装命令行）；并发为短事务，
//! 重试与 `events.pending` 降级在 M8-3 接入。

// M8-2 仅固化 schema 与 tick 语义；消费方：M8-3（record 子命令）、
// M8-4（重放评分）、M8-5（compaction）。
#![allow(dead_code)]

use crate::frecency::{EntryRecord, SnapshotEntry};
use redb::{ReadableDatabase, ReadableTable};
use std::path::{Path, PathBuf};

/// redb 表定义。
const EVENTS: redb::TableDefinition<u64, &[u8]> = redb::TableDefinition::new("events");
const SNAPSHOT: redb::TableDefinition<&str, &[u8]> = redb::TableDefinition::new("snapshot");
const META: redb::TableDefinition<&str, u64> = redb::TableDefinition::new("meta");

const META_GLOBAL_TICK: &str = "global_tick";
const META_DIR_TICK: &str = "dir_tick";
const META_FILE_TICK: &str = "file_tick";
const META_LAST_COMPACTION: &str = "last_compaction";
/// snapshot key 分隔符（unit separator，路径中不出现）。
const KEY_SEP: char = '\u{1f}';

/// redb 数据库句柄（多读单写由 redb 保证）。
pub struct Store {
    db: redb::Database,
    path: PathBuf,
}

impl Store {
    /// 打开（不存在则创建）数据库文件，并确保三表存在（redb 4 显式建表）。
    pub fn open(path: &Path) -> std::io::Result<Self> {
        let db = redb::Database::create(path)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let tx = db.begin_write().map_err(io_err("建表事务失败"))?;
        {
            tx.open_table(EVENTS).map_err(io_err("events 建表失败"))?;
            tx.open_table(SNAPSHOT)
                .map_err(io_err("snapshot 建表失败"))?;
            tx.open_table(META).map_err(io_err("meta 建表失败"))?;
        }
        tx.commit().map_err(io_err("建表提交失败"))?;
        Ok(Self {
            db,
            path: path.to_path_buf(),
        })
    }

    /// 数据库文件路径（测试与降级提示用）。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// snapshot key：`topic\x1fpath`。
    fn snapshot_key(topic: &str, path: &str) -> String {
        format!("{topic}{KEY_SEP}{path}")
    }

    /// 当前 `global_tick`（无 meta 表数据时为 0）。
    pub fn global_tick(&self) -> std::io::Result<u64> {
        let rx = self.db.begin_read().map_err(io_err("meta 读事务失败"))?;
        let table = rx.open_table(META).map_err(io_err("meta 表打开失败"))?;
        Ok(table
            .get(META_GLOBAL_TICK)
            .map_err(io_err("meta 读取失败"))?
            .map(|v| v.value())
            .unwrap_or(0))
    }

    /// 当前类型时钟（M8-10）：`dir` → `dir_tick`，其余 → `file_tick`。
    /// 缺失（旧库）时返回 `global_tick` 当前值——新时钟自旧全局时钟连续
    /// 分叉，旧数据的 aging 行为不变。
    pub fn type_tick(&self, topic: &str) -> std::io::Result<u64> {
        let key = if topic == "dir" {
            META_DIR_TICK
        } else {
            META_FILE_TICK
        };
        let rx = self.db.begin_read().map_err(io_err("meta 读事务失败"))?;
        let table = rx.open_table(META).map_err(io_err("meta 表打开失败"))?;
        if let Some(v) = table.get(key).map_err(io_err("meta 读取失败"))? {
            return Ok(v.value());
        }
        self.global_tick()
    }

    /// 追加事件（M8-10 独立时钟）：事务内——
    /// 1. `global_tick += 1` 作为**全局事件序号**（events 表 key，唯一）；
    /// 2. 按 `record.topic` 推进对应类型时钟（`dir_tick`/`file_tick`，
    ///    缺失时以 `global_tick` 当前值初始化），写入事件 JSON 的 `clock`；
    /// 3. 事件以全局序号为 key 写入。
    ///
    /// 返回全局序号（compaction 游标用）。record 是唯一写入入口（M8-3 封装）。
    pub fn append_event(&self, record: &EntryRecord) -> std::io::Result<u64> {
        let tx = self.db.begin_write().map_err(io_err("events 写事务失败"))?;
        let new_seq = {
            let mut meta = tx.open_table(META).map_err(io_err("meta 表打开失败"))?;
            let cur = meta
                .get(META_GLOBAL_TICK)
                .map_err(io_err("meta 读取失败"))?
                .map(|v| v.value())
                .unwrap_or(0);
            let new_seq = cur + 1;
            meta.insert(META_GLOBAL_TICK, new_seq)
                .map_err(io_err("meta 写入失败"))?;

            // 类型时钟推进（dir/file 各自独立；缺失则以全局序号起点初始化）。
            let clock_key = if record.topic == "dir" {
                META_DIR_TICK
            } else {
                META_FILE_TICK
            };
            let new_clock = match meta.get(clock_key).map_err(io_err("meta 读取失败"))? {
                Some(v) => v.value() + 1,
                None => cur + 1, // 旧库无该时钟：自当前全局值连续分叉
            };
            meta.insert(clock_key, new_clock)
                .map_err(io_err("meta 写入失败"))?;

            let mut rec = record.clone();
            rec.clock = new_clock;
            let json = serde_json::to_vec(&rec).map_err(io_err("事件序列化失败"))?;
            let mut events = tx.open_table(EVENTS).map_err(io_err("events 表打开失败"))?;
            events
                .insert(new_seq, json.as_slice())
                .map_err(io_err("events 写入失败"))?;
            new_seq
        };
        tx.commit().map_err(io_err("events 提交失败"))?;
        Ok(new_seq)
    }

    /// 读取 `seq > since` 的全部事件（按全局序号升序；重放评分用）。
    pub fn events_since(&self, since: u64) -> std::io::Result<Vec<(u64, EntryRecord)>> {
        let rx = self.db.begin_read().map_err(io_err("events 读事务失败"))?;
        let table = rx.open_table(EVENTS).map_err(io_err("events 表打开失败"))?;
        let mut out = Vec::new();
        for pair in table
            .range((std::ops::Bound::Excluded(since), std::ops::Bound::Unbounded))
            .map_err(io_err("events 范围读取失败"))?
        {
            let (k, v) = pair.map_err(io_err("events 迭代失败"))?;
            let record: EntryRecord =
                serde_json::from_slice(v.value()).map_err(io_err("事件反序列化失败"))?;
            out.push((k.value(), record));
        }
        Ok(out)
    }

    /// 写入/更新 snapshot 条目（compaction 用，M8-5 接入调用方）。
    pub fn snapshot_upsert(&self, entry: &SnapshotEntry) -> std::io::Result<()> {
        let json = serde_json::to_vec(entry).map_err(io_err("snapshot 序列化失败"))?;
        let key = Self::snapshot_key(&entry.topic, &entry.path);
        let tx = self
            .db
            .begin_write()
            .map_err(io_err("snapshot 写事务失败"))?;
        {
            let mut table = tx
                .open_table(SNAPSHOT)
                .map_err(io_err("snapshot 表打开失败"))?;
            table
                .insert(key.as_str(), json.as_slice())
                .map_err(io_err("snapshot 写入失败"))?;
        }
        tx.commit().map_err(io_err("snapshot 提交失败"))?;
        Ok(())
    }

    /// 读取全部 snapshot 条目（重放起点）。
    pub fn snapshot_all(&self) -> std::io::Result<Vec<SnapshotEntry>> {
        let rx = self
            .db
            .begin_read()
            .map_err(io_err("snapshot 读事务失败"))?;
        let table = rx
            .open_table(SNAPSHOT)
            .map_err(io_err("snapshot 表打开失败"))?;
        let mut out = Vec::new();
        for pair in table.iter().map_err(io_err("snapshot 迭代失败"))? {
            let (_k, v) = pair.map_err(io_err("snapshot 迭代失败"))?;
            let entry: SnapshotEntry =
                serde_json::from_slice(v.value()).map_err(io_err("snapshot 反序列化失败"))?;
            out.push(entry);
        }
        Ok(out)
    }

    /// 上次 compaction 时已重放的 tick（惰性 compaction 起点，M8-5 用）。
    pub fn last_compaction(&self) -> std::io::Result<u64> {
        let rx = self.db.begin_read().map_err(io_err("meta 读事务失败"))?;
        let table = rx.open_table(META).map_err(io_err("meta 表打开失败"))?;
        Ok(table
            .get(META_LAST_COMPACTION)
            .map_err(io_err("meta 读取失败"))?
            .map(|v| v.value())
            .unwrap_or(0))
    }

    /// 记录 last_compaction（M8-5）。
    pub fn set_last_compaction(&self, tick: u64) -> std::io::Result<()> {
        let tx = self.db.begin_write().map_err(io_err("meta 写事务失败"))?;
        {
            let mut meta = tx.open_table(META).map_err(io_err("meta 表打开失败"))?;
            meta.insert(META_LAST_COMPACTION, tick)
                .map_err(io_err("meta 写入失败"))?;
        }
        tx.commit().map_err(io_err("meta 提交失败"))?;
        Ok(())
    }

    /// 删除 `tick <= upto` 的已合并事件（compaction 裁剪，M8-5）。
    pub fn prune_events_le(&self, upto: u64) -> std::io::Result<()> {
        let tx = self
            .db
            .begin_write()
            .map_err(io_err("events 裁剪事务失败"))?;
        {
            let mut events = tx.open_table(EVENTS).map_err(io_err("events 表打开失败"))?;
            let keys: Vec<u64> = events
                .range(..=upto)
                .map_err(io_err("events 范围读取失败"))?
                .filter_map(|pair| pair.ok().map(|(k, _)| k.value()))
                .collect();
            for k in keys {
                events.remove(k).map_err(io_err("events 删除失败"))?;
            }
        }
        tx.commit().map_err(io_err("events 裁剪提交失败"))?;
        Ok(())
    }
}

fn io_err<E: std::fmt::Display>(msg: &'static str) -> impl Fn(E) -> std::io::Error {
    move |e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{msg}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frecency::EntryKind;
    use tempfile::TempDir;

    fn rec(path: &str, ts: u64) -> EntryRecord {
        EntryRecord {
            path: path.to_owned(),
            topic: "dir".to_owned(),
            kind: EntryKind::Navigate,
            ts,
            clock: 0,
        }
    }

    #[test]
    fn open_creates_database_file() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("frecency.redb");
        let store = Store::open(&p).unwrap();
        assert_eq!(store.global_tick().unwrap(), 0);
        assert!(p.exists());
        assert_eq!(store.path(), p);
    }

    #[test]
    fn append_events_increments_tick_monotonically() {
        // M8-2 tick 语义：每次 append global_tick 单调 +1，事件 key = 该 tick。
        let tmp = TempDir::new().unwrap();
        let store = Store::open(&tmp.path().join("f.redb")).unwrap();

        assert_eq!(store.append_event(&rec("/a", 100)).unwrap(), 1);
        assert_eq!(store.append_event(&rec("/b", 200)).unwrap(), 2);
        assert_eq!(store.append_event(&rec("/a", 300)).unwrap(), 3);
        assert_eq!(store.global_tick().unwrap(), 3);

        let events = store.events_since(0).unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].0, 1);
        assert_eq!(events[0].1.path, "/a");
        assert_eq!(events[2].1.ts, 300);

        // 增量重放：since=1 只取 tick>1。
        let since = store.events_since(1).unwrap();
        assert_eq!(since.len(), 2);
        assert_eq!(since[0].0, 2);
    }

    #[test]
    fn snapshot_upsert_and_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let store = Store::open(&tmp.path().join("f.redb")).unwrap();

        let entry = SnapshotEntry {
            path: "/w/proj".to_owned(),
            topic: "dir".to_owned(),
            keyword: "proj".to_owned(),
            score: 4.5,
            tick: 7,
            clock: 7,
        };
        store.snapshot_upsert(&entry).unwrap();
        // 同 key 再写 = 更新（upsert）。
        let updated = SnapshotEntry {
            score: 9.0,
            tick: 12,
            clock: 12,
            ..entry.clone()
        };
        store.snapshot_upsert(&updated).unwrap();

        let all = store.snapshot_all().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].score, 9.0);
        assert_eq!(all[0].tick, 12);
        assert_eq!(all[0].keyword, "proj");
    }

    #[test]
    fn compaction_marker_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let store = Store::open(&tmp.path().join("f.redb")).unwrap();
        assert_eq!(store.last_compaction().unwrap(), 0);
        store.set_last_compaction(42).unwrap();
        assert_eq!(store.last_compaction().unwrap(), 42);
    }

    #[test]
    fn reopened_store_persists_state() {
        // 持久化：重开数据库后 tick/事件/snapshot 均保留。
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("f.redb");
        {
            let store = Store::open(&p).unwrap();
            store.append_event(&rec("/a", 1)).unwrap();
            store
                .snapshot_upsert(&SnapshotEntry {
                    path: "/a".to_owned(),
                    topic: "dir".to_owned(),
                    keyword: String::new(),
                    score: 1.0,
                    tick: 1,
                    clock: 1,
                })
                .unwrap();
        }
        let store = Store::open(&p).unwrap();
        assert_eq!(store.global_tick().unwrap(), 1);
        assert_eq!(store.events_since(0).unwrap().len(), 1);
        assert_eq!(store.snapshot_all().unwrap().len(), 1);
    }
}
