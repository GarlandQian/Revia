#!/usr/bin/env python3
"""Reproduce task04 attribute-context findings in disposable Git repositories."""

import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile


ROOT = Path(tempfile.mkdtemp(prefix="revia-task04-review-attributes-", dir="/private/tmp"))
HOME = ROOT / "home"
HOME.mkdir()
(HOME / ".gitconfig").write_text("")
GIT = "/opt/homebrew/bin/git"
ENV = {key: value for key, value in os.environ.items() if not key.startswith(("GIT_", "REVIA_"))}
ENV.update(HOME=str(HOME), XDG_CONFIG_HOME=str(HOME), GIT_CONFIG_GLOBAL=str(HOME / ".gitconfig"),
           GIT_CONFIG_NOSYSTEM="1", GIT_ATTR_NOSYSTEM="1", GIT_TERMINAL_PROMPT="0")
DIFF = ["--literal-pathspecs", "-c", "core.quotePath=false", "diff", "--no-color", "--no-ext-diff",
        "--no-textconv", "--src-prefix=a/", "--dst-prefix=b/", "--unified=3", "--binary", "--full-index", "--", "target.txt"]
APPLY = ["-c", "apply.ignoreWhitespace=no", "apply", "--reverse", "--whitespace=nowarn", "-"]


def git(repo, args, patch=None, check=True):
    result = subprocess.run([GIT, "-C", str(repo), *args], env=ENV, input=patch, capture_output=True, timeout=30)
    if check and result.returncode:
        raise RuntimeError(f"{args!r}: {result.stderr.decode(errors='replace')}")
    return result


def init(name):
    repo = ROOT / name
    repo.mkdir()
    git(repo, ["init", "-q", "-b", "main"])
    for key, value in [("user.name", "Review Fixture"), ("user.email", "fixture@example.invalid"),
                       ("commit.gpgsign", "false"), ("core.autocrlf", "false")]:
        git(repo, ["config", key, value])
    return repo


def text(changed=False, crlf=False):
    data = "".join(f"const value{n:03} = {n + 1000 if changed and n in (10, 80, 140) else n};\n" for n in range(1, 181))
    return data.replace("\n", "\r\n").encode() if crlf else data.encode()


def commit(repo):
    git(repo, ["add", "--all"])
    git(repo, ["commit", "-q", "-m", "fixture"])


def sha(data):
    return hashlib.sha256(data).hexdigest()


repo = init("changed-context")
(repo / ".gitattributes").write_bytes(b"target.txt text eol=lf\n")
(repo / "target.txt").write_bytes(text())
commit(repo)
(repo / "target.txt").write_bytes(text(changed=True))
patch_before = git(repo, DIFF).stdout
starts = [match.start() for match in re.finditer(rb"(?m)^@@ ", patch_before)]
assert len(starts) == 3
hunk_patch = patch_before[:starts[0]] + patch_before[starts[1]:starts[2]]
index_before = git(repo, ["ls-files", "--stage", "-z"]).stdout
scratch = init("scratch-preview")
(scratch / ".gitattributes").write_bytes((repo / ".gitattributes").read_bytes())
(scratch / "target.txt").write_bytes((repo / "target.txt").read_bytes())
git(scratch, APPLY, hunk_patch)
expected_after = (scratch / "target.txt").read_bytes()
(repo / ".gitattributes").write_bytes(b"target.txt text eol=crlf\n")
patch_after = git(repo, DIFF).stdout
before_write = (repo / "target.txt").read_bytes()
git(repo, APPLY, hunk_patch)
actual_after = (repo / "target.txt").read_bytes()
assert patch_after == patch_before
assert before_write == text(changed=True)
assert b"value010 = 1010;\r\n" in actual_after
assert b"value080 = 80;\r\n" in actual_after
assert b"value140 = 1140;\r\n" in actual_after
assert expected_after != actual_after
assert index_before == git(repo, ["ls-files", "--stage", "-z"]).stdout

info_repo = init("info-attributes")
(info_repo / ".git" / "info" / "attributes").write_bytes(b"*.txt text eol=crlf\n")
(info_repo / "target.txt").write_bytes(text(crlf=True))
commit(info_repo)
(info_repo / "target.txt").write_bytes(text(changed=True, crlf=True))
info_patch = git(info_repo, DIFF).stdout
info_index_before = git(info_repo, ["ls-files", "--stage", "-z"]).stdout
info_scratch = init("info-scratch-preview")
(info_scratch / "target.txt").write_bytes((info_repo / "target.txt").read_bytes())
preview_output = git(info_scratch, APPLY, info_patch, check=False)
real_output = git(info_repo, APPLY, info_patch)
assert preview_output.returncode != 0
assert (info_repo / "target.txt").read_bytes() == text(crlf=True)
assert info_index_before == git(info_repo, ["ls-files", "--stage", "-z"]).stdout

result = {
    "root": str(ROOT), "gitVersion": git(repo, ["--version"]).stdout.decode().strip(),
    "changedContext": {
        "normalizedPatchUnchanged": patch_after == patch_before,
        "workingBytesUnchangedBeforeApply": before_write == text(changed=True),
        "expectedLF": expected_after.count(b"\n"), "expectedCRLF": expected_after.count(b"\r\n"),
        "actualLF": actual_after.count(b"\n"), "actualCRLF": actual_after.count(b"\r\n"),
        "expectedSHA256": sha(expected_after), "actualSHA256": sha(actual_after),
        "unselectedHunkLFChanged": True, "indexUnchanged": True,
    },
    "infoAttributes": {
        "previewExitCode": preview_output.returncode,
        "previewStderr": preview_output.stderr.decode(errors="replace"),
        "realApplyExitCode": real_output.returncode, "actualRestoredCRLF": True, "indexUnchanged": True,
    },
}
(ROOT / "result.json").write_text(json.dumps(result, indent=2) + "\n")
print(json.dumps(result, indent=2))
