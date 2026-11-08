#!/usr/bin/env python3
"""
M9 性能测试大仓库 fixture 生成器（规格 D 节，任务 06）。

生成目标：
- 约 20,000 个文件（100 个目录 × 200 个文件）
- 30,000 个提交（通过 fast-import 流式导入）
- 300 个未提交改动文件（工作区 dirty）
"""

import os
import subprocess
import sys
import time

def generate_large_repo(dest_dir: str):
    os.makedirs(dest_dir, exist_ok=True)
    git_bin = os.environ.get("GIT", "/usr/bin/git")

    # 初始化仓库
    subprocess.run([git_bin, "-C", dest_dir, "init", "--quiet", "--initial-branch=main"], check=True)
    subprocess.run([git_bin, "-C", dest_dir, "config", "user.name", "Revia Large Fixture"], check=True)
    subprocess.run([git_bin, "-C", dest_dir, "config", "user.email", "fixture@revia.dev"], check=True)
    subprocess.run([git_bin, "-C", dest_dir, "config", "commit.gpgsign", "false"], check=True)

    t0 = time.time()
    p = subprocess.Popen(
        [git_bin, "-C", dest_dir, "fast-import", "--date-format=raw", "--quiet"],
        stdin=subprocess.PIPE,
        text=True,
    )

    # 1. 初始提交：创建 20,000 个文件（100 个目录 × 200 个文件）
    p.stdin.write(
        "commit refs/heads/main\n"
        "mark :1\n"
        "committer Revia <fixture@revia.dev> 1700000000 +0000\n"
        "data <<EOF\n"
        "fixture: initial 20000 files\n"
        "EOF\n"
    )
    for d in range(100):
        for f in range(200):
            p.stdin.write(
                f"M 644 inline src/d{d}/f{f}.ts\n"
                "data <<EOF\n"
                "export const val = 1;\n"
                "EOF\n"
            )

    # 2. 依次生成第 2 至第 30,000 个提交（共 30,000 个提交）
    for i in range(2, 30001):
        d = (i - 2) % 100
        p.stdin.write(
            f"commit refs/heads/main\n"
            f"mark :{i}\n"
            f"committer Revia <fixture@revia.dev> {1700000000 + i} +0000\n"
            "data <<EOF\n"
            f"commit {i}\n"
            "EOF\n"
            f"from :{i - 1}\n"
            f"M 644 inline src/d{d}/f0.ts\n"
            "data <<EOF\n"
            f"export const val = {i};\n"
            "EOF\n"
        )

    p.stdin.close()
    p.wait()
    if p.returncode != 0:
        raise RuntimeError(f"fast-import failed with exit code {p.returncode}")

    t1 = time.time()
    # 3. 检出工作区
    subprocess.run([git_bin, "-C", dest_dir, "reset", "--hard", "HEAD", "--quiet"], check=True)
    t2 = time.time()

    # 4. 生成 300 个改动文件
    for f in range(300):
        d = f % 100
        idx = f // 100 + 1
        filepath = os.path.join(dest_dir, f"src/d{d}/f{idx}.ts")
        with open(filepath, "a") as fp:
            fp.write("// modified for revia m9 fixture\n")

    t3 = time.time()
    print(f"大仓库 fixture 生成完毕：{t1 - t0:.2f}s fast-import, {t2 - t1:.2f}s 检出, {t3 - t2:.2f}s 改动 300 文件")

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: gen_large_repo.py <dest_dir>", file=sys.stderr)
        sys.exit(1)
    generate_large_repo(sys.argv[1])
