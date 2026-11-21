# TASKS — rvifm 任务清单

**本文件是唯一任务来源。** 每轮只取第一个未勾选任务，完成后勾选并更新 [STATE.md](STATE.md)。

- 任务粒度：一次提交能完成。
- **条目 = 一行摘要 + 状态标记 + SPEC 章节引用；不放完整需求原文**（见 AGENTS.md §8）。
- 提交信息：`<task-id>: <summary>`，例如 `M0-1: bootstrap cargo project`。
- 同一任务 3 轮不过验收：缩小范围，在 [NOTES.md](NOTES.md) 记录，标记 `deferred`，继续下一个。
- 任务对应的验收标准见 [ACCEPTANCE.md](ACCEPTANCE.md)。
- 需求边界见 [SPEC.md](SPEC.md)。

图例：`[ ]` 待做，`[x]` 完成，`[~]` 进行中/暂停（不参与“取第一个未勾选任务”），`deferred` 已延期。
节排序：按编号升序（2026-11-19 重排）；AUDIT 为已确认的 P0 整改批次，置于 M26 之前以保证其为第一个未勾选任务（2026-11-19 用户确认）；"排队缺口"为补充段置于末尾。

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

> 历史注：marks 曾移除由 frecency `pick` 覆盖；2026-11-19 用户指令反转后，最终确定由 frecency 实现并持久化（见 M25-2 重定义条目）。

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
- [x] M7-9 `.7z` / `.rar` 不支持虚拟浏览，给出解压提示（7z 部分由 M26-3 升级）

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

## M16 — 导航增强批次（2026-11-15 增补；marks 替代方案 + vifm history 对齐，见 SPEC M16 节）

> 来源：用户实测讨论蒸馏。执行顺序 NAV-1 → NAV-3 → NAV-2 → NAV-4（栈先定型，菜单消费最终结构）。

- [x] NAV-1 frecency 快速映射层（`'`+单字符 → keyword 最高排名条目直达 cd；keyword 设置入口 `:keyword <chars>` + `record --keyword` 走 append-only keyword 事件；`'` 未命中提示不静默；`[quickjump]` char→keyword 可选别名；跳转走既有 record 链）
- [x] NAV-3 :histprev/:histnext（Tab.history 改 back/forward 游标、新导航截断 forward；无默认键，help + keymap.toml 示例注明 Ctrl-I==Tab 冲突；跨 tab 独立）
- [x] NAV-2 :history 菜单（当前 tab back 栈菜单：j/k、Enter/l 导航、dd 移除内存记录；按 tab 独立；Esc/q 关闭）
- [x] NAV-4 o 选择器"近期"视角（JumpKind::Recent：last_visit 倒序 + 会话内存评分合并；复用 JumpSelect 交互，入口命令名实现时定）

## M17 — 递归搜索与查看器批次（2026-11-15 增补；结果=虚拟目录 + 交互式查看器，见 SPEC M17 节）

> 来源：用户设计（替代 frecency picker 方案）。依赖：ignore（新，理由见 NOTES）+ regex（升直接）。

- [x] SRCH-1 :find 递归文件名查找（ignore 遍历 + gitignore/show_hidden；结果虚拟目录沿 :tree 同构：Tab.results + items/entries 平行表；全路径显示；j/k/gg/G/yy/预览=目录语义；l/Enter=定位目录+光标；Esc/h=退出；只读守卫；上限 1 万）
- [x] SRCH-2 :grep 内容搜索（ignore + regex 行匹配；条目 路径:行号: 内容；二进制/非 UTF-8 跳过；regex 非法可读报错；l/Enter=定位文件）
- [x] VIEW-1 预览查看器（w 激活：行号 + 滚动 + / 搜索定位 + 匹配高亮 + n/N；grep 命中定位命中行；q/Esc/w 退出）

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

## M21 — UI/UX 打磨批次（2026-11-17 增补；缺陷修复×9 + 体验增强×8，见 SPEC M21 节）

> 来源：用户需求讨论稿蒸馏（编号映射 FIX-n→M21-Fn、ENH-n→M21-En，见 NOTES）。顺序：F1→F9 → E1→E8。

- [x] M21-F1 界面风格（pane 边框+居中+顶部路径+类型着色+底部详情；状态栏去路径）
- [x] M21-F2 焦点视觉（非焦点 pane 当前行 DIM；状态栏去焦点文字）
- [x] M21-F3 搜索 Esc 清理（状态栏+高亮全清，再入干净）
- [x] M21-F4 `h` 返回定位（子目录/归档文件条目）
- [x] M21-F5 外部变更自动刷新（notify crate；归档不监听；失败静默）
- [x] M21-F6 目录 tree 预览（浅层只读，不走文本读取路径）
- [x] M21-F7 PDF/Office 预览分流核实（不落文本通用路径；类型化错误）
- [x] M21-F8 opener stderr 抑制（Stdio::null；失败进状态栏）
- [x] M21-F9 进入目录 record（l/Enter 成功后调用；失败不记）
- [x] M21-E1 列表属性列（大小+日期 yyyy-mm-dd HH:mm:ss；自适应；配置开关）
- [x] M21-E2 `w` 预览模式验证补缺（已实现于 view_mode，验证+补缺）
- [x] M21-E3 语法高亮（tree-sitter/syntect 复用；.txt 推断；截断提示）
- [x] M21-E4 灵活重命名（I/cc/A/R；R=$EDITOR 多选，复用 cw 管线；整批 u）
- [x] M21-E5 `:mkdir` 定位 / `:mk` 进入
- [x] M21-E6 `.tar.xz`/`.tar.bz2` 验证（已实现于 M7-8，验证+补缺）
- [x] M21-E7 `=` 过滤增强（实时+通配符+隐式 *模式*）
- [x] M21-E8 跳转选择器增强（80% 区域不遮列表+Vim 模式+记录补全：目录/文件成功路径）

