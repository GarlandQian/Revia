#!/usr/bin/env python3
"""Version-pinned targeted RM recheck, normal Git and fresh app data."""

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

ROOT = Path('/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-task04-dv8o95m8')
WORKTREE = Path('/Users/garland/.codex/worktrees/revia-m2/Revia')
BINARY = WORKTREE / 'src-tauri/target/release/bundle/macos/Revia.app/Contents/MacOS/revia'
EVIDENCE = ROOT / 'evidence/desktop-r2'
HOME = ROOT / 'home-ui-r2'
DATA = ROOT / 'data-ui-r2'
parser = argparse.ArgumentParser()
parser.add_argument('action', choices=['launch', 'snapshot'])
parser.add_argument('label')
parser.add_argument('--source', required=True)
parser.add_argument('--binary-sha256', required=True)
args = parser.parse_args()
assert re.fullmatch('[A-Za-z0-9_-]+', args.label)
assert re.fullmatch('[0-9a-f]{40}', args.source)
assert re.fullmatch('[0-9a-f]{64}', args.binary_sha256)
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=WORKTREE, text=True).strip() == args.source
assert hashlib.sha256(BINARY.read_bytes()).hexdigest() == args.binary_sha256

if args.action == 'launch':
    for directory in (HOME, DATA, EVIDENCE):
        directory.mkdir(exist_ok=False)
    (HOME / '.gitconfig').write_text('')
    for startup in ('.zshenv', '.zshrc'):
        (HOME / startup).write_text('export PATH=/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin\n')
    worktree_id = hashlib.sha256(str((ROOT / 'B').resolve()).encode()).hexdigest()[:16]
    (DATA / 'settings.json').write_text(json.dumps({'repos': [str(ROOT / 'B')], 'lastWorktreeId': worktree_id, 'diffFontSize': 12}) + '\n')
    env = {key: value for key, value in os.environ.items() if not key.startswith(('GIT_', 'REVIA_'))}
    env.update(HOME=str(HOME), USERPROFILE=str(HOME), XDG_CONFIG_HOME=str(HOME / '.config'),
               GIT_CONFIG_NOSYSTEM='1', GIT_CONFIG_GLOBAL=str(HOME / '.gitconfig'), GIT_ATTR_NOSYSTEM='1',
               GIT_TERMINAL_PROMPT='0', REVIA_ACCEPTANCE='1', REVIA_ACCEPTANCE_DIR=str(DATA),
               REVIA_MEASURE_LOG=str(EVIDENCE / 'app.log'))
    with (EVIDENCE / 'stdout.log').open('x') as stream:
        child = subprocess.Popen([str(BINARY)], env=env, stdout=stream, stderr=subprocess.STDOUT, start_new_session=True)
    launch = {'source': args.source, 'binarySHA256': args.binary_sha256, 'binary': str(BINARY),
              'pid': child.pid, 'at': datetime.now(timezone.utc).isoformat(), 'dataDirectory': str(DATA),
              'home': str(HOME), 'diagnosticWrapper': None, 'productionWatcherEnabled': True}
    (EVIDENCE / 'launch.json').write_text(json.dumps(launch, indent=2) + '\n')
    print(json.dumps(launch, indent=2))
else:
    # Reuse the pinned read-only snapshot functions, not its parser or actions.
    code = Path('/tmp/revia-task04-desktop.py').read_text().split('\ndef prepare():', 1)[0]
    namespace = {}
    exec(compile(code, '/tmp/revia-task04-desktop.py', 'exec'), namespace)
    namespace.update(DATA=DATA, EVIDENCE=EVIDENCE, EXPECTED_SOURCE=args.source, EXPECTED_BINARY=args.binary_sha256)
    namespace['save_snapshot'](args.label)
