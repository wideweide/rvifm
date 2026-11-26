# TASKS — rvifm 任务清单

**本文件是唯一任务来源。** 每轮只取第一个未勾选任务，完成后勾选并更新 [STATE.md](STATE.md)。

- 任务粒度：一次提交能完成。
- **条目 = 一行摘要 + 状态标记 + SPEC 章节引用；不放完整需求原文**（见 AGENTS.md §8）。
- 提交信息：`<task-id>: <summary>`，例如 `M0-1: bootstrap cargo project`。
- 同一任务 3 轮不过验收：缩小范围，在 [NOTES.md](NOTES.md) 记录，标记 `deferred`，继续下一个。
- 任务对应的验收标准见 [ACCEPTANCE.md](ACCEPTANCE.md)。
- 需求边界见 [SPEC.md](SPEC.md)。

图例：`[ ]` 待做，`[x]` 完成，`[~]` 进行中/暂停（不参与“取第一个未勾选任务”），`deferred` 已延期。
节排序：按编号升序（2026-11-19 重排）；AUDIT 为已确认的 P0 整改批次，置于 M26 之前以保证其为第一个未勾选任务（2026-11-19 用户确认）；M41 为用户实测缺陷修复批次（2026-11-21），优先级排高，置于 M38 之前以保证其为第一个未勾选任务（用户指令）；"排队缺口"为补充段置于末尾。

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
- [x] M31-5 pdf-visual 护栏实测（rustc≥1.97 环境语料耗时/内存数据入 NOTES；见 SPEC M31-5；2026-11-21 完成：examples/pdf_bench 工具 + 本机实测数据入 NOTES——1224×1584px≈194 万像素护栏内、22–23ms/页、CJK 8ms、损坏 0ms 拒绝不 panic）

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
- [x] M36-5 冲突策略 Ask/Overwrite/Skip/RenameAuto/KeepNewer（应用到全部；见 SPEC M36-5；2026-11-21 完成：resolve_conflict 纯函数 + [transfer] conflict_policy + paste 接线，443 单测）
- [x] M36-6 偏移续传（.part + .part.meta + write_range；见 SPEC M36-6；2026-11-21 完成：upload_chunked + XferPartMeta + 头哈希校验，443 单测；断网循环集成验证人工）
- [x] M36-7 `:queue` 传输队列（见 SPEC M36-7；2026-11-21 完成：入队判定 + 覆盖层 + run 循环消费，443 单测）
- [x] M36-8 递归远端目录传输（三方向 + 链接策略；见 SPEC M36-8；2026-11-21 完成：walk_remote + RecurseUpload 方向 + paste 目录路由，443 单测）

## M37 — 文件运维（2026-11-21 增补；见 SPEC M37 节，已确认登记·按序执行）

> 来源：同上。目标 0.13.0。顺序：1（:chmod）→ 2（:extract）→ 3（:sync）→ 4（:du）→ 5（:dup）→ 6（:checksum）→ 7（暂存区）。

- [x] M37-1 `:chmod <mode>` 批量属性（八进制/符号、递归；POSIX only；见 SPEC M37-1；2026-11-21 完成：parse_symbolic_mode + 递归收集，444 单测）
- [x] M37-2 `:extract` 归档解压（选中/全部、zip-slip 防护、job 化、u 可逆；见 SPEC M37-2；2026-11-21 完成：ArchiveFs walk + zip-slip 防护 + 逐成员撤销，445 单测；job 化随 M36-4 迁移）
- [x] M37-3 `:sync` 目录同步（dry-run、删除显式开关；见 SPEC M37-3；2026-11-21 完成：l2r/r2l + --apply/--delete + dry-run 缺省，446 单测；三策略 deferred——复选框漂移修正 2026-11-21，STATE/NOTES 均记 M37 7/7 关闭，见 NOTES）
- [x] M37-4 `:du` 磁盘占用视图（并行、可取消、top-N；见 SPEC M37-4；2026-11-21 完成：10k 上限顺序遍历 + top-5，449 单测）
- [x] M37-5 `:dup` 查重（三段哈希、硬链接识别、结果可操作；见 SPEC M37-5；2026-11-21 完成：SHA-256 精确比对 + 分组报告；inode 裁剪）

- [x] M37-6 `:checksum` 生成/校验（并行、sidecar 兼容；见 SPEC M37-6；2026-11-21 完成：md5 sidecar md5sum 兼容）
- [x] M37-7 暂存区增强（容量/清空/按时间清理/恢复到指定路径；见 SPEC M37-7；2026-11-21 完成：:trash clear/purge，450 单测；容量配置与恢复到指定路径裁剪）

## M41 — 右栏一致性与交互缺陷修复（2026-11-21 增补；见 SPEC M41 节，用户实测缺陷·优先级排高）

> 来源：用户 0.11.x 实测反馈（2026-11-21）。优先级排高（用户指令），置于 M38 之前以保证其为第一个未勾选任务（节序先例同 AUDIT）；执行顺序 1→4。登记前已逐项代码定位（见 NOTES）。

- [x] M41-1 右 pane `:` 命令模式 + `gt`/`gT`/`Ngt` 标签切换（与左 pane 一致；见 SPEC M41-1；2026-11-21 完成：右分支拦截 `:`→EnterCommand + `g` 起始 seq 复用左 pane 机制，ToTop 臂焦点化；单键 `g` 直跳退役升格 `gg`，422 单测）
- [x] M41-2 方向键导航 `↑↓←→` = k/j/h/l 等价动作（左/右 pane 一致、声明式可覆盖；见 SPEC M41-2；2026-11-21 完成：别名进同款硬编码臂（环绕/计数/..行 一致）+ 默认 keymap 注册 + 树视图守卫扩展，424 单测）
- [x] M41-3 鼠标点击焦点与高亮双侧一致（点击切焦点 + 光标高亮；见 SPEC M41-3；2026-11-21 完成：handle_mouse 双 pane 几何 + hit_right_index，滚轮焦点化，右键菜单保持左栏，424 单测）
- [x] M41-4 视口滚动钉底修复（k 上移列表不动、高亮上移；见 SPEC M41-4；2026-11-21 完成：list_offset_sticky 有状态化 + sync_viewport_scroll 每帧落盘 Tab/Pane.scroll，渲染/命中测试同源，426 单测）

## M38 — 检索与 shell 集成（2026-11-21 增补；见 SPEC M38 节，已确认登记·按序执行）

> 来源：同上。目标 0.13.x。顺序：1（结果可操作）→ 2（pick 兼容）→ 3（init）→ 4（frecency 管理）→ 5（find 过滤器）。

- [x] M38-1 `:find`/`:grep` 结果可操作（yy/dd/p/:tag/:rename 映射真实路径；见 SPEC M38-1；2026-11-21 完成：results_actions_enabled + Delete 臂，450 单测；:tag/:chmod 随守卫复用）
- [x] M38-2 `pick` 兼容面（--query/--multi/--print0/--preview/--height/--no-sort/--exit-0/--select-1；见 SPEC M38-2；2026-11-21 完成：parse_pick_options + cmd_pick 消费，450 单测）
- [x] M38-3 `rvifm init bash`/`zsh`/`fish`/`powershell`（见 SPEC M38-3；2026-11-21 完成：shell_init 模块 + main 臂；init 只输出不写 rc）
- [x] M38-4 `:frecency stats`/`export`/`purge`/`prune`（见 SPEC M38-4；2026-11-21 完成：四子命令 + purge confirm 确认门 + JSON 全量导出 + prune --before 日期裁剪（snapshot 不动）；发现前会话 :frecency 臂已提交但缺 prune/确认门——替换为完整版，见 NOTES；429 单测 + 8 cli）
- [x] M38-5 `:find` 过滤器（size/mtime/type/depth）+ 命名搜索（见 SPEC M38-5；2026-11-21 完成：find_files_filtered + 纯谓词/spec 解析 + --k=v/--k v 双形态 + save/load 仅内存同名覆盖（load 未命中列出集合）；远端视图保持 M22-5 既有链路；首版漏排 root 自身被 m22_5/端到端测试双双拦下；434 单测 + 8 cli）

## M39 — 预览与平台（2026-11-21 增补；见 SPEC M39 节，已确认登记·按序执行）

> 来源：同上。目标 0.14.0。顺序：1（hex）→ 2（CSV/PDF 翻页/EXIF）→ 3（zip 容器）→ 4（SFTP 硬化）→ 5（UAC）→ 6（标签布局）。