## M22 — SFTP 安全与远端体验补全（2026-11-17 增补；见 SPEC M22 节）

> 来源：用户采纳批次建议蒸馏。顺序：M22-1（修复）→ 2/3（安全）→ 4/5/6（远端体验）→ 7（文档收尾）。

- [x] M22-1 `:cd` 相对路径锚定当前 tab 目录（Location 恒绝对路径；失败不改位置）
- [x] M22-2 known_hosts 校验（首连写入；指纹变更拒连含指引；`known_hosts = false` 显式跳过）
- [x] M22-3 SFTP 口令认证（密钥失败回退；AskPass 输入不落盘；取消干净中止）
- [x] M22-4 远端 `:mk` 创建并进入 + 远端白名单加 `mk` + 拒绝提示文本修正
- [x] M22-5 `:find`/`:grep` 作用远端视图（递归遍历 1 万上限；单文件 1 MiB 上限；结果虚拟目录复用）
- [x] M22-6 远端文件打开（临时副本→opener；进程退出清理；不入撤销栈）
- [x] M22-7 README 里程碑表补齐至 M21（状态与 PLAN 一致）

## M23 — Frecency 对齐增强与 Pane 模式（2026-11-17 增补；见 SPEC M23 节）

> 来源：DeepSeek 分享讨论蒸馏（对照 wideweide/frecency 差距分析 + Frecency Pane 模式设计）。讨论稿建议编号 M19 已被 SFTP 批次占用，按下一空闲号 M23 登记（见 NOTES）。顺序：1–5（评分外语义）→ 6/7（Pane 模式）→ 8（可 defer）→ 9～12（2026-11-18 追加）。

- [x] M23-1 多词搜索与匹配分级（Keyword/LastSegment/Substring 三级排序；ordered/any-order 多词；o/pick 共用）
- [x] M23-2 keyword unbook 保留累积历史（`:keyword -` 只清元数据；现状核对登记）
- [x] M23-3 容量清理与书签保护（per-topic max_entries 默认 1500；超限删最低分未书签项至 90%）
- [x] M23-4 IGNORE_PATTERNS（record 入口 glob 静默跳过；TOML 配置；不引新 crate）
- [x] M23-5 per-topic LAMBDA / MAX_ENTRIES 配置（[frecency] 段；unified 0.005 / dir-file 0.01；缺省兼容）
- [x] M23-6 Frecency Pane 模式（o 进入焦点 pane 内嵌搜索；对侧 PreviewRenderer 预览 100–200 行+截断标记；Esc 三级退出；键位见 M23-7 修订）
- [x] ~~M23-7 c/v/e 模式内重映射~~ **修订**：c/v 移除、键位与 Normal 一致（见 M23-9/10 与 SPEC M23-7 修订）
- [x] M23-8 选择器内 d 删除 + trash 表 + u 撤销（中优先级；超 3 轮即 deferred）
- [x] M23-9 远端文件跟踪上传（notify 监听副本；防抖 2s 自动上传；失败状态栏提示）
- [x] M23-10 y 复制序列与 e 编辑器（yn 名称/yf 全路径/yc 内容；目录/文件/多选；OSC52 优先）
- [x] M23-11 自定义命令快捷键与内置变量（%d/%f/%F/%D；,g lazygit 仓库判定优先级；terminal 分流复用）
- [x] M23-12 git 集成评估（git2/gitoxide 与 FM 集成点调研；决策入 NOTES，不实现）

> 2026-11-18 追加调整：M23-7 修订（cve 移除、Frecency 键位与 Normal 一致）+ 新增 M23-9～12（远端跟踪上传/y*/e/自定义命令/git 评估）。执行顺序：M23-9 → 10 → 11 → 12 → 6（含修订后的 7）→ 8。

## M24 — 交互增强与列表规范（2026-11-19 增补；见 SPEC M24 节）

> 来源：DeepSeek 对标分析蒸馏 + 用户列对齐要求。顺序：1（列对齐）→ 2（自然排序）→ 3/4（Vim 增强）→ 5（计数扩展）→ 6/7（命令增强）→ 8/9（对比/任务视图）。

- [x] M24-1 Pane 列对齐规范（文件名左对齐+填充、大小右对齐人性化、日期左对齐分钟精度；左右 pane 共用）
- [x] M24-2 自然排序（数字感知比较器 natural_cmp；S 对话框增 natural 字段）
- [x] M24-3 行可视模式 V（与 v 互斥；d/y/x 对选中区域执行；tag 集合复用）
- [x] M24-4 可视模式操作扩展（v/V 内 d 删除/y 复制路径/x 别名；VISUAL 计数指示）
- [x] M24-5 计数前缀扩展（Nj/Nk/NG/Ngg/Nh/Nl 消耗 InputState.count；Ndd/Nyy 批量）
- [x] M24-6 命令历史（: 模式 ↑/↓ 浏览；内存 Vec 上限 100；Enter 执行/Esc 退出）
- [x] M24-7 路径补全（: 模式 Tab 补全文件/目录名；前缀匹配循环；~ 展开 HOME）
- [x] M24-8 目录对比 :compare（按名+大小；结果虚拟目录 =/!/</> 前缀；只读）
- [x] M24-9 任务视图增强（:jobs j/k 选择/x 取消/Esc 关闭；Job cancel_requested）

## M25 — 会话持久化、marks、归档创建与 git 集成（2026-11-19 增补；见 SPEC §25）

