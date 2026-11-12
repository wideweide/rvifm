# AGENTS.md — rvifm 执行协议

本文件是编码 Agent 的唯一执行协议。需求、决策、任务、验收已经固化在仓库文档中；聊天记录、外部会话和临时讨论**不是**事实来源。

## 1. 必读顺序

每轮开始按以下顺序读取，不要跳读，也不要通读全部文档：

1. [README.md](README.md) —— 项目定位、技术栈、非目标。
2. 本文件 —— 执行规则与硬约束。
3. [docs/STATE.md](docs/STATE.md) —— 当前阶段、当前任务、下一步、阻塞。
4. [docs/TASKS.md](docs/TASKS.md) —— 唯一任务来源，取第一个未勾选任务。
5. [docs/SPEC.md](docs/SPEC.md) 中与该任务相关的片段 —— 用任务 ID 或关键词定位，不通读全文。
6. [docs/ACCEPTANCE.md](docs/ACCEPTANCE.md) 中对应阶段的验收标准。
7. 需要理解“为什么”时再读 [docs/NOTES.md](docs/NOTES.md)。

## 2. 文档职责边界

| 文件 | 回答的问题 | 不负责 |
|---|---|---|
| `README.md` | 项目是什么、为什么、对外定位 | 具体任务顺序 |
| `AGENTS.md` | 每轮怎么执行、什么不能做 | 功能细节 |
| `docs/SPEC.md` | 做什么、做到什么程度 | 任务拆分与进度 |
| `docs/PLAN.md` | 分几阶段、依赖与发布划分 | 单次提交粒度 |
| `docs/TASKS.md` | 下一个可执行任务是什么 | 验收细节 |
| `docs/ACCEPTANCE.md` | 怎样算完成 | 任务排序 |
| `docs/STATE.md` | 现在跑到哪、下一步是什么 | 需求定义 |
| `docs/NOTES.md` | 为什么这么定、放弃了什么 | 当前进度 |

冲突处理优先级：**SPEC 的“做什么” > NOTES 的“为什么” > PLAN 的顺序 > TASKS 的勾选状态 > STATE 的位置记录**。若仍冲突，停止该任务，把冲突写入 `docs/NOTES.md` 的“待定事项”，不要自行发明需求。

## 3. 每轮执行循环

严格按以下步骤，一轮只做一个任务：

1. 读 `docs/STATE.md`，确认当前阶段和当前任务。
2. 从 `docs/TASKS.md` 取**第一个未勾选任务**；若 `STATE.md` 的任务与之一致，直接开始；不一致则以 `TASKS.md` 为准并修正 `STATE.md`。
3. 只注入该任务相关的 `SPEC.md` 片段和 `ACCEPTANCE.md` 条目。
4. 实现该任务，不顺手做相邻任务。
5. 依次执行质量门禁：
   - `cargo fmt`
   - `cargo clippy -- -D warnings`
   - `cargo build`
   - `cargo test`
   - 可执行程序至少验证 `--version` / `--help`；涉及 TUI 的改动按 `ACCEPTANCE.md` 做手工验证
6. 对照 `docs/ACCEPTANCE.md` 对应阶段自检；未通过不得提交。
7. 更新文档：
   - `docs/TASKS.md`：勾选该任务；
   - `docs/STATE.md`：更新当前阶段、当前任务、已完成、下一步、预算；
   - `docs/NOTES.md`：记录新决策、范围收缩、踩坑（仅在确有内容时）。
8. `git commit`，提交信息格式：`<task-id>: <summary>`，例如 `M0-1: bootstrap cargo project`。
9. 若该阶段所有任务完成，跑一次完整阶段验收，更新 `docs/PLAN.md` 的阶段状态，然后进入下一阶段。

### 迭代上限

同一任务连续 3 轮仍无法通过验收：

1. 缩小该任务范围，写清“做了什么、没做什么、为什么”；
2. 在 `docs/NOTES.md` 记录；
3. 在 `docs/TASKS.md` 把该任务标记为 `deferred`；
4. 继续下一个任务，不硬磕。

### 自动续轮（2026-11-12 增补）

一轮提交完成后，若同时满足以下三个条件，**自动进入下一轮，无需人工确认**：

1. 全部质量门禁一次通过（fmt / clippy --all-targets / build / test 全绿）；
2. 过程无未解决异常（无必须回退的缺陷、无偏离 SPEC 的实现）；
3. 无必须人工执行的步骤（如 crates.io 发布、真实终端手工验证）。

任一条件不满足时暂停并报告，等待人工决策。crates.io 发布已获持久授权（沿用本机已存凭据），不视为必须确认的步骤。

## 4. 任务粒度

