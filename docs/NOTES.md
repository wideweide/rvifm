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
- **历史裁剪（已由 FIX-7 解除）**：M14 最初限制右 pane 为导航；FIX-7 将普通模式键位与命令统一改为焦点分派，Location 能力限制仍生效；右 pane 无独立撤销语义（恢复后双 pane 刷新），不实现第三 pane（issue #1062 不立项）。
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

### 2026-11-21 — M34 蒸馏登记（WHY / CUT / 用户裁决）

- **来源**：DeepSeek 讨论分享稿（chat.deepseek.com/share/thsffd9kmrse3gvf4t，用户实测反馈经 DeepSeek 评估）。按 §8.3 蒸馏登记，编号 M34（下一空闲号——M0–M33、FIX/FEAT（M15）、NAV/SRCH/VIEW、AUDIT、GAP、PROC、MISC 均已占用）。
- **登记前现状核查（Explore 只读代理，2026-11-21）**：讨论稿 10 项中 2 项已覆盖不登记——`pick` 独立终端"左搜索右预览"即 AUDIT-3 交付（≥60 列双栏、窄端单栏降级）；dracula/gruvbox-dark 为 opaline 0.4.2 内置主题，激活链路 AUDIT-13 已通。属实的登记项：默认主题实为"无主题 → 16 色 vifm 回退"（非 gruvbox）；右 pane `/` 不可用系 M14"精简导航集"SIMPLIFIED 裁剪（keys.rs:353 注释明示，非回归）；frecency query 无回显（标题仅模式+计数）；结果行无图标（`[目录]/[文件]` 文本标记）；`.jar` 不在 `is_archive_file`（archive.rs:636）；二进制预览仅错误行/lossy 乱码，无信息面板。核查新发现缺陷：`preview_path()`（tab.rs:1296）只读左 pane 字段——焦点在右 pane 时 `w`/`:view` 预览错位（与其自身文档"活动 pane 光标文件"矛盾），并入 M34-4。
- **用户裁决（AskUserQuestion，2026-11-21）**：① 预览统一取**方案 A**——预览恒在焦点 pane 对侧（:view 与 frecency 共用模型；焦点在右时预览渲染到左 pane；无预览模式时右 pane 保持第二目录列表）；② 二进制预览信息面板**裁剪 `--version` 子进程探测**——预览即执行未知二进制有安全风险，仅元数据（大小/修改时间/类型推断）。
- **CUT**：不做 `.vifm` 配色解析器（一次性颜色提取 + TOML 移植，与 AGENTS §5.6 一致）；`.war`/`.apk` 等衍生归档格式不做（仅 `.jar`）；不做主题热重载。
- **实现边界注记**：M34-1 高亮组映射走数据不写死主题分支，M27-2 的 16 色 vifm 回退保留为无主题兜底；snowwhite 为 rvifm 捆绑 TOML（include_str），不回传 opaline；M34-4 只统一"预览目标与放置"，不合并 :view 与 frecency 的全部交互细节（M17 查看器能力维持现状）。

### 2026-11-21 — 0.10.0 发布验证（CI 5 job 绿 + tag 重指向 2 次）

- **过程**：首推 windows clippy 失败（`msg_none` dead-code——m302 测试 cfg(unix) 包裹致 Windows 下无调用者，Linux 侧不报）→ 内联修复；二推 windows test 失败（`m323` 归档会话路径 `split_once(':')` 被 Windows 盘符冒号切断 → `rsplit_once`）；三推全绿。诊断通道固化：CI clippy/test 失败时 `::error::` 注解（匿名 check-runs annotations 可读——errexit 曾吞掉注解步骤，`set +e` 修正）。
- **教训**：① Linux 全绿 ≠ Windows 绿——本会话三度（lstat/E0369/msg_none/盘符）验证 AGENTS §6 "以 CI run 为准" 纪律的必要性；② 路径含冒号（Windows 盘符）的序列化一律 rsplit 切分；③ test cfg(unix) 包裹的辅助函数会成 Windows dead-code。
- **结果**：GitHub Release `v0.10.0` 已发布（三归档，含 rvifm+rv 双二进制）。crates.io 留待 Windows 侧（凭据边界）。

### 2026-11-21 — M11-6 foot 实测（清单第 1 项）+ M34-9 预览尺寸适配补登

- **用户实测（foot 终端，2026-11-21）**：图片预览渲染效果良好——M31-3 清单第 1 项通过（foot 支持 Kitty graphics，协议探测链生效）；PDF 无文本语料首页视觉渲染可用、速度略慢。后者为定性数据：首次栅格化成本属预期（AUDIT-2 结果缓存使二次查看免栅格化）；量化耗时/内存基线仍归 M31-5（rustc ≥1.97 环境）。M11-6 保持 deferred（清单 2–6 未验：tmux passthrough / 无协议兜底 / NO_COLOR / 损坏图 / 大图）。
- **新增需求蒸馏**：用户指令"图片/PDF 实际尺寸小于预览区按实际大小居中；超过缩放宽度与预览区一致" → 登记 **M34-9**（原文档收尾任务顺延为 M34-10）。设计默认（实现期可依 SPEC 调整）：不放大用 `Resize::Fit` 基线（`Fill` 会放大，禁用）；fit-width 后高度超出在交互查看器滚动、普通预览窗格显示顶部（同文本预览"前 N 行"语义）；pick 预览同源适用。

### 2026-11-21 — 会话停止收口：STATE/PLAN 同步 + M34 暂缓挂起（用户指令）

- **背景**：执行会话于 0.10.0 GitHub Release 验证（2a8d88e）后停止，未读取 M34 登记（TASKS 中 10 任务 unchecked）。
- **用户指令**：检查解决文件不一致与冲突；**不启动 M34 执行**（Windows 侧收尾由用户操作）；提交推送。
- **crates.io 实查（2026-11-21 API）**：最新发布 **0.8.0**（2026-11-18）——0.9.0 与 0.10.0 均未发布；Windows 侧需分别检出 `v0.9.0`/`v0.10.0` tag 执行 `cargo publish`（或只发 0.10.0，crates.io 不要求连续）。
- **一致性修正**：STATE 下一步仍停在"从 M28-1 推进 M28→M33 自动发布 0.10.0"（已全部完成）——重写为 Windows 收尾/人工补验/M34 待指令三段；环境口径修正为双本机（Windows Server 2016 + Linux 开发机，与近期 foot 实测会话一致）；PLAN 0.10.0 节从"发布中"改"GitHub Release 已发布 + crates.io 缺口"；M34 条目整体翻 `[~]` 挂起——防止任何新会话按"取第一个未勾选"规则误启动，启动时用户指令后整体翻转回 `[ ]`。

### 2026-11-21 — 本机工具链升级 rustup stable 1.100 + M34 启动（用户指令"升级后开跑"）

- **升级**：rustup 用户级安装（`~/.cargo/bin` PATH 优先于 apt 的 /usr/bin 1.95，系统包未动、无需 sudo）；stable **1.100.0** ≥ exprim MSRV **1.97**（crates.io API 实查 rust_version=1.97）。
- **全门禁实测（2026-11-21）**：fmt ✓；clippy `--all-targets -D warnings` ✓——1.95→1.100 跨 5 个 minor **零新 lint**；build ✓（1m52s）；test **418+9 全绿**；`--version` 冒烟 ✓；**pdf-visual check 本机首次通过**（28.96s）——此前 CI 独占的门禁缺口闭环。
- **口径同步**：STATE 质量门禁"已知偏差"仅剩 aws-lc-sys cross-check 一项（pdf-visual 项移入"已闭环"）；M31-5 自本机可执行（仍 deferred 待运行）；环境记录 rustup 1.100。
- **观察（不处理）**：cargo manifest 警告 unused dep `tokio-stream`（既有，非本次引入），留待后续卫生项。
- **M34-1 映射基准**：vifm-colors `gruvbox.vifm`（cterm 提取）→ opaline gruvbox-dark token（bg.panel/bg.highlight/accent.primary/text.muted/text.dim 等标准词表）；TopLineSel bg cterm237 #3a3a3a ≈ gruvbox bg1 #3c3836（bg.highlight）几乎重合；CurrLine（vifm bold+inverse）与 rvifm 既有 `highlight_style(REVERSED)` 等价，不改动。

### 2026-11-21 — M34-1 实现：默认主题 gruvbox-dark（映射表 + 踩坑）

- **映射实现**：`vifm_style` 三层解析——主题命名样式优先 → `theme::derive_vifm_group` 标准 token 推导（TopLine/TopLineSel/TabLine/TabLineSel/StatusLine/Border → accent.primary / bg.panel / bg.highlight / text.secondary / text.muted / text.dim，任一所需 token 缺失整组回退）→ M27-2 16 色回退。gruvbox-dark 实测值：TopLine fg #fe8019 / bg #282828；TopLineSel bg #3c3836；StatusLine fg #bdae93（≈vifm cterm144 #afaf87）；Border fg #a89984。
- **CurrLine 等价性（不改）**：vifm gruvbox CurrLine = `bold,inverse`（default fg/bg）——rvifm 既有 `highlight_style(REVERSED)` 语义等价，维持不动；`Selected` 组 rvifm 未消费（选中行另有 styling），映射表不含。
- **踩坑**：clippy 1.100 新 lint「let-else 可改写为 `?`」只命中返回 Option 的新函数——旧 `style()` 的 else 返回 `Style` 类型不触发；跨 minor 升级的新 lint 按可改写性选择性命中。
- **文档**：README 两处"20 套"→"39 套"+ 默认 gruvbox-dark；help `:theme` 行补默认值。422 单测（+4：gruvbox 推导断言 / 无主题·缺 token·未知组 None / dracula activate+推导 / vifm_style 集成）。

### 2026-11-21 — M34-2 实现：捆绑 snowwhite 主题（机制 + 取色）

- **机制**：`theme.rs` 新增捆绑层 `BUNDLED_THEMES`（include_str! 数据，首项 `src/themes/snowwhite.toml`）；activate 顺序 opaline 内置 → rvifm 捆绑 → 用户目录；`builtin_names()` 并入捆绑名——`:theme` 无参列表自动含 snowwhite，exec.rs 零改动。
- **取色**：cterm 提取 16=#000000 / 231=#ffffff / 255=#eeeeee / 208=#ff8700 / 196=#ff0000；vifm 高亮组精确值直写主题命名样式（TopLine/TabLine/StatusLine/Border = fg16/bg255、TopLineSel/TabLineSel 反转 fg231/bg16——vifm 无 TabLine 组，镜像 TopLine）；selected fg208（默认底，同 vifm）、active_selected = CurrLine 面 fg231/bg208 不加粗（vifm cterm=none）。**主题命名样式优先于 M34-1 token 推导**——snowwhite 直接命中，不走推导。
- **辅助灰阶**：原 .vifm 仅 5 色；text.secondary/muted/dim 的 gray1/2/3（#444444/#666666/#999999）为移植补全，TOML 头部与本条双注记。
- **踩坑**：theme.rs 测试引用 `crate::app::render` 触发 E0603——`render` 模块对 app 外私有（子模块可见性按目录层级，`pub(crate)` 项不等于模块本身可见）；该断言移除，集成覆盖由 app/tests.rs 子模块测试承担。
- 424 单测（+2：snowwhite 激活/精确取值/图标 Light 联动、捆绑名列表与未知名错误）。

### 2026-11-21 — M34-3 实现：右 pane `/` 搜索（焦点感知搜索链路）

- **链路**：右焦点分支拦截 `/`（EnterSearch）/n/N（keys.rs），与左 pane 共用 Search 输入模式与 `Tab.search` 状态；`Tab::search_move`/`search_status` focus-aware（右 pane 无 `..` 行，`on_parent_row` 离行仅左 pane）；EnterSearch 的 origin 取焦点 pane 光标（mod.rs）；exec.rs 三助手（submit/live/cancel）焦点分派。
- **渲染**：`render_right_pane` 增加 `mode` 参数——右焦点时 live/committed 高亮（FIX-1 同款黄底黑字）；左 pane 高亮行为不变。**已知边界**：SearchState 为 Tab 级共享（vifm 是 per-pane pattern）——左 pane 高亮不随焦点熄灭，per-pane 化超出本任务范围。
- **语义注记**：单匹配自回绕（M1-7 既有语义）——n/N 在唯一匹配上原地不动；新测试初版误设"环绕到其它条目"断言，改用多匹配查询（'a' 三文件全中）演示环绕。
- **清理**：`Tab::find_match` 调用方全部迁至 `find_match_in` 后成死代码 → 删除；切片级 `find_match_in` 抽为 `pub(crate)` 自由函数。
- **踩坑**：误编辑曾把 find_match 方法体替换成 search_move 签名行（old/new 粘贴错位），Read 现场后按行修复；多字段借用（entries/cursor 与 on_parent_row 写）字段级不相交，NLL 下直接编译通过。
- 426 单测（+2：tab.rs 右 pane move/status、app 集成链路）+ 9 集成。

### 2026-11-21 — M34-4 实现：对侧预览统一（方案 A）

- **preview_path 焦点感知**：预览目标 = 焦点 pane 光标文件（results 结果视图优先级不变）——修复焦点在右时 `:view` 预览错位到左 pane 文件的缺陷；tab.rs 自身文档"活动 pane 光标文件"自此成立。
- **放置统一**：查看器渲染抽出 `render_view_content`（preview.rs，263 行脚本搬迁——行号切割 + 锚点断言 + 统一去缩进，AUDIT-5 awk 手法先例）；分派两处：`render_right_pane`（焦点左 → 右 pane 查看器，M13-9 现状）与 `render()`（焦点右 → 查看器占左 pane，右 pane 保持列表导航）。渲染共用同一函数；frecency 对侧预览（M23-6）本就是对侧语义，未动。
- **模态边界（M17 既有，保持）**：view 模式下查看器吞掉全部按键（含 Tab）——焦点右 + 查看器在左时同样如此，切焦点需先 w/Esc 退出；无新增冲突。
- **踩坑**：① 左 pane 列表块被 if/else 包装后 `visual_range` 留在 else 内而状态栏公共段仍引用 → 提升至分派之前；② 搬迁尾巴 `return;` 成 needless_return；③ 大块搬迁用 python 行号切割比手写 Edit 稳（先 grep 锚点行号 + 断言首尾）。
- 428 单测（+2：preview_path 焦点跟随、TestBackend 双焦点放置断言）+ 9 集成。

### 2026-11-21 — M34-5 实现：frecency Pane 视觉与回显

- **标题**：固定 `Frecency - 高频目录/文件搜索`（原 `Frecency · Normal（N 项）`；计数移至底部提示行，Normal/搜索两态均含 `（N 项）`）。
- **行渲染**：devicons 图标 + 全路径替换 `[目录]/[文件]` 文本标记——目录条目路径补尾斜杠触发 devicons 文件夹判定（其内部 `ends_with('/') || is_dir()`，历史路径不存在也能出文件夹图标）；✦keyword 保留；`icon_for_active` 走激活主题 variant（M9-3/M34-1 明暗联动）。
- **回显**：搜索态底部 `/query  输入=筛选 Enter=跳转 Esc=Normal（N 项）`——键入即时更新、清空同步（Normal 态提示行不变）。
- **踩坑**：TestBackend 断言 CJK 文案——缓冲区每格一 symbol、CJK 占 2 格且空位插空格，连续文案 `contains` 必须先去空白（compact）再断言。
- 429 单测（+1）。

### 2026-11-21 — M34-6 实现：.jar 归档识别（双表同步教训）

- **扩展维护点有三处**：`is_archive_file`、`open_dispatch`（独立扩展分派表！）、`is_unsupported_archive`——首版只改第一处，roundtrip 测试立刻暴露 open_dispatch 漏接（`.jar` → "不支持的归档格式"）。**后续加格式必须三处核对**。
- 取路径：`.jar` → ZipFs（zip crate 复用）；浏览/预览/搜索链路经 is_archive_file 自动生效；`:archive` 创建不覆盖 `.jar`（创建产物为 `.zip`，SPEC OUT_OF_SCOPE）。
- README 浏览列表与对比段补 `.jar`；430 单测（+1：识别/大小写/roundtrip list+read）。

### 2026-11-21 — M34-7 实现：二进制/不可读预览信息面板

