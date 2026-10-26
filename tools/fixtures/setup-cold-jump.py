#!/usr/bin/env python3
"""Create an isolated >20,000-row diff without changing the normal benchmark fixture."""
import argparse
import os
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    root = args.directory.resolve()
    root.mkdir(parents=True, exist_ok=False)
    env = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
    env.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull,
               GIT_AUTHOR_NAME="Revia Fixture", GIT_AUTHOR_EMAIL="fixture@revia.invalid",
               GIT_COMMITTER_NAME="Revia Fixture", GIT_COMMITTER_EMAIL="fixture@revia.invalid")

    def git(*arguments):
        subprocess.run(["git", "-C", str(root), "-c", "commit.gpgsign=false",
                        "-c", "core.hooksPath=" + os.devnull, *arguments],
                       env=env, check=True, capture_output=True)

    git("init", "--quiet", "--initial-branch=main")
    path = root / "cold.ts"
    path.write_text("".join(f"export const value_{i} = {i};\n" for i in range(30_000)))
    git("add", "--", "cold.ts")
    git("commit", "--quiet", "-m", "fixture: cold jump baseline")
    path.write_text("".join(f"export const value_{i} = {i + 1};\n" for i in range(30_000)))
    print(root)


if __name__ == "__main__":
    main()
