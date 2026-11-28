# SPEC — rvifm 需求规格

本文件是 `rvifm` 的唯一需求来源。所有“做什么、做到什么程度”的结论都在这里；讨论过程与取舍理由见 [NOTES.md](NOTES.md)。

- 项目定位：Rust 编写的 vim 风格 TUI 文件管理器，参考 vifm 交互范式，**非 fork，非行为级兼容**。
- 规格来源：vifm 官方文档/wiki 的特性清单 + 本项目已拍板的简化与重设计。
- 范围分级：**MUST**（必须）、**SIMPLIFIED**（允许简化）、**OUT_OF_SCOPE**（明确不做）。
- 执行顺序以 [TASKS.md](TASKS.md) 为准；验收标准见 [ACCEPTANCE.md](ACCEPTANCE.md)。

## 1. 术语

| 术语 | 含义 |
|---|---|
| Physical | 真实文件系统路径 |
| Archive | 归档文件内部的虚拟路径，形如 `host:/inner/path` |
| Tab | 一个独立的浏览上下文，持有自己的 `Location`、光标、滚动、搜索状态 |
| tag | `t` 键标记文件，用于批量操作；**不是** marks |
| mark | `m` + 字母标记路径、`'` + 字母跳转；由 frecency 实现并持久化（M25-2），与 bookmark 共用数据 |
| Action | 文件操作语义的原子动作，如 `Yank`、`PutMove`、`PutCopy` |
| UndoableOp | 可撤销操作的记录，带所属 tab 与路径 |
| frecency | 基于访问频率与时间衰减的排序机制 |
| pick | 只读模糊选择器子命令 |
| record | 唯一写入访问事件的子命令 |
| PreviewRenderer | 文件管理器与 pick 共用的预览渲染抽象 |
| opener | 文件打开规则与命令配置 |

## 2. 总原则

1. **零配置可用**：不写配置文件也能完成日常浏览、打开、预览、归档浏览、撤销。
2. **定制是加分项**：所有可定制行为遵循同一套分层模型：默认可用 → `prepend` 覆盖 → `append` 补充 → `replace` 完全替换。
3. **核心先闭环**：M0–M2 必须可用，尤其是 `u` 撤销；后续里程碑是增强，不阻塞 v0.1.0。
4. **不追求行为等价**：不逐条复刻 vifm man page；允许简化、重新设计和不兼容。
5. **内建优先**：能在编译期解决的能力，不推给插件、外部脚本或内核模块。
6. **只读边界清晰**：归档内只读；frecency 的 `pick` 只读；写入入口唯一。

## 3. 全局范围分级

### 3.1 全局 MUST

- Linux / Windows 优先；macOS 尽力支持。
- 目录浏览、多标签、文件操作、撤销、选择与批量操作、搜索、配置、帮助。
- 归档浏览不依赖 FUSE 或外部挂载工具。
- 图标与主题内建，不依赖 vifm 插件。
- 图片预览在无原生图形协议的终端上仍有 halfblocks 回退。
- 文件打开零配置可用，且支持 TOML 分层定制。
- frecency 不依赖 fzf、SQLite 或后台守护进程。
- CI 在 M0 起覆盖 Linux + Windows。

### 3.2 全局 SIMPLIFIED

- 配置使用 TOML，不支持 Vim 脚本。
- 命令模式只实现最小 `:` 子集。
- 归档首批支持 `.zip` / `.tar` / `.tar.gz`；压缩 tar 变体随后支持。
- Office 预览只做文本/Markdown 提取，不保证格式还原。
- PDF 预览优先文本提取；视觉预览走可选纯 Rust 后端（`pdf-visual` feature）或可选外部工具。
- `.rar` 不提供虚拟浏览，提示用户解压；`.7z` 虚拟浏览已由 M26-3 交付（加密/损坏可读错误）。
- 主题数量以 `opaline` 内置为准；主题数量优先时可换 `tui-nuances`。
- 键位解析优先复用 `keymap-rs` 系列；不达标时降级为轻量自写 Trie。

### 3.3 全局 OUT_OF_SCOPE

- 与 vifm 的行为级兼容、man page 逐条等价。
- Vim 脚本引擎、插件系统、用户自定义 Action 实现。
- 完整 Vim 引擎（`narwhal-vim` / `vimcore` / `hjkl-engine`）。
- FUSE、`archivemount`、fzf、SQLite、Überzug++、Chafa 作为必需依赖。
- 写入、删除、重命名、移动归档内成员。
- 嵌套归档（归档内的归档）递归浏览。
- 内嵌视频播放。
- PDF 渲染引擎作为默认/必需依赖（`tdf` / `pdfium-render` 完全禁止；`exprim` 仅允许以非默认 `pdf-visual` feature 引入）。
- 保证 PDF 的零配置视觉预览（默认构建只承诺文本提取 + 可选外部工具回退）。
- 内嵌 SVG 或字体文件；Nerd Font 仍由终端提供。

## 4. 全局行为契约

### 4.1 导航与路径

- 支持双栏与单栏浏览；进入/返回目录、刷新。
- 内部路径统一用 `/`；归档内部路径必须规范化后再渲染、比较、索引。
- 归档位置用显式模型表示，禁止把 `/home/a.zip/src/main.rs` 误判为真实路径：

```rust
enum Location {
    Physical(PathBuf),
    Archive { host: PathBuf, inner: PathBuf },
}
```

- 状态栏/面包屑对归档位置显示为 `host:/inner/path`。
- 归档根目录按 `h` 返回归档文件所在的物理目录。

### 4.2 多标签与状态

- 每个 tab 独立持有 `Location`、光标位置、滚动位置、搜索状态、前进/后退历史。
- 多标签是 M1 的核心浏览能力，不得推迟到后期增强。
- 撤销栈全局共享，`UndoableOp` 记录所属 tab 与路径；跨 tab 撤销能正确定位。

```rust
struct Tab {
    location: Location,
    cursor: usize,
    scroll: usize,
    search: Option<SearchState>,
    history: Vec<Location>,
}

struct AppState {
    tabs: Vec<Tab>,
    active: usize,
    undo_stack: UndoStack, // 全局，操作记录带 tab 标识
}
```

### 4.3 撤销

- `u` 按操作逆序撤销。
- 至少覆盖：删除、重命名、移动、新建目录/文件。
- 删除优先走可恢复路径（回收站或应用内暂存），保证 `u` 可逆。
- 不可逆操作入栈时标记为不可撤销，并给出明确提示。
- 连续多次操作后，`u` 按逆序逐条撤销。
- 跨 tab 撤销：操作发生在非当前 tab 时，撤销能正确定位；必要时自动切回该 tab 或提示。
- 重做 `Ctrl-r` 为可选能力（SIMPLIFIED），不阻塞 M2 验收。
- 归档浏览只读，不进入撤销栈。

### 4.4 搜索、过滤与跳转

- `/` 搜索当前视图，`n/N` 循环跳转。
- 简单 glob 过滤。
- 路径跳转由 frecency 的 `pick` 覆盖；M25-2 marks 由 frecency 实现并持久化（见 §25），与 quickjump 共用 keyword 链路。
- 归档内搜索复用同一套搜索链路。

### 4.5 归档浏览

- 只读虚拟文件系统，不依赖 FUSE / `archivemount`。
- 首批格式：`.zip`、`.tar`、`.tar.gz`。
- 第二阶段：`.tar.xz`、`.tar.bz2`、`.tar.zst`，通过解码层包一层实现。
- `.rar` 不支持虚拟浏览，提示用户解压后浏览；`.7z` 已升级为虚拟浏览（M26-3，加密/损坏可读错误）。
- 打开归档时建立成员索引；读取时按需解码请求的成员，内存占用不随整个归档体积线性增长。
- 归档内导航按键与物理目录一致：`j/k/h/l`、`gg/G`、`/` 搜索、`n/N`。
- 归档内不提供删除、重命名、移动入口（或明确禁用）。
- 错误处理：损坏归档、加密归档、空归档、超大归档索引均不得 panic，给出可理解提示。

抽象接口：

```rust
trait ArchiveFs {
    fn list(&self, inner: &Path) -> Result<Vec<Entry>>;
    fn read(&self, inner: &Path) -> Result<Box<dyn Read>>;
    fn metadata(&self, inner: &Path) -> Result<EntryMeta>;
}
```

`ZipFs` 与 `TarFs` 各自实现该 trait；导航层通过 `Location` 分派，不得在导航层直接塞 `zip`/`tar` 分支。

### 4.6 frecency 选择器

#### 子命令与契约

```text
rvifm           → 文件管理器 TUI
rvifm pick      → 只读模糊选择器（替代 fzf 的角色）
rvifm record    → 静默记录一次访问事件
```

- `pick` 的 stdin/stdout 契约与 fzf 同构：
  - stdin：候选列表，每行一个；
  - stdout：选中的一行；
  - 退出码：0 选中 / 1 取消。
- `pick` **只读**：不写 redb、不更新任何时钟（`global_tick`/`dir_tick`/`file_tick`）、选中后不追加事件。
- `record` 是**唯一写入入口**；只在 shell 真正 `cd` 成功、或文件管理器真正跳转/打开成功后调用。
- 典型用法：

```bash
selected=$(rvifm pick --topic dir < <(fd -t d))
rvifm record --topic dir "$PWD"
```

#### 存储模型

redb 三表，无后台守护进程：

| 表 | 内容 | 写入方 | 读取方 |
|---|---|---|---|
| `events` | append-only 事件：`{path, topic, kind, ts, clock}`，key 为全局单调序号 | `record`、TUI 退出时批量追加 | `pick` / TUI 启动时读增量 |
| `snapshot` | compaction 产物：key = `path+topic`，value = `{content, keyword, score, tick, clock}` | compaction | `pick` / TUI 启动时读 |
| `meta` | `global_tick`（全局事件序号）、`dir_tick` / `file_tick`（类型时钟，2026-11-14 增补）、`last_compaction` | `record`、compaction | 所有读方 |

- `record` 追加事件、推进 `global_tick` 并推进**对应类型时钟**（dir → `dir_tick`，file → `file_tick`），**不做评分**。

**独立事件时钟（2026-11-14 增补）**：目录（`topic = dir`）与文件（`topic = file`）事件分别推进 `dir_tick` / `file_tick`，事件 JSON 携带其类型时钟值 `clock`；EMS aging 的 Δtick 一律取**同类型时钟差**——目录高频访问不得稀释文件条目的新鲜度，反之亦然。`global_tick` 退化为全局事件序号：仅作 events 表 key 与 compaction 增量游标，不参与 aging。兼容：旧库缺 `clock` 字段时以全局序号兜底（旧语义等价）；`dir_tick`/`file_tick` 缺失时以 `global_tick` 当前值初始化（新旧 aging 连续分叉）。
- `pick` / TUI 启动时读取 `snapshot`，以 `meta.last_compaction` 为增量起点读取 `tick > last_compaction` 的事件，在内存中重放得到当前评分；`snapshot` 中每条记录的 `tick` 用于 aging。
- TUI 运行中所有跳转先在内存中更新评分，不每次写盘；退出时把本次会话访问批量追加到 `events`。
- compaction 在 `events` 超过阈值（默认 5000 条，2026-11-19 定稿）时由 **TUI 启动**惰性触发，或由 `rvifm maintenance compact` 手动执行；写 `snapshot` 后截断已合并事件。惰性触发由 AUDIT-4 实现。**不在 `pick` 路径触发**——`pick` 只读，不得写 snapshot/截断 events（见 AUDIT-1）。
- 无定时器、无守护进程。

#### 评分与分级

- EMS 评分 + event clock。
- 读时统一 aging：存储分数只在其最后一次访问的类型时钟值上有效，查询时按 `score * exp(-λ * (clock_now - clock))` 对齐后再比较；`clock_now` 取该条目 topic 对应的类型时钟（dir → `dir_tick`，file → `file_tick`）。
- 分级匹配（0–7 档，具体档位在 M8-1 固化）：书签 > 文件名 > 中间段 > 通用子串；在 Rust 内存中排序。
- 书签：`keyword != ''` 作为分级标志，书签自身也参与 EMS 评分；取消书签后保留累积分数正常衰减。

#### M17 — 递归搜索与查看器批次（2026-11-15 增补；用户设计：结果=虚拟目录 + 交互式查看器）

**背景**：vifm 的 :find/:grep 是外部工具封装（findprg/grepprg），结果进一次性菜单；社区靠 fzf 管道弥补。rvifm 用 ripgrep 拆出的 `ignore` crate（并行遍历 + gitignore 感知）+ `regex` 原生实现，且结果以**虚拟目录**形态进入既有导航体系——比"一次性 picker"更符合工作流（用户拍板的设计修订，替代 frecency picker 方案）。

**MUST**

- **SRCH-1 `:find [pattern]`**：
  - `ignore` crate 递归遍历当前 tab 物理目录；尊重 .gitignore/.ignore；隐藏文件开关跟随 show_hidden 配置；结果上限 1 万（截断时状态栏提示）。
  - 结果以**虚拟目录**呈现（沿 :tree 同构：`Tab.results`，items/entries 平行表）——条目显示**全路径**，既有导航操作（j/k/gg/G/Ctrl-u/d/yy/Ctrl-G/w）与对侧预览跟随即目录语义。
  - `l`/Enter：定位到命中文件所在目录 + 光标停在该文件（退出结果视图）；Esc/h：退出回到原目录。
  - 只读守卫：写操作（dd/DD/cw/rename/copy/move/t/v）在结果视图内拒绝；`:find`/`:grep` 重执行 = 替换结果。
- **SRCH-2 `:grep [pattern]`**：
  - `ignore` 遍历 + `regex` 逐行匹配；条目显示 `路径:行号: 内容`；二进制（含 NUL）/非 UTF-8 文件跳过。
  - regex 非法表达式返回可读错误（不 panic）；其余语义与 SRCH-1 一致。
- **VIEW-1 预览查看器**：
  - `w` 在结果视图（或任意文件上）激活交互式查看器（右 pane）：**行号** + j/k/gg/G/Ctrl-u/d 滚动 + `/` 搜索（定位第一个匹配 + 匹配子串高亮）+ `n/N` 循环 + q/Esc/w 退出。
  - `:grep` 命中打开查看器时定位到命中行。
  - 查看器状态由 tab 持有；光标移动/重进重置。

**SIMPLIFIED**

- 遍历只收文件（目录不进结果）；匹配为大小写不敏感子串（:find）/正则（:grep）；非 UTF-8 文件跳过。
- 查看器不做语法高亮（tree-sitter 另立任务）；匹配高亮复用 FIX-1 的 match_range。
- 会话结果不缓存（重执行即重新遍历）；无后台化（M13-8 基础设施后续可接）。

**OUT_OF_SCOPE**

- 外部 find/grep 二进制封装（findprg/grepprg 路线）。
- 归档内递归搜索（M7 只读 VFS 边界）；树视图/右 pane 结果模式。
- 搜索替换（amber 路线）；外部搜索 TUI 工具集成。
- opener 行号插值（"打开并跳行"）；语法高亮。

#### M21 — UI/UX 打磨批次（2026-11-17 增补；缺陷修复×9 + 体验增强×8）

**硬约束**：界面调整只做视觉/布局，不改键位语义与文件操作行为；目录 tree 预览只读不导航；`w` 激活后焦点留在左 pane，j/k/gg/G 右 pane 跟随；重命名键位 `I`/`cc`/`A`/`R`，`R`=`:rename` 走 $EDITOR 多选列表；跳转选择器不隐藏双 pane 列表、默认搜索模式；不引入新 GUI 依赖（语法高亮复用 tree-sitter/syntect 已有路径）；不新建事件总线——record 调用点直接落在既有"目录切换/文件打开成功"路径。

**缺陷修复（MUST）**
- **M21-F1 界面风格**：左右 pane 各带边框（焦点态高亮）；居中分开；pane 顶部显示各自路径，状态栏不再显示路径；按文件类型着色（目录/可执行/符号链接/归档/图片/视频/代码，参考 yazi filetype + vifm 配色语义）；选中文件时底部显示详情（大小/权限/时间/类型）。
- **M21-F2 焦点视觉**：状态栏不显示"当前焦点 pane"文字；焦点 pane 当前行正常高亮，非焦点侧当前行 DIM 灰化（vifm TopLineSel/TopLine 语义）。
- **M21-F3 搜索 Esc 清理**：Esc 退出搜索后状态栏与列表高亮全部清除；再次 `/` 干净进入，不残留上次匹配。
- **M21-F4 `h` 返回定位**：从 B 返回 A 时光标落在 B 条目；归档内 `h` 返回物理目录时光标落在归档文件上。
- **M21-F5 外部变更自动刷新**：`notify` crate 监听当前物理目录（创建/删除/重命名/修改）→ 自动刷新并保持光标；归档视图不监听；远端不在本轮；监听失败静默降级。
- **M21-F6 目录 tree 预览**：光标在目录上时右 pane 浅层 tree（深度 1–2，条目数上限）；走 tree 渲染路径不走文本读取（不报"无法读取/非 UTF-8"）。
- **M21-F7 PDF/Office 预览分流**：核实预览分流——pdf 走文本提取+可选视觉、doc/docx 走 office_oxide，均不得落"文本文件读取"通用路径；失败提示按类型（"PDF 文本提取失败"/"Office 文档解析失败"）。
- **M21-F8 opener stderr 抑制**：`system_open` 不把 GUI 程序 stderr 漏到终端（重定向 Stdio::null 或等效）；失败错误进状态栏。
- **M21-F9 进入目录 record**：TUI 中 `l`/`Enter` 成功进入目录后调用 `record`；失败/取消不记录；与 shell hook 语义一致不重复。

**体验增强（MUST）**
- **M21-E1 列表属性列**：大小列（目录 `<DIR>`）+ 修改时间列（`yyyy-mm-dd HH:mm:ss`）；列宽自适应，窄终端优先文件名；配置开关（默认布局 + 显示/隐藏）。
- **M21-E2 `w` 预览模式验证补缺**：现状 view_mode（M13-9/M14-1/VIEW-1）已实现"焦点不离开左 pane + j/k 跟随预览"——本任务为验证 + 补缺（无则修）。
- **M21-E3 语法高亮**：文本预览语法高亮（复用 tree-sitter/syntect）；`.txt` 内容推断（shebang/配置模式），失败降级纯文本；大文件截断提示。
- **M21-E4 灵活重命名**：`I`（光标在名首）、`cc`（清空）、`A`（光标在名尾）、`R`/`:rename`（$EDITOR 多选列表，批量应用）；批量入撤销栈整批 `u` 可逆（`R` 复用 M13-6 cw 管线）。
- **M21-E5 `:mkdir` 定位 / `:mk` 进入**：`:mkdir <名>` 后光标定位新目录；`:mk <名>` 创建并进入；失败不改光标。
- **M21-E6 归档 `.tar.xz`/`.tar.bz2` 验证**：现状已实现（M7-8 五压缩形态）——本任务为验证 + 补缺。
- **M21-E7 `=` 过滤增强**：实时过滤 + `*` 通配符；无通配符时隐式 `*模式*`（vifm 语义）；Esc 清除恢复。
- **M21-E8 跳转选择器增强**：区域 80%×80% 不隐藏双 pane；默认搜索模式，Esc→Normal 再 Esc 退出；Vim 操作（j/k/gg/G/Enter/l）；记录补全（目录：l/Enter/:cd/鼠标点击进入成功；文件：l/Enter/鼠标点击打开成功；失败/取消/预览不记录）；`:j`/`:J` topic 过滤复用（现状已有，验证）。

**SIMPLIFIED**
- M21-F5：若迭代超 5 轮缩到"仅当前目录刷新"；M21-F9/记录补全缩到"仅目录进入记录"。
- M21-E1：列顺序定制后续增强，本轮仅开关。
- M21-E8：记录补全不新建事件系统，调用点直接落既有成功路径。

**OUT_OF_SCOPE**
- 键位语义与文件操作行为变更（视觉/布局限定）。
- 归档视图监听外部变更；远端 VFS 自动刷新。
- 新外部 GUI 依赖；新事件总线/钩子系统。

#### M24 — 交互增强与列表规范（2026-11-19 增补；来源：DeepSeek 对标分析蒸馏 + 用户列对齐要求）

**硬约束**：列对齐使用固定列宽 + 填充（非终端 tab）；自然排序不引外部 crate（自写数字感知比较器）；可视模式操作与 M2/M3 撤销栈集成；命令历史仅内存（不持久化）；所有新键位经 keymap.rs Action 驱动。

**MUST**
- **M24-1 Pane 列对齐规范**：文件列表条目行——文件名**左对齐** + 空白填充至固定列宽；大小**右对齐** + 人性化展示（B/KiB/MiB/GiB，复用 format_size）；日期**左对齐** （分钟精度，format_mtime 截取）；三列间距 2 空格；窄终端隐藏属性列（复用 M21-E1 自适应）；左右 pane 共用。
- **M24-2 自然排序**：文件名含数字段时按数值排序（1.txt < 2.txt < 10.txt）；实现为 `natural_cmp(a, b) -> Ordering`（分段解析数字/非数字，数字段按 u64 值比较、非数字段按字典序）；受 Config.ignore_case_sort 影响；S 对话框增加 "natural" 字段。
- **M24-3 可视模式（2026-11-28 修订）**：仅 `v` 进入可视选择（选中整行/目录），j/k 扩展选区、d/y/x 复用 M3 批量管线、Esc 退出；原 `V` 与 `v` 同实现且无额外语义，按用户裁决移除（见 M86-7）。
- **M24-4 可视模式操作扩展**：可视模式（v/V）内 `d` 删除选中项（入撤销栈）、`y` 复制选中项路径（M23-10 yf 同语义）、`x` 删除（=d 别名）；状态栏显示选中项计数（复用 VISUAL 指示）。
- **M24-5 计数前缀扩展**：`Nj`/`Nk`/`NG`/`Ngg`/`Nh`/`Nl` 消耗 `InputState.count` 执行 N 次；`Ndd`/`Nyy` 对 N 个条目批量执行；现有 `N gt` 不变。
- **M24-6 命令历史**：`:` 模式按 ↑/↓ 浏览已执行命令（内存 Vec，会话内有效）；Enter 执行选中历史命令；Esc 退出；无持久化。
- **M24-7 路径补全**：`:` 模式按 Tab 补全当前输入词为文件/目录名（前缀匹配，循环切换候选）；`~` 展开为 HOME；补全候选来自当前 pane 目录列表（M21-E1 同源）。
- **M24-8 目录对比 :compare**：`:compare` 比较左右 pane 目录内容（按文件名+大小），结果复用 Tab.results 虚拟目录（条目前缀：`=` 相同、`!` 不同、`<` 仅左、`>` 仅右）；只读不入撤销栈；l/Enter 定位到实际路径。
- **M24-9 任务视图增强**：`:jobs` 覆盖层增交互——j/k 选择作业、x 取消运行中作业（通过 JobStatus 标记取消）、Esc 关闭；取消实现：Job 增 cancel_requested 标志，后台线程检查后终止。

**SIMPLIFIED**
- M24-2：自然排序支持小数（1.5.txt < 2.txt）；不支持负数/科学计数法。
- M24-3：V 模式选中即打 tag（复用 M3-2 tag 集合），不新建选择结构。
- M24-6：命令历史仅存本轮会话已成功执行的命令（失败不记）；上限 100 条。
- M24-7：补全仅前缀匹配（无模糊）；候选超过 10 个显示计数。

**OUT_OF_SCOPE**
- 块可视模式 Ctrl-v；命令别名（% 变量已有）；符号链接创建（deferred）；磁盘用量 :du（deferred）；过滤历史循环（= 已有实时过滤）。会话持久化 / 归档创建 / Git 状态列已于 2026-11-19 经用户指令重新立项为 M25（见 §25），不再属于本节范围。

#### M26 — 归档创建扩展与外部工具降级（2026-11-19 增补；来源：用户讨论稿蒸馏，与 M25-3/4 现状合并评估择优）

**硬约束**：内建写端仅纯 Rust crate（零 C 编译期依赖原则不变）；外部 CLI 仅显式探测后调用，缺失优雅终止（可读提示、不生成半成品、不 panic）；不改写已有归档、不做嵌套归档递归浏览（AGENTS §5.1 边界维持）；产物与校验文件入撤销栈（`u` 可逆）；外部命令参数逐项传递不经 shell 解释（防注入）。

**MUST**
- **M26-1 :archive 后台作业化**：迁移 M13-8 Job 模型（同 :tag copy/move）——创建过程后台线程执行，UI 不阻塞；完成回包（撤销栈入栈 + 刷新 + 提示）；:jobs 可见、x 可取消（M24-9 cancel_requested）。
- **M26-2 7z 内建创建**：`sevenz-rust 0.6`（纯 Rust，crates.io 核实 2026-11-19）`ArchiveFormat::SevenZ` 分派；缺扩展名补齐、同名不覆盖、校验文件链路（M25-4）与对话框格式列表同步扩展。
- **M26-3 7z 只读浏览升级**：M7-9 的 7z"解压提示"升级为 ArchiveFs 虚拟浏览（sevenz-rust 读端复用同一依赖；损坏/加密/空可读错误；M7 只读边界不变）。
- **M26-4 外部 CLI 归档降级**：rar / cab / lzh / tar.xz / tar.bz2 / tar.zst 创建——PATH 探测（7z/7za/7zr、rar；M25-6 find_git_tui 同款探测模式）→ `std::process::Command` 调用 → 输出抑制（M21-F8 同款）→ 缺失优雅终止（"创建 .rar 需要 rar/7z，未在系统中找到"）；CLI 失败按退出码/stderr 给可读错误；不生成半成品；产物入撤销栈。
- **M26-5 README 归档创建文档**：格式行为表（内建：zip/tar/tar.gz/7z；外部 CLI：xz/bz2/zst/rar/cab/lzh；校验默认 md5）；libarchive_oxide / OxiArc 列观察项（统一归档库成熟后再评估，见 NOTES）。

**SIMPLIFIED**
- M26-1：作业粒度 = 单次 :archive 命令（无进度百分比，完成/失败二元提示 + :jobs 状态）。
- M26-2：7z 压缩方法缺省 LZMA2 默认参数（不暴露压缩级别配置）。
- M26-4：外部 CLI 命令模板按格式硬编码（`7z a <归档> <条目...>` / `rar a ...`）；探测在首次创建该格式时执行（不缓存启动态）。
- 通用：M25-4 的对话框/校验/撤销语义对所有新格式透明复用，不新增配置面。

**OUT_OF_SCOPE**
- ArchiveWriter trait 抽象层（评估否决：枚举分派 + pack_* 函数已满足，格式数少不值得间接层——见 NOTES M26 立项）；归档内写入；7z 加密归档创建；分卷（multi-volume）；xz/bz2/zst 纯 Rust 编码器（crates.io 核实不可用/不成熟——lzma-rust 不产出 .xz 容器、bzip2 纯编码器不存在、yolowzstd 0.1.x 过年轻，走 M26-4 外部 CLI）。

#### M27 — 0.8.0 实测问题与 vifm 视觉对齐（2026-11-19 增补；来源：用户 0.8.0 实测反馈）

**硬约束**：符号链接目录必须在 rvifm 内部导航，不得因类型判定错误落到系统 opener；视觉调整不得引入新依赖、不得为特定终端写硬编码分支（AGENTS §5.5/§5.6），主题缺失、`NO_COLOR`、无 Nerd Font 时必须安全回退；PDF 仍遵守 AGENTS §5.5 的探测顺序——文本提取必须先执行，`prefer_image` 只改变“展示优先级”，不得跳过文本提取。

**MUST**
- **M27-1 符号链接目录进入**：物理视图分类条目时必须跟随符号链接目标判定 `is_dir`（保留 `is_link` 用于着色）；`ln -s <dir> link` 后 `l`/`Enter` 进入目标目录（rvifm 内部 `navigate_to`），不得调用 `system_open()`；目录优先排序、`..` 返回、目录树/预览的目录判定与之一致；断链不得使目录列表加载失败，`Enter` 给出可读提示且不 panic。
- **M27-2 vifm 风格布局与配色**：默认去掉左右 pane 的整框边框，改为 vifm 式布局——标签栏 1 行、每个 pane 顶部路径行 1 行、双 pane 之间 1 列分隔线（**M45-1 裁决后改为可配置 `[ui] pane_separator`，默认 false = 不显示；true 恢复分隔线**）、底部状态行；用背景色而非边框区分：`TabLine`/`TabLineSel`、`TopLine`/`TopLineSel`（非焦点/焦点 pane 路径行）、`Border`（分隔线）、`StatusLine`/`CmdLine`、`CurrLine`/`OtherLine`、`Selected`；文件类型映射 vifm 组名（`Directory`/`Link`/`BrokenLink`/`Executable` 等），缺失时回退现有 `ft_*`/16 色；新增 `[ui] pane_borders`（默认 false；true 恢复旧整框外观）；主题样式存在时优先，未激活主题时使用 vifm Default 风格的 16 色回退。
- **M27-3 PDF 首页/前两页图像预览（可配置）**：TUI 查看器（`w`/view）在 PDF 文本提取后，可按配置生成前 1–2 页图像交给 `ImageEngine` 渲染；`[preview] pdf_image_pages = 0|1|2`（默认 1；0 关闭）与 `pdf_prefer_image = false|true`（默认 false）；`prefer_image=false` 时仅在文本层不足（`NeedsVisual`）才走图像，`true` 时文本提取成功后仍优先展示图像；探测顺序仍为 `pdf-inspector` 文本 → `exprim`（仅 `pdf-visual`）→ `pdftoppm`/`mutool`；任一环节失败（工具缺失/非零退出/解码失败/超页数）回退到已提取文本或可读提示，不 panic；外部命令逐参数传递、不经 shell，临时图像文件在预览切换/退出时清理；输入文件大小与导出像素沿用 M11/AUDIT-2 护栏；`pick` 的 PDF 预览不在本任务（AUDIT-3 决定）。