> 来源：用户批次需求讨论稿蒸馏。marks 由 frecency 实现并持久化（决策反转见 NOTES）；git 走 M23-12 已评估的 CLI 子进程路线。顺序：1（会话）→ 2（marks）→ 3/4（归档）→ 5/6（git）→ 7（评估）。

- [x] M25-1 会话持久化（退出保存/启动恢复 tabs；物理 Location；配置目录 session.toml；损坏静默降级；[ui] session_restore）
- [x] M25-2a（原定义，已撤销）~~marks 独立模块（暂停）~~ **撤销原任务定义**：marks 改由 frecency 实现并持久化（见 AUDIT 之后重定义的 M25-2 条目；编号加 `a` 避免重复）
- [x] M25-3 归档创建（:archive zip|tar|tar.gz；纯 Rust 写端；tags/光标集合；入撤销栈；同名不覆盖；xz/bz2/zst 走 M26-4 外部 CLI）
- [x] M25-4 归档对话框与校验（无参交互对话框；校验文件默认 md5，sha256/none 可选；[archive] checksum）
- [x] M25-5 git 状态列（porcelain 缓存映射 + 着色列；仓库根探测；git CLI 子进程零编译期依赖；[ui] git_column）
- [x] M25-6 git 状态栏 + gi（分支 + diff 摘要缓存；PATH 优先级探测 gitui→ziggity→lazygit 挂起启动；:gitui 同语义）
- [x] M25-7 frecency 通用化评估（命令历史/路径补全迁移可行性 + 命令书签；决策入 NOTES，不实现）

## AUDIT — 评估报告整改（2026-11-19 增补；见 SPEC AUDIT 段 / docs/REVIEW.md，**已确认**）

> 来源：全面评估报告（第二轮复核新增 AUDIT-13～17），用户已确认批次（2026-11-19）。执行顺序：AUDIT-1 → AUDIT-14 → AUDIT-9 → AUDIT-15 → AUDIT-7/13 → AUDIT-2/3/4/16 → 其余。AUDIT-1 方案（pending 合并移出 pick / 只读 open）已拍板，见 NOTES §10。

- [x] AUDIT-1 pick 只读性修复（pending 合并移出 pick 或只读 open；写 tick 回归测试，见 SPEC AUDIT-1；2026-11-20 完成：redb ReadOnlyDatabase，合并归 record/TUI 启动）
- [x] AUDIT-2 PDF 视觉探测链最小接线（第 1 页：exprim → pdftoppm/mutool → 提示，或文档降级；1–2 页与 prefer 配置由 M27-3 扩展，见 SPEC AUDIT-2；2026-11-20 完成：`cached_visual_png` 结果缓存 + viewer 接线 ImageEngine）
- [x] AUDIT-3 pick 预览同源接线或撤回承诺（见 SPEC AUDIT-3；2026-11-20 完成：pick 交互式选择器接同源预览窗格——render_preview/ImageEngine，窄终端降级单栏）
- [x] AUDIT-4 惰性 compaction（TUI 启动阈值 5000 触发，pick 不触发，见 SPEC AUDIT-4；2026-11-20 完成：`lazy_compact_on_startup` 钩子 + `compact_store` 核心抽出，手动命令共用）
- [x] AUDIT-5 app.rs 按 keys/exec/render/jobs/preview 拆分，测试随迁（见 SPEC AUDIT-5；2026-11-20 完成：15261 行上帝文件 → `src/app/` 7 模块，测试独立 `tests.rs`，零行为变化 350+4 全绿）
- [x] AUDIT-6 多键序列可配置超时（见 SPEC AUDIT-6；2026-11-20 完成：`[keymap] seq_timeout_ms`（缺省 500）+ run 循环待决 seq 超时清空——原实现无超时、被弃前缀吞后续键）
- [x] AUDIT-7 opener 整层接线（TOML 加载 + mime/name + prepend/append/rules 三层 + 命令执行；见 SPEC AUDIT-7；2026-11-20 完成：`open_with_layers` 生产接线 `open_selected`，block=true 经 pending_external 挂起执行）
- [x] AUDIT-8 生产 panic 面清理与错误上浮（见 SPEC AUDIT-8；2026-11-20 完成：生产 unwrap/expect/unreachable 清零，fs-change 自动刷新失败上浮状态栏）
- [x] AUDIT-9 CI 门禁增强（clippy all-targets + Linux cross-check 入 CI，见 SPEC AUDIT-9；2026-11-20 完成：ci.yml 双步增补，cross-check 仅 ubuntu job——host triple 即该 target）
- [x] AUDIT-10 统一 glob 匹配（含 opener `*x*` 缺陷）与配置失败回退实现（见 SPEC AUDIT-10；2026-11-20 完成：删除 opener 私有 glob_match_simple，统一 tab::glob_match，`*x*` 双星测试）
- [x] AUDIT-11 help.txt 与 README 功能同步（`:find`/`:grep`、marks/主题/opener 状态、keymap 路径；见 SPEC AUDIT-11；2026-11-20 完成：help 补 :find/:grep/:theme/:sort/S/:remote/:!/:V/y 序列 + keymap.toml→config.toml 修正；README 12 处状态/示例同步）
- [x] AUDIT-12 keymap/state/session/CLI 子命令分派测试补强（见 SPEC AUDIT-12；2026-11-20 完成：keymap 分层合并/replace、state 作业回包、CLI maintenance 臂；顺带修复 Store::open 自动创建数据目录——全新安装 record/compact 不再失败）
- [x] AUDIT-13 主题选择接线（内置/用户 TOML/`:theme`/明暗与图标对齐；见 SPEC AUDIT-13；2026-11-20 完成：`[ui] theme` 启动激活 + `:theme` 运行时切换；ACTIVE 升级 RwLock 修复二次切换静默失效）
- [x] AUDIT-14 测试环境隔离（`TMUX`/`STY` 依赖修复；见 SPEC AUDIT-14；2026-11-20 完成：`run_external_target_with` 注入路由 + 复用器输出静音）
- [x] AUDIT-15 pick CLI 契约（`--topic`、文件 topic、数据目录缺失、与预览接线；见 SPEC AUDIT-15；2026-11-20 完成：`--topic dir|file` 对称评分、缺失库空评分降级、CLI 集成测试）
- [x] AUDIT-16 frecency 一致性（compaction per-topic λ、pick 初始化设置；见 SPEC AUDIT-16；2026-11-20 完成：replay_merge 去硬编码 λ、cmd_pick init_settings，对照测试）
- [x] AUDIT-17 归档成员读取上限（超限可读错误、不 OOM；见 SPEC AUDIT-17；2026-11-20 完成：64MB 成员守卫 + xz 256MB 限长解码，端到端炸弹测试）
- [x] AUDIT-18 本机测试失败整改（m249 作业行 70 列断言依赖宿主路径长度；m253 第二次 undo 未移除归档；均 HEAD 复现、与 AUDIT-1 无关，见 NOTES 2026-11-20；同日完成：m249 改固定短标签、m253 撤销前释放 ZipFs 句柄，全量 328 单测无 skip 双环境绿）

