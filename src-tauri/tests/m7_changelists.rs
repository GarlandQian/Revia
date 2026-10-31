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
    let v = f.change(
        &v,
        ChangeAction::Create {
            name: "different default".into(),
        },
    );
    let default = v
        .groups
        .iter()
        .find(|group| group.name == "different default")
        .unwrap()
        .id
        .clone();
    let v = f.change(&v, ChangeAction::Default { group: default });
    let remaining = v
        .items
        .iter()
        .find(|item| item.path == "mixed.txt" && item.hunk == Some(0))
        .unwrap()
        .clone();
    let v = f.select(&v, "mixed.txt", Some(1));
    assert!(f.commit(&v).unwrap().success);
    let restored = ChangelistStore::new(&f.data).view(&f.session).unwrap();
    let items: Vec<_> = restored
        .items
        .iter()
        .filter(|item| item.path == "mixed.txt")
        .collect();
    assert_eq!(
        items.len(),
        1,
        "own partial commit must not create a stale duplicate"
    );
    assert_eq!(items[0].group, remaining.group);
    assert!(!items[0].stale);
    assert!(!items[0].selected);
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

#[test]
fn adjacent_segments_share_hunk_but_can_move_and_commit_independently() {
    let f = Fixture::new();
    let base = fs::read_to_string(f.repo.join("mixed.txt")).unwrap();
    let staged = base.replace("line04: base", "line04: already staged");
    fs::write(f.repo.join("mixed.txt"), &staged).unwrap();
    f.git(&["add", "mixed.txt"]);
    let both = staged.replace("line08: base", "line08: selected second segment");
    fs::write(f.repo.join("mixed.txt"), &both).unwrap();
    let view = f.view();
    let parts: Vec<_> = view
        .items
        .iter()
        .filter(|i| i.path == "mixed.txt")
        .collect();
    assert_eq!(parts.len(), 2);
    assert!(parts.iter().all(|i| i.hunk == Some(0)));
    let second = parts
        .iter()
        .find(|i| i.segment == Some(1))
        .unwrap()
        .id
        .clone();
    let view = f.change(
        &view,
        ChangeAction::Create {
            name: "second".into(),
        },
    );
    let group = view.groups.last().unwrap().id.clone();
    let view = f.change(
        &view,
        ChangeAction::Move {
            items: vec![second.clone()],
            group,
        },
    );
    let view = f.change(
        &view,
        ChangeAction::Select {
            items: vec![second],
            selected: true,
        },
    );
    assert!(f.commit(&view).unwrap().success);
    let head = f.git(&["show", "HEAD:mixed.txt"]);
    assert!(head.contains("line04: base"));
    assert!(head.contains("line08: selected second segment"));
    assert!(f.git(&["diff", "--cached"]).is_empty());
    assert!(f.git(&["diff"]).contains("line04: already staged"));
    assert_eq!(fs::read_to_string(f.repo.join("mixed.txt")).unwrap(), both);
    let current = f.view();
    let remaining: Vec<_> = current
        .items
        .iter()
        .filter(|item| item.path == "mixed.txt")
        .collect();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].group, "default");
    assert!(!remaining[0].stale);
}
#[test]
fn two_of_three_segments_in_one_hunk_apply_together() {
    let f = Fixture::new();
    let text = fs::read_to_string(f.repo.join("mixed.txt"))
        .unwrap()
        .replace("line04: base", "line04: selected")
        .replace("line08: base", "line08: leave")
        .replace("line12: base", "line12: selected");
    fs::write(f.repo.join("mixed.txt"), text).unwrap();
    let view = f.view();
    let ids = view
        .items
        .iter()
        .filter(|i| i.path == "mixed.txt" && matches!(i.segment, Some(0) | Some(2)))
        .map(|i| i.id.clone())
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 2);
    let view = f.change(
        &view,
        ChangeAction::Select {
            items: ids,
            selected: true,
        },
    );
    assert!(f.commit(&view).unwrap().success);
    let head = f.git(&["show", "HEAD:mixed.txt"]);
    assert!(head.contains("line04: selected"));
    assert!(head.contains("line12: selected"));
    assert!(head.contains("line08: base"));
    assert!(f.git(&["diff"]).contains("line08: leave"));
}
#[test]
fn view_transmits_compact_summaries_instead_of_patch_bodies() {
    let f = Fixture::new();
    fs::write(
        f.repo.join("mixed.txt"),
        (0..2000)
            .map(|n| format!("replacement line {n}\n"))
            .collect::<String>(),
    )
    .unwrap();
    let view = f.view();
    assert!(view.items.iter().all(|i| i.summary.chars().count() <= 200));
    assert!(serde_json::to_vec(&view).unwrap().len() < 3000);
}
#[cfg(unix)]
#[test]
fn tracked_and_untracked_symlinks_are_readonly_before_any_index_write() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    symlink("keep.txt", f.repo.join("link")).unwrap();
    let view = f.view();
    let link = view.items.iter().find(|i| i.path == "link").unwrap();
    assert_eq!(
        link.readonly_reason,
        Some(revia_lib::git::status::ReadonlyReason::Symlink)
    );
    assert!(f
        .store
        .change(
            &f.session,
            &token(&view),
            ChangeAction::Select {
                items: vec![link.id.clone()],
                selected: true
            }
        )
        .is_err());
    f.git(&["add", "link"]);
    f.git(&["commit", "-m", "fixture symlink"]);
    fs::remove_file(f.repo.join("link")).unwrap();
    symlink("other.txt", f.repo.join("link")).unwrap();
    let view = f.view();
    let link = view
        .items
        .iter()
        .find(|i| i.path == "link" && !i.stale)
        .unwrap();
    assert!(link.readonly_reason.is_some());
    assert!(!link.selected);
}
impl Fixture {
    fn amend_message_only(&self, view: &ChangelistView) -> revia_lib::error::Result<CommitOutcome> {
        use revia_lib::commit_flow::AmendConfirmation;
        let drafts = DraftStore::new(self.data.clone());
        let context = self.session.commit_context(&drafts)?;
        let options = CommitOptions {
            message: "message only".into(),
            amend: true,
            skip_hooks: false,
            amend_confirmation: Some(AmendConfirmation {
                upstream_version: context.upstream_version,
                expectation_fingerprint: context.expectation.fingerprint.clone(),
                message: "message only".into(),
                amend: true,
                skip_hooks: false,
            }),
            expected: context.expectation,
            client_nonce: "amend-fixture".into(),
            draft_revision: String::new(),
        };
        self.session.execute_changelist_commit_flow(
            &self.store,
            &token(view),
            &drafts,
            &RecoveryStore::new(self.data.clone()),
            self.data.clone(),
            options,
            "fixture-amend".into(),
            Arc::new(|_| {}),
        )
    }
}
#[test]
fn amend_message_only_does_not_include_or_reset_unselected_staged_paths() {
    let f = Fixture::new();
    f.staged_other();
    let tree = f.git(&["rev-parse", "HEAD^{tree}"]);
    let entries = f.git(&["ls-files", "--stage"]);
    let view = f.view();
    assert!(f.amend_message_only(&view).unwrap().success);
    assert_eq!(f.git(&["rev-parse", "HEAD^{tree}"]), tree);
    assert_eq!(f.git(&["ls-files", "--stage"]), entries);
    assert_eq!(f.git(&["log", "-1", "--format=%B"]).trim(), "message only");
}
#[test]
fn empty_diff_merge_finalization_remains_available() {
    let f = Fixture::new();
    f.git(&["checkout", "-b", "side"]);
    f.git(&["commit", "--allow-empty", "-m", "fixture empty side"]);
    f.git(&["checkout", "main"]);
    f.git(&["merge", "--no-ff", "--no-commit", "side"]);
    let view = f.view();
    assert!(view.items.is_empty());
    let result = f.commit(&view).unwrap();
    assert!(result.success);
    assert_eq!(
        f.git(&["rev-list", "--parents", "-1", "HEAD"])
            .split_whitespace()
            .count(),
        3
    );
}
#[cfg(unix)]
#[test]
fn actual_rebase_edit_stop_allows_selection_and_message_amend() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    fs::write(f.repo.join("other.txt"), "second commit\n").unwrap();
    f.git(&["add", "other.txt"]);
    f.git(&["commit", "-m", "second"]);
    let editor = f._temp.path().join("owned-sequence-editor.sh");
    fs::write(
        &editor,
        "#!/bin/sh\nsed 's/^pick /edit /' \"$1\" >\"$1.tmp\"\ncat \"$1.tmp\" >\"$1\"\n",
    )
    .unwrap();
    fs::set_permissions(&editor, fs::Permissions::from_mode(0o755)).unwrap();
    f.git(&[
        "-c",
        &format!("sequence.editor={}", editor.display()),
        "rebase",
        "-i",
        "HEAD~1",
    ]);
    let view = f.view();
    assert!(f.amend_message_only(&view).unwrap().success);
    fs::write(f.repo.join("keep.txt"), "split at actual edit stop\n").unwrap();
    let view = f.select(&f.view(), "keep.txt", None);
    assert!(f.commit(&view).unwrap().success);
    f.git(&["rebase", "--continue"]);
    assert!(f.repo.join(".git/rebase-merge").exists() == false);
}