- 一个任务应当能在**一次提交**内完成。
- 任务必须可独立验收；无法独立验收的，先拆任务。
- 禁止一次勾选多个任务、并行实现、跳过前置任务。
- 禁止预先把后续里程碑的代码塞进当前阶段，除非 `SPEC.md` 明确要求。

## 5. 硬约束（违反即回退）

### 5.1 产品边界

- 项目名、crate 名、二进制名统一为 `rvifm`；定位是“参考 vifm，非 fork，非行为级兼容”。
- **不实现 marks**（`m` + 字母标记路径、`'` + 字母跳转）；路径跳转由 frecency 的 `pick` 覆盖。
- **`t` 标记（tag）必须保留**，它用于 M3 批量操作，与 mark 语义不同，不得混淆或移除。
- **不实现 Vim 脚本引擎、插件系统、FUSE、archivemount、fzf、SQLite、完整 Vim 引擎**。
- **不把 Überzug++、Chafa 作为必需依赖或编译期依赖**；它们只能作为用户显式配置的可选外部工具。
- **不写入归档内成员**，不做嵌套归档递归浏览。
- **不保证 PDF 的零配置视觉预览**；视觉预览依赖可选的 Poppler/MuPDF。
- **不做内嵌视频播放**。

### 5.2 状态与撤销

- 多标签从 M1 起就是核心能力；每个 tab 独立持有 `Location`、光标、滚动位置、搜索状态。
- 撤销栈是**全局的**，不是 per-tab；`UndoableOp` 必须记录所属 tab 与路径，跨 tab 撤销能正确定位。
- 删除优先走可恢复路径（回收站或应用内暂存），保证 `u` 可逆；不可逆操作必须明确提示。
- 归档浏览只读，不进入撤销栈；不要为归档内修改扩展撤销逻辑。

### 5.3 frecency

- `pick` 是**纯只读选择器**：不得写 redb、不得更新 `global_tick`、选中后只输出到 stdout 即退出。
- `record` 是**唯一写入入口**：只在 shell 真正 `cd` 成功、或文件管理器真正跳转/打开成功后调用。
- 存储采用 `events`（append-only）/ `snapshot`（compaction 产物）/ `meta`（`global_tick`）三表；**没有后台守护进程**。
- 评分在 `pick` / TUI 启动时通过 `snapshot + events` 重放计算；`record` 不做评分。
- compaction 由读方启动时惰性触发，或由 `rvifm maintenance compact` 手动执行；不引入定时器。
- 并发用 redb 短事务 + 有限重试（例如 3 次、退避 10ms）；失败降级到 `events.pending` 纯文本文件，下次启动合并。
- 禁止引入 SQLite、fzf 二进制或外部 shell 预览命令。

### 5.4 键位

- 采用**动作驱动**的声明式键位系统；`Action` 枚举承载文件操作语义。
- 默认键位与 vifm 对齐：`yy` → `Yank`，`P` → `PutMove`，`p` → `PutCopy`，`dd` → `Delete`。
- `PutMove` 与 `PutCopy` 是独立 Action，不得在键位解析层合并。
- 支持多键序列（`yy`、`gg`、`gt` 等）与可配置超时；模式状态（Normal / Visual / View / Command-line）由 `rvifm` 持有。
- 用户配置 TOML 分层：`prepend_keymap` 覆盖默认、`append_keymap` 补充、`keymap` 完全替换。
- 禁止为文件管理器引入完整 Vim 引擎（`narwhal-vim` / `vimcore` / `hjkl-engine`）。

### 5.5 预览与打开

- 图片预览必须走 `ratatui-image` 的协议探测与回退链：原生协议（Kitty/iTerm2/Sixel）→ halfblocks（无条件可用）→ 可选外部字符画工具。
- **禁止为特定终端写硬编码分支**；协议差异交给 `ratatui-image`。
- 文本预览复用 `PreviewRenderer`；`pick` 的预览与 TUI 预览必须同源，禁止单独实现一套。
- 文件打开默认走系统 opener（Linux `gio open` → `xdg-open`；macOS `open`；Windows `start`）；不得要求用户先配置才能打开文件。
- opener 配置分层必须与键位系统同构：`prepend_rules` / `append_rules` / `rules`。
- Office 预览只能用纯 Rust 解析库，禁止引入外部命令行工具。
- PDF 预览按“文本提取（`pdf-inspector`）→ `exprim` 栅格化（仅 `pdf-visual` feature）→ `pdftoppm`/`mutool` → 回退提示/系统 opener”顺序探测，不得跳过文本提取直接进入视觉渲染。
- 不把 `tdf` / `pdfium-render` 作为编译期依赖；`exprim` 只能以非默认 `pdf-visual` feature 引入，固定 `=0.4.0`，默认构建的依赖树不得包含它。
- 启用 `pdf-visual` 时：导出像素上限限制在预览窗格级别（200 万–400 万像素）、限制输入 PDF 文件大小、release profile 保持 `panic = "unwind"`（不得设为 `abort`）。

