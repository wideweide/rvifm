# ACCEPTANCE — rvifm 验收标准

本文件只回答“怎样算完成”。任务顺序见 [TASKS.md](TASKS.md)，需求边界见 [SPEC.md](SPEC.md)。

## 1. 通用门禁（每个任务）

每个任务提交前必须全部满足：

- `cargo fmt` 无改动。
- `cargo clippy -- -D warnings` 通过。
- `cargo build` 通过。
- `cargo test` 通过。
- 程序可启动、可退出、无 panic。
- 本任务对应的阶段验收条目通过。
- `docs/TASKS.md` 已勾选，`docs/STATE.md` 已更新。

通用测试规则：

- 集成测试不得依赖真实 TTY；TUI 渲染使用 `ratatui` 的 `TestBackend`。
- 冒烟测试只跑 `rvifm --version` / `rvifm --help`，不进入交互模式。
- 测试 fixture 使用 `tempfile`，不硬编码 `/tmp`。
- 路径内部统一 `/`；跨平台比较前规范化。

## 2. M0 — 骨架 + SPEC + 双平台 CI

- `cargo build` 成功，`rvifm` 可启动。
- TUI 能显示当前目录列表和状态栏。
- 按 `q` 能正常退出，无 panic。
- `rvifm --version` 和 `rvifm --help` 正常输出并以 0 退出。
- `docs/SPEC.md` 已包含 MUST / SIMPLIFIED / OUT_OF_SCOPE 分级，且与 `docs/PLAN.md` 的里程碑一致。
- GitHub Actions 在 `ubuntu-latest` 与 `windows-latest` 上跑通 `fmt / clippy / build / test`，状态为绿。

### LICENSE 建档

- 仓库根目录存在 `LICENSE-MIT` 与 `LICENSE-APACHE`，内容为对应许可证全文。
- `Cargo.toml` 的 `license = "MIT OR Apache-2.0"`，与两个文件一致。
- README 的许可证段声明与文件、`Cargo.toml` 一致。
- `cargo package --list` 输出包含两个 LICENSE 文件，且包含在发布包内。

### 发布 `0.1.0-alpha.1`

- 前置条件已满足：M0 全部任务完成，`ubuntu-latest` + `windows-latest` CI 全绿。
- 发布前复查 `https://crates.io/api/v1/crates/rvifm` 未被占用；若被占用，按 `docs/SPEC.md` §6.1 改用备选包名并保持二进制名 `rvifm`。
- `cargo package --list` 输出内容符合预期：包含 `Cargo.toml`、`Cargo.lock`、`src/`、`README.md`、`LICENSE-MIT`、`LICENSE-APACHE`，不含 `target/` 等构建产物。
- `cargo package` 检查通过（可打包、可编译）。
- 人工执行 `cargo publish`，发布版本为 `0.1.0-alpha.1`；该步骤依赖 crates.io 凭据，**不在 CI 自动运行**。
- 发布后 `cargo install rvifm --version 0.1.0-alpha.1` 成功，安装得到的 `rvifm --version` 输出包含 `0.1.0-alpha.1`。
- 若发布失败，记录原因并回退版本号，不发布空包占名。

## 3. M1 — 核心浏览、多标签与 vim 按键

- `j/k` 上下移动光标，边界不越界、不 panic。
- `l`/`Enter` 进入目录，`h` 返回父目录；根目录返回时行为明确。
- `gg` 跳到首项，`G` 跳到末项；`Ctrl-u`/`Ctrl-d` 按预期翻页。
- 状态栏显示当前路径、选中项、文件大小/权限。
- `/` 输入搜索后能定位匹配项，`n/N` 能循环跳转。
- `:tabnew` 新建 tab；`gt`/`gT` 循环切换；`N gt` 跳到第 N 个；`:tabclose` 关闭；`:tabmove` 调整顺序。
- 两个 tab 打开不同目录，切换后各自光标位置独立保持。
- 在一个 tab 中进入归档，切到另一个 tab 再切回，归档状态保持（M7 完成后回归验证）。
- M1 阶段 `Tab` / `AppState` 结构已落地，撤销栈预留 tab 标识。

## 4. M2 — 文件操作与 `u` 撤销

### 基础操作

- 复制、移动、删除、重命名、新建目录/文件均可用。
- 操作前有必要的确认提示；取消操作不产生副作用。
- 操作失败（权限不足、目标已存在等）有错误提示，不 panic。

### 撤销