- [x] M39-1 十六进制查看器（偏移跳转、ASCII 侧栏、分块；见 SPEC M39-1；2026-11-21 完成：w 打开二进制文件（前 8KB NUL 判定）→ 16 字节/行 hex（偏移+ASCII 侧栏）、viewer_hex_lines 分块读取（视口窗口 ≤1MB）、`:goto 0x…/十进制` 行对齐跳转（结果视图豁免清单加 goto）；m39_1 全链路测试，435 单测）
- [x] M39-2 CSV/TSV 预览、PDF 翻页、图片 EXIF 摘要（见 SPEC M39-2；2026-11-21 完成：csv/tsv 表格对齐预览（naive 分隔 SIMPLIFIED）+ PDF 查看器单页 `[`/`]` 翻页（PAGE_CACHE 任意页按需渲染，门控沿用 pdf_image_pages；旧 1–2 页网格与 pdf_overflow_note 退役）+ 容器级尺寸摘要（PNG/JPEG/GIF/BMP 头解析入信息面板）；m39_2 四测试，438 单测 + 8 cli）
- [x] M39-3 通用 zip 容器识别（docx/xlsx/pptx/apk/war；见 SPEC M39-3；2026-11-21 完成：ZIP_CONTAINERS 常量表——is_archive_file 与 open_dispatch 共用（M34-6 三表漂移制度化），魔数 PK 由 ZipFs::open 校验（非 zip 内容可读拒绝）；m393 测试（识别/大小写/roundtrip/损坏拒绝），439 单测）
- [x] M39-4 SFTP 硬化（指纹确认 ask 缺省、ProxyJump、agent、keepalive；见 SPEC M39-4；2026-11-21 完成：strict_host_key = ask|accept|no（缺省 ask——RemoteFingerprint 覆盖层 y/n，accept_new 重连放行；no 等效 known_hosts=false）+ ProxyJump（ssh_config_host_fields 透传，direct-tcpip 二段连接，认证材料同目标 SIMPLIFIED）+ ssh-agent（SSH_AUTH_SOCK，unix-only，密钥→agent→口令序列）+ keepalive 30s/3；RemoteConnectErr 三态共享体；m394 测试（config 解析 + 判定矩阵 + 不可达重连失败状态不变 + sshd 全链路 cfg(unix) CI），441 单测 + 8 cli）
- [x] M39-5 Windows 操作级 UAC（ShellExecuteEx runas；见 SPEC M39-5；2026-11-21 完成：EACCES → Escalate 消费分支 cfg(windows) 直调 ShellExecuteExW("runas")（cmd.exe 承载单操作、等待退出码、ERROR_CANCELLED 可读"用户取消"、主进程不提升）+ PrivOp::uac_command 六操作映射（路径引号保护）；unsafe 面 = privilege.rs 模块级 FFI（AGENTS §7 注释豁免）；UAC 弹窗手工清单项保留，442 单测）
- [x] M39-6 命名标签、溢出滚动、布局选项（见 SPEC M39-6；2026-11-21 完成：`:tabname <名>`（无参清除，仅内存）+ tab_bar_spans 共享（命名显示 + 溢出 ‹› 窗口滚动，三渲染点去重）+ `[ui] layout` 四模式（Single 全宽右零宽 / Dual 现状 / PreviewRight 查看器恒右 / PreviewBottom 下半全宽，pane_layout 布局感知 + viewer_rect + 覆盖守卫（命中测试与渲染同源，Dual 对侧预览覆盖 quirk 顺带修复）；m39_6 测试，443 单测 + 8 cli）

## M40 — 1.0 稳定化（2026-11-21 增补；见 SPEC M40 节，已确认登记·按序执行）

> 来源：同上。目标 1.0.0。顺序：1（基准）→ 2（fuzz）→ 3（proptest）→ 4（覆盖率）→ 5（供应链）→ 6（平台）→ 7（发布）。

- [x] M40-1 criterion 性能基准（启动/10 万项/排序/replay/1GB 吞吐；见 SPEC M40-1；2026-11-21 完成：lib 化（src/lib.rs 模块树共享，main.rs 仅 CLI 分派）+ benches/rvifm_bench.rs 五组 + docs/bench.md 基线（本机 rustc 1.99.1：100k 加载 6.83s≈68µs/条目记为潜在优化点、1GB 复制 1.79GB/s、replay 10 万事件 65ms、自然排序 10k 3.26ms）；顺带修 TransferDirection pub（lib 化 private_interfaces）+ README 状态句漂移，443 单测）
- [x] M40-2 cargo-fuzz 模糊测试（归档解析/路径规范化/反序列化；见 SPEC M40-2；2026-11-21 完成：fuzz/ 独立工作区（5 靶——zip/tar.gz/7z 解析驱动 + normalize_inner·normalize_lexical 路径 + EntryRecord 反序列化）+ CI fuzz job（ubuntu nightly + cargo-fuzz，3×120s 归档 + 2×60s 路径/反序列化 ≈10 分钟总量，rss/max_len 护栏）；24h 本地长跑 deferred（libFuzzer 需 nightly/Linux——Windows 本机不可跑，随 Linux 开发机执行后回填 NOTES）；normalize_lexical pub 化）
- [x] M40-3 proptest 属性测试（glob/自然排序/撤销栈；见 SPEC M40-3；2026-11-21 完成：三组十属性——glob 4（`*` 任意/字面量等值/`*p*` 包含/`?` 计数）+ 自然排序 3（对称性/数字感知/幂等）+ 撤销 3（rename roundtrip/create 移除/逆序恢复空态）；**抓到并修复真实缺陷**——natural_cmp 非全序（Equal 非传递 → std sort panic，M24-2 起 shipped），重写为分块比较 + 回归测试，见 NOTES；454 单测 + 8 cli）
- [~] M40-4 覆盖率 ≥75%（核心模块；见 SPEC M40-4；2026-11-21 完成 4/5——cargo-llvm-cov 实测：ops **79.70%**（+6.6pt，trash clear/purge 注入形态 + create_link 自动化 + auto_rename/Checksum 测试）、frecency 83.04% ✓、archive 94.52% ✓、keymap 91.27% ✓；**deferred：remote/sftp.rs 15.57%**——actor 线程需真实 SSH 服务器（russh connect_async 硬路径，mock 无法穿透；CI 无 sshd 配置），需 CI sshd 服务或 sftp mock server + coverage 运行，环境绑定缺口见 NOTES；顺带修复 junction undo EPERM 真实缺陷（undo_op Create 臂 is_symlink 分支 remove_file 对目录型 reparse point 报拒绝访问 → remove_dir 优先回退 remove_file））
- [x] M40-5 供应链（cargo-deny/audit/SBOM/SHA256；见 SPEC M40-5；2026-11-21 完成：deny.toml（许可证白名单/通告 fail/来源锁定）+ **sevenz-rust → sevenz-rust2 迁移**（RUSTSEC-2026-0246 弃维护 + 提取穿越——ArchiveReader/ArchiveWriter/ArchiveEntry API 映射 + Error::Io 直接构造，read-only 用法回归测试过）+ RUSTSEC-2023-0071（rsa Marvin Attack）与 RUSTSEC-2024-0320（yaml-rust 弃维护，syntect 传递）带理由豁免 + CI supply-chain job（deny/audit/SBOM artifact）+ release job SHA256SUMS.txt；cargo deny check 四项全绿本机实测，audit 本机 DB 拉取网络抖动（deny 同库 advisories 已过））
- [x] M40-6 macOS CI + aarch64 Linux 归档（见 SPEC M40-6；2026-11-21 完成：test 矩阵加 macos-latest（fmt/clippy/build/test，sshd 探测测试自动 skip）+ release 矩阵加 aarch64-unknown-linux-gnu 腿（gcc-aarch64-linux-gnu 链接器注入 .cargo/config.toml，SHA256SUMS 统一覆盖）；aws-lc-sys aarch64 交叉构建首跑 CI 验证）
- [x] M40-7 1.0.0 发布（CHANGELOG 完整 + crates.io；见 SPEC M40-7；2026-11-22 完成：CHANGELOG/版本号/tag v1.0.0 → run 44984795450 全绿 + GitHub Release 四归档+SHA256SUMS；发布修红三层——aarch64 c_char E0308 本地交叉复现修复、smoke 交叉产物 ELF 断言替代 qemu（apt 装 qemu runner 不稳）、annotate_errors.sh 注解精确化；**crates.io 已上线（本机凭据 publish + API 验证 newest/max=1.0.0）**）

## M42 — :tutor 导师模式（2026-11-22 增补；用户蒸馏 Qwen 讨论稿·已确认，见 SPEC M42 节）

> 来源：用户 Qwen 讨论稿蒸馏（人工验证与辅助学习合并为 TUI 内交互模式）。
> 单一模式、无报告文件、错题本本地持久化。顺序：1（框架/UI）→ 2（数据/错题本）
> → 3（内容）→ 4（文档收尾）。

