# TASKS — rvifm 任务清单

**本文件是唯一任务来源。** 每轮只取第一个未勾选任务，完成后勾选并更新 [STATE.md](STATE.md)。

- 任务粒度：一次提交能完成。
- **条目 = 一行摘要 + 状态标记 + SPEC 章节引用；不放完整需求原文**（见 AGENTS.md §8）。
- 提交信息：`<task-id>: <summary>`，例如 `M0-1: bootstrap cargo project`。
- 同一任务 3 轮不过验收：缩小范围，在 [NOTES.md](NOTES.md) 记录，标记 `deferred`，继续下一个。
- 任务对应的验收标准见 [ACCEPTANCE.md](ACCEPTANCE.md)。
- 需求边界见 [SPEC.md](SPEC.md)。

图例：`[ ]` 待做，`[x]` 完成，`[~]` 进行中，`deferred` 已延期。

## M0 — 骨架 + SPEC + 双平台 CI

- [x] M0-1 初始化 Cargo 项目与目录结构（`src/`、`docs/`、`.github/workflows/`）
- [x] M0-2 固化 SPEC.md 的 MUST / SIMPLIFIED / OUT_OF_SCOPE 分级（由 harness 初始化提供初稿）
- [x] M0-3 实现 ratatui + crossterm 事件循环骨架
- [x] M0-4 显示当前目录列表 + 状态栏 + `q` 退出
- [x] M0-5 配置 GitHub Actions：ubuntu + windows 跑 fmt/clippy/build/test
- [x] M0-6 增加 `--version` / `--help` 冒烟测试
- [x] M0-7 LICENSE 建档：新增 `LICENSE-MIT`、`LICENSE-APACHE`；`Cargo.toml` 设 `license = "MIT OR Apache-2.0"`；README 许可证段与文件一致
- [x] M0-8 发布 `0.1.0-alpha.1`：前置条件为 M0 全部任务完成且 `ubuntu-latest` + `windows-latest` CI 全绿；发布前复查 crate 名称占用、跑 `cargo package` 检查；人工执行真实发布（crates.io 凭据不放入 CI，该任务不在 CI 自动运行）；发布后用 `cargo install rvifm --version 0.1.0-alpha.1` 验证二进制可装可跑

## M1 — 核心浏览、多标签与 vim 按键

- [x] M1-0 定义 `Tab` / `AppState` 结构：tabs 列表 + active 索引 + 独立状态字段
- [x] M1-1 目录列表渲染与光标移动 `j/k`
- [x] M1-2 进入/返回目录 `l/h`、刷新
- [x] M1-3 `gg/G`、`Ctrl-u/Ctrl-d`
- [x] M1-4 状态栏显示路径、选中项、文件大小/权限
- [x] M1-5 多标签：`:tabnew` / `:tabclose` / `gt` / `gT` / `N gt` / `:tabmove`
- [x] M1-6 每个 tab 独立的 `Location`、光标、滚动、搜索状态
- [x] M1-7 基础搜索 `/` 与 `n/N`（M4 再扩展过滤）

## M2 — 文件操作与 `u` 撤销

- [x] M2-1 定义撤销栈与 `UndoableOp` 枚举（含 tab 标识与路径）
- [x] M2-2 删除（可恢复路径）+ `u` 撤销
- [x] M2-3 重命名 + `u` 撤销
- [x] M2-4 移动 + `u` 撤销
- [x] M2-5 新建目录/文件 + `u` 撤销
- [x] M2-6 连续操作逆序撤销 + 不可逆操作提示
- [x] M2-4b 复制 + `u` 撤销（SPEC §5 M2 MUST，原清单遗漏；按 SPEC 补登记）
- [x] M2-7 跨 tab 撤销：操作发生在非当前 tab 时正确定位

## M3 — 选择、标记与批量操作

