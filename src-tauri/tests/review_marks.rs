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

    /// 真实路径根（tempdir 在 macOS 上是 /var 别名）。
    fn root(&self) -> PathBuf {
        self.temp.path().canonicalize().unwrap()
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
    let marks = session.set_review_mark("a.txt", true, None).unwrap();
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
    session.set_review_mark("a.txt", true, None).unwrap();
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
    session.set_review_mark("a.txt", true, None).unwrap();

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
        session.set_review_mark("a.txt", true, None).unwrap();
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

    session.set_review_mark("a.txt", true, None).unwrap();
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
        session.set_review_mark("a.txt", true, None).unwrap();
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
    session.set_review_mark("a.txt", true, None).unwrap();
    session.set_review_mark("b.txt", true, None).unwrap();
    assert_eq!(session.refresh().unwrap().review.reviewed_count, 2);

    session.set_review_mark("a.txt", false, None).unwrap();
    let after = session.refresh().unwrap();
    assert_eq!(after.review.reviewed, vec!["b.txt".to_string()]);
    assert!(!after.entries.iter().find(|e| e.path == "a.txt").unwrap().reviewed);

    // 不在改动列表里的路径：拒绝，不写入。
    let error = session.set_review_mark("missing.txt", true, None).unwrap_err();
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
    let result = session.set_review_mark("a.txt", true, None);
    fs::set_permissions(&f.data, original).unwrap();

    let error = result.expect_err("写入失败必须返回错误");
    assert!(!error.message.is_empty(), "错误信息必须可读：{error:?}");
    // 内存里的进度不能因为失败而显示为已审阅。
    assert_eq!(session.refresh().unwrap().review.reviewed_count, 0);
}