- [x] M42-1 `:tutor` 命令 + 底部 Tutor Panel + Tutor 模式按键拦截（y/n/q/Esc；见 SPEC M42-1；2026-11-22 完成：src/tutor.rs（TutorModule/Item + include_str 内嵌 core_nav）+ InputMode::TutorList/TutorSession + tutor_list_key/tutor_session_key 纯函数 + exec 分派（无参列模块/带参进入/未知可读报错）+ 四视图白名单放行 + render 列表覆盖层与底部 Panel（1/4，左 prompt/expected 右进度+键提示，done 结算 Summary）+ m42_1 双测试，495 单测）
- [x] M42-2 TutorModule/Item 数据 + `tutor_progress.toml` 持久化 + 错题本优先调度 + `:tutor reset`（见 SPEC M42-2；2026-11-22 完成：ModuleProgress/TutorProgress（BTreeMap→[module_id] 表）+ load/save（缺失/损坏静默降级）+ merge_session（mastered/skipped 互斥迁移）+ review_order（skipped→unseen→mastered 三桶保序）+ 进入会话复习模式（Panel 标题（复习）+ 🔁 提示行）+ 关闭自动落盘（失败进状态栏）+ `:tutor reset <id>`（exec 错误分支测试，成功路径路径注入单测——m38_4 家规）；496 单测 + 12 集成）
- [x] M42-3 首批内容 3 模块（core_nav/file_ops/search_frecency；见 SPEC M42-3；2026-11-22 完成：core_nav 7 项（M42-1 随框架落成）+ file_ops 8 项（yy/p·P 独立动作语义/dd 确认+暂存/u 逆序/:rename 单项直改与无参批量预览）+ search_frecency 7 项（/ incsearch·n·N、= 过滤、o 选择器、:recent 近期、选择器只读）——条目语义逐项对照 exec.rs/tab.rs 现行为；builtin_modules_parse_and_unique 测试固化）
- [x] M42-4 文档收尾（GUIDE/README 指引 + ACCEPTANCE §42 + 批次同步；见 SPEC M42-4；2026-11-22 完成：GUIDE §2 导师模式推荐、help.txt :tutor 行、README 核心特性+快速开始、CHANGELOG 未发布段；**M42 批次 4/4 关闭**）

## M43 — tutor 全功能覆盖与放行重构（2026-11-22 增补；用户裁决·已确认，见 SPEC M43 节）

> 来源：M42 首批（23 项）扩为全功能键位/命令手册（13 模块 ~85 项，逐键粒度
> ——用户裁决问题 5 全量）；键位交互重构为透明放行（边学边练，Ctrl+Y/N 标记
> ——用户裁决问题 1 采纳建议）。顺序：1（放行重构）→ 2（数据模型）→ 3/4/5
> （内容三批）→ 6（文档同步+维护规则）。

- [x] M43-1 TutorSession 迁 state.tutor 覆盖层 + 透明放行（Ctrl+Y/N 标记、Ctrl+Q 退出）+ 模块列表滚动（见 SPEC M43-1；2026-11-22 完成：InputMode::TutorSession 退役→AppState.tutor 覆盖层状态（任意模式切换不丢会话）+ tutor_hotkey 前置拦截（Ctrl+Y/N/Q；Esc 放行保清搜索语义）+ 作答即时落盘（幂等合并，中途退出 app 不丢）+ advance/skipped_count 会话方法 + 列表 List 组件视口滚动 + m43_1 双测试（放行/热键/滚动渲染），498 单测 + 12 集成）
- [x] M43-2 TutorItem requires 字段（editor|sudo|sftp|git|osc52|graphic|tmux）+ Panel 环境提示（见 SPEC M43-2；2026-11-22 完成：requires Option<String>（缺省 None、多 token 逗号分隔保留原文）+ Panel 左列 🔧 提示行 + m43_2 解析测试（缺省/多 token/非法 token 固化），499 单测 + 12 集成）
- [x] M43-3 内容批 1：search_filter/visual_tag/marks_frecency/tabs_history（见 SPEC M43-3；2026-11-22 完成：7+7+8+8=30 项——/·n·N·焦点感知·=·f/F·;/,、v/V/可视 d·y·t·:select·:tag copy、o/:j/:J/:recent/选择器 d·u/m·'/ :bmark 族/:cmdmark/:keyword、:tabnew/close/move/name/gt·gT·2gt/独立状态/:hist·:history/跨 tab u——逐条对照既有验收语义；499 单测 + 12 集成）
- [x] M43-4 内容批 2：panes_sort/archive_ops/maintenance（见 SPEC M43-4；2026-11-22 完成：8+5+9=22 项——w 查看器·查看器搜索·S/:sort 多字段·:compare byname/bycontents·dp/do·:tree、归档进入·:archive 校验形态·7z/外部 CLI·:extract、sandbox 引导首条目·:chmod·:du·:dup·:checksum·:sync dry-run·:trash 了解即可·:jobs·:queue（requires=sftp）——参数形态对照 exec.rs（:extract [目录]/:sync l2r·r2l·--apply·--delete/:checksum 选中项）；499 单测 + 12 集成）
- [x] M43-5 内容批 3：command_line/clipboard_git/remote_sftp/tutor_meta + core_nav 补 Space（见 SPEC M43-5；2026-11-23 完成：9+5+5+5=24 项 + core_nav Space 1 项——命令行历史/补全/:cd/:mk/:copy·move/Ctrl-G·:file/:theme/:help、yn/yf/yc（yc requires=osc52）/git 列/:gi（requires=git）、:remote 列表/进入/远端写/跨 VFS/.part（requires=sftp）、Ctrl 标记/放行/退出/复习/reset meta 模块——参数形态对照 exec.rs；499 单测 + 12 集成）
- [x] M43-6 文档同步 + tutor 条目维护规则固化（见 SPEC M43-6；2026-11-23 完成：README 核心特性（13 模块 100 项/Ctrl 键）+ help.txt :tutor 行 + GUIDE §2 推荐（13 模块清单/Ctrl+Y·N·Q/reset）+ CHANGELOG 未发布段 M43 小节；**维护规则固化于 SPEC M43-6 MUST**（此后批次新增键位/命令时同步补 tutor 条目——批次验收条约束，不进 harness_check）；**M43 批次 6/6 关闭**）

## REL — 发布流水线解耦与人工验证（2026-11-22 增补；用户蒸馏指令，见 SPEC REL 节）

> 来源：用户讨论稿蒸馏（GitHub 自动 / crates.io 人工发起解耦）。**同日两度
> 修订**：上午 REL-1 建立清单门（RELEASE_CHECKLIST.md）；同日用户裁决撤销
> 清单门，人工验证方式改为 `:tutor` 自行验证（见 REL-2 与 M42）。

- [x] REL-1 发布两通道解耦（GitHub Release 自动 / crates.io publish 人工发起；AGENTS §3 自动发布段修订；2026-11-22 完成；清单门形态当日撤销，见 REL-2）
- [x] REL-2 门禁简化（删除 RELEASE_CHECKLIST.md；SPEC REL/AGENTS §3·§10/PLAN §4/ACCEPTANCE §16 stub 改写为"publish 仅人工发起 + `:tutor` 自验"；2026-11-22 完成）

## M44 — 预览焦点修复（2026-11-23 增补；用户指令）

- [x] M44-1 `w`/`:view` 焦点保持 + 光标跟随 + `Tab` 进入/返回 + 关闭恢复（见 SPEC M44；2026-11-23 完成：`Tab.preview_focus` + `refresh_viewer_follow` + 3 个回归用例；500 单测）
- [x] M44-2 文档同步与 1.1.0 GitHub Release（crates.io 不发布；见 SPEC M44；2026-11-23 完成）

## FIX — 维护期实测修复批次（2026-11-23 增补；蒸馏自用户实测反馈，已确认执行）

> 来源：用户 2026-11-23 实测反馈三条——① `v1.1.0`/`v1.0.0` GitHub Release 的
> `SHA256SUMS.txt` 只含单个归档（4 个并行 release job 各自生成并 `--clobber`
> 覆盖，最后一个完成的 job 胜出）；② `:tutor` 模块列表覆盖层与底部 Panel 背景
> 透明，与目录列表等功能界面互相干扰看不清（覆盖层无 `Clear`，透出下层文字）；
> ③ 会话退出保存后启动未恢复——`save_session` 写 `~/.config/rvifm/session.toml`
> 时配置目录可能不存在，`fs::write` 静默失败（与 tutor progress 的
> `save_progress_to` 已做 `create_dir_all` 不对称）。顺序：REL-3 → FIX-5 → FIX-6。

- [x] REL-3a CI 修复：release job 不再各自生成/上传 SHA256SUMS，新增独立 release-checksums 作业汇总全部归档（含数量守卫 4）；见 SPEC REL-3-1（2026-11-23 完成：YAML 解析通过，`scripts/repair-release-checksums.sh` 备查）
- [x] REL-3b 历史回填 v1.0.0/v1.1.0 的 SHA256SUMS.txt（2026-11-23 人工鉴权执行 `scripts/repair-release-checksums.sh` 完成；两版本各 4 行、逐条哈希实测匹配；见 SPEC REL-3-2）
- [x] FIX-5 `:tutor` 模块列表覆盖层与底部 Panel 不透明化（Clear + 背景样式，消除与目录列表的视觉干扰；见 SPEC FIX-5；2026-11-23 完成：render.rs 两处 Clear + fix5_* 双测试，504 单测）
- [x] FIX-6 会话保存前自动创建配置目录（`save_session` 补 `create_dir_all`，与 tutor progress 同模式；见 SPEC FIX-6；2026-11-23 完成：`save_session_with_path` 路径注入 + fix6_* 双测试，506 单测）

## REL-4 — Release 归档简化为 musl-only（2026-11-23 增补；用户指令·已评估，见 SPEC REL-4 节）

