from collections import Counter
from datetime import datetime, timezone
import gzip
import hashlib
import json
import os
from pathlib import Path
import stat
import sys


ROOT = Path(__file__).resolve().parent
OLD = Path('/private/tmp/revia-m2-desktop-_2le4ddh')
NEW = Path('/private/tmp/revia-b10-vzc_he0k/desktop')
FORBIDDEN = {'.git', 'target', 'node_modules', '__pycache__'}
OLD_COMMIT = '5f632d661ce5dc37b237c27f60205c8d41fcf86c'
NEW_COMMIT = '2a51b160d1c3871b9c67fee81ae6f082910bc1f8'
OLD_BINARY = '0623cacdf0c395a00d2eba975df7699c0e72d788a0d80d84f7b94a25c1e3a09d'
NEW_BINARY = 'fd5ffc86131aebbe77850487192005166a63c1a98f9e97aa72b3ddaa1d96988d'


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def image_media_type(raw):
    if raw.startswith(b'\x89PNG\r\n\x1a\n'):
        return 'image/png'
    if raw.startswith(b'\xff\xd8\xff'):
        return 'image/jpeg'
    raise AssertionError('unrecognized original image signature')


def load(path):
    return json.loads(path.read_bytes())


def source_state(path):
    value = path.lstat()
    assert stat.S_ISREG(value.st_mode), ('not regular', str(path))
    assert path.resolve(strict=True) == path, ('symlink source', str(path))
    assert not path.is_relative_to(FIXTURE)
    assert not any(part in FORBIDDEN or part.endswith('.app') for part in path.parts)
    return (value.st_dev, value.st_ino, value.st_size, value.st_mtime_ns,
            value.st_ctime_ns, value.st_mode)


assert len(sys.argv) == 2, 'Supply the final verdict path explicitly after approval to seal.'
verdict = Path(sys.argv[1])
assert verdict.is_absolute() and verdict.parent == NEW and verdict.is_file()
assert verdict.resolve(strict=True) == verdict
assert sha(verdict.read_bytes()) == 'f16b3eafb340dccdccc1169087d1c3bcdc4c0c13cd698098f661072621b54f80'
selection = load(ROOT / 'selection-before-verdict.json')
FIXTURE = Path(selection['liveFixtureExcluded'])
entries = list(selection['entries'])
assert str(verdict) not in {entry['source'] for entry in entries}
verdict_stat = source_state(verdict)
entries.append({
    'source': str(verdict), 'relative': str(verdict.relative_to(NEW)),
    'group': 'desktop-2a51', 'sourceBytes': verdict_stat[2],
    'sourceMode': oct(stat.S_IMODE(verdict_stat[5])),
    'sourceMtimeNs': verdict_stat[3], 'sourceSha256': sha(verdict.read_bytes()),
    'encoding': 'gzip',
})


def selected_tree(root):
    files = set()
    for parent, directories, names in os.walk(root, followlinks=False):
        parent = Path(parent)
        for name in list(directories):
            path = parent / name
            if path == FIXTURE or name in FORBIDDEN or name.endswith('.app'):
                directories.remove(name)
            else:
                assert not path.is_symlink(), ('unexpected directory symlink', str(path))
        for name in names:
            path = parent / name
            if name in FORBIDDEN or name.endswith('.app'):
                continue
            source_state(path)
            files.add(str(path))
    return files


for tree, group in ((OLD, 'desktop-5f'), (NEW, 'desktop-2a51')):
    assert selected_tree(tree) == {entry['source'] for entry in entries if entry['group'] == group}, ('selection changed', str(tree))

raw = {}
states = {}
for entry in entries:
    path = Path(entry['source'])
    states[str(path)] = source_state(path)
    content = path.read_bytes()
    assert len(content) == entry['sourceBytes']
    assert sha(content) == entry['sourceSha256'], ('original changed since enumeration', str(path))
    assert source_state(path) == states[str(path)]
    raw[str(path)] = content


def j(path):
    return json.loads(raw[str(path)])


def b(path):
    return raw[str(path)]


runs = {}
for label, directory, commit, binary in (
    ('5f-run01', OLD / 'run01', OLD_COMMIT, OLD_BINARY),
    ('2a51-run01', NEW / 'run01', NEW_COMMIT, NEW_BINARY),
    ('2a51-run02', NEW / 'run02', NEW_COMMIT, NEW_BINARY),
):
    launch = j(directory / 'launch.json')
    exit_record = j(directory / 'exit.json')
    assert launch['sourceCommit'] == commit and launch['binarySha256'] == binary
    assert exit_record['exitCode'] == 0
    assert exit_record['sourceUnchanged'] is True and exit_record['binaryUnchanged'] is True
    runs[label] = {'pid': launch['pid'], 'sourceCommit': commit, 'binarySha256': binary,
                   'launchSource': str(directory / 'launch.json'),
                   'exitSource': str(directory / 'exit.json'),
                   'recordedExit': exit_record,
                   'unchangedClaimBasis': 'preserved exit record; no current source or binary was inspected by archive'}

