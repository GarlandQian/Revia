import datetime
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

repo = Path('/Volumes/T7 Shield/project/Revia-m2-ui')
head = '35da167f8a2202b19a211ade2c311f5ff430cd90'
out = Path(tempfile.mkdtemp(prefix='revia-task13-check01-', dir='/private/tmp'))
env = os.environ.copy()
env['PATH'] = '/opt/homebrew/bin:' + env.get('PATH', '')

def git(*args):
    return subprocess.check_output(['git', '-C', str(repo), *args])

assert git('rev-parse', 'HEAD').decode().strip() == head
(out / 'product.diff').write_bytes(git('diff', '--', 'src', 'src-tauri', 'tools'))
paths = [p for p in git('ls-files').decode().splitlines()
         if p.startswith(('src/', 'src-tauri/src/', 'tools/'))
         or p in ['package.json', 'pnpm-lock.yaml', 'src-tauri/tauri.conf.json',
                  'src-tauri/capabilities/default.json', 'src-tauri/Cargo.toml', 'src-tauri/Cargo.lock']]
paths += ['tools/test-ui-input.mjs', 'tools/test-jump-dom-probe.mjs']

def source_hashes():
    return {p: hashlib.sha256((repo / p).read_bytes()).hexdigest() for p in sorted(set(paths))}

(out / 'source-before.json').write_text(json.dumps(source_hashes(), indent=2) + '\n')
(out / 'base.txt').write_text(head + '\n')
results = []

def run(name, command, expected=0):
    meta = {'name': name, 'cwd': str(repo), 'command': command,
            'startedAt': datetime.datetime.now(datetime.timezone.utc).isoformat(),
            'expectedExit': expected}
    with (out / (name + '.stdout')).open('xb') as stdout, (out / (name + '.stderr')).open('xb') as stderr:
        process = subprocess.run(command, cwd=repo, env=env, stdout=stdout, stderr=stderr)
    meta.update(exit=process.returncode, endedAt=datetime.datetime.now(datetime.timezone.utc).isoformat())
    (out / (name + '.json')).write_text(json.dumps(meta, indent=2) + '\n')
    results.append(meta)
    print(json.dumps(meta), flush=True)
    return process.returncode == expected

print(json.dumps({'logDirectory': str(out), 'base': head}), flush=True)
ok = run('bindings', ['pnpm', 'bindings'])
ok &= run('typecheck', ['pnpm', 'exec', 'tsc', '-b'])
ok &= run('lint', ['pnpm', 'lint'])
ok &= run('build', ['pnpm', 'build'])
for path in sorted((repo / 'tools').glob('test-*.mjs')):
    ok &= run(path.stem, ['node', str(path.relative_to(repo))])
ok &= run('python', ['python3', '-m', 'unittest', 'discover', '-s', 'tools', '-p', 'test_*.py'])
ok &= run('rust', ['/opt/homebrew/bin/cargo', 'test', '--manifest-path', 'src-tauri/Cargo.toml'])
ok &= run('diff-check', ['git', 'diff', '--check'])

# Sensitivity only: the same new regression checks must reject the unchanged reviewed source.
for name, module in [('ui-input', 'platform'), ('jump-dom-probe', 'jumpDomProbe')]:
    old = out / (module + '-35da167.ts')
    old.write_bytes(git('show', f'{head}:src/lib/{module}.ts'))
    source = (repo / 'tools' / f'test-{name}.mjs').read_text()
    original_import = f'"../src/lib/{module}.ts"'
    assert source.count(original_import) == 1
    test_path = out / f'sensitivity-{name}.mjs'
    test_path.write_text(source.replace(original_import, json.dumps(old.as_uri()), 1))
    ok &= run(f'sensitivity-{name}', ['node', str(test_path)], expected=1)

after = source_hashes()
(out / 'source-after.json').write_text(json.dumps(after, indent=2) + '\n')
before = json.loads((out / 'source-before.json').read_text())
unchanged = before == after
summary = {'base': head, 'logDirectory': str(out), 'sourceUnchanged': unchanged,
           'allExpectedExits': bool(ok), 'commands': len(results),
           'afterSourceSha256': hashlib.sha256((out / 'source-after.json').read_bytes()).hexdigest(),
           'boundary': 'Uncommitted task13 corrections; no Release/GUI/D06/CI/long-run or merge. Baseline sensitivity uses the same regression tests against 35da167 modules, not production execution.'}
(out / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
(out / 'commands.json').write_text(json.dumps(results, indent=2) + '\n')
print(json.dumps(summary), flush=True)
raise SystemExit(0 if ok and unchanged else 1)
