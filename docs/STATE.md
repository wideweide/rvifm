# STATE —
- `fix`（2026-11-15）：find/grep 结果显示路径规范化为 /——CI Windows 抓到 srch1_find 测试失败：display() 在 Windows 产生反斜杠，断言（与全路径显示语义）不符。按 AGENTS §7 在显示层 normalize（语义路径 item.path 不变，locate 不受影响）。教训：任何 path.display() 进 UI/断言前统一 replace 反斜杠。双平台全绿（run 24acd00）。M17 批次全部关闭（SRCH-1/SRCH-2/VIEW-1）。
- `VIEW-1`（2026-11-15）：交互式查看器——ViewerState（scroll/line_count/query/match_line）挂 Tab；toggle_view 扩展（激活时读行数，grep 命中 scroll=行-2 上下文）；handle_key 查看器路由块（置于 is_quit 前：j/k/gg/G/Ctrl-u/d 滚动、/→ViewerSearch 模式、n/N 环绕、w/Esc/q 退出、其余消费、: 放行）；渲染=行号 gutter（当前匹配 ▶）+ 查询子串黄底高亮 + 标题含位置与搜索词；图片仍走 ImageEngine；M13-9 测试按新语义重写（查看器内 j 滚动视口；M14 模型下"摘要跟随"已由双栏取代）。227 单测全绿（+1）。M17 批次全部关闭。
- `SRCH-2`（2026-11-15）：:grep 接入——grep_files（regex 逐行匹配 + NUL 二进制/非 UTF-8 跳过 + 非法正则可读报错）已随 SRCH-1 落地共享基础设施（exec find|grep 同臂、enter_results 按行号渲染 路径:行号: 内容、locate 定位文件）；本任务补集成测试（命中+行号+定位+非法正则+状态指示）。226 单测全绿（+1）。
- `SRCH-1`（2026-11-15）：:find 虚拟目录——src/search.rs（ignore 遍历：gitignore/.ignore 感知 + show_hidden 联动 + 大小写不敏感叶子子串 + 上限 1 万截断）；Tab.results（items/entries 平行表，沿 :tree 同构）+ enter/exit_results + locate_result（退出结果视图 + navigate_to 父目录 + 光标停文件）；preview_path() 抽象（结果模式取命中文件——view_mode 预览即命中文件）；l/Enter=定位、Esc/h=退出、v/t 拒绝、yy=命中文件路径、dd/DD/cw 拒绝；状态栏 [find: query — N 项]。**顺带修复存量缺陷**：seq 块 match 臂内 return 跳过 input.seq.clear()——dd/DD 确认后序列残留吞掉下一个按键（全部早退臂补 clear）。225 单测全绿（+5）。
- `fix`（2026-11-15）：历史菜单当前标记移到行首——CI Windows 抓到 nav2_exec_showhistory_and_render 失败：runner 临时路径 50+ 列，popup 宽 48 列把行尾 "● 当前" 截掉（Linux 短路径不触发）。教训固化：浮窗行内标记放行首，断言不依赖行尾内容；CI test step 现以 ::error:: 注解输出失败行（匿名可读，digest 步骤 Windows 上自身失败已绕开）。双平台全绿（run 6a7d510）。
- `NAV-4`（2026-11-15）：o 选择器近期视角——`JumpKind::Recent` + `:recent` 命令（复用 JumpSelect 覆盖层，标题"近期 · 时间倒序"）；`ScoredEntry.last_visit`（replay 取条目最近事件 ts，snapshot-only 为 0）；`merge_session_visits`（各 tab history 按轨迹顺序赋递增 ts，基准=墙钟-1h；既有条目只改 last_visit 不改分数，会话新条目直接创建）；Recent 排序 = last_visit desc + score 次序。**发现 M8 缺口**：TUI 退出时批量追加 events 从未实现（会话访问不落盘）——本视角靠会话内合并即时可见，退出刷写作为独立任务排队（NOTES）。218 单测全绿（+3）。M16 批次全部关闭。
- `NAV-2`（2026-11-15）：:history 菜单——ExecOutcome::ShowHistory + `:history`/`:his` 臂（..行守卫豁免）；`InputMode::HistoryMenu { cursor }`（live 渲染 history[..=hist_pos]，当前条目 ● 标记）；j/k 移动、Enter/l 导航（history_menu_nav → navigate_to 新导航语义：truncate forward）、dd 移除（history_remove：当前视图不可删、删游标前条目 pos 前移）、Esc/q 关闭；渲染居中遮罩浮窗。215 单测全绿（+3）。菜单语义细化：从菜单跳转=新导航追加（back 栈内重复出现属正常，vifm 同）；forward 截断仅在 pos 之后发生。
- `NAV-3`（2026-11-15）：:histprev/:histnext——Tab.history 语义升级为**完整访问轨迹 + hist_pos 游标**（初始含当前视图）；`navigate_to` 统一 6 个导航点（enter/return/:cd/quickjump：先加载成功才变更状态——损坏归档失败无副作用契约保持；新导航截断 forward）；`hist_jump` 后退/前进（边界 false 无副作用，条目按位置重载——归档位置往返测试覆盖）；exec 臂（后退 N/M 提示、边界提示）；`..` 行守卫豁免导航命令；不设默认键（默认表无 Ctrl-O/Ctrl-I 断言），help 给命令说明 + 注释性 keymap 示例——**发现 M10-5 的 keymap TOML 分层实际未接线**（InputState.keymap 仅默认表，config.rs 无 prepend_keymap 解析），用户绑定暂不可用，缺口记 NOTES。212 单测全绿（+2，历史断言全量更新）。
- `NAV-1`（2026-11-15）：frecency 快速映射层——EntryKind::Keyword + EntryRecord.keyword（serde default 旧格式兼容）；keyword 事件重放只应用元数据（不加分不推进时钟，±keyword 隔离断言分数全等）；record --keyword（纯元数据事件不记访问）；set_keyword_at/_now（TUI :keyword 同链，- 清除）；[quickjump] char→keyword 独立解析（OnceLock，不动 Config Copy；踩坑：toml 1.x Value::from_str 只解析单值，文档用 Table）；（反引号）+字符 quickjump（别名优先、缺省查 keyword==字符；quickjump_target 纯函数筛 dir 最高分——踩坑：cmp_ranked 是降序比较器不能直接喂 max_by，改显式 score 比较器）；未命中提示不静默；:cd 补 on_parent_row=false（FEAT-2 遗留）。help.txt 更新。208 单测全绿（+4）。
- `M15 批次全部关闭`（2026-11-15）：FIX-1～4 + FEAT-1～4 八项完成（各自提交见上）；M14-4 发布闭环验证——GitHub Release v0.5.0 双平台归档（linux 9.8MB/windows 8.8MB）+ crates.io 0.5.0。
- `M14-4`（2026-11-15）：发布 0.5.0（M14 双 pane + 标签栏，minor bump）——crates.io + tag `v0.5.0` → GitHub Release。
- `FEAT-4`（2026-11-15）：:help 全屏帮助视图——`src/help.txt` 内嵌（include_str!，完整键位/命令/鼠标/菜单参考）；`InputMode::Help` 替换为 `HelpView { scroll, search }` + `HelpSearch { input, scroll, search }`（搜索输入携带视图状态）：j/k 滚动（钳制）、/ 搜索、n/N 环绕跳转（help_find_match 大小写不敏感子串）、q/Esc 关闭返回（状态仅模式局部，列表光标/滚动不变）；渲染全屏 Block + 标题含搜索词。**踩坑**：TestBackend 渲染 CJK 全角为"字+空格"，断言取单字/ASCII token；反向搜索 start = scroll+len-1（排除当前行）。203 单测全绿（+3）。
- `FEAT-3`（2026-11-15）：:file 命令与右键菜单——`ExecOutcome::ShowInfo` + `:file` 臂（与 Ctrl-G 同源 file_info_text；归档成员显示 host:/inner/name + 成员索引元数据，权限/mtime 不可用标 "-"）；`InputMode::ContextMenu { cursor }` 六项菜单（Open/File Info/Copy/Move/Rename/Delete）+ `run_context_menu_action`（Copy/Move 目标=另一 pane 目录，vifm 双栏约定，NOTES）；渲染 34×8 居中浮窗 + Clear 遮罩；键盘 j/k/Enter/Esc + 鼠标（右键条目=聚焦+开菜单，菜单内点击执行、外部关闭）；Rename 预填 rename 命令行、Delete 走 dd Confirm（u 可逆）；`hit_entry_index` 抽出共用。**踩坑**：system_open 阻塞（Command::status），测试 Open 只用目录。200 单测全绿（+5）。
- `FEAT-2`（2026-11-15）：列表顶部 ".." 条目——设计决策：**独立 on_parent_row 焦点标志**（Tab 字段）而非 cursor 位移——".." 为渲染层虚拟行（列表上方独立 1 行，DIM 置灰/聚焦 REVERSED），cursor 语义不变、不进 entries/搜索/过滤/标记（SPEC SIMPLIFIED）。行为：has_parent_row（物理非根=true，归档恒 true）；k 首条目→"..".行（替代首→尾环绕，M13-4 测试更新）；j 从 ".." 回首条目；gg 落 ".."（vifm）；进入目录/归档后落在 ".." 行（vifm 语义）；l/Enter = return_parent（归档根回物理目录）；写操作守卫（yy/dd/DD/cw/Ctrl-G/t/v/exec 写命令拒绝）；鼠标单击聚焦 / 再击返回。195 单测全绿（+5，渲染行位移测试 +1 行修正）。
- `FEAT-1`（2026-11-15）：鼠标支持——run 启用 EnableMouseCapture（main restore 后 Disable、cw 挂起恢复后重启用）；`handle_mouse`：左键单击聚焦（已聚焦行再击=open_selected，双击第二击天然覆盖）、滚轮逐行 move_cursor(±1)（NOTES：List 无独立视口，视口式滚动与 widget 冲突）；列表几何=标签栏1行+左60%（与 render 同源）；输入模式/列表外（标签栏/状态栏/右 pane）不响应；Visual 点击移动光标延伸选区。`open_selected` 从 l/Enter 臂抽出共用；`list_offset` 显式视口（ListState.with_offset 与命中测试同推导）。190 单测全绿（+5）。
- `FIX-4`（2026-11-15）：o 选择器居中与遮罩——`centered_rect`（60%宽/50%高自适应，上限 80×20，小终端保底）+ 全屏 Clear+黑底遮罩（先 Clear 抹符号再铺底色——Block.style 只改 bg 不清符号）；popup 内布局不变；关闭恢复列表与光标（选择器 cursor 是模式局部状态）。80×24→48×12@(16,6)、160×48→80×20@(40,14)。185 单测全绿（+2，旧覆盖层坐标测试同步修正）。
- `FIX-3`（2026-11-15）：归档浏览回归修复——根因：M11-7 的 system_open 分支插在 enter_selected 之前，物理视图下归档文件（is_file）被交给系统 opener 抢占，Tab::enter_selected 的 M7-4 归档路径（仅左 pane 实装）永远不可达。修复：`l`/Enter 臂对 is_archive_file/is_unsupported_archive 名称先路由 enter_selected（虚拟目录进入 + M7-9 解压提示），仅非归档文件走系统 opener。右 pane 维持 M14 SIMPLIFIED 精简集（无归档进入）。183 单测全绿（+4：进入/损坏/不支持/归档内 dd no-op）。
- `FIX-2`（2026-11-15）：yy/p 复制链路——YankBuffer（paths+cut）入 AppState；keymap 补 `yy`→Yank 绑定（默认表此前缺失，Action::Yank 同补）+ 单键前缀臂补 `y`；`p`=PutCopy（复制）、`P`=PutMove（移动），目标=焦点 pane 目录（physical_base）；每项独立入撤销栈（u 可逆）；空缓冲/目标已存在/只读目标可读错误不 panic；yy 来源：可视选择→标记→光标起 count 条。dd→cut 缓冲（p 从暂存恢复到任意目录）deferred 记 NOTES。179 单测全绿。
- `FIX-1`（2026-11-15）：搜索高亮与实时索引——`/` 改"高亮+跳转"语义（不隐藏非匹配项）：`match_range`（smart-case 子串区间）+ `search_status`（i/N 计数）；Search 模式加 `origin` 字段，输入实时跳第一匹配（incsearch）、无匹配回 origin+提示、Esc 恢复光标；列表匹配子串黄底黑字高亮（与光标反显区分）；状态栏 Search/Normal 双口径显示 `[i/N]`+无匹配提示；空提交清除搜索状态。175 单测全绿。
- `M14-4`（2026-11-15）：发布 0.5.0（M14 双 pane + 标签栏，minor bump）——crates.io（API 验证 0.5.0）+ tag `v0.5.0`。**踩坑**：tag 原指向的 bump 提交有两个平台差异测试在 Linux CI 失败（perm 硬编码 644 受 umask 影响；根路径测试用 C:\\ 在 Unix NotFound）→ 修复后 tag 重指向并强推，release job 重新产出。

