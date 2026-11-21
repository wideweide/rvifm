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
/// M30-1：`cmd` topic 独立类型时钟（命令历史 frecency；缺失旧库自全局
/// 时钟连续分叉，与 dir/file 同语义）。
const META_CMD_TICK: &str = "cmd_tick";
const META_LAST_COMPACTION: &str = "last_compaction";
/// snapshot key 分隔符（unit separator，路径中不出现）。
const KEY_SEP: char = '\u{1f}';

/// 数据库句柄：`Rw` 供建表与写路径；`Ro` 供 `pick` / 跳转选择器只读打开
/// （AUDIT-1：类型层面禁止写事务，保证 mtime 与 tick 不变）。
enum DbHandle {
    Rw(redb::Database),
    Ro(redb::ReadOnlyDatabase),
}

/// redb 数据库句柄（多读单写由 redb 保证）。
pub struct Store {
    db: DbHandle,
    path: PathBuf,
}

impl Store {
    /// 打开（不存在则创建）数据库文件，并确保三表存在（redb 4 显式建表）。
    /// AUDIT-12：数据目录缺失时自动创建——全新安装的首次
    /// `record` / `maintenance compact` 不再因父目录不存在而失败
    /// （此前 record 靠降级文件的 create_dir_all 兜底、compact 直接失败）。
    pub fn open(path: &Path) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(io_err("数据目录创建失败"))?;
        }
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
            db: DbHandle::Rw(db),
            path: path.to_path_buf(),
        })
    }

    /// 只读打开（AUDIT-1）：库必须已存在（建表由写路径负责）；不 `begin_write`、
    /// 不建表、不写任何事务。库缺失返回 `ErrorKind::NotFound`（调用方按空库降级）。
    pub fn open_readonly(path: &Path) -> std::io::Result<Self> {
        let db = redb::ReadOnlyDatabase::open(path).map_err(|e| {
            let kind = if path.exists() {
                std::io::ErrorKind::InvalidData
            } else {
                std::io::ErrorKind::NotFound
            };
            std::io::Error::new(kind, e)
        })?;
        Ok(Self {
            db: DbHandle::Ro(db),
            path: path.to_path_buf(),
        })
    }

    /// 读事务入口：Rw/Ro 均支持（redb 4 两类句柄的 `begin_read` 同型）。
    fn begin_read(&self, ctx: &'static str) -> std::io::Result<redb::ReadTransaction> {
        match &self.db {
            DbHandle::Rw(db) => db.begin_read(),
            DbHandle::Ro(db) => db.begin_read(),
        }
        .map_err(io_err(ctx))
    }

    /// 写事务入口：只读句柄在此拒绝（pick 路径误写的类型层防护）。
    fn begin_write(&self, ctx: &'static str) -> std::io::Result<redb::WriteTransaction> {
        match &self.db {
            DbHandle::Rw(db) => db.begin_write().map_err(io_err(ctx)),
            DbHandle::Ro(_) => Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("{ctx}: 只读库不可写（AUDIT-1）"),
            )),
        }
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
        let rx = self.begin_read("meta 读事务失败")?;
        let table = rx.open_table(META).map_err(io_err("meta 表打开失败"))?;
        Ok(table
            .get(META_GLOBAL_TICK)
            .map_err(io_err("meta 读取失败"))?
            .map(|v| v.value())
            .unwrap_or(0))
    }

    /// 当前类型时钟（M8-10）：`dir` → `dir_tick`，`cmd` → `cmd_tick`
    /// （M30-1），其余 → `file_tick`。
    /// 缺失（旧库）时返回 `global_tick` 当前值——新时钟自旧全局时钟连续
    /// 分叉，旧数据的 aging 行为不变。
    pub fn type_tick(&self, topic: &str) -> std::io::Result<u64> {
        let key = if topic == "dir" {
            META_DIR_TICK
        } else if topic == "cmd" {
            META_CMD_TICK
        } else {
            META_FILE_TICK
        };
        let rx = self.begin_read("meta 读事务失败")?;
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
        let tx = self.begin_write("events 写事务失败")?;
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
            } else if record.topic == "cmd" {
                META_CMD_TICK
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
        let rx = self.begin_read("events 读事务失败")?;
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
        let tx = self.begin_write("snapshot 写事务失败")?;
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
        let rx = self.begin_read("snapshot 读事务失败")?;
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
        let rx = self.begin_read("meta 读事务失败")?;
        let table = rx.open_table(META).map_err(io_err("meta 表打开失败"))?;
        Ok(table
            .get(META_LAST_COMPACTION)
            .map_err(io_err("meta 读取失败"))?
            .map(|v| v.value())
            .unwrap_or(0))
    }

    /// 记录 last_compaction（M8-5）。
    pub fn set_last_compaction(&self, tick: u64) -> std::io::Result<()> {
        let tx = self.begin_write("meta 写事务失败")?;
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
        let tx = self.begin_write("events 裁剪事务失败")?;
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

    /// M38-4：events 表条数（统计用）。
    pub fn events_count(&self) -> std::io::Result<usize> {
        let rx = self.begin_read("events 读事务失败")?;
        let table = rx.open_table(EVENTS).map_err(io_err("events 表打开失败"))?;
        Ok(table.iter().map_err(io_err("events 迭代失败"))?.count())
    }

    /// M38-4：meta 全部键值（统计/导出用；时钟与 compaction 标记）。
    pub fn meta_all(&self) -> std::io::Result<Vec<(String, u64)>> {
        let rx = self.begin_read("meta 读事务失败")?;
        let table = rx.open_table(META).map_err(io_err("meta 表打开失败"))?;
        let mut out = Vec::new();
        for pair in table.iter().map_err(io_err("meta 迭代失败"))? {
            let (k, v) = pair.map_err(io_err("meta 迭代失败"))?;
            out.push((k.value().to_owned(), v.value()));
        }
        Ok(out)
    }

    /// M38-4：全清——清空 events/snapshot/meta 三表。返回 (events 删除数,
    /// snapshot 删除数)。meta（时钟与 last_compaction）一并复位（库退回
    /// 全新状态）。
    pub fn purge_all(&self) -> std::io::Result<(usize, usize)> {
        let tx = self.begin_write("purge 写事务失败")?;
        let counts = {
            let mut events = tx.open_table(EVENTS).map_err(io_err("events 表打开失败"))?;
            let keys: Vec<u64> = events
                .iter()
                .map_err(io_err("events 迭代失败"))?
                .filter_map(|pair| pair.ok().map(|(k, _)| k.value()))
                .collect();
            let ev = keys.len();
            for k in keys {
                events.remove(k).map_err(io_err("events 删除失败"))?;
            }
            let mut snapshot = tx
                .open_table(SNAPSHOT)
                .map_err(io_err("snapshot 表打开失败"))?;
            let keys: Vec<String> = snapshot
                .iter()
                .map_err(io_err("snapshot 迭代失败"))?
                .filter_map(|pair| pair.ok().map(|(k, _)| k.value().to_owned()))
                .collect();
            let sn = keys.len();
            for k in keys {
                snapshot
                    .remove(k.as_str())
                    .map_err(io_err("snapshot 删除失败"))?;
            }
            let mut meta = tx.open_table(META).map_err(io_err("meta 表打开失败"))?;
            let keys: Vec<String> = meta
                .iter()
                .map_err(io_err("meta 迭代失败"))?
                .filter_map(|pair| pair.ok().map(|(k, _)| k.value().to_owned()))
                .collect();
            for k in keys {
                meta.remove(k.as_str()).map_err(io_err("meta 删除失败"))?;
            }
            (ev, sn)
        };
        tx.commit().map_err(io_err("purge 提交失败"))?;
        Ok(counts)
    }

    /// M38-4：按时间裁剪事件——删除 `ts < before` 的事件。返回删除数。
    /// snapshot 不动（分数经 EMS 衰减与容量清理自然收敛，见 NOTES M38-4）。
    pub fn prune_events_before(&self, before: u64) -> std::io::Result<usize> {
        let tx = self.begin_write("events 裁剪事务失败")?;
        let removed = {
            let mut events = tx.open_table(EVENTS).map_err(io_err("events 表打开失败"))?;
            let doomed: Vec<u64> = events
                .iter()
                .map_err(io_err("events 迭代失败"))?
                .filter_map(|pair| {
                    let (k, v) = pair.ok()?;
                    let record: EntryRecord = serde_json::from_slice(v.value()).ok()?;
                    (record.ts < before).then(|| k.value())
                })
                .collect();
            for k in &doomed {
                events.remove(*k).map_err(io_err("events 删除失败"))?;
            }
            doomed.len()
        };
        tx.commit().map_err(io_err("events 裁剪提交失败"))?;
        Ok(removed)
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
            keyword: String::new(),
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

    /// AUDIT-1：只读打开不改库——mtime 与 `global_tick` 均不变。
    /// 读序列与 `cmd_pick` 一致（snapshot / 游标 / 类型时钟 / 增量事件）。
    #[test]
    fn open_readonly_does_not_write_db() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("f.redb");
        {
            let store = Store::open(&p).unwrap();
            store.append_event(&rec("/a", 1)).unwrap();
        }
        let mtime_before = std::fs::metadata(&p).unwrap().modified().unwrap();
        let ro = Store::open_readonly(&p).unwrap();
        let tick_before = ro.global_tick().unwrap();
        let since = ro.last_compaction().unwrap();
        let _ = ro.snapshot_all().unwrap();
        let _ = ro.type_tick("dir").unwrap();
        let _ = ro.type_tick("file").unwrap();
        let _ = ro.events_since(since).unwrap();
        drop(ro);
        assert_eq!(
            std::fs::metadata(&p).unwrap().modified().unwrap(),
            mtime_before,
            "只读打开不得改数据库 mtime"
        );
        let ro2 = Store::open_readonly(&p).unwrap();
        assert_eq!(
            ro2.global_tick().unwrap(),
            tick_before,
            "只读打开不得推进 tick"
        );
    }

    /// AUDIT-1：库缺失时只读打开报 `NotFound`（调用方按空库降级，不裸失败）。
    #[test]
    fn open_readonly_missing_db_is_notfound() {
        let tmp = TempDir::new().unwrap();
        let err = match Store::open_readonly(&tmp.path().join("none.redb")) {
            Err(e) => e,
            Ok(_) => panic!("缺失库不应成功打开"),
        };
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }

    /// AUDIT-1：只读句柄在类型层拒绝写事务（pick 路径误写防护）。
    #[test]
    fn readonly_store_rejects_writes() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("f.redb");
        drop(Store::open(&p).unwrap());
        let ro = Store::open_readonly(&p).unwrap();
        let err = ro.append_event(&rec("/a", 1)).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
    }
}