- **`file_info_text`（src/preview.rs，pub）**：`[二进制/不可读预览] {path}` + 类型 + 大小 + 修改——类型 = 归档扩展（is_archive_file）→ 魔数嗅探（10 类：PDF/zip/gzip/7z/RAR/PNG/JPEG/GIF/ELF/PE）→ 扩展名兜底；有界 1KB 读取（`File::read` 截断，不整文件加载）；**零子进程**（--version 探测已裁剪的硬约束）。
- **接入点**：`read_text_preview` 的 NUL 分支与非 UTF-8 分支（`from_utf8_lossy` → `from_utf8` Err 面板化——乱码渲染消失）；查看器 `read_to_string` 失败分支（"无法读取（非 UTF-8 或已删除）"退役）→ 同一面板；pick 同源自动生效（preview_lines → render_preview）。
- **边界**：`[文件过大]` 截断提示保留（SPEC 未要求改）；魔数仅在二进制/不可读路径生效——纯文本的 `%PDF` 假文件仍按文本渲染（NUL 分支不触发），测试改用 `%PDF+NUL`。
- 431 单测（+1：非 UTF-8 面板/魔数嗅探/无乱码；既有二进制测试改面板断言）。

### 2026-11-21 — M34-8 实现：docs/GUIDE.md 用户手册

- 13 节（ranger(1) 式分节）：安装与启动 → 5 分钟快速上手（浏览/操作撤销闭环/归档/frecency）→ 浏览 → 文件操作（含可逆性标注表）→ 选择与批量 → 搜索与过滤 → 归档 → 跳转（含隐私注记）→ 视图与标签 → 远端 → 配置（文件表 + 最小 TOML 示例）→ 键位速查表 → 命令速查表。
- 内容来源：help.txt（权威键位/命令）+ README 事实（安装/配置路径/边界）；与 help.txt 分工——help = TUI 内速查，GUIDE = 仓库侧手册；README 未扩写（指针随 M34-10）。
- 纯文档任务，无代码改动；门禁照跑全绿（431+9 不变）。

### 2026-11-21 — M34-9 实现：图片/PDF 预览尺寸适配

- **实现点**：`ImageEngine::render_file` 单点重写——查看器图片、PDF 页、pick 全部调用方同源受益。三分支：① 原始尺寸 ≤ 窗格 → `Resize::Fit` 天然不放大 + 窗格内居中子矩形；② 超出但 fit-width 后纵向放得下 → 协议按 (窗格宽, 等比高) 构建 + 纵向居中；③ fit-width 后仍超高 → `SlicedProtocol::new_with_resize` + `SlicedImage` position(0,0) 顶部切片（窗格内裁剪，kitty/sixel/halfblocks 通用）。
- **格子换算**：`picker.font_size()`（像素/格；halfblocks 回退 10×20）——cell = ceil(像素/格宽)；fit-width scale = 窗格宽×格宽 / 图像宽。
- **保持**：`Fill` 不使用（会放大，禁用）；图像纵向滚动不在范围（SPEC SIMPLIFIED——SlicedImage 机制已为查看器图像滚动留路）。
- **踩坑**：`font_size()` 返回 `FontSize` 结构体（非元组）；`SignedPosition` 的 From 是 `(i16, i16)`。
- 433 单测（+2：小图居中不顶格、大图 fit-width 到达右缘 + 切片不越界；不透明像素 PNG 生成器）。

### 2026-11-21 — M34-10 文档收尾 + 0.11.0 自动发布触发

- **文档同步**：ACCEPTANCE §34（逐任务勾选）、README 状态句（v0.11.0/补 M34/GUIDE 指针）、图片预览行（尺寸语义）、help `w` 行、PLAN（M34 ✅ 10/10、§4 0.11.0 发布中）、STATE（批次关闭 + 下一步重写）。
- **版本 bump**：Cargo.toml 0.10.0 → **0.11.0**（Cargo.lock 随构建更新）；发布外循环 = tag `v0.11.0` 推送 → CI 三 target 归档 + GitHub Release；crates.io 凭据仅在 Windows（0.9.0/0.10.0/0.11.0 由用户逐 tag 发布）。
- **M34 全程注记**：批次在本机（Linux，rustup 1.100）执行；每任务独立提交 + 推送 + 全门禁；用户裁决两处（方案 A 对侧预览、--version 探测裁剪）+ 追加任务一处（M34-9 用户实测反馈）。

### 2026-11-21 — 0.11.0 发布期修复：windows runner 长路径截断（M34-5 测试）

- tag `v0.11.0` 首推 windows test 失败：`m34_5_frecency_pane_title_icons_and_query_echo` 的 ✦rs/main.rs 断言——**runner 临时目录名过长**（`C:\Users\RUNNER~1\AppData\Local\Temp\...`），frecency 结果行超出 pane 宽度被截断。修复：测试改短合成路径（`"p"` / `"p/main.rs"`——frecency 记录允许不存在的历史路径，图标按尾斜杠/扩展名推断），平台无关。ubuntu job 同 run 全绿、release job 因此 skipped。
- **教训**：渲染类测试断言含"路径尾部内容"（如 keyword 后缀）时，必须用合成短路径，禁用宿主 TempDir 全路径（M24-9 教训的变体——宿主路径长度依赖）。

### 2026-11-21 — 0.11.0 crates.io 发布（verified）+ 旧版跳过决策

- **发布**：tag 重指向 CI 全绿（16 分钟，windows test 修复生效）→ `cargo publish` 上线 **rvifm 0.11.0**（dry-run 预检先行）→ crates.io API 验证（0.11.0 未 yank）。凭据由用户部署到本机（`~/.cargo/credentials.toml`）。
- **用户决策（2026-11-21）**：**0.9.0 / 0.10.0 跳过不再发布**，只发最新——版本序列 0.8.0 → 0.11.0；crates.io 不要求连续发布，旧版对应 tag 保留可随时追发。
- **流程注记**：发布外循环全流程首次在本机（Linux）闭环——bump/tag/GitHub Release/crates.io 均无 Windows 依赖；Windows 侧仅剩人工验证项。
### 2026-11-21 — 发布节奏调整（用户指令）

- M36–M40 各批次完成后**不再单独发版**；全部批次（含 M40 稳定化）关闭后按 **1.0.0** 一次发布。各批次验收仍独立执行（批次关闭 = ACCEPTANCE 通过）。CHANGELOG 按 1.0.0 总条目 + 分批次小节记录。

### 2026-11-21 — M35–M40 蒸馏登记（v0.11.0 复核报告；WHY）

- **用户点名核查（解压能力）**：归档 VFS 内 `yy` 被显式拒绝（keys.rs Yank 臂对 Location::Archive 置空选集并提示"无可复制的条目"）——归档内 yy→对侧 p **不可解压**，与报告"归档解压 ○"一致；全仓库无解压函数（ops.rs 仅打包）。解压能力由 **M37-2 :extract** 补齐（硬约束：从归档读到磁盘，不违反"不写归档内成员"边界）。
- **报告事实核对**：H-1（AGENTS §10 过期）✓、H-2（SPEC 7z/rar 三处）✓、H-3（STATE/TASKS 矛盾）✓、H-5（PLAN 0.10 口径）✓、H-6（README 发布中）✓、R-12（tokio-stream 零引用/DBG）✓——全部属实后蒸馏。0.9.0/0.10.0 crates.io 跳过为用户决策（STATE 既有记录）。
- **蒸馏决策**：报告 M34"小步交付"被确立为后续批次模板（NOTES §8.3 佐证）；R-1（复制丢元数据）定 P0 并入 M36-1；R-3（下载双读）并入 M36-2；SPEC M32-2 SIMPLIFIED（整文件重写）升级路径 = M36-3/M36-6（范围读写 + 偏移续传）。
- **白名单口径**：SPEC"7z/rar 不支持"三处的修订以 M26-3 实际行为为准；PLAN"0.10.0 跳过"为用户决策记录（非缺陷）。
- **执行序**：M35（文档一致性）→ M36 → M37 → M38 → M39 → M40 → 1.0.0；依赖关系见 PLAN 批次表。**M35-1 在蒸馏时一并完成**（漂移修正与蒸馏落笔同文件同动作）。
### 2026-11-21 — M36–M38 断点（token 预算暂停；下一会话起点）

- **已完成并提交**：M35 批次 5/5（漂移清零/harness_check 七规则 CI/REVIEW 归档/CHANGELOG/依赖卫生）+ **0.11.1 发布**（GitHub Release 三归档 + crates.io 上线——本机凭据验证 owner 后 publish）；M36 批次 8/8（copy_tree 元数据/下载单读+原子 rename/RemoteFs 范围读写/Transfer 执行体/冲突策略/偏移续传 upload_chunked/.part.meta 头哈希校验/:queue 入队+覆盖层+消费/递归远端）；M37 批次 7/7（:chmod/:extract zip-slip/:sync/:du/:dup/:checksum/:trash clear·purge）；M38-1 检索结果可操作（results_actions_enabled + Delete 臂）、M38-2 pick 兼容面（parse_pick_options 八参数）、M38-3 init 四 shell。
- **未完成（下会话起点）**：M38-4 frecency 管理（stats/export/purge——实现代码已在工作区 exec.rs frecency 臂，未勾选验收）→ M38-5 find 过滤器 → M39 全部（hex/CSV·PDF/zip 容器/SFTP 硬化/UAC/标签布局）→ M40 全部（基准/fuzz/proptest/覆盖率/供应链/平台/1.0 发布）。
- **关键事实**：frecency stats/export 臂已实现（exec.rs frecency 臂——cargo fmt 后在位，未加测试/勾选，下会话先验后勾）；443→450 单测演进中；harness_check OK。
- **发布**：按用户决策 M36–M40 全部完成后再统一发布 1.0.0（各批次不再单独发版）。

### 2026-11-21 — M38-4 断点勘误（未提交代码未随 checkpoint 转移）

- 前条"M38-4 frecency stats/export 臂已在工作区（先验后勾）"指 **Linux 开发机未提交树**——未随 checkpoint 提交 2bff891 转移；Windows 树 grep 证实 `exec.rs`/`main.rs`/`frecency.rs` 无任何 stats/export/purge/prune 代码。**M38-4 需从零实现，"先验后勾"作废**。教训：断点记录中的"工作区在位"若未提交，跨机器不可复现——断点事实必须以提交为准。

### 2026-11-21 — M41 登记（用户实测缺陷，优先级排高）+ 漂移修正

- **来源**：用户 0.11.x 实测反馈四项，登记前逐项代码定位属实：
  1. 右 pane `:`/`gt`/`gT`/`Ngt` 失效——`handle_right_key`（keys.rs）仅含导航键；`:`（EnterCommand）与 gt 序列分派只存在于左 pane `handle_key`（Action::NextTab/PrevTab + count）。
  2. 方向键不可用——默认 keymap（keymap.rs `default_map`）无 ↑↓←→ 绑定；右 pane 处理器亦无（parse_key 已支持 "up"/"down"/"left"/"right" 名称，仅缺默认绑定）。
  3. 鼠标点击不切焦点/不高亮——`handle_mouse` 仅左 pane 几何（M15 FEAT-1 注释明示"右 pane 不响应"），且左 pane 点击不切 `tab.focus`（焦点在右时点左列表只移光标、无 CurrLine 高亮）。用户指令升格为双侧一致。
  4. 光标底行按 k 视口钉底——`list_offset(cursor, height)`（render.rs:19）每帧由 cursor 无状态重算 `cursor+1-height`，光标 ≥ height 时恒钉视口底行；`Tab.scroll`/`Pane.scroll` 字段已存在但视口未使用（cd 时清零逻辑已在）。
- **优先级（用户指令"优先级排高"）**：M41 置于 M38-4/5 之前执行（TASKS 节序同 AUDIT 先例）；不单独发版，随 M36–M40 统一 1.0.0（PLAN §4 已同步）。
- **M37-3 勾选漂移修正**：TASKS 复选框 `[ ]` 而 STATE/NOTES 记 M37 7/7 关闭——按冲突优先级（NOTES > TASKS 勾选状态）翻转为 `[x]`；`:sync` 三策略 deferred 属实（l2r/r2l + --apply/--delete + dry-run 已验收），后续按需补登。
- **PLAN/AGENTS/STATE 状态漂移一并清零**：PLAN 总表 M35–M38 状态列（实际 M35 5/5、M36 8/8、M37 7/7 已关、M38 3/5 执行中）+ 发布注记 M41；AGENTS §10（原停 M35）；STATE 当前阶段/任务（原停 M35 执行中）。
- **设计取向**：方向键不新增 Action（复用 CursorUp/CursorDown/ReturnParent/EnterSelected，符合 §5.4 动作驱动约束）；视口偏移有状态化复用既有 scroll 字段；右键菜单保持左 pane（右 pane 文件操作边界不变，SIMPLIFIED）。
- **M41-1 实现决策**：右 pane `g` 原为单键直跳顶部（M14-1"无 gg 序列"），与 `gt` 序列冲突——改为 `g` 起始待决序列复用左 pane seq 机制：`gt`/`gT`/`Ngt` 直接命中默认 keymap 的 NextTab/PrevTab（计数行为天然一致），`gg` 顶部经 ToTop 臂焦点化；单键 `g` 直跳语义退役（SPEC M41-1 已记录升格）。`:` 返回 Outcome::EnterCommand，命令作用于焦点 pane 的链路 M14-1 focus_base 既有。

### 2026-11-21 — M38-4 实现与断点记录二次勘误

- **断点勘误（第二次）**：NOTES 前条"M38-4 frecency stats/export 臂已在工作区（未提交）"部分失实——前会话的 `:frecency` 臂（exec.rs:1994）**实际已随 checkpoint 提交**（stats/export/purge，无 prune、无确认门、export 仅 snapshot、PowerShell 脚本缺 pick 钩子）。当次 grep 用"frecency stats"字面量未命中（臂内为"stats"子命令分支）导致误判。教训：**断点核对必须按命令臂/函数名 grep，不能用子命令字面量**。
- **处置**：旧臂替换为完整实现——四子命令 + `purge confirm` 二步确认门（SPEC M38-4 MUST"确认"；旧版直接删库文件，且 Windows 下有句柄打开时 remove_file 失败风险）+ export 全量（snapshot+events+meta）/purge 改三表清空（不动库文件）/prune --before。
- **prune 设计（SIMPLIFIED 决策）**：只删 `ts < before` 的 events；**snapshot 不动**——snapshot 条目无逐条时间戳（tick/clock 是时钟非墙钟），加字段需 schema 迁移；旧分数经 EMS aging 衰减 + M23-3 容量清理自然收敛，符合数据卫生目标。日期解析用纯 std（Hinnant days_from_civil，UTC 0 点），不引日期 crate（jiff 在依赖树但不为此扩面）。
- **M38-3 验收补课**：TASKS 勾选但 ACCEPTANCE 未验——补 cli 集成测试（四 shell record/pick 标记 + 未知 shell 报错 + cfg(unix) `bash -n` 语法检查走 CI ubuntu）+ PowerShell 脚本补 pick 快捷键（PSReadLine Ctrl-t）。双重教训：勾选与验收须同步；shell 脚本逐 shell 断言内容而非仅"非空"。
- **large_directory 时序测试**：本机（Windows）并行负载下高频抖动（当日 3 次，单跑均 5s 绿）——既有先例记录在案，以单跑 + CI 为准。

### 2026-11-21 — M38-5 实现与 M38 批次关闭

- **过滤器边界**：`--type` 未配置时保持既有行为（仅文件）；`--type d` 仅目录。`--depth N` 相对 root（直接子项 = 1）。`--mtime` 单位 = 天（`+N` 早于 N 天前 / `-N` 最近 N 天内）；日期换算纯谓词注入 now 可测。size 规格支持 `--k=v` 与 `--k v` 双形态（与 trash/frecency prune 的 `=`/空格先例一致）。
- **远端排除（SIMPLIFIED）**：过滤器与命名搜索仅本地视图——远端 `:find` 保持 M22-5 RemoteFs 链路（远端条目无本地 metadata）；GUIDE 注记。
- **首版缺陷（测试双拦截）**：① find 分支抢在 Remote 判断前——m22_5 集成测试拦下（cd 落点错误），补 `is_remote` 守卫；② walker 产出 root 自身（depth 0）——`--type d` 端到端测试拦下（3 ≠ 2），补 depth==0 跳过。两条都是"验证任务揪真缺陷"的又一例（M21-E6/M34-6 同款）。
- **命名搜索**：仅内存（AppState.named_searches，不跨会话——SPEC SIMPLIFIED）；save 需当前结果视图；同名覆盖；load 未命中时列出已保存名（可发现性）。保存 = SearchResults 克隆（含 pattern/root/items/truncated），load 走 enter_results 复用结果虚拟目录全链路（可操作 yy/dd 语义随 M38-1）。
- **M38 批次自此 5/5 关闭**；下一批 M39（预览与平台 6 项）。

