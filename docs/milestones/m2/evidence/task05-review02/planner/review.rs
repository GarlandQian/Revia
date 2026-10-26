use std::{collections::HashMap,fs,path::{Path,PathBuf},process::{Command,Stdio},sync::Arc,io::Write};
use revia_lib::{git::{CommandLog,status::ChangesKind},recovery::{RecoveryStore,DiscardPhase},session::{WorktreeSession,ChangeEntry},shell::{ShellEnv,EnvCaptureInfo}};
struct Fixture {repo:PathBuf, vars:HashMap<String,String>, paths:Vec<String>, store:RecoveryStore}
impl Fixture {
 fn new(root:&Path,name:&str)->Self {
  let base=root.join(name); let repo=base.join("repo"); let home=base.join("home"); fs::create_dir_all(&repo).unwrap();fs::create_dir_all(&home).unwrap();
  let path=std::env::var("PATH").unwrap(); let paths=std::env::split_paths(&path).map(|p|p.display().to_string()).collect();
  let vars=HashMap::from([("PATH".into(),path),("HOME".into(),home.display().to_string()),("GIT_CONFIG_GLOBAL".into(),"/dev/null".into()),("GIT_CONFIG_NOSYSTEM".into(),"1".into()),("GIT_TERMINAL_PROMPT".into(),"0".into())]);
  let f=Self{repo,vars,paths,store:RecoveryStore::new(base.join("data"))}; f.git(&["init","-q"]);f.git(&["config","user.name","Fixture"]);f.git(&["config","user.email","fixture@example.invalid"]);f.git(&["config","commit.gpgsign","false"]);f.git(&["config","core.autocrlf","false"]);f
 }
 fn command(&self,args:&[&str])->Command {let mut c=Command::new("git");c.current_dir(&self.repo).env_clear().envs(&self.vars).args(args);c}
 fn git(&self,args:&[&str])->String {let o=self.command(args).output().unwrap();assert!(o.status.success(),"{:?} {}",args,String::from_utf8_lossy(&o.stderr));String::from_utf8_lossy(&o.stdout).into_owned()}
 fn git_input(&self,args:&[&str],input:&str){let mut c=self.command(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();c.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();let o=c.wait_with_output().unwrap();assert!(o.status.success(),"{}",String::from_utf8_lossy(&o.stderr));}
 fn setup(&self){fs::write(self.repo.join("target.txt"),b"base\n").unwrap();self.git(&["add","--all"]);self.git(&["commit","-qm","base"]);fs::write(self.repo.join("target.txt"),b"saved change\n").unwrap();}
 fn session(&self)->Arc<WorktreeSession>{WorktreeSession::open("fixture",&self.repo,ShellEnv{vars:self.vars.clone(),path:self.paths.clone(),info:EnvCaptureInfo{worktree:self.repo.display().to_string(),shell:"fixture".into(),elapsed_ms:0,timed_out:false,used_fallback:true,var_count:self.vars.len(),path_entries:self.paths.clone(),diagnostics:None}},Arc::new(CommandLog::default())).unwrap()}
 fn entry(&self,s:&WorktreeSession)->ChangeEntry{s.refresh().unwrap().entries.into_iter().find(|e|e.path=="target.txt"&&e.changes==ChangesKind::Unstaged).unwrap()}
}

fn main() {
 let root=PathBuf::from(std::env::args().nth(1).unwrap());
 let f=Fixture::new(&root,"before-write");f.setup();let s=f.session();let e=f.entry(&s);
 s.discard_file(&e.path,&e.fingerprint,&e.snapshot_revision,&f.store).unwrap();
 let id=f.store.list(None).unwrap()[0].id.clone();let i=s.inspect_record(&id,&f.store).unwrap();
 let expected=revia_lib::recovery::RestoreExpectation{paths:i.paths.clone(),state:i.state};
 let r=s.restore_record_with_hook(&id,&expected,&f.store,|phase|{
  if phase==revia_lib::recovery::RestorePhase::BeforeWrite {fs::write(f.repo.join("target.txt"),b"later external valuable content\n").unwrap();} Ok(())});
 println!("external_edit_restore_ok={}",r.is_ok());
 println!("external_edit_was_overwritten={}",fs::read(f.repo.join("target.txt")).unwrap()==b"saved change\n");

 let f=Fixture::new(&root,"before-complete");f.setup();let s=f.session();let e=f.entry(&s);
 s.discard_file(&e.path,&e.fingerprint,&e.snapshot_revision,&f.store).unwrap();
 let id=f.store.list(None).unwrap()[0].id.clone();let i=s.inspect_record(&id,&f.store).unwrap();
 let expected=revia_lib::recovery::RestoreExpectation{paths:i.paths.clone(),state:i.state};
 let r=s.restore_record_with_hook(&id,&expected,&f.store,|phase|{
  if phase==revia_lib::recovery::RestorePhase::BeforeComplete {return Err(revia_lib::error::AppError::internal("controlled completion receipt loss"));} Ok(())});
 println!("completion_loss_error={:?}",r.unwrap_err().kind);
 println!("completion_loss_worktree_written={}",fs::read(f.repo.join("target.txt")).unwrap()==b"saved change\n");
 println!("completion_loss_journal={:?}",f.store.restore_journal(&id).unwrap().unwrap().state);
 println!("completion_loss_list_status={:?}",f.store.list(None).unwrap()[0].status);

 let f=Fixture::new(&root,"identity-switch");f.setup();let s=f.session();let e=f.entry(&s);
 s.discard_file(&e.path,&e.fingerprint,&e.snapshot_revision,&f.store).unwrap();
 let id=f.store.list(None).unwrap()[0].id.clone();let i=s.inspect_record(&id,&f.store).unwrap();
 let expected=revia_lib::recovery::RestoreExpectation{paths:i.paths.clone(),state:i.state};
 let r=s.restore_record_with_hook(&id,&expected,&f.store,|phase|{
  if phase==revia_lib::recovery::RestorePhase::BeforeWrite {
   fs::rename(&f.repo,f.repo.with_file_name("original-repo")).unwrap();
   fs::create_dir(&f.repo).unwrap();f.git(&["init","-q"]);
   fs::write(f.repo.join("target.txt"),b"base\n").unwrap();
  } Ok(())});
 println!("identity_switch_restore_ok={}",r.is_ok());
 println!("identity_switch_new_worktree_written={}",fs::read(f.repo.join("target.txt")).unwrap()==b"saved change\n");
}
