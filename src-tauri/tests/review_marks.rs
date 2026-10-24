//! 任务 07：持久审阅标记与连续审阅导航。
//!
//! 全部走生产入口（`WorktreeSession` + `ReviewStore`），用真实 Git 临时仓库验证：
//! HEAD／工作区版本任一变化使标记失效；只暂存／取消暂存保留；重启保留有效标记；
//! 同路径重建不继承；同文件跨组共享且进度去重。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use revia_lib::git::{CommandLog, status::ChangesKind};
use revia_lib::review::ReviewStore;
use revia_lib::session::{ChangeEntry, WorktreeSession};
use revia_lib::shell::{EnvCaptureInfo, ShellEnv};

struct Fixture {
    temp: tempfile::TempDir,
    repo: PathBuf,
    data: PathBuf,
    vars: HashMap<String, String>,
    paths: Vec<String>,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let home = temp.path().join("home");
        let data = temp.path().join("data");
        fs::create_dir_all(&repo).unwrap();
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&data).unwrap();
        fs::write(home.join(".gitconfig"), "").unwrap();
        let mut paths = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|path| path.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        #[cfg(unix)]
        paths.extend(["/opt/homebrew/bin".into(), "/usr/bin".into(), "/bin".into()]);
        let vars = HashMap::from([
            ("HOME".into(), home.display().to_string()),
            ("USERPROFILE".into(), home.display().to_string()),
            ("XDG_CONFIG_HOME".into(), home.display().to_string()),
            ("GIT_CONFIG_GLOBAL".into(), home.join(".gitconfig").display().to_string()),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("GIT_TERMINAL_PROMPT".into(), "0".into()),
            (
                "PATH".into(),
                std::env::join_paths(paths.iter().map(Path::new)).unwrap().to_string_lossy().into_owned(),
            ),
        ]);
        let fixture = Self { temp, repo, data, vars, paths };
        fixture.git(&["init", "-b", "main"]);
        for (key, value) in [
            ("user.name", "Revia Review"),
            ("user.email", "review@example.invalid"),
            ("commit.gpgsign", "false"),
            ("core.autocrlf", "false"),
        ] {
            fixture.git(&["config", key, value]);
        }
        fixture
    }

    fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(&self.repo)
            .env_clear()
            .envs(&self.vars)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "Git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn write(&self, path: &str, bytes: &[u8]) {
        let full = self.repo.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, bytes).unwrap();
    }

    fn commit(&self) {
        self.git(&["add", "--all"]);
        self.git(&["commit", "-m", "fixture"]);
    }

    /// 生产入口：带审阅存储打开会话。
    fn session(&self) -> Arc<WorktreeSession> {
        let shell_env = ShellEnv {
            vars: self.vars.clone(),
            path: self.paths.clone(),
            info: EnvCaptureInfo {
                worktree: self.repo.display().to_string(),
                shell: "fixture".into(),
                elapsed_ms: 0,
                timed_out: false,
                used_fallback: true,
                var_count: self.vars.len(),
                path_entries: self.paths.clone(),
                diagnostics: None,
            },
        };
        WorktreeSession::open_with_review(
            "review-worktree",
            &self.repo,
            shell_env,
            Arc::new(CommandLog::default()),
            Some(Arc::new(ReviewStore::new(self.data.clone()))),
        )
        .unwrap()
    }

    fn entry(&self, session: &WorktreeSession, path: &str) -> ChangeEntry {
        session
            .refresh()
            .unwrap()
            .entries
            .into_iter()
            .find(|entry| entry.path == path)
            .unwrap_or_else(|| panic!("缺少改动条目：{path}"))
    }
}