> 来源：用户 2026-11-23 指令——GitHub Release 不再生成 gnu 版本，只保留 musl
> 版本，aarch64 也改为 musl。评估：musl.cc GCC 11.2.1 编译 aws-lc-sys 触发
> ICE；裸 `zig cc` 被 cc-rs 追加的 Rust 风格 `--target` 破坏；`cargo-zigbuild`
> 正确翻译，本地实测 cross-build 成功且产物为 ARM aarch64 静态 ELF。顺序：
> 1（CI 配置）→ 2（版本 bump + 发布）。

- [x] REL-4-1 CI release matrix 改 musl-only（3 腿：x64-musl / aarch64-musl / windows-x64；aarch64 用 cargo-zigbuild 交叉编译 + 静态链接 smoke；SHA256SUMS 数量守卫 3；见 SPEC REL-4-1；2026-11-23 完成：YAML 解析 + 本地 cross-build 实测通过）
- [x] REL-4-2 版本 bump 1.2.0 + 文档同步 + tag → GitHub Release（三归档 + SHA256SUMS；crates.io 不发布；见 SPEC REL-4-2；2026-11-23 完成：tag run 45132119778 全绿，三 musl 归档 + SHA256SUMS 3 行 `sha256sum -c` 实测通过，aarch64 产物静态 ELF + `--version` 通过）

## REL-5 — M45 批次发布（1.3.0；自动续轮触发）

- [x] REL-5 版本 bump 1.3.0 + 文档同步 + tag v1.3.0 → GitHub Release（三 musl 归档 + SHA256SUMS；crates.io 不发布；见 PLAN §4；2026-11-24 完成：tag run 45256802332 全绿，三归档 + SHA256SUMS 3 行 `sha256sum -c` 实测通过，aarch64 静态 ELF + `--version` = rvifm 1.3.0）

## REL-6 — M46 批次发布（1.4.0；CI 全绿后触发）

- [x] REL-6 版本 bump 1.4.0 + 文档同步 + tag v1.4.0 → GitHub Release（三 musl 归档 + SHA256SUMS；crates.io 不发布；见 PLAN §4；2026-11-25 完成：tag run 45452618998 九 job 全绿，Release v1.4.0 三归档 + SHA256SUMS 3 行，`sha256sum -c` 实测通过，aarch64 静态 ELF + x64-musl `--version = rvifm 1.4.0`）

## M45 — 用户实测批次（2026-11-23 蒸馏；vifm 对齐 + 缺陷修复，待用户确认，见 SPEC M45 节）

> 来源：用户 2026-11-23 测试反馈（29 项，按 §8 蒸馏）。登记前逐项核对代码与
> 文档：已实现 6 项（分隔线/标签样式/整行反显/tutor 进度持久化/opener 引擎/
> `:extract`）保持不动或仅细化；缺功能 9 项、缺陷 9 项、冲突需拍板 5 项（见
> NOTES）。编号沿用 M15/M21 用户实测批次先例取 M45；本批次属维护期 FIX/FEAT
> 蒸馏。顺序：1→10（冲突项在 SPEC/NOTES 标注，实施前需用户裁决）。

> 分批执行，每完成一子任务即勾选 + 更新 STATE + 单独提交（方便切换模型续跑）。
> 第一批（P0 缺陷）：M45-3b/3d · M45-5a · M45-6a · M45-8a/8b · M45-9a；

- [x] M45-3d `v` 多选 + `d` 进 `ConfirmTagDelete`（见 SPEC M45-3④；2026-11-23 完成：Outcome::ConfirmTagDelete + keys.rs 路由 + m453d 测试，507 单测）← 第一批 P0
- [x] M45-5a `o` 过滤 cmd topic（cd/tabnew 不可跳转；见 SPEC M45-5①；2026-11-23 完成：without_cmd_topic + o/:recent 接线 + m455a 测试，508 单测）← 第一批 P0
- [x] M45-6a `:cd` 遵循焦点 pane（修 M41-1 违规；见 SPEC M45-6①；2026-11-23 完成：focus_base + 焦点 pane 导航 + m456a 测试，509 单测）← 第一批 P0
- [x] M45-8a tutor 列表 `l`/Enter 后退出 `TutorList`（见 SPEC M45-8①；2026-11-23 完成：Enter 臂设 mode=Normal + m458a 渲染不变量测试）
- [x] M45-8b Panel 提示改 Ctrl+Y/N/Q（见 SPEC M45-8②；2026-11-23 完成：render.rs 三行提示 + m42_1 测试更新，510 单测）
- [x] M45-9a `:archive` 覆盖 `v` 多选（见 SPEC M45-9①；2026-11-23 完成：archive_plan 增补 v 选区分支 + m459a 测试，511 单测）← 第一批 P0
- [x] M45-1a `Location::display_path()` HOME 下显示 `~/…` 缩写（仅显示层；见 SPEC M45-1①；2026-11-23 完成：abbreviate_home 纯函数 + m451a 测试，512 单测）
- [x] M45-1b 标签栏鼠标点击切换 tab（见 SPEC M45-1②；2026-11-23 完成：tab_bar_hit_test 同源 geometry + m451b 测试，513 单测）
- [x] M45-1c 标签样式细化（灰白底反色字，与路径行解耦；见 SPEC M45-1③；2026-11-23 完成：tab_label_style 纯函数 + m451c 测试）
- [x] M45-1d `[ui] pane_separator` 可配置（默认 false 不显示分隔线，true 恢复；见 SPEC M45-1④；2026-11-23 完成：UiConfig 字段 + 渲染守卫 + m451d 测试，515 单测）
- [x] M45-2a 未搜索态整行背景一致（非搜索匹配子串高亮，保留 FIX-1；见 SPEC M45-2③；2026-11-23 完成：反显行去文件类型 fg + m452a 测试，516 单测）
- [x] M45-2b 类型图标推断（可执行/文本/jar/deb，无后缀不显示 `*`；见 SPEC M45-2②；2026-11-23 完成：infer_icon + 4KB 嗅探（仅常规文件）+ m452b 测试，519 单测）
- [x] M45-3a opener 配置错误提示具体化 + 示例指引（不新增自动生成；见 SPEC M45-3①；2026-11-23 完成：路径+单行压缩+opener 表形态提示 + help.txt 示例 + m453a 测试，520 单测）
- [x] M45-3b 可执行文件 Enter 特例：仅提示不执行（见 SPEC M45-3②；2026-11-23 完成：is_exec + 无 run 绑定时仅提示，显式 [open] run 规则可绕过 + m453b 测试，521 单测）
- [x] M45-3c 删除确认居中阻塞弹窗带图标/高亮（见 SPEC M45-3③；2026-11-23 完成：confirm_dialog/confirm_rect + m453c 测试；同批串行化共享 trash 索引测试，522 单测）
- [x] M45-3e `h`→`l` 光标还原（per-location 光标记忆；见 SPEC M45-3⑤；2026-11-23 完成：Location Hash + Tab/Pane cursor_memory，navigate_to/enter_selected/return_parent 记忆还原 + m453e 测试，524 单测）
- [x] M45-4a 图片预览越界修复（native 协议字体尺寸兜底；见 SPEC M45-4①；2026-11-23 完成：safe_font_size 合理性钳制 + clamp_slice_height 三屏上限 + frame intersection + m454a 测试，525 单测；真实终端验证 C6 deferred）
- [x] M45-4b 归档预览显示成员列表（`PreviewKind::Archive`，Office 先于归档；见 SPEC M45-4②；2026-11-23 完成：preview_kind Archive 分流 + archive_preview（损坏/.rar 回退）+ render_view_content NUL 探测 + m454b 测试，527 单测）
- [x] M45-5b 初版覆盖 `:j`/`:J` JumpSelect 浮层鼠标点击；`o` 为嵌入式 FrecencyPane，鼠标入口漏测，后由 FIX-8-1 补齐（见 SPEC FIX-8）
- [x] M45-5c `:` 命令名辅助提示（见 SPEC M45-5③；2026-11-23 完成：范围自决为命令名 Tab 补全（NOTES 记录）+ COMMAND_NAMES + m455c 测试，529 单测）
- [x] M45-6b `o` 结果跳转写焦点 pane（见 SPEC M45-6②；2026-11-23 完成：jump_to 焦点感知（右 pane 直接 location+refresh/左 pane 沿用 history）+ m456b 测试，530 单测）
- [x] M45-6c 右键栏归档 `l`/Enter 进入虚拟目录（见 SPEC M45-6③；2026-11-23 完成：Pane::enter_selected/refresh/return_parent 归档分支 + m456c 测试，531 单测）
- [x] M45-7a `:extract` 对侧/指定侧或路径（见 SPEC M45-7①；2026-11-23 完成：extract_dest 缺省对侧 + left/right/当前/对侧/路径 + m457a 测试，532 单测）
- [x] M45-7b 归档虚拟目录 `yy`+`p`/`P` 解压粘贴（见 SPEC M45-7②；2026-11-23 完成：YankSource::Archive + extract_members 共用 + m457b 测试，533 单测）
- [x] M45-8c tutor 布局避让（见 SPEC M45-8③；2026-11-23 完成：tutor_panel_height 同源 render/视口同步 + m458c 测试，534 单测）
- [x] M45-8d 模块列表左侧显示进度（见 SPEC M45-8④；2026-11-23 完成：tutor_progress 缓存 + tutor_progress_label + m458d 测试，535 单测）
- [x] M45-9b `[名, checksum=]` 格式推断（见 SPEC M45-9②；2026-11-23 完成：archive 命令双参 [名,checksum=] 臂从扩展名推断 + m459b 测试，536 单测）
- [x] M45-9c 对话框宽度/去后缀/默认名/选中高亮（见 SPEC M45-9③；2026-11-23 完成：archive_dialog_rect/strip_archive_ext/默认名多选父目录 + 逐项反显 + m459c 测试，537 单测）
- [x] M45-10 文档收尾（README/GUIDE/help/ACCEPTANCE/CHANGELOG 同步；见 SPEC M45-10；2026-11-23 完成：CHANGELOG [Unreleased] + GUIDE/README/help 同步 + tutor archive_ops 条目（extract 对侧默认 + 归档 yy 粘贴）+ harness_check 通过）

