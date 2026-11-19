# REVIEW — 项目全面评估报告（2026-11-19 · 第二轮复核）

> **评估基线**：`9532cd0`（本报告生成时的 `HEAD`，不是后续执行的锚点）。上一轮报告已归档至 [reviews/REVIEW-2026-11-19.md](reviews/REVIEW-2026-11-19.md)，本轮不沿用其结论，全部重新核验。
>
> **评估环境**：Linux（`x86_64-unknown-linux-gnu`），运行于 tmux 会话中（该事实影响第 5.6 节的测试结论）。未在 Windows / 真实图形终端 / 打包产物上复测。
>
> **本次改动**：只改文档，不改代码，**不提交**。结论按 AGENTS §8 投递：REQ → SPEC AUDIT 段；SEQ → TASKS / PLAN；HOW → ACCEPTANCE §26；WHY → NOTES §10。报告中的“当前/仍”均指评估基线 `9532cd0`；文档状态陈述已在同轮修正，执行时以 [STATE.md](STATE.md) 与 [TASKS.md](TASKS.md) 为准。

## 1. 结论摘要

| 维度 | 评价 | 说明 |
|---|---|---|
| harness 机制 | **专业** | 职责边界、一轮一任务、门禁、3 轮降级、蒸馏规则齐全，且本轮确实按该机制完成了复核与投递。 |
| 文档真实性 | **不合格（P0）** | README / STATE 仍把未接线的 marks、主题切换、opener 分层当作已完成能力；`M0–M25 关闭` 与 `M25-2` 未完成直接矛盾。 |
| 硬约束合规 | **1 处 P0 违反** | `pick` 在 `events.pending` 存在时写 redb 并推进时钟（已复现），违反 AGENTS §5.3。 |
| 功能完成度 | **核心链路好，边缘能力“有代码无接线”** | opener 整层、主题选择整层、PDF 视觉链、pick 预览与 `--topic` 契约均未进入生产路径。 |
| 架构与代码组织 | **可持续，但已到拆分阈值** | `app.rs` 15,021 行，占源码 55.4%；11 个模块用 `#![allow(dead_code)]` 掩盖未接线代码。 |
| 测试与 CI | **基本合格，存在环境依赖** | 336 单测 + 6 集成全绿；但 `TMUX` 存在时 1 个测试必失败，CI 因无 `TMUX` 而看不见。CI 缺 `clippy --all-targets` 与 Linux cross-check。 |
| 发布工程 | **良好** | v0.8.0 tag、Cargo.lock、双平台 CI、`panic = "unwind"`、`pdf-visual` 非默认依赖均符合约定。 |

**总体判断**：工程底座（模块划分、错误降级、撤销/归档/远端的边界）是扎实的；问题集中在**“文档声称的能力”与“生产路径实际接线”之间的系统性偏差**，以及 `pick` 只读红线。建议在 AUDIT 批次之外先补齐 5 个新登记项（见 §5），再恢复 M26；否则会继续在未验收的能力上叠加新功能。

## 2. 评估范围与方法

