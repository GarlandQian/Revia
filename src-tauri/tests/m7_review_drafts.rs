//! F09：本地草稿身份/正文/发布检查；只用自建仓库、独占appdata和虚构HTTP。
use revia_lib::{
    git::CommandLog,
    m7::github::{
        pr::{self, PrContext, ReviewCommentInput, ReviewEvent, ReviewSubmitRequest},
        reviews::{self, ReviewDraftScope},
        store::ReviewDraft,
        GithubAccount, GithubAuthMethod, GithubClient, GithubState,
    },
    session::WorktreeSession,
    shell::{EnvCaptureInfo, ShellEnv},
};
use std::{
    collections::{HashMap, VecDeque},
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
        let vars = HashMap::from([
            ("HOME".into(), home.display().to_string()),
            (
                "GIT_CONFIG_GLOBAL".into(),
                home.join("gitconfig").display().to_string(),
            ),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
        ]);
        let path = vec!["/usr/bin".into(), "/bin".into(), "/opt/homebrew/bin".into()];
        let env = ShellEnv {
            vars,
            path: path.clone(),
            info: EnvCaptureInfo {
                worktree: repo.display().to_string(),
                shell: "/bin/sh".into(),
                elapsed_ms: 1,
                timed_out: false,
                used_fallback: true,
                var_count: 3,
                path_entries: path,
                diagnostics: None,
            },
        };
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
    bodies: Arc<Mutex<Vec<String>>>,
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
        let bodies = Arc::new(Mutex::new(Vec::new()));
        let received = Arc::clone(&bodies);
        let stop = Arc::new(AtomicBool::new(false));
        let seen = Arc::clone(&requests);
        let done = Arc::clone(&stop);
        let api = base.clone();
        let worker = thread::spawn(move || {
            let mut workers = Vec::new();
            while !done.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let queue = Arc::clone(&queue);
                        let seen = Arc::clone(&seen);
                        let api = api.clone();
                        let received = Arc::clone(&received);
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
                            let split = header.windows(4).position(|v| v == b"\r\n\r\n").unwrap() + 4;
                            let length: usize = String::from_utf8_lossy(&header[..split]).lines().find_map(|line| line.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse().unwrap())).unwrap_or(0);
                            while header.len() < split + length { let n = stream.read(&mut buf).unwrap(); if n == 0 { break; } header.extend_from_slice(&buf[..n]); }
                            received.lock().unwrap().push(String::from_utf8_lossy(&header[split..]).to_string());
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
            bodies,
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
const H1: &str = "1111111111111111111111111111111111111111";
const H2: &str = "2222222222222222222222222222222222222222";
fn scope(f: &Fixture, head: &str) -> ReviewDraftScope {
    ReviewDraftScope::resolve(&f.session, "account-a", "origin", 1, head).unwrap()
}
fn draft(kind: &str, body: &str) -> ReviewDraft {
    ReviewDraft {
        id: String::new(),
        repo_key: String::new(),
        number: 0,
        scope_key: None,
        target_key: None,
        kind: kind.into(),
        head_sha: None,
        path: "a.txt".into(),
        side: "new".into(),
        line: 1,
        body: body.into(),
        created_at_ms: 0,
        updated_at_ms: 0,
    }
}
fn pull(head: &str) -> Response {
    Response {
        status: 200,
        body: serde_json::json!({"number":1,"title":"fixture","state":"open","head":{"sha":head}})
            .to_string(),
        next: None,
        gate: None,
    }
}
fn success() -> Response {
    Response {
        status: 200,
        body: "{}".into(),
        next: None,
        gate: None,
    }
}
fn review(
    f: &Fixture,
    scope: &ReviewDraftScope,
    inline: &ReviewDraft,
    body: &str,
) -> ReviewSubmitRequest {
    ReviewSubmitRequest {
        worktree_id: f.session.id.clone(),
        draft_scope: scope.clone(),
        draft_ids: vec![inline.id.clone()],
        remote: Some("origin".into()),
        number: 1,
        event: ReviewEvent::Comment,
        body: body.into(),
        comments: vec![ReviewCommentInput {
            path: inline.path.clone(),
            side: inline.side.clone(),
            line: inline.line,
            body: inline.body.clone(),
        }],
        expected_head_sha: scope.head_sha.clone(),
    }
}
#[test]
fn local_forms_round_trip_and_old_head_never_enters_current_send_set() {
    let f = Fixture::new();
    let h1 = scope(&f, H1);
    let h2 = scope(&f, H2);
    for kind in ["commentBody", "reviewBody", "inlineForm", "inline"] {
        reviews::save(&f.session, &f.state, &h1, draft(kind, "old text")).unwrap();
    }
    reviews::save(&f.session, &f.state, &h2, draft("reviewBody", "new text")).unwrap();
    let restarted = GithubState::new(&f.data);
    let bundle = reviews::list(&f.session, &restarted, "account-a", "origin", 1, H2).unwrap();
    assert_eq!(bundle.drafts.len(), 1);
    assert_eq!(bundle.older.len(), 4);
    assert_eq!(bundle.drafts[0].body, "new text");
    let original = reviews::list(&f.session, &restarted, "account-a", "origin", 1, H1).unwrap();
    assert_eq!(original.drafts.len(), 4);
    reviews::save(&f.session, &f.state, &h2, draft("reviewBody", "")).unwrap();
    assert_eq!(
        reviews::list(&f.session, &restarted, "account-a", "origin", 1, H2)
            .unwrap()
            .drafts[0]
            .body,
        ""
    );
}
#[test]
fn account_remote_repository_number_and_worktree_are_isolated() {
    let f = Fixture::new();
    let original = scope(&f, H1);
    f.git(&[
        "remote",
        "add",
        "upstream",
        "https://github.com/fixture/project.git",
    ]);
    for other in [
        ReviewDraftScope::resolve(&f.session, "account-b", "origin", 1, H1).unwrap(),
        ReviewDraftScope::resolve(&f.session, "account-a", "upstream", 1, H1).unwrap(),
        ReviewDraftScope::resolve(&f.session, "account-a", "origin", 2, H1).unwrap(),
    ] {
        assert_ne!(other.target_key, original.target_key);
    }
    let saved = reviews::save(&f.session, &f.state, &original, draft("inline", "keep")).unwrap();
    let foreign = ReviewDraftScope::resolve(&f.session, "account-b", "origin", 1, H1).unwrap();
    assert!(reviews::delete(&f.session, &f.state, &foreign, &saved.id).is_err());
    f.git(&[
        "remote",
        "set-url",
        "origin",
        "https://github.com/fixture/other.git",
    ]);
    assert!(reviews::save(
        &f.session,
        &f.state,
        &original,
        draft("reviewBody", "do not move")
    )
    .is_err());
    assert_ne!(scope(&f, H1).key, original.key);
    fs::rename(f.repo.join(".git"), f._temp.path().join("old-git")).unwrap();
    f.git(&["init", "-b", "main"]);
    assert!(original.verify(&f.session).is_err());
    assert_eq!(
        f.state.drafts().list_target(&original.target_key).unwrap()[0].body,
        "keep"
    );
}
#[test]
fn old_head_or_wrong_account_draft_is_rejected_before_any_post() {
    let f = Fixture::new();
    let old = scope(&f, H1);
    let current = scope(&f, H2);
    let inline = reviews::save(&f.session, &f.state, &old, draft("inline", "old anchor")).unwrap();
    let server = Server::new(vec![]);
    let context = f.context(&server, "account-a", "origin", "project");
    assert!(pr::submit_review(&context, &review(&f, &current, &inline, "text")).is_err());
    let wrong = f.context(&server, "account-b", "origin", "project");
    assert!(pr::submit_review(&wrong, &review(&f, &old, &inline, "text")).is_err());
    assert!(server.requests.lock().unwrap().is_empty());
}
#[test]
fn changed_head_and_permission_failure_preserve_body_and_inline() {
    for response in [pull(H2), Response::failure(403)] {
        let f = Fixture::new();
        let scope = scope(&f, H1);
        let inline =
            reviews::save(&f.session, &f.state, &scope, draft("inline", "keep anchor")).unwrap();
        reviews::save(
            &f.session,
            &f.state,
            &scope,
            draft("reviewBody", "keep body"),
        )
        .unwrap();
        let server = Server::new(vec![response]);
        let context = f.context(&server, "account-a", "origin", "project");
        assert!(pr::submit_review(&context, &review(&f, &scope, &inline, "keep body")).is_err());
        assert_eq!(
            f.state
                .drafts()
                .list_target(&scope.target_key)
                .unwrap()
                .len(),
            2
        );
        assert!(server
            .requests
            .lock()
            .unwrap()
            .iter()
            .all(|line| line.starts_with("GET ")));
    }
}
#[test]
fn actual_publish_pins_commit_and_only_clears_unchanged_sent_version() {
    let f = Fixture::new();
    let old = scope(&f, H1);
    let current = scope(&f, H2);
    let old_inline =
        reviews::save(&f.session, &f.state, &old, draft("inline", "old version")).unwrap();
    let inline = reviews::save(
        &f.session,
        &f.state,
        &current,
        draft("inline", "send anchor"),
    )
    .unwrap();
    reviews::save(
        &f.session,
        &f.state,
        &current,
        draft("reviewBody", "send body"),
    )
    .unwrap();
    let gate = Arc::new(Gate::default());
    let server = Server::new(vec![pull(H2), success().gated(&gate)]);
    let context = f.context(&server, "account-a", "origin", "project");
    let request = review(&f, &current, &inline, "send body");
    thread::scope(|threads| {
        let sending = threads.spawn(|| pr::submit_review(&context, &request));
        gate.wait();
        reviews::save(
            &f.session,
            &f.state,
            &current,
            draft("reviewBody", "new input while sending"),
        )
        .unwrap();
        gate.release();
        assert!(sending.join().unwrap().is_ok());
    });
    let all = f.state.drafts().list_target(&current.target_key).unwrap();
    assert!(all.iter().any(|d| d.id == old_inline.id));
    assert!(all.iter().any(|d| d.body == "new input while sending"));
    assert!(!all.iter().any(|d| d.id == inline.id));
    let bodies = server.bodies.lock().unwrap();
    let payload: serde_json::Value = serde_json::from_str(&bodies[1]).unwrap();
    assert_eq!(payload["commit_id"], H2);
    assert_eq!(payload["comments"][0]["body"], "send anchor");
}
#[test]
fn comment_head_change_preserves_local_form() {
    let f = Fixture::new();
    let scope = scope(&f, H1);
    reviews::save(
        &f.session,
        &f.state,
        &scope,
        draft("commentBody", "keep comment"),
    )
    .unwrap();
    let server = Server::new(vec![pull(H2)]);
    let context = f.context(&server, "account-a", "origin", "project");
    assert!(pr::create_comment(&context, 1, "keep comment", &scope).is_err());
    assert_eq!(
        f.state.drafts().list_target(&scope.target_key).unwrap()[0].body,
        "keep comment"
    );
    assert_eq!(server.requests.lock().unwrap().len(), 1);
}
#[test]
fn concurrent_saves_keep_both_targets_and_corrupt_storage_is_not_overwritten() {
    let f = Fixture::new();
    let a = scope(&f, H1);
    let b = scope(&f, H2);
    thread::scope(|threads| {
        let one =
            threads.spawn(|| reviews::save(&f.session, &f.state, &a, draft("reviewBody", "A")));
        let two =
            threads.spawn(|| reviews::save(&f.session, &f.state, &b, draft("reviewBody", "B")));
        one.join().unwrap().unwrap();
        two.join().unwrap().unwrap();
    });
    assert_eq!(
        f.state.drafts().list_target(&a.target_key).unwrap().len(),
        2
    );
    let path = f.data.join("github-review-drafts.json");
    fs::write(&path, b"broken original").unwrap();
    assert!(reviews::save(
        &f.session,
        &f.state,
        &a,
        draft("reviewBody", "must not replace")
    )
    .is_err());
    assert_eq!(fs::read(path).unwrap(), b"broken original");
}
#[cfg(unix)]
#[test]
fn failed_local_write_keeps_original_draft() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let scope = scope(&f, H1);
    reviews::save(
        &f.session,
        &f.state,
        &scope,
        draft("reviewBody", "original"),
    )
    .unwrap();
    let path = f.data.join("github-review-drafts.json");
    let original = fs::read(&path).unwrap();
    let mode = fs::metadata(&f.data).unwrap().permissions();
    fs::set_permissions(&f.data, fs::Permissions::from_mode(0o555)).unwrap();
    let result = reviews::save(&f.session, &f.state, &scope, draft("reviewBody", "failed"));
    fs::set_permissions(&f.data, mode).unwrap();
    assert!(result.is_err());
    assert_eq!(fs::read(path).unwrap(), original);
}

#[cfg(unix)]
#[test]
fn remote_success_with_local_record_failure_is_needs_verification_and_keeps_draft() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let current = scope(&f, H1);
    let inline = reviews::save(&f.session, &f.state, &current, draft("inline", "keep")).unwrap();
    reviews::save(&f.session, &f.state, &current, draft("reviewBody", "body")).unwrap();
    let gate = Arc::new(Gate::default());
    let server = Server::new(vec![pull(H1), success().gated(&gate)]);
    let context = f.context(&server, "account-a", "origin", "project");
    let request = review(&f, &current, &inline, "body");
    let mode = fs::metadata(&f.data).unwrap().permissions();
    let error = thread::scope(|threads| {
        let send = threads.spawn(|| pr::submit_review(&context, &request));
        gate.wait();
        fs::set_permissions(&f.data, fs::Permissions::from_mode(0o555)).unwrap();
        gate.release();
        let result = send.join().unwrap();
        fs::set_permissions(&f.data, mode).unwrap();
        result.unwrap_err()
    });
    assert_eq!(error.kind, revia_lib::error::ErrorKind::NeedsVerification);
    assert!(error.payload().cause.is_some());
    assert_eq!(
        f.state
            .drafts()
            .list_target(&current.target_key)
            .unwrap()
            .len(),
        2
    );
    assert_eq!(server.requests.lock().unwrap().len(), 2);
}
#[test]
fn successful_comment_only_clears_its_exact_local_body() {
    let f = Fixture::new();
    let current = scope(&f, H1);
    let old = scope(&f, H2);
    reviews::save(
        &f.session,
        &f.state,
        &current,
        draft("commentBody", "send comment"),
    )
    .unwrap();
    reviews::save(
        &f.session,
        &f.state,
        &current,
        draft("reviewBody", "keep review"),
    )
    .unwrap();
    reviews::save(
        &f.session,
        &f.state,
        &old,
        draft("commentBody", "keep other head"),
    )
    .unwrap();
    let server = Server::new(vec![pull(H1), success()]);
    let context = f.context(&server, "account-a", "origin", "project");
    pr::create_comment(&context, 1, "send comment", &current).unwrap();
    let all = f.state.drafts().list_target(&current.target_key).unwrap();
    assert_eq!(all.len(), 2);
    assert!(all.iter().any(|d| d.body == "keep review"));
    assert!(all.iter().any(|d| d.body == "keep other head"));
}