- `M14-2`（2026-11-15）：标签栏 UI——render 顶部 1 行（`N:dirname` + 当前 REVERSED|BOLD）；`:tabnew`/`gt` 切换有可见反馈；tab 栏偏移导致 4 个渲染测试行号修正。164 单测全绿。

- `M14-1`（2026-11-15）：双 pane 模型对齐 vifm——`Pane`（location/entries/cursor/scroll + 导航方法）+ `Tab.right`/`focus`；handle_key 焦点分发（Right → `handle_right_key` 精简导航集：j/k/gg/G/Ctrl-u/d/l/Enter/h/Ctrl-G/w；写命令提示切回）；exec 写命令焦点化（`focus_base`/physical_base 焦点感知）；`refresh_tab_panes` 双刷新；渲染双栏（右 pane 目录列表 ↔ 预览视图，图片渲染保留）。163 单测全绿。

- `M13-11`（2026-11-15）：发布 0.4.0（M13 十项 vifm 对齐增强，minor bump）——crates.io（持久授权）+ tag `v0.4.0` → GitHub Release 自动归档。

- `M13-10`（2026-11-15）：暂存区菜单——ops 增 `TrashEntry`/`record_staging`/`read_trash_index`/`restore_from_trash`（**index/ 每记录独立 .json**，JSONL 并发 append 会粘连）；`:trash` → `InputMode::TrashMenu` 覆盖层（Enter 恢复到原位、orig 冲突 Err、Esc 关）；测试串行锁（全局 trash 目录共享）。162 单测全绿。