samples = {
    'C01StageStay': {'status': 'pass', 'run': '5f-run01', 'scope': 'Window-close ask, Stay, explicit delivery, remained open.'},
    'C02UnstageStay': {'status': 'failed', 'run': '5f-run01', 'scope': 'Successful Git result captured, then Cmd-Q exited before result delivery. Not evidence that Git was killed or data was lost.'},
    'R07OldRead01': {'status': 'partial', 'run': '5f-run01', 'scope': 'Historical held-read/review observations; not a new-version rerun or complete task07 acceptance.'},
    'B10C01StageStay': {'status': 'pass', 'run': '2a51-run01', 'scope': 'Window-close ask, Stay, explicit delivery, remained open.'},
    'B10C02UnstageStay': {'status': 'pass', 'run': '2a51-run01', 'scope': 'Real Cmd-Q ask, Stay, explicit delivery, remained open.'},
    'B10C03DockWait': {'status': 'invalid', 'run': '2a51-run01', 'scope': 'Dock tool timed out; Dock Quit was never executed. explicit:false automatic delivery is not an explicit-release, Dock Quit, or Wait pass.'},
    'B10C04MenuWait': {'status': 'pass', 'run': '2a51-run01', 'scope': 'Native Quit menu, Wait, explicit delivery, then exit 0.'},
    'B10C05RestorePending': {'status': 'not-executed', 'run': '2a51-run02', 'scope': 'Earlier restore confirmation was not approved; no restore command ran. Permissions were restored. This is not the later approved sample.'},
    'B10C05ApprovedPending': {'status': 'pass', 'run': '2a51-run02', 'scope': 'Approved real restore preparation failed before any path write; Close/Wait, explicit error delivery, pending blocked auto-close, Stay, then separately authorized final acknowledgement on a new Close led to exit 0.', 'finalVerdictSource': str(verdict)},
    'B10C06SaveAsStay': {'status': 'partial', 'run': '2a51-run02', 'scope': 'Native Save As to a new path succeeded and Cmd-Q ask/Stay was observed. Delivery timed out automatically (explicit:false); not an explicit-release Stay pass.'},
    'font-shortcuts': {'status': 'partial', 'run': '2a51-run01', 'scope': 'Font decrease/save observed; font increase is not verified. Repeated plus-key names do not prove DOM plus-key delivery.'},
    'review-persistence-observations': {'status': 'partial', 'run': '2a51-run01', 'scope': 'Preserved startup review-mark observation; not a complete task07 acceptance verdict.'},
    'review-recovery-observations': {'status': 'partial', 'run': '2a51-run02', 'scope': 'Preserved restart, shell-error, readonly/recovery, and external-edit observations; no expanded acceptance verdict is generated by archival.'},
}

# These assertions inspect only already-recorded bytes, not the live fixture.
assert not (OLD / 'run01-after/data/release/C02UnstageStay').as_posix() in raw
assert not (OLD / 'run01-after/data/delivered/C02UnstageStay.json').as_posix() in raw
assert j(OLD / 'C02-captured/data/captured/C02UnstageStay.json')['outcome'] == 'ok'
assert j(NEW / 'C03-auto-timeout/data/delivered/B10C03DockWait.json')['explicit'] is False
assert j(NEW / 'C06-delivered-timeout/data/delivered/B10C06SaveAsStay.json')['explicit'] is False
assert j(NEW / 'C05-approved-captured/data/captured/B10C05ApprovedPending.json')['outcome'] == 'error'
receipt = j(NEW / 'C05-approved-released/data/delivered/B10C05ApprovedPending.json')
assert receipt['explicit'] is True
confirmation = j(NEW / 'close-confirmation.json')
assert confirmation['authorizedByUser'] is True and confirmation['pid'] == runs['2a51-run02']['pid']
assert receipt['releasedAtMs'] < confirmation['clickedAtMs'] < runs['2a51-run02']['recordedExit']['endedAt'] * 1000
for label in ('head', 'status', 'index', 'worktree-diff', 'index-diff'):
    assert b(NEW / ('C05-approved-before/' + label + '.stdout')) == b(NEW / ('run02-after/' + label + '.stdout'))