- 文档：`README.md`、`AGENTS.md`、`docs/STATE.md`、`docs/TASKS.md`、`docs/PLAN.md`、`docs/SPEC.md`、`docs/ACCEPTANCE.md`、`docs/NOTES.md`、`.github/workflows/ci.yml`、`Cargo.toml`、`.cargo/config.toml`。
- 代码：`src/` 全部 26 个 `.rs`（27,101 行）、`tests/cli.rs`；对 `opener` / `theme` / `pdf` / `image` / `preview` / `frecency` / `archive` / `remote` 做了调用方核查。
- 执行：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo build --locked`、`cargo test --locked -- --test-threads=1`（分别在默认环境与 `env -u TMUX -u STY` 下）、`rvifm --version/--help`，以及 `pick` / `record` / 归档读取的手工复现。
- 方法：静态阅读 + 符号引用核查（“有定义、无调用方”即判定未接线）+ 可复现行为验证。未做性能剖析、模糊测试与 Windows 实测。

## 3. 上一轮 AUDIT 复核

上一轮登记的 AUDIT-1～12 全部仍未勾选；本轮逐项核对代码，结论如下（不是重复上一轮结论，而是重新验证）：

| 项 | 代码现状（本轮核验） |
|---|---|
| AUDIT-1 `pick` 只读 | **仍违反**：`open_store()` 无条件 `merge_pending()`，`cmd_pick` 使用它；已复现 pending 被消费、DB 被创建。 |
| AUDIT-2 PDF 视觉链 | **未接线**：`find_external_renderer()` 与 `pdf::visual::render_page()` 无生产调用方；启用 `pdf-visual` 对 TUI 行为无影响。 |
| AUDIT-3 pick 预览 | **未接线**：`pick_interactive` 只渲染路径列表，不引用 `PreviewRenderer`。 |
| AUDIT-4 惰性 compaction | **未实现**：TUI 启动不调用 compaction；仅在打开 frecency 选择器时 `open_store()` 顺带合并 pending。 |
| AUDIT-5 `app.rs` 拆分 | **未动**：15,021 行，生产/测试各约一半。 |
| AUDIT-6 键位超时 | **未实现**：`Resolver` 无计时器，待决序列无限等待。 |
| AUDIT-7 opener 分层 | **比“缺 rules 层”更严重**：TOML 从未读取，`mime` 不存在、无命令执行器，`OpenResult` 仅测试使用。 |
| AUDIT-8 错误上浮 | **部分**：仍有 2 处 Mutex `unwrap()`；多处 `let _ = tab.refresh(...)` 静默吞错；已知 known_hosts 权限设置失败被忽略。 |
| AUDIT-9 CI 增强 | **未动**：clippy 不带 `--all-targets`；无 Linux target cross-check。 |
| AUDIT-10 重复实现 | **未动**：`tab::glob_match` 与 `opener::glob_match_simple` 两套语义；后者有 `*x*` 匹配缺陷。 |
| AUDIT-11 帮助同步 | **未动**：`help.txt` 仍缺 `:find` / `:grep`。 |
| AUDIT-12 测试补强 | **未动**：`app.rs` 测试占比仍约 50%。 |

结论：上一轮的 P0/P1 清单**仍然有效**；本轮新增的是“整层未接线”的确切证据，以及上一轮未覆盖的契约/环境缺陷。

## 4. P0 级问题

### P0-1 `pick` 仍违反只读红线（AGENTS §5.3）

- 证据：`src/frecency.rs:460-465` 的 `open_store()` 打开 redb 后立即 `merge_pending()`；`merge_pending()`（`src/frecency.rs:467` 起）逐条 `append_event()`，推进 `global_tick` 与类型时钟；`cmd_pick` 在 `src/frecency.rs:863` 调用 `open_store()`。
- 复现：在全新 `XDG_DATA_HOME` 下执行 `rvifm record --topic dir /x`（DB 目录缺失 → 事件降级写入 `events.pending`），随后执行 `rvifm pick < /dev/null`；`events.pending` 被删除并生成 `frecency.redb`。这不是“只读”。
- 影响：`pick` 变成隐式写入入口，破坏“record 是唯一写入入口”的契约；并发时还会与 `record`/TUI 争用 `global_tick` 序号。
- 方向：AUDIT-1（已拍板）——提供只读打开路径，pending 合并在 `record` / TUI 启动执行；补“存在 pending 时 pick 前后 DB mtime + tick 不变”的回归测试。

### P0-2 “已完成”的文档陈述与实现不符

这不是单个笔误，而是多处互相印证的过度声明：

| 文档位置 | 声明 | 代码事实 |
|---|---|---|
| `README.md:5,209,223,238` | marks 已由 frecency 实现并持久化 | **无** `m<char>` / `'<char>` Action 或键位处理；`m` 仅出现在排序对话框。`TASKS.md` 的 M25-2 未勾选。 |
| `README.md:42,54` | 主题内置且支持运行时切换、用户 TOML 主题 | `theme::set_builtin` / `set_user_theme` / `discover_user_themes` **零调用方**；无 `:theme` 命令、无配置项；`ACTIVE` 从未设置，`style()` 恒返回默认样式。 |
| `README.md:58` + 配置示例 | opener 三层（prepend/append/rules）与 mime 匹配 | `OpenerConfig` 只有 prepend/append；无 TOML 解析、无 mime、无 `rules`、无执行器；生产路径只调 `system_open()`。 |
| `README.md:149` | PDF 视觉不可用时探测 Poppler/MuPDF | 未调用 `find_external_renderer()`；只显示“需要启用 feature 或安装工具”的提示。 |
| `README.md:157,166` / `help.txt:74` | 键位配置在 `keymap.toml` | 实际从 `config.toml` 的 `[normal]` 读取（`config.rs:317-318`）。 |
| `docs/STATE.md:8` | `M0–M25 关闭` | 同段又承认 M25-2 重开；M9 / M11 的未接线项也未关闭。 |