/// ST07-1 / SP07-7：不同 worktree 并发保存不能互相覆盖（存储级事务 + 独占临时文件）。
#[test]
fn concurrent_marks_from_two_worktrees_are_all_kept() {
    let f = Fixture::new();
    f.write("a.txt", b"base\n");
    f.commit();
    f.write("a.txt", b"changed\n");

    let session = f.session();
    f.entry(&session, "a.txt");
    let identity = revia_lib::recovery::WorktreeIdentity::capture(&session.git).unwrap();
    let other_identity = {
        // 另一个身份：真实路径不同、标识不同（同路径重建同理）。
        let mut clone = identity.clone();
        clone.worktree_path = identity.worktree_path.join("other");
        clone.filesystem_ids = vec![Some("other-inode".into()); 3];
        clone
    };
    let store = Arc::new(ReviewStore::new(f.data.clone()));

    // 两个身份并发写入各自的标记：不能互相覆盖。
    let mut handles = Vec::new();
    for (tag, id) in [("a.txt", identity.clone()), ("b.txt", other_identity.clone())] {
        let store = store.clone();
        handles.push(std::thread::spawn(move || {
            for _ in 0..8 {
                let versions = revia_lib::review::ReviewVersions {
                    head: Some(tag.into()),
                    worktree: Some(tag.into()),
                };
                store.set_mark(&id, tag, versions, true).unwrap();
            }
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(store.marks(&identity).unwrap().len(), 1);
    assert_eq!(store.marks(&other_identity).unwrap().len(), 1);
}

/// ST07-3 / SP07-5：用户所见版本与执行时不一致时必须拒绝，不能把新版本标成已审阅。
#[test]
fn stale_expected_versions_are_rejected() {
    let f = Fixture::new();
    f.write("a.txt", b"base\n");
    f.commit();
    f.write("a.txt", b"first\n");
    let session = f.session();
    let entry = f.entry(&session, "a.txt");
    let _ = &entry;
    let seen = revia_lib::review::ReviewVersions {
        head: entry.head_version.clone(),
        worktree: entry.worktree_version.clone(),
    };

    // 外部继续编辑 → 当前版本变化。
    f.write("a.txt", b"second\n");
    session.refresh().unwrap();
    let error = session.set_review_mark("a.txt", true, Some(seen)).unwrap_err();
    assert!(error.message.contains("已变化"), "必须因版本变化被拒：{error:?}");
    assert_eq!(session.refresh().unwrap().review.reviewed_count, 0, "不得持久化未看过的版本");

    // 用当前版本则成功。
    let current = f.entry(&session, "a.txt");
    let versions = revia_lib::review::ReviewVersions {
        head: current.head_version.clone(),
        worktree: current.worktree_version.clone(),
    };
    session.set_review_mark("a.txt", true, Some(versions)).unwrap();
    assert_eq!(session.refresh().unwrap().review.reviewed_count, 1);
}

/// ST07-2 / SP07-6：活动会话里同路径重建后，旧标记不得继续被认领。
#[test]
fn active_session_reconfirms_identity_and_stops_claiming_marks() {
    let f = Fixture::new();
    f.write("a.txt", b"base\n");
    f.commit();
    f.write("a.txt", b"changed\n");
    let session = f.session();
    f.entry(&session, "a.txt");
    session.set_review_mark("a.txt", true, None).unwrap();
    assert_eq!(session.refresh().unwrap().review.reviewed_count, 1);

    // 同一路径重建：删掉 .git 并重新初始化，再恢复成同样的改动内容。
    fs::remove_dir_all(f.repo.join(".git")).unwrap();
    let output = Command::new("git")
        .current_dir(&f.repo)
        .env_clear()
        .envs(&f.vars)
        .args(["init", "-b", "main"])
        .output()
        .unwrap();
    assert!(output.status.success());
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

    // 同一个会话（未重开）继续刷新：身份已变，不能认领旧标记。
    let after = session.refresh().unwrap();
    assert_eq!(after.review.reviewed_count, 0, "活动会话必须重新确认身份");
}

/// ST07-7：存储损坏不能被吞成「全部未审阅」。
#[test]
fn corrupt_store_surfaces_an_error() {
    let f = Fixture::new();
    f.write("a.txt", b"base\n");
    f.commit();
    f.write("a.txt", b"changed\n");
    fs::write(f.data.join("review.json"), b"{ not json").unwrap();

    let session = f.session();
    let error = session.refresh().unwrap_err();
    assert!(
        error.message.contains("审阅") || error.message.contains("解析"),
        "存储损坏必须如实报错：{error:?}"
    );
}

/// ST07-R3-4 / SP07-R3-3：packed-refs 回归必须走**真实符号分支** + 生产标记入口。
///
/// 主体是子模块（两个分支的提交不同，检出版本真的会变）：
/// 1. 检出符号分支（HEAD 是 `ref: refs/heads/...`，不是 detached SHA）；
/// 2. `pack-refs --all` 并断言 loose ref **真的消失**；
/// 3. 经生产 `set_review_mark` 标记；
/// 4. 切到另一个**也只在 packed-refs 里**的分支；
/// 5. 刷新后标记必须失效。
///
/// 手工读 `HEAD`/`refs/...` 的实现拿不到 packed 分支，会退化成常量版本、错误保留标记。
/// 同时断言两种 `.git` 形态都真实存在：子模块的 `.git` 是指针文件，
/// 它的 gitDir 与 commonDir 是真实目录。
#[test]
fn packed_refs_symbolic_branch_switch_invalidates_submodule_mark() {
    let f = Fixture::new();
    f.write("outer.txt", b"base\n");
    f.commit();

    let run = |dir: &Path, args: &[&str]| -> String {
        let output = Command::new("git")
            .current_dir(dir)
            .env_clear()
            .envs(&f.vars)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?} 失败：{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    };

    // 子模块源仓库：main 上一个提交，branch-b 上是另一个提交。
    let source = f.root().join("submodule-source");
    fs::create_dir_all(&source).unwrap();
    run(&source, &["init", "-b", "main"]);
    run(&source, &["config", "user.name", "Sub"]);
    run(&source, &["config", "user.email", "sub@example.invalid"]);
    run(&source, &["config", "commit.gpgsign", "false"]);
    fs::write(source.join("file.txt"), b"one\n").unwrap();
    run(&source, &["add", "--all"]);
    run(&source, &["commit", "-m", "one"]);
    let first_commit = run(&source, &["rev-parse", "HEAD"]).trim().to_string();
    run(&source, &["checkout", "-b", "branch-b"]);
    fs::write(source.join("file.txt"), b"two\n").unwrap();
    run(&source, &["add", "--all"]);
    run(&source, &["commit", "-m", "two"]);
    let second_commit = run(&source, &["rev-parse", "HEAD"]).trim().to_string();
    assert_ne!(first_commit, second_commit);

    // 真实 submodule add，并检出**符号分支**（不是 detached SHA）。
    run(&f.repo, &["-c", "protocol.file.allow=always", "submodule", "add", source.to_str().unwrap(), "sub"]);
    f.commit();
    run(&f.repo, &["-c", "protocol.file.allow=always", "submodule", "update", "--init", "--recursive"]);
    let sub_dir = f.repo.join("sub");
    run(&sub_dir, &["checkout", "main"]);
    let head_text = fs::read_to_string(sub_dir.join(".git")).unwrap();
    assert!(
        head_text.trim().starts_with("gitdir:"),
        "子模块的 .git 应是指针文件：{head_text:?}"
    );
    let sub_git_dir = {
        let raw = head_text.trim().strip_prefix("gitdir:").unwrap().trim();
        let candidate = PathBuf::from(raw);
        if candidate.is_absolute() { candidate } else { sub_dir.join(candidate) }
    };
    assert!(sub_git_dir.is_dir(), "子模块的 gitDir 必须是真实目录");
    let sub_head = fs::read_to_string(sub_git_dir.join("HEAD")).unwrap();
    assert!(
        sub_head.trim().starts_with("ref: refs/heads/"),
        "子模块 HEAD 必须是符号引用：{sub_head:?}"
    );

    // 打包引用，并确认 loose ref 真的消失。
    run(&sub_dir, &["pack-refs", "--all"]);
    let loose_main = sub_git_dir.join("refs").join("heads").join("main");
    let loose_b = sub_git_dir.join("refs").join("heads").join("branch-b");
    assert!(!loose_main.exists(), "pack-refs 后 loose ref 必须消失：{}", loose_main.display());
    assert!(!loose_b.exists(), "pack-refs 后 loose ref 必须消失：{}", loose_b.display());
    let packed = fs::read_to_string(sub_git_dir.join("packed-refs")).unwrap();
    assert!(packed.contains("refs/heads/main"), "main 必须进入 packed-refs");
    assert!(packed.contains("refs/heads/branch-b"), "branch-b 必须进入 packed-refs");

    // 生产入口：标记。
    let session = f.session();
    assert!(session.refresh().unwrap().entries.iter().any(|e| e.path == "sub"));
    session.set_review_mark("sub", true, None).unwrap();
    assert_eq!(session.refresh().unwrap().review.reviewed_count, 1, "标记应生效");

    // 切到另一个同样只存在于 packed-refs 的分支。
    run(&sub_dir, &["checkout", "branch-b"]);
    assert!(!sub_git_dir.join("refs").join("heads").join("branch-b").exists());
    let after = session.refresh().unwrap();
    assert_eq!(
        after.review.reviewed_count, 0,
        "packed 分支切换后标记必须失效（不能退化常量版本）"
    );
}

/// ST07-R2-1：子模块目录拿不到真实版本时，不能生成可认领的常量版本。
#[test]
fn unreadable_submodule_version_is_not_claimable() {
    let f = Fixture::new();
    f.write("outer.txt", b"base\n");
    f.commit();
    // 普通目录（不是子模块）：目录本身没有 Git 版本可读。
    fs::create_dir_all(f.repo.join("plain-dir")).unwrap();
    let session = f.session();
    let version = revia_lib::review::worktree_version(Some(&session.git), &f.repo, "plain-dir").unwrap();
    assert!(version.is_none(), "读不到真实版本时必须返回 None，而不是常量：{version:?}");
}

/// ST07-R2-2 / SP07-R2-2：linked worktree 的 `.git` 改指另一个**仍存在**的 gitDir 后，
/// 不能继续沿用旧身份认领标记（旧目录还在，只 stat 旧路径看不出来）。
#[test]
fn gitdir_repointed_to_existing_directory_invalidates_identity() {
    let f = Fixture::new();
    f.write("a.txt", b"base\n");
    f.commit();
    f.write("a.txt", b"changed\n");

    // 真实 linked worktree：`.git` 是指向主仓库 .git/worktrees/<name> 的文件指针。
    let linked = f.root().join("linked");
    let linked_arg = revia_lib::platform::external_path(&linked);
    f.git(&["worktree", "add", "-b", "other", linked_arg.to_str().unwrap()]);

    let linked_session = WorktreeSession::open_with_review(
        "linked-review",
        &linked,
        ShellEnv {
            vars: f.vars.clone(),
            path: f.paths.clone(),
            info: EnvCaptureInfo {
                worktree: linked.display().to_string(),
                shell: "fixture".into(),
                elapsed_ms: 0,
                timed_out: false,
                used_fallback: true,
                var_count: f.vars.len(),
                path_entries: f.paths.clone(),
                diagnostics: None,
            },
        },
        Arc::new(CommandLog::default()),
        Some(Arc::new(ReviewStore::new(f.data.clone()))),
    )
    .unwrap();

    // 在 linked worktree 里制造改动并标记。
    fs::write(linked.join("a.txt"), b"linked change\n").unwrap();
    let snapshot = linked_session.refresh().unwrap();
    assert!(snapshot.entries.iter().any(|e| e.path == "a.txt"));
    linked_session.set_review_mark("a.txt", true, None).unwrap();
    assert_eq!(linked_session.refresh().unwrap().review.reviewed_count, 1);

    // 把 linked worktree 的 `.git` 指向**另一个仍然存在**的 gitDir：
    // 复制一份主仓库的 .git/worktrees/<name> 目录，再把指针改过去。
    let pointer = linked.join(".git");
    let original = fs::read_to_string(&pointer).unwrap();
    let original_dir = {
        let raw = original.trim().strip_prefix("gitdir:").unwrap().trim();
        let candidate = PathBuf::from(raw);
        if candidate.is_absolute() { candidate } else { linked.join(candidate) }
    };
    let other_dir = original_dir.parent().unwrap().join("other-identity");
    assert!(!other_dir.exists(), "目标目录名不应已存在");
    copy_dir(&original_dir, &other_dir);
    // 让 gitDir 的 commondir 指回同一个真实仓库：路径变了、归属其实也变了。
    fs::write(&pointer, format!("gitdir: {}\n", other_dir.display())).unwrap();

    // 同一个会话继续刷新：Git 现在解析出的是另一个 gitDir，旧身份不得继续认领标记。
    let after = linked_session.refresh().unwrap();
    assert_eq!(
        after.review.reviewed_count, 0,
        "gitDir 改指另一个现存目录后不得继承旧标记"
    );
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// ST07-R3-3 / ST07-R2-2：linked worktree 的 gitDir 不变，但 `commondir` 改指另一个
/// **仍然存在**的 commonDir 时，实际归属已变，不得继续认领旧标记。
#[test]
fn commondir_repointed_while_gitdir_unchanged_invalidates_identity() {
    let f = Fixture::new();
    f.write("a.txt", b"base\n");
    f.commit();
    f.write("a.txt", b"changed\n");

    let linked = f.root().join("linked");
    let linked_arg = revia_lib::platform::external_path(&linked);
    f.git(&["worktree", "add", "-b", "other", linked_arg.to_str().unwrap()]);
    fs::write(linked.join("a.txt"), b"linked change\n").unwrap();

    let linked_session = open_linked(&f, &linked);
    assert!(linked_session.refresh().unwrap().entries.iter().any(|e| e.path == "a.txt"));
    linked_session.set_review_mark("a.txt", true, None).unwrap();
    assert_eq!(linked_session.refresh().unwrap().review.reviewed_count, 1);

    // gitDir 不变，只把它的 `commondir` 指向另一个真实存在的 commonDir 副本。
    let pointer = linked.join(".git");
    let text = fs::read_to_string(&pointer).unwrap();
    let raw = text.trim().strip_prefix("gitdir:").unwrap().trim();
    let git_dir = {
        let candidate = PathBuf::from(raw);
        if candidate.is_absolute() { candidate } else { linked.join(candidate) }
    };
    let original_common = fs::read_to_string(git_dir.join("commondir")).unwrap();
    let original_common = {
        let candidate = PathBuf::from(original_common.trim());
        if candidate.is_absolute() { candidate } else { git_dir.join(candidate) }
    };
    let other_common = f.root().join("other-common-dir");
    copy_dir(&original_common, &other_common);
    // 旧 commonDir 仍然存在（这是关键：不能靠「旧目录消失」才判定失效）。
    assert!(original_common.is_dir(), "旧 commonDir 必须仍然存在");
    fs::write(git_dir.join("commondir"), format!("{}\n", other_common.display())).unwrap();

    let after = linked_session.refresh().unwrap();
    assert_eq!(
        after.review.reviewed_count, 0,
        "commonDir 改指另一个现存目录后不得继承旧标记"
    );
}

/// ST07-R3-3：Unix 上根目录名含换行时，身份捕获与重新确认都必须无歧义。
#[cfg(unix)]
#[test]
fn identity_capture_handles_newline_in_repository_path() {
    let f = Fixture::new();
    // 真实仓库根目录名里带换行（Unix 合法文件名）。
    let newline_root = f.root().join("repo\nwith-newline");
    fs::create_dir_all(&newline_root).unwrap();
    let output = Command::new("git")
        .current_dir(&newline_root)
        .env_clear()
        .envs(&f.vars)
        .args(["init", "-b", "main"])
        .output()
        .unwrap();
    assert!(output.status.success(), "init 失败：{output:?}");
    for (key, value) in [("user.name", "NL"), ("user.email", "nl@example.invalid")] {
        let out = Command::new("git")
            .current_dir(&newline_root)
            .env_clear()
            .envs(&f.vars)
            .args(["config", key, value])
            .output()
            .unwrap();
        assert!(out.status.success());
    }
    fs::write(newline_root.join("a.txt"), b"base\n").unwrap();
    for args in [vec!["add", "--all"], vec!["commit", "-m", "base"]] {
        let out = Command::new("git")
            .current_dir(&newline_root)
            .env_clear()
            .envs(&f.vars)
            .args(&args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?} 失败：{out:?}");
    }
    fs::write(newline_root.join("a.txt"), b"changed\n").unwrap();

    let session = open_linked(&f, &newline_root);
    let identity = revia_lib::recovery::WorktreeIdentity::capture(&session.git)
        .expect("换行路径必须能捕获身份");
    // 捕获出的 gitDir/commonDir 必须真实存在且指向这个仓库。
    assert!(identity.git_dir.is_dir(), "gitDir 必须存在：{}", identity.git_dir.display());
    assert_eq!(
        identity.git_dir,
        newline_root.canonicalize().unwrap().join(".git"),
        "gitDir 必须是这个含换行目录里的 .git，而不是被拆错的前缀"
    );
    assert!(identity.revalidate(&session.git).is_some(), "重新确认也必须成功");

    // 生产入口：标记与刷新都正常。
    assert!(session.refresh().unwrap().entries.iter().any(|e| e.path == "a.txt"));
    session.set_review_mark("a.txt", true, None).unwrap();
    assert_eq!(session.refresh().unwrap().review.reviewed_count, 1);
}

fn open_linked(f: &Fixture, path: &Path) -> Arc<WorktreeSession> {
    WorktreeSession::open_with_review(
        "linked-session",
        path,
        ShellEnv {
            vars: f.vars.clone(),
            path: f.paths.clone(),
            info: EnvCaptureInfo {
                worktree: path.display().to_string(),
                shell: "fixture".into(),
                elapsed_ms: 0,
                timed_out: false,
                used_fallback: true,
                var_count: f.vars.len(),
                path_entries: f.paths.clone(),
                diagnostics: None,
            },
        },
        Arc::new(CommandLog::default()),
        Some(Arc::new(ReviewStore::new(f.data.clone()))),
    )
    .unwrap()
}

/// ST07-R3-2 / SP07-R3-2：未初始化子模块（`submodule status` 前缀 `-`）不能当作已确认版本。
///
/// `-` 时 Git 给出的是索引里的 gitlink OID，不是当前检出版本：暂存 gitlink 变化后
/// deinit → 标记 → init（同 OID）会让空目录变成真实内容却保留标记。
#[test]
fn deinitialized_submodule_is_not_confirmed_and_cannot_be_claimed() {
    let f = Fixture::new();
    f.write("outer.txt", b"base\n");
    f.commit();

    let run = |dir: &Path, args: &[&str]| -> String {
        let output = Command::new("git")
            .current_dir(dir)
            .env_clear()
            .envs(&f.vars)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?} 失败：{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    };

    let source = f.root().join("sub-source");
    fs::create_dir_all(&source).unwrap();
    run(&source, &["init", "-b", "main"]);
    run(&source, &["config", "user.name", "Sub"]);
    run(&source, &["config", "user.email", "sub@example.invalid"]);
    run(&source, &["config", "commit.gpgsign", "false"]);
    fs::write(source.join("f.txt"), b"one\n").unwrap();
    run(&source, &["add", "--all"]);
    run(&source, &["commit", "-m", "one"]);

    run(&f.repo, &["-c", "protocol.file.allow=always", "submodule", "add", source.to_str().unwrap(), "sub"]);
    f.commit();
    run(&f.repo, &["-c", "protocol.file.allow=always", "submodule", "update", "--init", "--recursive"]);
    // 让父仓库里的 gitlink 相对基线产生改动：子模块里新提交并暂存到父仓库，
    // 这样 `sub` 会稳定出现在改动列表里（不靠「文件离开列表」兜底）。
    fs::write(f.repo.join("sub").join("f.txt"), b"two\n").unwrap();
    run(&f.repo.join("sub"), &["add", "--all"]);
    run(&f.repo.join("sub"), &["commit", "-m", "two"]);
    run(&f.repo, &["add", "--", "sub"]);

    let session = f.session();
    let snapshot = session.refresh().unwrap();
    assert!(
        snapshot.entries.iter().any(|e| e.path == "sub"),
        "子模块 gitlink 改动后必须出现在改动列表：{:?}",
        snapshot.entries.iter().map(|e| &e.path).collect::<Vec<_>>()
    );
    session.set_review_mark("sub", true, None).unwrap();
    assert_eq!(session.refresh().unwrap().review.reviewed_count, 1);

    // deinit：子模块目录清空，`submodule status` 前缀变成 `-`。
    run(&f.repo, &["submodule", "deinit", "-f", "--", "sub"]);
    let status_line = run(&f.repo, &["submodule", "status", "--", "sub"]);
    assert!(status_line.starts_with('-'), "deinit 后必须是未初始化状态：{status_line:?}");
    assert!(
        revia_lib::review::worktree_version(Some(&session.git), &f.repo, "sub")
            .unwrap()
            .is_none(),
        "未初始化子模块不能给出可认领的检出版本"
    );

    // 生产入口：刷新后旧标记失效（版本无法确认）。
    assert_eq!(
        session.refresh().unwrap().review.reviewed_count, 0,
        "未初始化后不得继续认领旧标记"
    );
    // 而且不能新增认领。
    let error = session.set_review_mark("sub", true, None).unwrap_err();
    assert!(
        error.message.contains("无法确认"),
        "未初始化子模块不能新增认领：{error:?}"
    );

    // 重新 init（同 OID）：仍然不能把「空目录变真实内容」当作已看过。
    run(&f.repo, &["-c", "protocol.file.allow=always", "submodule", "update", "--init", "--recursive"]);
    let after_init = session.refresh().unwrap();
    assert_eq!(
        after_init.review.reviewed_count, 0,
        "重新 init 后不能凭空恢复旧标记"
    );
}

/// ST07-R3-1 / SP07-R3-1：版本无法确认的目录不能经生产入口新增认领，但取消仍然允许。
#[test]
fn unconfirmed_directory_cannot_be_claimed_but_can_be_unmarked() {
    let f = Fixture::new();
    f.write("a.txt", b"base\n");
    f.commit();
    // 未登记为子模块的**嵌套仓库**：Git 把它显示成一个目录条目（`nested/`），
    // 但没有 Git 版本可读，因此版本无法确认（ST07-R3-1 的原始场景）。
    let nested = f.repo.join("nested");
    fs::create_dir_all(&nested).unwrap();
    let init = Command::new("git")
        .current_dir(&nested)
        .env_clear()
        .envs(&f.vars)
        .args(["init", "-b", "main"])
        .output()
        .unwrap();
    assert!(init.status.success(), "嵌套仓库 init 失败：{init:?}");
    fs::write(nested.join("n.txt"), b"n\n").unwrap();

    let session = f.session();
    let snapshot = session.refresh().unwrap();
    let entry = snapshot
        .entries
        .iter()
        .find(|e| e.path.starts_with("nested"))
        .expect("嵌套仓库应出现在改动列表");
    assert!(!entry.worktree_version_confirmed, "嵌套仓库的版本应无法确认");
    let target = entry.path.clone();

    // 新增认领被拒绝。
    let error = session.set_review_mark(&target, true, None).unwrap_err();
    assert!(error.message.contains("无法确认"), "必须拒绝未知版本：{error:?}");
    assert_eq!(session.refresh().unwrap().review.reviewed_count, 0, "不能计入进度");

    // 正常文件仍可标记。
    f.write("a.txt", b"changed\n");
    session.refresh().unwrap();
    session.set_review_mark("a.txt", true, None).unwrap();
    let marked = session.refresh().unwrap();
    assert_eq!(marked.review.reviewed_count, 1);
    assert!(marked.entries.iter().find(|e| e.path == "a.txt").unwrap().reviewed);

    // 取消能力保留：取消一个已存在的标记不需要版本确认。
    session.set_review_mark("a.txt", false, None).unwrap();
    assert_eq!(session.refresh().unwrap().review.reviewed_count, 0);
}

/// Q20：标记按**该文件**的 HEAD 版本 + 工作区版本判定。
///
/// 只切换分支、HEAD 提交变了，但这个文件的两个版本都没变时，标记必须**保留**
/// （不能因为全局 HEAD 变化就失效，那会偏离既有的文件级版本定义）。
#[test]
fn unrelated_head_change_keeps_mark_when_file_versions_unchanged() {
    let f = Fixture::new();
    f.write("a.txt", b"base\n");
    f.write("b.txt", b"base\n");
    f.commit();
    // 两个分支指向同一提交（内容完全相同）。
    f.git(&["branch", "alias"]);
    f.write("a.txt", b"changed\n");

    let session = f.session();
    assert!(session.refresh().unwrap().entries.iter().any(|e| e.path == "a.txt"));
    session.set_review_mark("a.txt", true, None).unwrap();
    assert_eq!(session.refresh().unwrap().review.reviewed_count, 1);

    // 切到另一个分支：HEAD 引用变了，但 a.txt 的 HEAD blob 与工作区字节都没变。
    f.git(&["checkout", "alias"]);
    let after = session.refresh().unwrap();
    assert_eq!(
        after.review.reviewed_count, 1,
        "目标文件两个版本都没变时，标记必须保留"
    );
    assert!(after.entries.iter().find(|e| e.path == "a.txt").unwrap().reviewed);
}
