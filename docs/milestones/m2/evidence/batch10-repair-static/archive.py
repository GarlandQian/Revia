from __future__ import annotations

from collections import Counter
from datetime import datetime, timezone
import gzip
import hashlib
import json
from pathlib import Path
import stat


ROOT = Path(__file__).resolve().parent
OLD = Path('/private/tmp/revia-m2-ui-integrated-qnvblzpc')
NEW = Path('/private/tmp/revia-b10-vzc_he0k')
OLD_COMMIT = '5f632d661ce5dc37b237c27f60205c8d41fcf86c'
NEW_COMMIT = '2a51b160d1c3871b9c67fee81ae6f082910bc1f8'
OLD_BINARY = '0623cacdf0c395a00d2eba975df7699c0e72d788a0d80d84f7b94a25c1e3a09d'
NEW_BINARY = 'fd5ffc86131aebbe77850487192005166a63c1a98f9e97aa72b3ddaa1d96988d'
COMMON = [
    'base-sha.txt', 'invocations.json', 'record.py', 'tested-source-list.txt',
    'tested-source-sha256-before.txt', 'tested-source-sha256-after.txt',
]
FORBIDDEN = {'.git', 'target', 'node_modules', 'desktop', 'run02'}


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def load(path):
    return json.loads(path.read_bytes())


def state(path):
    value = path.lstat()
    assert stat.S_ISREG(value.st_mode), ('not a regular file', str(path))
    assert path.resolve(strict=True) == path, ('noncanonical source', str(path))
    assert not any(part in FORBIDDEN or part.endswith('.app') for part in path.parts)
    return (value.st_dev, value.st_ino, value.st_size, value.st_mtime_ns,
            value.st_ctime_ns, value.st_mode)


def category(path):
    name = path.name
    if name.endswith(('.out', '.stdout')):
        return 'raw-command-stdout'
    if name.endswith(('.err', '.stderr')):
        return 'raw-command-stderr'
    if name.endswith('.exit'):
        return 'raw-command-exit-code'
    if name.endswith('.py'):
        return 'original-recorder-script-not-executed'
    if name == 'invocations.json' or name.startswith('build-'):
        return 'original-command-metadata'
    if name == 'binary.json':
        return 'recorded-binary-identity-no-binary-copy'
    if name in ('source-before.json', 'source-after.json') or 'sha256' in name:
        return 'recorded-source-hashes'
    if name == 'tested-source-list.txt':
        return 'recorded-source-file-list'
    return 'recorded-source-identity'


def association(commit, binary, *, base=None, phase='fixed-source', pid=None):
    return {
        'sourceCommit': commit,
        'associatedFixedCommit': commit,
        'recordedBaseCommit': base or commit,
        'sourcePhase': phase,
        'binarySha256': binary,
        'binaryRelation': 'associated-release-not-executed-by-this-archive',
        'pid': pid,
        'pidMeaning': 'historical-desktop-pid' if pid else 'not-recorded-or-not-applicable',
    }


OLD_ASSOC = association(OLD_COMMIT, OLD_BINARY)
NEW_ASSOC = association(NEW_COMMIT, NEW_BINARY)
CHECK_ASSOC = association(NEW_COMMIT, NEW_BINARY, base=OLD_COMMIT,
                          phase='pre-commit-working-content-associated-by-preserved-hash-check')
RESOLVE_ASSOC = dict(CHECK_ASSOC, sourceCommit=None,
                    sourcePhase='preparatory-dependency-resolution-with-lockfile-change')
items = []
commands = []


def add(source, stored, group, assoc, status='pass', limitations=None, kind=None):
    assert stored.endswith('.gz')
    assert not any(part in FORBIDDEN or part.endswith('.app') for part in Path(stored).parts)
    items.append({
        'source': str(source),
        'stored': stored,
        'group': group,
        'contentCategory': kind or category(source),
        'sampleStatus': status,
        'knownLimitations': ['STATIC_ARCHIVE_ONLY'] + (limitations or []),
        'association': assoc,
    })


def add_recorded_block(source, destination, group, assoc, *, status='pass', limitations=None):
    invocations = load(source / 'invocations.json')
    names = list(COMMON)
    for invocation in invocations:
        stem = invocation['stem']
        names.extend(stem + suffix for suffix in ('.out', '.err', '.exit'))
        code = int((source / (stem + '.exit')).read_text().strip())
        assert code == invocation['exit'] == 0
        commands.append({
            'source': str(source / 'invocations.json'),
            'stem': stem,
            'argv': invocation['argv'],
            'exitCode': code,
            'group': group,
            'purpose': 'dependency-preparation-not-product-test' if group == 'repair-resolve' else 'recorded-static-check-or-build',
        })
    for name in names:
        add(source / name, destination + '/' + name + '.gz', group, assoc,
            status, limitations)