assert j(NEW / 'C05-approved-before/state.json')['files'] == j(NEW / 'run02-after/state.json')['files']
assert j(NEW / 'C05-approved-permission-control.json')['verifiedMode'] == 493
final = j(verdict)
assert final['sourceCommit'] == NEW_COMMIT and final['binarySha256'] == NEW_BINARY
assert final['pid'] == runs['2a51-run02']['pid'] and final['passed'] is True
assert len(final['checks']) == 13 and all(value is True for value in final['checks'].values())


def json_if_object(path):
    if path.suffix.lower() != '.json':
        return {}
    value = json.loads(raw[str(path)])
    return value if isinstance(value, dict) else {}


def context(entry):
    relative = Path(entry['relative'])
    top = relative.parts[0]
    if entry['group'] == 'desktop-5f':
        if top in {'fixture.json', 'prepare.json', 'prepare.stdout', 'prepare.stderr', 'launch.py', 'evidence.py'}:
            return None, ['5f-run01']
        return '5f-run01', ['5f-run01']
    if top == 'ui':
        number = int(relative.name[:2])
        run = '2a51-run01' if number <= 9 else '2a51-run02'
        identity = NEW / 'ui' / (relative.stem + '.json')
        if str(identity) in raw:
            assert j(identity)['pid'] == runs[run]['pid']
            assert j(identity)['source'] == NEW_COMMIT
        return run, [run]
    if top in {'fixture.json', 'fixture-continuity.json', 'launch.py', 'evidence.py'}:
        return None, ['2a51-run01', '2a51-run02']
    run02 = (top.startswith(('C05', 'C06', 'run02')) or top in {
        'readonly-before-external', 'exports', 'permission-control.json',
        'binary-external-edit.json', 'save-as-target.json', 'close-confirmation.json', verdict.name,
    })
    run = '2a51-run02' if run02 else '2a51-run01'
    return run, [run]


def sample(entry, obj):
    relative = Path(entry['relative'])
    marker = obj.get('id') or relative.name.removesuffix('.json')
    if marker in samples:
        return marker
    top = relative.parts[0]
    if entry['group'] == 'desktop-5f':
        if top.startswith('C01'):
            return 'C01StageStay'
        if top == 'C02-captured' or top == 'run01-after' or str(relative) == 'run01/exit.json':
            return 'C02UnstageStay'
        if top == 'ui':
            number = int(relative.name[:2])
            return 'C02UnstageStay' if number == 16 else ('C01StageStay' if number >= 11 else 'R07OldRead01')
        if top.startswith('review-'):
            return 'R07OldRead01'
        return None
    if top == 'ui':
        number = int(relative.name[:2])
        if number == 1:
            return 'review-persistence-observations'
        if number in (2, 3):
            return 'B10C01StageStay'
        if 4 <= number <= 6:
            return 'B10C02UnstageStay'
        if 7 <= number <= 9:
            return 'B10C04MenuWait'
        if number in (11, 12):
            return 'B10C05RestorePending'
        if number in (17, 18):
            return 'B10C06SaveAsStay'
        if number >= 19:
            return 'B10C05ApprovedPending'
        return 'review-recovery-observations'
    if top.startswith('C05-approved') or top in {'close-confirmation.json', verdict.name, 'run02-after'} or str(relative) == 'run02/exit.json':
        return 'B10C05ApprovedPending'
    if top in {'C05-before', 'C05-not-executed', 'permission-control.json'}:
        return 'B10C05RestorePending'
    if top.startswith('C06') or top in {'exports', 'save-as-target.json'}:
        return 'B10C06SaveAsStay'
    if top.startswith('C01'):
        return 'B10C01StageStay'
    if top.startswith('C02'):
        return 'B10C02UnstageStay'
    if top == 'C03-auto-timeout' or top == 'dock-tool-failure.json':
        return 'B10C03DockWait'
    if top in {'C04-waiting', 'run01-after', 'run01-verdict.json'} or str(relative) == 'run01/exit.json':
        return 'B10C04MenuWait'
    if top in {'font-adjusted', 'font-shortcuts.json'}:
        return 'font-shortcuts'
    if top in {'readonly-before-external', 'binary-external-edit.json'}:
        return 'review-recovery-observations'
    return None