- [x] M3-1 `v` 可视选择
- [x] M3-2 `t` 标记文件（tag，不是 mark）
- [x] M3-3 通过 `:tag` 命令执行批量复制/移动/删除
- [x] M3-4 批量操作后清空标记状态

## M4 — 搜索、过滤与跳转

- [x] M4-1 搜索增强：在 M1 基础搜索上补充匹配规则（大小写、路径/内容按需）
- [x] M4-2 简单 glob 过滤

> marks 已移除，由 frecency 的 `pick` 覆盖；不要新增 marks 任务。

## M5 — 配置与最小命令模式

- [x] M5-1 TOML 配置文件读取
- [x] M5-2 基本选项配置（键位覆盖推迟到 M10）
- [x] M5-3 最小 `:` 命令子集：`:q`、`:cd`、`:mkdir`、`:touch`、`:help`
- [x] M5-4 统一命令解析，纳入 M1 的多标签命令

## M6 — 打磨与 v0.1.0 发布

- [x] M6-1 错误处理与确认提示
- [x] M6-2 帮助页
- [x] M6-3 README 更新与 `v0.1.0` tag
- [x] M6-4 大目录（>10k 项）性能检查，滚动不阻塞退出
- [x] M6-5 发布门禁：CI 双平台全绿 + `--release` 产物 + Windows/Linux 冒烟

## M7 — 归档浏览（只读 VFS）

- [x] M7-1 定义 `Location` 与 `ArchiveFs` trait
- [x] M7-2 实现 `ZipFs`（基于 `zip` crate）：list/read/metadata
- [x] M7-3 实现 `TarFs`（基于 `tar` + `flate2`），覆盖 `.tar` 与 `.tar.gz`
- [x] M7-4 导航层接入：归档文件上 `l`/`Enter` 进入虚拟目录，`h` 返回物理父目录
- [x] M7-5 状态栏/面包屑显示 `host:/inner/path`
- [x] M7-6 归档内搜索复用 M4 的搜索链路
- [x] M7-7 错误处理：损坏、加密、空、超大归档索引
- [x] M7-8 第二阶段：`.tar.xz` / `.tar.bz2` / `.tar.zst`
- [x] M7-9 `.7z` / `.rar` 不支持虚拟浏览，给出解压提示

## M8 — 原生 frecency / bookmark 选择器

- [x] M8-1 定义 `EntryRecord` / `ScoredEntry` / `Grade` 数据结构与 0–7 分级档位
- [x] M8-2 redb 三表：`events` / `snapshot` / `meta`，定义 schema 与 tick 语义
- [x] M8-3 `record` 子命令：短事务追加事件 + 有限重试 + `events.pending` 降级
- [x] M8-4 重放器：`snapshot + events` 增量 → 内存评分，`pick` / TUI 共用
- [x] M8-5 compaction：阈值触发 + `rvifm maintenance compact` 手动子命令
- [x] M8-6 `rvifm pick`：自绘 ratatui 选择器（skim 降级，见 NOTES）+ 重放 + 输出选中项，不写事件
- [x] M8-7 shell hook 示例：仅在 `cd` 成功后调用 `record`
- [x] M8-8 抽出 `PreviewRenderer`（文本/目录/图像），TUI 和 pick 共用
- [x] M8-9 在 vifm 场景下验证：替换现有薄插件，对比排名稳定性（aging 测试固化，见 STATE）
- [x] M8-10 目录/文件独立事件时钟（2026-11-14 增补）：meta 增 `dir_tick`/`file_tick`，事件携带类型时钟 `clock`，aging 按同类型时钟衰减；旧库自动兼容（无迁移，clock=0 兜底全局序号）
- [x] M8-11 发布 0.2.1（M8-10 后）：crates.io（API 验证 0.2.1）+ tag `v0.2.1` → GitHub Release（双平台 job 绿，双归档已上传）
- [x] M8-12 TUI 跳转选择器（2026-11-14 增补）：`o` 合一视角（`replay_unified` 全局时钟）+ `:j`/`:J` 仅目录/仅文件（独立时钟）；模糊筛选分级与 pick 一致；选定 = 目录 cd / 文件定位光标；选择器纯只读
- [x] M8-13 发布 0.3.0（M8-12 后）：crates.io（API 验证 0.3.0）+ tag `v0.3.0` → GitHub Release（四 job 全绿，双归档已上传）

