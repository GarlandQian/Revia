//! 受控device flow/刷新 HTTP + 独占凭据文件；子进程显式验收变量避免任何真实钥匙串操作。
use revia_lib::m7::github::{
    GithubState,
    auth::{self, DevicePoll},
};
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

fn child(case: &str, root: &Path) {
    assert_eq!(std::env::var("REVIA_ACCEPTANCE").unwrap(), "1");
    assert_eq!(
        std::env::var("REVIA_GITHUB_TOKEN_STORE").unwrap(),
        root.join("tokens").display().to_string()
    );
    let data = root.join("appdata");
    fs::create_dir(&data).unwrap();
    let repo = root.join("repo");
    fs::create_dir(&repo).unwrap();
    let state = GithubState::new(&data);
    let flow = auth::start_device_flow(&state).unwrap();
    std::thread::sleep(Duration::from_millis(flow.interval_ms + 30));
    match auth::poll_device_flow(&state, &flow.flow_id).unwrap() {
        DevicePoll::Authorized { account } => assert_eq!(account.login, "fixture-account"),
        _ => panic!("fixture did not authorize"),
    }
    let saved = state.tokens.credentials("fixture-account").unwrap();
    assert!(saved.expires_at_ms.unwrap() <= revia_lib::m7::now_ms());
    let before = fs::read(root.join("tokens/fixture-account.txt")).unwrap();
    if case == "rotate" {
        let (_, client) = state.client_for(&repo).unwrap();
        let (user, _): (serde_json::Value, _) = client.get_json("/user").unwrap();
        assert_eq!(user["login"], "fixture-account");
        let rotated = state.tokens.credentials("fixture-account").unwrap();
        assert_eq!(rotated.access_token, "ghu_FAKE-ROTATED-ACCESS");
        assert_eq!(
            rotated.refresh_token.as_deref(),
            Some("ghr_FAKE-ROTATED-REFRESH")
        );
        assert!(rotated.expires_at_ms.unwrap() > revia_lib::m7::now_ms() + 60_000);
        assert!(!root.join("tokens/fixture-account.refresh.txt").exists());
        let restarted = GithubState::new(&data);
        let (_, client) = restarted.client_for(&repo).unwrap();
        let _: (serde_json::Value, _) = client.get_json("/user").unwrap();
        // 并发取客户端复用新凭据，不重复刷新、也不复用旧refresh。
        std::thread::scope(|scope| {
            for _ in 0..3 {
                scope.spawn(|| {
                    assert!(state.client_for(&repo).is_ok());
                });
            }
        });
    } else {
        let error = state
            .client_for(&repo)
            .err()
            .expect("expired credentials must fail");
        assert_eq!(error.code, Some("errors.m7.githubNotLoggedIn"));
        assert_eq!(
            fs::read(root.join("tokens/fixture-account.txt")).unwrap(),
            before
        );
    }
    let accounts = fs::read_to_string(data.join("github-accounts.json")).unwrap();
    assert!(
        !accounts.contains("FAKE-"),
        "ordinary config must not contain credentials"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(root.join("tokens/fixture-account.txt"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    let legacy = root.join("tokens/fixture-account.refresh.txt");
    fs::create_dir(&legacy).unwrap();
    assert!(
        auth::logout(&state, "fixture-account").is_err(),
        "cleanup failure must not be swallowed"
    );
    assert_eq!(
        state.accounts().len(),
        1,
        "account removal is not reported before credential cleanup"
    );
    assert!(state.client_for(&repo).is_err());
    fs::remove_dir(&legacy).unwrap();
    auth::logout(&state, "fixture-account").unwrap();
    assert!(!root.join("tokens/fixture-account.txt").exists());
    assert!(state.accounts().is_empty());
    assert!(state.client_for(&repo).is_err());
    // 旧版分条目仍可读取；兼容写接口迁移为同一原子条目，不留下旧refresh。
    state
        .tokens
        .save("legacy-fixture", "ghu_FAKE-LEGACY")
        .unwrap();
    let legacy = root.join("tokens/legacy-fixture.refresh.txt");
    fs::write(&legacy, "ghr_FAKE-LEGACY").unwrap();
    assert_eq!(
        state.tokens.load_refresh("legacy-fixture").as_deref(),
        Some("ghr_FAKE-LEGACY")
    );
    state
        .tokens
        .save_refresh("legacy-fixture", "ghr_FAKE-MIGRATED")
        .unwrap();
    assert_eq!(
        state.tokens.load("legacy-fixture").unwrap(),
        "ghu_FAKE-LEGACY"
    );
    assert_eq!(
        state.tokens.load_refresh("legacy-fixture").as_deref(),
        Some("ghr_FAKE-MIGRATED")
    );
    assert!(!legacy.exists());
    state.tokens.delete("legacy-fixture").unwrap();
}

fn request(stream: &mut TcpStream) -> (String, String) {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).unwrap(), 1);
        header.push(byte[0]);
    }
    let header = String::from_utf8(header).unwrap();
    let length: usize = header
        .lines()
        .find_map(|line| {
            line.split_once(':').and_then(|(key, value)| {
                key.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse().unwrap())
            })
        })
        .unwrap_or(0);
    let mut body = vec![0; length];
    stream.read_exact(&mut body).unwrap();
    (header, String::from_utf8(body).unwrap())
}
#[test]
fn device_tokens_refresh_with_rotation_and_isolated_storage() {
    if let Ok(case) = std::env::var("REVIA_M7_AUTH_CHILD") {
        child(
            &case,
            Path::new(&std::env::var("REVIA_M7_AUTH_ROOT").unwrap()),
        );
        return;
    }
    for case in ["rotate", "denied", "expired", "redirect"] {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        eprintln!("GitHub fake-token fixture {case}: {}", root.display());
        fs::create_dir(root.join("home")).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let done = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&done);
        let server = std::thread::spawn(move || {
            let mut device = 0;
            let mut refresh = 0;
            let mut users = 0;
            let deadline = Instant::now() + Duration::from_secs(20);
            while !stop.load(Ordering::SeqCst) {
                let (mut stream, _) = match listener.accept() {
                    Ok(client) => client,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "fixture timed out");
                        std::thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                    Err(error) => panic!("accept: {error}"),
                };
                let (header, body) = request(&mut stream);
                let (status, reply) = if header.starts_with("POST /login/device/code ") {
                    assert!(body.contains("client_id=fixture-client"));
                    (
                        200,
                        serde_json::json!({"device_code":"FAKE-DEVICE","user_code":"FAKE-CODE","expires_in":60,"interval":1}),
                    )
                } else if header.starts_with("POST /login/oauth/access_token ") {
                    assert!(!body.contains("client_secret"));
                    if body.contains("grant_type=refresh_token") {
                        refresh += 1;
                        assert_eq!(refresh, 1, "no automatic retry");
                        assert!(body.contains("refresh_token=ghr_FAKE-INITIAL-REFRESH"));
                        if case == "denied" || case == "redirect" {
                            (
                                if case == "redirect" { 307 } else { 401 },
                                serde_json::json!({"error":"bad_refresh_token"}),
                            )
                        } else {
                            assert_eq!(case, "rotate");
                            (
                                200,
                                serde_json::json!({"access_token":"ghu_FAKE-ROTATED-ACCESS","refresh_token":"ghr_FAKE-ROTATED-REFRESH","expires_in":3600,"refresh_token_expires_in":7200}),
                            )
                        }
                    } else {
                        device += 1;
                        assert!(body.contains("device_code=FAKE-DEVICE"));
                        (
                            200,
                            serde_json::json!({"access_token":"ghu_FAKE-INITIAL-ACCESS","refresh_token":"ghr_FAKE-INITIAL-REFRESH","expires_in":0,"refresh_token_expires_in":if case=="expired" {0}else{7200}}),
                        )
                    }
                } else {
                    assert!(header.starts_with("GET /user "));
                    users += 1;
                    let expected = if users == 1 {
                        "ghu_FAKE-INITIAL-ACCESS"
                    } else {
                        "ghu_FAKE-ROTATED-ACCESS"
                    };
                    assert!(header.to_ascii_lowercase().contains(&format!(
                        "authorization: bearer {}",
                        expected.to_ascii_lowercase()
                    )));
                    (
                        200,
                        serde_json::json!({"login":"fixture-account","name":"Fixture","avatar_url":null}),
                    )
                };
                let bytes = serde_json::to_vec(&reply).unwrap();
                let redirect = if status == 307 {
                    "Location: /credential-leak\r\n"
                } else {
                    ""
                };
                write!(stream,"HTTP/1.1 {status} Fixture\r\n{redirect}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",bytes.len()).unwrap();
                stream.write_all(&bytes).unwrap();
            }
            assert_eq!(device, 1);
            assert_eq!(refresh, if case == "expired" { 0 } else { 1 });
            assert_eq!(users, if case == "rotate" { 3 } else { 1 });
        });
        let result = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "device_tokens_refresh_with_rotation_and_isolated_storage",
                "--nocapture",
            ])
            .env("REVIA_M7_AUTH_CHILD", case)
            .env("REVIA_M7_AUTH_ROOT", &root)
            .env("HOME", root.join("home"))
            .env("REVIA_ACCEPTANCE", "1")
            .env("REVIA_GITHUB_TOKEN_STORE", root.join("tokens"))
            .env("REVIA_GITHUB_CLIENT_ID", "fixture-client")
            .env("REVIA_GITHUB_WEB_BASE", &base)
            .env("REVIA_GITHUB_API_BASE", &base)
            .env("NO_PROXY", "127.0.0.1,localhost")
            .env_remove("HTTP_PROXY")
            .env_remove("HTTPS_PROXY")
            .env_remove("ALL_PROXY")
            .output()
            .unwrap();
        std::io::stdout().write_all(&result.stdout).unwrap();
        std::io::stderr().write_all(&result.stderr).unwrap();
        done.store(true, Ordering::SeqCst);
        server.join().unwrap();
        assert!(result.status.success(), "child case {case} failed");
    }
}
