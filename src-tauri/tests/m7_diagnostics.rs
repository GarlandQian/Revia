//! 生产 Git 完成入口 → 七天摘要 → 预览/导出；只用独占 fixture、HOME 和 appdata。
use revia_lib::{
    git::{CommandLog, Git},
    m7::diagnostics::{self, DiagnosticsRequest, DiagnosticsSummaryStore},
    settings::Settings,
    shell::{EnvCaptureInfo, ShellEnv},
};
use std::{collections::HashMap, fs, path::PathBuf, sync::Arc};

struct Fixture {
    _root: tempfile::TempDir,
    repo: PathBuf,
    data: PathBuf,
    env: ShellEnv,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        eprintln!("diagnostics fixture: {}", root_path.display());
        let repo = root_path.join("外置仓库-PATH-PRIVATE");
        let home = root_path.join("home");
        let data = root_path.join("appdata");
        for path in [&repo, &home, &data] {
            fs::create_dir_all(path).unwrap();
        }
        let config = home.join("empty.gitconfig");
        fs::write(&config, "").unwrap();
        let path = vec!["/usr/bin".into(), "/bin".into()];
        let vars = HashMap::from([
            ("HOME".into(), home.display().to_string()),
            ("GIT_CONFIG_GLOBAL".into(), config.display().to_string()),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("GIT_TERMINAL_PROMPT".into(), "0".into()),
            ("PATH".into(), "/usr/bin:/bin".into()),
        ]);
        let env = ShellEnv {
            vars,
            path: path.clone(),
            info: EnvCaptureInfo {
                worktree: repo.display().to_string(),
                shell: "/bin/sh".into(),
                elapsed_ms: 0,
                timed_out: false,
                used_fallback: false,
                var_count: 5,
                path_entries: path,
                diagnostics: None,
            },
        };
        Self {
            _root: root,
            repo,
            data,
            env,
        }
    }
    fn git(&self, log: Arc<CommandLog>, owner: &str) -> Git {
        Git::new(&self.repo, &self.env, log)
            .unwrap()
            .with_worktree_id(owner)
    }
    fn request(&self, owner: Option<&str>) -> DiagnosticsRequest {
        DiagnosticsRequest {
            destination: self.data.join("export.json").display().to_string(),
            worktree_id: owner.map(str::to_string),
            snapshot_id: None,
        }
    }
}

#[cfg(unix)]
#[test]
fn real_hook_output_never_enters_summary_and_export_is_exact_preview() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let store = Arc::new(DiagnosticsSummaryStore::new(&f.data));
    let log = Arc::new(CommandLog::with_diagnostics(Arc::clone(&store)));
    let git = f.git(Arc::clone(&log), "a");
    for args in [
        vec!["init", "-b", "main"],
        vec!["config", "user.name", "Fixture"],
        vec!["config", "user.email", "测试@example.invalid"],
        vec!["config", "commit.gpgsign", "false"],
    ] {
        assert_eq!(git.run(&args).unwrap().exit_code, 0);
    }
    let body = "BODY-PRIVATE 中文正文";
    fs::write(f.repo.join("secret.txt"), body).unwrap();
    git.run(&["add", "--", "secret.txt"])
        .unwrap()
        .into_success("add")
        .unwrap();
    let hook = f.repo.join(".git/hooks/pre-commit");
    fs::write(&hook, format!("#!/bin/sh\nprintf '%s\\n' '{body}' '+++ PRIVATE-DIFF' 'https://example.invalid/PRIVATE-REMOTE' '测试@example.invalid' 'ghp_FAKEPRIVATE' >&2\nexit 1\n")).unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o700)).unwrap();
    assert_ne!(
        git.run(&["commit", "-m", "PRIVATE-MESSAGE"])
            .unwrap()
            .exit_code,
        0
    );
    assert!(
        log.snapshot()
            .iter()
            .any(|record| record.output.contains(body))
    );
    // 未打开诊断界面，摘要已经写出。
    let stored = fs::read_to_string(f.data.join("diagnostics-summary.json")).unwrap();
    let mut settings = Settings::default();
    settings.theme_light = f.repo.join("PRIVATE-THEME.json").display().to_string();
    let mut request = f.request(Some("a"));
    let preview = diagnostics::preview(&request, &settings, &log, &store, None).unwrap();
    for forbidden in [
        body,
        "PRIVATE-DIFF",
        "PRIVATE-REMOTE",
        "测试@example.invalid",
        "ghp_FAKEPRIVATE",
        "PRIVATE-MESSAGE",
        "PRIVATE-THEME",
        "PATH-PRIVATE",
    ] {
        assert!(
            !preview.text.contains(forbidden),
            "preview leaked {forbidden}"
        );
        assert!(!stored.contains(forbidden), "store leaked {forbidden}");
    }
    let parsed: serde_json::Value = serde_json::from_str(&preview.text).unwrap();
    assert_eq!(parsed["failures"].as_array().unwrap().len(), 1);
    assert!(!parsed["history"].as_array().unwrap().is_empty());
    // 两次 UI 操作之间新命令完成，导出仍须等于原来看到的字节和时间。
    git.run(&["status"]).unwrap();
    request.snapshot_id = Some(preview.snapshot_id.clone());
    let exported = diagnostics::export(&request, &store).unwrap();
    assert_eq!(fs::read_to_string(&exported.path).unwrap(), preview.text);
    assert_eq!(
        exported.generated_at_ms,
        parsed["generatedAtMs"].as_u64().unwrap()
    );
    request.worktree_id = Some("b".into());
    assert!(diagnostics::export(&request, &store).is_err());
    request.worktree_id = Some("a".into());
    request.snapshot_id = Some("forged".into());
    assert!(diagnostics::export(&request, &store).is_err());

    let restarted_store = DiagnosticsSummaryStore::new(&f.data);
    let restarted = diagnostics::preview(
        &f.request(Some("a")),
        &settings,
        &CommandLog::default(),
        &restarted_store,
        None,
    )
    .unwrap();
    let restarted: serde_json::Value = serde_json::from_str(&restarted.text).unwrap();
    assert_eq!(restarted["failures"].as_array().unwrap().len(), 1);
    assert!(log.clear_diagnostics(Some("a")).unwrap() > 0);
    assert!(store.list(Some("a")).unwrap().is_empty());
    assert!(log.snapshot().is_empty());
    request.snapshot_id = Some(preview.snapshot_id);
    assert!(diagnostics::export(&request, &store).is_err());
    for _ in 0..2 {
        let p = diagnostics::preview(&request, &settings, &log, &store, None).unwrap();
        let p: serde_json::Value = serde_json::from_str(&p.text).unwrap();
        assert!(p["failures"].as_array().unwrap().is_empty());
    }
}

