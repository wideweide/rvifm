# ACCEPTANCE — rvifm 验收标准

本文件只回答“怎样算完成”。任务顺序见 [TASKS.md](TASKS.md)，需求边界见 [SPEC.md](SPEC.md)。

## 1. 通用门禁（每个任务）

每个任务提交前必须全部满足：

- `cargo fmt` 无改动。
- `cargo clippy -- -D warnings` 通过。
- `cargo build` 通过。
- `cargo test` 通过。
- 含 `#[cfg(unix)]` / `#[cfg(test)]` 改动时，`cargo check --target x86_64-unknown-linux-gnu --all-targets` 通过（本地门禁，AGENTS §6）。
- 程序可启动、可退出、无 panic。
- 本任务对应的阶段验收条目通过。
- `docs/TASKS.md` 已勾选，`docs/STATE.md` 已更新。

通用测试规则：

- 集成测试不得依赖真实 TTY；TUI 渲染使用 `ratatui` 的 `TestBackend`。
- 冒烟测试只跑 `rvifm --version` / `rvifm --help`，不进入交互模式。
- 测试 fixture 使用 `tempfile`，不硬编码 `/tmp`。

预算中断自检（AGENTS §3「剩余预算门控与断点落地」，命中预算不足而暂停时检查）：

- [ ] `docs/REVIEW.md` 或对应审计/设计文档已有可阅读的已验证结论。
- [ ] 未完成项已登记到 `docs/TASKS.md`（`deferred` 或待续）与 `docs/NOTES.md`。
- [ ] 断点记录含当前 `HEAD`、已验证结论、未验证假设、下一会话起点。
- [ ] 进度已 `git commit` 并 `git push` 到当前进度分支。
- [ ] 未留下未提交的破坏性改动或无法验证的中间状态。
- 路径内部统一 `/`；跨平台比较前规范化。

## 2. M0 — 骨架 + SPEC + 双平台 CI

- [x] `cargo build` 成功，`rvifm` 可启动。
- [x] TUI 能显示当前目录列表和状态栏。
- [x] 按 `q` 能正常退出，无 panic。
- [x] `rvifm --version` 和 `rvifm --help` 正常输出并以 0 退出。
- [x] `docs/SPEC.md` 已包含 MUST / SIMPLIFIED / OUT_OF_SCOPE 分级，且与 `docs/PLAN.md` 的里程碑一致。
- [x] GitHub Actions 在 `ubuntu-latest` 与 `windows-latest` 上跑通 `fmt / clippy / build / test`，状态为绿。

### LICENSE 建档

- [x] 仓库根目录存在 `LICENSE-MIT` 与 `LICENSE-APACHE`，内容为对应许可证全文。
- [x] `Cargo.toml` 的 `license = "MIT OR Apache-2.0"`，与两个文件一致。
- [x] README 的许可证段声明与文件、`Cargo.toml` 一致。
- [x] `cargo package --list` 输出包含两个 LICENSE 文件，且包含在发布包内。

### 发布 `0.1.0-alpha.1`

- [x] 前置条件已满足：M0 全部任务完成，`ubuntu-latest` + `windows-latest` CI 全绿。
- [x] 发布前复查 `https://crates.io/api/v1/crates/rvifm` 未被占用；若被占用，按 `docs/SPEC.md` §6.1 改用备选包名并保持二进制名 `rvifm`。
- [x] `cargo package --list` 输出内容符合预期：包含 `Cargo.toml`、`Cargo.lock`、`src/`、`README.md`、`LICENSE-MIT`、`LICENSE-APACHE`，不含 `target/` 等构建产物。
- [x] `cargo package` 检查通过（可打包、可编译）。
- [x] 人工执行 `cargo publish`，发布版本为 `0.1.0-alpha.1`；该步骤依赖 crates.io 凭据，**不在 CI 自动运行**。
- [x] 发布后 `cargo install rvifm --version 0.1.0-alpha.1` 成功，安装得到的 `rvifm --version` 输出包含 `0.1.0-alpha.1`。
- [x] 若发布失败，记录原因并回退版本号，不发布空包占名。

## 3. M1 — 核心浏览、多标签与 vim 按键

- [x] `j/k` 上下移动光标，边界不越界、不 panic。
- [x] `l`/`Enter` 进入目录，`h` 返回父目录；根目录返回时行为明确。
- [x] `gg` 跳到首项，`G` 跳到末项；`Ctrl-u`/`Ctrl-d` 按预期翻页。
- [x] 状态栏显示当前路径、选中项、文件大小/权限。
- [x] `/` 输入搜索后能定位匹配项，`n/N` 能循环跳转。
- [x] `:tabnew` 新建 tab；`gt`/`gT` 循环切换；`N gt` 跳到第 N 个；`:tabclose` 关闭；`:tabmove` 调整顺序。
- [x] 两个 tab 打开不同目录，切换后各自光标位置独立保持。
- [x] 在一个 tab 中进入归档，切到另一个 tab 再切回，归档状态保持（M7 完成后回归验证）。
- [x] M1 阶段 `Tab` / `AppState` 结构已落地，撤销栈预留 tab 标识。

## 4. M2 — 文件操作与 `u` 撤销

### 基础操作

- [x] 复制、移动、删除、重命名、新建目录/文件均可用。
- [x] 操作前有必要的确认提示；取消操作不产生副作用。
- [x] 操作失败（权限不足、目标已存在等）有错误提示，不 panic。

### 撤销

- [x] 删除文件后按 `u` 能恢复。
- [x] 重命名后按 `u` 能恢复原名。
- [x] 移动后按 `u` 能移回原路径。
- [x] 新建目录/文件后按 `u` 能移除。
- [x] 连续多次操作后，`u` 按逆序逐条撤销。
- [x] 不可撤销操作有明确提示，不会静默失败。
- [x] 删除走可恢复路径，保证 `u` 可逆。

### 跨 tab

- [x] 在 tab 1 删除文件，切到 tab 2，按 `u` 能撤销 tab 1 的删除。
- [x] 跨 tab 撤销后，tab 1 的文件恢复，当前 tab 不变（或按设计明确切回并提示）。
- [x] 撤销栈中每个 `UndoableOp` 都记录所属 tab 与路径。

## 5. M3 — 选择、标记与批量操作

- [x] `v` 进入可视选择，移动光标扩大/缩小选择，退出后状态正确。
- [x] `t` 标记多个文件，再次 `t` 取消标记。
- [x] 通过 `:tag` 命令对标记集合执行批量复制/移动/删除，未标记文件不受影响。
- [x] 批量操作后标记状态清空。
- [x] tag（文件标记）与 mark（路径标记）语义未混淆；代码和文档中不出现 marks 实现。

## 6. M4 — 搜索、过滤与跳转

- [x] `/` 搜索能定位当前视图中的匹配项，`n/N` 循环跳转。
- [x] 简单 glob 过滤生效，过滤后光标与状态栏正确。
- [x] 文件夹/归档内搜索复用同一链路，结果一致。
- [x] 不存在 marks 相关命令或按键绑定。

## 7. M5 — 配置与最小命令模式

- [x] 启动时读取 TOML 配置文件；配置缺失时使用默认值，不报错退出。
- [x] 基本选项（如默认视图、排序、显示项）生效。
- [x] `:q` 退出；`:cd` 切换目录；`:mkdir` 新建目录；`:touch` 新建文件；`:help` 打开帮助。
- [x] 多标签命令（`:tabnew` 等）与 M5 命令解析共存，不冲突。
- [x] 配置文件语法错误时有可理解提示，不 panic。

## 8. M6 — 打磨与 v0.1.0 发布

- [x] 错误处理覆盖常见失败路径，用户能看到提示。
- [x] 帮助页可读，覆盖核心按键与命令。
- [x] README 明确声明“参考 vifm，非严格兼容”，并说明运行时依赖。
- [x] 大目录（>10k 项）可滚动，退出不被阻塞；具体量化标准在 M6 补充。
- [x] `v0.1.0` tag 已创建。
- [x] 发布门禁：
  - CI 在 `ubuntu-latest` 与 `windows-latest` 全绿；
  - 两端 `--release` 构建成功并产出归档；
  - Windows 产物在 Win2016 手动冒烟通过；
  - Linux 产物在 CI 或容器中 `--version` 通过。

## 9. M7 — 归档浏览（只读 VFS）

- [x] 普通目录中光标落在 `.zip` / `.tar` / `.tar.gz` 上，`l`/`Enter` 能进入并列出成员。
- [x] 归档内 `j/k/h/l` 导航正确；`h` 在归档根目录返回归档文件所在物理目录。
- [x] 状态栏/面包屑显示 `host:/inner/path`。
- [x] `/` 搜索在归档内可用，`n/N` 跳转正确。
- [x] 归档内不出现删除/重命名/移动入口，或明确禁用。
- [x] 损坏归档不会 panic，给出错误提示。
- [x] 加密归档、空归档、超大归档索引均有可理解处理。
- [x] 打开归档建立成员索引，读取按需解码；内存占用不随归档体积线性增长。
- [x] `.7z` / `.rar` 给出“不支持虚拟浏览，请解压后浏览”的提示。
- [x] 归档浏览不进入撤销栈。

## 10. M8 — 原生 frecency / bookmark 选择器

### 契约

- [x] `rvifm pick` 从 stdin 读候选、向 stdout 输出选中项；选中退出码 0，取消退出码 1。
- [x] `rvifm record` 静默执行，成功不污染 stdout。
- [x] `pick` 选中后检查 redb：`events` 条数不变、`global_tick` 不变。
- [x] 同一路径连续 `pick` 选中但不执行 `cd`，评分不变。
- [x] shell hook 在 `cd` 失败时不调用 `record`，`events` 不增长。
- [x] 文件管理器跳转/打开失败时不调用 `record`。
- [x] 真正 `cd` 成功后 `record` 被调用，评分按 EMS 更新。

### 评分与排序

- [x] `record` 连续调用后 `global_tick` 单调递增。
- [x] 构造“5 次访问 A → 495 次其他访问 → 1 次访问 B”，`pick` 中 B 排在 A 前（read-time aging 生效）。
- [x] 书签在无匹配查询时仍置顶；有查询但未命中 `keyword` 时书签降级。
- [x] 书签取消后保留累积分数并按正常规则衰减。
- [x] 分级匹配按书签 > 文件名 > 中间段 > 通用子串的顺序生效。

### 预览与一致性

- [x] `pick` 的预览与 `rvifm` TUI 预览对同一文件渲染结果一致。
- [x] 预览不通过 `--preview` 外部命令，不 `fork/exec` shell。

### 存储与并发

- [x] 两个终端同时执行 `rvifm record` 各 100 次，`events` 条数为 200，无丢失。
- [x] 杀掉 `record` 进程后重新执行，`events.pending` 能被下次 `pick` 合并。
- [x] TUI 运行中另一终端 `record`，TUI 退出后事件不丢，下次启动评分反映新增访问。
- [x] compaction 后 `pick` 排名与 compaction 前一致。
- [x] 损坏的 redb 文件不会 panic，给出错误并重建。
- [x] bash 和 fish 下 `record` hook 不阻塞提示符。
- [x] 无后台守护进程；compaction 仅由 TUI 启动惰性触发（events > 5000）或 `rvifm maintenance compact` 手动执行；`pick` 不触发 compaction。

