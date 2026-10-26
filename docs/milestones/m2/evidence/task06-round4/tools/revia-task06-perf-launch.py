import os,json,hashlib,pathlib,subprocess,sys,datetime
w=pathlib.Path('/Users/garland/.codex/worktrees/revia-m2/Revia'); r=pathlib.Path('/private/tmp/revia-task06-2fuu2r92'); binary=w/'src-tauri/target/release/bundle/macos/Revia.app/Contents/MacOS/revia'
kind,view,attempt=sys.argv[1:]; assert kind in ('normal','cold') and view in ('split','unified') and attempt.isdigit()
out=r/'perf-runs'/f'{kind}-{view}-{attempt}';out.mkdir(parents=True,exist_ok=False); data=out/'data';data.mkdir()
env={k:v for k,v in os.environ.items() if not k.startswith(('GIT_','REVIA_'))}; env.update(HOME=str(r/'home'),GIT_CONFIG_NOSYSTEM='1',GIT_CONFIG_GLOBAL=str(r/'home/.gitconfig'),REVIA_MEASURE='1',REVIA_MEASURE_ONLY=view,REVIA_MEASURE_KEEP_OPEN='1',REVIA_MEASURE_DATA_DIR=str(data),REVIA_MEASURE_REPO=str(r/('perf-fixtures/diffs' if kind=='normal' else 'perf-cold')),REVIA_MEASURE_PATH='src/generated.ts' if kind=='normal' else 'cold.ts',REVIA_MEASURE_LOG=str(out/'result.json'))
with (out/'stdout.log').open('x') as log: p=subprocess.Popen([str(binary)],env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
v={'source':subprocess.check_output(['git','rev-parse','HEAD'],cwd=w,text=True).strip(),'binary':str(binary),'binarySha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'pid':p.pid,'kind':kind,'view':view,'output':str(out),'launchedAt':datetime.datetime.now().astimezone().isoformat(),'dataDir':str(data),'repo':env['REVIA_MEASURE_REPO'],'path':env['REVIA_MEASURE_PATH'],'keepOpen':True}
(out/'launch.json').write_text(json.dumps(v,indent=2));print(json.dumps(v,indent=2))