## M25-2（重定义）— marks 由 frecency 实现（2026-11-19 用户指令；见 SPEC §25）

> 原「独立模块 + 暂停」定义撤销（上方已划掉）。数据记录与导航均由 frecency 模块实现，marks/bookmark 统一持久化在 frecency redb；执行顺序排在 AUDIT 之后（用户已确认）。keyword 按长度分工（单字符 marks 导航 / 多字符搜索高优先级匹配）已确认，见 SPEC §25。

- [x] M25-2 marks/bookmark 由 frecency 实现（`m<char>` / `'<char>` + vifm 对齐 `:bmark` / `:bmarks` / `:bmgo` / `:delbmarks`，`:keyword` 保留兼容；keyword 持久化 redb + 频率记录；单字符 marks 导航 / 多字符 `o`/`:j`/`:J` 搜索高优先级匹配；2026-11-20 完成：quickjump 扩展 file 语义，bmark 命令族共用 keyword 数据）

## M26 — 归档创建扩展与外部工具降级（2026-11-19 增补；见 SPEC §26，已确认启动）

> 来源：用户讨论稿蒸馏（纯 Rust 内建 + 外部 CLI 降级），与 M25-3/4 现状合并评估择优（命令名保持 :archive、否决 ArchiveWriter trait、采纳后台作业化、纯 Rust 编码器经 crates.io 核实仅 7z 可采——见 NOTES）。顺序：1（Job 化）→ 2（7z 创建）→ 3（7z 浏览）→ 4（外部 CLI）→ 5（文档收尾）。

- [x] M26-1 :archive 后台作业化（迁移 M13-8 Job 模型；大目录不阻塞 UI；完成回包入撤销栈；:jobs 可见可取消；2026-11-20 完成：archive_plan/execute_archive_job 分离，SpawnJob 与对话框双入口共用 spawn_job_from_plan）
- [x] M26-2 7z 内建创建（sevenz-rust 0.6 写端；ArchiveFormat::SevenZ 分派；校验文件与对话框格式扩展；2026-11-20 完成：pack_7z 递归打包，roundtrip 读端测试，对话框 4 格式循环）
- [x] M26-3 7z 只读浏览升级（M7-9 的"解压提示"→ ArchiveFs 新后端 SevenZFs；损坏/加密可读错误；2026-11-20 完成：open_dispatch/is_archive_file 更新，roundtrip+损坏+空归档测试，FIX-3 测试改写 .rar/.7z 双语义）
- [x] M26-4 外部 CLI 归档降级（rar/cab/lzh/tar.xz/tar.bz2/tar.zst：PATH 探测 7z/rar → Command 调用 → 缺失优雅终止；输出抑制；无半成品；入撤销栈；2026-11-20 完成：cli_tool_candidates/cli_create_args/pack_cli，纯函数测试 + SpawnJob 形态断言）
- [x] M26-5 README 归档创建段（内建/外部/不支持格式行为表；libarchive_oxide 列观察项；2026-11-20 完成：新增"归档创建"行为表，对比表/状态句/frecency 段同步）

## M27 — 0.8.0 实测问题与 vifm 视觉对齐（2026-11-19 增补；见 SPEC §27，**已确认，按序排队**）

> 来源：用户 0.8.0 实测反馈（符号链接目录、vifm 配色、PDF 首页图像）。用户 2026-11-19 确认撤销「暂不启动」标记，按 TASKS 顺序排队执行（AUDIT → M25-2 → M26 → M27）。依赖关系：M27-1 无前置；M27-2 依赖 AUDIT-13（主题接线）；M27-3 依赖 AUDIT-2（最小链接线）。

- [x] M27-1 符号链接目录 l/Enter 内部进入（跟随目标判定 is_dir、保留 is_link；断链可读；目录优先/树/预览一致；见 SPEC M27-1；2026-11-20 完成）
- [x] M27-2 vifm 风格布局与配色（默认去 pane 边框，TabLine/TopLine/StatusLine/Border/CurrLine/OtherLine；`[ui] pane_borders`；见 SPEC M27-2；2026-11-20 完成：vifm_style/vifm_fallback 16 色回退、pane_gap_area 分隔线、双模式渲染）
- [x] M27-3 PDF 前 1–2 页图像预览可配置（`[preview] pdf_image_pages` / `pdf_prefer_image`；失败回退文本；见 SPEC M27-3；2026-11-20 完成：中断提交验证修正 spawn 注入/切换清理/提示分支 + M27-1 lstat 误判与 AUDIT-7 E0369 潜伏缺陷修复，见 NOTES）

