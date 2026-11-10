# rvifm

> 用 Rust 编写的 vim 风格 TUI 文件管理器。参考 vifm 的交互范式，但不是 fork，也不追求行为级兼容。目标是**零配置就能好用，定制只是加分项**。

**当前状态：M0 骨架完成——最小 TUI 可运行，双平台 CI 绿，`0.1.0-alpha.1` 发布准备中。** 执行协议见 [AGENTS.md](AGENTS.md)，实时进度见 [docs/STATE.md](docs/STATE.md)。

## 文档导航

| 文档 | 职责 |
|---|---|
| [AGENTS.md](AGENTS.md) | 编码 Agent 的执行协议：每轮读什么、做什么、何时提交 |
| [docs/SPEC.md](docs/SPEC.md) | 唯一需求规格：M0–M11 的 MUST / SIMPLIFIED / OUT_OF_SCOPE |
| [docs/PLAN.md](docs/PLAN.md) | 里程碑顺序、依赖、发布划分与预算 |
| [docs/TASKS.md](docs/TASKS.md) | 唯一任务来源，可勾选，单次提交粒度 |
| [docs/ACCEPTANCE.md](docs/ACCEPTANCE.md) | 每阶段可手工执行的验收标准 |
| [docs/STATE.md](docs/STATE.md) | 当前阶段、任务、下一步、预算、环境 |
| [docs/NOTES.md](docs/NOTES.md) | 关键决策、取舍、已放弃方案与待定事项 |

## 为什么做这个

vifm 是一个成熟、强大的双栏文件管理器，但它的许多核心能力建立在**外部依赖和手动配置**之上：

- 归档浏览要装 FUSE 或 `archivemount`；
- 图标要装 Nerd Font、装插件，再改 `vifmrc`；
- 图片预览靠调用外部 GUI 工具；
- 文件打开要逐条写 `filetype` / `filextype`；
- 模糊跳转要靠 fzf 薄插件加 SQLite。

这些设计在 2000 年代是合理的，但在今天，它们把大量本应由应用内建的能力推给了用户。

`rvifm` 的出发点很简单：**把 vifm 需要“装插件、配外部工具、写脚本”才能得到的能力，变成编译进二进制的默认行为。** 同时保留 vim 风格的操作范式，让 vifm 用户几乎不需要重新学习键位。

这不是一次逐行移植。完整评估的结论是：vifm 约 **17 万行 C**，依赖 ncurses、GLib、libmagic，忠实重写预计需要 **12–18 个月**、**1.5 亿–4 亿 token**，投入产出比并不理想。因此 `rvifm` 改为**以官方文档的特性清单为规格来源、允许简化和重新设计**，优先保证核心链路可用，再逐步叠加增强能力。

## 与 vifm 的对比

| 能力 | vifm 的做法 | rvifm 的做法 |
|---|---|---|
| **归档浏览** | 依赖 FUSE / archivemount 挂载 | 内置只读 VFS，`.zip` / `.tar` / `.tar.gz` 直接当目录浏览，**无内核模块、无外部工具** |
| **图标** | 装 Nerd Font + 装插件 + 改 `vifmrc` | 编译期集成 `devicons`，一行调用返回图标与颜色；**只需装字体** |
| **主题** | 手动配置颜色方案 | 集成 `opaline`，内置 20 套主题，TOML 定义，运行时切换 |
| **图片预览** | 调用外部 GUI 看图工具 | `ratatui-image` 统一协议适配，**原生协议 → halfblocks 无条件回退**，任何终端都有图可看 |
| **文件打开** | 逐条写 `filetype` / `filextype` | 默认走系统 opener（`gio open` / `xdg-open` / `open` / `start`），**零配置可用**；定制用 TOML 分层覆盖 |
| **模糊跳转** | 依赖 fzf 薄插件 + SQLite | 原生 `pick` / `record` 子命令，redb 存储，**无 fzf、无 SQLite**，可作为独立程序供 bash/fish 使用 |
| **文档预览** | 无内建能力 | Office 纯 Rust 内建预览；PDF 探测式回退 |
| **键位定制** | vimrc 风格脚本 | 动作驱动 + TOML 分层（`prepend` / `append` / `replace`），与文件打开定制**同构** |
| **多标签** | 支持 | 从 M1 起支持，每个 tab 独立导航状态 |