- 删除文件后按 `u` 能恢复。
- 重命名后按 `u` 能恢复原名。
- 移动后按 `u` 能移回原路径。
- 新建目录/文件后按 `u` 能移除。
- 连续多次操作后，`u` 按逆序逐条撤销。
- 不可撤销操作有明确提示，不会静默失败。
- 删除走可恢复路径，保证 `u` 可逆。

### 跨 tab

- 在 tab 1 删除文件，切到 tab 2，按 `u` 能撤销 tab 1 的删除。
- 跨 tab 撤销后，tab 1 的文件恢复，当前 tab 不变（或按设计明确切回并提示）。
- 撤销栈中每个 `UndoableOp` 都记录所属 tab 与路径。

## 5. M3 — 选择、标记与批量操作

- `v` 进入可视选择，移动光标扩大/缩小选择，退出后状态正确。
- `t` 标记多个文件，再次 `t` 取消标记。
- 通过 `:tag` 命令对标记集合执行批量复制/移动/删除，未标记文件不受影响。
- 批量操作后标记状态清空。
- tag（文件标记）与 mark（路径标记）语义未混淆；代码和文档中不出现 marks 实现。

## 6. M4 — 搜索、过滤与跳转

- `/` 搜索能定位当前视图中的匹配项，`n/N` 循环跳转。
- 简单 glob 过滤生效，过滤后光标与状态栏正确。
- 文件夹/归档内搜索复用同一链路，结果一致。
- 不存在 marks 相关命令或按键绑定。

## 7. M5 — 配置与最小命令模式

- 启动时读取 TOML 配置文件；配置缺失时使用默认值，不报错退出。
- 基本选项（如默认视图、排序、显示项）生效。
- `:q` 退出；`:cd` 切换目录；`:mkdir` 新建目录；`:touch` 新建文件；`:help` 打开帮助。
- 多标签命令（`:tabnew` 等）与 M5 命令解析共存，不冲突。
- 配置文件语法错误时有可理解提示，不 panic。

## 8. M6 — 打磨与 v0.1.0 发布

- 错误处理覆盖常见失败路径，用户能看到提示。
- 帮助页可读，覆盖核心按键与命令。
- README 明确声明“参考 vifm，非严格兼容”，并说明运行时依赖。
- 大目录（>10k 项）可滚动，退出不被阻塞；具体量化标准在 M6 补充。
- `v0.1.0` tag 已创建。
- 发布门禁：
  - CI 在 `ubuntu-latest` 与 `windows-latest` 全绿；
  - 两端 `--release` 构建成功并产出归档；
  - Windows 产物在 Win2016 手动冒烟通过；
  - Linux 产物在 CI 或容器中 `--version` 通过。

## 9. M7 — 归档浏览（只读 VFS）

- 普通目录中光标落在 `.zip` / `.tar` / `.tar.gz` 上，`l`/`Enter` 能进入并列出成员。
- 归档内 `j/k/h/l` 导航正确；`h` 在归档根目录返回归档文件所在物理目录。
- 状态栏/面包屑显示 `host:/inner/path`。
- `/` 搜索在归档内可用，`n/N` 跳转正确。
- 归档内不出现删除/重命名/移动入口，或明确禁用。
- 损坏归档不会 panic，给出错误提示。
- 加密归档、空归档、超大归档索引均有可理解处理。
- 打开归档建立成员索引，读取按需解码；内存占用不随归档体积线性增长。
- `.7z` / `.rar` 给出“不支持虚拟浏览，请解压后浏览”的提示。
- 归档浏览不进入撤销栈。

## 10. M8 — 原生 frecency / bookmark 选择器

### 契约

- `rvifm pick` 从 stdin 读候选、向 stdout 输出选中项；选中退出码 0，取消退出码 1。
- `rvifm record` 静默执行，成功不污染 stdout。
- `pick` 选中后检查 redb：`events` 条数不变、`global_tick` 不变。
- 同一路径连续 `pick` 选中但不执行 `cd`，评分不变。
- shell hook 在 `cd` 失败时不调用 `record`，`events` 不增长。
- 文件管理器跳转/打开失败时不调用 `record`。
- 真正 `cd` 成功后 `record` 被调用，评分按 EMS 更新。

### 评分与排序

- `record` 连续调用后 `global_tick` 单调递增。
- 构造“5 次访问 A → 495 次其他访问 → 1 次访问 B”，`pick` 中 B 排在 A 前（read-time aging 生效）。
- 书签在无匹配查询时仍置顶；有查询但未命中 `keyword` 时书签降级。
- 书签取消后保留累积分数并按正常规则衰减。
- 分级匹配按书签 > 文件名 > 中间段 > 通用子串的顺序生效。

