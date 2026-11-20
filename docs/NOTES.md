# NOTES — 决策记录

本文件记录“为什么这么定、评估过什么、放弃了什么”。不记录当前进度（看 [STATE.md](STATE.md)），不定义功能边界（看 [SPEC.md](SPEC.md)）。

- **来源**：DeepSeek 分享会话 `https://chat.deepseek.com/share/2dpnwrtmiwm1pr21k2`，2026-11-10 蒸馏。
- **原则**：只保留会影响实现和边界的结论；聊天过程、客套、重复举例不进入本文档。

## 1. 背景与可行性判断

- vifm 约 **17 万行 C**，开发约 **17 年**，依赖 ncurses、GLib、libmagic。
- 完整、忠实地重写 vifm 预计需要 **12–18 个月**、**1.5 亿–4 亿 token**，投入产出比不理想。
- 因此放弃“全量移植”，改为 **以 vifm 官方文档/wiki 的特性清单为规格来源，允许简化和重新设计**。
- 项目采用 **harness 模式**：先把需求、决策、任务、验收固化成文档，再让编码 Agent 每轮只执行一个小任务。
- 预算规划已取消（实际消耗远超估算，数量级参考已无意义）。

## 2. 产品定位与命名

- 名称：`rvifm`；crate / binary 同名。
- 定位：**Rust 编写的 vim 风格 TUI 文件管理器；参考 vifm，非 fork，非行为级兼容**。
- 不逐条对齐 vifm man page；允许简化、重新设计和不兼容。
- 短别名 `rv` 已定稿提供（2026-11-19），与 `rvifm` 并存；项目名/crate/主二进制名不变。
- 核心卖点：**零配置可用，定制是加分项**。

## 3. 技术栈选型

| 领域 | 选择 | 理由 / 备注 |
|---|---|---|
| TUI | `ratatui` + `crossterm` | 生态成熟，跨平台 |
| 图标 | `devicons`（`rust-devicons`） | 编译期映射，返回 glyph + 颜色；避免 vifm 插件式方案 |
| 主题 | `opaline` | TOML、token 驱动、20 套内置、渐变、zero unsafe |
| 图片预览 | `ratatui-image` | 统一 Sixel / Kitty / iTerm2 / halfblocks，协议探测成熟 |
| 归档 | `zip`、`tar`、`flate2`；参考 `virtual-filesystem` | 纯 Rust，不依赖 FUSE |
| 存储 | `redb` | 纯 Rust、跨平台、ACID；替代 SQLite |
| 键位 | `keymap-core` + `keymap-seq` + `keymap-config` | 支持序列、模式、TOML 配置；不达标则自写 Trie |
| 模糊匹配 | `skim` 库 | 可嵌入，自带 shell 集成参考 |
| 文本高亮 | `tree-sitter` + `tui-syntax` | 语法高亮 |
| Office | `office_oxide ~0.1.16` | 纯 Rust，三格式统一 IR，2026-11-11 定稿 |
| PDF 预览 | `pdf-inspector =1.25.2`（文本）+ 可选 `exprim =0.4.0`（视觉） | 文本优先；`exprim` 仅非默认 `pdf-visual` feature，2026-11-11 定稿 |
| 配置 | TOML | 不做 Vim 脚本 |

## 4. 核心功能决策

### 4.1 多标签

- 多标签从 **M1** 起就是核心能力，不推迟。
- 每个 tab 独立持有 `Location`、光标位置、滚动位置、搜索状态、历史。
- 撤销栈**全局共享**，不是 per-tab；`UndoableOp` 记录所属 tab 与路径。
- 跨 tab 撤销必须能正确定位；操作发生在非当前 tab 时，撤销后按设计切回或提示。
- 原因：晚加多标签会导致 M2 撤销、M4 搜索、M7 归档浏览全部回头改“当前 tab 上下文”，返工成本高。

### 4.2 撤销

- `u` 撤销核心可逆操作：删除、重命名、移动、新建目录/文件。
- 删除优先走可恢复路径（回收站或应用内暂存），保证 `u` 可逆。
- 连续操作按逆序逐条撤销。
- 不可逆操作入栈时明确标记并提示。
- 重做 `Ctrl-r` 为可选能力，不阻塞 M2。
- 归档内只读，不进入撤销栈。
- **删除可恢复路径选型（2026-11-11，M2-2）**：选**应用内暂存**而非 OS 回收站——`trash` crate 的 restore API 无法精确对齐单次操作（恢复粒度/匹配不可控），应用内暂存能保证 `u` 精确可逆。实现：move 到 `trash_dir`（Windows `%LOCALAPPDATA%\rvifm\trash`，Unix `$XDG_DATA_HOME` 或 `~/.local/share` 下 `rvifm/trash`），跨盘回退 copy+delete；undo = 暂存移回原位（父目录缺失则重建；原位被占用则报错防覆盖）；撤销失败时操作回栈保留可重试。`UndoableOp::Delete` 因此携带 `staged` 字段。
- **TASKS 缺"复制"任务冲突（2026-11-11，M2-4b）**：ACCEPTANCE §4 与 SPEC §5 M2 MUST 均含"复制"，但 TASKS.md M2 清单无对应任务。按 AGENTS.md 冲突优先级（SPEC > TASKS）在 M2-7 轮补实现并登记任务 `M2-4b`（勾选态）。复制 undo 映射为 `Create` 语义（删除副本），`recursive` 标记区分 copy 目录树（整树移除）与 mkdir（仅空目录）。`p`（PutCopy）键位仍在 M10。

### 4.3 marks 与 tag

- **历史决策（2026-11-15～19 之前）**：marks 不实现，路径跳转由 frecency 的 `pick` 覆盖。
- **中间方案（已废弃）**：一度立项 M25-2 为独立模块（不写 redb），后因用户指令反转撤销。
- **当前决策（2026-11-19）**：marks 与 bookmark **统一由 frecency 模块实现并持久化在其 redb 中**，带频率记录与评分排序；不设独立 marks 存储。可见命令：`m<char>` 标记 / `'<char>` 跳转（vifm marks 口径 `[a-z][A-Z][0-9]`）；bookmark 命令按 vifm 对齐（`:bmark` / `:bmarks` / `:bmgo` / `:delbmarks`）。NAV-1 quickjump 与 marks 共用同一条 keyword 链路，无键位冲突。
- **tag 保留**：`t` 标记文件用于 M3 批量操作，与 mark 语义不同，文档与代码必须避免混称。

### 4.4 归档浏览

- 不依赖 FUSE / `archivemount`；内置只读虚拟文件系统。
- 首批 `.zip` / `.tar` / `.tar.gz`；第二阶段 `.tar.xz` / `.tar.bz2` / `.tar.zst`。
- `.7z` / `.rar` 不做虚拟浏览，提示解压。
- `Location` 显式区分物理路径与归档内路径，避免 `a.zip/src/main.rs` 被误判为普通路径。
- `ArchiveFs` trait 抽象 list/read/metadata；`ZipFs` / `TarFs` 分别实现。
- 导航层通过 `Location` 分派，不把 `zip`/`tar` 分支塞进导航核心。
- 打开归档建立成员索引；读取按需解码；内存占用受最大单成员约束。
- 归档内只读，不提供删除/重命名/移动入口。
- 不做嵌套归档递归浏览。
- 可参考 `virtual-filesystem` 的 `ZipFS` / `TarFS`，但导航层仍需包 `Location`。

### 4.5 frecency / bookmark

这是会话中改动最多、最需要防止“想当然”的部分。

**否定方案**

- 不引入 SQLite；不照搬 zoxide 的 SQL 技巧。
- 不依赖 fzf 二进制；不保留 fzf 的 `--preview` 外部命令模式。
- 不引入后台守护进程，不用定时 compaction。

**存储模型**

- 早期曾设计 `record` 写时更新评分 + 读时 aging；后改为 **append-only + 读时重放**，原因是避免读-改-写长事务、降低多进程锁冲突。SPEC 以最终方案为准。
- redb 三表：
  - `events`：append-only 事件 `{path, topic, kind, ts}`，key 为单调 tick；
  - `snapshot`：compaction 产物，key = `path+topic`，value = `{content, keyword, score, tick}`；
  - `meta`：`global_tick`、`last_compaction`。
- `record` **只追加事件并推进 `global_tick`，不评分**。
- `pick` / TUI 启动时读取 `snapshot`，以 `meta.last_compaction` 为增量起点读取 `tick > last_compaction` 的事件，在内存中重放评分；`snapshot` 中每条记录的 `tick` 用于 aging。
- TUI 运行中先在内存更新评分，退出时批量追加事件。
- 原因：避免 `record` 做读-改-写长事务，降低多进程并发冲突，写入路径单一。

**评分**

- EMS 评分 + event clock。
- 读时统一 aging：`score * exp(-λ * (T_now - tick))`，避免“新记录分数很低”的冷启动偏差。
- 分级匹配：书签 > 文件名 > 中间段 > 通用子串；0–7 档，具体档位在 M8-1 固化。
- 书签：`keyword != ''` 作为分级标志；书签参与 EMS 评分；取消书签后保留累积分数正常衰减。
- 1000–2000 条记录量级在内存中重放与排序，不引入 SQL。

**pick / record 契约**

- 子命令：

```text
rvifm pick    → 只读模糊选择器
rvifm record  → 静默记录访问
```

- `pick` 的 stdin/stdout 与 fzf 同构；退出码 0 选中 / 1 取消。
- `pick` **只读**：不写 redb、不更新 tick、选中后不追加事件。
- `record` **唯一写入入口**：只在 shell 真正 `cd` 成功、或文件管理器真正跳转/打开成功后调用。
- 原因：解决“用户在 picker 里浏览但取消”被误记为访问的 bug；记录已发生的访问，而非意图访问。

**compaction 与并发**

- compaction 由 TUI 启动惰性触发（events > 5000 条，2026-11-19 定稿），或 `rvifm maintenance compact` 手动执行；`pick` 不触发（只读红线，见 AUDIT-1）；无定时器、无守护进程。
- 多终端同时 `record` 是唯一真实并发场景；redb 短事务 + 有限重试（例如 3 次、退避 10ms）。
- 重试失败降级到 `events.pending` 纯文本，下次启动合并。
- TUI 运行中另一终端写入的事件，本次会话可能看不到，下次启动生效；这是可接受的一致性模型。

**模糊匹配与预览**

- 优先 `skim` 库嵌入；3 轮不达标降级为 `nucleo` + 自绘 picker。
- `pick` 预览复用 `PreviewRenderer`，不在外部命令里跑预览。
- bash/fish 集成：仅在 `cd` 成功后 `record`；避免失败路径污染评分。

**验证目标**

- 替换 vifm 现有“fzf 薄插件 + SQLite”方案，对比排名稳定性。
- A/B aging 测试：5 次访问 A → 495 次其他 → 1 次 B，B 应排在 A 前。
- 两终端各 100 次 `record`，事件不丢；compaction 前后排名一致。

### 4.6 图标与主题

- 图标 `devicons`：默认 Dark，支持 Light；覆盖常见类型与特殊文件名。
- 主题 `opaline`：TOML、token 驱动、20 套内置、渐变、zero unsafe。
- 备选 `tui-nuances`：90 套主题、WCAG AA；如果主题数量优先可切换。
- 图标层与主题层解耦；`PreviewRenderer`、文件列表、归档浏览共享同一套视觉语义。
- 无 Nerd Font / `NO_COLOR` 时图标降级为纯文本。
- 不实现 vifm 图标插件、不注入 `vifmrc`、不嵌入 SVG/字体。
- 用户主题目录：`~/.config/rvifm/themes/`。

### 4.7 声明式键位

- 采用**动作驱动**：`Action` 枚举承载语义，键位解析与动作执行分离。
- 默认键位与 vifm 对齐：
  - `yy` → `Yank`
  - `P` → `PutMove`
  - `p` → `PutCopy`
  - `dd` → `Delete`
- `PutMove` / `PutCopy` 是独立 Action，不能在解析层合并。
- 语义细节：`yy` 后 `P` 移动、`p` 复制；`dd` 后 `p` 移动被“删除”的文件。
- `Yank` / `Delete` 把路径放入 yank 缓冲区并记录 copy/cut 类型；`PutMove` / `PutCopy` 消费缓冲区，实际移动还是复制由缓冲区类型决定。`dd` 的 `Delete` 同时放入 cut 缓冲区。
- 支持多键序列、前缀树、可配置超时。
- 模式：Normal / Visual / View / Command-line；模式状态由 `rvifm` 持有。
- TOML 分层：`prepend_keymap` 覆盖默认、`append_keymap` 补充、`keymap` 完全替换。
- 优先 `keymap-rs`（`keymap-core` + `keymap-seq` + `keymap-config`）。
- 不达标时降级为 `keybindings` crate 或自写轻量 Trie；不引入完整 Vim 引擎。
- 明确不做：Vim 寄存器、宏、`.` 重复、text objects、Vim 脚本映射。
- 用户只能绑定到内置 Action，不能自定义动作实现。

### 4.8 预览与文件打开

**图片**

- `ratatui-image` 统一协议适配；`Picker::from_query_stdio()` 启动探测。
- 三级回退：
  1. 原生协议：Kitty / iTerm2 / Sixel；
  2. Halfblocks：所有终端无条件可用；
  3. 可选外部字符画工具：用户显式配置。
- 与 yazi 的关键差异：yazi 在无原生协议终端依赖 Überzug++ / Chafa；`rvifm` 的第二级是内建的，任何支持 Unicode 与颜色的终端都有图可看。
- 不把 Überzug++ / Chafa 作为硬依赖。
- 不为特定终端写硬编码分支。
- tmux/Zellij passthrough 不在代码层解决，文档注明所需终端配置。

**文本**

- `tree-sitter` + `tui-syntax` 高亮。
- 目录预览复用文件列表渲染。
- 预览抽象 `PreviewRenderer` 在 M8 抽出，TUI 与 pick 共用，避免两套实现。

**Office（2026-11-11 定稿）**

- 纯 Rust 解析库：`office_oxide`，固定 `~0.1.16`（MIT OR Apache-2.0，MSRV 1.88，20 个依赖/10 个非可选，crate 约 1.2 MB）。
- 一个依赖覆盖 DOCX/XLSX/PPTX：DOCX 段落/run 样式/图片/文本框/表格；XLSX 单元格值/公式/合并单元格/样式/图片；PPTX 文本框/图片字节/表格/备注/OLE。
- 支持文本/Markdown 提取，零外部工具依赖；不保证格式还原，只要求可读。
- 选择理由：三格式共用一套 IR，避免为每种格式各引一个库；`office_oxide` 的结构化数据模型比直接输出 Markdown 的 `undoc` 更适合接入 `PreviewRenderer`。
- **PPTX 退化策略**：图表、SmartArt、组合形状、动画、OLE 等复杂对象跳过对应 shape 继续渲染，不 panic、不整页失败；解析失败或提取为空时显示提示，并允许 `Enter` 用系统 opener。
- 风险：0.1.x API 可能在小版本间变动，依赖固定到 `~0.1.16` 并封装 `OfficePreview` 适配层，避免 API 变更扩散到渲染层；87k 行源码带来可感知编译时间增量。
- 备选：`undoc 0.27.3`（MIT，直接输出 Markdown/text，采用量约为 `office_oxide` 的 1/37）；`docparser-office` 因仓库 404、无法核实维护状态被否。

**PDF（2026-11-11 定稿：文本优先 + `exprim` 非默认视觉后端）**