#[test]
fn real_local_submodule_is_readonly_in_changelists() {
    let f = Fixture::new();
    let module = f._temp.path().join("owned-module");
    fs::create_dir(&module).unwrap();
    git(&module, &f.home, &["init", "-b", "main"]);
    git(&module, &f.home, &["config", "user.name", "fixture"]);
    git(
        &module,
        &f.home,
        &["config", "user.email", "fixture@example.invalid"],
    );
    git(&module, &f.home, &["config", "commit.gpgsign", "false"]);
    fs::write(module.join("module.txt"), "base\n").unwrap();
    git(&module, &f.home, &["add", "."]);
    git(&module, &f.home, &["commit", "-m", "base"]);
    f.git(&[
        "-c",
        "protocol.file.allow=always",
        "submodule",
        "add",
        module.to_str().unwrap(),
        "module",
    ]);
    f.git(&["commit", "-m", "owned submodule"]);
    fs::write(f.repo.join("module/module.txt"), "dirty local submodule\n").unwrap();
    let before = f.git(&["ls-files", "--stage"]);
    let view = f.view();
    let item = view.items.iter().find(|i| i.path == "module").unwrap();
    assert_eq!(
        item.readonly_reason,
        Some(revia_lib::git::status::ReadonlyReason::Submodule)
    );
    assert!(f
        .store
        .change(
            &f.session,
            &token(&view),
            ChangeAction::Select {
                items: vec![item.id.clone()],
                selected: true
            }
        )
        .is_err());
    assert_eq!(f.git(&["ls-files", "--stage"]), before);
}

