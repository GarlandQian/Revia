import json
import os
from pathlib import Path
import subprocess
import tempfile

root = Path(tempfile.mkdtemp(prefix='revia-accept03-', dir='/tmp')).resolve()
home = root / 'home'
repo = root / 'repo'
home.mkdir()
repo.mkdir()
config = home / '.gitconfig'
config.write_text('[user]\n\tname = Revia fixture\n\temail = fixture@example.invalid\n[init]\n\tdefaultBranch = main\n[core]\n\tautocrlf = false\n[commit]\n\tgpgsign = false\n')
env = {key: value for key, value in os.environ.items() if not key.startswith('GIT_')}
env.update(HOME=str(home), GIT_CONFIG_NOSYSTEM='1', GIT_CONFIG_GLOBAL=str(config))
def git(*args):
    return subprocess.run(['git', *args], cwd=repo, env=env, check=True, capture_output=True).stdout

git('init')
base = [f'export const value{i:03d} = {i};\n' for i in range(1, 181)]
(repo / 'a-review.ts').write_text(''.join(base))
large = [f'export const item{i:04d} = {i};\n' for i in range(1, 1001)]
(repo / 'many.ts').write_text(''.join(large))
(repo / 'binary.dat').write_bytes(b'original\0bytes\n')
git('add', '.')
git('commit', '-m', 'fixture base')
for index in (9, 79, 139):
    base[index] = base[index].replace(f'= {index + 1};', f'= {index + 1001};')
(repo / 'a-review.ts').write_text(''.join(base))
for index in range(9, 1000, 20):
    large[index] = large[index].replace(f'= {index + 1};', f'= {index + 1001};')
(repo / 'many.ts').write_text(''.join(large))
(repo / 'binary.dat').write_bytes(b'changed\0bytes\n')
(repo / 'untracked.txt').write_text('not tracked\n')
settings = home / 'Library/Application Support/io.github.garlandqian.revia/settings.json'
settings.parent.mkdir(parents=True)
settings.write_text(json.dumps({'repos': [str(repo)], 'lastWorktreeId': None, 'diffFontSize': 12}))
(root / 'setup.json').write_text(json.dumps({'root': str(root), 'home': str(home), 'repo': str(repo)}, indent=2))
Path('/tmp/revia-accept03-current.json').write_text((root / 'setup.json').read_text())
print((root / 'setup.json').read_text())
print(git('diff', '--stat').decode())
