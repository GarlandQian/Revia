# Task 04 targeted desktop recheck and D06

Planning-agent transcription of observed CUA AX states and screenshots, not raw
AX output or local screenshot files. Source 764d13e0629c3e43c32b17015bdf20916a339233,
binary SHA256 246618d92c041ab723741e8140cdadb957870d1cea961e5aa0d532cb88176f71.

## Targeted RM recheck

- Ordinary UI PID 54348, independent HOME home-ui-r2 and data-ui-r2 under the
  fixed task04 fixture root, real Git 2.56.0, production watcher enabled, no
  wrapper/hold/armed write. A/L were not operated on in this round.
- Initial ordinary 00-hunks.ts retained its three hunk actions. B's Git RM
  unstaged M displayed old -> new metadata, readable diff, whole-file stage and
  discard, but no hunk buttons in split or unified views.
- Whole-file discard modal identified 50-renamed-new.txt and the complete B
  worktree path. Cancel was focused by default. Tab then Space confirmed.
  The addition disappeared; staged R remained. The new Completed record stored
  both related paths, the complete original addition and old-path absence.
- Pinned CLI mutation restored only that synthetic addition after asserting
  baseline bytes and nonsymlink type. Watcher automatically restored RM.
- UI whole-file stage succeeded without a Git error. Staged R displayed the
  addition, with no hunk buttons in either view. UI whole-file unstage produced
  old-path unstaged D plus new-path untracked, without changing worktree files.
- Recovery UI displayed timestamp, whole-file source, saved/completed status,
  original B path, and both old/new related paths. No restore/cleanup claimed.
- Version-pinned snapshots and verify-r2 script check exact bytes, index entries,
  related paths, identity and unchanged unrelated A/B/L files. This is separate
  from round1's 151 assertions, not a replacement or rerun of round1.

## Tool detour and ordinary rendering

- While adding the standard fixture through the native folder picker, key/paste
  attempts did not change its Go To text. Binding the system panel service with
  cua.getApp timed out. No app completion/failure is inferred from that timeout.
  Rebinding the tested app worked; selecting the already-observed fixture recent
  row and Open via AX opened the correct repository. No system process was killed.
- Vue split showed both sides colored after asynchronous highlighting. Swift
  unified likewise showed expected colors. Standard generated.ts was expanded.
- An element-targeted scroll did not visibly change position, so it is excluded.
  Coordinate-targeted diff scrolling did move: unified down10/up5 showed around
  lines657 then406; split down10/up5 settled near840 after inertia, initially
  plain text then automatically colored without another click.
- Split scrollbar drag reached around7006 then returned around840; unified drag
  reached around6987 then returned around839. Text was readable, no whole blank
  block, cached colors restored. Screenshots are in CUA conversation only.
- Ordinary PID54348 exited normally with Cmd-Q; CLI subsequently confirmed absent.
  The long tool-detour interval was not an active 30-minute soak and is not used
  as task11/Q98 evidence.

## Four independent D06 processes

Each launch pinned source and binary hash, isolated HOME/data, kept a visible
window open, and used production cache policy. Actual results record focused=true,
focusLosses=0, hidden=false, hiddenTransitions=0, ok=true. Each had 60 requested
screens, 60 actual screens, 60 rAF, no fallback and no uncovered frames.

| Kind/view | PID | First screen | Original appear/warm | Separate outward DOM text | Return DOM text/full colors |
| --- | ---: | ---: | ---: | ---: | ---: |
| Standard split | 5871 | 113ms | 21/7ms, cached | 3ms | 1/34ms |
| Standard unified | 6449 | 99ms | 17/6ms, cached | 2ms | 1/33ms |
| True-cold split | 7012 | 106ms, not standard load | 26/3ms, initially uncached | 4ms | 0/30ms |
| True-cold unified | 7479 | 111ms, not standard load | 20/4ms, initially uncached | 5ms | 1/31ms |

- Standard legacy appear21/17 are above its nominal16.7 budget; both remain in
  original JSON with withinOneFrame=false. Warm7/6 pass. Separate DOM probes
  directly observed cached outward and return text in their first rAF; those
  numbers are not substituted for legacy measurements.
- Cold fixture has30000 source lines modified,30001 split/60001 unified display
  rows. Both legacy and independent outward targetWasCached=false; independent
  probe was missing40 rows beforehand. Cold split's first rAF at3ms showed a
  visible40-row placeholder and aligned canvas, with text at4ms. Cold unified's
  first rAF at8ms already showed all40 text rows (text timestamp5ms).
- Returned complete colors took30-34ms, not first-frame full color. Standard
  split/unified and cold split frame maxima34/23/30ms remain; cold unified19ms.
  Q84 does not require every slow frame to disappear.
- Four measurement windows were inspected after automatic timing, not during
  timing. Each exited normally with Cmd-Q; CLI confirmed all four PIDs absent.
- No code was rebuilt or changed while the five tested processes were alive.
  No task11 final30-minute soak is claimed.
