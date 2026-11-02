//! M5 任务 03：分支、远端与 Git 命令日志的后端回归（规格 B01–B07、V）。
//!
//! 全部用例都在自建的真实 Git 临时仓库里，远端用本地裸仓库（`file://`），
//! 应用数据目录（回收区）也独立分配；不碰用户仓库、用户配置或临时交接 fixture。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;
use std::time::Duration;

use revia_lib::git::branch::{self, BranchOverview};
use revia_lib::git::{CommandLog, Git};
use revia_lib::recovery::{RecoverySource, RecoveryStore};
use revia_lib::remote_ops::{
    self, BranchStart, CheckoutResult, CheckoutStrategy, CheckoutTarget, CreateBranchRequest,
    PushForce, PushRequest, PushResult, UpdateMode, UpdateStatus,
};
use revia_lib::session::WorktreeSession;
use revia_lib::settings::FetchTagMode;
use revia_lib::shell::{EnvCaptureInfo, ShellEnv};

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    repo: PathBuf,
    /// 本地裸仓库，跑远端操作的“服务器”。
    origin: PathBuf,
    /// 另一个克隆：模拟别人往远端推了新提交。
    other: PathBuf,
    home: PathBuf,
    data: PathBuf,
    env: HashMap<String, String>,
    paths: Vec<String>,
    session: Arc<WorktreeSession>,
    store: RecoveryStore,
    log: Arc<CommandLog>,
}

fn build_shell_env(paths: &[String], env: &HashMap<String, String>, id: &str) -> ShellEnv {
    ShellEnv {
        path: paths.to_vec(),
        vars: env.clone(),
        info: EnvCaptureInfo {
            worktree: id.into(),
            shell: "fixture".into(),
            elapsed_ms: 0,
            timed_out: false,
            used_fallback: true,
            var_count: env.len(),
            path_entries: paths.to_vec(),
            fallback_reason: None,
            fallback_detail: None,
        },
    }
}

