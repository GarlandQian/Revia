//! F11：受控loopback HTTP、真实Git/本地remote、自建HOME与appdata。
//! 不接触真实账号、系统钥匙串或用户仓库；不属于桌面验收。
use revia_lib::{
    git::CommandLog,
    m7::github::{
        pr::{self, PrCheckoutRequest, PrContext},
        GithubClient, GithubState,
    },
    recovery::RecoveryStore,
    remote_ops::{CheckoutResult, CheckoutStrategy},
    session::WorktreeSession,
    shell::{EnvCaptureInfo, ShellEnv},
    stash_archive::StashArchiveStore,
};
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

struct Fixture {
    _temp: tempfile::TempDir,
    repo: PathBuf,
    home: PathBuf,
    session: Arc<WorktreeSession>,
    github: GithubState,
    recovery: RecoveryStore,
    archive: StashArchiveStore,
    head: String,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let home = temp.path().join("home");
        let data = temp.path().join("appdata");
        for path in [&repo, &home, &data] {
            fs::create_dir(path).unwrap();
        }
        fs::write(home.join("gitconfig"), "").unwrap();
        let git = |args: &[&str]| run_git(&repo, &home, args);
        git(&["init", "-b", "main"]);
        git(&["config", "user.name", "PR fixture"]);
        git(&["config", "user.email", "fixture@example.invalid"]);
        git(&["config", "commit.gpgsign", "false"]);
        fs::write(repo.join("a.txt"), "base\n").unwrap();
        fs::write(repo.join("keep.txt"), "keep\n").unwrap();
        git(&["add", "--all"]);
        git(&["commit", "-m", "base"]);
        git(&["checkout", "-b", "fixture-head"]);
        fs::write(repo.join("a.txt"), "PR version\n").unwrap();
        git(&["add", "a.txt"]);
        git(&["commit", "-m", "PR head"]);
        let head = git(&["rev-parse", "HEAD"]).trim().to_string();
        git(&["checkout", "main"]);
        let remote = temp.path().join("remote.git");
        fs::create_dir(&remote).unwrap();
        run_git(&remote, &home, &["init", "--bare"]);
        run_git(
            &remote,
            &home,
            &[
                "fetch",
                "--no-tags",
                repo.to_str().unwrap(),
                "refs/heads/fixture-head:refs/pull/1/head",
            ],
        );
        git(&["remote", "add", "origin", remote.to_str().unwrap()]);
        let vars = HashMap::from([
            ("HOME".into(), home.display().to_string()),
            (
                "GIT_CONFIG_GLOBAL".into(),
                home.join("gitconfig").display().to_string(),
            ),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
        ]);
        let paths = vec!["/usr/bin".into(), "/bin".into(), "/opt/homebrew/bin".into()];
        let env = ShellEnv {
            path: paths.clone(),
            vars,
            info: EnvCaptureInfo {
                worktree: repo.display().to_string(),
                shell: "/bin/sh".into(),
                elapsed_ms: 1,
                timed_out: false,
                used_fallback: true,
                var_count: 3,
                path_entries: paths,
                diagnostics: None,
            },
        };
        let session =
            WorktreeSession::open("fixture-pr", &repo, env, Arc::new(CommandLog::default()))
                .unwrap();
        let github = GithubState::new(&data);
        let recovery = RecoveryStore::new(data.clone());
        let archive = StashArchiveStore::new(&data);
        Self {
            _temp: temp,
            repo,
            home,
            session,
            github,
            recovery,
            archive,
            head,
        }
    }
    fn git(&self, args: &[&str]) -> String {
        run_git(&self.repo, &self.home, args)
    }
    fn output(&self, args: &[&str]) -> Output {
        git_output(&self.repo, &self.home, args)
    }
    fn request(&self, strategy: CheckoutStrategy) -> PrCheckoutRequest {
        PrCheckoutRequest {
            worktree_id: self.session.id.clone(),
            remote: "origin".into(),
            number: 1,
            expected_head_sha: self.head.clone(),
            strategy,
            local_branch: None,
            overwrite_confirmed: false,
        }
    }
    fn context(&self, server: &Server) -> PrContext<'_> {
        PrContext {
            session: &self.session,
            state: &self.github,
            client: GithubClient::new(server.base.clone(), "fixture-FAKE-token".into()).unwrap(),
            login: "fixture-account".into(),
            repo_key: GithubState::repo_key(&self.repo),
            owner: "fixture".into(),
            repo: "project".into(),
            remote: "origin".into(),
        }
    }
    fn no_temporary_refs(&self) {
        assert!(self
            .git(&[
                "for-each-ref",
                "--format=%(refname)",
                "refs/revia/pr-checkout/"
            ])
            .is_empty());
    }
}
fn git_output(path: &Path, home: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .current_dir(path)
        .env("HOME", home)
        .env("GIT_CONFIG_GLOBAL", home.join("gitconfig"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args(args)
        .output()
        .unwrap()
}
fn run_git(path: &Path, home: &Path, args: &[&str]) -> String {
    let output = git_output(path, home, args);
    assert!(
        output.status.success(),
        "fixture git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
struct Server {
    base: String,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(head: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let seen = Arc::clone(&requests);
        let done = Arc::clone(&stop);
        let body = serde_json::json!({"number":1,"title":"fixture PR","state":"open","head":{"sha":head,"ref":"feature"},"base":{"ref":"main"}}).to_string();
        let thread = thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        let mut header = Vec::new();
                        let mut chunk = [0; 1024];
                        while !header.windows(4).any(|v| v == b"\r\n\r\n") && header.len() < 16_384
                        {
                            let n = stream.read(&mut chunk).unwrap();
                            if n == 0 {
                                break;
                            }
                            header.extend_from_slice(&chunk[..n]);
                        }
                        let first = String::from_utf8_lossy(&header)
                            .lines()
                            .next()
                            .unwrap_or_default()
                            .to_string();
                        seen.lock().unwrap().push(first.clone());
                        let (status, answer) =
                            if first.starts_with("GET /repos/fixture/project/pulls/1 ") {
                                ("200 OK", body.as_str())
                            } else {
                                ("404 Not Found", "{}")
                            };
                        write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", answer.len(), answer).unwrap();
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("fixture server: {error}"),
                }
            }
        });
        Self {
            base,
            requests,
            stop,
            thread: Some(thread),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap();
    }
}

#[test]
fn active_merge_rejects_before_http_fetch_or_any_ref_write() {
    let f = Fixture::new();
    f.git(&["checkout", "-b", "conflicting"]);
    fs::write(f.repo.join("a.txt"), "conflicting side\n").unwrap();
    f.git(&["add", "a.txt"]);
    f.git(&["commit", "-m", "conflict"]);
    f.git(&["checkout", "main"]);
    fs::write(f.repo.join("a.txt"), "main conflict\n").unwrap();
    f.git(&["add", "a.txt"]);
    f.git(&["commit", "-m", "main conflict"]);
    assert!(!f.output(&["merge", "conflicting"]).status.success());
    let refs = f.git(&["show-ref"]);
    let index = fs::read(f.repo.join(".git/index")).unwrap();
    let text = fs::read(f.repo.join("a.txt")).unwrap();
    let server = Server::new(&f.head);
    let context = f.context(&server);
    let error = pr::checkout(
        &context,
        &f.request(CheckoutStrategy::Smart),
        &f.recovery,
        &f.archive,
    )
    .unwrap_err();
    assert_eq!(
        error.code,
        Some("errors.remoteOps.anOperationProgressFinishAbortBefore")
    );
    assert!(server.requests.lock().unwrap().is_empty());
    assert_eq!(f.git(&["show-ref"]), refs);
    assert_eq!(fs::read(f.repo.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(f.repo.join("a.txt")).unwrap(), text);
    f.no_temporary_refs();
}
#[test]
fn smart_pr_checkout_preserves_working_content_and_original_index_archive() {
    let f = Fixture::new();
    fs::write(f.repo.join("keep.txt"), "staged keep\n").unwrap();
    f.git(&["add", "keep.txt"]);
    fs::write(f.repo.join("keep.txt"), "staged keep\nunstaged keep\n").unwrap();
    let server = Server::new(&f.head);
    let result = pr::checkout(
        &f.context(&server),
        &f.request(CheckoutStrategy::Smart),
        &f.recovery,
        &f.archive,
    )
    .unwrap();
    assert!(matches!(result.result, CheckoutResult::Done(_)));
    assert_eq!(f.git(&["branch", "--show-current"]).trim(), "pr/1");
    assert_eq!(f.git(&["rev-parse", "HEAD"]).trim(), f.head);
    // 沿既有Q150策略stash apply：恢复正文，原暂存区版本保留在归档中；不冒称--index恢复。
    assert_eq!(f.git(&["show", ":keep.txt"]), "keep\n");
    let identity = f.session.require_identity().unwrap();
    let archives = f.archive.list(Some(&identity)).unwrap();
    assert_eq!(archives.len(), 1);
    let archive = f
        ._temp
        .path()
        .join("appdata/stash-archive")
        .join(&archives[0].id)
        .join("archive.git");
    assert_eq!(
        run_git(
            &archive,
            &f.home,
            &["show", &format!("{}^2:keep.txt", archives[0].stash_oid)]
        ),
        "staged keep\n"
    );
    assert_eq!(
        fs::read_to_string(f.repo.join("keep.txt")).unwrap(),
        "staged keep\nunstaged keep\n"
    );
    assert!(!f
        .recovery
        .list(Some(&f.session.require_identity().unwrap()))
        .unwrap()
        .is_empty());
    assert!(!f.repo.join(".git/FETCH_HEAD").exists());
    f.no_temporary_refs();
}
#[test]
fn declined_plain_checkout_rolls_back_existing_or_new_local_branch() {
    for existing in [false, true] {
        let f = Fixture::new();
        if existing {
            f.git(&["branch", "pr/1"]);
        }
        fs::write(f.repo.join("a.txt"), "local unsaved version\n").unwrap();
        let before = f.git(&["show-ref"]);
        let server = Server::new(&f.head);
        let result = pr::checkout(
            &f.context(&server),
            &f.request(CheckoutStrategy::Plain),
            &f.recovery,
            &f.archive,
        )
        .unwrap();
        assert!(matches!(
            result.result,
            CheckoutResult::NeedsConfirmation { .. }
        ));
        assert_eq!(f.git(&["show-ref"]), before);
        assert_eq!(f.git(&["branch", "--show-current"]).trim(), "main");
        assert_eq!(
            fs::read_to_string(f.repo.join("a.txt")).unwrap(),
            "local unsaved version\n"
        );
        f.no_temporary_refs();
    }
}
#[test]
fn existing_write_guard_rejects_pr_checkout_without_http() {
    let f = Fixture::new();
    let _write = f.session.acquire_write_lock().unwrap();
    let server = Server::new(&f.head);
    let error = pr::checkout(
        &f.context(&server),
        &f.request(CheckoutStrategy::Smart),
        &f.recovery,
        &f.archive,
    )
    .unwrap_err();
    assert_eq!(
        error.code,
        Some("errors.session.aWriteOperationProgressWorktreeWait")
    );
    assert!(server.requests.lock().unwrap().is_empty());
    f.no_temporary_refs();
}
#[test]
fn diverged_local_branch_is_not_overwritten_without_confirmation() {
    let f = Fixture::new();
    fs::write(f.repo.join("private.txt"), "fixture local commit\n").unwrap();
    f.git(&["add", "private.txt"]);
    f.git(&["commit", "-m", "local only"]);
    f.git(&["branch", "pr/1"]);
    let before = f.git(&["show-ref"]);
    let server = Server::new(&f.head);
    let error = pr::checkout(
        &f.context(&server),
        &f.request(CheckoutStrategy::Smart),
        &f.recovery,
        &f.archive,
    )
    .unwrap_err();
    assert_eq!(error.code, Some("errors.m7.githubPrBranchDiverged"));
    assert_eq!(f.git(&["show-ref"]), before);
    f.no_temporary_refs();
}
#[test]
fn real_index_lock_failure_rolls_back_branch_and_preserves_lock() {
    let f = Fixture::new();
    let before = f.git(&["show-ref"]);
    let lock = f.repo.join(".git/index.lock");
    fs::write(&lock, "fixture-owned-lock").unwrap();
    let server = Server::new(&f.head);
    assert!(pr::checkout(
        &f.context(&server),
        &f.request(CheckoutStrategy::Plain),
        &f.recovery,
        &f.archive
    )
    .is_err());
    assert_eq!(f.git(&["show-ref"]), before);
    assert_eq!(fs::read_to_string(&lock).unwrap(), "fixture-owned-lock");
    f.no_temporary_refs();
}
#[cfg(unix)]
#[test]
fn post_checkout_failure_is_unverified_and_does_not_rewind_checked_out_head() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let hook = f.repo.join(".git/hooks/post-checkout");
    fs::write(&hook, "#!/bin/sh\nexit 23\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    let server = Server::new(&f.head);
    let error = pr::checkout(
        &f.context(&server),
        &f.request(CheckoutStrategy::Plain),
        &f.recovery,
        &f.archive,
    )
    .unwrap_err();
    assert_eq!(error.kind, revia_lib::error::ErrorKind::NeedsVerification);
    assert_eq!(f.git(&["branch", "--show-current"]).trim(), "pr/1");
    assert_eq!(f.git(&["rev-parse", "HEAD"]).trim(), f.head);
    assert_eq!(
        fs::read_to_string(f.repo.join("a.txt")).unwrap(),
        "PR version\n"
    );
    f.no_temporary_refs();
}

#[cfg(unix)]
#[test]
fn older_reported_git_version_rejects_before_pr_network_or_writes() {
    use std::os::unix::fs::PermissionsExt;
    let mut f = Fixture::new();
    let program = f._temp.path().join("fixture-git-228");
    fs::write(
        &program,
        r#"#!/bin/sh
if [ "$1" = --version ]; then echo 'git version 2.28.0'; else exec /usr/bin/git "$@"; fi
"#,
    )
    .unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    f.session = WorktreeSession::open_with_options(
        "fixture-oldgit",
        &f.repo,
        f.session.shell_env.clone(),
        Arc::new(CommandLog::default()),
        None,
        Some(&program),
    )
    .unwrap();
    let before = f.git(&["show-ref"]);
    let server = Server::new(&f.head);
    let error = pr::checkout(
        &f.context(&server),
        &f.request(CheckoutStrategy::Smart),
        &f.recovery,
        &f.archive,
    )
    .unwrap_err();
    assert_eq!(error.code, Some("errors.environment.gitVersion"));
    assert!(server.requests.lock().unwrap().is_empty());
    assert_eq!(f.git(&["show-ref"]), before);
    f.no_temporary_refs();
}