- 探测链：
  1. `pdf-inspector =1.25.2` 纯 Rust 文本提取（MIT，MSRV 1.88，总下载 139 万，GitHub 19,985 stars）；
  2. `exprim =0.4.0` 纯 Rust 栅格化，仅在 `pdf-visual` feature 下编译；
  3. `pdftoppm`（Poppler）或 `mutool`（MuPDF）外部工具转图像；
  4. 回退提示 + 系统 opener。
- 不保证 PDF 零配置视觉预览（默认构建只承诺文本提取）；这是已声明的边界。
- 不把 `tdf` / `pdfium-render` 作为编译期依赖。
- `pdf-rs` / `lopdf` 不作为文本兜底：两者是底层解析器，不提供高质量文本提取或栅格化；exprim 失败时的正确兜底是 `pdf-inspector` 文本提取。

**exprim 0.4.0 实测数据（查证时间 2026-11-10，x86_64-unknown-linux-gnu，rustc 1.99.1；来源：crates.io API、GitHub API、源码审计、真实编译渲染）**

| 项目 | 实测值 |
|---|---|
| crates.io | 0.4.0，2026-11-10 发布，总下载 7，0 反向依赖，MIT，MSRV 1.97，15 个依赖（14 非可选），crate 1.96 MB |
| GitHub | `liminalism/exprim`，2026-11-10 创建，0 star，24 commit 全部在同日；前身为同作者 Lege 仓库的 `lege-pdf`（2026-07 起） |
| 干净构建 | dev 133s / release 239s，依赖树 107 条 |
| 体积 | release 二进制 strip 后 6.21 MB；空程序基线 0.34 MB，净增约 +5.88 MB |
| 渲染验证 | 最小 PDF → 2,466 B 合法 PNG（200×100 RGB）；真实 PDF（65/33/477 页）open 0.8–21ms，150 DPI 导出 33–139ms，峰值 RSS ≤ 32 MB |
| 像素格式 | `RasterPlane::Rgb8`（w×h×3 连续字节），可直接构造 `image::RgbImage` 喂给 `ratatui-image`，无需 PNG 编解码往返 |
| 稳定性 | 1,000 次随机字节变异 0 panic（887ms，峰值 10.3 MB）；垃圾输入/截断全部 typed error；`MediaBox 1e9×1e9` 被拒绝（默认 1 亿像素预算，约 1ms） |
| 工程保证 | 单维护者；82 处 `unsafe`（均带 `// SAFETY` 注释）；非测试代码 0 处 `unwrap`/`expect`/`panic!`；9 个 fuzz target + chaos 套件，但主 crate 没有 Linux/Windows CI；`jbig2enc-rust`、`jp2lam` 为同作者生态新 crate（均纯 Rust，无 C 编译） |
| 文本质量抽样 | 三份真实 PDF 第一页：exprim 与 pdf-inspector 字符数接近，exprim 更快（样本仅 3 份，不构成全面结论） |
| 工具链 | MSRV 1.97 在本机 rustc 1.99.1 可编译；本机无 rustup，无法验证精确 1.97；Windows（x86_64-pc-windows-msvc）未验证 |

- **定稿结论（2026-11-11）**：保持**文本优先**；`exprim 0.4.0` 以非默认 `pdf-visual` feature 引入，`exprim = { version = "=0.4.0", optional = true }`，`Cargo.lock` 入库；默认构建的依赖树不含 `exprim`。
- **为何不设为默认依赖**：能力与内存边界已实测合格，但供应链信号不足——2026-11-10 首发、crates.io 总下载 7、零反向依赖、单维护者、主 crate 无 Linux/Windows CI、Windows 构建未验证、release 增量约 +5.88 MB。设为默认会把整个用户群绑定到一个发布首日、零采用、无跨平台 CI 的 crate 上；作为可选 feature 既能提供纯 Rust 视觉预览，又不污染默认依赖树。
- **护栏**：启用 feature 时必须限制导出像素上限（预览窗格级，200 万–400 万像素，覆盖 exprim 默认的 1 亿像素预算）、限制输入 PDF 文件大小、release profile 保持 `panic = "unwind"`（不得设 `abort`，exprim 依赖 `catch_unwind` 隔离渲染错误）、补充 CJK/加密/损坏 PDF 回归语料。
- **转正复评条件（不阻塞当前方案）**：观察 4–8 周，满足以下多数条件后再评估是否转为默认视觉后端——① 至少 2 个后续版本且无破坏性 API 变更；② Linux+Windows CI 全绿且有持续 fuzz 记录；③ 出现外部反向依赖或下载量达万级；④ 无未处理的高危安全 issue；⑤ rvifm 自建 CJK/加密/损坏 PDF 语料回归通过。
- spike 产物：`2026-11-10/pdf-exprim-spike/work/`（测试代码与日志，已清理构建产物）。

**许可证与 crates.io 名称（2026-11-11 定稿）**

- **许可证**：`MIT OR Apache-2.0` 双许可，前提是独立重实现。
- **vifm 的许可证事实**：`COPYING` 为 GPL v2；`src/vifm.c` 文件头写明 “either version 2 of the License, or (at your option) any later version”，即 **GPL-2.0-or-later**。
- **参考 vs 衍生的边界**：只参考 vifm 的公开行为、man page、wiki 与用户可见交互，不复制代码、注释、标识符命名体系、数据结构或测试用例，则属于独立实现，可采用 MIT/Apache-2.0；一旦复制、逐行翻译或高度模仿源码，即构成衍生作品，必须整体 GPL-2.0-or-later，双许可声明失效。
- **缓解措施**：实现前记录设计来源；代码评审禁止粘贴 C 代码；在本文档保留“独立实现”决策记录；CI 引入 `cargo-deny` 做许可证审计；同时提供 `LICENSE-MIT`、`LICENSE-APACHE`（建档任务 M0-7）、`NOTICE`/`THIRD_PARTY`（Apache-2.0 依赖如 `ppt-rs` 需保留声明）。
- **未选 GPL 的理由**：GPL 虽可与 vifm 代码复用、与上游社区道义一致，但分发衍生作品必须 GPL、不能闭源嵌入、部分 Rust 生态用户回避、且无法双许可；只有在明确打算大量借用 vifm 实现时才应选它。
- **crates.io 名称**：`rvifm` 当前可用（2026-11-10 查询 `https://crates.io/api/v1/crates/rvifm` 返回 404）；`rvifm-tui`、`rvifm-cli` 同样 404，作为被占用时的备选包名。名称先到先得，`cargo publish --dry-run` 不预留。
- **发布策略**：M0 全部任务完成且双平台 CI 全绿后，人工发布真实的 `0.1.0-alpha.1`（crates.io 凭据不进 CI，发布不在 CI 自动运行）；发布前跑 `cargo package` 检查，发布后用 `cargo install rvifm --version 0.1.0-alpha.1` 验证；不发空包占名。若未来包名被占用，改用 `rvifm-tui` / `rvifm-cli`，通过 `[[bin]] name = "rvifm"` 保持二进制名不变，GitHub 仓库仍叫 `rvifm`。对应任务 M0-8。
- **命名禁忌**：不用 `vifm-rs`——它暗示 fork 或衍生，容易引发许可证误解。

**文件打开**

- 默认层：系统 opener 零配置兜底。
  - Linux：`gio open` → `xdg-open`
  - macOS：`open`
  - Windows：`start`
- 目录在 `rvifm` 内部进入，文件才调用系统 opener。
- TOML 分层与键位系统同构：
  - `prepend_rules`：覆盖默认，优先命中；
  - `append_rules`：补充规则，默认之后；
  - `rules`：完全替换默认规则表。
- 规则支持 `mime` 与 `name`，按顺序取第一个命中；`mime` 优先，`name` 回退。
- 原因：普通用户不配置也能用；高级用户学一次分层模型即可同时理解键位和打开规则。

## 5. 平台与 CI

- Linux 与 Windows 为优先目标，macOS 尽力支持。
- 本地开发环境：Windows Server 2016；本地目标是 `x86_64-pc-windows-msvc`；Linux 验证交给 CI。
- **ZCode 是 Electron 应用，官方要求 Windows 10/11，在 Win2016 上可能无法启动**；改用 VS Code + `rust-analyzer` 或纯命令行。这是环境问题，不影响项目选型。
- CI 三级策略：
  1. M0：`ubuntu-latest` + `windows-latest` 跑 `fmt / clippy / build / test`；
  2. M2 完成（`u` 撤销闭环）后：增加 `--release` 构建 + 产物归档；
  3. M6 发布前：tag 触发 Release 上传。
- 发布后验证：Windows 产物在 Win2016 手动冒烟；Linux 产物在 CI 或容器中 `--version` 冒烟。
- CI 无 TTY：集成测试禁用真实终端，TUI 用 `TestBackend`；冒烟只跑 `--version` / `--help`。
- 路径统一 `/`；`Location::Archive { inner }` 在渲染、比较、索引前规范化。
- 测试 fixture 用 `tempfile`，不硬编码 `/tmp`。
- Windows 注意：文件名大小写不敏感、符号链接权限、`trash` crate 回收站行为、CRLF/LF。
- 不引入 `unsafe`，除非有明确必要并注释原因。
- 本机（Win2016）工具链踩坑（2026-11-11）：新版 VS 引导器会拒绝系统内置的 .NET 4.6.2，需先装 .NET Framework 4.8 并重启；VS 2022 17.14 目录已移除 `Windows10SDK.20348` 组件 ID（`--add` 静默跳过），装 SDK 应使用 `Microsoft.VisualStudio.Component.Windows10SDK.19041`（支持 14393 宿主）；VC 工具与 SDK 各触发一次 soft-reboot，重启前不要补装。装齐前 rustc 检测不到 MSVC，会回退到 Git Bash 的 GNU `link` 并报 "extra operand"。

## 6. 里程碑顺序

- 最终顺序：**M0–M6 交付 v0.1.0 核心；M7–M11 为后续增强**。
- 早期讨论中曾提出：
  - 把 M7 归档浏览插到 M4 搜索之后、M5 配置之前；
  - 把 M8 frecency、M11 预览放到 M6 发布之前。
- 这些插入位置与“M6 是 v0.1.0 发布”冲突，最终以 [PLAN.md](PLAN.md) 的总表为准。
- 若预算或时间受限，优先 M7 与 M8（用户价值最高），M9–M11 可延后。

## 7. 已评估但放弃的方案

| 方案 | 放弃原因 |
|---|---|
| 全量忠实重写 vifm | 17 万行 C、12–18 个月、1.5 亿–4 亿 token，投入产出比低 |
| 嵌入完整 Vim 引擎（`narwhal-vim` / `vimcore` / `hjkl-engine`） | 文件管理器需要文件操作语义，不是文本缓冲区变异；复杂度和体积不划算 |
| Vim 脚本配置 / 插件系统 | 范围蔓延，安全与兼容成本高；TOML + Action 足够 |
| FUSE / `archivemount` | 依赖内核模块和系统权限，容器、Windows、无 root 环境不可用 |
| `.7z` / `.rar` 虚拟浏览 | 首批解码库与边界复杂；提示解压更务实 |
| 嵌套归档递归浏览 | 交互与性能边界复杂，用户价值低 |
| marks | 与 frecency `pick` 功能重叠；移除以简化 |
| SQLite / zoxide SQL 技巧 | 引入 C 依赖，跨平台构建复杂；redb 纯 Rust 更合适 |
| fzf 二进制 / `--preview` 外部命令 | 部署依赖与进程开销；原生 `pick` + 共享 `PreviewRenderer` 更一致 |
| 后台守护进程 / 定时 compaction | 常驻进程与平台差异；惰性 compaction 足够 |
| Überzug++ / Chafa 作为必需依赖 | 用户需额外安装；halfblocks 内建回退覆盖所有终端 |
| 为每个终端写图形协议分支 | 维护成本高；`ratatui-image` 已统一适配 |
| `tdf` / `pdfium-render` 作为编译期依赖 | 体积、平台依赖、维护成本；改为文本提取 + 可选后端 |
| `exprim` 作为默认/必需依赖 | 2026-11-10 发布首日、总下载 7、0 反向依赖、主 crate 无 CI、Windows 未验证；供应链与维护风险过高，改为可选 `pdf-visual` feature（实测数据见 4.8） |
| 调用 LibreOffice / `textract` 预览 Office | 外部工具部署负担；纯 Rust 解析更符合“零配置” |
| `docparser-office` 作为 Office 主选 | 2026-10-28 发布、84 次下载，crates.io 仓库地址 404，源码与维护状态无法核实 |
| `pdf-extract` 作为 PDF 文本主线 | 依赖较老的 `lopdf 0.42`，font/CMap/CJK 已知缺陷与 panic issue 较多，仓库无 LICENSE 文件；仅适合保守 fallback |
| GPL-2.0-or-later | 分发衍生作品必须 GPL、不能闭源嵌入、无法双许可；仅当大量复用 vifm 源码时才选 |
| `vifm-rs` 作为包名 | 暗示 fork/衍生，易引发许可证误解；已定 `rvifm` |
| vifm 图标插件 / `vifmrc` 注入 | 与“内建优先”定位冲突 |
| 嵌入 SVG / 字体 | 体积与许可问题；Nerd Font 由终端提供 |
| 实例级提权（重启 root rvifm） | vifm 的 `:!sudo vifm %c` 路线：状态同步差、攻击面大；改为操作级内联提权（M12） |

### 2026-11-15 — M14 双栏模型与标签栏（用户实测反馈驱动）

- **背景**：v0.4.0 在 Linux（CachyOS）实测发现：①“启动进入 TUI（列表 + 右侧预览窗格）”与 vifm 经典模型不符——vifm 默认左右两个 pane 都是目录列表，w/:view 才把非活动 pane 切成预览；②多标签功能存在（exec/测试覆盖）但无标签栏 UI，`:tabnew` 落点为同目录导致视觉零变化，用户无从验证。
- **根源归属**：①SPEC M1 SIMPLIFIED“可先做单栏”被 M1/M11 沿用且 M13 :view 立项时未把双栏模型作为前置一起修正（需求文档缺陷）；②SPEC §4.2 未定义标签栏 UI，ACCEPTANCE §3.C 无可见性验收（验收文档缺陷）。§16 手册标题滞留 v0.2.0（文档维护缺陷）。
- **结论（用户确认“按此立项推进”）**：M14-1 右 pane 改为独立目录列表（独立 Location/光标，可导航，Tab 键切焦点，写操作作用于焦点 pane）+ `w`/:view 切换右 pane 预览视图；M14-2 标签栏 UI；M14-3 ACCEPTANCE §16 全面修订（0.5.0 语义）；M14-4 发布 0.5.0。
- **SIMPLIFIED 裁剪（NOTES 记录）**：右 pane 仅导航（搜索/可视选择/标记/树/选择器/提权仅左 pane，触发时提示）；右 pane 无独立撤销语义（恢复后双 pane 刷新）；不实现第三 pane（issue #1062 不立项）。
- **影响文件**：docs/SPEC.md M1 SIMPLIFIED/M14 节、docs/TASKS.md M14、docs/ACCEPTANCE.md §16、src/state.rs（Pane 结构）、src/tab.rs、src/app.rs（焦点分发/渲染）。

### 2026-11-14 — M13 vifm 对齐增强立项（第一/二档）+ :view 语义澄清

