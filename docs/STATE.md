# STATE — 当前状态

> 本文件是中断恢复的锚点：只回答“现在跑到哪、下一步是什么”。需求定义见 [SPEC.md](SPEC.md)，任务清单见 [TASKS.md](TASKS.md)，决策理由见 [NOTES.md](NOTES.md)，评估结论见 [REVIEW.md](REVIEW.md)。详细历史以 git log 为准，本文件不重复流水账。

- **更新时间**：2026-11-23
- **当前阶段**：**M0–M44 + FIX + REL-4 全部完成；v1.2.0 GitHub Release**（2026-11-23；发布产物 musl-only 三归档，aarch64 静态 musl 经 cargo-zigbuild；crates.io 不发布，见 PLAN §4）
- **当前任务**：无待办——FIX 批次（REL-3a/3b/5/6）与 REL-4（CI 配置 + 1.2.0 发布）全部完成。后续变更走 FIX/FEAT 蒸馏批次。
- **阶段状态**：M0–M38-3 + AUDIT + MISC 全部完成（450 单测 + 9 集成，checkpoint 2bff891）；M11-6 清单第 1 项 foot 已过；**0.9.0/0.10.0 经用户决策跳过不再发布**（版本序列 0.8.0 → 0.11.0 → 0.11.1）
- **基线**：不固化 commit hash，以执行时的 `HEAD` 为准；提交文档后 HEAD 变化属正常，无需回改本文档

## 进行中 / 暂停 / 延期

- M11-6：真实终端图片显示（M31-3 六步清单）——第 1 项已过（foot，2026-11-21，Kitty graphics）；2–6 待验（tmux passthrough / 无协议兜底 / NO_COLOR / 损坏图 / 大图）。
- M31-5：pdf-visual 护栏量化实测——**本机（rustc 1.100 ≥ 1.97）已可执行**（升级前受 1.95 阻塞），仍 deferred 待运行；foot 定性数据已记 NOTES。
- M28-1 Windows junction 回退人工验证；M31-2 装 7z 环境 cab/lzh 提示区分（人工，随 M11-6 模式保留）。
- PROC-1：预算门控与断点落地协议已生效（用户 2026-11-19 确认），为「自动续轮」第 3 条停止条件。
- REL-3b：**已完成**——用户鉴权后回填 v1.0.0/v1.1.0 的 `SHA256SUMS.txt`（各 4 行，API 核实）。

## 下一步

1. **无待办任务**：v1.2.0 GitHub Release 已落地（三 musl 归档 + SHA256SUMS）；后续问题按 FIX/FEAT 蒸馏登记。
2. **人工实测（顺带）**：真实终端跑 `:tutor` 13 模块与 `w`/`:view` 预览焦点；发现偏差按 NOTES 记录 + FIX 登记。
3. **crates.io**：按用户决策不发布；保留 1.0.0 为最新已发布版本，后续按需人工发起。

## 批次关闭记录

