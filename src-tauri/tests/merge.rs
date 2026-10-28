//! 内置三方合并（Q143，设计第 5 节的测试）。
//!
//! 全部用真实 Git 仓库验证：内容冲突、按块分组与 Git 的合并结果一致、
//! 两边都新增、一边删除一边修改、二进制、rebase 的栏头、关闭“自动应用”时的
//! 初始结果、应用前外部修改被拒、应用先存回收区、应用后不再是未解决冲突、
//! 整文件接受一侧（含指纹校验与删除的一侧）、CRLF 保持、不改 Git 配置、
//! 临时目录被清理。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use revia_lib::error::ErrorKind;
use revia_lib::git::{CommandLog, Git};
use revia_lib::merge::{MergeChunkKind, MergeSide, MergeSideRole, MergeUnavailable, TempDir};
use revia_lib::recovery::{RecoveryStore, WorktreeIdentity};
use revia_lib::session::WorktreeSession;
use revia_lib::shell::{EnvCaptureInfo, ShellEnv};

struct Fixture {
    temp: tempfile::TempDir,
    repo: PathBuf,
    env: HashMap<String, String>,
    paths: Vec<String>,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let home = temp.path().join("home");
        fs::create_dir_all(&repo).unwrap();
        fs::create_dir_all(&home).unwrap();
        fs::write(home.join(".gitconfig"), "").unwrap();
        let mut paths: Vec<String> = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        #[cfg(unix)]
        for extra in ["/opt/homebrew/bin", "/usr/bin", "/bin", "/usr/local/bin"] {
            if !paths.iter().any(|path| path == extra) {
                paths.push(extra.into());
            }
        }
        let mut env = HashMap::from([
            ("HOME".to_string(), home.display().to_string()),
            ("USERPROFILE".to_string(), home.display().to_string()),
            ("XDG_CONFIG_HOME".to_string(), home.display().to_string()),
            ("GIT_CONFIG_GLOBAL".to_string(), home.join(".gitconfig").display().to_string()),
            ("GIT_CONFIG_NOSYSTEM".to_string(), "1".into()),
            ("GIT_TERMINAL_PROMPT".to_string(), "0".into()),
            (
                "PATH".to_string(),
                std::env::join_paths(paths.iter().map(Path::new)).unwrap().to_string_lossy().into_owned(),
            ),
        ]);
        for key in ["SYSTEMROOT", "WINDIR", "TEMP", "TMP", "TMPDIR"] {
            if let Ok(value) = std::env::var(key) {
                env.insert(key.into(), value);
            }
        }
        let fixture = Self { temp, repo, env, paths };
        fixture.git(&["init", "-b", "main"]);
        for (key, value) in [
            ("user.name", "Revia Merge Test"),
            ("user.email", "[EMAIL]"),
            ("commit.gpgsign", "false"),
            ("core.quotePath", "false"),
            ("core.autocrlf", "false"),
            ("merge.conflictStyle", "diff3"),
        ] {
            fixture.git(&["config", key, value]);
        }
        fixture
    }

    fn command(&self) -> Command {
        let mut command = Command::new("git");
        command.current_dir(&self.repo).env_clear().envs(&self.env);
        command
    }

    fn git(&self, args: &[&str]) -> String {
        let output = self.command().args(args).output().expect("execute Git");
        assert!(
            output.status.success(),
            "git {args:?}: {} / {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    /// 允许失败的 Git 调用（merge 冲突时退出码非零）。
    fn git_allow_failure(&self, args: &[&str]) {
        let _ = self.command().args(args).output().expect("execute Git");
    }

    fn write(&self, path: &str, text: &str) {
        let full = self.repo.join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, text).unwrap();
    }

    fn write_bytes(&self, path: &str, bytes: &[u8]) {
        let full = self.repo.join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, bytes).unwrap();
    }

    fn read(&self, path: &str) -> String {
        fs::read_to_string(self.repo.join(path)).unwrap()
    }

    fn track(&self, path: &str, text: &str) {
        self.write(path, text);
        self.git(&["--literal-pathspecs", "add", "--", path]);
        self.git(&["commit", "-m", "fixture"]);
    }

    fn shell_env(&self) -> ShellEnv {
        ShellEnv {
            path: self.paths.clone(),
            vars: self.env.clone(),
            info: EnvCaptureInfo {
                worktree: self.repo.display().to_string(),
                shell: if cfg!(windows) { "cmd.exe" } else { "/bin/zsh" }.into(),
                elapsed_ms: 0,
                timed_out: false,
                used_fallback: true,
                var_count: self.env.len(),
                path_entries: self.paths.clone(),
                diagnostics: None,
            },
        }
    }

    fn open(&self) -> Arc<WorktreeSession> {
        WorktreeSession::open("test-worktree", &self.repo, self.shell_env(), Arc::new(CommandLog::default())).unwrap()
    }

    fn bare_git(&self) -> Git {
        Git::new(&self.repo, &self.shell_env(), Arc::new(CommandLog::default())).unwrap()
    }

    fn store(&self) -> RecoveryStore {
        RecoveryStore::new(self.temp.path().join("data"))
    }

    fn data_dir(&self) -> PathBuf {
        self.temp.path().join("data")
    }

    fn config_list(&self) -> String {
        self.git(&["config", "--list"])
    }

    /// 制造内容冲突：main 与 feature 改同一行。
    fn content_conflict(&self, path: &str) -> String {
        self.track(path, "a\nb\nc\n");
        self.git(&["checkout", "-b", "feature"]);
        self.write(path, "a\nTHEIRS\nc\n");
        self.git(&["--literal-pathspecs", "commit", "-am", "theirs"]);
        self.git(&["checkout", "main"]);
        self.write(path, "a\nOURS\nc\n");
        self.git(&["--literal-pathspecs", "commit", "-am", "ours"]);
        self.git_allow_failure(&["merge", "feature"]);
        path.to_string()
    }
}

