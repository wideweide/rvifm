# bench.md — 性能基准基线（M40-1）

criterion 基准集（`benches/rvifm_bench.rs`；SPEC M40-1 五组）。运行方式：

```sh
cargo bench --bench rvifm_bench            # 全部
cargo bench --bench rvifm_bench -- copy_1gb # 单组
```

## 基线（2026-11-21）

- **环境**：Windows Server 2016（x86_64-pc-windows-msvc）、rustc 1.99.1、
  bench profile（optimized）、criterion 0.7（`--measurement-time 3 --warm-up-time 1`）。
- **语料**：固定语料一次生成（OnceLock 复用，不进计时）；100k 目录 = 100k
  文件 + 10k 子目录；1GB = 8MB×128 分块写；replay = 10 万事件（dir/file 交替）
  + 10 条快照。

| 基准 | 结果 | 备注 |
|---|---|---|
| dir_load_small_100 | 6.60 ms | 100 条目加载 + 构造（冷启动路径近似） |
| load_100k/load_entries_100k | **6.83 s** | ≈68 µs/条目——每条目 metadata stat 为主；**潜在优化点**（批量 stat / 并行遍历），1.0 后评估 |
| natural_sort/sort_10k_natural | 3.26 ms | 10k 名排序 |
| natural_sort/sort_10k_lexical | 588 µs | 对照：自然排序 ≈ 字典序 5.5×（可接受） |
| frecency_replay/replay_100k_events | 65.0 ms | 10 万事件重放（≈0.65 µs/事件） |
| copy_1gb/copy_file_chunked_1gb | 570 ms | ≈ **1.79 GB/s** 吞吐（M36-1 分块复制） |

## 复跑口径

- 100k 与 1GB 语料首次生成较慢（本机数分钟），criterion 二次运行复用
  `target/criterion` 状态只做测量；语料本身每次进程重建（临时目录）。
- CI 不跑 bench（cargo test/build 不含 bench 运行；`cargo check --all-targets`
  覆盖 bench 编译门禁）。
- 1.0 发布后如做 load_100k 优化，以本表为对照基线（同机同参复跑）。