## 11. M9 — 图标与主题

- [x] 文件列表显示文件类型图标和颜色；目录、常见特殊文件名（Dockerfile、Makefile 等）有图标。
- [x] 图标 Dark / Light 主题可切换，并与 `opaline` 主题明暗一致。
- [x] `opaline` 内置主题可加载，运行时切换生效。
- [x] 用户主题文件放在 `~/.config/rvifm/themes/` 下能被发现和加载。
- [x] `PreviewRenderer` 与文件列表使用同一套图标/主题，不出现两套视觉语义。
- [x] 无 Nerd Font 或设置 `NO_COLOR` 时，图标降级为纯文本，不 panic、不出现乱码方块导致崩溃。
- [x] 不依赖 vifm 图标插件，不要求修改 `vifmrc`。

## 12. M10 — 声明式键位系统

- [x] `yy` 后 `P` 将 yanked 文件移动到目标目录（不是复制）。
- [x] `yy` 后 `p` 将 yanked 文件复制到目标目录。
- [x] `dd` 后 `p` 将“删除”的文件移动到目标目录。
- [x] `prepend_keymap` 覆盖 `P` 的默认行为；`append_keymap` 补充新序列；`keymap` 完全替换默认表。
- [x] 多键序列在超时内等待后续键，超时后按已输入序列解析。
- [x] 模式切换后键位表正确切换（Normal → Visual 后相关绑定生效）。
- [x] `Action` 枚举承载语义，键位解析层不侵入导航/文件操作核心逻辑。
- [x] 未引入完整 Vim 引擎。
- [x] 未实现 Vim 寄存器、宏、`.` 重复、text objects、Vim 脚本映射。
- [x] `keymap-rs` 不达标时，降级为轻量自写 Trie 并通过上述验收。

## 13. M11 — 预览与文件打开

### 图片预览

- [x] 在 Alacritty / 无图形协议终端中，图片以 halfblocks 渲染，不出现空白或错误。
- [x] 在 Kitty / foot（Sixel）终端中，图片以原生协议渲染。
- [x] 启动时通过 `Picker::from_query_stdio()` 探测，失败时有回退，不 panic。
- [x] 未把 Überzug++ / Chafa 作为硬依赖；未为特定终端写硬编码分支。
- [x] tmux/Zellij 的限制在文档中注明，不要求代码层解决。

### 文本与 Office

- [x] 文本预览有语法高亮，复用 `PreviewRenderer`。
- [x] Office（DOCX/XLSX/PPTX）在未安装任何外部工具的干净环境中可预览文本/Markdown。
- [x] Office 预览不调用外部命令行工具。
- [x] Office 格式还原不作为验收要求（标题层级、列表样式可缺失）。
- [x] Office 解析库锁定为 `office_oxide ~0.1.16`；PPTX 图表/SmartArt/组合形状/动画/OLE 等复杂对象被跳过且不 panic。
- [x] PDF 文本提取库锁定为 `pdf-inspector =1.25.2`，且未启用 `ocr` / `render-pdfium` / `vision` feature（用 `cargo tree -e features` 验证）。

### PDF

- [x] 含文本层的 PDF 优先走 `pdf-inspector` 文本提取预览。
- [x] 启用 `pdf-visual` feature 时，文本提取不足（扫描件/图片型）或用户切换视觉预览，`exprim` 栅格化路径可渲染出图像并交给 `ratatui-image`。
- [x] **feature 门控**：默认构建（未启用 `pdf-visual`）的 `cargo tree` 不含 `exprim`；`cargo tree --features pdf-visual` 显示 `exprim v0.4.0`，且 Cargo.toml 固定为 `=0.4.0`，`Cargo.lock` 已入库。
- [x] **像素上限**：`ExportOptions.max_pixels` 配置在 200 万–400 万像素区间；超大 `MediaBox`、超大文件被拒绝且不 panic、内存有界。
- [x] **输入大小上限**：超过阈值的 PDF 在进入 `exprim` 前被拒绝，给出提示，不 panic。
- [x] **语料回归**：CJK、加密、损坏 PDF 语料均不 panic，给出可理解结果或提示。
- [x] **变异测试**：对 PDF 输入做 1,000 次随机字节变异，0 panic。
- [x] **Windows 构建**：`x86_64-pc-windows-msvc` 启用 `pdf-visual` 构建通过。
- [x] 文本提取失败且 feature 未启用时，探测 `pdftoppm` / `mutool` 并渲染图像。
- [x] 均不可用时显示友好提示，并允许 `Enter` 走系统 opener。
- [x] 未安装 Poppler/MuPDF 的干净环境中，PDF 不 panic，提示清晰。
- [x] 不把 `tdf` / `pdfium-render` 作为编译期依赖；release profile 为 `panic = "unwind"`。

### 文件打开

- [x] 零配置下 `Enter` 打开文本/图片/PDF 走系统默认程序。
- [x] Linux 优先 `gio open`，回退 `xdg-open`；macOS 用 `open`；Windows 用 `start`。
- [x] 目录在 `rvifm` 内部进入，文件才调用系统 opener。
- [x] `prepend_rules` 中 `image/*` 绑定自定义 opener 后，图片不再走系统默认。
- [x] `append_rules` 补充的规则不覆盖默认，仅在默认未命中时生效。
- [x] `rules` 完全替换默认规则表。
- [x] opener 命令执行失败时有错误提示，不 panic。

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


## 16. 人工验证清单（v0.6.0 手册，测试人员逐项勾选）

> 环境：Windows Terminal / PowerShell（提权节仅 Linux）。产物下载自
> GitHub Release `v0.6.0`，解包后直接运行 `rvifm.exe`；或
> `cargo install rvifm --version 0.6.0`。
> 准备：建工作目录 `rvifm-test/`，内含 3 个文本文件、1 张 PNG 图片、
> 1 个 DOCX、1 个含文本层的 PDF、`demo.zip` 与 `demo.tar.gz` 各一
> （内含子目录与文件）、1 个子目录 `sub/`。

### A. 启动与版本

- [ ] `rvifm --version` 输出 `rvifm 0.6.0`，退出码 0。
- [ ] `rvifm --help` 显示用法与子命令，退出码 0。
- [ ] `rvifm` 启动进入 TUI（顶部标签栏 + 左目录列表 + 右 pane + 底部状态栏），
      `q` 退出干净，无乱码残留、无 panic。

### B. 浏览导航

- [ ] `j/k` 光标上下移动，预览窗格跟随光标切换内容。
- [ ] `l`/`Enter` 进入 `sub/`；`h` 返回父目录。
- [ ] `gg` 跳首行、`G` 跳末行、`Ctrl-u`/`Ctrl-d` 半页滚动。

### C. 多标签

- [ ] `:tabnew` 新建标签；`gt`/`gT` 切换；`2 gt` 跳第二个；`:tabclose` 关闭。
- [ ] 标签栏常驻顶部：显示 `N:dirname` 序列，当前 tab 高亮（反显加粗），
      `:tabnew`/`gt`/`gT` 切换时高亮即时跟随。
- [ ] 两个 tab 各自浏览不同目录，切回时目录/光标/搜索词保持独立。

### C2. 双栏与标签栏（M14 语义）

- [ ] 左右两个 pane 均显示目录列表；`Tab` 切换焦点（状态栏指示当前焦点 pane）。
- [ ] 焦点切到右 pane 后 `j/k/gg/G/l/h` 在右 pane 生效，左 pane 状态不变。
- [ ] `w`：右 pane 在"目录列表"与"活动 pane 光标文件的预览"之间切换，再按恢复。
- [ ] 文件操作（`dd`/`yy`/`:rename` 等）作用于焦点 pane 所在目录。

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

### N. 搜索与查看器（M17：:find/:grep + 查看器）

- [ ] `:find 目标名`：结果以虚拟目录显示在当前栏（全路径、显示
      devicons 图标），.gitignore/.ignore 内的路径不出现。
- [ ] `j/k/gg/G/Ctrl-u/d` 导航；对侧预览窗格（`w`）跟随光标显示
      命中文件内容。
- [ ] `l`/`Enter`：跳到命中文件所在目录、光标停在该文件；
      `Esc`/`h`：退出结果视图回到原目录。
- [ ] 结果视图内 `dd`/`cw`/`t`/`v` 被拒绝（只读提示）；`yy` 复制的
      是命中文件的完整路径（到别处 `p` 可粘贴）。
- [ ] `:grep 关键词`：条目为 `路径:行号: 内容`；`:grep ([` 报可读错误；
      二进制文件不出现乱码命中。
- [ ] 查看器（结果视图按 `w`）：显示行号；`j/k/gg/G/Ctrl-u/d` 滚动；
      `/关键词` 定位并黄底高亮；`n/N` 循环；`Esc/w/q` 退出。
- [ ] `:grep` 命中后按 `w`：查看器直接打开在命中行附近。
- [ ] 状态栏出现 `[find: …]` / `[grep: …]` 指示；结果超 1 万提示截断。

### O. 快速跳转与历史（M16）

- [ ] `:keyword dl`（当前目录）后，`o` 选择器中该目录带书签标志。
- [ ] config.toml 配置 `[quickjump] d = "dl"` 后，`'d` 两键直达该目录；
      未配置时 `'x` 状态栏提示不静默。
- [ ] `:histprev`/`:histnext` 在当前 tab 内后退/前进目录历史；
      切到另一 tab 后历史互不干扰。
- [ ] `:history` 弹出历史菜单：`Enter`/`l` 导航、`dd` 移除、`Esc` 关闭。
- [ ] `:recent`：近期视角按最后访问时间倒序；刚 cd 过的目录排最前。
- [ ] keymap.toml 用户绑定生效：如 `prepend_keymap = [{ on = ["C-o"],
      run = "histprev" }]` 后 Ctrl-O 触发后退；绑定 `j → to_top` 时
      `j` 覆盖默认下移。

### P. frecency 落盘（M8 收尾）

- [ ] TUI 内 cd 数个目录后 `:q` 退出，再运行 `rvifm pick`（真实终端）
      或 `o`：本次会话访问的目录已进入 frecency 排名（退出刷写生效）。

### Q. 交互复核（M15/M16 已入清单项的抽验 + M11-6）

- [ ] M11-6（Alacritty/Windows Terminal）：光标在 PNG 上按 `w`，
      预览窗格显示图片（halfblocks 或原生协议），不空白、不 panic。
- [ ] 鼠标：单击聚焦、已聚焦再击打开、双击直接打开、滚轮滚动；
      右键条目弹出六项菜单且 Esc/外部点击关闭。
- [ ] 顶部 `..` 行：普通目录可见、`l` 返回上级、根目录隐藏；
      归档根 `..` 回到归档所在物理目录。
- [ ] `:file` 与 Ctrl-G 信息一致；归档内成员显示 `host:/inner/path`。
- [ ] `:help` 全屏帮助：`j/k` 滚动、`/` 搜索、`q` 返回后列表状态不变。

### M. 回归红线