- 影响：Agent 会按“已完成”跳过验证，用户会按不存在的配置项配置；这比单个功能缺失更危险，因为它破坏了 harness 的“文档即契约”。
- 已做：本轮同步修正 README / STATE / PLAN 的状态陈述；新增 AUDIT-13（主题接线）、AUDIT-15（pick CLI 契约）等任务，避免只改文字不改行为。

## 5. P1 级问题

### 5.1 opener 整层未接线（M11 名义完成）

- `src/opener.rs:10` 以 `#![allow(dead_code)]` 开头；`OpenerConfig` 无 `rules` 字段，`OpenerRule` 只有 `pattern` / `command`，无 `mime`、无 `use`/`run` 解析、无 `%s` 展开、无子进程执行。
- 全仓库仅 `app.rs:1032`（远端临时副本）与 `app.rs:3831`（本地打开）调用 `system_open()`；`resolve_opener()` 除测试外无调用方。
- `glob_match_simple`（`src/opener.rs:118-126`）先判 `strip_prefix('*')`，导致 `*foo*` 永远无法命中（该分支直接 `return n.ends_with("foo*")`，为 false）。现有测试只覆盖 `*.ext` / 精确名，未暴露。
- 结论：opener 不只是“少一个 `rules` 层”，而是**整层从未接入**。AUDIT-7 的范围需扩大到“TOML 加载 + mime/name 匹配 + 三层合并 + 命令执行 + glob 统一”。

### 5.2 主题引擎未接线（M9 名义完成）

- `theme.rs` 的 `set_builtin()`、`set_user_theme()`、`discover_user_themes()`、`load_user_theme()` 均无调用方；`ACTIVE` 只被 `style()` / `active_variant()` 读取，从未写入。
- 因此 `theme::style("ft_dir")` 恒为空样式，`filetype_style()` 一直走 16 色硬编码回退；用户无法选择内置主题，也无法加载 `~/.config/rvifm/themes/`。
- 图标明暗同样未生效：`icons::active_theme()` 依赖 `theme::icon_theme()`，而 `ThemeVariant` 恒为 Dark；远端列表还直接硬编码 `IconTheme::Dark`（`app.rs:1200/1261/1344`、`tab.rs:1228`）。
- 结论：M9 的“引擎层 + 测试”是真的，但“加载/切换/用户主题/明暗对齐”四项验收均未达到。登记 AUDIT-13。

### 5.3 PDF 视觉探测链是死代码（AUDIT-2）

