#!/usr/bin/env bash
# 解析需要构建的文档版本清单。
#
# 规则：
#   - 版本列表由 git tag 驱动（每个 minor 取最后一个稳定 tag；没有稳定 tag 时取最后一个预发布 tag）
#   - 如果存在对应的 release/x.y 分支，则以分支为构建源（分支用于线上 hotfix）
#   - 最高的 minor 是最新版本，部署在根路径 /，其余在 /vX.Y/
#   - 额外输出一行 next（master 开发版），调用方决定从哪个 ref 构建
#
# 输出（TSV，每行一个版本）：version <TAB> path <TAB> ref <TAB> latest(true|false)
# 环境变量：
#   GIT_REMOTE  设置后（如 origin），release 分支和 tag 都以远端为准（CI 用）
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
REMOTE="${GIT_REMOTE:-}"

branch_exists() {
  local branch="release/$1"
  if [ -n "$REMOTE" ]; then
    git -C "$REPO_ROOT" show-ref --verify --quiet "refs/remotes/$REMOTE/$branch"
  else
    git -C "$REPO_ROOT" show-ref --verify --quiet "refs/heads/$branch"
  fi
}

branch_ref() {
  if [ -n "$REMOTE" ]; then
    echo "$REMOTE/release/$1"
  else
    echo "release/$1"
  fi
}

has_docs() {
  git -C "$REPO_ROOT" cat-file -e "$1:docs/package.json" 2>/dev/null
}

# 每个 minor 取构建源：优先 release/x.y 分支，否则最后一个稳定 tag，再否则最后一个 tag
MINORS="$(git -C "$REPO_ROOT" tag --list 'v*' | sed -nE 's/^v([0-9]+\.[0-9]+)\..*/\1/p' | sort -Vu)"

# 先收集实际会构建的行，再从“已纳入”的版本里挑最高的当 latest。
# （不能直接拿 tag 里最高的 minor 当 latest：它可能只有预发布 tag、又没切 release 分支，
#   会在下面被跳过，最后没有任何一行是 latest，根站点就没了）
ROWS=()
for minor in $MINORS; do
  stable="$(git -C "$REPO_ROOT" tag --list "v$minor.*" | { grep -E "^v${minor}\.[0-9]+$" || true; } | sort -V | tail -1)"
  if branch_exists "$minor"; then
    ref="$(branch_ref "$minor")"
  elif [ -n "$stable" ]; then
    ref="$stable"
  else
    # 只有预发布 tag、又没切 release 分支的 minor 视为“还没正式发布”，跳过（比如 0.0.x alpha 线）
    continue
  fi

  # 没有 docs/ 目录的远古版本直接跳过
  if ! has_docs "$ref"; then
    continue
  fi

  ROWS+=("$minor"$'\t'"$ref")
done

LAST_INDEX=$((${#ROWS[@]} - 1))
for i in "${!ROWS[@]}"; do
  minor="${ROWS[$i]%%$'\t'*}"
  ref="${ROWS[$i]#*$'\t'}"
  if [ "$i" = "$LAST_INDEX" ]; then
    printf '%s\t%s\t%s\t%s\n' "$minor" "/" "$ref" "true"
  else
    printf '%s\t%s\t%s\t%s\n' "$minor" "/v$minor/" "$ref" "false"
  fi
done