- **来源边界（用户指定）**：候选清单仅依据三来源——vifm GitHub 仓库、open issues（按评论/热度）、vifm 官方文档（vifm.info/docs.shtml）；博客/对比文/个人笔记视为可能过时或自定义配置，不采用。
- **三条语义修正（用户核对官方文档确认）**：
  1. vifm 默认（cpoptions 含 t flag）下 Ctrl-I/Tab/Space 均为**切换活动窗格**，非历史跳转——清单撤下 Ctrl-I 条目；
  2. h 即返回父目录（cd ..），与目录历史无关——rvifm ReturnParent 语义已一致；
  3. t = select or unselect (tag) 单文件（官方原文），rvifm t 已对齐；:select/:unselect 补的是**按 pattern 批量**形态。
- **:view 语义澄清（用户）**：w 默认映射 :view；视图打开于**非活动窗格**（对侧预览窗格），非全屏、受窗格布局限制；光标保持在列表窗格，j/k 移动时预览跟随更新；再次 w/:view 关闭。rvifm 落地：预览窗格在摘要预览（现状）与完整文件视图间切换，保持双栏布局。
- **立项范围**：第一批 f/F/;/,、:select/:unselect、DD、j/k wrap、Ctrl-G；第二批 cw 批量重命名、:tree、后台作业+:jobs、view mode、暂存区菜单。发布节奏 0.4.0。
- **备选与放弃**：Ctrl-I 历史跳转（默认被 Tab 占用）；第三档 :compare/VCS/自定义视图与第四档（寄存器/OS 回收站/Lua）暂不立项（第四档需先解禁 SPEC 约束）。

### 2026-11-14 — frecency 目录/文件独立事件时钟

- **背景**：单一 `global_tick` 下，目录跳转（cd，高频）与文件打开（Open，低频）共用一个 aging 时钟——目录重度使用会加速文件条目老化（文件条目的 Δtick 被目录事件数推高），两类访问频率互相稀释排名。
- **结论**：meta 增加 `dir_tick` / `file_tick` 两个类型时钟；事件 JSON 携带其类型时钟值 `clock`；aging 的 Δtick 只取同类型时钟差。`global_tick` 保留为全局事件序号（events 表 key 必须全局单调唯一 + compaction 增量游标），不再参与 aging。
- **兼容策略（无迁移）**：事件/snapshot 的 `clock` 字段 `#[serde(default)]`——旧 JSON 无此字段时值为 0，重放侧以全局序号（events key / snapshot.tick）兜底，旧语义等价；`dir_tick`/`file_tick` 首次缺失时以 `global_tick` 当前值初始化，新旧 aging 连续分叉。
- **备选与放弃**：拆两张 events 表（dir/file 各一）——compaction 游标与合并逻辑翻倍且仍需第三计数器，收益不抵复杂度；用 wall-clock ts aging——改变 EMS 的"事件数衰减"语义（M8-9 排名稳定性测试基于事件时钟），不采用。
- **影响文件**：docs/SPEC.md §4.4、docs/TASKS.md M8-10/M8-11、src/store.rs、src/frecency.rs。

### 2026-11-12 — M12 采用操作级内联提权，不模仿 vifm 的实例级提权

- **背景**：vifm 官方明确无内建 sudo 支持（"there is no builtin support for executing anything as a different user"），社区靠 `:!sudo cp %c` 或重启提权实例（`nnoremap <c-s> :!sudo -E vifm %c`）绕过，两种方式都离开 TUI 上下文：前者打断文件管理流程，后者需要两实例间同步状态且原实例不自动刷新。
- **结论**：rvifm 用 `sudo -A` + `rvifm --askpass` 辅助模式实现**操作级**内联提权——文件操作因 EACCES 失败时，设置 `SUDO_ASKPASS` 指向自身辅助模式，在现有 TUI 上渲染密码输入框（ratatui MaskedInput），验证后原地重试当前单个操作。不启动 root rvifm 实例，不引入特权守护进程。
- **备选与放弃原因**：`sudo_prompt` crate（轻量，可作 askpass 封装的备选）；`elevated-command`（pkexec 路线 Linux 体验原生但 Windows 支持不明确，且依赖 polkit 服务运行）——不引入 pkexec 作为硬依赖。实例级提权路线放弃：状态同步是固有缺陷，且完整 root 文件管理器的攻击面远大于单命令提权。
- **影响文件**：`docs/SPEC.md` §5 M12、`docs/TASKS.md` M12-1～M12-5、`docs/ACCEPTANCE.md` §15、`AGENTS.md` §5.7。
- **待定/风险**：Windows 提权（UAC 机制不同）尽力而为；TUI 内 askpass 被 sudo 子进程调用时的终端 I/O 交接细节在 M12-2 实现。

## 8. 待定事项

执行到对应任务时必须先决策，并更新本文档与 [SPEC.md](SPEC.md) 的待定列表：

- **【已决策 2026-11-19】pending 合并职责**：采纳方案①——`events.pending` 合并只在 `record` / TUI 启动路径执行，`pick` 走只读 open（不写 redb、不推进任何 tick）。方案②（保留 pick 合并但削弱只读承诺）已放弃，因为 AGENTS §5.3 的只读红线优先于合并便利。执行 AUDIT-1；详见 §10 同日决策记录。
- **【已决策 2026-11-19】keyword 长度分工**：单字符 → `'<char>` marks 导航；多字符 → `o` / `:j` / `:J` 搜索的高优先级匹配（keyword 命中项排前）。`:bmark` 系列为 vifm 对齐视图，`:keyword` 保留兼容。详见 §10 同日决策记录。
- **【已决策 2026-11-19】compaction 阈值**：默认 5000 条事件；惰性触发由 AUDIT-4 实现，配置面随任务落地。
- **【已决策 2026-11-19】短别名 `rv`**：提供，与 `rvifm` 并存；项目名/crate/主二进制名不变。
- 【已决策 2026-11-13】keymap-rs 评估（M10-2）：聚合 crate 不存在；keymap-core/seq/config 均为 0.1.2（下载 500–700，单维护者，MSRV 1.85）。采纳自写轻量 Trie 解析层，TOML 分层自实现。理由：低采用量 crate 的供应链风险高于自写 ~40 行解析器的维护成本。
- 【已闭环】tar 变体排期（M7-8 读端 + M26-4 外部 CLI）、大目录性能阈值（M6-4 验收）、主题备选（opaline 持续使用）。

## 9. 文档蒸馏规则

已迁移至 [AGENTS.md](../AGENTS.md) §8（唯一维护点，含文档职责边界、信息类型投递、新需求处理流程与禁止事项）。

## 10. 决策记录模板

新增决策时使用：

```text
### YYYY-MM-DD — 决策标题

- 背景：
- 结论：
- 备选与放弃原因：
- 影响文件：
- 待定/风险：
```

### 2026-11-15 — FIX-2 范围：dd→cut 缓冲接线 deferred

- **背景**：SPEC §4.8 要求 `dd` 的 Delete 同时放入 cut 缓冲区，`p` 按移动处理。但 rvifm 的 dd 是"确认后立即暂存到应用内 trash"（M2-2），文件已离开原位；此时 `p` 需要的是"从暂存区恢复到任意目录"（恢复目标 ≠ 原位），与 M13-10 的 `restore_from_trash`（恢复到 orig 原位）是不同机制。
- **结论**：FIX-2 只接 `yy`→copy 缓冲（`p` 复制 / `P` 移动）；cut 缓冲字段（`YankBuffer.cut`）已就位但 `dd` 暂不写入。dd→cut 完整接线（p 从暂存恢复到当前目录 + 撤销语义）作为独立任务后续实现。
- **影响**：`dd` 后 `p` 当前行为 = "缓冲区为空"提示（dd 不再污染复制缓冲）；不影响 FIX-2 验收（yy→p 复制链路）。

### 2026-11-15 — FEAT-1 滚轮语义：逐行移动光标（非独立视口滚动）

- **背景**：FEAT-1 要求"滚轮上下滚动列表，光标不跳变"。rvifm 的列表用 ratatui `List` 渲染，无独立视口偏移（`Tab.scroll` 仅在导航时清零）；`List` 渲染时强制保证选中项可见——若只改视口 offset 而不动光标，超出可见窗口后 widget 会把 offset 拉回，滚轮表现为"失效"。
- **结论**：滚轮每格精确移动光标一行（`move_cursor(±1)`，边界钳制、无环绕）。"不跳变"取"每次精确一行、不跳到无关位置"的解释；列表随之滚动，选中项永不消失。这也是 vifm/ranger 的实际行为。
- **配套**：视口偏移改为显式维护（`list_offset(cursor, height)` 与 `ListState::with_offset` 同源），渲染与鼠标命中测试共用同一推导，单击行号 → 条目索引的映射不再依赖 ratatui 内部调整。
- **双击**：无需计时器——单击聚焦 → 已聚焦行再击打开，双击的第二击天然落入"再击已聚焦行"路径。

### 2026-11-15 — FEAT-3 右键菜单 Copy/Move 目标语义 + system_open 测试踩坑

- **Copy/Move 目标**：菜单动作无目标参数，采用 vifm 双栏约定——复制/移动到**另一 pane 所在目录**（焦点 pane 的对侧）。对侧在归档视图时报可读错误（无物理目录）；同名冲突走 ops::copy_to/move_to 的既有报错（不覆盖）。
- **Rename**：预填 `rename ` 进命令行（SIMPLIFIED：复用既有输入行，不弹专用对话框）；Delete 走 dd 的 Confirm 流程（应用内暂存，u 可逆）；Open 与 l/Enter 同链（目录/归档进入、文件走系统 opener）。
- **踩坑**：`opener::system_open` 用 `Command::status()` **阻塞等待**外部程序退出——单测对"文件"执行 Open 动作会挂起整个测试进程（本机 gio 可用时会真的弹出查看器）。测试中 Open 动作只能对**目录**验证（enter_selected 无外部进程）。

### 2026-11-15 — M16 立项：marks 不回归，以 frecency 快速映射层补齐"两键直达"

- **背景**：用户实测提出 `'d` 与 `o d <Enter>` 的差异（少一次确认，高频跳转可感知）。vifm marks = 单字符两键手动标记（约 62 个、需预设置、顺序不保留）；frecency = 自动、概率、容量近乎无限。两者分工不同，非二选一。
- **结论**：marks 维持移除（SPEC OUT_OF_SCOPE / 本文档 §4.3 不推翻）；新增 `'`+单字符快速映射层（M16 NAV-1）——vim 肌肉记忆保留，底层仍是 frecency 评分/衰减/事件日志，无第二套路径存储。
- **设计分叉（用户拍板）**：char→keyword 形态——配置只存 char→keyword（`[quickjump]` 可选别名层，缺省 `'d` 即查 keyword=="d"），跳转 = 该 keyword 最高排名条目直达 cd。**发现的缺口**：frecency keyword 字段此前无任何设置入口——以 append-only keyword 事件补（`record --keyword` / TUI `:keyword` 同链，replay 应用），不破"record 只追加、写入口唯一"红线。
- **vifm history 调研结论**：`:history` 是菜单而非跳转；Ctrl-O/Ctrl-I 行为条件化（cpoptions `t` 标志）；历史绑定 view/pane、按 tab 独立；Ctrl-I 与 Tab 同键码且 rvifm Tab 已占用 pane 焦点——NAV-3 不设默认键是**必须**而非偏好。
- **:ghistory 评估**：不做独立命令（命令面膨胀）；并入 `o` 选择器第四视角（JumpKind::Recent，last_visit 倒序 + 会话内存评分合并——注意会话内跳转只在内存、退出才写 events，必须合并否则显示旧数据）。replay 顺手记 last_visit，与 NAV-1 keyword 查找共用一次重放。
- **顺序**：NAV-1 → NAV-3 → NAV-2 → NAV-4（NAV-3 栈改造先定型，NAV-2 菜单消费最终结构）。
- **影响文件**：SPEC M16、TASKS M16、ACCEPTANCE §18、本节。
- **待定/风险**：keyword 事件的 replay 应用语义（与 Grade 分级/书签标志的合并）在 NAV-1 实现时细化；`:history` 菜单排序（时间正序/倒序）在 NAV-2 实现时定。

### 2026-11-15 — NAV-1 实现决策与踩坑

- **keyword 事件语义**：EntryKind::Keyword 只应用 keyword 字段，不加分、不推进时钟（书签设置本身不产生访问权重）；空 keyword = 清除书签（保留分数正常衰减，SPEC §4.6 一致）。隔离验证：同一访问序列 ± keyword 事件重放分数完全相等。
- **toml 1.x 踩坑**：Value::from_str 只解析单值（"unexpected content, expected nothing"），文档解析必须 Table::from_str。
- **cmp_ranked 是降序比较器**（b-vs-a），直接传给 max_by 会取到最小值——quickjump_target 用显式 score.total_cmp + 路径 tiebreak（候选 keyword 相同 → 档位相同 → 分数是唯一区分）。
- **:keyword 写路径**：与 record 同链（Store 短事务 + 3 次重试 + events.pending 降级）；record --keyword 产生纯元数据事件（不记访问），TUI 会话内的跳转仍走退出时批量追加。

### 2026-11-15 — NAV-3 实现决策与发现的缺口

- **history 语义升级**：Vec（只存"来自哪里"）→ 完整轨迹 + hist_pos 游标。navigate_to 先加载后变更（损坏归档失败无副作用，M7-4 契约保持）；新导航截断 forward（vifm 语义）。
- **发现的缺口**：M10-5 的 keymap TOML 分层（prepend/append/replace）实际未接线——InputState.keymap 只有默认表，config.rs 无解析。用户把键绑到 :histprev/:histnext 暂不可用；help 中的 keymap 示例为注释性展示。补全接线作为独立后续任务（在 TASKS 之外排队，属 M10-5 收尾）。
- **无默认键是必须**：Ctrl-I 与 Tab 同键码，Tab 已被 M14 pane 焦点占用。

### 2026-11-15 — NAV-2 菜单语义细化

- **菜单导航 = 新导航**：从历史菜单选中条目走 navigate_to（追加轨迹 + 截断 forward），不是 histprev 式游标移动——vifm 同语义。back 栈内的目录重复出现属正常（每次访问都是一条轨迹）。
- **dd 边界**：当前视图（hist_pos）不可移除（避免悬空游标）；删除游标之前条目时 hist_pos 前移保持指向同一视图。

### 2026-11-15 — NAV-4 实现决策与发现的 M8 缺口

- **会话合并**：不引入会话事件缓冲——直接合并各 tab 的 history（NAV-3 使 navigate_to 成为唯一导航咽喉，history 即会话访问轨迹）。轨迹顺序 = 访问顺序 → 递增时间戳（基准 = 墙钟 -1h，保证新于大多数已写盘事件且会话内有序）。只影响 last_visit（近期排序），不改分数（分数合并属 M8 会话评分语义，随退出刷写一起做）。
- **发现的 M8 缺口**：SPEC §4.6 "TUI 运行中跳转在内存更新评分，退出时批量追加 events" 从未实现——会话访问退出即丢失，frecency 只学到 shell hook 的 cd。退出刷写（drain history → append_events）作为独立任务排队，优先级中（影响 frecency 数据完整性，不影响正确性）。
- **入口命令名**：`:recent`（清晰、不与既有命令冲突）；SPEC SIMPLIFIED 授权实现时定名。

### 2026-11-15 — M17 立项：递归搜索原生内建（结果=虚拟目录）

