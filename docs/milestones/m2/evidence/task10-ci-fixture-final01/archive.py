from collections import Counter
from datetime import datetime, timezone
import gzip
import hashlib
import io
import json
from pathlib import Path
import re
import stat


ROOT = Path(__file__).resolve().parent
SELECTION = Path('/private/tmp/revia-ci-fixture-archive.zP817m/selection-before-ci-terminal.json')
REPAIR = Path('/private/tmp/revia-ci-submod-lvvaQ9')
FINAL = REPAIR / 'run-final-ZUIsp1'
DOC = Path('/private/tmp/revia-ci-docfix-sk2QnK')
CI = Path('/private/tmp/revia-m2-ci-final.cXI0qX')
BASE = '2a51b160d1c3871b9c67fee81ae6f082910bc1f8'
FIX = 'd952a857b3faef2a4101ab5a6582217bd5a7c8e0'
HEAD = 'f10fc971c14e556c239cb4648cabfbbfc28ad4b4'
TARGET = 'deinitialized_submodule_is_not_confirmed_and_cannot_be_claimed'
FORBIDDEN = {'.git', '__pycache__', 'cargo', 'fixture', 'fixtures', 'home',
             'isolated-home', 'node_modules', 'target'}
CI_NAMES = [
    'push-command.txt', 'push-cwd.txt', 'push-start.txt', 'push-end.txt',
    'push.exit', 'push.stdout', 'push.stderr', 'source-commit.txt', 'status-before.txt',
    'ci-runs.json', 'ci-runs.stderr', 'ci-runs.exit',
    'ci-runs-escalated.json', 'ci-runs-escalated.stderr', 'ci-runs-escalated.exit',
    'ci-run-40174661706-check01.json', 'ci-check01.stderr', 'ci-check01.exit',
    'ci-run-40174661706.log', 'ci-log.stderr', 'ci-log.exit', 'ci-counts-derived.json',
]


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def state(path):
    s = path.lstat()
    assert stat.S_ISREG(s.st_mode), ('not regular', str(path))
    assert path.resolve(strict=True) == path, ('noncanonical', str(path))
    assert not any(p in FORBIDDEN or p.endswith('.app') for p in path.parts)
    return (s.st_dev, s.st_ino, s.st_size, s.st_mtime_ns, s.st_ctime_ns, s.st_mode)


def load(path):
    return json.loads(path.read_bytes())


def hashes(path):
    return {line.split(None, 1)[1]: line.split(None, 1)[0]
            for line in path.read_text().splitlines() if line.strip()}


def category(path):
    if path.name == 'ci-counts-derived.json':
        return 'original-derived-summary-not-raw-execution'
    if path.suffix in {'.out', '.stdout', '.log'}:
        return 'original-command-output'
    if path.suffix in {'.err', '.stderr'}:
        return 'original-command-stderr'
    if path.suffix == '.exit':
        return 'original-exit-record-preserving-format'
    if path.suffix == '.py':
        return 'original-recorder-preserved-not-executed'
    if 'sha256' in path.name:
        return 'original-source-hash-list'
    if path.name == 'review_marks.rs.fixed':
        return 'original-fixed-test-source-snapshot'
    if path.suffix == '.md':
        return 'original-analysis-dispatch-or-identity-record'
    return 'original-metadata-or-configuration'


