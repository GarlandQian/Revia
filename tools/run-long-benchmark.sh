#!/usr/bin/env bash
# Revia 长期运行基线自动化测试脚本（M2 任务 01）
#
# 用法：
#   ./tools/run-long-benchmark.sh 30        # 运行 30 分钟长测（默认）
#   ./tools/run-long-benchmark.sh 1         # 运行 1 分钟验证
set -euo pipefail
cd "$(dirname "$0")/.."

MINUTES="${1:-30}"

export PATH="/opt/homebrew/bin:$PATH"
echo "==> 构建当前源码的 Release 应用..."
pnpm tauri build --bundles app

echo "==> 启动长期运行基线（${MINUTES} 分钟）..."
caffeinate -di python3 tools/bench-long-running.py --minutes "$MINUTES"
