# REVIEW — 评估报告索引与最新结论（M35-3：归档制）

> **本文件只保留最新评估的结论摘要与历史索引**；完整报告按批次归档在
> `docs/reviews/REVIEW-<日期>.md`（AGENTS §2 职责边界；M35-3 确立的归档制——
> 避免"主文过期、附录最新"的读者陷阱）。

## 历史报告

| 日期 | 报告 | 基线 | 状态 |
|---|---|---|---|
| 2026-11-19 | [REVIEW-2026-11-19.md](reviews/REVIEW-2026-11-19.md) | `9532cd0`（v0.8.x） | 已整改（AUDIT 18/18 闭环） |
| 2026-11-21 | [REVIEW-2026-11-21.md](reviews/REVIEW-2026-11-21.md) | `ba7c995`（v0.11.0） | M35–M40 由本报告蒸馏登记 |
| 2026-11-27 | [REVIEW-2026-11-27.md](reviews/REVIEW-2026-11-27.md) | `6cded08`（v1.4.0+） | **最新**——M47–M85 全面审计 + 发布门禁结论 |

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

## 最新评估（2026-11-27，发布前全面审计）结论摘要

- M47–M85 实现与本地门禁全绿；CI 曾因 Windows CRLF / macOS 标题截断连续失败，已在 AUDIT-1 修复，以最新 main CI 为准。
- harness 文档矛盾（「不发布」口径）与用户授权对齐；ACCEPTANCE 未勾项完成自动/manual 分诊；tutor 验证条款落实为 14 模块 105 项严格计数测试。
- 发布建议：1.5.0（minor）、tag v1.5.0、三 musl 归档 + SHA256SUMS；crates.io 不发布；人工终端项保持 deferred。
- 完整证据见 [reviews/REVIEW-2026-11-27.md](reviews/REVIEW-2026-11-27.md)。