def interpretation(group, name):
    limits = ['STATIC_ARCHIVE_ONLY']
    association = {'relatedTestFixCommit': FIX, 'relatedDocumentationCommit': HEAD,
                   'recordedBaseCommit': BASE, 'sourceCommit': None,
                   'sourcePhase': 'historical-context', 'pid': None,
                   'pidMeaning': 'not-recorded-or-not-applicable'}
    status = 'context'
    if group in {'repair-records', 'final-local-run'}:
        association['sourcePhase'] = 'precommit-working-test-content-matches-fix-recorded-hash'
        status = 'recorded-check-pass'
        limits += ['RECORDER_TEXT_AND_PARTIAL_ENV', 'HASH_LIST_SCOPE_DIFFERS', 'NOT_MISSING_NOTIFY_FAILURE']
        if group == 'final-local-run':
            association['sourceCommit'] = FIX
            limits += ['NOT_SAME_CONDITION_POSITIVE_CONTROL']
    elif group == 'sensitivity':
        association['sourcePhase'] = 'control-variant-or-fixed-snapshot-not-exact-committed-tree'
        limits += ['NO_RECREATED_MISSING_LOGS', 'SENSITIVITY_PROVENANCE_PARTIAL']
        if name.startswith('pre-fix-useconfigonly.'):
            status = 'preserved-pre-fix-identity-failure'
        elif name.startswith('pre-fix.'):
            status = 'invalid-control-despite-exit-zero'
            limits += ['OUTER_CONFIG_CONTROL_INVALID']
        elif name == 'review_marks.rs.fixed':
            association['sourceCommit'] = FIX
            association['sourcePhase'] = 'fixed-test-file-snapshot-matches-recorded-test-hash'
    elif group == 'documentation-correction':
        association.update(sourceCommit=HEAD, recordedBaseCommit=FIX,
                           sourcePhase='documentation-only-correction')
        status = 'documentation-only-diff-check-pass'
        limits += ['DOC_ONLY_NO_PRODUCT_RERUN', 'DOCUMENTATION_CHECK_METADATA_PARTIAL']
    elif group == 'independent-review':
        association.update(sourceCommit=FIX, sourcePhase='review-of-2a51-to-d952-before-doc-correction')
        status = 'original-findings-preserved'
        limits += ['REVIEWS_PRECEDE_CORRECTION', 'HISTORICAL_PENDING_CI_NOT_CURRENT_STATUS']
    elif group == 'correction-dispatch':
        association.update(recordedBaseCommit=FIX, sourcePhase='instruction-not-execution-result')
        limits += ['DISPATCH_NOT_EXECUTION']
    elif group == 'terminal-ci':
        association.update(sourceCommit=HEAD, sourcePhase='push-and-ci-on-fixed-head',
                           ciRunId=40174661706)
        limits += ['CI_NOT_DESKTOP_OR_PACKAGING', 'CI_QUERY_METADATA_PARTIAL']
        status = 'recorded-terminal-ci-pass'
        if name.startswith('ci-runs.'):
            status = 'preserved-api-connection-failure'
        elif name.startswith('ci-runs-escalated.'):
            status = 'historical-in-progress-query-success'
            limits += ['NONTERMINAL_OBSERVATION_PRESERVED']
        elif name == 'ci-counts-derived.json':
            status = 'derived-counts-cross-checked-against-original-log'
        elif name.startswith('push') or name in {'source-commit.txt', 'status-before.txt'}:
            status = 'recorded-normal-push-context'
    return association, status, limits


selection_raw = SELECTION.read_bytes()
selection = json.loads(selection_raw)
assert len(selection['entries']) == 47
assert set(CI_NAMES) == {p.name for p in CI.iterdir()}
selected = [(Path(x['source']), x['group'], x['relative'], x) for x in selection['entries']]
selected += [(CI / name, 'terminal-ci', name, None) for name in CI_NAMES]
source_states = {}
source_bytes = {}
for path, group, relative, prior in selected:
    assert str(path) not in source_bytes
    source_states[str(path)] = state(path)
    raw = path.read_bytes()
    assert state(path) == source_states[str(path)]
    if prior:
        assert len(raw) == prior['sourceBytes'] and sha(raw) == prior['sourceSha256']
    source_bytes[str(path)] = raw