**SIMPLIFIED**
- M27-1：进入后路径可显示链接路径或解析后的目标路径；不实现“编辑链接目标/创建链接”UI。
- M27-2：只覆盖 TUI 主界面（列表/预览/归档共用样式）；不实现 vifm 的完整 `:highlight` 语法或目录级 colorscheme；不新增第二行命令栏，状态行沿用 1 行。
- M27-3：只渲染前 1–2 页，不做任意页跳转/缩略图/连续滚动；DPI 固定（约 150），不暴露配置；2 页在预览区上下排列，空间不足时至少显示第 1 页并提示。

**OUT_OF_SCOPE**
- 符号链接创建/修改、链接目标解析对话框。
- vifmrc/完整 vifm 颜色方案解析、运行时 `:highlight` 命令。
- PDF 全页渲染、文本层重建、OCR、PDF 编辑。

#### M28 — 运维操作补齐（2026-11-20 增补；来源：评估报告蒸馏，用户确认 P1；对标 ranger/Total Commander/Double Commander/vifm）

**硬约束**：新建链接/批量重命名均为可逆写操作——必须入撤销栈且失败无半成品；`M27` 曾把"符号链接创建"列为该里程碑 OUT_OF_SCOPE，本批次以独立任务将其转入范围；Windows 平台差异（符号链接权限）只做回退与可读提示，不引入提权路径。

**MUST**
- **M28-1 `:link` 符号链接创建**：`:link <dest>`（作用域 = tags / `v` 选择 / 光标条目，与 `:archive` 同口径；dest 相对当前目录或绝对路径）为条目创建符号链接；Windows 目录条目在符号链接权限不足时回退目录联接（junction），文件条目无回退、可读提示；成功入撤销栈（`u` = 删除新建链接）；失败（已存在/权限/非法路径）可读提示、不覆盖、无半成品。
- **M28-2 `:rename` 结构化批量重命名**：`:rename s/<pat>/<repl>/[i]`（regex 替换，`i` 大小写不敏感标志可选）、`:rename prefix=<str>`、`:rename suffix=<str>`、`:rename seq=<start>,<step>,<width>`（序号插入）；作用域同 `:archive`；执行前在虚拟结果目录预览 `旧名 → 新名`（M24-8 结果前缀语义复用），确认后整批应用、取消不动；不符规则条目跳过并列出（沿用 M13-6 cw 的"同名/非法跳过"口径）；整批经既有 rename 撤销链路（`u` 一次逆序恢复全部）。
- **M28-3 `:jobs` 任务视图增强**：`p` 暂停/恢复运行中作业（`Job` 增 paused 协作式标志，检查点在条目循环边界，与 cancel_requested 同模式）；Enter 打开作业详情覆盖层（类型/源/目标/已处理条目数/传输字节数/速率滑动平均/已运行时长），Esc 返回；`:jobs` 列表增状态列（running/paused/done/cancelled）；暂停期间事件循环不阻塞（导航/`:q` 照常响应）。
- **M28-4 M28 文档收尾**：help.txt 补 `:link` / `:rename` / `:jobs` 增强键位；README 核心特性段同步；ACCEPTANCE §28 全部勾选后批次方可关闭。

**SIMPLIFIED**
- `:rename` 第一期仅上述结构化形态；大小写转换/扩展名专用子命令不做（`s///` 已覆盖）；预览仅确认/取消，不可逐行编辑。
- `:jobs` 详情速率用滑动平均估算，不做精确 ETA；暂停粒度为作业级。
- `:link` 不做硬链接；链接目标按用户输入原样（不做相对化/绝对化选项）。

**OUT_OF_SCOPE**
- 硬链接创建与硬链接计数列、符号链接循环检测。
- 作业优先级、多作业并行暂停编排。

#### M29 — 对比增强（2026-11-20 增补；来源：评估报告蒸馏，用户确认 P1；对标 vifm `:compare bycontents` + `dp`/`do`）

**硬约束**：`dp`/`do` 是写操作——复制走既有确认与撤销链路，删除走可恢复路径；内容对比必须有单文件体积上限（沿用 M11/AUDIT-17 护栏口径），不 OOM、不 panic。

**MUST**
- **M29-1 `:compare bycontents`**：`:compare bycontents` 对比左右 pane 目录内容（无参 `:compare` 保持 M24-8 按名+大小行为）；内容判定 = SHA-256（现有 `sha2` 依赖）；单文件读取超护栏上限的条目标注跳过、不 OOM；结果复用 `Tab.results` 虚拟目录前缀（`=` 相同 / `!` 不同 / `<` 仅左 / `>` 仅右）；不递归（与 M24-8 同口径）。
- **M29-2 对比结果 `dp`/`do` 同步**：`dp` = 将另一侧差异条目复制到当前侧（仅 `>` 条目；`<`/`!`/`=` 可读拒绝）；`do` = 删除当前侧多出条目（仅 `<` 条目，走可恢复路径）；均入撤销栈（`u` 可逆）；操作后对比结果自动重建（前缀变化正确；`u` 撤销后结果为静态快照、重跑 `:compare` 刷新——与 :find/:grep 语义一致）。**覆盖同步裁剪（2026-11-20 实现期决策，见 NOTES）**：`!` 条目不支持覆盖同步（copy 链路无覆盖确认流，引入属新范围）——可读拒绝，键位沿用 vimdiff 借名（dp/d 键序列经 Keymap 注册）。
- **M29-3 M29 文档收尾**：help.txt / README 补对比增强条目；ACCEPTANCE §29 逐项勾选后批次关闭。

**SIMPLIFIED**
- 内容对比为顶层条目级；`!` 条目预览显示大小/哈希差异信息行，不做逐字节 diff 视图。
- `dp`/`do` 仅顶层条目；不做目录树合并与双向自动 sync。
- `!`（两侧内容不同）条目不支持覆盖同步——可读拒绝（copy 覆盖确认流不在既有基建中，引入属新范围）。

**OUT_OF_SCOPE**
- 时间戳比较策略切换（内容哈希判定已足够）；递归目录树对比。

#### M30 — frecency 通用化（2026-11-20 增补；来源：M25-7 评估立项（NOTES"可行、迁移成本低"）+ 用户指令"对齐同级 frecency 项目最近更新"）

**对齐基线**（同级项目 `frecency`（bash+SQLite+fzf），2026-11-19 HEAD `d7e03d0`）：topic 命名用 `cmd`（其架构注释 dir/file/cmd/自定义 topic）；清理仅容量裁剪、**无分数下限**——同级项目 2026-11-18 引入 per-topic score floor 当日撤销（理由：分数是序数、绝对值跨使用模式无意义、阈值是 MAX_ENTRIES 的不可预测替身、低分重要条目必须可达），REGRESSION 固化 "no score-threshold cleanup" 不变量；书签 = keyword 事件、unbook 保留历史（rvifm M25-2/M23-2 已同构）；pick 只读、选择不计访问（rvifm AUDIT-1 已同构）；其 picker `g` action（gitui/ziggity/lazygit 优先级探测）与 rvifm M25-6 `gi` 同源，无需改动。

**MUST**
- **M30-1 cmd 事件 schema 与记录链路**：`EntryKind` 增 `Command` 变体（serde 新变体，旧数据零迁移兼容）；topic = `"cmd"`；`:` 命令 Enter 执行成功后 record（M24-6"成功才记录"语义扩展到写命令路径，失败不记录）；`IGNORE_PATTERNS` 扩展到 cmd topic（glob 匹配命令文本，命中静默跳过）；`[frecency]` per-topic λ / max_entries 增 cmd 档（λ 缺省 0.01，与 dir/file 同；compaction/pick 一致性沿用 AUDIT-16 口径）。
- **M30-2 `:` 命令历史迁移**：↑/↓ 数据源从会话内存 Vec 切换为 replay cmd topic 顶层 N 条（跨会话持久；排序 = 前缀分级 M23-1 + EMS 分数）；当前会话未落盘命令并入显示（NAV-4 会话合并语义）；↑/↓ 交互形态与 Esc/Enter 行为不变。
- **M30-3 路径补全加权**：`:` 模式 Tab 补全候选排序 = frecency 分数（Navigate/Open 事件，TUI 启动 replay 只读读取）优先、无记录条目字典序兜底；store 缺失/空时零成本降级（行为与现状一致）；候选来源仍为焦点 pane 条目（不引入全库模糊搜索）。
- **M30-4 命令书签**：命令文本可设为 keyword 书签（设置入口实现时定——历史菜单标记或 `:cmdmark <chars>`；消费 = `:` 模式内展开为命令文本供编辑/执行）；unbook 保留历史（M23-2 语义）；多字符书签在 `o` 搜索享高优先级（M23-1 既有，cmd topic 验证）。
- **M30-5 文档与隐私收尾**：README / help 明示"命令文本持久化写入 frecency redb（IGNORE_PATTERNS 可排除）"；对齐结论入 NOTES；ACCEPTANCE §30 勾选后批次关闭。

**SIMPLIFIED**
- 第一期 pick 不加命令视角（仅 TUI 命令行消费；同级项目 picker 已含 command preview——列为后续增强，不阻塞本期）。
- 命令书签设置/消费键位在实现批次内定，help 记录即可。

**OUT_OF_SCOPE**
- 不引入分数下限清理（对齐同级项目不变量）；清理仅容量裁剪（M23-3 既有语义）。
- shell 集成级命令自动记录（bash `PROMPT_COMMAND` 等）——rvifm 只记录 TUI `:` 内命令，同级项目的 shell 侧记录不属于 rvifm 职责。

#### M31 — 文档与验收补齐（2026-11-20 增补；来源：评估报告第四/五节建议，用户确认采纳）

**MUST**
- **M31-1 README 状态与边界同步**：开发路线表 M26 行"已确认启动"→ 已完成（漂移修正）；归档创建行为表补加密 7z / 分卷 7z 支持边界两行（措辞以 M26-3 实际行为为准——加密与损坏均给可读错误、分卷不支持虚拟浏览）；提权/运行时依赖段补 Windows UAC 边界句（M12 为 `sudo -A` 口径，Windows 尽力而为）。
- **M31-2 ACCEPTANCE 补可执行条目**：`:jobs` 取消响应时延条目（长传输作业中途 `x` 的终止时延上限，数值实现时定并写入条目）；外部 CLI 归档"安装/不安装"双环境验收（未装工具 → 可读错误；装 7z 但不支持 cab 打包 → 精确提示）。
- **M31-3 M11-6 手工验证清单**：ACCEPTANCE 独立小节列出真实终端逐项清单（Alacritty halfblocks / tmux passthrough / Kitty 原生协议 / 无协议降级链），M11-6 条目引用该清单——使 deferred 项可在任意真实终端 10 分钟补验。
- **M31-4 ACCEPTANCE M0–M13 复选框化**：普通 bullet 转 `- [ ]` 复选框（零语义、机械转换；REVIEW P2-3 既有建议）；转完后以 grep 核验无残留裸 bullet。
- **M31-5 pdf-visual 护栏实测**：在 rustc ≥ 1.97 环境（CI ubuntu 或 Windows 本机）跑 pdf-visual 语料回归 + `cargo check --features pdf-visual`，记录 exprim 渲染耗时与内存峰值（含 100MB 级输入语料）到 NOTES；本任务只测不改护栏，若超预期则另行登记收紧任务。

**SIMPLIFIED**
- M31-4 一次提交内完成机械转换，不重写条目文字。
- M31-5 实测环境二选一（CI 临时 job 或 Windows 本机），一组数据即结论。

**OUT_OF_SCOPE**
- 不为实测引入新语料生成工具；护栏数值调整不在本任务范围。

#### M32 — 远端健壮性与会话扩展（2026-11-20 增补；来源：评估报告 P2 蒸馏，用户确认补登）

**硬约束**：断线重连不得产生半成品（M20-3 口径维持：显式取消清理）；重连失败后 TUI 状态与操作前一致；会话文件损坏静默降级（M25-1 口径维持）。

**MUST**
- **M32-1 SFTP 连接健康检查与自动重连**：远端操作（list/read/write）失败且错误分类为连接类（M19-1 分类复用）时，标记连接断开；同会话后续操作自动重连一次，成功则重载当前远端目录继续，再失败给可读提示（含重连失败原因）；重连过程不阻塞事件循环（后台线程，复用 M19-3b 异步模型）。
- **M32-2 大文件传输断点续传**：M20-3 跨 VFS 上传/下载因连接类失败中断时保留传输现场 `<target>.part`（本地临时目录）；自动重连（M32-1 机制）成功后重写目标并移除 `.part`；显式取消（`x`）不产生半成品（M20-3 口径）；撤销语义不变（入撤销栈为成品）。**SIMPLIFIED 修订（2026-11-20 实现期）**：会话 write/read 无偏移参数——重连后整文件重写，`.part` 为诊断/重试现场而非偏移续传；跨进程断电恢复不在范围。
- **M32-3 会话持久化范围扩展**：session.toml 保存/恢复 `Location::Archive { host, inner }` 与 `Location::Remote { remote, path }`；启动恢复时归档 host 存在即直接进入，远端尝试重连（失败降级为跳过该 tab + 状态栏可读提示，不 panic）；物理 tab 恢复行为不回归。
- **M32-4 M32 文档收尾**：help/README 补重连与续传行为说明；ACCEPTANCE §32 勾选后批次关闭。

**SIMPLIFIED**
- 重连后整文件重写（会话接口无偏移参数——偏移续传需 RemoteFs 增接口，列入后续）；不保证跨进程断电恢复（会话级现场）。
- 重连为操作触发式（下次操作时重连一次），不做后台保活心跳。
- 结果虚拟目录（`:find`/`:grep`）与树视图不持久化（只恢复 base 目录）。

**OUT_OF_SCOPE**
- 分块并行传输、rsync 协议级续传。
- 远端目录树缓存的离线浏览。

#### M33 — 键位模式分层扩展（2026-11-20 增补；来源：评估报告 P2 蒸馏，用户确认补登；README"仅 [normal] 可配"限制的补齐）

**硬约束**：分层模型与 `[normal]` 同构（prepend/append/replace）；未配置的模式必须零行为变化（回退内建默认）；序列超时沿用全局 `[keymap] seq_timeout_ms`。

**MUST**
- **M33-1 三层 TOML 解析与合并**：`[visual]` / `[view]` / `[command]` 三节支持 `prepend_keymap` / `append_keymap` / `keymap`（replace），解析与合并复用 `[normal]` 既有机制（GAP-1 代码路径）；缺省节为空。
- **M33-2 模式化消费接线**：Visual / View / Command-line 模式的 Resolver 挂接对应层；`on` → `run` 语义与 `[normal]` 一致（动作名白名单按模式校验——不可映射动作给可读错误）；未配置模式行为与现状完全一致。
- **M33-3 M33 文档收尾**：help / README 键位段与 config.toml 示例补三层用法；ACCEPTANCE §33 勾选后批次关闭。

**SIMPLIFIED**
- Command-line 模式可映射集合较小（历史/补全/Esc 类动作），白名单实现时定并写入 help。
- 不做 per-mode 独立序列超时（沿用全局配置）；不做模式间绑定继承（visual 不自动继承 normal 绑定）。

**OUT_OF_SCOPE**
- 每模式独立 seq 超时、绑定继承规则、用户自定义 Action（全局 OUT_OF_SCOPE 既有）。

#### M34 — 主题、预览统一与文档（2026-11-21 增补；来源：DeepSeek 讨论蒸馏·用户实测反馈，用户确认登记；裁决见 NOTES 2026-11-21）

**硬约束**：预览渲染同源红线不变（TUI 预览与 pick 共用 `render_preview` / `ImageEngine`，M8-8 / AUDIT-3）；高亮组映射走数据（opaline token / TOML），不为特定主题写死代码分支；二进制预览信息面板**不执行文件内容、不 spawn 子进程**（`--version` 探测已裁剪）；主题默认值变更只影响未显式配置 `[ui] theme` 的安装。

**MUST**
- **M34-1 默认主题 gruvbox**：`[ui] theme` 缺省值 None → `gruvbox-dark`（opaline 内置；fresh install 视觉等价 vifm gruvbox，显式配置不受影响）。核查 M27-2 vifm 高亮组（TopLine/TabLine/TopLineSel/TabLineSel/Border/CurrLine/OtherLine/StatusLine/Selected）在激活主题下的映射——缺 token 的组按 gruvbox 色板补映射或明确回退；验证 `[ui] theme = "dracula"` 与 `:theme dracula` 生效；M27-2 的 16 色 vifm 回退保留为无主题兜底，不删除。
- **M34-2 snowwhite 内置主题移植**：从 vifm-colors 仓库 `snowwhite.vifm`（约 32 行高亮组）一次性提取颜色映射 opaline token schema，生成 rvifm 捆绑 TOML（include_str! 或等价机制，经 AUDIT-13 activate 链路可发现）；`[ui] theme = "snowwhite"` / `:theme snowwhite` 可选。
- **M34-3 右 pane `/` 搜索**：解除 M14"搜索仅左 pane"SIMPLIFIED——`/` 按 `tab.focus` 分派，焦点在右 pane 时对右 pane 目录列表执行搜索（匹配高亮 + 实时跳转 + 计数 + n/N + Esc 清理，与左 pane M15 FIX-1 语义一致）；后续 FIX-7 统一左右 pane 的普通模式键位分派。
- **M34-4 对侧预览统一（用户裁决方案 A）**：`preview_path()` 焦点感知修复——预览目标 = 当前焦点 pane 光标文件（与其自身文档"活动 pane 光标文件"一致；含归档/远端 Location 语义）。:view（M13-9 交互查看器）与 frecency Pane（M23-6）统一为同一"焦点 pane 交互 + 对侧 pane 预览"模型——预览渲染在非焦点 pane（焦点在左预览在右，与现状兼容；焦点在右预览渲染到左 pane）；无预览模式时右 pane 保持第二目录列表默认；两处共用同一"对侧预览"定位/渲染路径（消除两套预览逻辑），Tab 切回焦点语义不变。
- **M34-5 frecency Pane 视觉与输入回显**：搜索模式实时回显当前 query（底部状态栏，键入即时更新、清空同步）；结果行渲染 devicons 图标 + 全路径（图标链路与文件列表/预览共用，无 Nerd Font 降级同 M9-5；`[目录]/[文件]` 文本标记移除，`✦keyword` 标记保留）；顶部标题文案 `Frecency - 高频目录/文件搜索`。
- **M34-6 `.jar` 归档识别**：`is_archive_file` 增加 `.jar`（复用 M7-2 ZipFs zip 后端）；浏览/预览/搜索链路自动生效；roundtrip 与损坏归档测试同 M26-3 模式。
- **M34-7 二进制/不可读预览信息面板**：NUL 二进制、非 UTF-8、读取失败统一渲染结构化信息面板——文件大小、修改时间、类型推断（扩展名 + 魔数嗅探），替代现有 lossy 乱码渲染与单一错误行；不执行文件（硬约束）；TUI 预览与 pick 预览同源。
- **M34-8 docs/GUIDE.md 用户手册**：快速上手（安装 → 首次浏览/文件操作/撤销 → 归档 → frecency 跳转）+ 键位/命令速查表；分节参考 ranger(1) 结构（浏览/文件操作/选择/搜索/归档/跳转/配置），每节最小示例；README 不扩写；GUIDE（仓库侧手册）与 help.txt（TUI 内速查）分工；内容以实现现状为准，不描写未实现功能。
- **M34-9 图片/PDF 预览尺寸适配（2026-11-21 用户实测后追加）**：ImageEngine 图像与 PDF 视觉页（AUDIT-2 / M27-3 链路）渲染尺寸语义——原始尺寸不大于预览窗格时按实际尺寸**不放大、窗格内居中**；超过窗格时**按宽度缩放至预览窗格宽度**，高度超出部分**显示顶部不滚动**（与文本预览"前 N 行"语义一致；M45 裁决去除旧"查看器内可滚动"措辞，与 SIMPLIFIED 统一）；实现基线 ratatui-image `Resize::Fit`（天然不放大，`Fill` 会放大不使用）；pick 预览同源适用。
- **M34-10 M34 文档收尾**：help.txt（右 pane 搜索语义、主题列表）、README（默认主题、jar 支持）与 ACCEPTANCE §34 逐条同步；批次勾选完成后关闭。

**SIMPLIFIED**
- M34-1：opaline 缺 token 的组允许整组回退 16 色兜底（不逐组造 token）；不新增主题管理界面（沿用 `:theme` + 配置）。
- M34-2：颜色映射为一次性手工移植；snowwhite 为 rvifm 捆绑而非回传 opaline。
- M34-4：只统一"预览目标与放置"，不合并 :view 与 frecency 的全部交互细节（M17 查看器能力维持现状）。
- M34-5：query 回显仅搜索模式（Normal 模式底部提示维持现状）。
- M34-7：类型推断做到"目录/文本/二进制/归档/已知扩展名"粒度，不追求完整 MIME 数据库。
- M34-9：普通预览窗格不提供滚动（超出高度显示顶部，同文本预览语义）；fit-width 仅在图像大于窗格时生效，小于窗格绝不放大。

**OUT_OF_SCOPE**
- `.vifm` 配色文件解析器（一次性提取移植替代；与 AGENTS §5.6"不实现 vifmrc 注入"一致）。
- 可执行文件 `--version` 子进程探测（用户裁决裁剪：预览即执行未知二进制有安全风险）。
- `.war` / `.apk` 等其它 zip 衍生格式的归档识别（仅 `.jar`）。
- 主题热重载（文件监视自动刷新）。

#### M25 — 会话持久化、marks、归档创建与 git 集成（2026-11-19 增补；来源：用户批次需求讨论稿蒸馏）

**硬约束**：marks 由 frecency 模块实现，数据持久化在 frecency 的 redb 存储中（与 `keyword` bookmark 共用），带频率记录，不设独立 marks 存储/模块；归档创建仅纯 Rust 编码器（zip / tar / tar.gz），`.tar.xz` / `.tar.bz2` / `.tar.zst` 保持只读（**M26-4 起改为外部 CLI 降级，见 §26**）；git 集成零编译期依赖——`git` CLI 子进程实现，不引入 git2/gitoxide（M23-12 评估路线）；会话/校验文件损坏静默降级不 panic；所有新键位经 keymap.rs Action 驱动。

**MUST**
- **M25-1 会话持久化**：TUI 退出（既有退出清理点）保存会话到配置目录 `session.toml`——tabs（物理 Location + cursor + on_parent_row + focus）+ active tab 索引；启动恢复（仅物理目录 tab；非物理 Location 的 tab 丢弃）；文件缺失/损坏/空 → 静默新会话；`[ui] session_restore` 默认开（false = 不读不写）。
- **M25-2 marks（frecency 实现）**：`m<char>` 把当前光标条目（目录或文件；`..` 行 = 当前目录）以 keyword = `<char>` 写入 frecency redb（与 bookmark 共用 `EntryRecord.keyword`），带频率记录；`'<char>` 跳转复用 NAV-1 quickjump 链路——按该 keyword 的 frecency 评分选最高条目，目录 cd（navigate_to 入历史）、文件父目录定位光标（M8-12 文件语义）；未命中提示不静默；合法字符 = `[a-z][A-Z][0-9]`（vifm marks 口径）。
  - **keyword 长度分工（2026-11-19 用户确认）**：**单字符** keyword 用于 `'<char>` 两键导航（marks 语义）；**多字符** keyword 用于 `o` / `:j` / `:J` 交互选择器搜索时的**高优先级匹配**——输入多字符 keyword（如 `rvifm`）时，该 keyword 命中的条目排在候选列表前面，Enter 后在当前 pane 跳转到对应路径或文件（`:j` 仅目录、`:J` 仅文件）。两种长度共用同一份 `EntryRecord.keyword` 数据与 frecency 评分，不建第二套索引。
  - **命令视图**：`:bmark` / `:bmarks` / `:bmgo` / `:delbmarks` 为 vifm 对齐的 bookmark 命令，与 `m<char>` / `'<char>` 共用数据；既有 `:keyword <chars>` 保留为设置 keyword 的兼容命令（见 `help.txt`）。marks/bookmark 统一持久化于 frecency redb，不设独立数据文件。
- **M25-3 归档创建**：`:archive [zip|tar|tar.gz] <名>` 将标记集合（tags；空 = 光标条目）打包为当前目录下 `<名>`；纯 Rust 写端（zip crate / tar + flate2 rust backend）；目标同名已存在报错不覆盖；入撤销栈（Create，`u` 可逆）；`.tar.xz` / `.tar.bz2` / `.tar.zst` 创建请求可读提示不支持（只读边界；**M26-4 起该三类格式改为外部 CLI 降级路径，见 §26**）。
- **M25-4 归档对话框与校验文件**：`:archive` 无参 → 交互对话框（名称输入预填光标条目名；j/k/空格切换格式与校验；Enter 执行 Esc 取消）；归档完成后生成校验文件，默认 md5（`<名>.md5`，md5sum 兼容行格式），可选 sha256 / none（命令参数 + 配置 `[archive] checksum = "md5"`）；md5/sha2 纯 Rust crate。
- **M25-5 git 状态列（异步）**：目录加载时探测仓库根（逐级 .git）；`git --no-optional-locks status --porcelain -z --untracked-files=normal -- <当前目录>/` 子进程查询（pathspec 限定当前目录；不加锁不写 index）；**后台线程异步执行、UI 不等待**——列表先渲染，状态列就绪后填充（gitui 同款体感策略）；per-目录缓存，导航与 watcher 事件触发刷新；文件列表追加状态字符列（M/A/D/R/?? 等，着色；非仓库或开关关闭时隐藏）；`[ui] git_column` 默认开。
- **M25-6 git 状态栏与 gi 快捷键**：状态栏显示分支名 + diff 摘要（porcelain `-b` / `git diff --shortstat`）——**per-仓库缓存 + 异步刷新**（同 M25-5 线程模式，非每键重算）；`gi`（Action::GitUi）按优先级探测 PATH：gitui → ziggity → lazygit，首个命中挂起 TUI 启动（M18-4/M23-11 分流复用）；全部未命中可读报错；`:gitui` 命令同语义。
- **M25-7 frecency 通用化评估**（评估任务，不实现）：评估命令历史/路径补全迁移到 frecency 记录评分体系——记录与评分查询方法泛化（EntryKind 扩展命令类事件、命令书签 = keyword 事件泛化、补全候选 = 目录数据 + 当前路径邻近加权）的可行性、schema 影响、迁移成本；结论与边界入 NOTES；若可行给出下一批次任务建议。

**SIMPLIFIED**

- M25-1：会话文件不存归档/远端视图、不存 marks；无备份/迁移机制。
- M25-2：marks 持久化在 frecency redb（与 bookmark 同源），不设 session.toml 存储；合法字符不含符号。
- M25-3：目录递归打包，软链接按链接本身收录；打包条目数/总大小无上限护栏（用户显式发起）。
- M25-5：状态映射仅对当前目录直接子项；异步结果未就绪时状态列暂空（不阻塞列表渲染）；超大仓库进一步提速建议用户侧开启 `core.fsmonitor` + `core.untrackedCache`（README 提示，rvifm 不代改配置）。
- M25-6：diff 摘要仅工作区（staged 不分列）；gitui/ziggity/lazygit 为外部工具，用户自行安装。

#### M22 — SFTP 安全与远端体验补全（2026-11-17 增补）

**硬约束**：known_hosts 校验为安全默认——指纹变更**拒绝连接**并给出可读错误，跳过校验必须是用户显式配置（`[remotes.toml] known_hosts = false`）；口令**不得**写入任何配置/日志；远端文件打开走临时副本（FUSE 仍 OUT_OF_SCOPE），临时副本不进撤销栈；远端 `:find`/`:grep` 无 gitignore 语义（不读远端 ignore 文件），须有单文件大小与总量上限防大文件拖垮。