/// 只暂存／取消暂存不改变 HEAD 与工作区版本，标记必须保留（T12）。
#[test]
fn mark_survives_stage_and_unstage() {
    let f = Fixture::new();
    f.write("a.txt", b"base\n");
    f.commit();
    f.write("a.txt", b"changed\n");

    let session = f.session();
    let entry = f.entry(&session, "a.txt");
    let marks = session.set_review_mark("a.txt", true).unwrap();
    assert_eq!(marks.reviewed, vec!["a.txt".to_string()]);
    assert_eq!((marks.reviewed_count, marks.total), (1, 1));

    // 暂存：进入已暂存组，但两个版本未变 → 标记保留。
    let snapshot = session.refresh().unwrap();
    let unstaged = snapshot.entries.iter().find(|e| e.path == "a.txt").unwrap();
    session.stage_file("a.txt", &unstaged.fingerprint).unwrap();
    let after_stage = session.refresh().unwrap();
    assert!(
        after_stage.entries.iter().any(|e| e.path == "a.txt" && e.changes == ChangesKind::Staged),
        "暂存后应出现在已暂存组"
    );
    assert!(after_stage.entries.iter().filter(|e| e.path == "a.txt").all(|e| e.reviewed));
    assert_eq!(after_stage.review.reviewed, vec!["a.txt".to_string()]);

    // 取消暂存：同样保留。
    let staged = session.refresh().unwrap();
    let staged_entry = staged.entries.iter().find(|e| e.path == "a.txt").unwrap();
    session.unstage_file("a.txt", &staged_entry.fingerprint).unwrap();
    let after_unstage = session.refresh().unwrap();
    assert!(after_unstage.entries.iter().find(|e| e.path == "a.txt").unwrap().reviewed);
}

/// 工作区版本变化使标记失效，且不需要额外提示（Q20、T12、D04）。
#[test]
fn worktree_change_invalidates_mark() {
    let f = Fixture::new();
    f.write("a.txt", b"base\n");
    f.commit();
    f.write("a.txt", b"first change\n");

    let session = f.session();
    f.entry(&session, "a.txt");
    session.set_review_mark("a.txt", true).unwrap();
    assert_eq!(session.refresh().unwrap().review.reviewed_count, 1);

    // 外部继续编辑 → 工作区版本变化。
    f.write("a.txt", b"second change\n");
    let after = session.refresh().unwrap();
    assert_eq!(after.review.reviewed_count, 0, "工作区版本变化必须使标记失效");
    assert!(after.entries.iter().all(|e| !e.reviewed));
}

/// HEAD 版本变化使标记失效（例如提交后同一路径又有了新改动）。
#[test]
fn head_change_invalidates_mark() {
    let f = Fixture::new();
    f.write("a.txt", b"base\n");
    f.commit();
    f.write("a.txt", b"changed\n");

    let session = f.session();
    f.entry(&session, "a.txt");
    session.set_review_mark("a.txt", true).unwrap();

    // 提交这次改动（HEAD 版本变化），再制造新的改动。
    f.commit();
    f.write("a.txt", b"changed again\n");
    let after = session.refresh().unwrap();
    assert_eq!(after.review.reviewed_count, 0, "HEAD 版本变化必须使标记失效");
}

/// 重启后保留有效标记（T10、T12）。
#[test]
fn marks_survive_restart() {
    let f = Fixture::new();
    f.write("a.txt", b"base\n");
    f.write("b.txt", b"base\n");
    f.commit();
    f.write("a.txt", b"changed\n");
    f.write("b.txt", b"changed\n");

    {
        let session = f.session();
        f.entry(&session, "a.txt");
        session.set_review_mark("a.txt", true).unwrap();
    }
    // 重新打开会话（相当于重启应用）：标记仍在。
    let reopened = f.session();
    let snapshot = reopened.refresh().unwrap();
    assert_eq!(snapshot.review.reviewed, vec!["a.txt".to_string()]);
    assert_eq!((snapshot.review.reviewed_count, snapshot.review.total), (1, 2));
    assert!(snapshot.entries.iter().find(|e| e.path == "a.txt").unwrap().reviewed);
    assert!(!snapshot.entries.iter().find(|e| e.path == "b.txt").unwrap().reviewed);
}