assert (REPAIR / 'base-sha.txt').read_text().strip() == BASE
assert (FINAL / 'base-sha.txt').read_text().strip() == BASE
assert (FINAL / 'MANIFEST.md').read_text().strip() == 'commit=' + FIX
assert (DOC / 'base-sha.txt').read_text().strip() == FIX
assert (DOC / 'MANIFEST.md').read_text().strip() == 'commit=' + HEAD
assert (CI / 'source-commit.txt').read_text().strip() == HEAD
assert (REPAIR / 'sensitivity/pre-fix.exit').read_bytes() == b'0\n'
assert (REPAIR / 'sensitivity/pre-fix-useconfigonly.exit').read_bytes() == b'pre-fix exit=101\n'
assert (DOC / 'git-diff-check.exit').read_bytes() == b'diff-check exit=0\n'
commands = []
for folder, group in [(REPAIR, 'repair-records'), (FINAL, 'final-local-run')]:
    for invocation in load(folder / 'invocations.json'):
        stem = invocation['stem']
        assert int((folder / (stem + '.exit')).read_text().strip()) == invocation['exit'] == 0
        assert b'test result: ok. 31 passed; 0 failed;' in (folder / (stem + '.out')).read_bytes()
        commands.append({'originalMetadata': str(folder / 'invocations.json'),
                         'group': group, 'record': invocation})
test_path = 'src-tauri/tests/review_marks.rs'
local_lists = {name: hashes(FINAL / name) for name in [
    'source-sha256-before.txt', 'source-sha256-after.txt', 'source-sha256-committed.txt']}
test_hash = local_lists['source-sha256-before.txt'][test_path]
assert all(value[test_path] == test_hash for value in local_lists.values())
assert hashes(REPAIR / 'source-sha256-before.txt')[test_path] == test_hash
assert sha((REPAIR / 'sensitivity/review_marks.rs.fixed').read_bytes()) == test_hash
doc_names = ['source-sha256-before.txt', 'source-sha256-after.txt', 'source-sha256-committed.txt']
assert len({(DOC / name).read_bytes() for name in doc_names}) == 1