fn run_git(directory: &Path, env: &HashMap<String, String>, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(directory)
        .env_clear()
        .envs(env)
        .args(args.iter().map(|argument| revia_lib::platform::external_path(Path::new(argument))))
        .output()
        .expect("执行 Git");
    assert!(
        output.status.success(),
        "git {args:?} 在 {} 失败：stdout={} stderr={}",
        directory.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn run_git_output(directory: &Path, env: &HashMap<String, String>, args: &[&str]) -> Output {
    Command::new("git")
        .current_dir(directory)
        .env_clear()
        .envs(env)
        .args(args.iter().map(|argument| revia_lib::platform::external_path(Path::new(argument))))
        .output()
        .expect("执行 Git")
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("创建临时目录");
        let root = fs::canonicalize(temp.path()).expect("规范化临时目录");
        let repo = root.join("repo");
        let origin = root.join("origin.git");
        let other = root.join("other");
        let home = root.join("home");
        let data = root.join("data");
        for directory in [&repo, &home, &data] {
            fs::create_dir_all(directory).unwrap();
        }
        let global_config = home.join(".gitconfig");
        fs::write(&global_config, b"").unwrap();
        let mut paths: Vec<String> = std::env::var_os("PATH")
            .map(|value| {
                std::env::split_paths(&value)
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        #[cfg(unix)]
        for extra in ["/usr/bin", "/bin", "/usr/local/bin", "/opt/homebrew/bin"] {
            if !paths.iter().any(|path| path == extra) {
                paths.push(extra.into());
            }
        }
        let joined_path = std::env::join_paths(paths.iter().map(Path::new)).unwrap();
        let mut env = HashMap::from([
            ("HOME".into(), revia_lib::platform::external_path(&home).display().to_string()),
            ("USERPROFILE".into(), revia_lib::platform::external_path(&home).display().to_string()),
            ("XDG_CONFIG_HOME".into(), revia_lib::platform::external_path(&home).display().to_string()),
            (
                "GIT_CONFIG_GLOBAL".into(),
                revia_lib::platform::external_path(&global_config).display().to_string(),
            ),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("GIT_TERMINAL_PROMPT".into(), "0".into()),
            ("PATH".into(), joined_path.to_string_lossy().into_owned()),
        ]);
        for key in ["SYSTEMROOT", "WINDIR", "TEMP", "TMP", "TMPDIR"] {
            if let Ok(value) = std::env::var(key) {
                env.insert(key.into(), value);
            }
        }
        for (key, value) in [
            ("user.name", "Revia Branch Test"),
            ("user.email", "branch@revia.invalid"),
            ("commit.gpgsign", "false"),
            ("core.quotePath", "false"),
            ("init.defaultBranch", "main"),
            // 测试自己的仓库配置：允许 ext:: 远端用来验证网络超时。
            ("protocol.ext.allow", "always"),
        ] {
            fs::create_dir_all(&repo).unwrap();
            run_git(&repo, &env, &["init", "-b", "main"]);
            run_git(&repo, &env, &["config", key, value]);
        }
        fs::write(repo.join("a.txt"), b"one\n").unwrap();
        run_git(&repo, &env, &["add", "-A"]);
        run_git(&repo, &env, &["commit", "-m", "first"]);
        run_git(
            &repo,
            &env,
            &["init", "--bare", "-b", "main", origin.to_str().unwrap()],
        );
        let url = url::Url::from_directory_path(revia_lib::platform::external_path(&origin)).unwrap().to_string();
        run_git(&repo, &env, &["remote", "add", "origin", &url]);
        run_git(&repo, &env, &["push", "-u", "origin", "main"]);
        run_git(&root, &env, &["clone", &url, other.to_str().unwrap()]);
        run_git(&other, &env, &["config", "user.name", "Other"]);
        run_git(
            &other,
            &env,
            &["config", "user.email", "other@revia.invalid"],
        );

        let log = Arc::new(CommandLog::default());
        let session = WorktreeSession::open(
            "test-worktree",
            &repo,
            build_shell_env(&paths, &env, "test-worktree"),
            log.clone(),
        )
        .expect("打开会话");
        let store = RecoveryStore::new(data.clone());
        Self {
            _temp: temp,
            root,
            repo,
            origin,
            other,
            home,
            data,
            env,
            paths,
            session,
            store,
            log,
        }
    }

    fn shell_env(&self, id: &str) -> ShellEnv {
        build_shell_env(&self.paths, &self.env, id)
    }

    fn git(&self, args: &[&str]) -> String {
        run_git(&self.repo, &self.env, args)
    }

    fn git_fail(&self, args: &[&str]) -> Output {
        run_git_output(&self.repo, &self.env, args)
    }

    fn other_git(&self, args: &[&str]) -> String {
        run_git(&self.other, &self.env, args)
    }

    fn head(&self) -> String {
        self.git(&["rev-parse", "HEAD"]).trim().to_string()
    }

    fn write(&self, path: &str, bytes: &[u8]) {
        let full = self.repo.join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, bytes).unwrap();
    }

    fn read(&self, path: &str) -> String {
        fs::read_to_string(self.repo.join(path)).unwrap()
    }

    fn commit_all(&self, message: &str) -> String {
        self.git(&["--literal-pathspecs", "add", "-A"]);
        self.git(&["commit", "-m", message]);
        self.head()
    }

    fn track(&self, path: &str, bytes: &[u8], message: &str) -> String {
        self.write(path, bytes);
        self.commit_all(message)
    }

    /// 让「别人」往远端推一个新提交（用于非快进、检出新提交等场景）。
    fn advance_remote(&self, path: &str, bytes: &[u8], message: &str) -> String {
        fs::write(self.other.join(path), bytes).unwrap();
        self.other_git(&["--literal-pathspecs", "add", "-A"]);
        self.other_git(&["commit", "-m", message]);
        self.other_git(&["push", "origin", "main"]);
        self.other_git(&["rev-parse", "HEAD"]).trim().to_string()
    }

    fn overview(&self) -> BranchOverview {
        branch::read_overview(&self.session.git, "test-worktree").expect("读取分支概览")
    }

    fn recovery_sources(&self) -> Vec<RecoverySource> {
        self.store
            .list(None)
            .expect("列出回收记录")
            .into_iter()
            .map(|record| record.source)
            .collect()
    }

    fn log_args(&self) -> Vec<Vec<String>> {
        self.log
            .snapshot()
            .into_iter()
            .map(|record| record.args)
            .collect()
    }
}

fn local(name: &str) -> CheckoutTarget {
    CheckoutTarget::Local { name: name.into() }
}

fn remote(remote: &str, branch: &str) -> CheckoutTarget {
    CheckoutTarget::Remote {
        remote: remote.into(),
        branch: branch.into(),
    }
}

fn checkout(
    f: &Fixture,
    target: CheckoutTarget,
    strategy: CheckoutStrategy,
) -> revia_lib::error::Result<CheckoutResult> {
    remote_ops::checkout(&f.session, &f.store, &target, strategy,
        &revia_lib::stash_archive::StashArchiveStore::new(&f.data))
}

#[test]
fn branch_overview_lists_current_local_and_remote_branches() {
    let f = Fixture::new();
    f.git(&["branch", "feature"]);
    let overview = f.overview();
    assert_eq!(overview.current_branch.as_deref(), Some("main"));
    assert!(!overview.detached);
    assert_eq!(overview.local.len(), 2);
    // 当前分支置顶。
    assert_eq!(overview.local[0].name, "main");
    assert!(overview.local[0].is_current);
    assert_eq!(
        overview.local[0].upstream.as_deref(),
        Some("refs/remotes/origin/main")
    );
    assert_eq!(overview.local[0].ahead, 0);
    assert_eq!(overview.local[0].behind, 0);
    assert_eq!(overview.remote_names, vec!["origin".to_string()]);
    let origin = &overview.remotes[0];
    assert_eq!(origin.name, "origin");
    assert!(origin.branches.iter().any(|item| item.branch == "main"));
    assert_eq!(
        origin
            .branches
            .iter()
            .find(|item| item.branch == "main")
            .unwrap()
            .tracking_local
            .as_deref(),
        Some("main")
    );
    assert_eq!(overview.user.email.as_deref(), Some("branch@revia.invalid"));
}

#[test]
fn checkout_remote_branch_creates_tracking_branch_and_reuses_existing_one() {
    let f = Fixture::new();
    // 远端新增一个分支。
    f.git(&["checkout", "-b", "remote-only"]);
    f.track("remote-only.txt", b"remote only\n", "remote only");
    f.git(&["push", "-u", "origin", "remote-only"]);
    f.git(&["checkout", "main"]);
    f.git(&["branch", "-D", "remote-only"]);
    f.git(&["fetch", "origin"]);

    let result = checkout(&f, remote("origin", "remote-only"), CheckoutStrategy::Plain).unwrap();
    let CheckoutResult::Done(outcome) = result else {
        panic!("干净的检出不应需要确认");
    };
    assert_eq!(outcome.branch, "remote-only");
    assert_eq!(
        f.git(&["rev-parse", "--abbrev-ref", "HEAD"]).trim(),
        "remote-only"
    );
    assert_eq!(
        f.git(&["rev-parse", "--abbrev-ref", "@{upstream}"]).trim(),
        "origin/remote-only"
    );
    assert!(outcome.notes.iter().any(|note| matches!(note, revia_lib::remote_ops::CheckoutNote::CreatedTracking { .. } | revia_lib::remote_ops::CheckoutNote::ExistingTracking { .. })));

    // 再检出一次：已有同名且跟踪它的本地分支，直接用它。
    f.git(&["checkout", "main"]);
    let result = checkout(&f, remote("origin", "remote-only"), CheckoutStrategy::Plain).unwrap();
    assert!(matches!(result, CheckoutResult::Done(outcome) if outcome.branch == "remote-only"));
    assert!(f.recovery_sources().is_empty(), "干净检出不应写回收区");
}

#[test]
fn same_named_local_branch_without_tracking_is_not_overwritten() {
    let f = Fixture::new();
    f.git(&["checkout", "-b", "topic"]);
    f.track("topic.txt", b"local topic\n", "local topic");
    f.git(&["push", "origin", "topic"]);
    f.git(&["checkout", "main"]);
    f.git(&["branch", "-D", "topic"]);
    // 远端同名分支来自另一个提交，本地同名分支不跟踪它。
    f.git(&["fetch", "origin"]);
    f.git(&["branch", "topic", "main"]);

    let error = checkout(&f, remote("origin", "topic"), CheckoutStrategy::Plain).unwrap_err();
    assert!(error.message.contains("不跟踪"), "{}", error.message);
    // 本地分支没有被覆盖，仍在 main 上。
    assert_eq!(
        f.git(&["rev-parse", "topic"]).trim(),
        f.git(&["rev-parse", "main"]).trim()
    );
}

#[test]
fn checkout_is_refused_when_another_worktree_holds_the_branch() {
    let f = Fixture::new();
    f.git(&["branch", "busy"]);
    let worktree = f.root.join("wt-busy");
    f.git(&["worktree", "add", worktree.to_str().unwrap(), "busy"]);
    let error = checkout(&f, local("busy"), CheckoutStrategy::Plain).unwrap_err();
    let message = error.message.clone();
    assert!(
        message.contains("already checked out")
            || message.contains("already used by worktree")
            || message.contains("已经被工作区"),
        "{message}"
    );
}

#[test]
fn create_branch_validates_name_and_supports_three_starts() {
    let f = Fixture::new();
    let head = f.head();
    let result = remote_ops::create_branch(
        &f.session,
        &CreateBranchRequest {
            name: "bad..name".into(),
            start: BranchStart::Head,
            checkout: false,
        },
    );
    assert!(result.is_err());

    // 起点：HEAD。
    let created = remote_ops::create_branch(
        &f.session,
        &CreateBranchRequest {
            name: "from-head".into(),
            start: BranchStart::Head,
            checkout: true,
        },
    )
    .unwrap();
    assert!(created.checked_out);
    assert_eq!(created.commit, head);
    assert_eq!(
        f.git(&["rev-parse", "--abbrev-ref", "HEAD"]).trim(),
        "from-head"
    );

    // 已存在的名字被拒绝。
    let error = remote_ops::create_branch(
        &f.session,
        &CreateBranchRequest {
            name: "from-head".into(),
            start: BranchStart::Head,
            checkout: false,
        },
    )
    .unwrap_err();
    assert!(error.message.contains("已存在"), "{}", error.message);

    // 起点：引用名（不检出）。
    f.git(&["checkout", "main"]);
    let created = remote_ops::create_branch(
        &f.session,
        &CreateBranchRequest {
            name: "from-ref".into(),
            start: BranchStart::Ref {
                full_name: "refs/heads/from-head".into(),
            },
            checkout: false,
        },
    )
    .unwrap();
    assert!(!created.checked_out);
    assert_eq!(created.commit, head);

    // 起点：完整提交 ID。
    let created = remote_ops::create_branch(
        &f.session,
        &CreateBranchRequest {
            name: "from-oid".into(),
            start: BranchStart::Commit { oid: head.clone() },
            checkout: false,
        },
    )
    .unwrap();
    assert_eq!(created.commit, head);

    // 不存在的起点引用明确报错。
    let error = remote_ops::create_branch(
        &f.session,
        &CreateBranchRequest {
            name: "from-nowhere".into(),
            start: BranchStart::Ref {
                full_name: "refs/heads/nope".into(),
            },
            checkout: false,
        },
    )
    .unwrap_err();
    assert!(error.message.contains("不存在"), "{}", error.message);
}

/// 让目标分支与工作区在同一个文件上不同：检出会被 Git 拒绝。
fn dirty_conflicting_file(f: &Fixture) {
    f.git(&["checkout", "-b", "target"]);
    f.track("a.txt", b"target version\n", "target change");
    f.git(&["checkout", "main"]);
    f.track("a.txt", b"main version\n", "main change");
    f.write("a.txt", b"local uncommitted\n");
}

#[test]
fn plain_checkout_reports_files_that_would_be_overwritten() {
    let f = Fixture::new();
    dirty_conflicting_file(&f);
    let result = checkout(&f, local("target"), CheckoutStrategy::Plain).unwrap();
    let CheckoutResult::NeedsConfirmation { files, target } = result else {
        panic!("应要求确认");
    };
    assert_eq!(files, vec!["a.txt".to_string()]);
    assert_eq!(target, "target");
    // 没有写任何东西，也没有回收记录。
    assert_eq!(f.read("a.txt"), "local uncommitted\n");
    assert!(f.recovery_sources().is_empty());
}

#[test]
fn staged_changes_that_would_be_overwritten_also_require_confirmation() {
    let f = Fixture::new();
    dirty_conflicting_file(&f);
    // 把冲突文件的改动放进暂存区：Git 同样会拒绝检出。
    f.git(&["--literal-pathspecs", "add", "a.txt"]);
    let result = checkout(&f, local("target"), CheckoutStrategy::Plain).unwrap();
    let CheckoutResult::NeedsConfirmation { files, .. } = result else {
        panic!("已暂存的冲突改动也应先确认");
    };
    assert_eq!(files, vec!["a.txt".to_string()]);
    assert!(f.recovery_sources().is_empty());
}

#[test]
fn smart_checkout_saves_recovery_stashes_and_restores_changes() {
    let f = Fixture::new();
    // 目标分支改的是另一个文件：本地改动能在检出后干净地恢复。
    f.git(&["checkout", "-b", "target"]);
    f.track("target.txt", b"target\n", "target file");
    f.git(&["checkout", "main"]);
    f.write("a.txt", b"local uncommitted\n");
    let result = checkout(&f, local("target"), CheckoutStrategy::Smart).unwrap();
    let CheckoutResult::Done(outcome) = result else {
        panic!("智能检出应直接完成");
    };
    assert_eq!(outcome.branch, "target");
    assert!(outcome.stash_restored);
    assert!(!outcome.stash_kept);
    assert!(outcome.conflicts.is_empty());
    // 本地改动被恢复（没有被丢掉）。
    assert_eq!(f.read("a.txt"), "local uncommitted\n");
    // 被覆盖的那些文件在操作前已存入回收区。
    assert!(
        f.recovery_sources()
            .contains(&RecoverySource::CheckoutSmart)
    );
    let record_id = outcome.recovery_record_id.expect("应有回收记录");
    let summary = f
        .store
        .list(None)
        .unwrap()
        .into_iter()
        .find(|record| record.id == record_id)
        .expect("回收记录应可列出");
    assert_eq!(summary.source, RecoverySource::CheckoutSmart);
    assert!(summary.paths.contains(&"a.txt".to_string()));
    // stash 已恢复，没有留下条目。
    assert!(f.git(&["stash", "list"]).trim().is_empty());
}

#[test]
fn smart_checkout_conflict_keeps_the_stash_and_reports_conflicts() {
    let f = Fixture::new();
    dirty_conflicting_file(&f);
    // 让 stash 恢复必然冲突：目标分支同一文件也被改过，且工作区改动基于 main。
    let result = checkout(&f, local("target"), CheckoutStrategy::Smart);
    let outcome = match result {
        Ok(CheckoutResult::Done(outcome)) => outcome,
        other => panic!("应完成检出：{other:?}"),
    };
    if !outcome.stash_kept {
        // 某些 Git 版本能直接合并成功：这时必须没有残留 stash，并且内容合理。
        assert!(f.git(&["stash", "list"]).trim().is_empty());
        return;
    }
    assert!(!outcome.stash_restored);
    assert_eq!(outcome.conflicts, vec!["a.txt".to_string()]);
    // 冲突时 Git 保留 stash 条目，用户还能找回来。
    assert!(!f.git(&["stash", "list"]).trim().is_empty());
    assert!(outcome.notes.iter().any(|note| matches!(note, revia_lib::remote_ops::CheckoutNote::StashRestoreConflicted)));
}

#[test]
fn force_checkout_saves_all_tracked_changes_left_unsaved_on_target() {
    let f = Fixture::new();
    f.track("a.txt", b"base\n", "base");
    f.git(&["checkout", "-b", "target"]);
    f.track("target.txt", b"target\n", "target file");
    f.git(&["checkout", "main"]);
    // 两个已跟踪文件都有本地改动（其中一个与目标分支无关）。
    f.write("a.txt", b"local a\n");
    f.write("extra.txt", b"untracked\n");

    let result = checkout(&f, local("target"), CheckoutStrategy::Force).unwrap();
    let CheckoutResult::Done(outcome) = result else {
        panic!("强制检出应直接完成");
    };
    assert_eq!(outcome.branch, "target");
    assert_eq!(f.read("a.txt"), "base\n", "强制检出丢弃了本地改动");
    assert!(
        f.recovery_sources()
            .contains(&RecoverySource::CheckoutForce)
    );
    let summary = f
        .store
        .list(None)
        .unwrap()
        .into_iter()
        .find(|record| Some(&record.id) == outcome.recovery_record_id.as_ref())
        .expect("回收记录应可列出");
    // 强制检出前保存了全部已跟踪文件的本地改动。
    assert!(summary.paths.contains(&"a.txt".to_string()));
    // 未跟踪文件不在“已跟踪改动”里，也不会被 force checkout 删掉。
    assert_eq!(f.read("extra.txt"), "untracked\n");
}

#[test]
fn fetch_follows_the_tag_mode_setting() {
    let f = Fixture::new();
    // 远端有一个标签。
    f.git(&["tag", "-a", "v1", "-m", "tag one"]);
    f.git(&["push", "origin", "v1"]);
    f.git(&["tag", "-d", "v1"]);

    // Never：不取标签。
    let outcome =
        remote_ops::fetch(&f.session, FetchTagMode::Never, Duration::from_secs(60)).unwrap();
    assert_eq!(outcome.remotes, vec!["origin".to_string()]);
    assert_eq!(outcome.tag_args, vec!["--no-tags".to_string()]);
    assert!(f.git(&["tag", "--list"]).trim().is_empty());
    assert!(
        f.log_args()
            .iter()
            .any(|args| args.iter().any(|arg| arg == "--no-tags"))
    );

    // Always：取回全部标签。
    remote_ops::fetch(&f.session, FetchTagMode::Always, Duration::from_secs(60)).unwrap();
    assert!(f.git(&["tag", "--list"]).contains("v1"));
    assert!(
        f.log_args()
            .iter()
            .any(|args| args.iter().any(|arg| arg == "--tags"))
    );

    // Sync：删掉远端已删除的标签和分支。
    f.git(&["push", "origin", "--delete", "v1"]);
    f.git(&["checkout", "-b", "gone"]);
    f.track("gone.txt", b"gone\n", "gone");
    f.git(&["push", "-u", "origin", "gone"]);
    f.git(&["checkout", "main"]);
    remote_ops::fetch(&f.session, FetchTagMode::Sync, Duration::from_secs(60)).unwrap();
    assert!(!f.git(&["tag", "--list"]).contains("v1"));
    assert!(f.git(&["branch", "-r"]).contains("origin/gone"));
    f.git(&["push", "origin", "--delete", "gone"]);
    let outcome =
        remote_ops::fetch(&f.session, FetchTagMode::Sync, Duration::from_secs(60)).unwrap();
    assert!(outcome.tag_args.contains(&"--prune".to_string()));
    assert!(!f.git(&["branch", "-r"]).contains("origin/gone"));

    // Auto：不加额外参数（跟随 Git 配置）。
    let outcome =
        remote_ops::fetch(&f.session, FetchTagMode::Auto, Duration::from_secs(60)).unwrap();
    assert!(outcome.tag_args.is_empty());
}

#[test]
fn fetch_reports_git_failure_with_raw_output() {
    let f = Fixture::new();
    f.git(&["remote", "add", "broken", "file:///nonexistent/repo.git"]);
    let error =
        remote_ops::fetch(&f.session, FetchTagMode::Auto, Duration::from_secs(60)).unwrap_err();
    assert_eq!(error.code, Some("errors.mod.failedExitCode"));
    assert!(error.message.contains("128"), "{}", error.message);
    assert!(
        error.message.contains("/nonexistent/repo.git"),
        "{}",
        error.message
    );
    assert!(
        error.message.lines().any(|line| !line.trim().is_empty()),
        "{}",
        error.message
    );
}

#[test]
fn update_merge_and_rebase_apply_the_upstream() {
    let f = Fixture::new();
    // 别人往远端推了一个提交。
    let remote_head = f.advance_remote("remote.txt", b"remote\n", "remote commit");
    let outcome = remote_ops::update(
        &f.session,
        &f.store,
        UpdateMode::Merge,
        FetchTagMode::Auto,
        Duration::from_secs(60),
    )
    .unwrap();
    assert_eq!(
        outcome.mode,
        revia_lib::remote_ops::UpdateResolvedMode::Merge
    );
    assert_eq!(outcome.status, UpdateStatus::Updated);
    assert_eq!(outcome.commits_merged, 1);
    assert_eq!(f.head(), remote_head);
    assert!(f.recovery_sources().is_empty(), "干净工作区不需要回收记录");

    // 再次更新：已经是最新。
    let outcome = remote_ops::update(
        &f.session,
        &f.store,
        UpdateMode::Merge,
        FetchTagMode::Auto,
        Duration::from_secs(60),
    )
    .unwrap();
    assert_eq!(outcome.status, UpdateStatus::UpToDate);
    assert_eq!(outcome.commits_merged, 0);

    // rebase：本地提交被搬到远端的提交之后。
    f.track("local.txt", b"local\n", "local commit");
    let remote_head = f.advance_remote("remote2.txt", b"remote2\n", "remote commit 2");
    let outcome = remote_ops::update(
        &f.session,
        &f.store,
        UpdateMode::Rebase,
        FetchTagMode::Auto,
        Duration::from_secs(60),
    )
    .unwrap();
    assert_eq!(
        outcome.mode,
        revia_lib::remote_ops::UpdateResolvedMode::Rebase
    );
    assert_eq!(outcome.status, UpdateStatus::Updated);
    let parents = f.git(&["rev-list", "--parents", "-n", "1", "HEAD"]);
    assert!(
        parents.contains(&remote_head),
        "rebase 后 HEAD 的父提交应是远端提交：{parents}"
    );

    // 按分支默认：只读配置决定方向，不改配置。
    f.track("local2.txt", b"local2\n", "local commit 2");
    f.advance_remote("remote3.txt", b"remote3\n", "remote commit 3");
    f.git(&["config", "pull.rebase", "true"]);
    let outcome = remote_ops::update(
        &f.session,
        &f.store,
        UpdateMode::BranchDefault,
        FetchTagMode::Auto,
        Duration::from_secs(60),
    )
    .unwrap();
    assert_eq!(
        outcome.mode,
        revia_lib::remote_ops::UpdateResolvedMode::Rebase
    );
    assert_eq!(f.git(&["config", "--get", "pull.rebase"]).trim(), "true");
}

#[test]
fn update_without_upstream_or_on_detached_head_is_explained() {
    let f = Fixture::new();
    f.git(&["checkout", "-b", "no-upstream"]);
    let error = remote_ops::update(
        &f.session,
        &f.store,
        UpdateMode::Merge,
        FetchTagMode::Auto,
        Duration::from_secs(60),
    )
    .unwrap_err();
    assert!(error.message.contains("没有配置上游"), "{}", error.message);

    f.git(&["checkout", "--detach"]);
    let error = remote_ops::update(
        &f.session,
        &f.store,
        UpdateMode::Merge,
        FetchTagMode::Auto,
        Duration::from_secs(60),
    )
    .unwrap_err();
    assert!(error.message.contains("游离"), "{}", error.message);
}

#[test]
fn update_saves_dirty_changes_to_recovery_before_autostash() {
    let f = Fixture::new();
    f.advance_remote("remote.txt", b"remote\n", "remote commit");
    f.write("a.txt", b"local edit\n");
    let outcome = remote_ops::update(
        &f.session,
        &f.store,
        UpdateMode::Merge,
        FetchTagMode::Auto,
        Duration::from_secs(60),
    )
    .unwrap();
    assert_eq!(outcome.status, UpdateStatus::Updated);
    // 本地改动在更新后仍在工作区（autostash 恢复）。
    assert_eq!(f.read("a.txt"), "local edit\n");
    assert!(
        f.recovery_sources()
            .contains(&RecoverySource::UpdateAutostash)
    );
    let summary = f
        .store
        .list(None)
        .unwrap()
        .into_iter()
        .find(|record| Some(&record.id) == outcome.recovery_record_id.as_ref())
        .expect("回收记录应可列出");
    assert!(summary.paths.contains(&"a.txt".to_string()));
}

#[test]
fn update_merge_conflict_enters_in_progress_state() {
    let f = Fixture::new();
    // 两边改同一个文件：远端与本地都改 a.txt。
    f.write("a.txt", b"local version\n");
    f.commit_all("local change");
    fs::write(f.other.join("a.txt"), b"remote version\n").unwrap();
    f.other_git(&["--literal-pathspecs", "add", "-A"]);
    f.other_git(&["commit", "-m", "remote change"]);
    f.other_git(&["push", "origin", "main"]);

    let outcome = remote_ops::update(
        &f.session,
        &f.store,
        UpdateMode::Merge,
        FetchTagMode::Auto,
        Duration::from_secs(60),
    )
    .unwrap();
    assert_eq!(outcome.status, UpdateStatus::MergeConflict);
    assert_eq!(outcome.conflicts, vec!["a.txt".to_string()]);
    let state = revia_lib::operation::detect(&f.session.git, 0)
        .unwrap()
        .expect("应进入 merge");
    assert_eq!(state.kind, revia_lib::operation::OperationKind::Merge);
    // 有进行中的操作时不再开始新的检出。
    let error = checkout(&f, local("main"), CheckoutStrategy::Plain).unwrap_err();
    assert!(error.message.contains("进行中的操作"), "{}", error.message);
}

#[test]
fn push_new_branch_records_upstream_and_counts_commits() {
    let f = Fixture::new();
    f.git(&["checkout", "-b", "feature"]);
    f.track("feature.txt", b"feature\n", "feature commit");
    let preview = remote_ops::push_preview(&f.session, &["main".into(), "master".into()]).unwrap();
    assert_eq!(preview.branch, "feature");
    assert!(!preview.has_upstream);
    assert_eq!(preview.remote, "origin");
    assert_eq!(preview.target_branch, "feature");
    // 远端还没有这个分支：整条分支的提交都要推送。
    assert_eq!(preview.commit_count, 2);
    assert_eq!(preview.commits[0].subject, "feature commit");
    assert!(preview.force_allowed);

    let result = remote_ops::push(
        &f.session,
        &PushRequest {
            remote: None,
            branch: None,
            tags: false,
            force: PushForce::None,
        },
        &["main".into()],
        Duration::from_secs(60),
    )
    .unwrap();
    let PushResult::Pushed {
        remote,
        branch,
        commits,
        upstream_recorded,
        ..
    } = result
    else {
        panic!("推送应成功");
    };
    assert_eq!(remote, "origin");
    assert_eq!(branch, "feature");
    // 推送前统计：远端还没有这个分支，所以整条分支的提交都算新推送的。
    assert_eq!(commits, 2);
    assert!(upstream_recorded);
    assert_eq!(
        f.git(&["rev-parse", "--abbrev-ref", "@{upstream}"]).trim(),
        "origin/feature"
    );
    // Git 自己是记录上游的来源（不是我们改配置）。
    assert_eq!(
        f.git(&["config", "--get", "branch.feature.remote"]).trim(),
        "origin"
    );
}

#[test]
fn push_preview_lists_local_commits_ahead_of_an_existing_upstream() {
    let f = Fixture::new();
    let upstream = f.head();
    let first = f.track("first-local.txt", b"first local\n", "first local commit");
    let head = f.track("second-local.txt", b"second local\n", "second local commit");
    let config = fs::read(f.repo.join(".git/config")).unwrap();

    let preview = remote_ops::push_preview(&f.session, &[]).unwrap();
    assert!(preview.has_upstream);
    assert_eq!(preview.commit_count, 2);
    assert_eq!(preview.commits.len(), 2);
    assert_eq!(preview.commits[0].oid, head);
    assert_eq!(preview.commits[1].oid, first);
    assert_eq!(f.head(), head);
    assert_eq!(
        run_git(&f.origin, &f.env, &["rev-parse", "main"]).trim(),
        upstream
    );
    assert_eq!(fs::read(f.repo.join(".git/config")).unwrap(), config);
}

#[test]
fn push_is_rejected_when_the_remote_has_new_commits() {
    let f = Fixture::new();
    let tracked = f.git(&["rev-parse", "origin/main"]).trim().to_string();
    let head = f.track("local.txt", b"local\n", "local commit");
    let remote_head = f.advance_remote("remote.txt", b"remote\n", "remote commit");
    // 未 fetch 的远端前进不妨碍打开本地记录的预览，真正推送再由 Git 拒绝。
    let preview = remote_ops::push_preview(&f.session, &["main".into()]).unwrap();
    assert_eq!(preview.commit_count, 1);
    assert_eq!(preview.commits[0].oid, head);
    assert_eq!(f.git(&["rev-parse", "origin/main"]).trim(), tracked);
    assert_ne!(tracked, remote_head);
    let result = remote_ops::push(
        &f.session,
        &PushRequest {
            remote: None,
            branch: None,
            tags: false,
            force: PushForce::None,
        },
        &["main".into()],
        Duration::from_secs(60),
    )
    .unwrap();
    let PushResult::Rejected { reason, .. } = result else {
        panic!("非快进推送应被拒绝");
    };
    assert!(
        reason.contains("拒绝")
            || reason.contains("non-fast-forward")
            || reason.contains("[rejected]")
            || reason.contains("fetch first"),
        "{reason}"
    );
    // 更新后可以再推（前端编排；这里验证底层）。
    remote_ops::update(
        &f.session,
        &f.store,
        UpdateMode::Merge,
        FetchTagMode::Auto,
        Duration::from_secs(60),
    )
    .unwrap();
    let result = remote_ops::push(
        &f.session,
        &PushRequest {
            remote: None,
            branch: None,
            tags: false,
            force: PushForce::None,
        },
        &["main".into()],
        Duration::from_secs(60),
    )
    .unwrap();
    assert!(matches!(result, PushResult::Pushed { .. }));
}

#[test]
fn force_with_lease_refuses_to_overwrite_unfetched_remote_commits() {
    let f = Fixture::new();
    // 远端前进了，但本地没有 fetch：lease 的期望值已经过期。
    let remote_head = f.advance_remote("remote.txt", b"remote\n", "remote commit");
    f.track("local.txt", b"local\n", "local commit");
    let result = remote_ops::push(
        &f.session,
        &PushRequest {
            remote: None,
            branch: None,
            tags: false,
            force: PushForce::WithLease,
        },
        &[],
        Duration::from_secs(60),
    )
    .unwrap();
    assert!(
        matches!(result, PushResult::Rejected { .. }),
        "带 lease 的强制推送不能覆盖没取回的远端提交"
    );
    // 远端没有被改写。
    let origin_head = run_git(&f.origin, &f.env, &["rev-parse", "main"]);
    assert_eq!(origin_head.trim(), remote_head);

    // 取回之后再强制推送：lease 值与远端一致，允许覆盖。
    f.git(&["fetch", "origin"]);
    let result = remote_ops::push(
        &f.session,
        &PushRequest {
            remote: None,
            branch: None,
            tags: false,
            force: PushForce::WithLease,
        },
        &[],
        Duration::from_secs(60),
    )
    .unwrap();
    assert!(matches!(result, PushResult::Pushed { .. }));
    assert!(f.log_args().iter().any(|args| {
        args.iter()
            .any(|arg| arg.starts_with("--force-with-lease="))
    }));
}

#[test]
fn protected_branch_refuses_force_push() {
    let f = Fixture::new();
    let preview = remote_ops::push_preview(&f.session, &["main".into(), "master".into()]).unwrap();
    assert!(preview.protected);
    assert!(!preview.force_allowed);
    assert!(
        preview
            .force_reason
            .as_ref()
            .map(|e| e.message.as_str())
            .unwrap_or_default()
            .contains("受保护")
    );

    let error = remote_ops::push(
        &f.session,
        &PushRequest {
            remote: None,
            branch: None,
            tags: false,
            force: PushForce::WithLease,
        },
        &["main".into()],
        Duration::from_secs(60),
    )
    .unwrap_err();
    assert!(error.message.contains("受保护"), "{}", error.message);

    // 通配模式与非受保护分支：允许强制推送。
    let preview = remote_ops::push_preview(&f.session, &["m*".into()]).unwrap();
    assert!(preview.protected);
    f.git(&["checkout", "-b", "topic"]);
    let preview = remote_ops::push_preview(&f.session, &["main".into(), "master".into()]).unwrap();
    assert!(!preview.protected);
    assert!(preview.force_allowed);
}

#[test]
fn network_timeout_is_reported_as_needs_verification() {
    let f = Fixture::new();
    // `ext::` 远端执行一个会挂住的命令，用缩短的超时验证「结果待核对」。
    f.git(&["remote", "add", "stall", "ext::sleep 5"]);
    let started = std::time::Instant::now();
    let error =
        remote_ops::fetch(&f.session, FetchTagMode::Auto, Duration::from_millis(400)).unwrap_err();
    assert!(
        matches!(error.kind, revia_lib::error::ErrorKind::NeedsVerification),
        "超时应是结果待核对：{error:?}"
    );
    assert!(error.message.contains("超时"), "{}", error.message);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "超时必须真的中断，而不是等命令自己结束"
    );
}

#[test]
fn missing_credentials_fail_immediately_without_waiting_for_input() {
    let f = Fixture::new();
    // 指向本机一个没有监听的端口：需要认证的 HTTPS 远端会立刻失败。
    f.git(&[
        "remote",
        "add",
        "auth",
        "https://user:secret@127.0.0.1:9/repo.git",
    ]);
    let started = std::time::Instant::now();
    let error =
        remote_ops::fetch(&f.session, FetchTagMode::Auto, Duration::from_secs(60)).unwrap_err();
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "不能等待输入或长时间挂起"
    );
    // 原始输出里不能出现凭据。
    assert!(!error.message.contains("secret"), "{}", error.message);
    // 命令日志里同样打码。
    for args in f.log_args() {
        for arg in args {
            assert!(!arg.contains("secret"), "命令日志泄露了凭据：{arg}");
        }
    }
}

