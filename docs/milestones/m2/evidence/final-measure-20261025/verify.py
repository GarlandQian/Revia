#!/usr/bin/env python3
"""Verify archived bytes offline; optionally compare still-existing sources."""

import argparse
import gzip
import hashlib
import json
from pathlib import Path, PurePosixPath
import stat
import sys


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check-sources", action="store_true")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent
    manifest_bytes = (root / "manifest.json").read_bytes()
    expected_manifest_sha = (root / "manifest.sha256").read_text().split()[0]
    manifest = json.loads(manifest_bytes)
    errors = []
    if sha256(manifest_bytes) != expected_manifest_sha:
        errors.append("manifest SHA256 does not match manifest.sha256")
    for support in manifest.get("supportFiles", []):
        if support["path"] not in ("README.md", "verify.py"):
            errors.append("unexpected support path: " + support["path"])
            continue
        content = (root / support["path"]).read_bytes()
        if len(content) != support["bytes"] or sha256(content) != support["sha256"]:
            errors.append("support file mismatch: " + support["path"])
    results = []
    seen = set()
    for entry in manifest["entries"]:
        relative = PurePosixPath(entry["archivePath"])
        item_errors = []
        if relative.is_absolute() or ".." in relative.parts or relative.parts[0] != "artifacts":
            errors.append("unsafe archive path: " + str(relative))
            continue
        if str(relative) in seen:
            errors.append("duplicate archive path: " + str(relative))
            continue
        seen.add(str(relative))
        archived = root.joinpath(*relative.parts)
        try:
            if not stat.S_ISREG(archived.lstat().st_mode):
                raise ValueError("archive is not a regular file")
            if not archived.resolve().is_relative_to(root):
                raise ValueError("archive resolves outside package")
            compressed = archived.read_bytes()
            if len(compressed) != entry["gzipBytes"]:
                item_errors.append("compressed size mismatch")
            if sha256(compressed) != entry["gzipSha256"]:
                item_errors.append("compressed SHA256 mismatch")
            original = gzip.decompress(compressed)
            if len(original) != entry["sourceBytes"]:
                item_errors.append("decompressed size mismatch")
            if sha256(original) != entry["sourceSha256"]:
                item_errors.append("decompressed SHA256 mismatch")
            if args.check_sources:
                source = Path(entry["sourcePath"])
                if not stat.S_ISREG(source.lstat().st_mode):
                    raise ValueError("source is not a regular file")
                current = source.read_bytes()
                if current != original:
                    item_errors.append("source bytes differ from decompressed bytes")
        except Exception as error:
            item_errors.append(type(error).__name__ + ": " + str(error))
        results.append({"archivePath": str(relative), "ok": not item_errors, "errors": item_errors})
    actual = {p.relative_to(root).as_posix() for p in (root / "artifacts").rglob("*") if p.is_file()}
    if actual != seen:
        errors.append("payload inventory mismatch: " + json.dumps({
            "extra": sorted(actual - seen), "missing": sorted(seen - actual)}))
    if len(results) != manifest["totals"]["fileCount"]:
        errors.append("manifest entry count mismatch")
    for field, entry_field in (("sourceBytes", "sourceBytes"), ("gzipBytes", "gzipBytes")):
        if sum(e[entry_field] for e in manifest["entries"]) != manifest["totals"][field]:
            errors.append("manifest total mismatch: " + field)
    passed = sum(item["ok"] for item in results)
    report = {
        "mode": "sources-and-offline" if args.check_sources else "offline",
        "manifestSha256": sha256(manifest_bytes),
        "fileCount": len(results),
        "passed": passed,
        "failed": len(results) - passed,
        "errors": errors,
        "ok": not errors and passed == len(results),
        "results": results,
    }
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
