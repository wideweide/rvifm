# REVIEW — 评估报告索引与最新结论（M35-3：归档制）

> **本文件只保留最新评估的结论摘要与历史索引**；完整报告按批次归档在
> `docs/reviews/REVIEW-<日期>.md`（AGENTS §2 职责边界；M35-3 确立的归档制——
> 避免"主文过期、附录最新"的读者陷阱）。

## 历史报告

| 日期 | 报告 | 基线 | 状态 |
|---|---|---|---|
| 2026-11-19 | [REVIEW-2026-11-19.md](reviews/REVIEW-2026-11-19.md) | `9532cd0`（v0.8.x） | 已整改（AUDIT 18/18 闭环） |
| 2026-11-21 | [REVIEW-2026-11-21.md](reviews/REVIEW-2026-11-21.md) | `ba7c995`（v0.11.0） | M35–M40 由本报告蒸馏登记 |
| 2026-11-27 | [REVIEW-2026-11-27.md](reviews/REVIEW-2026-11-27.md) | `6cded08`（v1.4.0+） | M47–M85 全面审计 + 发布门禁结论 |
| 2026-11-28 | [REVIEW-2026-11-28.md](reviews/REVIEW-2026-11-28.md) | `0538dd5`（v1.5.0） | **最新**——用户复核报告；结论蒸馏为 M90–M95 |

## 历史评估（2026-11-21，v0.11.0 复核）结论摘要

- **总体**：M34 的小步交付质量明显好于早期批次；但 `ops.rs` / `remote/mod.rs` 与
  v0.9.0 逐字节相同——复制与传输两条主链未动（P0：复制丢元数据/链接、双读、
  整文件入内存、同名即失败）。
- **harness**：0 个未勾任务；但 AGENTS §10 / SPEC 7z-rar / STATE-TASKS-PLAN-README
  存在 6 处漂移（H-1~H-6）——**已随 M35-1 清零**；一致性校验固化为
  `scripts/harness_check.py` 七规则（M35-2，CI required）。
- **竞品差距**（按"会不会换回 TC/DC"排序）：①搬数据不可靠（M36）；
  ②归档只能看不能取（M37-2 :extract）；③检索结果不可操作（M38-1）；
  ④批量运维缺失（M37）；⑤远端只到能用（M39-4）。
- **完整报告与证据索引**：[reviews/REVIEW-2026-11-21.md](reviews/REVIEW-2026-11-21.md)。

## 结论投递

报告结论已按 AGENTS §8.2 蒸馏：M35–M40 → SPEC/TASKS/ACCEPTANCE/PLAN；
核查记录与实现期决策 → NOTES；本文档此后只做索引与摘要更新。

## 历史评估（2026-11-27，发布前全面审计）结论摘要

- M47–M85 实现与本地门禁全绿；CI 曾因 Windows CRLF / macOS 标题截断连续失败，已在 AUDIT-1 修复，以最新 main CI 为准。
- harness 文档矛盾（「不发布」口径）与用户授权对齐；ACCEPTANCE 未勾项完成自动/manual 分诊；tutor 验证条款落实为 14 模块 105 项严格计数测试。
- 发布建议：1.5.0（minor）、tag v1.5.0、三 musl 归档 + SHA256SUMS；crates.io 不发布；人工终端项保持 deferred。
- 完整证据见 [reviews/REVIEW-2026-11-27.md](reviews/REVIEW-2026-11-27.md)。

## 发布验证（2026-11-27，v1.5.0）

- main CI run 45993907109：三平台 test + fuzz + supply-chain 全绿。
- tag CI run 45996577565：release + release-checksums 全绿；GitHub Release 含 3 个 musl 归档与 3 行 SHA256SUMS，`sha256sum -c` 通过；aarch64 静态 ELF、x64-musl `--version = rvifm 1.5.0`。
- crates.io 未发布（停 1.0.0）；人工终端项保持 deferred。

## 最新评估（2026-11-28，v1.5.0 用户复核）结论摘要

- 代码面合格：生产路径零 `unwrap`/`expect`/`panic!`，fuzz/proptest/deny 齐备；预览与渲染栈异步化是最大技术跃升。
- 可信度是主要短板：main HEAD 三平台 CI 连红（Windows/macOS 主题与高亮断言）；crates.io 仍停 1.0.0 而 README 引导 `cargo install`；harness_check 三条正则退化且 R6 提前 `break`，实际给出假绿；100k 目录 6.83s 未优化。
- 结论按类型投递：可信度恢复 → M90；传输补完 → M91；目录性能 → M92；运维深度 → M93；质量债 → M94；体验治理 → M95。原稿 M89–M94 因 M89 已占用而顺延（见 NOTES）。
- 本轮仅蒸馏登记，等待用户确认后执行；报告原文见 [reviews/REVIEW-2026-11-28.md](reviews/REVIEW-2026-11-28.md)。

## 补充评估（2026-11-28，Arke 异步原则对照）

- 用户提供的 Arke 分析核心是「UI 线程永不阻塞」；rvifm 预览链已具备同构实现（M60/M61/M62/M83/M85 + generation/debounce）。
- 剩余真实缺口为乐观占位反馈与 LRU 缓存淘汰，已登记 M96；目录/运维异步化由 M92/M93 覆盖，不重复。
- 本结论只登记，等待用户确认后执行；原文不复制，按 §8.2 投递到 SPEC/NOTES。

## 最新评估（2026-11-28 复核，`~/rc.md`）结论摘要

- 基线 `2727040`（M100-2）：比 `v1.5.0` 领先 77 提交 / 50 文件，Release 仍是 v1.5.0、crates.io 仍 1.0.0——代码改进未交付给用户。
- 三平台 test job 已全绿，但 main run 46131729042 因 supply-chain `cargo deny check`（exit 4）整体 failure；harness_check 已修好却未接入 CI，R12 休眠。
- 100k 首屏 450.97ms 达成但口径改为「跳过全量 stat」，且 `lazy_attrs` 下 `:sort size/mtime` 产生排序正确性回归（P1）。
- 8 条新命令三处文档全缺；向导覆盖率按全命令口径 78%，非 93%。
- 结论投递：CI/文档可信度 → M101；排序正确性 → M102；队列后台化 → M103；可发现性 → M104。发布建议因用户既有「不发布」决策不采纳（见 NOTES）。
- 本轮仅蒸馏登记，等待用户确认后执行；报告原文见 [reviews/REVIEW-2026-11-28-rc.md](reviews/REVIEW-2026-11-28-rc.md)。

## 发布闭环（2026-11-28，v1.6.0）

- main CI run 46143616681（最终工作流修正后）：三平台 test + fuzz + supply-chain + harness consistency 全绿（自审修复 Windows 2s 加载阈值抖动后）。
- tag `v1.6.0`（指向 35f6ea0）→ tag run 46145048740：三平台 test + supply-chain + 3 release 腿 + release-checksums 全绿；GitHub Release 含 3 musl 归档与 3 行 SHA256SUMS。
- crates.io `rvifm 1.6.0` 已发布（API `max_version=1.6.0`），实现双渠道交付闭环。
- 自审追加：首次 tag run 46140235498 因 fuzz 25m 超时 + harness 在 tag 误注入 CI 结论而取消；已放宽 fuzz 到 45m、harness 仅 main 运行，重打 tag 后 46145048740 全绿；Release 归档由 `--clobber` 重传，版本不变。
- 发布前自审修复：deny 许可证精确豁免、harness 入 CI + 阶段上界规则、排序补齐、传输队列 spawn/暂停边角、CI 性能阈值抖动。