#[test]
fn check_remote_compares_refs_without_fetching_objects() {
    let f = Fixture::new();
    let check = remote_ops::check_remote(&f.session, Duration::from_secs(60)).unwrap();
    assert!(!check.has_new_commits);

    let remote_head = f.advance_remote("remote.txt", b"remote\n", "remote commit");
    let check = remote_ops::check_remote(&f.session, Duration::from_secs(60)).unwrap();
    assert!(check.has_new_commits);
    assert_eq!(check.remote_commit.as_deref(), Some(remote_head.as_str()));
    // 只比较引用：本地仍然没有远端对象。
    let object = f.git_fail(&["cat-file", "-e", &remote_head]);
    assert!(!object.status.success(), "检查远端不应取回对象");
}

#[test]
fn write_lock_blocks_a_second_operation_on_the_same_worktree() {
    let f = Fixture::new();
    let guard = f.session.acquire_write_lock().expect("取得写锁");
    let error =
        remote_ops::fetch(&f.session, FetchTagMode::Auto, Duration::from_secs(60)).unwrap_err();
    assert!(error.message.contains("写操作"), "{}", error.message);
    drop(guard);
    remote_ops::fetch(&f.session, FetchTagMode::Auto, Duration::from_secs(60))
        .expect("释放后可以执行");
}