## M9 — 图标与主题

- [x] M9-1 集成 `devicons`，文件列表渲染带图标和颜色
- [x] M9-2 集成 `opaline`，加载内置主题并替换硬编码 `Style`（引擎层）
- [x] M9-3 图标 `Theme::Dark` / `Theme::Light` 与主题明暗对齐
- [x] M9-4 `PreviewRenderer` 复用同一图标/主题
- [x] M9-5 无 Nerd Font / `NO_COLOR` 环境下的降级验证
- [x] M9-6 用户 TOML 主题发现与加载（`~/.config/rvifm/themes/`）

## M10 — 声明式键位系统

- [x] M10-1 定义 `Action` 枚举与默认键位表（vifm 对齐，含 `yy`/`P`/`p`/`dd`）
- [x] M10-2 评估 `keymap-rs`（`keymap-core` + `keymap-seq` + `keymap-config`）可行性
- [x] M10-3 集成键位解析层，支持多键序列与超时
- [x] M10-4 实现 Normal / Visual / View / Command-line 模式切换
- [x] M10-5 TOML 配置加载：`prepend_keymap` / `append_keymap` / `keymap` 分层合并（Yank 缓冲区暂未实现，PutMove/PutCopy 的实际操作在 M11 opener 阶段接入）
- [x] M10-6 `PutMove` 语义：`yy` 后 `P` 移动、`p` 复制；`dd` 后 `p` 移动（Yank buffer + 键位接线，暂未接文件操作）
- [x] M10-7 降级路径：自写轻量 Trie 状态机（Keymap 结构即 Trie，M10-2 评估确认 keymap-rs 不采用）

## M11 — 预览与文件打开

- [x] M11-1 集成 `ratatui-image`（协议探测→halfblocks 回退；真实终端显示验证随 M11-6）
- [x] M11-2 图片预览接入 PreviewRenderer（TUI 双栏预览窗格 + `preview_kind` 分流；真实终端显示随 M11-6）
- [x] M11-3 系统 opener 默认层：`gio open` / `xdg-open` / `open` / `start`
- [x] M11-4 TOML opener 配置解析：`opener` 定义 + `prepend` / `append` / `replace` 规则
- [x] M11-5 规则匹配：`mime` 优先，`name` 回退，按顺序取第一个
- [~] M11-6 验证：Alacritty 下图片显示（deferred：需真实终端）
- [x] M11-7 验证：零配置下 `Enter` 打开文件走系统默认，不报错
- [x] M11-8 集成 `office_oxide`（~0.1.16 锁定，实际 0.1.17；DOCX/XLSX/PPTX/DOC/XLS/PPT 文本提取，零外部工具）
- [x] M11-9 Office 提取接入 PreviewRenderer（`PreviewKind::Office` 分流 + 提取失败友好提示，TUI/pick 同源）
- [x] M11-10 PDF 预览探测链（pdf-inspector =1.25.2 文本提取先行 + PdfType 判定 + 外部工具 PATH 探测）
- [x] M11-11 Poppler-free 验证（`PreviewKind::Pdf` 接入共享 PreviewRenderer；干净环境文本提取 + NeedsVisual/Unavailable 友好提示，不依赖 Poppler/MuPDF）
- [x] M11-12 `pdf-visual` feature 门控与依赖边界（验证：default tree 无 exprim，--features pdf-visual 含 0.4.0）
- [x] M11-13 pdf-visual 资源护栏（输入 64MB 上限 + 导出像素 400 万上限/300 万目标 + release `panic = "unwind"` 显式固化）
- [x] M11-14 pdf-visual 语料回归（CJK 无嵌入字体/假加密/损坏/截断语料全部不 panic；正常语料渲染像素在护栏内）
- [x] M11-15 pdf-visual Windows 构建验证（本地 msvc release 构建成功 + CI 双平台 `cargo check --features pdf-visual`；真实终端显示随 M11-6）