- FIX 批次（2026-11-23，用户实测反馈，蒸馏后确认执行）：3 任务——REL-3a CI 修复（4 并行 release job 各自生成/`--clobber` 上传 SHA256SUMS 导致只含 1 行 → 新增独立 `release-checksums` 汇总作业 + 数量守卫 4 + `scripts/repair-release-checksums.sh` 回填脚本）；FIX-5 tutor 覆盖层不透明（render.rs `TutorList`/底部 Panel 两处 Clear，fix5_* 双测试）；FIX-6 会话保存建目录（`save_session_with_path` 路径注入 + `create_dir_all`，fix6_* 双测试）。宿主门禁 506 单测 + 12 集成 + clippy/fmt/build 全绿；**CI run 45119651902 全绿**（test ubuntu/macos/windows + fuzz + supply-chain；release/release-checksums 在分支推送正确 skipped，仅 tag 触发）。FIX-5 初版测试在 macOS 暴露 TestBackend CJK 续格伪影（见 NOTES），已加固后 CI 复验通过。**REL-3b 历史回填已由用户人工完成**（v1.0.0/v1.1.0 各 4 行，API 核实）——批次 3/3 全清零。
- REL-4 + 1.2.0 发布（2026-11-23）：发布产物 musl-only——release matrix 3 腿（x64-musl / aarch64-musl / windows-x64），移除 gnu 腿；aarch64 由 gnu 动态链接改静态 musl，用 cargo-zigbuild + zig 0.14.1 交叉编译（musl.cc GCC 11 ICE / 裸 zig cc target 冲突，评估见 NOTES）；SHA256SUMS 数量守卫 3；版本 bump 1.2.0。首跑 tag run 45129475971 的 `release-checksums` 因无 checkout、gh 未显式 `--repo` 而上传失败（归档已上传，SHA256SUMS 缺）；加 `--repo "$GITHUB_REPOSITORY"` + tag 重指向后 **run 45132119778 九 job 全绿**；GitHub Release 三归档 + SHA256SUMS 3 行，下载实测 `sha256sum -c` 全通过，aarch64 产物静态 ELF + `--version` 通过。crates.io 未发布（停在 1.0.0）。
- M44 + 1.1.0 发布（2026-11-23）：预览焦点修复——`w`/`:view` 不再移动焦点，原 pane `j/k` 切换文件且对侧预览跟随；`Tab` 显式进入预览窗格滚动/搜索、再 `Tab` 返回；关闭恢复 normal 键位表。新增 `Tab.preview_focus` + `refresh_viewer_follow` + `ViewerState.path`，更新 3 个旧用例并新增 3 个回归用例；500 lib 单测通过（2 个 opener 用例在沙箱挂起已跳过）+ clippy all-targets 绿；**GitHub Release 1.1.0，crates.io 不发布（用户指令）**。
- M40 + 1.0.0 发布（2026-11-22）：M40-7 收官——发布修红三层：① aarch64 `c_char` E0308（privilege.rs mkfifo FFI `*const i8` 硬编码，aarch64 上 c_char=u8；匿名 API 日志通道全失效后本地 apt+dpkg 装交叉工具链完整复现，f955c67 修复）；② smoke 步骤对交叉产物不可执行（exit 2）→ qemu 冒烟在 runner 不稳（apt exit 100）→ 确定性 ELF 断言（ab102ec）；③ annotate_errors.sh 注解精确化（行首 error 锚定 + 仅失败发射）——tag 第 5 次重指向后 run 44984795450 全绿，GitHub Release 四归档 + SHA256SUMS；**crates.io publish 本机凭据执行成功**（用户 2026-11-22 在 Linux 机配置 `~/.cargo/credentials.toml`；`cargo package --locked` 干跑核验 73 文件 2.3MiB 后 `cargo publish --locked`，API 验证 newest/max=1.0.0）——**1.0.0 双渠道闭环，批次清单全清零**。宿主门禁 490 单测 + 12 集成 + aarch64 交叉构建 EXIT=0 全绿。
- M42 :tutor 导师模式（2026-11-22）：4/4 完成——src/tutor.rs（TutorModule/Item + assets/tutor 三模块 23 项 include_str + 进度持久化/错题本/review_order）+ InputMode::TutorList/TutorSession + 底部 Panel（左 prompt/expected 右进度键提示，复习态提示行，done 结算 Summary）+ `:tutor`/`:tutor <id>`/`:tutor reset <id>` 分派 + 四视图白名单；496 单测（m42_1×2 + m42_2 + tutor 单测×4）+ 12 集成；GUIDE/help/README/CHANGELOG 同步。
- M43 tutor 全功能覆盖（2026-11-22/23）：6/6 完成——M43-1 透明放行（TutorSession 迁 state.tutor 覆盖层、Ctrl+Y/N/Q 前置拦截、Esc 放行保清搜索语义、作答即时落盘、列表 List 滚动）+ M43-2 requires 字段（7 种 token，Panel 🔧 提示行）+ M43-3/4/5 内容三批（13 模块 100 项：core_nav 8/file_ops 9/search_frecency 7/search_filter 7/visual_tag 7/marks_frecency 8/tabs_history 8/panes_sort 8/archive_ops 5/maintenance 9/command_line 9/clipboard_git 5/remote_sftp 5/tutor_meta 5——含 core_nav 补 Space）+ M43-6 文档同步（README/help/GUIDE/CHANGELOG）与维护规则（SPEC M43-6 MUST）；499 单测（m43_1×2/m43_2）+ 12 集成。
- M43 登记（2026-11-22）：用户裁决——问题 5 全量 85 项/8 自评无异议/其余采纳建议（放行重构 Ctrl+Y/N·requires 字段·sandbox 引导·危险命令了解即可·M43 编号·维护规则入 SPEC）。批次 6 任务，见 SPEC M43 节。
- REL-2 门禁简化 + M42 登记（2026-11-22）：用户裁决撤销清单门——RELEASE_CHECKLIST.md 删除，SPEC REL/AGENTS §3·§10/PLAN §4/ACCEPTANCE §16 stub 改写为"crates.io publish 仅人工发起 + `:tutor` 自验"；M42 :tutor 导师模式登记（4 任务，SPEC M42 节/PLAN 批次表/ACCEPTANCE §42，目标 1.1.0 人工发起）。WHY 见 NOTES 同日条目。
- REL-1 发布流水线解耦 + 人工验证清单（2026-11-22）：GitHub Release 自动 / crates.io publish 人工验证门（AGENTS §3 修订 + SPEC REL 节 + PLAN §4 新规则）；`docs/RELEASE_CHECKLIST.md` 1.0.0 全量清单（11 组 ~68 项，按代码事实核验命令语法）；ACCEPTANCE §16（v0.6.0）/§35-A（0.11.0）收编为历史入口。WHY 见 NOTES 同日条目。
- M41：右栏一致性与交互缺陷修复 4 项全部完成（2026-11-21，用户实测当日闭环）——M41-1 右 pane `:` 命令模式 + gt/gT/Ngt/gg（seq 机制复用，ToTop 焦点化；单键 `g` 直跳退役，见 NOTES）、M41-2 方向键 ↑↓←→ = k/j/h/l（别名臂 + 默认 keymap + 树视图守卫扩展）、M41-3 鼠标点击切焦点 + 高亮双侧一致（hit_right_index + 滚轮焦点化，右键菜单保持左栏）、M41-4 视口滚动钉底修复（list_offset_sticky 有状态化 + 每帧 sync_viewport_scroll）；顺带修复 M37-1 chmod POSIX 死代码的 Windows clippy 违规；426 单测 + 6 集成全绿。

