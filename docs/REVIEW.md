# REVIEW — 项目全面评估报告（2026-11-19）

> 本报告由一次独立评估生成，评估时基线为 `9fd484d`（当时的 `main` 与 `origin/main`；此 hash 仅为评估快照，不代表当前 HEAD）。报告是**评估结论**；结论中的 REQ / CUT / WHY / SEQ / HOW 已按 AGENTS §8 投递到对应 harness 文档。AUDIT 批次、AUDIT-1 方案、M26 启动及所有待定项已于 2026-11-19 获用户确认，状态已同步至各文档。本次改动**未提交**。
>
> **版本管理**：本文件是**当前评估报告**，也是首份报告，现在**不需要**预先建立归档。下一次评估开始时，先把本文件按 `docs/reviews/REVIEW-<YYYY-MM-DD>.md` 命名归档（日期取原报告生成日 2026-11-19），再用本文件名写新报告；以后每次评估依此轮换。历史记录持久保存在 `docs/reviews/`，不覆盖。

## 1. 结论摘要

| 维度 | 评价 | 说明 |
|---|---|---|
| harness 机制 | **专业，可执行** | 文档职责边界、一轮一任务、质量门禁、3 轮降级、蒸馏规则均完整；在同类个人项目中属于高水准。 |
| 文档一致性 | **不合格（P0）** | `README` / `STATE` / `PLAN` / `SPEC` / `NOTES` 之间大面积状态漂移；`STATE.md` 存在双 header 与互相矛盾的“下一步”。这是当前最危险的缺陷，因为它破坏中断恢复能力。 |
| 架构与代码组织 | **待优化（P1）** | `src/app.rs` 15,022 行（占源码 55%），生产段与测试段各约一半；测试 52% 集中在该文件，属典型“上帝文件”。 |
| 硬约束合规 | **存在 1 处 P0 违反 + 5 处实现缺失** | `pick` 在 pending 合并路径写 redb 并推进 `global_tick`；PDF 视觉探测链、pick 预览同源、惰性 compaction、键位超时、opener `rules` 层未落地。 |
| 功能覆盖 | **良好** | M0–M25 主体功能已实现，M26 已登记待确认；与 vifm/ranger/yazi 的定位差异清晰。 |
| 测试与 CI | **基本合格** | 346 个测试；CI 覆盖双平台与发布归档，与现行 AGENTS 门禁一致。未把 AGENTS §6 的 Linux target cross-check 放进 CI（该项目前是本地门禁），且 clippy 未覆盖 all-targets，属建议增强而非违规。 |
| 发布工程 | **基本合格** | `v0.8.0` tag 存在，`Cargo.lock` 已入库，`Cargo.toml` / README 版本一致；已改为任务全部完成后自动发布 0.9.0。 |

**总体判断**：项目的技术选型、边界控制和 harness 设计是专业的，代码具备可持续演进的基础；但**文档已严重落后于代码**，且有 1 处硬性红线违反和若干“文档声称已实现、实际未接线”的能力缺口。建议优先修复 P0 项，再进入 M26。

## 2. 评估范围与方法

- 文档：完整读取 `README.md`、`AGENTS.md`、`docs/STATE.md`、`docs/TASKS.md`、`docs/PLAN.md`，并抽取 `docs/SPEC.md`、`docs/ACCEPTANCE.md`、`docs/NOTES.md` 的相关章节。
- 代码：`src/` 26 个 `.rs` 文件（27,104 行）、`tests/cli.rs`、`examples/sftp_spike.rs`、`.github/workflows/ci.yml`、`.cargo/config.toml`、`Cargo.toml`。
- 方法：静态阅读 + 符号/行号检索 + 双方提交历史核对；未运行完整质量门禁（本次为评估任务，不改动代码）。

## 3. 文档治理评估

### 3.1 做得好的部分

- `AGENTS.md` §2 文档职责表与 §8 蒸馏规则清晰，`TASKS.md` 条目严格保持“一行摘要 + 状态 + SPEC 引用”。
- `ACCEPTANCE.md` 每个里程碑有可勾选条目，§16 人工验证清单覆盖真实终端场景。
- `NOTES.md` 的决策记录（WHY）保留了关键技术判断的来龙去脉，例如 M23-12 git 评估、M26 crate 核实。

### 3.2 P0 缺陷

1. **`STATE.md` 结构性损坏**：文件同时存在两个 `# STATE —`（第 1 行与第 144 行）；第 178–184 行的“下一步”仍是 `M10-1` / `M11-1` / `M12-1`，与第 153–155 行“当前 M26-1”直接冲突；“进行中：无”与 M26-1 待确认冲突；“已完成”中仍有“M12 未实现”。该文件是 AGENTS 定义的中断恢复锚点，当前状态会误导 Agent 执行已完成任务。
2. **marks 决策在 5 个文档中互相矛盾**：AGENTS §5.1 与 SPEC §25 声明“marks 是独立模块（M25-2）”；README 第 188/215/230 行仍写“marks 已移除 / 不实现”；SPEC 第 18/66/129/453/750/770 行同时存在“不实现”和“立项”；NOTES §4.3（第 64 行）与新决策（第 588/635 行）并存。M25-2 已暂停，`'` 与 quickjump 的键位归属未拍板，但旧结论未清理。
3. **README 对外事实失真**：状态行停在 v0.8.0，里程碑表止于 M22“进行中”；技术栈表列 `keymap-core`、`skim`、`tree-sitter`（实际为自写 Trie、自绘 picker、syntect）；快速开始仍写 `v0.1.0 不包含 pick/record`；clone 地址为 `github.com/<your-org>/rvifm` 占位符。