## M12 — 权限不足时的内联提权（2026-11-12 增补）

- [x] M12-1 检测写操作 EACCES（`ops::is_permission_denied`：ErrorKind + raw errno EACCES/EPERM；真实 0o555 目录写入测试）
- [x] M12-2 askpass 辅助模式（`rvifm --askpass <fifo>`：FIFO 读密码→stdout；wrapper 兼容所有 sudo 版本；CLI 集成测试闭环）
- [x] M12-3 sudo -A 重试（PrivOp 命令构造 + wrapper 脚本 + TUI AskPass 模式接线；CI NOPASSWD sudo 真实提权测试通过）
- [x] M12-4 取消干净中止（Esc 路径：密码丢弃、状态一致断言、无副作用）
- [x] M12-5 vifm 场景验证（CI ubuntu：EACCES 检测→AskPass→sudo -A 全链路绿；Windows 降级提示退出 2）

## M13 — vifm 对齐增强（2026-11-14 增补；第一/二档，见 SPEC M13 节）

- [x] M13-1 f/F/;/, 首字母快速搜索（环绕列表首尾，区分大小写；Tab.char_search 状态 + ; 同向/, 反向重复）
- [x] M13-2 :select / :unselect 按 pattern 批量选择/取消（`Tab::select_by_pattern`：glob 匹配打标/移除、无参选当前/清全部；t 语义不变）
- [x] M13-3 DD 永久删除（`Action::DeletePermanent` + Confirm 强提示 + `execute_delete` permanent 路径：直接 fs 删除、不进撤销栈）
- [x] M13-4 j/k 列表环绕（`Tab::move_cursor_wrap`：尾→首/首→尾；半页等钳制语义不变）
- [x] M13-5 Ctrl-G 文件详情对话框（`InputMode::FileInfo` 覆盖层：名称/类型/大小/修改时间/权限/路径，任意键关闭）
- [x] M13-6 cw 批量重命名（`c,w` → `Action::RenameBatch` → TUI 挂起 → `$EDITOR`/VISUAL/平台缺省名单 → 行对齐应用（不符整批中止、同名/非法跳过）→ `rename_to` 入撤销栈逐项可 `u` 逆）
- [x] M13-7 :tree 树视图（SIMPLIFIED：静态快照只读浏览，depth=N 缺省 8、上限 1 万条；目录优先相对路径缩进；:tree 再执行恢复平铺；写/导航守卫拒绝）
- [x] M13-8 后台作业 + :jobs（`event::poll` 轮询改造 + `Job`/`JobUpdate`/`apply_job_update` + `execute_job_body` 线程共用体；`:tag copy/move` 后台化、`:jobs` 覆盖层）
- [x] M13-9 view mode（`Tab.view_mode` + `w`/`:view` toggle：预览窗格 max_lines 放大 5000 显示完整内容，j/k 光标移动视图跟随重置，双栏保持非全屏）
- [x] M13-10 暂存区菜单（`index/` 每记录独立 .json（无写竞争）+ `:trash` 覆盖层 + `restore_from_trash` 移回原位；冲突/缺失 Err；测试串行锁）
- [x] M13-11 发布 0.4.0：crates.io（API 验证 0.4.0）+ tag `v0.4.0` → GitHub Release（四 job 全绿，双归档已上传）

## M14 — 双栏模型与标签栏（2026-11-15 增补；用户实测反馈，见 SPEC M14 节）

