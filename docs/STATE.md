# STATE — 当前状态

> 本文件是中断恢复的锚点：只回答“现在跑到哪、下一步是什么”。需求定义见 [SPEC.md](SPEC.md)，任务清单见 [TASKS.md](TASKS.md)，决策理由见 [NOTES.md](NOTES.md)，评估结论见 [REVIEW.md](REVIEW.md)。详细历史以 git log 为准，本文件不重复流水账。

- **更新时间**：2026-11-21
- **当前阶段**：M0–M34 + AUDIT + MISC 全部完成并关闭（v0.11.0 已发布）；**M35 文档与发布一致性执行中**（M36–M40 已登记按序排队，2026-11-21 复核报告蒸馏）；统一目标随批次递进（M35→0.11.1 … M40→1.0.0）
- **当前任务**：M38-1 检索结果可操作（M37 批次 7/7 已关闭；自动续轮中）
- **阶段状态**：M0–M33 + AUDIT + MISC + **M34 全部完成**（10/10，437 单测 + 9 集成，rustc 1.100 本机门禁绿含 pdf-visual）；M11-6 清单第 1 项 foot 已过；**0.9.0/0.10.0 经用户决策跳过不再发布**（2026-11-21，只发最新）
- **基线**：不固化 commit hash，以执行时的 `HEAD` 为准；提交文档后 HEAD 变化属正常，无需回改本文档

## 进行中 / 暂停 / 延期

- M11-6：真实终端图片显示（M31-3 六步清单）——第 1 项已过（foot，2026-11-21，Kitty graphics）；2–6 待验（tmux passthrough / 无协议兜底 / NO_COLOR / 损坏图 / 大图）。
- M31-5：pdf-visual 护栏量化实测——**本机（rustc 1.100 ≥ 1.97）已可执行**（升级前受 1.95 阻塞），仍 deferred 待运行；foot 定性数据已记 NOTES。
- M28-1 Windows junction 回退人工验证；M31-2 装 7z 环境 cab/lzh 提示区分（人工，随 M11-6 模式保留）。
- PROC-1：预算门控与断点落地协议已生效（用户 2026-11-19 确认），为「自动续轮」第 3 条停止条件。

## 下一步

1. **人工验证项（任意环境顺带）**：M11-6 清单 2–6（tmux passthrough / 无协议兜底 / NO_COLOR / 损坏图 / 大图）；M28-1 Windows junction；M31-2 装 7z 环境 cab/lzh 提示；Win2016 冒烟（0.11.0 归档）。
2. **M31-5 本机补验**：rustc 1.100 已解锁，语料量化随时可跑（用户指令）。
3. **后续批次**：待用户需求（无排队任务）。

## 批次关闭记录

- 0.11.0 发布（2026-11-21）：M34 完成后自动发布——tag 首推 windows test 失败（M34-5 测试 runner 长路径截断，短合成路径修复）→ **tag 重指向后 CI 全绿 + GitHub Release + crates.io 上线**（本机凭据，用户部署；`cargo publish` + API 验证）；**0.9.0/0.10.0 经用户决策跳过不再发布**（版本序列 0.8.0 → 0.11.0）。
- M34：主题、预览统一与文档 10 项全部完成（2026-11-21）——M34-1 默认 gruvbox-dark（vifm 高亮组三层解析）、M34-2 捆绑 snowwhite、M34-3 右 pane `/` 搜索（焦点感知）、M34-4 对侧预览统一（preview_path 焦点修复 + render_view_content）、M34-5 frecency 视觉回显、M34-6 `.jar`、M34-7 二进制信息面板、M34-8 GUIDE.md、M34-9 预览尺寸适配（Fit 居中/fit-width/顶部切片）；433 单测 + 9 集成，本机 rustc 1.100 门禁绿（含 pdf-visual）。
- 0.10.0 发布（2026-11-21）：M28–M33 全部完成后自动发布——GitHub Release 三归档发布并验证（CI 5 job 全绿；发布期间修复 windows clippy dead-code 与 M32-3 Windows 盘符路径两缺陷，tag 重指向 2 次，CI 诊断注解通道固化，见 NOTES）；crates.io 停在 0.8.0，0.9.0/0.10.0 待 Windows（用户执行）。
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
- M33：键位模式分层扩展 3 项全部完成（2026-11-20）——M33-1 三层 TOML 解析（ModeLayers）、M33-2 消费接线（visual/view 切换换表；[command] 消费面裁剪见 NOTES）、M33-3 文档收尾；418 单测 + 9 集成。**任务列表（M28–M33）自此全部完成，进入 0.10.0 自动发布。**
- M32：远端健壮性与会话扩展 4 项全部完成（2026-11-20）——M32-1 SFTP 自动重连（is_connection_class + as_any downcast）、M32-2 传输现场 .part（偏移续传裁剪见 NOTES/SPEC）、M32-3 会话持久化扩展（SessionTab.kind + 跳过提示接状态栏）、M32-4 文档收尾；416 单测 + 9 集成。
- M31：文档与验收补齐（2026-11-20）——M31-1～4 完成（README 边界/验收条目/M11-6 清单 6 步/复选框化 137 条）；M31-5 pdf-visual 实测 deferred（本机 rustc 1.95 < 1.97，CI 编译验证持续绿）。
- M29：对比增强 3 项全部完成（2026-11-20）——M29-1 `:compare bycontents`（SHA-256 分块哈希 + 64MB 护栏）、M29-2 dp/do 同步（DiffPut/DiffObtain Action + 可恢复删除 + 操作后重建；`!` 覆盖同步裁剪见 NOTES）、M29-3 文档收尾；399 单测 + 9 集成。
- M28：运维操作补齐 4 项全部完成（2026-11-20）——M28-1 `:link` 符号链接创建（junction 回退；undo_op symlink 感知修复）、M28-2 `:rename` 结构化批量重命名（RenamePreview 预览 + y 应用）、M28-3 `:jobs` 暂停/详情（JobControls/JobProgress）、M28-4 文档收尾；396 单测 + 9 集成；Windows junction 人工验证项随 M11-6 模式保留。
- MISC-1：`rv` 短别名二进制（2026-11-20）——Cargo 双 `[[bin]]` 同 `src/main.rs`（rv `test=false`）、CI release 归档含双二进制、README 三处说明、冒烟测试；385 单测 + 9 集成。**任务列表自此全部完成，进入 0.9.0 自动发布。**
- M27：0.8.0 实测问题与 vifm 视觉对齐 3 项全部完成（2026-11-20）——M27-1 符号链接目录跟随（lstat→path metadata 修正随 M27-3 验证落地）、M27-2 vifm 布局配色（pane_borders=false 默认 + 16 色回退）、M27-3 PDF 前 1–2 页图像可配置（[preview] 表 + 中断提交 3 缺陷修正，见 NOTES）；M27 批次自此全部完成；385 单测 + 8 集成。
- M26：归档创建扩展 5 项全部完成（2026-11-20）——`:archive` 后台作业化（archive_plan/execute_archive_job 分离，对话框携带作业参数）、7z 内建创建（sevenz-rust 0.6）、7z 虚拟浏览（SevenZFs，加密/损坏可读）、外部 CLI 降级（rar/cab/lzh/tar.xz/tar.bz2/tar.zst，PATH 探测 + 输出捕获）、README 归档创建行为表；362 单测 + 5 集成。