### 预览与一致性

- `pick` 的预览与 `rvifm` TUI 预览对同一文件渲染结果一致。
- 预览不通过 `--preview` 外部命令，不 `fork/exec` shell。

### 存储与并发

- 两个终端同时执行 `rvifm record` 各 100 次，`events` 条数为 200，无丢失。
- 杀掉 `record` 进程后重新执行，`events.pending` 能被下次 `pick` 合并。
- TUI 运行中另一终端 `record`，TUI 退出后事件不丢，下次启动评分反映新增访问。
- compaction 后 `pick` 排名与 compaction 前一致。
- 损坏的 redb 文件不会 panic，给出错误并重建。
- bash 和 fish 下 `record` hook 不阻塞提示符。
- 无后台守护进程；compaction 仅由读方启动惰性触发或手动执行。

## 11. M9 — 图标与主题

- 文件列表显示文件类型图标和颜色；目录、常见特殊文件名（Dockerfile、Makefile 等）有图标。
- 图标 Dark / Light 主题可切换，并与 `opaline` 主题明暗一致。
- `opaline` 内置主题可加载，运行时切换生效。
- 用户主题文件放在 `~/.config/rvifm/themes/` 下能被发现和加载。
- `PreviewRenderer` 与文件列表使用同一套图标/主题，不出现两套视觉语义。
- 无 Nerd Font 或设置 `NO_COLOR` 时，图标降级为纯文本，不 panic、不出现乱码方块导致崩溃。
- 不依赖 vifm 图标插件，不要求修改 `vifmrc`。

## 12. M10 — 声明式键位系统

- `yy` 后 `P` 将 yanked 文件移动到目标目录（不是复制）。
- `yy` 后 `p` 将 yanked 文件复制到目标目录。
- `dd` 后 `p` 将“删除”的文件移动到目标目录。
- `prepend_keymap` 覆盖 `P` 的默认行为；`append_keymap` 补充新序列；`keymap` 完全替换默认表。
- 多键序列在超时内等待后续键，超时后按已输入序列解析。
- 模式切换后键位表正确切换（Normal → Visual 后相关绑定生效）。
- `Action` 枚举承载语义，键位解析层不侵入导航/文件操作核心逻辑。
- 未引入完整 Vim 引擎。
- 未实现 Vim 寄存器、宏、`.` 重复、text objects、Vim 脚本映射。
- `keymap-rs` 不达标时，降级为轻量自写 Trie 并通过上述验收。

## 13. M11 — 预览与文件打开

### 图片预览

- 在 Alacritty / 无图形协议终端中，图片以 halfblocks 渲染，不出现空白或错误。
- 在 Kitty / foot（Sixel）终端中，图片以原生协议渲染。
- 启动时通过 `Picker::from_query_stdio()` 探测，失败时有回退，不 panic。
- 未把 Überzug++ / Chafa 作为硬依赖；未为特定终端写硬编码分支。
- tmux/Zellij 的限制在文档中注明，不要求代码层解决。

### 文本与 Office

- 文本预览有语法高亮，复用 `PreviewRenderer`。
- Office（DOCX/XLSX/PPTX）在未安装任何外部工具的干净环境中可预览文本/Markdown。
- Office 预览不调用外部命令行工具。
- Office 格式还原不作为验收要求（标题层级、列表样式可缺失）。
- Office 解析库锁定为 `office_oxide ~0.1.16`；PPTX 图表/SmartArt/组合形状/动画/OLE 等复杂对象被跳过且不 panic。
- PDF 文本提取库锁定为 `pdf-inspector =1.25.2`，且未启用 `ocr` / `render-pdfium` / `vision` feature（用 `cargo tree -e features` 验证）。

### PDF