- `pdf::find_external_renderer()`（`src/pdf.rs:64`）无调用方；`pdf::visual::render_page()`（`src/pdf.rs:123`，仅 `pdf-visual`）也无调用方。
- `preview::render_preview()` 的 PDF 分支（`src/preview.rs:100-112`）只处理 `Text` / `NeedsVisual` / `Unavailable`：`NeedsVisual` 直接输出提示，不尝试视觉渲染。
- 后果：启用 `pdf-visual` 编译通过但运行时无差别；安装 Poppler/MuPDF 也无差别。这与 SPEC §4.9 的三级回退及 ACCEPTANCE §13 的 PDF 条目不符。

### 5.4 `pick` 的 CLI 契约与首启行为

1. **`--topic` 未实现**：`cmd_pick` 把 `args.first()` 直接当初始查询（`src/frecency.rs:857-860`）。按 SPEC §4.6 的示例 `rvifm pick --topic dir` 调用时，查询词变成 `--topic`，几乎必然无命中。已复现：`printf '/a\n/b\n' | rvifm pick --topic dir` 退出码 1 且无输出；去掉参数则正常。
2. **文件候选按 dir topic 评分**：`src/frecency.rs:885` 固定 `topic = "dir"`，因此 `record --topic file` 的数据对 `pick` 完全无效。
3. **首次使用直接失败**：数据目录不存在时 `Store::open()` 返回 `NotFound`，`pick` 以 `Error: ... No such file or directory` 退出；只有先执行过 `record`/TUI 退出（降级路径创建目录）才可用。
4. **无预览**：见 AUDIT-3。

这四点都属于“契约已写、实现未达”，登记 AUDIT-15。

### 5.5 frecency 的一致性问题

- `cmd_pick`（`src/frecency.rs:857`）未调用 `init_settings()`，而 `cmd_record`（`:494`）与 TUI（`app.rs:5128`）会调用；因此 `[frecency]` 的 per-topic λ / `max_entries` 在 `pick` 中不生效。
- `cmd_compact`（`:797`）同样不初始化设置；`replay_merge()`（`:820`）把 λ 硬编码为 `AGING_LAMBDA = 0.01`，而统一视角默认应为 0.005（`:283-285`）。配置了 per-topic λ 后，compaction 会把 snapshot 分数按错误 λ 固化，造成与实时查询不一致。
- 惰性 compaction（AUDIT-4）仍未实现；当前只有手动 `maintenance compact` 与“打开 frecency 选择器时顺带 merge pending”。

登记 AUDIT-16（compaction λ + pick 设置初始化）。

### 5.6 测试非 hermetic：`TMUX` 决定门禁结果

- `src/app.rs:572` 的 `run_external_target()` 读取 `$TMUX` / `$STY` 决定 `Multiplexer` / `CurrentPane`；测试 `m18_4_bang_routes_and_config`（`:12565` 附近）未隔离环境。
- 本机（tmux 内）完整测试：**335 passed / 1 failed**，失败信息为 `expected RunExternal, got Ok(Done { msg: Some("已在新窗口执行: ...") })`；`env -u TMUX -u STY` 后 **336 + 6 全绿**。
- CI 没有 `TMUX`，所以长期看不见该失败；但 AGENTS §6 的“本地 `cargo test` 通过”在 tmux 用户机器上无法满足。这违反 AGENTS §7“集成测试不得依赖真实终端”的精神。
- 另有次要问题：该测试会向 stdout 打印外部命令的 `hi`，污染测试输出。
- 登记 AUDIT-14：测试显式清理/注入 `TMUX`/`STY`（或改为可注入的“终端策略”参数），并在 AGENTS §7 固化“环境变量影响行为的测试必须隔离”。

### 5.7 键位系统：无超时、仅 `[normal]` 可配