- 0.11.0 发布（2026-11-21）：M34 完成后自动发布——tag 首推 windows test 失败（M34-5 测试 runner 长路径截断，短合成路径修复）→ **tag 重指向后 CI 全绿 + GitHub Release + crates.io 上线**（本机凭据，用户部署；`cargo publish` + API 验证）；**0.9.0/0.10.0 经用户决策跳过不再发布**（版本序列 0.8.0 → 0.11.0）。
- M34：主题、预览统一与文档 10 项全部完成（2026-11-21）——M34-1 默认 gruvbox-dark（vifm 高亮组三层解析）、M34-2 捆绑 snowwhite、M34-3 右 pane `/` 搜索（焦点感知）、M34-4 对侧预览统一（preview_path 焦点修复 + render_view_content）、M34-5 frecency 视觉回显、M34-6 `.jar`、M34-7 二进制信息面板、M34-8 GUIDE.md、M34-9 预览尺寸适配（Fit 居中/fit-width/顶部切片）；433 单测 + 9 集成，本机 rustc 1.100 门禁绿（含 pdf-visual）。
- 0.10.0 发布（2026-11-21）：M28–M33 全部完成后自动发布——GitHub Release 三归档发布并验证（CI 5 job 全绿；发布期间修复 windows clippy dead-code 与 M32-3 Windows 盘符路径两缺陷，tag 重指向 2 次，CI 诊断注解通道固化，见 NOTES）；crates.io 停在 0.8.0，0.9.0/0.10.0 待 Windows（用户执行）。
- M0–M6：核心浏览、多标签、`u` 撤销闭环、选择批量、搜索过滤、配置与 v0.1.0 发布。
- M7–M11：只读归档 VFS、frecency（`pick`/`record`）、图标主题、声明式键位、预览与打开；M11-6 真实终端验证 deferred。
- M12：操作级内联提权（`sudo -A` + `--askpass`）；M12-2 教训固化为 Linux target 检查门禁。
- M13–M14：vifm 对齐增强（DD/tree/cw/:jobs/view）与双栏/标签栏；发布 0.4.0 / 0.5.0。
- M15–M17：用户实测反馈、导航增强、递归搜索与查看器；发布 0.6.0。
- M18–M20：排序与终端集成、SFTP 浏览/配置、远端写与跨 VFS。
- M21：UI/UX 打磨（F1–F9 + E1–E8），273 单测；发布 0.7.0。
- M22：SFTP 安全与远端体验补全（7 项），283 单测；发布 0.8.0。
- M23：frecency 对齐增强与 Pane 模式（12 项），292 单测；0.8.0。
- M24：交互增强与列表规范（9 项），318 单测 + 6 集成；列对齐、自然排序、行可视、计数前缀、命令历史、路径补全、`:compare`、`:jobs`。
- M25：会话持久化、归档创建/校验、异步 git 状态列与 `gi`、frecency 通用化评估；**M25-2 marks/bookmark 由 frecency 实现补齐（2026-11-20）**——`m<char>` 写入 keyword 事件（dir/file topic、`..` 行=当前目录）、`'<char>` 经 jump_to 支持 file 语义、`:bmark`/`:bmarks`/`:bmgo`/`:delbmarks` 命令族（`:delbmarks` 清除 = 空 keyword 事件保留累积分数）、多字符 keyword 搜索高优先级（M23-1 既有）。**M25 批次自此全部完成**；361 单测 + 5 集成。
- AUDIT：评估整改 18 项全部完成（2026-11-20）——pick 只读红线（redb ReadOnlyDatabase）、opener/主题/预览整层接线、app.rs 拆分 `src/app/` 7 模块、CI 门禁增强（clippy all-targets + Linux cross-check）、测试隔离与可移植性整改（含 Store::open 数据目录自动创建）、生产 panic 面清零、glob 统一修 `*x*`、help/README 同步、惰性 compaction 与 per-topic λ 一致性、归档成员/xz 解压上限；357 单测 + 5 集成双环境绿。
- M33：键位模式分层扩展 3 项全部完成（2026-11-20）——M33-1 三层 TOML 解析（ModeLayers）、M33-2 消费接线（visual/view 切换换表；[command] 消费面裁剪见 NOTES）、M33-3 文档收尾；418 单测 + 9 集成。**任务列表（M28–M33）自此全部完成，进入 0.10.0 自动发布。**
- M32：远端健壮性与会话扩展 4 项全部完成（2026-11-20）——M32-1 SFTP 自动重连（is_connection_class + as_any downcast）、M32-2 传输现场 .part（偏移续传裁剪见 NOTES/SPEC）、M32-3 会话持久化扩展（SessionTab.kind + 跳过提示接状态栏）、M32-4 文档收尾；416 单测 + 9 集成。
- M31：文档与验收补齐（2026-11-20）——M31-1～4 完成（README 边界/验收条目/M11-6 清单 6 步/复选框化 137 条）；M31-5 pdf-visual 实测 deferred（本机 rustc 1.95 < 1.97，CI 编译验证持续绿）。
- M29：对比增强 3 项全部完成（2026-11-20）——M29-1 `:compare bycontents`（SHA-256 分块哈希 + 64MB 护栏）、M29-2 dp/do 同步（DiffPut/DiffObtain Action + 可恢复删除 + 操作后重建；`!` 覆盖同步裁剪见 NOTES）、M29-3 文档收尾；399 单测 + 9 集成。
- M28：运维操作补齐 4 项全部完成（2026-11-20）——M28-1 `:link` 符号链接创建（junction 回退；undo_op symlink 感知修复）、M28-2 `:rename` 结构化批量重命名（RenamePreview 预览 + y 应用）、M28-3 `:jobs` 暂停/详情（JobControls/JobProgress）、M28-4 文档收尾；396 单测 + 9 集成；Windows junction 人工验证项随 M11-6 模式保留。
- MISC-1：`rv` 短别名二进制（2026-11-20）——Cargo 双 `[[bin]]` 同 `src/main.rs`（rv `test=false`）、CI release 归档含双二进制、README 三处说明、冒烟测试；385 单测 + 9 集成。**任务列表自此全部完成，进入 0.9.0 自动发布。**
- M27：0.8.0 实测问题与 vifm 视觉对齐 3 项全部完成（2026-11-20）——M27-1 符号链接目录跟随（lstat→path metadata 修正随 M27-3 验证落地）、M27-2 vifm 布局配色（pane_borders=false 默认 + 16 色回退）、M27-3 PDF 前 1–2 页图像可配置（[preview] 表 + 中断提交 3 缺陷修正，见 NOTES）；M27 批次自此全部完成；385 单测 + 8 集成。
- M26：归档创建扩展 5 项全部完成（2026-11-20）——`:archive` 后台作业化（archive_plan/execute_archive_job 分离，对话框携带作业参数）、7z 内建创建（sevenz-rust 0.6）、7z 虚拟浏览（SevenZFs，加密/损坏可读）、外部 CLI 降级（rar/cab/lzh/tar.xz/tar.bz2/tar.zst，PATH 探测 + 输出捕获）、README 归档创建行为表；362 单测 + 5 集成。

