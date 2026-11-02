//! F10：真实loopback HTTP分页/并发 + 生产缓存/有效并集；仓库、账号资料与appdata均为独占fixture。
#[path = "support/git_environment.rs"]
mod git_environment;

use revia_lib::{
    git::CommandLog,
    m7::github::{
        pr::PrContext, protection, GithubAccount, GithubAuthMethod, GithubClient, GithubState,
    },
    session::WorktreeSession,
};
use std::{
    collections::VecDeque,
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    thread,
    time::Duration,
};
struct Fixture {
    _temp: tempfile::TempDir,
    repo: PathBuf,
    home: PathBuf,
    data: PathBuf,
    session: Arc<WorktreeSession>,
    state: GithubState,
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
        git(&repo, &home, &["init", "-b", "main"]);
        git(&repo, &home, &["config", "user.name", "Protection fixture"]);
        git(
            &repo,
            &home,
            &["config", "user.email", "fixture@example.invalid"],
        );
        git(&repo, &home, &["config", "commit.gpgsign", "false"]);
        fs::write(repo.join("a.txt"), "fixture\n").unwrap();
        git(&repo, &home, &["add", "a.txt"]);
        git(&repo, &home, &["commit", "-m", "initial"]);
        // 只读解析这条虚构URL，不对它执行Git传输。
        git(
            &repo,
            &home,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/fixture/project.git",
            ],
        );
        let env = git_environment::shell_env(&home, &repo, &home.join("gitconfig"));
        let session = WorktreeSession::open(
            "protection-fixture",
            &repo,
            env,
            Arc::new(CommandLog::default()),
        )
        .unwrap();
        let state = GithubState::new(&data);
        state.upsert_account(account("account-a")).unwrap();
        Self {
            _temp: temp,
            repo,
            home,
            data,
            session,
            state,
        }
    }
    fn git(&self, args: &[&str]) -> String {
        git(&self.repo, &self.home, args)
    }
    fn context<'a>(
        &'a self,
        server: &Server,
        login: &str,
        remote: &str,
        repo: &str,
    ) -> PrContext<'a> {
        PrContext {
            session: &self.session,
            state: &self.state,
            client: GithubClient::new(server.base.clone(), "fixture-FAKE-token".into()).unwrap(),
            login: login.into(),
            remote: remote.into(),
            owner: "fixture".into(),
            repo: repo.into(),
            repo_key: GithubState::repo_key(&self.repo),
        }
    }
}
fn account(login: &str) -> GithubAccount {
    GithubAccount {
        login: login.into(),
        name: None,
        avatar_url: None,
        auth_method: GithubAuthMethod::Token,
        scopes: vec![],
        added_at_ms: 1,
    }
}
fn git(repo: &Path, home: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(repo)
        .env("HOME", home)
        .env("GIT_CONFIG_GLOBAL", home.join("gitconfig"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "fixture git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}
#[derive(Default)]
struct Gate {
    state: Mutex<(bool, bool)>,
    changed: Condvar,
}
impl Gate {
    fn arrive(&self) {
        let mut state = self.state.lock().unwrap();
        state.0 = true;
        self.changed.notify_all();
        let (_guard, timeout) = self
            .changed
            .wait_timeout_while(state, Duration::from_secs(5), |s| !s.1)
            .unwrap();
        assert!(!timeout.timed_out(), "fixture response gate timed out");
    }
    fn wait(&self) {
        let state = self.state.lock().unwrap();
        let (_guard, timeout) = self
            .changed
            .wait_timeout_while(state, Duration::from_secs(5), |s| !s.0)
            .unwrap();
        assert!(!timeout.timed_out(), "fixture request did not arrive");
    }
    fn release(&self) {
        self.state.lock().unwrap().1 = true;
        self.changed.notify_all();
    }
}
struct Response {
    status: u16,
    body: String,
    next: Option<String>,
    gate: Option<Arc<Gate>>,
}
impl Response {
    fn branches(names: &[&str]) -> Self {
        Self {
            status: 200,
            body: serde_json::to_string(
                &names
                    .iter()
                    .map(|name| serde_json::json!({"name":name,"protected":true}))
                    .collect::<Vec<_>>(),
            )
            .unwrap(),
            next: None,
            gate: None,
        }
    }
    fn failure(status: u16) -> Self {
        Self {
            status,
            body: "{\"message\":\"fixture denied\"}".into(),
            next: None,
            gate: None,
        }
    }
    fn next(mut self, path: &str) -> Self {
        self.next = Some(path.into());
        self
    }
    fn gated(mut self, gate: &Arc<Gate>) -> Self {
        self.gate = Some(Arc::clone(gate));
        self
    }
}
struct Server {
    base: String,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(responses: Vec<Response>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let queue = Arc::new(Mutex::new(VecDeque::from(responses)));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let seen = Arc::clone(&requests);
        let done = Arc::clone(&stop);
        let api = base.clone();
        let worker = thread::spawn(move || {
            let mut workers = Vec::new();
            while !done.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream.set_nonblocking(false).unwrap();
                        let queue = Arc::clone(&queue);
                        let seen = Arc::clone(&seen);
                        let api = api.clone();
                        workers.push(thread::spawn(move || {
                            stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap(); let mut header = Vec::new(); let mut buf = [0; 1024];
                            while !header.windows(4).any(|v| v == b"\r\n\r\n") && header.len() < 16_384 {
                                let n = match stream.read(&mut buf) {
                                    Ok(n) => n,
                                    Err(error) if header.is_empty() && matches!(error.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => return,
                                    Err(error) => panic!("fixture HTTP read: {error}"),
                                };
                                if n == 0 { break; }
                                header.extend_from_slice(&buf[..n]);
                            }
                            if header.is_empty() { return; }
                            seen.lock().unwrap().push(String::from_utf8_lossy(&header).lines().next().unwrap_or_default().to_string());
                            let response = queue.lock().unwrap().pop_front().unwrap_or_else(|| Response::failure(500));
                            if let Some(gate) = response.gate { gate.arrive(); }
                            let link = response.next.map(|next| format!("Link: <{}>; rel=\"next\"\r\n", next.replace("{base}", &api))).unwrap_or_default();
                            write!(stream, "HTTP/1.1 {} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{link}Connection: close\r\n\r\n{}", response.status, response.body.len(), response.body).unwrap();
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("fixture HTTP listener: {error}"),
                }
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
        Self {
            base,
            requests,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.worker.take().unwrap().join().unwrap();
        println!(
            "PROTECTION_REQUESTS={}",
            serde_json::to_string(&*self.requests.lock().unwrap()).unwrap()
        );
    }
}
const PAGE_TWO: &str = "{base}/repos/fixture/project/branches?protected=true&per_page=100&page=2";

#[test]
fn complete_pagination_replaces_snapshot_and_feeds_push_history_union() {
    let f = Fixture::new();
    let server = Server::new(vec![
        Response::branches(&["main"]).next(PAGE_TWO),
        Response::branches(&["release/1", "main"]),
        Response::branches(&[]),
    ]);
    let context = f.context(&server, "account-a", "origin", "project");
    let local = vec!["local/*".into()];
    assert!(protection::state(&context, "main", true).unwrap().protected);
    let union = protection::effective(&f.session, &f.state, Some("origin"), &local).unwrap();
    assert_eq!(union, vec!["local/*", "main", "release/1"]);
    assert!(revia_lib::remote_ops::is_protected("release/1", &union));
    assert!(revia_lib::remote_ops::is_protected("local/work", &union));
    assert_eq!(
        protection::history_branch(&f.session, &f.state, &local)
            .unwrap()
            .as_deref(),
        Some("main")
    );
    let refused = revia_lib::remote_ops::push(
        &f.session,
        &revia_lib::remote_ops::PushRequest {
            remote: Some("origin".into()),
            branch: Some("release/1".into()),
            tags: false,
            force: revia_lib::remote_ops::PushForce::WithLease,
        },
        &union,
        Duration::from_secs(1),
    )
    .unwrap_err();
    assert_eq!(
        refused.code,
        Some("errors.remoteOps.isProtectedBranchForcePush")
    );
    let cached = protection::state(&context, "release/1", false).unwrap();
    assert_eq!(cached.source, "api");
    assert!(cached.protected);
    assert_eq!(server.requests.lock().unwrap().len(), 2);
    let refreshed = protection::state(&context, "main", true).unwrap();
    assert!(!refreshed.protected);
    assert_eq!(refreshed.source, "api");
    assert_eq!(
        protection::effective(&f.session, &f.state, Some("origin"), &local).unwrap(),
        local
    );
    assert!(protection::history_branch(&f.session, &f.state, &local)
        .unwrap()
        .is_none());
    assert!(server.requests.lock().unwrap().iter().all(|line| line
        .starts_with("GET /repos/fixture/project/branches?protected=true&per_page=100&page=")));
}
#[test]
fn permission_not_found_rate_limit_and_bad_pages_keep_previous_result_stale() {
    for status in [401, 403, 404, 429, 500] {
        let f = Fixture::new();
        let server = Server::new(vec![
            Response::branches(&["main"]),
            Response::failure(status),
        ]);
        let context = f.context(&server, "account-a", "origin", "project");
        let first = protection::state(&context, "main", true).unwrap();
        thread::sleep(Duration::from_millis(5));
        let failed = protection::state(&context, "main", true).unwrap();
        assert!(failed.protected);
        assert_eq!(failed.source, "stale");
        assert_eq!(failed.checked_at_ms, first.checked_at_ms);
        assert_eq!(
            protection::effective(&f.session, &f.state, Some("origin"), &[]).unwrap(),
            vec!["main"]
        );
    }
    for bad in [
        "{broken",
        "[{\"name\":\"release\"}]",
        "[{\"name\":\"release\",\"protected\":false}]",
    ] {
        let f = Fixture::new();
        let server = Server::new(vec![
            Response::branches(&["main"]),
            Response {
                status: 200,
                body: bad.into(),
                next: None,
                gate: None,
            },
        ]);
        let context = f.context(&server, "account-a", "origin", "project");
        protection::state(&context, "main", true).unwrap();
        let failed = protection::state(&context, "main", true).unwrap();
        assert!(failed.protected);
        assert_eq!(failed.source, "stale");
    }
}
#[test]
fn partial_cycle_or_foreign_page_never_publishes_partial_or_sends_token_elsewhere() {
    let foreign = Server::new(vec![]);
    for next in [
        PAGE_TWO.to_string(),
        "{base}/repos/fixture/project/branches?protected=true&per_page=100&page=1".into(),
        format!("{}/steal", foreign.base),
    ] {
        let f = Fixture::new();
        let server = Server::new(vec![
            Response::branches(&["main"]),
            Response::branches(&["release"]).next(&next),
            Response::failure(503),
        ]);
        let context = f.context(&server, "account-a", "origin", "project");
        protection::state(&context, "main", true).unwrap();
        let failed = protection::state(&context, "main", true).unwrap();
        assert!(failed.protected);
        assert_eq!(failed.source, "stale");
        assert_eq!(
            protection::effective(&f.session, &f.state, Some("origin"), &[]).unwrap(),
            vec!["main"]
        );
    }
    assert!(foreign.requests.lock().unwrap().is_empty());
}
#[test]
fn first_failure_is_unsynced_and_cache_is_scoped_to_account_remote_and_repository() {
    let f = Fixture::new();
    let server = Server::new(vec![
        Response::branches(&["main"]),
        Response::failure(404),
        Response::branches(&["release"]),
        Response::failure(404),
    ]);
    protection::state(
        &f.context(&server, "account-a", "origin", "project"),
        "main",
        true,
    )
    .unwrap();
    f.state.upsert_account(account("account-b")).unwrap();
    f.state.bind_repo(&f.repo, "account-b").unwrap();
    assert!(
        protection::effective(&f.session, &f.state, Some("origin"), &[])
            .unwrap()
            .is_empty()
    );
    let unknown = protection::state(
        &f.context(&server, "account-b", "origin", "project"),
        "main",
        false,
    )
    .unwrap();
    assert_eq!(unknown.source, "unknown");
    assert_eq!(unknown.checked_at_ms, 0);
    f.state.bind_repo(&f.repo, "account-a").unwrap();
    f.git(&[
        "remote",
        "add",
        "upstream",
        "https://github.com/fixture/other.git",
    ]);
    protection::state(
        &f.context(&server, "account-a", "upstream", "other"),
        "release",
        true,
    )
    .unwrap();
    assert_eq!(
        protection::effective(&f.session, &f.state, Some("origin"), &[]).unwrap(),
        vec!["main"]
    );
    assert_eq!(
        protection::effective(&f.session, &f.state, Some("upstream"), &[]).unwrap(),
        vec!["release"]
    );
    f.git(&[
        "remote",
        "set-url",
        "origin",
        "https://github.com/fixture/replacement.git",
    ]);
    assert!(
        protection::effective(&f.session, &f.state, Some("origin"), &[])
            .unwrap()
            .is_empty()
    );
    let unknown = protection::state(
        &f.context(&server, "account-a", "origin", "replacement"),
        "main",
        false,
    )
    .unwrap();
    assert_eq!(unknown.source, "unknown");
    f.git(&[
        "remote",
        "set-url",
        "origin",
        "https://github.com/fixture/project.git",
    ]);
    assert_eq!(
        protection::effective(&f.session, &f.state, Some("origin"), &[]).unwrap(),
        vec!["main"]
    );
}
#[test]
fn old_http_completion_cannot_replace_new_success_or_failure() {
    for latest_ok in [true, false] {
        let f = Fixture::new();
        let gate = Arc::new(Gate::default());
        let server = Server::new(vec![
            Response::branches(&["main"]),
            Response::branches(&[]).gated(&gate),
            if latest_ok {
                Response::branches(&["main", "release"])
            } else {
                Response::failure(403)
            },
        ]);
        let context = f.context(&server, "account-a", "origin", "project");
        protection::state(&context, "main", true).unwrap();
        thread::scope(|threads| {
            let old = threads.spawn(|| protection::state(&context, "main", true));
            gate.wait();
            let latest = protection::state(&context, "main", true);
            gate.release();
            let old = old.join().unwrap().unwrap();
            let latest = latest.unwrap();
            assert!(old.protected);
            assert!(latest.protected);
            assert_eq!(old.source, if latest_ok { "api" } else { "stale" });
        });
        assert_eq!(
            protection::effective(&f.session, &f.state, Some("origin"), &[]).unwrap(),
            if latest_ok {
                vec!["main", "release"]
            } else {
                vec!["main"]
            }
        );
    }
}
#[test]
fn restart_retains_cache_and_replacement_worktree_does_not_adopt_it() {
    let mut f = Fixture::new();
    let server = Server::new(vec![Response::branches(&["main"]), Response::failure(404)]);
    protection::state(
        &f.context(&server, "account-a", "origin", "project"),
        "main",
        true,
    )
    .unwrap();
    let restarted = GithubState::new(&f.data);
    assert_eq!(
        protection::effective(&f.session, &restarted, Some("origin"), &[]).unwrap(),
        vec!["main"]
    );
    let before = fs::read(f.data.join("github-protection-scoped.json")).unwrap();
    fs::rename(f.repo.join(".git"), f._temp.path().join("old-git")).unwrap();
    f.git(&["init", "-b", "main"]);
    f.git(&["config", "user.name", "Replacement fixture"]);
    f.git(&["config", "user.email", "replacement@example.invalid"]);
    f.git(&["config", "commit.gpgsign", "false"]);
    f.git(&["add", "a.txt"]);
    f.git(&["commit", "-m", "replacement"]);
    f.git(&[
        "remote",
        "add",
        "origin",
        "https://github.com/fixture/project.git",
    ]);
    assert!(protection::effective(&f.session, &f.state, Some("origin"), &[]).is_err());
    f.session = WorktreeSession::open(
        "protection-fixture",
        &f.repo,
        f.session.shell_env.clone(),
        Arc::new(CommandLog::default()),
    )
    .unwrap();
    assert!(
        protection::effective(&f.session, &f.state, Some("origin"), &[])
            .unwrap()
            .is_empty()
    );
    let unknown = protection::state(
        &f.context(&server, "account-a", "origin", "project"),
        "main",
        false,
    )
    .unwrap();
    assert_eq!(unknown.source, "unknown");
    assert_eq!(
        fs::read(f.data.join("github-protection-scoped.json")).unwrap(),
        before
    );
}
#[cfg(unix)]
#[test]
fn failed_atomic_cache_write_keeps_original_and_returns_error() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let server = Server::new(vec![Response::branches(&["main"]), Response::branches(&[])]);
    let context = f.context(&server, "account-a", "origin", "project");
    protection::state(&context, "main", true).unwrap();
    let path = f.data.join("github-protection-scoped.json");
    let before = fs::read(&path).unwrap();
    let mode = fs::metadata(&f.data).unwrap().permissions();
    fs::set_permissions(&f.data, fs::Permissions::from_mode(0o555)).unwrap();
    let result = protection::state(&context, "main", true);
    fs::set_permissions(&f.data, mode).unwrap();
    assert!(result.is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(
        protection::effective(&f.session, &f.state, Some("origin"), &[]).unwrap(),
        vec!["main"]
    );
}