- 含文本层的 PDF 优先走 `pdf-inspector` 文本提取预览。
- 启用 `pdf-visual` feature 时，文本提取不足（扫描件/图片型）或用户切换视觉预览，`exprim` 栅格化路径可渲染出图像并交给 `ratatui-image`。
- **feature 门控**：默认构建（未启用 `pdf-visual`）的 `cargo tree` 不含 `exprim`；`cargo tree --features pdf-visual` 显示 `exprim v0.4.0`，且 Cargo.toml 固定为 `=0.4.0`，`Cargo.lock` 已入库。
- **像素上限**：`ExportOptions.max_pixels` 配置在 200 万–400 万像素区间；超大 `MediaBox`、超大文件被拒绝且不 panic、内存有界。
- **输入大小上限**：超过阈值的 PDF 在进入 `exprim` 前被拒绝，给出提示，不 panic。
- **语料回归**：CJK、加密、损坏 PDF 语料均不 panic，给出可理解结果或提示。
- **变异测试**：对 PDF 输入做 1,000 次随机字节变异，0 panic。
- **Windows 构建**：`x86_64-pc-windows-msvc` 启用 `pdf-visual` 构建通过。
- 文本提取失败且 feature 未启用时，探测 `pdftoppm` / `mutool` 并渲染图像。
- 均不可用时显示友好提示，并允许 `Enter` 走系统 opener。
- 未安装 Poppler/MuPDF 的干净环境中，PDF 不 panic，提示清晰。
- 不把 `tdf` / `pdfium-render` 作为编译期依赖；release profile 为 `panic = "unwind"`。

### 文件打开

- 零配置下 `Enter` 打开文本/图片/PDF 走系统默认程序。
- Linux 优先 `gio open`，回退 `xdg-open`；macOS 用 `open`；Windows 用 `start`。
- 目录在 `rvifm` 内部进入，文件才调用系统 opener。
- `prepend_rules` 中 `image/*` 绑定自定义 opener 后，图片不再走系统默认。
- `append_rules` 补充的规则不覆盖默认，仅在默认未命中时生效。
- `rules` 完全替换默认规则表。
- opener 命令执行失败时有错误提示，不 panic。

## 14. 发布后回归

每次发布前至少执行：

- 双平台 CI 全绿。
- `rvifm --version` / `--help` 在 Linux 与 Windows 产物上通过。
- 手工冒烟：浏览、多标签、复制/移动/删除、`u` 撤销、搜索、归档浏览、`pick`/`record`、图片 halfblocks、系统 opener。
- 回归检查：marks 未出现、`pick` 未写事件、归档内只读、未引入禁用依赖。

## 15. M12 — 权限不足时的内联提权

- 在普通目录 yy 文件，切到仅 root 可写的目录按 p，TUI 内弹出密码提示。
- 输入正确密码后复制成功，TUI 刷新并显示新文件。
- 取消密码提示后操作中止，光标位置和已标记文件不变。
- rvifm 进程本身不以 root 运行（ps 确认）。


## 16. 人工验证清单（v0.2.0 手册，测试人员逐项勾选）

> 环境：Windows Terminal / PowerShell（提权节仅 Linux）。产物下载自
> GitHub Release `v0.2.0`，解包后直接运行 `rvifm.exe`；或
> `cargo install rvifm --version 0.2.0`。
> 准备：建工作目录 `rvifm-test/`，内含 3 个文本文件、1 张 PNG 图片、
> 1 个 DOCX、1 个含文本层的 PDF、`demo.zip` 与 `demo.tar.gz` 各一
> （内含子目录与文件）、1 个子目录 `sub/`。

### A. 启动与版本

- [ ] `rvifm --version` 输出 `rvifm 0.2.0`，退出码 0。
- [ ] `rvifm --help` 显示用法与子命令，退出码 0。
- [ ] `rvifm` 启动进入 TUI（列表 + 右侧预览窗格 + 底部状态栏），
      `q` 退出干净，无乱码残留、无 panic。

### B. 浏览导航

- [ ] `j/k` 光标上下移动，预览窗格跟随光标切换内容。
- [ ] `l`/`Enter` 进入 `sub/`；`h` 返回父目录。
- [ ] `gg` 跳首行、`G` 跳末行、`Ctrl-u`/`Ctrl-d` 半页滚动。

### C. 多标签

- [ ] `:tabnew` 新建标签；`gt`/`gT` 切换；`2 gt` 跳第二个；`:tabclose` 关闭。
- [ ] 两个 tab 各自浏览不同目录，切回时目录/光标/搜索词保持独立。

### D. 搜索与过滤

- [ ] `/txt` 搜索，`n`/`N` 正反向跳匹配；状态栏显示搜索词。
- [ ] `=*.txt` 过滤，列表只剩匹配项，状态栏出现 `[filter: *.txt]`；
      Esc 清除。

### E. 可视选择与标记

- [ ] `v` 进入可视选择（状态栏 VISUAL 计数），`j/k` 扩选，`Esc` 退出。
- [ ] `t` 标记 2 个文件（加粗），状态栏 `2 tagged`。
- [ ] `:tag copy <目录>` 批量复制成功，完成后标记自动清空。