/// 制造一次 merge：共同祖先、你的版本、对方的版本三份内容。
fn three_way(fixture: &Fixture, path: &str, base: &str, ours: &str, theirs: &str) {
    fixture.track(path, base);
    fixture.git(&["checkout", "-b", "feature"]);
    fixture.write(path, theirs);
    fixture.git(&["--literal-pathspecs", "commit", "-am", "theirs"]);
    fixture.git(&["checkout", "main"]);
    fixture.write(path, ours);
    fixture.git(&["--literal-pathspecs", "commit", "-am", "ours"]);
    fixture.git_allow_failure(&["merge", "feature"]);
}

fn lines_of(text: &str, start: u32, end: u32) -> String {
    text.split_inclusive('\n').skip(start as usize).take((end - start) as usize).collect()
}

#[test]
fn merge_conflict_layout_reports_sides_chunks_and_result() {
    let fixture = Fixture::new();
    let path = fixture.content_conflict("f.txt");
    let session = fixture.open();
    session.refresh().unwrap();

    let layout = session.merge_layout(&fixture.data_dir(), &path, true).unwrap();
    assert_eq!(layout.unavailable, None);
    assert_eq!(layout.left_label.role, MergeSideRole::CurrentBranch);
    assert_eq!(layout.left_label.name.as_deref(), Some("main"));
    assert_eq!(layout.right_label.role, MergeSideRole::MergedIn);
    assert_eq!(layout.right_label.name.as_deref(), Some("feature"));
    assert_eq!(layout.chunks.len(), 1);
    assert_eq!(layout.chunks[0].kind, MergeChunkKind::Conflict);
    // 冲突块在结果里先放共同祖先的内容，不放冲突标记。
    assert_eq!(layout.result_text, "a\nb\nc\n");
    assert_eq!(lines_of(&layout.left_text, layout.chunks[0].ours_start, layout.chunks[0].ours_end), "OURS\n");
    assert_eq!(lines_of(&layout.right_text, layout.chunks[0].theirs_start, layout.chunks[0].theirs_end), "THEIRS\n");
    assert!(!layout.fingerprint.is_empty());
}