- [x] M14-1 双 pane 模型（`Pane` 结构 + `Tab.right`/`focus`；Tab 键焦点切换；`handle_right_key` 精简导航集；exec 写命令焦点化（`focus_base`）；`w`/`:view` 右 pane 目录列表↔预览切换；渲染双栏）
- [x] M14-2 标签栏 UI（顶部 1 行：`N:dirname` 列表 + 当前 tab REVERSED|BOLD 高亮；`:tabnew`/`gt` 可见反馈；相关渲染测试行号随 tab 栏偏移修正）
- [x] M14-3 ACCEPTANCE §16 修订（0.5.0 语义：双栏描述/标签栏可见性验收）
- [x] M14-4 发布 0.5.0：crates.io（API 验证 0.5.0）+ tag `v0.5.0` → GitHub Release（双平台归档已上传；tag 重指向测试修复提交后由 release job 产出）

## M17 — 递归搜索与查看器批次（2026-11-15 增补；结果=虚拟目录 + 交互式查看器，见 SPEC M17 节）

> 来源：用户设计（替代 frecency picker 方案）。依赖：ignore（新，理由见 NOTES）+ regex（升直接）。

- [x] SRCH-1 :find 递归文件名查找（ignore 遍历 + gitignore/show_hidden；结果虚拟目录沿 :tree 同构：Tab.results + items/entries 平行表；全路径显示；j/k/gg/G/yy/预览=目录语义；l/Enter=定位目录+光标；Esc/h=退出；只读守卫；上限 1 万）
- [x] SRCH-2 :grep 内容搜索（ignore + regex 行匹配；条目 路径:行号: 内容；二进制/非 UTF-8 跳过；regex 非法可读报错；l/Enter=定位文件）
- [x] VIEW-1 预览查看器（w 激活：行号 + 滚动 + / 搜索定位 + 匹配高亮 + n/N；grep 命中定位命中行；q/Esc/w 退出）

## M21 — UI/UX 打磨批次（2026-11-17 增补；缺陷修复×9 + 体验增强×8，见 SPEC M21 节）

> 来源：用户需求讨论稿蒸馏（编号映射 FIX-n→M21-Fn、ENH-n→M21-En，见 NOTES）。顺序：F1→F9 → E1→E8。

- [x] M21-F1 界面风格（pane 边框+居中+顶部路径+类型着色+底部详情；状态栏去路径）
- [x] M21-F2 焦点视觉（非焦点 pane 当前行 DIM；状态栏去焦点文字）
- [ ] M21-F3 搜索 Esc 清理（状态栏+高亮全清，再入干净）
- [ ] M21-F4 `h` 返回定位（子目录/归档文件条目）
- [ ] M21-F5 外部变更自动刷新（notify crate；归档不监听；失败静默）
- [ ] M21-F6 目录 tree 预览（浅层只读，不走文本读取路径）
- [ ] M21-F7 PDF/Office 预览分流核实（不落文本通用路径；类型化错误）
- [ ] M21-F8 opener stderr 抑制（Stdio::null；失败进状态栏）
- [ ] M21-F9 进入目录 record（l/Enter 成功后调用；失败不记）
- [ ] M21-E1 列表属性列（大小+日期 yyyy-mm-dd HH:mm:ss；自适应；配置开关）
- [ ] M21-E2 `w` 预览模式验证补缺（已实现于 view_mode，验证+补缺）
- [ ] M21-E3 语法高亮（tree-sitter/syntect 复用；.txt 推断；截断提示）
- [ ] M21-E4 灵活重命名（I/cc/A/R；R=$EDITOR 多选，复用 cw 管线；整批 u）
- [ ] M21-E5 `:mkdir` 定位 / `:mk` 进入
- [ ] M21-E6 `.tar.xz`/`.tar.bz2` 验证（已实现于 M7-8，验证+补缺）
- [ ] M21-E7 `=` 过滤增强（实时+通配符+隐式 *模式*）
- [ ] M21-E8 跳转选择器增强（80% 区域不遮列表+Vim 模式+记录补全：目录/文件成功路径）