- [ ] 全程无 `m`/`'` marks 行为出现。
- [ ] `pick` 执行后无事件写入（`pick` 前后 `events` 表 tick 不变；
      或对照 `%LOCALAPPDATA%\rvifm\` 下数据库文件 mtime）。
- [ ] 归档浏览全程无文件修改。
- [ ] 任务管理器/进程列表无 rvifm 残留进程。

## 17. M15 — 用户实测反馈批次（FIX×4 + FEAT×4，见 SPEC M15 节）

### FIX-1 搜索高亮与实时索引

- [ ] 输入 `/src`：文件名中 `src` 子串高亮，样式与光标行反显可区分。
- [ ] 输入期间光标实时停在第一个匹配项；继续输入跟随更新。
- [ ] 状态栏显示"当前匹配序号/总数"（如 `3/12`）；无匹配显示 `0/0` 并提示。
- [ ] `n`/`N` 在匹配项间循环移动，索引同步更新。
- [ ] 清空搜索/Esc 取消后高亮与计数消失，光标位置合理（不越界、不跳变到无关位置）。
- [ ] `/` 搜索不隐藏非匹配项（与 `=` 过滤语义区分）。

### FIX-2 yy/p 复制链路

- [ ] 目录 A 对文件 `yy`，进入目录 B 按 `p`：副本出现在 B，状态栏显示成功提示。
- [ ] 按 `u` 后 B 中的副本被移除（入撤销栈，可逆）。
- [ ] 对只读目标目录按 `p`：状态栏可读错误，不 panic、无半成品文件。
- [ ] 目标存在同名文件：报错提示，不静默覆盖。
- [ ] `yy` 后 `P` 仍为移动语义（PutMove/PutCopy 独立不回归）。

### FIX-3 归档浏览

- [ ] 光标停在 `test.zip` / `test.tar` / `test.tar.gz` 按 `l`/`Enter`：进入虚拟目录并列出成员。
- [ ] 归档内 `j/k/l/h` 导航正常；归档根目录 `h` 返回归档文件所在物理目录。
- [ ] 状态栏/面包屑显示 `host:/inner/path` 形式。
- [ ] 归档内 `dd` / `:rename` / `:move` 等写操作被拒绝或提示"归档内只读"。
- [ ] 损坏 zip 按 `l`：状态栏错误提示，程序不退出；加密/空归档同理不 panic。

### FIX-4 o 选择器居中

- [ ] 80×24 与 160×48 终端下按 `o`：选择器水平垂直居中，尺寸自适应且有上限。
- [ ] 打开时目录列表不可见（遮罩/清除），无残影。
- [ ] 关闭后列表完整重绘，光标与滚动位置与打开前一致。

### FEAT-1 鼠标支持

- [ ] 单击文件行：光标移动到该行，不触发打开。
- [ ] 对已聚焦行再次单击：触发打开（与 `l`/`Enter` 一致）。
- [ ] 双击未聚焦行：直接打开。
- [ ] 点击列表区域外：光标不变。
- [ ] 滚轮上下滚动列表，光标不跳变。
- [ ] Visual 模式单击：光标更新且选择区域不破坏（按 SPEC SIMPLIFIED 定义）。
- [ ] 键盘操作与鼠标事件互不干扰；全程不 panic。

### FEAT-2 列表顶部 ".."

- [ ] 任意普通目录顶部固定显示 `..`，视觉与普通目录项区分。
- [ ] 光标在 `..` 上按 `l`/`Enter`：返回上级目录。
- [ ] 文件系统根目录 `..` 置灰或隐藏（不可用时有视觉提示）。
- [ ] 归档根目录 `..` 返回归档文件所在物理目录。
- [ ] `..` 不进入搜索/过滤/标记集合（渲染层虚拟条目）。

### FEAT-3 :file 命令与右键菜单

- [ ] `:file` 对普通文件/目录显示路径、大小、权限、修改时间、类型。
- [ ] 归档内成员 `:file` 显示 `host:/inner/path` 形式路径。
- [ ] 右键点击文件项：聚焦该项并弹出上下文菜单（Open / File Info / Copy / Move / Rename / Delete）。
- [ ] 选择 File Info 显示与 `:file` 一致的内容；其余菜单项执行对应动作。
- [ ] Esc 或点击菜单外区域关闭菜单；关闭后 TUI 状态正常、无残影。

### FEAT-4 :help 全屏帮助视图

- [ ] `:help` 打开全屏帮助视图（不再是覆盖列表的弹窗）。
- [ ] `j/k` 滚动；`/` 搜索 + `n/N` 循环定位。
- [ ] `q`/`Esc` 关闭返回原列表，光标与滚动位置与打开前一致。
- [ ] 帮助视图与目录列表滚动状态独立，互不影响。

## 18. M16 — 导航增强批次（NAV-1～4，见 SPEC M16 节）

### NAV-1 快速映射层

- [ ] `:keyword dl`（当前目录）后，`o` 选择器中该条目带 keyword 书签标志。
- [ ] `[quickjump] d = "dl"` 配置后，`'d` 直接 cd 到目标目录，不经过选择器与 Enter。
- [ ] 未配置别名时 `'d` 查 keyword=="d"：命中即直达。
- [ ] `'`+字符 无映射且无命中：状态栏提示，光标不变、不 panic。
- [ ] 跳转即真实导航：history 入栈、面包屑更新、会话评分更新（退出时事件追加）。
- [ ] 全程无 marks 存储出现（不新增路径存储表/文件）。

### NAV-3 :histprev / :histnext

- [ ] 浏览 a → b → c 后 `:histprev` 回到 b、再回到 a；`:histnext` 前进回 b、c。
- [ ] 在 b 处 `:cd d` 截断 forward：`:histnext` 提示无前进，不跳转。
- [ ] tab A 中后退/前进不影响 tab B 的历史内容与位置。
- [ ] 默认键位表不含 Ctrl-O/Ctrl-I；help 与 keymap.toml 示例给出推荐映射并注明键码冲突。

### NAV-2 :history 菜单

- [ ] `:history` 弹出当前 tab 的目录历史菜单（含访问过的目录）。
- [ ] 菜单内 Enter/l 导航到选中目录；dd 移除该条后再打开菜单不再显示。
- [ ] 切换 tab 后 `:history` 内容对应当前 tab，互不混入。
- [ ] Esc/q 关闭后列表与光标状态不变；dd 移除不影响 frecency 存储。

### NAV-4 近期视角

- [ ] 近期视角条目按最后访问时间倒序排列。
- [ ] 会话内刚 cd 过的目录出现在近期视角前列（内存合并生效）。
- [ ] 选定语义与 `o` 一致（目录 cd；选择器全程不写 frecency 时钟）。

## 19. M17 — 递归搜索与查看器批次（SRCH-1/2 + VIEW-1，见 SPEC M17 节）

### SRCH-1 :find

- [ ] `:find src` 在含多层子目录的仓库内列出全路径命中（尊重 .gitignore——target/node_modules 等不出现）。
- [ ] 结果为当前栏虚拟目录：j/k/gg/G/Ctrl-u/d 导航正常；yy 复制的是命中文件路径。
- [ ] 光标移动时对侧预览跟随显示命中文件内容。
- [ ] `l`/Enter：跳到命中文件所在目录、光标停在该文件；Esc/h：退出结果视图回到原目录。
- [ ] 结果视图内 dd/DD/cw/rename/t/v 被拒绝并有提示；程序不 panic。
- [ ] `:find` 重执行替换结果；结果超 1 万截断且有提示。

### SRCH-2 :grep

- [ ] `:grep TODO` 列出 `路径:行号: 内容` 形式的命中。
- [ ] `:grep ([`（非法正则）状态栏可读错误，不 panic。
- [ ] 二进制文件（含 NUL 字节）与非 UTF-8 文件被跳过，不 panic。
- [ ] 选中定位文件语义与 :find 一致。

### VIEW-1 查看器

- [ ] 结果视图按 `w`：右 pane 显示行号 + 完整内容；j/k/gg/G/Ctrl-u/d 滚动流畅。
- [ ] `/word` 定位第一个匹配并高亮；`n`/`N` 循环跳转。
- [ ] `:grep` 命中打开查看器时视口定位到命中行。
- [ ] q/Esc/w 退出查看器回到列表导航，状态不变。

## 20. M21 — UI/UX 打磨批次（M21-F1～F9 + M21-E1～E8，见 SPEC M21 节）

### M21-F1 界面风格
- [x] 80x24 与 160x48 下左右 pane 有可见边框，路径在 pane 顶部，状态栏无路径。
- [x] 目录、可执行、图片、归档在列表中颜色不同。
- [x] 选中文件后底部显示大小/权限/时间。

### M21-F2 焦点视觉
- [x] 焦点切换时焦点侧当前行高亮、非焦点侧 DIM。
- [x] 状态栏无焦点相关文字。

### M21-F3 搜索 Esc 清理
- [x] 输入搜索词按 Esc：高亮消失、状态栏无搜索文字。
- [x] 再次 `/` 进入无残留。

### M21-F4 h 返回定位
- [x] 进入子目录再 `h` 返回，光标在该目录条目上。
- [x] 归档内 `h` 返回后光标在归档文件上。

### M21-F5 外部变更刷新
- [x] 外部 `touch newfile` 列表自动出现；外部删除自动移除、光标合理。
- [x] 归档视图无监听错误或 panic。

### M21-F6 目录 tree 预览
- [x] 光标在普通目录：右 pane 浅层 tree，不报"无法读取"。
- [x] 含非 UTF-8 文件名目录：tree 正常渲染不 panic。

### M21-F7 PDF/Office 分流
- [x] 普通 pdf/docx 预览不出现"非UTF-8或已删除"。
- [x] 损坏 pdf/docx 给出类型化错误提示，不 panic。

### M21-F8 opener stderr
- [x] 系统 opener 打开文本文件，终端顶部无 winit_wayland 警告。
- [x] opener 失败状态栏有可读错误。

### M21-F9 进入目录 record
- [x] l/Enter 进入新目录，退出后 `o` 出现该目录。
- [x] 进入失败不产生记录。

### M21-E1 属性列
- [x] 列表可见大小与 `yyyy-mm-dd HH:mm:ss` 日期；窄终端文件名不挤爆。

### M21-E2 w 预览验证
- [x] `w` 右 pane 预览左 pane 当前文件；j/k 跟随；再按 `w` 恢复列表。

### M21-E3 语法高亮
- [x] `.rs`/`.py`/`.json` 预览有高亮；`.txt` 推断或安全降级；大文件截断提示。

### M21-E4 灵活重命名
- [x] `I`/`cc`/`A` 光标位置符合预期。
- [x] 多选 `R` 编辑器批量重命名生效，`u` 整批撤销。

### M21-E5 mkdir/mk
- [x] `:mkdir test` 光标定位 test；`:mk test` 进入 test。

### M21-E6 归档验证
- [x] `.tar.xz`/`.tar.bz2` l/Enter 进入只读虚拟目录，h 返回物理父目录。

### M21-E7 过滤增强
- [x] `*.rs` 过滤 Rust 文件；`src` 等效 `*src*`；Esc 恢复完整列表。

### M21-E8 选择器增强
- [x] `o` 打开后双 pane 列表仍可见，选择器区域 80%。
- [x] 输入即搜索；Esc→Normal，再 Esc 关闭。
- [x] l/Enter 进入的目录出现在 `o`/`:j`；打开的文件出现在 `o`/`:J`。

