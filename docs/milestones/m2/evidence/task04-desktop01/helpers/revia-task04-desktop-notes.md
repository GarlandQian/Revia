# Task 04 desktop round 1: real CUA observations

Recorded by the planning agent from CUA AX states and screenshots on 2026-10-23.
This is a transcription, not raw AX output or a screenshot artifact.

- Source: 36dad654fe0d2d337e34d7bd69aaae04d833903f.
- Binary SHA256: 6e119703a7ecf9db37b0f2db0072bbb802f6088432b59b819caeebab21d8dfc9.
- Main real run PID 71299, restart PID 79082, isolated HOME/data-ui-r1.
- Earlier PID 70061 used Apple Git 2.54, exited without any write; excluded from
  write acceptance. Corrected isolated .zshrc made the tested run use Git 2.56.
- All fixture writes initiated in the UI were limited to prepared A/L repositories.
  B was retained as an untouched independent repository.
- Screenshots were displayed by CUA only. No local screenshot file is claimed.

## Observed passes

1. Split middle hunk: confirmation showed 00-hunks.ts, exact A worktree path,
   section value076, full-file preservation and unchanged index warning.
   Default focus was Cancel. Alt+Super+Z inside modal did not change its scope.
   Return on Cancel created no record; re-open and Tab/Space on confirmation
   removed only line 80's change and stored the entire original file.
2. Unified first hunk: switched view, selected section value006, confirmed with
   Tab/Space. Only line 10's change was removed; line 140 remained modified.
3. Stale modal: opened line 140 hunk confirmation, externally edited line 80 to
   3080 via pinned fixture helper. Production watcher invalidated confirmation:
   message said file or selection changed, and confirm button became disabled.
   Tab/Space could not execute it. After cancellation, both fresh hunks appeared.
4. Whole-file shortcut: clicked diff body near line 140 and pressed Alt+Super+Z.
   Modal explicitly said whole file. Confirming removed both remaining hunks,
   rather than only the nearest hunk. A previous invocation with view-toggle
   focus also opened whole-file confirmation, then was canceled.
5. Storage failure: chmod only isolated recovery directory to 0500. Confirmed
   10-partial.ts whole-file discard; UI showed environment/IO Permission denied.
   Worktrees, index and record count remained unchanged. Restored 0700.
6. Hold/isolation: armed wrapper for one actual A apply, confirmed 10-partial.ts.
   Same-worktree stage/discard/hunk buttons disabled with Operation in progress.
   Browsed Chinese-space file while held, then switched to linked worktree L.
   L remained browsable and writable. Explicitly released within 90 seconds.
   A completion did not replace L's three-hunk display. Later A showed only the
   staged 10-partial.ts change, proving discard restored index, not HEAD.
7. Lost result: armed wrapper for one real L apply. Confirmed middle hunk; wrapper
   ran real Git successfully, then returned 141. UI showed Needs verification,
   saved record and no automatic continuation. Fresh diff showed only line
   10/140 changes. Record had complete before state but no outcome.json.
8. Recovery UI in L showed timestamps, source, target, saved/completed versus
   saved/pending status and original A/L paths. A records were marked noncurrent
   or identity unconfirmed. In A, L record was marked noncurrent instead.
9. Binary and mode-only targets had whole-file discard and no hunk action UI.
   Whole-file discards succeeded. Binary bytes were preserved in saved state;
   executable-before=true and after=false were preserved for mode-only.
10. Deleted file whole discard saved nonexistent-before and restored baseline.
    Rename whole discard saved both old/new paths and removed only unstaged
    addition; staged rename remained. Chinese-space filename whole discard
    succeeded and its full path was shown without escaping damage.
11. Staged-only rename had no discard button; shortcut did not open a modal.
    Symlink was readonly with no discard and shortcut did nothing. Untracked
    group expanded, selected untracked file had only stage and no discard;
    shortcut did nothing. Both symlink and untracked bytes stayed unchanged.
12. Quit PID 71299 normally, relaunch same binary/data as PID 79082. Recovery UI
    showed all ten records, original timestamps/sources/paths, nine completed
    and one pending. Snapshots were byte-identical and lost apply count stayed
    one. PID 79082 was then quit normally.

## New finding: R04-GUI1 / P2

In A, the already-staged Git rename old->new also had an unstaged text addition.
The row was 50-renamed-new.txt +1 -0 M with Help showing old -> new. Its diff
exposed both Stage this hunk and Discard this hunk for @@ -1 +1,2 @@.
This was observed again immediately after whole-file confirmation began, when
both hunk buttons appeared disabled during writing. No hunk operation was
executed on the rename; whole-file discard was the only executed operation.

Spec section 2/T03 requires Git-recognized renames to have whole-file operations
only. supports_hunk_actions checks status enum but not original_path. Porcelain
type-2 RM creates a staged Renamed entry plus an unstaged Modified entry, retaining
original_path for both. Therefore this unstaged entry passes the shared gate;
the UI exposes and backend hunk_patch accepts credentials for it.

Fix shared capability checks using actual Git rename metadata, cover type-2 RM
without guessing delete/untracked pairs, and keep both whole-file operations and
full before/after records working. Check staging/unstaging callers too.

## Evidence timing caveat

partial-finished-on-linked.json was captured immediately after releasing the
wrapper and before the operation completed. Despite its label, it is not used
as proof of completion. The next stable linked-lost-result snapshot, final
Completed outcome and after-restart snapshot prove the A completion. The
assertion script accounts for both explicitly initiated A/L writes across this
interval rather than manufacturing an intermediate snapshot.

## Remaining

New rename fix, incremental review and targeted desktop recheck remain required.
D06 has not run for this source. CI 39965209864 is in progress at dispatch time.
No Windows desktop execution or final 30-minute task 11 test is claimed.