**MUST**
- **M22-1 `:cd` 相对路径锚定**：相对路径（`..`/`./x`/`sub`）相对**当前 tab 目录**解析，不相对进程 CWD；解析成功后 `Location::Physical` 恒为绝对路径（归一化）。失败（目录不存在）不改位置。
- **M22-2 known_hosts 校验**：SFTP 连接时读取 `~/.ssh/known_hosts`（OpenSSH 格式：明文 hostname 与 `|1|` hashed 两种；`@revoked`/`@cert-authority` 等标记行跳过不解析）。首连（无记录）：接受并**追加写入** known_hosts（含端口非 22 的 `[host]:port` 形式）；已记录：指纹比对，**变更即拒连**，错误含新旧指纹与处理指引；`known_hosts = false` 时跳过全部校验（用户显式配置）。known_hosts 文件权限保持 0600（存在则不改写权限）。
- **M22-3 SFTP 口令认证**：密钥探测/认证失败后回退口令认证；口令经 TUI AskPass 模式（M12-3 通道）输入，masked 渲染，不落盘不进配置；认证失败可读错误，取消干净中止（TUI 状态不变）。
- **M22-4 远端 `:mk` 与提示修正**：远端视图 `:mk <名>` = `create_dir` + 进入该目录（已存在/失败报错不进入）；远端白名单加 `mk`；未接命令的拒绝提示去掉过时的"在 M20 接入"表述，改为"远端不支持: {cmd}"。
- **M22-7 README 补齐**：里程碑表补 M12–M21 行（状态与 PLAN 一致）；状态行核对。

**SIMPLIFIED**
- **M22-5 远端 `:find`/`:grep`**：远端视图下递归遍历当前远端子树（RemoteFs list，条目上限复用 1 万）；`:find` 文件名子串匹配、`:grep` 逐行 regex（read 字节 → UTF-8 lossy）；单文件 read 上限 1 MiB（超限跳过）；结果复用既有 results 虚拟目录与定位链路；无 ignore 语义（硬约束）。
- **M22-6 远端文件打开**：RemoteFs read → 临时副本（系统临时目录，`rvifm-open-` 前缀 + 随机后缀）→ system opener；副本保留至进程退出统一清理（Windows opener 可能锁定文件，不做即时删除）；只读用途，不入撤销栈；下载失败可读错误。

**OUT_OF_SCOPE**
- `cd -` 前目录跳转（histprev/:history 已覆盖）；known_hosts 的 `@cert-authority` 证书校验；SFTP 主机口令改存配置；远端流式/断点打开大文件；远端自动刷新（M21-F5 同界）。

#### M23 — Frecency 对齐增强与 Pane 模式（2026-11-17 增补；来源：DeepSeek 分享讨论蒸馏）

**硬约束**：不动已对齐的评分核心（events+snapshot+meta 三表、write-time 记录 + read-time 重放、dir/file 独立时钟、EMS 数学形式）；`pick` 保持纯只读；`c`/`v`/`e` 仅在 Frecency 模式下重映射，退出后恢复原语义（`v` 不得侵入 Visual、`e` 不与其他绑定冲突）；keyword 事件仍是元数据事件（不推进时钟、不加分）；容量清理永不删除带 keyword 的条目；IGNORE_PATTERNS 在 record 入口静默跳过（不追加事件）；Frecency 模式不新建独立预览渲染路径（复用既有 `:view` / PreviewRenderer 路径）。

**MUST**
- **M23-1 多词搜索与匹配分级**：`o`/`pick` 筛选升级——`MatchGrade`（Keyword > LastSegment > Substring）分级排序（grade 升序、score 降序）；多词查询 `split_whitespace`：ordered（按序命中路径）与 any-order（全词命中）两种模式；无命中降级现状子串过滤。
- **M23-2 keyword unbook 保留历史**：取消书签（`:keyword -`）只移除 keyword 元数据，**保留累积分数与历史**（现状核对：若已满足则登记验证）。
- **M23-3 容量清理与书签保护**：record 写事务/compaction 时检查 per-topic `max_entries`（默认 1500）；超限删除最低分的**未书签**条目至 90%；带 keyword 条目永不删除。
- **M23-4 IGNORE_PATTERNS**：record 入口 glob 匹配配置模式（如 `*/.git`、`*/node_modules/*`），命中静默跳过不追加事件；模式列表来自 TOML 配置（复用既有 glob 匹配能力，不引新 crate）。
- **M23-5 per-topic LAMBDA / MAX_ENTRIES 配置**：`[frecency]` TOML 段按 topic（dir/file/unified）各自配置 `lambda` 与 `max_entries`；默认值 unified 0.005、dir/file 0.01（与参考项目一致）；旧配置缺省兼容。
- **M23-6 Frecency Pane 模式**：`o` 让**焦点 pane** 进入 Frecency 内嵌搜索（非浮层），对侧 pane 保持可见；结果光标移动时对侧自动预览当前条目（目录浅层 tree、图片、Office/PDF、文本）；FIX-8 起复用 `:view` 的按路径渲染链（ImageEngine、语法高亮与信息面板），不再使用纯文本摘要预览；Esc 逐级退出（Search → Normal → Directory）。
- **M23-7 `c`/`v`/`e` 模式内重映射**：`c` 复制选中路径到剪贴板（状态栏 `copied <path>`，不退出）；`v` 对侧进入 View 全文模式并聚焦（j/k/gg/G、`/` 搜索、n/N；Tab 切回 Frecency 搜索区）；`e` `$EDITOR` 打开选中文件并退出 Frecency（不自动跳转）；View 超大文件上限 10 万行（超出提示"文件过大，仅显示前 N 行"）。

**SIMPLIFIED**
- **M23-8 选择器内删除 + 撤销**：`o` 结果项 `d` 删除条目 → redb `trash` 表，`u` 可撤销移回；设合理上限（如 200 条）；实现超 3 轮不过验收即 deferred（讨论稿标注中优先级）。

**OUT_OF_SCOPE**
- SQL/SQLite 技巧与 bash 实现（以 redb + Rust 内存排序等价表达）；修改 EMS 数学形式；keyword 事件推进时钟或加分；删除带 keyword 条目；`d` 物理删除文件（仅删除 frecency 条目记录）。

**MUST（2026-11-18 追加与调整）**
- **M23-9 远端文件跟踪上传**：远端文件经临时副本打开后，监听副本修改（notify）；修改**停止**（防抖 2s）→ 自动上传回服务器（RemoteFs write）；上传失败状态栏可读提示（下次修改自然重试）；多次修改多次上传；退出清理逻辑不变（M22-6）。
- **M23-10 y 复制序列与 e 编辑器（Normal 模式；目录/文件通用；多选）**：`yn` 复制名称（单选=光标条目；多选=标记/可视选择全部，换行连接）；`yf` 复制全路径（多选语义同）；`yc` 复制内容（文件=文本内容；目录=目录内条目名清单，每行一个）；`e` = `$EDITOR` 打开当前光标**文件**（目录提示"目录不支持编辑"）；剪贴板写入机制实现时定（OSC52 零依赖优先），状态栏 `copied <目标>` 反馈。
- **M23-11 自定义命令快捷键与内置变量（vifm 风格）**：TOML 键位可绑定到**外部命令模板**，模板支持内置变量（vifm 对齐：`%d` 焦点 pane 目录、`%f` 光标文件名、`%F` 光标完整路径、`%D` 对侧 pane 目录；其余参考其它 FM 调研取用）；示例配置 `,g` 打开 lazygit——**判定优先级**：焦点 pane 为 git 仓库（存在 .git）→ 在该目录打开；否则当前高亮条目目录在仓库内 → 打开；否则提示"不是 git 仓库"；外部命令执行复用 `[ui] terminal_in_new_window` 分流。
- **M23-7（修订 2026-11-18）Frecency Pane 键位与 Normal 一致**：除 `l`/`Enter`（跳转选定）外，其余键与 Normal 模式一致——**作用于光标所在的结果条目**（y*/e/w/view_mode 等照常可用）；原计划的 `c`/`v` 模式内重映射**移除**（c/v 恢复 Normal 语义：c=rename 前缀、v=Visual）；Esc 三级退出（Search → Normal → Directory）不变。

**SIMPLIFIED（2026-11-18 追加）**
- **M23-12 git 集成评估（研究型）**：调研 Rust 可用 git 模块（git2/libgit2、gitoxide）与其它 FM/工具的 git 集成点（状态装饰、分支显示、gitscape 等）；可行性决策与功能清单写入 NOTES——可行再立项，本轮只评估不实现。

#### M18 — 排序系统与终端集成（2026-11-15 增补；vifm 排序对齐）

**硬约束**：`s` 保持 shell 入口；`S` 为排序唯一入口（对话框内字母直接触发，无快速排序前缀）；空格=切换活动窗格（vifm 语义，与 Tab 并存），仅对话框内用于升降切换；排序快捷键成对（小写升/大写降）；多字段仅经 :sort 参数，无 TUI 字段栈。

**MUST**
- `S` 排序对话框：列出字段（name/mtime/size/ext）与当前排序状态；对话框内 n/N/m/M/s/S/e/E 直接应用并关闭（小写升、大写降）；j/k 浏览、空格切换高亮字段升降（不立即应用）、回车应用；Esc 取消。应用后列表立即重排，光标保持在同一文件。
- `:sort <字段列表>`：逗号分隔，字段后可跟 asc/desc（如 `:sort name,size desc`）；解析失败可读错误；无多字段 TUI 栈。
- `s`：当前 pane 启动 $SHELL，退出后 raw mode/alternate screen 恢复、光标滚动不变；失败可读提示。
- `:!<cmd>`：执行外部命令；`[ui] terminal_in_new_window`（默认 true）：true 时 $TMUX/$STY 检测新窗口执行否则当前 pane，false 始终当前 pane；同时作用于 `s` 与 `:!`。
- Space：Normal 模式切换活动窗格（vifm 语义，与 Tab 等效）。

**SIMPLIFIED**：排序状态为单字段（tab 持有）；重排仅当前条目集（跟随刷新恢复 tab 排序）；新窗口执行先支持 tmux。

**OUT_OF_SCOPE**：多字段 TUI 栈；排序持久化到配置。

#### M19 — SFTP 远端 VFS：浏览与配置（2026-11-15 增补）

**MUST**
- RemoteFs trait（协议无关）：list/read/metadata；错误分类：连接失败/认证失败/路径不存在/权限不足/超时；预留 write/remove/rename/create_dir 签名（M19 返回未实现）。
- `Location::Remote { remote, path }`：导航 j/k/l/h 与 Physical 一致；h 在远端根返回进入前本地目录；面包屑 `remote:/path`。
- 选型 spike（examples/sftp_spike.rs，dev-dependencies）：候选 russh+justsftp / remotefs-rs；对 127.0.0.1 SSH 完成根目录 list；记录编译/依赖体积/认证/异步模型/集成难度与写操作支持；结论入 NOTES 后**暂停等确认**再集成。
- 正式后端（按 NOTES 锁定方案）：密钥认证为主、密码可选增强；超时与错误映射；网络 IO 不阻塞事件循环；M19 仅 list/read/metadata。
- 浏览与配置：`~/.ssh/config` Host 别名 + `~/.config/rvifm/remotes.toml`（优先级最高，可覆盖同名 Host，protocol/host/port/user/key/root）；`:remote <name>` 进入远端根；`:remote` 列出全部；连接/认证/超时可读提示。

**SIMPLIFIED**：远端排序与列表内 `/` 搜索复用本地实现（entries 基）；`:find`/`:grep` 不作用远端（ignore 遍历仅本地 fs，远端递归搜索为后续增强）；远端归档不进入虚拟目录；预览经 RemoteFs read 的文本/图片按本地同链路（SFTP read 供数据）。

**OUT_OF_SCOPE**：写操作（M20）；远端递归搜索；密码交互式认证（可选增强）。

#### M20 — SFTP 远端 VFS：写操作与本地体验对齐（2026-11-15 增补；依赖 M19 验收）

**MUST**
- RemoteFs write/remove/rename/create_dir 实现；远端写操作进入既有撤销栈（`UndoableOp::Remote` 变体：remote 名/路径/类型）；dd/yy/p/P/重命名/:mkdir 与本地同键同语义；失败可读错误。
- 不可逆边界：覆盖远端已存在文件、删除非空目录弹确认（文案同本地场景）；远端删除走可恢复路径（后端支持临时目录）否则显式提示"远端删除不可撤销"并在撤销栈标记不可逆；取消后状态不变。
- 跨 VFS 复制/移动：本地 yy → 远端 p/P 及反向；同进度提示与撤销语义；大文件传输显示状态/进度且不阻塞 UI 输入；失败清理半成品。
- 验收边界：上传→列表→重命名→删除→撤销删除全链路可重复；断网写操作可读错误、不留半成品。

**SIMPLIFIED**：进度先做"传输中"状态行，细粒度进度条后续；可恢复删除依赖后端能力探测。

**OUT_OF_SCOPE**：SFTP 之外的协议（scp/rsync/ftp）；并发多远端连接池。

#### M16 — 导航增强批次（2026-11-15 增补；marks 替代方案 + vifm history 对齐）

**背景**：用户实测提出"两键直达"诉求（`'d` 对比 `o d <Enter>` 少一次确认，高频跳转可感知）。M16 当时结论为 **marks 不回归**，以 frecency 快速映射层补齐体验差异；history 对齐 vifm 的"菜单 + 可映射命令"机制（非单键跳转）。**2026-11-19 用户指令反转**：marks 定案由 frecency 实现并持久化（M25-2），与 bookmark 统一，原结论仅作历史背景。决策与调研结论见 NOTES 2026-11-15 / 2026-11-19 条目。

**MUST**

- **NAV-1 frecency 快速映射层**：
  - Normal 模式 `'` + 单字符：直接跳到 frecency 中该 keyword 的最高排名条目（重放评分 + 读时 aging）；选定目录即 cd（等价 `:cd`，history 入栈，跳转即真实导航，走既有 record 链）。
  - 缺省语义 `'d` 即查 `keyword == "d"`；配置 `~/.config/rvifm/` TOML `[quickjump]` 提供 char→keyword 可选别名（如 `d = "dl"`，配置不存路径）。
  - keyword 设置入口（补 M8 缺口）：append-only keyword 事件（replay 时应用到条目）；`rvifm record --keyword <chars> <path>` 与 TUI `:keyword <chars>`（标记当前目录）同链产生。
  - `'` 未配置且无命中：状态栏提示（如"无 keyword d 的条目（:keyword d 设置）"），不静默失败。
- **NAV-3 `:histprev` / `:histnext`**：
  - `Tab.history` 从单向 Vec 改造为 back/forward 游标结构；新导航（enter/return/`:cd` 等）截断 forward。
  - `:histprev` 后退、`:histnext` 前进，作用于当前 tab 的视图历史；边界（无后退/前进）给提示。
  - **不设默认键位**；help 与 keymap.toml 示例给推荐映射并注明 Ctrl-I==Tab 键码冲突（Tab 已被 pane 焦点占用）。
  - 跨 tab 独立：tab A 的后退/前进不影响 tab B。
- **NAV-2 `:history` 菜单**：
  - `:history` / `:his` 弹出当前 tab 视图的目录历史菜单（back 栈）。
  - 菜单内 j/k 移动、Enter/l 导航到选中目录、dd 移除该条记录（纯内存操作，不触碰 frecency 存储）。
  - 按 tab 独立（切 tab 后菜单内容对应当前视图）；Esc/q 关闭，列表与光标状态不变。
- **NAV-4 `o` 选择器"近期"视角**：
  - JumpKind::Recent：按最后访问时间倒序（last_visit desc）；数据 = frecency replay + 当前会话内存评分合并（会话内跳转即时可见）。
  - 复用 JumpSelect 覆盖层交互（模糊筛选/j/k/Enter/Esc）；Enter 语义与 `o` 一致；选择器只读红线不变。

**SIMPLIFIED**

- NAV-1：`'` 仅支持单字符第二键（f/F 通配序列模式）；多字符 keyword 不进快速层（仍可经 `o` 检索）；跳转目标限定 dir。
- NAV-3：历史只含目录导航（不含命令行/搜索历史）；历史不持久化（会话内存态）。
- NAV-2：菜单只做目录历史（不做 vifm 的 c[md]/s[earch] 变体）；dd 移除仅会话内有效。
- NAV-4：视角入口命令名实现时定（写 NOTES）；条目量级与 `o` 一致（内存重放）。

**OUT_OF_SCOPE**

- M16 阶段不实现 marks；2026-11-19 用户指令反转后定案由 frecency 实现并持久化（M25-2，见 §25），此处不再作为永久边界。
- 命令行历史 / 搜索历史菜单（`:history` 的 c/s 变体）。
- Ctrl-O / Ctrl-I 默认绑定（终端键码冲突，仅用户经 keymap 映射）。
- 目录历史持久化到磁盘。

#### M15 — 用户实测反馈批次（2026-11-15 增补；基线 0.5.0，缺陷修复×4 + 新增×4）

**缺陷修复（MUST）**

- **FIX-1 搜索高亮与实时索引**：`/` 搜索为"高亮 + 跳转"语义（vifm hlsearch 模型）——列表完整保留（与 `=` 过滤的"隐藏非匹配项"明确区分）；输入期间所有匹配子串实时高亮（样式与光标行反显可区分），光标实时跳到第一个匹配项；状态栏显示"当前匹配序号/总匹配数"（如 `3/12`，无匹配 `0/0` 并提示）；`n/N` 在匹配项间循环且索引同步；清空/取消后高亮与计数消失，光标位置合理。
- **FIX-2 yy/p 复制链路**：修复 yank 缓冲区与 PutCopy 动作之间的断链——`yy` 后 `p` 在焦点 pane 所在目录执行**复制**（不是移动）；成功后状态栏提示（如 `Copied 1 item to <path>`）；失败（权限、目标同名、跨设备等）状态栏可读错误，不 panic、不留半成品；复制入撤销栈，`u` 可逆（M2-4b 的 Create 语义：undo = 删除副本）。
- **FIX-3 归档浏览回归**：`.zip` / `.tar` / `.tar.gz` 光标 `l`/`Enter` 进入只读虚拟目录（M7 规格不变，此为 0.4.x 回归修复）；归档内 `j/k/h/l` 正常；归档根 `h`/`..` 返回归档文件所在物理目录；状态栏/面包屑显示 `host:/inner/path`；归档内删除/重命名/移动入口禁用或明确提示"归档内只读"；损坏/加密/空归档可读错误，不 panic、不退出。
- **FIX-4 `o` 选择器居中**：跳转选择器界面在终端内水平垂直居中，尺寸按终端自适应并设上限；打开时背景列表被遮罩/清除，无残影；关闭后恢复原列表渲染，光标与滚动位置不变（80×24 与 160×48 两档验证）。

**新增（MUST）**

- **FEAT-1 鼠标支持**：crossterm 鼠标捕获启用；单击文件行仅移动光标（不触发打开）；对已聚焦行再次单击触发 `l`/`Enter` 对应的打开事件；双击直接触发打开；点击列表区域外不改变光标；滚轮上下滚动列表且光标不跳变；鼠标与键盘事件互不干扰；Visual/Command 等模式下行为按 SIMPLIFIED 定义，不 panic。
- **FEAT-2 列表顶部 `..`**：目录列表顶部固定显示 `..` 条目，视觉与普通目录项区分（如置灰/特殊图标）；光标在 `..` 上 `l`/`Enter` 跳上级目录；文件系统根目录 `..` 置灰或隐藏；归档根目录 `..` 行为与 `h` 一致：返回归档文件所在物理目录。
- **FEAT-3 `:file` 命令与右键菜单**：`:file` 显示当前选中项详情（路径、大小、权限、修改时间、类型；归档内成员显示 `host:/inner/path`），与 Ctrl-G 信息对话框同源；鼠标右键点击文件项：聚焦该项并弹出上下文菜单，至少含 Open、File Info (:file)、Copy、Move、Rename、Delete 六项；菜单项选择后执行对应既有动作（复用 exec 链，含撤销语义）；Esc 或点击菜单外区域关闭；关闭后 TUI 状态正常、无残影。
- **FEAT-4 `:help` 全屏帮助视图**：`:help` 打开独立的全屏帮助视图（对齐 vim/vifm help buffer，替代弹窗覆盖）；`j/k` 滚动、`/` 搜索（`n/N` 跳转）、`q`/`Esc` 关闭返回原列表且光标/滚动不变；帮助视图与目录列表滚动状态独立；内容从内嵌 help 文本（`include_str!`）加载，便于维护。

**SIMPLIFIED**

- FEAT-1：Visual 模式下单击更新光标位置（延伸/收缩选择），再击/双击打开仅在 Normal 模式生效；Command/Search 等输入模式下鼠标事件忽略；鼠标捕获默认开启，配置开关留待后续。
- FEAT-2：`..` 为渲染层虚拟条目，不进入 entries 数据与搜索/过滤/标记集合。
- FEAT-3：右键菜单为内建单级列表菜单（无子菜单）；Rename 复用既有输入行；菜单不含 frecency/主题等非文件动作。
- FEAT-4：帮助 `/` 搜索仅子串定位（`n/N` 循环），无高亮持久化；帮助内容单文件，不做多章节跳转。

**OUT_OF_SCOPE**

- 鼠标拖拽多选、右键拖拽、点击标签栏切换 tab（键盘 `gt`/`gT` 已覆盖）。
- Vim 式帮助多 buffer / 多标签帮助体系。

#### M14 — 双栏模型与标签栏（2026-11-15 增补；用户实测反馈 + vifm 官方模型）

**背景**：v0.4.0 实测（Linux/CachyOS）发现两处与 vifm 模型不符——①rvifm 右窗格为常驻预览，vifm 默认左右两个 pane 均为**目录列表**（可各自导航），w/:view 才把非活动 pane 切换为预览；②多标签功能已实现但无标签栏 UI，切换无可见反馈。

**MUST**
- 双 pane：左右各自持有独立 `Location`/条目列表/光标/滚动，均可导航（j/k/gg/G/l/h/Enter）；两栏均显示 `..`（根目录隐藏），`Tab` 键切换焦点，导航、键位与命令均作用于焦点 pane。
- `w` / `:view`：右 pane 在“目录列表”与“活动 pane 光标文件的预览视图”之间切换（预览内容跟随活动 pane 光标移动重置）；再按恢复。
- 标签栏 UI：显示每个 tab 的索引与目录名、当前 tab 高亮；`:tabnew`/`gt`/`gT` 切换有可见反馈。
- 每个标签内含完整双 pane 状态；多标签命令语义不变。

**SIMPLIFIED**
- 左右 pane 的普通模式键位及目录/文件命令按焦点 pane 分派，不得因 pane 在右侧而拒绝（包括 `/`、`o`/`:j`/`:J`、Visual/tag、`yy`/`p`/`P`、`:mk` 与归档成员复制）。操作受当前 Location 能力限制（如归档只读、远端能力白名单）时，按该能力边界提示；树视图、选择器、提权等专用交互的显示位置可沿用现有布局，但其路径来源/目标须遵循焦点 pane。
- 右 pane 无独立撤销语义：撤销恢复后双 pane 均刷新。

**OUT_OF_SCOPE**
- 超过两个 pane（vifm issue #1062）。

#### vifm 对齐增强（M13，2026-11-14 增补；依据：vifm 官方文档 + GitHub issues）

**第一批（低成本）**
- f/F/;/,：首字母快速搜索（按文件名首字符正向/反向移动，官方定义环绕列表首尾；;/, 为重复导航）。
- :select / :unselect：按 pattern 批量选择/取消（如 :select *.md），形态对齐官方 [range]select {pattern}；t 单文件切换语义不变（官方 t = select or unselect (tag)）。
- DD：永久删除——跳过应用内暂存，不可逆强提示（官方：like dd but always omits trash）。
- j/k 列表环绕（wrap-around）：光标到列表尾/首后继续移动则环绕（官方 issue #874）。
- Ctrl-G：文件详情对话框（名称/大小/权限/时间/路径等，比状态栏更全）。

**第二批（中成本）**
- cw 批量重命名：多选时生成名单文件并交 $EDITOR 编辑，应用后每文件可 u 逆（官方：cw 多选时 behaves as :rename without arguments；issues #515/#682）。
- :tree 树视图：当前目录为根的树形浏览，与文件系统状态和过滤器联动（官方：automatically kept in sync）。
- 后台作业 + :jobs：重操作（复制/移动）后台执行不阻塞 UI（官方：background command by adding " &"；issues #481 最高呼声/#210/#336）。
- view mode（w / :view）：切换**非活动窗格（右预览窗格）**为当前光标文件的完整内容视图（**非全屏**：保持双栏布局；j/k 在列表移动时视图跟随光标文件重置）；再次 w 或 :view 关闭并恢复摘要预览。用户澄清：vifm 默认 w 映射 :view，显示于对侧窗格、受窗格布局限制、光标保持在列表窗格。
- 暂存区菜单（对齐官方 :lstrash）：列出应用内暂存文件并可逐项恢复（对 u 逆序撤销的补充，纯导航语义）。
- 选择器与上述新命令均不触碰 frecency 时钟（只读红线不变）。

#### TUI 跳转选择器（2026-11-14 增补）

- 合一的 events 表与全局时钟（`global_tick` 序号）**保留**：统一视角 = 全部条目按全局序号 aging（dir + file 混合排名）。
- Normal 模式 `o`：打开**合一跳转选择器**——数据取合一表，统一视角排名；模糊查询实时筛选（分级与 `pick` 一致）；`j/k` 移动、`Enter` 选定、`Esc` 取消。
- `:j`：打开**仅目录**选择器（`topic = dir` 条目，独立 `dir_tick` 排名）；`:J`：**仅文件**选择器（`topic = file` 条目，独立 `file_tick` 排名）。交互与 `o` 一致。
- 选定语义（纯导航，不调系统 opener）：目录条目 → 当前 tab cd 到该目录；文件条目 → 切换到其父目录并光标定位该文件。
- 选择器**纯只读**：不写事件、不推进任何时钟（与 `pick` 同红线）。
- 1000–2000 条记录量级要求在内存中完成重放与排序，不引入 SQL。

#### 并发

- 多终端同时 `record` 是唯一真实并发场景。
- redb 短事务 + 有限重试（例如 3 次、每次退避 10ms）。
- 重试仍失败时降级追加到纯文本 `events.pending`，下次 `pick` / TUI 启动时合并。
- `record` 不得读取 `snapshot` 做评分，避免读-改-写长事务。

#### 模糊匹配与预览

- 优先使用 `skim` 库嵌入；`skim` 自带 bash/fish/zsh 集成脚本，可作为独立程序能力的参考。
- 若 `skim` 库 API 在 3 轮内无法满足验收，降级为 `nucleo` + 自绘列表。
- `pick` 的预览必须通过共享的 `PreviewRenderer`，不得通过 `--preview` 外部命令。

### 4.7 图标与主题

- 图标：`devicons`，默认 Dark，支持 Light；覆盖常见文件类型与特殊文件名（Dockerfile、Makefile 等）。
- 主题：`opaline`，TOML 定义，token 驱动，内置 39 套主题（另含 rvifm 捆绑 snowwhite，M34-2），支持运行时切换、深色/浅色变体、渐变，`zero unsafe`。
- 图标颜色与主题语义色协调；文件列表、预览、归档浏览共用同一套渲染。
- 终端不支持 Nerd Font 或设置 `NO_COLOR` 时，图标降级为纯文本，不 panic。
- 不实现 vifm 风格的图标插件或 `vifmrc` 注入；不嵌入 SVG 或字体文件。
- 主题文件可放在 `~/.config/rvifm/themes/` 下被自动发现。
- 主题数量优先时可替换为 `tui-nuances`（90 套、WCAG AA）；图标层与主题层解耦。

### 4.8 声明式键位

- 动作层：`Action` 枚举承载文件操作语义，例如：

```rust
enum Action {
    MoveCursor(Direction),
    Enter,
    GoParent,
    Yank,
    PutMove,
    PutCopy,
    Delete,
    Rename,
    Mkdir,
    Search,
    Tag,
    Visual,
    // 后续按需扩展
}
```

- 默认键位与 vifm 对齐：
  - `yy` → `Yank`
  - `P` → `PutMove`
  - `p` → `PutCopy`
  - `dd` → `Delete`
- 语义：
  - `yy` 后 `P` 将 yanked 文件移动到目标目录；
  - `yy` 后 `p` 将 yanked 文件复制到目标目录；
  - `dd` 后 `p` 将“删除”的文件移动到目标目录。
- `Yank` / `Delete` 把路径放入 yank 缓冲区，并记录缓冲区类型（copy / cut）；`PutMove` / `PutCopy` 消费缓冲区，实际执行移动还是复制由缓冲区类型决定。`dd` 的 `Delete` 同时把对象放入 cut 缓冲区，因此后续 `p` 按移动处理。
- 键位解析层负责多键序列与模式，不侵入导航/文件操作核心逻辑。
- 模式：Normal / Visual / View / Command-line；模式状态由 `rvifm` 持有。
- 多键序列支持前缀树与超时；超时行为可配置。
- 用户配置 TOML：

