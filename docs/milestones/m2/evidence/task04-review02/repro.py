#!/usr/bin/env python3
"""Small real-Git checks for the task04 repair; all writes stay in a temp root."""

import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile


ROOT = Path(tempfile.mkdtemp(prefix="revia-task04-review02-", dir="/private/tmp"))
HOME = ROOT / "home"
HOME.mkdir()
(HOME / ".gitconfig").write_text("")
ENV = {key: value for key, value in os.environ.items() if not key.startswith(("GIT_", "REVIA_"))}
ENV.update(HOME=str(HOME), XDG_CONFIG_HOME=str(HOME), GIT_CONFIG_GLOBAL=str(HOME / ".gitconfig"),
           GIT_CONFIG_NOSYSTEM="1", GIT_ATTR_NOSYSTEM="1", GIT_TERMINAL_PROMPT="0")
LOG = []


def git(repo, *args, env=None, patch=None, check=True):
    result = subprocess.run(["/opt/homebrew/bin/git", "-C", str(repo), *args], env=ENV if env is None else env,
                            input=patch, capture_output=True, timeout=30)
    LOG.append({"cwd": str(repo), "args": list(args), "exit": result.returncode,
                "stdout": result.stdout.decode(errors="replace"), "stderr": result.stderr.decode(errors="replace")})
    if check and result.returncode:
        raise RuntimeError(LOG[-1])
    return result.stdout


def init(name):
    repo = ROOT / name
    repo.mkdir()
    git(repo, "init", "-q", "-b", "main")
    for key, value in [("user.name", "Review Fixture"), ("user.email", "fixture@example.invalid"),
                       ("commit.gpgsign", "false"), ("core.autocrlf", "false")]:
        git(repo, "config", key, value)
    return repo


def sha(data):
    return hashlib.sha256(data).hexdigest()


# GIT_CONFIG is not removed by in_isolated_directory. A copied attributes-file
# setting is therefore written into the original config, not scratch's config.
repo = init("config-repo")
attrs = ROOT / "external-attributes"
attrs.write_bytes(b"*.txt text eol=crlf\n")
git(repo, "config", "core.attributesFile", str(attrs))
config = repo / ".git/config"
config_before = config.read_bytes()
scratch = ROOT / "config-preview"
scratch.mkdir()
redirected = ENV | {"GIT_CONFIG": str(config)}
git(scratch, "init", "-q", env=redirected)
copied_attrs = scratch / ".attributes-file"
copied_attrs.write_bytes(attrs.read_bytes())
git(scratch, "config", "core.attributesFile", str(copied_attrs), env=redirected)
config_after = config.read_bytes()
assert config_before != config_after
assert git(repo, "config", "core.attributesFile").strip() == str(copied_attrs).encode()


# Linked worktrees read info/attributes from commonDir, not their per-worktree
# gitDir. The repair copies the latter, so its equality check rejects this case.
main = init("main-repo")
(main / ".git/info/attributes").write_bytes(b"*.txt text eol=crlf\n")
(main / "target.txt").write_bytes(b"alpha\r\nbeta\r\n")
git(main, "add", "--all")
git(main, "commit", "-q", "-m", "fixture")
linked = ROOT / "linked"
git(main, "worktree", "add", "-q", "-b", "linked", str(linked))
(linked / "target.txt").write_bytes(b"alpha\r\nBETA\r\n")
git_dir = Path(git(linked, "rev-parse", "--path-format=absolute", "--git-dir").decode().strip())
actual_info = Path(git(linked, "rev-parse", "--path-format=absolute", "--git-path", "info/attributes").decode().strip())
assert not (git_dir / "info/attributes").is_file()
assert actual_info.is_file()
linked_attrs = git(linked, "check-attr", "-a", "-z", "--", "target.txt")
linked_preview = init("linked-preview")
scratch_attrs = git(linked_preview, "check-attr", "-a", "-z", "--", "target.txt")
assert linked_attrs != scratch_attrs
patch = git(linked, "diff", "--no-color", "--no-ext-diff", "--no-textconv", "--binary", "--full-index", "--", "target.txt")
index_before = git(linked, "ls-files", "--stage", "-z")
git(linked, "-c", "apply.ignoreWhitespace=no", "apply", "--reverse", "--whitespace=nowarn", "-", patch=patch)
assert (linked / "target.txt").read_bytes() == b"alpha\r\nbeta\r\n"
assert git(linked, "ls-files", "--stage", "-z") == index_before