def category(entry):
    relative = Path(entry['relative'])
    if entry['encoding'] == 'identity':
        return 'original-ui-image-unmodified'
    if relative.parts[0] == 'ui':
        return 'original-ui-accessibility-text' if relative.suffix == '.txt' else 'original-ui-identity-record'
    if 'data' in relative.parts:
        return 'frozen-application-data-snapshot-not-live-data'
    if 'worktree' in relative.parts:
        return 'frozen-fixture-small-file-bytes-not-live-repository'
    if relative.parts[0] == 'exports':
        return 'original-new-path-save-as-output'
    if relative.name.endswith(('.stdout', '.stderr')) or relative.name in {'stdout', 'stderr'}:
        return 'raw-recorded-command-or-process-output'
    if relative.suffix == '.py':
        return 'original-recorder-script-not-executed'
    if 'verdict' in relative.name or relative.name in {'close-confirmation.json', 'dock-tool-failure.json', 'font-shortcuts.json'}:
        return 'recorded-analysis-or-action-context-not-raw-cua-transcript'
    return 'original-snapshot-or-run-metadata'


files = []
for entry in sorted(entries, key=lambda value: (value['group'], value['relative'])):
    path = Path(entry['source'])
    original = raw[str(path)]
    use_gzip = entry['encoding'] == 'gzip'
    encoded = gzip.compress(original, compresslevel=9, mtime=0) if use_gzip else original
    stored = entry['group'] + '/' + entry['relative'] + ('.gz' if use_gzip else '')
    destination = ROOT / stored
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open('xb') as handle:
        handle.write(encoded)
    recovered = gzip.decompress(destination.read_bytes()) if use_gzip else destination.read_bytes()
    assert recovered == original
    obj = json_if_object(path)
    run, related_runs = context(entry)
    sample_id = sample(entry, obj)
    record_pid = obj.get('pid')
    marker = obj.get('id') or path.name.removesuffix('.json')
    record_run = samples[marker]['run'] if marker in samples else None
    if record_pid is not None and record_run:
        assert record_pid == runs[record_run]['pid']
    if record_run is None and record_pid is not None:
        matches = [key for key, value in runs.items() if value['pid'] == record_pid]
        record_run = matches[0] if matches else None
    files.append({
        'source': str(path), 'stored': stored, 'group': entry['group'],
        'gzip': use_gzip, 'contentCategory': category(entry),
        'originalExtension': path.suffix,
        'detectedImageMediaType': image_media_type(original) if not use_gzip else None,
        'imageExtensionMismatch': not use_gzip and path.suffix.lower() == '.png' and image_media_type(original) != 'image/png',
        'sourceBytes': len(original), 'storedBytes': len(encoded),
        'sourceSha256': sha(original), 'storedSha256': sha(encoded),
        'sourceMode': entry['sourceMode'], 'sourceMtimeNs': entry['sourceMtimeNs'],
        'sourceCommit': OLD_COMMIT if entry['group'] == 'desktop-5f' else NEW_COMMIT,
        'binarySha256': OLD_BINARY if entry['group'] == 'desktop-5f' else NEW_BINARY,
        'captureRun': run, 'capturePid': runs[run]['pid'] if run else None,
        'relatedRuns': related_runs,
        'sampleId': sample_id,
        'sampleStatus': samples[sample_id]['status'] if sample_id else 'partial',
        'sampleStatusMeaning': 'Association with a bounded sample, not independent proof from this file or whole-task acceptance.',
        'embeddedRecordPid': record_pid,
        'embeddedRecordRun': record_run,
        'embeddedRecordSourceCommit': runs[record_run]['sourceCommit'] if record_run else None,
        'historicalRecordFromEarlierRun': bool(record_run and run and record_run != run),
        'knownLimitations': ['SCOPED_SAMPLE_ONLY', 'SNAPSHOT_HISTORY_NOT_REEXECUTION'],
        'originalBytesVerified': True,
    })

for source, original in raw.items():
    assert source_state(Path(source)) == states[source], ('source state changed while archiving', source)
    assert Path(source).read_bytes() == original, ('source bytes changed while archiving', source)

