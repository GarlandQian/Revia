#!/usr/bin/env bash
# M0 测试 fixture 生成脚本。
#
# 生成 4 个仓库（生成物不进版本库，脚本进版本库）：
#   node-versioned  带 .node-version（v24.11.1）+ husky pre-commit hook
#   node-default    没有 .node-version，hook 应拿到 fnm 默认版本 v26.5.0
#   mergetool       配置 merge.tool=vscode，供 git mergetool 验证
#   diffs           TypeScript / Vue / Swift 改动 + 大文件 + lock 文件 + 二进制文件
#
# 用法：fixtures/setup.sh [输出目录]   默认 $TMPDIR/revia-m0-fixtures
set -euo pipefail

OUT="${1:-${TMPDIR:-/tmp}/revia-m0-fixtures}"
NODE_VERSIONED="v24.11.1"
NODE_DEFAULT="v26.5.0"

GIT=/usr/bin/git
PYTHON=/usr/bin/python3

if ! command -v fnm >/dev/null 2>&1; then
  echo "警告：找不到 fnm，Node 版本验证需要它" >&2
fi
if [ ! -x /opt/homebrew/bin/code ]; then
  echo "警告：/opt/homebrew/bin/code 不存在，mergetool 验证会缺少真实 code 路径" >&2
fi

echo "fixture 输出目录：$OUT"
mkdir -p "$OUT"

init_repo() {
  local name="$1"
  rm -rf "$OUT/$name"
  mkdir -p "$OUT/$name"
  $GIT -C "$OUT/$name" init --quiet --initial-branch=main
  # fixture 是生成出来的测试仓库，可以自由改它的本地配置。
  $GIT -C "$OUT/$name" config user.name "Revia M0 Fixture"
  $GIT -C "$OUT/$name" config user.email "fixture@revia.local"
  $GIT -C "$OUT/$name" config commit.gpgsign false
}

commit_all() {
  local name="$1" message="$2"
  $GIT -C "$OUT/$name" add -A
  $GIT -C "$OUT/$name" commit --quiet -m "$message"
}

# ---------------------------------------------------------------- node hook

# 注意：hook 由 git 用 /bin/sh 执行。macOS 的 /bin/sh 是 bash 3.2，
# 在把中文紧邻 $变量 输出时会错位丢字节，所以这里用 printf 的格式化输出。
write_hook() {
  local name="$1"
  mkdir -p "$OUT/$name/.husky"
  cat > "$OUT/$name/.husky/pre-commit" <<'HOOK'
#!/bin/sh
# M0 验证：hook 里能拿到哪个 node。
node_path="$(command -v node || echo missing)"
node_version="$(node -v 2>/dev/null || echo missing)"
printf '[husky] node path: %s\n' "$node_path"
printf '[husky] node version: %s\n' "$node_version"
if [ -f .node-version ]; then
  want="$(tr -d '[:space:]' < .node-version)"
  want="${want#v}"
  got="${node_version#v}"
  if [ "$want" != "$got" ]; then
    printf '[husky] FAIL: expected node v%s, got v%s\n' "$want" "$got" >&2
    exit 1
  fi
  printf '[husky] OK: node v%s matches .node-version\n' "$got"
else
  printf '[husky] OK: no .node-version in this repo\n'
fi
HOOK
  chmod +x "$OUT/$name/.husky/pre-commit"
  $GIT -C "$OUT/$name" config core.hooksPath .husky
}

write_node_repo() {
  local name="$1" node_version="$2"
  init_repo "$name"
  if [ -n "$node_version" ]; then
    printf '%s\n' "$node_version" > "$OUT/$name/.node-version"
  fi
  cat > "$OUT/$name/package.json" <<'JSON'
{
  "name": "revia-m0-node-fixture",
  "version": "1.0.0",
  "private": true
}
JSON
  write_hook "$name"
  printf 'fixture repo: %s\n' "$name" > "$OUT/$name/README.md"
  # 初始化提交不跑 hook：hook 的验证在 env-check 里用真实 git commit 触发。
  $GIT -C "$OUT/$name" -c core.hooksPath=/dev/null add -A
  $GIT -C "$OUT/$name" -c core.hooksPath=/dev/null commit --quiet -m "fixture: 初始化 $name"
}

write_node_repo node-versioned "$NODE_VERSIONED"
write_node_repo node-default ""

# ---------------------------------------------------------------- mergetool

init_repo mergetool
printf 'base line\n' > "$OUT/mergetool/README.txt"
$GIT -C "$OUT/mergetool" config merge.tool vscode
commit_all mergetool "fixture: 初始化 mergetool"

# ---------------------------------------------------------------- diffs

init_repo diffs
mkdir -p "$OUT/diffs/src"

# TypeScript：约 1 万行改动的来源。先写入 v1 并提交，再原地改成 v2。
$PYTHON "$(dirname "$0")/gen_diff_repo.py" "$OUT/diffs" "$NODE_DEFAULT"

cat > "$OUT/diffs/src/App.vue" <<'VUE'
<template>
  <div class="counter">
    <span>{{ label }}</span>
  </div>
</template>

<script setup lang="ts">
const label = computed(() => `${count.value} 次`);
const count = ref(0);
</script>

<style scoped>
.counter {
  font-weight: 600;
}
</style>
VUE

cat > "$OUT/diffs/src/Model.swift" <<'SWIFT'
import Foundation

struct Repository {
    let path: URL
    var branch: String
}

extension Repository: CustomStringConvertible {
    var description: String {
        "Repository(\(branch), \(path.lastPathComponent))"
    }
}
SWIFT

commit_all diffs "fixture: 初始化 diffs"

# 改 Vue 与 Swift，并生成 lock 文件、二进制文件、超大文件；
# staged.ts 额外暂存，用来验证“已暂存改动”的 diff。
$PYTHON "$(dirname "$0")/gen_diff_repo.py" "$OUT/diffs" --modify
$GIT -C "$OUT/diffs" add -- src/staged.ts

echo
printf '=== fixture 概览 ===\n'
for name in node-versioned node-default mergetool diffs; do
  printf '%-16s %s\n' "$name" "$OUT/$name"
done
printf 'diffs 仓库改动行数：%s\n' \
  "$($GIT -C "$OUT/diffs" diff --numstat | awk '{a+=$1; d+=$2} END {printf "%d 行新增 / %d 行删除", a, d}')"
echo
printf '验证命令示例：\n'
printf '  REVIA_M0_FIXTURES=%s <app> env-check hook\n' "$OUT"
printf '  REVIA_M0_WORKTREE=%s/node-default REVIA_M0_FIXTURES=%s <app> env-check hook\n' "$OUT" "$OUT"
printf '  REVIA_M0_FIXTURES=%s <app> env-check mergetool\n' "$OUT"
