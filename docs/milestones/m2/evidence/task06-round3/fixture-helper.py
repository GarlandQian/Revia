#!/usr/bin/env python3
"""Explicit, fixture-pinned helpers; no operation runs without a subcommand."""

import argparse
import base64
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile
import time


PINNED_ROOT = Path("/private/tmp/revia-task06-2fuu2r92")
PINNED_GIT = Path("/opt/homebrew/bin/git")
DEFAULT_CONTEXT = Path("/tmp/revia-task06-desktop-context.json")
TARGET = "10-target.ts"
FILES = ("00-first.ts", "05-before.ts", TARGET, "20-after.ts", "99-last.ts", "untracked.txt")
LABEL = re.compile(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,79}\Z")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def timestamp():
    return datetime.datetime.now().astimezone().isoformat(timespec="milliseconds")


def digest(data):
    return hashlib.sha256(data).hexdigest()


def show(value):
    print(json.dumps(value, ensure_ascii=False, indent=2))


def no_links(path, root=PINNED_ROOT):
    """Reject symlink traversal rather than resolving a target outside the fixture."""
    path = Path(path)
    require(path == root or root in path.parents, f"Outside pinned fixture: {path}")
    for part in (path, *path.parents):
        require(not part.is_symlink(), f"Symlink is not allowed: {part}")
        if part == root:
            break
    return path


def regular(path, missing=False):
    no_links(path)
    if missing and not path.exists():
        return None
    info = path.stat()
    require(stat.S_ISREG(info.st_mode), f"Not a regular file: {path}")
    require(info.st_nlink == 1, f"Hard-linked file is not allowed: {path}")
    return info