- **背景**：用户提出 :find/:grep 需求后两轮设计收敛——第一轮"复用 frecency picker"被用户修订：picker"选中即跳转、列表消失"不符合搜索工作流；改为**结果=虚拟目录**（同目录导航语义探索）+ **预览窗格升级为交互式查看器**。这是 rvifm 用 Rust 内建能力超越 vifm"封装外部命令 + 菜单"的定位。
- **依赖理由（获批）**：`ignore`（ripgrep 遍历组件，MIT OR Apache-2.0——原讨论中"MIT/UNLICENSE"表述有误已修正）为唯一净新增，其关键传递依赖（crossbeam-channel/regex-automata）已在树中；`regex 1.13.1` 原为间接依赖升为直接依赖（零新增编译单元）。纯 Rust、双许可、成熟度极高（ripgrep 生态）——符合"零配置、内建优先"原则。
- **架构决策**：结果虚拟目录沿 :tree 同构（Tab.results 静态条目 + 只读守卫 + toggle/定位退出），items/entries 平行表——entries 驱动既有光标/预览/yy 机制，items 携带语义路径/行号。预览源抽象 preview_path()（结果模式取 item.path）。
- **:grep 选中降预期**：系统 opener 无行号语义——选中=定位文件，"打开并跳行"依赖 opener 插值扩展（OUT_OF_SCOPE）；VIEW-1 查看器提供行级定位（grep 命中打开查看器直达命中行）弥补。
- **发现勘误**：原讨论称 ignore 为"MIT/UNLICENSE"有误；"swpui"名称无法核实，以"外部搜索 TUI 工具"泛指并列入 OUT_OF_SCOPE。
- **影响文件**：SPEC M17、TASKS M17、ACCEPTANCE §19、本节、Cargo.toml（ignore/regex）。
- **待定/风险**：大仓库遍历的同步阻塞（ripgrep 级速度 + 1 万上限，暂可接受；后台化后续接 M13-8）。

### 2026-11-15 — GAP-1 实现决策

- **键位分层语义**：prepend 覆盖默认（含内建单键处理——explicit 集合在单键区最前拦截）、append 低于默认、replace 完全替换。用户序列全部记入 explicit 集合，单键按下时若该键是某显式绑定的完整单键序列则直接执行（绕过内建硬编码臂）。
- **execute_action 抽取**：seq 派发的 match 臂与单键用户绑定共用同一执行函数；序列清理由调用方统一处理（根除"早退跳过 clear → 残留吞键"缺陷类，此前 dd/DD 确认后即受此害）。
- **run 字段命名**：snake_case 为主 + vifm 惯用别名（put_move/paste_move 等）；未知动作名与非法键**整条跳过**不 panic（配置容错与 M5-1 一致）。
- **`histprev`/`histnext` 落为 Action 变体**（无默认键），用户 keymap.toml 可绑（如 C-o/C-i）——NAV-3 的推荐映射示例自此真正可用。

### 2026-11-15 — GAP-2 实现决策

- **事件边界**：history[0]（启动落点/:tabnew 落点）不算导航——SPEC"record 只在真实跳转成功后调用"的延伸；history[1..] 每条 = 一次真实 navigate_to（含 cd/quickjump/定位/前进后退）。重复访问保留（每次 = 一次访问，评分语义）。
- **降级**：与 record 同链——Store 打不开整批写 events.pending；单条重试 3 次失败也落 pending；下次打开时 merge_pending 合并。刷写失败 eprintln 不阻塞退出。

### 2026-11-15 — M18/M19/M20 立项：编号冲突修正 + 排序/SFTP 决策

- **编号冲突（必须重编号）**：用户提案的 M16（排序）/M17（SFTP 浏览）/M18（SFTP 写）与已完成批次 M16（导航增强 NAV）/M17（递归搜索查看器）冲突——TASKS/SPEC 均已占用。按下一空闲号重编为 **M18/M19/M20**，任务 ID 相应映射（M16-1→M18-1、M17-3a→M19-3a、M18-1→M20-1），内容与硬约束不变。
- **键位现状核实**：`s`/`S`/Space 此前均未绑定——无冲突。"空格=切换活动窗格"按 vifm 语义在 M18 新增（与既有 Tab 切换并存，M14 语义不变）。
- **远端搜索边界修正**：用户原文"远端排序、搜索、预览复用本地同一套实现"中，排序与列表内 `/` 搜索可复用（entries 基）；`:find`/`:grep` 基于 ignore 遍历仅作用本地 fs——远端递归搜索列入后续增强（SPEC SIMPLIFIED 已注明）。
- **SFTP spike 纪律**：候选 russh+justsftp / remotefs-rs；spike 只进 dev-dependencies + examples/（正式集成待用户确认 NOTES 结论后）；spike 完成**暂停等确认**再 M19-3b。
- **影响文件**：SPEC M18/M19/M20、TASKS、PLAN（里程碑表补 M12–M20 行）、本节。

### 2026-11-15 — M18-4 踩坑与流程教训

- **[ui] 嵌套表**：用户规格的 `[ui] terminal_in_new_window` 是嵌套表——Config 平铺结构收不到（serde 忽略未知顶层表 → 字段始终默认值）。实现为嵌套 UiConfig（Copy，Default terminal=true）。经验：TOML 分节配置必须用嵌套 struct 对齐。
- **流程教训（重要）**：用 python 大块 replace 重写 handle_key 关键区域时，锚点静默不匹配 + 重复插入导致 app.rs 结构损坏（重复定义/截断/串位）。本次靠 stash 丢弃损坏工作、以 Edit 工具小步重做恢复。**规则固化：跨函数结构改动必须用 Edit 工具小步提交式修改，每步 cargo check；禁止对核心文件做多处大范围 python replace。**
- **tmux 新窗口**：`tmux new-window -d "sh -c '<cmd>'"`（fire-and-forget，退出即关窗）；POSIX 单引号转义 shell_quote。screen 走 `screen -X screen -t rvifm sh -c`。

### 2026-11-15 — M19-3a SFTP spike 结论（实测 127.0.0.1，rustc 1.95 / Linux）

**候选 A：russh 0.65 + russh-sftp 3.0 + tokio（推荐）**
- 纯 Rust、MIT/Apache；无 C 依赖（Windows MSVC 构建友好）
- 实测：密钥认证（Ed25519）→ connect+auth 77ms → list /（26 项）→ 写操作全链路通过（create_dir / write（需 `create()`+`write_all`，`write()` 仅 WRITE flag 不建文件）/ read / rename / remove_file / remove_dir）
- 异步模型：tokio async；集成 ratatui 需同步桥（专用 runtime 线程 + channel，或 `Runtime::block_on` 于非渲染阶段）——集成难度中等
- API 摩擦（spike 中实际踩到 4 处）：① `check_server_key` 默认拒绝全部（必须覆写接受/校验）；② 通道须先 `request_subsystem("sftp")` 否则握手超时；③ 路径参数是 `String` 而非 Path（SFTPv3 规约）；④ `write()` 不带 CREATE flag（新文件须 `create()`+`write_all`）
- 依赖体积：dev 树新增 russh/russh-sftp/tokio/tokio-stream + 传递（全部纯 Rust）

**候选 B：remotefs 1.0 + remotefs-ssh 1.1（否决）**
- 同步 API 与本项目 RemoteFs trait 形态 1:1（list_dir/create_dir/rename/remove_file/remove_dir_all + stream）——集成代码最少
- 实测：密钥认证（需实现 `SshKeyStorage` trait 提供 ~/.ssh 密钥路径）→ connect+auth 188ms → list + 写操作全链路通过
- **否决理由**：默认后端 libssh2（C 绑定）+ openssl（C）——Cargo.lock 拉入 libssh2-sys/openssl；Windows MSVC 下 openssl/libssh2 构建链复杂，违背"跨平台零 C 依赖"取向；另 `SftpFs` 泛型带 session 类型参数、remotefs 1.0 与 0.2 两代 API 并存的版本碎片风险
- 备选价值：若 M20 需要 scp 或更完整的企业特性可复评

**结论（待用户确认后执行 M19-3b）**：采用候选 A russh + russh-sftp；同步适配层放 `src/remote/sftp.rs`（tokio runtime 于后台线程常驻，RemoteFs 同步方法 block_on 桥接）；密钥路径默认 ~/.ssh/id_ed25519/id_rsa，密钥口令交互 M19 不做。
- **dev 依赖现状**：russh/russh-sftp/tokio/tokio-stream 已入 dev-dependencies（spike 运行依赖）；M19-3b 集成时迁移至 [dependencies]。
- **影响文件**：examples/sftp_spike.rs（候选 A 实测代码，保留作回归参考）、Cargo.toml/lock、本节。

### 2026-11-16 — M19-4 实现说明

- **配置合并**：resolve_all = remotes.toml（优先，可覆盖同名）+ ~/.ssh/config 别名兜底（host=别名本身，HostName 指令未解析——SIMPLIFIED：连接走别名 host 由 sshd 端配置；后续可接 HostName）。
- **ssh config 解析**：裸 Host 行收集别名；通配 `*`/`?` 跳过；Match 块内的 Host 跳过（简化：Match 状态到下一个指令重置）。
- **toml::Value::as_u64 不存在**（toml 1.x）——整数用 `as_integer()`。
- **[ui] 嵌套教训复用**：remotes.toml 用嵌套表解析（每 [name] 一节），与 M18-4 的 UiConfig 同套路。

### 2026-11-17 — M20-2/3/4 实现说明

- **DD 远端分流**：DD（永久删除）在远端视图绕过 trash，直接 remove_permanent/remove_dir；dd 走 move_to_trash 可恢复。execute_delete 内按 permanent 标志分流。
- **跨 VFS 移动语义**：paste_yanked_remote/paste_remote_to_local 在成功复制后从来源删除——远端来源 remove_permanent、本地来源 fs::remove_file。移动语义与本地 copy+delete 原语一致。
- **进度提示**：状态栏 "Moved/Copied N item(s) to remote:path" 已覆盖；细粒度进度条后续增强（SIMPLIFIED）。

### 2026-11-17 — M21 立项：编号映射 + 现状核对

- **编号冲突**：讨论稿 FIX-1..9 与 M15 已用 FIX-1..4 冲突 → 批次 M21，任务 ID 映射 FIX-n→M21-Fn、ENH-n→M21-En（ENH 前缀无冲突但统一映射保持批次一致性）。
- **现状核对（避免重复实现）**：
  - ENH-2（w 预览/焦点不变/跟随）已实现于 M13-9 view_mode + M14-1 + VIEW-1 交互查看器 → 登记为验证补缺。
  - ENH-6（.tar.xz/.tar.bz2）已实现于 M7-8（archive.rs 五压缩形态）→ 验证补缺。
  - ENH-4 的 R/:rename $EDITOR 多选已实现于 M13-6 cw → R 复用 cw 管线，I/cc/A 为新增光标定位变体。
  - FIX-6：directory_preview 已存在（M11-2），但用户实测报"无法读取"——缺陷在视图层分流（view_mode 或右 pane 路径），登记为修复。
  - FIX-7：preview_kind 有 Office/Pdf 分流，但实测仍报错——同上，视图层分流缺陷。
  - FIX-8 根因：`system_open` 用 `Command::status()` 继承 GUI 程序 stderr（winit_wayland 警告直漏终端）→ 修复 Stdio::null。
  - FIX-9/ENH-8 记录补全与 GAP-2（退出批量刷写）互补：F9/E8 是实时 record 调用点。
- **降级规则**（用户指定）：F5 超 5 轮缩到"仅当前目录刷新"；F9/记录补全缩到"仅目录进入记录"。
- **影响文件**：SPEC M21、TASKS M21、ACCEPTANCE §20、PLAN M21 行、STATE、本节。

### 2026-11-17 — M21-E4 批量撤销口径

- **"整批 u 可逆"的实现口径**：沿用 M13-6 cw 管线的逐项入栈（SPEC E4 明确"复用 cw 管线"），连续 `u` 全批可逆；未引入成组撤销机制（撤销栈结构与 M2-6 逆序语义不动）。若未来需要一次 `u` 撤整批，需撤销栈支持事务分组——范围外。
- **`:rename` 语义分叉**：带参 = M2-3 单项直改（提权链保留）；无参 = $EDITOR 批量（E4 SPEC"R/:rename 多选列表"）。存量测试"缺少新名字"断言按新语义更新。

### 2026-11-17 — M21-F9 实时记录与 GAP-2 的去重设计

- **计数而非有无**：`live_recorded` 记 (tab, 'L'/'R', 路径) → 次数——同一目录多次进入各计一次（vifm frecency 按访问次数评分）；退出刷写对左 pane 历史按 min(已刷写条目序, live 次数) 跳过。侧别隔离：右 pane 记录无退出对账面（exit flush 只走 tab.history），独立 'R' 键防跨污染。
- **测试环境跳过真实写入**（cfg(not(test))）：任何调用 open_selected/enter_selected 的测试若触发 flush_visits 会 (a) 写入开发者真实 frecency 库（污染用户数据），(b) 并行下 redb 锁竞争复现（nav4 前车之鉴）。可观察态改为 `live_recorded`；flush_visits 本身由 frecency 模块测试覆盖，验收"退出后 o 出现该目录"由生产路径（GAP-2 已验证的退出刷写 + 实时写入同链路）保证。
- **调用点**：仅 l/Enter 成功路径（open_selected 的 `..` 行返回/进入、右 pane 臂）；`h`/`:cd`/histprev 不实时记录（exit flush 兜底）——与 SPEC F9 字面边界一致。

### 2026-11-17 — M21-F1 实现说明与踩坑

- **类型着色方案**：`FileTypeClass` 8 类 + 主题命名样式（`ft_dir` 等）优先、缺失回落 16 色近似——内置 opaline 主题均无 ft_* 样式，回落保证零配置即有不同的颜色；用户主题可通过同名样式覆盖（与 §5.6 主题机制同构）。可执行判定：unix 按执行位，Windows/远端/归档按扩展名近似（无执行位可用来源共用 `is_exec_name`）。
- **时间格式化**：新增直接依赖 `jiff 0.2.43`（此前仅 env_logger 传递引入，零新增传递依赖）——本地时区 `yyyy-mm-dd HH:mm:ss`；自写 civil-from-days 不支持时区且 Windows tz 读取麻烦，不值得。`format_mtime` 同时服务 F1 详情与后续 E1 属性列。
- **踩坑（构建环境）**：M19-3b 引入 russh → `aws-lc-sys` 在无 NASM 的 Windows 上构建失败。解法：`AWS_LC_SYS_PREBUILT_NASM=1` 启用 crate 自带 prebuilt 对象，已固化到 `.cargo/config.toml [env]`（CI 有 NASM，变量自动不生效）。
- **踩坑（跨平台门禁降级）**：同因 russh 依赖链含 C 代码，AGENTS §6 的 `cargo check --target x86_64-unknown-linux-gnu --all-targets` 在本机无法编译（缺 x86_64-linux-gnu-gcc）。cfg(unix) 代码的 Linux 侧验证自 M19 起实际由 CI ubuntu job 承担；本地门禁保留为"逐行自查 cfg 镜像 + CI 兜底"。
- **踩坑（存量偶发）**：`nav4_recent`/`exec_j_and_big_j` 并行下 redb `Database already open`（原始 pull 代码 3/3 复现，非本任务引入）。按 M13-10 trash 串行锁先例加 `STORE_TEST_LOCK` 串行化全部打开全局 store 的测试。
- **渲染几何教训复用**：TestBackend 渲染 CJK 全角为"字+空格"（FEAT-4 已知）——"[文件]" 断言去空格比对；长 temp 路径在 80 宽 pane 内必然截断——验收测试注入短路径 Location 而非依赖真实 tempdir 路径。

