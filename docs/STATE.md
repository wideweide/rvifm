# STATE — 当前状态

> 本文件是中断恢复的锚点：只回答“现在跑到哪、下一步是什么”。需求定义见 [SPEC.md](SPEC.md)，任务清单见 [TASKS.md](TASKS.md)，决策理由见 [NOTES.md](NOTES.md)，评估结论见 [REVIEW.md](REVIEW.md)。详细历史以 git log 为准，本文件不重复流水账。

- **更新时间**：2026-11-20
- **当前阶段**：M28–M33 新批次**已登记（用户确认，执行已批准启动）**——M28 运维操作补齐 / M29 对比增强 / M30 frecency 通用化（对齐同级 frecency 项目）/ M31 文档与验收补齐 / M32 远端健壮性与会话扩展（P2 补登）/ M33 键位模式分层扩展（P2 补登）；统一目标 0.10.0
- **当前任务**：M31-4 ACCEPTANCE M0–M13 复选框化（M31-3 已于 2026-11-20 完成；自动续轮中）
- **阶段状态**：M0–M27 + AUDIT + MISC 全部完成；v0.9.0 已发布（GitHub Release 三归档含 rv 别名，CI #271 五 job 全绿）；**crates.io publish 待 Windows 机器凭据执行**（Linux 侧无凭据，非停止条件）；M11-6 真实终端验证 deferred（清单化任务已登记 M31-3）
- **基线**：不固化 commit hash，以执行时的 `HEAD` 为准；提交文档后 HEAD 变化属正常，无需回改本文档

## 进行中 / 暂停 / 延期

- PROC-1：预算门控与断点落地协议已写入 `AGENTS.md` §3 与 `ACCEPTANCE.md` §1，**已生效（用户 2026-11-19 确认）**，为「自动续轮」第 3 条停止条件。
- M11-6：Alacritty 真实终端图片显示验证，需人工环境，deferred。

## 下一步

1. **执行已批准**：从 M28-1（`:link` 符号链接创建）开始，按 TASKS 升序推进 M28 → M29 → M30 → M31 → M32 → M33，全部完成后自动发布 0.10.0（PLAN §4）。
2. Windows 侧执行 `cargo publish`（0.9.0；凭据仅在该机器）→ 完成后提交 `release: 0.9.0 verified`。
3. M11-6 真实终端验证：待 M31-3 清单落地后可在任意真实终端补验。

## 批次关闭记录

- M0–M6：核心浏览、多标签、`u` 撤销闭环、选择批量、搜索过滤、配置与 v0.1.0 发布。
- M7–M11：只读归档 VFS、frecency（`pick`/`record`）、图标主题、声明式键位、预览与打开；M11-6 真实终端验证 deferred。
- M12：操作级内联提权（`sudo -A` + `--askpass`）；M12-2 教训固化为 Linux target 检查门禁。
- M13–M14：vifm 对齐增强（DD/tree/cw/:jobs/view）与双栏/标签栏；发布 0.4.0 / 0.5.0。
- M15–M17：用户实测反馈、导航增强、递归搜索与查看器；发布 0.6.0。
- M18–M20：排序与终端集成、SFTP 浏览/配置、远端写与跨 VFS。
- M21：UI/UX 打磨（F1–F9 + E1–E8），273 单测；发布 0.7.0。
- M22：SFTP 安全与远端体验补全（7 项），283 单测；发布 0.8.0。
- M23：frecency 对齐增强与 Pane 模式（12 项），292 单测；0.8.0。
- M24：交互增强与列表规范（9 项），318 单测 + 6 集成；列对齐、自然排序、行可视、计数前缀、命令历史、路径补全、`:compare`、`:jobs`。
- M25：会话持久化、归档创建/校验、异步 git 状态列与 `gi`、frecency 通用化评估；**M25-2 marks/bookmark 由 frecency 实现补齐（2026-11-20）**——`m<char>` 写入 keyword 事件（dir/file topic、`..` 行=当前目录）、`'<char>` 经 jump_to 支持 file 语义、`:bmark`/`:bmarks`/`:bmgo`/`:delbmarks` 命令族（`:delbmarks` 清除 = 空 keyword 事件保留累积分数）、多字符 keyword 搜索高优先级（M23-1 既有）。**M25 批次自此全部完成**；361 单测 + 5 集成。
- AUDIT：评估整改 18 项全部完成（2026-11-20）——pick 只读红线（redb ReadOnlyDatabase）、opener/主题/预览整层接线、app.rs 拆分 `src/app/` 7 模块、CI 门禁增强（clippy all-targets + Linux cross-check）、测试隔离与可移植性整改（含 Store::open 数据目录自动创建）、生产 panic 面清零、glob 统一修 `*x*`、help/README 同步、惰性 compaction 与 per-topic λ 一致性、归档成员/xz 解压上限；357 单测 + 5 集成双环境绿。
- M29：对比增强 3 项全部完成（2026-11-20）——M29-1 `:compare bycontents`（SHA-256 分块哈希 + 64MB 护栏）、M29-2 dp/do 同步（DiffPut/DiffObtain Action + 可恢复删除 + 操作后重建；`!` 覆盖同步裁剪见 NOTES）、M29-3 文档收尾；399 单测 + 9 集成。
- M28：运维操作补齐 4 项全部完成（2026-11-20）——M28-1 `:link` 符号链接创建（junction 回退；undo_op symlink 感知修复）、M28-2 `:rename` 结构化批量重命名（RenamePreview 预览 + y 应用）、M28-3 `:jobs` 暂停/详情（JobControls/JobProgress）、M28-4 文档收尾；396 单测 + 9 集成；Windows junction 人工验证项随 M11-6 模式保留。
- MISC-1：`rv` 短别名二进制（2026-11-20）——Cargo 双 `[[bin]]` 同 `src/main.rs`（rv `test=false`）、CI release 归档含双二进制、README 三处说明、冒烟测试；385 单测 + 9 集成。**任务列表自此全部完成，进入 0.9.0 自动发布。**
- M27：0.8.0 实测问题与 vifm 视觉对齐 3 项全部完成（2026-11-20）——M27-1 符号链接目录跟随（lstat→path metadata 修正随 M27-3 验证落地）、M27-2 vifm 布局配色（pane_borders=false 默认 + 16 色回退）、M27-3 PDF 前 1–2 页图像可配置（[preview] 表 + 中断提交 3 缺陷修正，见 NOTES）；M27 批次自此全部完成；385 单测 + 8 集成。
- M26：归档创建扩展 5 项全部完成（2026-11-20）——`:archive` 后台作业化（archive_plan/execute_archive_job 分离，对话框携带作业参数）、7z 内建创建（sevenz-rust 0.6）、7z 虚拟浏览（SevenZFs，加密/损坏可读）、外部 CLI 降级（rar/cab/lzh/tar.xz/tar.bz2/tar.zst，PATH 探测 + 输出捕获）、README 归档创建行为表；362 单测 + 5 集成。