### 2026-11-21 — M39-3 实现：zip 容器白名单制度化

- **三表漂移根治**：M34-6（.jar）暴露的"is_archive_file / open_dispatch / is_unsupported_archive 三处独立清单"改为 `ZIP_CONTAINERS` 常量表——前两处共用（is_unsupported_archive 语义不同仍独立：仅 .rar）；后续新容器只改 ZIP_CONTAINERS 一处。
- **魔数 PK 校验落位**：SPEC"魔数 PK + 白名单"——白名单在扩展名层（is_archive_file 无 IO）；PK 校验由 `ZipFs::open`（zip crate）承担：非 zip 内容 → 损坏/加密可读错误（M7-7 语义），测试覆盖 bad.docx。
- **与 Office 预览并存**：docx 等在 preview_kind 中 Office 先于归档命中——预览面板 = Office 文本提取、l/Enter = zip 虚拟浏览，两链路不冲突。

### 2026-11-21 — M39-4 实现决策（SFTP 硬化）

- **指纹确认机制**：check_server_key 在 russh connect 的异步握手中运行，无法直接交互——ask 首连 = FirstUse 时填 `confirm_fp` 共享槽 + `Ok(false)` 拒绝本次 → connect 返回 `RemoteError::FingerprintConfirm`（非连接类，不触发 M32-1 自动重连）→ TUI `RemoteFingerprint` 覆盖层（y/n/Esc）→ 确认后 `accept_new_host_key = true` 重连放行写入。known_hosts 按 host:port 区分，跳板与目标各自门控。
- **ProxyJump**：仅 `~/.ssh/config` 透传（`ProxyCommand` 不支持，SIMPLIFIED）——`ssh_config_host_fields` 解析 Hostname/Port/User/ProxyJump；连接 = 跳板单跳（认证材料同目标，SIMPLIFIED——跳板独立 user/key 未做）→ `channel_open_direct_tcpip` → `connect_stream` 目标会话。known_hosts 跳板/目标分开记录。
- **ssh-agent**：russh 0.65 agent 客户端**仅 unix**（`connect_env` cfg(unix)；Windows 命名管道不支持）——cfg(not(unix)) 桩返回 false 静默跳过；认证序列 = 密钥文件 → agent → 口令（M22-3 回退链保持）。agent 不可用/出错静默降级，不阻塞连接。
- **连接体拆分**：单跳与跳板共用 `authenticate_session`/`open_sftp_session`；`:remote` 臂与确认重连共用 `remote_connect_enter`（RemoteConnectErr 三态装箱 params——clippy result_large_err）。
- **CI 边界**：sshd 全链路测试（ask→确认→写入→重连成功）在 cfg(unix) 模块（CI ubuntu 覆盖）；Windows 本机仅跑 config 解析与不可达主机失败路径（平台无关）。

### 2026-11-21 — M39 批次 6/6 关闭（预览与平台）

- **M39-6 布局机制**：pane_layout 布局感知（`pane_layout(area, layout, viewing)`）——9 调用点（渲染×2/命中×2/鼠标/视口同步/frecency）统一签名；`viewer_rect` 提供 Single/PreviewBottom 查看器占位；`pane_covered_by_viewer` 守卫（命中测试与渲染同源）——**顺带修复 Dual 对侧预览覆盖 pane 仍可点击的既有 quirk**。frecency 模式忽略布局（保持 M23-6/M34-4 几何）。
- **标签栏三渲染点去重**：`tab_bar_spans(state, width, borders)` 共享（命名标签 Tab.name 覆盖目录名 + 溢出 ‹› 窗口滚动——宽度按字符数近似 CJK 计 1，SIMPLIFIED）；SessionTab 不含 name（会话恢复不保存命名，SIMPLIFIED）。
- **M39-5 unsafe 面**：ShellExecuteExW FFI（privilege.rs 模块级声明，SAFETY 注释——AGENTS §7 豁免口径：操作级提权唯一受支持途径）；结构体 112 字节与 SDK x64 布局一致；UAC 弹窗交互为手工清单（ACCEPTANCE §35-D 保留）。
- **M39-4 CI 边界**：sshd 全链路 ask 流程在 cfg(unix)（CI ubuntu）；Windows 本机跑 config 解析与不可达失败路径。ssh-agent 仅 unix（russh 无 Windows 命名管道支持，cfg(not(unix)) 桩静默跳过）。

### 2026-11-21 — M40-1/2 实现决策（基准 + fuzz）

- **lib 化**（M40-1 前置）：`src/lib.rs` 模块树共享（main.rs 只留 CLI 分派 + `use rvifm::`）；模块内 `crate::` 路径在 lib 根同样成立、纯搬移零行为变化。连带修两处潜伏可见性问题：`TransferDirection` pub（state::TransferQueueEntry 公开字段引用，private_interfaces lint）、`load_entries`/`copy_file_chunked`/`CopyControls::none`/`normalize_lexical` pub 化（bench/fuzz 导入面）。
- **基准发现**：load_100k 6.83s ≈ 68µs/条目（每条目 metadata stat 为主）——记入 docs/bench.md 作 1.0 后优化评估点（批量 stat / 并行遍历）；1GB 复制 1.79GB/s 达标。
- **cargo-fuzz 边界**：libFuzzer 需 nightly+Linux——Windows 本机不可跑，fuzz/ 为独立工作区（主 Cargo.toml [workspace] exclude），CI ubuntu fuzz job 承担 10 分钟自动 fuzz（SPEC SIMPLIFIED 口径）；**24h 本地长跑 deferred**（Linux 开发机执行后回填 NOTES）；fuzz 目标首次 CI 运行若发现崩溃按真实缺陷整改流程处理。
- **bench 靶口诀**：`cargo bench` 会先跑 lib 测试靶（libtest 不认 criterion 参数）——需 `cargo bench --bench rvifm_bench`；根 Cargo.toml 已设 `[lib] bench = false`。

### 2026-11-21 — M40-3 proptest 抓到真实缺陷：natural_cmp 非全序（已修复）

- **缺陷**：M24-2 逐字符 natural_cmp 在"一侧有数字段、另一侧无"时**丢弃数字信息**——数字段被收集消费后落入 else 分支按裸字符比较，产生非传递 Equal：`"1a" == "a"` 但 `"1a" < "2" < "a"`。std sort（slice sort 共享实现）检测到非全序比较器即 panic——**任意含 "1a"/"2"/"a" 这类混合名的目录按 S=natural 排序可触发运行时崩溃**（M24-2 起随 0.9.0 shipped，本地/CI 测试从未覆盖到该输入组合）。
- **发现路径**：P7 排序幂等属性（排序两次结果一致）在 std sort panic 处暴露；P5 对称性单独通过（非对称性非唯一违反形态，全序被 Equal 链破坏）。
- **修复**：分块比较重写——字符串切分为数字段/文本段交替序列逐段比较：数字段按 u64（前导零等值/超长回退 MAX 继续下一段）、文本段字典序、数字段 < 文本段（保持既有"数字在前"排序行为）。语义变化：`"1a" == "a"` → `"1a" < "a"`；`"a0" == "a"` → `"a0" > "a"`（原相等语义本身即缺陷产物）。既有 5 项 M24-2 语义测试全部通过（纯排序用例不受影响）。
- **回归固化**：m40_3_natural_cmp_total_order_regression 用 std sort 抽样触发序列（34 项含 "0"/"a"/"0a"/"000" 等的列表排序两次幂等）+ 语义断言。
- **启示**：全序比较器必须属性测试（单点语义测试无法覆盖传递性）；proptest 剩余 6 属性（glob 4 项/对称性/数字感知）首次运行即绿，无假阳性。

### 2026-11-21 — M40-4 覆盖率实测 + junction undo 缺陷修复

- **度量**：cargo-llvm-cov（Windows rustc 1.99.1，--lib，454 单测）。核心五模块 4/5 达标：ops 79.70%（补测前 73.08%）、frecency 83.04%、archive 94.52%、keymap 91.27%；remote/sftp.rs **15.57%**——actor 线程的 connect_async/命令循环需真实 SSH 服务器（russh 网络握手不可 mock 穿透；M19-3b 架构决策即"真实集成 cfg(unix)+sshd 探测"，而 CI 无 sshd 配置）→ **deferred**：补 CI sshd 服务（或 sftp mock server）后以 cargo-llvm-cov 复测；非"补几条单测"可解的架构性覆盖缺口。
- **junction undo EPERM（真缺陷二连，M40-4 create_link 自动化测试抓到）**：M28-1 junction 回退产物在 undo 时报拒绝访问——现代 std（1.99）对 junction 的 symlink_metadata.file_type().is_symlink() 返回 **true**，undo_op Create 臂取 remove_file 分支，但 Windows 删除目录型 reparse point 需要 RemoveDirectoryW（remove_file → EPERM）。修复：remove_dir 优先 + remove_file 回退（unix 文件符号链接/目录符号链接两形态均验证兼容）。二次探针确认 remove_dir(junction) 只删链接本身、目标内容完好。
- **M28-1 人工项闭环**：junction 回退 + undo 全链路由 create_link_automated_and_undo 测试自动化覆盖（原"Windows junction 回退人工验证 deferred"可勾）。
- **trash ops 可注入化**：trash_clear_all/purge/read_index 拆 _at(root) 形态（公共包装走全局 trash_dir），测试用临时根——refactor 中一度把 root 当 index 目录传入（测试立即拦下）。

### 2026-11-21 — M40-5 供应链：sevenz-rust → sevenz-rust2 迁移 + deny/audit 豁免口径

- **RUSTSEC-2026-0246**（sevenz-rust 弃维护 + extract 路径穿越，仓库已删）→ 迁移 `sevenz-rust2 0.25`（续维护 fork）。API 映射：SevenZReader→ArchiveReader（open 签名同型）、SevenZWriter→ArchiveWriter（create 同型）、SevenZArchiveEntry→ArchiveEntry、`Error::io_msg` 私有化 → 直接构造 `Error::Io(err, msg.into())`。feature：compress（writer）+ bzip2 显式开启（--no-default-features 下）。read-only 用法（list/read，无 extract-to-disk）回归测试全过；打包（push_archive_entry）同型。jiff/lzma-rust2 随迁。
- **deny.toml 口径**：许可证白名单含 bzip2-1.0.6（libbz2-rs-sys 捆绑许可证名）与 NCSA（fuzz 工作区 libfuzzer-sys——deny 会扫 fuzz/ 子包图）；advisories 默认 fail + 两条带理由豁免：RUSTSEC-2023-0071（rsa Marvin Attack，russh 传递、rvifm 为客户端持钥侧不受 oracle 场景影响）与 RUSTSEC-2024-0320（yaml-rust 弃维护，syntect 传递、输入为内置高亮定义）。audit 豁免在 `.cargo/audit.toml`（工具各自独立配置）。
- **本机 audit 网络抖动**：cargo audit 拉取 advisory-db（github git）超时——cargo deny 的 advisories 检查（同一 RUSTSEC 库、缓存命中）已全绿；audit 由 CI supply-chain job 承担。
- **SHA256SUMS.txt**：release job 三归档同目录 sha256sum 生成并随 Release 上传（gh release upload --clobber）。

### 2026-11-21 — M31-5 pdf-visual 护栏实测（本机解锁后补验）

- **环境**：Windows Server 2016 / rustc 1.99.1 / release / exprim（pdf-visual feature）。运行入口：`cargo run --release --example pdf_bench --features pdf-visual`（examples/pdf_bench.rs 保留为可复测工具）。
- **数据**：A4 页面 1224×1584px（≈194 万像素，护栏 400 万内、未触发缩放）；单页耗时 22–23ms（首启 58ms 含 exprim 初始化）、CJK 无嵌入字体 8ms、损坏文件 0ms 可读拒绝（不 panic）；RGB 内存 ≈5.8MB/页。结论：护栏（200–400 万像素 + 64MB 输入上限）在生产面无性能压力。

### 2026-11-21 — CI 红灯断点（M39-4 推送起；下一会话起点）

- **现状**：GitHub Actions 自 f91317d（M39-4）推送起持续红；最后绿 = 400ae34（M39-3）。四个失败面：① ubuntu/macos `cargo build --locked` 失败（曾见 E0631 type mismatch——M40-x 中途一次运行 build 通过，疑似 rust-cache 陈旧工件；0ef4e60 已修 tee 无条件 exit 1 + 去锚 grep）；② windows test 步骤失败（本地 459 测试全绿——疑似新测试 runner 环境敏感（M24-9/M34-5 runner 路径长教训），未定位）；③ fuzz job build fuzz targets 失败（fuzz 靶编译问题未定位）；④ supply-chain cargo audit 失败（疑似新 Cargo.lock 通告或 DB 拉取网络，未定位）。
- **诊断受阻**：GitHub API 匿名限流（60/h 耗尽）+ 日志下载需 admin + digest summary patch 静默失败（gh auth 环境未配全）——远程日志不可读。已增强：clippy/build 步骤 tee + grep 注解（0ef4e60，grep 去锚适配 ANSI 色码）。
- **下一会话起点**：① Linux 开发机 `cargo build --locked && cargo test --locked` 全量复现（可编译 cfg(unix) 测试 + fuzz 靶，Windows 不编译）→ 定位 E0631/windows test/fuzz 三面；② 修红后推 main，CI 全绿 → M40-7 1.0.0 发布（tag → 三平台+aarch64 归档 + SHA256SUMS → GitHub Release → crates.io，凭据在 Windows 机）；③ M40-7 前置：CHANGELOG 0.8.0→1.0.0 批次并入条目（M35–M41 各节）+ Cargo.toml 版本 1.0.0。
- **当日已交付**（M41 4/4 + M38 5/5 + M39 6/6 + M40 1–6 + M31-5，22 提交）：三个真实缺陷修复（natural_cmp 非全序 panic / junction undo EPERM / sevenz-rust 弃维护迁移）+ 漂移清零 + 供应链门禁 + 基准/覆盖率/fuzz/proptest 质量基建。

### 2026-11-22 — CI 红灯闭环（fuzz 靶两处编译错误；Windows 门禁天然不可见）

- **四面结论**：ubuntu/macos build、windows test、audit 三面在 0e232c7（M40-6）已绿（匿名 GitHub API job/step 级核实；E0631 确系 M40-6 前修复 + rust-cache 陈旧混合，M40-6 后未再复现）；仅剩 fuzz build 一面红。
- **fuzz 根因（两处，fuzz 工作区从未在任何环境编译过——Windows 无 nightly/cargo-fuzz，CI 注解被淹没）**：① `fuzz_paths.rs` 引用 `rvifm::app::exec::normalize_lexical`，M40-2 只 pub 了函数没 pub 模块（E0603 module `exec` is private）→ `src/app/mod.rs` `mod exec` → `pub mod exec`（模块内其余项均 pub(crate)，对外实际仅暴露该函数）；② `fuzz_targets/common.rs` `fsx` 缺 `mut`（E0596，`ArchiveFs::list/read` 取 `&mut self`）。

- **注解误报教训**：CI fuzz annotate 的 `grep -E "error"` 把 crate 名含 "error" 的行（quick-error、thiserror-impl）连 -A4 上下文一起注解，真实错误文本（E0603/E0596）不在注解中——注解通道对 fuzz job 无诊断价值，本次靠 Linux 本地复现定位。
- **顺带删除**：`src/remote/sftp.rs` tests 模块死函数 `backend()`（M39-4 改 auth-tolerant 后无人调用）——本地 Arch rustc 1.99.1 的 dead-code 比 CI stable（≥1.100）更严，仅本地 clippy -D warnings 报错；CI 与本地结论以 CI 为准（STATE 门禁段已记）。
- **fuzz/Cargo.lock 转为提交**：原被 cargo-fuzz 模板 .gitignore 忽略，CI fuzz job 每次全新 resolve——锁定解析消除漂移红风险（1.0 前收口）。
- **环境事实（Linux 开发机已变化）**：rustup 不在（系统 rust 1.99.1）；crates.io index 与 static.rust-lang.org 直连超时（312s 0 字节），依赖下载与 rustup 走用户代理 `socks5://192.168.1.88:15235` 后恢复；gh 未安装，匿名 GitHub API（runs/jobs/check-runs annotations）可用。
- **验证**：Linux 全量 490 单测 + 12 集成全绿、clippy all-targets 绿、fuzz 工作区 `cargo check` 绿（修复后 EXIT=0）、`--version`/`--help` 冒烟过；M40-2 的 24h fuzz 长跑仍 deferred（需 rustup+nightly，环境事实见上）。