```toml
[normal]
prepend_keymap = [
    { on = "P", run = "put_move", desc = "移动 yanked 文件" },
]
append_keymap = [
    { on = ["g", "t"], run = "next_tab" },
]
```

- `prepend_keymap` 优先级高于默认；`append_keymap` 低于默认；`keymap` 完全替换默认。
- 不实现 Vim 寄存器、宏、`.` 重复、text objects、Vim 脚本映射。
- 只允许绑定到内置 Action，不开放用户自定义动作实现。

### 4.9 预览与文件打开

#### 统一预览

```rust
enum PreviewContent {
    Text { lines: Vec<Line<'static>> },
    Directory { entries: Vec<Entry> },
    Image { widget: StatefulImage },
    Empty,
}

trait PreviewRenderer {
    fn render(&self, path: &Path, area: Rect) -> PreviewContent;
}
```

- `PreviewRenderer` 由文件管理器与 `pick` 共用。
- 文本：`tree-sitter` + `tui-syntax` 高亮。
- 目录：复用文件管理器目录列表渲染逻辑，不响应光标移动。
- 图片：`ratatui-image` 的 `StatefulImage`。
- `pick` 的预览在同一进程内完成，不 `fork/exec` 外部命令。

#### 图片预览回退链

| 层级 | 协议/方式 | 适用条件 | 质量 |
|---|---|---|---|
| 第一级 | Kitty / iTerm2 / Sixel | 终端原生支持，`Picker` 探测成功 | 像素级或调色板位图 |
| 第二级 | Halfblocks（Unicode 半块） | **所有终端，无条件可用** | 字符画近似，但始终有图 |
| 第三级 | 外部字符画工具（可选） | 用户显式配置 | 依赖 `chafa` 等外部工具 |

- 启动时用 `Picker::from_query_stdio()` 探测终端支持的协议与单元格像素尺寸。
- 第二级必须内建且无条件可用；Alacritty 等无原生协议终端不得出现空白。
- 不把 Überzug++ / Chafa 作为编译期或运行期硬依赖。
- 不解决 tmux/Zellij 的 passthrough 协议问题；在文档中注明所需终端配置。

#### 文档预览

- **Office（内建，2026-11-11 定稿）**：编译期依赖 `office_oxide`，固定 `~0.1.16`（纯 Rust，MIT OR Apache-2.0，MSRV 1.88），支持 DOCX/XLSX/PPTX 的文本/Markdown 提取；零外部工具依赖。PPTX 的图表、SmartArt、组合形状、动画、OLE 等复杂对象跳过该 shape 继续渲染，不 panic；解析失败或提取为空时提示并允许 `Enter` 用系统 opener。
- **PDF（探测回退，2026-11-11 定稿）**：
  1. 文本提取：`pdf-inspector =1.25.2`（纯 Rust，MIT，MSRV 1.88）；
  2. 视觉预览：编译期启用 `pdf-visual` feature 时用 `exprim =0.4.0` 栅格化页面，输出 RGB 平面交给 `ratatui-image`，无外部工具依赖；
  3. `pdf-visual` 未启用或渲染失败时，探测 `pdftoppm`（Poppler）或 `mutool`（MuPDF）转图像；
  4. 均不可用时，显示“PDF 视觉预览需要启用 `pdf-visual` 或安装 Poppler/MuPDF”的友好提示，并允许 `Enter` 用系统 opener 打开。
- `exprim` 是非默认可选依赖：`exprim = { version = "=0.4.0", optional = true }`，仅在 `pdf-visual` feature 下编译；版本固定 + `Cargo.lock` 入库。
- 启用 `pdf-visual` 时的硬约束：导出像素上限限制在预览窗格级别（200 万–400 万像素）、限制输入 PDF 文件大小、release profile 保持 `panic = "unwind"`（exprim 依赖 `catch_unwind` 做错误隔离）。
- 不保证 PDF 的零配置视觉预览（默认构建只承诺文本提取）；这是已声明的边界。
- 不做 PDF/视频的内嵌完整解码。

#### 文件打开

- 默认层：系统 opener 兜底，零配置可用。
  - Linux：`gio open`（优先）→ `xdg-open`（回退）
  - macOS：`open`
  - Windows：`start`（或 `cmd /c start`）
- 目录在 `rvifm` 内部进入；文件才调用系统 opener。
- 用户覆盖层 TOML：

```toml
[opener]
preview_image = { run = "imv %s", desc = "用 imv 打开图片" }
edit_text = { run = "nvim %s", block = true, desc = "用 nvim 编辑" }

[open]
prepend_rules = [
    { mime = "image/*", use = ["preview_image"] },
    { mime = "text/*", use = ["edit_text", "open"] },
]
append_rules = [
    { name = "*.md", use = ["edit_text"] },
]
```

- `prepend_rules` 插入默认规则之前，优先级更高；`append_rules` 插入之后；`rules` 完全替换默认规则表。
- 规则支持 `mime` 与 `name` 匹配，按顺序取第一个命中；`mime` 优先，`name` 回退。
- opener 命令执行失败时有错误提示，不 panic。

### 4.10 平台与 CI

- 本地开发内循环：`cargo fmt` → `clippy -D warnings` → `build` → `test` → `commit`。
- 发布外循环：`push/tag` → CI 矩阵构建（Linux + Windows）→ 测试 → 归档产物 → 发布 Release。
- M0 就加入最小 CI：`ubuntu-latest` + `windows-latest` 跑 `fmt / clippy / build / test`。
- M2 完成（`u` 撤销闭环）后，CI 增加 `--release` 构建 + 产物归档。
- M6 发布前，加 tag 触发 Release 上传。
- 发布后验证：Windows 产物在 Win2016 手动冒烟；Linux 产物在 CI 或容器中 `--version` 冒烟。
- 不要求 macOS 进入首批发布矩阵；尽力支持即可。

## 5. 里程碑规格

### M0 — 骨架 + SPEC + 双平台 CI

**MUST**
- 初始化 Cargo 项目与目录结构。
- 搭好 `ratatui + crossterm` 事件循环。
- 最小可运行 TUI：显示当前目录列表、状态栏、`q` 退出。
- 固化 `SPEC.md` 的 MUST / SIMPLIFIED / OUT_OF_SCOPE 分级（本仓库已预置初稿）。
- GitHub Actions：`ubuntu-latest` + `windows-latest` 跑 `fmt / clippy / build / test`。
- `--version` / `--help` 冒烟测试。

**SIMPLIFIED**
- TUI 只显示当前目录，不做导航与文件操作。

**OUT_OF_SCOPE**
- 任何文件修改、归档、预览、frecency 能力。

### M1 — 核心浏览、多标签与 vim 按键

**MUST**
- 目录列表渲染、光标移动 `j/k`。
- 进入/返回目录 `l/h`、刷新。
- `gg/G`、`Ctrl-u/Ctrl-d`。
- 状态栏显示路径、选中项、文件大小/权限。
- 多标签：`:tabnew` / `:tabclose` / `gt` / `gT` / `N gt` / `:tabmove`。
- 每个 tab 独立持有 `Location`、光标、滚动、搜索状态。
- 基础搜索：`/` 与 `n/N`（M4 再扩展过滤能力）。
- `Tab` / `AppState` 结构在 M1 落地；撤销栈全局共享，`UndoableOp` 预留 tab 标识。

**SIMPLIFIED**
- 双栏布局（M14 起为默认，对齐 vifm）：左/右两个 pane 各自独立导航；M11–M13 时期的“单栏 + 常驻右预览窗格”模型由 M14 改造取代。

**OUT_OF_SCOPE**
- 文件修改、归档浏览（marks 由 frecency 实现，见 M25-2）。

### M2 — 文件操作与 `u` 撤销

**MUST**
- 复制、移动、删除、重命名、新建目录/文件。
- `u` 撤销：删除、重命名、移动、新建，按逆序撤销。
- 删除优先走可恢复路径（回收站或应用内暂存），保证 `u` 可逆。
- 连续操作后 `u` 按逆序逐条撤销。
- 不可逆操作有明确提示。
- 撤销栈记录 tab 标识；跨 tab 撤销能正确定位。

**SIMPLIFIED**
- 重做 `Ctrl-r` 可选，不阻塞 M2 验收。
- 批量操作在 M3 实现。

**OUT_OF_SCOPE**
- 归档内修改、永久删除不可恢复路径（除非用户显式确认不可撤销）。

### M3 — 选择、标记与批量操作

**MUST**
- `v` 可视选择。
- `t` 标记文件（tag）。
- 通过 `:tag` 命令对标记集合执行批量复制/移动/删除（如 `:tag copy` / `:tag move`）。
- 批量操作后标记状态清空。
- 明确区分 tag（文件标记）与 mark（路径标记）；mark 由 frecency 实现并持久化（M25-2），不得混用。

**SIMPLIFIED**
- 可视选择可先支持行选择，再扩展矩形/块选择。

**OUT_OF_SCOPE**
- 跨目录持久化 tag。

### M4 — 搜索、过滤与跳转

**MUST**
- 在 M1 基础搜索之上增强匹配能力（大小写、路径/内容匹配等，具体范围在 M4-1 定）。
- `n/N` 在基础搜索与增强搜索下行为一致。
- 简单 glob 过滤。
- 搜索链路保持可复用，供 M7 归档内搜索接入。

**SIMPLIFIED**
- 搜索可先做名称匹配，再扩展路径/内容匹配。

**OUT_OF_SCOPE**
- 持久化全文检索索引（`:grep` 为按需逐行扫描，不是 Elasticsearch 式倒排索引；后者不实现）。
- marks 独立存储模块（marks 由 frecency 实现并持久化于其 redb，见 M25-2）。

### M5 — 配置与最小命令模式

**MUST**
- 启动时读取 TOML 配置文件。
- 基本选项配置。
- 最小 `:` 命令子集：`:q`、`:cd`、`:mkdir`、`:touch`、`:help`。
- 多标签命令在 M1 已实现，M5 将其纳入统一命令解析。

**SIMPLIFIED**
- 键位覆盖不在 M5 实现，统一由 M10 的声明式键位系统接管。
- 配置用 TOML，不支持 Vim 脚本。

**OUT_OF_SCOPE**
- Vim 脚本、完整 vifm 命令兼容。

### M6 — 打磨与 v0.1.0 发布

**MUST**
- 错误处理、确认提示、帮助页。
- README 与使用说明。
- 大目录（>10k 项）可滚动、不阻塞退出。
- 打 tag `v0.1.0`。
- 发布门禁：CI 双平台全绿；两端 `--release` 构建成功并产出归档；Windows 产物在 Win2016 手动冒烟；Linux 产物 `--version` 通过。
- README 明确声明“参考 vifm，非严格兼容”。

**SIMPLIFIED**
- 性能优化先保证可用，不做极致调优。

**OUT_OF_SCOPE**
- M7–M11 的增强能力不阻塞 v0.1.0。

### M7 — 归档浏览（只读 VFS）

**MUST**
- `Location::Physical` / `Location::Archive { host, inner }` 抽象。
- `ArchiveFs` trait 与 `ZipFs` / `TarFs` 实现。
- `.zip` / `.tar` / `.tar.gz` 光标 `l`/`Enter` 进入只读虚拟目录。
- 归档内 `j/k/h/l`、`gg/G`、`/` 搜索、`n/N` 与物理目录一致。
- `h` 在归档根目录返回归档文件所在物理目录。
- 状态栏/面包屑显示 `host:/inner/path`。
- 打开归档建立成员索引；读取按需解码；内存占用受最大单成员约束。
- 错误处理：损坏、加密、空、超大归档索引不 panic。

**SIMPLIFIED**
- `.tar.xz` / `.tar.bz2` / `.tar.zst` 作为第二阶段支持。
- `.rar` 不支持虚拟浏览，给出解压提示（`.7z` 已由 M26-3 升级为虚拟浏览）。
- 可复用 `virtual-filesystem` 的 `ZipFS` / `TarFS`，但导航层仍需包一层 `Location`。

**OUT_OF_SCOPE**
- 归档内删除/重命名/移动。
- 嵌套归档递归浏览。
- FUSE / `archivemount`。
- 归档内修改进入撤销栈。

### M8 — 原生 frecency / bookmark 选择器

**MUST**
- 子命令 `rvifm pick` / `rvifm record`；stdin/stdout 契约与 fzf 同构。
- 存储 redb 三表：`events` / `snapshot` / `meta`。
- `record` 只追加事件、推进 `global_tick`，不做评分。
- `pick` / TUI 启动时通过 `snapshot + events` 重放评分。
- EMS + event clock；读时 aging。
- 分级匹配：书签 > 文件名 > 中间段 > 通用子串；内存排序。
- 书签：`keyword != ''` 作为分级标志，参与 EMS 评分；取消后正常衰减。
- `pick` 只读，不写事件；`record` 是唯一写入入口。
- 预览通过共享 `PreviewRenderer`。
- bash/fish 集成示例：仅在 `cd` 成功后调用 `record`。
- compaction 惰性触发或 `rvifm maintenance compact` 手动执行；无守护进程。
- 并发：短事务 + 有限重试；失败降级 `events.pending`。

**SIMPLIFIED**
- 模糊匹配优先 `skim` 库；3 轮不达标降级为 `nucleo` + 自绘 picker。
- 多进程 `record` 先采用 append-only 事件 + pending 兜底，不强制复杂并发控制。
- 不实现 SQLite SQL 兼容层。

**OUT_OF_SCOPE**
- fzf 的 `--preview` 外部命令模式。
- zoxide 的目录跳转语法。
- 后台守护进程、定时 compaction。

### M9 — 图标与主题

**MUST**
- 集成 `devicons`，文件列表渲染带图标和颜色。
- 集成 `opaline`，加载内置主题并替换硬编码 `Style`。
- 图标 `Theme::Dark` / `Theme::Light` 与主题明暗对齐。
- `PreviewRenderer` 复用同一图标/主题。
- 无 Nerd Font / `NO_COLOR` 环境下图标降级为纯文本，不 panic。
- 用户 TOML 主题发现与加载（`~/.config/rvifm/themes/`）。

**SIMPLIFIED**
- 主题选择器可先使用 `opaline` 自带 `ThemeSelector`，不做完整设置界面。
- 主题数量优先时可换 `tui-nuances`。

**OUT_OF_SCOPE**
- vifm 风格图标插件或 `vifmrc` 注入。
- 嵌入 SVG 或字体文件。
- 自定义图标映射用户脚本。
- 非 Nerd Font 的图片图标渲染（Sixel/Kitty 图像与文件图标是不同层级）。

### M10 — 声明式键位系统

**MUST**
- `Action` 枚举承载文件操作语义，键位解析与动作执行分离。
- 多键序列（`yy`、`gg`、`gt` 等）与可配置超时。
- 模式：Normal / Visual / View / Command-line；模式状态由 `rvifm` 持有。
- 默认键位与 vifm 对齐：`yy` → `Yank`，`P` → `PutMove`，`p` → `PutCopy`，`dd` → `Delete`。
- `PutMove` / `PutCopy` 为独立 Action。
- TOML 分层：`prepend_keymap` / `append_keymap` / `keymap`。
- 默认表与用户表分层合并：prepend 高于默认，append 低于默认。
- 评估 `keymap-rs`（`keymap-core` + `keymap-seq` + `keymap-config`）；不达标时降级为轻量自写 Trie。

**SIMPLIFIED**
- 不实现 Vim 寄存器、宏、`.` 重复、text objects。
- 不嵌入完整 Vim 引擎。
- 用户只能绑定到内置 Action。

**OUT_OF_SCOPE**
- Vim 脚本映射（`:map` 的完整 RHS 表达式）。
- 用户自定义动作实现。

### M11 — 预览与文件打开

**MUST**
- 图片预览使用 `ratatui-image`，启动时 `Picker::from_query_stdio()` 探测最佳协议。
- 回退链：原生协议 → Halfblocks（无条件可用）→ 可选外部字符画（用户显式配置）。
- 文本预览复用 `PreviewRenderer`，支持 `tree-sitter` 语法高亮。
- 文件打开默认走系统 opener：Linux `gio open` → `xdg-open`；macOS `open`；Windows `start`。
- 目录在 `rvifm` 内部进入，文件才调用系统 opener。
- 用户配置 TOML 分层：`prepend_rules` / `append_rules` / `rules`。
- opener 规则支持 `mime` 和 `name` 匹配，按顺序取第一个命中。
- Office：编译期依赖纯 Rust 解析库，支持 DOCX/XLSX/PPTX 文本/Markdown 提取，零外部工具依赖。
- PDF：按“`pdf-inspector` 文本提取 → `exprim` 栅格化（仅 `pdf-visual` feature）→ `pdftoppm`/`mutool` → 回退提示/系统 opener”顺序探测。
- 可选 `pdf-visual` feature：`exprim = { version = "=0.4.0", optional = true }`；非默认依赖，关闭 feature 时依赖树不含 exprim。
- 启用 `pdf-visual` 时的护栏：导出像素上限、输入文件大小上限、`panic = "unwind"`、CJK/加密/损坏 PDF 回归语料。
- `pick` 的预览与 TUI 预览对同一文件渲染结果一致。

**SIMPLIFIED**
- 不内置 Überzug++ / Chafa；它们仅作为用户可选外部 opener。
- 不解决 tmux/Zellij 的 passthrough 协议问题；文档注明所需终端配置。
- 不做 PDF/视频的内嵌完整解码；PDF 视觉预览走可选纯 Rust 后端（`pdf-visual` feature）或可选外部工具。
- Office 只提取文本/Markdown，不保证标题层级、列表样式等格式还原。

**OUT_OF_SCOPE**
- 内嵌视频播放。
- PDF 渲染引擎作为默认/必需依赖（`tdf` / `pdfium-render` 完全禁止；`exprim` 仅限可选 `pdf-visual` feature）。
- 保证 PDF 零配置视觉预览（默认构建只承诺文本提取）。
- 终端图形协议的底层实现（由 `ratatui-image` 和终端负责）。

### M12 — 权限不足时的内联提权（2026-11-12 增补）

**MUST**
- 文件操作（复制/移动/删除/重命名）因权限失败时，TUI 内弹出 sudo 密码提示。
- 使用 `sudo -A` + `rvifm --askpass` 辅助模式实现，不启动新的 rvifm 实例。
- 密码提示可取消，取消后操作中止，TUI 状态与光标不变。
- 提权仅作用于**当前单个操作**，不将整个 rvifm 进程提升为 root。

**SIMPLIFIED**
- 优先支持 Linux/macOS；Windows 的提权行为尽力而为（UAC 机制不同）。
- 可复用 `sudo_prompt` crate 作为 askpass 的轻量封装。

**OUT_OF_SCOPE**
- 不实现"重启一个 root rvifm 实例"的模式（vifm 的 `:!sudo vifm %c` 路线）。
- 不将整个文件管理器进程以 root 身份运行。

#### AUDIT — 评估报告整改（2026-11-19 增补；第二轮复核扩充至 17 项；见 docs/REVIEW.md，已确认执行）

**硬约束**：整改不得削弱 AGENTS §5 已定红线；`pick` 只读性优先于 `events.pending` 合并便利；文档声称的能力要么接线、要么降级表述，不得两头并存。

**MUST**
- **AUDIT-1 pick 只读性**：`pick` 路径不得写 redb、不得推进任何 tick；`events.pending` 合并移出 pick 路径或提供只读 open。与“下次启动合并”的冲突决策入 NOTES §8。
- **AUDIT-2 PDF 视觉探测链（最小接线）**：按 `pdf-inspector` 文本提取 → `exprim`（仅 pdf-visual）→ `pdftoppm`/`mutool` → 提示 + opener 接线，或明确文档降级；本任务先接通第 1 页，前 1–2 页与 `prefer_image` 配置由 M27-3 扩展。
- **AUDIT-3 pick 预览同源**：pick 接入 `PreviewRenderer`，或从 SPEC/README 删除“同源”承诺。
- **AUDIT-4 惰性 compaction**：TUI 启动时按阈值（默认 5000 条）触发写 snapshot + 截断 events；`pick` 不触发（只读）；原“读方启动”表述已按 AUDIT-1 修正为“TUI 启动”。
- **AUDIT-5 app.rs 拆分**：按 keys/exec/render/jobs/preview 分片，测试随迁，单文件生产代码降到可维护规模。
- **AUDIT-6 键位超时**：多键序列支持可配置超时。
- **AUDIT-7 opener 分层**：整层接线——读取 opener TOML（`prepend_rules` / `append_rules` / `rules`）、支持 `mime` 与 `name` 匹配、执行 `use`/`run` 命令；当前 `OpenerConfig` 无 TOML 来源、无 mime、无 `rules`、无执行器，仅测试可达。
- **AUDIT-8 错误上浮**：清理生产 `unwrap/expect/unreachable`，刷新/会话/known_hosts 权限失败不得静默。
- **AUDIT-9 CI 增强**：clippy 覆盖 all-targets；将 AGENTS §6 现为本地门禁的 Linux target cross-check 纳入 CI（非现行规范违反）。
- **AUDIT-10 重复实现统一**：glob 匹配与配置失败回退各保留一份实现；修复 `opener::glob_match_simple` 对 `*x*` 的匹配缺陷（`strip_prefix('*')` 分支先返回导致双星模式失效）。
- **AUDIT-11 帮助与 README 同步**：`help.txt` 补齐 `:find`/`:grep` 等已实现命令；README 修正 keymap 配置路径（`config.toml` 的 `[normal]`，非 `keymap.toml`）、marks/主题/opener 的实施状态、致谢去 `skim`。
- **AUDIT-12 测试补强**：`keymap`/`state`/`session`/CLI 子命令分派补测，降低 `app.rs` 测试占比。
- **AUDIT-13 主题选择接线**：`theme::set_builtin` / 用户主题发现与加载必须有生产调用方（`:theme` 命令或 `[ui] theme` 配置）；切换后列表/预览/归档共用样式立即生效；图标 Dark/Light 跟随主题 variant；无主题文件时默认 Dark 且不 panic。
- **AUDIT-14 测试环境隔离**：测试不得由宿主的 `TMUX` / `STY` 等环境变量改变结果；`m18_4_bang_routes_and_config` 在 tmux 内、外均须稳定通过，且不向测试 stdout 泄漏外部命令输出。
- **AUDIT-15 pick CLI 契约**：支持 `--topic dir|file`（与 `record` 对称）；候选按对应 topic 评分；数据目录缺失时 `pick` 不得以裸 `NotFound` 失败（空评分或只读打开）；与 AUDIT-3 的预览承诺统一。
- **AUDIT-16 frecency 一致性**：`cmd_pick` 初始化 `[frecency]` 设置；`compaction` 使用 per-topic λ（不得硬编码 `AGING_LAMBDA`），与实时查询结果一致。
- **AUDIT-17 归档读取上限**：单成员读取与 tar.xz 整体解压设置体积上限；超限返回可读错误，不 OOM、不 panic。

**SIMPLIFIED**
- 整改按 P0 → P1 → P2 分批提交；AUDIT-1、AUDIT-14 与 AUDIT-9 优先。
- 不要求一次性完成全部 17 项，但每项必须独立可验收。
- 主题接线仅要求“可选择 + 可加载 + 即时生效”，不要求完整设置界面（沿用 `opaline` 内置列表即可）。

**OUT_OF_SCOPE**
- 不为整改引入新依赖或重写 TUI 框架。

#### M35 — 文档与发布一致性（2026-11-21 增补；来源：评估复核报告蒸馏，用户确认执行）

**MUST**
- **M35-1 状态漂移清零**：AGENTS §10（当前阶段/任务/基线指向 STATE 实际值）、SPEC 7z/rar 三处（已随本蒸馏修正）、STATE"执行中/全部完成"矛盾、TASKS M34 段标题、PLAN 0.10 发布口径、README"发布中"句——六处全部与代码实际一致。
- **M35-2 harness 一致性脚本**：`scripts/harness_check.py`（CI + 可 pre-commit）至少七条规则——STATE 批次关闭后不得写"执行中"；AGENTS §10 与 STATE 一致；TASKS 批次标题与勾选状态一致；PLAN 版本小节标题与正文一致；SPEC SIMPLIFIED/OUT_OF_SCOPE 与 README 能力表不冲突（人工白名单）；文档测试数字 = `grep -c '#[test]'`；README 版本状态与 STATE 发布状态一致。
- **M35-3 REVIEW 归档**：旧报告移入 `docs/reviews/REVIEW-<日期>.md`；`REVIEW.md` 只保留当前结论 + 历史索引。
- **M35-4 CHANGELOG**：`CHANGELOG.md` 补 0.8.0 → 0.11.0 用户可见变化；说明 0.9.0/0.10.0 跳过（用户决策）。
- **M35-5 依赖与计数卫生**：移除零引用 `tokio-stream`；清 `config.rs` `DBG parsed` 调试输出；文档测试数字由脚本校验。

**SIMPLIFIED**
- harness 脚本七条规则允许"人工白名单"处理无法自动判定的一致性项；pre-commit 只做示例说明（不强制安装）。

**OUT_OF_SCOPE**
- 不做全量文档自动生成（标题/表格手写，脚本只校验）。

#### M36 — 传输引擎（2026-11-21 增补；来源：评估复核报告蒸馏，用户确认执行）

**硬约束**：撤销栈语义不变（成品入栈）；临时文件（`.part`/临时副本）失败可清理或明确标记；不新增重量级依赖（时间戳/权限用 std `File::set_times`/`set_permissions`）；`RemoteFs` 现有 `read`/`write` 签名不变（范围读写为默认实现的增量方法）。

**MUST**
- **M36-1 `copy_tree` 重写**：本地复制保留 mode/mtime/symlink（`symlink_metadata` 判型，链接不解引用）；分块读写；进度回调（接 `JobProgress`）；支持取消；与 `copy_to`/`move_to` 接线。
- **M36-2 远端下载单次读取**：消除"成功路径读两遍"；`read` → 本地临时文件 → 校验 → 原子 rename；失败清理临时文件；保留远端 mtime（`metadata` 先查）。
- **M36-3 `RemoteFs` 范围读写**：`read_range(path, offset, len)` / `write_range(path, offset, data)` 默认实现 = `read`+截拼/整写（兼容 mock 与现有后端）；SftpBackend 覆盖为真范围调用。
- **M36-4 `Transfer` 状态机**：Pending/Running/Paused/Done/Failed/Cancelled 统一状态 + 进度；复用 `Job`/`JobProgress`/`JobControls`；`paste_yanked*` 三方向（本地/上传/下载）路由到 Transfer。
- **M36-5 冲突策略**：Ask / Overwrite / Skip / RenameAuto / KeepNewer；策略为纯函数；"应用到全部"；Ask 为覆盖确认对话框扩展。

- **M36-6 偏移续传**：`.part` + `.part.meta`（offset/size/mtime/分块哈希）；`write_range` 续写；完成原子 rename；取消清理；哈希不一致从头。
- **M36-7 传输队列 `:queue`**：多选入队、顺序执行、全部暂停/恢复/取消、失败重试；TUI 退出可选持久化（`session.toml` 同目录）。
- **M36-8 递归远端目录**：list walk + `create_dir` + 逐文件 Transfer；链接策略（跟随/跳过）；三方向（本地↔远端、远端↔远端经本地中转）。

**SIMPLIFIED**
- 冲突策略"应用到全部"按 (方向+策略) 一次记忆，不做跨会话持久。
- 队列持久化仅在 `session.queue = true` 时启用（缺省关）。
- 远端↔远端经本地中转（不做服务器端 copy）。

**OUT_OF_SCOPE**
- 带宽限制、并行多连接传输。
- rsync 协议兼容。

#### M37 — 文件运维（2026-11-21 增补；来源：评估复核报告蒸馏，用户确认执行）

**硬约束**：全部入撤销栈或明确不可逆提示；`:extract` 是"从归档读出写到磁盘"（不写归档内成员——OUT_OF_SCOPE 边界不变）；zip-slip 防护（路径规范化拒绝 `..`/绝对路径成员）。

**MUST**
- **M37-1 `:chmod <mode>`**：八进制/符号模式；作用于光标/tags/v 选集；`-R` 递归；批量失败逐项报告；POSIX only（Windows 给可读"不支持"）。
- **M37-2 `:extract [目标]`**：归档内选中项/全部解压到目标目录（**M45-7 裁决推翻旧默认**：缺省 = 对侧物理 pane；`:extract left|right|当前|对侧|绝对路径` 指定侧/路径，`left`/`right` 取对应 pane 的物理目录，左右栏都可调用）；zip-slip 防护；单成员 64MB 上限沿用护栏；冲突走 M36-5 策略；后台 job 化；逐条入撤销栈（`u` 可逆）。**M45-7 追加**：归档虚拟目录内 `yy` 标记成员后 `p`/`P` 粘贴到焦点 pane 物理目录 = 按成员解压（推翻 NOTES 2026-11-21「归档 yy 不支持」决策，仍不写归档内）。
- **M37-3 `:sync left→right|right→left`**：基于 `:compare` 结果的镜像同步；三策略（仅复制缺失/覆盖较新/镜像含删除）；删除需显式 `--delete` 开关 + 确认；先 dry-run 预览再应用。
- **M37-4 `:du`**：并行遍历当前目录；按大小排序虚拟视图（目录聚合 + 总计）；可取消；万条上限。
- **M37-5 `:dup`**：三段查重（size → 头部哈希 → 全文 SHA-256）；硬链接识别（inode）；结果虚拟视图可 `dd`/`:tag`。
- **M37-6 `:checksum [md5|sha256]`**：生成 sidecar（`md5sum -c` 兼容）/ 校验已有 / 批量比对；并行哈希 + 进度。
- **M37-7 暂存区增强**：容量与条目数配置、`:trash` 清空/按时间清理、恢复到指定路径。