### F. 文件操作与撤销

- [ ] `dd` → 状态栏确认提示 → `y`：文件消失（进应用内暂存）。
- [ ] `u`：文件恢复原位，`u` 连按可逆序撤销多步。
- [ ] `:rename 新名` / `:move ..` / `:copy ..` / `:mkdir d1` / `:touch f1`
      各执行一次并各 `u` 一次还原。
- [ ] 不可逆操作（对已删除文件再次操作）有明确提示，不静默。

### G. 归档浏览（只读）

- [ ] 光标停在 `demo.zip` 按 `l`：进入归档虚拟目录，状态栏/面包屑显示
      `demo.zip:/内部路径`。
- [ ] 归档内 `j/k`/`l`/`h` 正常；`dd` 等写操作被拒绝（只读提示）。
- [ ] `h` 一路返回到物理目录；`demo.tar.gz` 同样验证。

### H. 预览窗格（M11 核心，含 M11-6）

- [ ] 光标在文本文件：预览显示文本内容。
- [ ] 光标在目录：预览显示成员列表（目录优先排序）。
- [ ] 光标在 PNG：预览窗格以 **halfblocks 色块**渲染出图片（无图形协议
      终端即验证此项 = M11-6 的 Windows 部分）；Kitty/WezTerm 下若支持
      则显示原生协议图片。
- [ ] 光标在 DOCX：预览显示提取的文本；不依赖任何外部工具。
- [ ] 光标在 PDF：预览显示文本层内容；损坏/加密 PDF 显示友好提示，
      不崩溃。
- [ ] 图片/Office/PDF 预览失败时降级为提示行，TUI 不 panic。

### H2. 跳转选择器（M8-12：o / :j / :J）

- [ ] Normal 模式按 `o`：弹出合一选择器（列出 frecency 中目录+文件混合条目），输入模糊词实时筛选，`j/k` 移动，`Enter` 选定后：目录 → cd；文件 → 切到其父目录并光标定位；`Esc` 取消无副作用。
- [ ] `:j`：仅目录条目的选择器（dir 时钟排名）；`:J`：仅文件条目（file 时钟排名）。
- [ ] 高频 cd 多个目录后，`o` 的排名随统一访问频率变化；`:j` 的目录排名与 `o` 中目录子集排名可能不同（独立时钟 vs 统一时钟）。
- [ ] 选择器打开/取消/选定全程不产生残留进程、不 panic。

### I. 文件打开

- [ ] 光标在文本文件按 `Enter`：系统默认程序打开该文件（记事本）。
- [ ] 光标在图片按 `Enter`：系统看图程序打开。
- [ ] 光标在目录按 `Enter`：rvifm 内部进入（不调外部 opener）。

### J. frecency

- [ ] `echo a.txt | rvifm pick` 输出 `a.txt`（非 TTY 管道模式）。
- [ ] `rvifm record --topic dir <目录>` 后 `rvifm pick` 交互模式
      （真实终端）能列出该目录候选。
- [ ] `rvifm maintenance compact` 正常执行退出。

### K. 图标与主题

- [ ] 文件列表/预览显示 devicons 类型图标（Nerd Font 终端）；
      无 Nerd Font 终端显示占位字符，不 panic、不错位。
- [ ] `NO_COLOR=1 rvifm` 启动无颜色输出，渲染正常。

### L. 提权（仅 Linux/macOS；Windows 验证降级）

- [ ] Linux：对 root 可写目录执行复制/移动（`yy`+`p` 或 `:copy`），
      EACCES 后出现 `sudo 密码:` 提示（密码以 `*` 显示）。
- [ ] 输入正确密码：操作成功、列表刷新、提示"提权完成"。
- [ ] 输入错误密码：提示"提权失败"，TUI 状态与操作前一致。
- [ ] `Esc` 取消：中止操作，光标/标记不变，无半完成状态。
- [ ] `ps` 确认 rvifm 主进程 UID 未变（非 root）。
- [ ] Windows：触发写权限错误时显示"提权仅支持 Unix"，不 panic。

### M. 回归红线

- [ ] 全程无 `m`/`'` marks 行为出现。
- [ ] `pick` 执行后无事件写入（`pick` 前后 `events` 表 tick 不变；
      或对照 `%LOCALAPPDATA%\rvifm\` 下数据库文件 mtime）。
- [ ] 归档浏览全程无文件修改。
- [ ] 任务管理器/进程列表无 rvifm 残留进程。