### 2026-11-17 — M22 立项：SFTP 安全与远端体验补全

- **known_hosts（M22-2，安全动机）**：M19-3b 起 check_server_key 为"首连接受任意密钥"（TODO 遗留）——TOFU 但无固定，存在中间人风险。实现选型：**自实现 OpenSSH known_hosts 解析**（明文 hostname + `|1|` hashed 两种，base64+sha256 可算；`@revoked`/`@cert-authority` 标记行跳过），不引第三方 crate——供应链原则与 M10-2（自写 Trie）一致。指纹变更拒绝为安全默认，跳过需用户在 `[remotes.toml]` 显式 `known_hosts = false`（错误信息含指引）。首连追加写入保持 0600。
- **口令认证（M22-3）**：复用 M12-3 AskPass 通道（已有安全输入路径 + masked 渲染 + 取消干净中止语义）；口令不写配置/日志。触发条件：密钥探测与认证均失败后回退。
- **`:cd` 锚定（M22-1）**：现状相对路径相对**进程 CWD** 解析（M21-E5 CI 实测踩中：`cd ..` 后 `mk` 把目录建到仓库根）。语义决策：相对路径相对**当前 tab 目录**（用户预期 + vifm 行为一致），解析后 Location 恒绝对路径。`cd -` 不做（histprev/:history 已覆盖）。
- **远端文件打开（M22-6）**：FUSE 与流式均在边界外（AGENTS §5.1 / 无断点能力）——临时副本方案；副本保留至进程退出统一清理（Windows opener 可能锁定文件，即时删除会打断打开），不入撤销栈（只读用途）。
- **远端 `:find`/:grep（M22-5）**：无 gitignore 语义（远端不读 ignore 文件，行为差异记录在 SPEC 硬约束）；单文件 1 MiB read 上限 + 1 万条目上限复用，防大文件/深树拖垮事件循环。
- **README 里程碑表**：0.7.0 发布时仅同步状态行，表体停在 M11（M6 仍标"进行中"）——M22-7 补齐为文档维护项。
- **编号检查**：M22 未被占用（TASKS 最高 M21）✓。目标版本 0.8.0（minor：known_hosts + 口令认证为行为新增）。

### 2026-11-17 — M23 立项：Frecency 对齐增强与 Pane 模式

- **来源**：DeepSeek 分享讨论 `https://chat.deepseek.com/share/blcqlg4gg0qq7l209c`（浏览器渲染提取；页面正文仅渲染提问，回答经 `/api/v0/share/content` 端点取回）。两段讨论：① 对照 `wideweide/frecency`（用户参考项目，SQLite+bash+fzf）的差距分析；② Frecency Pane 模式设计。
- **编号映射**：讨论稿建议编号 "M19" 已被 SFTP 批次占用 → 按 §8.3 取下一空闲号 **M23**。
- **已对齐不动**（讨论稿明确）：events+snapshot+meta 三表、write-time+read-time 重放、dir/file 独立时钟、EMS 数学形式、pick 纯只读、keyword 元数据事件——全部维持现状，仅补"评分之外的工程语义"。
- **M23-2 现状核对口径**：NAV-1 的 keyword 事件是元数据事件（重放只应用 keyword 字段、不加分）——`:keyword -` 清除大概率已保留分数，M23-2 执行时先写对照测试验证，若已满足则登记验证后勾选（同 M21-E2 模式）。
- **per-topic lambda 默认值**（讨论稿）：unified 0.005（时钟混合计数 dir+file 事件）、dir/file 0.01——与参考项目默认一致；rvifm 现状 lambda 为全局常量，M23-5 落地 per-topic 配置时保持旧配置缺省兼容。
- **容量清理触发点**：record 写事务末尾（讨论稿语义"每次写入后检查"）；清理对象 = 非书签条目按 score 排序删除至 90%；带 keyword 永不删。
- **IGNORE_PATTERNS 用既有 glob 能力**：M21-E7 的 glob 匹配已在依赖树（自写 glob_match）；ignore crate 依赖的 globset 不直接引入——用自写 glob（模式语义 `*/.git` 需要 `*` 跨段？——实现时核对 glob_match 的 `*` 是否跨 `/`，不跨则需扩展，登记实现注意）。
- **Frecency Pane 模式（M23-6/7）**：讨论稿核心判断——frecency 从"浮层"改为"焦点 pane 的一种模式"，比 fzf popup 更贴合 rvifm 双 pane 架构；预览/主题/键位全部复用现有能力。c/v/e 仅模式内重映射（v 不得侵入 Visual）。View 全文上限 10 万行（对齐 VIEW-1/查看器语义）。
- **M23-8 可 defer**：讨论稿标注"中等优先级，视预算决定"——登记但标注 3 轮迭代上限即 deferred（AGENTS §3）。
- **执行顺序**：M22（已确认）先行 → M23 接续；两批完成后发布 **0.8.0**（用户指示"全部完成后发布新版本"）。

### 2026-11-18 — M23 追加调整（用户指示）

- **M23-7 修订（用户明确）**：Frecency Pane 键位**除 l/Enter 外与 Normal 一致**（作用于光标结果条目）——原 c/v/e 模式内重映射移除。理由：消除模式间语义分歧（v=Visual、c=rename 前缀不因进入 Frecency 而改变）；e 在 Normal 先落地（M23-10），Frecency 态直接复用。对侧全文查看经由既有 view_mode（w）覆盖。
- **M22-6 增强划入 M23-9**：远端文件打开后跟踪副本修改（notify 已在依赖树），防抖 2s 自动上传；上传失败不阻塞（下次修改重试）。M22-6 已提交的打开/清理逻辑不变。
- **y 复制序列（M23-10）**：vifm 对齐 y{motion} 家族（yn/yf/yc）；多选 = 标记或可视选择（M3 语义复用）；剪贴板 OSC52 零依赖优先（arboard 等 GUI 剪贴板 crate 视为可选后续）。
- **git 集成（M23-11/12）**：,g lazygit 为用户给定的示例与优先级规范（焦点 pane 仓库 → 高亮目录 → 拒绝提示）；内置变量以 vifm % 变量集为基线；git2/libgit2（C 依赖，aws-lc-sys 教训——需评估构建链）vs gitoxide（纯 Rust）在 M23-12 评估后决策，本轮只出决策不实现。
- **执行顺序调整**：M23-9（远端跟踪）→ 10（y*/e，Frecency Pane 的前置依赖）→ 11（自定义命令）→ 12（git 评估）→ 6（含修订后 7 的键位语义）→ 8 → 发布 0.8.0。

### 2026-11-18 — M23-12 git 集成评估（决策）

- **git2（libgit2 绑定）**：功能全面（status/branch/diff/stage）；但 C 依赖 libgit2-sys 需要编译工具链（aws-lc-sys NASM 教训——同风险），且默认构建新增 C 编译期依赖，违反零 C 编译原则（AGENTS §7 不引入 unsafe 无明确必要 + 构建链风险）。**不采纳**。
- **gitoxide（纯 Rust）**：功能逐步完善（read-only 较成熟，mutate 仍在开发）；纯 Rust 零 C 依赖 ✓；但 crate 体积较大且 API 尚在演进。当前无刚需场景（状态装饰/分支显示均为 UI 增强）。
- **决策**：本轮**不引入任何 git 模块**——状态装饰/分支显示可后续用 `git` CLI 子进程实现（零依赖，成本最低），不需要库级 API。若未来需要更深度集成（暂存/提交），gitoxide 是纯 Rust 首选。
- **集成点清单（后续立项参考）**：① 状态栏显示分支名（`git rev-parse --abbrev-ref HEAD`）② 列表条目 git 状态装饰（`git status --porcelain`）③ Frecency Pane 内 git 操作快捷键（stage/commit/push）。均为 UI 增强，优先级低。
- **M23-11 `,g` lazygit**：用户配置示例——TOML `run = "cmd:lazygit %g"`，%g 变量解析为 git 仓库根（非仓库 → 提示）。命令执行复用 M18-4 terminal分流。lazygit 本身是外部工具（用户自行安装）。

### 2026-11-19 — M24 立项：交互增强与列表规范

- **来源**：DeepSeek 对标分析（vifm/ranger/yazi 功能差距）+ 用户列对齐要求。
- **现状核对（去重）**：计数前缀（M1-5 `N gt` 已有基础）、目录优先排序（Config 已有）、大小写排序（Config 已有）、实时过滤（M21-E7 已有）、自定义命令（M23-11 已完成）、剪贴板（M23-10 OSC52 已完成）——均不重复立项。
- **列对齐（M24-1）**：用户明确要求——文件名左对齐+填充、大小右对齐人性化（format_size 复用）、日期左对齐分钟精度；左右 pane 共用；窄终端隐藏复用 M21-E1。
- **自然排序（M24-2）**：不引外部 crate（natord 等），自写数字感知分段比较器；`*` 跨 `/` 的 glob_match 已有可复用。
- **可视模式（M24-3/4）**：V 与 v 互斥、tag 集合复用（不新建选择结构）；d/y/x 复用 M2/M3 撤销栈。
- **deferred 项**（SPEC M24 OUT_OF_SCOPE）：块可视 Ctrl-v、符号链接创建、归档创建、Git 状态列、磁盘用量 :du、过滤历史循环、会话持久化——均登记不立项。
- **编号检查**：M24 未被占用（TASKS 最高 M23）✓。

### 2026-11-19 — M25 立项：会话持久化、marks、归档创建与 git 集成

- **来源**：用户批次需求讨论稿（一句话条目蒸馏；按 §8.3 确认前不执行）。
- **范围重开（用户指令，决策反转登记）**：
  - **marks**：原 OUT_OF_SCOPE（"路径跳转由 frecency 的 pick 覆盖"）。用户明确"类 vim 的 m' 单键记录导航还是要做"——实现为**独立模块**（不与 frecency 关联、不写 redb），AGENTS §5.1 已同步修订；TASKS"不在清单"中的 marks 条目已划掉。**键位冲突待确认**：`'` 已被 M16 NAV-1 quickjump 占用，默认方案 `'` 归 marks、quickjump 迁移至反引号 `` ` ``（vifm 同款 `'`/`` ` `` 分工）。
  - **归档创建 / Git 状态列 / 会话持久化**：M24 立项时登记 deferred 不立项，本批次重新打开，M24 OUT_OF_SCOPE 清单已同步移除。
  - **git**：M23-12"本轮不引入任何 git 模块"决策的延续而非反转——其评估结论原文即"状态装饰/分支显示可后续用 git CLI 子进程实现（零依赖，成本最低）"；本批次照此落地（git CLI 子进程），git2/gitoxide 维持不引入，无库级 API 需求（无暂存/提交场景）。
- **归档创建编码器边界（WHY）**：写端仅 zip / tar / tar.gz——zip 与 flate2（rust_backend）纯 Rust；`.tar.xz`/`.tar.bz2`/`.tar.zst` 的主流编码器（xz2/bzip2/zstd）均为 C 绑定，违反零 C 编译期依赖原则，三者保持只读（M7-8 读端不变）。
- **会话持久化（WHY）**：存储 `<配置目录>/session.toml`（与 config.toml/remotes.toml 同目录，跟随 config_path() 约定）；仅还原物理 Location——远端会话需重认证、归档属临时浏览态；损坏静默降级——会话文件属可丢状态，无备份/迁移机制。
- **校验文件默认 md5（REQ 原文）**：`<名>.md5` md5sum 兼容行格式；md-5/sha2 均纯 Rust（RustCrypto 系，零 C 依赖）；sha256/none 经命令参数与 `[archive] checksum` 配置可选。
- **gi 优先级（REQ 原文）**：gitui → ziggity → lazygit，首个 PATH 命中者挂起 TUI 启动（M23-11 run 分流复用）；三者均为外部工具，用户自行安装。
- **frecency 通用化（M25-7，评估任务）**：用户提出"记录及评分查询方法需更通用，比如命令支持书签，路径使用目录数据及当前路径数据"——按 M23-12 同模式登记：只出决策不实现。评估点：EntryKind 扩展命令类事件、keyword 书签泛化到命令、补全候选 = 目录 frecency + 当前路径邻近加权的 schema 影响与迁移成本。
- **编号检查**：M25 未被占用（TASKS 最高 M24）✓。版本目标当时记为 0.10.0；**已于 2026-11-19 统一为 0.9.0**，随任务列表全部完成后自动发布。

### 2026-11-19 — M26 立项：归档创建扩展与外部工具降级（用户讨论稿蒸馏 + 与 M25-3/4 合并评估）

- **来源**：用户讨论稿（纯 Rust 内建 + 外部 CLI 降级 + ArchiveWriter 抽象 + 后台作业化）；讨论稿把批次编为"M24"并建议排期 v0.6.0——M24 已关闭、v0.8.0 已发，按下一空闲号 **M26** 登记（编号检查 ✓，TASKS 最高 M25）。
- **技术方案冲突评估（讨论稿 vs 已上线 M25-3/4，逐项择优）**：
  - **命令名 `:archive-create` 否决**：`:archive` 已上线且格式/校验/对话框齐备，沿用现名。
  - **ArchiveWriter trait 抽象层否决（YAGNI）**：枚举分派 + pack_* 函数满足现状；ArchiveFs 是读 VFS trait，写端不构成对称需求；格式数少，间接层无收益。
  - **后台作业化采纳**：当前 :archive 同步执行阻塞 UI（大目录/大归档），迁移 M13-8 Job 模型（:tag copy/move 同款），:jobs 可见可取消（M24-9）。
  - **纯 Rust 编码器主张核实（crates.io API，2026-11-19）**：
    - `sevenz-rust 0.6.1` ✓ 纯 Rust 7z 压缩/解压（2024-07 后未更新——codec 类 crate 格式稳定，2 年未更新可容忍）→ **采纳**（创建 M26-2 + 浏览升级 M26-3）。
    - `lzma-rust 0.1.7` ✓ 存在但仅为 LZMA/LZMA2 codec，**不产出 .xz 容器格式**（流头/块头/索引/尾框需自实现）→ 否决。
    - `yolowzstd 0.1.2` ✓ 纯 Rust zstd（2026-11 活跃）但 0.1.x 过年轻 → 暂不采，观察项。
    - bzip2 纯 Rust 编码器**不存在**（bzip2-rs 仅解码，2021 后未更新）→ 讨稿"bzip2-pure"主张有误。
    - `oxiarc` 未在 crates.io 找到（讨论稿有误）；`libarchive_oxide 0.2.0` 存在（统一创建/解压，0.2.0 年轻）→ 列观察项，与讨论稿"优先单一职责成熟 crate"结论一致。
  - **xz/bz2/zst 创建路线（M25-3 边界修订）**："保持只读"修订为"内建不支持、走 M26-4 外部 CLI 降级"——纯 Rust 编码器不可用（上述核实）而外部 CLI（7z/rar）是零编译期依赖下唯一稳健路径；M25-3 的可读提示保留为 CLI 缺失时的终止文案。
  - **7z 浏览（追加，超出讨论稿）**：sevenz-rust 同时提供读端——M7-9 的 7z"仅提示解压"可低成本升级虚拟浏览，登记 M26-3。
