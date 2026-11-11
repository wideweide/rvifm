# STATE — 当前进度

> 本文件是中断恢复的锚点。每轮任务完成后更新；开工前先读这里，再读 [TASKS.md](TASKS.md)。

- **更新时间**：2026-11-11
- **当前阶段**：M5 — 配置与最小命令模式（M0–M4 已完成）
- **当前任务**：`M5-3` 最小 `:` 命令子集：`:q`、`:cd`、`:mkdir`、`:touch`、`:help`
- **阶段状态**：M0 已完成（已发布）；M1–M4 已完成；M5 进行中

## 已完成

- harness 文档初始化：
  - `README.md`
  - `AGENTS.md`
  - `docs/SPEC.md`
  - `docs/PLAN.md`
  - `docs/TASKS.md`
  - `docs/ACCEPTANCE.md`
  - `docs/STATE.md`
  - `docs/NOTES.md`
- `M0-2`：SPEC.md 的 MUST / SIMPLIFIED / OUT_OF_SCOPE 分级初稿已固化（由 harness 初始化提供）。
- PDF 预览路线定稿（2026-11-10）：文本提取用 `pdf-inspector =1.25.2`；`exprim =0.4.0` 设为可选 `pdf-visual` 视觉后端（非默认依赖）。实测数据与决策记录见 `docs/NOTES.md` §4.8，任务见 M11-10～M11-15。
- exprim 定稿（2026-11-11）：保持文本优先，`exprim = { version = "=0.4.0", optional = true }` 仅随非默认 `pdf-visual` feature 编译；默认构建依赖树不含 exprim；探测顺序为文本提取 → exprim 栅格化（仅 feature）→ `pdftoppm`/`mutool` → 提示/opener；护栏为像素上限 200 万–400 万、输入 PDF 大小上限、`panic = "unwind"`。见 `docs/SPEC.md` §6.1、`docs/NOTES.md` §4.8。
- Office / 许可证 / crates.io 名称定稿（2026-11-11）：`office_oxide ~0.1.16`；`MIT OR Apache-2.0`；`rvifm` 已确认可用（备选 `rvifm-tui` / `rvifm-cli`，二进制名保持 `rvifm`）。见 `docs/SPEC.md` §6.1、`docs/NOTES.md` §4.8。
- `M0-1`（2026-11-11）：Cargo 项目骨架落地——`Cargo.toml`（edition 2024，license 字段留待 M0-7）、`src/main.rs` 占位（事件循环在 M0-3 接入）、`.github/workflows/` 目录占位（CI 配置在 M0-5）；`Cargo.lock` 入库。fmt / clippy / build / test 全绿，二进制裸跑与带参运行均 exit 0 无 panic。
- `M0-3`（2026-11-11）：接入 `ratatui 0.30` 事件循环骨架（`src/app.rs`）：`ratatui::init/restore` 管理终端生命周期，循环 = 绘制 → 阻塞读事件 → 分发；骨架阶段 `Ctrl-C` 退出（`q` 留给 M0-4）；渲染经 `TestBackend` 单测验证，无 TTY 冒烟确认进入备用屏渲染无 panic。
- `M0-4`（2026-11-11）：最小可运行 TUI——启动时加载当前目录条目（目录在前、名称不区分大小写排序，目录带 `/` 后缀），底部 1 行状态栏显示 cwd 与条目数；`q` / `Ctrl-C` 退出；仅忽略 `Press` 事件（Windows 会多发 `Release`）。共 4 项单测（键位、排序、渲染），门禁全绿。
- `M0-5`（2026-11-11）：`.github/workflows/ci.yml`——`ubuntu-latest` + `windows-latest` 矩阵，步骤 checkout → dtolnay/stable（含 rustfmt/clippy）→ rust-cache → `fmt --check` / `clippy -D warnings` / `build --locked` / `test --locked`。**CI 已验证全绿**（run [42875515797](https://github.com/wideweide/rvifm/actions/runs/42875515797)，M0-7 提交触发，两 job 全部步骤 success）。
- `M0-6`（2026-11-11）：`--version` / `-V` / `--help` / `-h` 旗标（TUI 初始化前处理，无 TTY 可跑）；未知参数 stderr 提示并以 exit 2 退出；`tests/cli.rs` 3 项冒烟（版本输出、帮助输出、未知参数非零退出）。
- `M0-7`（2026-11-11）：`LICENSE-MIT` / `LICENSE-APACHE` 全文建档，`Cargo.toml` 设 `license = "MIT OR Apache-2.0"`；README 许可证段原本即一致，未改动。`cargo package --list` 验证通过（含两个 LICENSE 文件）。遗留：manifest 无 `repository`/`homepage` 的 warning 留待 M0-8 发布前补。
- **M0 验收自检（2026-11-11，除 M0-8 外全部通过）**：build 成功且 rvifm 可启动 ✓；TUI 显示目录列表与状态栏（TestBackend 单测）✓；`q`/`Ctrl-C` 退出无 panic（单测；真机交互冒烟待用户）✓；`--version`/`--help` 输出且 exit 0 ✓；SPEC 分级与 PLAN 一致（M0-2）✓；双平台 CI 绿 ✓。剩余：M0-8 人工发布。
- **M0-8 完成（2026-11-11）**：`cargo publish` 成功，[rvifm 0.1.0-alpha.1](https://crates.io/crates/rvifm) 上线（API 确认 max_version）；`cargo install rvifm --version 0.1.0-alpha.1` 成功，安装版 `--version` 输出 `0.1.0-alpha.1` 且 exit 0。**M0 阶段全部任务完成，阶段关闭。**
- `M1-0`（2026-11-11）：`src/tab.rs` 落地 `Location`（Physical/Archive，显示 `host:/inner/path`）、`FileItem`、`SearchState`、`Tab`（location/entries/cursor/scroll/search/history）；`src/state.rs` 落地 `AppState`（tabs + active + 全局 undo_stack）与预留 `tab_id` 的 `UndoableOp` 占位。渲染改由 `AppState.active_tab()` 驱动，行为与 M0 一致；未接线字段用带任务指向的 `#[allow(dead_code)]` 标注。9 单测 + 3 冒烟全绿。
- `M1-1`（2026-11-11）：`Tab::move_cursor(delta)` 钳制两端、空目录 no-op；`j/k` 接入 `handle_key`；渲染改用 `ListState` + `highlight_style(REVERSED)` 高亮选中行（TestBackend 断言 modifier）。13 单测 + 3 冒烟全绿。
- `M1-2`（2026-11-11）：`l`/`Enter` 进入选中目录、`h` 返回父目录；导航即重读条目（刷新）；读取失败不导航不产生副作用（提示在 M6-1）；文件系统根 `h` 为 no-op（Windows 盘根 parent() 为 None）；进入/返回均写入 `Tab.history`。16 单测 + 3 冒烟全绿。
- `M1-3`（2026-11-11）：`gg`（`g` 前缀 Pending 序列，机制与 M1-5 `gt/gT` 共用）、`G`、`Ctrl-u/d` 半页翻页（步进 = (视口高-1)/2，随终端尺寸计算）。语义修正：待决序列优先于退出与普通分发，Release 先于序列过滤（避免 Release 误消费序列）。19 单测 + 3 冒烟全绿。
- `M1-4`（2026-11-11）：`FileItem` 增加 `size`/`perm`（unix 3 位八进制 / Windows `ro`/`rw`，加载时物化避免渲染层 cfg）；状态栏 = `路径 | 选中项 [大小] 权限 | N items`（目录省略大小）；`format_size` B/KiB/MiB/GiB。22 单测 + 3 冒烟全绿。
- `M1-5`（2026-11-11）：多标签落地——`gt`/`gT` 循环切换、`N gt` 数字计数跳转（计数被其他动作键丢弃）；最小命令行 `:`（Enter 提交 / Esc 取消 / Backspace 退格），`tabnew`（当前目录）、`tabclose`（最后一个 tab 关闭即退出）、`tabmove [N]`（无参移到末尾）；命令错误临时显示在状态栏 `[msg]`（M6-1 统一提示）。`InputMode::Normal/Command` 先行，M5-4 统一。27 单测 + 3 冒烟全绿。
- `M1-6`（2026-11-11）：tab 状态独立性契约测试——切换 tab 后 `Location`/`cursor`/`scroll`/`search` 全部独立保持。说明：视口滚动由 `ListState` 从各 tab 的 cursor 派生（天然独立）；`Tab.scroll` 显式偏移存储留待 M10；归档 tab 回归验证按 ACCEPTANCE 留待 M7。28 单测 + 3 冒烟全绿。
- `M1-7`（2026-11-11）：基础搜索——`/` 进入搜索输入（Enter 提交 / Esc 取消），`Tab.find_match` 大小写不敏感子串环绕匹配，提交后移动光标并写入 `Tab.search`；`n`/`N` 沿方向跳转（环绕）；无搜索词/无匹配以状态栏提示（M6-1 统一）；Normal 模式状态栏显示 `/{query}` 指示。32 单测 + 3 冒烟全绿。
- **M1 阶段验收自检（2026-11-11，ACCEPTANCE §3）**：`j/k` 边界钳制 ✓；`l/Enter/h` 导航 + 根目录明确 no-op ✓；`gg/G/Ctrl-u/d` ✓；状态栏路径/选中项/大小/权限 ✓；`/` 与 `n/N` ✓；`:tabnew/:tabclose/gt/gT/N gt/:tabmove` ✓；两 tab 不同目录切换后状态独立（契约测试）✓；`Tab/AppState` 落地且撤销栈预留 tab 标识 ✓；归档 tab 回归按 ACCEPTANCE 自身定义留待 M7 ✓；交互真机冒烟待用户 `cargo run`。**M1 阶段关闭。**
- `M2-1`（2026-11-11）：`UndoableOp` 枚举落地（Delete/Rename/Move/Create，全部携带 `tab_id` 与路径）+ `tab_id()`/`primary_path()` 访问器；`UndoStack` LIFO（push/pop/len/is_empty）。各操作构造与 `u` 执行随 M2-2～M2-5 接入（定向 allow 标注消费任务）。32 单测 + 3 冒烟全绿。
- `M2-2`（2026-11-11）：删除可逆闭环——`dd` 触发确认模式（`y`/Enter 执行，其余取消无副作用）；`ops::delete_to_trash` 移入应用内暂存（跨盘回退 copy+delete，选型见 NOTES §4.2）；`u` 弹栈执行逆操作（父目录缺失重建、目标已存在报错防覆盖、失败回栈可重试）；删除/撤销后刷新所在 tab 并钳制光标；空栈提示"无可撤销操作"。`UndoableOp::Delete` 携带 `staged` 暂存路径。37 单测 + 3 冒烟全绿。
- `M2-3`（2026-11-11）：`:rename <新名字>` 重命名选中条目——仅改文件名（拒绝路径分隔符/空名/目标已存在），成功即入撤销栈并刷新；`u` 反向 rename 恢复原名。含空格的名字在 M5-4 统一解析。40 单测 + 3 冒烟全绿。
- `M2-4`（2026-11-11）：`:move <目标目录>` 移动选中条目（保留原名；`yy`/`P` 键位在 M10）；`u` 反向移回。**修复真实缺陷**：目录移入自身内部会让跨盘 copy 回退无限递归（栈溢出）——`move_to` 增加 `to.starts_with(path)` 自嵌套守卫。43 单测 + 3 冒烟全绿。
- `M2-5`（2026-11-11）：`:mkdir <名字>` / `:touch <名字>` 在当前目录新建（拒绝空名/路径分隔符/已存在）；`u` 移除新建路径（非空目录保护性报错，避免连带删内容）；undo 对"逆操作源已不存在"幂等处理（避免过期操作卡栈）。46 单测 + 3 冒烟全绿。
- `M2-6`（2026-11-11）：连续操作逆序撤销 E2E 测试（mkdir→touch→rename 三步、三次 `u` 逆序还原）；`u` 成功后状态栏反馈"已撤销: <描述>"（`UndoableOp::describe`）；`:tabclose` 关闭非最后 tab 提示"不可撤销"。`exec_command` 重构为 `ExecOutcome{Quit, Done{msg}}`（测试兼容包装保留旧契约）。47 单测 + 3 冒烟全绿（更正：前一行原记 48 有误）。
- `M2-7`（2026-11-11）：跨 tab 撤销契约测试——tab 1 删除 → 切到 tab 2 → `u` → tab 1 文件恢复、当前 tab 不变、tab 1 列表刷新包含恢复项。设计取向"当前 tab 不变"（ACCEPTANCE §4 允许项）；原 tab 已关闭时跳过列表刷新，磁盘恢复不受影响。
- `M2-4b`（2026-11-11，SPEC §5 M2 MUST 补任务）：`:copy <目标目录>` 复制选中条目（保留原名，源不动）；undo = 删除生成的副本（`Create` 语义，`recursive` 标记目录树整体移除、mkdir 保持非空目录保护）；拒绝同名冲突/目标目录不存在/自嵌套。`p`（PutCopy）键位在 M10 接入。51 单测 + 3 冒烟全绿。
- **M2 阶段验收自检（2026-11-11，ACCEPTANCE §4）**：复制/移动/删除/重命名/新建均可用 ✓；`dd` 确认 + 取消无副作用，显式输入命令自带确认 ✓；操作失败有状态栏提示、无 panic ✓；删除/重命名/移动/新建均可 `u` 恢复 ✓；连续操作逆序撤销 E2E ✓；不可逆操作明确提示（tabclose/空栈/失败回栈）✓；删除走应用内暂存 ✓；跨 tab 撤销正确定位、当前 tab 不变 ✓；`UndoableOp` 记录 tab 与路径 ✓；归档内修改不在 M2 范围（M7 只读边界）。**M2 阶段关闭。**
- `M3-1`（2026-11-11）：`v` 可视选择——`Tab.visual` 锚点（每 tab 独立），`visual_range()` 闭区间随光标移动扩大/收缩；选区行反显渲染、状态栏 `VISUAL (n selected)`；`Esc` 退出；空目录 no-op；refresh 钳制锚点。54 单测 + 3 冒烟全绿。
- `M3-2`（2026-11-11）：`t` tag 标记（tag ≠ mark 红线在代码与文档显式声明）——`Tab.tags` 按文件名记录（每 tab 独立，目录重读后按名匹配，改名/删除条目批量时跳过）；标记行加粗渲染、状态栏 `n tagged`；空目录 no-op。56 单测 + 3 冒烟全绿。
- `M3-3`（2026-11-11）：`:tag delete / copy|move <目录>` 批量操作——逐文件执行并入撤销栈（LIFO 逐条还原）；`delete` 先进确认模式（`y`/Enter 执行、其余取消）；已消失标记条目跳过并计数；未标记文件不受影响；空标记/未知动作有明确提示。57 单测 + 3 冒烟全绿。
- `M3-4`（2026-11-11）：批量操作（copy/move/delete）完成后自动清空标记状态（取消确认不清空）；tag_batch 测试覆盖三批操作各自清空断言。
- **M3 阶段验收自检（2026-11-11，ACCEPTANCE §5）**：`v` 进入/扩大/收缩/退出状态正确 ✓；`t` 多文件标记、再次 t 取消 ✓；`:tag` 批量复制/移动/删除且未标记不受影响 ✓；批量后标记清空 ✓；tag ≠ mark 语义未混淆（代码注释 + TASKS/SPEC 无 marks 实现）✓。**M3 阶段关闭。**
- `M4-1`（2026-11-11）：搜索匹配规则增强——smart-case（查询含大写字母 → 区分大小写；全小写 → 不区分；vim/vifm 惯例），`query_matches()` 统一匹配入口，子串语义保持。内容/路径搜索按需后续扩展。58 单测 + 3 冒烟全绿。
- `M4-2`（2026-11-11）：glob 过滤——`=` 进入过滤输入（Enter 应用 / Esc 取消），手写 `*`/`?` 匹配器（不区分大小写）；`Tab.filter` 每 tab 独立，`refresh()` 加载后保留匹配条目（批量/标记操作天然只作用于可见条目）；空 pattern 清除；状态栏 `[filter: …]`；应用后光标与选区归零。61 单测 + 3 冒烟全绿。
- **M4 阶段验收自检（2026-11-11，ACCEPTANCE §6）**：`/` 定位 + `n/N` 循环 ✓；glob 过滤生效、过滤后光标与状态栏正确 ✓；归档/文件夹内搜索复用同一链路（find_match 作用于 tab.entries，M7 接入后自动生效）✓；无 marks 相关命令或按键 ✓。**M4 阶段关闭。**
- `M5-1`（2026-11-11）：TOML 配置读取——新增 `serde`/`toml` 依赖；`src/config.rs`：`Config{show_hidden, sort_dirs_first, ignore_case_sort}`（`#[serde(default)]` 逐键回退）、平台路径（Windows `%APPDATA%\rvifm\config.toml` / Unix `$XDG_CONFIG_HOME` 或 `~/.config`）、`load()`（缺失→默认值不退出；解析失败→"配置解析失败: …" 提示）；启动时加载并挂到 `AppState.config`，错误进状态栏。65 单测 + 3 冒烟全绿。
- `M5-2`（2026-11-11）：基本选项生效——`load_entries(dir, &Config)` 按 `show_hidden` 跳过 `.` 开头条目、按 `sort_dirs_first`/`ignore_case_sort` 排序；配置经 `from_physical`/`refresh`/`apply_filter`/导航全部接线（`Config` 为 Copy）。65 单测 + 3 冒烟全绿。

## 进行中

- 无。

## 下一步

1. `M5-1`：TOML 配置文件读取。
2. `M5-2`～`M5-4`：基本选项配置、最小 `:` 命令子集、统一命令解析（顺序见 [TASKS.md](TASKS.md)）。
3. M5 完成后跑阶段验收（[ACCEPTANCE.md](ACCEPTANCE.md) §7）。
3. M2 完成后跑阶段验收（[ACCEPTANCE.md](ACCEPTANCE.md) §4），确认 `u` 可逆闭环。
3. M1 完成后跑阶段验收（[ACCEPTANCE.md](ACCEPTANCE.md) §3）。
4. M11 阶段按 M11-12～M11-15 落地 `pdf-visual` 门控、资源护栏、语料回归与 Windows 构建（不改变当前优先级）。

## 阻塞与风险

- 无阻塞。
- 环境风险：本地为 Windows Server 2016，ZCode（Electron）可能无法启动；使用 VS Code + `rust-analyzer` 或纯命令行。Linux 验证交给 CI。
- 预算风险：见 [PLAN.md](PLAN.md) 的预算表；优先保证 M0–M2。

## 预算

- 规划区间：约 **7500 万–1.1 亿 token**（数量级参考，非硬上限）。
- 已使用：少量（M0-1 骨架已完成；另消耗一次本地工具链安装会话）。
- 控制原则：每轮只做一个任务，只注入相关 SPEC 片段；同一任务 3 轮不过验收就缩小范围并 `deferred`。

## 环境

- 本地：Windows Server 2016，目标 `x86_64-pc-windows-msvc`。
- CI：`ubuntu-latest` + `windows-latest`。
- 本地内循环：`cargo fmt` → `clippy -D warnings` → `build` → `test` → `commit`。
- 发布外循环：`push/tag` → CI 矩阵构建 → 测试 → 归档 → Release。

## 恢复方式

1. 读本文件，确认当前阶段与任务。
2. 打开 [TASKS.md](TASKS.md)，取第一个未勾选任务。
3. 读 [AGENTS.md](../AGENTS.md) 第 3 节执行循环。
4. 只读该任务相关的 [SPEC.md](SPEC.md) 片段与 [ACCEPTANCE.md](ACCEPTANCE.md) 条目。
5. 开始实现；完成后更新本文件与 TASKS，再提交。