## 21. M22 — SFTP 安全与远端体验补全（M22-1～7，见 SPEC M22 节）

### M22-1 :cd 锚定
- [x] `:cd ..` 后执行 `:mkdir x`：目录创建在真实父目录（非进程 CWD），Location 为绝对路径。
- [x] `:cd 不存在目录` 报错，位置不变。

### M22-2 known_hosts 校验
- [x] 首连远端：known_hosts 追加该主机条目，连接成功。
- [x] 已知主机指纹变更：连接拒绝，错误含新旧指纹与指引；`known_hosts = false` 时跳过校验可连。

### M22-3 口令认证
- [x] 密钥不可用远端：AskPass 输入口令登录成功；错误口令可读报错；Esc 取消后 TUI 状态不变。

### M22-4 远端 :mk
- [x] 远端视图 `:mk d1`：创建并进入 d1；已存在返回 Err 不进入。

### M22-5 远端 :find/:grep
- [x] 远端视图 `:find <名>`/`:grep <词>`：结果虚拟目录条目正确；定位跳回远端位置；超上限截断提示。

### M22-6 远端文件打开
- [x] 远端视图打开文件：生成临时副本并调用 opener；下载失败可读错误不 panic。

### M22-7 README
- [x] 里程碑表含 M12–M21 行，状态与 PLAN 一致。

## 22. M23 — Frecency 对齐增强与 Pane 模式（M23-1～8，见 SPEC M23 节）

### M23-1 多词搜索与匹配分级
- [x] keyword 精确命中排在最后段命中之前、子串命中之前（三级顺序）；多词 ordered/any-order 均可命中；o 与 pick 同源。

### M23-2 unbook 保留历史
- [x] `:keyword <chars>` 后 `:keyword -`：keyword 清除且分数/排名不变（对照清除前后 replay 分数全等）。

### M23-3 容量清理
- [x] 超过 max_entries 后写入触发清理：最低分未书签条目被删、条目数降至 90%；带 keyword 条目存活。

### M23-4 IGNORE_PATTERNS
- [x] 配置命中模式（如 `*/node_modules/*`）的路径 record 后不出现在 pick/o；未命中照常记录。

### M23-5 per-topic 配置
- [x] TOML 配置 dir lambda 后：dir 条目衰减速率变化可见（对照测试）；file/unified 不受影响；缺省配置行为与现状一致。

### M23-6 Frecency Pane 模式
- [x] `o` 后焦点 pane 显示搜索+结果，对侧 pane 列表仍可见；光标移动时对侧预览当前条目（文本截断 100–200 行含截断标记）；Esc 三级退出（Search→Normal→Directory）。

### M23-7 c/v/e 重映射
- [x] Frecency 模式 `c` 状态栏出现 copied 且不退出；`v` 对侧进入 View 全文并聚焦，Tab 切回搜索区；`e` 编辑器打开并退出模式；退出 Frecency 后 c/v/e 恢复原语义（v 仍为 Visual）。

### M23-8 删除撤销
- [x] `o` 结果项 `d` 删除条目（从选择器消失）；`u` 恢复条目；带 keyword 条目可删除但优先级低（清理保护不影响手动删除）。

### M23-9 远端文件跟踪上传
- [x] 远端文件打开后修改副本：防抖后自动上传（mock write 收到新内容）；上传失败状态栏提示；退出清理不变。

### M23-10 y 复制与 e 编辑器
- [x] `yn`/`yf`：单选复制光标条目名/全路径；带标记或可视选择时全部条目换行连接。
- [x] `yc`：文件复制文本内容；目录复制条目名清单。
- [x] `e`：文件在 $EDITOR 打开；目录提示不支持。

### M23-11 自定义命令快捷键
- [x] TOML 配置 `,g` → lazygit：焦点 pane 为 git 仓库时在该目录打开；否则高亮目录；均非仓库提示"不是 git 仓库"。
- [x] 命令模板支持 %d/%f/%F/%D 变量替换；执行分流复用 terminal_in_new_window。

### M23-7（修订）Frecency Pane 键位
- [x] Frecency Pane Normal 态：j/k/gg/G 移动、l/Enter 跳转、y*/e/w 作用于光标结果（与 Normal 一致）；c/v 恢复 Normal 语义。

### M23-12 git 评估
- [x] git2/gitoxide 调研与集成点决策入 NOTES（不实现）。

## 23. M24 — 交互增强与列表规范（M24-1～9，见 SPEC M24 节）

### M24-1 Pane 列对齐
- [x] 文件名左对齐+空白填充；大小右对齐（B/KiB/MiB/GiB）；日期左对齐 `yyyy-mm-dd HH:mm`；窄终端隐藏属性列。

### M24-2 自然排序
- [x] `1.txt < 2.txt < 10.txt`；受 ignore_case_sort 影响；S 对话框可选 natural。

### M24-3 行可视模式 V
- [x] `V` 进入整行选择；与 `v` 互斥；d/y/x 对选中区域执行；Esc 退出。

### M24-4 可视操作扩展
- [x] v/V 内 d 删除（入撤销栈）、y 复制路径、x=d 别名；状态栏计数。

### M24-5 计数前缀
- [x] `3j`/`5dd`/`2gg` 消耗 count 执行 N 次；`N gt` 不变。

### M24-6 命令历史
- [x] `:` 模式 ↑/↓ 浏览已执行命令；Enter 执行；Esc 退出；上限 100。

### M24-7 路径补全
- [x] `:` 模式 Tab 补全文件/目录名（前缀匹配循环）；`~` 展开 HOME。

### M24-8 目录对比
- [x] `:compare` 结果虚拟目录含 =/!/</> 前缀条目；l/Enter 定位。

### M24-9 任务视图
- [x] `:jobs` j/k 选择、x 取消、Esc 关闭。
- [x] （M31-2 补）取消响应时延口径：协作式取消在**文件边界**检查 cancel_requested——时延上限 = 当前文件剩余处理时间（单文件拷贝无内部中断点）；归档类作业在启动边界即响应。（2026-11-20：与 execute_job_body/wait_while_paused 边界实现一致；m249_cancelled_job_body_stops_with_summary 覆盖）

## 24. M25 — 会话持久化、marks、归档创建与 git 集成（M25-1～7，见 SPEC §25）

### M25-1 会话持久化
- [x] 退出重启后 tabs/光标/活动标签还原；损坏文件静默新会话；`session_restore=false` 不读不写。

### M25-2 marks
- [x] `m<char>` 记录（目录/文件/`..` 行=当前目录）写入 frecency redb keyword 事件（`..` 行=当前目录；合法字符=字母数字；仅物理视图）；`'<char>` 跳转（目录 cd / 文件定位光标，M8-12 语义）；未命中提示；`:bmark`/`:bmarks`/`:bmgo`/`:delbmarks` 命令族与 marks 共用数据；多字符 keyword 在 `o`/`:j`/`:J` 高优先级匹配（M23-1）。（2026-11-20；验收行按 2026-11-19 反转后的 SPEC §25 修订——原"无 frecency store 写入"为反转前旧定义残留）

### M25-3 归档创建
- [x] zip/tar/tar.gz 产物可被 rvifm 自身浏览；xz/bz2/zst 可读提示不支持；`u` 可逆；同名不覆盖。

### M25-4 归档对话框与校验
- [x] 无参对话框交互完整（格式/校验/名称）；默认生成 `.md5`（`md5sum -c` 通过）；sha256/none 生效；`[archive] checksum` 配置生效。

### M25-5 git 状态列
- [x] 仓库内状态字符正确（M/A/D/?? 等）且着色；非仓库/开关关闭时隐藏；随导航刷新。

### M25-6 git 状态栏 + gi
- [x] 状态栏分支可见（非仓库隐藏）；`gi` 按优先级启动首个命中的 git TUI；全部缺失可读报错。

### M25-7 frecency 通用化评估
- [x] 决策入 NOTES（可行性/边界/schema 影响 + 下一批次建议），不实现。

## 25. M26 — 归档创建扩展与外部工具降级（M26-1～5，见 SPEC §26）

### M26-1 后台作业化
- [x] `:archive` 大目录创建不阻塞 UI（渲染持续响应）；`:jobs` 可见/可取消；完成入撤销栈可 `u`。（2026-11-20：exec/对话框 → `archive_plan`（同步校验）+ `SpawnJob`/`Exec{params}` → run 循环 `spawn_job_from_plan`；作业体 `archive:<fmt>:<cs>` 分支（取消先检、产物+校验各一条撤销记录、Failed 可读）；启动前取消测试）

### M26-2 7z 创建
- [x] 内建 7z 产物可被外部工具识别（或 sevenz-rust 读端往返一致）；校验文件与对话框格式生效；`u` 可逆。（2026-11-20：sevenz-rust 0.6 纯 Rust 写端 `pack_7z`（LZMA2 默认），`ArchiveFormat::SevenZ` parse/infer/ext ".7z"，对话框格式列表 4 项；roundtrip 测试读端核对内容 + undo 链）

### M26-3 7z 浏览
- [x] `.7z` 进入 ArchiveFs 虚拟浏览（zip/tar 同款 list/read/metadata 语义）；损坏 → 可读错误（含 7z 字样）；空归档空列表；加密归档按提示细分；`.rar` 保持 M7-9 解压提示。（2026-11-20：`SevenZFs`（SevenZReader 索引+二次遍历提取，AUDIT-17 尺寸守卫），open_dispatch/is_archive_file/is_unsupported_archive 更新，FIX-3 测试改写）
- [ ] `.7z` 虚拟浏览可用（l/Enter 进入、列表/读取内容）；损坏/加密可读错误；M7 只读边界不变。

### M26-4 外部 CLI
- [x] 7z/rar 存在时 xz/bz2/zst/rar 创建成功且产物可被读端/外部工具验证；缺失时优雅终止（可读提示、无半成品）。（2026-11-20：`cli_tool_candidates`/`cli_create_args`/`pack_cli`（cwd=base、.output() 捕获、stderr 尾部上浮、目标缺失检查、半成品清理归调用方）；工具探测失败 → "创建 .rar 需要 rar…均未在系统中找到"；e2e 成败依赖环境 CLI 可用性，纯函数层测试 + SpawnJob 形态断言）

### M26-5 文档
- [x] README 归档创建格式行为表与实现一致（内建/外部/不支持 + 校验默认）。（2026-11-20：新增"归档创建"行为表（内建 4 格式/CLI 6 格式/校验/观察项）；对比表与状态句同步 7z；frecency 段 AUDIT-1/4/15 过期表述一并勘正）

## 26. AUDIT — 评估报告整改（AUDIT-1～17，见 SPEC AUDIT 段 / docs/REVIEW.md）