### 5.6 视觉

- 图标使用 `devicons`，默认 Dark 主题，支持 Light 切换。
- 主题使用 `opaline`，TOML 定义，内置主题可直接选用。
- 文件列表、预览、归档浏览共用同一套图标/主题渲染。
- 终端不支持 Nerd Font 时图标降级为纯文本，不得 panic。
- 不实现 vifm 风格的图标插件或 `vifmrc` 注入；不嵌入 SVG 或字体文件。

### 5.7 权限与提权（M12，2026-11-12 增补）

- M12 提权必须是操作级，禁止将 rvifm 主进程提升为 root。
- 提权失败或取消后，TUI 状态必须与操作前一致，不得残留半完成状态。
- 优先复用 `sudo_prompt` 或自建 askpass，不引入 pkexec 作为硬依赖。

## 6. 质量门禁

每个任务提交前必须全部满足：

- `cargo fmt` 无改动；
- `cargo clippy -- -D warnings` 通过；
- `cargo build` 通过；
- `cargo test` 通过；
- 程序可启动、可退出、无 panic；
- 本任务对应的 `ACCEPTANCE.md` 条目通过；
- `docs/TASKS.md` 与 `docs/STATE.md` 已更新。

发布门禁（M6 及后续）：

- CI 在 `ubuntu-latest` 与 `windows-latest` 全绿；
- 两端 `--release` 构建成功并产出归档；
- Windows 产物在 Win2016 手动冒烟通过；
- Linux 产物在 CI 或容器中 `--version` 通过。

## 7. 平台与测试规则

- 本地开发环境：Windows Server 2016；本地验证目标是 `x86_64-pc-windows-msvc`。Linux 目标交给 CI。
- **ZCode 是 Electron 应用，官方要求 Windows 10/11，在 Win2016 上可能无法启动**。优先使用 VS Code + `rust-analyzer`，或纯命令行 + 任意编辑器；这不影响项目内容。
- M0 就配置双平台 CI（`ubuntu-latest` + `windows-latest`），不要等到发布前才加。
- CI 无 TTY：**集成测试禁止依赖真实终端**，TUI 渲染使用 `ratatui` 的 `TestBackend`；冒烟测试只跑 `--version` / `--help`，不进入交互模式。
- 路径内部统一用 `/`；`Location::Archive { inner }` 在渲染、比较、索引前必须规范化，避免 `a\b` 与 `a/b` 分叉。
- 测试 fixture 不要硬编码 `/tmp`，使用 `tempfile` crate。
- 注意 Windows 行为差异：文件名大小写不敏感、符号链接权限、`trash` crate 的回收站行为、CRLF/LF。
- 不引入 `unsafe`，除非有明确必要并在代码中注释原因。

## 8. 文档维护规则（蒸馏规则）

新增或修改结论时，按信息类型投递到唯一位置，不要重复散落：

| 信息类型 | 去向 |
|---|---|
| **REQ**：要做什么功能 | `docs/SPEC.md` 的 MUST / SIMPLIFIED |
| **CUT**：明确不做什么 | `docs/SPEC.md` 的 OUT_OF_SCOPE + `docs/NOTES.md` 决策 |
| **WHY**：为什么这么定、评估过什么 | `docs/NOTES.md` |
| **SEQ**：先做哪个、依赖关系 | `docs/PLAN.md` + `docs/TASKS.md` |
| **HOW**：怎么验收 | `docs/ACCEPTANCE.md` |
| **NOISE**：讨论过程、备选方案、客套、举例 | 丢弃 |

判断标准：一句话删掉后，执行者会做错事或不知道边界，它就不是 NOISE。

## 9. 启动自检

在正式执行 `M0-1` 之前，先输出以下三项，供人工核对：

1. 当前阶段与当前任务；
2. 第一条要执行的验收标准；
3. 三条不可违反的硬约束（从第 5 节任选）。

如果复述与文档不一致，先修文档，不要开始写代码。

## 10. 起点

- 当前阶段：`M0`
- 当前任务：`M0-1`（初始化 Cargo 项目与目录结构）
- M0 结束前必须让 CI 在 `ubuntu-latest` 和 `windows-latest` 上全绿。
- 预算优先保证 `M0–M2` 闭环，尤其是 `u` 撤销；`M3–M11` 可简化，但不得跳过 `docs/STATE.md` 更新。
