#!/usr/bin/env bash
# 失败日志注解（匿名 API 只能读 check-run annotations）。
#
# M40-7 教训（2026-11-22）：旧模式 `grep -E "error" -A4` 会命中
# "Compiling thiserror/quick-error" 等 crate 名行，且 grep 输出的每一行
# （含上下文）各占一条注解——GitHub 10 条 error 注解/步的上限内，
# 真实 error 行被误报挤掉（tag run 44977467118 aarch64 腿实测）。
#
# 改为：行首 error 锚定（error[E...] / error:）+ 去 ANSI；首个错误块
# 一条注解携带 5 行上下文，其余行首 error 至多 9 条一行注解；
# 无行首 error 命中时回退输出日志尾部 30 行。
#
# 用法: bash annotate_errors.sh <logfile>
set -u
f=${1:?usage: annotate_errors.sh <logfile>}
[ -f "$f" ] || exit 0

clean="$f.clean"
# 去 ANSI（CARGO_TERM_COLOR=always 时 tee 文件含彩色序列，注解可读性差）
sed -e 's/\x1b\[[0-9;]*[a-zA-Z]//g' "$f" > "$clean" 2>/dev/null || cp "$f" "$clean"

# 行首 error 行号（≤10 条，GitHub error 注解上限/步）
nums=$(grep -nE '^error' "$clean" | cut -d: -f1 | head -10) || true
if [ -n "$nums" ]; then
  first=$(printf '%s\n' "$nums" | head -1)
  # 首个错误块：error 行 + 其后 5 行（escape 顺序：% → CR → LF）
  block=$(sed -n "${first},$((first + 5))p" "$clean" \
    | sed -e 's/%/%25/g' -e 's/\r/%0D/g' \
    | sed -z 's/\n/%0A/g' \
    | head -c 2500)
  echo "::error line=$first::build failure — first error block (line $first):%0A$block"
  printf '%s\n' "$nums" | tail -n +2 | while IFS= read -r n; do
    msg=$(sed -n "${n}p" "$clean" | head -c 400 | sed -e 's/%/%25/g')
    echo "::error line=$n::$msg"
  done
else
  tail=$(tail -30 "$clean" \
    | sed -e 's/%/%25/g' -e 's/\r/%0D/g' \
    | sed -z 's/\n/%0A/g' \
    | head -c 2500)
  echo "::error::no '^error' lines matched — log tail (30):%0A$tail"
fi