## 排队缺口（2026-11-15 M16 实施中发现，见 NOTES 同日条目）

- [x] GAP-1 M10-5 收尾：keymap TOML 分层接线（config.rs 解析 prepend/append/replace 并合并进 InputState.keymap——当前仅默认表，用户绑定不可用；NAV-3 的推荐映射示例依赖此项）
- [x] GAP-2 M8 收尾：TUI 退出时批量追加会话访问事件（SPEC §4.6"退出时批量追加"未实现——会话访问退出即丢，drain 各 tab history → append_events；NAV-4 的会话合并只覆盖即时可见性）

## FIX-7 — 双栏焦点能力一致性（2026-11-24 增补；用户确认）

- [x] FIX-7-1 两栏 `..` 行与父目录导航统一（见 SPEC FIX-7；2026-11-24：右 pane 独立 parent-row 状态、渲染/键盘/鼠标/视口同步，fix7_1 测试；538 单测 + 12 集成全绿）
- [x] FIX-7-2 修复焦点 pane 下快捷键分派（`o`、Visual/tag、`yy`/`p`/`P` 等，见 SPEC FIX-7；2026-11-24：右栏共用 Action 分发 + 焦点投影回写；o/复制粘贴/归档 yank 回归，541 单测 + 12 集成全绿）
- [x] FIX-7-3 修复焦点 pane 下目录命令与复制目标（`:mk`/`:mkdir`、`yy`→`p/P`，见 SPEC FIX-7；2026-11-24：右栏创建/进入、mkdir 光标定位、P 移动到焦点栏；回归覆盖通过）
- [x] FIX-7-4 全仓键位/命令审计、文档与 tutor 同步（见 SPEC FIX-7；2026-11-24：移除旧右栏禁用分支与过时说明，更新 README/GUIDE/help/验收；tutor 无右栏限制文本）

## 不在任务清单中的事项

以下内容明确不实现，不要为它们创建任务：

- Vim 脚本引擎、插件系统
- FUSE / `archivemount`
- fzf / SQLite
- 完整 Vim 引擎
- 归档内写入、嵌套归档递归浏览
- 内嵌视频播放
- PDF 渲染引擎作为默认依赖（`exprim` 仅限非默认 `pdf-visual` feature；`tdf` / `pdfium-render` 完全禁止）

## FIX-8 — Frecency 搜索面板融合与预览修复（见 SPEC FIX-8）

## FIX-9 — Normal 模式输入法自动切换（2026-11-24 增补；用户确认，见 SPEC FIX-9）

- [x] FIX-9-1 Linux IBus/Fcitx/Fcitx5 命令后端、`[ime]` 配置及输入模式边界保存/切换/恢复接线；零新增依赖（见 SPEC FIX-9）
- [x] FIX-9-2 用户文档、内置帮助与 tutor 验收项同步（见 SPEC FIX-9-3）
- [deferred] FIX-9-M1 在真实 Linux IBus/Fcitx 桌面会话人工验证进入/离开文本输入模式与退出恢复（见 SPEC FIX-9）
- [deferred] FIX-9-CI Windows/macOS 编译分支在对应 CI runner 补验（本地仅有 Linux target，见 SPEC FIX-9）

- [x] FIX-8-1 统一 TUI frecency 焦点面板、查询高亮、鼠标跳转、`:view` 预览复用及文档收尾（2026-11-24：`o`/`:j`/`:J` 共用面板；README/GUIDE/help/tutor 同步；546 单测 + 12 集成全绿）

## M46 — 用户实测批次 2（2026-11-24 蒸馏；vifm 对齐 + 缺陷真修复 + 配置/归档增强，见 SPEC M46 节）

> 来源：用户 2026-11-24 实测反馈 12 项，按 AGENTS §8 蒸馏。多项为 M45/FIX 既有任务的"未真正修复"回退，登记时逐项对照代码定位根因（见 NOTES）。编号沿用 M45 用户实测批次先例取 M46。顺序：1→12（缺陷修复优先，文档收尾殿后；环境/平台相关人工验收 deferred）。

- [x] M46-1 图片预览越界真修复（预览 pane 几何/Clear/pane 级相交 + 原生协议 SlicedImage 高度钳制；见 SPEC M46-1；2026-11-24 完成：plan_image 纯几何 + 等高顶部裁剪替代 3 屏 SlicedProtocol、viewer_rect 与 pane 同源、m461 双测试，555 单测）
- [x] M46-2 搜索高亮仅应用当前焦点 pane（左 pane 增焦点守卫，vifm 对齐；见 SPEC M46-2；2026-11-24 完成：render.rs active_query 增 left_focused 守卫，m462 焦点对称测试，556 单测）
- [x] M46-3 右 pane 未搜索态整行背景一致（M45-2a 同款 row_reversed 守卫右移；见 SPEC M46-3；2026-11-24 完成：render_right_pane 增 row_reversed/ft 守卫，m463 右栏反显行 fg 断言，557 单测）
- [x] M46-4 `h`→`l` 光标还原真修复（on_parent_row 不再覆盖还原 cursor；见 SPEC M46-4；2026-11-24 完成：enter_selected 增 had_memory 判定，左右 pane 与归档分支同源，m453e 双测试补 on_parent_row 断言）
- [x] M46-5 `:extract` 解压到对侧后刷新目标 pane（extract/yy→p + job 完成刷新双侧；见 SPEC M46-5；2026-11-24 完成：extract 命令臂/归档 yy→p 接 refresh_tab_panes，apply_job_update 双侧刷新，m465 端到端 + m457b 断言，558 单测）
- [x] M46-6 启动创建默认 config.toml（注释说明默认设置）+ 键位/命令示例配置，不覆盖已有（见 SPEC M46-6；2026-11-24 完成：三份内嵌模板 + ensure_default_config_at 路径注入、启动钩子接线、m466 模板自解析/不覆盖测试，559 单测）
- [x] M46-7 `:help` 修复：不透明背景 + 新终端窗口打开 + `/` 搜索可用（见 SPEC M46-7；2026-11-24 完成：help Clear+黑底不透明、HelpSearch 渲染分支、rvifm help 子命令 + [ui] help_in_new_window 新窗口路径，m467 渲染/配置 + CLI 冒烟，561 单测 + 13 集成；新窗口真实 tmux 激活为尽力而为）
- [x] M46-8 归档 `w` 预览提速（去每帧全量解压/重开，缓存 + 护栏；见 SPEC M46-8；2026-11-24 完成：ArchivePreviewCache 按路径+mtime+size、64MB 输入/4000 成员护栏、大归档快速提示，m468 双测试，563 单测）
- [x] M46-9 `.deb`/`.apk`/`.rar` 的 `w` 预览与 `l`/Enter VFS（apk 保持；deb ar+tar；rar 外部 CLI；见 SPEC M46-9；2026-11-24 完成：DebFs（ar 解析 + 内存 data.tar + data/ 虚拟目录）、RarFs（bsdtar/unrar/7z 只读）、Decoder::from_bytes、扩展名真值表同步，m469 三测试，566 单测；rar 工具可用性的真实环境验证待用户补验）
- [x] M46-10 删除确认弹窗 yazi 风格重做（标题居中+图标+文件列表+固定高度+滚动条+j/k；见 SPEC M46-10；2026-11-24 完成：标题居中 ⚠ + 无提示文字 + 图标/类型色文件列表 + 6 行固定高度 + 右侧滚动条 + j/k 导航，m453c/m4610 测试，567 单测）
- [x] M46-11 更新内置 help 文档（见 SPEC M46-11；2026-11-24 完成：help.txt 新增 M46 段并更新 :help/:extract/w/dd 行，GUIDE §11 配置生成说明 + 变化摘要，CHANGELOG [Unreleased] M46 小节；harness_check 通过）
- [x] M46-12 更新导师模式测试项目与提示（见 SPEC M46-12；2026-11-24 完成：search_filter/command_line/archive_ops/file_ops/tutor_meta/core_nav/panes_sort 七个模块的 prompt/expected 同步 M46 变化，保持 13 模块 100 项；builtin_modules_parse_and_unique 绿）

