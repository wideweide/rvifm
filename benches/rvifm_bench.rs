//! M40-1：criterion 性能基准（SPEC M40-1 五组）——
//! 1. dir_load_small：冷启动路径近似（小目录 entries 加载 + AppState 构造）；
//! 2. load_100k：10 万项目录加载；
//! 3. natural_sort：自然排序（1 万名排序）；
//! 4. frecency_replay：frecency 重放 10 万事件；
//! 5. copy_1gb：1GB 复制吞吐（copy_file_chunked + CopyControls::none）。
//!
//! 产物基线记录于 docs/bench.md（本机环境；CI 不跑 bench）。
//! 固定语料生成一次（OnceLock 复用），不进计时。

use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, OnceLock};

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use rvifm::config::Config;
use rvifm::ops::CopyControls;
use rvifm::state::JobProgress;
use rvifm::tab;

/// bench 期间持有的临时目录（进程退出清理）。
struct Fixture {
    _guard: tempfile::TempDir,
    path: PathBuf,
}

/// 通用语料目录：`n` 个文件（内容 "x"）+ `n/10` 个子目录。
fn make_dir(dir: &Path, n: usize) {
    fs::create_dir_all(dir).unwrap();
    for i in 0..n {
        fs::write(dir.join(format!("f{i:06}.txt")), "x").unwrap();
        if i % 10 == 0 {
            fs::create_dir_all(dir.join(format!("d{i:06}"))).unwrap();
        }
    }
}

/// 小目录（冷启动路径近似；100 条目）。
fn small_dir() -> &'static Fixture {
    static FIX: OnceLock<Mutex<Fixture>> = OnceLock::new();
    let m = FIX.get_or_init(|| {
        let g = tempfile::tempdir().unwrap();
        let path = g.path().join("small");
        make_dir(&path, 100);
        Mutex::new(Fixture { _guard: g, path })
    });
    let guard = m.lock().unwrap();
    // OnceLock 里是 Mutex<Fixture>——泄漏一份静态借用（bench 进程生命周期内有效）。
    let leaked: &'static Fixture = Box::leak(Box::new(Fixture {
        _guard: tempfile::tempdir().unwrap(),
        path: guard.path.clone(),
    }));
    leaked
}

/// 10 万项目录。
fn big_dir() -> &'static Fixture {
    static FIX: OnceLock<Mutex<Fixture>> = OnceLock::new();
    let m = FIX.get_or_init(|| {
        let g = tempfile::tempdir().unwrap();
        let path = g.path().join("big100k");
        make_dir(&path, 100_000);
        Mutex::new(Fixture { _guard: g, path })
    });
    let guard = m.lock().unwrap();
    Box::leak(Box::new(Fixture {
        _guard: tempfile::tempdir().unwrap(),
        path: guard.path.clone(),
    }))
}

/// 1GB 语料文件（源与目标目录）。
fn big_file() -> (&'static Fixture, PathBuf, PathBuf) {
    static FIX: OnceLock<Mutex<(Fixture, PathBuf, PathBuf)>> = OnceLock::new();
    let m = FIX.get_or_init(|| {
        let g = tempfile::tempdir().unwrap();
        let src = g.path().join("src_1gb.bin");
        // 分块写 1GB（8MB 缓冲 128 次）。
        let buf = vec![0x41u8; 8 * 1024 * 1024];
        let mut f = fs::File::create(&src).unwrap();
        for _ in 0..128 {
            use std::io::Write as _;
            f.write_all(&buf).unwrap();
        }
        drop(f);
        let dst = g.path().join("dst_1gb.bin");
        Mutex::new((
            Fixture {
                _guard: g,
                path: src.clone(),
            },
            src,
            dst,
        ))
    });
    let guard = m.lock().unwrap();
    let f: &'static Fixture = Box::leak(Box::new(Fixture {
        _guard: tempfile::tempdir().unwrap(),
        path: guard.0.path.clone(),
    }));
    (f, guard.1.clone(), guard.2.clone())
}

/// frecency 10 万事件语料（dir/file 交替；快照 10 条）。
type ReplayCorpus = (
    Vec<rvifm::frecency::SnapshotEntry>,
    Vec<(u64, rvifm::frecency::EntryRecord)>,
);

