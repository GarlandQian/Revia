//! Q120/Q197：真实 Git/生产提交入口，独占 HOME/appdata；不是桌面验收替代品。
#[path = "support/git_environment.rs"]
mod git_environment;
use revia_lib::{
    commit::DraftStore,
    commit_flow::{CommitOptions, CommitOutcome},
    git::CommandLog,
    m7::changelists::{ChangeAction, ChangelistSelection, ChangelistStore, ChangelistView},
    recovery::RecoveryStore,
    session::WorktreeSession,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};
struct Fixture {
    _temp: tempfile::TempDir,
    repo: PathBuf,
    home: PathBuf,
    data: PathBuf,
    session: Arc<WorktreeSession>,
    store: ChangelistStore,
}
impl Fixture {
    fn new() -> Self {
        let t = tempfile::tempdir().unwrap();
        let repo = t.path().join("repo");
        let home = t.path().join("home");
        let data = t.path().join("appdata");
        for p in [&repo, &home, &data] {
            fs::create_dir(p).unwrap();
        }
        fs::write(home.join("gitconfig"), "").unwrap();
        git(&repo, &home, &["init", "-b", "main"]);
        git(&repo, &home, &["config", "user.name", "fixture"]);
        git(
            &repo,
            &home,
            &["config", "user.email", "fixture@example.invalid"],
        );
        git(&repo, &home, &["config", "commit.gpgsign", "false"]);
        fs::write(repo.join("keep.txt"), "BASE\n").unwrap();
        fs::write(repo.join("other.txt"), "OTHER\n").unwrap();
        fs::write(
            repo.join("mixed.txt"),
            (1..=40)
                .map(|n| format!("line{n:02}: base\n"))
                .collect::<String>(),
        )
        .unwrap();
        git(&repo, &home, &["add", "--all"]);
        git(&repo, &home, &["commit", "-m", "base"]);
        let session = WorktreeSession::open(
            "fixture-changelist",
            &repo,
            git_environment::shell_env(&home, &repo, &home.join("gitconfig")),
            Arc::new(CommandLog::default()),
        )
        .unwrap();
        let store = ChangelistStore::new(&data);
        Self {
            _temp: t,
            repo,
            home,
            data,
            session,
            store,
        }
    }
    fn git(&self, a: &[&str]) -> String {
        git(&self.repo, &self.home, a)
    }
    fn view(&self) -> ChangelistView {
        self.store.view(&self.session).unwrap()
    }
    fn change(&self, v: &ChangelistView, a: ChangeAction) -> ChangelistView {
        self.store.change(&self.session, &token(v), a).unwrap()
    }
    fn select(&self, v: &ChangelistView, path: &str, hunk: Option<usize>) -> ChangelistView {
        let ids = v
            .items
            .iter()
            .filter(|i| i.path == path && !i.stale && (hunk.is_none() || i.hunk == hunk))
            .map(|i| i.id.clone())
            .collect();
        self.change(
            v,
            ChangeAction::Select {
                items: ids,
                selected: true,
            },
        )
    }
    fn commit(&self, v: &ChangelistView) -> revia_lib::error::Result<CommitOutcome> {
        let d = DraftStore::new(self.data.clone());
        let expected = self.session.commit_context(&d)?.expectation;
        self.session.execute_changelist_commit_flow(
            &self.store,
            &token(v),
            &d,
            &RecoveryStore::new(self.data.clone()),
            self.data.clone(),
            CommitOptions {
                message: "selected only".into(),
                amend: false,
                skip_hooks: false,
                expected,
                amend_confirmation: None,
                client_nonce: "fixture-nonce".into(),
                draft_revision: "".into(),
            },
            "fixture-commit".into(),
            Arc::new(|_| {}),
        )
    }
    fn staged_other(&self) {
        fs::write(self.repo.join("keep.txt"), "ALREADY STAGED 中文\n").unwrap();
        self.git(&["add", "keep.txt"]);
        fs::write(self.repo.join("other.txt"), "SELECTED 中文\n").unwrap();
    }
}
fn token(v: &ChangelistView) -> ChangelistSelection {
    ChangelistSelection {
        revision: v.revision.clone(),
        identity: v.identity.clone(),
    }
}
fn git(repo: &Path, home: &Path, a: &[&str]) -> String {
    let env = git_environment::shell_env(home, repo, &home.join("gitconfig"));
    let o = Command::new(env.which("git").unwrap())
        .args(a)
        .current_dir(repo)
        .env_clear()
        .envs(env.vars)
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "git {a:?}: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8(o.stdout).unwrap()
}
#[test]
fn untouched_staged_path_is_excluded_and_preserved() {
    let f = Fixture::new();
    f.staged_other();
    let v = f.select(&f.view(), "other.txt", None);
    let result = f.commit(&v).unwrap();
    assert!(result.success);
    assert!(!result.needs_verification);
    assert_eq!(
        f.git(&["show", "--format=", "--name-only", "HEAD"]).trim(),
        "other.txt"
    );
    assert_eq!(f.git(&["show", ":keep.txt"]), "ALREADY STAGED 中文\n");
    assert_eq!(f.git(&["show", "HEAD:keep.txt"]), "BASE\n");
    assert_eq!(
        fs::read_to_string(f.repo.join("keep.txt")).unwrap(),
        "ALREADY STAGED 中文\n"
    );
    assert!(f
        .view()
        .items
        .iter()
        .any(|i| i.path == "keep.txt" && !i.stale));
}
#[test]
fn partial_same_path_matches_observed_rebased_index() {
    let f = Fixture::new();
    let base = fs::read_to_string(f.repo.join("mixed.txt")).unwrap();
    let first = base.replace("line04: base", "line04: STAGED 中文");
    fs::write(f.repo.join("mixed.txt"), &first).unwrap();
    f.git(&["add", "mixed.txt"]);
    let both = first.replace("line30: base", "line30: SELECTED 中文");
    fs::write(f.repo.join("mixed.txt"), &both).unwrap();
    let v = f.view();
    assert_eq!(v.items.iter().filter(|i| i.path == "mixed.txt").count(), 2);
    let v = f.select(&v, "mixed.txt", Some(1));
    assert!(f.commit(&v).unwrap().success);
    assert!(f.git(&["show", "HEAD:mixed.txt"]).contains("line04: base"));
    assert!(f
        .git(&["show", "HEAD:mixed.txt"])
        .contains("line30: SELECTED 中文"));
    assert!(f.git(&["diff", "--cached"]).is_empty());
    assert!(f.git(&["diff"]).contains("line04: STAGED 中文"));
    assert_eq!(fs::read_to_string(f.repo.join("mixed.txt")).unwrap(), both);
}
#[test]
fn groups_moves_default_delete_and_restart_keep_unsubmitted() {
    let f = Fixture::new();
    f.staged_other();
    let v = f.change(
        &f.view(),
        ChangeAction::Create {
            name: "later".into(),
        },
    );
    let group = v
        .groups
        .iter()
        .find(|g| g.name == "later")
        .unwrap()
        .id
        .clone();
    let keep = v
        .items
        .iter()
        .find(|i| i.path == "keep.txt")
        .unwrap()
        .id
        .clone();
    let v = f.change(
        &v,
        ChangeAction::Move {
            items: vec![keep.clone()],
            group: group.clone(),
        },
    );
    let _v = f.change(
        &v,
        ChangeAction::Default {
            group: group.clone(),
        },
    );
    fs::write(f.repo.join("new.txt"), "new\n").unwrap();
    let v = f.view();
    assert!(v
        .items
        .iter()
        .any(|i| i.path == "new.txt" && i.group == group));
    let v = f.select(&v, "other.txt", None);
    assert!(f.commit(&v).unwrap().success);
    let restarted = ChangelistStore::new(&f.data).view(&f.session).unwrap();
    assert!(restarted
        .items
        .iter()
        .any(|i| i.id == keep && i.group == group));
    let v = f.change(&restarted, ChangeAction::Delete { group });
    assert_eq!(v.groups.len(), 1);
    assert!(v.items.iter().all(|i| i.group == v.default_group));
    let _ = v;
}
#[test]
fn changed_hunk_retains_group_and_cancels_stale_selection() {
    let f = Fixture::new();
    f.staged_other();
    let v = f.change(
        &f.view(),
        ChangeAction::Create {
            name: "original".into(),
        },
    );
    let group = v.groups.last().unwrap().id.clone();
    let ids = v
        .items
        .iter()
        .filter(|i| i.path == "other.txt")
        .map(|i| i.id.clone())
        .collect();
    let v = f.change(
        &v,
        ChangeAction::Move {
            items: ids,
            group: group.clone(),
        },
    );
    let selected = f.select(&v, "other.txt", None);
    let head = f.git(&["rev-parse", "HEAD"]);
    fs::write(f.repo.join("other.txt"), "external change\n").unwrap();
    assert!(f.commit(&selected).is_err());
    assert_eq!(f.git(&["rev-parse", "HEAD"]), head);
    let current = f.view();
    assert!(current
        .items
        .iter()
        .any(|i| i.group == group && i.stale && !i.selected));
    assert!(current.items.iter().all(|i| !i.selected));
}
#[test]
fn index_change_or_lock_rejects_before_commit_and_keeps_lock() {
    let f = Fixture::new();
    f.staged_other();
    let v = f.select(&f.view(), "other.txt", None);
    let head = f.git(&["rev-parse", "HEAD"]);
    f.git(&["add", "other.txt"]);
    assert!(f.commit(&v).is_err());
    assert_eq!(f.git(&["rev-parse", "HEAD"]), head);
    let v = f.select(&f.view(), "other.txt", None);
    let lock = f.repo.join(".git/index.lock");
    fs::write(&lock, "owned fixture lock").unwrap();
    assert!(f.commit(&v).is_err());
    assert_eq!(fs::read_to_string(lock).unwrap(), "owned fixture lock");
    assert_eq!(f.git(&["rev-parse", "HEAD"]), head);
}
#[test]
fn whole_binary_untracked_delete_are_exact_and_leave_worktree() {
    let f = Fixture::new();
    fs::write(f.repo.join("binary.dat"), [0, 1, 2, 255]).unwrap();
    fs::remove_file(f.repo.join("other.txt")).unwrap();
    let v = f.select(&f.view(), "binary.dat", None);
    let v = f.select(&v, "other.txt", None);
    assert!(f.commit(&v).unwrap().success);
    assert_eq!(
        fs::read(f.repo.join("binary.dat")).unwrap(),
        vec![0, 1, 2, 255]
    );
    assert!(!f.repo.join("other.txt").exists());
    assert!(f.git(&["diff", "--cached"]).is_empty());
    assert_eq!(
        f.git(&["ls-tree", "--name-only", "HEAD"])
            .lines()
            .collect::<Vec<_>>(),
        vec!["binary.dat", "keep.txt", "mixed.txt"]
    );
}
#[test]
fn metadata_read_and_switch_do_not_modify_index_or_worktree() {
    let f = Fixture::new();
    f.staged_other();
    let index = f.git(&["ls-files", "--stage"]);
    let cached = f.git(&["diff", "--cached", "--binary"]);
    let working = f.git(&["diff", "--binary"]);
    let v = f.view();
    let _v = f.change(
        &v,
        ChangeAction::Create {
            name: "keep across modes".into(),
        },
    );
    for _ in 0..3 {
        ChangelistStore::new(&f.data).view(&f.session).unwrap();
    }
    assert_eq!(f.git(&["ls-files", "--stage"]), index);
    assert_eq!(f.git(&["diff", "--cached", "--binary"]), cached);
    assert_eq!(f.git(&["diff", "--binary"]), working);
}
#[cfg(unix)]
#[test]
fn failed_hook_preserves_real_index_worktree_and_selection() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    f.staged_other();
    let hook = f.repo.join(".git/hooks/pre-commit");
    fs::write(&hook, "#!/bin/sh\necho owned-hook-failure >&2\nexit 1\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    let v = f.select(&f.view(), "other.txt", None);
    let before = f.git(&["ls-files", "--stage"]);
    let head = f.git(&["rev-parse", "HEAD"]);
    let result = f.commit(&v).unwrap();
    assert!(!result.success);
    assert!(result.output.contains("owned-hook-failure"));
    assert_eq!(f.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(f.git(&["ls-files", "--stage"]), before);
    assert!(f
        .view()
        .items
        .iter()
        .any(|i| i.path == "other.txt" && i.selected));
    assert!(fs::read_dir(f.data.join("changelists/commit-evidence"))
        .unwrap()
        .next()
        .is_some());
}

#[test]
fn initial_commit_uses_selected_untracked_content_only() {
    let f = Fixture::new();
    f.git(&["checkout", "--orphan", "unborn-fixture"]);
    f.git(&["rm", "--cached", "-r", "."]);
    let view = f.select(&f.view(), "other.txt", None);
    let outcome = f.commit(&view).unwrap();
    assert!(outcome.success);
    assert_eq!(
        f.git(&["ls-tree", "--name-only", "HEAD"]).trim(),
        "other.txt"
    );
    assert!(f.repo.join("keep.txt").exists());
    assert!(f.git(&["diff", "--cached"]).is_empty());
}
#[test]
fn replaced_repository_identity_does_not_inherit_changelists() {
    let f = Fixture::new();
    f.staged_other();
    let view = f.select(&f.view(), "other.txt", None);
    fs::rename(
        f.repo.join(".git"),
        f._temp.path().join("original-owned-git"),
    )
    .unwrap();
    f.git(&["init", "-b", "main"]);
    assert!(f.store.view(&f.session).is_err());
    assert!(f.commit(&view).is_err());
}