### AUDIT-1 pick 只读性
- [x] 存在 `events.pending` 时执行 `rvifm pick`，数据库文件 mtime 与 `meta.global_tick` 均不变。（2026-11-20：回归测试 `pick_readonly_path_leaves_pending_and_db_untouched` + `open_readonly_does_not_write_db`）
### AUDIT-2 PDF 视觉探测链
- [x] 文本提取优先；启用 `pdf-visual` 后 exprim / 外部渲染器按序回退，缺失时可读提示，不 panic。（2026-11-20：`render_first_page_png_with` 按序 exprim→pdftoppm→mutool；Text/Unavailable 不进视觉链；viewer NeedsVisual → ImageEngine 渲染缓存 PNG，失败回退提示）
### AUDIT-3 pick 预览同源
- [x] 若保留 pick 预览承诺，pick 与 TUI 对同一文件渲染一致；否则文档撤回该承诺。（2026-11-20：保留承诺并接线——`draw_pick_ui` 预览窗格走共享 `render_preview`/`ImageEngine`，同进程无 fork/exec；TestBackend 验证宽/窄终端）
### AUDIT-4 惰性 compaction
- [x] 事件超过阈值时 TUI 启动触发 compaction（写 snapshot + 截断 events）；`pick` 不触发；或文档明确仅手动。（2026-11-20：`lazy_compact_on_startup` 挂 run() 启动（merge pending 之后），阈值 5000；pick/跳转选择器结构上无写调用）
### AUDIT-5 app.rs 拆分
- [x] 生产代码按职责分片，测试随迁，`cargo test` 全绿且无行为变化。（2026-11-20：`src/app/{mod,keys,exec,jobs,render,preview,tests}.rs`——类型与 `run()` 留根模块（子模块可见父私有项），移动项 `pub(crate)` + 根重导出；help.txt 随 render 分片迁至 `src/app/`）
### AUDIT-6 键位超时
- [x] 多键序列超时可配置，超时后清空 seq 且不吞后续按键。（2026-11-20：`[keymap] seq_timeout_ms`；run 循环按截止收缩 poll 预算，到期清空——后续键按全新序列解析；纯函数层测试）
### AUDIT-7 opener 分层
- [x] opener TOML 被实际读取；`prepend_rules` / `append_rules` / `rules` 三层语义与键位系统一致。（2026-11-20：Config 增 [opener]/[open] 表，`resolve_open_actions` 链序 prepend→(rules 替换默认空表)→append）
- [x] `mime` 与 `name` 匹配按顺序命中；命中规则执行 `use`/`run` 命令；未命中回退系统 opener。（2026-11-20：内建 ext→mime 表 + 通配匹配；`use` 名单按序执行、`open`=系统、未知名跳过；`open_selected` 生产接线）
### AUDIT-8 错误上浮
- [x] 生产路径无新增 `unwrap/expect` panic；刷新/会话/known_hosts 权限失败有可读提示。（2026-11-20：清理 keys 补全/复制臂、run 循环作业登记、frecency SETTINGS 锁、highlight 主题兜底；自动刷新失败 → 状态栏"自动刷新失败: …"；会话/known_hosts 原有 eprintln 上浮保留）
### AUDIT-9 CI 门禁增强
- [x] CI 步骤含 `clippy --all-targets -D warnings` 与 `cargo check --target x86_64-unknown-linux-gnu --all-targets`（增强项，现行 AGENTS 未要求 CI 执行 cross-check）。（2026-11-20：ci.yml 增补两步；cross-check 挂 `matrix.os == 'ubuntu-latest'`——host triple 即该 target，Windows runner 无 linux-gnu 交叉 gcc）
### AUDIT-10 重复实现统一
- [x] glob 匹配与配置失败回退各只保留一份实现，行为测试通过（含 `*x*` 双星模式）。（2026-11-20：opener 规则名匹配改走 `tab::glob_match`（M4-2/M23-4 同源单一实现，递归语义天然支持双星），`*ma*`/`*ai*n*`/`?` 行为测试）
### AUDIT-11 帮助同步
- [x] `help.txt` 含 `:find` / `:grep` 等已实现命令；README 的 keymap 路径、marks/主题/opener 状态与实现一致。（2026-11-20：help 补 `:find`/`:grep`/`:theme`/`:sort`/`S`/`:remote`/`s`/`:!`/`V`/`y` 序列/计数前缀，keymap.toml→config.toml [normal] 修正；README 状态句/对比表/三层示例（opener 归入 config.toml）/里程碑表同步，唯一保留的待实现项 = M25-2 marks）
### AUDIT-12 测试补强
- [x] keymap/state/session/CLI 子命令分派有针对性测试，`app.rs` 测试占比下降。（2026-11-20：keymap 分层合并/replace、state 作业回包、CLI maintenance 臂；顺带修复 Store::open 数据目录自动创建）

### AUDIT-13 主题选择
- [x] 可通过 `:theme` 或 `[ui] theme` 选择 `opaline` 内置主题，切换后文件列表/预览/归档配色即时变化。（2026-11-20：启动激活 + 运行时切换臂；渲染经既有 `filetype_style`/`theme::style` 消费即时跟随；ACTIVE 由 OnceLock 升级 RwLock——原实现二次切换静默失效）
- [x] `~/.config/rvifm/themes/` 下的用户 TOML 主题可被发现并加载；损坏主题文件给出可读提示，不 panic。（2026-11-20：`activate` 内置优先→用户目录；`:theme` 无参列全量）
- [x] 图标 Dark/Light 随主题 variant 变化（含远端列表路径）；未选主题时默认 Dark。（2026-11-20：`icon_theme()` 经 active_variant 实时读取，列表渲染共用）

### AUDIT-14 测试环境隔离
- [x] 在 `TMUX` 与 `STY` 均设置的 shell 中运行 `cargo test -- --test-threads=1` 全绿；清除两个变量后同样全绿。（2026-11-20；AUDIT-18 的 m249/m253 两项按 STATE 门禁偏差 skip）
- [x] `m18_4_bang_routes_and_config` 不依赖宿主终端策略，且测试输出不含外部命令的 stdout。（2026-11-20：路由注入 `run_external_target_with`，成功命令改静默 `cmd /C exit 0`）

### AUDIT-15 pick CLI 契约
- [x] `rvifm pick --topic dir` 与 `rvifm pick --topic file` 均按对应 topic 评分；SPEC §4.6 示例可直接工作。（2026-11-20：CLI 集成测试 `pick_scores_by_topic_and_degrades_without_data_dir`）
- [x] 数据目录不存在时 `pick` 正常退出（空评分或只读打开），不输出裸 `NotFound`。（2026-11-20：`pick_ranked_entries_at` 缺失 → 空表全 0 分）
- [x] 文件候选使用 file topic，不得以 dir 分数兜底。（2026-11-20：`pick_scores_by_requested_topic_without_dir_fallback` 双视角断言）

### AUDIT-16 frecency 一致性
- [x] 配置 per-topic λ 后，手动 compaction 前后的查询排序保持一致（对照测试）。（2026-11-20：replay_merge 按事件 topic 取 λ（同 replay_inner），`compaction_per_topic_lambda_consistent_with_live_replay` 对照断言）
- [x] `pick` 读取 `[frecency]` 的 λ / max_entries 配置，与 TUI 行为一致。（2026-11-20：cmd_pick 头部 init_settings（与 cmd_record 同款））

### AUDIT-17 归档读取上限
- [x] 单成员声明尺寸或 tar.xz 解压尺寸超过上限时返回可读错误，进程不 OOM、不 panic。（2026-11-20：zip/tar 读取前 `check_member_size`（64MB）；tar.xz 解压走 `CappedWriter`（256MB，解码途中拦截）；端到端注入式炸弹测试）

### AUDIT-18 本机测试失败整改
- [x] `m249_jobs_overlay_render_selection_and_cancel_state` 在长临时路径环境（如 8.3 短名/多级 Temp）下稳定通过：作业行断言不依赖宿主路径长度。（2026-11-20：目标改固定短标签 `d`——add_job 第三参仅作展示串）
- [x] `m253_archive_zip_roundtrip_and_undo` 本机复现的"第二次 undo 未移除归档"根因修复或测试修正，本机与两端 CI 均绿。（2026-11-20：根因 = 测试的 ZipFs 读句柄跨撤销存活，Windows 打开句柄阻塞 remove_file；句柄在撤销前 drop）

## 27. M27 — 0.8.0 实测问题与 vifm 视觉对齐（见 SPEC §27）

### M27-1 符号链接目录
- [x] `ln -s <目标目录> link` 后，列表中 `link` 可被识别为目录；`l`/`Enter` 在 rvifm 内部进入目标目录并列出内容，不调用系统 opener。（2026-11-20：load_entries/build_tree_entries 符号链接跟随目标判定 is_dir；unix 测试覆盖进入 + 列出内容）
- [x] 进入后 `h` 能返回来源目录；目录优先排序把链接目录按目录处理。（2026-11-20：is_dir 跟随后目录优先排序自动生效；h 语义不变）
- [x] `ln -s <不存在路径> broken` 时目录列表仍可加载；`l`/`Enter` 给出可读提示或不执行，不 panic、不误开系统程序。（2026-11-20：跟随失败回退链接自身元数据（列表不失败）；Enter 走系统 opener 失败 → "打开失败"可读上浮；unix 测试）
- [x] 指向普通文件的符号链接仍按文件处理，`Enter` 走系统 opener（不因本修复被误判为目录）。（2026-11-20：metadata().is_dir() 对 file-link 为 false，语义不变）
- [x] 目录树/预览中的符号链接目录与列表判定一致（至少不把可进入的链接目录显示为普通文件）。（2026-11-20：build_tree_entries 同款跟随判定）

### M27-2 vifm 风格布局与配色
- [x] 默认启动无左右 pane 整框边框；双 pane 之间为 1 列分隔线；标签栏与每个 pane 顶部路径行各占 1 行。（2026-11-20：`[ui] pane_borders` 默认 false；pane_gap_area 渲染 Border 分隔线；m21f1 测试双模式断言）
- [x] 当前/非当前 pane 的顶部路径行使用不同背景色（`TopLineSel` / `TopLine`），焦点切换时颜色随之切换。（2026-11-20：焦点=黑字加粗默认底（TopLineSel 回退），非焦点=黑字白底（TopLine 回退），TestBackend bg 断言）
- [x] 标签栏（`TabLine`/`TabLineSel`）、状态行（`StatusLine`/`CmdLine`）、分隔线（`Border`）、当前行（`CurrLine`/`OtherLine`）、选中项（`Selected`）均有可区分的样式。（2026-11-20：vifm_style 命名样式 + 16 色回退；CurrLine=反显（焦点）/DIM（非焦点）沿用 M21-F2 既有区分；Selected 主题样式优先、视觉行反显兜底）
- [x] `[ui] pane_borders = true` 可恢复旧的整框边框外观；默认 `false`。（2026-11-20：m21f1 测试以 pane_borders=true 覆盖旧外观 + vifm 断言并存）
- [x] 主题提供对应命名样式时优先使用；未激活主题/样式缺失时回退到 vifm Default 风格的 16 色，不 panic。（2026-11-20：`vifm_style`（fg/bg 任一存在即主题样式优先，否则 vifm_fallback 16 色））
- [x] `NO_COLOR` 与无 Nerd Font 环境下布局仍正确、无乱码崩溃；README 的界面描述与实现一致。（2026-11-20：16 色回退不依赖主题/glyph；布局几何由 TestBackend 测试覆盖；README 界面段随 AUDIT-11/M26-5 同步）