### 3.3 P1/P2 缺陷

- `PLAN.md` 第 38 行多余空行把里程碑表拆成两张表；§3 阶段依赖未覆盖 M12–M26；§4 发布划分只写 v0.1.0 与 M7–M11；§5 预算/风险仍是早期版本（skim、keymap-rs 降级已不适用）。
- `SPEC.md` 章节顺序错乱（§4 中 M26 在 M25 之前，M22/M23/M18–M20 插在 M17 之后）；§6.2 仍列已定稿的 keymap-rs、tar 变体、性能阈值。
- `ACCEPTANCE.md` §1 原本与 AGENTS §6 一致；本次仅把 Linux target cross-check 明确标注为“本地门禁”，避免被误读为 CI 已执行。CI 未覆盖该项与 clippy all-targets 属增强建议（AUDIT-9），非规范违反。
- `src/help.txt` 缺 `:find` / `:grep`，但 README 状态行已宣称该能力。

## 4. 代码与架构评估

### 4.1 结构

- 顶层模块按领域划分（`config` / `keymap` / `ops` / `tab` / `preview` / `archive` / `frecency` / `remote/*`），命名可读。
- `src/app.rs` 15,022 行（生产约 7,400 行、测试约 7,600 行），承载输入分发、命令执行、事件循环、渲染、远程会话、后台作业；建议拆为 `app/keys.rs`、`app/exec.rs`、`app/render.rs`、`app/jobs.rs`、`app/preview.rs`，测试随职责迁移。
- `src/frecency.rs` 1,516 行混合数据模型、redb 写入、CLI 子命令与 TUI pane 渲染，是第二优先级拆分对象。

### 4.2 稳健性

- 生产代码显式 `panic!` 为 0，但仍有 9 处 `unwrap` / `expect` / `unreachable`：
  - `src/frecency.rs:293,302`：全局设置 Mutex 被 poison 时 panic；
  - `src/app.rs:304,323,2993,5314`：依赖不变量的补全/复制/作业查表；
  - `src/app.rs:1104`：上传监听线程 spawn 失败直接 panic；
  - `src/highlight.rs:41`：syntect 默认主题集为空时 panic。
- 静默错误吞掉：`src/tab.rs:1186-1190,1253-1255` 忽略刷新失败；`src/session.rs:66` 序列化失败写空文件；`src/frecency.rs:597-604,620-622` 降级写失败仍返回成功；`src/remote/known_hosts.rs:133,145` 忽略目录创建与 0600 权限设置失败（安全相关）。
- `unsafe` 仅 `src/privilege.rs:36-52` 的 `mkfifo` 薄封装，范围可控，但缺 AGENTS §5 要求的必要性注释，且 `to_string_lossy()` 可能构造错误路径。

### 4.3 重复与死代码

- `src/opener.rs:118` 与 `src/tab.rs:1503` 各有一套 glob 匹配，能力不同（`?` 支持不一致）。
- 配置解析失败回空值的逻辑在 `src/config.rs:174-180` 与 `src/remote/config.rs:57-61` 重复。
- 12 个模块有 `#![allow(dead_code)]`，`src/tab.rs:17,24` 对实际大量使用的 `Location::Archive/Remote` 也加了该属性，掩盖真实死代码。
- 命名：`set_keyword_at` / `set_keyword_now`、`flush_visits_at` / `flush_visits` 的 `at`/`now` 后缀实际表达“路径可注入 vs 默认路径”，语义误导。

### 4.4 测试

- 共 346 个测试（340 单测 + 6 集成），但 178 个在 `app.rs`；`keymap.rs` 4 个、`state.rs` 4 个、`session.rs` 2 个，覆盖偏薄；`main.rs` 无单测，`record/pick/maintenance compact` 子命令分派仅靠 CLI 冒烟；`remote/sftp.rs` 真实后端测试在无 sshd 时自动跳过。

## 5. 硬约束与规范冲突（按严重度）