## M18 — 排序系统与终端集成（2026-11-15 增补；vifm 排序对齐，见 SPEC M18 节）

> 来源：用户设计。编号说明：原提案 M16/M17 与已完成批次冲突，按下一空闲号重编（内容不变，见 NOTES）。

- [x] M18-1 `S` 排序对话框（列出字段与当前状态；对话框内 n/N/m/M/s/S/e/E 直接应用并关闭（小写升/大写降）；j/k 浏览 + 空格切换高亮字段升降（不立即应用）+ 回车应用；应用后重排且光标跟随原文件；对话框内 `s` 不触发 shell、空格不切窗格；无多字段组合）
- [x] M18-2 多字段排序仅命令参数（`:sort name,size desc` 逗号分隔 + asc/desc 后缀；解析失败可读错误；无 TUI 字段栈）
- [x] M18-3 `s` 进入 shell（$SHELL；raw mode/alternate screen 退出恢复；光标与滚动不变；失败可读提示）
- [x] M18-4 `:!<cmd>` + `[ui] terminal_in_new_window`（默认 true；$TMUX/$STY 检测新窗口执行，否则当前 pane；false 始终当前 pane；同时作用于 `s` 与 `:!`；退出后 TUI 状态一致）+ Space=切换活动窗格（vifm 语义，与 Tab 并存）

## M19 — SFTP 远端 VFS：浏览与配置（2026-11-15 增补，见 SPEC M19 节）

- [x] M19-1 定义 RemoteFs trait（list/read/metadata + 错误分类：连接/认证/不存在/权限/超时；协议无关；预留 write/remove/rename/create_dir 签名默认未实现）
- [x] M19-2 `Location::Remote { remote, path }` 与导航接入（j/k/l/h 同 Physical；h 远端根返回本地目录；面包屑 remote:/path；排序/列表内搜索/预览复用本地实现；:find/:grep 不作用远端——后续增强）
- [x] M19-3a SFTP 选型 spike（examples/sftp_spike.rs：russh+justsftp 或 remotefs-rs 连 127.0.0.1 list 根目录；记录编译/依赖/认证/异步模型/集成难度 + 写操作支持；结论入 NOTES 后暂停等确认；dev-dependencies 不进正式依赖）
- [x] M19-3b SFTP 后端正式实现（依赖 3a 结论；密钥认证为主；超时/错误映射；网络 IO 不阻塞事件循环；仅 list/read/metadata）
- [x] M19-4 浏览与配置（~/.ssh/config Host 别名 + remotes.toml 覆盖（protocol/host/port/user/key/root）；`:remote <name>` 进入；`:remote` 列出全部；连接/认证/超时可读提示）

## M20 — SFTP 远端 VFS：写操作与本地体验对齐（2026-11-15 增补，见 SPEC M20 节；依赖 M19 验收）

- [x] M20-1 远端写操作基础（write/remove/rename/create_dir；UndoableOp::Remote 变体（remote/路径/类型）入既有撤销栈；dd/yy/p/P/重命名/:mkdir 与本地同键；失败可读错误）
- [x] M20-2 确认与不可逆边界（覆盖/删非空目录确认文案同本地；远端删除走可恢复路径否则显式"不可撤销"并标记；取消后状态不变）
- [x] M20-3 跨 VFS 复制/移动（本地 yy → 远端 p/P 及反向；同进度提示与撤销语义；大文件传输状态/进度不阻塞 UI；失败清理半成品）
- [x] M20-4 验收边界（上传→列表→重命名→删除→撤销删除 全链路可重复；覆盖确认取消不变；断网写操作可读错误不留半成品）

## 排队缺口（2026-11-15 M16 实施中发现，见 NOTES 同日条目）