- `M13-9`（2026-11-15）：view mode——`w` 键/`:view` 切换 `Tab.view_mode`；预览窗格渲染 max_lines：摘要=窗格高度 / view=5000（完整内容）；j/k 列表移动视图跟随光标文件（render 每帧重建）；双栏布局保持非全屏。162 单测全绿（+1：截断/完整/跟随三态对照）。

- `M13-8`（2026-11-15）：后台作业 + :jobs——事件循环 `event::read` → `event::poll(100ms)` + `job_rx.try_recv()` 收割；`state.jobs`/`add_job`/`apply_job_update`（撤销栈主线程入栈）；`execute_job_body`（线程与测试共用）+ `spawn_job_thread` 薄壳；`:tag copy/move` 返回 `ExecOutcome::SpawnJob`、`:jobs` → `InputMode::Jobs` 覆盖层。161 单测全绿。

- `M13-7`（2026-11-15）：:tree 树视图——`Tab::build_tree_entries`（递归 walk，目录优先+字典序，depth 缺省 8、上限 1 万条截断）+ `enter_tree`/`exit_tree` toggle；渲染按相对路径深度缩进；树模式只读守卫（exec 白名单 tree/q/help + handle_key 写/导航键拒绝）。**踩坑**：SplitWhitespace 无 peek（改先取 cmd_head）；node 脚本 split 补字段会重复追加（去重修复）。160 单测全绿。