## 质量门禁

- 本地：`cargo fmt` → `cargo clippy --all-targets -- -D warnings` → `cargo build` → `cargo test` → `--version` / `--help` 冒烟。
- **当前已知门禁偏差**：本机可全量 `cargo test`（418 单测 + 9 集成，rustc 1.100 实测 2026-11-21）；残留以 CI 为准：① `cargo check --target x86_64-unknown-linux-gnu` 的 aws-lc-sys 交叉构建本机不可行（AUDIT-9 固化进 ubuntu job）。**已闭环**：`cargo check --features pdf-visual` 本机恢复可执行（rustup stable 1.100 ≥ exprim MSRV 1.97，2026-11-21 实测通过）。
- 含 `cfg(unix)` / `cfg(test)` 改动：Windows 本机不编译 cfg(unix) 测试代码——以 CI ubuntu job 或 Linux 开发机全量运行为准（M12-2/M27-3 两度教训）。
- CI：`ubuntu-latest` + `windows-latest` 全绿；tag `v*` 触发三 target release 归档。

## 预算与风险

- 控制原则：一轮一任务，只注入相关 SPEC 片段；同一任务 3 轮不过验收即缩小范围并 `deferred`；一轮结束自动进入下一轮，无需人工确认（AGENTS §3）；**M34 暂缓期间不得自行启动**。
- 主要风险：`pick` 只读红线（结构性保证，回归测试固化）；cfg(unix) 测试代码 Windows 本机不编译——以 CI ubuntu job 与 Linux 开发机全量运行为准（M27-3 连续发现两例潜伏缺陷，见 NOTES）；`pdf-visual` 在 rustc <1.97 的本机不可检查，CI 覆盖。
- 环境：双本机——Windows Server 2016（`x86_64-pc-windows-msvc`；crates.io 凭据所在）+ Linux 开发机（近期会话所在，foot 实测终端，rustup stable 1.100）；其它平台交给 CI（ubuntu-latest / windows-latest）。

## 环境

- 本机：双环境——Windows Server 2016（目标 `x86_64-pc-windows-msvc`；crates.io publish 在此执行）+ Linux 开发机（rustup stable **1.100.0**，`~/.cargo/bin` 优先于 apt 的 /usr/bin 1.95；foot 终端实测）。
- CI：`ubuntu-latest` + `windows-latest`（fmt --check / clippy -D warnings / build --locked / test --locked / pdf-visual check）。
- 发布外循环：`push/tag` → CI 矩阵构建 → 测试 → 归档 → GitHub Release（crates.io 发布沿用 Windows 机器凭据）。

## 恢复方式

1. 读本文件确认当前阶段与任务。
2. 打开 [TASKS.md](TASKS.md)，取第一个未勾选任务——**当前无 `[ ]` 任务：M34 段为 `[~]` 待用户指令，不得自行翻转执行**。
3. 读 [AGENTS.md](../AGENTS.md) §3 执行循环。
4. 只读该任务相关的 [SPEC.md](SPEC.md) 片段与 [ACCEPTANCE.md](ACCEPTANCE.md) 条目。
5. 完成后更新本文件与 TASKS，再提交。