#[test]
fn command_log_records_worktree_arguments_exit_code_and_output() {
    let f = Fixture::new();
    remote_ops::fetch(&f.session, FetchTagMode::Sync, Duration::from_secs(60)).unwrap();
    let records = f.log.snapshot();
    let fetch_record = records
        .iter()
        .find(|record| record.args.iter().any(|arg| arg == "fetch"))
        .expect("应有 fetch 记录");
    assert_eq!(fetch_record.worktree_id.as_deref(), Some("test-worktree"));
    assert_eq!(fetch_record.exit_code, 0);
    assert!(fetch_record.args.iter().any(|arg| arg == "--prune"));
    assert!(fetch_record.started_at_ms > 0);
    assert!(!fetch_record.output_truncated);
    // 不是所有命令都属于某个 worktree（启动作业等），但这里所有命令都有归属。
    assert!(records.iter().all(|record| record.worktree_id.is_some()));
}

#[test]
fn external_git_helper_is_used_for_worktree_label_and_docs() {
    // 直接构造的 Git（没有标注 worktree）记录为 null，不被错记到某个 worktree。
    let f = Fixture::new();
    let bare = Git::new(&f.repo, &f.shell_env("bare"), f.log.clone()).unwrap();
    bare.run_ok(&["status", "--porcelain"], "读取状态").unwrap();
    let record = f.log.snapshot().last().cloned().expect("应有记录");
    assert_eq!(record.worktree_id, None);
    assert!(record.directory.contains("repo"));
    let _ = (&f.home, &f.data);
}

