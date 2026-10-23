#!/usr/bin/env python3
"""Verify pinned desktop snapshots and independent visible D06 runs."""

import base64
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path('/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-task04-dv8o95m8')
EVIDENCE = ROOT / 'evidence/desktop-r2'
SOURCE = '764d13e0629c3e43c32b17015bdf20916a339233'
BINARY_SHA = '246618d92c041ab723741e8140cdadb957870d1cea961e5aa0d532cb88176f71'
checks = []


def check(label, value):
    assert value, label
    checks.append(label)


def load(label):
    result = json.loads((EVIDENCE / f'{label}.json').read_text())
    check(f'{label}: source', result['source'] == SOURCE)
    check(f'{label}: binary', result['binarySHA256'] == BINARY_SHA)
    return result


def files(snapshot, name):
    return {item['path']: item for item in snapshot['worktrees'][name]['files']}


def index(snapshot, name):
    result = {}
    for entry in base64.b64decode(snapshot['worktrees'][name]['indexEntriesBase64']).split(b'\0'):
        if entry:
            header, path = entry.split(b'\t', 1)
            result[path.decode()] = header.decode()
    return result


initial = load('initial')
discard = load('rename-discard-finished')
before_stage = load('rename-stage-before')
stage = load('rename-stage-finished')
unstage = load('rename-unstage-finished')
target = '50-renamed-new.txt'
old_path = '50-renamed-old.txt'
original = b'tracked rename baseline\nunstaged addition\n'
baseline = b'tracked rename baseline\n'

for name in ('A', 'B', 'L'):
    check(f'discard: {name} index unchanged', index(initial, name) == index(discard, name))
for snapshot_name, snapshot in [('discard', discard), ('before-stage', before_stage), ('stage', stage), ('unstage', unstage)]:
    for name in ('A', 'L'):
        check(f'{snapshot_name}: {name} unchanged', initial['worktrees'][name] == snapshot['worktrees'][name])
    check(f'{snapshot_name}: unrelated B files unchanged',
          {k: v for k, v in files(initial, 'B').items() if k != target} ==
          {k: v for k, v in files(snapshot, 'B').items() if k != target})
check('discard: target baseline', base64.b64decode(files(discard, 'B')[target]['contentBase64']) == baseline)
check('discard: exactly one record', len(discard['recovery']) == 1)
record = discard['recovery'][0]
saved = record['saved']
check('discard: source is whole-file', saved['source'] == 'discardFile')
check('discard: B identity', saved['identity']['worktreePath'] == str(ROOT / 'B'))
check('discard: filesystem identity present', len(saved['identity']['filesystemIds']) == 3)
check('discard: related paths', {item['path'] for item in saved['before']} == {target, old_path})
before = {item['path']: item for item in saved['before']}
after = {item['path']: item for item in record['outcome']['completed']['after']}
check('discard: complete bytes saved', bytes(before[target]['content']) == original)
check('discard: old path absence saved', before[old_path]['content'] is None)
check('discard: target executable bit saved', before[target]['executable'] is False)
check('discard: completed baseline bytes', bytes(after[target]['content']) == baseline)
check('discard: completed old path absent', after[old_path]['content'] is None)
check('mutation: only target restored', files(before_stage, 'B') == files(initial, 'B'))
check('mutation: index unchanged', index(before_stage, 'B') == index(initial, 'B'))
check('stage: worktree unchanged', files(stage, 'B') == files(before_stage, 'B'))
check('stage: only target index entry changes',
      {k: v for k, v in index(before_stage, 'B').items() if k != target} ==
      {k: v for k, v in index(stage, 'B').items() if k != target})
expected_blob = subprocess.check_output(['/opt/homebrew/bin/git', 'hash-object', '--stdin'], input=original).decode().strip()
check('stage: expected complete blob', index(stage, 'B')[target] == f'100644 {expected_blob} 0')
check('stage: old path remains absent in index', old_path not in index(stage, 'B'))
check('unstage: worktree paths and content unchanged', files(unstage, 'B') == files(stage, 'B'))
check('unstage: new path absent from index', target not in index(unstage, 'B'))
check('unstage: old path restored to index', old_path in index(unstage, 'B'))
check('unstage: unrelated index entries unchanged',
      {k: v for k, v in index(stage, 'B').items() if k not in (target, old_path)} ==
      {k: v for k, v in index(unstage, 'B').items() if k not in (target, old_path)})
