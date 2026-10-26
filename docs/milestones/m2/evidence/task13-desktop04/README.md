# Completed desktop original-evidence archive

This package contains the allowlisted, separately recorded desktop snapshots,
UI captures, run metadata and stdout/stderr from old 5f run01 (PID 23756) and
B10 2a51 run01/run02 (PIDs 47783 and 55553), plus the final approved C05 verdict.

Original screenshot files are stored unchanged. All 31 are named `.png` but
their byte signatures identify JPEG; the manifest records the mismatch and
keeps both original names and bytes without transcoding or renaming. All other
original files, including binary
snapshot/recovery/save-as bytes, AX text, JSON and empty output, are compressed
from their original bytes with gzip mtime=0. Every item has an original absolute
path, version/PID associations and original/stored SHA256 in `manifest.json`.
No JSON or log has been reserialized, edited or reconstructed as evidence.

The entire live fixture root is excluded, including its real repository, .git,
home and application data. Frozen `worktree/` and `data/` directories already
created by the original snapshot recorder remain included. No .app, target,
node_modules, .git, __pycache__, source repository or current binary is included.

Important sample boundaries remain separate:

- Old 5f C02 is a failure despite exitCode 0: result delivery had not occurred.
  The captured Git operation had succeeded; this is not a data-loss claim.
- B10 C03 is invalid: Dock Quit did not execute and delivery was automatic.
- B10 C06 Save As succeeded to a new path, but its hold automatically timed out;
  it is not an explicit-release/Stay pass.
- The earlier C05-not-executed sample stays not-executed. The later approved C05
  sample has a real preparation failure, explicit result delivery, pending/Stay,
  separate final user-approved close acknowledgement, and recorded exit 0.
- Font increase remains unverified. Historical action/verdict records and
  intermediate boundaries are preserved, not rewritten as current conclusions.
- Later snapshots carry earlier-PID markers. Embedded marker ownership is kept
  separately from the snapshot run; a copied marker is never a repeated action.

The full raw CUA transcript was not supplied. PNG/AX, identity records, action
notes and process logs are not represented as interchangeable evidence. Missing
capture companions are not fabricated. The manifest's pass/status labels only
refer to bounded samples, not independent proof from every file or full M2
completion. No GUI action, product test, source check or fixture write happened
while packaging.

`selection-before-verdict.json` is the newly generated initial allowlist. The
final verdict path is separately recorded in the final manifest. Packaging
scripts and this scope note are not historical evidence.

`packaging-image-signature-note.json` records an earlier verifier's incorrect
PNG-magic assumption and the subsequent read-only signature inventory. That
attempt remains at its original temporary path without modification/deletion;
this fresh package uses the corrected verifier. The note is a packaging incident
record, not fabricated raw tool output or a product failure.

Verify archive bytes without reading original locations:

```sh
python3 verify.py
```

Also compare with still-existing original files:

```sh
python3 verify.py --check-sources
```

Do not rerun `archive.py` in this directory; it refuses to overwrite outputs.
