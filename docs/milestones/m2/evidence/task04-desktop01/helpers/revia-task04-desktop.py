#!/usr/bin/env python3
"""Fixture-pinned process and file helpers; UI actions are done separately with CUA."""

import argparse
import base64
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import stat
import subprocess


ROOT = Path('/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-task04-dv8o95m8')
WORKTREE = Path('/Users/garland/.codex/worktrees/revia-m2/Revia')
BINARY = WORKTREE / 'src-tauri/target/release/bundle/macos/Revia.app/Contents/MacOS/revia'
EXPECTED_SOURCE = '36dad654fe0d2d337e34d7bd69aaae04d833903f'
EXPECTED_BINARY = '6e119703a7ecf9db37b0f2db0072bbb802f6088432b59b819caeebab21d8dfc9'
GIT = Path('/opt/homebrew/bin/git').resolve()
HOME = ROOT / 'home-ui-r1'
DATA = ROOT / 'data-ui-r1'
CONTROL = ROOT / 'control-ui-r1'
EVIDENCE = ROOT / 'evidence' / 'desktop-r1'
LABEL = re.compile(r'[A-Za-z0-9][A-Za-z0-9_-]{0,60}\Z')


def sha(data):
    return hashlib.sha256(data).hexdigest()


def now():
    return datetime.now(timezone.utc).isoformat()


def checked_label(value):
    assert LABEL.fullmatch(value), 'Invalid label'
    return value


def env_for(home):
    result = {key: value for key, value in os.environ.items() if not key.startswith(('GIT_', 'REVIA_'))}
    result.update(HOME=str(home), USERPROFILE=str(home), XDG_CONFIG_HOME=str(home / '.config'),
                  GIT_CONFIG_GLOBAL=str(home / '.gitconfig'), GIT_CONFIG_NOSYSTEM='1', GIT_ATTR_NOSYSTEM='1',
                  GIT_TERMINAL_PROMPT='0')
    return result


def git(repo, *args):
    assert repo in [ROOT / name for name in ('A', 'B', 'L')]
    return subprocess.check_output([str(GIT), '-C', str(repo), *args], env=env_for(ROOT / 'home'), stderr=subprocess.PIPE)


def worktree_id(name):
    assert name in ('A', 'B', 'L')
    return sha(str((ROOT / name).resolve()).encode())[:16]


def snapshot(name):
    repo = ROOT / name
    files = []
    for path in sorted(repo.rglob('*')):
        rel = path.relative_to(repo)
        if '.git' in rel.parts or not (path.is_file() or path.is_symlink()):
            continue
        info = path.lstat()
        data = os.fsencode(os.readlink(path)) if path.is_symlink() else path.read_bytes()
        files.append({'path': rel.as_posix(), 'kind': 'symlink' if path.is_symlink() else 'file',
                      'mode': oct(stat.S_IMODE(info.st_mode)), 'sha256': sha(data), 'bytes': len(data),
                      'contentBase64': base64.b64encode(data).decode()})
    return {'path': str(repo), 'worktreeId': worktree_id(name), 'head': git(repo, 'rev-parse', 'HEAD').decode().strip(),
            'indexEntriesBase64': base64.b64encode(git(repo, 'ls-files', '--stage', '-z')).decode(),
            'statusBase64': base64.b64encode(git(repo, 'status', '--porcelain=v2', '-z')).decode(), 'files': files}


def recovery():
    result = []
    for directory in sorted((DATA / 'recovery').glob('record-*')):
        result.append({'directory': directory.name, 'saved': json.loads((directory / 'saved.json').read_text()),
                       'outcome': json.loads((directory / 'outcome.json').read_text()) if (directory / 'outcome.json').is_file() else None})
    return result


def save_snapshot(label):
    value = {'at': now(), 'source': EXPECTED_SOURCE, 'binarySHA256': EXPECTED_BINARY,
             'worktrees': {name: snapshot(name) for name in ('A', 'B', 'L')}, 'recovery': recovery()}
    output = EVIDENCE / f'{checked_label(label)}.json'
    with output.open('x') as stream:
        json.dump(value, stream, indent=2)
        stream.write('\n')
    print(json.dumps({'snapshot': str(output), 'records': len(value['recovery']),
                      'indexSHA256': {name: sha(base64.b64decode(value['worktrees'][name]['indexEntriesBase64'])) for name in ('A', 'B', 'L')}}))


