#!/usr/bin/env bash
# 本地构建完整的版本化文档站（与 CI 同一条流水线）：
#   /        = 最新 release 分支（无横幅）
#   /vX.Y/   = 旧版本（带旧版横幅）
#   /next/   = 当前工作区（含未提交改动，带开发版横幅）
#
# 版本清单由 docs/scripts/resolve-versions.sh 从 git tag + release/* 分支解析，无静态表。
# 输出到 docs/.vitepress/dist-all，配合 bun run docs:preview:all 查看。
set -euo pipefail

DOCS_DIR="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$DOCS_DIR/.vitepress/dist-all"

rm -rf "$OUT"

echo "版本清单:"
"$DOCS_DIR/scripts/resolve-versions.sh" | tee /tmp/asterdrive-docs-versions.tsv

while IFS=$'\t' read -r version path ref latest; do
  if [ "$latest" = "true" ]; then
    "$DOCS_DIR/scripts/build-version.sh" "$ref" / "$OUT"
  else
    "$DOCS_DIR/scripts/build-version.sh" "$ref" "$path" "$OUT$path" \
      "这是 v$version 的旧版文档。" "/" "查看最新版本 →"
  fi
done < /tmp/asterdrive-docs-versions.tsv

# /next/ 从当前工作区构建，能看到未提交的改动
(
  cd "$DOCS_DIR"
  VITEPRESS_BASE=/next/ \
  VITE_VERSION_BANNER="这是开发中版本的文档，内容可能尚未发布。" \
  VITE_VERSION_BANNER_URL="/" \
  VITE_VERSION_BANNER_LINK_TEXT="查看稳定版本 →" \
    bun run docs:build >/dev/null
)
cp -R "$DOCS_DIR/.vitepress/dist" "$OUT/next"

echo
echo "完整版本化站点已输出到 $OUT"
echo "预览: bun run docs:preview:all"