## M47 — 用户实测批次 3（2026-11-25 蒸馏；焦点化收尾 + 视觉间距 + 防御性 IME + 缺陷修复，见 SPEC M47 节）

> 来源：用户 2026-11-25 实测反馈 7 条（「右栏仅导航」含 3 子项），按 AGENTS §8 蒸馏。编号沿用 M45/M46 用户实测批次先例取 M47。顺序：1→10（缺陷/焦点化先行，文档殿后）。**本批次不触发版本 bump 与 GitHub Release（用户指令）。**

- [x] M47-1 目录/归档成员预览图标按层级缩进（见 SPEC M47-1；2026-11-25 完成：preview_indent + TUI viewer 缩进移到图标前，m471 双测试）
- [x] M47-2 `=` 过滤作用于焦点 pane + 顶部路径显示状态 + Esc 取消（见 SPEC M47-2；2026-11-25 完成：Pane.filter + 焦点化 apply_filter + path_line_text + Normal Esc 清除，m472 测试）
- [x] M47-3 `:archive` 源=焦点栏、产物=对侧栏；`:extract` 焦点化核对（见 SPEC M47-3；2026-11-25 完成：archive_plan 源/目标分离 + focused_entries/cursor + opposite_physical_base，m473 双测试）
- [x] M47-4 剩余快捷键/冒号命令焦点化审计（见 SPEC M47-4；2026-11-25 完成：selected_names/%d/%f/`:file`/marks/`:sort`/`:select` 焦点化，审计清单入 NOTES，m474 测试）
- [x] M47-5 Frecency 面板样式融合焦点栏 + 新标题（见 SPEC M47-5；2026-11-25 完成：无边框 + TopLineSel 顶部路径 + 新标题 + 点击几何同源，m475 测试）
- [x] M47-6 IME 启动/Normal Esc 防御性激活英文（见 SPEC M47-6；2026-11-25 完成：force_english 不覆盖 saved + 250ms 节流 + FocusGained/Esc 触发，m476 双测试）
- [x] M47-7 右栏顶部 `..` 行只高亮该行（见 SPEC M47-7；2026-11-25 完成：row_reversed 守卫替代无条件 idx==cursor，m477 测试）
- [x] M47-8 左右栏 `v` 多选状态各自独立（见 SPEC M47-8；2026-11-25 完成：Pane.visual/tags + 投影 + 渲染各读自己栏，m478 测试）
- [x] M47-9 双栏左侧留白 + 整行高亮覆盖间隙（见 SPEC M47-9；2026-11-25 完成：LIST_LEADING_BLANK + 列宽预算同步 + 双侧/frecency 列表，m479 测试）
- [x] M47-10 文档收尾（README/GUIDE/help/CHANGELOG/ACCEPTANCE/tutor；见 SPEC M47-10；2026-11-25 完成：README/GUIDE/help/CHANGELOG/tutor 四模块提示同步，13 模块 100 项可解析，harness_check 通过）

## M48 — 用户实测批次 4（2026-11-25 蒸馏；外窗/打开/复制解压/归档预览/tutor/弹窗，见 SPEC M48 节）

> 来源：用户 2026-11-25 实测反馈 12 条，按 AGENTS §8 蒸馏。编号沿用 M45–M47 先例取 M48。顺序：1→13（缺陷先行，文档殿后）。沿用「不 bump 版本 / 不 tag / 不 Release」口径。

- [x] M48-1 `s`/`:!` 新窗口激活前台 + 路径取焦点栏（见 SPEC M48-1；2026-11-25 完成：去 tmux `-d` + `-c <focused dir>` + screen cwd/next，m481 测试）
- [x] M48-2 文本/`.sh`/`.md5` 打开优先新窗口且不覆盖界面（见 SPEC M48-2；2026-11-25 完成：mime .md5 + is_text_like + exec 特例文本优先 + block opener 新窗路由，m482 双测试）
- [x] M48-3 右栏 `l`/Enter 文件打开与左栏一致（见 SPEC M48-3；2026-11-25 完成：open_selected 焦点化 + 右栏 Enter/鼠标再击走共享 opener，m483 测试）
- [x] M48-4 `v` 多选 `y` 后对侧 `p`/`P` 缓存缺失修复（见 SPEC M48-4；2026-11-25 完成：可视态 y 同步写 YankBuffer + 剪贴板，m484 测试）
- [x] M48-5 `p`/`P` 冲突改阻塞式确认弹窗（见 SPEC M48-5；2026-11-25 完成：预扫冲突 + Outcome/InputMode::ConfirmPaste + 复用删除窗口渲染，m485 测试）
- [x] M48-6 归档成员粘贴路径/递归/多选解压修复（见 SPEC M48-6；2026-11-25 完成：公共 inner 前缀剥离 + 完整路径读取去重 + 递归展开，m486 测试）
- [x] M48-7 归档快速预览（可选外部列目录 + 大包可展开）（见 SPEC M48-7；2026-11-25 完成：archive_external_listing 配置 + 固定工具映射 + 4MB 输出护栏 + 内置回退，m487 测试）
- [x] M48-8 `:archive` 无参直执行 / `-i` 交互 + 对话框细节（见 SPEC M48-8；2026-11-25 完成：默认 zip 直执行 + 目标名规范化 + 字段高亮 + 只清弹窗矩形，m488 测试）
- [x] M48-9 IME 切回自动切英文稳健化（Normal 周期探测）（见 SPEC M48-9；2026-11-25 完成：Normal 1s 周期 force_english + Esc/FocusGained 重置计时，m489 测试）
- [x] M48-10 tutor 列表 `/` 搜索 / `d` reset / 仅错题 / Ctrl+N 重选（见 SPEC M48-10；2026-11-25 完成：TutorList search 态 + reset 分派 + wrong_items_only + done 时 Ctrl+N 回列表，m4810 测试）
- [x] M48-11 删除确认 yazi 真窗口化（状态栏/滚动条在框内）（见 SPEC M48-11；2026-11-25 完成：footer 独立 widget + 窗口内缘滚动条列 + 高度调整，既有 m453c/m4610 回归通过）
- [x] M48-12 `:help` 新窗口修复 + `rv help` 改 view 查看器（见 SPEC M48-12；2026-11-25 完成：run_with_initial_mode + run_help_viewer + 标题高亮 + 无 TTY 回退，CLI 13 集成通过）
- [x] M48-13 文档收尾（README/GUIDE/help/CHANGELOG/ACCEPTANCE/tutor；见 SPEC M48-13；2026-11-25 完成：四文档 + help.txt + tutor_meta 同步，harness_check 通过，未发版）

## M49 — 用户实测批次 5（2026-11-26 蒸馏；预览固定右侧 + 命令/间距/预览效率/frecency 键位，见 SPEC M49 节）

> 用户 2026-11-26 指令（LAYOUT-1..4）+ 反馈 124–141，按 AGENTS §8 蒸馏；LAYOUT-n → M49-1..4，其余按原序。沿用「不 bump / 不 tag / 不 Release」口径。

- [x] M49-1 LAYOUT-1 frecency 搜索左/预览右固定布局（见 SPEC M49-1；2026-11-26 完成：共享 preview_layout_areas + FrecencyPane 预览焦点态 + m491 测试）
- [x] M49-2 LAYOUT-2 `:view` 预览固定右侧（见 SPEC M49-2；2026-11-26 完成：render/render_right_pane/preview_path_with 统一固定右侧，动态对侧用例改 fixed_side=false）
- [x] M49-3 LAYOUT-3 `[preview] fixed_side` 配置（默认 true，新会话生效）（见 SPEC M49-3；2026-11-26 完成：PreviewConfig + config.default.toml，渲染/点击同源读取）
- [x] M49-4 LAYOUT-4 布局文档（GUIDE/SPEC/NOTES/ACCEPTANCE）（见 SPEC M49-4；2026-11-26 完成：GUIDE §2.4/§3 与 NOTES 决策同步，SPEC/ACCEPTANCE 已登记）
- [x] M49-5 `:gitui` → `:gi`（保留 `:gitui` 兼容别名）（见 SPEC M49-5）；2026-11-26 完成：:gi 主命令 + :gitui 兼容别名 + 文档/测试
- [x] M49-6 列表左侧留白收窄至约图标半宽（1 空格）（见 SPEC M49-6）；2026-11-26 完成：LIST_LEADING_BLANK 1 空格 + 高亮覆盖（m479 更新）
- [x] M49-7 可视模式 `D` 永久删除选中项（见 SPEC M49-7）；2026-11-26 完成：v+D 永久删除（标记 + 确认 + 直删，DD 不回归）
- [x] M49-8 `:extract` 显式目录条目只建空目录修复（见 SPEC M49-8）；2026-11-26 完成：显式目录条目落地 + RarFs 完整路径 + m498 测试
- [x] M49-9 快速切换轻量预览（图片/PDF 元数据 + 文本 200–300 字）（见 SPEC M49-9）；2026-11-26 完成：quick 态图片元数据/文本 300 字 + PDF 跳过栅格化
- [x] M49-10 预览获焦后全量加载（见 SPEC M49-10）；2026-11-26 完成：preview_focus 驱动全量加载（m4910 测试）
- [x] M49-11 `:archive`/`:extract` 焦点化复核与残留命令（见 SPEC M49-11）；2026-11-26 完成：:rename/:move/:copy 焦点化复核；archive/extract 已焦点化
- [x] M49-12 frecency 标题动态 `${mode}`/`[filter]`（见 SPEC M49-12）；2026-11-26 完成：标题 NORMAL/SEARCH + [filter] 黄底高亮
- [x] M49-13 SEARCH 提示 + `Ctrl+J/K`（见 SPEC M49-13）；2026-11-26 完成：SEARCH 提示方向键 + Ctrl+J/K + Ctrl+U 清空
- [x] M49-14 SEARCH Esc 保留过滤（见 SPEC M49-14）；2026-11-26 完成：Esc 返回 NORMAL 保留 query（m4912 测试）
- [x] M49-15 NORMAL `dd` 删记录 / `u` 恢复 / `yf`·`yn`·`yc` / `gi`（见 SPEC M49-15）；2026-11-26 完成：dd 删记录 / u 经 pending 恢复 / yf·yn·yc / gi（m4915 测试）
- [x] M49-16 文档收尾（GUIDE/help/CHANGELOG/tutor/ACCEPTANCE）（见 SPEC M49-16；2026-11-26 完成：四文档 + help/tutor 同步，harness_check 通过，未发版）