def prepare():
    initial = json.loads((ROOT / 'evidence/initial-state.json').read_text())
    for name in ('A', 'B', 'L'):
        current = snapshot(name)
        comparable = [{key: value for key, value in item.items() if key != 'contentBase64'} for item in current['files']]
        assert comparable == initial[name]['files'], f'{name} initial file state differs'
        assert current['indexEntriesBase64'] == initial[name]['indexEntriesBase64'], f'{name} index differs'
    for directory in (HOME, DATA, CONTROL, EVIDENCE):
        directory.mkdir(exist_ok=False)
    (HOME / '.gitconfig').write_text('')
    bindir = HOME / 'bin'
    bindir.mkdir()
    path_line = f'export PATH={shlex.quote(str(bindir))}:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin\n'
    (HOME / '.zshenv').write_text(path_line)
    (HOME / '.zshrc').write_text(path_line)
    script = f'''#!/bin/sh
REAL={shlex.quote(str(GIT))}
CTL={shlex.quote(str(CONTROL))}
found=""
for arg in "$@"; do [ "$arg" = "apply" ] && found=1; done
actual=$(pwd -P)
case "$actual" in
  {shlex.quote(str(ROOT / 'A'))}|{shlex.quote(str(ROOT / 'B'))}|{shlex.quote(str(ROOT / 'L'))}) allowed=1 ;;
  *) allowed=0 ;;
esac
if [ -n "$found" ] && [ "$allowed" = 1 ] && [ -f "$CTL/next" ]; then
  read label mode < "$CTL/next"
  mv "$CTL/next" "$CTL/claimed-$label" || exit 143
  printf '%s\\n' "$actual" > "$CTL/cwd-$label"
  if [ "$mode" = hold ]; then
    n=0
    while [ ! -f "$CTL/release-$label" ] && [ "$n" -lt 900 ]; do
      sleep 0.1
      n=$((n + 1))
    done
    [ -f "$CTL/release-$label" ] || exit 142
  fi
  "$REAL" "$@"; code=$?
  printf '%s\\n' "$code" >> "$CTL/applied-$label"
  [ "$mode" = lost ] && [ "$code" -eq 0 ] && exit 141
  exit "$code"
fi
exec "$REAL" "$@"
'''
    wrapper = bindir / 'git'
    wrapper.write_text(script)
    wrapper.chmod(0o755)
    (DATA / 'settings.json').write_text(json.dumps({'repos': [str(ROOT / 'A'), str(ROOT / 'B')],
        'lastWorktreeId': worktree_id('A'), 'diffFontSize': 12}, indent=2) + '\n')
    save_snapshot('initial')


def launch(label):
    checked_label(label)
    source = subprocess.check_output(['git', '-C', str(WORKTREE), 'rev-parse', 'HEAD'], text=True).strip()
    assert source == EXPECTED_SOURCE
    assert sha(BINARY.read_bytes()) == EXPECTED_BINARY
    output = EVIDENCE / f'run-{label}'
    output.mkdir(exist_ok=False)
    env = env_for(HOME)
    env.update(REVIA_ACCEPTANCE='1', REVIA_ACCEPTANCE_DIR=str(DATA), REVIA_MEASURE_LOG=str(output / 'app.log'))
    with (output / 'stdout.log').open('x') as stream:
        child = subprocess.Popen([str(BINARY)], env=env, stdout=stream, stderr=subprocess.STDOUT, start_new_session=True)
    value = {'at': now(), 'source': source, 'binary': str(BINARY), 'binarySHA256': EXPECTED_BINARY,
             'pid': child.pid, 'dataDirectory': str(DATA), 'home': str(HOME), 'commandLog': str(output / 'app.log'),
             'diagnosticWrapper': str(HOME / 'bin/git'), 'wrapperInactiveUnlessExplicitlyArmed': True}
    (output / 'launch.json').write_text(json.dumps(value, indent=2) + '\n')
    print(json.dumps(value, indent=2))


parser = argparse.ArgumentParser()
sub = parser.add_subparsers(dest='action', required=True)
sub.add_parser('prepare')
sub.add_parser('fix-shell')
for action in ('launch', 'snapshot', 'release'):
    sub.add_parser(action).add_argument('label')
arm = sub.add_parser('arm')
arm.add_argument('mode', choices=('lost', 'hold'))
arm.add_argument('label')
edit = sub.add_parser('edit')
edit.add_argument('repo', choices=('A', 'B', 'L'))
edit.add_argument('line', type=int, choices=(10, 80, 140))
edit.add_argument('value', type=int)
permission = sub.add_parser('store-permission')
permission.add_argument('mode', choices=('blocked', 'open'))
args = parser.parse_args()
assert ROOT.resolve() == ROOT and ROOT.is_dir()
if args.action == 'prepare':
    prepare()
elif args.action == 'fix-shell':
    (HOME / '.zshrc').write_text(f'export PATH={shlex.quote(str(HOME / "bin"))}:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin\n')
    print('Updated only the isolated HOME startup file after macOS path_helper reset PATH.')
elif args.action == 'launch':
    launch(args.label)
elif args.action == 'snapshot':
    save_snapshot(args.label)
elif args.action == 'arm':
    checked_label(args.label)
    assert not (CONTROL / 'next').exists()
    assert not (CONTROL / f'claimed-{args.label}').exists()
    (CONTROL / 'next').write_text(f'{args.label} {args.mode}\n')
    print(json.dumps({'armed': args.mode, 'label': args.label, 'at': now()}))
elif args.action == 'release':
    (CONTROL / f'release-{checked_label(args.label)}').write_text(now())
    print(json.dumps({'released': args.label, 'at': now()}))
elif args.action == 'edit':
    assert -1000000 < args.value < 1000000
    path = ROOT / args.repo / '00-hunks.ts'
    assert not path.is_symlink()
    lines = path.read_text().splitlines(keepends=True)
    assert len(lines) == 180
    lines[args.line - 1] = f'export const value{args.line:03} = {args.value};\n'
    path.write_text(''.join(lines))
    print(json.dumps({'edited': str(path), 'line': args.line, 'value': args.value, 'at': now()}))
elif args.action == 'store-permission':
    target = DATA / 'recovery'
    target.mkdir(exist_ok=True)
    assert not target.is_symlink()
    target.chmod(0o500 if args.mode == 'blocked' else 0o700)
    print(json.dumps({'directory': str(target), 'mode': args.mode, 'at': now()}))
