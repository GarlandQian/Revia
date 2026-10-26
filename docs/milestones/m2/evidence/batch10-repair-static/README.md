# B10 static evidence archive

This temporary package preserves original static evidence only:

- `integration-5f/`: all 56 original check/build files for commit
  `5f632d661ce5dc37b237c27f60205c8d41fcf86c`, plus the separately sourced
  historical `failure.json` context for PID 23756.
- `repair-2a51/`: 9 dependency-resolution files, 15 Rust-check files,
  16 build files, and 6 original analysis/identity/review files associated
  with commit `2a51b160d1c3871b9c67fee81ae6f082910bc1f8`.
- `manifest.json`: exact original absolute path mapping, original/stored
  byte counts and SHA256, version and PID associations, content categories,
  per-file scope/status, recorded command outcomes, and known limitations.
- `verification.json`: a separately run archive verifier's output.

Every original is gzip-compressed without changing its bytes. Empty output,
warnings, historical failures, command preparation, and source-manifest
differences remain preserved. Gzip uses mtime=0; every item is decompressed
and checked against its original SHA256.

The dependency-resolution before/after lists intentionally differ only at
`src-tauri/Cargo.lock`; this is preparation, not a passing product test.
The pre-commit resolve/rust base remains 5f in the original records. The
recorded source association with fixed 2a51 is kept separately, not rewritten.
The old build did not record exact environment values; this gap is retained.

`archive-audit.md` is historical analysis preserved verbatim, not a current
desktop verdict. In particular, its C05/run02 statements describe the time
it was written. Later run02 actions are intentionally outside this package.
No current C05 or run02 status is asserted here. The old C02 failure remains
a failure; its post-hoc `failure.json` is not represented as a raw CUA log.

No desktop files, fixtures, live data/logs, app bundles, executables, target,
node_modules, .git, or full repositories are included. No current source-tree
check, build, test, GUI action, commit, or milestone completion was performed.

To verify the package without accessing any original location:

```sh
python3 verify.py
```

To additionally compare the preserved files with the still-existing originals:

```sh
python3 verify.py --check-sources
```

Do not rerun `archive.py` in this directory: it intentionally refuses to
overwrite archived files. The scripts and this scope note are newly generated
packaging artifacts, not historical evidence.