## M28 — 运维操作补齐（2026-11-20 增补；见 SPEC M28 节，已确认登记）

> 来源：2026-11-20 评估报告蒸馏（P1），用户确认。对标 ranger/Total Commander/Double Commander/vifm。顺序：1（链接）→ 2（重命名）→ 3（任务视图）→ 4（文档收尾）。

- [x] M28-1 `:link` 符号链接创建（tags/选择优先；Windows 目录 junction 回退；入撤销栈；见 SPEC M28-1；2026-11-20 完成（补勾）：undo_op symlink 感知修复）
- [x] M28-2 `:rename` 结构化批量重命名（s///、prefix/suffix/seq；预览虚拟目录；整批撤销；见 SPEC M28-2；2026-11-20 完成（补勾）：RenamePreview 结果视图 + y 应用/Esc 取消，逐项撤销）
- [x] M28-3 `:jobs` 任务视图增强（`p` 暂停/恢复 + Enter 详情覆盖层 + 状态列；见 SPEC M28-3；2026-11-20 完成：JobControls 束 + JobProgress 采样 + JobDetail 覆盖层，396 单测）
- [x] M28-4 M28 文档收尾（help/README/ACCEPTANCE §28 同步；见 SPEC M28-4；2026-11-20 完成，批次 4/4 关闭）

## M29 — 对比增强（2026-11-20 增补；见 SPEC M29 节，已确认登记）

> 来源：同上（P1）。依赖 M24-8（已完成）。顺序：1（bycontents）→ 2（dp/do）→ 3（文档收尾）。

- [x] M29-1 `:compare bycontents` 内容对比（SHA-256；64MB 护栏；虚拟目录前缀；见 SPEC M29-1；2026-11-20 完成：compare_panes by_contents + 分块哈希，398 单测）
- [x] M29-2 对比结果 `dp`/`do` 同步（入撤销栈；`!` 覆盖同步裁剪为可读拒绝；操作后重建；见 SPEC M29-2；2026-11-20 完成：DiffPut/DiffObtain Action + compare_sync，399 单测）
- [x] M29-3 M29 文档收尾（help/README/ACCEPTANCE §29 同步；见 SPEC M29-3；2026-11-20 完成，批次 3/3 关闭）

## M30 — frecency 通用化（2026-11-20 增补；M25-7 评估立项；见 SPEC M30 节，已确认登记）

> 来源：M25-7 评估（NOTES：可行、迁移成本低）+ 用户指令"对齐同级 frecency 项目最近更新"（对齐基线见 SPEC M30：topic=cmd、无分数下限清理、书签=keyword 事件、pick 只读）。顺序：1（schema+record）→ 2（历史迁移）→ 3（补全加权）→ 4（命令书签）→ 5（文档收尾）。

- [x] M30-1 cmd 事件 schema 与 record 链路（EntryKind::Command、IGNORE_PATTERNS 扩展 cmd、cmd 档配置；见 SPEC M30-1；2026-11-20 完成：META_CMD_TICK + lambda_cmd + record_command_line，404 单测）
- [x] M30-2 `:` 命令历史迁移（↑/↓ replay 顶层 N + 会话合并、跨会话持久；见 SPEC M30-2；2026-11-20 完成：cmd_history_ranked + InputMode::Command.ranked 快照，405 单测）
- [x] M30-3 路径补全加权（frecency 分数优先、字典序兜底、缺失零成本降级；见 SPEC M30-3；2026-11-20 完成：path_scores + completion_candidates，406 单测）
- [x] M30-4 命令书签（:cmdmark + `:` 内 `'` 展开；unbook 保留历史；见 SPEC M30-4；2026-11-20 完成：cmd_bookmark + cmdmark 臂 + cmd_mark_pending 态，408 单测）
- [x] M30-5 M30 文档与隐私收尾（README/help 明示命令文本入库 + IGNORE_PATTERNS 排除；见 SPEC M30-5；2026-11-20 完成，批次 5/5 关闭）

## M31 — 文档与验收补齐（2026-11-20 增补；见 SPEC M31 节，已确认登记）

> 来源：评估报告第四（现有功能提升）/第五（横切建议）节落地。顺序：1（README）→ 2（验收条目）→ 3（M11-6 清单）→ 4（复选框化）→ 5（pdf-visual 实测）。

- [x] M31-1 README 状态与边界同步（M26 行漂移修正、.7z 加密/分卷边界、Windows UAC 边界；见 SPEC M31-1；2026-11-20 完成）
- [x] M31-2 ACCEPTANCE 补条目（:jobs 取消时延、外部 CLI 双环境验收；见 SPEC M31-2；2026-11-20 完成）
- [x] M31-3 M11-6 手工验证清单（ACCEPTANCE 独立小节；见 SPEC M31-3；2026-11-20 完成：6 步清单，M11-6 勾选以清单补验为准）
- [x] M31-4 ACCEPTANCE M0–M13 复选框化（零语义机械转换 + grep 核验；见 SPEC M31-4；2026-11-20 完成：137 条）
- [~] M31-5 pdf-visual 护栏实测（rustc≥1.97 环境语料耗时/内存数据入 NOTES；见 SPEC M31-5；deferred——2026-11-21 起本机 rustc 1.100 已解锁，语料实测待运行）

## M32 — 远端健壮性与会话扩展（2026-11-20 增补；见 SPEC M32 节，已确认登记）

