# CHANGELOG

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)；
版本号遵循 [SemVer](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### M65 境界起点改名与 CI 跨平台修复（仅提交代码，不触发版本 bump / tag / GitHub Release）

- 境界第 0 级由「凡人」改为「入门」（能打开软件即算入门），符号 `○` 不变。
- 修复 Windows CI 测试编译：`m482` 的 POSIX 执行位测试加 `#[cfg(unix)]`；修正 macOS 4 处长临时路径导致的平台相关渲染断言。

### M64 向导修仙境界分级（仅提交代码，不触发版本 bump / tag / GitHub Release）

- 向导新增 11 级修仙境界（入门 ○→真仙 ★）与初期/中期/后期/圆满小阶段，只在学习进度栏显示符号+中文；筑基绑定核心导航完成，真仙需 100% 且零错题。
- `tutor_meta`（向导用法）移到模块列表首位，首次打开先学使用方法。

### M63 其它预览增强（仅提交代码，不触发版本 bump / tag / GitHub Release）

- 目录树预览按目录 mtime 缓存，不再每帧重扫；JSON 缩进 / Markdown 弱化标记的轻量结构化预览。
- 新增非默认 `pdf-rust` feature（`karet-pdf` 纯 Rust 栅格化）：文本提取仍先行，视觉路径优先纯 Rust；默认构建不含该依赖。二进制已有 hex dump，语义注释延后。

### M62 图片缩放与编码流水线（仅提交代码，不触发版本 bump / tag / GitHub Release）

- 后台 worker 用 `fast_image_resize`（SIMD/Nearest，默认最快）把解码图预缩到 2048px 以内；UI 线程复用已编码协议缓存，不再每帧 resize+encode。

### M61 大文本视口虚拟化（仅提交代码，不触发版本 bump / tag / GitHub Release）

- 新增 `memmap2` 视口读取器与 `[preview] virtual_text_threshold`（默认 1MiB）；超阈值本地文件不再整文件读取/内存驻留，只渲染视口行，聚焦不再触发全量加载。
- 高亮状态按路径跨帧复用（尽力而为，流畅优先）；远端 Location 不产生本地预览路径，保持仅元数据。

### M60 真异步预览架构（仅提交代码，不触发版本 bump / tag / GitHub Release）

- 引入预览后台 worker + generation token：快速 j/k 的中间目标被丢弃，文本读取/hex 判定、图片解码、Office/归档/PDF 文本行解析均不在 UI 线程完成。
- UI 线程只做协议编码与文本高亮；PDF 视觉页因依赖 `Frame` 仍在 UI 线程（SIMPLIFIED）。
- 图片协议默认做一次带超时查询（tmux/screen 下也查），失败回退 halfblocks；`RVIFM_IMAGE_PROTOCOL_QUERY=0/false/off` 可显式关闭。

### M59 预览与归档 VFS 体验修复（仅提交代码，不触发版本 bump / tag / GitHub Release）

- `rvifm pick` 文本预览与 TUI 查看器共用 `highlight::styled_lines`（同源语法高亮，仍纯只读）。
- 归档成员 `w` 预览恢复 j/k 滚动与 Tab 进/出（viewer 走临时副本路径）、语法高亮保留；`l`/Enter 改走 `[opener]`/`[open]` 分层而非硬编码系统 opener。
- 快速文本预览行数改为 `max(36, 视口+10)`；渲染 committed viewer 路径消除「先二进制后文本」闪现，缓存命中只克隆所需行；预览获焦蓝色边框。
- frecency `Tab` 固定进/出预览、`Shift+Tab` 切左右栏；`r` 改为两键确认仅清 bookmark/mark（保留条目与分数），`u` 连 keyword 恢复。

### M58 命令补全增强（仅提交代码，不触发版本 bump / tag / GitHub Release）

- 补全浮层标题提示 `Space=选定到输入栏`，描述常显、输入即实时过滤；命令行 500ms 闪烁光标；Space 选定替换最后一个词。
- 修复 Tab 循环三处问题：尽量全显候选+滚动条、自动滚动使选中项可见、索引与描述同源；浮层不再固定 ≤40%/15 行。
- shell 历史补全与 frecency `cmd` 改为整行（含参数/路径），推翻 M50「仅命令名入库」边界；多行命令顶部预览；外部命令成功也显示返回码。
- 新增只读 `rvifm cmd [query]`（fzf Ctrl+R 同款）；`rvifm pick` 仍保持纯只读。

### M56 向导模式（tutor 重构；仅提交代码，不触发版本 bump / tag / GitHub Release）

- tutor 统一更名为「向导模式」；首次无配置文件时自动进入；欢迎对话框占屏 80%（≤40×10 降级全屏），底部带边框进度状态栏显示模块/功能点/百分比。
- `/` 输入实时过滤、有记录模块仅显示错题且保持原编号、模块完成 `Enter` 回列表、100% 模块重入重置确认。
- 进度按稳定 ID 忽略未知/改名条目；NOTES 建立向导↔ACCEPTANCE 映射表并由 harness_check 规则 8 守护；🏅/🏆/⭐ 成就徽章在状态栏角落显示。

### M55 frecency 搜索态区分与实时递归查找（仅提交代码，不触发版本 bump / tag / GitHub Release）

- frecency SEARCH 态标题改为焦点边框色+加粗，底部绘制带边框输入框 + 可见光标；列表保持无边框，NORMAL/SEARCH 明确区分。
- 新增 `O` 实时递归查找：后台线程流式扫描（复用 `:find` ignore 规则，深度 20/条目 5 万护栏），输入实时子串过滤、scanning 计数、`Enter` 定位选中（走 record）、`Esc` 退出；不写 redb、不替代 `:find`。

### M57 帮助外部入口收敛（仅提交代码，不触发版本 bump / tag / GitHub Release）

- 移除 `rvifm help` 外部子命令；`:help` 只在应用内以不透明覆盖层打开（页内 `/` 搜索、Esc/q 返回），不再新开终端窗口。
- `[ui] help_in_new_window` 配置键保留以兼容旧配置，但不再生效。

### M48 用户实测批次 4（仅提交代码，不触发版本 bump / tag / GitHub Release）

- 外窗修复：新窗口起始目录先校验存在（失效时回退），避免 `getcwd() failed` 刷屏；`gi`/`:gitui` 非 git 仓库不再启动并循环提示。

### M50 命令补全增强（仅提交代码，不触发版本 bump / tag / GitHub Release）

- `:` 键入补全内部命令（名+描述，按 frecency 频率排序），`:!` 补全 shell 历史完整命令行；底部两列浮层 ≤40% 终端高，Tab 循环、Enter 执行。
- shell 历史读取默认开、增量记账；完整命令行仅内存候选，只有命令名进 frecency；不扫描 PATH、不做 execve 守护、不补参数。
- 新增 `open_external_command_line` Action（无默认键位，用户自定义）；`[command] quote_expansion` 开关让 `%d/%f/%F/%D` 自动加双引号。
- 测试隔离：单测不再调用真实系统 opener（避免残留 cosmic-edit 进程）；命令补全读 frecency 改只读句柄，避免并发测试锁冲突。
- 启动性能：debug 二进制 461M→152M（关闭依赖 debuginfo）；tmux/screen 内跳过阻塞的终端协议查询；shell 历史改单事务批量导入 + 字节偏移增量（首次 0.26s / 常规 0.18s）。诊断开关 `RVIFM_STARTUP_TRACE=1`。

### M53 标签栏对齐、归档 VFS 预览与预览性能（仅提交代码，不触发版本 bump / tag / GitHub Release）

- 标签显示 `[序号:焦点栏全路径]` 且随焦点切换；`:tabname` 与右栏活动项写入 session.toml，重启后保持（旧会话兼容）。
- 归档成员 `w` 预览、`l`/Enter 解临时只读副本交系统 opener；frecency NORMAL 全键位提示与 `r` 删记录。
- 文本快速预览 ≥36 行 + 行缓存；150ms 防抖跳过快速 j/k 的中间预览；tmux passthrough 下恢复 foot 原生图片协议；`.deb` 用 dpkg-deb -c 列成员。

### M52 预览布局策略反转（仅提交代码，不触发版本 bump / tag / GitHub Release）

- `[preview] fixed_side` 默认改为 `false`，且只作用于 rvifm 内 `o`/`:j`/`:J` 的 frecency 模式；`:view` 恢复对侧预览（焦点左→右、焦点右→左），`rvifm pick` 恒左候选/右预览。已显式配置 `fixed_side = true` 的用户需手动移除才能回到默认对侧。
- 右栏能力审计：Normal 键位与冒号命令与左栏基本一致（焦点投影 + `focused_*` 分派），预览焦点 `Tab` 进出与侧别无关；全量文档一致性收尾。
- 稳定性：frecency 只读打开 redb 增加 10×20ms 有限重试，消除并行测试/双开实例偶发 `Database already open`。

### M51 用户实测批次 6（仅提交代码，不触发版本 bump / tag / GitHub Release）

- IME：打开外部窗口/失焦期间暂停周期性切英文，避免干扰 vim 等程序的中文输入法；回焦或按键后恢复。
- 配置：默认 `edit_text` 改为 `vim %s`；`.vifm`/`.vim` 与 `LICENSE` 等无扩展名文本通过有界内容嗅探识别为 text/plain，走 vim 打开。
- 粘贴同名冲突弹窗补上 `y`/Enter、n/Esc、j/k 按键提示（与删除确认底栏一致）。

### M49 用户实测批次 5（仅提交代码，不触发版本 bump / tag / GitHub Release）

- 布局：frecency 与 `:view` 默认搜索/列表固定左侧、预览固定右侧（共享 `preview_layout_areas`）；`Tab` 仅切换预览焦点，位置不随焦点移动；`[preview] fixed_side = false` 恢复旧动态对侧（仅新会话生效）。**（此策略随后被 M52 反转：默认改为对侧预览，且开关仅作用于 frecency；历史条目保留供追溯。）**
- 命令/编辑：`:gi` 为主命令（`:gitui` 兼容别名）；`v` 多选 `D` 永久删除；`:extract` 显式目录条目解压出真实文件；列表左侧留白收窄为 1 空格。
- 预览：快速切换时图片/PDF 仅元数据、文本仅前 300 字；`Tab` 进入预览后全量加载。
- frecency：标题动态 `NORMAL/SEARCH` + `[filter]` 黄底高亮；SEARCH 提示方向键并支持 `Ctrl+J/K`；Esc 保留过滤；`dd` 删记录 / `u` 恢复 / `yf`·`yn`·`yc` / `gi`。

- 外窗：`s`/`:!`/`gi`/`:help` 新窗口前台激活，起始目录取焦点 pane（tmux 去 `-d` + `-c`；screen cd + next）。文本编辑类 opener 优先新窗口，`.md5` MIME 补齐，`.sh` 执行位文本优先。
- 浏览：右栏文件 `l`/Enter/再击走共享 opener；`v` 多选 `y` 写 yank 缓冲；粘贴冲突改阻塞式确认弹窗。
- 归档：成员粘贴剥离公共 inner 前缀（不重建上级目录）、目录递归解压、多选混合正确；`w` 预览可选外部 `unzip -Z1`/`tar -tf`/`7z l -ba` 快速列目录（4MB 输出护栏，缺工具回退内置）。
- 命令：`:archive` 无参直接执行（zip + `.zip` 名）、`-i` 交互；对话框任意字段回车执行、`Tab` 高亮当前字段、只清弹窗矩形。
- IME：Normal 模式 1s 周期切英文兜底；tutor 列表 `/` 搜索 / `d` 重置 / 仅错题 / 结束 Ctrl+N 回列表；删除确认真窗口化；`rvifm help` 改为可搜索/高亮查看器（无 TTY 回退纯文本）。


### M47 用户实测批次 3（10 任务；仅提交代码，不触发版本 bump / GitHub Release）

- 焦点化收尾：`=` glob 过滤作用于焦点 pane，状态从状态栏移入该栏顶部路径行、Esc 清除；`:archive` 源取焦点栏文件、产物落在对侧栏；`:extract` 源归档取焦点栏、缺省解压到对侧；批量选集、变量替换 `%d/%f/%F`、`:file`、marks、`:sort`、`:select`/`:unselect` 均按焦点栏分派。
- Frecency：`o`/`:j`/`:J` 面板去掉边框、背景/几何融合焦点栏，标题为「Frecency - 高频目录/文件搜索 - Normal 模式」，点击命中与渲染同源。
- 视觉：目录/归档成员预览图标按层级缩进；左右列表增加前导留白且整行高亮覆盖留白；右栏 `gg` 到顶部只高亮 `..` 行；左右栏 `v` 多选状态各自独立。
- IME：Linux 下切回终端窗口（FocusGained）与 Normal 模式按 `Esc` 防御性切英文，不覆盖已保存输入法状态、250ms 内重复触发节流；既有模式边界切换/恢复语义保留。
- 文档：README / GUIDE / 内置 help / tutor 条目同步；本批次不发布版本、不推送 tag。

## [1.4.0] — 2026-11-25

M46 用户实测批次 2（12 项，含 5 项"未真正修复"的回退真修复 + 配置/归档增强）：

- Linux Normal 模式自动切换英文输入状态，支持系统已有 IBus/Fcitx/Fcitx5 命令并恢复先前输入法状态；无需新增 Cargo/C 依赖，可用 `[ime]` 配置关闭或设置 IBus 英文 engine。

### M46 用户实测批次 2（缺陷真修复 + 配置/归档增强）

- 修复图片预览越界：原生协议图形不再溢出到窗口顶部，严格限制在预览窗格内。
- `/` 搜索高亮只作用于焦点 pane；右栏未搜索态整行背景与左栏一致；`h`→`l` 往返还原光标而非高亮 `..`。
- `:extract` 与归档 `yy`→`p` 解压后立即刷新目标栏；后台作业完成刷新左右双侧。
- 首次启动自动生成带注释的 `config.toml` 与 `keymap.example.toml`/`opener.example.toml`，不覆盖已有文件。
- `:help` 背景不透明、页内 `/` 搜索可用；tmux/screen 下新窗口打开（`[ui] help_in_new_window`），新增 `rvifm help` 子命令。
- 归档 `w` 预览按 (路径, mtime, size) 缓存，大包给「按 l 进入浏览」提示；新增 `.deb`（ar + 内嵌 data.tar）与 `.rar`（外部 bsdtar/unrar/7z）只读虚拟目录，`.apk` 保持可用。
- 删除确认改为居中 yazi 风格：标题 + 图标 + 待删文件列表（图标/类型色）+ 固定高度滚动条 + `j`/`k` 导航。

## [1.3.0] — 2026-11-24

M45 用户实测批次（28 子任务，冲突裁决全部落地）：

- 浏览：HOME 下路径显示 `~/…`；标签栏鼠标点击切换 tab、灰白底反色字；双 pane 分隔线改为 `[ui] pane_separator` 可配置（默认关）；未搜索态整行背景一致；无后缀/可执行/文本/jar/deb 按类型推断图标。
- 打开与确认：opener 解析错误给出路径+字段+示例指引（`:help`）；可执行文件 `Enter` 仅提示「请在终端执行」（`[open]` 显式 `run` 规则可绕过）；删除确认为居中 ⚠ 弹窗；`h`→`l` 往返还原光标。
- 预览：大图字体尺寸兜底与切片高度钳制（不越出窗格）；归档预览显示成员列表（Office/PDF 优先）；文本查看器对含 NUL 文件回退信息面板（tar 不再乱码）。
- 命令与焦点：`o`/`:j`/`:J` 选择器支持鼠标点击；`:` 首词 Tab 补全命令名；`:cd` 与 `o` 跳转写入焦点 pane；右栏归档 `l`/Enter 进入虚拟目录。
- 解压：`:extract` 缺省解压到对侧 pane（支持 `left`/`right`/`当前`/`对侧`/路径）；归档虚拟目录 `yy`→`p`/`P` 按成员解压到焦点 pane（可撤销）。
- Tutor：列表显示学习进度百分比、布局避让底部面板、进度缓存即时刷新；`:archive <名> checksum=…` 从扩展名推断格式；归档对话框加宽/去后缀/多选默认父目录名。

## [1.2.0] — 2026-11-23

发布产物简化：GitHub Release 仅分发 musl 归档，aarch64 改为静态 musl。crates.io 不发布，仅 GitHub Release。

**发布产物（REL-4）**
- Linux 归档由 gnu + musl 双份改为**仅 musl**：`linux-x64-musl.tar.gz` + `linux-aarch64-musl.tar.gz`（aarch64 由 gnu 动态链接改为 **静态 musl**，无需 glibc）；Windows 归档不变。
- SHA256SUMS 由独立汇总作业在所有归档上传后一次性生成（REL-3 修复并行覆盖缺陷），含且仅含 3 个归档。

**缺陷修复（随 1.1.0 归档修复批次，FIX）**
- `SHA256SUMS.txt` 完整性与历史回填（v1.0.0/v1.1.0）。
- `:tutor` 模块列表与底部 Panel 不透明化，消除与目录列表的视觉干扰。
- 会话保存自动创建配置目录，修复全新安装下退出未保存/重启未恢复标签的问题（会话设计本就为退出保存/启动恢复）。

## [1.1.0] — 2026-11-23

1.1 版本：tutor 导师模式（M42/M43）+ 预览焦点修复（M44）。crates.io 不发布，仅 GitHub Release 分发。

**预览焦点修复**
- `w`/`:view` 开启预览不再移动焦点：原 pane `j/k` 切换文件，对侧预览自动跟随当前文件；`Tab` 显式进入预览窗格滚动/搜索，再 `Tab` 返回原 pane；关闭预览后焦点与原键位表保持不变。

**导师模式（M42）**
- `:tutor` 导师模式：TUI 内交互式教程+自验清单（单一模式，两用）。底部 Tutor Panel（左 prompt/expected、右进度与按键提示），y=掌握 / n=未掌握记错题本 / q 退出保存；模块完成后 Panel 内结算。
- 三内嵌模块：core_nav（7 项）/ file_ops（9 项）/ search_frecency（7 项），条目与实际键位语义对齐。
- 进度持久化 `tutor_progress.toml`（数据目录，缺失/损坏静默降级）；错题本未掌握项下次进入优先复习；`:tutor reset <模块>` 清空。
- 发布流程解耦（REL）：GitHub Release 自动发布；crates.io publish 仅由人工发起（人工验证可借 `:tutor`）。

**导师模式全量覆盖与放行重构（M43）**
- 透明放行重构：Tutor 会话迁为覆盖层状态（不占输入模式，命令/帮助/确认切换不丢进度）；练习键照常可用，标记键 **Ctrl+Y 掌握 / Ctrl+N 未掌握 / Ctrl+Q 退出**；作答即时落盘。
- 模块扩至 **13 个 100 项**，覆盖全部默认键位与 `:` 命令（新增 search_filter/visual_tag/marks_frecency/tabs_history/panes_sort/archive_ops/maintenance/command_line/clipboard_git/remote_sftp/tutor_meta）。
- 条目 `requires` 环境依赖标注（editor/sudo/sftp/git/osc52/graphic/tmux），无环境可跳过；危险命令「了解即可」标注；运维模块含 sandbox 引导。
- 模块列表滚动视口；`:tutor reset <模块>` 重置进度。

## [1.0.0] — 2026-11-21

1.0 稳定化统一发布：M35–M41 各批次并入（发布节奏按用户决策，中间版本不单独发版）。自 0.8.0 以来的全部用户可见变化如下（分批次小节）。

### 新增（M35–M41 并入）

**传输引擎（M36）**
- `copy_tree` 重写：保留 mode/mtime/symlink、进度、取消（跨盘 move 同享）。
- 远端下载单次读取 + 临时文件 + 原子 rename；`RemoteFs` 范围读写（read_range/write_range）。
- `Transfer` 状态机统一进度；冲突策略五态（Ask/Overwrite/Skip/RenameAuto/KeepNewer，`[transfer] conflict_policy`）。
- 大文件偏移续传（.part + .part.meta 头哈希校验）；`:queue` 传输队列；递归远端目录传输（三方向）。

**文件运维（M37）**
- `:chmod <mode>`（八进制/符号、-R 递归；POSIX）。
- `:extract` 归档解压（zip-slip 防护、job 化、u 可撤销）。
- `:sync` 目录同步（dry-run 缺省、--apply/--delete；l2r/r2l）。
- `:du` 磁盘占用（top-5）、`:dup` 查重（SHA-256）、`:checksum` md5 sidecar（md5sum 兼容）。
- `:trash clear` / `:trash purge --before=<天数>`。

**检索与 shell 集成（M38）**
- `:find`/`:grep` 结果可操作（yy/dd/p/:tag/:rename/:chmod 映射真实路径；跨目录去重；只读条目拒绝）。
- `pick` fzf 兼容参数八项（--query/--multi/--print0/--preview/--height/--no-sort/--exit-0/--select-1）。
- `rvifm init bash|zsh|fish|powershell`（cd 后 record、快捷键调 pick；只输出不写 rc）。
- `:frecency stats | export <file> | purge confirm | prune --before <日期>`（JSON 全量导出、二步确认门、按时间裁剪）。
- `:find` 过滤器 `--size [+-=]N[bkmg]`、`--mtime [+-]N(天)`、`--type f|d`、`--depth N`；命名搜索 `:find save/load <名>`（仅本会话）。

**预览与平台（M39）**
- hex 查看器（二进制文件 w 进入：16 字节/行 + ASCII 侧栏、分块读取 ≤1MB；`:goto 0x…/十进制` 跳转）。
- CSV/TSV 表格对齐预览；PDF 查看器 `[`/`]` 任意页翻页；图片信息面板容器尺寸/格式（PNG/JPEG/GIF/BMP 头解析）。
- 通用 zip 容器虚拟浏览（.docx/.xlsx/.pptx/.apk/.war）。
- SFTP 硬化：首连指纹确认（`strict_host_key = ask|accept|no`，缺省 ask）+ ProxyJump（ssh config 透传）+ ssh-agent 认证（unix）+ keepalive。
- Windows UAC 操作级提权（ShellExecuteExW "runas"；不提升主进程；取消状态不变）。
- 命名标签（`:tabname <名>`）、标签溢出滚动（‹ ›）、`[ui] layout = single|dual|preview-right|preview-bottom`。

**右栏一致性与交互修复（M41，用户实测）**
- 右 pane `:` 命令模式（命令作用于焦点 pane）+ `gt`/`gT`/`Ngt`/`gg`（与左栏一致）。
- 方向键 `↑↓←→` = k/j/h/l 等价（双栏一致、计数/环绕/`..` 行全保持）。
- 鼠标单击切换焦点 pane 并聚焦条目（左右一致；已聚焦再击打开；滚轮作用于焦点 pane）。
- 视口滚动有状态化：光标底行按 k 列表不动、高亮上移（原实现钉底）。

**质量与工程（M40）**
- criterion 基准集五组（`cargo bench --bench rvifm_bench`；基线 docs/bench.md）。
- cargo-fuzz 五靶 + CI 10 分钟 fuzz；proptest 三组十属性。
- cargo-deny/audit + CycloneDX SBOM + Release SHA256SUMS.txt。
- macOS CI（test + clippy）+ aarch64 Linux 归档。
- src/lib 化（bench/fuzz 导入内部函数）。

### 修复
- **natural_cmp 非全序**（M40-3 proptest 抓获）：Equal 非传递可致 std sort panic——重写为分块比较（数字段/文本段交替；数字段 < 文本段）。**含日常文件名排序崩溃风险，升级强烈建议**。
- **junction undo EPERM**（M40-4）：Windows 目录联接撤销报拒绝访问——改 remove_dir 优先（reparse point 删除语义），M28-1 撤销链自动化验证。
- **sevenz-rust 弃维护**（RUSTSEC-2026-0246，M40-5）：迁移 sevenz-rust2 0.25（API 映射 + 打包/读端回归通过）。
- :sync 仅实现 l2r/r2l 与 --apply/--delete（三策略 deferred，见 SPEC M37-3）。

### 平台
- CI：test 矩阵 ubuntu/windows/**macOS**；release 归档 linux-musl / linux-gnu / **linux-aarch64-gnu** / windows + SHA256SUMS.txt；fuzz 与 supply-chain 独立 job。
- crates.io：1.0.0 统一发布（0.9.0/0.10.0 跳过；0.11.0/0.11.1 已先行上线）。

### 迁移提示
- 0.8.0 → 1.0.0 无配置破坏性变更；新增配置段：`[transfer] conflict_policy`、`[preview] pdf_image_pages/pdf_prefer_image`（1.0.0 起支持任意页翻页）、`[ui] layout`。默认键位不变；右栏 `g` 单键直跳顶部改为 `gg` 序列（与左栏统一）。
- 依赖变化：`sevenz-rust` → `sevenz-rust2`（7z 读/写行为不变；若从源码构建需重新解析依赖树）。

## [0.11.1] — 2026-11-21

### 新增
- harness 一致性检查脚本 `scripts/harness_check.py`（七条规则，CI required check）。
- `CHANGELOG.md` 本文件（0.8.0 → 0.11.0 用户可见变化回填）。
- REVIEW 评估报告归档制（`docs/reviews/REVIEW-<日期>.md`）。

### 修复
- 文档状态漂移六处（AGENTS §10 过期任务指针、SPEC 7z/rar 能力三处、STATE/TASKS/PLAN/README 状态句矛盾）。
- 依赖卫生：移除零引用 `tokio-stream`；清理测试调试输出。
- 文档测试计数与实际代码一致（437）。

## [0.11.0] — 2026-11-21

### 新增
- 默认主题 gruvbox-dark；捆绑 snowwhite 主题（`:theme` 直接可用，39 + 捆绑内置主题）。
- 右 pane `/` 搜索（焦点 pane 感知，Esc 恢复）。
- 对侧预览统一：右焦点 `:view`/预览不再错位（preview_path 焦点修复）。
- frecency 选择器视觉回显（标题、全路径、底部 query）。
- `.jar` 归档虚拟浏览。
- 二进制文件信息面板（1KB 有界读取 + 10 类魔数，不执行文件）。
- `docs/GUIDE.md` 用户手册（快速上手/键位/命令速查）。
- 图片/PDF 预览尺寸适配（小图居中不放大、超宽 fit-width、超高顶部切片）。

### 平台
- Windows CI 测试与 clippy 全绿（本版本起 cfg(unix) 测试隔离纪律固化为 CI 口径）。

## [0.10.0] — 2026-11-21（GitHub Release；crates.io 跳过）

### 新增
- `:link` 符号链接创建（Windows 目录联接回退；`u` 可撤销）。
- `:rename` 结构化批量重命名（`s/old/new/[i]`、`prefix=`、`suffix=`、`seq=start,step,width`；预览结果视图 y=应用/Esc=取消）。
- `:jobs` 任务视图增强：`p` 暂停/恢复、Enter 详情（进度/速率/时长）、状态列。
- `:compare bycontents`（SHA-256 内容判定，单文件 64MB 护栏）；对比结果视图 `dp`/`do` 同步（可恢复）。
- frecency cmd topic：`:` 命令历史跨会话 frecency 排序、Tab 补全加权、`:cmdmark` 命令书签。
- SFTP 连接类错误自动重连一次；传输失败保留 `.part` 现场。
- 会话恢复归档/远端标签（失败跳过 + 状态栏提示）。
- `[visual]`/`[view]` 键位分层 TOML（进入模式时用户绑定生效）。

### 修复
- 符号链接目录判定（DirEntry lstat 误判，M27-1 残留缺陷）。
- 归档会话路径 Windows 盘符冒号切分。

### 变更
- frecency 清理策略固化为"仅容量裁剪，无分数下限"（对齐同级项目不变量）。
- `:compare` 结果视图 `compare` 命令可重执行（结果替换）。

## [0.9.0] — 跳过发布（2026-11-21 用户决策；变更并入 0.10.0 说明）

- M24–M27 / AUDIT 18 项 / M25-2 marks / M26 归档创建扩展 / MISC-1 rv 别名的变更
  随 0.10.0 发布面世；版本序列 0.8.0 → 0.10.0 → 0.11.0。

## [0.8.0] — 2026-11-19

- SFTP 远端浏览/写/安全补全（M19–M22）、frecency 对齐增强与 Pane 模式（M23）、
  UI/UX 打磨（M21）、任务视图/对比/命令历史/补全（M24）。