#[test]
fn scope_expiry_legacy_scrubbing_and_storage_failure_are_observable() {
    let f = Fixture::new();
    let settings = Settings::default();
    let store = Arc::new(DiagnosticsSummaryStore::new(&f.data));
    let log = Arc::new(CommandLog::with_diagnostics(Arc::clone(&store)));
    f.git(Arc::clone(&log), "a").run(&["version"]).unwrap();
    f.git(Arc::clone(&log), "b").run(&["status"]).unwrap(); // self-built, not a Git repository yet
    let no_owner = diagnostics::preview(&f.request(None), &settings, &log, &store, None).unwrap();
    let no_owner: serde_json::Value = serde_json::from_str(&no_owner.text).unwrap();
    assert!(no_owner["commands"].as_array().unwrap().is_empty());
    assert!(no_owner["history"].as_array().unwrap().is_empty());
    assert_eq!(store.list(Some("a")).unwrap().len(), 1);
    assert_eq!(store.list(Some("b")).unwrap().len(), 1);
    log.clear_diagnostics(Some("a")).unwrap();
    assert_eq!(store.list(Some("b")).unwrap().len(), 1);

    let path = f.data.join("diagnostics-summary.json");
    let mut entries: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    entries[0]["startedAtMs"] = 0.into();
    fs::write(&path, serde_json::to_vec(&entries).unwrap()).unwrap();
    assert!(store.list(Some("b")).unwrap().is_empty());
    f.git(Arc::clone(&log), "a").run(&["version"]).unwrap();
    let mut entries: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(entries.as_array().unwrap().len(), 1); // expired row physically removed on next completion
    entries[0]["summary"] = "PRIVATE-BODY".into();
    entries[0]["program"] = "/Volumes/PRIVATE-PATH/git".into();
    entries[0]["args"] = serde_json::json!([
        "version",
        "https://example.invalid/PRIVATE-REMOTE",
        "测试@example.invalid"
    ]);
    fs::write(&path, serde_json::to_vec(&entries).unwrap()).unwrap();
    let fresh = DiagnosticsSummaryStore::new(&f.data);
    let preview = diagnostics::preview(
        &f.request(Some("a")),
        &settings,
        &CommandLog::default(),
        &fresh,
        None,
    )
    .unwrap();
    assert!(!preview.text.contains("PRIVATE-"));
    assert!(!preview.text.contains("测试@example.invalid"));

    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    f.git(Arc::clone(&log), "a").run(&["version"]).unwrap(); // Git result remains usable
    let error =
        diagnostics::preview(&f.request(Some("a")), &settings, &log, &store, None).unwrap_err();
    assert_eq!(error.code, Some("errors.m7.diagnosticsStoreFailed"));
    fs::remove_dir(&path).unwrap();
    log.clear_diagnostics(Some("a")).unwrap();
    assert!(diagnostics::preview(&f.request(Some("a")), &settings, &log, &store, None).is_ok());
}