> 来源：评估报告 P2 蒸馏补登（用户确认）。依赖 M19–M20（已完成）、M25-1（已完成）。顺序：1（重连）→ 2（续传）→ 3（会话扩展）→ 4（文档收尾）。

- [x] M32-1 SFTP 连接健康检查与自动重连（连接类错误分类复用；操作触发式重连一次；不阻塞事件循环；见 SPEC M32-1；2026-11-20 完成：remote_with_reconnect + is_connection_class + as_any downcast，411 单测）
- [x] M32-2 大文件传输断点续传（.part + 偏移；完成原子改名；显式取消清理；见 SPEC M32-2；2026-11-20 完成（补勾）：连接类失败保留 .part 现场 + 自动重连重试，偏移续传裁剪为 SIMPLIFIED）
- [x] M32-3 会话持久化范围扩展（Archive/Remote Location 保存恢复；失败降级可读；见 SPEC M32-3；2026-11-20 完成：SessionTab.kind + restore_session_with_hints，416 单测）
- [x] M32-4 M32 文档收尾（help/README/ACCEPTANCE §32 同步；见 SPEC M32-4；2026-11-20 完成，批次 4/4 关闭）

## M33 — 键位模式分层扩展（2026-11-20 增补；见 SPEC M33 节，已确认登记）

> 来源：评估报告 P2 蒸馏补登（用户确认）；README"仅 [normal] 可配"限制的补齐。依赖 GAP-1（已完成）。顺序：1（解析合并）→ 2（消费接线）→ 3（文档收尾）。

- [x] M33-1 `[visual]`/`[view]`/`[command]` 三层 TOML 解析与合并（与 [normal] 同构；缺省空；见 SPEC M33-1；2026-11-20 完成：ModeLayers + parse_mode_layers_standalone，417 单测）
- [x] M33-2 模式化消费接线（visual/view 切换换表；未配置零行为变化；[command] 消费面裁剪见 NOTES；见 SPEC M33-2；2026-11-20 完成：switch_mode_map + with_mode_maps，418 单测）
- [x] M33-3 M33 文档收尾（help/README 键位段 + config 示例；见 SPEC M33-3；2026-11-20 完成，批次 3/3 关闭）

## M34 — 主题、预览统一与文档（2026-11-21 增补；见 SPEC M34 节，✅ 已完成 10/10）

> 来源：DeepSeek 讨论蒸馏（2026-11-20 分享稿，用户实测反馈）。登记前经代码现状核查（2026-11-21）：`pick` 双栏预览（AUDIT-3 已覆盖，≥60 列双栏窄端降级）与 dracula/gruvbox 内置可用（opaline 0.4.2 内置 39 主题）不重复登记；右 pane `/` 不可用为 M14 SIMPLIFIED 裁剪（非回归）；核查另发现 `preview_path` 焦点缺陷并入 M34-4。用户裁决（2026-11-21）：预览统一取方案 A（对侧预览）；二进制信息面板裁剪 `--version` 探测（见 NOTES）。顺序：1（默认主题）→ 2（snowwhite）→ 3（右栏搜索）→ 4（预览统一）→ 5（frecency 视觉回显）→ 6（jar）→ 7（二进制信息面板）→ 8（GUIDE）→ 9（预览尺寸适配，2026-11-21 追加）→ 10（文档收尾）。

> ✅ 全部完成（2026-11-21，10/10；0.11.0 已发布）。

- [x] M34-1 默认主题 gruvbox（`[ui] theme` 缺省 `gruvbox-dark`；vifm 高亮组映射核查补充；dracula 验证；见 SPEC M34-1；2026-11-21 完成：`vifm_style` 三层解析——主题样式 → 标准 token 推导（accent/bg/text 词表，缺 token 整组回退）→ 16 色；gruvbox 色值断言 + dracula activate/推导验证；README/help 默认值同步，422 单测）
- [x] M34-2 snowwhite 内置主题移植（vifm-colors 颜色 → opaline TOML 捆绑；见 SPEC M34-2；2026-11-21 完成：捆绑层 `BUNDLED_THEMES`（include_str）+ activate 内置→捆绑→用户三段、`:theme` 列表并入；vifm 高亮组精确值直写命名样式（fg16/bg255、TopLineSel 反转），selected/active_selected=208 橙面，浅色 variant + 图标 Light 联动；424 单测）
- [x] M34-3 右 pane `/` 搜索（focus 感知分派，解除 M14 SIMPLIFIED；见 SPEC M34-3；2026-11-21 完成：/·n·N 右焦点拦截共用 Search 链路，origin/恢复取焦点 pane 光标，`Tab::search_move`/`search_status` focus-aware，`render_right_pane` 增 mode 参数做同款匹配高亮，`Tab::find_match` 死代码删除（统一走 `find_match_in`）；426 单测）
- [x] M34-4 对侧预览统一（preview_path 焦点修复 + :view/frecency 共用对侧模型，方案 A；见 SPEC M34-4；2026-11-21 完成：`preview_path` 焦点感知修复右焦点预览错位 + 查看器渲染抽出 `render_view_content`（`render_right_pane`/`render()` 双侧分派——焦点右时查看器占左 pane、右 pane 保持列表导航）；frecency 对侧语义不动；428 单测）
- [x] M34-5 frecency Pane 视觉与回显（query 底部回显 + 图标全路径行 + 标题文案；见 SPEC M34-5；2026-11-21 完成：标题固定"Frecency - 高频目录/文件搜索"，行=devicons 图标+全路径（目录条目尾斜杠触发文件夹图标）替换 `[目录]/[文件]`，✦keyword 保留，搜索态底部 `/query` 实时回显、计数移至提示行；429 单测）
- [x] M34-6 `.jar` 归档识别（zip 后端复用；见 SPEC M34-6；2026-11-21 完成：is_archive_file + open_dispatch 双表同步 `.jar` → ZipFs——roundtrip 测试先暴露只改一表的漏接（"不支持的归档格式"），浏览/预览/搜索链路自动生效，README 浏览列表/对比段同步；430 单测）
- [x] M34-7 二进制/不可读预览信息面板（大小/时间/类型推断；不执行文件；见 SPEC M34-7；2026-11-21 完成：`file_info_text`（魔数嗅探 10 类 + 扩展名兜底 + 大小/修改时间，有界 1KB 读取、零子进程）替代 `[二进制文件]` 单行与 lossy 乱码；查看器读取失败分支同源接入；431 单测）
- [x] M34-8 docs/GUIDE.md 用户手册（快速上手 + 键位/命令速查；见 SPEC M34-8；2026-11-21 完成：13 节——安装/5 分钟快速上手（浏览/操作撤销/归档/frecency）/浏览/文件操作/选择批量/搜索/归档/跳转/视图标签/远端/配置/键位速查表/命令速查表，内容以 help.txt+README 现状为准；README 指针随 M34-10 收尾加入）
- [x] M34-9 图片/PDF 预览尺寸适配（小于窗格原尺寸居中不放大、超过 fit-width；见 SPEC M34-9；2026-11-21 完成：`ImageEngine::render_file` 单点三分支——Fit 天然不放大 + 窗格内居中 / fit-width 纵向居中 / `SlicedImage` 顶部切片不越界；查看器图片/PDF 页/pick 同源生效；433 单测）
- [x] M34-10 M34 文档收尾（help/README/ACCEPTANCE §34 同步；见 SPEC M34-10；2026-11-21 完成：ACCEPTANCE §34 逐条勾选、README 状态句/GUIDE 指针/预览语义、help `w` 行预览语义、PLAN/STATE 批次关闭 → 0.11.0 自动发布触发，433 单测）