- [x] GAP-1 M10-5 收尾：keymap TOML 分层接线（config.rs 解析 prepend/append/replace 并合并进 InputState.keymap——当前仅默认表，用户绑定不可用；NAV-3 的推荐映射示例依赖此项）
- [x] GAP-2 M8 收尾：TUI 退出时批量追加会话访问事件（SPEC §4.6"退出时批量追加"未实现——会话访问退出即丢，drain 各 tab history → append_events；NAV-4 的会话合并只覆盖即时可见性）

## M16 — 导航增强批次（2026-11-15 增补；marks 替代方案 + vifm history 对齐，见 SPEC M16 节）

> 来源：用户实测讨论蒸馏。marks 不回归（决策维持）；执行顺序 NAV-1 → NAV-3 → NAV-2 → NAV-4（栈先定型，菜单消费最终结构）。

- [x] NAV-1 frecency 快速映射层（`'`+单字符 → keyword 最高排名条目直达 cd；keyword 设置入口 `:keyword <chars>` + `record --keyword` 走 append-only keyword 事件；`'` 未命中提示不静默；`[quickjump]` char→keyword 可选别名；跳转走既有 record 链）
- [x] NAV-3 :histprev/:histnext（Tab.history 改 back/forward 游标、新导航截断 forward；无默认键，help + keymap.toml 示例注明 Ctrl-I==Tab 冲突；跨 tab 独立）
- [x] NAV-2 :history 菜单（当前 tab back 栈菜单：j/k、Enter/l 导航、dd 移除内存记录；按 tab 独立；Esc/q 关闭）
- [x] NAV-4 o 选择器"近期"视角（JumpKind::Recent：last_visit 倒序 + 会话内存评分合并；复用 JumpSelect 交互，入口命令名实现时定）

## M15 — 用户实测反馈批次（2026-11-15 增补；基线 0.5.0，见 SPEC M15 节）

> 来源：用户实测反馈。先缺陷修复（FIX-1～4），后新增（FEAT-1～4），按序执行。

- [x] FIX-1 搜索高亮与实时索引（`/` 为高亮+跳转语义：匹配子串高亮 + 输入实时跳第一匹配 + 状态栏计数 i/N；n/N 同步；清空还原；不隐藏非匹配项）
- [x] FIX-2 yy/p 复制链路修复（yank 缓冲区→PutCopy 接线；成功/失败状态栏提示；入撤销栈 u 可逆；目标同名报错不覆盖）
- [x] FIX-3 归档浏览回归修复（l/Enter 进入 zip/tar/tar.gz 虚拟目录；归档根 h 返回物理目录；host:/inner/path 面包屑；只读守卫；损坏/加密/空可读错误）
- [x] FIX-4 o 选择器居中与遮罩（水平垂直居中 + 尺寸上限 + 背景清除；关闭恢复光标/滚动）
- [x] FEAT-1 鼠标支持（单击聚焦/已聚焦再击打开/双击直接打开；列表外不响应；滚轮滚动；Visual 按 SIMPLIFIED 处理）
- [x] FEAT-2 列表顶部 ".." 上级目录项（视觉区分；根目录置灰/隐藏；归档根返回物理目录；渲染层虚拟条目不进搜索/过滤/标记）
- [x] FEAT-3 :file 命令与右键上下文菜单（六项动作 Open/File Info/Copy/Move/Rename/Delete；Esc/外部点击关闭；与 Ctrl-G 信息同源）
- [x] FEAT-4 :help 全屏帮助视图（j/k 滚动、/ 搜索 n/N、q/Esc 关闭恢复原状态；include_str! 内嵌文本）

## 不在任务清单中的事项

以下内容明确不实现，不要为它们创建任务：

- marks（`m` + 字母 / `'` + 字母）
- Vim 脚本引擎、插件系统
- FUSE / `archivemount`
- fzf / SQLite
- 完整 Vim 引擎
- 归档内写入、嵌套归档递归浏览
- 内嵌视频播放
- PDF 渲染引擎作为默认依赖（`exprim` 仅限非默认 `pdf-visual` feature；`tdf` / `pdfium-render` 完全禁止）