// macOS APFS拒绝创建非法UTF-8文件名；原始EILSEQ失败已保留，本例只在支持原字节路径的平台实跑。
#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn non_utf8_untracked_path_is_readonly_without_hiding_valid_files() {
    use std::os::unix::ffi::OsStringExt;
    let f = Fixture::new();
    f.staged_other();
    let name = std::ffi::OsString::from_vec(vec![b'b', 0xff, b'd']);
    fs::write(f.repo.join(name), b"owned invalid path").unwrap();
    let view = f.view();
    assert!(view
        .items
        .iter()
        .any(|item| item.readonly_reason
            == Some(revia_lib::git::status::ReadonlyReason::NonUtf8Path)));
    let selected = f.select(&view, "other.txt", None);
    assert!(f.commit(&selected).unwrap().success);
}
#[test]
fn changelist_preview_uses_head_to_worktree_after_index_intermediate_edit() {
    use revia_lib::{
        git::{
            rows::ViewMode,
            segments::{IgnoreWhitespace, WordGranularity},
            status::ChangesKind,
        },
        session::FileVersion,
    };
    let f = Fixture::new();
    let base = fs::read_to_string(f.repo.join("mixed.txt")).unwrap();
    fs::write(
        f.repo.join("mixed.txt"),
        base.replace("line04: base", "line04: INDEX_INTERMEDIATE_B"),
    )
    .unwrap();
    f.git(&["add", "mixed.txt"]);
    fs::write(
        f.repo.join("mixed.txt"),
        base.replace("line04: base", "line04: WORKTREE_C"),
    )
    .unwrap();
    f.session.set_changelist_mode(true);
    let snapshot = f.session.refresh().unwrap();
    assert!(snapshot
        .entries
        .iter()
        .any(|entry| entry.changes == ChangesKind::Staged));
    assert!(snapshot
        .entries
        .iter()
        .any(|entry| entry.changes == ChangesKind::Unstaged));
    let entry = snapshot
        .entries
        .iter()
        .find(|entry| entry.changes == ChangesKind::Changelist && entry.path == "mixed.txt")
        .unwrap();
    let rows = f
        .session
        .rows(
            "mixed.txt",
            ChangesKind::Changelist,
            ViewMode::Unified,
            0,
            100,
        )
        .unwrap();
    let text = rows
        .rows
        .iter()
        .filter_map(|row| row.text.as_deref())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("line04: base"));
    assert!(text.contains("line04: WORKTREE_C"));
    assert!(!text.contains("INDEX_INTERMEDIATE_B"));
    assert_eq!(rows.entry.fingerprint, entry.fingerprint);
    assert!(rows
        .rows
        .iter()
        .all(|row| row.hunk_action.is_none() && row.segment_action.is_none()));
    let old = f
        .session
        .side_lines(
            "mixed.txt",
            ChangesKind::Changelist,
            IgnoreWhitespace::Off,
            3,
            FileVersion::Old,
            0,
            40,
            WordGranularity::Word,
        )
        .unwrap();
    assert!(old.iter().any(|line| line.text == "line04: base"));
    assert!(!old
        .iter()
        .any(|line| line.text.contains("INDEX_INTERMEDIATE_B")));
    let view = f.select(&f.view(), "mixed.txt", None);
    assert!(view
        .items
        .iter()
        .filter(|item| !item.stale)
        .all(|item| item.source == Some(ChangesKind::Changelist)));
    assert!(f.commit(&view).unwrap().success);
    assert!(f
        .git(&["show", "HEAD:mixed.txt"])
        .contains("line04: WORKTREE_C"));
}
#[test]
fn repeated_segment_content_keeps_both_head_worktree_positions() {
    use revia_lib::git::{rows::ViewMode, status::ChangesKind};
    let f = Fixture::new();
    let base = fs::read_to_string(f.repo.join("mixed.txt"))
        .unwrap()
        .replace("line04: base", "same original")
        .replace("line30: base", "same original");
    fs::write(f.repo.join("mixed.txt"), &base).unwrap();
    f.git(&["add", "mixed.txt"]);
    f.git(&["commit", "-m", "repeated base"]);
    let staged = base.replacen("same original", "same replacement", 1);
    fs::write(f.repo.join("mixed.txt"), &staged).unwrap();
    f.git(&["add", "mixed.txt"]);
    fs::write(
        f.repo.join("mixed.txt"),
        staged.replace("same original", "same replacement"),
    )
    .unwrap();
    f.session.set_changelist_mode(true);
    f.session.refresh().unwrap();
    let view = f.view();
    assert_eq!(
        view.items
            .iter()
            .filter(|item| item.path == "mixed.txt" && !item.stale)
            .count(),
        2
    );
    assert!(view
        .items
        .iter()
        .all(|item| item.source == Some(ChangesKind::Changelist)));
    let rows = f
        .session
        .rows(
            "mixed.txt",
            ChangesKind::Changelist,
            ViewMode::Unified,
            0,
            100,
        )
        .unwrap();
    let positions = rows
        .rows
        .iter()
        .filter(|row| row.text.as_deref() == Some("same original"))
        .filter_map(|row| row.old_line)
        .collect::<Vec<_>>();
    assert_eq!(positions, vec![4, 30]);
}
#[test]
fn ordinary_mode_keeps_original_three_group_snapshot() {
    use revia_lib::git::status::ChangesKind;
    let f = Fixture::new();
    f.staged_other();
    let before = f.session.refresh().unwrap();
    assert!(before
        .entries
        .iter()
        .all(|entry| entry.changes != ChangesKind::Changelist));
    let index = f.git(&["ls-files", "--stage"]);
    f.session.set_changelist_mode(true);
    assert!(f
        .session
        .refresh()
        .unwrap()
        .entries
        .iter()
        .any(|entry| entry.changes == ChangesKind::Changelist));
    f.session.set_changelist_mode(false);
    let after = f.session.refresh().unwrap();
    assert!(after
        .entries
        .iter()
        .all(|entry| entry.changes != ChangesKind::Changelist));
    assert_eq!(f.git(&["ls-files", "--stage"]), index);
}