- `Resolver::feed()`（`src/keymap.rs:314-330`）只做前缀匹配，无超时；用户按下 `g` 后无限等待第二个键，`gg`/`gt` 等序列会“吃掉”后续单键，直到下一个键到达。
- `config.rs:289-291` 明确“当前支持 `[normal]`；其余模式保留解析框架”，而 SPEC §4.8 要求覆盖 Normal / Visual / View / Command-line 的模式状态；至少应在文档中把“仅 Normal 可配”标为 SIMPLIFIED，而不是让 README/help 呈现为通用三层。
- AUDIT-6 已登记超时；建议同时把“仅 `[normal]`”写进 SPEC 的 SIMPLIFIED，避免再次漂移。

### 5.8 生产错误被静默吞掉

- `app.rs` 多处 `let _ = tab.refresh(cfg);`（`:1616,1637,3725-3726,4383,4876,5995`）：目录权限变化、设备拔出、网络盘断开时，列表保持旧内容且无提示。
- `remote/known_hosts.rs:133,145`：`create_dir_all` 与 `set_permissions(0600)` 失败被忽略；在共享/多用户环境下可能留下错误权限的 known_hosts。
- `theme.rs:47,134`：`ACTIVE.set()` 的 `Err` 被丢弃（重复设置时静默失败）。
- `frecency.rs:293,302`：`Mutex` 被 poison 时 `unwrap()` panic（虽概率低，但属于 AUDIT-8 的“生产 panic 面”）。
- 这些不是“必须 panic”的错误，而是应当进入状态栏的可读提示。AUDIT-8 的范围应覆盖 `refresh` 与权限设置，而不只是 `unwrap`。

### 5.9 `app.rs` 上帝文件

- 15,021 行 / 27,101 行 = 55.4%；生产与测试各约一半。输入分发、命令执行、渲染、远端会话、后台作业、补全、对话框全部耦合在一个模块。
- 直接后果：任何跨功能改动都落在同一文件，冲突面大；测试只能通过 `press()` 驱动整条链路，难以定位失败；`#![allow(dead_code)]` 掩盖了未使用符号。
- AUDIT-5 已登记；建议拆分顺序为 `keys`（InputState/Outcome/Resolver 接线）→ `render`（pane/list/status/dialog）→ `exec`（命令与文件操作）→ `jobs`/`preview`/`remote`，每次只移动不改行为，测试随迁。

### 5.10 CI 门禁缺口

- `.github/workflows/ci.yml` 的 clippy 为 `cargo clippy -- -D warnings`，不含 `--all-targets`；测试代码的 lint 问题不会在 CI 暴露（本地 `--all-targets` 当前通过，属“现在没坏，但以后可能坏”）。
- 没有 `cargo check --target x86_64-unknown-linux-gnu --all-targets`：Windows 端不编译 `cfg(unix)` 测试代码，M12-2 的教训只靠本地自觉。
- AUDIT-9 已登记；这是增强项，不是现行规范违反，但应在 0.9.0 发布前补齐。

### 5.11 `#![allow(dead_code)]` 掩盖未接线代码

11 个模块文件级禁用 dead-code 警告：`pdf`、`opener`、`theme`、`archive`、`frecency`、`image`、`preview`、`privilege`、`keymap`、`office`、`store`。其中 `opener`、`theme` 正是因为该属性而没有在构建时暴露“整层无调用方”。

建议：改为对**单个符号** `#[allow(dead_code)]`，或至少在 CI 增加 `RUSTFLAGS="-D dead_code"` 的临时作业来盘点；这比事后人工核查可靠。

### 5.12 归档读取缺少体积上限

- `ZipFs::read()`（`src/archive.rs:152-165`）按归档声明的 `size` 调用 `Vec::with_capacity(size)`；恶意/损坏 zip 可声明超大解压尺寸，导致巨额分配。
- `TarFs` 的 xz 分支（`:242-247`）在开档时把整个归档解压进内存；没有归档大小/解压后大小上限。
- SPEC / 代码注释都写“内存占用受最大单成员约束”，但实际没有“最大单成员”的阈值与拒绝逻辑。登记 AUDIT-17：成员读取上限 + 超限可读错误 + 不 OOM。