**SIMPLIFIED**
- `:extract` 首期 zip/tar/tar.gz/7z（与浏览同表）；`.rar` 保持解压提示。
- `:du` 不做可视化 treemap（列表排序足够）。
- `:sync` 不做双向合并。

**OUT_OF_SCOPE**
- 写入归档内成员（不变）；`.rar` 解压（无纯 Rust 解码器，保持提示）。

#### M38 — 检索与 shell 集成（2026-11-21 增补；来源：评估复核报告蒸馏，用户确认执行）

**MUST**
- **M38-1 检索结果可操作**：`:find`/`:grep` 结果视图 `yy`/`dd`/`p`/`:tag`/`:rename`/`:chmod`（M37-1 后）映射回真实路径；跨目录去重；只读条目（无对应文件）拒绝写并提示。
- **M38-2 `pick` 兼容面**：`--query`、`--multi`、`--print0`、`--preview`、`--height`、`--no-sort`、`--exit-0`、`--select-1`（覆盖 fzf 常用子集；未知参数可读错误）。
- **M38-3 `rvifm init <shell>`**：bash/zsh/fish/powershell 输出初始化脚本（`cd` 后 record、快捷键调 pick）；shell hook 示例文档同步。
- **M38-4 frecency 管理**：`:frecency stats`（条目数/占用/各 topic 统计）、`export <file>`（JSON）、`purge`（全清，确认）、`prune --before <date>`（按时间清理）。
- **M38-5 `:find` 过滤器**：`--size`、`--mtime`、`--type`、`--depth` 参数 + 命名搜索（`:find save <名>` / `:find load <名>`，会话内）。

**SIMPLIFIED**
- `pick --multi` 输出以换行分隔（`--print0` 才是 NUL）。
- 命名搜索仅内存（不跨会话）。

**OUT_OF_SCOPE**
- `pick` 完整 fzf 兼容（模糊算法、布局选项）。
- shell 集成自动安装（init 只输出，不写用户 rc 文件）。

#### M39 — 预览与平台（2026-11-21 增补；来源：评估复核报告蒸馏，用户确认执行）

**MUST**
- **M39-1 十六进制查看器**：view 模式二进制文件自动进入 hex；偏移跳转（`:goto`）、ASCII 侧栏、分块读取（1MB 窗口）。
- **M39-2 预览扩展**：CSV/TSV 表格对齐预览；PDF 翻页（`[`/`]`，基于 pdf_image_pages 机制扩展到任意页）；图片 EXIF 摘要（尺寸/格式，不引依赖——解析文件头）。
- **M39-3 通用 zip 容器**：`.docx`/`.xlsx`/`.pptx`/`.apk`/`.war` 只读虚拟浏览（魔数 PK + 扩展名白名单；`.docx` 等已有 Office 提取，浏览为增量）。
- **M39-4 SFTP 硬化**：首次指纹确认（`strict_host_key = ask|accept|no`，缺省 ask——TOFU 改为交互确认）；ProxyJump（ssh config 解析透传）；ssh-agent 认证；keepalive。
- **M39-5 Windows UAC**：`ShellExecuteExW("runas")` 操作级提权（EACCES → UAC 弹窗）；失败/取消状态不变；不提升主进程。
- **M39-6 标签与布局**：命名标签（`:tabname <名>`）；标签溢出滚动；`[ui] layout = single|dual|preview-right|preview-bottom`。

**SIMPLIFIED**
- hex 查看器不做编辑。
- EXIF 只做容器级摘要（不做 EXIF 全字段）。
- ProxyJump 只透传 ssh config 已配置项（不做 UI 配置）。

**OUT_OF_SCOPE**
- hex 编辑、CSV 编辑。
- Windows UAC 主进程提升（不变）。

#### M40 — 1.0 稳定化（2026-11-21 增补；来源：评估复核报告蒸馏，用户确认执行）

**MUST**
- **M40-1 性能基准**：criterion 基准集——冷启动、10 万项目录、自然排序、frecency 重放 10 万事件、1GB 复制吞吐；结果入 `docs/bench.md` 基线。
- **M40-2 模糊测试**：cargo-fuzz 目标——zip/tar/7z 解析、路径规范化、EntryRecord 反序列化；24h 无 crash 记录入 NOTES。
- **M40-3 属性测试**：proptest——glob 匹配、自然排序、撤销栈逆序恢复。
- **M40-4 覆盖率**：cargo-llvm-cov；核心模块（ops/remote/frecency/archive/keymap）≥ 75%。
- **M40-5 供应链**：cargo-deny + cargo-audit（CI job）；SBOM 生成；Release 归档 SHA256 校验文件。
- **M40-6 平台**：macOS CI job（test + clippy）；aarch64 Linux release 归档。
- **M40-7 1.0 发布**：CHANGELOG 完整、版本 1.0.0、crates.io 发布、发布说明含迁移提示。

**SIMPLIFIED**
- fuzz 时长 CI 内 10 分钟（24h 本地跑一次记录）。
- Win2016 冒烟保持人工清单（无自动机）。

**OUT_OF_SCOPE**
- 覆盖率目标不含 tests.rs。
- 文档站（GUIDE.md 已覆盖首版需求）。

#### M41 — 右栏一致性与交互缺陷修复（2026-11-21 增补；来源：用户 0.11.x 实测反馈，优先级排高）

**MUST**
- **M41-1 右 pane 命令模式与标签切换**：右 pane 焦点下 `:` 进入命令模式，输入的内部命令作用于焦点 pane（沿用 focus_base 既有语义）；`gt`/`gT`/`Ngt` 切换标签，语义与计数行为与左 pane 完全一致。普通模式键位与命令按焦点 pane 分派；文件操作依 Location 能力判定。实现语义升格：M14-1 的右 pane 单键 `g` 直跳顶部退役，统一为 `gg` 序列（经待决序列机制，与左 pane 一致；未知第二键整体丢弃）。
- **M41-2 方向键导航**：Normal 模式 `↑/↓/←/→` 等价 `k/j/h/l` 对应动作（左 pane 经默认 keymap 声明式注册，用户 TOML 三层覆盖规则对其生效；右 pane 处理器同步支持四键）。输入模式（命令行/搜索/对话框）内方向键既有编辑语义不变。
- **M41-3 鼠标点击焦点与高亮一致**：单击任一 pane 的条目行 → 焦点切换到该 pane 且该 pane 光标移至命中条目（CurrLine/OtherLine 样式随焦点呈现）；已聚焦行再次单击 = 打开（**M45-6 裁决改规格**：右 pane 打开 = 进入目录语义，物理目录与归档虚拟目录均可进入，文件仍 no-op）；滚轮滚动作用于焦点 pane。左右双侧表现一致。
- **M41-4 视口滚动钉底修复**：列表视口偏移改为有状态——仅当光标越出可见窗口（offset ≤ cursor < offset+height）时才滚动；光标自视口底行按 `k` 上移时列表保持不动、高亮上移一行。渲染（ListState offset）与鼠标命中测试共用同一偏移推导，不得分叉。

**SIMPLIFIED**
- 右键上下文菜单保持左 pane；键盘与命令的目标仍按焦点 pane 判定。
- 右 pane 打开语义沿用 M45-6 裁决（物理目录与归档虚拟目录可进入，文件 no-op）。
- 方向键不新增 Action，复用既有动作（CursorUp/CursorDown/ReturnParent/EnterSelected）。

**OUT_OF_SCOPE**
- 鼠标右键菜单在右 pane 的入口。
- 鼠标拖拽选择、拖拽滚动。

## M42 — :tutor 导师模式（2026-11-22 增补；用户蒸馏 Qwen 讨论稿·已确认，见 NOTES 同日条目）

> 目标：把人工验证与新手上手合并为 TUI 内交互模式——既是教程也是自验清单
> 执行器。单一模式（学习/测试双模式已裁剪）；无外部报告文件；进度本地持久化
> + 错题本。顺序：1（框架/UI）→ 2（数据/错题本）→ 3（内容）→ 4（文档收尾）。

**MUST**
- **M42-1 框架与 UI**：`:tutor` 命令（无参 → 模块列表覆盖层；`:tutor <module_id>` → 直接进入该模块）；底部约 1/4 Tutor Panel（左侧 prompt + expected 多行渲染，右侧进度 n/m 与按键提示）；Tutor 模式（InputMode::Tutor）拦截全局按键，仅响应 y（掌握/下一项）、n（未掌握记错题本/下一项）、q 与 Esc（退出并保存进度）；主 TUI 背景保持可见作为操作上下文。
- **M42-2 数据与错题本**：`TutorModule`/`TutorItem` 数据结构（`assets/tutor/*.toml`，`include_str!` 编译期内嵌——先例 help.txt/BUNDLED_THEMES）；进度持久化 `data_dir()/tutor_progress.toml`（复用 M8-3 既有数据目录路径）；再次进入模块时未掌握项优先复习（Panel 提示"优先复习未掌握项"）；`:tutor reset <module_id>` 清空该模块进度。
- **M42-3 首批内容**：core_nav（j/k、h/l、gg/G、Tab 焦点切换）、file_ops（yy、p、dd、u、:rename）、search_frecency（/、=、o、:recent）三模块，每模块 5–10 项，条目含 prompt（操作指引）与 expected（预期结果），语义与实际键位/命令一致。
- **M42-4 文档收尾**：GUIDE.md/README 增"新手推荐：`:tutor` 快速上手"指引；ACCEPTANCE §42 逐条勾选；批次文档同步（SPEC/PLAN/TASKS/STATE）；模块结算 Summary 在 Panel 内呈现（掌握/未掌握计数），不生成外部文件。

**SIMPLIFIED**
- Tutor 模式键位硬编码（y/n/q/Esc），不进 ModeLayers 用户映射（M33 三层覆盖不适用此模式）。
- 无报告文件生成（tutor_report 废弃）；无学习/测试双模式；无间隔重复调度（错题本 = skipped 项优先的简单队列）。
- 进度文件损坏静默降级为全新进度（沿用 session.toml 容错模式）。
- 模块内条目线性顺序执行，不跨模块混合调度。

**OUT_OF_SCOPE**
- 外部报告/导出文件；学习/测试双模式切换；Anki 式间隔重复；多语言内容；
  Tutor 键位用户映射；Agent 自动读取验证结果（结果供人看，不进自动化）。

## M43 — tutor 全功能覆盖与放行重构（2026-11-22 增补；用户裁决·已确认，见 NOTES 同日条目）

> 目标：tutor 扩为全功能键位/命令手册（13 模块 ~85 项，逐键粒度——用户裁决
> 问题 5 全量），并重构键位交互为透明放行（边学边练）。M42 首批 23 项保留
> 并入。顺序：1（放行重构）→ 2（数据模型）→ 3/4/5（内容三批）→ 6（文档同步
> + 维护规则）。

**MUST**
- **M43-1 透明放行重构**：TutorSession 从 InputMode 迁移为 **AppState.tutor
  覆盖层状态**（不再占用 InputMode——命令/帮助/确认等任意模式切换不丢会话）；
  会话激活时按键照常进入 Normal 处理器（j/dd/:命令等可实际执行，Panel 保持
  显示）；标记键前置拦截：**Ctrl+Y 掌握 / Ctrl+N 未掌握（错误题本）/
  Ctrl+Q 退出并保存**（Ctrl 组合当前全部空闲，已核实；Esc 放行保留清搜索/
  过滤语义）；结算态 Summary 持续显示至 Ctrl+Q。模块列表覆盖层改 List 组件
  渲染（13 模块超屏时视口跟随）。
- **M43-2 数据模型扩展**：TutorItem 增可选 `requires` 字段（逗号分隔 token：
  editor|sudo|sftp|git|osc52|graphic|tmux）；条目含 requires 时 Panel 左列
  显示「环境依赖（…），无此环境可 Ctrl+N 跳过」提示行；不做环境探测、不过滤
  条目。
- **M43-3 内容批 1**：search_filter（`/`·n·N·`=`·f/F·;/,）+ visual_tag
  （v/V/t/:select/:unselect/:tag copy·move·delete）+ marks_frecency
  （o/:j/:J/:recent/m·'/ :bmark 族/:keyword/:cmdmark·'展开）+ tabs_history
  （:tabnew/:tabclose/:tabmove/gt·gT·Ngt/:tabname/:histprev·next/:history/
  跨 tab u）。
- **M43-4 内容批 2**：panes_sort（w/:view/:sort/S/:compare/dp/do/:tree/
  [ui] layout）+ archive_ops（归档进入/:archive/:extract/校验文件）+
  maintenance（:chmod/:du/:dup/:checksum/:sync/:trash/:jobs/:queue）。
- **M43-5 内容批 3**：command_line（: 模式 ↑↓ 历史/Tab 补全/'展开/:cd/:mk/
  :mkdir/:touch/:copy/:move/:file/Ctrl-G/:theme/:help/:q）+ clipboard_git
  （yn/yf/yc/:gi/:gitui/git 列）+ remote_sftp（:remote/:mk/远端写/.part）+
  tutor_meta（:tutor reset/错题本用法）+ core_nav 补 Space。
- **M43-6 文档同步与维护规则**：GUIDE/help/README/CHANGELOG 更新；**tutor
  条目维护规则固化**（此后批次新增键位/命令时同步补 tutor 条目——批次验收
  条约束，不进 harness_check 自动检查）；批次文档同步。

**SIMPLIFIED**
- 危险/不可逆命令条目（:frecency purge confirm/:trash purge/:sync --apply/
  DD 等）prompt 标注「了解即可」，不要求真实执行；file_ops/maintenance 模块
  首条目引导先建 sandbox 目录（`:mkdir tutor-sandbox` + `:cd`）；GUIDE 注明
  建议在临时目录跑 tutor（写路径 sandbox 重定向不做）。
- 标记键硬编码 Ctrl 组合（同 M42，不进 ModeLayers）；requires 不做环境探测；
  条目不过滤；自评为用户自证（非自动化验收，ACCEPTANCE 不受影响）。

**OUT_OF_SCOPE**
- 写路径 sandbox 重定向；环境自动探测；逐条自动判定对错；学习/测试双模式；
  间隔重复调度；报告文件。

## M44 — 预览焦点修复（2026-11-23 增补；用户指令，见 NOTES 同日条目）

> 目标：修复 `w` / `:view` 预览模式的焦点行为——开启预览后焦点留在原 pane，
> 原 pane 的 `j/k` 继续切换文件、对侧预览自动跟随；`Tab` 显式进入预览窗格
> 滚动/搜索，再 `Tab` 返回；关闭预览焦点不变。

**MUST**
- **M44-1 焦点与跟随**：`w` / `:view` 开关预览不得改写 `Tab.focus`；`Tab.preview_focus` 仅在预览窗格内为 true。焦点在原 pane 时 `j/k`、`gg/G`、`Ctrl-u/d`、目录切换与搜索结果移动必须使对侧预览同步到当前光标文件（同一文件保留滚动，换文件重置到顶部/命中行）；预览窗格内 `Tab` 返回原 pane，`w`/`Esc`/`q` 关闭预览并恢复 normal 键位表。
- **M44-2 验收与文档**：ACCEPTANCE §44 逐项通过；GUIDE/CHANGELOG/README/harness 文档同步；CI 三平台绿后随 1.1.0 发布（crates.io 不发布，用户 2026-11-23 指令）。

**SIMPLIFIED**
- 预览跟随在每帧绘制前统一刷新（不做文件监听）；同一路径的外部修改不主动重载，仅光标换文件时重建。
- 预览焦点是 `Tab` 上的布尔状态，不扩展 `Focus` 枚举，避免影响既有左右 pane 语义。

**OUT_OF_SCOPE**
- 鼠标在预览窗格内滚动/搜索；预览窗格独立历史；文件内容热更新监听。

## REL — 发布流水线解耦（2026-11-22 增补；同日用户裁决修订：门禁简化为人工发起，见 NOTES）

> 背景：GitHub Release（二进制归档）可逆可覆盖；crates.io 版本号发布后不可
> 回收。据此两通道解耦——自动化止步于 GitHub，crates.io 仅由人工发起。

**MUST**
- **REL-1 发布两通道解耦**：tag `v*` 推送 → CI（test×3 + fuzz + supply-chain）
  全绿 → **GitHub Release 自动发布**（四平台归档 + SHA256SUMS，既有 ci.yml
  release job 承担），此段无需人工干预；**crates.io publish 仅由人工发起**
  ——何时发布由人决定（2026-11-22 裁决撤销清单门：原 RELEASE_CHECKLIST.md
  删除，人工验证方式改为 `:tutor` 自行验证（随 M42 落地），测试人员知悉
  即可，无强制门禁文档）。
- **REL-2 人工验证形态**：`:tutor`（M42）既是新手教程也是自验清单执行器；
  发布前是否跑、跑哪些模块由人工自行判断；结果不生成外部文件（进度仅本地
  持久化，见 M42）。

**SIMPLIFIED**
- crates.io 在持凭据机器人工执行（凭据可随环境配置，见 NOTES 2026-11-22
  环境事实修正）。
- 历史清单（ACCEPTANCE §16 v0.6.0、§35-A 0.11.0、RELEASE_CHECKLIST v1.0.0）
  全部退役：当前入口为 :tutor，历史原文见 git 历史。

**OUT_OF_SCOPE**
- 自动化 UI 回归（截图比对/UI 测试框架）。
- crates.io 自动 publish（任何形式）。
- 强制门禁文档/勾选清单（RELEASE_CHECKLIST.md 模式废弃）。

## REL-3 — SHA256SUMS.txt 完整性（2026-11-23 增补；维护期发布修复，见 NOTES 同日条目）

> 来源：用户 2026-11-23 验证 v1.1.0 GitHub Release 发现——`SHA256SUMS.txt`
> 只含 1 个归档（v1.1.0 仅 Windows 行、v1.0.0 仅 aarch64 行）；根因为 4 个
> 并行 release job 各自 `cd dist && sha256sum *.tar.gz > SHA256SUMS.txt`
> （其 `dist/` 只含本 target 归档）再 `gh release upload ... --clobber`，最后
> 完成的 job 覆盖前面全部。

**MUST**
- **REL-3-1 单一汇总步骤**：SHA256SUMS 必须由**一个**在全部 release 归档上传
  后执行的步骤/作业生成——下载/汇集全部已上传归档到同一 `dist/`，对全部
  `*.tar.gz` 一次性 `sha256sum`，再 `--clobber` 上传；不得由各并行 job 各自生成。
  最终 `SHA256SUMS.txt` 必须包含全部 4 个归档且与各归档实际 sha256 一致。
- **REL-3-2 历史回填**：用各归档的真实 sha256 回填已发布的 `v1.0.0` 与
  `v1.1.0` 的 `SHA256SUMS.txt`（两版本四归档齐全），回填后逐条校验通过。

**OUT_OF_SCOPE**
- 不改归档内容、不改 tag、不发布 crates.io（certs 停在 1.0.0，版本序列不变）。

## FIX-5 — tutor 覆盖层不透明化（2026-11-23 增补；用户实测反馈，见 NOTES 同日条目）

**MUST**
- **FIX-5-1 模块列表覆盖层**：`InputMode::TutorList` 全屏渲染前必须清除下层
  TUI 内容（`ratatui::widgets::Clear` 或等价不透明背景），使目录列表/状态栏
  不透出；保留 `j/k` 选择、Enter/l 进入、q/Esc 关闭与键提示语义。
- **FIX-5-2 底部 Panel**：该覆盖层渲染期间底部 Tutor Panel 区域必须不透明
  （清屏/背景色），其下的状态栏文字不得透出干扰 prompt/expected/进度可读性；
  Panel 主 TUI 背景保持可见（操作上下文）的意图保留——即**仅面板自身区域**
  不透明，面板之外的目录列表仍可见。
- **FIX-5-3 不透明化仅限 tutor 面板/列表区域**，不得改动其它视图的渲染或
  M43-1 的透明放行（按键路由）语义。

## FIX-6 — 会话保存目录自动创建（2026-11-23 增补；用户实测反馈，见 NOTES 同日条目）

> 来源：用户 2026-11-23 反馈——多个 tab、`:q` 退出、再打开未恢复。代码核查：
> 退出路径（`:q` 与 `q` 键）均调用 `save_session`，序列化含全部 tab；但
> `save_session` 直接 `fs::write(session_path(), text)`，而 `session_path()` =
> `~/.config/rvifm/session.toml`，该目录在全新安装（无 config.toml）时不存在，
> 写入静默失败（仅 `eprintln`），启动恢复读不到文件 → 保持初始 CWD tab。
> `tutor::save_progress_to` 已做 `create_dir_all`，二者不对称。

**MUST**
- **FIX-6-1** `save_session` 写文件前必须创建父目录（`create_dir_all`），
  与 `save_progress_to` 同模式；失败仍 `eprintln` 不阻塞退出。
- **FIX-6-2** 补回归测试：注入不存在的配置目录 → `save_session` → 文件存在
  且可解析；`session_restore=false` 时仍不写不读（既有语义不变）。
- **FIX-6-3** 设计澄清：会话**原设计即自动保存**（M25-1 退出保存/启动恢复），
  本次修复是实现缺陷，不改变需求；文档（README/GUIDE/help）维持既有描述。

## FIX-8 — Frecency 搜索面板融合与预览修复（2026-11-24 增补；用户指令）

**设计裁决**
- TUI 内 `o`/`:j`/`:J` 共用焦点 pane 的嵌入式 frecency 搜索面板；候选来自 frecency 存储并以虚拟列表呈现（仅展示路径与条目，不挂载或修改原路径）。面板替换焦点 pane 的列表，对侧 pane 显示当前候选预览；焦点左右两侧的布局尺寸相同。
- 独立 `rvifm pick` 没有原文件管理器 pane 焦点，固定左侧为候选搜索区、右侧为预览区；窄终端仍可降级单栏。
- TUI 候选路径按查询词高亮；点击候选与键盘选定走同一跳转行为。

**MUST**
- **FIX-8-1** `o`、`:j`、`:J` 共用相同嵌入式面板、焦点 pane 位置与几何；`:j`/`:J` 保持类型过滤与跳转语义。候选路径高亮查询词；鼠标点击候选与 Enter 同样跳转；对侧预览复用 `:view` 的图片协议引擎与文本语法高亮/信息面板分流，跟随当前候选。同步 SPEC / ACCEPTANCE / NOTES / README / GUIDE / 内置 help 与 tutor；独立 `pick` 仍固定左候选、右预览；更正 M45-5b 历史验收描述。

**OUT_OF_SCOPE**
- 不把独立 `pick` 重新设计成可切换左右候选区；不变更 frecency 评分、存储、记录或只读边界。

## FIX-7 — 双栏焦点能力一致性（2026-11-24 增补；用户确认）

**MUST**
- 左右 pane 使用同一套 Normal 模式键位与文件/目录命令分派；除当前 Location 的能力限制外，不因焦点在右栏而禁用操作。操作目标始终为焦点 pane。
- 两栏均显示 `..` 行（根目录隐藏），`h`/左方向键与选择 `..` 后 `l`/Enter 均返回各自上级目录。
- 焦点 pane 支持 `o`/`:j`/`:J`、`yy`/`p`/`P`、`:mk`、Visual/tag 与搜索；选择器跳转写回焦点 pane。归档焦点 pane 的 `yy` 复制成员，`p`/`P` 按既有语义解压到焦点 pane 物理目录。
- 逐项审计键盘快捷键、冒号命令、鼠标入口、帮助/指南/tutor 内容及代码注释，移除“右栏仅导航”的 pane 限制；归档只读、远端能力白名单、输入模态键语义等既有能力边界保留。
- 回归验收覆盖左右焦点对称、操作目标、`..` 进退、复制粘贴、归档成员复制与解压。

## FIX-9 — Normal 模式输入法自动切换（2026-11-24 增补；用户确认）

**MUST**
- **FIX-9-1** Linux 上从可编辑输入模式进入 Normal 时，若检测到 IBus、Fcitx 或 Fcitx5，保存原输入法状态并切换为英文输入状态；优先通过停用当前中文输入状态保留已选引擎。IBus 无可用英文状态接口时切换到可配置英文引擎（默认 `xkb:us::eng`）；Fcitx 系列使用已有远程命令接口，不引入 D-Bus crate。
- **FIX-9-2** 从 Normal 重新进入命令、搜索、过滤、重命名、查看器搜索及其它文本编辑模式时恢复切换前状态；退出 rvifm 时尽力恢复。只在实际模式边界切换，不重复覆盖已保存状态。
- **FIX-9-3** 增加 `[ime]` 配置，允许关闭自动切换并配置 IBus 英文引擎标识；默认在 Linux 自动检测支持的后端后启用。子进程通过 `std::process::Command` 传固定可执行文件名及独立参数，不经 shell；后端不存在、命令失败或不支持时静默降级，不阻塞 TUI 主循环或妨碍键盘输入。
- **FIX-9-4** 覆盖 Linux 外平台：配置可解析；不尝试调用 IBus/Fcitx 命令，保持原键盘行为。

**SIMPLIFIED**
- 仅支持 Linux 用户会话里的 IBus、Fcitx 与 Fcitx5 命令接口；不添加 Rust crate、C 库、D-Bus 绑定或必需的输入法依赖。Fcitx5 可用 `fcitx5-remote`，旧 Fcitx 可用 `fcitx-remote`，IBus 使用 `ibus engine`。
- Fcitx 只在此前处于激活状态时保存并恢复激活状态；IBus 保存并恢复输入引擎。外部桌面状态被用户或其他程序并发改变时，不保证合并这些并发状态变更。

**OUT_OF_SCOPE**
- 不修改系统键盘布局、不安装/启动输入法、不注入终端按键、不实现 Windows/macOS 输入法控制或桌面全局快捷键。

## REL-4 — Release 归档简化为 musl-only（2026-11-23 增补；用户指令，见 NOTES 同日条目）

> 来源：用户 2026-11-23 指令——GitHub Release 不再生成 gnu 版本，只保留 musl
> 版本，aarch64 也改为 musl。评估结论（本地实测见 NOTES）：musl.cc GCC 11.2.1
> 编译 aws-lc-sys 触发 ICE；裸 `zig cc` 被 cc-rs 追加的 Rust 风格 `--target`
> 破坏（`UnknownOperatingSystem`）；`cargo-zigbuild` 正确翻译，本地
> `cargo zigbuild --release --locked --target aarch64-unknown-linux-musl` 成功，
> 产物 `file` = ARM aarch64 statically linked ELF。

**MUST**
- **REL-4-1** release matrix 仅 3 腿：`x86_64-unknown-linux-musl`、
  `aarch64-unknown-linux-musl`、`x86_64-pc-windows-msvc`；移除两个 gnu 归档腿
  （x64-gnu 与 aarch64-gnu）。aarch64 腿用 `cargo zigbuild` + zig 交叉编译
  （CI 下载固定版本 zig tarball 到 `$RUNNER_TEMP`，`cargo install cargo-zigbuild`），
  smoke 断言 `ARM aarch64` + `statically linked`（静态 musl 无 `libc.so.6`
  NEEDED，原 gnu 断言不再适用）。`release-checksums` 数量守卫由 4 改为 3。
  归档命名：`rvifm-<tag>-linux-x64-musl.tar.gz` /
  `rvifm-<tag>-linux-aarch64-musl.tar.gz` / `rvifm-<tag>-windows-x64.tar.gz`。
- **REL-4-2** 版本 bump 至 `1.2.0`（产物矩阵用户可见变更，semver minor），
  CHANGELOG / README / SPEC / PLAN / STATE / NOTES / `scripts/repair-release-checksums.sh`
  （数量守卫容错）同步；tag `v1.2.0` → GitHub Release 自动发布三归档 +
  `SHA256SUMS.txt`（3 行）；**crates.io 不发布**（沿用 REL-1 解耦与用户
  2026-11-23 "仅 GitHub Release"决策，crates.io 仍停在 1.0.0）。

**OUT_OF_SCOPE**
- 不保留任何 gnu 归档（含旧版本不改，历史 Release 保持原样）。
- 不引入 FUSE/外部 CLI 依赖；不发布 crates.io。

## M45 — 用户实测批次（2026-11-23 蒸馏；vifm 对齐 + 缺陷修复，见 NOTES 同日条目）

