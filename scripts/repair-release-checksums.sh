#!/usr/bin/env bash
# REL-3：回填历史 GitHub Release 的 SHA256SUMS.txt（含全部归档）。
#
# 背景：v1.0.0 / v1.1.0 发布时 ci.yml 的 4 个并行 release job 各自生成并
# --clobber 上传 SHA256SUMS.txt，最后完成的 job 覆盖其余，导致文件只含
# 1 个归档。此脚本按 Release 上实际归档重算完整清单并覆盖上传。
# CI 侧已由独立 release-checksums 作业修复（未来 tag 不再出现此问题）。
#
# 用法：
#   gh auth login                       # 需要 contents:write 权限
#   bash scripts/repair-release-checksums.sh v1.2.0          # REL-4 起 3 归档
#   EXPECTED_ARCHIVES=4 bash scripts/repair-release-checksums.sh v1.0.0 v1.1.0
#       # 历史 4 归档版本（REL-4 之前）需显式覆盖数量守卫
#
# 说明：脚本只重算/上传 SHA256SUMS.txt，不改归档、不改 tag、不发布 crates.io。
set -euo pipefail

if ! command -v gh >/dev/null 2>&1; then
  echo "错误：需要 gh CLI（https://cli.github.com）" >&2
  exit 1
fi
if [ "$#" -eq 0 ]; then
  echo "用法: $0 <tag> [tag ...]" >&2
  exit 1
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

for tag in "$@"; do
  echo "==> $tag"
  dir="$tmp/$tag"
  mkdir -p "$dir"
  # 只下载归档，不下载旧 SHA256SUMS.txt（避免被计入哈希）
  gh release download "$tag" --repo wideweide/rvifm \
    --pattern '*.tar.gz' --dir "$dir" --clobber
  expected=${EXPECTED_ARCHIVES:-3}
  n=$(find "$dir" -maxdepth 1 -name '*.tar.gz' | wc -l)
  echo "归档数: $n（期望 $expected）"
  if [ "$n" -ne "$expected" ]; then
    echo "错误：$tag 期望 $expected 个归档，实得 $n — 中止，不上传（防止不完整清单）" >&2
    exit 1
  fi
  (cd "$dir" && sha256sum *.tar.gz > SHA256SUMS.txt)
  cat "$dir/SHA256SUMS.txt"
  gh release upload "$tag" "$dir/SHA256SUMS.txt" --repo wideweide/rvifm --clobber
  echo "==> $tag 已回填完整 SHA256SUMS.txt"
done