### 2026-11-22 — M40-7 发布断点（aarch64 release 腿修红；用户换机暂停）

- **进度**：CI 修红全绿后（565f4c9）按自动续轮进入 M40-7——CHANGELOG 1.0.0 批次并入（已备）+ Cargo.toml/lock/README 1.0.0（1109c92，`--version` 冒烟过）+ tag v1.0.0 首推。
- **首推结果（run 44928568409）**：test×3 + supply-chain 绿；release 四腿中 windows/musl/gnu 三腿绿且 windows 腿已创建 GitHub Release v1.0.0（windows 归档已上传）；**aarch64-unknown-linux-gnu 腿 build --release 失败（exit 101）**——release 步骤无 tee/grep，check-run 注解为空，job 日志需 admin（匿名 403），本机浏览器直连 github 超时，真实错误文本不可得。
- **排查结论（远程信息受限下的排除法）**：非 aws-lc-rs#1022（`__ARM_ARCH` 未定义系 GCC 4.8 古老镜像；CI ubuntu gcc-aarch64-linux-gnu 为 GCC 13）；lock 面 aws-lc-sys 0.47.0 / aws-lc-rs 1.19.0 / russh 0.65.1；嫌疑收敛于 aws-lc-sys aarch64 交叉 C 构建（cmake/cc/bindgen 阶段）或链接阶段；本地复现受阻——本机无 rustup、无 aarch64 交叉工具链、sudo 需密码、代理仅 SOCKS5（容器 apt/镜像拉取不可用）。
- **已做（b21beef）**：ci.yml release build 步骤 tee + `grep -E "error" -A4` 注解化（同 clippy/test 模式，注解经匿名 API 可读；crate 名含 error 的行仍会误报，真实 error[ 行会混在其中）；tag v1.0.0 重指向 b21beef，tag run 44977467118 进行中（截断点时 aarch64 腿未出结果）。
- **下一会话起点**：① 匿名 API 两步——`GET /actions/runs/{最新}/jobs` 取 aarch64 job id → `GET /check-runs/{job id}/annotations` 读真实错误行（grep 注解含 -A4 上下文，head -60 截断）；② 按错误修复（若为 aws-lc-sys 交叉构建类，候选方向：`AWS_LC_SYS_PREBUILT_NASM=1` 对非 x86 目标的影响核查、CC_aarch64_unknown_linux_gnu/AR 显式注入、CMAKE_ARGS/toolchain file、或升级 aws-lc-sys；若为链接类查 libgcc_s/链接器配置）；③ 修复后 tag 重指向重跑（gh release create `|| true` 幂等、upload `--clobber` 覆盖，模式同 0.10.0/0.11.0）；④ 四腿全绿后勾 M40-7 主体、crates.io 单列（凭据在 Windows 机，Linux 无）。
- **环境事实（补充）**：本机下载必需代理 `socks5://192.168.1.88:15235`（crates.io index 与 static.rust-lang.org 直连 312s 超时）；rustup 安装未完成——rustup-init 二进制经代理可下载（21MB 快），但须 `chmod +x` 后直接执行（`sh <elf>` 报 RC=126），target add aarch64-unknown-linux-gnu 未执行。

### 2026-11-22 — M40-7 aarch64 红腿根因闭环（c_char 陷阱；本地完整复现）

- **注解通道二次失守**：tag run 44977467118 的 `grep -E "error" -A4` 注解被 crate 名（thiserror/quick-error）误报逐行占据，GitHub **10 条 error 注解/步上限**内真实 error 行被挤掉——上一断点"真实 error[ 行会混在其中"的预估过于乐观。修复：新 helper `.github/workflows/annotate_errors.sh`（行首 `^error` 锚定 + 去 ANSI + 首个错误块一条注解带 5 行上下文 + ≤10 条 + 无命中回退日志尾部），ci.yml 三处 `grep -E "error"`（test build / fuzz build / release build）统一换用；正向/回退/ANSI 三形态本地验证过。
- **取证路线（匿名 API 全失效后）**：job logs API 403（需 admin）、网页分步日志端点 404（需登录）→ 转本地复现——rustup 1.100 已在本机（NOTES 前条"rustup 不在"已过时）、apt download + dpkg -x 装交叉工具链（gcc-aarch64-linux-gnu 等 8 包 40MB，meta 包的 alternatives 符号链接须手动补建；运行时补 libgcc-s1/libc6-arm64-cross；解包路径错位用 `--sysroot` 重映射，仅影响最终链接不影响编译/汇编取证）。
- **根因（E0308，非 aws-lc-sys）**：`src/privilege.rs:46` mkfifo FFI 声明硬编码 `*const i8`——**aarch64 linux 上 `c_char = u8`**（x86_64 = i8），`CString::as_ptr()` 返回 `*const c_char` 不匹配。M12-2 起即潜伏，三主机腿（x86_64 win/mac/musl/gnu）全绿、唯独 aarch64 编译失败；CI 死时 ~221-297s ≈ 全量冷构建 2/3 处（rvifm 本体最后编译，时间线吻合）。修复一行：`*const core::ffi::c_char`；全库 grep 确认无同类硬编码。
- **环境敏感测试（顺带）**：`m394_ask_gates_first_use_then_accepts_on_confirm` 在本机失败——宿主 `~/.ssh/known_hosts` 含 127.0.0.1 哈希条目（`|1|`，grep 明文匹配不到），指纹门对已知主机放行 → unwrap_err panic；CI runner 干净环境绿。修复：测试前置 `first_use_gate_precondition()`（known_hosts 缺失/空 → 跑；明文 127.0.0.1/localhost 或任何 `|1|` 哈希条目 → skip），CI 覆盖不缩水。
- **本地验证全绿**：aarch64 `cargo build --release --locked`（EXIT=0）+ 宿主 fmt/clippy/build + 490 单测 + 12 集成 + 冒烟；修复提交 f955c67，tag v1.0.0 第三次重指向（run 44982915466）。

### 2026-11-22 — M40-7 收官（发布修红第二/三层；1.0.0 GitHub Release 全绿）

- **第二层（smoke 不可执行）**：c_char 修复后 tag run 44982915466 中 build 首次绿，但 smoke `cargo run --target aarch64` 在 x86_64 runner 必然不可执行（exit 2）——此前两轮被 build 红遮住。首修走 qemu-user-static -L sysroot（本地验证 rvifm/rv 双二进制在 qemu 下正确输出 1.0.0），但 runner 上 `apt install qemu-user-static` 失败（exit 100 无诊断输出——-qq 静音 + binfmt 注册/apt 镜像两类嫌疑，不可自动化诊断）→ **改为确定性 ELF 断言**（test -x + file 架构匹配 "ARM aarch64" + readelf NEEDED libc.so.6，零新增依赖），执行冒烟由 musl/gnu 两腿覆盖，aarch64 真机 `--version` 人工验证单列（STATE 下一步）。
- **注解仅失败发射**：annotate_errors.sh 首次实战在绿腿也会发射"no ^error lines"日志尾部误报注解（run 44982915466 aarch64 腿实测）——三处调用点改为 `if [ $rc -ne 0 ]` 门控。
- **取证方法论沉淀**：匿名 GitHub API 排查链已到顶（annotations→job logs 403→网页分步日志 404 需登录）；本地复现 = 唯一可靠取证路径，环境要素 = rustup target + apt download/dpkg -x 交叉工具链（libgcc-s1/libc6-arm64-cross 运行时包 + alternatives 符号链接手动补建 + --sysroot 重映射解包路径）+ `CARGO_TARGET_*_LINKER` 环境变量等价注入。
- **收官**：tag 第 5 次重指向 ab102ec → run 44984795450 **全绿**（test×3 + fuzz + supply-chain + release 四腿），GitHub Release v1.0.0 四归档 + SHA256SUMS.txt 齐全；crates.io publish 单列（凭据在 Windows 机，执行后 API 验证）。

### 2026-11-22 — 1.0.0 crates.io 上线（双渠道闭环；凭据环境事实更新）

- **发布**：`cargo package --locked` 干跑核验（73 文件 / 2.3MiB，验证构建干净，唯一 warning 为 MISC-1 双 bin 既有项）→ `cargo publish --locked` 上传成功；API 验证 crates.io `newest/max_version = 1.0.0`（2026-11-22T05:44Z）。版本序列 1.0.0 → 0.11.1 → 0.11.0 → 0.8.0 → 0.7.0（0.9.0/0.10.0 跳过不变）。
- **环境事实修正**：NOTES 前条"crates.io 凭据只在 Windows 机"已过时——用户 2026-11-22 在 Linux 机配置 `~/.cargo/credentials.toml`，publish 直接成功；且 crates.io index 直连本次未超时（前条"必需代理"不再绝对，代理配置保留为兜底）。M40 批次自此全清零（M40-2 24h fuzz 长跑、M40-4 sftp 覆盖仍 deferred，均不阻塞发版）。

### 2026-11-22 — REL：发布两通道解耦与人工验证清单收编（WHY）

- **WHY 解耦**：GitHub Release（二进制归档）随时可删除/覆盖=可逆；crates.io
  版本号一经发布永久占用=不可逆。原"自动发布规则"（PLAN §4 0.10.0 段、AGENTS
  §3）把 publish 与 GitHub Release 绑在同一自动段——不可逆通道缺人工门。修订
  为：自动止步于 GitHub；publish 由 `docs/RELEASE_CHECKLIST.md` 门禁必做组
  （A/B/C/D/E/G/H）勾选完成放行（SPEC REL-1 MUST）。
- **WHY 清单独立成 RELEASE_CHECKLIST.md**（不并入 ACCEPTANCE）：清单是版本
  作用域、QA 面向、随版本增量修订的活文档（REL-2）；ACCEPTANCE 是批次验收
  历史。原 §16（v0.6.0 手册清单 A–K）与 §35-A（0.11.0 全量 36 项）的键位/
  命令事实已与 1.0.0 代码脱节（pick 双模语义、右 pane 能力、M36–M41 全部新增
  面均缺失），收编为历史入口，正文按 1.0.0 代码重写。
- **清单按代码事实核验**（防止 QA 按图索骥失败）：pick stdin 候选 + stdout
  TTY 分流（TTY=交互/非 TTY=fzf filter）、`:archive [格式] <名>
  [checksum=…]`、`:goto 0x100`/十进制、`--find` 过滤器真实形态
  `--size +1k/--mtime -7/--type f/--depth 1`（--k=v 与 --k v 双形态）、
  `[transfer] conflict_policy` 缺省 ask/取值 overwrite·skip、数据目录 Unix
  `$XDG_DATA_HOME/rvifm`（缺省 ~/.local/share/rvifm）。
- **WHY 不落流水线 YAML**：讨论稿的 4 阶段 pipeline 是外部审批系统形态；
  本项目发布自动化由 GitHub Actions（ci.yml）+ 文档协议（AGENTS/SPEC/PLAN）
  承载，不引入新工具——人工门以清单勾选 + 结论记录实现，语义等价（NOISE
  丢弃，REQ/HOW/WHY 按 §8.2 投递）。
- **1.0.0 形态特例**：crates.io 1.0.0 已于本日发布（用户指令"验证并执行"），
  故 1.0.0 清单为发布后全量实测验收（0.9–1.0 功能首次人工实测入口）；自
  下一版本起清单转为发布前门禁。

### 2026-11-22 — REL-2 门禁简化 + M42 :tutor 登记（WHY）

- **WHY 撤销清单门（同日两度修订）**：REL-1 上午版以 RELEASE_CHECKLIST.md
  勾选为 crates.io 放行证据；用户随后裁决——清单只有人工校验部分，与
  `:tutor`（M42）职能重叠，维护两套人工面是冗余。定稿：publish 仅由人工发起
  （何时发布由人决定），人工验证方式 = `:tutor` 自行验证（知悉即可，非强制
  门禁）；RELEASE_CHECKLIST.md 删除（1.0.0 清单历史见 git）。
- **M42 蒸馏来源**：Qwen 讨论稿「人工验证清单优化策略」（chat.qwen.ai share，
  2026-11-22）——人工校验按辅助学习层级设计：:tutor 命令 + 底部 Tutor Panel
  + y/n/q 按键 + 进度持久化 + 错题本。用户裁剪项（CUT，投 SPEC OUT_OF_SCOPE）：
  学习/测试双模式、tutor_report 外部报告文件、Agent 自动读取验证结果；保留项
  （REQ）：单模式自验 + 错题本 + reset。
- **设计决策**：模块内容 `assets/tutor/*.toml` + include_str!（先例
  help.txt/BUNDLED_THEMES）；进度 TOML 落 data_dir()（复用 M8-3 路径）；Tutor
  键位硬编码不进 ModeLayers（SIMPLIFIED，避免 M33 面膨胀）；结算 Summary 在
  Panel 内呈现。类似工具对照（vim-tutor/Helix :tutor/Anki）仅 WHY 记录，不
  进 SPEC。
- **编号**：M42 空闲（M41 为最后已用里程碑号）；REL-2 编号未占用。版本目标
  1.1.0（功能特性，semver minor；发布走 REL-1 人工发起）。

### 2026-11-22 — M43 裁决记录（tutor 全功能覆盖与放行重构）

- **用户裁决（对评估报告 8 问）**：问题 5 = 全量（13 模块 ~85 项，逐键粒度）；
  问题 8 = 自评语义无异议；其余采纳建议——问题 1 透明放行（Ctrl+Y/N 标记，
  回归原始草案设计）、问题 2 requires 字段、问题 3 sandbox 引导（轻量）、
  问题 4 危险命令「了解即可」、问题 6 M43 新批次、问题 7 维护规则入 SPEC。
- **WHY 放行重构改架构（state.tutor 而非 InputMode）**：M42-1 将会话放在
  InputMode::TutorSession——放行后用户执行 `:help`/确认/AskPass 等会切换
  InputMode，会话状态即丢。改为 AppState.tutor 覆盖层状态：tutor 不再是输入
  模式，仅是"Panel 是否显示"的标志——按键照常走 Normal 及各模式分支，任意
  模式迁移不丢会话；标记键（Ctrl+Y/N/Q）在事件分发最前置拦截。Ctrl+Q 退出
  （Esc 放行保留清搜索/过滤语义——检索模块条目需要练 Esc）。
- **WHY Ctrl+Q 而非 Esc 退出**：放行后 Esc 在 Normal 有既有语义（M21-F3 清
  搜索），检索类条目本身要练 Esc；Ctrl 组合经 crossterm raw mode 全部可捕获
  （IXON 关闭），Ctrl+Q 无冲突（Ctrl+M==Enter、Ctrl+I==Tab 除外——均不选）。

### 2026-11-23 — M43 收官（13 模块 100 项；透明放行落地）

- **实现要点**：放行重构采用 state.tutor 覆盖层方案（NOTES 2026-11-22 WHY 条
  落地）——InputMode::TutorSession 退役，tutor_hotkey（Ctrl+Y/N/Q）在事件分发
  最前置拦截、其余键照常走模式分支；**作答即时落盘**（幂等合并）替代"关闭时
  保存"——中途退出/应用退出均不丢进度。Esc 放行保留清搜索语义（检索条目要练
  Esc），退出键定 Ctrl+Q（raw mode 下 Ctrl 组合全可捕获）。
- **内容规模**：13 模块 100 项（含 core_nav 补 Space、tutor_meta 自述模块）；
  每条 prompt/expected 逐条对照 exec.rs/tab.rs 现行行为核验（发现并修正
  `:rename` 无参=$EDITOR 批量 vs 带参=结构化预览两形态混写）。requires 字段
  7 token（editor/sudo/sftp/git/osc52/graphic/tmux），Panel 🔧 提示行不过滤。
- **维护规则（SPEC M43-6 MUST 固化）**：此后批次新增键位/命令时同步补 tutor
  条目——批次验收条约束（tutor 条目数允许漂移，语义漂移靠内容测试与人工实测
  发现；不进 harness_check 自动检查）。
- **验收**：499 单测（m43_1 放行/热键/滚动渲染 + m43_2 requires 解析）+ 12
  集成 + 门禁全绿/轮；harness_check 过。CI 结论以 tag 推送为准。

### 2026-11-23 — M44 预览焦点修复（1.1.0）

- **WHY 独立 `preview_focus` 而非扩展 `Focus` 枚举**：既有两个 pane 的焦点语义
  （Left/Right）被渲染、鼠标命中、命令作用域大量复用；把预览作为第三种 Focus
  会强迫所有 match 分支重审。`preview_focus: bool` 只回答"按键是否路由给查看器"，
  预览位置仍由既有"渲染在非焦点 pane"规则推导。
- **WHY 跟随在绘制前统一刷新**：光标可由 j/k、gg/G、目录切换、搜索结果、鼠标、
  命令等多种路径改变；逐路径调用易漏。事件循环在每帧 `draw` 前调用
  `refresh_viewer_follow()`，单一入口覆盖全部路径；同一文件仅钳制滚动，换文件
  按新文件重建（保持 grep 命中定位语义）。
- **WHY 版本 1.1.0 且不发 crates.io**：M42/M43 为新增功能、M44 为交互修复，
  semver minor；用户 2026-11-23 明确要求只发布 GitHub Release，crates.io 仍停在
  1.0.0。

### 2026-11-23 — M45 用户实测批次蒸馏（29 项；待用户确认）

> 来源：用户测试反馈文本。按 AGENTS §8 投递（REQ→SPEC M45 / SEQ→TASKS M45 +
> PLAN / HOW→ACCEPTANCE §47 / WHY+冲突→本节）。登记前用三个并行只读调查核对
> 代码，结论如下（file:line 为核查时点）。

- **已实现（不重复登记）**：中间分隔线（`render.rs:629-633`）、标签样式独立
  `TabLine`/`TabLineSel`（`render.rs:178-191`，已非路径行色）、整行反显
  （`render.rs:783-795` 全宽 REVERSED）、opener 分层引擎接线 Enter（`exec.rs:3805`）、
  tutor 进度持久化（`tutor.rs:245-282` + `mod.rs:767`）、`:extract` 按规格实现
  （`exec.rs:2571-2671`）。
- **确认为缺陷**：`v`+`d` 无确认（`keys.rs:649-666` 直接 `execute_tag_delete`）；
  `:cd` 忽略焦点恒改左（`exec.rs:3017/3027` 用 `location` 非 `physical_base`，
  违反 M41-1）；`o` 含 cmd topic 不可跳转（`frecency.rs:370-449` +
  `exec.rs:1817-1862`）；tutor 列表 `l`/Enter 不退出 mode（`mod.rs:1582-1607`
  只设 state.tutor）；tutor 提示 `y/n/q` 但实需 Ctrl（`render.rs:1358-1360`
  vs `mod.rs:446-459`）；`:archive` 忽略 `v` 选择（`exec.rs:4664-4696`
  archive_plan 仅 tags + 光标）；main viewer `read_to_string` 无 NUL 探测导致
  tar 乱码（`app/preview.rs:175-179`）。
- **确认为缺失**：`~` 路径缩写（display_path `tab.rs:34-46` 恒绝对）；标签栏鼠标
  （`keys.rs:1691-1692` 明示不响应）；无后缀类型图标（`icons.rs:31-41` 仅
  devicons 直通，无 `*` 也无类型推断）；`o` 鼠标选中（`handle_mouse` 非 Normal
  即返回 `keys.rs:1701-1703`）；命令名辅助（仅 Tab 路径补全/历史/书签）；
  per-location 光标记忆（`navigate_to` 重置 cursor=0 `tab.rs:594-605`）；
  tutor 列表进度显示（`render.rs:893-897` 仅 id/name/description）；归档成员
  列表预览（`PreviewKind` 无 Archive `preview.rs:48-59`）。
- **5 项冲突须用户裁决（未拍板不得开写）**：
  1. **分隔线**：用户要删，SPEC M27-2 明确要求「双 pane 之间 1 列分隔线」。
  2. **搜索子串高亮**：用户「文字部分单独高亮」若指 `bg(LightYellow)`
     （`render.rs:718-721`），删除将违反 FIX-1「匹配子串实时高亮」。
  3. **可执行文件仅提示**：与 AGENTS §5.5「文件打开默认走系统 opener」冲突
     （可作可执行文件特例，需确认）。
  4. **右栏归档 `l`/Enter**：SPEC M41-3 明确「右 pane 归档/文件 no-op」，
     用户要求进入需改规格。
  5. **`:extract` 对侧 / 虚拟目录 `yy`**：M37-2 默认当前物理 pane；
     虚拟目录 yy 在 NOTES 2026-11-21 明确 OUT_OF_SCOPE（只读 VFS）。
- **需求澄清**：`opener` nvim 未生效根因 = 未生成 config.toml（无自动生成）
  + TOML 字符串形态 `edit_text = "nvim %s"` 反序列化失败（正确为表形态，
  README:194-204 有示例），非分层引擎 bug；tutor「进度未持久化」疑似因按错
  提示键（y/n/q 而非 Ctrl+Y/N/Q）未记分，代码层面持久化已实现；归档格式单参
  推断其实已支持（`exec.rs:2807-2819`），缺的是 `[名, checksum=]` 组合。
- **既有文档内部矛盾（顺带登记）**：SPEC M34-9 正文（`SPEC.md:451`）称大图
  「交互查看器内可滚动查看完整内容」，同条 SIMPLIFIED（`SPEC.md:460`）称
  「普通预览窗格不提供滚动，显示顶部）——用户裁决 **34-9 去除可滚动**，已统一为
  「高度超出显示顶部不滚动」（M34-9 正文改写）。
- **编号**：M45 取下一空闲里程碑（M0–M44 已用），沿用 M15/M21 用户实测批次
  先例；维护期按 AGENTS §10 走 FIX/FEAT 蒸馏，本批次以 M45 umbrella 登记。

#### 2026-11-23 — M45 冲突裁决（用户拍板，已落文档）

1. **分隔线**：选「改为 `[ui] pane_separator` 可配置」——默认 false = 不显示
   （满足 vifm 对齐诉求），true 恢复；SPEC M27-2 已修。用户原话在"删除/可配置"
   间未显式二选一，按"默认不显示"意图取可配置形态，可逆且对齐零配置哲学。
2. **搜索子串高亮**：**保留** FIX-1。用户澄清反馈的是「未搜索时文字部分有不同于
   整体背景的颜色」——属主题 `ft_*` 文字色/风格问题，非搜索高亮，M45-2 只修
   未搜索态的整行背景一致性，不动 `bg(LightYellow)` 匹配高亮。
3. **可执行文件**：作为默认 opener 的**特例**——Enter 不执行也不走系统 opener，
   仅状态栏提示「可执行文件，请在终端执行」；AGENTS §5.5 已增条。
4. **右栏归档进入**：**改规格**——M41-3 原「归档 no-op」改为右 pane 也可进入
   归档虚拟目录（文件仍 no-op）；M41-3 已修。
5. **extract 对侧 + 归档 yy**：**推翻**旧决策——M37-2 默认目标改为对侧 pane，
   新增 left/right/当前/对侧/绝对路径指定；归档虚拟目录 `yy` + `p`/`P` 可粘贴
   到焦点 pane 物理目录（按成员解压，不写归档内）；旧 NOTES「归档 yy 不支持」
   决策作废，SPEC M37-2 已修。
6. **34-9**：去除「可滚动」措辞（见上）。
- **进度追踪**：M45 任务按子项粒度执行，每完成一项即勾选 TASKS + 更新 STATE
  并单独提交；上下文切换时以 `docs/STATE.md` 当前任务 + TASKS 第一个 `[ ]` 为准。
- **测试失败定位**：M45-9a 首版用 `selected_names()` 替换 archive_plan 选集，
  导致 `m253_archive_guards` 失败——`selected_names` 把 tags 按 entries 过滤，
  丢掉了测试刻意注入的缺失条目 `ghost.txt`（原语义要求作业体报"源缺失"）。
  修法：archive_plan 保留 tags 原样（含已消失条目，作业体报错），仅在 tags
  为空时增补 v 可视选区；m459a 测试锁定 v 多选。修正后 511 单测全绿。
  M45-6a 前曾见过一次未定位的偶发失败（复跑 3 次全绿），若再现再排查。
- **M45-2b 踩坑（只能嗅探常规文件）**：类型图标对 devicons 默认 `*` 的无后缀
  文件做 4KB 文本嗅探；首版未判文件类型，`Tab::from_physical("/tmp")` 时打开
  `/tmp/vifm-ipc-vifm`（FIFO）与若干 socket，`open/read` 永久阻塞 → 整测试
  套件挂死（`visual_selection_flow` 等看似无关的测试被拖住）。修法：嗅探前
  `metadata.is_file()` 守卫（符号链接经 metadata 跟随判定，指向 FIFO 同样拒绝），
  并加 `m452b_sniff_skips_socket_without_blocking` 回归测试。WHY 不用
  O_NONBLOCK：跨平台且需 unsafe，metadata 守卫已足够且无 unsafe。
- **M45-1a Windows CI 修复（2026-11-23）**：M45 batch 全勾后 CI `test (windows-latest)`
  失败 2 项（`feat2_parent_row_navigates_like_h` / `feat2_mouse_click_parent_row_focus_then_open`）——
  M45-1a 把 `display_path()` 统一为 `/` 分隔，旧测试断言直接比 `tmp.path().to_string_lossy()`
  （Windows 为 `\`）。修法：测试侧同样 `.replace('\\', "/")` 再比（沿用 AGENTS §7
  路径统一 `/` 口径）；本地 Linux 无法复现，CI 失败日志经 `gh api .../jobs/<id>/logs`
  一次定位。WHY 不改生产代码：`/` 统一是 M45-1a 明确需求，回退会复发显示分叉。
- **M45-10 文档收尾（2026-11-23）**：CHANGELOG 新增 `[Unreleased]` 汇总 M45
  全部用户可见变更；README/GUIDE/help 同步（路径缩写/标签栏/分隔线/确认弹窗/
  可执行文件特例/对侧解压/归档 yy 粘贴/命令名补全）；tutor `archive_ops`
  更新 `:extract` 缺省对侧语义并新增归档成员 `yy`→`p` 条目（M43-6 维护规则）；
  28 子任务 + 文档收尾全部勾选，TASKS 无未勾选项。人工验证项仅 C6（原生协议
  大图不越界）deferred，可任意真实终端补做。
- **M45-5c 范围自决（2026-11-23）**：SPEC M45-5③ 标注「需用户确认范围
  （命令名补全 vs 参数提示）」，但不在用户 5 项冲突裁决之列，且未标注「不可
  自行决定」。按 AGENTS §3 自行决策：实现**命令名 Tab 补全**（首词位置
  `COMMAND_NAMES` 前缀循环，有空白后仍为文件路径补全），不做参数提示
  （需集中命令签名注册表，当前 exec 分派无此结构，超出「辅助提示」最小
  语义）。决策已落 SPEC/TASKS；若用户后续要参数提示，按 FIX/FEAT 批次补。
- **gate 卡死教训**：`cargo test` 被后台 shell 中断后残留的测试进程会继续跑，
  排查时先 `ps` 确认无残留；用 `timeout` 包裹单测避免再次悬挂。
- **暂存索引测试互踩（M45-3c 期间复现）**：`m377_trash_clear_and_purge` 的
  `:trash clear` 与其它 `execute_delete`/restore 测试并行时，会把对方刚 staged
  的文件/索引删掉 → `clear 失败 1: No such file or directory` 偶发失败（此前
  M45-6a 前的「未定位偶发失败」疑同源）。修法：现有 `TRASH_TEST_LOCK` 扩到
  全部真实 trash 索引读写测试（m377 / execute_delete_then_undo_restores /
  cross_tab_undo…，锁获取用 `unwrap_or_else(|p| p.into_inner())` 防中毒级联）。
  WHY 不改 `trash clear` 忽略 NotFound：生产并发场景本就不该报错，但属行为
  变更，留待独立 FIX 评估；测试串行化已足以消除本项目内的不确定性。

### 2026-11-24 — M45 批次收尾断点（自动发布待 CI fuzz job）

- **HEAD**：`47673ea`（M45-1a Windows 路径分隔符测试修复）；工作树干净，main 已
  推送 origin。M45 全部 28 子任务 + M45-10 文档收尾已勾选；TASKS 无 `[ ]`。
- **已验证**：本地 fmt/clippy -D warnings/build/537 单测/12 集成/harness_check
  全绿；CI run 45240675080 的 `test (ubuntu|windows|macos)` 与 `supply-chain`
  均 success（Windows 首轮曾因 M45-1a 路径 `/` 统一失败 2 项，已在 47673ea 修）。
- **已完成（2026-11-24 同日续轮）**：run 45240675080/45243627876 全绿 → bump
  1.3.0 + tag v1.3.0 → tag run 45256802332 **9 job 全绿** → GitHub Release v1.3.0
  三 musl 归档 + SHA256SUMS 3 行（`sha256sum -c` 实测通过，aarch64 静态 ELF +
  `--version` = rvifm 1.3.0）。crates.io 未发布（停在 1.0.0，用户 2026-11-23 决策）。
  C6（原生协议大图不越界）仍是唯一人工 deferred 验收项。

### 2026-11-23 — FIX 批次蒸馏（REL-3 / FIX-5 / FIX-6；用户实测反馈，待确认）

- **REL-3 根因（已核实）**：`v1.1.0` tag run 45101107044 九 job 全绿、四归档
  齐全，`SHA256SUMS.txt` 却只有 1 行（`873b... windows-x64`）；`v1.0.0` 同病
  （仅 aarch64 行）。`ci.yml` release job 为 4-target matrix，每腿在自身
  `dist/`（只含本 target 归档）执行 `sha256sum *.tar.gz > SHA256SUMS.txt`
  再 `gh release upload ... --clobber`——最后完成腿覆盖前面全部。WHY 用独立
  汇总步骤而非让某腿聚合：matrix 各腿并行、完成顺序不定，聚合点必须在所有
  归档上传之后且只执行一次；历史版本回填用 GitHub Release 实际归档的真实
  sha256（下载实测），不改归档/tag/版本。
- **REL-3 四归档实测 sha256（2026-11-23 下载 GitHub Release 归档 `sha256sum`）**：
  - v1.0.0：`915e2375…` aarch64-gnu（现有唯一行，正确但不全）、
    `a47cc01f…` x64-gnu、`9e2e7fdf…` x64-musl、`3c73bd91…` windows-x64。
  - v1.1.0：`4efd7268…` aarch64-gnu、`73cd9a33…` x64-gnu、
    `474f5297…` x64-musl、`873b1524…` windows-x64（现有唯一行）。
  - 现有单行哈希与实测一致 → 回填只需补全缺失三行，不改动已有正确行。
- **REL-3b 延期**：本机 `gh` 未安装、无 `GITHUB_TOKEN`/凭据（git remote 为 SSH，
  `git credential fill` 无返回）→ 上传需人工鉴权。备置
  `scripts/repair-release-checksums.sh`（`gh release download` → 数量守卫 4 →
  `sha256sum` → `gh release upload --clobber`），人工 `gh auth login` 后一条命令
  回填两版本；CI 侧修复对未来 tag 生效。属「必须人工执行步骤」按 AGENTS §3
  延期处理，不阻塞 FIX-5/FIX-6。
- **FIX-5 根因（代码定位）**：`app/render.rs` 中 SortDialog/ArchiveDialog/
  HistoryMenu/ContextMenu 等覆盖层均先 `render_widget(Clear, area)`，而
  `InputMode::TutorList`（~L887）与底部 Tutor Panel（`state.tutor`，~L1281）
  直接 `render_widget(...)` 无 Clear/背景 → 下层目录列表/状态栏文字透出。
  WHY 只清面板/列表区域而非全屏黑底：M42-1 设计意图是"主 TUI 背景保持可见"
  作为练习上下文（透明放行 = 按键路由，非视觉透明）；用户诉求是面板**自身区域**
  不透出干扰，两者不冲突。
- **FIX-5 测试坑（CI macOS 首跑暴露）**：TestBackend 下 CJK 按"字+空格"占双宽，
  其续格会残留上一帧的 symbol（ratatui/TestBackend 伪影，真实终端无此问题）。
  最初断言用"compact 后不含 f00/f01/f02"——这些串被 CJK 续格拆开而假通过；
  macOS 长 temp 路径的续格落到标题区又使"核心导航"连续性断言失败。改为断言
  **面板内无 CJK 的空白行**（基线含 f00、Clear 后为空）——平台无关且直击
  "不透出"语义。教训：覆盖层测试应挑无 CJK 的断言区域，不依赖跨 CJK 的
  字符连续性。
- **REL-3b 已完成（2026-11-23 人工）**：用户 `gh auth login` 后执行
  `scripts/repair-release-checksums.sh v1.0.0 v1.1.0`，两版本 SHA256SUMS 各
  回填 4 行，API 核实哈希与实测一致（首次 `curl` 读到空行系 GitHub CDN 缓存，
  加 cache-buster / 走 API octet-stream 即见最新）。REL 批次全清零。
- **REL-4 评估（aarch64 musl 交叉编译）**：① musl.cc `aarch64-linux-musl-cross`
  （GCC 11.2.1，2021）编译 aws-lc-sys `pkcs7.c` 触发 ICE（`alloc_aux_for_block`）——
  放弃；② 裸 `zig cc -target aarch64-linux-musl` 被 cc-rs 自动追加的
  `--target=aarch64-unknown-linux-musl` 覆盖，Zig 0.14 报
  `UnknownOperatingSystem`——放弃；③ `cargo-zigbuild 0.24.1` 翻译 Rust target，
  本地实测 `cargo zigbuild --release --locked --target aarch64-unknown-linux-musl`
  成功（5m24s 冷编译），`file` = `ELF 64-bit LSB ... ARM aarch64, statically
  linked, stripped`，无 INTERP/NEEDED——采用。CI 用固定 zig `0.14.1` tarball +
  `cargo install cargo-zigbuild --locked`，避免版本漂移。
- **REL-4 版本 1.2.0**：产物矩阵变更（移除 gnu、aarch64 转 musl）用户可见，
  semver minor；crates.io 仍停在 1.0.0（用户 2026-11-23 仅 GitHub Release 决策）。
- **REL-4 首跑失败（tag run 45129475971）**：`release-checksums` 上传步骤
  `gh release upload ...` 报 `failed to run git: fatal: not a git repository`——
  该作业无 `actions/checkout`，gh 无法从 `.git` 推断仓库（下载归档/算哈希步骤
  均成功，仅上传失败；归档 3 个已上传，SHA256SUMS 缺失）。WHY 不用 checkout
  而加 `--repo "$GITHUB_REPOSITORY"`：汇总作业只需 artifacts + gh API，加
  checkout 是多余 I/O；显式 repo 更轻且杜绝隐式 git 依赖。修复后 tag 重指向
  （`git push -f origin v1.2.0`，本项目先例）→ run 45132119778 九 job 全绿，
  三归档 + SHA256SUMS 3 行，下载实测 `sha256sum -c` 通过；aarch64 产物
  `file` = ARM aarch64 statically linked ELF 且 `--version` 输出 `rvifm 1.2.0`。
- **教训（CI 诊断）**：`gh` 在无 checkout 的作业里须显式 `--repo`；匿名 API
  看不到作业日志（`403 Must have admin rights`），本机 `gh auth login` 后
  `gh run view <id> --job <id> --log` 一次定位根因。
- **FIX-6 根因（代码定位 + 测试盲区）**：`save_session`（session.rs L91）无
  `create_dir_all`，`session_path()` = config.toml 同目录 `session.toml`；全新
  安装无 `~/.config/rvifm/` 时 `fs::write` 静默失败（仅 eprintln）。路由本身正确
  （`:q` 与 `q` 均调用 save_session，序列化含全部 tab；测试 `m251_roundtrip`
  直接驱动纯函数故未覆盖真实路径）。WHY 不作为需求变更：会话原设计即自动保存
  （M25-1），本次是实现对不齐 tutor progress 已建立的 `create_dir_all` 模式。


### 2026-11-24 — FIX-7 双栏焦点能力一致性

- **决策**：撤销 M14 初始“右 pane 仅导航”裁剪；键盘 Normal 动作与目录/文件命令按焦点 pane 分派，实际能力仍由 Location（归档只读、远端白名单等）决定。
- **实现结论**：右 pane 的父目录行与左 pane 对齐；右 pane 普通键复用共享 Action 分发；`:mk`/`:mkdir`、粘贴目标及 `o` 历史入口按焦点处理；归档成员 `yy` 可经 `p`/`P` 解压到物理目标。旧 `d/D/c` 右栏禁用提示已移除。
- **文档**：README、GUIDE、内置帮助与 SPEC/ACCEPTANCE 同步；tutor 内容未发现右栏限制。


### 2026-11-24 — FIX-8 Frecency 面板设计评估

- **裁决**：TUI 内保持“焦点 pane = frecency 搜索区、对侧 pane = 当前候选预览”；不统一强制左侧。该方案与双栏焦点一致，也保留右栏作为目标目录的场景。独立 `pick` 无文件管理器焦点，固定左搜索/右预览符合传统阅读方向。
- **初查差异（FIX-8-1 已修复）**：`o` 原使用嵌入式 FrecencyPane，`:j`/`:J` 原用 JumpSelect 浮层；预览曾直接调用纯文本 `render_preview`，遗漏 ImageEngine 与 `:view` 语法高亮；鼠标事件未路由到嵌入面板。FIX-8-1 已统一面板、预览与点击路径，并补回归测试。
- **边界**：`pick` 的固定左右双栏保留；不改评分、存储与 `pick` 只读语义。

### 2026-11-24 — FIX-9 输入法切换方案

- **决策**：沿用 Vim/Neovim 成熟的模式边界策略：离开可编辑模式时记录状态并转为英文，重新进入时恢复；不只拦截 Esc。Fcitx 优先停用当前输入法状态以保留引擎；IBus 通过保存/切换/恢复 engine 实现英文回退。
- **依赖边界**：不增加 Cargo crate、C 库或 D-Bus 绑定。采用 Linux 桌面已有 `fcitx5-remote` / `fcitx-remote` / `ibus engine` 命令，以固定参数通过 Rust `Command` 调用；其缺失、失败或桌面会话不可用均降级为原行为。该命令接口做法与 Vim 插件生态一致，Fcitx5 原生 D-Bus 虽更直接，但为了零新增依赖不采用。
- **Fcitx 状态码**：`fcitx-remote` 与 `fcitx5-remote` 的 `1` 是已连接但未激活，`2` 是激活；仅状态 `2` 才保存激活态并发送 `-c`，返回编辑模式时才发送 `-o`。状态 `1` 保持原样。
- **范围**：默认仅 Linux 自动探测；输入法并发由桌面或用户修改时不保证合并状态。英文 IBus engine 可配置，避免假定所有用户都使用同一键盘布局标识。
- **验收延期**：真实桌面上的 IBus/Fcitx/Fcitx5 状态切换依赖登录中的图形会话，当前自动化环境不具备该会话，人工往返测试列为 FIX-9-M1 deferred；假命令 runner 覆盖自动化状态机，不依赖宿主输入法。
- **跨平台延期**：本机只安装 Linux target；Windows/macOS 下 worker 条件分支等待对应 CI runner 编译验证（FIX-9-CI deferred）。

### 2026-11-25 — REL-6 发布：平台相关默认值不得进测试断言（WHY）

- **CI 首跑 red**：run 45439549429 的 Windows job 仅 `m466_default_config_generation_and_no_overwrite` 失败——M46-6 生成的 `config.toml` 模板硬编码 `[ime] enabled = true`，而 `ImeConfig::default().enabled = cfg!(target_os = "linux")`（Windows/macOS 为 false），模板解析结果 != 内置默认，断言失败。Linux 本机该值恰为 true 故掩盖了缺陷（AUDIT-14 同类：宿主/平台相关值不得进测试断言）。
- **修复**：模板把 `enabled = true` 改为注释行，由 serde 采用平台默认——既让模板准确（Linux 默认开、Windows/macOS 默认关且选项无效），又使三平台断言一致。run 45444178315 全绿（三平台 test + fuzz + supply-chain）。
- **教训固化**：新增"默认配置模板"类内嵌文件时，必须逐字段核对 `Config::default()` 是否平台相关；凡 `cfg!(target_os)` / `cfg!(windows)` 默认值一律不硬编码进模板，留注释交 serde。与 NOTES 既有 AUDIT-14「测试环境隔离」同源，不单列新规则。

### 2026-11-24 — M46 用户实测批次蒸馏（12 项；照例先蒸馏后执行）

- **来源**：用户实测反馈 12 条，含 5 条"既有任务未真正修复"的回退。登记前对三条主线各做一次逐行代码定位（预览/归档、渲染/导航、配置/帮助/弹窗），根因均落到具体函数，避免再次"表面勾选"。
- **回退根因（WHY）**：
  - M45-4a 图片越界：`image.rs` 只把图片 area 与 `frame.area()`（整屏）相交而非预览 pane；原生协议 `SlicedImage` 虚拟高度可达 3×窗格，绝对定位图形不受 widget 裁剪；`render.rs` 中 `viewer_rect` 用未收缩的 frame 而 pane 被 tutor 面板收缩，几何错位。三处同修才算真修复。
  - M45-2a 右栏：`render_right_pane` 的 `ft` 文字色无条件应用，缺左 pane `row_reversed` 守卫；且右栏无 `highlight_style`，整行背景无从统一。
  - M45-3e `h`→`l`：`enter_selected` 在 `restore_cursor()` 还原后无条件 `on_parent_row = has_parent_row()`，视觉上高亮 `..` 覆盖还原值；旧测试只断言 `cursor` 不断言 `on_parent_row`，故漏检。
  - M45-7/M37-2 extract：`extract_members` 与命令臂均不刷新任何 pane；`apply_job_update` 仅 `tab.refresh` 不刷 `tab.right`；watcher 只监听左栏目录。
  - FIX-5 思路复用：help 覆盖层同样缺 `Clear`（bug a）；`/` 进 `HelpSearch` 后 render 无对应分支而落到普通双栏（bug c，内容"被清空"实为下层列表透视）。
- **决策**：
  - M46-2 只做渲染层焦点守卫，不把 `SearchState` 拆成 per-pane（改动面小、与右栏现有守卫对称）；per-pane 化作为 CUT 放弃（vifm 的 per-pane pattern 不在本批次范围）。NOTES 2026-11-21 M34-3 条旧边界记录自此更新。
  - M46-6 默认配置**仅在 TUI 启动钩子**生成（复用 `config_path()` + `create_dir_all`），不放入 `load_from`/`load`，避免污染 temp 路径测试与 `cargo test`；旧注释"不自动生成配置文件"（`opener_hint`）随本任务改写；不引入 `dirs`/xdg crate。
  - M46-7 新窗口走既有 `run_external_target_with` + tmux/screen（无 GUI 终端模拟器硬依赖），新增 `rvifm help` 子命令输出内嵌文本；`[ui] help_in_new_window` 默认 true，无复用器回退应用内不透明覆盖层；"激活"为尽力而为（可选中/切换），不强制抢焦点（SIMPLIFIED）。
  - M46-8 预览提速采用内存缓存 (路径, mtime, size) + 大小护栏，不改 frecency/只读边界；`.apk` 已支持保持不动，`.deb`/`.rar` 新增只读后端，统一扩展名真值表避免三表漂移（M34-6 教训）；写端 `ops.rs` 不在本批次范围（CUT）。
  - M46-10 删除确认重做沿用 M45-3c 的居中弹窗基座，扩展为携带文件列表 + 光标/滚动 + 滚动条，状态类型与 `Outcome` 同步扩容；tagged 多选与单文件共用一套渲染。
- **排序**：1→12，缺陷修复先行，help/tutor 文档殿后；真实终端大图与 Windows/macOS 平台行为按既有 deferred 处理，不阻塞批次落地。
- **M46-9 实现决策**：`.deb` 不引入新 crate——手写 ar 头解析（60 字节头 + 偶数对齐）后把 `data.tar.*` 解码为内存字节，经新增 `Decoder::from_bytes` / `TarFs::from_bytes` 复用既有 tar 索引与读取逻辑；VFS 以虚拟目录 `data/` 暴露，导航层仍按叶子名拼接 inner 路径，无需改 tab.rs。`.rar` 走外部只读工具（bsdtar → unrar → 7z），缺失返回含「请先解压后浏览」的可读错误（保留旧 fix3 用例语义）；`is_unsupported_archive` 现恒 false，仅为兼容预览回退调用保留。真实工具可用性与大包性能待用户补验。
- **M46-7 测试隔离**：宿主环境存在 `TMUX`，故 `:help` 新窗口路径在既有 `exec_q_cd_help_unified_parse` 用例中显式设 `help_in_new_window=false` 以确定性覆盖应用内分支（AUDIT-14 原则）；新窗口路径用 `rvifm help` CLI 冒烟覆盖，不依赖真实终端。
- **M46-5 实施中发现（观察项，未在批次范围）**：`make_test_zip` 夹具含显式目录条目时，`:extract` 会把成员路径拼成 `src/src/main.rs` 并报"条目不存在"——`ZipFs::list` 返回全路径而 `extract_members` 又追加父目录前缀。属既有缺陷，与"刷新"无关；本批次用仅含文件的夹具绕开，后续批次单独登记修复。

### 2026-11-25 — M47 用户实测批次蒸馏（7 条反馈，照例先蒸馏后执行）

- **编号**：沿用 M45/M46 用户实测批次先例取 M47。范围 = 预览缩进 / 焦点化收尾（`=`/`:archive`/`:extract`/剩余快捷键）/ frecency 面板融合 / IME 防御性激活 / 右栏 `..` 高亮 / 每栏独立选择 / 栏间距高亮。**用户明确指令：本批次只提交并推送代码，不 bump 版本、不推送 tag、不触发 GitHub Release。**
- **M47-1 根因**：TUI 目录预览（`app/preview.rs::render_view_path`）把 `"  ".repeat(depth)` 加在文件名前、图标仍在最左列；共享预览（`preview.rs`）已有 2 空格基础缩进但嵌套成员不分层。裁决：统一为「缩进在图标之前、按路径层级递增」，显示层专用。
- **M47-2 根因**：`filter` 只存在 `Tab`（左栏字段），`Pane` 无该字段，且 `Tab::refresh` 只在左栏 `entries` 上 retain；`dispatch_right_pane` 只投影 5 个字段不含 `filter`，`=` 返回的 `Outcome::EnterFilter` 在投影撤销后才被 `mod.rs` 处理 → 永远写左栏。裁决：`Pane` 增 `filter`，`apply_filter` 按焦点写对应 pane；顶部路径显示状态栏指示移自 `render.rs:2004-2007`；Normal 模式 Esc 清焦点栏过滤。
- **M47-3 根因**：`archive_plan` 源取 `tab.tags`/`tab.visual_range()`/`tab.entries[tab.cursor]`（左栏）而 `base=physical_base()` 已焦点感知，产物落焦点栏；`:extract` 目标已默认对侧（`extract_dest`）但源归档恒取左栏光标。裁决：源改焦点栏、产物改对侧栏；`:extract` 源改焦点栏。
- **M47-4 根因**：`dispatch_right_pane` 仅投影 location/entries/cursor/scroll/on_parent_row；`keys.rs` 右栏早退块先处理 `/`、`:`、`g`、`n`/`N` 后落到 `handle_right_key`（导航 only）；exec.rs 多处命令（`selected_names`/`quickjump`/`mark_set`/`:sort`/`:select` 等）直接读左栏字段。裁决：增量改为焦点感知，不恢复「右栏仅导航」限制；能力边界（归档只读、远端白名单）保留。
- **M47-5 根因**：`render_frecency_panes` 用 `Block::bordered()` + `pane_border_style(true)` 产生弹出观感；标题缺模式后缀；点击命中几何同用 bordered inner，改无边框需同步命中。
- **M47-6 根因**：`ImeWorker::start` 只在启动时发一次 `EnterNormal`，之后仅在 `ime_editable_mode(&mode)` 翻转时切英文；Normal 下 Esc（`keys.rs:1494-1503`）不改 mode、不触发；从其它窗口切回时无焦点事件处理。且 `ImeManager::enter_normal` 有 `saved.is_some()` 守卫，重复发送无效。裁决：新增不覆盖 `saved` 的 `force_english` 通道（新 WorkerCommand），触发点 = 启动 + Normal 模式 Esc（终端焦点事件不可靠，不以桌面级监听替代）；节流靠「查询后仅在非英文时发切换命令」天然幂等；不修改 FIX-9 既有模式边界语义。
- **M47-7 根因**：`render_right_pane` 的 `..` 行按 `on_parent_row` 反显（正确），但条目高亮在 `idx == cursor` 时无条件 `REVERSED`（缺 `!on_parent_row` 守卫），与左栏 `ListState` 守卫不对称。
- **M47-8 根因**：`visual`/`tags` 是 `Tab` 级共享字段，左右渲染器都无条件 `tab.tags.contains(...)`，右栏多选后切焦点左栏仍读到同一集合。裁决：`Pane` 增 `visual`/`tags`，`dispatch_right_pane` 一并投影，使键盘管道仍操作左栏字段（实为焦点栏数据）；渲染端各读自己 pane 的集合；exec.rs 命令显式按焦点取集合。
- **M47-9 裁决**：采用「列表项前增加前导空白 + 整行高亮天然覆盖该空白」的低风险方案；不改动 `pane_layout` 的 gap 宽度（大量 TestBackend 几何断言依赖现有列坐标），避免无关回归。
- **M47-4 审计结论（2026-11-25，逐项代码定位）**：
  - 键位管道已由 `dispatch_right_pane` 投影 location/entries/cursor/scroll/on_parent_row（FIX-7）；本批次增投 `filter`/`visual`/`tags`，使 `=`/`v`/`t`/Esc 在右栏复用共享管道后写回右栏。
  - 命令层已焦点化：`selected_names`（批量/重命名/复制公共选集）、`substitute_vars` 的 `%d`/`%f`/`%F`（原 `%d`/`%F` 恒左）、`current_item_info`（`:file`）、`mark_set`/`bmark_set`、`archive_plan`/`:extract`、`:sort`、`:select`/`:unselect`。`jump_to`/`physical_base`/`extract_dest`/搜索系在 FIX-7/M45/M46 已焦点化，不重复改。
  - 保留既有边界：右键上下文菜单固定左栏（M41-3 裁决，鼠标右键命中同源）；`history_menu_nav` 依赖 Tab 级历史（右 pane 无历史栈，保持）；归档只读、远端白名单、输入模态语义不变。
  - 排序比较器抽为 `sort_entries_specs` 纯函数，左/右 pane 共用；右 pane 增 `sort` 字段并在刷新后重应用。
- **提交粒度说明**：M47-3 与 M47-4 共享 `focused_entries`/`focused_cursor` 等焦点访问器与同一批文件，合并为一次提交便于切换模型续跑，任务勾选仍逐项独立。
- **批次收尾（2026-11-25）**：10/10 全部勾选，579 单测 + 13 集成、fmt/clippy/build/`--version`/harness_check 全绿；文档（README/GUIDE/help/CHANGELOG/tutor）已同步，tutor 保持 13 模块 100 项。按用户指令**不执行版本 bump / tag / GitHub Release**，覆盖 AGENTS §3 的「全部任务完成后自动发布」规则——此为一次性用户裁决，不改变后续批次的默认发布流程。
- **排序**：1→10，缺陷/焦点化先行，文档殿后；人工终端类验证沿用既有 deferred 口径。

### 2026-11-25 — M48 用户实测批次蒸馏（12 条反馈，照例先蒸馏后执行）

- **编号**：沿用 M45–M47 先例取 M48，13 任务。沿用用户 2026-11-25「不 bump / 不 tag / 不 Release」口径（本批次未另授权发布，保守沿用）。
- **已定裁决（蒸馏时拍板，执行到对应任务不再等待）**：
  - M48-7 归档预览采用**可选外部列目录工具**（zip→`unzip -l`、tar→`tar -tf`、7z/rar→`7z l`、deb→`bsdtar -tf`）快速路径：命令固定参数、不经 shell、输出与成员数有上限；内置 VFS 仍是默认与回退，不引入必需依赖。不把 `vim`/`view` 本身作为预览手段（会覆盖 TUI 且无法与预览窗格同源），只借 vifm `fileviewer` 的「调外部只读列表命令」思路。
  - M48-8 `:archive` 无参直接按默认方式执行，交互入口改 `-i`（原无参开对话框行为作废）；对话框字段结构不变，仅改触发与视觉。
  - M48-9 终端 FocusGained 在多数终端不可达，故以 Normal 模式**周期探测**（1s 量级）兜底，触发仍走 `ImeWorker::force_english`（不覆盖 saved）；不新增桌面级监听。
  - M48-2 可执行文件默认特例保留，只对 `.sh`/`.txt`/`.md` 等文本类按文本优先识别；编辑器类 opener 走新终端窗口优先，避免遮住 rvifm。
- **批次收尾（2026-11-25）**：13/13 全部勾选，590 单测 + 13 集成、fmt/clippy/build/`--version`/harness_check 全绿；README/GUIDE/help/CHANGELOG/tutor 与验收同步。**不触发版本 bump / tag / GitHub Release**（沿用用户 M47 指令；不改变后续批次默认发布流程）。
- **M48-1/12 外窗裁决细节**：tmux `new-window` 去掉 `-d`（默认选中新窗口即激活），追加 `select-window -t :` 兜底；`-c <focused dir>` 指定起始目录；screen 用 `cd <dir> && cmd` 包裹并 `screen -X next` 激活。该路由覆盖 `s`/`:!`/`gi`/自定义命令/`:help`。
- **M48-7 外部预览裁决**：不调用 `vim`/`view` 本身（会覆盖 TUI 且无法与预览窗格同源），只借 vifm `fileviewer` 的「调外部只读列表命令」思路；`unzip -Z1`/`tar -tf`/`7z l -ba` 固定参数、`Stdio::null()` stderr、4MB stdout 上限；`[preview] archive_external_listing` 默认 true，缺失/失败/超限静默回退内置 VFS；不引入新 crate。
- **M48-8 行为变更**：`:archive` 无参从「打开对话框」改为「按默认直接执行」（zip + 文件名去扩展名 + `.zip` 后缀），交互入口改 `-i`；`archive_plan` 统一按格式补扩展名，避免预填名与源文件名冲突导致回车失败（这正是用户实测「任意字段回车不执行」的根因之一）。
- **M48-6 根因**：归档后端 `list()` 返回的 `ArchiveEntry.path` 已是完整归档内路径，`extract_members` 再拼 `inner/` 造成 `a/a/c.json` 读取失败（目录只建空壳）；yy 固化 `base_inner` 前缀又使单文件落盘成 `a/c.json`。修法：目录分支直接用 `entry.path`；粘贴时取所有选中路径的最长公共 inner 前缀剥离。
- **M48 外窗回归修复（2026-11-25，用户实测反馈）**：
  - 现象：外窗执行时反复弹出 `sh: 0: getcwd() failed: No such file or directory` 与 `Not in a git repository. Create one here? (y/N)`（20+ 次）。
  - 根因①：`tmux new-window -c <focused dir>` 未校验目录存在——焦点栏目录被删除/移走后传给新 shell 的 cwd 无效；`sh` 启动即 getcwd 失败，其启动脚本反复报错。修法：`safe_external_dir` 逐级回退（焦点目录 → 当前目录 → HOME → 临时目录），只把真实存在的目录交给 `-c`。
  - 根因②：`gi`/`:gitui` 在非 git 仓库仍启动 `gitui`，其「Not a git repository. Create one here?」在无 TTY 的新窗口里循环刷屏。修法：启动前用 `git_root_of(focus_base)` 判定，非仓库直接给可读错误、不 spawn。
  - 另移除 `tmux select-window -t :` 兜底调用（目标表达式有歧义，`new-window` 不带 `-d` 本身已选中新窗口）。
- **排序**：1→13，缺陷/交互先行，文档殿后。

### 2026-11-26 — M49 用户实测批次蒸馏（LAYOUT-1..4 + 反馈 124–141）

- **布局决策（LAYOUT-1..3，已被 M52 反转）**：frecency 与 `:view` 默认预览**固定右侧**，搜索/目录列表固定左侧——理由是空间稳定性优先于视线距离（预览区不随焦点切换跳动，利于快速瞥确认内容）；对齐 yazi/ranger。动态对侧（焦点栏搜索、对侧预览）保留为 `[preview] fixed_side = false` 配置选项，不作默认。
- **共享实现**：抽出 `render::preview_layout_areas()` 作为两模式共用的位置语义；`FrecencyPane` 增 `preview_focus/preview_scroll/preview_query/preview_match`，Tab 在原布局只切焦点归属、不切 pane，预览焦点复用 viewer 渲染链（滚动/搜索/高亮）。`Tab::preview_path_with(fixed_side)` 在固定模式恒取左栏光标文件。
- **热切换**：配置只影响新打开的会话（用户明确授权「热切换成本高则按新会话交付」）；已打开的 frecency/`:view` 不重排。
- **不发布**：沿用用户 2026-11-25 口径，本批次只提交并推送，不 bump / tag / Release。

### 2026-11-26 — M50 命令补全增强（DeepSeek 分享稿蒸馏，用户已确认）

- **来源**：用户提供 DeepSeek 分享链接（4 轮 Q&A，主题「rvifm 命令模式自动补全增强」）。蒸馏后仅汇报、未执行；用户确认 5 项决策后才登记 M50。
- **用户确认的决策**：① 登记并执行；② UI 复用现有 frecency overlay、高度 ≤40% 终端（不照搬 Tridactyl 半屏）；③ 采纳 shell 历史解析、**默认开**、优先**增量读取**；④ 新增 `OpenExternalCommandLine` Action 但**不设默认键位**（用户自定义 keymap 触发）；⑤ 变量先落地裸 `%f/%F/%d/%D` + `[command] quote_expansion` 开关，`%"f` 引号变体本批次不做。
- **WHY（数据源）**：fzf Ctrl+R 读的是已落盘 shell 历史文件（不是实时监控），且 fzf 本身不扫描 PATH——所以外部补全候选取「历史完整命令行」，不做 PATH 发现；评分按命令名聚合、展示完整行。
- **WHY（不做 B）**：`execve` 守护进程监控引入常驻进程、Linux 特有、隐私敏感，性价比低，明确 OUT_OF_SCOPE。
- **隐私边界**：完整命令行仅作内存补全候选；只有**命令名**进入 frecency `cmd` topic（持久化），文档需写明「当前仍打开的 shell 会话尚未落盘的命令可能缺失」。
- **排序**：M50-1 → M50-6；沿用不 bump / tag / Release。
- **M50 执行后修复（用户实测反馈）**：`cargo test` 期间残留大量 `cosmic-edit /tmp/.tmp*/a.txt` 进程——根因是单测通过无 opener 规则的文本条目走到 `system_open` 真实启动了 GUI。修法：`system_open` 在 `#[cfg(test)]` 下只模拟结果、不启动外部程序（AUDIT-14 测试隔离同源）。另把补全用的 `cmd_scores()` 改为 `open_store_readonly()`，避免与并发测试争 redb 写锁导致 `Database already open` 抖动。
- **M50 启动性能修复（用户实测：debug 二进制 460M、启动极慢）**：
  - 二进制：`[profile.dev.package."*"] debug = false` 关闭依赖 DWARF（russh/syntect 等占大头），rvifm 自身保留源码级调试；461M → **152M**。
  - 启动慢根因①：`Picker::from_query_stdio()` 在 tmux/screen 里发送被包裹的转义查询且等不到响应 → 阻塞；改为复用器内直接 halfblocks（无条件可用）。
  - 启动慢根因②：M50 shell 历史导入每次启动逐条 `append_event`（2000 行历史 = 2000 个写事务，并把库膨胀到 3MB 触发每次 compaction）。修法：`Store::append_events_batch()` 单事务批量写；只导入尚未入库的命令名、上限 500；`shell_history.json` 记录每文件字节偏移实现真正增量。修复后首次启动 0.26s、常规启动 0.18s（实测 pty）。
  - 诊断开关：`RVIFM_STARTUP_TRACE=1 rvifm` 打印各启动阶段耗时（默认关闭）。

### 2026-11-26 — M51 用户实测批次 6 蒸馏

- **IME 过度干扰根因**：输入法状态是桌面全局的；M48-9 的 1s 周期 `force_english` 在 rvifm 仍处 Normal 时持续把全局输入法切回英文，导致 `s` 新窗口里的 vim/任意程序无法保持中文输入。修法：进程内 `IME_SUSPENDED` 标志——外部窗口 spawn 成功、`FocusLost` 时暂停；`FocusGained` / 任意按键时恢复；`force_english` 暂停期直接返回（不影响 FIX-9 保存/恢复）。tmux 内无焦点事件时以「spawn 暂停 + 按键恢复」为准。
- **测试隔离补充**：`run_external_target_with_and_dir` 在 `#[cfg(test)]` 下直接返回 `CurrentPane`，不再真实 spawn tmux 窗口（否则改变全局 IME/焦点状态并污染并行测试）。
- **文本识别**：`mime_of` 对未知扩展名做 4KB 有界嗅探（无 NUL + UTF-8 即 text/plain），扩展名表补 `vifm/vim/cfg`；`LICENSE` 等无扩展名文本也可命中 `text/*` opener 规则。默认 `edit_text` 改 `vim %s`。

### 2026-11-26 — M52 预览布局策略反转（用户确认「理解正确」后登记）

- **变更**：M49 刚落地的「默认固定右侧」被反转。新口径：`[preview] fixed_side` 默认 `false`、**仅作用于 rvifm 内 `o`/`:j`/`:J` 的 frecency 模式**；`:view`（`w`）恢复对侧预览（M34-4/M39-6 语义，焦点左→右、焦点右→左）；`rvifm pick` 恒左候选/右预览（本就如此，不变）。
- **WHY**：固定右侧牺牲了 `:view` 的对侧视线一致性；用户决定把布局选择权缩小到 frecency 一个场景，其余恢复 vifm 习惯。M49 的「空间稳定性优先」理由不再适用，但 M49 历史条目与提交保留（在 SPEC/ACCEPTANCE/CHANGELOG 标注被 M52 取代），便于追溯。
- **兼容**：保留 `fixed_side` 键（显式 `true` 仍可用，仅新会话生效）；已有 `config.toml` 若显式写了 `fixed_side = true` 不会被默认值改变，需用户手动移除/改 false。
### 2026-11-27 — M77–M78 裁决

- 徽章移除但保留境界：用户希望界面更干净，等级信息由境界单独承担；进度条改为满宽背景色块，数字只在进度文字中出现一次。
- 面板计数统一取完整模块的持久化进度，避免 session 子集导致「50% 与 0/6」并存；入口改为错题+未学习（排除已掌握），skipped 清空不再回退全部。
- 默认配置补 vim 文本规则（system opener 仍是最终回退）；IME 属偶发问题，采取防御性修复 + `RVIFM_IME_TRACE` 日志，不声称彻底消除。

### 2026-11-27 — M73–M76 对齐裁决

- 标签底色以主题 `TopLine` 派生再调暗（而非直接复用 `TabLineSel`），避免与路径行同色；失焦选中项用显式浅灰底替代 DIM/反显。
- frecency 对齐上游 `wideweide/frecency`：无查询按分数、书签不置顶；有查询 keyword 命中优先但必须真实匹配；选择器选中不记录，只有真实导航/打开才 record（本地 `record` 仍是唯一写入链）。
- `clean-tmp` 只清 frecency 存储里 `/tmp/` 前缀记录，不删本机临时文件（用户澄清后的口径）。
- `restore_entry` 改为直写 redb，避免只读 selector 看不到 u 的恢复结果；frecency 预览默认关闭以减轻卡顿，`w` 显式开启。

### 2026-11-27 — M71 块级丢弃边界

- 用 `git apply -R` 对单个 hunk 做反向应用（临时补丁文件 + 固定参数，不经 shell），比自行改文本更贴近 git 语义；仅作用工作区，未暂存内容。
- 丢弃前保存原文件字节到 `git_undo`，`u` 立即恢复；标注实验性，因为相邻 hunk 或已暂存内容混合时可能存在边界情况。
- 实现中发现并修复 `parse_unified` 只保留最后一个 `@@` 的 bug（多 hunk 文件此前会漏掉前面的差异块）。

### 2026-11-27 — M70 Git 写操作边界

- 写操作统一走 `git` CLI 逐参数执行（无 shell），成功后重建 diff session 以反映新状态，失败只提示不改视图。
- `a` 按当前 index 状态在 stage all / unstage all 间切换；`A` 明确为 `--amend --no-edit`，与 lazygit 一致；`d` 用 `git restore --worktree` 保留已暂存内容。
- `[git] confirm_destructive=true` 仅控制破坏性操作（discard/amend）的确认；提交/拉取/推送始终确认（比链接建议更保守）。pull/push 当前同步执行，可能阻塞 UI，记为已知限制。

### 2026-11-27 — M69 Git diff 技术取舍

- 沿用 M23-12「零编译期 git 依赖」路线：diff 由 `git diff --no-color -U3` 产出，`src/gitdiff.rs` 自解析 hunks；不引入 `similar`/`diffs-rs`/`gix`，避免新的 C/构建链风险。
- overlay 局部接管 lazygit 键位：Normal 全局 `A/p/P/=` 已占用，故所有写键只在 `:diff` 模式内解释；`A` 按 lazygit 语义为 Amend，不复用其全局 RenameAppend 含义。
- `J/K` 采用用户判断（单键优于 `]c/[c` 双键）跳 hunk；`j/k` 仍逐行。`=` 对照完整文件、`^w` 软换行默认开启，均对齐 ziggity。
- 只读边界：worker 只用 `git status`/`git diff`/`git show`，无 `--no-optional-locks` 之外的写参数；不替代 `gi`/`:gitui`。

### 2026-11-27 — M66–M68 向导安全/沙箱/审计取舍

- 安全提示选择改写 `tutor_meta` 现有 3 项而非新增：保持 102 项与境界阈值稳定，进度按稳定 ID 自动兼容；三遍提示分别落在不同条目，避免单条过长。
- 沙箱使用 `std::env::temp_dir()/rvifm-tutor`（Windows 自动适配），不是字面 `/tmp/tutor`；进入向导时重建目录并把左右栏定位到 `left/`、`right/`。
- 沙箱内操作按路径前缀排除 frecency 记账（用户确认），但保留撤销栈以便练 `u`；会话恢复遇已清理路径时重建。
- README 的 vifm 表述已核实：`:view` 内建读取文本/目录，PDF/图片/Office 才靠 `fileviewer` + 外部工具；「无内建能力」为错误陈述。harness 规则 9 固化该事实与 14/102 数字。

### 2026-11-27 — M65 CI 失败根因与处理

- Windows `test` 失败：`src/app/tests.rs` 的 `m482` 未 gate 就 `use std::os::unix::fs::PermissionsExt` / `Permissions::from_mode`，Windows std 无该模块 → E0433/E0599。已加 `#[cfg(unix)]`，CI 不再编译 POSIX 专测。
- macOS `test` 4 项失败均为长临时路径把标签/过滤标记/文件名挤出固定列宽，属平台相关断言而非功能缺陷；改为只断言平台无关标记（可见标签、过滤标记、快速标记）。
- 境界 0 级改名「入门」：用户认为能打开软件即算入门；符号与阈值不变。

### 2026-11-27 — M64 修仙境界取舍

- 采用网文通用九境 + 入门起点 + 真仙终点，共 11 级；符号选单宽纯文本 Unicode（避免 emoji 彩色/宽度漂移），境界中文名直接显示在进度栏。
- 筑基不按百分比硬切：用户确认必须完成 `core_nav` 核心导航模块；其余境界按 102 项掌握率阈值 5/15/30/45/60/75/90/100 推进；100% 且有错题记录封顶渡劫，零错题才真仙（沿用 ⭐ 徽章语义）。
- 四小阶段按当前百分比区间的四分位推导，纯展示、不落盘。`tutor_meta` 置顶使首次默认光标 0 即进入「向导模式用法」，先学使用方法。

### 2026-11-27 — M63 PDF 渲染与 deferred 记录

- 现有 PDF 文本提取**只取第 1 页**（`pdf.rs::PREVIEW_PAGES = 1`），视觉默认第 1 页（`pdf_image_pages = 1`，上限 2），且文本先行；已符合「默认最快、只提前几页」。
- 新增 `pdf-rust`（`karet-pdf`）为**非默认 feature**：文本仍先行，视觉路径在该 feature 下优先纯 Rust 栅格化（无 Poppler/MuPDF），失败再落 exprim/外部工具；默认构建依赖树不含 karet-pdf/hayro。
- 二进制已有 hex dump（M39-1）；「文件头语义注释」价值密度低，登记 deferred，不纳入 M63。

### 2026-11-26 — M61 大文件视口虚拟化取舍

- 用户确认阈值 1MiB（非链接建议的 10MB），自建 `memmap2` 视口读取器而不引 `vix-large-file`/`qem`：后两者采用度极低（82 / 282 下载），自建可控且与现有 generation worker 同构。
- mmap 只映射页面、稀疏检查点每 1024 行记录一次字节偏移，定位视口最多扫描一个 stride；不写文件、不写 redb。
- 高亮跨帧复用 syntect 状态；快速跳转时未经过的行不推进状态，可能颜色偏差——按用户确认「流畅性优先」接受。
- 远端 Location 的 `preview_path()` 本就返回 None，因此远端大文件天然只走元数据，不下载、不写回。

### 2026-11-26 — M60 异步预览取舍

- viewer 目标稳定后由 `AppState::refresh_viewer_async` 投递后台线程，载荷为 `ViewerPayload`（文本行 / hex 判定 / 完整行数）；事件循环 `drain_preview_results` 只接受 `preview_generation` 匹配的结果，快速 j/k 的中间目标不会安装到 viewer。
- 文本读取与 hex 判定已移入 worker；**每帧 syntax 着色仍留在 UI 线程**（SIMPLIFIED：Highlighter 需逐帧按视口起点推进，跨行状态复杂），缓存命中已在 M59-6 改为按需克隆，小文件卡顿的主要来源已消除。
- 用户确认项 4：默认做一次原生协议查询（tmux/screen 下也查），由 ratatui-image 内置 2s 超时控制，失败回退 halfblocks；`RVIFM_IMAGE_PROTOCOL_QUERY=0/false/off` 显式跳过。协议探测本身仍在启动时同步执行（未放入 worker），图片**解码**已后台化。
- 图片解码（M60-3）与归档/Office/PDF**文本**预览行（M60-4）均走同一 generation worker；PDF **视觉页**渲染因 `ratatui-image` 协议编码需要 `Frame`，仍留在 UI 线程（SIMPLIFIED，已在 ACCEPTANCE 标注）。

### 2026-11-26 — M59-9 r/u 语义与写入红线

- `r` 不再调用 `remove_entry`（整条 tombstone）——改为既有 `set_keyword_topic_now(path,topic,"")` 清空 keyword，保留条目、分数与 last_visit，不违反 `record` 是评分唯一写入口的红线（keyword 事件本就是 marks/bookmark 的既有写入路径）。
- `r` 采用面板内两键确认（r→y，其它键取消），避免复用 `InputMode::Confirm` 误触文件删除。
- `u` 优先恢复 `last_keyword` 中的原 keyword（连条目分数保留）；`dd` 的整条恢复仍走 `restore_entry`。

### 2026-11-26 — M58 命令补全边界翻转与 M59/M60 拆分确认

- 用户确认推翻 M50「只读取命令名、不读参数/路径」的隐私边界：shell 历史**整行**作为补全候选并写入 frecency `cmd` topic；SPEC/GUIDE/help/源码注释已同步改写。
- 用户确认 `rvifm cmd` 为独立只读入口（fzf Ctrl+R 同款），复用 `pick_interactive`，不写 redb、不推进 tick。
- 用户确认 M59/M60 拆分登记：`r` 只清 bookmark/mark 保留条目与分数、`u` 连 keyword 恢复；frecency `Tab` 固定进/出预览；图片协议允许默认后台查询。

### 2026-11-26 — M56-10 向导条目 ↔ ACCEPTANCE 映射与同步规则

> 机制：向导每个模块以稳定 `module_id` 对应一组 ACCEPTANCE 验收条目；条目以稳定 `item_id`
> 对应具体验收点。ACCEPTANCE 变更时必须回查本表：若验收项被删除/改名，先更新 `assets/tutor/<id>.toml`
> 的 `[[items]]`（保留或新增稳定 id，进度文件按 id 自动忽略未知项），再勾选该批次的 M56-10 同步项。
> `scripts/harness_check.py` 规则 8 校验下表覆盖全部模块且引用的 ACCEPTANCE 章节存在。

<!-- TUTOR-ACCEPTANCE-MAP:START -->
| module_id | 模块 | 对应 ACCEPTANCE 条目 | 备注 |
|---|---|---|---|
| core_nav | 核心导航 | §3, §18 | 浏览/h/j/k/gg/G、`'` 快速跳转 |
| file_ops | 文件操作 | §4, §5 | 复制/移动/删除/撤销与批量 |
| visual_tag | 选择与批量 | §5 | `v`/`t`/`yy`/`p` 多选链路 |
| search_filter | 搜索与过滤 | §6, §19 | `/`、`=`、`f/F`、`:find` |
| search_frecency | 检索与记忆 | §10, §19, §61 | `O` 实时递归、`:j`/`:J`、`pick` |
| marks_frecency | marks 与 frecency | §10, §18, §22 | `m<char>`/`'<char>`/keyword |
| panes_sort | 视图与排序 | §3, §20 | 双栏、`S` 排序、对比 |
| tabs_history | 标签与历史 | §3, §18 | `gt`/`gT`/`N gt`、历史前进后退 |
| command_line | 命令行 | §7, §55 | `:` 命令与补全（M50） |
| clipboard_git | 剪贴板与 git | §4, §5 | `y*`/`gi` 与系统剪贴板 |
| archive_ops | 归档操作 | §9 | 只读 VFS、创建/解压 |
| remote_sftp | 远端 SFTP | §21 | 远端浏览/上传下载 |
| maintenance | 运维操作 | §10, §38 | frecency compaction、shell 集成 |
| tutor_meta | 向导模式用法 | §42, §43, §62, §71, §72, §73 | 向导自身（M56/M64/M66–M68：安全提醒、境界、沙箱） |
<!-- TUTOR-ACCEPTANCE-MAP:END -->

### 2026-11-26 — M55 实时递归模式（`O`）实现决策

- `O` 是**独立实时模式**（`InputMode::LiveFind` + `AppState.live_find`），不复用 `:find` 的静态结果虚拟目录；`:find`/`:grep` 语义保持不变。
- 扫描后端复用 `ignore` walker（与 `:find` 同源：gitignore/.ignore + 隐藏文件按 `show_hidden`），后台线程逐条 `mpsc::send`，深度 20 / 条目 5 万护栏；只收文件，不写 redb、不更新 `global_tick`。
- 过滤只做大小写不敏感子串（多词 AND），不做模糊评分；query 变更全量重建命中下标，新条目到达只追加命中项（增量）。
- 子串匹配针对**相对扫描根的路径**（`strip_prefix(root)`），避免根目录名（如 tempfile 的随机 `.tmpXXXX`）命中 query 造成误匹配/测试偶发失败。
- `j/k` 在实时模式是**输入字符**（与 frecency Search 态一致），移动用 ↑↓/Ctrl+J/K——UI 通过边框输入框 + `j/k 为输入` 提示避免误用。
- Enter 复用 `jump_to`（定位父目录并选中文件）+ `record_live_visit` 走 record 唯一写入入口；Esc/退出置 `stop` 标志并丢弃 receiver，线程自然停止。
- M55-2（后端）与 M55-3（交互）共享同一状态机且相互编译依赖，为保持树可编译在同一提交落地；TASKS/ACCEPTANCE 仍分别勾选。

### 2026-11-26 — M57 帮助入口收敛决策

- 用户报告 `rv help` 外部子命令启动的查看器按 `Esc` 不退出、反而显示 rv 双栏，决定**移除该外部入口**，帮助只在应用内 `:help`（不透明覆盖层 + `/` 搜索 + Esc/q 返回）执行。
- `[ui] help_in_new_window` 配置键保留以兼容旧配置文件（避免解析失败），但 `:help` 不再依据它新开窗口——该键成为惰性兼容项，已在 help/CHANGELOG/config 默认模板标注。
- 历史 CHANGELOG 条目（M46-7/M48-12）如实保留，不重写已发布版本记录；新增 [Unreleased] M57 段落说明当前行为。

### 2026-11-26 — M54 z 系列取舍理由

- 内置 `za/zo/zm`（隐藏文件切换）与 `zR/zM`（过滤器临时重置/恢复）是文件管理高频刚需，且共享既有 `show_hidden` / `filter` 状态源，成本可控。
- 内置 `zt/zz/zb`（视口定位）为低成本定位增强，只改 `scroll` 不动光标索引，`[ui] scrolloff` 默认 0 保持既有行为。
- `zO` 清空焦点栏永久过滤器（等价于清空 `=` 过滤），不动隐藏开关；`zR` 快照仅会话内，不写配置。
- `[z`/`]z`/`zj`/`zk`（兄弟目录跳转）与现有 j/k 线性导航重叠且当前无对应 Action，本轮不内置，仅在 GUIDE 给自定义键位示例。

### 2026-11-26 — M54 ZKEY-1 声明 vs 实现排查

| 键/配置 | GUIDE/help | Keymap 默认表 | Action | 判定 | 处理 |
|---|---|---|---|---|---|
| `za` / `zo` / `zm` | 无声明 | 无 | 无 | 未实现（非文档错误） | ZKEY-2 新增 |
| `zR` / `zM` / `zO` | 无声明 | 无 | 无 | 未实现 | ZKEY-3 新增 |
| `zt` / `zz` / `zb` | 无声明（用户反馈称已列出，但当前树未找到） | 无 | 无 | 未实现 | ZKEY-4 新增 |
| `[z` / `]z` / `zj` / `zk` | 无声明 | 无 | 无 | 按范围决策不内置 | ZKEY-5 仅给自定义示例 |
| `[ui] scrolloff` | 无声明 | — | — | 配置缺失 | ZKEY-4 新增（默认 0） |

结论：当前树**没有**「GUIDE/help 已声明 zt 但按下无反应」的项——用户记忆中的提示来自旧版本或其它工具。ZKEY-1 按要求优先修 z 系列；非 z 系列的全量键位/文档一致性扫描成本高，登记为后续任务（不纳入 M54）。

- **一致性**：M52-3 全量扫 harness 文档（SPEC/TASKS/ACCEPTANCE/STATE/PLAN/NOTES）与项目文档（GUIDE/README/CHANGELOG/help/tutor）及代码测试，消除 M49 遗留的「默认固定右侧」冲突；M23-6/M34-4/M44-1 旧验收条目（对侧预览）与新口径恢复一致。
### 2026-11-26 — M53 蒸馏决策（用户确认）

- 用户确认两点：① `r` = 删除当前光标条目的 frecency bookmark/mark 记录（不是删文件）；② 归档成员 `l`/Enter 允许系统 opener 打开**临时只读副本**（不写归档内）。
- 其余按需求原文：标签 `[N:焦点栏全路径]`、`:tabname` 与右栏活动项持久化、归档 `w` 预览、36 行文本预览与卡顿排查、foot 原生图片协议修复、`.deb` 误提示消除、真异步/防抖预览。
- 沿用不 bump / tag / Release；历史批次条目保留。

- **M52-3 附加稳定性修复**：并行测试偶发 `Database already open`（`nav4_recent_exec_opens_selector`）源于 redb 读写句柄争锁；`open_store_readonly_at` 增加 10×20ms 有限重试（生产双开 rvifm 同样受益），连续 4 次全量 `cargo test` 全绿。
- **M52-2 右栏审计结论**：`dispatch_right_pane` 投影 8 个状态字段后复用共享 `handle_key`，Normal 键位（j/k/gg/G、v/t、yy/p/P、o 等）左右一致；冒号命令在 `exec_command_impl` 与 `focused_*` 访问器下按焦点栏分派（`:sort`/`:select`/`:rename`/`:move`/`:copy`/`:archive`/`:extract`/`:file`/`gi` 均已焦点化）。预览焦点 `Tab` 进出与侧别无关，新增 m521 断言「焦点右 + 查看器在左 + Tab 进出预览且焦点不跳栏」。剩余 `tab.location`/`tab.entries[tab.cursor]` 直读仅在**右键上下文菜单**（M41-3 明确固定左栏，非「仅导航」限制）与 `jump_to` 左分支（焦点左时正确）——无残留「右栏仅导航」限制。