multiline = init("multiline-filter")
git(multiline, "config", "filter.copy.clean", "cat")
git(multiline, "config", "filter.copy.smudge", ":\ncat")
git(multiline, "config", "filter.copy.required", "true")
(multiline / ".gitattributes").write_bytes(b"target.txt filter=copy\n")
(multiline / "target.txt").write_bytes(b"alpha\nbeta\n")
git(multiline, "add", "--all")
git(multiline, "commit", "-q", "-m", "fixture")
(multiline / "target.txt").write_bytes(b"alpha\nBETA\n")
multiline_patch = git(multiline, "diff", "--no-color", "--no-ext-diff", "--no-textconv", "--binary", "--full-index", "--", "target.txt")
multiline_preview = init("multiline-preview")
(multiline_preview / ".gitattributes").write_bytes((multiline / ".gitattributes").read_bytes())
(multiline_preview / "target.txt").write_bytes((multiline / "target.txt").read_bytes())
config_output = git(multiline, "config", "--get-regexp", "^filter.").decode()
copied = []
for line in config_output.splitlines():
    line = line.rstrip()
    if " " in line:
        key, value = line.split(" ", 1)
        copied.append([key, value])
        git(multiline_preview, "config", "--add", key, value)
same_attrs = git(multiline, "check-attr", "-a", "-z", "--", "target.txt") == git(multiline_preview, "check-attr", "-a", "-z", "--", "target.txt")
assert same_attrs
git(multiline_preview, "-c", "apply.ignoreWhitespace=no", "apply", "--reverse", "--whitespace=nowarn", "-", patch=multiline_patch, check=False)
multiline_preview_exit = LOG[-1]["exit"]
assert multiline_preview_exit != 0
git(multiline, "-c", "apply.ignoreWhitespace=no", "apply", "--reverse", "--whitespace=nowarn", "-", patch=multiline_patch)
assert (multiline / "target.txt").read_bytes() == b"alpha\nbeta\n"


result = {
    "root": str(ROOT),
    "gitConfigRedirect": {"beforeSHA256": sha(config_before), "afterSHA256": sha(config_after),
                          "configuredAttributesBefore": str(attrs), "configuredAttributesAfter": str(copied_attrs),
                          "originalConfigChanged": True, "windowsReachability": "static shell.rs process-env inheritance; not a Windows desktop run"},
    "linkedInfoAttributes": {"perWorktreeGitDir": str(git_dir), "attemptedSourceExists": False,
                              "actualInfoAttributes": str(actual_info),
                              "realCheckAttrHex": linked_attrs.hex(), "scratchCheckAttrHex": scratch_attrs.hex(),
                              "equalityCheckWouldReject": True, "actualGitApplySucceeded": True, "indexUnchanged": True},
    "multilineFilter": {"sourceConfigOutput": config_output, "copiedEntries": copied,
                        "sameEffectiveAttributes": same_attrs, "scratchApplyExit": multiline_preview_exit,
                        "realApplyExit": 0, "realRestoredBytesHex": (multiline / "target.txt").read_bytes().hex()},
}
(ROOT / "result.json").write_text(json.dumps(result, indent=2) + "\n")
(ROOT / "commands.json").write_text(json.dumps(LOG, indent=2) + "\n")
(ROOT / "config.before").write_bytes(config_before)
(ROOT / "config.after").write_bytes(config_after)
print(json.dumps(result, indent=2))