class Fixture:
    def __init__(self, context_path, repo):
        context_path = Path(context_path).resolve(strict=True)
        require(Path("/private/tmp") in context_path.parents, "Context must be under /tmp")
        context = json.loads(context_path.read_text())
        self.root = Path(context["root"])
        require(self.root == PINNED_ROOT, "Context does not name the pinned task06 fixture")
        require(self.root.parent == Path("/private/tmp") and self.root.name.startswith("revia-task06-"), "Invalid fixture prefix")
        no_links(self.root)
        require(self.root.is_dir(), "Fixture root is missing")
        local_context = self.root / "context.json"
        regular(local_context)
        require(json.loads(local_context.read_text()) == context, "External and fixture contexts differ")
        require(Path(context["git"]) == PINNED_GIT, "Unexpected Git executable")
        self.git = PINNED_GIT.resolve(strict=True)
        require(self.git.is_file() and os.access(self.git, os.X_OK), "Git is not executable")
        self.home = self.root / "home"
        require(Path(context["home"]) == self.home, "Unexpected HOME")
        require(no_links(self.home).is_dir(), "Isolated HOME is missing")
        regular(self.home / ".gitconfig")
        for name in ("A", "B", "L"):
            expected = self.root / name
            require(Path(context[name]) == expected, f"Unexpected {name} path")
            require(no_links(expected).is_dir(), f"Fixture {name} is missing")
        self.name = repo
        self.repo = self.root / repo
        self.env = {k: v for k, v in os.environ.items() if not k.startswith(("GIT_", "REVIA_"))}
        self.env.update(HOME=str(self.home), XDG_CONFIG_HOME=str(self.home / ".config"),
                        GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=str(self.home / ".gitconfig"),
                        GIT_OPTIONAL_LOCKS="0", GIT_ATTR_NOSYSTEM="1", GIT_TERMINAL_PROMPT="0")
        self.paths = None

    def run(self, args):
        started = time.monotonic()
        command = [str(self.git), "--no-optional-locks", "-c", "core.fsmonitor=false", "-C", str(self.repo), *args]
        result = subprocess.run(command, env=self.env, capture_output=True, timeout=30)
        return {"args": args, "exitCode": result.returncode,
                "elapsedMs": round((time.monotonic() - started) * 1000, 3),
                "stdoutText": result.stdout.decode("utf-8", "replace"),
                "stdoutBase64": base64.b64encode(result.stdout).decode("ascii"),
                "stderrText": result.stderr.decode("utf-8", "replace")}

    def ok(self, args):
        result = self.run(args)
        require(result["exitCode"] == 0, f"Git read failed: {result['args']}: {result['stderrText']}")
        return base64.b64decode(result["stdoutBase64"])

    def identity(self):
        if self.paths is not None:
            return self.paths
        no_links(self.repo / ".git")
        top = Path(os.fsdecode(self.ok(["rev-parse", "--show-toplevel"])).strip()).resolve()
        require(top == self.repo, "Git toplevel differs from the selected fixture")
        git_dir = Path(os.fsdecode(self.ok(["rev-parse", "--absolute-git-dir"])).strip())
        common = Path(os.fsdecode(self.ok(["rev-parse", "--git-common-dir"])).strip())
        if not common.is_absolute():
            common = self.repo / common
        # Normalize '..' in linked metadata, then check every actual path component.
        git_dir = Path(os.path.abspath(git_dir))
        common = Path(os.path.abspath(common))
        no_links(git_dir)
        no_links(common)
        expected_common = self.root / ("A" if self.name == "L" else self.name) / ".git"
        require(common == expected_common, "Git common directory escaped the selected fixture")
        require(git_dir == common if self.name != "L" else git_dir.parent == common / "worktrees", "Unexpected per-worktree Git directory")
        for child in ("objects", "refs", "logs", "config"):
            no_links(common / child)
        index = Path(os.fsdecode(self.ok(["rev-parse", "--git-path", "index"])).strip())
        if not index.is_absolute():
            index = self.repo / index
        index = Path(os.path.abspath(index))
        require(index == git_dir / "index", "Unexpected index path")
        regular(index, missing=True)
        self.paths = {"worktree": str(self.repo), "gitDir": str(git_dir), "commonDir": str(common), "index": str(index)}
        return self.paths

    def file(self, name):
        require(name in FILES, "File is not on the fixture allowlist")
        path = self.repo / name
        regular(path, missing=True)
        return path

    def file_info(self, path):
        info = regular(path, missing=True)
        if info is None:
            return {"path": str(path), "exists": False}
        data = path.read_bytes()
        return {"path": str(path), "exists": True, "sha256": digest(data), "bytes": len(data),
                "lines": len(data.splitlines()), "mode": format(stat.S_IMODE(info.st_mode), "04o"),
                "inode": info.st_ino, "mtimeNs": info.st_mtime_ns}

    def snapshot(self, name):
        started = timestamp()
        paths = self.identity()
        path = self.file(name)
        commands = {"head": ["rev-parse", "--verify", "HEAD"],
                    "status": ["status", "--porcelain=v2", "--branch", "-z", "--untracked-files=all"],
                    "indexEntry": ["ls-files", "--stage", "-z", "--", name],
                    "diff": ["diff", "--no-color", "--no-ext-diff", "--no-textconv", "--binary", "--"],
                    "diffCached": ["diff", "--cached", "--no-color", "--no-ext-diff", "--no-textconv", "--binary", "--"]}
        results = {key: self.run(args) for key, args in commands.items()}
        require(all(value["exitCode"] == 0 for value in results.values()), "Snapshot Git read failed; no complete snapshot recorded")
        blobs = []
        for entry in base64.b64decode(results["indexEntry"]["stdoutBase64"]).split(b"\0"):
            if not entry:
                continue
            metadata, entry_path = entry.split(b"\t", 1)
            mode, oid, stage = metadata.decode("ascii").split()
            data = self.ok(["cat-file", "blob", oid])
            blobs.append({"path": os.fsdecode(entry_path), "mode": mode, "oid": oid, "stage": int(stage),
                          "bytes": len(data), "sha256": digest(data)})
        return {"startedAt": started, "finishedAt": timestamp(), "repo": self.name, "target": name, "gitProgram": str(self.git),
                "readOnly": True, "consistency": "Sequential observations, not an atomic cross-command snapshot",
                "paths": paths, "worktreeFile": self.file_info(path),
                "indexFile": self.file_info(Path(paths["index"])), "indexBlobs": blobs, "git": results}

    def evidence(self, name):
        directory = no_links(self.root / "evidence")
        directory.mkdir(exist_ok=True)
        return no_links(directory / name)

    def append(self, name, value):
        path = self.evidence(name)
        regular(path, missing=True)
        fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_APPEND | os.O_NOFOLLOW, 0o600)
        with os.fdopen(fd, "a", encoding="utf-8") as stream:
            stream.write(json.dumps(value, ensure_ascii=False) + "\n")
            stream.flush()
            os.fsync(stream.fileno())
        return str(path)


