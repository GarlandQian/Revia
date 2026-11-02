//! 真实本地 remote 克隆；mock app 仅接事件，不作为桌面验收。
#![cfg(unix)]
use revia_lib::{
    git::{CommandLog, Git},
    m7::clone::{CloneManager, CloneRequest},
    shell::{EnvCaptureInfo, ShellEnv},
};
use std::{
    collections::HashMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

fn env(home: &Path, repo: &Path) -> ShellEnv {
    fs::create_dir_all(home).unwrap();
    let config = home.join(".gitconfig");
    fs::write(&config, "").unwrap();
    ShellEnv {
        vars: HashMap::from([
            ("HOME".into(), home.display().to_string()),
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("GIT_CONFIG_GLOBAL".into(), config.display().to_string()),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
        ]),
        path: vec!["/usr/bin".into(), "/bin".into()],
        info: EnvCaptureInfo {
            worktree: repo.display().to_string(),
            shell: "/bin/sh".into(),
            elapsed_ms: 0,
            timed_out: false,
            used_fallback: false,
            var_count: 4,
            path_entries: vec![],
            diagnostics: None,
        },
    }
}
fn wait(manager: &CloneManager) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !manager.active().is_empty() {
        assert!(Instant::now() < deadline, "clone did not finish");
        std::thread::sleep(Duration::from_millis(20));
    }
}
#[test]
fn same_manager_uses_current_program_and_logs_actual_redacted_argv() {
    let root = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(root.path()).unwrap();
    eprintln!("clone fixture: {}", base.display());
    let remote = base.join("remote");
    fs::create_dir(&remote).unwrap();
    let env = env(&base.join("home"), &remote);
    let setup = Git::new(&remote, &env, Arc::new(CommandLog::default())).unwrap();
    for args in [
        vec!["init", "-b", "main"],
        vec!["config", "user.name", "Fixture"],
        vec!["config", "user.email", "fixture@example.invalid"],
        vec!["config", "commit.gpgsign", "false"],
    ] {
        setup
            .run(&args)
            .unwrap()
            .into_success("fixture setup")
            .unwrap();
    }
    fs::write(remote.join("content"), "local fixture").unwrap();
    setup
        .run(&["add", "content"])
        .unwrap()
        .into_success("add")
        .unwrap();
    setup
        .run(&["commit", "-m", "fixture"])
        .unwrap()
        .into_success("commit")
        .unwrap();
    let log = Arc::new(CommandLog::default());
    let manager = Arc::new(CloneManager::new(Arc::clone(&log)));
    let app = tauri::test::mock_builder()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();
    for number in 0..2 {
        let program = base.join(format!("selected-git-{number}"));
        fs::write(&program, "#!/bin/sh\nprintf '%s\\n' 'https://fixture-user:fixture-secret@example.invalid/o/r' >&2\nexec /usr/bin/git \"$@\"\n").unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
        let target = base.join(format!("clone-{number}"));
        manager
            .start(
                app.handle().clone(),
                CloneRequest {
                    url: remote.display().to_string(),
                    target: target.display().to_string(),
                    branch: Some("main".into()),
                    depth: Some(1),
                    recurse_submodules: true,
                },
                env.clone(),
                Some(program.clone()),
            )
            .unwrap();
        wait(&manager);
        assert_eq!(
            fs::read_to_string(target.join("content")).unwrap(),
            "local fixture"
        );
        let records = log.snapshot();
        let record = records.last().unwrap();
        assert_eq!(record.program, program.display().to_string());
        assert_eq!(record.exit_code, 0);
        assert_eq!(
            record.args,
            vec![
                "clone",
                "--progress",
                "--branch",
                "main",
                "--depth",
                "1",
                "--recurse-submodules",
                "--",
                remote.to_str().unwrap(),
                target.to_str().unwrap()
            ]
        );
        assert!(!record.output.contains("fixture-secret"));
        assert!(record.output.contains("https://***@example.invalid"));
    }
    // 受控执行器在本地退出，不进行任何网络访问；覆盖 URL 本身的日志打码。
    let fail = base.join("failing-git");
    fs::write(&fail, "#!/bin/sh\nprintf '%s\\n' \"$@\" >&2\nexit 11\n").unwrap();
    fs::set_permissions(&fail, fs::Permissions::from_mode(0o700)).unwrap();
    manager
        .start(
            app.handle().clone(),
            CloneRequest {
                url: "https://fixture-user:fixture-secret@example.invalid/o/r".into(),
                target: base.join("failed").display().to_string(),
                branch: None,
                depth: None,
                recurse_submodules: false,
            },
            env,
            Some(fail),
        )
        .unwrap();
    wait(&manager);
    let records = log.snapshot();
    let last = records.last().unwrap();
    assert_eq!(last.exit_code, 11);
    assert!(!last.args.join(" ").contains("fixture-secret"));
    assert!(!last.output.contains("fixture-secret"));
}
