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

## 8. 待定事项

执行到对应任务时必须先决策，并更新本文档与 [SPEC.md](SPEC.md) 的待定列表：

- **compaction 阈值**：建议默认 5000 条事件，具体值与可配置方式在 M8-5 定。
- **M7 压缩 tar 变体排期**：`.tar.xz` / `.tar.bz2` / `.tar.zst` 可作为 M7 第二阶段。
- **主题备选**：`opaline` 不满足时是否切 `tui-nuances`。
- **键位库降级**：`keymap-rs` 不满足时用 `keybindings` crate 还是自写 Trie。
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