## 杂项定稿任务（2026-11-19 用户确认，见 SPEC §6.1）

- [x] PROC-1 预算门控与断点落地协议（已写入 AGENTS §3 + ACCEPTANCE §1；无代码改动；用户 2026-11-19 复核蒸馏准确性后确认生效）

- [x] MISC-1 提供 `rv` 短别名二进制（与 `rvifm` 并存；项目名/crate/主二进制名不变；Cargo `[[bin]]` + 安装产物 + README 说明；2026-11-20 完成：双 [[bin]] 同 main.rs、rv test=false、CI 归档双二进制、冒烟测试）

## M35 — 文档与发布一致性（2026-11-21 增补；见 SPEC M35 节，已确认登记并执行）

> 来源：v0.11.0 复核报告蒸馏（H-1~H-6 + R-12），用户指令"蒸馏并执行"。顺序：1（漂移清零）→ 2（脚本）→ 3（REVIEW 归档）→ 4（CHANGELOG）→ 5（卫生）。

- [x] M35-1 状态漂移清零（AGENTS §10 / SPEC 7z-rar 三处 / STATE-TASKS-PLAN-README 状态句；见 SPEC M35-1；2026-11-21 完成——蒸馏时一并落笔）
- [x] M35-2 `scripts/harness_check.py` + CI job + pre-commit 示例（七条规则；见 SPEC M35-2；2026-11-21 完成：正反向验证 + R6 顺带抓出 433→437 漂移）
- [x] M35-3 REVIEW 归档（旧报告移 docs/reviews/，主文只留当前结论 + 索引；见 SPEC M35-3；2026-11-21 完成）
- [x] M35-4 CHANGELOG.md（0.8.0 → 0.11.0 用户可见变化 + 版本跳过说明；见 SPEC M35-4；2026-11-21 完成）
- [x] M35-5 依赖与计数卫生（移除 tokio-stream / 清 DBG parsed / 数字脚本校验；见 SPEC M35-5；2026-11-21 完成）

## M36 — 传输引擎（2026-11-21 增补；见 SPEC M36 节，已确认登记·按序执行）

> 来源：同上。目标 0.12.0。顺序：1（copy_tree）→ 2（下载单读）→ 3（范围读写）→ 4（Transfer 状态机）→ 5（冲突策略）→ 6（偏移续传）→ 7（:queue）→ 8（递归远端）。

- [x] M36-1 `copy_tree` 重写（mode/mtime/symlink/进度/取消；见 SPEC M36-1；2026-11-21 完成：CopyControls + copy_file_chunked + preserve_dir_times，move_path 同享，436 单测）
- [x] M36-2 远端下载单次读取 + 临时文件 + 原子 rename（见 SPEC M36-2；2026-11-21 完成：CountingMockFs 断言 read==1 + mtime 保留，437 单测）
- [x] M36-3 `RemoteFs` 范围读写 read_range/write_range（默认实现兼容 mock；见 SPEC M36-3；2026-11-21 完成：trait 默认 + SftpCmd 变体 + actor seek 臂，437 单测）
- [x] M36-4 `Transfer` 状态机 + 统一进度（复用 Job/JobProgress/JobControls；见 SPEC M36-4；2026-11-21 完成：execute_transfer_body Upload/Download + m364 三断言，439 单测；Paste 后台路由随 M36-5/6/7 统一迁移）
- [ ] M36-5 冲突策略 Ask/Overwrite/Skip/RenameAuto/KeepNewer（应用到全部；见 SPEC M36-5）
- [ ] M36-6 偏移续传（.part + .part.meta + write_range；见 SPEC M36-6）
- [ ] M36-7 `:queue` 传输队列（见 SPEC M36-7）
- [ ] M36-8 递归远端目录传输（三方向 + 链接策略；见 SPEC M36-8）