- **TASKS 重排（用户指令）**：TASKS.md 各节改为编号升序（原 M15/M16/M18–M20 排在 M24/M25 之后为历次插入所致）；M23-9～12 从 M24 节归位到 M23 节；M4 节 marks 历史注释同步 M25-2 现状。仅调序不改内容。
- **版本**：当时记为 0.10.0；**已统一为 0.9.0**（任务列表全部完成后自动发布，不按批次拆版本）。

### 2026-11-19 — M25-7 frecency 通用化评估（决策；评估任务不实现）

**结论：可行，迁移成本低——建议按需立项（M26 候选）。** 三个评估点与 schema 影响：

- **命令历史迁移（M24-6 → frecency cmd 事件）：可行，收益最大。**
  - 记录：`EntryKind` 增 `Command` 变体（serde 新变体向后兼容，旧数据无此变体）+ topic 扩展 `"cmd"`（topic 本就是 String 非枚举——**零迁移**）；`path` 字段承载命令文本（String，无路径解析语义，安全）。record/pick/replay/compact/IGNORE_PATTERNS/容量清理（M23-3）全链路基建直接复用。
  - 查询：EMS aging + per-topic λ（M23-5 FrecencySettings 加 cmd 档）+ grade_for 前缀分级（M23-1）天然支持"输入前缀 → 按使用频率排序"——比 M24-6 的 FIFO 浏览强（常用命令置顶）+ 跨会话持久（会话内存 → redb）。
  - UI：↑/↓ 浏览交互不变，数据源从内存 Vec 换 replay 顶层 N 条；M24-6 的"成功才记录"语义映射为 record 写入条件。
- **路径补全加权（M24-7 → 目录数据 + 当前路径邻近）：可行，成本低。**
  - 候选仍来自焦点 pane 条目（不引入全库模糊搜索）；排序键 = 字典序 → 改为「frecency 分数（Navigate/Open 事件）优先、无记录条目字典序兜底」。分数从 TUI 启动 replay 快照读（pick/TUI 同源只读，M8-8 语义 ✓，不额外写库）。
  - 数据结构：`CmdComp.candidates` 升为 `Vec<(String, f64)>`；store 缺失/空 → 全量兜底字典序（零成本路径）。
- **命令书签（keyword 泛化）：schema 零成本，消费交互需新设计。**
  - keyword 事件本就是 (path, topic, keyword) 三元组——topic="cmd" 即命令书签；Grade 7 级书签命中逻辑直接生效。
  - 唯一新设计点：消费入口（如 `:` 模式内 `<书签字符>` 展开为完整命令，或 `:k<char>`）——留实现批次定。
- **迁移成本估计**：约 1 个批次 5～7 任务（cmd 事件 schema + record 链路 / : 模式数据源 / 补全加权 / 命令书签交互 / 验收发布）。
- **边界与隐私（CUT）**：命令文本持久化入库——IGNORE_PATTERNS 需扩展语义到 cmd topic（glob 匹配命令文本）供用户排除敏感命令；文档明示"命令文本写入 redb"。SIMPLIFIED 第一期 pick 不加命令视角（仅 TUI 命令行消费）。反面理由（不迁移）：M24-6 刚落地、当前痛点不足——**是否立项由用户指令决定**。

### 2026-11-19 — M25 调整（用户指示）：M25-2 暂停 + git 状态列性能方案修订

- **M25-2 marks 定案**：撤销“暂停 / 独立模块”，改由 frecency 实现并持久化（2026-11-19 用户指令）；TASKS 中重开任务，排在 AUDIT 之后。
- **git 状态列性能（回应"gitui 为何大仓库仍快"）**：
  - gitui 的体感快来自三层：① **异步架构**——status 在后台任务计算，UI 先渲染后填充，慢查询不等于慢界面（体感最大来源）；② **libgit2 进程内调用**（git2-rs）——无子进程 spawn / porcelain 文本解析开销，且可按需裁剪查询范围；③ **缓存 + 事件驱动刷新**——非每帧重算。大仓库场景还隐含 git 自身配置收益：`core.fsmonitor`（daemon 只 stat 变更文件，跳过全树遍历）、`core.untrackedCache`（缓存未跟踪判定）、index 的 stat 缓存（未变更文件 O(1) 判定）。
  - `git status` 本身慢的根因：必须 stat 全仓工作树 + 对 stat 变化的文件重算哈希 + 枚举未跟踪文件（gitignore 匹配）。子进程方式下还有 spawn + 文本序列化成本。
  - rvifm 落地（CLI 约束不变，M23-12 结论维持）：**异步后台查询**（复用 M23-9/M13-8 线程模式）+ **pathspec 限定当前目录**（`-- <dir>/`——只扫当前子树，比全仓扫描便宜一个量级；我们只需要直接子项，查询范围天然小）+ `--no-optional-locks`（浏览不写 index、不与用户其他 git 操作抢锁）+ per-目录缓存。SPEC M25-5/6 已按此修订。
  - **不引入 gitoxide**：gix status 有并行扫描优化，但依赖树大、API 演进中（M23-12 评估不变）；CLI + 异步在"每次导航一次、只需直接子项"的场景足够。
  - 超大仓库用户侧建议（README 提示，rvifm 不代改用户 git 配置）：`git config core.fsmonitor enable` + `git config core.untrackedCache true`。

### 2026-11-19 — 全面评估（REVIEW）与文档漂移整改（WHY）

- **来源**：用户要求对 harness 文档与代码做严肃评估，报告见 [REVIEW.md](REVIEW.md)。结论已按 AGENTS §8 投递：REQ → SPEC AUDIT 段；SEQ → PLAN/TASKS AUDIT 批次；HOW → ACCEPTANCE §26；WHY → 本节。
- **为什么先修文档**：`STATE.md` 出现双 header 且“下一步”停在 M10–M12，会直接误导 Agent 重做已完成任务；README 对外宣称的选型与实现不符，会误导用户。因此把“恢复锚点唯一化”置于功能整改之前。
- **为什么把 `pick` 只读列 P0**：AGENTS §5.3 明令 `pick` 不写 redb、不更新 tick；当前 `open_store()` 的 pending 合并使其条件性违反红线。这是规范内部“下次启动合并”与“pick 纯只读”的冲突，必须用户拍板合并点，不能由实现猜测。
- **为什么把能力缺口登记为任务而不是直接改代码**：本次是评估任务，用户明确要求“不提交、等确认”；代码/CI 改动涉及行为与发布风险，故只登记 AUDIT-1～12，待确认后按 P0→P1 执行。
- **未采纳的建议**：未重写 `src/app.rs`、未改 CI、未动依赖，原因是评估范围止于文档与结论；相关模块已在 REVIEW 中给出拆分/修复方向。

### 2026-11-19 — marks/bookmark 统一 frecency 化 + 发布/续轮规则调整（用户指令）

- **marks 实现反转（WHY）**：撤销“独立模块 + 暂停”方案。`m<char>` / `'<char>` 与 `:bmark` / `:bmarks` 等 bookmark 命令共用一份 frecency redb 数据（keyword + 频率记录 + 评分排序）。理由：单字符 marks 与 keyword bookmark 在数据语义上都是“路径 → 标签 → 评分”，无需两套存储；统一后还能带频率排序。
- **命令对齐（已确认）**：vifm 官方命令为 `:bmark <tag> [tag2...]`、`:bmarks [tag...]`、`:bmgo [tag...]`、`:delbmarks [tag...]` / `:delbmarks! <path...>`；marks 键位为 `m<char>` / `'<char>`（合法字符 `[a-z][A-Z][0-9]`）。**keyword 按长度分工**：单字符用于 `'<char>` 两键导航；多字符用于 `o` / `:j` / `:J` 交互搜索时的高优先级匹配（如 `o` → 输入 `rvifm` → Enter，在当前 pane 跳到该 keyword 对应路径/文件）。不建第二套索引；`:bmark` 系列为 vifm 对齐视图，`:keyword` 保留为兼容命令。已写入 SPEC §25/§6.1。
- **发布规则**：从“阶段性任务完成后人工决策”改为“`TASKS.md` 全部任务完成后自动发布”（版本 bump + CI + crates.io + tag/Release），已写入 AGENTS §3 与 PLAN §4。
- **续轮规则**：强化为“一轮提交后立即启动下一轮，不得输出等待确认提示”；仅质量门禁失败、无法继续的异常才暂停；必须人工执行的验收改为 deferred 跳过，已写入 AGENTS §3。
- **预算/环境**：取消 token 预算估算（已不适用）；本机以实际环境为准，其它平台交给 CI。
- **2026-11-19 续轮前置决策（用户确认）**：compaction 阈值 5000 条、提供 `rv` 短别名、M26 批次启动；三项已写入 SPEC §6.1，§6.2 清空。至此所有已知待定项定稿，续轮无决策中断点。

### 2026-11-19 — AUDIT-1 方案拍板（用户确认）

- **背景**：`pick` 复用 `open_store()` 会在存在 `events.pending` 时合并事件，写 redb 并推进 `global_tick` / 类型时钟，违反 AGENTS §5.3 “`pick` 纯只读”。
- **结论**：采纳方案①——`events.pending` 合并只在 `record` 与 TUI 启动路径执行；`pick` 改用只读 open，不写 redb、不更新任何 tick。方案②（保留合并、削弱只读承诺）放弃。
- **WHY**：只读红线是 frecency 语义正确性的基础（记录的是“已发生的访问”）；合并是可用性降级路径，不能反过来破坏只读契约。
- **执行**：AUDIT-1 为 AUDIT 批次首个任务，验收见 ACCEPTANCE §26（存在 pending 时执行 `pick`，DB mtime 与 `global_tick` 不变）。状态已同步至 SPEC §6.1、TASKS、STATE、PLAN、AGENTS §10。

### 2026-11-19 — M27 需求评估：符号链接 / vifm 配色 / PDF 页预览（WHY）

- **来源**：用户 0.8.0 实测反馈（三项）；本轮只评估并登记 SPEC/TASKS/ACCEPTANCE/PLAN/STATE，不实现代码，待用户确认。
- **符号链接根因**：`tab::load_entries` 用 `DirEntry::file_type()`（lstat，不跟随链接）判定 `is_dir`，symlink-to-dir 得 `false`；`app::open_selected` 对 `!is_dir` 走 `system_open()`，于是系统文件管理器打开。修复方向：用 `fs::metadata`（跟随）判定目录，失败回退 `DirEntry::metadata`（断链）；`is_link` 保留用于着色；目录优先排序、目录树、预览的目录判定一并统一。
- **vifm 配色调研**：vifm 不用每 pane 整框边框；`Border` 只表示垂直分隔；`TopLine`（非焦点 pane 顶部路径行）/`TopLineSel`（焦点）、`TabLine`/`TabLineSel`、`StatusLine`/`CmdLine`、`CurrLine`/`OtherLine`、`Selected` 用背景色区分。Default.vifm 关键值：TopLine 黑字白底、TopLineSel 黑字默认底加粗、StatusLine 黑字白底、Border 黑字白底、CurrLine 默认字蓝底、Selected 品红加粗。rvifm 现状是每个 pane `Block::bordered()` 整框 + 标题，焦点靠边框 bold/dim，tab 用 REVERSED，与 vifm 差距大。
- **决策 M27-2**：默认 `[ui] pane_borders = false`（vifm 式）；`true` 保留旧整框。默认用 vifm Default 风格的 16 色回退，主题命名样式存在时覆盖；不实现 vifmrc、`:highlight`、目录级 colorscheme。依赖 AUDIT-13，避免再次“有样式无加载”。
- **PDF 调研（yazi）**：yazi 默认 `pdf.lua` 用 `pdftoppm -f N -l N -singlefile -jpeg ...` 逐页转图并缓存，preloader 预热；poppler 缺失或 Windows winget 版缺 `-jpeg` 时报错。rvifm 改用 `-png`（`image` crate 已支持，避开 `-jpeg` 平台差异），外部工具逐页转 PNG 后交 `ImageEngine`。
- **PDF 与硬约束**：AGENTS §5.5 要求“文本提取 → exprim → pdftoppm/mutool”，不得跳过文本提取。用户“优先图片、失败文本”解释为**展示优先级**：`pdf_prefer_image = true` 时仍先执行 PDF 文本提取，再优先显示图像；失败回退已提取文本。若要求完全跳过文本提取，必须修订 AGENTS §5.5，本轮不做。`pdf_image_pages` 默认 1（文本层不足时使用），0 关闭，2 在预览区上下显示两页。
- **范围收缩**：M27-1 不实现链接创建/编辑；M27-2 不做完整 vifm colorscheme；M27-3 不做任意页跳转/缩略图/OCR/全页渲染；`pick` 的 PDF 图像预览归 AUDIT-3 决定。
- **顺序**：M27-1 是用户实测功能 bug，如启动建议优先于 AUDIT-1；M27-2/3 依赖 AUDIT-13/2。
- **用户决定（2026-11-19）**：M27 批次暂不启动，仅保留登记；执行顺序不变，AUDIT 批次继续。登记文档已随 `981e0bb` 提交（本条原文“保留在工作区未提交”已过期，勘正）。

### 2026-11-19 — 第二轮复核：AUDIT 扩至 17 项（WHY）

- **来源**：用户要求对 harness 与代码做第二轮严肃评估；上一轮报告已归档 `docs/reviews/REVIEW-2026-11-19.md`，新结论见 `docs/REVIEW.md`（基线 `9532cd0`）。本轮不采信上一轮结论，全部重新核验。
- **为什么新增 AUDIT-13（主题接线）**：`theme::set_builtin` / `set_user_theme` / `discover_user_themes` 有零个生产调用方，`ACTIVE` 从未写入；`style()` 恒返回默认样式。M9 只完成了“引擎 + 单测”，而 ACCEPTANCE §11 的“内置主题可加载、运行时切换、用户主题可发现”均不成立。把它当作“文档改小”会掩盖真实功能缺口，故登记为代码任务。
- **为什么把 AUDIT-7 从“补 rules 层”扩为“整层接线”**：代码事实是 `OpenerConfig` 无 TOML 来源、无 `mime`、无执行器，`resolve_opener` 仅测试可达；只补一个字段仍然不会生效。另发现 `glob_match_simple` 的 `*x*` 分支先返回，永远无法命中，归入 AUDIT-10。
- **为什么新增 AUDIT-14（测试隔离）**：`run_external_target` 读取 `TMUX`/`STY`，测试未隔离；本机 tmux 内 `cargo test` 336 项中 1 项失败，清除变量后全绿。CI 无 `TMUX` 因而长期看不见，属于“门禁失真”而非单纯 flaky。AGENTS §7 已同步固化“环境变量影响行为的测试必须隔离”。
- **为什么新增 AUDIT-15（pick CLI）**：SPEC §4.6 的示例 `rvifm pick --topic dir` 当前会把 `--topic` 当查询词（已复现无输出、退出码 1）；所有候选仍按 `dir` topic 评分；数据目录不存在时直接返回裸 `NotFound`。这是对外契约，不是内部实现细节。
- **为什么新增 AUDIT-16/17**：`cmd_compact` 的 `replay_merge` 硬编码 `AGING_LAMBDA=0.01`，与 unified 默认 0.005 冲突；`cmd_pick` 不初始化 `[frecency]` 设置。归档读取无成员大小上限、tar.xz 全量解压，与“内存受最大单成员约束”的注释/SPEC 不符。
- **为什么把 M0–M25 改成 M0–M24 + M25 主体完成**：State/TASKS 中 M25-2 仍未勾选；继续写“全部关闭”会让下一轮 Agent 跳过 marks。M9/M11 同样改为“部分完成/引擎完成”，并指向具体 AUDIT 项。
- **编号修正**：原已撤销的 `M25-2` 改名 `M25-2a`，重定义条目保留 `M25-2`；避免提交信息与任务检索歧义。
- **未做**：未改任何代码、未改 CI、未动依赖；本轮只做文档与任务登记，等用户确认后再按 AUDIT 顺序执行（AUDIT-1 → 14 → 9 → 15 → 7/13 → 2/3/4/16 → 其余）。
- **风险**：文档现在把“未接线”写在明面上，短期会显得完成度下降；这是为了避免第三轮继续在错误前提上排期。若用户希望保留“已完成”口径，应改为“引擎完成、接线待办”，不得恢复“能力已完成”的表述。

