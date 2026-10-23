#!/usr/bin/env python3
"""Create an isolated, real-Git desktop fixture without touching user repos."""

import base64
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import tempfile


ROOT = Path(tempfile.mkdtemp(prefix="revia-task04-")).resolve()
HOME = ROOT / "home"
HOME.mkdir()
(HOME / ".gitconfig").write_text("")
GIT = Path("/opt/homebrew/bin/git").resolve(strict=True)
ENV = {k: v for k, v in os.environ.items() if not k.startswith(("GIT_", "REVIA_"))}
ENV.update(
    HOME=str(HOME), USERPROFILE=str(HOME), XDG_CONFIG_HOME=str(HOME / ".config"),
    GIT_CONFIG_GLOBAL=str(HOME / ".gitconfig"), GIT_CONFIG_NOSYSTEM="1",
    GIT_TERMINAL_PROMPT="0", GIT_ATTR_NOSYSTEM="1",
)


def git(repo, *args):
    result = subprocess.run([str(GIT), "-C", str(repo), *args], env=ENV,
                            capture_output=True, timeout=30)
    if result.returncode:
        raise RuntimeError(f"Git {args!r}: {result.stderr.decode(errors='replace')}")
    return result.stdout


def write(repo, path, content):
    destination = repo / path
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_bytes(content)


def text_lines(changes=None):
    changes = changes or {}
    return "".join(f"export const value{i:03d} = {changes.get(i, i)};\n"
                   for i in range(1, 181)).encode()


def snapshot(repo):
    entries = []
    for item in sorted(repo.rglob("*")):
        if ".git" in item.relative_to(repo).parts or not (item.is_file() or item.is_symlink()):
            continue
        info = item.lstat()
        content = os.fsencode(os.readlink(item)) if item.is_symlink() else item.read_bytes()
        entries.append({"path": item.relative_to(repo).as_posix(),
                        "kind": "symlink" if item.is_symlink() else "file",
                        "mode": oct(stat.S_IMODE(info.st_mode)),
                        "sha256": hashlib.sha256(content).hexdigest(),
                        "bytes": len(content)})
    return {"path": str(repo), "head": git(repo, "rev-parse", "HEAD").decode().strip(),
            "indexEntriesBase64": base64.b64encode(git(repo, "ls-files", "--stage", "-z")).decode(),
            "statusBase64": base64.b64encode(git(repo, "status", "--porcelain=v2", "-z")).decode(),
            "files": entries}


A = ROOT / "A"
B = ROOT / "B"
for repo in (A, B):
    repo.mkdir()
    git(repo, "init", "-b", "main")
    write(repo, "00-hunks.ts", text_lines())
    write(repo, "10-partial.ts", text_lines())
    write(repo, "20-binary.bin", b"\x00before\xffbinary\n")
    write(repo, "30-mode.sh", b"#!/bin/sh\nprintf '%s\\n' fixture\n")
    (repo / "30-mode.sh").chmod(0o644)
    write(repo, "40-deleted.txt", b"original deleted file\n")
    write(repo, "50-renamed-old.txt", b"tracked rename baseline\n")
    write(repo, "60-\u4e2d\u6587 \u7a7a\u683c.ts", text_lines())
    write(repo, "70-symlink.txt", b"original regular file\n")
    write(repo, "80-unrelated.txt", b"unrelated baseline\n")
    git(repo, "add", "--all")
    git(repo, "-c", "user.name=Revia Desktop Fixture", "-c", "user.email=fixture@example.invalid",
        "-c", "commit.gpgsign=false", "commit", "-m", "task04 baseline")

L = ROOT / "L"
git(A, "worktree", "add", "-b", "linked", str(L))

for repo in (A, B, L):
    write(repo, "00-hunks.ts", text_lines({10: 1010, 80: 1080, 140: 1140}))
    write(repo, "10-partial.ts", text_lines({10: 2010}))
    git(repo, "add", "--", "10-partial.ts")
    write(repo, "10-partial.ts", text_lines({10: 2010, 80: 2080, 140: 2140}))
    write(repo, "20-binary.bin", b"\x00after\xffbinary\n")
    (repo / "30-mode.sh").chmod(0o755)
    (repo / "40-deleted.txt").unlink()
    git(repo, "mv", "--", "50-renamed-old.txt", "50-renamed-new.txt")
    write(repo, "50-renamed-new.txt", b"tracked rename baseline\nunstaged addition\n")
    write(repo, "60-\u4e2d\u6587 \u7a7a\u683c.ts", text_lines({80: 6080}))
    (repo / "70-symlink.txt").unlink()
    (repo / "70-symlink.txt").symlink_to("80-unrelated.txt")
    write(repo, "90-untracked.txt", b"must never offer discard/delete\n")

for name in ("data", "evidence"):
    (ROOT / name).mkdir()
context = {"root": str(ROOT), "home": str(HOME), "git": str(GIT),
           "A": str(A), "B": str(B), "L": str(L), "data": str(ROOT / "data")}
(ROOT / "context.json").write_text(json.dumps(context, indent=2) + "\n")
(ROOT / "evidence" / "initial-state.json").write_text(
    json.dumps({name: snapshot(ROOT / name) for name in ("A", "B", "L")}, indent=2) + "\n")
print(json.dumps(context, indent=2))
