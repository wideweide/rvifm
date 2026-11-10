# STATE — 当前进度

> 本文件是中断恢复的锚点。每轮任务完成后更新；开工前先读这里，再读 [TASKS.md](TASKS.md)。

- **更新时间**：2026-11-11
- **当前阶段**：M0 — 骨架 + SPEC + 双平台 CI
- **当前任务**：`M0-8` 发布 `0.1.0-alpha.1`（人工执行，crates.io 凭据不进 CI）
- **阶段状态**：进行中

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
- `M0-5`（2026-11-11）：`.github/workflows/ci.yml`——`ubuntu-latest` + `windows-latest` 矩阵，步骤 checkout → dtolnay/stable（含 rustfmt/clippy）→ rust-cache → `fmt --check` / `clippy -D warnings` / `build --locked` / `test --locked`。**CI 状态绿待验证**：策略为 M0-7 完成后统一 push 触发一次双平台运行（本地无 TTY 也无法观察 CI）。
- `M0-6`（2026-11-11）：`--version` / `-V` / `--help` / `-h` 旗标（TUI 初始化前处理，无 TTY 可跑）；未知参数 stderr 提示并以 exit 2 退出；`tests/cli.rs` 3 项冒烟（版本输出、帮助输出、未知参数非零退出）。
- `M0-7`（2026-11-11）：`LICENSE-MIT` / `LICENSE-APACHE` 全文建档，`Cargo.toml` 设 `license = "MIT OR Apache-2.0"`；README 许可证段原本即一致，未改动。`cargo package --list` 验证通过（含两个 LICENSE 文件）。遗留：manifest 无 `repository`/`homepage` 的 warning 留待 M0-8 发布前补。

## 进行中

- 无。

## 下一步

1. `M0-8`：M0 全部任务完成且双平台 CI 全绿后，人工发布 `0.1.0-alpha.1` 并用 `cargo install` 验证。
2. 跑通 M0 验收，进入 M1。
3. M11 阶段按 M11-12～M11-15 落地 `pdf-visual` 门控、资源护栏、语料回归与 Windows 构建（不改变 M0 优先级）。

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