#[cfg(unix)]
#[test]
fn external_changelist_diff_receives_head_and_worktree_not_intermediate_index() {
    use revia_lib::{
        external_tools::{self, ExternalToolKind, ExternalToolRequest},
        git::status::ChangesKind,
    };
    let f = Fixture::new();
    let base = fs::read_to_string(f.repo.join("mixed.txt")).unwrap();
    fs::write(
        f.repo.join("mixed.txt"),
        base.replace("line04: base", "line04: INDEX_B"),
    )
    .unwrap();
    f.git(&["add", "mixed.txt"]);
    let worktree = base.replace("line04: base", "line04: WORKTREE_C");
    fs::write(f.repo.join("mixed.txt"), &worktree).unwrap();
    let left = f.data.join("captured-left");
    let right = f.data.join("captured-right");
    f.git(&[
        "config",
        "difftool.capture.cmd",
        "cat \"$LOCAL\" > \"$REVIA_CAPTURE_LEFT\"; cat \"$REMOTE\" > \"$REVIA_CAPTURE_RIGHT\"",
    ]);
    let mut env = git_environment::shell_env(&f.home, &f.repo, &f.home.join("gitconfig"));
    env.vars.insert(
        "REVIA_CAPTURE_LEFT".into(),
        left.to_string_lossy().into_owned(),
    );
    env.vars.insert(
        "REVIA_CAPTURE_RIGHT".into(),
        right.to_string_lossy().into_owned(),
    );
    let session = WorktreeSession::open(
        "external-changelist",
        &f.repo,
        env,
        Arc::new(CommandLog::default()),
    )
    .unwrap();
    session.set_changelist_mode(true);
    let snapshot = session.refresh().unwrap();
    let entry = snapshot
        .entries
        .iter()
        .find(|entry| entry.path == "mixed.txt" && entry.changes == ChangesKind::Changelist)
        .unwrap();
    let outcome = external_tools::launch(
        &session,
        &RecoveryStore::new(f.data.clone()),
        &ExternalToolRequest {
            expected_context: external_tools::confirmation_context(&session).unwrap(),
            kind: ExternalToolKind::Diff,
            path: "mixed.txt".into(),
            tool: Some("capture".into()),
            revision: None,
            changes_kind: Some(ChangesKind::Changelist),
            expected_fingerprint: Some(entry.fingerprint.clone()),
        },
        None,
    )
    .unwrap();
    assert_eq!(outcome.exit_code, Some(0));
    assert_eq!(fs::read_to_string(left).unwrap(), base);
    assert_eq!(fs::read_to_string(right).unwrap(), worktree);
    assert!(f.git(&["show", ":mixed.txt"]).contains("line04: INDEX_B"));
    assert_eq!(
        fs::read_to_string(f.repo.join("mixed.txt")).unwrap(),
        worktree
    );
}
#[cfg(unix)]
#[test]
fn stale_external_changelist_diff_is_rejected_before_tool_launch() {
    use revia_lib::{
        external_tools::{self, ExternalToolKind, ExternalToolRequest},
        git::status::ChangesKind,
    };
    let f = Fixture::new();
    f.staged_other();
    f.git(&["config", "difftool.capture.cmd", "echo owned-tool-started"]);
    f.session.set_changelist_mode(true);
    let snapshot = f.session.refresh().unwrap();
    let entry = snapshot
        .entries
        .iter()
        .find(|entry| entry.path == "other.txt" && entry.changes == ChangesKind::Changelist)
        .unwrap();
    let context = external_tools::confirmation_context(&f.session).unwrap();
    fs::write(f.repo.join("other.txt"), "external later version\n").unwrap();
    let error = external_tools::launch(
        &f.session,
        &RecoveryStore::new(f.data.clone()),
        &ExternalToolRequest {
            expected_context: context,
            kind: ExternalToolKind::Diff,
            path: "other.txt".into(),
            tool: Some("capture".into()),
            revision: None,
            changes_kind: Some(ChangesKind::Changelist),
            expected_fingerprint: Some(entry.fingerprint.clone()),
        },
        None,
    )
    .unwrap_err();
    assert_eq!(error.kind, revia_lib::error::ErrorKind::Rejected);
    assert!(f
        .session
        .git
        .log
        .snapshot()
        .iter()
        .all(|record| !record.args.iter().any(|arg| arg == "difftool")));
}

