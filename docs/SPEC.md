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
| mark | `m` + 字母标记路径、`'` + 字母跳转；**本项目不实现** |
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
- marks（`m` + 字母 / `'` + 字母）。
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
- **marks 不实现**；路径跳转由 frecency 的 `pick` 覆盖。
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
- compaction 在 `events` 超过阈值（建议默认 5000 条，具体值待定）时由 `pick` / TUI 启动惰性触发，或由 `rvifm maintenance compact` 手动执行；写 `snapshot` 后截断已合并事件。
- 无定时器、无守护进程。

#### 评分与分级

- EMS 评分 + event clock。
- 读时统一 aging：存储分数只在其最后一次访问的类型时钟值上有效，查询时按 `score * exp(-λ * (clock_now - clock))` 对齐后再比较；`clock_now` 取该条目 topic 对应的类型时钟（dir → `dir_tick`，file → `file_tick`）。
- 分级匹配（0–7 档，具体档位在 M8-1 固化）：书签 > 文件名 > 中间段 > 通用子串；在 Rust 内存中排序。
- 书签：`keyword != ''` 作为分级标志，书签自身也参与 EMS 评分；取消书签后保留累积分数正常衰减。
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
- 双栏布局可先做单栏，但数据结构必须为双栏预留。

**OUT_OF_SCOPE**
- marks、文件修改、归档浏览。

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
- 明确区分 tag（文件标记）与 mark（路径标记）；mark 不实现。

**SIMPLIFIED**
- 可视选择可先支持行选择，再扩展矩形/块选择。

**OUT_OF_SCOPE**
- marks、跨目录持久化 tag。

### M4 — 搜索、过滤与跳转

**MUST**
- 在 M1 基础搜索之上增强匹配能力（大小写、路径/内容匹配等，具体范围在 M4-1 定）。
- `n/N` 在基础搜索与增强搜索下行为一致。
- 简单 glob 过滤。
- 搜索链路保持可复用，供 M7 归档内搜索接入。

**SIMPLIFIED**
- 搜索可先做名称匹配，再扩展路径/内容匹配。

**OUT_OF_SCOPE**
- marks 路径跳转（由 frecency 的 `pick` 覆盖）。
- 全文检索索引。

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

### 6.2 仍待定

- **compaction 阈值**：建议默认 5000 条事件，具体值与可配置方式在 M8 定。
- **M7 压缩 tar 变体**：`.tar.xz` / `.tar.bz2` / `.tar.zst` 的具体排期。
- **主题备选**：若 `opaline` 不满足需求，是否切换 `tui-nuances`。
- **键位库稳定性**：若 `keymap-rs` API 不满足，降级为 `keybindings` crate 或自写 Trie。
- **性能阈值**：大目录“不阻塞退出”的具体量化标准未定，M6 验收时补充。
