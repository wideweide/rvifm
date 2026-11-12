# STATE — 当前进度

> 本文件是中断恢复的锚点。每轮任务完成后更新；开工前先读这里，再读 [TASKS.md](TASKS.md)。

- **更新时间**：2026-11-13
- **当前阶段**：M10 — 声明式键位系统（M0–M9 已完成）
- **当前任务**：`M10-1` 定义 `Action` 枚举与默认键位表（vifm 对齐，含 `yy`/`P`/`p`/`dd`）
- **阶段状态**：M0–M9 已完成；M10 进行中
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