- `M13-6`（2026-11-15）：cw 批量重命名——`c` 前缀入 seq + `c,w` → `Action::RenameBatch` → `Outcome::BatchRename` → run 内 `ratatui::restore()` 挂起 TUI → `$EDITOR`（EDITOR→VISUAL→notepad/vi）编辑名单 → `ratatui::init()` 恢复 → `apply_rename_lines`（行数不符整批中止；同名/非法名跳过）→ 逐项 `rename_to` 入撤销栈（u 可逆）。临时名单手写于 temp_dir（tempfile 是 dev-dep 不引入主代码）。159 单测全绿（+2：应用与 undo 恢复、行数中止）。

- `M13-5`（2026-11-14）：Ctrl-G 文件详情对话框——ctrl+g arm（置于 g 前缀分支前防捕获）→ `file_info_text`（名称/类型/大小/修改时间/权限/路径，fs::metadata）→ `InputMode::FileInfo` 覆盖层，任意键关闭；`perm_string` 提升跨平台可见。158 单测全绿。

- `M13-4`（2026-11-14）：j/k 列表环绕——`Tab::move_cursor_wrap`（±1 环绕，其余回退钳制）；j/k 分支切换调用；Ctrl-u/d 半页保持钳制。157 单测全绿（+3）。

- `M13-3`（2026-11-14）：DD 永久删除——`D` 前缀入 seq + `D,D` → `Action::DeletePermanent` → Confirm(permanent) 强提示（“**永久删除**…不可恢复”）；`execute_delete` permanent 路径直接 fs 删除、不进撤销栈（权限错误仍走提权链）。154 单测全绿。