#[test]
fn partial_commit_keeps_remaining_group_when_old_line_numbers_shift() {
    let f = Fixture::new();
    let base = fs::read_to_string(f.repo.join("mixed.txt")).unwrap();
    let changed = base
        .replace("line04: base\n", "line04: selected\nnew selected line\n")
        .replace("line30: base", "line30: remains");
    fs::write(f.repo.join("mixed.txt"), &changed).unwrap();
    let view = f.change(
        &f.view(),
        ChangeAction::Create {
            name: "remaining".into(),
        },
    );
    let group = view
        .groups
        .iter()
        .find(|group| group.name == "remaining")
        .unwrap()
        .id
        .clone();
    let item = view
        .items
        .iter()
        .find(|item| item.path == "mixed.txt" && item.hunk == Some(1))
        .unwrap()
        .id
        .clone();
    let view = f.change(
        &view,
        ChangeAction::Move {
            items: vec![item],
            group: group.clone(),
        },
    );
    let view = f.select(&view, "mixed.txt", Some(0));
    assert!(f.commit(&view).unwrap().success);
    let current = f.view();
    let residual: Vec<_> = current
        .items
        .iter()
        .filter(|item| item.path == "mixed.txt")
        .collect();
    assert_eq!(residual.len(), 1);
    assert_eq!(residual[0].group, group);
    assert!(!residual[0].stale);
    assert!(!residual[0].selected);
    assert_eq!(
        fs::read_to_string(f.repo.join("mixed.txt")).unwrap(),
        changed
    );
}