## 6. P2 级问题

1. **调试残留**：`src/config.rs:181-182` 的 `#[cfg(test)] eprintln!("DBG parsed: {value:?}")` 会在测试时刷屏，应删除。
2. **任务编号重复**：`TASKS.md` 同时存在已撤销的原 `M25-2`（`[x] ~~...~~`）和重定义的 `[ ] M25-2`；提交信息无法区分。本轮把原条目改名为 `M25-2a（原定义，已撤销）`。
3. **验收不可勾选**：ACCEPTANCE 现有 401 个条目，其中 70 个 `[x]`、174 个 `[ ]`、157 个普通 bullet（本轮新增 AUDIT 项后统计）；M0–M13 多数里程碑（含 M7/M9/M11）用普通 bullet，无法证明“关闭”。本轮不批量改写（避免噪声），但建议在触及对应里程碑时转为复选框。
4. **帮助与 README 细节漂移**：`help.txt` 缺 `:find`/`:grep`（AUDIT-11）；README 配置示例的路径错误（`keymap.toml` → `config.toml`）；README 致谢仍列 `skim`（实际为自绘 picker）；README 称“pick 和 TUI 启动时重放评分”，实际 TUI 只在打开选择器时读库。
5. **图标主题硬编码**：远端列表构造路径硬编码 `IconTheme::Dark`（`app.rs:1200/1261/1344`、`tab.rs:1228`），即使将来主题切换生效也不会跟随。
6. **测试代码的 unsafe 与临时目录**：`tests/cli.rs:44-61,95-98` 为 FIFO 测试复制了两份 `unsafe` `mkfifo`，使用 `std::env::temp_dir()` 且未清理 FIFO；与 AGENTS §7 的 `tempfile` 约定不一致（`tempfile` 无法直接建 FIFO，但可封装一个 helper 并注明必要性）。
7. **deferred 标记不一致**：`TASKS.md` 的 `M11-6` 用 `[~]`（图例为“进行中/暂停”），STATE 用 `deferred`；建议统一为 `deferred` 语义，避免被误认为仍在执行。
8. **`pick` 与 TUI 的“同源”承诺**：SPEC §4.9 写 `PreviewRenderer` 由文件管理器与 `pick` 共用；代码中 pick 完全没有预览。AUDIT-3 已覆盖，二选一：接线或删承诺，不要继续保留两句。

## 7. 做得好的部分（避免只列问题）

- **硬边界守得住**：`Cargo.toml` 无 SQLite / fzf / FUSE / pdfium / tdf；`exprim` 仅 `pdf-visual` 可选且固定 `=0.4.0`；`profile.release` 显式 `panic = "unwind"`；归档只读、远端写走独立撤销变体。
- **撤销模型正确**：全局栈 + `tab_id` + 路径，跨 tab 撤销有明确语义；删除走应用内暂存，`u` 可逆。
- **降级路径有设计**：frecency 写失败降级 `events.pending`；图片协议探测失败回退 halfblocks；Office/PDF 解析失败给提示而不是 panic；git 查询异步、`--no-optional-locks` 不干扰用户仓库。
- **测试基线干净**：`env -u TMUX -u STY cargo test` 336 + 6 全绿；`cargo fmt --check`、`clippy --all-targets`、`build --locked` 全通过；`--version`/`--help` 可运行。
- **文档骨架专业**：AGENTS 的职责边界与蒸馏规则、SPEC 的 MUST/SIMPLIFIED/OUT_OF_SCOPE、ACCEPTANCE 的分阶段条目，在个人项目中属于上游水平；本轮的问题主要是“执行未闭环”，不是“没有制度”。

## 8. 整改建议与任务映射

