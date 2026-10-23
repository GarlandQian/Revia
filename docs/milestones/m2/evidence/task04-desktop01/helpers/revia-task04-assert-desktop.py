#!/usr/bin/env python3
"""Verify retained snapshots from the real desktop run; no UI automation."""

import base64
import json
from pathlib import Path

ROOT = Path('/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-task04-dv8o95m8')
EVIDENCE = ROOT / 'evidence/desktop-r1'


def load(label):
    return json.loads((EVIDENCE / f'{label}.json').read_text())


def files(value, repo):
    return {item['path']: item for item in value['worktrees'][repo]['files']}


def content(value, repo, path):
    return base64.b64decode(files(value, repo)[path]['contentBase64'])


checks = []


def check(label, value):
    assert value, label
    checks.append(label)


initial = load('initial')
final = load('after-restart')
snapshots = {p.stem: json.loads(p.read_text()) for p in EVIDENCE.glob('*.json')}
for label, value in snapshots.items():
    for repo in ('A', 'B', 'L'):
        check(f'{label}/{repo}: index unchanged',
              value['worktrees'][repo]['indexEntriesBase64'] == initial['worktrees'][repo]['indexEntriesBase64'])
    check(f'{label}: unrelated repository B unchanged', value['worktrees']['B'] == initial['worktrees']['B'])

check('cancelled modal makes no changes or records',
      load('cancelled-modal')['worktrees'] == initial['worktrees'] and not load('cancelled-modal')['recovery'])
split = load('split-middle-hunk')
original = content(initial, 'A', '00-hunks.ts')
expected = original.replace(b'value080 = 1080;', b'value080 = 80;')
check('split: only selected middle hunk removed', content(split, 'A', '00-hunks.ts') == expected)
unified = load('unified-first-hunk')
expected = expected.replace(b'value010 = 1010;', b'value010 = 10;')
check('unified: only first hunk removed', content(unified, 'A', '00-hunks.ts') == expected)
check('stale confirmation: no new recovery record', len(load('stale-confirm-external-edit')['recovery']) == 2)
whole = load('whole-shortcut')
baseline = ''.join(f'export const value{i:03} = {i};\n' for i in range(1, 181)).encode()
check('whole shortcut: both remaining hunks removed', content(whole, 'A', '00-hunks.ts') == baseline)
check('storage failure: all worktrees and records unchanged',
      load('before-storage-failure')['worktrees'] == load('storage-failure')['worktrees']
      and load('before-storage-failure')['recovery'] == load('storage-failure')['recovery'])
held = load('hold-on-linked')
check('hold: full record published before actual write', len(held['recovery']) == 4
      and held['recovery'][-1]['outcome'] is None
      and content(held, 'A', '10-partial.ts') == content(initial, 'A', '10-partial.ts'))
partial_expected = content(initial, 'A', '10-partial.ts').replace(b'value080 = 2080;', b'value080 = 80;').replace(b'value140 = 2140;', b'value140 = 140;')
check('partial staging: discarded to index, not HEAD', content(final, 'A', '10-partial.ts') == partial_expected
      and b'value010 = 2010;' in partial_expected)
lost = load('linked-lost-result')
linked_expected = content(initial, 'L', '00-hunks.ts').replace(b'value080 = 1080;', b'value080 = 80;')
check('lost receipt: real selected hunk was applied', content(lost, 'L', '00-hunks.ts') == linked_expected)
check('lost receipt: no false completion outcome', lost['recovery'][-1]['outcome'] is None)
check('lost receipt: actual apply exactly once even after restart',
      (ROOT / 'control-ui-r1/applied-linked-lost-result').read_text().splitlines() == ['0'])
check('hold: explicit release and exactly one apply',
      (ROOT / 'control-ui-r1/release-partial-original-A').is_file()
      and (ROOT / 'control-ui-r1/applied-partial-original-A').read_text().splitlines() == ['0'])
check('no armed diagnostic remains', not (ROOT / 'control-ui-r1/next').exists())
check('restart: all records, outcomes, worktrees byte-identical',
      load('before-restart')['worktrees'] == final['worktrees']
      and load('before-restart')['recovery'] == final['recovery'])