assert (OLD / 'base-sha.txt').read_text().strip() == OLD_COMMIT
assert (OLD / 'build/source.txt').read_text().strip() == OLD_COMMIT
assert load(OLD / 'build/binary.json')['sha256'] == OLD_BINARY
assert (NEW / 'build/base-sha.txt').read_text().strip() == NEW_COMMIT
assert (NEW / 'build/source.txt').read_text().strip() == NEW_COMMIT
assert load(NEW / 'build/binary.json')['sha256'] == NEW_BINARY
fixed = load(NEW / 'fixed-source-check.json')
assert fixed == {'sourceCommit': NEW_COMMIT, 'fileCount': 154, 'mismatches': []}

add_recorded_block(OLD, 'integration-5f', 'integration-checks', OLD_ASSOC,
                   limitations=['OLD_CHECKS_ARE_NOT_NEW_VERSION_RERUN'])
add(OLD / 'build.py', 'integration-5f/build.py.gz', 'integration-build', OLD_ASSOC,
    limitations=['OLD_BUILD_ENVIRONMENT_NOT_FULLY_RECORDED'])
old_build_names = ['source.txt', 'source-before.json', 'source-after.json', 'binary.json']
for number in (0, 1):
    name = 'build-' + str(number)
    metadata = load(OLD / 'build' / (name + '.json'))
    assert metadata['exitCode'] == 0
    commands.append({'source': str(OLD / 'build' / (name + '.json')),
                     'argv': metadata['command'], 'exitCode': 0,
                     'group': 'integration-build', 'purpose': 'recorded-build-preparation-or-build'})
    old_build_names.extend(name + suffix for suffix in ('.json', '.stdout', '.stderr'))
for name in old_build_names:
    add(OLD / 'build' / name, 'integration-5f/build/' + name + '.gz',
        'integration-build', OLD_ASSOC, limitations=['OLD_BUILD_ENVIRONMENT_NOT_FULLY_RECORDED'])

add_recorded_block(NEW / 'checks-cpi33j4q/resolve', 'repair-2a51/checks/resolve',
                   'repair-resolve', RESOLVE_ASSOC, status='partial',
                   limitations=['RESOLVE_IS_PREPARATION', 'CHECK_BASE_PRECEDES_FIXED_COMMIT'])
add_recorded_block(NEW / 'checks-cpi33j4q/rust', 'repair-2a51/checks/rust',
                   'repair-rust-checks', CHECK_ASSOC,
                   limitations=['CHECK_BASE_PRECEDES_FIXED_COMMIT', 'RUST_CHECKS_DO_NOT_REPLACE_DESKTOP'])
add_recorded_block(NEW / 'build', 'repair-2a51/build', 'repair-build', NEW_ASSOC)
for name in ('source.txt', 'source-before.json', 'source-after.json', 'binary.json'):
    add(NEW / 'build' / name, 'repair-2a51/build/' + name + '.gz',
        'repair-build', NEW_ASSOC)

context = {
    'checks-path.txt': ('recorded-check-directory', 'partial', []),
    'dependency-behavior.json': ('historical-local-dependency-analysis', 'partial', ['ANALYSIS_IS_NOT_RAW_EXECUTION']),
    'fixed-source-check.json': ('recorded-fixed-source-association', 'pass', ['RECORDED_ASSOCIATION_NOT_NEW_REPO_CHECK']),
    'standards-review.md': ('independent-static-standards-review', 'pass', ['REVIEWS_ARE_STATIC_ONLY']),
    'spec-review.md': ('independent-static-spec-review', 'pass', ['REVIEWS_ARE_STATIC_ONLY']),
    'archive-audit.md': ('historical-archive-boundary-analysis', 'partial', ['HISTORICAL_ANALYSIS_NOT_CURRENT_DESKTOP_STATE']),
}
for name, (kind, status, limitations) in context.items():
    add(NEW / name, 'repair-2a51/context/' + name + '.gz', 'repair-context',
        NEW_ASSOC, status, limitations, kind)