ci = load(CI / 'ci-run-40174661706-check01.json')
assert ci['headSha'] == HEAD and ci['databaseId'] == 40174661706
assert ci['status'] == 'completed' and ci['conclusion'] == 'success'
derived = load(CI / 'ci-counts-derived.json')['counts']
log_lines = (CI / 'ci-run-40174661706.log').read_text().splitlines()
result_re = re.compile(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;')
ci_jobs = []
for job in ci['jobs']:
    name = job['name']
    assert job['status'] == 'completed' and job['conclusion'] == 'success'
    suites, references, targets, release_finished, release_command = [], [], [], [], []
    for line_number, line in enumerate(log_lines, 1):
        fields = line.split('\t', 2)
        if len(fields) != 3 or fields[0] != name:
            continue
        match = result_re.search(line)
        if fields[1] == 'Run Rust tests' and match:
            suites.append([int(x) for x in match.groups()])
            references.append(line_number)
        if fields[1] == 'Run Rust tests' and 'test ' + TARGET + ' ... ok' in line:
            targets.append(line_number)
        if fields[1] == 'Build release binary':
            if 'Finished' in line and '`release` profile' in line:
                release_finished.append(line_number)
            if 'cargo build --release --manifest-path src-tauri/Cargo.toml' in line:
                release_command.append(line_number)
    assert suites == derived[name]['suites']
    totals = {key: sum(s[index] for s in suites) for index, key in enumerate(['passed', 'failed', 'ignored'])}
    assert all(totals[key] == derived[name][key] for key in totals)
    expected_passed = 243 if name.startswith('Windows') else 271
    assert totals == {'passed': expected_passed, 'failed': 0, 'ignored': 1}
    assert len(targets) == 1 and release_finished and release_command
    required = ['Check frontend types', 'Lint frontend', 'Build frontend', 'Run Rust tests', 'Build release binary']
    assert all(any(s['name'] == n and s['conclusion'] == 'success' for s in job['steps']) for n in required)
    ci_jobs.append({'name': name, 'jobId': job['databaseId'], 'url': job['url'],
                    'status': job['status'], 'conclusion': job['conclusion'],
                    'testCounts': totals, 'suiteResults': suites,
                    'reviewMarksPassed': 29 if name.startswith('Windows') else 31,
                    'testResultLogLineNumbers': references, 'targetTestLogLineNumbers': targets,
                    'releaseCommandLogLineNumbers': release_command,
                    'releaseFinishedLogLineNumbers': release_finished,
                    'requiredStepsSucceeded': required})
assert len(ci_jobs) == 2
for name, code in [('push.exit', 0), ('ci-runs.exit', 1), ('ci-runs-escalated.exit', 0),
                   ('ci-check01.exit', 0), ('ci-log.exit', 0)]:
    assert int((CI / name).read_text().strip()) == code

items = []
for source, group, relative, prior in selected:
    original = source_bytes[str(source)]
    stored = 'payload/' + group + '/' + relative + '.gz'
    destination = ROOT / stored
    assert not destination.exists()
    destination.parent.mkdir(parents=True, exist_ok=True)
    output = io.BytesIO()
    with gzip.GzipFile(filename='', mode='wb', fileobj=output, mtime=0, compresslevel=9) as handle:
        handle.write(original)
    encoded = output.getvalue()
    with destination.open('xb') as handle:
        handle.write(encoded)
    assert gzip.decompress(destination.read_bytes()) == original
    association, status, limitations = interpretation(group, source.name)
    items.append({'source': str(source), 'stored': stored, 'group': group,
                  'contentCategory': category(source), 'evidenceStatus': status,
                  'association': association, 'knownLimitations': limitations,
                  'sourceBytes': len(original), 'sourceSha256': sha(original),
                  'storedBytes': len(encoded), 'storedSha256': sha(encoded),
                  'sourceMtimeNs': source_states[str(source)][3],
                  'sourceMode': oct(stat.S_IMODE(source_states[str(source)][5])),
                  'originalBytesVerified': True})
for source, original in source_bytes.items():
    assert state(Path(source)) == source_states[source]
    assert Path(source).read_bytes() == original
assert set(CI_NAMES) == {p.name for p in CI.iterdir()}
assert SELECTION.read_bytes() == selection_raw

manifest = {
    'schemaVersion': 1, 'createdAtUtc': datetime.now(timezone.utc).isoformat(),
    'archiveKind': 'completed-ci-fixture-repair-and-documentation-evidence',
    'scope': 'Preserve existing files only. No repository, fixture, product, GUI, benchmark or CI command was executed or modified during archival.',
    'statusMeaning': 'Statuses describe each original historical record, not milestone acceptance. Invalid controls, expected identity failures and missing evidence remain explicit.',
    'versions': {'baseAndReviewStart': BASE, 'testFixtureFix': FIX, 'documentationCorrectionAndCiHead': HEAD},
    'excluded': ['Full fixtures and repositories', 'home and isolated-home', 'cargo and target',
                 'node_modules, .git and __pycache__', '.app bundles and binaries',
                 'Application data', 'All Q98 and other active logs or desktop evidence'],
    'knownLimitations': {
        'STATIC_ARCHIVE_ONLY': 'Only existing evidence bytes were packaged; no acceptance action was performed.',
        'RECORDER_TEXT_AND_PARTIAL_ENV': 'Original recorder uses text=True and omits PATH from recorded environment; missing binary capture or environment details were not reconstructed.',
        'HASH_LIST_SCOPE_DIFFERS': 'Final before/after lists cover the test source only; committed list additionally covers task documentation. Equality is asserted only for their shared test-file entry.',
        'NOT_MISSING_NOTIFY_FAILURE': 'Existing root and final test records are success/exit0. They are not the reportedly earlier unisolated-HOME notify offline failure101 whose raw output is missing.',
        'NOT_SAME_CONDITION_POSITIVE_CONTROL': 'Final ordinary 31/0 run uses empty fixture global configuration, not the missing fixed-code useConfigOnly positive control. Later CI cannot retroactively supply that control.',
        'NO_RECREATED_MISSING_LOGS': 'Missing fixed-code same-condition positive control and earlier notify failure logs were not recreated. Their absence does not prove those runs never happened; disclosed deletion history is not erased.',
        'SENSITIVITY_PROVENANCE_PARTIAL': 'Available sensitivity output, exit text, fixed source and config are preserved. Independent complete argv/environment/timing records do not exist in this selected material.',
        'OUTER_CONFIG_CONTROL_INVALID': 'The old-code pass under outer configuration is an invalid sensitivity control because fixture env_clear replaced that configuration.',
        'DOC_ONLY_NO_PRODUCT_RERUN': 'f10 is a documentation-only correction; its local validation is diff-check, not another product test.',
        'DOCUMENTATION_CHECK_METADATA_PARTIAL': 'Doc correction retains textual exit=0 and hashes but lacks separately recorded exact argv, environment and timing.',
        'REVIEWS_PRECEDE_CORRECTION': 'Original Standards and Spec findings cover 2a51...d952 before f10. They are not rewritten as a new zero-finding post-correction review.',
        'HISTORICAL_PENDING_CI_NOT_CURRENT_STATUS': 'Historical Windows-pending statements keep their original time boundary; later terminal CI evidence is separately associated.',
        'DISPATCH_NOT_EXECUTION': 'Instructions are preserved as instructions, not as proof of execution.',
        'CI_NOT_DESKTOP_OR_PACKAGING': 'CI success and release binary compilation do not establish installer publication, Windows desktop, D06, Q98 or whole-M2 acceptance.',
        'CI_QUERY_METADATA_PARTIAL': 'CI query/download stdout, stderr and exit are retained but their exact argv, full environment and start/end are not independently recorded. Push has separate command/cwd/time records.',
        'NONTERMINAL_OBSERVATION_PRESERVED': 'Earlier successful query still reports the new run in_progress and old failures. It is retained unchanged rather than replaced by terminal status.',
    },
    'selectionBasis': {'source': str(SELECTION), 'sha256': sha(selection_raw),
                       'initialStaticOriginals': 47, 'terminalCiOriginals': len(CI_NAMES),
                       'initialSelectedSha256Rechecked': True},
    'recordedLocalCommands': commands,
    'recordedSourceComparisons': {
        'testFile': test_path, 'fixedTestSha256': test_hash,
        'finalLocalLists': local_lists,
        'rootBeforeTestMatches': True, 'fixedSnapshotMatches': True,
        'documentationCorrectionListsEqualWithinCorrection': True,
        'documentationCorrectionLists': {n: hashes(DOC / n) for n in doc_names},
        'comparisonScope': 'Recorded evidence only; no current source tree or Git comparison was run.',
    },
    'recordedCi': {'runId': ci['databaseId'], 'headSha': ci['headSha'], 'url': ci['url'],
                   'status': ci['status'], 'conclusion': ci['conclusion'], 'jobs': ci_jobs,
                   'logCountsMatchOriginalDerivedFile': True,
                   'targetTest': TARGET, 'logDownloadExit': 0, 'initialApiQueryExit': 1,
                   'normalPushExit': 0},
    'summary': {'originalFiles': len(items), 'rawBytes': sum(x['sourceBytes'] for x in items),
                'gzipBytes': sum(x['storedBytes'] for x in items),
                'emptyOriginalFiles': sum(x['sourceBytes'] == 0 for x in items),
                'countsByGroup': dict(sorted(Counter(x['group'] for x in items).items())),
                'allOriginalBytesRoundTripVerified': True,
                'allSelectedSourcesUnchangedDuringArchive': True},
    'packagingArtifacts': [{'stored': name, 'sha256': sha((ROOT / name).read_bytes()),
                           'category': 'new-archive-tool-or-scope-note-not-historical-evidence'}
                          for name in ['archive.py', 'verify.py', 'README.md']],
    'files': sorted(items, key=lambda x: x['stored']),
}
manifest_raw = (json.dumps(manifest, ensure_ascii=True, indent=2) + '\n').encode()
with (ROOT / 'manifest.json').open('xb') as handle:
    handle.write(manifest_raw)
with (ROOT / 'manifest.sha256').open('x', encoding='ascii') as handle:
    handle.write(sha(manifest_raw) + '  manifest.json\n')
print(json.dumps({'archive': str(ROOT), 'manifestSha256': sha(manifest_raw),
                  **manifest['summary']}, indent=2))