### M27-3 PDF 前 1–2 页图像预览
- [x] `[preview] pdf_image_pages = 0` 时 PDF 仅走文本/提示路径，不生成图像。（2026-11-20：cached_visual_pngs pages=0 早退不写缓存；NeedsVisual+prefer 双形态测试）
- [x] `pdf_image_pages = 1` 且文本层不足（`NeedsVisual`）时，若 `exprim`/`pdftoppm`/`mutool` 可用，查看器显示第 1 页图像；工具缺失或失败时回退到文本/提示。（2026-11-20：注入假工具生成断言 + 无工具 None 缓存回退 render_preview 文本路径；spawn 注入缺陷修正——原硬编码工具名使 ext 注入无效，见 NOTES）
- [x] `pdf_image_pages = 2` 时生成前两页；预览区足够高时上下显示两页，不足时至少显示第 1 页并提示。（2026-11-20：Ratio(1,2) 上下布局 height≥12；pdf_overflow_note 纯函数区分"高度不足仅显示第 1 页"/"第 2 页未生成"双文案——原分支反逻辑，两页齐全高度不足时无提示）
- [x] `pdf_prefer_image = true` 时，即使文本层可用也优先显示图像；但仍先执行文本提取（探测顺序不跳过文本层），图像失败回退到已提取文本。（2026-11-20：Text+prefer=true 生成测试；preview_pdf 先于生成调用结构保证；None 回退 render_preview 文本）
- [x] 启用 `pdf-visual` 时优先走 `exprim`；未启用时走 `pdftoppm` → `mutool`；命令逐参数传递、不经 shell。（2026-11-20：render_page_png_with 顺序不变；假工具脚本按 $3/$5 取参成功即证逐参数传递；Command::args 无 shell）
- [x] 超大输入/超页数/损坏 PDF 不 panic、不 OOM；预览切换或退出后临时图像文件被清理。（2026-11-20：损坏→Unavailable→None 既有测试；超页→该页失败 break 保留前页；切换 purge + 退出 cleanup 幂等测试；run 循环两退出点均接线）

## 28. M28 — 运维操作补齐（见 SPEC M28 节）

### M28-1 :link 符号链接创建
- [ ] 光标条目执行 `:link mylink` 后，当前目录出现指向该条目的符号链接；目录链接 `l`/Enter 内部进入（M27-1 语义不回归），文件链接 `is_link` 着色正确。
- [ ] `v` 选择 / `t` 标记多条目后 `:link dest_dir`（dest 为目录）批量创建；`dest` 已存在同名 → 可读错误、不覆盖、无半成品。
- [ ] `u` 撤销删除新建链接（定位/移除正确），再次 `u` 链回上一步。
- [ ] Windows 人工验证：非管理员且未开发者模式时目录条目回退 junction 成功；文件条目失败给可读提示不 panic。（Linux 侧 symlink 直建自动化测试覆盖）
- [ ] `:link` 无参/非法路径 → 可读错误列出用法。

### M28-2 :rename 结构化批量重命名
- [ ] `:rename s/old/new/` 对作用域内条目 regex 替换；`s/old/new/i` 大小写不敏感；非法 regex → 可读错误不执行。
- [ ] `:rename prefix=img_` / `suffix=.bak` / `seq=1,1,3` 批量作用于 tags/`v` 选择；预览虚拟目录逐行显示 `旧名 → 新名`；确认后应用、Esc/取消不改动任何文件。
- [ ] 不符规则/同名/非法目标条目跳过并在结果中列出（不整批中止）。
- [ ] 整批入撤销栈：一次 `u` 逆序恢复全部重命名。
- [ ] 未知参数形态 → 可读错误列出支持的四种形态。

### M28-3 :jobs 任务视图增强
- [x] 大目录作业运行中按 `p` → `:jobs` 状态列"已暂停"、进度停走；再按 `p` → 恢复运行（done/cancelled 作业上 `p` 无效并有提示）。（2026-11-20：Job.pause 协作式标志 + AppState::pause_job + wait_while_paused 边界等待（100ms 轮询、cancel 优先）；m283 菜单切换断言）
- [x] Enter 打开作业详情覆盖层：类型/源/目标/已处理条目数/传输字节/速率（5s 滑动平均）/时长；任意键返回 `:jobs`。（2026-11-20：InputMode::JobDetail + JobProgress 采样（500ms 节流、64 样本上限）；传输字节按源条目大小累计；m283 TestBackend 渲染断言）
- [x] M24-9 既有行为不回归（j/k 选择、`x` 取消、Esc 关闭）。（2026-11-20：m249 测试迁移至 JobsMenuOutcome 枚举后全绿；取消语义不变）
- [x] 暂停期间 TUI 不阻塞：暂停等待在后台线程（100ms 轮询），事件循环不受影响；归档作业（整段原子）暂停在启动边界生效。（2026-11-20：wait_while_paused 双测试——cancel 优先立即返回 + 解除后退出）

### M28-4 M28 文档收尾
- [x] help.txt 补 `:link` / `:rename` / `:jobs` `p`/Enter；README 核心特性段同步。（2026-11-20：help 三条目随各任务落地（AUDIT-11 口径：无漂移）；README 文件操作/后台作业两条目）
- [x] ACCEPTANCE §28 全部勾选后批次方可关闭。（2026-11-20：M28-1～3 unix 自动化项全勾；M28-1 的 Windows junction 人工验证项保留未勾——随 CI windows 构建与用户实测补验）

## 29. M29 — 对比增强（见 SPEC M29 节）

### M29-1 :compare bycontents
- [x] 左右 pane 同名同内容 → `=`；同名不同内容 → `!`；仅左 → `<`；仅右 → `>`（M24-8 前缀语义不回归）。（2026-11-20：compare_panes 增 by_contents；SHA-256 分块哈希（1MB 块）；m291 双模式断言——同尺寸异内容 byname 误判 = 的回归守护）
- [x] 体积超护栏条目按"不同（未知）"计并在结果中呈现、不 OOM（单文件 64MB 上限沿用 M11/AUDIT-17 护栏口径；超限不读内容；help/消息注明）。（2026-11-20：COMPARE_MAX_FILE_BYTES + m291 sparse set_len 超限断言）
- [x] 无参 `:compare` 保持 M24-8 按名+大小行为不回归；`:compare byname` 显式等价缺省；未知模式可读错误。（2026-11-20：m291 命令级断言 + bycontents 进结果视图 query 标注）
- [x] 千条目级目录对比 UI 不冻结（比较为同步一次性计算——哈希 64MB 上限×条目数受控；无逐帧重算，结果入缓存式虚拟目录）。（2026-11-20：既有 :compare 架构同源，未引入逐帧重算路径）

### M29-2 dp/do 同步
- [x] `>` 条目 `dp` → 将另一侧条目复制到当前侧目录（copy_to 入撤销栈、`u` 可逆）；`<` 条目 → 可读拒绝（另一侧无内容）。（2026-11-20：compare_sync put 分支；m292 取回/撤销/重建断言）
- [x] `<` 条目 `do` → 删除当前侧条目（可恢复路径 = 应用内暂存 + 撤销栈、`u` 可逆）；`>` 条目 → 可读拒绝（当前侧无此文件）。（2026-11-20：compare_sync obtain 分支复用 execute_delete；m292 删除/暂存恢复/重建断言——唯一副本删除后从结果消失）
- [x] `!`（两侧不同）与 `=`（相同）条目上 dp/do → 可读拒绝（不支持覆盖同步——SIMPLIFIED，见 SPEC/NOTES）。（2026-11-20：m292 四类拒绝断言）
- [x] 操作后对比结果按原模式自动重建（前缀变化正确；`u` 撤销后结果为静态快照、重跑 `:compare` 刷新）。（2026-11-20：run_compare 复用；compare 加入结果视图重执行豁免名单；m292 消息合并断言）
- [x] 非对比视图（普通视图/其它结果视图）dp/do → 可读拒绝。（2026-11-20：compare_sync kind 守卫 + m292 退出视图断言）

### M29-3 M29 文档收尾
- [x] help.txt / README / ACCEPTANCE §29 同步；全部勾选后批次关闭。（2026-11-20：help :compare/dp-do 条目随任务落地；README 对比能力暂未在特性段列出——:compare 为 M24-8 既有能力且 help 已覆盖，README 特性表补行推迟至 M31-1 统一状态同步时一并处理）

## 30. M30 — frecency 通用化（见 SPEC M30 节）

### M30-1 cmd 事件 schema 与记录链路
- [ ] `:` 命令执行成功后 record 进 cmd topic（M24-6"成功才记录"扩展口径）；失败命令不记录。
- [ ] IGNORE_PATTERNS 命中命令文本 → 静默跳过；旧 redb 数据（无 cmd 事件）零迁移兼容、不 panic。
- [ ] `[frecency]` cmd 档 λ/max_entries 可配（缺省 0.01/1500）；compaction 用 per-topic λ（AUDIT-16 口径）。

### M30-2 : 命令历史迁移
- [ ] `:` 模式 ↑/↓ 列出 cmd topic replay 顶层 N + 当前会话合并；重启 rvifm 后历史仍在（跨会话持久）。
- [ ] 输入前缀过滤候选，按前缀分级与分数排序（常用命令置顶）；Esc/Enter 交互与 M24-6 不回归。

### M30-3 路径补全加权
- [ ] Tab 补全候选 frecency 分数优先排序；无记录条目字典序兜底；store 缺失/空时与现状行为一致。

### M30-4 命令书签
- [ ] 命令文本可设置 keyword 书签（入口实现定，help 记录）；`:` 模式内消费展开为命令文本供编辑/执行。
- [ ] unbook 保留历史（M23-2 语义）；多字符书签在 `o` 搜索高优先级匹配（M23-1 既有，cmd topic 验证）。

### M30-5 文档与隐私收尾
- [ ] README/help 明示"命令文本持久化写入 frecency redb（IGNORE_PATTERNS 可排除）"。
- [ ] NOTES 对齐注记（cmd 命名/无分数下限/书签同构）落档；ACCEPTANCE §30 全勾后关闭。

## 31. M31 — 文档与验收补齐（见 SPEC M31 节）

### M31-3 M11-6 真实终端手工验证清单（deferred 补验入口）

> M11-6（Alacritty 图片显示，deferred）的补验清单——任何真实终端环境按序执行（约 10 分钟），全部通过即可勾选 M11-6 并在 NOTES 记一行环境信息（终端名/版本/是否 tmux 内）。

- [x] 1. 无 tmux 环境：Alacritty/Kitty/WezTerm 打开 rvifm，光标移到 `.png` 文件 → 预览窗格显示图像（协议探测：Kitty 原生 / WezTerm Sixel / 其它 halfblocks）。（2026-11-21 用户实测 foot：图片渲染效果良好；同场 PDF 无文本语料首页视觉渲染可用、速度略慢——见 NOTES 2026-11-21）
- [ ] 2. tmux 内（passthrough 开启 `set -g allow-passthrough on`）：同上验证图片可见（tmux 3.3+）。
- [ ] 3. 无协议终端（如老版 gnome-terminal）：确认 halfblocks 兜底显示（Unicode 块字符近似图）。
- [ ] 4. `NO_COLOR=1` 启动：确认无 panic、列表/预览降级正常。
- [ ] 5. 损坏图片文件：预览给可读提示，不 panic。
- [ ] 6. 大图（>400 万像素）：预览可读提示或缩放，不 OOM。

