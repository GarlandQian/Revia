#!/usr/bin/env python3
"""Start one version-pinned D06 run with independent data and visible UI."""

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

ROOT = Path('/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-task04-dv8o95m8')
FIXTURES = Path('/private/tmp/revia-task06-2fuu2r92')
WORKTREE = Path('/Users/garland/.codex/worktrees/revia-m2/Revia')
BINARY = WORKTREE / 'src-tauri/target/release/bundle/macos/Revia.app/Contents/MacOS/revia'
parser = argparse.ArgumentParser()
parser.add_argument('kind', choices=['normal', 'cold'])
parser.add_argument('view', choices=['split', 'unified'])
parser.add_argument('attempt', type=int)
parser.add_argument('--source', required=True)
parser.add_argument('--binary-sha256', required=True)
args = parser.parse_args()
assert re.fullmatch('[0-9a-f]{40}', args.source)
assert re.fullmatch('[0-9a-f]{64}', args.binary_sha256)
assert args.attempt > 0
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=WORKTREE, text=True).strip() == args.source
assert hashlib.sha256(BINARY.read_bytes()).hexdigest() == args.binary_sha256
output = ROOT / 'perf-runs' / f'{args.kind}-{args.view}-{args.attempt}'
output.mkdir(parents=True, exist_ok=False)
data = output / 'data'
data.mkdir()
home = output / 'home'
home.mkdir()
(home / '.gitconfig').write_text('')
for startup in ('.zshenv', '.zshrc'):
    (home / startup).write_text('export PATH=/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin\n')
repo = FIXTURES / ('perf-fixtures/diffs' if args.kind == 'normal' else 'perf-cold')
path = 'src/generated.ts' if args.kind == 'normal' else 'cold.ts'
assert repo.is_dir() and (repo / path).is_file()
env = {key: value for key, value in os.environ.items() if not key.startswith(('GIT_', 'REVIA_'))}
env.update(HOME=str(home), USERPROFILE=str(home), XDG_CONFIG_HOME=str(home / '.config'),
           GIT_CONFIG_NOSYSTEM='1', GIT_CONFIG_GLOBAL=str(home / '.gitconfig'),
           GIT_TERMINAL_PROMPT='0', REVIA_MEASURE='1', REVIA_MEASURE_ONLY=args.view,
           REVIA_MEASURE_KEEP_OPEN='1', REVIA_MEASURE_DATA_DIR=str(data),
           REVIA_MEASURE_REPO=str(repo), REVIA_MEASURE_PATH=path,
           REVIA_MEASURE_LOG=str(output / 'result.json'))
with (output / 'stdout.log').open('x') as stream:
    child = subprocess.Popen([str(BINARY)], env=env, stdout=stream, stderr=subprocess.STDOUT, start_new_session=True)
launch = {'source': args.source, 'binary': str(BINARY), 'binarySHA256': args.binary_sha256,
          'pid': child.pid, 'kind': args.kind, 'view': args.view, 'output': str(output),
          'at': datetime.now(timezone.utc).isoformat(), 'dataDirectory': str(data),
          'home': str(home), 'repo': str(repo), 'path': path, 'keepOpen': True,
          'fixtureContentSHA256': hashlib.sha256((repo / path).read_bytes()).hexdigest()}
(output / 'launch.json').write_text(json.dumps(launch, indent=2) + '\n')
print(json.dumps(launch, indent=2))
