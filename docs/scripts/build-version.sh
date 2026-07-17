#!/usr/bin/env bash
# 构建指定 ref（tag / 分支）的文档站点，输出到指定目录。
#
# 用法: docs/scripts/build-version.sh <ref> <base-path> <output-dir> [banner-text] [banner-url] [banner-link-text]
#
# 老 ref 的 config.ts 不支持 VITEPRESS_BASE 时，构建前自动 sed 注入 base；
# 主题不支持 VITE_VERSION_BANNER 时，横幅改为构建后 HTML 注入。
set -euo pipefail

REF="${1:?用法: build-version.sh <ref> <base-path> <output-dir> [banner-text] [banner-url] [banner-link-text]}"
BASE_PATH="${2:?缺少 base-path}"
OUT_DIR="${3:?缺少 output-dir}"
BANNER_TEXT="${4:-}"
BANNER_URL="${5:-}"
BANNER_LINK_TEXT="${6:-}"

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WORKTREE_PARENT="$(mktemp -d)"
WORKTREE="$WORKTREE_PARENT/src"

cleanup() {
  git -C "$REPO_ROOT" worktree remove --force "$WORKTREE" 2>/dev/null || true
  rm -rf "$WORKTREE_PARENT"
}
trap cleanup EXIT

git -C "$REPO_ROOT" worktree add --detach "$WORKTREE" "$REF" >/dev/null

CONFIG="$WORKTREE/docs/.vitepress/config.ts"
LEGACY=0
if ! grep -q 'VITEPRESS_BASE' "$CONFIG"; then
  LEGACY=1
  echo "ref $REF 的配置不支持 VITEPRESS_BASE，构建前注入 base"
  sed -i.bak "s#export default withMermaid(defineConfig({#export default withMermaid(defineConfig({\n  base: process.env.VITEPRESS_BASE || '/',#" "$CONFIG"
fi

if [ -n "${VERSIONS_JSON:-}" ]; then
  echo "VERSIONS_JSON 已废弃：版本清单由 config.ts 直接调用 resolve-versions.sh 解析" >&2
fi

(
  cd "$WORKTREE/docs"
  bun install --frozen-lockfile >/dev/null
  VITEPRESS_BASE="$BASE_PATH" \
  VITE_VERSION_BANNER="$BANNER_TEXT" \
  VITE_VERSION_BANNER_URL="$BANNER_URL" \
  VITE_VERSION_BANNER_LINK_TEXT="$BANNER_LINK_TEXT" \
    bun run docs:build >/dev/null
)

mkdir -p "$OUT_DIR"
cp -R "$WORKTREE/docs/.vitepress/dist/"* "$OUT_DIR/"

if [ -n "$BANNER_TEXT" ] && [ "$LEGACY" = "1" ]; then
  bun "$REPO_ROOT/docs/scripts/inject-banner.mjs" "$OUT_DIR" "$BANNER_TEXT" "$BANNER_URL" "$BANNER_LINK_TEXT"
fi

echo "已构建 $REF (base=$BASE_PATH) -> $OUT_DIR"