#[test]
fn chunks_match_git_merge_file_and_auto_apply() {
    let fixture = Fixture::new();
    let base = "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n";
    // 你的版本：改第 2 行、在第 9 行后插入；对方：改第 5 行、同样改第 9 行后插入（不同内容 → 冲突）、
    // 两边都把 ten 改成 TEN（相同 → 不冲突）。
    let ours = "one\nTWO\nthree\nfour\nfive\nsix\nseven\neight\nnine\nours-extra\nTEN\n";
    let theirs = "one\ntwo\nthree\nfour\nFIVE\nsix\nseven\neight\nnine\ntheirs-extra\nTEN\n";
    three_way(&fixture, "f.txt", base, ours, theirs);
    let session = fixture.open();
    session.refresh().unwrap();

    let layout = session.merge_layout(&fixture.data_dir(), "f.txt", true).unwrap();
    assert_eq!(layout.unavailable, None, "按块分组应与 git merge-file 一致");
    let kinds: Vec<_> = layout.chunks.iter().map(|chunk| chunk.kind).collect();
    assert_eq!(kinds, vec![MergeChunkKind::Ours, MergeChunkKind::Theirs, MergeChunkKind::Conflict]);
    // 自动应用：不冲突的块已放进结果，冲突块是共同祖先（第 10 行 ten 前面什么都没插）。
    assert_eq!(layout.result_text, "one\nTWO\nthree\nfour\nFIVE\nsix\nseven\neight\nnine\nten\n");
    assert!(layout.chunks[0].ours_applied && !layout.chunks[0].theirs_applied);
    assert!(layout.chunks[1].theirs_applied);
    assert!(!layout.chunks[2].ours_applied && !layout.chunks[2].theirs_applied);
    for chunk in &layout.chunks {
        assert_eq!(
            lines_of(&layout.result_text, chunk.result_start, chunk.result_end),
            match chunk.kind {
                MergeChunkKind::Ours => lines_of(&layout.left_text, chunk.ours_start, chunk.ours_end),
                MergeChunkKind::Theirs => lines_of(&layout.right_text, chunk.theirs_start, chunk.theirs_end),
                _ => lines_of(&layout.base_text, chunk.base_start, chunk.base_end),
            },
            "{chunk:?}"
        );
    }

    // 关闭自动应用：结果就是共同祖先，所有块都待处理（» / « 有对象）。
    let manual = session.merge_layout(&fixture.data_dir(), "f.txt", false).unwrap();
    assert_eq!(manual.result_text, base);
    assert_eq!(manual.chunks.len(), 3);
    assert!(manual.chunks.iter().all(|chunk| !chunk.ours_applied && !chunk.theirs_applied));
}

#[test]
fn apply_writes_result_saves_recovery_and_marks_resolved() {
    let fixture = Fixture::new();
    let path = fixture.content_conflict("f.txt");
    let session = fixture.open();
    session.refresh().unwrap();
    let store = fixture.store();
    let layout = session.merge_layout(&fixture.data_dir(), &path, true).unwrap();
    let config_before = fixture.config_list();

    let resolved = "a\nBOTH\nc\n";
    let outcome = session.merge_apply(&path, resolved, &layout.fingerprint, &store).unwrap();
    assert!(outcome.recovery_record_id.is_some(), "应用前先存回收区");
    assert_eq!(fixture.read(&path), resolved);
    assert!(revia_lib::merge::read_stages(&fixture.bare_git(), Some(&path)).unwrap().is_empty(), "不再是未解决冲突");
    assert_eq!(fixture.git(&["show", ":0:f.txt"]), resolved, "暂存区就是写入的结果");
    let records = store.list(WorktreeIdentity::capture(&fixture.bare_git()).ok().as_ref()).unwrap();
    let record = records.iter().find(|record| record.paths.contains(&path.to_string())).expect("回收记录");
    let _ = record;
    assert_eq!(fixture.config_list(), config_before, "不改 Git 配置");
}

#[test]
fn apply_rejects_when_file_changed_after_layout() {
    let fixture = Fixture::new();
    let path = fixture.content_conflict("f.txt");
    let session = fixture.open();
    session.refresh().unwrap();
    let store = fixture.store();
    let layout = session.merge_layout(&fixture.data_dir(), &path, true).unwrap();
    fixture.write(&path, "a\nEXTERNAL\nc\n");
    let error = session.merge_apply(&path, "a\nBOTH\nc\n", &layout.fingerprint, &store).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Rejected, "{error}");
    assert_eq!(fixture.read(&path), "a\nEXTERNAL\nc\n", "拒绝时不写工作区");
    assert!(!revia_lib::merge::read_stages(&fixture.bare_git(), Some(&path)).unwrap().is_empty(), "仍是未解决冲突");
}