> 来源：用户 2026-11-23 测试反馈 29 项，按 AGENTS §8 蒸馏。登记前已逐项核对
> 代码（三个并行只读调查）与既有文档。**5 项冲突 + 34-9 矛盾已于 2026-11-23
> 用户拍板并落 NOTES/相关节**（分隔线=可配置默认关 / 搜索高亮保留仅修未搜索态 /
> 可执行文件特例 / 右栏归档进入改 M41-3 / extract 对侧+归档 yy 推翻 /
> 34-9 去可滚动）。已实现项（分隔线引擎、标签样式、整行反显、tutor 进度持久化、
> opener 引擎、`:extract` 存在）不重复登记。

**MUST**
- **M45-1 路径与标签栏**：① `Location::display_path()` 对 HOME 下路径显示
  `~/…` 缩写（仅显示层，导航/比较仍用绝对路径）；② 标签栏支持鼠标点击切换
  tab（`handle_mouse` 增 tab_bar 命中，复用 Tab 切换逻辑）；③ 标签样式与
  `TopLine`/`TopLineSel` 解耦、灰白底反色字（现状已用独立 `TabLine`/`TabLineSel`
  组，仅需细化）；④ **分隔线（裁决）**：`[ui] pane_separator` 可配置，默认
  false 不显示、true 恢复，M27-2 同步已修（不需要按原样删除代码，仅默认关）。
- **M45-2 列表视觉**：① 确认整行反显已覆盖全宽（代码现状即如此，无需改）；
  ② 图标按实际类型推断（可执行文件/文本/jar/deb），无后缀不得显示 `*`
  （现状无 `*` 也好，仅补类型推断）；devicons 优先，特殊类型在 `icon_for`
  增兜底；③ **未搜索态文字背景（裁决）**：用户反馈的"文字部分单独颜色"指
  未搜索时的 `ft_*` 文字色，修为未搜索态整行背景一致；**搜索匹配子串
  `bg(LightYellow)` 高亮保留**（FIX-1 不推翻）。
- **M45-3 打开与确认**：① opener 覆盖层排查——无 config.toml 自动生成，且
  `edit_text = "nvim %s"` 字符串形态 TOML 反序列化失败（正确形态
  `edit_text = { run = "nvim %s", block = true }`，解析失败整体回退默认）；
  修复方向：配置解析错误提示保留具体字段 + 提供示例配置生成命令或文档指引，
  不改分层引擎；② **可执行文件（裁决）**：Enter 仅状态栏提示「可执行文件，
  请在终端执行」，不走系统 opener 也不直接执行作为特例（AGENTS §5.5 已增条）；
  ③ 删除等确认由状态栏 `[y/N]` 改为居中阻塞弹窗（带图标/高亮强调）；④ `v`
  多选 + `d` 必须进 `ConfirmTagDelete`（现状直接删，缺陷）；⑤ `h` 回父后 `l`
  重返还原光标（Tab 增 per-location 光标记忆，`navigate_to` 当前重置 0）。
- **M45-4 预览**：① 图片越界——`ImageEngine::render_file` 三分支均用 inner rect，
  原生协议/字体尺寸探测（fallback 10×20）是疑似根因；修字体尺寸兜底与
  `SlicedProtocol` 高度钳制；真实终端验证补入 M11-6 清单。② 新增
  `PreviewKind::Archive`：归档预览显示成员文件名列表（复用 `ArchiveFs::list`
  + `directory_preview`），**必须保持 Office/PDF 先于归档命中**（ZIP 容器
  docx/xlsx/pptx/jar/apk/war），损坏/加密/.rar 回退 `file_info_text`；修主 TUI
  `render_view_content` 用 `read_to_string` 无 NUL 探测导致 tar 乱码。
  与 SPEC M34-9「超高可滚动」/M34-9 SIMPLIFIED「无滚动显示顶部」既有内部
  矛盾一并澄清（见 NOTES）。
- **M45-5 命令与 frecency**：① `o` 的 `JumpKind::Unified` 过滤掉 `topic=cmd`
  条目（cd/tabnew 不可跳转，缺陷）；② `o`/`:j`/`:J` 选择器支持鼠标点击选中
  （`handle_mouse` 非 Normal 立即返回的现状需改）；③ `:` 命令名辅助提示
  （现状仅有 Tab 路径补全/↑↓ 历史/`'` 书签，无命令名补全）——新增需求，
  原需求未要求，需用户确认范围（命令名补全 vs 参数提示）。
- **M45-6 右栏一致性**：① `:cd` 用 `focus_base()` 解析相对路径并导航写入焦点
  pane（右 pane 无 history 栈直接设 location+refresh；现用 `active_tab().location` 恒左，
  违反 M41-1，缺陷）；② `o` 结果跳转
  写入焦点 pane（现恒写 `tab.location` 左）；③ **右栏归档（裁决）**：右键栏
  归档 `l`/Enter 进入虚拟目录——M41-3 已改规格（文件仍 no-op）。
- **M45-7 解压与虚拟目录（裁决已推翻旧决策）**：① `:extract` 支持解压到对侧/
  指定侧或路径（M37-2 默认已改为对侧）；② 虚拟目录 `yy` + `p`/`P` 粘贴到焦点
  pane 物理目录 = 按成员解压（旧 OUT_OF_SCOPE 决策作废，仍不写归档内）。
- **M45-8 tutor**：① 列表 `l`/Enter 后退出 `InputMode::TutorList`（现状只设
  `state.tutor` 不改 mode，列表残留，缺陷）；② Panel 右栏提示改
  `Ctrl+Y/Ctrl+N/Ctrl+Q`（现状 `y/n/q`，与 M43-1 必需 Ctrl 修饰矛盾）；③
  布局避让（G 跳转底部不被 Panel 挡住）；④ 模块列表左侧显示学习进度；
  进度持久化已实现（M42-2），用户「未持久化」疑似因按错提示键未记分。
- **M45-9 archive**：① `archive_plan` 用 `selected_names()` 覆盖 `v` 连续选择
  （现仅 tags + 光标，缺陷）；② 格式单参推断已支持，补 `[名, checksum=…]`
  形态；③ 对话框：宽度增大、文件名不显示后缀（后缀按格式右侧自动显示）、
  默认名单文件→原名/多选→父目录名、格式/校验仅高亮选中项、任意字段回车执行
  （最后一项已实现）。
- **M45-10 文档收尾**：README/GUIDE/help/ACCEPTANCE/CHANGELOG 同步；冲突裁决
  落 NOTES；核对 tutor 条目维护规则（M43-6）补新增命令对应条目。

**SIMPLIFIED**
- M45-1 路径缩写只做 `~`/`~/…`，不做多级路径缩写；M45-6 右栏归档进入仅
  虚拟目录，不写归档内；M45-9 对话框保留现有字段结构，不做可视化拖拽。
- 所有新行为沿用既有 `Action`/Keymap 与主题/图标渲染路径，不引入新依赖。

**OUT_OF_SCOPE**
- 不改归档内写入/嵌套递归；不做 vifm 脚本引擎或 `:highlight` 语法；
  不自动执行可执行文件；命令辅助不做外部 shell 补全脚本。

## M46 — 用户实测批次 2（2026-11-24 蒸馏；vifm 对齐 + 缺陷真修复 + 配置/归档增强，见 NOTES 同日条目）

> 来源：用户 2026-11-24 实测反馈 12 项，按 AGENTS §8 蒸馏。其中 M46-1/3/4/5 分别为 M45-4a/M45-2a/M45-3e/M45-7/M37-2 的"未真正修复"回退，登记时逐项对照代码定位根因（见 NOTES）。`.apk` 经代码核实已支持，保持不动；`.deb`/`.rar` 需新增只读后端。

**MUST**
- **M46-1 图片预览越界真修复**：图片（含原生 Kitty/iTerm2/Sixel、halfblocks、PDF 首页图像、`pick` 预览）必须严格绘制在预览窗格 inner rect 内，顶部不得出现在整个 rvifm 窗口顶部。修正三处：① `image.rs` 的相交从 `frame.area()` 改为调用方传入的预览 pane rect（禁止用整屏兜底）；② `SlicedImage`/`clamp_slice_height` 的虚拟高度与 widget 实际绘制区域一致，原生协议的绝对定位图形不得超出窗格；③ `render.rs` 中 `viewer_rect`（PreviewBottom）与经 tutor 面板收缩后的 pane rect 同源，消除几何错位。补 TestBackend 回归断言（图片 widget 目标 rect 完全落在预览 inner rect 内）；真实终端大图验证沿用 M11-6/C6 deferred。
- **M46-2 搜索高亮仅焦点 pane**：`/` 搜索的 LightYellow 匹配子串高亮只渲染在当前焦点 pane（左 pane `render()` 的 active_query 增焦点守卫，与右 pane `render_right_pane` 现有守卫对称）。Tab 级共享 `SearchState` 存储不变（不改成 per-pane），`n`/`N` 与状态栏计数行为不变。
- **M46-3 右 pane 整行背景一致**：`render_right_pane` 对当前行/可视选择行应用与 M45-2a 左 pane 相同的 `row_reversed` 守卫（反显/选中行去除 `ft_*` 文字前景色，未搜索态整行背景一致）；搜索匹配子串高亮保留（FIX-1 不推翻）；补右 pane 回归测试。
- **M46-4 `h`→`l` 光标还原真修复**：`enter_selected`（左右 pane）在通过 `restore_cursor()` 还原目标目录光标后，不得再无条件把 `on_parent_row` 置为 `true` 覆盖还原结果；仅当还原后的光标确实指向 `..` 行时才设 `on_parent_row=true`。`/tmp/test/b` → `h` → `l` 光标必须落在 `b` 而非 `..`。补断言 `on_parent_row == false` 的回归测试。
- **M46-5 `:extract` 目标 pane 刷新**：`:extract`、归档 `yy`→`p/P` 解压写入目标 pane（含对侧）后，必须立即刷新目标 pane 的目录条目；同步修正 `apply_job_update` 用 `refresh_tab_panes`（左右双侧）替代仅 `tab.refresh`。补"解压后对侧列表包含新文件"的回归测试。
- **M46-6 启动生成默认配置**：TUI 启动时若 `config.toml` 不存在，在 `config_path()` 的目录（`create_dir_all`）写入带中文注释的默认 `config.toml`，逐节说明 `ui`/`preview`/`frecency`/`archive`/`transfer`/`ime`/`[normal]`/`[opener]`/`[open]` 等默认项；同时生成键位与 opener 示例文件（如 `keymap.example.toml`/`opener.example.toml`）供参考。已存在文件绝不覆盖；生成失败静默降级不影响启动；仅 TUI 启动钩子生成，`load_from(temp)` 与 `cargo test` 不得被污染。补纯函数/路径注入测试。
- **M46-7 `:help` 修复**：① help 覆盖层与 `HelpSearch` 渲染分支用 `Clear` + 不透明背景，不再透出下层列表；② `/` 进入 `HelpSearch` 后可见输入框、可键入并 Enter 搜索、Esc 返回，内容不再被清空；③ `:help` 在检测到 tmux/screen 且 `[ui] help_in_new_window`（默认 true）为真时，通过既有 `run_external_target_with` 在新复用器窗口打开并激活 `rvifm help`（新增子命令，输出内嵌 help 文本）；无复用器或关闭时回退到应用内不透明覆盖层。补渲染/键位/CLI 冒烟测试。
- **M46-8 归档 `w` 预览提速**：预览不得在每一帧重新 `open_dispatch` 并全量解压（尤其 `tar.xz` 整文件解码）；按 (路径, mtime, size) 缓存成员列表，首次打开后复用；为预览打开加输入文件大小护栏与轻量成员上限，超限给可读提示而非卡死；大归档优先给出"按 l 进入浏览"的快速提示。zip/纯 tar 等快速路径保持。补"同路径二次预览不重复 open"与护栏测试。
- **M46-9 新增归档格式**：`.apk` 已在 `ZIP_CONTAINERS` 中，`w` 预览与 `l`/Enter VFS 均可用，保持不变并补文档。新增 `.deb`（`ar` 容器解析 + 内嵌 `data.tar.*` 复用 `TarFs`）与 `.rar`（外部 `unrar`/`7z`/`bsdtar` 只读列出/抽取，缺失给可读提示）的 `w` 预览成员列表与 `l`/Enter 虚拟目录进入；所有扩展名真值表（`ZIP_CONTAINERS`/`is_archive_file`/`is_unsupported_archive`/`open_dispatch`/`detect_kind`/`strip_archive_ext`）同步，统一单一来源避免漂移；沿用 64MB 成员上限与只读边界，不写归档内。补识别/浏览/损坏回退测试。
- **M46-10 删除确认 yazi 风格**：删除确认（单文件与 `v`/`t` 多选）改为居中弹窗：标题「⚠ 是否删除选定的 N 个文件」居中并带确认图标；标题下方不再显示提示文字；列出待删除文件，图标/颜色与双栏列表一致；高度固定，条目过多时右侧显示滚动条，`j`/`k`（含方向键）导航查看，`y`/Enter 确认、Esc/`n` 取消。补渲染/交互测试。
- **M46-11 help 文档同步**：`src/app/help.txt` 与 `docs/GUIDE.md`、`CHANGELOG.md` 同步本批次新增/修复项（图片越界、焦点高亮、右栏整行、h→l、extract 刷新、默认配置生成、help 新窗口与 `/` 搜索、归档预览提速与 deb/apk/rar、yazi 删除确认）。
- **M46-12 tutor 同步**：按 SPEC M43-6 维护规则，`assets/tutor/*.toml` 与提示同步本批次变化（如帮助新窗口/搜索、归档格式、删除确认等），更新相关 `prompt`/`expected` 与 Panel 提示，保持 13 模块 100 项结构可解析。

**SIMPLIFIED**
- M46-2 不把 `SearchState` 拆成 per-pane（仅渲染层焦点守卫）。
- M46-6 生成的示例文件为静态模板，不做交互式配置向导；不引入 `dirs`/`xdg` crate，复用 `config_path()`。
- M46-7 "激活"在新复用器窗口为尽力而为（tmux `new-window` 切换/可选中），不抢占用户当前工作区焦点；非复用器终端回退应用内覆盖层，不强制启动 GUI 终端模拟器。
- M46-8 缓存仅内存态（不落盘），进程重启后重建；预览成员上限给提示而非完整虚拟滚动。
- M46-9 `.deb` 只浏览 `data.tar.*` 内容（`debian-binary`/`control.tar` 不展示）；`.rar` 依赖用户系统只读工具，缺失给提示。
- 所有改动沿用既有 `Action`/Keymap、主题/图标与 `TestBackend` 测试路径，不引入必需外部依赖。

**OUT_OF_SCOPE**
- 不修改 `pick` 只读红线、frecency 评分/存储、撤销栈语义。
- 不实现归档内写入/嵌套递归浏览、不嵌入视频播放、不引入 Vim 脚本/插件系统。
- 不为 help 引入内嵌文档查看器依赖或外部终端模拟器的硬依赖。
- 不改动 crates.io 发布状态（仍停在 1.0.0）。

## M47 — 用户实测批次 3（2026-11-25 蒸馏；焦点化收尾 + 视觉间距 + 防御性 IME + 缺陷修复，见 NOTES 同日条目）

> 来源：用户 2026-11-25 实测反馈 7 条（其中「右栏仅导航」含 3 个子项），按 AGENTS §8
> 蒸馏。登记前逐项对照代码定位根因（见 NOTES）。编号沿用 M45/M46 用户实测批次先例取 M47。
> 顺序：1→10（缺陷与焦点化先行，文档收尾殿后）。本批次不触发版本 bump 与 GitHub Release。

**MUST**
- **M47-1 目录预览图标缩进**：目录/归档成员预览中的条目按层级缩进，图标紧跟缩进之后显示，不再全部贴左边缘；缩进只作用于预览显示层，不改变条目内容、排序或导航。
- **M47-2 `=` 过滤焦点化**：① `=` 过滤（含 glob 实时筛选）作用于当前焦点 pane，左右对称；② 取消状态栏中的过滤状态提示，改为在对应 pane 顶部路径行追加显示过滤状态（如 `path [filter: pattern]`），左右各自显示自己的过滤状态；③ 在过滤态按 Esc 清除该 pane 的过滤并恢复完整列表；过滤状态与搜索状态相互独立。
- **M47-3 `:archive` / `:extract` 焦点化**：`:archive` 的压缩源改为焦点 pane 的对应文件（tag / `v` 选择 / 光标的既有关联不变），产物目录改为对侧 pane 的物理目录；`:extract` 的源与解压目标按焦点 pane 决定（缺省对侧），与 M45-7/M46-5 的对侧刷新语义一致。归档只读边界与 `u` 撤销不变。
- **M47-4 剩余快捷键与命令焦点化**：继续 FIX-7 未完成的审计，把仍硬编码左栏或写死 `active_tab().location` 的快捷键/冒号命令/鼠标入口改为按焦点 pane 分派、操作目标为焦点 pane；不改变归档只读、远端白名单、输入模态等既有能力边界。审计结果落 NOTES。
- **M47-5 Frecency 搜索栏融合**：`o`/`:j`/`:J` 的搜索面板样式复用原焦点栏（不显示边框、背景色与焦点栏目录区完全一致、无弹出/浮层观感），仅替换该栏的列表内容；顶部路径行标题改为「Frecency - 高频目录/文件搜索 - Normal 模式」；候选高亮、预览、鼠标跳转与只读语义不变。
- **M47-6 IME 防御性激活英文（增补实现）**：在 Linux 且 `[ime] enabled` 时，TUI 启动后若处于 Normal 模式即调用一次切换到英文输入（覆盖「在其它终端/应用激活中文输入后切入 rvifm」的场景）；Normal 模式下按 Esc 同样触发一次英文激活（比依赖模式边界更可靠）。重复触发须节流，不得覆盖已进入文本输入模式时保存的输入法状态；后端缺失/失败静默降级，不阻塞主循环。保留 FIX-9 既有的「离开可编辑模式保存并切英文、重新进入恢复」语义。
- **M47-7 右栏顶行高亮修复**：右 pane 按 `gg` 或其它方式移动光标到顶部 `..` 行时，只高亮 `..` 一行，不再同时高亮第一条真实条目；左右栏行为对称。
- **M47-8 左右栏独立选择状态**：`v`/`V` 多选状态按 pane 独立保存；在右栏多选后用 `Tab`/`Space` 切换焦点到左栏，左栏不得呈现右栏的多选状态，两栏各自保持自己的选择集合。批量操作仍作用于焦点 pane；`t` tag 语义不变。
- **M47-9 界面间距与高亮覆盖**：左右双栏列表左侧增加一定留白（对齐 vifm 的栏间距观感），当前行/可视选择行的整行高亮必须覆盖该留白区域，不留彩色断层；左右对称，已有列对齐与主题语义不回归。
- **M47-10 文档收尾**：README / GUIDE / 内置 help / CHANGELOG / ACCEPTANCE / tutor 条目（按 SPEC M43-6 维护规则）同步本批次变化，保持 13 模块 100 项结构可解析。

**SIMPLIFIED**
- M47-1 缩进仅按成员路径深度（预定义层级），不做树形连接线或可折叠目录。
- M47-2 过滤状态为纯显示层追加，不新增配置文件字段；每个 pane 各自持有一个过滤模式。
- M47-5 面板复用焦点栏自身的样式与几何；独立 `rvifm pick` 仍保持既有固定左候选/右预览布局，不改评分、存储与只读边界。
- M47-6 终端「窗口获得焦点」事件在多数终端不可靠，故用「启动 + Normal 模式 Esc」两处触发近似，不引入全局快捷键或桌面级监听；不修改系统键盘布局。
- M47-8 选择集合按 pane 拆分存储（左/右各一份），不引入新的 Action；`t` tag 仍沿用 Tab 级集合。
- 所有改动沿用既有 `Action`/Keymap、主题/图标与 `TestBackend` 测试路径，不引入必需外部依赖。

**OUT_OF_SCOPE**
- 不恢复「右栏仅导航」限制，不新增 Vim 脚本、插件、FUSE 或外部预览命令。
- 不修改 frecency 评分/存储/`pick` 只读红线、撤销栈语义、归档只读边界。
- 不实现终端焦点事件的桌面级监听、不安装/启动输入法、不注入终端按键。
- **本批次不执行版本 bump、不推送 tag、不触发 GitHub Release**（用户 2026-11-25 指令）。

## M48 — 用户实测批次 4（2026-11-25 蒸馏；外窗/打开/复制解压/归档预览/tutor/弹窗，见 NOTES 同日条目）

> 来源：用户 2026-11-25 实测反馈 12 条，按 AGENTS §8 蒸馏。登记前逐项代码定位（见 NOTES）。
> 编号沿用 M45–M47 用户实测批次先例取 M48。顺序：1→13（缺陷先行，文档收尾殿后）。
> 沿用用户 2026-11-25「只提交并推送代码、不触发版本 bump / tag / GitHub Release」的保守口径（见 NOTES）。

**MUST**
- **M48-1 外窗 shell 修复**：`s` 与 `:!` 在 `[ui] terminal_in_new_window=true` 时创建的新终端窗口必须激活到前台，且新窗口的起始目录为**当前焦点 pane 的路径**（左右均可），不再固定为进程 CWD / 左栏路径；无复用器回退行为不变。
- **M48-2 文本打开优先新窗口**：`l`/Enter 在 `.md`/`.txt`/`.md5` 等文本类文件上不得覆盖 rvifm 界面；文本编辑类 opener（含 `[open]` 中 `block=true` 的 `edit_text` 类规则）优先在**新终端窗口**打开；键入/编辑器进程退出后 rvifm 界面保持完整。`.sh` 等带执行位的文本文件优先按文本识别，不走「可执行文件仅提示」特例；`.md5`、`.txt` 的扩展名/魔数识别不得再出现按 Enter 无反应。
- **M48-3 右栏文件打开一致性**：右栏焦点下 `l`/Enter 落在文件上时，与左栏一致执行打开/编辑链路（复用同一 opener 与配置），不再 no-op；目录仍走进入虚拟/物理目录，归档只读边界不变。
- **M48-4 `yyp` 缓存缺失修复**：`v` 多选（文件/目录）后 `y` 复制，到对侧 `p`/`P` 不得再报「没有缓存」；`p` 与 `P` 走同一套 yank 缓冲区读取，焦点投影不得丢失缓冲区。
- **M48-5 冲突确认弹窗**：`p`/`P` 目标已存在时的确认从状态栏提示改为**阻塞式居中弹窗**，视觉风格与删除确认一致（标题/图标/条目列表/固定高度/滚动条、`y`/Enter 确认、Esc/`n` 取消），取消后状态不变。
- **M48-6 归档成员粘贴修正**：`yy` 归档内成员后 `p`/`P` 解压：① 只按成员相对路径的**文件名/末段**落盘，不得重建归档内的上级目录（如 `zip/a/c.json` 只产出 `c.json`）；② 目录成员必须**递归**解压其下所有子目录与文件；③ 多选文件与目录混合时逐项正确落盘。仍不写归档内，保持只读与 `u` 可逆。
- **M48-7 归档快速预览提速**：`w` 预览大归档首次耗时与「文件过大未展开」问题：增加**可选外部列目录工具**快速路径（zip → `unzip -l`、tar → `tar -tf`、7z/rar → `7z l`、deb → `bsdtar -tf`），命中且输出在护栏内时按行渲染；内置 VFS 仍为回退且为默认能力，不引入必需依赖、不经 shell、不注入拼接参数，保留输出与成员数上限。对齐 vifm `fileviewer` 的体验。
- **M48-8 `:archive` 行为调整**：① 无参数 = 按默认方式**直接执行**（不打开对话框）；② `-i` = 打开交互对话框；③ 对话框任意字段回车即执行；④ `Tab` 切换字段时当前字段背景高亮可辨；⑤ 弹出层不再遮罩整个界面（缩小为居中窗口）。原无参打开对话框行为作废。
- **M48-9 IME 切回自动切英文（稳健化）**：从其它应用切回 rvifm 时 Normal 模式自动切英文必须在终端不发送 FocusGained 时仍生效；以 Normal 模式下的**周期探测**（仅发必要命令、不覆盖已保存状态、不与文本输入模式竞争）作为兜底，Esc/FocusGained 仍保留。
- **M48-10 tutor 列表增强**：`:tutor` 列表界面新增 `/` 搜索并跳转（复用现有搜索态语义）、`d` 对光标模块执行 reset 并刷新进度、`l`/Enter 进入「有记录模块」时仅展示错题本条目、模块全部结束时 `Ctrl+N` 退出当前会话并回到 `:tutor` 列表重新选模块。保持 13 模块 100 项结构与进度持久化语义。
- **M48-11 删除确认 yazi 真窗口化**：删除确认弹窗的状态栏文字置于**方框内部底部**，滚动条置于**方框内部右侧**，形成完整窗口观感；`j/k` 导航、`y`/Enter 确认、Esc/`n` 取消不变。
- **M48-12 `:help` 修复**：`:help` 新窗口路径必须真实打开并激活（不再只写状态栏提示）；`rvifm help` 子命令从「打印即退出」改为「以 `view` 形式打开内嵌帮助文本」的查看器体验（支持语法/高亮与页内 `/` 搜索、`n/N`、`q`/Esc 退出）；应用内覆盖层路径保持不透明可搜索。
- **M48-13 文档收尾**：README / GUIDE / 内置 help / CHANGELOG / ACCEPTANCE / tutor 条目（按 SPEC M43-6 维护规则）同步本批次变化。

**SIMPLIFIED**
- M48-1 新窗口「激活」为尽力而为（tmux `select-window` / 可选中），不抢占用户桌面全局焦点；无复用器仍回退当前 pane。
- M48-2 文本优先按扩展名（含 `.txt`/`.md`/`.md5`/`.sh`）+ 有界魔数嗅探判定；不引入新依赖；编辑器路径仍遵循既有 `[opener]`/`[open]` 分层。
- M48-7 外部列目录工具为**可选加速**：缺失、失败、超输出上限时静默回退内置 VFS；不新增 crate，不把外部工具作为必需依赖；不改 frecency 与归档只读边界。
- M48-8 对话框字段结构保持既有形态（即使用新触发方式也不新增字段）。
- M48-9 周期探测节流（≥1s），仅 Normal 模式生效；不提供桌面级全局快捷键。
- M48-10 列表搜索仅过滤模块名（不做全文内容搜索）；「有记录模块」= 进度文件或错题本中存在该模块记录。
- M48-12 内嵌帮助查看器复用 `PreviewRenderer` 的文本渲染链，不引入内嵌文档查看器依赖。
- 所有改动沿用既有 `Action`/Keymap、主题/图标与 `TestBackend` 测试路径，不引入必需外部依赖。

**OUT_OF_SCOPE**
- 不实现 Vim 脚本、插件、FUSE、归档写入、嵌套归档递归浏览。
- 不移除「可执行文件 Enter 仅提示」的默认特例（仅 `.sh` 等文本文件按文本优先识别），不自动执行用户文件。
- 不把外部列目录工具作为必需/编译期依赖，不经 shell 执行用户可控字符串。
- 不 bump 版本、不推送 tag、不触发 GitHub Release（沿用用户 2026-11-25 指令）。

## M49 — 用户实测批次 5（2026-11-26 蒸馏；预览固定右侧 + 命令/间距/预览效率/frecency 键位，见 NOTES 同日条目）

> 来源：用户 2026-11-26 指令（LAYOUT-1..4 硬约束）+ 实测反馈 124–141 条，按 AGENTS §8 蒸馏。
> 编号沿用 M45–M48 用户实测批次先例取 M49；用户给出的 LAYOUT-n 映射到 M49-1..4，124–141 按原序随后。
> **注意：M49-1/2/3 的「默认固定右侧」策略已被 M52 反转（默认改为对侧、仅 frecency 受开关控制）；历史条目保留供追溯。**
> 沿用「只提交并推送代码、不 bump / tag / GitHub Release」口径。