add(NEW / 'failure.json', 'integration-5f/context/failure.json.gz',
    'historical-5f-failure-context', association(OLD_COMMIT, OLD_BINARY, pid=23756),
    'failed', ['ANALYSIS_IS_NOT_RAW_EXECUTION', 'OLD_C02_FAILURE_REMAINS_FAILURE'],
    'historical-post-hoc-desktop-failure-record')

assert len(items) == 103
assert len({entry['source'] for entry in items}) == len(items)
assert len({entry['stored'] for entry in items}) == len(items)
assert set(str(p) for p in OLD.rglob('*') if p.is_file()) == {
    entry['source'] for entry in items if entry['source'].startswith(str(OLD) + '/')}
for path in (NEW / 'checks-cpi33j4q', NEW / 'build'):
    assert set(str(p) for p in path.rglob('*') if p.is_file()) == {
        entry['source'] for entry in items if entry['source'].startswith(str(path) + '/')}

source_stats = {}
source_bytes = {}
for entry in items:
    source = Path(entry['source'])
    source_stats[str(source)] = state(source)
    source_bytes[str(source)] = source.read_bytes()
    assert state(source) == source_stats[str(source)]


def raw(path):
    return source_bytes[str(path)]


def source_hashes(path):
    return dict(line.decode().split('  ', 1)[::-1] for line in raw(path).splitlines())


comparisons = []
for source, label in ((OLD, 'integration-checks'),
                      (NEW / 'checks-cpi33j4q/resolve', 'repair-resolve'),
                      (NEW / 'checks-cpi33j4q/rust', 'repair-rust-checks'),
                      (NEW / 'build', 'repair-build')):
    before = source_hashes(source / 'tested-source-sha256-before.txt')
    after = source_hashes(source / 'tested-source-sha256-after.txt')
    changes = [{'path': name, 'before': before.get(name), 'after': after.get(name)}
               for name in sorted(before.keys() | after.keys()) if before.get(name) != after.get(name)]
    comparisons.append({'group': label, 'scope': 'recorded-check-source-list',
                        'beforeCount': len(before), 'afterCount': len(after),
                        'equal': before == after, 'changes': changes})
    if label == 'repair-resolve':
        assert [change['path'] for change in changes] == ['src-tauri/Cargo.lock']
    else:
        assert before == after
for source, label in ((OLD / 'build', 'integration-build'), (NEW / 'build', 'repair-build')):
    before = json.loads(raw(source / 'source-before.json'))
    after = json.loads(raw(source / 'source-after.json'))
    assert before == after
    comparisons.append({'group': label, 'scope': 'recorded-tracked-file-hash-dictionary',
                        'beforeCount': len(before), 'afterCount': len(after),
                        'equal': True, 'changes': []})
assert raw(NEW / 'checks-cpi33j4q/resolve/tested-source-sha256-after.txt') == raw(NEW / 'checks-cpi33j4q/rust/tested-source-sha256-before.txt')
assert raw(NEW / 'checks-cpi33j4q/rust/tested-source-sha256-after.txt') == raw(NEW / 'build/tested-source-sha256-before.txt')

for entry in sorted(items, key=lambda value: value['stored']):
    original = source_bytes[entry['source']]
    encoded = gzip.compress(original, compresslevel=9, mtime=0)
    destination = ROOT / entry['stored']
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open('xb') as handle:
        handle.write(encoded)
    assert gzip.decompress(destination.read_bytes()) == original
    source = Path(entry['source'])
    entry.update({
        'gzip': True,
        'sourceBytes': len(original),
        'storedBytes': len(encoded),
        'sourceSha256': sha(original),
        'storedSha256': sha(encoded),
        'sourceMtimeNs': source_stats[str(source)][3],
        'originalBytesVerified': True,
    })

for source, original in source_bytes.items():
    assert state(Path(source)) == source_stats[source], ('source state changed', source)
    assert Path(source).read_bytes() == original, ('source bytes changed', source)