transitions = [
    ('initial', 'split-middle-hunk', 'A', '00-hunks.ts'),
    ('split-middle-hunk', 'unified-first-hunk', 'A', '00-hunks.ts'),
    ('unified-first-hunk', 'stale-confirm-external-edit', 'A', '00-hunks.ts'),
    ('stale-confirm-external-edit', 'whole-shortcut', 'A', '00-hunks.ts'),
    ('linked-lost-result', 'binary-whole', 'A', '20-binary.bin'),
    ('binary-whole', 'mode-whole', 'A', '30-mode.sh'),
    ('mode-whole', 'deleted-whole', 'A', '40-deleted.txt'),
    ('deleted-whole', 'rename-whole', 'A', '50-renamed-new.txt'),
    ('rename-whole', 'unicode-whole', 'A', '60-中文 空格.ts'),
]
for before_label, after_label, target_repo, target_path in transitions:
    before, after = load(before_label), load(after_label)
    for repo in ('A', 'B', 'L'):
        a, b = files(before, repo), files(after, repo)
        changed = {key for key in a.keys() | b.keys() if a.get(key) != b.get(key)}
        check(f'{before_label}->{after_label}/{repo}: no unrelated file change',
              changed == ({target_path} if repo == target_repo else set()))

# The immediate post-release snapshot preceded completion; use the next stable
# snapshot and account for both explicitly initiated writes, not a fabricated one.
for repo in ('A', 'B', 'L'):
    a, b = files(load('storage-failure'), repo), files(lost, repo)
    changed = {key for key in a.keys() | b.keys() if a.get(key) != b.get(key)}
    check(f'hold release and lost receipt/{repo}: only explicit targets changed',
          changed == {'A': {'10-partial.ts'}, 'B': set(), 'L': {'00-hunks.ts'}}[repo])

for record in final['recovery']:
    saved = record['saved']
    repo = Path(saved['identity']['worktreePath']).name
    check(f'{saved["id"]}: filesystem identity recorded', len(saved['identity']['filesystemIds']) == 3)
    check(f'{saved["id"]}: full before matches an observed complete file state', any(
        all((item['content'] is None and item['path'] not in files(snapshot, repo))
            or (item['content'] is not None and item['path'] in files(snapshot, repo)
                and bytes(item['content']) == content(snapshot, repo, item['path'])
                and item['executable'] == bool(int(files(snapshot, repo)[item['path']]['mode'], 8) & 0o111))
            for item in saved['before'])
        for snapshot in snapshots.values()))
    if record['outcome']:
        after = record['outcome']['completed']['after']
        check(f'{saved["id"]}: independent complete after matches observed state', any(
            all((item['content'] is None and item['path'] not in files(snapshot, repo))
                or (item['content'] is not None and item['path'] in files(snapshot, repo)
                    and bytes(item['content']) == content(snapshot, repo, item['path'])
                    and item['executable'] == bool(int(files(snapshot, repo)[item['path']]['mode'], 8) & 0o111))
                for item in after)
            for snapshot in snapshots.values()))

rename = next(r for r in final['recovery'] if r['saved']['target'] == '50-renamed-new.txt')
check('rename: both related paths and original nonexistence saved',
      [f['path'] for f in rename['saved']['before']] == ['50-renamed-new.txt', '50-renamed-old.txt']
      and rename['saved']['before'][1]['content'] is None)
deleted = next(r for r in final['recovery'] if r['saved']['target'] == '40-deleted.txt')
check('deletion: before is nonexistent and after is real content', deleted['saved']['before'][0]['content'] is None
      and bytes(deleted['outcome']['completed']['after'][0]['content']) == b'original deleted file\n')
mode = next(r for r in final['recovery'] if r['saved']['target'] == '30-mode.sh')
check('mode-only: executable before preserved, after nonexecutable', mode['saved']['before'][0]['executable']
      and not mode['outcome']['completed']['after'][0]['executable'])
check('untracked file preserved', files(final, 'A')['90-untracked.txt'] == files(initial, 'A')['90-untracked.txt'])
check('symlink preserved without dereferencing', files(final, 'A')['70-symlink.txt'] == files(initial, 'A')['70-symlink.txt'])
check('nine completed, one pending record', len(final['recovery']) == 10
      and sum(r['outcome'] is not None for r in final['recovery']) == 9)

result = {'ok': True, 'source': final['source'], 'binarySHA256': final['binarySHA256'],
          'checks': len(checks), 'assertions': checks,
          'boundary': 'Snapshot assertions supplement real CUA observations; no screenshot files are claimed.'}
(EVIDENCE / 'assertions.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps({key: result[key] for key in ('ok', 'source', 'checks')}))