## M50 — 命令补全增强（2026-11-26 蒸馏 DeepSeek 分享稿，已确认，见 SPEC M50 节）

> 用户确认 5 项决策：登记执行 / 复用 frecency overlay 且 ≤40% 高 / shell 历史默认开+增量 / 新 Action 无默认键位 / 变量先落地裸变量+quote_expansion 开关。沿用不发布口径。

- [x] M50-1 内部命令补全（命令表+frecency 排序+复用 overlay 两列）（见 SPEC M50-1）；2026-11-26 完成：命令表从 help.txt 解析 + cmd_scores + 两列浮层（m501）
- [x] M50-2 `!` 前缀来源分离（`:` 内部 / `:!` 历史完整命令行）（见 SPEC M50-2）；2026-11-26 完成：`!` 前缀分池 + 历史完整命令行候选（m502）
- [x] M50-3 shell 历史解析（默认开+增量读取+仅命令名入库）（见 SPEC M50-3）；2026-11-26 完成：shell_history 模块（bash/zsh + 增量签名 + 仅命令名入库）
- [x] M50-4 `Action::OpenExternalCommandLine`（无默认绑定）+ 配置示例（见 SPEC M50-4）；2026-11-26 完成：Action open_external_command_line + Outcome，无默认绑定（m504）
- [x] M50-5 `[command] quote_expansion` + 裸 `%f/%F/%d/%D` 引号（见 SPEC M50-5）；2026-11-26 完成：[command] quote_expansion + 裸变量引号（m505/m503）
- [x] M50-6 文档收尾（GUIDE/SPEC/NOTES/ACCEPTANCE/help/config 模板）（见 SPEC M50-6）；2026-11-26 完成：GUIDE/help/CHANGELOG/config 模板同步，harness_check 通过

## M51 — 用户实测批次 6（2026-11-26 蒸馏；IME 干扰 / 默认编辑器 / 文本识别 / 冲突弹窗，见 SPEC M51 节）

- [x] M51-1 IME 过度干扰修复（外部窗口/失焦期间暂停强制切英文）（见 SPEC M51-1）；2026-11-26 完成：IME_SUSPENDED 全局暂停 + FocusLost/外部窗口暂停、按键/FocusGained 恢复（m511）
- [x] M51-2 `edit_text` 默认命令改为 vim（见 SPEC M51-2）；2026-11-26 完成：config/opener/help 模板默认 edit_text=vim %s
- [x] M51-3 `.vifm` / 无扩展名文本（LICENSE）识别补齐（见 SPEC M51-3）；2026-11-26 完成：mime_of 未知扩展名 4KB 嗅探 + vifm/vim/cfg 文本表（opener 测试）
- [x] M51-4 粘贴冲突弹窗显示按键提示（见 SPEC M51-4）；2026-11-26 完成：ConfirmPaste 独立 footer 渲染按键提示（m514）
- [x] M51-5 文档收尾（GUIDE/help/CHANGELOG/ACCEPTANCE/NOTES）（见 SPEC M51-5）；2026-11-26 完成：GUIDE/help/CHANGELOG/NOTES 同步，harness_check 通过

## M52 — 预览布局策略反转（2026-11-26 蒸馏，已确认，见 SPEC M52 节）

> 用户确认「理解正确」后登记；反转 M49-1/2/3 的默认策略，历史条目保留。沿用不发布口径。

- [x] M52-1 策略反转：`fixed_side` 默认 false 且仅 frecency 生效；`:view` 恒对侧；pick 恒右（见 SPEC M52-1）；2026-11-26 完成：config 默认 false、render/preview/tab 拆分作用域，m521 通过
- [x] M52-2 右栏快捷键/命令一致性审计与补缺（见 SPEC M52-2）；2026-11-26 完成：审计结论入 NOTES（无残留仅导航限制），m521 覆盖预览焦点与侧别无关
- [x] M52-3 全量文档/代码一致性收尾 + 质量门禁（见 SPEC M52-3）；2026-11-26 完成：GUIDE/README/CHANGELOG/help/SPEC/ACCEPTANCE/STATE/PLAN/NOTES 全部同步，harness_check 通过，未发版

## M53 — 标签栏对齐、归档 VFS 预览与预览性能（2026-11-26 蒸馏，已确认，见 SPEC M53 节）

- [x] M53-1 标签标题 `[N:焦点栏全路径]` 且随焦点更新（见 SPEC M53-1）；2026-11-26 完成：tab_bar_labels_and_visible 取 focused_location 全路径，m531 通过
- [x] M53-2 `:tabname` 与右栏活动项会话持久化（见 SPEC M53-2）；2026-11-26 完成：SessionTab 增 name/right_path/right_cursor（serde default），m532 往返 + 旧会话兼容测试
- [x] M53-3 归档成员 `w` 预览 + `l`/Enter 临时只读副本交系统 opener（见 SPEC M53-3）；2026-11-26 完成：extract_member_to_temp + 预览缓存 + open_selected 归档臂，m533 通过
- [x] M53-4 frecency NORMAL 完整键位提示 + `r` 删记录（见 SPEC M53-4）；2026-11-26 完成：提示补全 dd/u/r/yf·yn·yc/gi 等，m534 通过
- [x] M53-5 文本 36 行预览 + 小文件卡顿排查（见 SPEC M53-5）；2026-11-26 完成：按行截取 ≥36 行 + 文本行缓存（path+mtime+size）+ 快速态标题标记，m535/m4910 通过
- [x] M53-6 foot/tmux 原生图片协议恢复（见 SPEC M53-6）；2026-11-26 完成：should_query_protocol + tmux allow-passthrough 探测 + env 覆盖，测试保留
- [x] M53-7 `.deb` 等大归档去除「归档较大」误提示（见 SPEC M53-7）；2026-11-26 完成：dpkg-deb -c 映射 + 表格输出末列解析，m537 通过
- [x] M53-8 真异步/防抖预览（快速切换跳过中间预览）（见 SPEC M53-8）；2026-11-26 完成：viewer/归档临时副本 150ms 防抖 + preview_debounce_due，m538 通过
- [x] M53-9 文档收尾 + 质量门禁（见 SPEC M53-9）；2026-11-26 完成：GUIDE/help/CHANGELOG/STATE/PLAN/NOTES 同步，harness_check 通过，未发版

## M54 — vifm z 系列快捷键（2026-11-26 蒸馏，见 SPEC M54 节）

- [x] ZKEY-1 声明/实现不一致排查（NOTES 对照表）（见 SPEC ZKEY-1）；2026-11-26 完成：z 系列与 scrolloff 均无声明/实现，对照表入 NOTES，非 z 全量审计 defer
- [x] ZKEY-2 隐藏文件切换 za/zo/zm（见 SPEC ZKEY-2）；2026-11-26 完成：3 个 Action + z 前缀 seq 接线 + show_hidden 同源 + 光标保持，zkey2 测试通过
- [x] ZKEY-3 过滤器临时重置 zR/zM/zO（见 SPEC ZKEY-3）；2026-11-26 完成：FilterSnapshot + 3 个 Action + zkey3 测试
- [x] ZKEY-4 视口定位 zt/zz/zb + `[ui] scrolloff`（见 SPEC ZKEY-4）；2026-11-26 完成：3 个 Action + scrolloff 默认 0 + zkey4 测试（光标不变）
- [x] ZKEY-5 键位注册、GUIDE/ACCEPTANCE/NOTES 同步（见 SPEC ZKEY-5）；2026-11-26 完成：9 键默认注册 + z 前缀 Resolver + GUIDE/help/config 默认模板 + 取舍 NOTES，harness_check 通过