/// 语料缓存槽。
static REPLAY_FIX: OnceLock<ReplayCorpus> = OnceLock::new();

fn replay_corpus() -> &'static ReplayCorpus {
    REPLAY_FIX.get_or_init(|| {
        use rvifm::frecency::{EntryKind, EntryRecord, SnapshotEntry};
        let snapshot = (0..10)
            .map(|i| SnapshotEntry {
                path: format!("/s{i}"),
                topic: "dir".to_owned(),
                keyword: String::new(),
                score: 1.0,
                tick: i,
                clock: i,
            })
            .collect();
        let events = (0..100_000u64)
            .map(|i| {
                (
                    i + 1,
                    EntryRecord {
                        path: format!("/e{}", i % 5000),
                        topic: if i % 2 == 0 { "dir" } else { "file" }.to_owned(),
                        kind: if i % 2 == 0 {
                            EntryKind::Navigate
                        } else {
                            EntryKind::Open
                        },
                        ts: 1_700_000_000 + i,
                        clock: i,
                        keyword: String::new(),
                    },
                )
            })
            .collect();
        (snapshot, events)
    })
}

fn bench_dir_load(c: &mut Criterion) {
    let small = small_dir();
    let cfg = Config::default();
    c.bench_function("dir_load_small_100", |b| {
        b.iter(|| {
            let entries = tab::load_entries(black_box(&small.path), black_box(&cfg)).unwrap();
            black_box(entries.len());
        })
    });
}

fn bench_load_100k(c: &mut Criterion) {
    let big = big_dir();
    let cfg = Config::default();
    let mut g = c.benchmark_group("load_100k");
    g.sample_size(10);
    g.bench_function("load_entries_100k", |b| {
        b.iter(|| {
            let entries = tab::load_entries(black_box(&big.path), black_box(&cfg)).unwrap();
            black_box(entries.len());
        })
    });
    g.finish();
}

fn bench_natural_sort(c: &mut Criterion) {
    // 1 万个混合数字文件名排序（自然 vs 字典序对照）。
    let names: Vec<String> = (0..10_000)
        .map(|i| format!("file{i:04}-part{}", i % 97))
        .collect();
    let mut g = c.benchmark_group("natural_sort");
    g.throughput(Throughput::Elements(10_000));
    g.bench_function("sort_10k_natural", |b| {
        b.iter(|| {
            let mut v = black_box(&names).clone();
            v.sort_by(|a, b| tab::natural_cmp(a, b));
            black_box(v.len());
        })
    });
    g.bench_function("sort_10k_lexical", |b| {
        b.iter(|| {
            let mut v = black_box(&names).clone();
            v.sort();
            black_box(v.len());
        })
    });
    g.finish();
}

fn bench_frecency_replay(c: &mut Criterion) {
    let (snapshot, events) = replay_corpus();
    let mut g = c.benchmark_group("frecency_replay");
    g.throughput(Throughput::Elements(100_000));
    g.bench_function("replay_100k_events", |b| {
        b.iter(|| {
            let out = rvifm::frecency::replay(
                black_box(snapshot),
                black_box(events),
                1_800_000_000,
                1_800_000_000,
                1_800_000_000,
            );
            black_box(out.len());
        })
    });
    g.finish();
}

fn bench_copy_1gb(c: &mut Criterion) {
    let (_fix, src, dst) = big_file();
    let mut g = c.benchmark_group("copy_1gb");
    g.sample_size(10);
    g.throughput(Throughput::Bytes(1024 * 1024 * 1024));
    g.bench_function("copy_file_chunked_1gb", |b| {
        b.iter(|| {
            let cancel = AtomicBool::new(false);
            let progress = JobProgress::default();
            let controls = CopyControls {
                cancel: &cancel,
                progress: &progress,
            };
            std::fs::remove_file(&dst).ok();
            rvifm::ops::copy_file_chunked(black_box(&src), black_box(&dst), &controls).unwrap();
        })
    });
    g.finish();
}

criterion_group!(
    benches,
    bench_dir_load,
    bench_load_100k,
    bench_natural_sort,
    bench_frecency_replay,
    bench_copy_1gb
);
criterion_main!(benches);
