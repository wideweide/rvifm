# STATE — 当前状态

> 本文件是中断恢复的锚点：只回答“现在跑到哪、下一步是什么”。需求定义见 [SPEC.md](SPEC.md)，任务清单见 [TASKS.md](TASKS.md)，决策理由见 [NOTES.md](NOTES.md)，评估结论见 [REVIEW.md](REVIEW.md)。详细历史以 git log 为准，本文件不重复流水账。

- **更新时间**：2026-11-20
- **更新时间**：2026-11-20
- **更新时间**：2026-11-20
- **当前阶段**：M0–M26 ✅ 全部完成（AUDIT 18/18、M25-2 marks、M26 归档扩展 5/5 均于 2026-11-20 落地）；下一阶段 M27（实测问题与视觉对齐）→ MISC-1 按序执行
- **当前任务**：M27-3 PDF 前 1–2 页图像预览可配置（M27-1/2 已于 2026-11-20 完成）
- **阶段状态**：M0–M26 + AUDIT 完成（M9/M11 的"未接线"能力已随 AUDIT-13/7/2/3 整改落地，**计入实质完成**）；M11-6 真实终端验证 deferred；M27 已确认按序排队；v0.8.0 已发布，0.9.0 在任务列表全部完成后自动发布
- **基线**：不固化 commit hash，以执行时的 `HEAD` 为准；提交文档后 HEAD 变化属正常，无需回改本文档

## 进行中 / 暂停 / 延期

- M27-1～M27-3：0.8.0 实测问题与 vifm 视觉对齐，**已确认按序排队**（M27-1 无前置；M27-2 依赖 AUDIT-13（已完成）；M27-3 依赖 AUDIT-2（已完成））。
- PROC-1：预算门控与断点落地协议已写入 `AGENTS.md` §3 与 `ACCEPTANCE.md` §1，**已生效（用户 2026-11-19 确认）**，为「自动续轮」第 3 条停止条件。
- M11-6：Alacritty 真实终端图片显示验证，需人工环境，deferred。

## 下一步

1. 执行 M27-1（符号链接目录进入）：跟随目标判定 is_dir、`l`/Enter 内部导航、断链可读（见 SPEC §27 / ACCEPTANCE §27）。
2. 之后按序推进 M27-2（vifm 风格布局配色）→ M27-3（PDF 前 1–2 页图像可配置）→ MISC-1（rv 别名）。
3. 全部任务完成后自动发布 0.9.0（无需人工决策）。

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
- M26：归档创建扩展 5 项全部完成（2026-11-20）——`:archive` 后台作业化（archive_plan/execute_archive_job 分离，对话框携带作业参数）、7z 内建创建（sevenz-rust 0.6）、7z 虚拟浏览（SevenZFs，加密/损坏可读）、外部 CLI 降级（rar/cab/lzh/tar.xz/tar.bz2/tar.zst，PATH 探测 + 输出捕获）、README 归档创建行为表；362 单测 + 5 集成。

## 质量门禁

- 本地：`cargo fmt` → `cargo clippy --all-targets -- -D warnings` → `cargo build` → `cargo test` → `--version` / `--help` 冒烟。
- **当前已知门禁偏差**：本机 `cargo test` 全绿（357 单测 + 5 集成，`TMUX`/`STY` 设/不设均验证，AUDIT-14/18 后无环境依赖）；唯一残留是 Linux target cross-check 自 russh（M19-3b）起无法在本机构建 aws-lc-sys（无 cross gcc），该门禁已由 AUDIT-9 固化进 CI（ubuntu job）。
- 含 `cfg(unix)` / `cfg(test)` 改动须额外跑 `cargo check --target x86_64-unknown-linux-gnu --all-targets`（M12-2 教训）——本机无法构建 aws-lc-sys 时以 CI 为准（AUDIT-9 起）。
- CI：`ubuntu-latest` + `windows-latest` 全绿；tag `v*` 触发三 target release 归档。

## 预算与风险

- 控制原则：一轮一任务，只注入相关 SPEC 片段；同一任务 3 轮不过验收即缩小范围并 `deferred`；一轮结束自动进入下一轮，无需人工确认（AGENTS §3）。
- 主要风险：文档与实现漂移（第二轮复核仍发现 marks/主题/opener 过度声明，已改状态陈述，行为待 AUDIT 修复）；`pick` 只读红线；`app.rs` 上帝文件；测试受 `TMUX` 影响；M27-2 配色改动可能回归列表/预览（用 TestBackend 快照 + `pane_borders` 保留旧外观），M27-3 PDF 外部工具平台差异（按序回退文本/提示）。
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