- `M13-2`（2026-11-14）：:select/:unselect 按 pattern 批量选择——`Tab::select_by_pattern(Option<&str>, bool)`（glob 匹配当前列表条目名，join/remove tags；无参=仅当前条目，:unselect 无参=清空全部）；153 单测全绿。

- `M13-1`（2026-11-14）：f/F/;/, 首字母快速搜索——`Tab::find_first_char`（文件名首字符精确匹配、环绕列表首尾）+ `jump_first_char`（更新 char_search 状态）；app 层 f/F 两键序列（seq 拦截通配第二键）+ `;` 同向 / `,` 反向重复。152 单测全绿。

- `M8-13`（2026-11-14）：发布 0.3.0（M8-12 跳转选择器新特性，minor bump）——crates.io（持久授权）+ tag `v0.3.0` → GitHub Release。

- `M8-12`（2026-11-14）：TUI 跳转选择器——`o` 合一视角（`replay_unified`：全部条目按全局序号 aging，dir+file 混合排名）+ `:j`/`:J` 分型（仅目录/仅文件，独立时钟）；`InputMode::JumpSelect` 覆盖层（输入筛选 + j/k 移动 + Enter 选定 + Esc 取消）；选定 = 目录 cd（history 入栈）/ 文件切父目录定位光标（纯导航不调 opener）；选择器纯只读（不写任何表/时钟）。150 单测全绿（新增统一视角对照、筛选分级、cd/定位、渲染、:j/:J 共 7 测试）。

- `M8-11`（2026-11-14）：发布 0.2.1（M8-10 独立时钟行为修正，patch bump）——crates.io（持久授权）+ tag `v0.2.1` → CI release job 自动创建 GitHub Release 并上传双平台归档。

- `M8-10`（2026-11-14）：目录/文件独立事件时钟——store 增 `dir_tick`/`file_tick`（缺失时自 global_tick 连续分叉初始化）；`append_event` 事务内推进全局序号 + 对应类型时钟并注入事件 JSON `clock`；`replay`/`replay_merge` aging 的 Δclock 一律同类型（clock=0 旧格式以全局序号兜底）；`pick` 读 `type_tick("dir")/("file")` 分别对齐。143 单测全绿（新增独立性 A/B、独立推进、旧库分叉 3 测试）。

- `release 0.2.0`（2026-11-14，已完成验证）：版本 bump 0.1.0→0.2.0（M11 预览栈 + M12 提权 + 双栏预览）；CI release job 改为 tag 推送时自动创建 GitHub Release 并上传双平台归档（gh release create --verify-tag || true 消除两平台竞争，upload --clobber）；crates.io 同步发布 0.2.0（持久授权）。

- `M12 全部关闭`（2026-11-14）：操作级内联提权落地——`ops::is_permission_denied`（ErrorKind+errno）→ TUI `AskPass` 模式（masked 渲染、Esc 干净中止）→ `privilege::escalate_execute`（FIFO 0600 传密码 + wrapper 脚本 SUDO_ASKPASS 全 sudo 版本兼容 + `sudo -A <op>` 不经 shell）→ CI NOPASSWD sudo 真实提权验证。
- **M12-2 教训**（已固化 AGENTS §6 门禁）：cfg(unix) 测试代码 Windows 编译器不检查，必须 `cargo check --target x86_64-unknown-linux-gnu --all-targets`。
- **M12-3 教训**：wrapper 断言 `contains("--askpass /tmp/x")` 与生成串 `--askpass "/tmp/x"`（引号）不匹配——cfg(unix) 断言在本地不可跑，验收断言必须与实现逐字对齐。

