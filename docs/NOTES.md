# NOTES — 决策记录

本文件记录“为什么这么定、评估过什么、放弃了什么”。不记录当前进度（看 [STATE.md](STATE.md)），不定义功能边界（看 [SPEC.md](SPEC.md)）。

- **来源**：DeepSeek 分享会话 `https://chat.deepseek.com/share/2dpnwrtmiwm1pr21k2`，2026-11-10 蒸馏。
- **原则**：只保留会影响实现和边界的结论；聊天过程、客套、重复举例不进入本文档。

## 1. 背景与可行性判断

- vifm 约 **17 万行 C**，开发约 **17 年**，依赖 ncurses、GLib、libmagic。
- 完整、忠实地重写 vifm 预计需要 **12–18 个月**、**1.5 亿–4 亿 token**，投入产出比不理想。
- 因此放弃“全量移植”，改为 **以 vifm 官方文档/wiki 的特性清单为规格来源，允许简化和重新设计**。
- 项目采用 **harness 模式**：先把需求、决策、任务、验收固化成文档，再让编码 Agent 每轮只执行一个小任务。
- 预算规划最终收敛到 **约 7500 万–1.1 亿 token**，优先保证 M0–M2 闭环。

## 2. 产品定位与命名

- 名称：`rvifm`；crate / binary 同名。
- 定位：**Rust 编写的 vim 风格 TUI 文件管理器；参考 vifm，非 fork，非行为级兼容**。
- 不逐条对齐 vifm man page；允许简化、重新设计和不兼容。
- 可选短别名 `rv`，但项目名仍用 `rvifm`；是否提供别名待定。
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

- **marks 不实现**：`m` + 字母标记路径、`'` + 字母跳转全部移除。
- 路径跳转由 frecency 的 `pick` 覆盖。
- **tag 保留**：`t` 标记文件用于 M3 批量操作，与 mark 语义不同。
- 文档与代码中必须避免把 tag 叫成 mark，防止后续 Agent 误实现 marks。

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

- compaction 由 `pick` / TUI 启动惰性触发（建议 events > 5000 条），或 `rvifm maintenance compact` 手动执行；无定时器、无守护进程。
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

- **compaction 阈值**：建议默认 5000 条事件，具体值与可配置方式在 M8-5 定。
- **M7 压缩 tar 变体排期**：`.tar.xz` / `.tar.bz2` / `.tar.zst` 可作为 M7 第二阶段。
- **主题备选**：`opaline` 不满足时是否切 `tui-nuances`。
- **键位库降级**：`keymap-rs` 不满足时用 `keybindings` crate 还是自写 Trie。
- 【已决策 2026-11-13】keymap-rs 评估（M10-2）：聚合 crate 不存在；keymap-core/seq/config 均为 0.1.2（下载 500–700，单维护者，MSRV 1.85）。采纳降级路线：自写轻量 Trie 解析层（M10-1 的 Keymap 已具备前缀/精确匹配能力），TOML 分层在 M10-5 自实现。理由：低采用量 crate 的供应链风险高于自写 ~40 行解析器的维护成本。
- **性能阈值**：大目录“不阻塞退出”的量化标准，M6-4 补充。
- **短别名**：是否提供 `rv` 二进制别名。

## 9. 文档蒸馏规则

后续从讨论中提炼结论时，按信息类型投递，避免重复：

| 类型 | 去向 |
|---|---|
| REQ：要做什么 | `SPEC.md` 的 MUST / SIMPLIFIED |
| CUT：明确不做 | `SPEC.md` 的 OUT_OF_SCOPE + 本文档“已评估但放弃” |
| WHY：为什么、评估过什么 | 本文档 |
| SEQ：顺序与依赖 | `PLAN.md` + `TASKS.md` |
| HOW：怎么验收 | `ACCEPTANCE.md` |
| NOISE：过程、客套、重复举例 | 丢弃 |

判断标准：删掉这句话后，执行者会做错事或不知道边界，它就不是 NOISE。

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