### 关键提升逐条说明

**归档即目录，不依赖 FUSE。** vifm 的归档浏览需要系统安装 FUSE 或 archivemount，这在许多受限环境（容器、Windows、无 root 的机器）里直接不可用。`rvifm` 在导航层引入 `Location` 抽象，把物理目录和归档内路径统一成同一种可浏览对象。`.zip` / `.tar` / `.tar.gz` 首批支持，`.tar.xz` / `.tar.bz2` / `.tar.zst` 随后；`.7z` / `.rar` 明确标记为不支持虚拟浏览并给出提示。归档内**只读**，不污染撤销栈。

**图标与主题内建，不靠插件。** `devicons` 直接返回 glyph 和关联颜色，内置 Dark/Light 两套图标配色；`opaline` 提供 TOML 驱动的主题引擎和 20 套内置主题。文件列表、预览窗格、归档浏览共享同一套视觉语义。用户仍然需要 Nerd Font，但**插件安装、配置片段、运行时外部进程全部消失**。

**图片预览：协议优先，halfblocks 兜底。** 这是与 yazi 最关键的差异。yazi 在支持原生图形协议的终端上体验很好，但在 Alacritty 这类不实现终端图形协议的终端上，需要用户额外安装 Überzug++ 或 Chafa。`rvifm` 用 `ratatui-image` 在启动时探测最佳协议，并建立三级回退：**原生协议（Kitty / iTerm2 / Sixel）→ Unicode halfblocks（无条件可用）→ 可选外部字符画工具**。第二级是内置的，意味着**任何能显示 Unicode 和颜色的终端都能看到图片预览**，不需要任何额外安装。

**文件打开：零配置默认，分层定制。** 普通用户不写任何配置，按 Enter 就用系统关联程序打开文件；目录在 `rvifm` 内部进入。高级用户通过 `prepend_rules`（覆盖默认）、`append_rules`（补充规则）、`rules`（完全替换）三层定制。这套分层模型和键位系统**结构完全一致**——用户学一次，就能同时理解两类定制。

**原生 frecency 选择器，去掉 fzf 和 SQLite。** 通过 `rvifm pick` / `rvifm record` 两个子命令提供与 fzf 同构的 stdin/stdout 契约，可作为独立程序被 bash/fish 调用。存储用纯 Rust 的 `redb`，采用 **events + snapshot + meta** 三表结构：`record` 只追加事件，`pick` 和 TUI 启动时重放评分，compaction 惰性触发。**`pick` 是纯只读选择器**，选中后不写任何事件；只有 shell 真正 `cd` 成功或文件管理器真正跳转/打开成功时，才调用 `record`。这保证 frecency 记录的是“已发生的访问”，而非“意图访问”。

**多标签从 M1 开始。** 每个 tab 独立持有 `Location`、光标、滚动位置和搜索状态；撤销栈全局共享，`UndoableOp` 记录所属 tab 与路径，跨 tab 撤销能正确定位。这避免了后期再改“当前 tab 上下文”导致的大范围返工。

## 核心特性

- **多标签**：`:tabnew` / `:tabclose` / `gt` / `gT` / `N gt` / `:tabmove`，每个 tab 独立状态
- **双栏/单栏浏览**，vim 风格按键：`j/k/h/l`、`gg/G`、`Ctrl-u/Ctrl-d`、`/` 搜索、`n/N`
- **文件操作与撤销**：复制、移动、删除、重命名、新建，`u` 逆序撤销核心可逆操作（含跨 tab）
- **选择与批量操作**：`v` 可视选择、`t` 标记（tag）、批量复制/移动/删除
- **归档浏览**：`.zip` / `.tar` / `.tar.gz` 作为只读虚拟目录
- **原生 frecency 跳转**：`pick` / `record` 子命令，无 fzf、无 SQLite
- **统一预览**：文本（语法高亮）、目录、图片（三级回退）、Office 文档
- **图标与主题**：`devicons` + `opaline`，内建而非插件
- **声明式键位**：动作驱动，TOML 分层定制，默认键位与 vifm 对齐（`yy` 后 `P` 移动、`p` 复制）
- **零配置文件打开**：系统 opener 兜底，TOML 分层覆盖

## 技术栈