### 2026-11-19 — PROC-1 生效与 M27 排队确认（用户指令）

- **PROC-1**：用户复核预算门控协议蒸馏准确性（逐条对照源规则与 AGENTS §3 / ACCEPTANCE §1）后确认生效。「自动续轮」停止条件 3（剩余 token / Credits 不足以完成最小可落地成果 MDO）即时生效；断点落地流程见 AGENTS §3，命中暂停时的验收自检见 ACCEPTANCE §1。
- **M27**：撤销「已登记，暂不启动」标记，按 TASKS 顺序排队（AUDIT → M25-2 → M26 → M27 → MISC-1）；TASKS / STATE / PLAN 状态已同步。执行仍从 AUDIT-1 开始，M27 不插队。

### 2026-11-20 — AUDIT-1 实现：pick 真只读 open（WHY）

- **只读打开选型**：`Store` 改持 `DbHandle` 枚举（`Rw`=redb `Database` / `Ro`=redb `ReadOnlyDatabase`，redb 4.5 两类句柄 `begin_read` 同型）。仅移除 pending 合并不够——`Store::open` 的建表写事务本身会触碰文件，mtime 验收过不了；RO 句柄还在类型层拒绝 `begin_write`（pick 误写的编译期防护）。
- **合并归属（方案①落地）**：删除 `open_store()`；pending 合并只在 record 写路径（原样）与 TUI 启动钩子 `merge_pending_startup()`（新增，`app::run` 初始化时静默执行）。`cmd_pick` 与 `load_jump_entries`（o/:j/:J/quickjump/Recent 共用）改走 `open_store_readonly()`。
- **库缺失降级**：RO 打开缺失库报 NotFound → pick 全 0 分、跳转选择器空列表（AUDIT-15 再细化 CLI 契约）。此前 pick 会**创建**库文件——同样是写行为，一并消除。
- **cmd_compact 保留"读写打开 + 先合并"**：维护写命令，建表与合并属其职责，行为不变。
- **并发语义变化（良性）**：pick 由独占锁（RW）变共享锁（RO）。record 与 pick 并发时 record 得 `DatabaseAlreadyOpen` → 降级 `events.pending`（与改动前一致）；多个 pick 现在可并发（改进）。
- **门禁偏差（登记 AUDIT-18）**：本机 HEAD 上 `m249_jobs_overlay_render_selection_and_cancel_state` 失败——作业行拼接宿主长临时路径（`ADMINI~1\...\Temp\2\`）超 70 列被截断丢"取消中"，测试违反 AGENTS §7 路径可移植规则；`m253_archive_zip_roundtrip_and_undo` 失败——第二次 undo 未移除归档（根因待查）。`git stash` 验证两项在干净 HEAD 同样失败，与 AUDIT-1 无关。本地门禁暂用 skip 命令（见 STATE §质量门禁），其余 326 单测 + 3 集成全绿。Linux cross-check 自 russh（M19-3b）起本机无法构建 aws-lc-sys，由 CI 承担（现状确认）。

### 2026-11-20 — AUDIT-14 实现：测试环境隔离（WHY）

- **失败机制**：`run_external_target` 直接读宿主 `$TMUX`/`$STY`；tmux 内 `tmux new-window` spawn 成功 → 返回 `Multiplexer` 而非 `RunExternal`，`m18_4_bang_routes_and_config` 断言炸；且 `echo hi` / tmux 的输出泄漏进测试 stdout。
- **修法（不走测试改环境变量路线）**：edition 2024 下 `env::set_var`/`remove_var` 是 unsafe，测试内清环境不可取。改注入式——`detect_mux()`（生产探测）→ `run_external_target_with(mux, cfg, cmd)`；测试只注入会短路到 `CurrentPane` 的组合（`Some+true` 会真实 spawn 复用器，不覆盖），exec 路径用 `state.config.ui.terminal_in_new_window = false` 与宿主解耦；成功命令换静默 `cmd /C exit 0`；复用器 spawn 输出 `Stdio::null()`（M21-F8 同款）。生产行为不变（仅复用器输出静音）。
- **验收**：`--test-threads=1` 在 `TMUX=1 STY=1` 与清除两变量下各全绿（326 单测 + 3 集成，AUDIT-18 两项按 STATE 偏差 skip）。
- **观察（待 AUDIT-12 复核）**：`TMUX=1` 并行 4 跑中出现一次未捕获测试名的偶发失败，随后 3 连绿未复现；不影响 AUDIT-14 验收结论，记录备查。

### 2026-11-20 — AUDIT-18 实现：两项本机测试失败根因与修复（WHY）

- **m249（长路径截断）**：作业行断言渲染 70 列，目标串用宿主临时路径（`ADMINI~1\...\Temp\2\.tmpXXXX` ≈54 字符）拼上"取消中"后越界截断。修复：`add_job` 第三参仅作展示串、不触 fs，测试改固定短标签 `d`——断言与宿主路径彻底解耦。生产行为不变（真实长路径的覆盖层截断属 UX 打磨，不在本任务）。
- **m253（二次 undo 不删归档）**：根因是**测试**的 `open_dispatch(&dest)`（ZipFs 持有打开的 `std::fs::File`）跨 `undo_last` 存活——Windows 上打开句柄令 `fs::remove_file` 共享冲突失败，`undo_last` 失败回推（app.rs）→ 第二次 undo 重复弹同一记录再失败；Linux 允许删打开文件故 CI 绿。修复：读句柄在撤销前 drop（作用域块）。生产删除路径本就正确（失败回推 + "撤销失败" 可读提示），无需改动。
- **结果**：全量 `cargo test` 无 skip 全绿（328 单测 + 3 集成），`TMUX`/`STY` 设/不设双验证；STATE 门禁偏差除 Linux cross-check（本机无法构建 aws-lc-sys，CI 承担）外全部解除。

### 2026-11-20 — AUDIT-9 实现：CI 门禁增强（WHY）

- **改动**：ci.yml `clippy` 步加 `--all-targets`（测试代码此前不在 clippy 覆盖内）；新增 `check linux target (all-targets)` 步，挂 `matrix.os == 'ubuntu-latest'`——ubuntu 上 host triple 即 `x86_64-unknown-linux-gnu`，等价原 AGENTS §6 本地门禁；Windows runner 无 linux-gnu 交叉 gcc，不挂该步。
- **本机限制固化**：AGENTS §6 同步改为"该检查以 CI 为准，本地不再强行执行"——本机自 russh（M19-3b）起 aws-lc-sys 无法交叉构建（无 cross gcc），此前该门禁事实上已由 CI 承担（见当日 AUDIT-1 条目）。

### 2026-11-20 — AUDIT-15 实现：pick CLI 契约（WHY）

- **`--topic dir|file`（与 record 对称）**：原实现把所有候选硬编码为 dir topic（`score_of(path, "dir")`）——文件候选永远拿 dir 分数或 0。重构为 `parse_pick_args`（--topic + 位置查询，缺省 dir）+ `pick_score_candidates`（纯函数，按传入 topic 查分，无对应条目 0 分、不跨 topic 兜底）+ `pick_ranked_entries_at`（只读加载，库缺失 → 空表全 0 分——AUDIT-1 的 `open_store_readonly` 降级在此落成 CLI 契约）。
- **测试分层**：单测覆盖解析/评分纯函数与缺失库降级；CLI 集成测试（tests/cli.rs）以 `LOCALAPPDATA`/`XDG_DATA_HOME` 子进程重定向数据目录，`record` 播种 dir+file 事件后断言双视角排序——宿主真实数据零接触。SPEC §4.6 示例（`rvifm pick --topic dir < <(fd -t d)`）自此真实可用。
- **"与 AUDIT-3 的预览承诺统一"**：本任务只动评分与 CLI 契约，pick 预览留与 AUDIT-3 决定，无交叉改动。

### 2026-11-20 — AUDIT-7 实现：opener 整层接线（WHY）

- **Schema 按 SPEC §4.9（yazi 风格）**：`[opener]` 命名定义（run 模板含 `%s` 占位、block、desc）+ `[open]` 规则（mime/name/use）。`use` 名单按序执行：`open` = 系统 opener 内建名，未知名跳过，名单空/全未知回退系统层；`block=true` 经 `state.pending_external` 交 run 循环挂起 TUI 执行（与 `:!` 共用新提取的 `suspend_execute`），`block=false` fire-and-forget（IO 静默，不等待退出）。
- **mime 内建表（SIMPLIFIED）**：不引 mime_guess 等新依赖（M23-4/M23-5 先例——无明确需要不加 crate）；内置 ext→mime 映射覆盖常见类别（text/image/audio/video/pdf/archive），未知 → `application/octet-stream`；`image/*` 前缀通配、精确值大小写不敏感。长尾类型规则作者用 `name` glob 命中。
- **规则匹配语义**：链序 prepend →（rules 覆盖默认规则表——默认为空，故 rules 单独生效）→ append，首个命中生效；单规则内 mime 先判、name 回退。
- **接线范围**：`app::open_selected`（l/Enter/鼠标/右键菜单共用）物理文件路径接入；M22-6 远端临时副本打开仍走 system_open（远端文件的规则匹配属后续增强，不在本任务）。
- **shell_quote 迁移**：M18-4 的 app 内部实现迁至 `opener::shell_quote`（pub(crate)），tmux 窗口命令与 `%s` 展开共用同一 POSIX 转义，消除重复（AUDIT-10 的 glob 去重另有其任务，`*x*` 缺陷未动）。
- **frecency 记录边界**：block=true 转交时退出码未知，不记 file_open 访问（避免记入未发生的访问——`pick` 只读红线的同源语义）。

### 2026-11-20 — AUDIT-13 实现：主题选择接线（WHY + 真 bug）

- **激活入口补齐**：`[ui] theme`（UiConfig 去 Copy 增 Option 字段，5 处拷贝点改 clone）启动激活 + `:theme <name>` 运行时切换臂（`activate` 内置优先→用户目录，无参列全量，错误可读）；`..` 行/远端/结果视图/树视图四个守卫放行（UI 全局只读操作）。渲染消费早已就绪（M21-F1 `filetype_style` 经 `theme::style` + 16 色回退；`icon_theme()`/预览明暗实时读活跃态）——REVIEW 所述"零调用方"指激活入口缺失，接线即生效。
- **真 bug（接线时发现）**：`ACTIVE` 用 `OnceLock` 只能写一次——`:theme` 第二次切换静默失效（`let _ = ACTIVE.set` 吞掉 Err）。升级 `RwLock<Option<ThemeState>>`（写失败仅毒化场景，UI 保持原样）；AUDIT-13 语义"运行时切换"要求可重复替换，OnceLock 设计与 M9 启动单次激活的假设绑死。
- **测试隔离**：主题激活改变进程级样式（图标/列表转 Rgb 前景），污染颜色敏感渲染测试（m21e3 "Rgb==语法着色" 断言）。新增 `theme::TEST_LOCK`（cfg(test) 共享锁）串行激活类测试与 m21e3，激活后 `reset_active()` 复位基线。
- **观察**：`large_directory_load_and_render_fast`（计时类）在 5 次全量中出现 1 次超时失败，单跑 3/3 过——负载敏感 flake，与主题改动无关；若复现频繁再登记整改（AUDIT-12 可吸收）。

### 2026-11-20 — AUDIT-2 实现：PDF 视觉链最小接线（WHY）

- **顺序合规**：AGENTS §5.5"不得跳过文本提取"——`cached_visual_png` 仅在 `preview_pdf` 判定 `NeedsVisual` 时进视觉链；`Text`/`Unavailable` 返回 None 走既有文本/提示路径。
- **视觉链（第 1 页）**：`render_first_page_png_with` 按序 exprim（仅 pdf-visual，M11-13 护栏内，失败落外部工具）→ `pdftoppm -png -f 1 -l 1 -singlefile` → `mutool draw -o`（外部工具 IO 静默，M21-F8 同款）；产物校验存在且非空。PNG 编码走 `image` crate（内存编码后落盘，避免半成品）。
- **每帧安全**：查看器逐帧渲染——`VISUAL_CACHE`（含**失败缓存**）保证外部工具探测/spawn 每路径仅一次；否则无工具环境每帧 spawn 两次。临时 PNG 进程内原子计数命名；清理随 M27-3 统一（其验收含"预览切换/退出清理"）。
- **接线点**：viewer（w/预览窗格）PDF 臂拆分——视觉命中走 `ImageEngine` 渲染缓存 PNG（标题"PDF 预览（第 1 页图像）"），渲染失败回退可读提示；pick 的 PDF 视觉预览归 AUDIT-3。注入式测试（`_with(None)`）保证 CI 无工具环境确定性。

### 2026-11-20 — AUDIT-17 实现 + AUDIT 批次关闭（WHY）

- **AUDIT-17 归档上限**：zip/tar 单成员读取前按**声明尺寸**守卫（64MB，先于缓冲分配——zip 的 `with_capacity(size)` 分配炸弹在读取前拦截）；tar.xz 解压走 `CappedWriter`（256MB，解码途中拦截压缩炸弹，而非解压完再检查）；`Decoder::open` 注入式 cap 支持小体积端到端炸弹测试（64B 解压 > 8B 上限 → 可读 Err）。守卫函数纯化（`check_member_size`）便于边界测试。
- **批次关闭**：AUDIT 18 项全部完成（2026-11-20，17 个整改项 + 门禁复现新增的 AUDIT-18）。9 项"未接线"能力（opener/主题/PDF 视觉/pick 预览+CLI/惰性 compaction）全部落地；两个真 bug 随整改修复（ACTIVE OnceLock 二次切换失效 → RwLock；Store::open 数据目录缺失 → 自动创建）；357 单测 + 5 集成双环境绿。M9/M11 的"引擎存在、未接线"状态自此**计入实质完成**（STATE/PLAN/README 已同步）。
- **下一阶段**：M25-2（marks 由 frecency 实现）→ M26（归档创建扩展 5 项）→ M27（实测问题与视觉对齐 3 项）→ MISC-1（rv 别名）→ 全部完成后自动发布 0.9.0（PLAN §4）。

### 2026-11-20 — M25-2 实现：marks/bookmark 由 frecency 承载（WHY）

- **数据层**：keyword 事件（EntryKind::Keyword）本就是 marks 的持久化形态——新增 `set_keyword_topic_at/now`（topic 参数，marks 支持 file 条目；旧 `set_keyword_at` = dir 委托保持兼容）。
- **键位**：`m<char>` 经 `'` 同款"通配序列"机制（handle_key 特判 + 第二键分支）；合法字符 = 字母数字（vifm 口径）；`..` 行 = 当前目录；仅物理视图（归档/远端/结果/树视图不记录——keyword 事件写本地路径）。
- **跳转**：`quickjump_target` 扩展为**不限 topic**（dir/file 都参与，最高分）；`'<char>` 经 `jump_to` 分派——目录 cd、文件父目录定位光标（M8-12 语义复用）。旧实现只取 dir 且直接 navigate_to，文件条目被排除。
- **命令族**：`:bmark <tag>...`（当前目录设 tag，tag 限字母数字——多字符 tag 即"多字符 keyword"，在 o/:j/:J 搜索享高优先级，M23-1 既有）；`:bmarks [tag...]` 列表；`:bmgo <tag>...` 按序跳转；`:delbmarks [tag...]` / `:delbmarks! <path...>`——清除 = 空 keyword 事件（保留累积分数，与 `:keyword -`/M23-2 同语义）。
- **验收勘正**：ACCEPTANCE M25-2 原文"无 frecency store 写入"为 2026-11-19 反转前的旧定义残留（marks 曾计划只读），已按反转后的 SPEC §25 修订并注记。
- **测试边界**：marks/bmark 写路径触全局 redb——单测只覆盖校验与非物理拒绝（不触 DB），写入语义由 frecency 模块注入式测试覆盖；`m252_jump_to_file_semantics` 纯 fixture。

### 2026-11-20 — AUDIT-3 实现：pick 同源预览（决策 + WHY）

- **决策：保留承诺并接线**（撤销"文档撤回"选项）。`draw_pick_ui` 抽出（TestBackend 可测）：查询行 + 候选列表 + 右侧预览窗格；≥60 列双栏（1:1），窄终端降级单栏。预览源与 TUI 完全一致——文本/目录/Office/PDF 文本走共享 `render_preview`，图像走 `ImageEngine::detect()`（协议探测 + halfblocks 回退）；同进程完成，不 fork/exec（SPEC §4.9）。
- **PDF 视觉不入 pick**（M27-3 遗留决策点）：pick 是纯只读选择器（AGENTS §5.3），PDF 视觉需要临时文件 + 外部 spawn/exprim——选择器内做视觉渲染超出其职责；pick 的 PDF 预览保持文本层（同源 `render_preview`）。TUI viewer 已有视觉链（AUDIT-2）。
- **测试可移植性**：候选全路径长度随宿主变化——宽终端用例 200 列（列表列 100）保证叶子名不截断；窄终端用例不断言列表内容（40 列必截断，列表渲染已由宽用例覆盖）。AUDIT-18 同款考量。

### 2026-11-20 — AUDIT-5 实现：app.rs 上帝文件拆分（WHY）

- **拆分法（零可见性改动）**：利用"子模块可见父模块私有项"——类型（`InputState`/`Outcome`/`InputMode`/`ExecOutcome` 等）与事件循环 `run()` 留在 `app/mod.rs`，函数按职责移入子模块 `keys/exec/jobs/render/preview`（子模块 `use super::*` 即可访问根的类型/字段/私有导入）；跨兄弟互访的移动项统一 `pub(crate)`，根 `pub(crate) use x::*;` 重导出（tests 与根代码经此引用）。测试整体随迁 `app/tests.rs`（原 `use super::*` 语义不变——tests 变为 app 的子模块）。
- **脚本化切割**：awk 按顶层条目行号分组（122 项→5 组 + 根 + tests），条目归属按命名约定（handle_*/press_*→keys；exec_command_impl/远程/粘贴/重命名/删除撤销→exec；render_*/布局→render；作业与 watcher→jobs；`render_right_pane`（预览/查看器窗格）→preview）；区域边界上溯 `///`/`#[` 保证 doc comment 随项迁移。
- **踩坑 2 个**：① tests 剥壳时残留 `mod tests {` 造成嵌套双 tests 模块、glob 失效（E0425 链）——剥净外壳行；② 根的私有 `use` 导入不经 glob 流向子模块——根 use 块改 `pub(crate) use` 重导出。`help.txt` 随 `HELP_TEXT` 迁至 `src/app/`（include_str 相对路径）。
- **结果**：app.rs 15261 行 → mod 1364 / keys 1597 / exec 3020 / jobs 200 / render 1093 / preview 302 / tests 7692；`cargo test` 350+4 双环境全绿、clippy `-D warnings` 干净、`--version/--help` 冒烟通过——零行为变化。