## 质量门禁

- 本地：`cargo fmt` → `cargo clippy --all-targets -- -D warnings` → `cargo build` → `cargo test` → `--version` / `--help` 冒烟。
- **当前已知门禁偏差**：本机 `cargo test` 全绿（385 单测 + 8 集成，M27-3 修复 AUDIT-7 遗留的 opener E0369 后恢复可编译，`TMUX`/`STY` 隔离不受宿主影响）；两残留均以 CI 为准：① Linux target cross-check 自 russh（M19-3b）起无法本机构建 aws-lc-sys（AUDIT-9 固化进 ubuntu job）；② `cargo check --features pdf-visual` 需 rustc 1.97（exprim@0.4.0），本机 1.95.0 不可执行，CI 工具链覆盖。
- 含 `cfg(unix)` / `cfg(test)` 改动须额外跑 `cargo check --target x86_64-unknown-linux-gnu --all-targets`（M12-2 教训）——本机无法构建 aws-lc-sys 时以 CI 为准（AUDIT-9 起）。
- CI：`ubuntu-latest` + `windows-latest` 全绿；tag `v*` 触发三 target release 归档。

## 预算与风险

- 控制原则：一轮一任务，只注入相关 SPEC 片段；同一任务 3 轮不过验收即缩小范围并 `deferred`；一轮结束自动进入下一轮，无需人工确认（AGENTS §3）。
- 主要风险：`pick` 只读红线（结构性保证，回归测试固化）；`cfg(unix)` 测试代码 Windows 本机不编译——依赖 CI ubuntu job 与本机 Linux 侧全量运行（M27-3 连续发现两例潜伏缺陷，见 NOTES）；`pdf-visual` 本机不可检查（rustc 1.95 < exprim 要求 1.97），CI 覆盖。
- 环境：本机以实际环境为准（Windows Server 2016 + `x86_64-pc-windows-msvc`），其它平台交给 CI（ubuntu-latest / windows-latest）。

## 环境

- 本机：以实际环境为准（Windows Server 2016，目标 `x86_64-pc-windows-msvc`）。
- CI：`ubuntu-latest` + `windows-latest`（fmt --check / clippy -D warnings / build --locked / test --locked / pdf-visual check）。
- 发布外循环：`push/tag` → CI 矩阵构建 → 测试 → 归档 → Release（crates.io 发布沿用本机凭据）。

## 恢复方式

1. 读本文件确认当前阶段与任务。
2. 打开 [TASKS.md](TASKS.md)，取第一个未勾选任务。
3. 读 [AGENTS.md](../AGENTS.md) §3 执行循环。
4. 只读该任务相关的 [SPEC.md](SPEC.md) 片段与 [ACCEPTANCE.md](ACCEPTANCE.md) 条目。
5. 完成后更新本文件与 TASKS，再提交。
