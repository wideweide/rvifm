# CHANGELOG

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)；
版本号遵循 [SemVer](https://semver.org/lang/zh-CN/)。

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