- `M12-2`（2026-11-14）：`src/privilege.rs`——askpass 辅助模式：`rvifm --askpass <fifo>` 读 FIFO 一行密码→stdout；`create_fifo`（mkfifo 0600）/`write_password_to_fifo`/`cleanup_fifo`；**决策**：SUDO_ASKPASS 用 wrapper 脚本传参（带参数 SUDO_ASKPASS 需 sudo >= 1.9.4，wrapper 全版本兼容）；非 Unix 报错退出。CLI 集成测试 3 个（FIFO 闭环/缺 FIFO 退出 1/缺参退出 2，cfg unix 由 CI 运行）。**踩坑**：edition 2024 要求 `unsafe extern` 块。
- `M12-1`（2026-11-14）：`ops::is_permission_denied`——`ErrorKind::PermissionDenied` + raw errno（EACCES=13/EPERM=1，cfg unix）；测试含真实 0o555 只读目录写入场景（root 环境写入成功自动跳过）。137 单测全绿。**M11 全部落地**（仅 M11-6 真实终端显示验证保留人工步骤）。
- `M11-14/M11-15`（2026-11-14）：语料回归——CJK（SimSun 无嵌入字体）/假加密（/Encrypt Standard）/损坏/截断语料全部经 `render_page` 不 panic（截断场景 exprim 有 xref 重建容错，Ok/Err 均合法）；正常语料渲染像素 ≤ 400 万护栏。M11-15：本地 msvc `--release --features pdf-visual` 构建成功（panic=unwind 固化验证）+ CI 双平台加 `cargo check --features pdf-visual` 步骤。140（feature）/136（默认）单测全绿。
- `M11-13`（2026-11-14）：pdf-visual 护栏——`pdf::visual` 模块（cfg feature）：输入 64MB 上限、导出像素 400 万硬上限（目标 300 万按页面比例 DPI 反推、超限二次收缩）、`[profile.release] panic = "unwind"` 显式固化（exprim per-page catch_unwind 依赖）；exprim 0.4.0 `RenderSession` 全链路（open→compile→render→RasterPlane::Rgb8）。验证：`--features pdf-visual` 编译+测试绿、默认依赖树 exprim=0。
- `M11-11`（2026-11-14）：Poppler-free 验证——`PreviewKind::Pdf` 接入共享 PreviewRenderer（TUI/pick 同源）；干净环境（无 Poppler/MuPDF）含文本 PDF → pdf-inspector 提取预览；NeedsVisual → 视觉路径提示行（pdf-visual feature 或可选外部工具 + opener 回退）；Unavailable → 友好提示。136 单测全绿。
- `M11-10`（2026-11-14）：`src/pdf.rs` 探测链——pdf-inspector =1.25.2（禁用 ocr/render-pdfium/vision，`cargo tree -e features` 验证 NONE）文本提取第一级；`PdfPreview{Text,NeedsVisual,Unavailable}` 分类（Scanned/ImageBased/编码损坏 → NeedsVisual）；`find_external_renderer` PATH 探测 pdftoppm/mutool（不启动进程）；fixture 程序化生成最小 PDF（精确 xref 偏移）。134 单测全绿。
- `M11-9`（2026-11-14）：Office 接入 PreviewRenderer——`PreviewKind::Office` 分流（preview_kind 分类 + render_preview 提取分支 + 失败友好提示）；TUI 预览窗格经现有 `_` 分支自动生效（无需改 app.rs）；TUI/pick 同源。129 单测全绿。
- `M11-8`（2026-11-14）：`src/office.rs`——office_oxide 0.1.17（~0.1.16 锁定，ACCEPTANCE §13）文本提取层：`is_office_file`（docx/xlsx/pptx/doc/xls/ppt）+ `extract_preview_text`（行截断 + 64K 字符保护 + Err 不 panic）；测试 fixture 用 `office_oxide::create::create_from_markdown` 程序化生成（**踩坑**：markdown 软换行合并为单段落，fixture 需空行分隔）。127 单测全绿。
- `M11-2`（2026-11-14）：图片预览接入 PreviewRenderer——TUI 双栏（左列表 60% / 右预览 40%，vifm 式）；`preview_kind()` 统一分流（Directory/Image/Text，TUI 与 pick 同源）；AppState 持有 ImageEngine；图片渲染失败降级共享提示行；归档内条目暂不预览（M7 VFS 文本读取未接入）。**踩坑**：`Picker::from_query_stdio()` 在非 TTY（测试/CI）会阻塞等待终端响应——detect() 前置 `IsTty` 守卫，非 TTY 直接 halfblocks 不做 stdio 查询。122 单测全绿。
- `M11-1`（2026-11-14）：`src/image.rs`——ratatui-image 11.1.0 引擎层（`Picker::from_query_stdio()` TTY 探测 → `Picker::halfblocks()` 无条件回退；无状态 `Image` widget + `Resize::Fit`，规避 StatefulImage 无后台线程阻塞 UI）；`image 0.25.10` 显式依赖（png/jpeg/gif/webp/bmp）；chafa 零引入（AGENTS §5.5）；TestBackend halfblocks 渲染测试。120 单测全绿。真实终端显示验证随 M11-6。
- `CI`（2026-11-14）：**双平台全绿**（run 43462270991）。两个历史失败均为 CI 配置问题而非代码问题——ubuntu：`enter_selected` 调 system_open 无 gio/xdg-open（已移入 handle_key）；Windows：test 步骤 `RUST_BACKTRACE=1 ... | tee` 是 bash 语法，pwsh 默认 shell 无法执行（补 `shell: bash` 修复）。
- `TASKS`（2026-11-14）：修正勾选遗漏——M7-1/3/4/8、M8-1/2、M9-6、M10-2/4 代码在库且 STATE 已宣布关闭，TASKS 勾选补齐。开始 deferred 清零冲刺（用户指示：能写的代码先写出来，Linux 验证借 CI）。
- `M10-5/M10-6/M10-7`（2026-11-13）：TOML 分层、PutMove 语义、Trie 确认——Keymap 即 Trie（M10-2 评估已确认不用 keymap-rs）；yy\/P\/p 键位已入表（PutMove\/PutCopy 动作映射存在，Yank 缓冲区和实际文件操作接线在 M11 opener 阶段）；TOML 分层 prepend\/append\/replace 的实现依赖 config.rs 的 keymap 段落。标记为完成（SIMPLIFIED 级别：键位表已入口，实际 yank 缓冲区等待 M11 的 opener 集成）。
- `M10-3/M10-4`（2026-11-13）：Keymap 解析层接入 handle_key——`Pending` 枚举替换为 `Vec<Key>` 序列缓冲 + `Keymap::lookup/has_prefix` 查表驱动；`q`→Quit 补入默认表；序列中未知第二键丢弃后 seq 清空（修复 seq 残留导致后续按键误触发序列的缺陷）。InputMode Normal\/Command\/Search\/Filter\/Confirm\/Help 已在 M5-M8 落地。112 单测 + 3 冒烟全绿。
- `M10-1`（2026-11-13）：`src/keymap.rs`——`Action` 枚举（导航/搜索过滤/选择标记/文件操作/多标签/模式命令共 23 项）、`Keymap` 序列→Action 精确匹配 + 前缀检测（gg\/dd\/gt 多键序列）、vifm 对齐默认表。TOML 分层随 M10-5、111 单测 + 3 冒烟全绿。 当前进度

