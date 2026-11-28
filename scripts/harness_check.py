#!/usr/bin/env python3
"""M35-2：harness 一致性检查（七条规则）。

CI（required check）与本地 pre-commit 共用。退出码非 0 = 存在漂移。
用法：python3 scripts/harness_check.py [repo_root]
"""
import re
import sys
from pathlib import Path

root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).parent.parent

def read(*parts):
    return (root.joinpath(*parts)).read_text(encoding="utf-8", errors="replace")

problems = []

def check(rule, ok, detail):
    if not ok:
        problems.append(f"[{rule}] {detail}")

# ── 读文档 ─────────────────────────────────────────────────────────────
tasks = read("docs", "TASKS.md")
state = read("docs", "STATE.md")
agents = read("AGENTS.md")
plan = read("docs", "PLAN.md")
readme = read("README.md")
spec = read("docs", "SPEC.md")

# 批次段抽取：## Mxx — 标题（含 ✅/执行中）
batch_heads = re.findall(
    r"^## ((?:M\d+|FIX-[\d-]+|REL-\d+)) — [^\n]*?(✅ 已完成[^\n]*|执行中[^\n]*|待确认[^\n]*)?$",
    tasks,
    re.M,
)
# 每批次内未勾选任务数
def unchecked_in_batch(batch_head_line):
    idx = tasks.find(batch_head_line)
    nxt = tasks.find("\n## ", idx + 1)
    body = tasks[idx:nxt if nxt >= 0 else len(tasks)]
    return len(re.findall(r"^- \[ \\]", body, re.M))

# ── 规则 1：STATE 不得在某批次全部勾选后仍写"执行中" ─────────────────
for head, status in batch_heads:
    mid = head  # findall 的组已是批次号（无 ## 前缀）
    if "执行中" in status or "▶" in status:
        # 批次标题写执行中——规则 3 一并覆盖（TASKS 自身）
        n = unchecked_in_batch(head)
        check("R1/R3-TASKS", n > 0,
              f"TASKS 批次 {mid} 标记执行中但未勾任务 {n} 个（应为已完成或真有未勾）")
    # 同分句限定（[^;。\n]）——避免"M0–M34 完成；M35 执行中"这类列举行误报。
    state_m = re.search(
        r"当前阶段[^;；。\n]*" + re.escape(mid)
        + r"[^;；。\n]*执行中", state)
    if state_m:
        n = unchecked_in_batch(head)
        check("R1-STATE", n > 0,
              f"STATE 写 {mid} 执行中，但 TASKS 中该批次未勾任务 {n} 个")

# ── 规则 2：AGENTS §10 当前阶段/任务/基线与 STATE 一致 ────────────────
state_ver = re.search(r"(\d+\.\d+\.\d+)[^\n]{0,40}已发布", state)
agents_ver = re.search(r"(\d+\.\d+\.\d+)[^\n]{0,40}已发布", agents)
if state_ver and agents_ver:
    check("R2-AGENTS", state_ver.group(1) == agents_ver.group(1),
          f"AGENTS 基线版本 {agents_ver.group(1)} ≠ STATE {state_ver.group(1)}")
if "AUDIT-1" in agents and "AUDIT 18/18" in state:
    check("R2-AGENTS", False, "AGENTS §10 仍指向 AUDIT-1（STATE 显示 AUDIT 已完成）")

# ── 规则 3：TASKS 批次标题与勾选状态一致（已并入规则 1 循环） ─────────

# ── 规则 4：PLAN 版本小节标题与正文一致 ──────────────────────────────
for m in re.finditer(r"^### (\d+\.\d+\.\d+)(（[^）]*）)\n", plan, re.M):
    ver, paren = m.group(1), m.group(2)
    body = plan[m.end():m.end() + 800]
    head_ok = ("已发布" in paren) == bool(
        re.search(r"GitHub Release 已发布|已发布并验证", body))
    skipped = "跳过" in body
    if not head_ok and not skipped:
        check("R4-PLAN", False,
              f"PLAN 版本 {ver} 小节标题（{paren.strip('（）')}）与正文发布结论不一致")

