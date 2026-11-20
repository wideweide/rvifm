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
- `.7z` / `.rar` 不提供虚拟浏览，提示用户解压。
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
- `.7z` / `.rar` 标记为不支持虚拟浏览，提示用户解压后浏览。
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
- **M24-3 行可视模式 V**：`V` 进入行可视模式（选中**整行/目录**）；与 `v` 互斥（进入 V 退出 v，反之亦然）；j/k 扩展选区；d/y/x 对选中区域执行（复用 M3 批量操作管线）；Esc 退出。
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
- **M27-2 vifm 风格布局与配色**：默认去掉左右 pane 的整框边框，改为 vifm 式布局——标签栏 1 行、每个 pane 顶部路径行 1 行、双 pane 之间 1 列分隔线、底部状态行；用背景色而非边框区分：`TabLine`/`TabLineSel`、`TopLine`/`TopLineSel`（非焦点/焦点 pane 路径行）、`Border`（分隔线）、`StatusLine`/`CmdLine`、`CurrLine`/`OtherLine`、`Selected`；文件类型映射 vifm 组名（`Directory`/`Link`/`BrokenLink`/`Executable` 等），缺失时回退现有 `ft_*`/16 色；新增 `[ui] pane_borders`（默认 false；true 恢复旧整框外观）；主题样式存在时优先，未激活主题时使用 vifm Default 风格的 16 色回退。
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
- **M32-2 大文件传输断点续传**：M20-3 跨 VFS 上传/下载因断连中断时保留 `<target>.part` 与已传偏移；自动重连成功后从偏移继续传输；完成时原子改名（`.part` → 目标名）；显式取消（`x`）清理半成品（M20-3 口径）；撤销语义不变（入撤销栈为成品）。
- **M32-3 会话持久化范围扩展**：session.toml 保存/恢复 `Location::Archive { host, inner }` 与 `Location::Remote { remote, path }`；启动恢复时归档 host 存在即直接进入，远端尝试重连（失败降级为跳过该 tab + 状态栏可读提示，不 panic）；物理 tab 恢复行为不回归。
- **M32-4 M32 文档收尾**：help/README 补重连与续传行为说明；ACCEPTANCE §32 勾选后批次关闭。

**SIMPLIFIED**
- 续传仅覆盖上传/下载类传输（远端↔本地复制/移动）；偏移校验用远端文件大小（不做内容哈希）；不保证跨进程断电恢复（会话级续传）。
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

**硬约束**：不动已对齐的评分核心（events+snapshot+meta 三表、write-time 记录 + read-time 重放、dir/file 独立时钟、EMS 数学形式）；`pick` 保持纯只读；`c`/`v`/`e` 仅在 Frecency 模式下重映射，退出后恢复原语义（`v` 不得侵入 Visual、`e` 不与其他绑定冲突）；keyword 事件仍是元数据事件（不推进时钟、不加分）；容量清理永不删除带 keyword 的条目；IGNORE_PATTERNS 在 record 入口静默跳过（不追加事件）；Frecency 模式不新建独立预览渲染路径（复用 PreviewRenderer）。

**MUST**
- **M23-1 多词搜索与匹配分级**：`o`/`pick` 筛选升级——`MatchGrade`（Keyword > LastSegment > Substring）分级排序（grade 升序、score 降序）；多词查询 `split_whitespace`：ordered（按序命中路径）与 any-order（全词命中）两种模式；无命中降级现状子串过滤。
- **M23-2 keyword unbook 保留历史**：取消书签（`:keyword -`）只移除 keyword 元数据，**保留累积分数与历史**（现状核对：若已满足则登记验证）。
- **M23-3 容量清理与书签保护**：record 写事务/compaction 时检查 per-topic `max_entries`（默认 1500）；超限删除最低分的**未书签**条目至 90%；带 keyword 条目永不删除。
- **M23-4 IGNORE_PATTERNS**：record 入口 glob 匹配配置模式（如 `*/.git`、`*/node_modules/*`），命中静默跳过不追加事件；模式列表来自 TOML 配置（复用既有 glob 匹配能力，不引新 crate）。
- **M23-5 per-topic LAMBDA / MAX_ENTRIES 配置**：`[frecency]` TOML 段按 topic（dir/file/unified）各自配置 `lambda` 与 `max_entries`；默认值 unified 0.005、dir/file 0.01（与参考项目一致）；旧配置缺省兼容。
- **M23-6 Frecency Pane 模式**：`o` 让**焦点 pane** 进入 Frecency 内嵌搜索（非浮层），对侧 pane 保持可见；结果光标移动时对侧自动预览当前条目（PreviewRenderer 分派：目录浅层 tree、图片、Office/PDF、文本截断 100–200 行 + 末尾"…已截断，按 v 查看全文"标记）；Esc 逐级退出（Search → Normal → Directory）。
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
- 双 pane：左（活动）+ 右（非活动）各自持有独立 `Location`/条目列表/光标/滚动，均可导航（j/k/gg/G/l/h/Enter）；`Tab` 键切换焦点，导航与文件操作作用于焦点 pane。
- `w` / `:view`：右 pane 在“目录列表”与“活动 pane 光标文件的预览视图”之间切换（预览内容跟随活动 pane 光标移动重置）；再按恢复。
- 标签栏 UI：显示每个 tab 的索引与目录名、当前 tab 高亮；`:tabnew`/`gt`/`gT` 切换有可见反馈。
- 每个标签内含完整双 pane 状态；多标签命令语义不变。

**SIMPLIFIED**
- 右 pane 仅导航 + 文件操作作用于焦点 pane；搜索（/）、可视选择（v）、标记（t）仅左 pane（右 pane 触发时提示）。树视图/跳转选择器/提权交互仅左 pane。
- 右 pane 无独立撤销语义：撤销恢复后双 pane 均刷新。

**OUT_OF_SCOPE**
- 右 pane 独立搜索/可视选择/标签；超过两个 pane（vifm issue #1062）。

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
- 主题：`opaline`，TOML 定义，token 驱动，内置 20 套主题，支持运行时切换、深色/浅色变体、渐变，`zero unsafe`。
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
- `.7z` / `.rar` 不支持虚拟浏览，给出提示。
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