#[test]
fn accept_side_takes_that_version_and_marks_resolved() {
    let fixture = Fixture::new();
    let path = fixture.content_conflict("f.txt");
    let session = fixture.open();
    session.refresh().unwrap();
    let store = fixture.store();
    let conflicts = session.list_conflicts().unwrap();
    assert_eq!(conflicts.len(), 1);
    let outcomes = session
        .merge_accept_side(&[(path.clone(), conflicts[0].fingerprint.clone())], MergeSide::Ours, &store)
        .unwrap();
    assert_eq!(outcomes.len(), 1);
    assert!(outcomes[0].recovery_record_id.is_some());
    assert_eq!(fixture.read(&path), "a\nOURS\nc\n");
    assert!(revia_lib::merge::read_stages(&fixture.bare_git(), Some(&path)).unwrap().is_empty());
}

#[test]
fn accept_side_rejects_when_file_changed_after_listing() {
    let fixture = Fixture::new();
    let path = fixture.content_conflict("f.txt");
    let session = fixture.open();
    session.refresh().unwrap();
    let store = fixture.store();
    let conflicts = session.list_conflicts().unwrap();
    fixture.write(&path, "a\nEDITED BY HAND\nc\n");
    let error = session
        .merge_accept_side(&[(path.clone(), conflicts[0].fingerprint.clone())], MergeSide::Theirs, &store)
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Rejected, "{error}");
    assert_eq!(fixture.read(&path), "a\nEDITED BY HAND\nc\n", "外部改动不能被覆盖");
}

#[test]
fn binary_conflict_only_allows_accepting_a_side() {
    let fixture = Fixture::new();
    fixture.write_bytes("bin.dat", &[0, 1, 2, 3, 0, 255]);
    fixture.git(&["--literal-pathspecs", "add", "--", "bin.dat"]);
    fixture.git(&["commit", "-m", "binary"]);
    fixture.git(&["checkout", "-b", "feature"]);
    fixture.write_bytes("bin.dat", &[0, 9, 9, 9, 0, 255]);
    fixture.git(&["--literal-pathspecs", "commit", "-am", "theirs"]);
    fixture.git(&["checkout", "main"]);
    fixture.write_bytes("bin.dat", &[0, 7, 7, 7, 0, 255]);
    fixture.git(&["--literal-pathspecs", "commit", "-am", "ours"]);
    fixture.git_allow_failure(&["merge", "feature"]);

    let session = fixture.open();
    session.refresh().unwrap();
    let layout = session.merge_layout(&fixture.data_dir(), "bin.dat", true).unwrap();
    assert_eq!(layout.unavailable, Some(MergeUnavailable::Binary));
    assert!(layout.chunks.is_empty() && layout.result_text.is_empty());
    let store = fixture.store();
    let conflicts = session.list_conflicts().unwrap();
    assert_eq!(conflicts[0].unavailable, Some(MergeUnavailable::Binary));
    let outcomes = session
        .merge_accept_side(&[("bin.dat".to_string(), conflicts[0].fingerprint.clone())], MergeSide::Theirs, &store)
        .unwrap();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(fs::read(fixture.repo.join("bin.dat")).unwrap(), vec![0, 9, 9, 9, 0, 255], "接受对方版本");
}

#[test]
fn both_sides_added_has_no_common_ancestor() {
    let fixture = Fixture::new();
    fixture.track("keep.txt", "keep\n");
    fixture.git(&["checkout", "-b", "feature"]);
    fixture.write("added.txt", "theirs\n");
    fixture.git(&["--literal-pathspecs", "add", "--", "added.txt"]);
    fixture.git(&["commit", "-m", "theirs added"]);
    fixture.git(&["checkout", "main"]);
    fixture.write("added.txt", "ours\n");
    fixture.git(&["--literal-pathspecs", "add", "--", "added.txt"]);
    fixture.git(&["commit", "-m", "ours added"]);
    fixture.git_allow_failure(&["merge", "feature"]);

    let session = fixture.open();
    session.refresh().unwrap();
    let layout = session.merge_layout(&fixture.data_dir(), "added.txt", true).unwrap();
    assert_eq!(layout.unavailable, None, "两边都新增仍可三栏合并（共同祖先为空）");
    assert_eq!(layout.chunks.len(), 1);
    assert_eq!(layout.chunks[0].kind, MergeChunkKind::Conflict);
    assert_eq!(layout.result_text, "", "冲突块里共同祖先为空");
}

