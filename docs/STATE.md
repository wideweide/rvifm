# STATE — 当前状态

> 本文件是中断恢复的锚点：只回答“现在跑到哪、下一步是什么”。需求定义见 [SPEC.md](SPEC.md)，任务清单见 [TASKS.md](TASKS.md)，决策理由见 [NOTES.md](NOTES.md)，评估结论见 [REVIEW.md](REVIEW.md)。详细历史以 git log 为准，本文件不重复流水账。

- **更新时间**：2026-11-19
- **当前阶段**：AUDIT 评估报告整改（**已确认，执行中**）；M26 归档创建扩展已确认启动；M27（0.8.0 实测问题与 vifm 视觉对齐）已确认按序排队
- **当前任务**：AUDIT-14 测试环境隔离（TASKS 中第一个未勾选任务；AUDIT-1 已于 2026-11-20 完成）
- **阶段状态**：M0–M24 名义关闭，但复核发现 M9 主题选择、M11 opener/PDF 视觉/pick 预览为“引擎存在、未接线”（已转 AUDIT-13/7/2/3），**整改完成前不计为实质完成**；M25 主体完成但 **M25-2（marks）未实现**；M11-6 真实终端验证 deferred；M27 已确认按序排队；v0.8.0 已发布，0.9.0 在任务列表全部完成后自动发布
- **基线**：不固化 commit hash，以执行时的 `HEAD` 为准；提交文档后 HEAD 变化属正常，无需回改本文档

## 进行中 / 暂停 / 延期

- AUDIT-1～AUDIT-17：评估报告整改（见 [REVIEW.md](REVIEW.md)），**用户已确认执行**；AUDIT-1 方案已拍板（NOTES §10）。第二轮复核新增 13（主题接线）/14（测试隔离）/15（pick CLI）/16（frecency 一致性）/17（归档上限）。
- 已核实的“未接线”能力：opener 整层（AUDIT-7）、主题选择（AUDIT-13）、PDF 视觉链（AUDIT-2）、pick 预览（AUDIT-3）、`pick --topic`（AUDIT-15）；关闭对应阶段时不得再以“模块存在”视作完成。
- M26-1～M26-5：已确认启动，排在 AUDIT 与 M25-2 之后。
- M27-1～M27-3：0.8.0 实测问题与 vifm 视觉对齐，**已确认按序排队**（M27-1 无前置；M27-2 依赖 AUDIT-13；M27-3 依赖 AUDIT-2）。
- PROC-1：预算门控与断点落地协议已写入 `AGENTS.md` §3 与 `ACCEPTANCE.md` §1，**已生效（用户 2026-11-19 确认）**，为「自动续轮」第 3 条停止条件。
- M25-2（重定义）marks/bookmark 由 frecency 实现：排在 AUDIT 之后；keyword 长度分工已确认（单字符 `'<char>` 导航 / 多字符 `o`/`:j`/`:J` 搜索高优先级匹配）。
- M11-6：Alacritty 真实终端图片显示验证，需人工环境，deferred。

## 下一步

1. 按 TASKS 现顺序执行：AUDIT 批次 → M25-2 → M26 → M27 → 杂项（MISC-1）；AUDIT-1 已完成，当前做 AUDIT-14。
2. 执行 AUDIT-14（测试隔离）：`TMUX`/`STY` 等环境变量不得改变测试结果，`m18_4_bang_routes_and_config` 在 tmux 内外均稳定（见 SPEC AUDIT-14 / ACCEPTANCE §26）。
3. 按 TASKS 顺序推进 AUDIT-18（本机测试失败整改）→ AUDIT-9（CI 门禁增强）→ AUDIT-15（pick CLI）→ AUDIT-7/13（opener/主题接线）→ AUDIT-2/3/4/16 → 其余；M27-2/3 在 AUDIT-13/2 之后。
4. AUDIT 批次完成后再按用户指令启动 M26-1（`:archive` 迁移 M13-8 Job 模型）。
5. 全部任务完成后自动发布 0.9.0（无需人工决策）。

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
- M25：会话持久化、归档创建/校验、异步 git 状态列与 `gi`、frecency 通用化评估（M25-2 marks 重开）；336 单测 + 6 集成。

## 质量门禁

- 本地：`cargo fmt` → `cargo clippy --all-targets -- -D warnings` → `cargo build` → `cargo test` → `--version` / `--help` 冒烟。
- **当前已知门禁偏差**：① 在 `TMUX`/`STY` 存在的 shell 中 `cargo test` 会因 `m18_4_bang_routes_and_config` 失败（AUDIT-14 整改）；② 本机 HEAD 上 `m249_jobs_overlay_render_selection_and_cancel_state`（作业行断言依赖宿主路径长度）与 `m253_archive_zip_roundtrip_and_undo`（第二次 undo 未移除归档）失败，均与 AUDIT-1 无关（stash 验证），AUDIT-18 整改。AUDIT-14/18 完成前本地门禁：`env -u TMUX -u STY cargo test -- --skip m249_jobs_overlay --skip m253_archive_zip`（其余 326 单测 + 3 集成全绿）。③ Linux target cross-check 自 russh（M19-3b）起本机无法构建 aws-lc-sys（无 cross gcc），该门禁由 CI 承担（现状确认）。
- 含 `cfg(unix)` / `cfg(test)` 改动须额外跑 `cargo check --target x86_64-unknown-linux-gnu --all-targets`（M12-2 教训）。
- CI：`ubuntu-latest` + `windows-latest` 全绿；tag `v*` 触发三 target release 归档。
- CI 增强建议（非违规）：clippy 未带 `--all-targets`；Linux target cross-check 目前只是本地门禁，已登记 AUDIT-9。

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