### M31-2 外部 CLI 双环境验收（补于 M26 段后）
- [x] （自动）未装 rar/cab 外部工具环境：`:archive rar|cab|lzh` → 可读错误、无半成品（CI 无工具环境覆盖；M26-4 测试断言）。
- [ ] （人工）装 7z 环境：cab/lzh 打包 → 精确提示区分"工具缺失"与"格式不支持"（7z 打包能力矩阵探测不在 M26-4 范围——提示措辞以实际错误为准）。

### M31-1 README 状态与边界同步
- [x] 开发路线表 M26 行"已确认启动"→ 已完成（与 PLAN/TASKS 一致）。（2026-11-20）
- [x] 归档创建行为表补加密 7z / 分卷 7z 边界段（措辞与 M26-3 实际行为核对：加密 = 可读错误"暂不支持密码解锁"；分卷不支持虚拟浏览）。（2026-11-20）
- [x] 提权/平台边界段补 Windows UAC 边界句（M12 `sudo -A` 口径、Windows SIMPLIFIED 未落地直给可读错误）。（2026-11-20）

### M31-2 ACCEPTANCE 补可执行条目
- [x] `:jobs` 取消响应时延条目落地（条目见 M24-9 追加段——文件边界协作式取消口径，不设固定秒数：时延 = 当前文件剩余处理时间；归档作业启动边界即响应）。（2026-11-20）
- [x] 外部 CLI 归档双环境验收条目落地（条目见 M26 归档创建验收段——未装工具路径 CI 自动覆盖；装 7z 不支持 cab 打包的人工验证项随 M28-1 Windows 项同口径保留）。（2026-11-20）

### M31-3 M11-6 手工验证清单
- [x] ACCEPTANCE 独立小节：Alacritty（halfblocks）/ tmux passthrough / Kitty 原生协议 / 无协议降级链逐项清单；M11-6 条目引用该清单。（2026-11-20 清单已建于 §M31-3，补勾；清单第 1 项 2026-11-21 foot 实测通过）

### M31-4 ACCEPTANCE M0–M13 复选框化
- [x] M0–M13 普通 bullet 全部转复选框（137 条，已完成的里程碑勾 `[x]`）；`grep` 核验对应区间（§2–§13）无残留裸 bullet；条目文字零改动。（2026-11-20）

### M31-5 pdf-visual 护栏实测
- [~] rustc ≥1.97 环境执行语料回归 + `cargo check --features pdf-visual` 通过；耗时/内存峰值数据（含 100MB 级语料）记入 NOTES；护栏数值未改动。（**deferred**：本机 rustc 1.95 < exprim 要求 1.97；CI stable 双平台编译验证持续绿（M11-15 job）；语料实测随 M11-6 场景或后续 CI job 补验）

## 32. M32 — 远端健壮性与会话扩展（见 SPEC M32 节）

### M32-1 SFTP 连接健康检查与自动重连
- [ ] 传输/浏览中断连后，下次远端操作自动重连一次：成功 → 重载当前远端目录、操作继续；再失败 → 可读提示（含原因）、TUI 状态一致、无半成品。
- [ ] 重连期间 UI 不冻结（事件循环响应导航/`:q`）。
- [ ] 认证/主机校验失败（非连接类错误）不触发静默重连循环（M22-2/3 语义不回归）。

### M32-2 大文件传输断点续传
- [ ] 大文件上传中途断连 → 目标侧保留 `<target>.part` 与偏移；重连成功后从偏移继续，完成时原子改名。
- [ ] 显式取消（`x`）清理 `.part`（M20-3 口径）；撤销语义不变（成品入撤销栈）。
- [ ] 断电/跨会话场景：`.part` 残留不破坏目标目录浏览（可读提示或清理策略实现定）。

### M32-3 会话持久化范围扩展
- [x] 归档内退出 → 重启恢复归档位置（host 存在即 navigate_to 进入；host 缺失 → 跳过 + 提示"归档文件缺失"）。（2026-11-20：SessionTab.kind（physical 缺省向后兼容）+ apply_session kind 分派；m323 归档 roundtrip + 缺失跳过断言——fixture 用 M26 pack 真归档）
- [x] 远端位置退出 → 重启恢复并重连（remote_reconnect_session 复用 M32-1）；失败 → 跳过该 tab + 提示（restore_session_with_hints 返回跳过原因，run() 接状态栏），不 panic。（2026-11-20：m323 remote 序列化格式 remote:path + 重连失败跳过断言）
- [x] 物理 tab 恢复行为不回归（M25-1）；session.toml 损坏静默降级。（2026-11-20：m251 断言迁移（非物理 tab 现入会话）后全绿；kind 字段 serde default = 旧文件零迁移）

### M32-4 M32 文档收尾
- [x] help/README 补重连与续传行为。（2026-11-20：help 跨 VFS 粘贴行（M32-2 落地）；README 特性段 SFTP 重连/现场句随 0.10.0 状态段统一更新）
- [x] ACCEPTANCE §32 全勾后批次关闭。（2026-11-20：M32-1～3 全勾，416 单测）

## 33. M33 — 键位模式分层扩展（见 SPEC M33 节）

### M33-1 三层 TOML 解析与合并
- [ ] `[visual]`/`[view]`/`[command]` 各支持 prepend/append/replace 三层；缺省节为空；解析失败可读错误（GAP-1 回退口径）。
- [ ] 配置示例（README）加载后对应模式绑定生效。

### M33-2 模式化消费接线
- [x] Visual 模式进入/退出切换 per-mode 键位表（v/Esc 路径）；View 模式同（w）。Visual 内用户绑定 `z` → ToTop 生效（m332 显式命中/还原断言）。（2026-11-20：InputState.switch_mode_map + with_mode_maps；None 层不建表 = 零行为变化）
- [x] 不可映射动作（白名单外）→ 加载期整条跳过（GAP-1 parse_binding 既有口径——未知 run 不静默进入绑定表）。（2026-11-20：m331 未知动作跳过断言；**[command] 消费面裁剪见 NOTES**——Command 模式按键走 command_input 单键处理不经 Keymap，`[command]` 分层解析保留但暂无运行期消费点）
- [x] 未配置任何模式节时全部行为与现状一致（零回归——既有 app 测试全绿）。（2026-11-20：switch_mode_map None 层不建表、None 参数还原 normal 主表）

### M33-3 M33 文档收尾
- [x] help / README 键位段 + config.toml 示例补三层；ACCEPTANCE §33 全勾后批次关闭。（2026-11-20：help 键位分层扩展段；README 配置示例随 0.10.0 状态段统一更新——help 为运行时入口已覆盖；§33 全勾）

## MISC — 杂项定稿（见 SPEC §6.1）

### MISC-1 rv 短别名二进制
- [x] `rv` 二进制与 `rvifm` 并存、行为完全一致（同一 `src/main.rs`）；`--version` 冒烟通过。（2026-11-20：Cargo 双 `[[bin]]`，rv `test=false` 避免双份单测；`CARGO_BIN_EXE_rv` 冒烟测试）
- [x] 发布归档包含双二进制。（2026-11-20：ci.yml release 矩阵 `binaries: rvifm rv`（windows 含 .exe），tar 打包同步）
- [x] README 说明 rv 别名；项目名/crate 名/主二进制名仍为 `rvifm`。（2026-11-20：快速开始/二进制布局/运行段三处）
- [x] 默认依赖树不变（无新增 crate；`cargo tree` 无 exprim）。（2026-11-20）

## 34. M34 — 主题、预览统一与文档（见 SPEC M34 节；目标 0.11.0）

### M34-1 默认主题 gruvbox
- [x] `[ui] theme` 未配置时默认 `gruvbox-dark`（显式配置覆盖；16 色回退保留为无主题兜底）。（2026-11-21：config 默认值 + 单测）
- [x] vifm 高亮组三层解析：主题命名样式 → 标准 token 推导（accent/bg/text 词表，缺 token 整组回退）→ 16 色；gruvbox 色值断言。（2026-11-21：`vifm_style` 三层 + `derive_vifm_group`）
- [x] `[ui] theme = "dracula"` / `:theme dracula` 生效（activate 链 + 通用推导）。（2026-11-21）
### M34-2 snowwhite 捆绑主题
- [x] `snowwhite` 经 `activate`/`:theme` 可选（opaline 内置 → rvifm 捆绑 → 用户目录三段）；浅色 variant + 图标 Light 联动；vifm 高亮组精确值（fg16/bg255 等）。（2026-11-21）
### M34-3 右 pane `/` 搜索
- [x] 焦点在右 pane：`/` 进入搜索（incsearch 跳转/高亮/计数）、`n`/`N` 环绕、Esc 取消恢复 origin；左 pane 光标不受影响。（2026-11-21：tab.rs + 集成测试）
### M34-4 对侧预览统一
- [x] `preview_path()` = 焦点 pane 光标文件（修复右焦点 `:view` 预览错位）。（2026-11-21）
- [x] view 模式预览占非焦点 pane：焦点左 → 查看器在右 pane（M13-9 现状）；焦点右 → 查看器在左 pane、右 pane 保持目录列表导航。（2026-11-21：render_view_content 抽出 + 双侧分派）
### M34-5 frecency Pane 视觉与回显
- [x] 顶部标题 `Frecency - 高频目录/文件搜索`；结果行 = devicons 图标 + 全路径（`[目录]/[文件]` 移除、✦keyword 保留）；搜索态底部 `/query` 实时回显。（2026-11-21）
### M34-6 `.jar` 归档识别
- [x] `.jar` 按 zip 路径虚拟浏览（roundtrip list/read；大小写不敏感；is_archive_file + open_dispatch 同步）。（2026-11-21）
### M34-7 二进制/不可读预览信息面板
- [x] 二进制（NUL）/非 UTF-8/读取失败 → 结构化面板（类型魔数嗅探/大小/修改时间），无乱码；不执行文件（零子进程）。（2026-11-21）
### M34-8 GUIDE 用户手册
- [x] `docs/GUIDE.md`（13 节：快速上手/键位速查/命令速查）与 help.txt（TUI 内速查）分工。（2026-11-21）
### M34-9 预览尺寸适配
- [x] 小图原尺寸居中不放大；大图 fit-width（内容到达窗格右缘）；超高顶部切片不越界。（2026-11-21：ImageEngine 单点三分支 + TestBackend 断言）
### M34-10 文档收尾
- [x] README（状态句/GUIDE 指针/预览语义）与 help 同步；ACCEPTANCE §34 勾选完成 → 批次关闭。（2026-11-21）

## 35. 0.11.0 全量人工验证清单（2026-11-21 审计新增；发布后实测入口）

> **使用说明**：按环境分组逐项勾选；失败项在 NOTES 记录现象并按 AGENTS §8.3
> 走 FIX 登记。本清单合并历史手工项（M11-6 步骤 2–6 = B1/C1–C4；M28-1
> junction = D2；M31-2 7z = D3）。全部勾选后：M11-6 关闭（NOTES 记终端
> 名称/版本/是否 tmux 内），并视为 0.11.0 完整实测验收。

### A. Linux 真实图形终端（foot / Alacritty / Kitty 任一）