manifest = {
    'schemaVersion': 1,
    'createdAtUtc': datetime.now(timezone.utc).isoformat(),
    'archiveKind': 'completed-desktop-original-evidence-package',
    'boundary': 'Original bytes from completed desktop runs only. Packaging performs no product test, GUI action, fixture mutation, or current repository/binary check. No whole-task or milestone completion is asserted.',
    'runs': runs,
    'samples': samples,
    'finalVerdictSource': str(verdict),
    'excludedLiveFixtureRoot': str(FIXTURE),
    'excludedNames': sorted(FORBIDDEN | {'*.app'}),
    'exclusionBoundary': 'The entire fixtureRoot (including its real repo, .git, home and revia-data) was excluded before traversal. Only separately recorded snapshot worktree/data directories are included.',
    'knownLimitations': {
        'SCOPED_SAMPLE_ONLY': 'Pass labels are limited to each sample catalog scope; error outcomes may be the expected error-handling case. Error, partial, invalid and not-executed observations are not dropped or promoted.',
        'SNAPSHOT_HISTORY_NOT_REEXECUTION': 'Later data snapshots contain prior-PID claimed/captured/release/delivered markers and recovery records. embeddedRecordRun identifies known marker ownership; copying a historical marker never counts as a new action on the snapshot PID.',
        'CUA_TRANSCRIPT_GAP': 'No complete raw CUA request/response transcript was supplied. PNG/AX, identity records, action/verdict notes and process logs remain distinct. Missing PNG/identity companions were not fabricated.',
        'FONT_INCREASE_UNVERIFIED': 'Font decrease/save and restart observations do not prove the increase shortcut. The original plus-key observations are preserved without blind repair or a passing increase claim.',
        'DOCK_SAMPLE_INVALID': 'B10C03DockWait did not execute Dock Quit and delivered with explicit:false. It is not an explicit-release, Dock Quit or Wait pass.',
        'SAVE_AS_PARTIAL': 'B10C06SaveAsStay created a new output file successfully, but automatic 300-second release means the hold/release part is partial.',
        'C05_GENERATIONS_SEPARATE': 'The earlier C05-not-executed sample remains not-executed. The later B10C05ApprovedPending sample has its own approved action, explicit delivery, pending/Stay, final user acknowledgement and exit record. Earlier intermediate boundaries are historical, not rewritten current verdicts.',
        'OLD_EXIT_FAILURE_RETAINED': 'Old 5f exitCode 0 is still a C02 safety failure because it occurred before result release. It does not imply Git was killed or file data was lost.',
        'NO_MILESTONE_COMPLETION': 'This archive does not independently establish remaining task05/07/08/12/13, CI, D06 or Q98 gates.',
        'IMAGE_EXTENSION_MISMATCH': 'The 31 original screenshot filenames end in .png but their original byte signatures identify JPEG. Their names and bytes are preserved without transcoding or renaming. This packaging fact is recorded, not treated as a product defect.',
    },
    'recordedByteComparisons': {
        'basis': 'Only the preserved snapshots and metadata were compared; no live fixture or current binary/source was read.',
        'c05ApprovedBeforeVsFinalGitOutputsEqual': True,
        'c05ApprovedBeforeVsFinalFileHashesAndModesEqual': True,
        'c05ExplicitDeliveryBeforeAuthorizedFinalAcknowledgementBeforeExit': True,
        'c05PermissionRecordRestoredMode': '0755',
        'threeRunsHaveRecordedExitZeroAndUnchangedSourceBinary': True,
    },
    'summary': {
        'originalFiles': len(files), 'rawBytes': sum(item['sourceBytes'] for item in files),
        'storedBytes': sum(item['storedBytes'] for item in files),
        'imageFiles': sum(not item['gzip'] for item in files),
        'pngNamedFiles': sum(not item['gzip'] and item['originalExtension'].lower() == '.png' for item in files),
        'imageMediaTypeCounts': dict(sorted(Counter(item['detectedImageMediaType'] for item in files if not item['gzip']).items())),
        'gzipFiles': sum(item['gzip'] for item in files),
        'emptyOriginalFiles': sum(item['sourceBytes'] == 0 for item in files),
        'countsByGroup': dict(sorted(Counter(item['group'] for item in files).items())),
        'allOriginalBytesRoundTripVerified': True,
        'allSelectedSourcesUnchangedDuringArchive': True,
    },
    'packagingArtifacts': [
        {'stored': name, 'sha256': sha((ROOT / name).read_bytes()),
         'category': 'new-packaging-artifact-not-original-desktop-evidence'}
        for name in ('archive.py', 'verify.py', 'README.md', 'selection-before-verdict.json', 'packaging-image-signature-note.json')
    ],
    'files': files,
}
manifest_bytes = (json.dumps(manifest, ensure_ascii=True, indent=2) + '\n').encode()
with (ROOT / 'manifest.json').open('xb') as handle:
    handle.write(manifest_bytes)
with (ROOT / 'manifest.sha256').open('x', encoding='ascii') as handle:
    handle.write(sha(manifest_bytes) + '  manifest.json\n')
print(json.dumps({'archive': str(ROOT), 'manifestSha256': sha(manifest_bytes), **manifest['summary']}, indent=2))