## M37 — 文件运维（2026-11-21 增补；见 SPEC M37 节，已确认登记·按序执行）

> 来源：同上。目标 0.13.0。顺序：1（:chmod）→ 2（:extract）→ 3（:sync）→ 4（:du）→ 5（:dup）→ 6（:checksum）→ 7（暂存区）。

- [ ] M37-1 `:chmod <mode>` 批量属性（八进制/符号、递归；POSIX only；见 SPEC M37-1）
- [ ] M37-2 `:extract` 归档解压（选中/全部、zip-slip 防护、job 化、u 可逆；见 SPEC M37-2）
- [ ] M37-3 `:sync` 目录同步（dry-run、三策略、删除显式开关；见 SPEC M37-3）
- [ ] M37-4 `:du` 磁盘占用视图（并行、可取消、top-N；见 SPEC M37-4）
- [ ] M37-5 `:dup` 查重（三段哈希、硬链接识别、结果可操作；见 SPEC M37-5）
- [ ] M37-6 `:checksum` 生成/校验（并行、sidecar 兼容；见 SPEC M37-6）
- [ ] M37-7 暂存区增强（容量/清空/按时间清理/恢复到指定路径；见 SPEC M37-7）

## M38 — 检索与 shell 集成（2026-11-21 增补；见 SPEC M38 节，已确认登记·按序执行）

> 来源：同上。目标 0.13.x。顺序：1（结果可操作）→ 2（pick 兼容）→ 3（init）→ 4（frecency 管理）→ 5（find 过滤器）。

- [ ] M38-1 `:find`/`:grep` 结果可操作（yy/dd/p/:tag/:rename 映射真实路径；见 SPEC M38-1）
- [ ] M38-2 `pick` 兼容面（--query/--multi/--print0/--preview/--height/--no-sort/--exit-0/--select-1；见 SPEC M38-2）
- [ ] M38-3 `rvifm init bash`/`zsh`/`fish`/`powershell`（见 SPEC M38-3）
- [ ] M38-4 `:frecency stats`/`export`/`purge`/`prune`（见 SPEC M38-4）
- [ ] M38-5 `:find` 过滤器（size/mtime/type/depth）+ 命名搜索（见 SPEC M38-5）

## M39 — 预览与平台（2026-11-21 增补；见 SPEC M39 节，已确认登记·按序执行）

> 来源：同上。目标 0.14.0。顺序：1（hex）→ 2（CSV/PDF 翻页/EXIF）→ 3（zip 容器）→ 4（SFTP 硬化）→ 5（UAC）→ 6（标签布局）。

- [ ] M39-1 十六进制查看器（偏移跳转、ASCII 侧栏、分块；见 SPEC M39-1）
- [ ] M39-2 CSV/TSV 预览、PDF 翻页、图片 EXIF 摘要（见 SPEC M39-2）
- [ ] M39-3 通用 zip 容器识别（docx/xlsx/pptx/apk/war；见 SPEC M39-3）
- [ ] M39-4 SFTP 硬化（指纹确认 ask 缺省、ProxyJump、agent、keepalive；见 SPEC M39-4）
- [ ] M39-5 Windows 操作级 UAC（ShellExecuteEx runas；见 SPEC M39-5）
- [ ] M39-6 命名标签、溢出滚动、布局选项（见 SPEC M39-6）

## M40 — 1.0 稳定化（2026-11-21 增补；见 SPEC M40 节，已确认登记·按序执行）

> 来源：同上。目标 1.0.0。顺序：1（基准）→ 2（fuzz）→ 3（proptest）→ 4（覆盖率）→ 5（供应链）→ 6（平台）→ 7（发布）。

- [ ] M40-1 criterion 性能基准（启动/10 万项/排序/replay/1GB 吞吐；见 SPEC M40-1）
- [ ] M40-2 cargo-fuzz 模糊测试（归档解析/路径规范化/反序列化；见 SPEC M40-2）
- [ ] M40-3 proptest 属性测试（glob/自然排序/撤销栈；见 SPEC M40-3）
- [ ] M40-4 覆盖率 ≥75%（核心模块；见 SPEC M40-4）
- [ ] M40-5 供应链（cargo-deny/audit/SBOM/SHA256；见 SPEC M40-5）
- [ ] M40-6 macOS CI + aarch64 Linux 归档（见 SPEC M40-6）
- [ ] M40-7 1.0.0 发布（CHANGELOG 完整 + crates.io；见 SPEC M40-7）

## 排队缺口（2026-11-15 M16 实施中发现，见 NOTES 同日条目）

- [x] GAP-1 M10-5 收尾：keymap TOML 分层接线（config.rs 解析 prepend/append/replace 并合并进 InputState.keymap——当前仅默认表，用户绑定不可用；NAV-3 的推荐映射示例依赖此项）
- [x] GAP-2 M8 收尾：TUI 退出时批量追加会话访问事件（SPEC §4.6"退出时批量追加"未实现——会话访问退出即丢，drain 各 tab history → append_events；NAV-4 的会话合并只覆盖即时可见性）

## 不在任务清单中的事项

以下内容明确不实现，不要为它们创建任务：

- Vim 脚本引擎、插件系统
- FUSE / `archivemount`
- fzf / SQLite
- 完整 Vim 引擎
- 归档内写入、嵌套归档递归浏览
- 内嵌视频播放
- PDF 渲染引擎作为默认依赖（`exprim` 仅限非默认 `pdf-visual` feature；`tdf` / `pdfium-render` 完全禁止）
