# STATE —
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

- **更新时间**：2026-11-14
- **当前阶段**：deferred 任务清零冲刺（M11/M12 未实现项逐个落地）
- **当前任务**：M8-13 发布 0.3.0
- **阶段状态**：M0–M12 关闭 + M8-10/11 增补闭环（M11-6 真实终端验证保留人工步骤）
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
