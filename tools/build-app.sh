#!/usr/bin/env bash
# 构建把前端产物嵌进二进制的应用。
#
# 为什么要 touch：Tauri 在编译期用宏把 dist 嵌进二进制，改前端后
# 必须重新编译 crate，否则跑的仍是旧界面（M0/M1 都踩过）。
set -euo pipefail
cd "$(dirname "$0")/.."

pnpm build
touch src-tauri/src/lib.rs

if [ "${1:-debug}" = "release" ]; then
  cargo build --manifest-path src-tauri/Cargo.toml --release
  echo "输出：src-tauri/target/release/revia"
else
  cargo build --manifest-path src-tauri/Cargo.toml
  echo "输出：src-tauri/target/debug/revia"
fi