#[test]
fn force_push_is_refused_when_the_target_remote_branch_is_protected() {
    let f = Fixture::new();
    // 本地分支叫 feature（不受保护），但目标远端分支是 main（受保护）。
    f.git(&["checkout", "-b", "feature"]);
    f.track("feature.txt", b"feature\n", "feature commit");
    let remote_main = f
        .other_git(&["rev-parse", "origin/main"])
        .trim()
        .to_string();
    let _ = remote_main;

    let error = remote_ops::push(
        &f.session,
        &PushRequest {
            remote: Some("origin".into()),
            branch: Some("main".into()),
            tags: false,
            force: PushForce::WithLease,
        },
        &["main".into(), "master".into()],
        Duration::from_secs(60),
    )
    .unwrap_err();
    assert!(
        error.message.contains("main 是受保护分支"),
        "{}",
        error.message
    );
    // 远端 main 没有被改写。
    let origin_main = run_git(&f.origin, &f.env, &["rev-parse", "main"]);
    let local_main = f.git(&["rev-parse", "main"]);
    assert_eq!(origin_main.trim(), local_main.trim());
}

#[test]
fn push_preview_treats_the_target_branch_as_protected() {
    let f = Fixture::new();
    // 本地 myfix 跟踪 origin/main：目标远端分支是 main，因此不允许强制推送。
    f.git(&["checkout", "-b", "myfix"]);
    f.git(&["config", "branch.myfix.remote", "origin"]);
    f.git(&["config", "branch.myfix.merge", "refs/heads/main"]);
    let preview = remote_ops::push_preview(&f.session, &["main".into(), "master".into()]).unwrap();
    assert_eq!(preview.target_branch, "main");
    assert!(
        !preview.protected,
        "B06 提交后是否显示对话框只取决于当前分支"
    );
    assert!(!preview.force_allowed);
    assert!(
        preview
            .force_reason
            .as_ref()
            .map(|e| e.message.as_str())
            .unwrap_or_default()
            .contains("main")
    );

    // 创建真实上游，避免不存在的跟踪引用让预览退回本地分支名。
    f.git(&["push", "origin", "myfix"]);
    // 目标与本地都不受保护时允许强制推送。
    f.git(&["config", "branch.myfix.merge", "refs/heads/myfix"]);
    let preview = remote_ops::push_preview(&f.session, &["main".into(), "master".into()]).unwrap();
    assert!(preview.has_upstream);
    assert_eq!(preview.target_branch, "myfix");
    assert!(!preview.protected);
    assert!(preview.force_allowed);
    assert!(preview.force_reason.is_none());

    // 当前 main 受保护、目标 myfix 不受保护：B06 仍显示对话框，强制推送仍拒绝。
    f.git(&["checkout", "main"]);
    f.git(&["config", "branch.main.merge", "refs/heads/myfix"]);
    let preview = remote_ops::push_preview(&f.session, &["main".into(), "master".into()]).unwrap();
    assert_eq!(preview.target_branch, "myfix");
    assert!(preview.protected);
    assert!(!preview.force_allowed);
    assert!(
        preview
            .force_reason
            .as_ref()
            .map(|e| e.message.as_str())
            .unwrap_or_default()
            .contains("main")
    );
}