| 领域 | 选型 |
|---|---|
| TUI | `ratatui` + `crossterm` |
| 图标 | `devicons`（GitHub 仓库名 `rust-devicons`） |
| 主题 | `opaline`（备选 `tui-nuances`，90 套主题） |
| 图片预览 | `ratatui-image`（Sixel / Kitty / iTerm2 / halfblocks） |
| 归档 VFS | `zip`、`tar`、`flate2`、`virtual-filesystem`（参考） |
| 存储 | `redb`（纯 Rust，跨平台，ACID） |
| 键位解析 | `keymap-core` + `keymap-seq` + `keymap-config`（备选：轻量自写 Trie） |
| 模糊匹配 | `skim` 库（备选降级：`nucleo` + 自绘 picker） |
| 文本高亮 | `tree-sitter` + `tui-syntax` |
| Office 预览 | `office_oxide ~0.1.16`（纯 Rust，三格式统一 IR） |
| PDF 预览 | `pdf-inspector =1.25.2`（文本提取）+ 可选 `exprim =0.4.0`（`pdf-visual` feature，纯 Rust 栅格化） |
| 配置 | TOML |

## 架构概览

```text
┌─────────────────────────────────────────────┐
│                  rvifm (binary)             │
│  rvifm          → 文件管理器 TUI             │
│  rvifm pick     → 只读模糊选择器              │
│  rvifm record   → 静默记录访问事件            │
└─────────────────────────────────────────────┘
         │                │                │
   ┌─────▼─────┐   ┌──────▼──────┐   ┌────▼────┐
   │  Keymap   │   │  Preview    │   │  Store  │
   │  Action   │   │  Renderer   │   │  redb   │
   │  TOML分层 │   │  文本/目录/ │   │ events  │
   │           │   │  图片/Office│   │ snapshot│
   └───────────┘   └─────────────┘   │ meta    │
         │                │           └─────────┘
   ┌─────▼────────────────▼─────┐
   │        Navigator            │
   │  Location::Physical         │
   │  Location::Archive{host,inner} │
   └─────────────────────────────┘
```

**设计原则：**

1. **预览不是为某个子命令单独实现的**，而是抽出 `PreviewRenderer`，文件管理器和 `pick` 共用。
2. **归档浏览只读**，不进入撤销栈，与物理文件操作干净解耦。
3. **`pick` 只读、`record` 只写**，评分在重放时计算，写入入口唯一。
4. **每类可定制行为都遵循同一套分层模型**：默认可用 → `prepend` 覆盖 → `append` 补充 → `replace` 完全替换。

## 快速开始

> 以下命令对应目标形态；`cargo install rvifm` 在 `0.1.0-alpha.1` 发布到 crates.io 后可用，本地可直接 `cargo run`。

```bash
# 构建
git clone https://github.com/<your-org>/rvifm
cd rvifm
cargo build --release

# 运行
./target/release/rvifm

# 作为独立选择器
fd -t d | ./target/release/rvifm pick

# 记录一次访问（shell hook 中调用）
./target/release/rvifm record --topic dir "$PWD"
```

### 平台支持

Linux 与 Windows 为优先目标，macOS 尽力支持。Rust 对 `x86_64-pc-windows-msvc` 和 `x86_64-unknown-linux-gnu` 提供一级支持；GitHub Actions 在 tag 推送时自动构建并发布 Linux / Windows 产物。

### 运行时依赖说明

- **Nerd Font**：显示文件图标所需，由终端提供。
- **PDF 预览**：文本提取内建（`pdf-inspector =1.25.2`，零配置、无外部依赖）；视觉预览可选 exprim（`pdf-visual` feature）或 Poppler/MuPDF；均不可用时回退系统 opener。默认构建的依赖树不包含 `exprim`。
- **其余能力**（归档浏览、图片 halfblocks 回退、Office 预览、frecency）**无外部依赖**。

## 配置哲学

`rvifm` 的定制模型统一为三层，所有可定制项共享同一结构：

```toml
# ~/.config/rvifm/keymap.toml
[normal]
prepend_keymap = [
    { on = "P", run = "put_move" },      # 覆盖默认
]
append_keymap = [
    { on = ["g", "t"], run = "next_tab" }, # 补充
]

# ~/.config/rvifm/opener.toml
[opener]
edit_text = { run = "nvim %s", block = true }

[open]
prepend_rules = [
    { mime = "image/*", use = ["preview_image"] },
]
append_rules = [
    { name = "*.md", use = ["edit_text"] },
]
```

