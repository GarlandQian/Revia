import datetime
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

repo = Path('/Volumes/T7 Shield/project/Revia-m2-ui')
previous = Path('/private/tmp/revia-task13-check01-ukzmi5rb')
out = Path(tempfile.mkdtemp(prefix='revia-task13-check02-', dir='/private/tmp'))
env = os.environ.copy()
env['PATH'] = '/opt/homebrew/bin:' + env.get('PATH', '')
before = json.loads((previous / 'source-before.json').read_text())
results = []

def hashes():
    return {p: hashlib.sha256((repo / p).read_bytes()).hexdigest() for p in before}

def run(name, command):
    meta = {'name': name, 'cwd': str(repo), 'command': command,
            'startedAt': datetime.datetime.now(datetime.timezone.utc).isoformat()}
    with (out / f'{name}.stdout').open('xb') as stdout, (out / f'{name}.stderr').open('xb') as stderr:
        result = subprocess.run(command, cwd=repo, env=env, stdout=stdout, stderr=stderr)
    meta.update(exit=result.returncode, endedAt=datetime.datetime.now(datetime.timezone.utc).isoformat())
    (out / f'{name}.json').write_text(json.dumps(meta, indent=2) + '\n')
    results.append(meta)
    print(json.dumps(meta), flush=True)
    return result.returncode == 0

print(str(out), flush=True)
(out / 'source-before-normalization.json').write_text(json.dumps(hashes(), indent=2) + '\n')
ok = run('normalize-bindings', ['node', 'tools/strip-binding-whitespace.mjs'])
normalized = hashes()
changes = {p: {'before': before[p], 'after': normalized[p]} for p in before if before[p] != normalized[p]}
(out / 'normalization-comparison.json').write_text(json.dumps(changes, indent=2) + '\n')
ok = ok and not changes
ok &= run('typecheck', ['pnpm', 'exec', 'tsc', '-b'])
ok &= run('test-ui-input', ['node', 'tools/test-ui-input.mjs'])
ok &= run('test-jump-dom-probe', ['node', 'tools/test-jump-dom-probe.mjs'])
ok &= run('diff-check', ['git', 'diff', '--check'])
after = hashes()
(out / 'source-after.json').write_text(json.dumps(after, indent=2) + '\n')
summary = {'previous': str(previous), 'sourceMatchesBeforeFullChecks': before == after,
           'allChecksPassed': bool(ok), 'commands': results,
           'boundary': 'Only ts-rs trailing whitespace normalized. Initial diff-check failure retained; no Rust rerun, Release, GUI, D06 or merge.'}
(out / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps(summary), flush=True)
raise SystemExit(0 if ok and before == after else 1)