manifest = {
    'schemaVersion': 1,
    'createdAtUtc': datetime.now(timezone.utc).isoformat(),
    'archiveKind': 'static-only-evidence-package',
    'scope': 'Original static check/build records for 5f and 2a51, independent B10 static reviews, and historical context only. No product checks or desktop actions were run during archival.',
    'statusMeaning': 'Per-file pass refers only to its recorded static command or static-review scope. Partial denotes preparatory or contextual material. No milestone or desktop completion is asserted.',
    'excluded': [
        'All desktop directories and every run02 artifact, including completed-looking intermediate snapshots',
        'All fixtures, live application data, process logs, and UI captures',
        'All app bundles, executable binaries, target, node_modules, .git, and full repositories',
        'Current source-tree state checks, rebuilds, tests, GUI actions, commits, and milestone status changes',
    ],
    'knownLimitations': {
        'STATIC_ARCHIVE_ONLY': 'This package is not a desktop, CI, D06, or Q98 acceptance result.',
        'OLD_CHECKS_ARE_NOT_NEW_VERSION_RERUN': 'The old 5f check records remain attributed to 5f; they are not represented as rerun on 2a51.',
        'OLD_BUILD_ENVIRONMENT_NOT_FULLY_RECORDED': 'Old build command JSON did not record exact environment values. The preserved build.py inherits HOME/PATH, prepends /opt/homebrew/bin, and sets CARGO_NET_OFFLINE; missing values were not reconstructed.',
        'RESOLVE_IS_PREPARATION': 'Offline cargo metadata was dependency preparation, not an extra product test. Its before/after lists differ only at Cargo.lock; the change is preserved.',
        'CHECK_BASE_PRECEDES_FIXED_COMMIT': 'resolve/rust base-sha is 5f because the B10 changes had not yet been committed. Their actual recorded source lists and fixed-source-check.json are preserved without rewriting base-sha. The post-resolution and Rust/build lists match.',
        'RUST_CHECKS_DO_NOT_REPLACE_DESKTOP': 'The recorded 271 passed / 0 failed / 1 ignored Rust result is an implementation check, not native AppKit or close/exit acceptance.',
        'ANALYSIS_IS_NOT_RAW_EXECUTION': 'This is preserved analysis or a post-hoc report, not a raw command/CUA response transcript.',
        'RECORDED_ASSOCIATION_NOT_NEW_REPO_CHECK': 'fixed-source-check.json is an existing record. Archival does not rerun its source-to-commit comparison.',
        'REVIEWS_ARE_STATIC_ONLY': 'The independent reports found no definite static finding within their stated range; they do not establish runtime or milestone completion.',
        'HISTORICAL_ANALYSIS_NOT_CURRENT_DESKTOP_STATE': 'archive-audit.md is preserved as written. Its C05/run02 and other desktop statements describe its historical observation boundary and must not be read as current desktop status; subsequent run02 work is excluded and no current verdict is generated.',
        'OLD_C02_FAILURE_REMAINS_FAILURE': 'The old 5f C02 captured a successful Git result, then exited before explicit result release. It remains a failure; this does not mean Git was killed or data was lost. failure.json is post-hoc context and the underlying desktop originals are outside this static package.',
    },
    'versions': {
        'integration-5f': {'commit': OLD_COMMIT, 'binarySha256': OLD_BINARY},
        'repair-2a51': {'commit': NEW_COMMIT, 'binarySha256': NEW_BINARY},
    },
    'comparisonBasis': 'Only preserved evidence files were compared. No current repository, binary, fixture, or live log was hashed by this script.',
    'sourceManifestComparisons': comparisons,
    'recordedCommands': commands,
    'summary': {
        'originalFiles': len(items),
        'rawBytes': sum(entry['sourceBytes'] for entry in items),
        'gzipBytes': sum(entry['storedBytes'] for entry in items),
        'emptyOriginalFiles': sum(entry['sourceBytes'] == 0 for entry in items),
        'countsByGroup': dict(sorted(Counter(entry['group'] for entry in items).items())),
        'allOriginalBytesRoundTripVerified': True,
        'allSelectedSourcesUnchangedDuringArchive': True,
    },
    'packagingArtifacts': [
        {'stored': name, 'sha256': sha((ROOT / name).read_bytes()),
         'category': 'new-archive-tool-or-scope-note-not-historical-evidence'}
        for name in ('archive.py', 'verify.py', 'README.md')
    ],
    'files': sorted(items, key=lambda value: value['stored']),
}
manifest_bytes = (json.dumps(manifest, ensure_ascii=True, indent=2) + '\n').encode()
with (ROOT / 'manifest.json').open('xb') as handle:
    handle.write(manifest_bytes)
with (ROOT / 'manifest.sha256').open('x', encoding='ascii') as handle:
    handle.write(sha(manifest_bytes) + '  manifest.json\n')
print(json.dumps({'archive': str(ROOT), 'manifestSha256': sha(manifest_bytes),
                  **manifest['summary']}, indent=2))