#[test]
fn delete_modify_conflict_cannot_merge_but_accepts_a_side() {
    let fixture = Fixture::new();
    fixture.track("gone.txt", "a\nb\n");
    fixture.git(&["checkout", "-b", "feature"]);
    fixture.write("gone.txt", "a\nCHANGED\n");
    fixture.git(&["--literal-pathspecs", "commit", "-am", "theirs modified"]);
    fixture.git(&["checkout", "main"]);
    fixture.git(&["--literal-pathspecs", "rm", "--", "gone.txt"]);
    fixture.git(&["commit", "-m", "ours deleted"]);
    fixture.git_allow_failure(&["merge", "feature"]);

    let session = fixture.open();
    session.refresh().unwrap();
    let layout = session.merge_layout(&fixture.data_dir(), "gone.txt", true).unwrap();
    assert_eq!(layout.unavailable, Some(MergeUnavailable::OursDeleted));
    let store = fixture.store();
    let error = session.merge_apply("gone.txt", "x\n", &layout.fingerprint, &store).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Rejected, "不能三栏合并时拒绝“应用”：{error}");
    session
        .merge_accept_side(&[("gone.txt".to_string(), layout.fingerprint.clone())], MergeSide::Theirs, &store)
        .unwrap();
    assert_eq!(fixture.read("gone.txt"), "a\nCHANGED\n");
}

#[test]
fn accepting_the_deleted_side_removes_the_file_from_the_index() {
    let fixture = Fixture::new();
    fixture.track("gone.txt", "a\nb\n");
    fixture.git(&["checkout", "-b", "feature"]);
    fixture.write("gone.txt", "a\nCHANGED\n");
    fixture.git(&["--literal-pathspecs", "commit", "-am", "theirs modified"]);
    fixture.git(&["checkout", "main"]);
    fixture.git(&["--literal-pathspecs", "rm", "--", "gone.txt"]);
    fixture.git(&["commit", "-m", "ours deleted"]);
    fixture.git_allow_failure(&["merge", "feature"]);

    let session = fixture.open();
    session.refresh().unwrap();
    let store = fixture.store();
    let layout = session.merge_layout(&fixture.data_dir(), "gone.txt", true).unwrap();
    // 接受“你的版本”（删除）：用 git rm 标记为已解决，先存回收区。
    let outcomes = session
        .merge_accept_side(&[("gone.txt".to_string(), layout.fingerprint.clone())], MergeSide::Ours, &store)
        .unwrap();
    assert!(outcomes[0].recovery_record_id.is_some());
    assert!(!fixture.repo.join("gone.txt").exists());
    let status = fixture.git(&["status", "--porcelain=v2"]);
    assert!(!status.contains("gone.txt"), "文件不再出现在改动列表：{status}");
}

#[test]
fn crlf_newline_style_is_preserved() {
    let fixture = Fixture::new();
    three_way(&fixture, "crlf.txt", "a\r\nb\r\nc\r\n", "a\r\nOURS\r\nc\r\n", "a\r\nTHEIRS\r\nc\r\n");
    let session = fixture.open();
    session.refresh().unwrap();
    let store = fixture.store();
    let layout = session.merge_layout(&fixture.data_dir(), "crlf.txt", true).unwrap();
    assert!(layout.crlf);
    assert!(!layout.result_text.contains('\r'), "界面拿到的文本统一用 \\n");
    session.merge_apply("crlf.txt", "a\nBOTH\nc\n", &layout.fingerprint, &store).unwrap();
    assert_eq!(fs::read(fixture.repo.join("crlf.txt")).unwrap(), b"a\r\nBOTH\r\nc\r\n", "保持 CRLF");
}

#[test]
fn rebase_conflict_uses_upstream_and_commit_labels() {
    let fixture = Fixture::new();
    fixture.track("f.txt", "a\nb\nc\n");
    fixture.git(&["checkout", "-b", "feature"]);
    fixture.write("f.txt", "a\nFEATURE\nc\n");
    fixture.git(&["--literal-pathspecs", "commit", "-am", "feature change"]);
    fixture.git(&["checkout", "main"]);
    fixture.write("f.txt", "a\nMAIN\nc\n");
    fixture.git(&["--literal-pathspecs", "commit", "-am", "main change"]);
    fixture.git(&["checkout", "feature"]);
    fixture.git_allow_failure(&["rebase", "main"]);

    let session = fixture.open();
    session.refresh().unwrap();
    let layout = session.merge_layout(&fixture.data_dir(), "f.txt", true).unwrap();
    assert_eq!(layout.left_label.role, MergeSideRole::Upstream);
    assert!(layout.left_label.name.as_deref().is_some_and(|name| name.ends_with("main change")), "{:?}", layout.left_label);
    assert_eq!(layout.right_label.role, MergeSideRole::YourCommit);
    assert!(layout.right_label.name.as_deref().is_some_and(|name| name.ends_with("feature change")), "{:?}", layout.right_label);
    assert_eq!(layout.chunks.iter().filter(|chunk| chunk.kind == MergeChunkKind::Conflict).count(), 1);
}