def set_line80(fixture, value, mode, apply):
    fixture.identity()
    path = fixture.file(TARGET)
    info = regular(path)
    before = path.read_bytes()
    lines = before.splitlines(keepends=True)
    require(len(lines) == 180 and all(line.endswith(b"\n") for line in lines), "Expected exactly 180 newline-terminated lines")
    require(re.fullmatch(r'(?:true|false|null|-?[0-9]+(?:\.[0-9]+)?|"[A-Za-z0-9_ .-]*")', value), "Use a short boolean, number, null, or quoted ASCII string")
    prefix = b"export const value080 = "
    require(lines[79].startswith(prefix) and lines[79].rstrip().endswith(b";"), "Line 80 no longer matches the fixture")
    ending = b"\r\n" if lines[79].endswith(b"\r\n") else b"\n"
    replacement = prefix + value.encode("ascii") + b";"
    padding = len(lines[79]) - len(replacement) - len(ending)
    require(padding >= 0, "New value is too long to preserve the original byte count")
    lines[79] = replacement + b" " * padding + ending
    after = b"".join(lines)
    require(len(after) == len(before) and len(after.splitlines()) == 180, "Length invariant failed")
    event = {"timestamp": timestamp(), "command": "set-line80", "repo": fixture.name, "path": str(path),
             "mode": mode, "dryRun": not apply, "value": value, "paddingSpaces": padding,
             "bytesBefore": len(before), "bytesAfter": len(after), "linesBefore": 180, "linesAfter": 180,
             "sha256Before": digest(before), "sha256After": digest(after), "fileMode": format(stat.S_IMODE(info.st_mode), "04o")}
    if apply:
        regular(fixture.evidence("mutations.jsonl"), missing=True)
        if mode == "inplace":
            fd = os.open(path, os.O_RDWR | os.O_NOFOLLOW)
            with os.fdopen(fd, "r+b") as stream:
                require(os.fstat(stream.fileno()).st_ino == info.st_ino and stream.read() == before, "File changed before write")
                stream.seek(0)
                stream.write(after)
                stream.flush()
                os.fsync(stream.fileno())
        else:
            fd, temporary = tempfile.mkstemp(prefix=".revia-task06-save-", dir=fixture.repo)
            try:
                with os.fdopen(fd, "wb") as stream:
                    os.fchmod(stream.fileno(), stat.S_IMODE(info.st_mode))
                    stream.write(after)
                    stream.flush()
                    os.fsync(stream.fileno())
                current = regular(path)
                require(current.st_ino == info.st_ino and path.read_bytes() == before, "File changed before replace")
                os.replace(temporary, path)
            finally:
                if os.path.exists(temporary):
                    os.unlink(temporary)  # Only this invocation's private temporary file.
        event["actual"] = fixture.file_info(path)
        event["recordedAt"] = fixture.append("mutations.jsonl", event)
    return event


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--context", default=str(DEFAULT_CONTEXT))
    sub = parser.add_subparsers(dest="command", required=True)
    snap = sub.add_parser("snapshot", help="Read Git and filesystem truth; optionally save a new evidence JSON")
    snap.add_argument("repo", choices=("A", "B", "L"))
    snap.add_argument("--file", choices=FILES, default=TARGET)
    snap.add_argument("--save", metavar="LABEL", help="Save evidence/LABEL.json; existing files are never overwritten")
    change = sub.add_parser("set-line80", help="Preview a same-line-count/same-byte-count change; --apply performs it")
    change.add_argument("repo", choices=("A", "B", "L"))
    change.add_argument("value", help='Examples: true, false, 2080, or a quoted value such as \'"A_v2"\'')
    change.add_argument("--mode", choices=("inplace", "atomic"), default="inplace")
    change.add_argument("--apply", action="store_true")
    for command in ("stage", "unstage"):
        operation = sub.add_parser(command, help="Preview one allowlisted file operation; --apply performs it")
        operation.add_argument("repo", choices=("A", "B", "L"))
        operation.add_argument("file", choices=FILES)
        operation.add_argument("--apply", action="store_true")
    record = sub.add_parser("record", help="Append a case observation only; no Git command or repository mutation")
    record.add_argument("repo", choices=("A", "B", "L"))
    record.add_argument("case")
    record.add_argument("--result", choices=("observed", "passed", "failed", "not-covered"), default="observed")
    record.add_argument("--note", default="")
    record.add_argument("--details", default="{}", help="Optional JSON object, e.g. appPid/selection/view/screenshot")
    args = parser.parse_args()
    fixture = Fixture(args.context, args.repo)
    if args.command == "snapshot":
        if args.save:
            require(LABEL.fullmatch(args.save), "Invalid evidence label")
        output = fixture.snapshot(args.file)
        if args.save:
            path = fixture.evidence(args.save + ".json")
            with path.open("x", encoding="utf-8") as stream:
                stream.write(json.dumps(output, ensure_ascii=False, indent=2) + "\n")
            output["savedTo"] = str(path)
    elif args.command == "set-line80":
        output = set_line80(fixture, args.value, args.mode, args.apply)
    elif args.command in ("stage", "unstage"):
        before = fixture.snapshot(args.file)
        command = ["add", "--", args.file] if args.command == "stage" else ["restore", "--staged", "--", args.file]
        output = {"timestamp": timestamp(), "command": args.command, "repo": args.repo,
                  "file": args.file, "dryRun": not args.apply, "gitArgs": command, "before": before}
        if args.apply:
            regular(fixture.evidence("mutations.jsonl"), missing=True)
            try:
                output["gitResult"] = fixture.run(command)
                output["after"] = fixture.snapshot(args.file)
            except Exception as error:
                output["result"] = "needs-verification"
                output["error"] = str(error)
                fixture.append("mutations.jsonl", output)
                show(output)
                return 1
            output["recordedAt"] = fixture.append("mutations.jsonl", output)
            show(output)
            return 0 if output["gitResult"]["exitCode"] == 0 else 1
    else:
        require(LABEL.fullmatch(args.case), "Invalid case ID")
        details = json.loads(args.details)
        require(isinstance(details, dict), "--details must be a JSON object")
        output = {"timestamp": timestamp(), "caseId": args.case, "repo": args.repo,
                  "worktree": str(fixture.repo), "result": args.result, "note": args.note, "details": details}
        output["recordedAt"] = fixture.append("desktop-checks.jsonl", output)
    show(output)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as error:
        print(json.dumps({"error": str(error)}, ensure_ascii=False), file=sys.stderr)
        sys.exit(1)