**普通用户不需要写任何配置。** 定制是让已经好用的东西变得更好用，而不是使用的前提。

## 开发路线

采用 **harness 模式**：先建立规格、计划、任务、状态文档作为执行脚手架，每个阶段产出可运行版本并提交。

| 里程碑 | 内容 | 状态 |
|---|---|---|
| M0 | 骨架 + SPEC + 双平台 CI | 进行中 |
| M1 | 核心浏览、多标签与 vim 按键 | 规划中 |
| M2 | 文件操作 + `u` 撤销闭环 | 规划中 |
| M3 | 选择、标记、批量操作 | 规划中 |
| M4 | 搜索、过滤（marks 已移除） | 规划中 |
| M5 | 配置与最小命令模式 | 规划中 |
| M6 | 打磨与 v0.1.0 发布 | 规划中 |
| M7 | 归档浏览（只读 VFS） | 规划中 |
| M8 | 原生 frecency 选择器 | 规划中 |
| M9 | 图标与主题 | 规划中 |
| M10 | 声明式键位系统 | 规划中 |
| M11 | 预览与文件打开 | 规划中 |

**优先保证 M0–M2 闭环**，尤其是 `u` 撤销的可逆语义。M0–M6 构成 v0.1.0 核心版本；M7–M11 是在核心可用之后逐步叠加的增强，不阻塞 v0.1.0 发布。

## 非目标

明确不做的事，避免范围蔓延：

- **不追求与 vifm 行为级兼容**。参考其交互范式，但不逐条对齐 man page。
- **不实现 Vim 脚本引擎**。配置用 TOML，键位绑定到内置 Action。
- **不嵌入完整 Vim 引擎**（`narwhal-vim` / `vimcore` / `hjkl-engine`）。文件管理器需要的是文件操作语义，不是文本缓冲区变异。
- **不把 FUSE、archivemount、fzf、SQLite、Überzug++、Chafa 作为必需依赖**。
- **不实现 marks**（`m` + 字母标记路径、`'` + 字母跳转）；路径跳转由 frecency 的 `pick` 覆盖。
- **不实现插件系统**。
- **不写入归档内成员**，不做嵌套归档递归浏览。
- **不保证 PDF 的零配置视觉预览**，视觉预览需要可选的 exprim（`pdf-visual` feature）或 Poppler/MuPDF。
- **不做内嵌视频播放**。

## 与同类工具的定位

| | vifm | ranger | yazi | **rvifm** |
|---|---|---|---|---|
| 归档浏览 | 需 FUSE | 需外部工具 | 支持 | **内置只读 VFS** |
| 图标 | 插件 + 配置 | 插件 | 内建 | **内建 crate** |
| 图片预览（无协议终端） | 无内嵌 | 外部工具链 | 需 Überzug++/Chafa | **halfblocks 内置回退** |
| 文件打开默认体验 | 需写规则 | 需写脚本 | 较完善 | **系统 opener 零配置** |
| frecency 跳转 | fzf 插件 + SQLite | 无 | 内建 | **原生 pick/record + redb** |
| 路径标记 | marks | 书签 | 无 | **不实现，由 frecency 覆盖** |
| 定制学习成本 | 中 | 高 | 中 | **低（统一分层模型）** |

## 致谢

`rvifm` 站在许多优秀开源项目之上：`ratatui` 生态提供了 TUI 基础设施，`devicons` 和 `opaline` 解决了图标与主题，`ratatui-image` 统一了图形协议适配，`redb` 提供了纯 Rust 的嵌入式存储，`skim` 提供了可嵌入的模糊选择器。交互范式上，vifm、ranger、yazi 都是重要的参考对象。

## 许可证

`MIT OR Apache-2.0` 双许可。rvifm 是独立重实现，不包含 vifm 源码；仅参考其公开行为与文档，不构成衍生作品。

---

**rvifm 不是 vifm 的 fork，也不追求替代 vifm。** 它是一个用现代 Rust 生态重新思考“文件管理器应该内建什么”的尝试：把那些本该由应用自己解决的问题，从用户手里拿回来。