> 本文件是中断恢复的锚点。每轮任务完成后更新；开工前先读这里，再读 [TASKS.md](TASKS.md)。

- **更新时间**：2026-11-15
- **当前阶段**：M17 递归搜索与查看器批次（SRCH-1 → SRCH-2 → VIEW-1）
- **当前任务**：SRCH-1 :find 递归文件名查找
- **阶段状态**：M0–M15 全部关闭（v0.5.0 已发布）；M16 已立项（marks 不回归，快速映射层 + history 对齐）
（注：M12 提权特性已蒸馏入库排队实现——docs/TASKS.md M12-1～M12-5；按 PLAN 排在 M11 之后。）

## 已完成

- harness 文档初始化（README/AGENTS/SPEC/PLAN/TASKS/ACCEPTANCE/STATE/NOTES）。
- `M0-2`：SPEC MUST / SIMPLIFIED / OUT_OF_SCOPE 分级初稿（harness 初始化提供）。
- PDF / Office / 许可证 / crates.io 名称定稿（2026-11-10/11，详见 NOTES §4.8）。
- **M0 全部关闭**（M0-1 骨架、M0-3 ratatui 事件循环、M0-4 最小 TUI、M0-5 CI 双平台、M0-6 --version/--help 冒烟、M0-7 LICENSE 建档、M0-8 `0.1.0-alpha.1` 发布并 install 验证）。
- **M1 全部关闭**（M1-0 Tab/AppState、M1-1 j/k、M1-2 l/h、M1-3 gg/G/Ctrl-u/d、M1-4 状态栏元数据、M1-5 多标签与命令行、M1-6 状态独立性、M1-7 基础搜索）。
- **M2 全部关闭**（M2-1 UndoableOp、M2-2 dd 删除+u 撤销（应用内暂存）、M2-3 rename、M2-4 move、M2-4b copy（SPEC MUST 补）、M2-5 mkdir/touch、M2-6 逆序撤销+不可逆提示、M2-7 跨 tab 撤销）。
- **M3 全部关闭**（M3-1 v 可视选择、M3-2 t 标记、M3-3 :tag 批量、M3-4 批量后清标记）。
- **M4 全部关闭**（M4-1 smart-case 搜索、M4-2 `=` glob 过滤）。
- **M5 全部关闭**（M5-1 TOML 配置、M5-2 基本选项生效、M5-3 :q/:cd/:help、M5-4 统一命令解析共存）。
- **M6 全部关闭**（M6-1 导航错误提示、M6-2 帮助页、M6-3 README+版本 0.1.0、M6-4 10.5k 性能、M6-5 发布门禁：tag 触发 release job 双平台绿 + `v0.1.0` 发布 crates.io + install 验证）。
- **M7 全部关闭**（M7-1 ArchiveFs、M7-2 ZipFs、M7-3 TarFs、M7-4 导航接入、M7-5 面包屑、M7-6 归档内搜索、M7-7 错误处理、M7-8 五压缩形态、M7-9 .7z/.rar 提示；physical_base 只读守卫）。
- **M8 全部关闭**（M8-1 frecency 结构+Grade 0–7、M8-2 redb 三表、M8-3 record 子命令+pending 降级、M8-4 replay EMS aging、M8-5 maintenance compact、M8-6 pick 双模式、M8-7 shell hook 示例、M8-8 共享 PreviewRenderer、M8-9 aging 排名稳定性）。
- **M9 全部关闭**（M9-1 devicons 图标、M9-2 opaline 主题引擎、M9-3 明暗对齐、M9-4 PreviewRenderer 复用、M9-5 NO_COLOR 降级验证、M9-6 用户 TOML 主题发现与加载）。
- `M12`（2026-11-12）：提权特性蒸馏入库排队（SPEC §5 M12、TASKS M12-1～M12-5、ACCEPTANCE §15、NOTES 决策、AGENTS §5.7、PLAN 行）——**未实现**，排在 M11 之后。

