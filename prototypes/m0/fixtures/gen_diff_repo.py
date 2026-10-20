#!/usr/bin/env python3
"""生成 M0 diff fixture 的内容。

    gen_diff_repo.py <repo> <node_version> [--count=N]   生成 v1 并写入仓库（不提交）
    gen_diff_repo.py <repo> --modify                     在 v1 之上生成 v2 改动（不提交）

固定种子，保证每次生成的 diff 完全一致，测量结果可比。
"""
from __future__ import annotations

import json
import random
import struct
import sys
import zlib
from pathlib import Path

SEED = 20261019
# 默认 1025 个函数块：每个块 v1/v2 各 7 行内容 + 1 空行 =>
# 每个块产生 8 行删除 + 8 行新增，总共约 16,400 行改动（含接口等固定行），
# 即“1 万行改动”量级。需要更大规模时用 --count 调整。
COUNT = 1025
LARGE_FILE_LINES = 5_000
LARGE_TS_COUNT = 2_500


def function_block(index: int, rng: random.Random, version: int) -> list[str]:
    """一个功能块的 v1 / v2 版本，每行都不同，保证 diff 里全部是 +/- 行。"""
    name = f"transform{index:04d}"
    if version == 1:
        return [
            f"export function {name}(input: string[]): number {{",
            f"  let total = {index};",
            "  for (const item of input) {",
            f"    total += item.length + {index % 97};",
            "  }",
            f"  return total;  // 处理第 {index} 组",
            "}",
            "",
        ]
    return [
        f"export function {name}(input: readonly string[], offset = {index % 31}): number {{",
        f"  let total = {index + 1} + offset;",
        "  for (const [position, item] of input.entries()) {",
        f"    total += item.length * {2 + (index % 7)} + position;",
        "  }",
        f"  return total;  // 处理第 {index} 组（修订）",
        "}",
        "",
    ]


def extra_block(index: int) -> list[str]:
    name = f"added{index:04d}"
    return [
        f"export const {name} = (value: number): number => value * {index + 2};",
        "",
    ]


def ts_v1() -> str:
    rng = random.Random(SEED)
    lines = [
        "/** 自动生成的测量样例：v1。 */",
        "",
        "export interface Stats {",
        "  total: number;",
        "  count: number;",
        "}",
        "",
    ]
    for index in range(COUNT):
        lines.extend(function_block(index, rng, 1))
    return "\n".join(lines) + "\n"


def ts_v2() -> str:
    rng = random.Random(SEED)
    lines = [
        "/** 自动生成的测量样例：v2。 */",
        "",
        "export interface Stats {",
        "  total: number;",
        "  count: number;",
        "  elapsedMs: number;",
        "}",
        "",
    ]
    for index in range(COUNT):
        lines.extend(function_block(index, rng, 2))
    for index in range(50):
        lines.extend(extra_block(index))
    return "\n".join(lines) + "\n"


def ts_large_v1() -> str:
    """超大文件：默认收起，点开才渲染。"""
    lines = ["/** 超大文件样例：默认收起。 */", ""]
    for index in range(LARGE_TS_COUNT):
        lines.extend(
            [
                f"export function large{index:04d}(input: readonly string[]): number {{",
                f"  return input.length + {index};  // 第 {index} 个函数",
                "}",
                "",
            ]
        )
    return "\n".join(lines) + "\n"


def lock_v1() -> str:
    packages = {
        f"node_modules/pkg-{index:04d}": {
            "version": f"1.{index}.0",
            "resolved": f"https://registry.example.com/pkg-{index:04d}/-/pkg-{index:04d}-1.{index}.0.tgz",
        }
        for index in range(600)
    }
    return json.dumps({"name": "fixture", "lockfileVersion": 3, "packages": packages}, indent=2) + "\n"


def lock_v2() -> str:
    text = lock_v1()
    # 改一半的版本号，制造一个“改了很多行”的 lock 文件。
    return text.replace('"version": "1.', '"version": "2.')


def large_v1() -> str:
    return "".join(f"2026-10-19T10:{index % 60:02d}:00Z INFO 生成的第 {index} 行日志\n" for index in range(LARGE_FILE_LINES))


def large_v2() -> str:
    return "".join(
        f"2026-10-19T11:{index % 60:02d}:00Z WARN 修改过的第 {index} 行日志\n"
        for index in range(LARGE_FILE_LINES)
    )


def png_bytes(seed: int) -> bytes:
    """手写一个最小 PNG，避免依赖 Pillow。"""
    def chunk(kind: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + kind
            + data
            + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
        )

    width = height = 16
    raw = b""
    rng = random.Random(seed)
    for _ in range(height):
        raw += b"\x00" + bytes(rng.randrange(256) for _ in range(width * 3))
    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")


def write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def generate(repo: Path, node_version: str) -> None:
    write(repo / "src" / "generated.ts", ts_v1())
    write(repo / "pnpm-lock.yaml", lock_v1())
    write(repo / "logs" / "app.log", large_v1())
    (repo / "assets").mkdir(parents=True, exist_ok=True)
    (repo / "assets" / "icon.png").write_bytes(png_bytes(1))
    write(repo / "src" / "large.ts", ts_large_v1())
    (repo / ".node-version").write_text(node_version + "\n", encoding="utf-8")
    write(repo / "README.md", "fixture repo: diffs\n")


def modify(repo: Path) -> None:
    write(repo / "src" / "generated.ts", ts_v2())
    write(repo / "pnpm-lock.yaml", lock_v2())
    write(repo / "logs" / "app.log", large_v2())
    (repo / "assets" / "icon.png").write_bytes(png_bytes(2))
    write(repo / "src" / "staged.ts", "export const staged = 1;\n")

    # Vue：模板与脚本都改，验证 SFC 高亮。
    write(
        repo / "src" / "App.vue",
        """<template>
  <section class="counter">
    <strong>{{ label }}</strong>
    <button @click="count++">加一</button>
  </section>
</template>

<script setup lang="ts">
const count = ref(0);
const label = computed(() => `已点击 ${count.value} 次`);
</script>

<style scoped>
.counter {
  font-weight: 700;
  letter-spacing: 0.02em;
}
</style>
""",
    )

    # Swift：加入泛型与多行字符串，验证跨行结构。
    write(
        repo / "src" / "Model.swift",
        """import Foundation

struct Repository<Branch: Hashable> {
    let path: URL
    var branch: Branch

    var summary: String {
        \"\"\"
        仓库：\\(path.lastPathComponent)
        分支：\\(branch)
        \"\"\"
    }
}

extension Repository: CustomStringConvertible {
    var description: String { summary }
}
""",
    )


def main() -> int:
    global COUNT
    args = sys.argv[1:]
    if len(args) < 2:
        print(__doc__)
        return 2
    repo = Path(args[0])
    if args[1] == "--modify":
        modify(repo)
    else:
        counted = [arg for arg in args if arg.startswith("--count=")]
        if counted:
            COUNT = int(counted[0].split("=", 1)[1])
        generate(repo, args[1])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