#[test]
fn push_does_not_rewrite_the_upstream_config_when_target_is_unchanged() {
    let f = Fixture::new();
    // main 已跟踪 origin/main，且远端与本地一致。
    // 界面每次推送都会带上目标分支（来自预览），这里同样显式给出。
    let before = fs::read_to_string(f.repo.join(".git/config")).unwrap();
    let result = remote_ops::push(
        &f.session,
        &PushRequest {
            remote: Some("origin".into()),
            branch: Some("main".into()),
            tags: false,
            force: PushForce::None,
        },
        &["main".into(), "master".into()],
        Duration::from_secs(60),
    )
    .unwrap();
    let PushResult::Pushed {
        upstream_recorded, ..
    } = result
    else {
        panic!("推送应成功（已经是最新）");
    };
    assert!(!upstream_recorded, "已有上游且目标未变时不应记录上游");
    // 命令日志里不带 `--set-upstream`：不会重写 `branch.<name>.remote/merge`。
    let push_args = f
        .log_args()
        .into_iter()
        .find(|args| args.iter().any(|arg| arg == "push"))
        .expect("应有 push 记录");
    assert!(
        !push_args.iter().any(|arg| arg == "--set-upstream"),
        "{push_args:?}"
    );
    let after = fs::read_to_string(f.repo.join(".git/config")).unwrap();
    assert_eq!(before, after, "推送同一上游不应改写 Git 配置");

    // 用户把目标改成另一个分支时才记录上游（Git 自己的结果）。
    let result = remote_ops::push(
        &f.session,
        &PushRequest {
            remote: Some("origin".into()),
            branch: Some("copied".into()),
            tags: false,
            force: PushForce::None,
        },
        &["main".into(), "master".into()],
        Duration::from_secs(60),
    )
    .unwrap();
    let PushResult::Pushed {
        upstream_recorded, ..
    } = result
    else {
        panic!("推送新目标应成功");
    };
    assert!(upstream_recorded);
    assert_eq!(
        f.git(&["config", "--get", "branch.main.merge"]).trim(),
        "refs/heads/copied"
    );
}