| 级别 | 问题 | 证据 | 建议 |
|---|---|---|---|
| **P0** | `pick` 条件性写 redb：存在 `events.pending` 时，`open_store()` 调 `merge_pending()`，后者 `append_event()` 推进 `global_tick` 并写 events | `src/frecency.rs:460-487`、`src/store.rs:103-139`；违反 AGENTS §5.3 与 SPEC §4.6:169 | 把 pending 合并移出 `pick` 路径（仅 `record`/TUI 启动执行），或为 pick 提供只读 `Store::open_readonly`；因 §5.3 内部“下次启动合并”措辞冲突，已入 NOTES §8 待决策 |
| **P0** | `STATE.md` 双 header + 过期下一步 | `docs/STATE.md:1,144,178-184` | 本次已重写为单一锚点 |
| **P1** | 文档声称 PDF 视觉探测链完整，生产只做文本提取；`find_external_renderer` / `render_page` 仅测试调用 | `src/pdf.rs:64-71,123`；`src/preview.rs:95-112` | 接入 exprim → pdftoppm/mutool 回退，或修订 README/AGENTS 表述 |
| **P1** | `pick` 未接入 `PreviewRenderer`，“TUI/pick 同源”未落地 | `src/frecency.rs` 无预览调用；`src/preview.rs:207` 仅测试调用 | 明确 pick 无预览，或补接线 |
| **P1** | 惰性 compaction 未实现（仅手动命令） | `src/frecency.rs:460-464`、`main.rs:52-55` | 在 TUI 启动路径补阈值检查（5000 条），pick 不触发 |
| **P1（建议增强，非违规）** | CI 未运行 Linux target cross-check；clippy 未覆盖 all-targets | `.github/workflows/ci.yml:27-28`；AGENTS §6 的 cross-check 是本地门禁 | AUDIT-9：按需加入 CI；`ACCEPTANCE.md` §1 保持与 AGENTS 一致 |
| **P1** | 多键序列可配置超时未实现 | `src/keymap.rs:290-341`、`src/config.rs` 无 timeout | 登记 AUDIT-6 |
| **P1** | opener 缺 `rules` 完全替换层（与键位分层不同构） | `src/opener.rs:85-90` | 登记 AUDIT-7 |
| **P2** | `.cargo/config.toml` 全局设置 `AWS_LC_SYS_PREBUILT_NASM=1` | `.cargo/config.toml:5-6` | 评估是否改为 CI/文档级配置，避免影响其他平台 |

## 6. 待追加功能点

已确认启动（M26）：`:archive` 后台作业化、7z 内建创建、7z 只读浏览、外部 CLI 降级、README 格式行为表。

本次评估新增登记（`AUDIT` 批次，**已确认执行**；AUDIT-1 方案已拍板）：

1. `pick` 只读性修复与 pending 合并职责重划。
2. PDF 视觉探测链接线（或文档降级）。
3. pick 预览同源接线（或文档降级）。
4. 惰性 compaction。
5. `app.rs` 拆分与测试迁移。
6. 键位多键序列超时配置。
7. opener `rules` 完全替换层。
8. 生产 panic 面清理与错误上浮。
9. CI 门禁增强（clippy all-targets、Linux cross-check 入 CI）。
10. 统一 glob 匹配与配置回退实现。
11. `help.txt` 与 README 功能同步。
12. 关键模块（keymap/state/session/main 子命令分派）测试补强。

既有延期项：M11-6 真实终端图片验证。M25-2 已定案由 frecency 实现并持久化（keyword 长度分工已确认，见 SPEC §25）。

## 7. 缺陷与待优化模块（优先级顺序）

1. **文档层**：`STATE.md`（P0，已修）、README（P0，已修）、PLAN/SPEC/NOTES（P1，已修）。
2. **frecency 层**：`pick` 只读性、惰性 compaction、错误吞掉、命名。
3. **app 层**：上帝文件拆分、unwrap/expect 清理、线程 spawn 失败降级。
4. **预览层**：PDF 视觉链、pick 预览同源、syntect 主题集失败降级。
5. **工程层**：CI 门禁、`dead_code` 属性清理、测试分布重组。

## 8. 本次已落盘的修改

- 新增本报告 `docs/REVIEW.md`。
- `README.md`：状态/特性/技术栈/里程碑/非目标/对比表/快速开始全部对齐当前事实。
- `docs/STATE.md`：重写为单一、无矛盾的恢复锚点。
- `docs/PLAN.md`：修复表格断裂，补齐 M24–AUDIT 行、发布划分、依赖与风险。
- `docs/SPEC.md`：清理 marks 矛盾表述，更新 §6 待定事项，新增 AUDIT 规格段。
- `docs/NOTES.md`：§4.3 决策反转、§8 待定事项、新增评估决策记录。
- `docs/ACCEPTANCE.md`：通用门禁与 AGENTS §6 对齐，新增 AUDIT 验收段。
- `docs/TASKS.md`：新增 AUDIT 批次（12 项，已确认执行）与 M25-2 重定义条目。
- `AGENTS.md`：§9/§10 从 M0 起点更新为当前状态。

## 9. 限制与未验证项

- 未执行 `cargo fmt` / `clippy` / `build` / `test`（本次不改代码，且为评估任务）。
- 已核验：`v0.8.0` tag 存在（tag 列表 v0.1.0–v0.8.0 连续），`Cargo.lock` 已由 git 跟踪。
- CI 结论基于 workflow 文件静态审查，未访问 GitHub Actions 实际运行结果。