#[test]
fn temp_directory_is_cleaned_up() {
    let fixture = Fixture::new();
    let path = fixture.content_conflict("f.txt");
    let session = fixture.open();
    session.refresh().unwrap();
    let dir = TempDir::create(&fixture.data_dir()).unwrap();
    let created = dir.path().to_path_buf();
    assert!(created.is_dir());
    drop(dir);
    assert!(!created.exists(), "临时目录用完删除");
    let _ = session.merge_layout(&fixture.data_dir(), &path, true).unwrap();
    let leftovers: Vec<_> = fs::read_dir(fixture.data_dir().join("tmp"))
        .map(|entries| entries.filter_map(Result::ok).collect())
        .unwrap_or_default();
    assert!(leftovers.is_empty(), "合并后不留临时目录：{leftovers:?}");
}

#[test]
fn user_diff_settings_do_not_change_the_chunks() {
    let fixture = Fixture::new();
    for (key, value) in [("diff.algorithm", "histogram"), ("diff.external", "false"), ("diff.indentHeuristic", "true")] {
        fixture.git(&["config", key, value]);
    }
    // 你的版本在两个函数之间插入一个函数（缩进启发式会挪动插入位置），并改了第一行；
    // 对方也改了第一行（冲突），另外给 b 加了函数体。
    let base = "fn a() {\n}\n\nfn b() {\n}\n";
    let ours = "fn a(x) {\n}\n\nfn new() {\n}\n\nfn b() {\n}\n";
    let theirs = "fn a(y) {\n}\n\nfn b() {\n  body\n}\n";
    three_way(&fixture, "f.rs", base, ours, theirs);
    let session = fixture.open();
    session.refresh().unwrap();
    let layout = session.merge_layout(&fixture.data_dir(), "f.rs", true).unwrap();
    assert_eq!(layout.unavailable, None, "外部 diff 与算法配置不影响分块");
}

/// 随机生成三方内容，按块分组必须与 `git merge-file` 一致（不出现“对不上”）。
#[test]
fn chunk_grouping_agrees_with_git_on_random_edits() {
    let fixture = Fixture::new();
    fixture.track("seed.txt", "seed\n");
    let git = fixture.bare_git();
    let mut state: u64 = 0x5eed_1234_abcd_0001;
    let mut next = |bound: u64| {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (state >> 33) % bound
    };
    let mut inconsistent = Vec::new();
    for case in 0..150 {
        let base: Vec<String> = (0..(5 + next(25))).map(|line| format!("line {}", line % 7)).collect();
        let mut edit = |lines: &[String], tag: &str| -> String {
            let mut out = Vec::new();
            for line in lines {
                match next(10) {
                    0 => {}                                              // 删除
                    1 => out.push(format!("{tag} {}", next(3))),         // 改
                    2 => { out.push(line.clone()); out.push(format!("{tag} new {}", next(3))); } // 插入
                    _ => out.push(line.clone()),
                }
            }
            let mut text = out.join("\n");
            if next(5) != 0 && !text.is_empty() {
                text.push('\n');
            }
            text
        };
        let ours = edit(&base, "o");
        // 四分之一的用例两侧用同一个标记，制造“两边改得一样”。
        let theirs = edit(&base, if case % 4 == 0 { "o" } else { "t" });
        let base_text = base.join("\n") + "\n";
        let merged = revia_lib::merge::chunk_merge(
            &git,
            &fixture.data_dir(),
            base_text.as_bytes(),
            ours.as_bytes(),
            theirs.as_bytes(),
            true,
        )
        .unwrap();
        if merged.is_none() {
            inconsistent.push(case);
        }
    }
    assert!(inconsistent.is_empty(), "与 git merge-file 对不上的用例：{inconsistent:?}");
}
