#!/usr/bin/env python3
"""Run unchanged concurrency test on pinned old/new sources, without patching either."""

from datetime import datetime, timezone
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

WORKTREE = Path('/Users/garland/.codex/worktrees/revia-m2/Revia')
ROOT = Path(tempfile.mkdtemp(prefix='revia-task04-refresh-audit-', dir='/private/tmp'))
TEST = 't13_concurrent_refreshes_never_publish_stale_content'
env = dict(os.environ)
env['PATH'] = '/opt/homebrew/bin:' + env.get('PATH', '')
env['CARGO_TARGET_DIR'] = str(WORKTREE / 'src-tauri/target')
result = {'root': str(ROOT), 'at': datetime.now(timezone.utc).isoformat(), 'runs': [],
          'boundary': 'Unmodified git-archived sources, three attempts per version; no relaxed assertions or production edits.'}
for source in ('a74e9d2', '41be300e845d66eb3a9fc734bfd247010c22edb8'):
    revision = subprocess.check_output(['git', 'rev-parse', source], cwd=WORKTREE, text=True).strip()
    destination = ROOT / source
    destination.mkdir()
    archive = subprocess.check_output(['git', 'archive', revision, 'src-tauri'], cwd=WORKTREE)
    subprocess.run(['tar', '-xf', '-', '-C', str(destination)], input=archive, check=True)
    shutil.copytree(WORKTREE / 'dist', destination / 'dist')
    command = ['/opt/homebrew/bin/cargo', 'test', '--manifest-path', str(destination / 'src-tauri/Cargo.toml'),
               '--test', 'refresh_isolation', TEST, '--', '--exact', '--nocapture']
    for attempt in range(1, 4):
        output = destination / f'attempt-{attempt}.log'
        with output.open('x') as stream:
            completed = subprocess.run(command, cwd=destination, env=env, stdout=stream, stderr=subprocess.STDOUT, timeout=300)
        row = {'source': revision, 'attempt': attempt, 'command': command, 'exitCode': completed.returncode, 'log': str(output)}
        result['runs'].append(row)
        print(json.dumps(row), flush=True)
        (ROOT / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps({'root': str(ROOT), 'failures': sum(run['exitCode'] != 0 for run in result['runs']), 'runs': len(result['runs'])}))