## 质量门禁

- 本地：`cargo fmt` → `cargo clippy --all-targets -- -D warnings` → `cargo build` → `cargo test` → `--version` / `--help` 冒烟。
- **当前已知门禁偏差**：Windows 本机不可执行 fuzz 靶编译（需 nightly+Linux）与 aws-lc-sys 交叉构建（AUDIT-9 固化进 ubuntu job）；**Linux 本机已全量绿**（2026-11-23：500 单测（沙箱跳过 2 个 opener 挂起用例）+ fuzz 工作区 `cargo check` + clippy all-targets）。**已闭环**：`cargo check --features pdf-visual` 双本机均可执行（≥ exprim MSRV 1.97）。
- 含 `cfg(unix)` / `cfg(test)` 改动：Windows 本机不编译 cfg(unix) 测试代码——以 CI ubuntu job 或 Linux 开发机全量运行为准（M12-2/M27-3 两度教训）；**fuzz 靶改动须在 Linux/CI 编译验证**（本轮两处编译错误 Windows 门禁天然不可见，教训见 NOTES）。
- 本地 rustc（Arch 1.99.1）的 dead-code 分析比 CI stable（≥1.100）更严——sftp 测试 `backend()` 死函数仅本地 clippy 报错，已删除；版本差异本身不构成门禁分叉，以 CI 结论为准。
- CI：`ubuntu-latest` + `windows-latest` + `macos-latest` 全绿；tag `v*` 触发三 target + aarch64 release 归档。