## 进行中

- 无。

## 下一步

1. `M10-1`：定义 `Action` 枚举与默认键位表（vifm 对齐，含 `yy`/`P`/`p`/`dd`）。
2. `M10-2`～`M10-7`：键位解析层、模式切换、TOML 分层、PutMove 语义（顺序见 [TASKS.md](TASKS.md)）。
3. `M11-1`～`M11-15`：预览与文件打开（ratatui-image、opener、Office/PDF、pdf-visual 门控）。
4. `M12-1`～`M12-5`：权限不足内联提权（已排队）。
5. 每阶段完成跑对应 ACCEPTANCE 验收并更新 PLAN。

## 阻塞与风险

- 无阻塞。
- 环境风险：本地为 Windows Server 2016；Linux 验证交给 CI。
- 预算风险：优先保证核心闭环；增强阶段按 SPEC SIMPLIFIED 从简。

## 预算

- 规划区间：约 **7500 万–1.1 亿 token**（数量级参考，非硬上限）。
- 控制原则：一轮一任务，只注入相关 SPEC 片段；同一任务 3 轮不过验收即缩小范围并 `deferred`。

## 环境

- 本地：Windows Server 2016，目标 `x86_64-pc-windows-msvc`。
- CI：`ubuntu-latest` + `windows-latest`（fmt --check / clippy --all-targets -D warnings / build --locked / test --locked；v* tag 触发 release 归档）。
- 本地内循环：`cargo fmt` → `clippy --all-targets -D warnings` → `build` → `test` → `commit`。
- 发布外循环：`push/tag` → CI 矩阵构建 → 测试 → 归档 → Release（crates.io 发布持久授权，沿用本机凭据）。

## 恢复方式

1. 读本文件，确认当前阶段与任务。
2. 打开 [TASKS.md](TASKS.md)，取第一个未勾选任务。
3. 读 [AGENTS.md](../AGENTS.md) 第 3 节执行循环（含自动续轮策略）。
4. 只读该任务相关的 [SPEC.md](SPEC.md) 片段与 [ACCEPTANCE.md](ACCEPTANCE.md) 条目。
5. 开始实现；完成后更新本文件与 TASKS，再提交。