# ── 规则 5：SPEC SIMPLIFIED/OUT_OF_SCOPE 与 README 能力表不冲突 ────────
# 人工白名单：SPEC 明确写"已升级/已交付"的例外不算冲突。
whitelist = [
    (".7z", "M26-3"),
    ("虚拟浏览已由 M26-3", "M26-3"),
]
for m in re.finditer(r"^(- `\.[a-z0-9]+` /? ?(?:`\.[a-z0-9]+` )?(?:不提供|不支持)虚拟浏览[^\n]*)$", spec, re.M):
    line = m.group(1)
    if ".rar" in line and ".7z" not in line:
        continue  # 仅 rar 的提示与 README（rar 保持提示）一致
    if any(w[0] in line and w[1] in line for w in whitelist):
        continue
    check("R5-SPEC", False, f"SPEC 仍写不支持虚拟浏览（与 README 能力表冲突）: {line[:80]}")

# ── 规则 6：文档测试数字 = src/ 内 #[test] 总数 ─────────────────────
n_tests = sum(
    rs.read_text(encoding="utf-8", errors="replace").count("#[test]")
    for rs in (root / "src").rglob("*.rs")
)
# 只校验 STATE 的"当前阶段/阶段状态"两行——批次关闭记录中的数字为
# 关闭时点快照（非当前值），不在校验范围。
if n_tests:
    for line in state.splitlines():
        if line.startswith("- **当前阶段**") or line.startswith("- **阶段状态**"):
            for m in re.finditer(r"(\d+) 单测", line):
                if int(m.group(1)) != n_tests:
                    check("R6-COUNT", False,
                          f"STATE 状态行写 {m.group(1)} 单测，实际 {n_tests}"
                          f"（口径：src/ 内 #[test] 计数）: {line[:60]}")

# ── 规则 7：README 版本状态与 STATE 发布状态一致 ──────────────────────
rm_publishing = "发布中" in readme[:600]
st_published = bool(re.search(r"已发布.*GitHub Release|GitHub Release 已发布", state[:1500]))
if rm_publishing and st_published:
    check("R7-README", False, "README 写'发布中'而 STATE 已记录发布完成")

# ── 规则 8：向导模块 ↔ ACCEPTANCE 映射表完整（M56-10） ───────────────
notes = read("docs", "NOTES.md")
acceptance = read("docs", "ACCEPTANCE.md")
s = notes.find("<!-- TUTOR-ACCEPTANCE-MAP:START -->")
e = notes.find("<!-- TUTOR-ACCEPTANCE-MAP:END -->")
map_text = notes[s:e] if s >= 0 and e > s else ""
module_ids = [p.stem for p in (root / "assets" / "tutor").glob("*.toml")]
mapped = set(re.findall(r"^\|\s*([a-z_]+)\s*\|", map_text, re.M))
missing_modules = [m for m in module_ids if m not in mapped]
check("R8-TUTOR-MAP", bool(map_text) and not missing_modules,
      f"NOTES 向导映射表缺失模块: {missing_modules}" if missing_modules
      else "NOTES 缺少 TUTOR-ACCEPTANCE-MAP 标记块")
sections = set(re.findall(r"^## (\d+)\.", acceptance, re.M))
bad_refs = [ref for ref in re.findall(r"§(\d+)", map_text) if ref not in sections]
check("R8-TUTOR-MAP", not bad_refs,
      f"向导映射表引用了不存在的 ACCEPTANCE 章节: {bad_refs}")

# ── 规则 9：README/GUIDE/help 无过期 tutor 数字与 vifm 预览错误表述 ──
readme_text = readme
guide_text = read("docs", "GUIDE.md")
help_text = read("src", "app", "help.txt")
stale_docs = [
    name
    for name, text in (
        ("README.md", readme_text),
        ("docs/GUIDE.md", guide_text),
        ("src/app/help.txt", help_text),
    )
    if "13 模块 100 项" in text or "13个模块100项" in text
]
check("R9-DOCS", not stale_docs, f"过期 tutor 条目数（应为 14 模块 105 项）: {stale_docs}")
check("R9-README", "无内建能力" not in readme_text,
      "README 仍称 vifm 文档预览『无内建能力』（实际 :view 内建文本/目录预览）")

# ── 结果 ─────────────────────────────────────────────────────────────
if problems:
    print(f"harness_check: {len(problems)} 项不一致")
    for p in problems:
        print("  " + p)
    sys.exit(1)
print("harness_check: OK（九条规则全部通过）")