**MUST**
- **M49-1（LAYOUT-1）frecency 固定右侧预览**：`o`/`:j`/`:J` 激活时，搜索/结果列表固定左侧、预览固定右侧，与焦点无关。焦点初始在左搜索区，`j/k` 切结果且右侧预览跟随；`Tab` 进右预览（`j/k` 滚动、`/`、`n/N` 搜索，左列表可见但不响应 `j/k`），再 `Tab` 回左；Esc 逐级退出语义不变。
- **M49-2（LAYOUT-2）`:view` 固定右侧预览**：`w`/`:view` 预览固定右侧、左侧保持原 pane 目录列表；焦点默认留左侧、`j/k` 切文件右侧跟随；`Tab` 进预览行为与 M49-1 一致，再 `Tab` 返回；再按 `w` 恢复双 pane 目录列表。
- **M49-3（LAYOUT-3）配置开关**：新增 `[preview] fixed_side = true|false`（默认 `true` 固定右侧；`false` = 焦点 pane 搜索/列表 + 对侧预览的动态布局）。仅对**新打开**的 frecency/`:view` 生效，不热切换已打开会话。
- **M49-4（LAYOUT-4）布局文档**：GUIDE / SPEC / NOTES / ACCEPTANCE 四处同步 LAYOUT-1..3 决策与验收，无矛盾描述。
- **M49-5（124）命令别名**：`:gitui` 改为 `:gi`（gitui 是外部软件名）。为避免破坏脚本，`:gitui` 保留为兼容别名，`:gi` 为新主命令；help/GUIDE/README/tutor 同步。
- **M49-6（125）栏间距收窄**：`LIST_LEADING_BLANK` 从 2 空格改为约图标半宽且不超过图标宽（用 1 空格），整行高亮仍覆盖留白；左右对称。
- **M49-7（126）可视模式 D**：`v` 多选下 `D` 对选中项执行永久删除（跳过暂存、不可恢复、强确认），与 `dd` 单文件永久删除语义一致；确认沿用 yazi 风格弹窗（M46-10/M48-11）；与既有「`dd`/`DD` 永久删除」文档不冲突（新增 v+D，不改 DD）。
- **M49-8（127）`:extract` 空目录修复**：修复含显式目录条目的归档（如用户样本 micro-archives）解压后只建空目录、无文件的问题；成员路径不得重复拼接父目录；保持 zip-slip / 64MB 护栏 / `u` 可逆 / 对侧刷新。
- **M49-9（128–130）预览效率**：frecency/`:view` 快速切换时，图片/PDF 预览先展示元数据/轻量信息（不触发重栅格化）；文本/文本类仅先展示 200–300 字预览；复用既有缓存与 `PreviewRenderer`，不引入必需依赖、不改 `pick` 同源。
- **M49-10（131）预览焦点全量加载**：预览区获得焦点（`Tab` 进入）后触发异步/按需全量加载（完整文本或图片渲染）；退出焦点回到轻量态；不阻塞事件循环。
- **M49-11（132–134）焦点化复核**：复核 `:archive` 源=焦点栏/产物=对侧栏、`:extract` 源=焦点栏/目标对侧，在 M47-3/M48-6/M49-8 之后仍成立；补齐残留左栏硬编码命令。
- **M49-12（136）frecency 动态标题**：顶部路径显示 `Frecency - 高频目录/文件搜索 - ${mode} 模式[filter xxx]`，mode 为实际 `NORMAL`/`SEARCH`（大写），filter 段样式与 `:view` 搜索态一致（黄底高亮）。
- **M49-13（137）SEARCH 提示与 Ctrl+J/K**：SEARCH 模式状态栏增加「方向键上下移动」提示；评估并加入 `Ctrl+J`/`Ctrl+K` 作为上下移动别名（符合 fzf/阅读习惯，不与既有键位冲突）。
- **M49-14（138）Esc 保留过滤**：SEARCH 模式按 Esc 返回 NORMAL 但**保留**当前 query/filter；清空 query 用其它显式键（如 Ctrl+U/退格），不得再静默清空。
- **M49-15（139）NORMAL 键位调整**：删除 frecency **记录**（非文件）从 `d` 改为 `dd`；`u` 恢复被删记录（接 frecency trash）；`y` 系列改为与目录列表一致的 `yf`/`yn`/`yc`（不跳转复制路径/名称/内容）；`gi` 查找当前目录或文件所在 git 仓库并调用交互 git TUI。Esc 逐级退出与其它 Normal 键位不变。
- **M49-16 文档收尾**：GUIDE / help / CHANGELOG / tutor / ACCEPTANCE 同步本批次其余变化（命令别名、间距、v+D、预览效率、frecency 键位）。

**SIMPLIFIED**
- M49-1/2 抽出共享「右侧预览区」布局函数供 frecency 与 `:view` 共用；独立 `rvifm pick` 保持左候选/右预览不变。
- M49-3 配置仅新会话生效（用户授权：热切换成本高则直接按新会话交付，NOTES 记录）。
- M49-9 轻量预览为显示层降级：图片/PDF 不加载像素而显示魔数/尺寸/页数等元数据；文本截断有明确省略提示；全量仍走 `PreviewRenderer`。
- M49-10 全量加载可在事件循环空闲时同步重建（不引入线程），以"不卡顿"为准；记录决策于 NOTES。
- M49-12 `[filter xxx]` 仅在 query 非空时显示，样式复用 `:view` 搜索高亮 span。
- M49-15 `gi` 在非 git 仓库给可读错误、不 spawn（M48-fix 已建立口径）。
- 所有改动沿用既有 `Action`/Keymap、主题/图标与 `TestBackend` 测试路径，不引入新 TUI 框架或必需外部依赖。

**OUT_OF_SCOPE**
- 不改变 frecency 评分/候选来源/只读语义/Esc 逐级退出/Ctrl+Y·N 判定；不改变普通双 pane 目录浏览模型。
- 预览区位置不随焦点切换移动（默认模式）；动态对侧仅显式配置开启。
- 不实现 Vim 脚本/插件/FUSE；不自动执行用户文件；不 bump 版本 / tag / GitHub Release。

## M50 — 命令补全增强（2026-11-26 蒸馏 DeepSeek 分享稿；用户已确认，见 NOTES 同日条目）

> 来源：用户提供的 DeepSeek 分享对话（4 轮 Q&A）蒸馏，用户 2026-11-26 确认 5 项决策：
> ① 登记执行；② UI 复用现有 frecency overlay、高度 ≤40% 终端，不做 Tridactyl 半屏；
> ③ 采纳 shell 历史解析、默认开、优先增量读取；④ 新增 Action 但**不设默认绑定**；
> ⑤ 变量先落地裸 `%f/%F/%d/%D` + `[command] quote_expansion` 配置开关，`%"f` 引号变体本批次不做。
> 沿用「不 bump / 不 tag / 不 GitHub Release」口径。

**MUST**
- **M50-1 内部命令补全**：`:` 命令模式键入时提供补全候选：内部命令表（名称 + 功能描述）+ 使用频率（frecency `cmd` topic）。排序 = 前缀匹配优先 → frecency 分数降序 → 字母序。UI 复用现有 frecency overlay 骨架，底部满宽、高度 ≤40% 终端（候选 <5 时约 5 行，最多 12–15 行），左列命令名、右列描述，选中项用主题 `selected` token；`Enter` 执行、`Tab` 仅补全前缀不执行。内部命令元数据从内嵌 help 文本（`include_str!`）或轻量表生成，避免描述重复维护。
- **M50-2 `!` 前缀来源分离**：`:` 开头仅补全内部命令；`:!` 开头仅补全 shell 历史中的**完整命令行（带参数）**。两者共用同一 overlay 与评分/过滤逻辑，候选池按首字符决定；不改变 vim/vifm 的 `:!` 执行语义。外部候选右侧显示：已记录次数 / 「外部命令」占位（不做 PATH 主动扫描）。
- **M50-3 shell 历史解析**：新增 shell 历史读取（至少 bash `~/.bash_history`、zsh `~/.zsh_history`），配置 `[command] scan_shell_history = true`（**默认开**）；支持**增量读取**（记录文件 size/mtime/offset，只解析新增行）。完整命令行作内存补全候选并写入 frecency `cmd` topic（M58-5 起按整行；旧「只有命令名」隐私边界已由用户确认推翻）。读取失败/文件缺失静默降级；不读参数/路径入库。
- **M50-4 新 Action（无默认键位）**：新增 `Action::OpenExternalCommandLine`（字符串名 `open_external_command_line`），进入命令模式并预填 `!`；**不设置任何默认绑定**，仅在 config.toml / GUIDE 给出 `[normal] append_keymap` 示例由用户自选按键。
- **M50-5 变量引号配置**：新增 `[command] quote_expansion = true|false`（默认 `false`，与 vifm 裸变量对齐）；现有 `%d`/`%f`/`%F`/`%D` 在该开关开启时展开值自动加双引号；引号变体 `%"f` 等本批次**不做**。
- **M50-6 文档收尾**：GUIDE / SPEC / NOTES / ACCEPTANCE / 内嵌 help / config 默认模板同步；记录「不做 PATH 扫描 / 不做 execve 守护 / 不做参数补全」边界。

**SIMPLIFIED**
- M50-1 命令表从内嵌帮助文本解析（编译时 `include_str!`，运行期一次性构建），不引入新 crate。
- M50-2 外部补全候选来自历史文件（完整行）而非 PATH 扫描；评分按命令名聚合、展示完整行。
- M50-3 shell 历史文件可能有「当前仍打开的 shell 会话尚未落盘」的已知边界，写入文档说明。
- M50-4 用户自定义键位沿用既有 Action 驱动 TOML 分层；不新增模式。
- M50-5 只做配置开关 + 裸变量引号；`%"f` 变体列为后续增强，不在本批次。
- 所有改动复用既有 overlay / 双栏渲染 / 键位系统，不引入 fzf、SQLite、PATH 扫描守护进程或新必需依赖。

**OUT_OF_SCOPE**
- 不扫描 PATH 发现外部命令、不实现 `execve` 守护进程监控、不补全参数/子命令（如 `git commit` 的 `commit`）。
- 不内置 `!` 快捷键；不为补全单独写一套浮层；完整命令行持久化到 frecency（M58-5 推翻旧边界；见 SPEC M58）。
- 不 bump 版本、不推送 tag、不触发 GitHub Release。

## M52 — 预览布局策略反转（2026-11-26 蒸馏，已确认；见 NOTES 同日条目）

> 来源：用户 2026-11-26 指令，确认「理解正确」后登记。本批次**反转 M49 LAYOUT-1..3 的默认策略**；
> M49 历史条目保留不抹除，在 SPEC/NOTES/CHANGELOG 显式标注「已被 M52 取代」。
> 沿用「不 bump / 不 tag / 不 GitHub Release」口径。

**MUST**
- **M52-1 策略反转**：`[preview] fixed_side` 默认值由 `true` 改为 **`false`**，且**只作用于 rvifm 内的 `o`/`:j`/`:J` frecency 模式**。
  - `:view`（`w`）：左右双栏，**始终在对侧栏预览**（焦点左→右预览，焦点右→左预览），恢复 M34-4/M39-6 语义，不再读取 `fixed_side`。
  - `rvifm pick`（外部调用）：始终**左侧候选、右侧预览**（现状保持，不改）。
  - rvifm 内 frecency：默认**对侧预览**（焦点栏搜索/列表、对侧预览）；显式 `fixed_side = true` 时固定右侧预览。
  - 配置仅对**新打开**的会话生效，不热切换（延续 M49-3 口径）。
- **M52-2 右栏能力审计**：右侧 pane 可用快捷键/冒号命令与左侧基本一致（焦点投影 + 命令分派）；`Tab` 进入预览、`j/k` 滚动、`/`、`n/N`、`Tab` 返回的语义在两种布局下均成立；审计结果落 NOTES。
- **M52-3 全量一致性收尾**：检查并统一全部 harness 文档（SPEC/TASKS/ACCEPTANCE/STATE/PLAN/NOTES）与项目文档（GUIDE/README/CHANGELOG/内置 help/tutor）及相关代码/测试，消除 M49 遗留的「默认固定右侧」描述冲突；质量门禁全绿。

**SIMPLIFIED**
- 不改 frecency 评分/候选来源/只读语义、Esc 逐级退出、Ctrl+Y/N 判定、标题与过滤态。
- 不移除 `fixed_side` 配置键（仅改默认值与作用域），避免已有配置解析失败；显式 `true` 仍可用。
- 共享几何函数保留，但 `:view` 路径不再传入该开关；`preview_path_with` 按用途分派（`:view` 焦点感知 / frecency 按开关）。

**OUT_OF_SCOPE**
- 不改变普通双栏目录浏览模型；不引入新依赖或新 TUI 框架；不 bump / tag / Release。

## M51 — 用户实测批次 6（2026-11-26 蒸馏；IME 干扰 / 默认编辑器 / 文本识别 / 冲突弹窗）

> 来源：用户 2026-11-26 实测反馈 4 条，按 AGENTS §8 蒸馏。沿用「不 bump / 不 tag / 不 GitHub Release」口径。

**MUST**
- **M51-1 IME 过度干扰修复**：rvifm 通过 `s` 等打开新终端窗口后，在该窗口运行 vim 或任意程序时**不得**被 rvifm 的 Normal 模式周期探测强制切回英文（输入法状态是桌面全局的）。要求：启动外部窗口/挂起 TUI 时暂停 IME 强制；收到 `FocusLost` 时暂停；回到 rvifm 并收到按键 / `FocusGained` 时恢复。Esc/FocusGained 触发与 FIX-9 既有保存/恢复语义不变。
- **M51-2 默认编辑器**：内嵌 `config.toml` / `keymap.example.toml` / `opener.example.toml` 中的 `edit_text` 默认命令由 `nvim` 改为 `vim`。
- **M51-3 文本类型识别**：补齐 `.vifm`（以及 `.vim` 等配置文本）与**无扩展名文本文件**（如 `LICENSE`）的识别——扩展名表补齐 + 有界内容嗅探（无 NUL 且 UTF-8 可解码即文本），使 `text/*` opener 规则命中并调用 `edit_text`（vim），不再落到系统 opener/无反应。不引入新依赖，读取有界（≤4KB）。
- **M51-4 粘贴冲突弹窗提示**：`p`/`P` 同名冲突的阻塞确认弹窗必须显示按键提示（`y`/Enter 确认、`n`/Esc 取消、`j/k` 浏览），与删除确认的窗口底栏一致；不得只显示文件列表而没有提示行。
- **M51-5 文档收尾**：GUIDE / help / CHANGELOG / ACCEPTANCE / NOTES 同步本批次，保持 harness_check 通过。

**SIMPLIFIED**
- M51-1 暂停标志用进程内原子量，不引入桌面级监听；tmux 内无焦点事件时以「外部窗口 spawn 时暂停、收到任意按键时恢复」为准。
- M51-3 内容嗅探只读前 4KB；路径不可读/二进制时回退既有分类，不改变 `pick` 与预览同源。
- 所有改动沿用既有键位/主题/TestBackend 测试路径，不引入必需外部依赖。

**OUT_OF_SCOPE**
- 不移除「可执行文件 Enter 仅提示」特例；不把外部编辑器作为必需依赖；不改变 frecency/撤销栈/归档只读边界。
- 不 bump 版本、不推送 tag、不触发 GitHub Release。

## M53 — 标签栏对齐、归档 VFS 预览与预览性能（2026-11-26 蒸馏，已确认）

> 来源：用户 2026-11-26 需求，确认「理解正确」且明确：① `r` = 删除当前条目的 frecency bookmark/mark 记录；
> ② 归档内 `l`/Enter 允许系统 opener 打开**临时只读副本**。沿用「不 bump / 不 tag / 不 Release」。

**MUST**
- **M53-1 标签栏全路径**：标签标题改为 `[序号:焦点栏全路径]`（如 `[1:/etc]`），随焦点切换同步更新；路径用 `/` 分隔，归档/远端用既有 `host:inner` / `remote:path` 显示；溢出滚动沿用同一几何。
- **M53-2 标签状态持久化**：`:tabname <名>` 修改的标签名**持久化到会话**并在重启后保持（无参清除）；各标签**右侧 pane 的当前活动项（location + cursor）**记录并在重启后恢复；会话结构增字段须 `#[serde(default)]` 保持向后兼容。
- **M53-3 归档 VFS 预览/打开**：归档虚拟目录内文件支持 `w` 预览；`l`/Enter 将该成员解到**临时只读副本**并交系统 opener 打开；不写归档内，临时副本登记清理（复用 temp_opens），大成员沿用 64MB 护栏。
- **M53-4 frecency NORMAL 提示与 r 键**：NORMAL 底部提示显示**全部**可用键位及说明（至少 `dd`/`u`/`yf`/`yn`/`yc`/`gi`/`e`/`w`/`l`/`Tab`/`Esc`）；新增 `r` 删除当前光标条目的 frecency bookmark/mark 记录（与 `dd` 同源 Remove 墓碑），不删对应文件。
- **M53-5 文本预览**：快速态文本按**行**截取至少 36 行（至少充满预览区），替代 M49-9 的 300 字截断；排查并修复小文本文件（如 JOURNEY.md）预览卡顿（有界读取 + 缓存，避免每帧重复解析/高亮）。
- **M53-6 foot 原生图片预览**：修复 tmux/foot 下被强制 halfblocks（马赛克）的问题——仅在实际未启用 passthrough 时降级；启用 passthrough 时走 `ratatui-image` 协议探测；保留 M50 启动性能修复（不在无 passthrough 环境阻塞查询）。
- **M53-7 大归档误提示**：`.deb`（及同类）不再提示「归档较大」——补 `dpkg-deb -c` / `ar t` 外部列目录回退；列表场景避免 64MB 护栏误伤；成员数/输出上限保留。
- **M53-8 真异步/防抖预览**：快速 `j/k` 导航时不生成中间预览（避免卡顿），停下后再渲染最终目标（图片仍完整预览）；用目标路径 + 稳定时间戳/生成序号实现，不引入新线程或必需依赖。
- **M53-9 文档收尾**：GUIDE / help / CHANGELOG / SPEC / ACCEPTANCE / STATE / PLAN / NOTES 同步；质量门禁全绿、未发版。

**SIMPLIFIED**
- M53-2 右栏恢复仅当前活动项（location + cursor），不持久化完整 cursor_memory（保留内存态语义）。
- M53-3 临时副本放在系统临时目录，退出时尽力清理；系统 opener 失败给可读提示。
- M53-5 缓存仅内存态（路径 + mtime + size 键），与 ArchivePreviewCache 同模式。
- M53-6 以 tmux `allow-passthrough` 判定 + 显式 env 覆盖实现，不做全局桌面焦点监听。
- M53-8 防抖阈值约 120–200ms；不引入新 TUI 框架。

**OUT_OF_SCOPE**
- 不写入归档内成员；不做嵌套归档递归浏览；不引入 fzf/SQLite/新必需依赖。
- 不 bump / tag / GitHub Release。

## M54 — vifm z 系列快捷键（2026-11-26 蒸馏，按顺序执行）

> 范围决策：内置 `za/zo/zm`（隐藏文件）、`zR/zM/zO`（过滤器临时重置/恢复）、`zt/zz/zb`（视口定位）；
> 不内置 `[z / ]z / zj / zk`（兄弟目录跳转，仅 keymap 示例）。只 Normal 模式生效，
> 不覆盖其它模式的 z 语义。沿用不发布口径。

**MUST**
- **ZKEY-1 声明/实现排查**：扫描 GUIDE/help/Keymap 默认表/Action 枚举，列出「文档或键位表声明但按下去无效果」的键，判定 handler 缺失 / 未注册 Resolver / 文档错误，在 NOTES 写「声明 vs 实现」对照表（修复/移除文档/defer）；本轮只修 z 系列，非 z 系列登记后续。
- **ZKEY-2 隐藏文件**：`za` 切换、`zo` 显示、`zm` 隐藏；与 `:set showhidden` / 现有 `show_hidden` 共享**同一状态源**，不引入第二份开关；切换后光标尽量保持原文件或最近可见邻居；状态栏轻量提示（`hidden: on/off`）。
- **ZKEY-3 过滤器重置**：`zR` 保存当前过滤器状态（含隐藏规则与永久过滤器）为**会话内临时快照**并临时全部可见；`zM` 恢复该快照（无快照给轻量提示、不报错）；`zO` 清空永久过滤器（不动隐藏开关）。快照不写配置、退出不保留。
- **ZKEY-4 视口定位**：`zt`/`zz`/`zb` 将光标文件置于视口顶部/中央/底部，**不改变光标索引**；尊重 `[ui] scrolloff`（无则新增，默认 0，zt/zb 计算时扣除）；条目不足一屏时无视觉变化但不报错。
- **ZKEY-5 注册与文档**：Keymap 默认表注册 9 个两键序列并保证 Resolver 识别；GUIDE 列出语义与「受 scrolloff 影响」，明确不内置 `[z/]z/zj/zk` 并给自定义示例；ACCEPTANCE 补 ZKEY-2..4 手工验收；NOTES 记录取舍理由。

**SIMPLIFIED**
- 光标保持策略：优先原文件，消失时取最近可见邻居（不做复杂历史栈）。
- `scrolloff` 仅 zt/zb 使用；普通 j/k 滚动行为不变。
- 两键序列复用既有 Keymap Resolver，不新增输入模式。

**OUT_OF_SCOPE**
- 不内置兄弟目录跳转；不引入 Vim 脚本/插件；不 bump / tag / Release。

## M57 — `rv help` 外部入口收敛（2026-11-26 蒸馏，已确认）

> 用户确认：移除 `rvifm help` 子命令（外部入口），帮助只在应用内执行。
> 沿用「不 bump / 不 tag / 不 Release」口径。

**MUST**
- 从 `src/main.rs` 移除 `help` 子命令分派与 usage 中的外部入口；`app::help_text()` 仅保留给应用内 `:help` 覆盖层与查看器使用。
- 同步内置 help / GUIDE / CHANGELOG / ACCEPTANCE / TASKS / STATE / NOTES，移除「`rvifm help` 可单独输出帮助」等外部入口描述。
- 集成测试不再断言 `rvifm help` 子命令；`--version` / `--help` 冒烟不变。

**OUT_OF_SCOPE**
- 不改变应用内 `:help` 的不透明覆盖层、`/` 搜索与查看器语义。
- 不 bump / tag / Release。

## M55 — frecency 搜索态区分与实时递归模式（2026-11-26 蒸馏，已确认）

> 用户确认按编号执行 M55。沿用「不 bump / 不 tag / 不 Release」口径。
> 链接讨论结论：列表保持无边框；改为**输入行加边框 + 光标 + 标题带 query** 区分 Normal/Search，
> 避免用户在 Normal 下误以为处于搜索态；`O` 递归模式复用 `:find` 遍历后端、后台流式扫描、
> 增量子串过滤，不替代 `:find`。

**MUST**
- **M55-1 搜索态视觉区分**：frecency 面板输入行在 Search 态加边框与闪烁/可见光标、标题带当前 query；Normal 态明确区分（无输入框光标/边框提示）；**列表区保持无边框**（不引入列表边框）；`j/k` 语义不变。
- **M55-2 递归扫描后端**：`O` 在 frecency/Normal 下触发实时递归文件查找；复用 `:find` 的目录遍历与 ignore 规则；后台线程 + channel 流式产出；深度上限 20、条目上限 5 万护栏，超限在状态栏提示；不写 frecency redb、不改 `global_tick`。
- **M55-3 递归交互**：维护 `all_entries` / `filtered_entries`，输入时对已扫描条目做**增量子串过滤**（不做评分）；状态栏显示 `scanning: N` / 最终总数；Enter 跳转选中项并记为一次真实 cd/打开（走 `record` 唯一写入入口）、Esc 退出；`O` 不替代 `:find`（`:find` 保持原语义）。
- **M55-4 文档与验收**：GUIDE/help/ACCEPTANCE/NOTES 同步；记录 `O` 与 `:find` 关系、护栏取值、搜索态视觉方案；质量门禁全绿，未发版。

**SIMPLIFIED**
- 过滤只做大小写不敏感子串匹配，不做模糊评分/正则。
- 扫描结果仅内存，不写盘、不进 frecency 直到用户真实选中跳转。
- 光标渲染复用既有输入行绘制路径，不新增独立 TUI 引擎。

**OUT_OF_SCOPE**
- 不引入 fzf 二进制/外部 walker 依赖；不递归进归档内部；不 bump / tag / Release。

## M56 — 向导模式（tutor 重构，2026-11-26 蒸馏，已确认）

> 用户确认：标题优选「向导模式」；成就徽章纳入本批；进度与版本解耦。
> 沿用「不 bump / 不 tag / 不 Release」口径。共 12 任务，按依赖顺序执行。

**MUST**
- **M56-1 术语统一**：tutor 在 UI 标题、GUIDE/help/命令描述中统一称「向导模式」；`:tutor` 命令保留。
- **M56-2 首次自动进入**：仅当启动时**未读取到任何配置文件**视为首次打开，自动进入向导；已有配置则正常进目录浏览。
- **M56-3 对话框几何**：向导不再全屏，改为宽/高各占终端 80% 的欢迎对话框；终端 ≤40 列或 ≤10 行时自动降级为全屏（避免过小不可读）。居中显示。
- **M56-4 底部进度状态栏**：对话框底部更高状态栏，显示欢迎提示（可学习全部功能、`:tutor` 可再次进入）与总体进度 `模块 N/M 功能点 A/B 已掌握本系统 X% 的功能`；进度带边框进度条。
- **M56-5 搜索实时更新**：`/` 搜索输入即时过滤，无输入时显示全部条目（Edit-as-you-type，不按 Enter 才生效）。
- **M56-6 错题过滤修复**：进入**有记录**的模块时只显示错题，且编号与此前保持一致（不能被重排）；无记录模块显示全部。
- **M56-7 模块完成提示**：模块全部结束后显示 `Enter=继续学习其它模块`，按 Enter 退出当前学习并执行 `:tutor`（回模块列表）。
- **M56-8 100% 模块重入**：已 100% 模块再次 `l`/Enter 进入时提示「本模块已全部掌握，重置后将重新显示所有条目。是否重置？」；取消不进入，确定则重置该模块进度后重新学习。
- **M56-9 进度持久化跨版本**：进度数据与 rvifm 版本解耦；条目被移除/重命名时按**稳定字符串 ID**忽略未知 ID，不报错、不丢失其它进度；旧格式（按索引）能优雅迁移或忽略。
- **M56-10 同步机制**：在 `docs/NOTES.md` 记录「向导条目 ↔ ACCEPTANCE 验收项映射表」；新增规则：ACCEPTANCE 变更时必须检查向导条目是否需同步（文档职责边界 + harness_check 校验）。
- **M56-11 成就徽章**：完成首个模块 🏅「初窥门径」；全部模块 100% 🏆「rvifm 大师」；零错题通过 ⭐「完美通关」；状态栏角落显示小图标。
- **M56-12 文档与验收同步**：GUIDE/help/ACCEPTANCE/CHANGELOG/STATE/PLAN/NOTES 同步；质量门禁全绿，未发版。

**SIMPLIFIED**
- 对话框进度条为单条文本进度条（带边框），不引入新绘图引擎。
- 进度文件沿用现有格式，以字符串 ID 为主键扩展，兼容旧索引文件（忽略未知项）。
- 徽章仅状态栏图标展示，不解锁额外功能。

**OUT_OF_SCOPE**
- 不引入外部教程引擎/网络内容；不 bump / tag / Release。

## M58 — 命令补全增强（2026-11-26 蒸馏，用户已确认）

> 推翻 M50 的边界决策「只读取命令名、不读参数/路径」→ 改为读取**完整命令行**作为补全候选
> （用户 2026-11-26 确认接受隐私文档改写）。沿用不 bump / 不 tag / 不 Release。

**MUST**
- **M58-1 补全浮层**：标题增加 `Space=选定到输入栏` 提示；内置命令描述**始终显示**；输入时**实时过滤**候选（不按 Tab 才刷新）。
- **M58-2 光标闪烁**：命令行输入行显示闪烁光标（复用事件循环 tick，不引入新线程）。
- **M58-3 Space 选定**：补全浮层中按 Space 将当前候选定到输入栏（含参数占位），可继续编辑。
- **M58-4 Tab 循环三 bug**：①在可用空间内尽量显示全部候选、超出显示滚动条；②循环到不可见项时自动滚动使其可见；③选中索引变更时同步更新描述文本（索引与描述同源）。
- **M58-5 外部命令策略**：补全候选读取 shell 历史的**完整命令行（含参数/路径）**，不再只读命令名；frecency `cmd` 写入口径与隐私文档同步改写。
- **M58-6 多行外部命令**：在顶部/预览区显示多行命令内容，避免单行截断。
- **M58-7 返回码提示**：外部命令执行完成后在状态栏显示返回码，成功/失败可辨。
- **M58-8 `rvifm cmd` 外部入口**：新增独立调用（对标 fzf Ctrl+R）；`rvifm pick` 仍保持纯只读，不写 redb/不更新 `global_tick`。
- **M58-9 文档与验收**：GUIDE/help/CHANGELOG/ACCEPTANCE/STATE/PLAN/NOTES 同步，质量门禁全绿，未发版。

**SIMPLIFIED**
- 光标闪烁由渲染层按事件循环 tick 推导，不新增后台线程。
- `rvifm cmd` 复用 `pick` 的只读选择器框架，仅新增命令行来源与输出。

**OUT_OF_SCOPE**
- 不引入 fzf 二进制；不修改 `record` 唯一写入红线；不 bump / tag / Release。

## M59 — 预览与归档 VFS 体验修复（2026-11-26 蒸馏，用户已确认）

> 用户确认：拆分登记；`r` 只清 bookmark/mark、保留条目与分数并可由 `u` 恢复；
> frecency `Tab` 固定进/出预览；图片协议允许默认后台查询。沿用不发布口径。

