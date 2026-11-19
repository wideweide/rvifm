# STATE — 当前状态

> 本文件是中断恢复的锚点：只回答“现在跑到哪、下一步是什么”。需求定义见 [SPEC.md](SPEC.md)，任务清单见 [TASKS.md](TASKS.md)，决策理由见 [NOTES.md](NOTES.md)，评估结论见 [REVIEW.md](REVIEW.md)。详细历史以 git log 为准，本文件不重复流水账。

- **更新时间**：2026-11-19
- **当前阶段**：AUDIT 评估报告整改（**已确认，执行中**）；M26 归档创建扩展已确认启动，排在 AUDIT 与 M25-2 之后
- **当前任务**：AUDIT-1 `pick` 只读性修复（TASKS 中第一个未勾选任务；方案已拍板：pending 合并移出 pick / 只读 open）
- **阶段状态**：M0–M25 关闭（M25-2 marks 已定案由 frecency 实现并重开任务；M11-6 真实终端验证 deferred）；v0.8.0 已发布，0.9.0 在任务列表全部完成后自动发布
- **基线**：不固化 commit hash，以执行时的 `HEAD` 为准；提交文档后 HEAD 变化属正常，无需回改本文档

## 进行中 / 暂停 / 延期

- AUDIT-1～AUDIT-12：评估报告整改（见 [REVIEW.md](REVIEW.md)），**用户已确认执行**；AUDIT-1 方案已拍板（NOTES §10）。
- M26-1～M26-5：已确认启动，排在 AUDIT 与 M25-2 之后。
- M25-2（重定义）marks/bookmark 由 frecency 实现：排在 AUDIT 之后；keyword 长度分工已确认（单字符 `'<char>` 导航 / 多字符 `o`/`:j`/`:J` 搜索高优先级匹配）。
- M11-6：Alacritty 真实终端图片显示验证，需人工环境，deferred。

## 下一步

1. 执行 AUDIT-1：`pick` 改用只读 open；`events.pending` 合并只在 `record` / TUI 启动路径执行；补“存在 pending 时 pick 不写 DB、不推进 tick”回归测试。验收见 ACCEPTANCE §26。
2. 按 TASKS 顺序推进 AUDIT-9（CI 门禁增强）→ AUDIT-8 → AUDIT-2/3/4 → AUDIT-5 → 其余。
3. AUDIT 批次完成后再按用户指令启动 M26-1（`:archive` 迁移 M13-8 Job 模型）。
4. 全部任务完成后自动发布 0.9.0（无需人工决策）。

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
- 含 `cfg(unix)` / `cfg(test)` 改动须额外跑 `cargo check --target x86_64-unknown-linux-gnu --all-targets`（M12-2 教训）。
- CI：`ubuntu-latest` + `windows-latest` 全绿；tag `v*` 触发三 target release 归档。
- CI 增强建议（非违规）：clippy 未带 `--all-targets`；Linux target cross-check 目前只是本地门禁，已登记 AUDIT-9。

## 预算与风险

- 控制原则：一轮一任务，只注入相关 SPEC 片段；同一任务 3 轮不过验收即缩小范围并 `deferred`；一轮结束自动进入下一轮，无需人工确认（AGENTS §3）。
- 主要风险：文档与实现漂移（本次评估发现，已部分修正）；`pick` 只读红线；`app.rs` 上帝文件。
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
