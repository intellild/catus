#!/usr/bin/env bash
# 安装项目 git hooks：把 core.hooksPath 指向仓库内的 .githooks 目录。
# 这样 .githooks/pre-commit 会在每次 git commit 时运行。
#
# 用法： ./scripts/install-git-hooks.sh
# 卸载： git config --unset core.hooksPath

set -euo pipefail

root=$(git rev-parse --show-toplevel)
hooks_dir="$root/.githooks"

if [ ! -d "$hooks_dir" ]; then
  echo "error: $hooks_dir 不存在" >&2
  exit 1
fi

# 确保脚本可执行
chmod +x "$hooks_dir"/* 2>/dev/null || true

git config core.hooksPath ".githooks"
echo "✓ git hooks 已安装到 core.hooksPath = .githooks"
echo "  pre-commit: 有 .rs / Cargo 改动时运行 cargo fmt --check / check / clippy / test"
echo "  跳过本次检查：git commit --no-verify"
echo "  卸载：git config --unset core.hooksPath"