| 优先级 | 问题 | 建议动作 | 任务 |
|---|---|---|---|
| P0 | `pick` 写入 | 只读 open + 回归测试 | AUDIT-1 |
| P0 | 文档/状态失真 | 本轮已修 README/STATE/PLAN；后续每次关闭必须回写验收 | 文档维护 |
| P1 | opener 整层未接线 + glob 缺陷 | TOML 解析、mime/name、三层、执行器、glob 统一 | AUDIT-7 + AUDIT-10 |
| P1 | 主题选择未接线 | `:theme` / 配置项 / 用户主题发现 / 明暗对齐 | **AUDIT-13（新）** |
| P1 | pick CLI 契约 | `--topic`、文件 topic、首启无 DB、预览 | **AUDIT-15（新）** + AUDIT-3 |
| P1 | PDF 视觉链 | 接线 exprim / 外部渲染器 / 提示 | AUDIT-2 |
| P1 | 惰性 compaction + λ 一致性 | 启动阈值触发；compaction 用 per-topic λ；pick 初始化设置 | **AUDIT-16（新）** + AUDIT-4 |
| P1 | 测试环境依赖 | 隔离 `TMUX`/`STY`，CI 增加变量矩阵或清除 | **AUDIT-14（新）** |
| P1 | 错误静默 | refresh / 权限 / Mutex / theme set | AUDIT-8 |
| P1 | `app.rs` | 按 keys/render/exec/jobs/preview 拆分 | AUDIT-5 |
| P1 | CI | clippy all-targets + Linux cross-check | AUDIT-9 |
| P1 | 归档读取上限 | 成员大小上限 + 超限错误 | **AUDIT-17（新）** |
| P2 | help/README 漂移 | 补 `:find`/`:grep`、修路径、去 skim | AUDIT-11 |
| P2 | 编号/验收/标记 | 已修 M25-2 编号；建议逐步复选框化 | 文档维护 |

**建议顺序**：AUDIT-1（红线）→ AUDIT-14（让门禁可信）→ AUDIT-9（CI）→ AUDIT-15（CLI 契约）→ AUDIT-7/13（两个“整层未接线”）→ AUDIT-2/3/4/16（frecency/PDF）→ AUDIT-5/8/10/11/12/17。

## 9. 评估限制

- 未在 Windows Server 2016（项目声明的主开发环境）上复测；未验证 Win2016 的 `start` 打开、回收站、路径大小写等差异。
- 未做真实终端图片协议（Kitty/Sixel）与 tmux passthrough 的手工验证；M11-6 仍是 deferred。
- 未做性能剖析：`app.rs` 拆分收益、归档 xz 内存峰值、超大目录刷新耗时均为静态判断。
- 未做模糊/变异测试；PDF 语料回归与 1,000 次变异是上一阶段声明的验收，本轮未复跑。
- `cargo test` 的“tmux 失败”已在本机复现，但未在无 tmux 的干净容器中跑完整套（仅用 `env -u` 模拟）。

## 10. 结论

`rvifm` 的**底层工程能力与 harness 设计是可信的**：核心浏览、撤销、归档只读、远端写、frecency 存储都有清晰边界，质量门禁在干净环境下全绿。当前真正的风险不是“代码不会写”，而是**“完成”的定义被文档提前兑现**：marks、主题切换、opener 分层、PDF 视觉链、pick 预览在文档里已经完成，在代码里却停在引擎层或完全无调用方。

因此建议：

1. **暂停 M26**，先把 AUDIT-1～17 中的 P0/P1 清完；
2. 每个任务关闭时，用 ACCEPTANCE 的**可执行条目**逐条勾选，而不是用“模块已存在”证明“能力已接入”；
3. 删除 `#![allow(dead_code)]` 作为“未接线”的遮羞布，让编译器重新成为第一道防线；
4. 在 0.9.0 发布前，至少完成 AUDIT-1/9/13/14/15 与一次双平台 + tmux 环境的门禁复跑。

在此之前，README 与 STATE 不应再出现“M0–M25 全部关闭”这类结论；本轮已按事实改写。
