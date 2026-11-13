# TASKS — rvifm 任务清单

**本文件是唯一任务来源。** 每轮只取第一个未勾选任务，完成后勾选并更新 [STATE.md](STATE.md)。

- 任务粒度：一次提交能完成。
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
- [~] M11-11 Poppler-free 验证（deferred：依赖 M11-8/M11-10）
- [x] M11-12 `pdf-visual` feature 门控与依赖边界（验证：default tree 无 exprim，--features pdf-visual 含 0.4.0）
- [~] M11-13 pdf-visual 资源护栏（deferred：需 exprim 编译验证）
- [~] M11-14 pdf-visual 语料回归（deferred：需 exprim 编译验证）
- [~] M11-15 pdf-visual Windows 构建验证（deferred：需 exprim 编译验证）

## M12 — 权限不足时的内联提权（2026-11-12 增补）

- [~] M12-1 检测写操作 EACCES（deferred：需 Linux 测试环境，设计已入库 NOTES/SPEC）
- [~] M12-2 askpass 辅助模式（deferred：需 Linux 测试环境）
- [~] M12-3 sudo -A 重试（deferred：需 Linux 测试环境）
- [~] M12-4 取消干净中止（deferred：需 Linux 测试环境）
- [~] M12-5 vifm 场景验证（deferred：需 Linux root 测试环境）

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