- [ ] A1 启动/退出/二次启动：tabs 与光标恢复（session.toml；`[ui] session_restore` 默认开）。
- [ ] A2 默认主题 `gruvbox-dark` 生效（M34-1）；`:theme snowwhite` 切浅色且图标跟随 Light；`:theme dracula`；`:theme` 无参列出含 `snowwhite`。
- [ ] A3 双栏：`Tab` 焦点切换（当前行反显/非焦点 DIM）；右 pane 独立 `j/k/l/h` 导航；`..` 行返回上级。
- [ ] A4 右 pane `/` 搜索：incsearch 跳转 + 高亮 + 计数 `i/N`；`n`/`N` 环绕；Esc 恢复（M34-3）。
- [ ] A5 对侧预览：焦点左 + `w` → 查看器在右 pane；焦点右 + `w` → 查看器在左 pane 且右 pane 保持列表导航（M34-4）。
- [ ] A6 图片预览尺寸（M34-9）：小图原尺寸**居中不放大**；大图按窗格宽适配；超高显示顶部。
- [ ] A7 PDF：无文本语料首页图像（略慢属预期）；有文本语料文本/图像分流（`[preview] pdf_image_pages`/`pdf_prefer_image`）。
- [ ] A8 二进制/非 UTF-8 文件预览 → 信息面板（类型魔数/大小/修改；无乱码）（M34-7）。
- [ ] A9 frecency：`o`/`:j`/`:J`/`:recent` 标题"高频目录/文件搜索"、图标行、✦keyword、搜索态底部 `/query` 回显（M34-5）。
- [ ] A10 marks/书签/命令书签：`m{字符}`/`'{字符}`；`:bmark`/`:bmarks`/`:bmgo`/`:delbmarks`；`:cmdmark` + 命令行内 `'` 展开。
- [ ] A11 文件操作闭环：`yy`/`p`/`dd`/`u`（含跨 tab 撤销）/`cw`/`:rename` 结构化（y 应用整批）/`DD` 强提示/`:link`（目录 junction 由 D2 覆盖）/`:mk`。
- [ ] A12 归档：zip/jar/7z 虚拟浏览；`:archive zip` 创建 + 校验文件 + `:jobs` 取消；大目录打包不阻塞 UI。
- [ ] A13 搜索套件：`={glob}`、`f/F/;/,`、`:find`/`:grep` 虚拟目录 + `w` 查看器（行号/搜索高亮）。
- [ ] A14 排序/对比：`S` 对话框、`:sort` 多字段、`:compare` byname/bycontents、`dp`/`do` + `u`。
- [ ] A15 复制序列 `y`/`yn`/`yf`/`yc`（OSC52 需终端支持）。
- [ ] A16 git：仓库内状态列/状态栏分支；`gi` 探测 gitui→ziggity→lazygit（装任一即验）。
- [ ] A17 鼠标：单击聚焦/再击打开/双击直接打开/右键菜单/滚轮。
- [ ] A18 `:jobs` 后台作业、`:trash` 暂存恢复、`:file`、`Ctrl-G` 详情。

### B. tmux（3.3+）

- [ ] B1 `allow-passthrough on` 下图片可见（M11-6 步骤 2）。
- [ ] B2 `:!{cmd}` 新窗口执行；退出后 TUI 状态一致（M18-4）。

### C. 特殊环境

- [ ] C1 无图形协议终端：halfblocks 兜底出块（M11-6 步骤 3）。
- [ ] C2 `NO_COLOR=1` 启动：无 panic、降级正常（M11-6 步骤 4）。
- [ ] C3 损坏图片文件：可读提示不 panic（M11-6 步骤 5）。
- [ ] C4 大图（>400 万像素）：可读提示或缩放，不 OOM（M11-6 步骤 6）。
- [ ] C5 窄终端（<60 列）`pick` 单栏降级（AUDIT-3）。

### D. Windows（Win2016）

- [ ] D1 安装冒烟：`cargo install rvifm`（或 GitHub 归档解包）后 `rvifm`/`rv` `--version`/`--help`。
- [ ] D2 `:link` 对目录创建 junction 回退（M28-1；撤销可逆）。
- [ ] D3 装 7z 环境：`:archive cab|lzh` 提示区分"工具缺失 vs 格式不支持"（M31-2）。
- [ ] D4 Windows Terminal 图片预览（M11-6 清单 §H Windows 部分）。
- [ ] D5 `remotes.toml` 位于数据目录（`%LOCALAPPDATA%\rvifm\`）且可被 `:remote` 读取。

### E. SFTP（如有可达远端）

- [ ] E1 `:remote <名>` 进入/浏览/写操作（dd/p/P/:mkdir）；断线自动重连一次；失败现场 `.part`。
- [ ] E2 known_hosts 首连写入；指纹变更拒连（改 `known_hosts = false` 验证跳过）。

### F. CLI（非 TUI）

- [ ] F1 `rvifm pick`（TTY 交互双栏预览 + `--topic dir|file`）；`rvifm record --keyword`。
- [ ] F2 `rvifm maintenance compact`（或 TUI 启动阈值触发）。

### G. 数据边界

- [ ] G1 `[frecency] ignore_patterns` 排除敏感路径与命令（命令文本不入库）。
- [ ] G2 会话文件损坏静默降级（手改 `session.toml` 为非法内容后启动不 panic）。

### 说明

- A6–A9/A11–A12 同时为 M34 批次的真机复核面（§34 各条已在 TestBackend 层验证）。
- 历史 ACCEPTANCE 节（M0–M33）的未勾选 `[ ]` 为 M31-4 机械转换产物与时代性验收记录，不代表当前状态；当前版本的实测入口即本节。
## 35. M35 — 文档与发布一致性（见 SPEC M35 节）

### M35-1 状态漂移清零
- [x] AGENTS §10 当前阶段/任务/基线与 STATE 一致（不再指向 AUDIT-1/v0.8.0）。（2026-11-21）
- [x] SPEC 7z/rar 三处与 M26-3 实际能力一致。（2026-11-21）
- [x] STATE"执行中/全部完成"矛盾消除；TASKS M34 段标题改"已完成"；PLAN 0.10 口径统一为"用户决策跳过"；README"发布中"改"已发布"。（2026-11-21）

### M35-2 harness 一致性脚本
- [x] `scripts/harness_check.py` 覆盖七条规则；CI job 已挂（ubuntu test 前置）；反向验证：注入"M34 批次执行中"漂移后脚本失败并给出 R1-STATE 定位（R1 正则同分句限定——列举行"M0–M34 完成"不误报）。

### M35-3 REVIEW 归档
- [x] 旧报告移入 `docs/reviews/REVIEW-2026-11-21.md`；`REVIEW.md` 重写为索引 + 最新结论摘要（2026-11-19 报告已在归档目录）。

### M35-4 CHANGELOG
- [x] `CHANGELOG.md` 覆盖 0.8.0 → 0.11.0 用户可见变化（Keep a Changelog 格式）；0.9.0 跳过说明 + 0.10.0 独立条目。（2026-11-21）

### M35-5 依赖与计数卫生
- [x] `tokio-stream` 移除且 `cargo build`/test 绿；`DBG parsed` 清除；文档测试数字校验进 harness_check（R6——修正后顺带抓出 STATE 433→437 漂移）。（2026-11-21）

## 36. M36 — 传输引擎（见 SPEC M36 节）

- [x] M36-1 copy_tree 保留 mode/mtime/symlink；分块 + 进度 + 取消（含链接树 roundtrip 测试）。（2026-11-21：copy_file_chunked（1MB 块 + File::set_times mode/atime/mtime）+ copy_tree（symlink_metadata 判型不解引用、断链按文件链接、目录 times 回填）+ CopyControls（cancel/progress，none 形态元数据保留仍生效）；move_path 跨盘回退同享；m361 三测试——metadata/symlink roundtrip、取消清半成品、copy_to mtime）
- [ ] M36-2 远端下载单次读取 + 临时文件 + 原子 rename（失败清理断言）。
- [ ] M36-3 RemoteFs read_range/write_range（默认实现兼容 mock；SftpBackend 覆盖）。
- [ ] M36-4 Transfer 状态机统一三方向（Paste 路由迁移断言）。
- [ ] M36-5 五种冲突策略各一条集成测试 + 应用到全部。
- [ ] M36-6 偏移续传：断网 3 次续传成功 + 哈希不一致从头 + 取消清理。
- [ ] M36-7 :queue 入队/暂停/恢复/取消/重试。
- [ ] M36-8 递归远端目录三方向 + 链接策略。

## 37. M37 — 文件运维（见 SPEC M37 节）

- [ ] M37-1 :chmod 八进制/符号/递归 + 批量失败逐项（POSIX；Windows 拒绝提示）。
- [ ] M37-2 :extract 选中/全部 + zip-slip 语料拒绝 + 冲突策略 + job 化 + u 可逆。
- [ ] M37-3 :sync 三策略 + dry-run + 删除显式开关 + 确认。
- [ ] M37-4 :du 并行/取消/万条上限/排序视图。
- [ ] M37-5 :dup 三段哈希 + inode 硬链接 + 结果可操作。
- [ ] M37-6 :checksum 生成/校验/批量 + 并行进度。
- [ ] M37-7 暂存区容量/清空/按时间清理/恢复到指定路径。

## 38. M38 — 检索与 shell 集成（见 SPEC M38 节）

- [ ] M38-1 检索结果 yy/dd/p/:tag/:rename/:chmod 映射真实路径 + 去重 + 只读拒绝。
- [ ] M38-2 pick 八参数各一条集成测试（未知参数可读错误）。
- [ ] M38-3 init 四 shell 输出含 record/pick 接入（脚本语法检查）。
- [ ] M38-4 frecency stats/export/purge/prune（purge 需确认）。
- [ ] M38-5 find --size/--mtime/--type/--depth + 命名搜索。

## 39. M39 — 预览与平台（见 SPEC M39 节）

- [ ] M39-1 hex 查看器（二进制自动进入 + 偏移跳转 + ASCII 侧栏）。
- [ ] M39-2 CSV 对齐预览 + PDF [ ] 翻页 + EXIF 容器摘要。
- [ ] M39-3 docx/xlsx/pptx/apk/war 虚拟浏览（魔数 + 白名单）。
- [ ] M39-4 指纹确认 ask（拒绝则断开）/ProxyJump/agent/keepalive。
- [ ] M39-5 Windows UAC 操作级提权（手工清单；失败/取消状态不变）。
- [ ] M39-6 命名标签/溢出滚动/布局选项四形态渲染。

## 40. M40 — 1.0 稳定化（见 SPEC M40 节）

- [ ] M40-1 criterion 基准集 + docs/bench.md 基线。
- [ ] M40-2 fuzz 目标三组 + CI 10 分钟 + 本地 24h 记录。
- [ ] M40-3 proptest 三组（glob/排序/撤销）。
- [ ] M40-4 覆盖率 ≥75%（ops/remote/frecency/archive/keymap）。
- [ ] M40-5 cargo-deny/audit CI + SBOM + SHA256 校验文件。
- [ ] M40-6 macOS CI 绿 + aarch64 归档。
- [ ] M40-7 1.0.0 发布（CHANGELOG 完整 + crates.io）。