/// 同文件跨组共享一个标记，进度按文件去重（T12）。
#[test]
fn same_file_in_both_groups_shares_one_mark_and_counts_once() {
    let f = Fixture::new();
    f.write("a.txt", b"one\ntwo\nthree\n");
    f.commit();
    // 部分暂存：同一文件同时出现在已暂存与未暂存两组。
    f.write("a.txt", b"ONE\ntwo\nthree\n");
    f.git(&["add", "--", "a.txt"]);
    f.write("a.txt", b"ONE\ntwo\nTHREE\n");

    let session = f.session();
    let snapshot = session.refresh().unwrap();
    let groups: Vec<ChangesKind> = snapshot
        .entries
        .iter()
        .filter(|e| e.path == "a.txt")
        .map(|e| e.changes)
        .collect();
    assert_eq!(groups.len(), 2, "同一文件应出现在两组");
    assert_eq!(snapshot.review.total, 1, "进度必须按文件去重");

    session.set_review_mark("a.txt", true).unwrap();
    let marked = session.refresh().unwrap();
    assert!(
        marked.entries.iter().filter(|e| e.path == "a.txt").all(|e| e.reviewed),
        "两组必须共享同一个标记"
    );
    assert_eq!(marked.review.reviewed_count, 1);
    assert_eq!(marked.review.total, 1);
}

/// 同路径重建（身份不同）不继承旧标记（Q87、T10）。
#[test]
fn recreated_worktree_at_same_path_does_not_inherit_marks() {
    let f = Fixture::new();
    f.write("a.txt", b"base\n");
    f.commit();
    f.write("a.txt", b"changed\n");

    {
        let session = f.session();
        f.entry(&session, "a.txt");
        session.set_review_mark("a.txt", true).unwrap();
    }

    // 删掉整个目录并在同一路径重新初始化一个仓库（真实身份不同）。
    fs::remove_dir_all(&f.repo).unwrap();
    fs::create_dir_all(&f.repo).unwrap();
    f.git(&["init", "-b", "main"]);
    for (key, value) in [
        ("user.name", "Revia Review"),
        ("user.email", "review@example.invalid"),
        ("commit.gpgsign", "false"),
    ] {
        f.git(&["config", key, value]);
    }
    f.write("a.txt", b"base\n");
    f.commit();
    f.write("a.txt", b"changed\n");

    let rebuilt = f.session();
    let snapshot = rebuilt.refresh().unwrap();
    assert_eq!(snapshot.review.reviewed_count, 0, "同路径重建不得继承旧标记");
    assert!(snapshot.entries.iter().all(|e| !e.reviewed));
}

/// 取消标记只影响目标文件；不存在的路径被拒绝（不伪造标记）。
#[test]
fn unmark_only_affects_target_and_unknown_paths_are_rejected() {
    let f = Fixture::new();
    f.write("a.txt", b"base\n");
    f.write("b.txt", b"base\n");
    f.commit();
    f.write("a.txt", b"changed\n");
    f.write("b.txt", b"changed\n");

    let session = f.session();
    f.entry(&session, "a.txt");
    session.set_review_mark("a.txt", true).unwrap();
    session.set_review_mark("b.txt", true).unwrap();
    assert_eq!(session.refresh().unwrap().review.reviewed_count, 2);

    session.set_review_mark("a.txt", false).unwrap();
    let after = session.refresh().unwrap();
    assert_eq!(after.review.reviewed, vec!["b.txt".to_string()]);
    assert!(!after.entries.iter().find(|e| e.path == "a.txt").unwrap().reviewed);

    // 不在改动列表里的路径：拒绝，不写入。
    let error = session.set_review_mark("missing.txt", true).unwrap_err();
    assert!(!error.message.is_empty());
    assert_eq!(session.refresh().unwrap().review.reviewed_count, 1);
}

/// 持久化失败如实报错，不伪称保存成功。
#[cfg(unix)]
#[test]
fn persistence_failure_is_reported_not_faked() {
    use std::os::unix::fs::PermissionsExt;

    let f = Fixture::new();
    f.write("a.txt", b"base\n");
    f.commit();
    f.write("a.txt", b"changed\n");
    let session = f.session();
    f.entry(&session, "a.txt");

    // 数据目录设为只读：写入必须失败。
    let original = fs::metadata(&f.data).unwrap().permissions();
    fs::set_permissions(&f.data, fs::Permissions::from_mode(0o500)).unwrap();
    let result = session.set_review_mark("a.txt", true);
    fs::set_permissions(&f.data, original).unwrap();

    let error = result.expect_err("写入失败必须返回错误");
    assert!(!error.message.is_empty(), "错误信息必须可读：{error:?}");
    // 内存里的进度不能因为失败而显示为已审阅。
    assert_eq!(session.refresh().unwrap().review.reviewed_count, 0);
}