## 预算与风险

- 控制原则：一轮一任务，只注入相关 SPEC 片段；同一任务 3 轮不过验收即缩小范围并 `deferred`；一轮结束自动进入下一轮，无需人工确认（AGENTS §3）；M41 用户实测缺陷批次按 TASKS 节序先行。
- 主要风险：`pick` 只读红线（结构性保证，回归测试固化）；cfg(unix) 测试代码 Windows 本机不编译——以 CI ubuntu job 与 Linux 开发机全量运行为准（M27-3 连续发现两例潜伏缺陷，见 NOTES）；`pdf-visual` 在 rustc <1.97 的本机不可检查，CI 覆盖。
- 环境：双本机——Windows Server 2016（`x86_64-pc-windows-msvc`；crates.io 凭据所在）+ Linux 开发机（近期会话所在，foot 实测终端，rustup stable 1.100）；其它平台交给 CI（ubuntu-latest / windows-latest）。

## 环境

- 本机：双环境——Windows Server 2016（目标 `x86_64-pc-windows-msvc`；crates.io publish 在此执行）+ Linux 开发机（CachyOS，系统 rust **1.99.1**，**rustup 已不在**——夜间 fuzz 长跑需先装 rustup+nightly；crates.io 与 static.rust-lang.org 直连超时，走用户代理 `socks5://192.168.1.88:15235`；gh 未安装，匿名 GitHub API 可读 run/job/check-run 注解）；foot 终端实测。
- CI：`ubuntu-latest` + `windows-latest` + `macos-latest`（fmt --check / clippy -D warnings / build --locked / test --locked / pdf-visual check / fuzz job / supply-chain job）。
- 发布外循环：`push/tag` → CI 矩阵构建 → 测试 → 归档 → GitHub Release（crates.io 发布沿用 Windows 机器凭据）。

## 恢复方式

1. 读本文件确认当前阶段与任务。
2. 打开 [TASKS.md](TASKS.md)，取第一个未勾选任务——**当前无可自动执行任务（全部勾选或明确 deferred）**。
3. 读 [AGENTS.md](../AGENTS.md) §3 执行循环。
4. 只读该任务相关的 [SPEC.md](SPEC.md) 片段与 [ACCEPTANCE.md](ACCEPTANCE.md) 条目。
5. 完成后更新本文件与 TASKS，再提交。