### 2026-11-20 — M27-3 中断提交验证 + 修正（WHY）

- **背景**：HEAD（dee51d4）为执行中断的 M27-3 提交，用户要求验证正确性。验证结论：**提交含 3 处缺陷 + 连带发现 2 处更早批次的潜伏缺陷**，全部本轮修复。根因共性：Windows 本机门禁不编译 `cfg(unix)` 测试代码（M12-2 教训再现×2），且 AUDIT-7 起本机 `cargo test` 因 opener E0369 根本无法编译——"357 单测双环境绿"的 STATE 陈述对 ubuntu 侧不成立，本轮起恢复以实际全量运行为准。
- **M27-3 缺陷**：① `render_page_png_with` spawn 用硬编码 `"mutool"`/`"pdftoppm"` 而非注入的程序路径——`ext` 注入形同虚设，测试假工具永远不执行（连带：假工具测试 expect 失败毒化全局缓存锁，级联其余缓存测试）；② "预览切换清理"只替换同 key 条目，换目标时旧 PDF 临时 PNG 留在磁盘——改为插入新 key 时 purge 其它路径（缓存同时只保留当前目标）；③ render 层提示分支反逻辑：`pngs.len()>=2 && 高度不足` 时**无提示**（违反"至少显示第 1 页并提示"），`pngs.len()<2` 时误报"高度不足"（实为第 2 页生成失败/单页文档）——抽出 `pdf_overflow_note(pages, pngs_len)` 纯函数区分两种文案并单测。
- **潜伏缺陷（更早批次）**：① AUDIT-7 的 `open_with_layers_falls_through_use_list_on_spawn_failure` 断言 `r == Ok(None)`——`io::Error` 无 PartialEq，Linux 下 E0369 编译失败（cfg(unix) 行在 Windows 从未编译）；改 `!matches!(r, Ok(Some(_)))` 同语义。② **M27-1 的符号链接跟随判定实际无效**：`DirEntry::metadata()` 与 `file_type()` 同为 lstat 语义（不穿越链接），`entry.metadata().is_dir()` 对 link-to-dir 恒 false——M27-1 的 unix 测试从未编译运行故未发现；改 path 版 `std::fs::metadata(entry.path())`（load_entries + build_tree_entries 两处 + meta 跟随取值），本机独立程序复现后修复，m271 测试随修复通过。该测试另有一处自伤：测试直改 `location` 绕过 navigate_to，残留 `on_parent_row=true` 使第二次 open_selected 正确走了 `..` 返回分支——测试补 `on_parent_row=false`（模拟 navigate_to 复位，生产行为无误）。
- **测试补齐（对照 ACCEPTANCE §27 M27-3 六项）**：pages=0 关闭不写缓存 / 假工具注入生成两页 / 第 2 页失败保留第 1 页 / Text+prefer_image=true 仍生成 / 切换 purge / 退出 cleanup（幂等）/ overflow 文案三分支 / [preview] 表解析与缺省。假工具 = 注入路径的 sh 脚本（mutool 分支 $3=输出 $5=页号，顺带验证逐参数传递）；VISUAL_CACHE 进程级全局——pdf.rs 缓存类测试统一 `cache_lock()` 串行（M13-10 同款）；AUDIT-2 旧 len 断言改 purge 无关形态。
- **环境边界**：本机 rustc 1.95.0，`exprim@0.4.0` 需 1.97——`cargo check --features pdf-visual` 本机不可执行，维持 CI 门禁（AGENTS §7 口径）；本轮未改动 `cfg(feature="pdf-visual")` 代码。门禁实测：385 单测 + 8 集成全绿（连续 4 次全量稳定）、clippy all-targets 干净、fmt 无改动、`--version/--help` 通过。

### 2026-11-20 — 0.9.0 自动发布（WHY + 边界）

- **触发**：任务列表全部完成（M0–M27、AUDIT、M26、M27、MISC-1）——按 AGENTS §3 自动发布条款执行，不等待人工决策。
- **归档内容变化**：0.9.0 起 release 归档含双二进制（`rvifm` + `rv`，MISC-1）——旧脚本/文档若只引用 `rvifm` 不受影响，`rv` 为等价别名。
- **平台边界**：crates.io 凭据仅在 Windows 机器（M8-13 起口径）——本机（Linux）只做 bump/tag/GitHub Release；crates.io publish 待 Windows 侧执行后补 `release: 0.9.0 verified` 记录。
- **发布失败处置（已触发）**：首推 release 三 job 全数失败于 smoke 步——MISC-1 双 `[[bin]]` 后裸 `cargo run` 因目标歧义报错（"could not determine which binary to run"）；`--bin rvifm` 显式指明后 tag 重指向强推重跑。教训：新增第二 bin target 时，所有 `cargo run` 类步骤都要复查目标歧义。
- **tag 重指向**：`git tag -f v0.9.0 <fix> && git push -f origin v0.9.0`（远端 tag 触发的 run 以重指向后的 commit 为准）。
- **结果（2026-11-20）**：重跑 run #271 五 job 全绿；GitHub Release `v0.9.0` 已发布（linux-gnu / linux-musl / windows-x64 三归档，均含 `rvifm`+`rv`）。首推失败的 test job（#268）本就全绿——失败面仅 release smoke，与门禁结论一致。crates.io 留待 Windows 侧（凭据边界，非阻塞）。

### 2026-11-20 — 评估蒸馏登记 M28–M31（WHY / CUT / 观察项）

- **来源**：2026-11-20 运维/开发视角评估报告（基于 harness 文档核验版），用户指令"蒸馏 P1/P2 + 第四/五节建议；P3 按建议；frecency 通用化对齐同级项目"。参考报告（README-only）勘误已并入登记口径：`:compare` 已有 M24-8 基座（M29 定位为增强而非新建）；M26 已完成（能力矩阵探测降为 M31 验收视角）；`:jobs` 已有选择/取消（M28-3 定位为暂停/详情增强）。
- **M29-2 实现期裁剪（覆盖同步）**：原登记"覆盖已存在走既有确认提示"——实现时确认 copy 链路**无覆盖确认流**（FIX-2/M20-2 均为"同名报错不覆盖"口径），引入覆盖确认属新范围 → `!` 条目（两侧内容不同）dp/do 一律可读拒绝，仅 `>`（取回）/`<`（删除）两类干净方向可用；SPEC M29-2 已同步修订。键位经 Keymap 注册（d-p/d-o 序列 → DiffPut/DiffObtain Action），普通视图 `d+p` 组合行为变化（原为超时后 p 执行粘贴，现为可读拒绝）——vifm 借名语义一致性优先。
- **批次划分**：M28 运维操作补齐（:link/:rename/:jobs 增强）、M29 对比增强（bycontents+dp/do）、M30 frecency 通用化、M31 文档与验收补齐；统一目标 **0.10.0**（PLAN §4 自动发布规则沿用）。
- **M30 实现期注记（2026-11-20）**：cmd_mark_pending 放 InputMode::Command 模式局部字段而非 InputState——InputState 跨模式复用、标志须随模式构造重置；`:` 内 `'`+tag 展开覆盖当前输入（草稿经 Enter 提交前不落库）。m304_cmdmark_command_arm 写全局 redb → STORE_TEST_LOCK 串行（redb "already open" 无重试，M21-F1 踩坑再现）。
### M30-5 文档与隐私收尾
- [x] README/help 明示"命令文本持久化写入 frecency redb（IGNORE_PATTERNS 可排除）"。（2026-11-20：README frecency 段隐私句 + ignore_patterns 提示；help :cmdmark/↑↓ 条目随 M30-2/4 落地）
- [x] NOTES 对齐注记（cmd 命名/无分数下限/书签同构）落档；ACCEPTANCE §30 全勾后关闭。（2026-11-20：对齐注记已在蒸馏期 NOTES 落档（M30 对齐基线段）；§30 全勾）：M32 远端健壮性与会话扩展（SFTP 重连 / .part+偏移续传 / 会话持久化扩展）、M33 键位模式分层扩展（[visual]/[view]/[command]）。要点：续传与 M20-3"显式取消清理半成品"并存——中断保留 `.part`、取消清理；重连为操作触发式一次（不做后台心跳）；M33 未配置模式必须零行为变化；command-line 可映射集合用白名单（实现时定、help 记录）。
- **同级 frecency 项目对齐（用户指令；HEAD `d7e03d0`，2026-11-19）**：
  - **topic 命名用 `cmd`**（其架构注释 dir/file/cmd/自定义 topic 口径）——M30-1 采纳。
  - **清理无分数下限是不变量**：其 2026-11-18 引入 per-topic score floor 当日撤销（分数是序数、绝对值跨使用模式无意义、阈值是 MAX_ENTRIES 的不可预测替身、低分重要条目必须可达），REGRESSION 固化 "no score-threshold cleanup"。rvifm M23-3 本就只用容量裁剪——**登记为 CUT：不引入分数下限清理**（SPEC M30 OUT_OF_SCOPE）。
  - **书签=keyword 事件、unbook 保留历史、pick 只读、选择不计访问**：两边同构（rvifm M25-2/M23-2/AUDIT-1 既有），无改动。
  - **picker `g` action**（gitui→ziggity→lazygit 优先级探测）与 rvifm M25-6 `gi` 同源，无需改动；其 `c/v/e` picker 动作键——rvifm M23-7 修订已定"键位与 Normal 一致"，**不复开**（嵌入式 pane 与 fzf 弹窗交互模型不同）。
  - **SIMPLIFIED 边界维持**：第一期 pick 不加命令视角（其 picker 已含 command preview——列为后续增强，不阻塞）；shell 侧命令自动记录（PROMPT_COMMAND）不在 rvifm 职责内（CUT）。
- **P3 观察项决策（不立任务）**：① `:du` 磁盘用量分析——方案已明确（后台扫描 + 分批追加虚拟结果目录），等用户高频清盘需求再立项；② git 列内暂存/取消暂存——**维持只读**（M23-12 CLI 路线可行但 FM 职责有张力，观察用户反馈）；③ 插件系统——维持非目标，rhai 成熟度观察；④ libarchive_oxide/OxiArc——README 既有观察项继续。
- **横切纪律（AGENTS §6 增补）**：任务关闭声明"双环境绿"前以最近一次 CI run 结论为准——cfg(unix) 测试代码 Windows 本机不编译（M12-2/M27-3 两度教训，本周 M27-3 验证连续暴露 5 缺陷的根因）。
- **pdf-visual 实测边界**：本 Linux 机 rustc 1.95 < exprim@0.4.0 要求 1.97，M31-5 须在 CI ubuntu 或 Windows 本机执行；只测不改护栏。

### 2026-11-20 — 0.10.0 自动发布（WHY + 变化面）

- **触发**：M28–M33 全部关闭（任务列表无未勾选非 deferred 项）——按 AGENTS §3 自动发布条款执行。
- **新增能力面**：:link/:rename 结构化/:jobs 暂停详情/:compare bycontents+dp·do/frecency cmd topic（命令历史 frecency 排序 + :cmdmark + Tab 加权）/SFTP 重连与 .part 现场/会话恢复归档远端 tab/[visual][view] 键位分层。
- **发布失败处置**：沿用既有口径——CI release job 失败则 tag 重指向修复提交强推重跑（0.9.0 先例：双 bin 后 cargo run 需 --bin rvifm，已固化）。
- **crates.io**：凭据仅在 Windows 机器——Linux 侧只做 tag/GitHub Release；publish 待 Windows 侧执行后补 verified 记录。