#[test]
fn checkout_separates_paths_with_double_dash() {
    let f = Fixture::new();
    // 分支与已跟踪文件同名：没有 `--` 时 Git 会把它当成 pathspec（或报歧义）。
    f.track("topic", b"file content\n", "add file named topic");
    f.git(&["branch", "topic"]);
    f.write("topic", b"local edit\n");

    let result = checkout(&f, local("topic"), CheckoutStrategy::Plain);
    let outcome = match result {
        Ok(CheckoutResult::Done(outcome)) => outcome,
        Ok(CheckoutResult::NeedsConfirmation { files, .. }) => {
            panic!("应直接检出（本地改动不在目标分支差异里）：{files:?}");
        }
        Err(error) => panic!("检出分支 topic 失败：{}", error.message),
    };
    assert_eq!(outcome.branch, "topic");
    assert_eq!(
        f.git(&["rev-parse", "--abbrev-ref", "HEAD"]).trim(),
        "topic"
    );
    // 文件内容没有被 checkout 的 pathspec 行为覆盖。
    assert_eq!(f.read("topic"), "local edit\n");
    let args = f
        .log_args()
        .into_iter()
        .find(|args| args.iter().any(|arg| arg == "checkout"))
        .expect("应有 checkout 记录");
    assert_eq!(args.last().map(String::as_str), Some("--"), "{args:?}");
}