**MUST**
- **M59-1 预览同源**：`rvifm pick` 改用 TUI 预览渲染链（保持只读）；锁定 frecency / `w`/`:view` / 归档成员同源测试。
- **M59-2 归档预览导航**：archive 成员的 `w` 预览 j/k 可滚动、`Tab` 可进/出预览（修 `activate_viewer` 对 archive 取 `preview_path()=None` 导致 `line_count=0`）。
- **M59-3 归档预览高亮**：确认临时副本保留成员扩展名，语法高亮与常规文本一致。
- **M59-4 归档 opener**：归档成员 `l`/Enter 改走 `opener::open_with_layers`（现为 `system_open`，绕过 `[opener]`/`[open]` 规则）。
- **M59-5 快速预览行数**：`max(36, 视口可见行+10)`，保证充满预览区。
- **M59-6 卡顿与二进制闪现**：渲染 committed `viewer.path`；缓存命中零克隆；按视口读取/高亮；复用 Highlighter。
- **M59-7 预览焦点视觉**：预览获焦时蓝色边框（沿用 theme `focused_border`）+ 明显高亮。
- **M59-8 frecency Tab**：`Tab` 固定进/出预览；左右栏切换改由其它键承担（用户确认保留 Tab 进预览）。
- **M59-9 r/u 语义**：`r` 仅清 keyword/mark（保留条目与分数），带确认；`u` 连 keyword 一起恢复；不新增 redb 写入口。
- **M59-10 文档与验收**：GUIDE/help/SPEC/ACCEPTANCE/STATE/PLAN/NOTES 同步 + 质量门禁。

**OUT_OF_SCOPE**：不写归档内成员；`pick` 不写 redb；不 bump / tag / Release。

## M60 — 真异步预览架构（2026-11-26 蒸馏，用户已确认）

> 依赖 M59；generation token 同时解决快速导航中间态与图片加载卡顿。

**MUST**
- **M60-1** `preview_tx/rx` + `preview_generation` 后台 worker，事件循环只接受 generation 匹配的结果。
- **M60-2** 文本读取/切行/高亮移入 worker；缓存改引用/Cow，消除整文件克隆。
- **M60-3** 图片解码移入 worker（UI 线程只做协议编码）；快速切换不渲染中间图片。
- **M60-4** 归档列目录、Office、PDF 缓存未命中移入 worker。
- **M60-5** 性能验收 + 文档 + 门禁；不 bump / tag / Release。

**SIMPLIFIED**：不新增异步运行时，复用 `std::thread` + `mpsc`（与 git/jobs 同源）。

## M61 — 大文本视口虚拟化（2026-11-26 蒸馏，用户已确认）

> 用户确认：自建 `memmap2` 视口读取器（不引 `vix-large-file`/`qem`）；阈值 **1MB**；
> 远端大文件仅预览元数据。沿用不发布口径。

**MUST**
- **M61-1** 新增 `[preview] virtual_text_threshold`（默认 1 MiB）与 `src/viewport.rs`（mmap + 稀疏行检查点 + 视口按需读取，纯只读）。
- **M61-2** 工作线程对超阈值文件不再 `read_to_string` 全量，仅判定 hex/估算行数；AppState 持有视口读取器。
- **M61-3** 渲染层对大文件走视口行，取消「快速截断→聚焦全量」两步；小文件保持现状。
- **M61-4** `Highlighter` 跨视口状态尽力而为（准确性可让步，流畅性优先）。
- **M61-5** 远端大文件仅元数据提示，不下载、不写回。
- **M61-6** 文档/验收/门禁。

## M62 — 图片缩放与编码流水线（2026-11-26 蒸馏，用户已确认）

**MUST**
- **M62-1** 新增 `fast_image_resize`（默认 `Nearest`，可选 `Box`），在 worker 内把解码图缩到目标像素。
- **M62-2** resize 结果交 UI 只做协议编码；缓存已编码结果（path+mtime+size+视口尺寸）。
- **M62-3** 文档/验收/门禁。

## M63 — 其它预览增强（2026-11-26 蒸馏，用户已确认）

**MUST**
- **M63-1** 目录树预览缓存（不再每帧 `build_tree_entries` 重扫）。
- **M63-2** 新增可选 `pdf-rust` feature 引入 `karet-pdf`：文本仍先行，视觉路径优先纯 Rust 栅格化；默认构建不含该依赖。
- **M63-3** JSON 折叠缩进 / Markdown 弱化标记的轻量结构化预览。
- **M63-4** 二进制 hex 已有，仅记录「文件头语义注释」为 deferred（低优先）。
- **M63-5** 文档/验收/门禁。

## M64 — 向导修仙境界分级（2026-11-27 蒸馏，用户已确认）

> 用户确认：11 级（入门→真仙）；符号+中文界面；筑基严格绑定 `core_nav` 完成；
> 每级再分初期/中期/后期/圆满；境界只在学习进度栏展示；`tutor_meta` 模块置顶，
> 首次进入先学使用方法。沿用不发布口径。

**MUST**
- **M64-1** 纯函数 `tutor_rank(progress)`：11 级 + 四小阶段；筑基需 `core_nav` 全部掌握；真仙需 100% 且无错题。
- **M64-2** 学习进度栏显示 `符号 境界[小阶段]`（中文），模块列表不显示。
- **M64-3** `tutor_meta` 移到内嵌模块表首位（列表第一个、首次默认光标 0 即它）。
- **M64-4** 文档/验收/门禁，未发版。

**OUT_OF_SCOPE**：不改进度文件格式；不新增依赖；不 bump / tag / Release。

## M65 — 境界起点改名与 CI 跨平台修复（2026-11-27 蒸馏，用户已确认）

> 用户确认：境界第 0 级由「入门」改为「入门」（能打开软件即算入门）；
> 修复 GitHub CI `test (windows-latest)` 失败，并检查其它冲突。沿用不发布口径。

**MUST**
- **M65-1** 境界 0 级改名「入门」（符号 `○` 不变），全量同步代码/文档/测试/验收。
- **M65-2** Windows CI：含 `std::os::unix::fs::PermissionsExt` 的测试加 `#[cfg(unix)]` 门禁。
- **M65-3** macOS CI：修正 4 个长临时路径导致的平台相关渲染断言（tab 可见项、过滤指示、查看器标题）。
- **M65-4** 文档/验收/门禁，CI 三平台跑绿，未发版。

## M66 — tutor_meta 与安全提示完善（2026-11-27 蒸馏，用户已确认）

> 用户确认：改写现有 3 项（保持 102 项）；「重要的事说三遍」；`Ctrl+Q` 明确 `:tutor` 重进；
> 14 个模块 TOML 的「导师模式」统一为「向导模式」。沿用不发布口径。

**MUST**
- **M66-1** `tutor_meta` 前三项加入三遍安全提示：向导期间的文件操作是**真实操作**，练习务必小心。
- **M66-2** `tutor_exit.expected` 明确 `Ctrl+Q` 退出后可 `:tutor` 重新进入。
- **M66-3** `tutor_meta` 其余项同步最新语义（错题仅显示且编号不变、模块完成 `Enter` 回列表、`Tab` 进预览）；14 个 TOML 统一「向导模式」称谓，稳定 ID 不变。
- **M66-4** 文档/验收/门禁。

## M67 — 向导沙箱（2026-11-27 蒸馏，用户已确认）

**MUST**
- **M67-1** 新增沙箱目录 `std::env::temp_dir()/rvifm-tutor/{left,right}`，跨平台；进入 tutor 会话时若不存在则创建。
- **M67-2** 初始化样例文件（文本/Markdown/JSON/二进制）于左右两目录。
- **M67-3** 进入 tutor 会话时左 pane 定位 `left/`、右 pane 定位 `right/`，焦点左。
- **M67-4** 沙箱内操作**不写 frecency**（路径前缀排除 `record`），但**保留撤销栈**以便练 `u`；会话恢复遇已清理路径时重建。
- **M67-5** 文档/验收/门禁。

## M68 — 全面审计与漂移治理（2026-11-27 蒸馏，用户已确认）

**MUST**
- **M68-1** 修正 README：vifm `:view` 有内建文本/目录快速预览（富格式/图片才依赖外部工具）；13 模块 100 项 → 14 模块 102 项。
- **M68-2** 更新 NOTES 向导↔ACCEPTANCE 映射表至 M66–M68；harness 规则 9 校验 README/GUIDE/help 无过期数字与错误 vifm 表述。
- **M68-3** 功能列表/快捷键/使用手册/tutor 条目一致性扫描并修正发现项。
- **M68-4** 文档/验收/门禁 + CI，未发版。

## M69 — :diff 只读浏览（Git P1，2026-11-27 蒸馏，用户已确认）

> 用户确认：对齐 lazygit 的 a/A 语义；J/K 跳差异块、j/k 逐行；`|` 左右对照、
> `=` 对照模式下显示完整文件（ziggity 惯例）；`^w` 切软换行（默认开）。
> 项目沿用 git CLI 零编译期依赖路线，diff 由 `git diff` 输出自解析，不引入 `similar`/`diffs-rs`。

**MUST**
- **M69-1** 新增 `src/gitdiff.rs`：解析 `git diff`/`git diff --cached` 的统一补丁为 hunks（行类型、新旧行号），列表/对照渲染纯函数。
- **M69-2** `:diff` overlay：左栏变更文件列表（j/k/gg/G/5j），右栏补丁；`Tab` 切焦点；异步 worker + generation 防过期。
- **M69-3** 右栏：`j/k` 逐行、`J/K` 跳差异块、`/`/`n/N` 搜索、`h/l` 横向（对照模式）、`|` 统一↔左右对照、`=` 仅差异↔完整文件、`^w` 换行开关（默认开）、`Esc/q` 退出。
- **M69-4** 文档/验收/门禁，未发版；不替代 `gi`/`:gitui`。

## M70 — Git 文件级写操作（Git P2，2026-11-27 蒸馏）

**MUST**
- `a` stage/unstage all；`A` amend last commit（lazygit 语义，仅 overlay 内）；`d` discard 当前文件（确认）；`c` commit（输入框）；`p` pull；`P` push；`Tab` 切面板；状态栏回显结果/退出码。
- 新增 `[git] confirm_destructive`（默认 true）；所有写操作经确认，失败/取消不改变仓库。
- 文档/验收/门禁，未发版。

## M71 — 块级 hunk 丢弃（Git P3，2026-11-27 蒸馏，最后实现）

**MUST**
- 左右对照/补丁面板中 `D` 丢弃当前 hunk：先预览+确认，执行后状态栏提示「按 u 撤销」，立即 `u` 可恢复原文件。
- 标注实验性，写失败/取消不留半完成状态；文档/验收/门禁，未发版。

## M72 — :diff 实测修复（2026-11-27 用户实测，已确认问题）

**MUST**
- 状态栏快捷键提示放进真正的 status area，不覆盖左右 pane 边框/内容。
- 打开 `:diff` 即异步加载当前光标文件的补丁（不等 `Tab` 进右栏）；过期 generation 结果不得永久把路径留在 pending。
- 补丁区标题只显示路径与视图标记，状态消息只进状态栏，不再串入标题。

## M73 — 界面交互一致性（2026-11-27 蒸馏，用户已确认）

**MUST**
- 顶部标签底色比路径行更深一档，避免连片（主题派生、不破坏纯函数测试）。
- 失焦 pane 的选中项用浅灰底而非反显/残留高亮；可视选区同步判焦点。
- 预览获焦不再叠加第二层蓝框，改为直接给现有 pane 边框上蓝色焦点样式。

## M74 — 向导提示与遮罩（2026-11-27 蒸馏，用户已确认）

**MUST**
- prompt/expected 长文本按面板宽度换行/截断，不被底部键位提示覆盖。
- `:tutor` 列表对话框不整帧 Clear：只清对话框矩形并用不透明底色填充，保留双栏上下文；确认弹窗同口径。

## M75 — frecency 交互与算法对齐（2026-11-27 蒸馏，用户已确认）

**MUST**
- Normal `/` 只进入 SEARCH 且保留 query；`c` 清空并进入 SEARCH；底部只有一个 `/`。
- 删除 `BookmarkOnly` 兜底：无匹配的书签不再无条件显示；对齐上游 keyword 命中优先但需真实匹配。
- Normal `v` 多选（范围高亮 + 批量 dd/u）。
- 搜索输入框改为无边框/无标题、只显示 `/{query}`（参考 `pick`）。

## M76 — frecency 数据与预览策略（2026-11-27 蒸馏，用户已确认）

**MUST**
- 清理 **frecency 存储中 path 以 `/tmp/` 开头的记录**（不是删除本地临时文件）。
- `l`/Enter 真实跳转后按 topic 写 record；`r` 清 keyword、`u` 同步恢复 keyword/整条删除并保留分数。
- 条目右侧显示评分；新增 `[preview] frecency_preview`（默认 false），`w` 在 frecency Normal 开启预览。

## M77 — tutor 进度与徽章修正（2026-11-27 蒸馏，用户已确认）

**MUST**
- 移除 🏅/🏆/⭐ 徽章，只保留修仙境界；进度条充满状态栏容器并作为进度文字背景，去掉右侧数字进度。
- 面板右侧计数改为「已掌握总数 / 模块完整条目数」（来自持久化进度），不再用本次 session 子集/本次作答数。
- 进入有记录模块只显示**错题 + 未学习**、排除已掌握，并保持原模块编号；skipped 清空但仍有未见项时不得回退显示全部；全掌握仍走重置确认。

## M78 — 默认 opener / IME / 命令描述修复（2026-11-27 蒸馏，用户已确认）

**MUST**
- 生成配置与 `Config::default()` 内置 `vim %s` 文本规则（缺 vim 仍回退系统 opener）。
- IME 防御性修复：补齐输入模式、force 前保存、恢复失败保留 saved、所有事件恢复、`suspend_execute` 成对 suspend/resume，加 trace 日志。
- 拆分 `:tabnew`/`:tabclose` 等合并帮助行，解析全部 `:name`；内部命令空描述不再回退「文件/路径」。

## M79 — git 状态列缓存失效修复（2026-11-27 用户实测，已确认）

**MUST**
- git 写操作（a/A/d/c/p/P / hunk discard）后清空 `git_cache` / `git_pending` / 时间戳缓存。
- git 目录状态缓存加 TTL（默认 3 秒）；超时由 `sync_status_trigger` 重新查询，不依赖 watcher 感知 `.git`。
- 文档/验收/门禁，未发版。

## M80 — 焦点样式重做（2026-11-26/27 用户纠正，已确认）

> 用户纠正：M73 对「标签调暗/失焦灰底」的理解有误，全部撤销。

**MUST**
- 焦点标签用主题选中背景色（偏白）+ 文本反色，与焦点栏路径标题背景区分；非焦点标签保持普通样式。
- 左右栏/预览区焦点通过标题栏 + 边框明暗变化标记；选中项背景同源，失焦侧更浅。
- 预览区改为与目录栏同风格标题栏 + U 型边框（左/右/下，顶部开口由标题承担），边框色与标题栏同色；不再叠加第二层蓝框。

## M81 — 焦点样式细调（2026-11-27 用户实测，已确认）

**MUST**
- 非焦点标签无背景、文字统一白色；焦点标签白底黑字反色。
- 预览获焦时对侧目录栏标题栏走失焦样式；左右目录栏焦点语义同步。
- 预览标题栏下方绘制完整方框（不再 U 型三边），边框用 `:diff` 弹窗同一套 `pane_border_style`，失焦为灰色。

## M82 — git diff 体验（2026-11-27 蒸馏，用户已确认）

**MUST**
- 补丁/对照视图按路径做 syntect 语法高亮；统一视图与对照视图同源。
- `|` 切换统一 ↔ 左右对照时支持全屏对照；两个相邻 Block + 连续分隔线，不再字符串拼接断线。
- 行级增删底色 + 词级差异高亮（新增纯 Rust `similar` 依赖）；`git diff` 上下文 -U1（可配置）。
- `=`/`^w` 保留 ziggity 语义；不引入 `hunk`（Node.js 应用，仅参考交互）。

## M83 — 文本预览两段式消除（2026-11-27 蒸馏，用户已确认）

**MUST**
- 文本不再「快速截断→获焦全量」：始终视口渲染 + 始终语法高亮（对齐本机 vifm `batcat --color=always -n --line-range=:200`）。
- `/` 查询不关闭语法高亮，改为叠加黄底；高亮缓存 key = `(path,mtime,size)`；扩展名优先 shebang。
- `G` = `line_count - 视口高度`；缺失文件首行只显示「文件不存在」，抑制二进制/不可读面板。
- 小文件也走轻量视口/`Arc` 缓存，避免每帧整文件 clone/逐行分配。

## M84 — 归档成员打开对齐（2026-11-27 蒸馏，用户已确认）

**MUST**
- 归档成员 `l`/Enter 与物理文件同一路径：`text_like` 且 `terminal_in_new_window` 时新窗口打开；否则 pending_external；成功后 `record_file_open`。

## M85 — 图片流水线（2026-11-27 蒸馏，用户已确认）

**MUST**
- `ThreadProtocol` 把 resize+encode 移出 UI 线程；图片 job 独立 generation；缓存矩形含坐标；修复 pending 卡死。
- 像素上限默认贴近 vifm（预缩放最长边 1600，宽度由预览 pane 约束）；显式 `Nearest`；协议查询提供开关（`image_protocol_query`，超时数值由 ratatui-image 固定不暴露）。
- **OUT_OF_SCOPE（2026-11-27 范围收缩，见 NOTES）**：磁盘缓存已编码协议——ratatui-image 不公开编码字节，除非 fork 或自写编码器；登记为 M85-1b deferred。

## M86 — test.md 实测反馈批次（2026-11-28 蒸馏，待用户确认）

来源：`/home/dlt/Downloads/test/test.md`（基于本地 debug 二进制 `/home/dlt/Source/rvifm/target/debug/rv`）。

**MUST**
- **预览/`:view`**：`.json` 不再误判二进制；按本机 vifm `vifmrc` 的 `text/*` 定义覆盖全部文本类型（含无扩展名场景）；`:view` 标题简化为「路径 · 预览」（对齐 frecency 清爽模式）；归档/大文件不再用 `size/80` 估算行数与「已截断」提示（实测 11MB zip 显示 `1/11692098`，实际仅 292 行）。
- **frecency**：SEARCH 标题为带边框输入框，显示关键词 + 闪烁光标，边框随焦点/失焦与其它组件同步；预览禁用逻辑分场景——rvifm 中对侧保持原面板、不被空白遮罩覆盖，frecency 保持 popup 置顶且原 tab 焦点快捷键失效，`w` 激活预览栏；独立 `pick` 搜索框充满 popup 为单栏，`w` 进入阅览双栏。NORMAL 多选后 `d` 删除（与目录列表一致）、`dd` 仅删当前条目。核实并修复：`l`/Enter 打开的条目未追加统一库/file（无返回状态时打开前异步记录）；`r` 仅移除 bookmark 不删条目，`u` 应恢复移除的 bookmark。
- **git diff**：复核 `|` 全屏对照未生效（左半空白、右半显示左栏、原右栏出屏）；若布局无法修复，改用 popup 覆盖原 diff 内容的对照方案（用户建议）。
- **tutor**：去「学习进度/实际进度」间小圆点改空格；不同境界图标按级别着色（自行参考设定）；底部快捷键提示移入进度方框底部（不再压框线）；去掉进度条；tutor 外框统一为焦点样式；学习进度去边框、顶部用一条分割线与上方分隔，获奖标题置于分割线下方左对齐（评估推荐）；长提示自动换行时不再覆盖按钮提示，提示区与按钮区间加竖线分隔并右移。
- **archive**：`:archive` 产物应落在焦点 pane 选中文件 → **对侧 pane**；核实并修复「又生成到同侧」。
- **多选**：`t` 标记条目需可见高亮；`d`/`D` 对 `t` 多选条目触发删除（与目录列表一致）。
- **V 键**：删除 `V` 快捷键（`v` 已覆盖可视模式；与 SPEC M24-3「行可视」声称互斥的漂移一并清理）。
- **粘贴冲突**：同路径粘贴提供 `rename` / `skip` / `skip all`；异路径粘贴在此基础上增加 `append the tail` / `overwrite` / `overwrite all`。

**OUT_OF_SCOPE / 决策**
- 用户确认 `V` 无实际用途 → 移除；保留 `v` 单一可视模式，SPEC M24-3 / ACCEPTANCE M24-3 的「V 行可视」口径同步修正。
- tutor 获奖标题位置由本批次自决：分割线下方左对齐（不压线，避免视觉抖动）。

## M87 — 命令补全对齐 fzf/frecency（2026-11-28 蒸馏，待用户确认）

**MUST**
- 按 `:` 直接进入命令补全模式，自带顶部输入框；模式默认为 SEARCH；`Esc` 进入 Normal，Normal 下 `Esc` 退出补全模式。
- 数据源：输入首字母为 `!` 时使用外部命令列表，否则内部命令列表；SEARCH 态实时过滤，默认选中第一项。
- 导航对齐 fzf：移除 `Tab` 作为补全导航键；↑/↓ 与 `Ctrl+j`/`Ctrl+k` 上下移动列表（两种模式都可用）。
- Normal 态补齐 `j/k/gg/G/yy` 等导航/复制；外部命令列表（数据源为 `!`）评估并引入 `dd`/`u` 等记录删除/恢复能力（内部命令列表不支持且不可用）。
- 选定操作（用户 2026-11-28 拍板）：**有匹配项时** `Enter` = 把选中项填充到 `:` 命令输入区（可再次编辑后回车执行），`Ctrl+Enter` = 直接执行选中项；**无匹配项时**两者均直接执行当前输入内容。
- 外部命令历史**只记录执行成功的命令**（用户 2026-11-28 拍板）；核实当前行为，若失败/取消路径也会记录则改为成功后记录，使 Normal 态仅用于偶发删除误输外部命令。

## M88 — 长文件名省略策略（2026-11-28 蒸馏，用户要求调研后执行）

**调研结论（主流 TUI 文件管理器）**
- ranger：`linemode filename` 以 `<basename>...<size>` 呈现，`unicode_ellipsis` 控制 `...`/`…`；列宽由 `column_ratios` 固定，文件名在自身列内截断，不挤占大小列。
- vifm（本机 `vifm-help.txt:4753-4763`）：超长文本用 Unicode 省略号；pane 标题过长时活动栏左侧截断、非活动栏右侧截断，可用 `l`/`r` 强制方向。
- 结论：文件名应在**固定宽度的名称列内**截断并加省略号，大小/时间列保持固定，不给文件名让位。

**选定方案**
- `clip_name` 改为预算不足时**中间省略 + 保留尾部扩展名链**（如 `学习观 从感觉懂了到真正学会 B5.pdf.md5` → `学习观 从…B5.pdf.md5`），扩展名链按连续 `.<alnum>` 后缀识别（最多保留末 2 段），确保 `.pdf.md5`/`.tar.gz` 可辨。
- 极小预算（`<=1`）退化为尾部字符，不输出单独省略号；纯 ASCII 超长名同样适用。

**MUST**
- 名称列按预算截断并加 `…`，不再直接 `chars().take(budget)`；属性/大小/时间列宽度不变。
- 新增 `clip_name` 纯函数单测（含 CJK、`.pdf.md5`、极小预算）。
- GUIDE/help 同步省略策略；验收与门禁通过。

## M89 — 多选模型统一：v 选择 → t 提交标记 + J/K 标记移动（2026-11-28 蒸馏，待用户确认）

**调研依据**
- 本机 vifm `vifm-help.txt:824`：visual 模式下 Normal 键「作用于已选条目」；`t` 在 Normal 下是 toggle 当前文件标记（:623）；`av`/`Ctrl-G` 提供 append/remove/invert 选区编辑；count 走操作符（`d[count]j`）。
- yazi：`Space` toggle 当前项并自动下移一项；非连续多段 = `v` 选段 → `Space` 提交 → 移动 → 重复（官方讨论 sxyazi/yazi#1581）。
- ranger：`Space` 离散标记、`V` 连续 visual、`J/K` 仅半页移动；vim 只支持单一连续 visual，无 `J/K` 选择键。

**MUST**
- Normal `t`：保持现状（开关当前条目标记）。
- Visual `t`：把当前 visual 区间提交/翻转到焦点栏 `tags` 集合——区间内存在未标记项则全部标记，已全部标记则全部取消；**提交后退出 visual**（用户按推荐确认），便于继续选下一段。
- Normal `J` / `K`：标记当前条目并向下/上移动；支持 count，`N K` = 标记当前 + 向上移动 N 项（共 N+1 项，与 yazi `Space` 同款语义）；与 `t` 共用同一 `tags` 集合。
- 视觉统一：`t` 标记行改用与 `v` 相同的背景样式（`selected_row_style`），使「v 临时区间 → t 提交为持久标记」在视觉上不跳变；状态栏保留 `N tagged` 计数区分来源。
- 每栏独立的 `tags` 集合语义不变；`d`/`D` 与 `:unselect` 行为不变（前者删文件、后者只清标记）。

**OUT_OF_SCOPE**
- 不引入 vifm `av`/`Ctrl-G` 的 append/remove/invert 选区编辑状态机（用户选方案 2 的简洁路线，保留未来扩展空间）。
- 不改 `Space` 的切 pane 语义。

## 6. 待定事项

6.1 为已定稿项，作为执行依据，不再需要每轮决策；6.2 为仍未拍板项，执行到对应任务时必须先决策并写入 `NOTES.md`。

### 6.1 已定稿（2026-11-11）

- **许可证**：`MIT OR Apache-2.0` 双许可。前提是独立重实现，不复制、不逐行翻译 vifm 源码。
- **crates.io 名称**：`rvifm` 已确认可用（2026-11-10 查询返回 404）；若未来被占用，包名改用 `rvifm-tui` 或 `rvifm-cli`，二进制名始终保持 `rvifm`。发布时机为 M0 全部任务完成且 Linux + Windows CI 全绿之后，首个版本号 `0.1.0-alpha.1`，不发布空包占名。
- **Office 解析库**：`office_oxide`，固定 `~0.1.16`；PPTX 复杂对象按上述退化策略处理。
- **PDF 文本提取库**：`pdf-inspector =1.25.2`，关闭 `ocr` / `render-pdfium` / `vision` feature。
- **PDF 视觉后端（exprim，2026-11-11 定稿）**：
  - **非默认依赖**：`exprim = { version = "=0.4.0", optional = true }`，只在非默认 `pdf-visual` feature 下编译；**默认构建的依赖树不得包含 `exprim`**（用 `cargo tree` 验证）。`Cargo.lock` 入库以锁定完整传递依赖。
  - **探测顺序**：`pdf-inspector =1.25.2` 文本提取 →（启用 `pdf-visual` 时）`exprim =0.4.0` 栅格化 → 可选 `pdftoppm`（Poppler）/ `mutool`（MuPDF）→ 提示 + 系统 opener。不得跳过文本提取直接进入视觉渲染。
  - **护栏**：`ExportOptions.max_pixels` 限制在预览窗格级别（**200 万–400 万像素**，覆盖 exprim 默认的 1 亿像素预算）；限制输入 PDF 文件大小；release profile 保持 `panic = "unwind"`（不得设为 `abort`，exprim 依赖 `catch_unwind` 隔离渲染错误）。
  - **转正复评条件**（不阻塞当前方案）：见 [NOTES.md](NOTES.md) §4.8。
- **pending 合并职责（2026-11-19 定稿）**：`events.pending` 合并只在 `record` / TUI 启动路径执行；`pick` 走只读 open，不写 redb、不更新任何 tick。理由与放弃方案见 NOTES §10「AUDIT-1 方案拍板」；执行 AUDIT-1。
- **keyword 长度分工（2026-11-19 定稿）**：单字符 keyword = marks 两键导航（`'<char>`）；多字符 keyword = `o` / `:j` / `:J` 搜索的高优先级匹配。`:bmark` 等 vifm 命令与 `:keyword` 共用 frecency 数据，后者保留为兼容命令。见 M25-2 与 NOTES §10。
- **发布规则（2026-11-19 定稿）**：任务列表全部完成后自动发布 0.9.0；见 AGENTS §3 与 PLAN §4。
- **compaction 阈值（2026-11-19 定稿）**：默认 5000 条事件触发 compaction；惰性触发机制由 AUDIT-4 实现，阈值配置面随该任务落地。
- **短别名（2026-11-19 定稿）**：提供 `rv` 二进制别名，与 `rvifm` 并存；项目名、crate 名、主二进制名仍为 `rvifm`。

### 6.2 仍待定

（无。所有已知待定项均已定稿。）

> 已定稿不再列此处：keymap-rs 降级（自写 Trie，NOTES 2026-11-13）、tar 变体排期（M7-8/M26-4）、大目录性能阈值（M6-4 已验收）、pending 合并职责、keyword 长度分工、发布规则、compaction 阈值、短别名 `rv`（均 2026-11-19）。