status = base64.b64decode(unstage['worktrees']['B']['statusBase64'])
check('unstage: old path is deleted, new untracked', b'1 .D ' in status and b'50-renamed-old.txt\0' in status and b'? 50-renamed-new.txt\0' in status)
check('stage and unstage: saved record unchanged', stage['recovery'] == discard['recovery'] == unstage['recovery'])

performance = []
for kind in ('normal', 'cold'):
    for view in ('split', 'unified'):
        run = ROOT / f'perf-runs/{kind}-{view}-1'
        launch = json.loads((run / 'launch.json').read_text())
        result = json.loads((run / 'result.json').read_text())
        label = f'{kind}-{view}'
        check(f'{label}: pinned source and binary', launch['source'] == SOURCE and launch['binarySHA256'] == BINARY_SHA)
        check(f'{label}: ok', result['ok'] is True)
        visibility = result['visibility']
        check(f'{label}: visible focused throughout', visibility['focused'] and not visibility['hidden'] and visibility['focusLosses'] == 0 and visibility['hiddenTransitions'] == 0)
        check(f'{label}: single expected view', len(result['combos']) == 1 and result['combos'][0]['view'] == view)
        combo = result['combos'][0]
        scroll = combo['scroll']
        check(f'{label}: 60 screens with rAF', scroll['screens'] == scroll['requestedScreens'] == scroll['frames'] == scroll['rafFrames'] == 60)
        check(f'{label}: no fallback or uncovered rows', scroll['fallbackFrames'] == scroll['uncoveredFrames'] == scroll['uncoveredMax'] == 0)
        check(f'{label}: jump probe no error', combo['jumpDomError'] is None and combo['jumpDom']['preparationError'] is None)
        for direction in ('outward', 'return'):
            probe = combo['jumpDom'][direction]
            check(f'{label} {direction}: frame and text budgets', probe['completed'] and not probe['timedOut'] and probe['frameReady'] and probe['textWithinBudget'])
        outward, returned = combo['jumpDom']['outward'], combo['jumpDom']['return']
        check(f'{label}: cached return text within first frame', returned['targetWasCached'] and returned['firstFrame']['dom']['textMatched'])
        check(f'{label}: eventual colors match', returned['final']['colorsMatched'])
        if kind == 'normal':
            check(f'{label}: standard first screen <=150ms', combo['firstScreen']['fromListChangesMs'] <= 150)
            check(f'{label}: prefetched target', combo['jump']['targetWasCached'] and outward['targetWasCached'])
            check(f'{label}: cached outward first frame text', outward['firstFrame']['dom']['textMatched'])
        else:
            check(f'{label}: truly uncached fixture', combo['firstScreen']['rowCount'] > 20000 and not combo['jump']['targetWasCached'] and not outward['targetWasCached'] and outward['missingRowsBefore'] == 40)
            check(f'{label}: cold legacy text <=100ms', combo['jump']['appearMs'] <= 100)
            first = outward['firstFrame']['dom']
            check(f'{label}: first frame text or visible placeholder', first['textMatched'] or first['placeholderExposed'])
        performance.append({'kind': kind, 'view': view, 'pid': launch['pid'], 'firstScreenMs': combo['firstScreen']['fromListChangesMs'],
            'legacyJump': combo['jump'], 'outwardTextDomMs': outward['textDomMs'], 'outwardFirstFrame': outward['firstFrame'],
            'returnTextDomMs': returned['textDomMs'], 'returnColorDomMs': returned['colorDomMs'], 'scroll': scroll, 'visibility': visibility})

output = ROOT / 'evidence/desktop-r2/assertions.json'
assert not output.exists()
output.write_text(json.dumps({'ok': True, 'source': SOURCE, 'binarySHA256': BINARY_SHA, 'assertions': len(checks), 'checks': checks, 'performance': performance}, indent=2) + '\n')
print(json.dumps({'ok': True, 'assertions': len(checks), 'output': str(output)}))