## M57 — `rv help` 外部入口收敛（2026-11-26 蒸馏，见 SPEC M57 节）

- [x] M57-1 移除 `rvifm help` 子命令与文档/测试中的外部入口描述（见 SPEC M57）；2026-11-26 完成：main.rs 去分派与 usage、app/mod.rs 去 help_text/run_help_viewer、exec.rs `:help` 恒应用内、cli 测试改为断言不可用、GUIDE/help/CHANGELOG/config 同步

## M55 — frecency 搜索态区分与实时递归模式（2026-11-26 蒸馏，见 SPEC M55 节）

- [x] M55-1 搜索态视觉区分（输入行边框+光标+标题 query，列表无边框）（见 SPEC M55-1）；2026-11-26 完成：Search 标题焦点边框色+加粗、状态栏 LEFT/RIGHT 边框输入框 + ▋ 光标 + j/k 输入提示，列表维持无边框，m551 通过
- [x] M55-2 实时递归扫描后端（O 触发、复用 :find、后台流式、深度/条目护栏）（见 SPEC M55-2）；2026-11-26 完成：spawn_live_find + LiveFindState + InputMode::LiveFind + O 触发 + 每帧限量 drain，m552 通过（深度 20 护栏）
- [x] M55-3 实时递归交互（增量子串过滤 + scanning 状态栏 + Enter 跳转 + Esc）（见 SPEC M55-3）；2026-11-26 完成：查询子串过滤 + ↑↓/Ctrl+J/K 移动 + scanning 标题/边框输入框 + Enter 走 jump_to/record + Esc 停线程，m553 通过
- [x] M55-4 文档与验收同步（GUIDE/help/ACCEPTANCE/NOTES + 质量门禁）（见 SPEC M55-4）；2026-11-26 完成：GUIDE/help/CHANGELOG 同步 O 与搜索态、619 单测 + 13 集成 + fmt/clippy/build + harness_check 绿，未发版

## M56 — 向导模式（tutor 重构，2026-11-26 蒸馏，见 SPEC M56 节）

- [x] M56-1 术语统一为「向导模式」（见 SPEC M56-1）；2026-11-26 完成：标题/搜索框/Panel/命令注释/GUIDE/help 统一，模块项计数同步 14/102，原断言改为向导模式
- [x] M56-2 首次无配置自动进入向导（见 SPEC M56-2）；2026-11-26 完成：run() 在 ensure_default_config 前检测 config_path().exists()，纯函数 initial_mode_for_startup + m562 测试
- [x] M56-3 对话框几何 80% + 极小终端全屏降级（见 SPEC M56-3）；2026-11-26 完成：tutor_dialog_rect（≤40/≤10 全屏）+ 列表移入对话框，m563 通过
- [x] M56-4 底部进度状态栏 + 带边框进度条（见 SPEC M56-4）；2026-11-26 完成：tutor_overall_progress + tutor_progress_bar + 对话框内「学习进度」状态栏（欢迎提示/模块/功能点/百分比）
- [x] M56-5 `/` 搜索实时更新（见 SPEC M56-5）；2026-11-26 完成：TutorListSearch 复用列表渲染并带光标，键入即过滤/清空全显示，m565 通过
- [x] M56-6 有记录模块仅显示错题且编号保持（见 SPEC M56-6）；2026-11-26 完成：`:tutor <id>` 改用 wrong_items_only，Panel 标题按 item id 回查原编号，m566 通过
- [x] M56-7 模块完成 Enter=继续学习其它模块（见 SPEC M56-7）；2026-11-26 完成：tutor_hotkey 完成态 Enter→ExitToList + Panel 提示，m567 通过
- [x] M56-8 100% 模块重入确认重置（见 SPEC M56-8）；2026-11-26 完成：tutor_module_is_complete + TutorResetConfirm 覆盖层 + y/Enter 重置、n/Esc 取消，m568 通过
- [x] M56-9 进度跨版本兼容 + 稳定 ID（见 SPEC M56-9）；2026-11-26 完成：load_progress_from 读入即 prune_unknown_ids（按 item id 过滤、保留其它模块），m569 通过
- [x] M56-10 向导条目 ↔ ACCEPTANCE 映射同步规则（见 SPEC M56-10）；2026-11-26 完成：NOTES 标记块映射表 14 模块 + 同步流程，harness_check 新增规则 8 校验覆盖/章节，八条规则绿
- [x] M56-11 成就徽章（见 SPEC M56-11）；2026-11-26 完成：tutor_badges（🏅首模块/🏆全模块/⭐零错题）+ 学习进度状态栏标题角落，m570 通过
- [x] M56-12 文档/验收同步 + 质量门禁（见 SPEC M56-12）；2026-11-26 完成：GUIDE/help/CHANGELOG/STATE/PLAN/ACCEPTANCE 同步，627 单测 + 13 集成 + fmt/clippy/build + 八条 harness 绿，未发版

## M58 — 命令补全增强（2026-11-26 蒸馏，见 SPEC M58 节）

- [x] M58-1 补全浮层：标题提示 + 描述常显 + 实时过滤（见 SPEC M58-1）；2026-11-26 完成：command_completion_pool 同源候选/描述 + 键入实时重建 + Space 标题提示，m581 通过
- [x] M58-2 命令行光标闪烁（见 SPEC M58-2）；2026-11-26 完成：SystemTime 500ms 翻转 ▋，无额外线程
- [x] M58-3 Space 选定补全到输入栏（见 SPEC M58-3）；2026-11-26 完成：completion_accept 保留头部参数，m583 通过
- [x] M58-4 Tab 循环三 bug 修复（见 SPEC M58-4）；2026-11-26 完成：浮层尽量全显+滚动条、completion_offset 自动滚动、索引/描述同源，m584 + m501 更新
- [x] M58-5 外部命令读取完整命令行（推翻 M50 边界）（见 SPEC M58-5）；2026-11-26 完成：shell_history 写整行 + cmd_full_lines 整行去重 + record_command_lines_bulk，SPEC/GUIDE/源码隐私注释同步改写
- [x] M58-6 多行外部命令预览（见 SPEC M58-6）；2026-11-26 完成：command_preview_lines + 顶部「外部命令预览」边框框，m586 通过
- [x] M58-7 外部命令返回码提示（见 SPEC M58-7）；2026-11-26 完成：成功也回显「返回码 0」，m587 + m18_4 更新
- [x] M58-8 `rvifm cmd` 外部入口（见 SPEC M58-8）；2026-11-26 完成：frecency::cmd_cmd 复用 pick_interactive（只读），main.rs 分派与 usage，cli 测试 14 项通过
- [x] M58-9 文档/验收同步 + 质量门禁（见 SPEC M58-9）；2026-11-26 完成：GUIDE/help/CHANGELOG/SPEC 隐私边界改写 + M59/M60 拆分登记，632 单测 + 14 集成 + 全门禁绿，未发版

## M59 — 预览与归档 VFS 体验修复（2026-11-26 蒸馏，见 SPEC M59 节）

- [x] M59-1 预览同源审计与统一（见 SPEC M59-1）；2026-11-26 完成：highlight::styled_lines 共享入口，pick 文本预览改用同源高亮，m591 通过
- [x] M59-2 归档成员预览 j/k/Tab 导航（见 SPEC M59-2）；2026-11-26 完成：Tab::preview_target（preview_path 或 archive 临时副本）供 activate_viewer/refresh/viewer_lines，m592 通过
- [x] M59-3 归档成员预览语法高亮（见 SPEC M59-3）；2026-11-26 完成：临时副本保留扩展名 + shared styled_lines，m592 断言高亮命中
- [x] M59-4 归档 l/Enter 走配置的 opener（见 SPEC M59-4）；2026-11-26 完成：open_with_layers 替代 system_open + block 规则走 pending_external，m594 通过
- [x] M59-5 快速文本行数 = 视口+10（至少 36）（见 SPEC M59-5）；2026-11-26 完成：quick_preview_line_count + m595
- [x] M59-6 卡顿与二进制闪现修复（见 SPEC M59-6）；2026-11-26 完成：render_view_content 优先 viewer.path（committed）、缓存命中只克隆所需行，m596 通过
- [ ] M59-7 预览焦点蓝色边框高亮（见 SPEC M59-7）
- [ ] M59-8 frecency Tab 固定进/出预览（见 SPEC M59-8）
- [ ] M59-9 r/u 仅清/恢复 bookmark 且带确认（见 SPEC M59-9）
- [ ] M59-10 文档/验收同步 + 门禁（见 SPEC M59-10）

## M60 — 真异步预览架构（2026-11-26 蒸馏，见 SPEC M60 节）

- [ ] M60-1 预览 worker + generation token + channel（见 SPEC M60-1）
- [ ] M60-2 文本读取/高亮 off-thread + 缓存引用化（见 SPEC M60-2）
- [ ] M60-3 图片解码 off-thread（见 SPEC M60-3）
- [ ] M60-4 归档/Office/PDF 缓存未命中 off-thread（见 SPEC M60-4）
- [ ] M60-5 性能验收 + 文档 + 门禁（见 SPEC M60-5）
