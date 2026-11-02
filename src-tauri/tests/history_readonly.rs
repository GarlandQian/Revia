//! M5 任务 01：只读提交历史的后端回归。
//!
//! 全部用例都在自建的真实 Git 临时仓库里，通过生产入口（`HistoryStore` / `HistoryTarget`）
//! 读取；应用数据目录也独立分配，不碰用户仓库、用户配置或临时交接 fixture。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;

use revia_lib::git::rows::ViewMode;
use revia_lib::git::segments::{IgnoreWhitespace, WordGranularity};
use revia_lib::git::{CommandLog, Git};
use revia_lib::history::{
    HistoryFile, HistoryFileStatus, HistoryFileTarget, HistoryStart, HistoryStore,
};
use revia_lib::session::{FileVersion, WorktreeSession};
use revia_lib::shell::{EnvCaptureInfo, ShellEnv};

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    repo: PathBuf,
    repo2: PathBuf,
    /// 独立的 Revia 应用数据目录。历史读取本身不持久化任何东西，但仍然按
    /// 本地验证约定单独分配，确保测试不碰用户数据目录。
    #[allow(dead_code)]
    data: PathBuf,
    env: HashMap<String, String>,
    paths: Vec<String>,
    session: Arc<WorktreeSession>,
    store: HistoryStore,
}

/// 用给定环境在指定目录执行 Git（构造 fixture 时还没有 `Fixture` 自身）。
fn run_git_in(repo: &Path, env: &HashMap<String, String>, args: &[&str]) -> Output {
    Command::new("git")
        .current_dir(repo)
        .env_clear()
        .envs(env)
        .args(args.iter().map(|argument| revia_lib::platform::external_path(Path::new(argument))))
        .output()
        .expect("执行 Git")
}

fn build_shell_env(
    paths: &[String],
    env: &HashMap<String, String>,
    id: &str,
) -> ShellEnv {
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
            diagnostics: None,
        },
    }
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("创建临时目录");
        let root = fs::canonicalize(temp.path()).expect("规范化临时目录");
        let repo = root.join("repo");
        let repo2 = root.join("repo2");
        let home = root.join("home");
        let data = root.join("data");
        fs::create_dir_all(&repo).unwrap();
        fs::create_dir_all(&repo2).unwrap();
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&data).unwrap();
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
            ("GIT_CONFIG_GLOBAL".into(), revia_lib::platform::external_path(&global_config).display().to_string()),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("GIT_TERMINAL_PROMPT".into(), "0".into()),
            ("PATH".into(), joined_path.to_string_lossy().into_owned()),
        ]);
        for key in ["SYSTEMROOT", "WINDIR", "TEMP", "TMP", "TMPDIR"] {
            if let Ok(value) = std::env::var(key) {
                env.insert(key.into(), value);
            }
        }
        for target in [&repo, &repo2] {
            let output = run_git_in(target, &env, &["init", "-b", "main"]);
            assert!(output.status.success(), "git init: {}", String::from_utf8_lossy(&output.stderr));
            for (key, value) in [
                ("user.name", "Revia History Test"),
                ("user.email", "history@example.com"),
                ("commit.gpgsign", "false"),
                ("core.quotePath", "false"),
                ("core.autocrlf", "false"),
                ("core.safecrlf", "false"),
            ] {
                let output = run_git_in(target, &env, &["config", key, value]);
                assert!(output.status.success(), "git config {key}");
            }
        }
        let session = WorktreeSession::open(
            "test-worktree",
            &repo,
            build_shell_env(&paths, &env, "test-worktree"),
            Arc::new(CommandLog::default()),
        )
        .expect("打开会话");
        Self {
            _temp: temp,
            root,
            repo,
            repo2,
            data,
            env,
            paths,
            session,
            store: HistoryStore::new(),
        }
    }

    fn shell_env(&self, id: &str) -> ShellEnv {
        build_shell_env(&self.paths, &self.env, id)
    }

    fn command_in(&self, repo: &Path) -> Command {
        let mut command = Command::new("git");
        command.current_dir(repo).env_clear().envs(&self.env);
        command
    }

    fn git_output_in(&self, repo: &Path, args: &[&str]) -> Output {
        self.command_in(repo).args(args.iter().map(|argument| revia_lib::platform::external_path(Path::new(argument)))).output().expect("执行 Git")
    }

    fn git_in(&self, repo: &Path, args: &[&str]) -> String {
        let output = self.git_output_in(repo, args);
        assert!(
            output.status.success(),
            "git {args:?}: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn git(&self, args: &[&str]) -> String {
        self.git_in(&self.repo, args)
    }

    fn bytes(&self, repo: &Path, args: &[&str]) -> Vec<u8> {
        let output = self.git_output_in(repo, args);
        assert!(output.status.success(), "git {args:?} 失败");
        output.stdout
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

    fn commit_all(&self, message: &str) -> String {
        self.git(&["--literal-pathspecs", "add", "-A"]);
        self.git(&["commit", "-m", message]);
        self.head()
    }

    fn track(&self, path: &str, bytes: &[u8], message: &str) -> String {
        self.write(path, bytes);
        self.commit_all(message)
    }

    /// 打开一个新的历史目标，返回 `(target_id, info)`。
    fn open(&self) -> (String, revia_lib::history::HistoryTargetInfo) {
        let info = self.store.open(&self.session).expect("打开历史目标");
        (info.target_id.clone(), info)
    }

    fn target(&self, id: &str) -> Arc<revia_lib::history::HistoryTarget> {
        self.store.target(id).expect("取得历史目标")
    }

    #[allow(dead_code)]
    fn bare_git(&self) -> Git {
        Git::new(&self.repo, &self.shell_env("bare"), Arc::new(CommandLog::default())).unwrap()
    }

    /// 三提交的线性历史：c1 -> c2 -> c3。
    fn linear(&self) -> (String, String, String) {
        let c1 = self.track("a.txt", b"one\n", "first");
        let c2 = self.track("a.txt", b"one\ntwo\n", "second");
        let c3 = self.track("b.txt", b"bee\n", "third");
        (c1, c2, c3)
    }
}

fn start_ref(name: &str) -> HistoryStart {
    HistoryStart::Ref { full_name: name.to_string() }
}

fn file<'a>(files: &'a [HistoryFile], path: &str) -> &'a HistoryFile {
    files
        .iter()
        .find(|item| item.path == path)
        .unwrap_or_else(|| panic!("缺少文件 {path}，实际 {:?}", files.iter().map(|f| &f.path).collect::<Vec<_>>()))
}

fn file_target(commit: &str, parent: Option<&str>, item: &HistoryFile) -> HistoryFileTarget {
    HistoryFileTarget {
        commit: commit.to_string(),
        parent: parent.map(str::to_string),
        path: item.path.clone(),
        original_path: item.original_path.clone(),
        old_oid: item.old_oid.clone(),
        new_oid: item.new_oid.clone(),
        status: item.status,
        diff_id: item.diff_id.clone().expect("可打开的文件应有 diff 身份"),
    }
}

fn row_texts(slice: &revia_lib::history::HistoryRowSlice) -> Vec<String> {
    slice.rows.iter().filter_map(|row| row.text.clone()).collect()
}

// ---------- H01：固定起点与有界分页 ----------

#[test]
fn log_page_keeps_the_start_object_when_the_ref_moves() {
    let f = Fixture::new();
    let (c1, _c2, c3) = f.linear();
    let (target_id, info) = f.open();
    assert_eq!(info.head.as_deref(), Some(c3.as_str()));
    assert!(!info.empty_repository);
    assert!(info.refs.iter().any(|item| item.full_name == "refs/heads/main" && item.current));
    let target = f.target(&target_id);

    let page1 = target
        .log_page(Some(start_ref("refs/heads/main")), false, 2, None)
        .expect("第一页");
    assert_eq!(page1.start, c3);
    assert_eq!(page1.commit_count, 2);
    assert!(!page1.exhausted);
    let cursor = page1.next_cursor.clone().expect("还有下一页");

    // 第一页之后把 main 移回第一个提交：旧游标仍读原来的历史。
    f.git(&["update-ref", "refs/heads/main", &c1]);
    let page2 = target
        .log_page(None, false, 2, Some(cursor))
        .expect("第二页");
    assert_eq!(page2.start, c3);
    assert_eq!(page2.commits.len(), 1);
    assert_eq!(page2.commits[0].oid, c1);
    assert!(page2.exhausted);
    assert!(page2.next_cursor.is_none());

    // 新查询读新引用。
    let fresh = target
        .log_page(Some(start_ref("refs/heads/main")), false, 10, None)
        .expect("新查询");
    assert_eq!(fresh.start, c1);
    assert_eq!(fresh.commits.len(), 1);
}

#[test]
fn start_rejects_rev_syntax_options_missing_objects_and_non_commits() {
    let f = Fixture::new();
    let (c1, _c2, _c3) = f.linear();
    let (target_id, _info) = f.open();
    let target = f.target(&target_id);
    let blob = f.git(&["rev-parse", "HEAD:a.txt"]).trim().to_string();
    let missing = "0123456789012345678901234567890123456789";

    for oid in ["--help", "HEAD~1", "abc123", "", "not-an-oid"] {
        let error = target
            .log_page(Some(HistoryStart::Commit { oid: oid.into() }), false, 10, None)
            .expect_err("必须拒绝");
        assert!(error.message.contains("完整"), "oid={oid} 的错误：{}", error.message);
    }
    let error = target
        .log_page(Some(HistoryStart::Commit { oid: missing.into() }), false, 10, None)
        .expect_err("缺失对象必须报错");
    assert!(error.message.contains("不存在"), "{}", error.message);
    let error = target
        .log_page(Some(HistoryStart::Commit { oid: blob }), false, 10, None)
        .expect_err("blob 不是提交");
    assert!(error.message.contains("不是提交"), "{}", error.message);
    let error = target
        .log_page(Some(start_ref("HEAD~1")), false, 10, None)
        .expect_err("rev 语法不合法");
    assert!(error.message.contains("引用不存在"), "{}", error.message);

    assert_eq!(c1.len(), 40);
}

#[test]
fn limit_is_bounded_and_over_span_is_not_reported_as_history_end() {
    let f = Fixture::new();
    f.linear();
    let (target_id, info) = f.open();
    let target = f.target(&target_id);
    assert_eq!(info.limits.max_page_size, 200);
    assert!(target.log_page(Some(start_ref("refs/heads/main")), false, 0, None).is_err());
    assert!(target.log_page(Some(start_ref("refs/heads/main")), false, 201, None).is_err());
    assert!(target.log_page(None, false, 10, None).is_err(), "首次查询必须给起点");
    // 游标形如 `q1:2`，伪造的越大越界的游标要明确失败，而不是当成历史末尾。
    assert!(target.log_page(None, false, 10, Some("q1:999999999".into())).is_err());
    assert!(target.log_page(None, false, 10, Some("bad-cursor".into())).is_err());
}

#[test]
fn cursor_is_bound_to_target_query_and_close() {
    let f = Fixture::new();
    f.linear();
    let (first_id, _) = f.open();
    let (second_id, _) = f.open();
    let first = f.target(&first_id);
    let second = f.target(&second_id);
    let page = first
        .log_page(Some(start_ref("refs/heads/main")), false, 2, None)
        .unwrap();
    let cursor = page.next_cursor.clone().unwrap();

    // 同一目标的游标不能倒退或重复使用。
    first.log_page(None, false, 2, Some(cursor.clone())).unwrap();
    assert!(first.log_page(None, false, 2, Some(cursor.clone())).is_err());
    // 另一个目标（同一 worktree）不认这个游标。
    assert!(second.log_page(None, false, 2, Some(cursor.clone())).is_err());
    // 遍历方式改变也要拒绝。
    let page2 = first
        .log_page(Some(start_ref("refs/heads/main")), false, 1, None)
        .unwrap();
    let cursor2 = page2.next_cursor.clone().unwrap();
    assert!(first.log_page(None, true, 1, Some(cursor2)).is_err());

    // 关闭幂等，关闭后请求与游标立即失效。
    assert!(f.store.close(&first_id));
    assert!(!f.store.close(&first_id));
    assert!(f.store.target(&first_id).is_err());
    assert!(first
        .log_page(Some(start_ref("refs/heads/main")), false, 2, None)
        .is_err());
    // 新目标不会继承旧目标的身份。
    let (third_id, _) = f.open();
    assert_ne!(third_id, first_id);
    assert!(f.target(&third_id)
        .log_page(Some(start_ref("refs/heads/main")), false, 2, None)
        .is_ok());
}

/// 审查第 4 项：查询编号每个目标都从 `q1` 起，游标必须自带目标身份；
/// 两个目标读到同样的进度时，A 的游标也不能在 B 上续页。
#[test]
fn cursor_is_not_accepted_by_another_target_with_the_same_progress() {
    let f = Fixture::new();
    f.linear();
    let (a_id, _) = f.open();
    let (b_id, _) = f.open();
    let a = f.target(&a_id);
    let b = f.target(&b_id);
    let page_a = a
        .log_page(Some(start_ref("refs/heads/main")), false, 2, None)
        .unwrap();
    let cursor_a = page_a.next_cursor.clone().expect("A 还有下一页");
    // B 也读完同样大小的第一页：查询编号同为 q1、skip 同为 2。
    let page_b = b
        .log_page(Some(start_ref("refs/heads/main")), false, 2, None)
        .unwrap();
    assert_eq!(page_b.query_id, page_a.query_id);
    let cursor_b = page_b.next_cursor.clone().expect("B 还有下一页");
    assert_ne!(cursor_a, cursor_b, "不同目标的游标不能是同一个字符串");
    let error = b
        .log_page(None, false, 2, Some(cursor_a))
        .expect_err("A 的游标不能在 B 上续页");
    assert!(error.message.contains("另一个历史目标"), "{}", error.message);
    // B 自己的游标仍然可用。
    assert!(b.log_page(None, false, 2, Some(cursor_b)).is_ok());
}

/// 审查第 5 项：末尾提交的主题为空时，`-z` 的结尾空字段不能把它的字段一起弹掉。
#[test]
fn page_reads_the_last_commit_when_its_subject_is_empty() {
    let f = Fixture::new();
    f.git(&["commit", "--allow-empty", "--allow-empty-message", "-m", ""]);
    f.track("a.txt", b"one\n", "second");
    f.track("a.txt", b"two\n", "third");
    let (target_id, _) = f.open();
    let target = f.target(&target_id);

    let page1 = target
        .log_page(Some(start_ref("refs/heads/main")), false, 2, None)
        .unwrap();
    let cursor = page1.next_cursor.clone().expect("还有一页");
    let page2 = target.log_page(None, false, 2, Some(cursor.clone())).expect("末尾空主题的页要能读出来");
    assert_eq!(page2.commits.len(), 1);
    assert_eq!(page2.commits[0].subject, "");
    assert!(page2.exhausted);

    // 空主题落在整页的最后一条也一样。
    let whole = target
        .log_page(Some(start_ref("refs/heads/main")), false, 3, None)
        .unwrap();
    assert_eq!(whole.commits.len(), 3);
    assert_eq!(whole.commits[2].subject, "");
}

#[test]
fn query_cap_expires_the_oldest_cursor_explicitly() {
    let f = Fixture::new();
    f.linear();
    let (target_id, info) = f.open();
    let target = f.target(&target_id);
    let mut first_cursor = None;
    for index in 0..=info.limits.max_open_queries {
        let page = target
            .log_page(Some(start_ref("refs/heads/main")), false, 1, None)
            .unwrap();
        if index == 0 {
            first_cursor = page.next_cursor;
        }
    }
    let error = target
        .log_page(None, false, 1, first_cursor)
        .expect_err("最早的查询应被淘汰");
    assert!(error.message.contains("过期"), "{}", error.message);
}

#[test]
fn merge_commit_keeps_all_real_parents_across_pages() {
    let f = Fixture::new();
    let base = f.track("base.txt", b"base\n", "base");
    f.git(&["checkout", "-b", "side"]);
    let side = f.track("side.txt", b"side\n", "side");
    f.git(&["checkout", "main"]);
    let ours = f.track("main.txt", b"main\n", "main");
    f.git(&["merge", "--no-ff", "side", "-m", "merge side"]);
    let merge = f.head();
    assert_ne!(base, merge);
    let (target_id, _) = f.open();
    let target = f.target(&target_id);

    let detail = target.commit_detail(&merge).unwrap();
    assert_eq!(detail.parents, vec![ours.clone(), side.clone()]);

    // 一页一条，父边不因为不在本页而丢失。
    let mut commits = Vec::new();
    let mut cursor = None;
    loop {
        let page = target
            .log_page(Some(start_ref("refs/heads/main")), false, 1, cursor)
            .unwrap();
        commits.extend(page.commits.clone());
        if page.exhausted {
            break;
        }
        cursor = page.next_cursor;
    }
    let merge_entry = commits.iter().find(|commit| commit.oid == merge).unwrap();
    assert_eq!(merge_entry.parents, vec![ours, side]);
    assert_eq!(commits.len(), 4);

    // 只走第一父提交：仍返回全部真实父提交，但遍历不到 side 分支独有的提交。
    let first_only = target
        .log_page(Some(start_ref("refs/heads/main")), true, 20, None)
        .unwrap();
    let merge_entry = first_only.commits.iter().find(|commit| commit.oid == merge).unwrap();
    assert_eq!(merge_entry.parents.len(), 2);
    assert!(!first_only.commits.iter().any(|commit| commit.subject == "side"));
}

#[test]
fn commit_detail_preserves_multiline_body_and_signatures() {
    let f = Fixture::new();
    f.track("a.txt", b"one\n", "first");
    f.write("a.txt", b"two\n");
    f.git(&["--literal-pathspecs", "add", "a.txt"]);
    f.git(&[
        "commit",
        "-m",
        "subject",
        "-m",
        "body line 1\n-- separator --\nbody line 3",
    ]);
    let oid = f.head();
    let (target_id, _) = f.open();
    let target = f.target(&target_id);
    let detail = target.commit_detail(&oid).unwrap();
    assert!(detail.message.starts_with("subject"));
    assert!(detail.message.contains("body line 1\n-- separator --\nbody line 3"));
    assert!(!detail.message_lossy);
    assert_eq!(detail.author.name, "Revia History Test");
    assert_eq!(detail.author.email, "history@example.com");
    assert!(detail.parents.len() == 1);

    // 空提交是合法的，不算读取失败。
    f.git(&["commit", "--allow-empty", "-m", "empty"]);
    let empty = f.head();
    let detail = target.commit_detail(&empty).unwrap();
    assert!(detail.parents.len() == 1);
    let files = target.commit_files(&empty, Some(&oid)).unwrap();
    assert!(files.files.is_empty());
    assert!(!files.truncated);
}

// ---------- H02：详情、父提交与改动文件 ----------

#[test]
fn merge_files_require_an_explicit_real_parent() {
    let f = Fixture::new();
    let base = f.track("base.txt", b"base\n", "base");
    f.git(&["checkout", "-b", "side"]);
    let side = f.track("side.txt", b"side\n", "side");
    f.git(&["checkout", "main"]);
    let ours = f.track("main.txt", b"main\n", "main");
    f.git(&["merge", "--no-ff", "side", "-m", "merge side"]);
    let merge = f.head();
    let (target_id, _) = f.open();
    let target = f.target(&target_id);

    let error = target.commit_files(&merge, None).expect_err("merge 不能默认取第一个父提交");
    assert!(error.message.contains("显式指定"), "{}", error.message);
    let error = target
        .commit_files(&merge, Some(&base))
        .expect_err("无关提交不能当父提交");
    assert!(error.message.contains("父提交"), "{}", error.message);

    let against_ours = target.commit_files(&merge, Some(&ours)).unwrap();
    assert!(!against_ours.empty_tree);
    assert_eq!(against_ours.base, ours);
    assert!(against_ours.files.iter().any(|item| item.path == "side.txt"));
    let against_side = target.commit_files(&merge, Some(&side)).unwrap();
    assert!(against_side.files.iter().any(|item| item.path == "main.txt"));
    assert!(!against_side.files.iter().any(|item| item.path == "side.txt"));
}

#[test]
fn root_commit_compares_against_the_repository_empty_tree() {
    let f = Fixture::new();
    let root = f.track("a.txt", b"one\n", "first");
    let expected = String::from_utf8(f.bytes(&f.repo, &["hash-object", "-t", "tree", "--stdin"])).unwrap();
    let expected = expected.trim().to_string();
    let (target_id, _) = f.open();
    let target = f.target(&target_id);
    let files = target.commit_files(&root, None).unwrap();
    assert!(files.empty_tree);
    assert_eq!(files.base, expected);
    assert_eq!(files.parent, None);
    let item = file(&files.files, "a.txt");
    assert_eq!(item.status, HistoryFileStatus::Added);
    assert!(item.old_oid.is_none());
    assert!(item.new_oid.is_some());
}

#[test]
fn rename_reads_both_sides_and_delete_reports_missing_side() {
    let f = Fixture::new();
    let base = f.track("old.txt", b"one\ntwo\n", "first");
    f.git(&["mv", "old.txt", "new.txt"]);
    let rename = f.commit_all("rename");
    f.track("gone.txt", b"bye\n", "add gone");
    let delete_parent = f.head();
    f.git(&["rm", "-q", "gone.txt"]);
    let delete = f.commit_all("delete");

    let (target_id, _) = f.open();
    let target = f.target(&target_id);

    let files = target.commit_files(&rename, Some(&base)).unwrap();
    let item = file(&files.files, "new.txt");
    assert_eq!(item.status, HistoryFileStatus::Renamed);
    assert_eq!(item.original_path.as_deref(), Some("old.txt"));
    assert!(item.openable);
    let target_file = file_target(&rename, Some(&base), item);
    let layout = target.file_side_layout(&target_file, IgnoreWhitespace::Off, 3).unwrap();
    assert_eq!(layout.left_line_count, 2);
    assert_eq!(layout.right_line_count, 2);
    assert!(!layout.left_missing);
    assert!(!layout.right_missing);
    let left = target
        .file_side_lines(&target_file, IgnoreWhitespace::Off, 3, FileVersion::Old, 1, 2, WordGranularity::Word)
        .unwrap();
    let right = target
        .file_side_lines(&target_file, IgnoreWhitespace::Off, 3, FileVersion::New, 1, 2, WordGranularity::Word)
        .unwrap();
    assert_eq!(left.iter().map(|line| line.text.as_str()).collect::<Vec<_>>(), vec!["one", "two"]);
    assert_eq!(right.iter().map(|line| line.text.as_str()).collect::<Vec<_>>(), vec!["one", "two"]);

    let files = target.commit_files(&delete, Some(&delete_parent)).unwrap();
    let item = file(&files.files, "gone.txt");
    assert_eq!(item.status, HistoryFileStatus::Deleted);
    assert!(item.old_oid.is_some());
    assert!(item.new_oid.is_none());
    let target_file = file_target(&delete, Some(&delete_parent), item);
    let layout = target.file_side_layout(&target_file, IgnoreWhitespace::Off, 3).unwrap();
    assert_eq!(layout.left_line_count, 1);
    assert_eq!(layout.right_line_count, 0);
    assert!(!layout.left_missing);
    assert!(layout.right_missing, "删除的新侧是没有对象，不是空文件");
}

#[test]
fn empty_file_differs_from_a_missing_side() {
    let f = Fixture::new();
    f.track("a.txt", b"one\n", "first");
    f.write("empty.txt", b"");
    let added = f.commit_all("add empty");
    f.write("a.txt", b"");
    let emptied = f.commit_all("empty a");

    let (target_id, _) = f.open();
    let target = f.target(&target_id);

    let files = target
        .commit_files(&added, Some(&f.git(&["rev-parse", &format!("{added}^")]).trim().to_string()))
        .unwrap();
    let item = file(&files.files, "empty.txt");
    assert!(item.old_oid.is_none(), "新增文件的旧侧不存在");
    assert!(item.new_oid.is_some(), "空文件仍是一个真实对象");
    let parent = f.git(&["rev-parse", &format!("{added}^")]).trim().to_string();
    let layout = target
        .file_side_layout(&file_target(&added, Some(&parent), item), IgnoreWhitespace::Off, 3)
        .unwrap();
    assert!(layout.left_missing);
    assert_eq!(layout.left_line_count, 0);
    assert_eq!(layout.right_line_count, 0);
    assert_eq!(layout.empty_reason, Some(revia_lib::history::HistoryEmptyReason::EmptyFile));

    let parent = f.git(&["rev-parse", &format!("{emptied}^")]).trim().to_string();
    let files = target.commit_files(&emptied, Some(&parent)).unwrap();
    let item = file(&files.files, "a.txt");
    assert!(item.old_oid.is_some());
    assert!(item.new_oid.is_some());
    let layout = target
        .file_side_layout(&file_target(&emptied, Some(&parent), item), IgnoreWhitespace::Off, 3)
        .unwrap();
    assert!(!layout.left_missing);
    assert!(!layout.right_missing);
    assert_eq!(layout.left_line_count, 1);
    assert_eq!(layout.right_line_count, 0);
}

#[test]
fn binary_and_mode_only_changes_are_explicit() {
    let f = Fixture::new();
    f.track("a.txt", b"one\n", "first");
    f.write("bin.dat", b"head\0tail\n");
    let binary = f.commit_all("binary");
    f.write("script.sh", b"#!/bin/sh\necho hi\n");
    f.commit_all("script");
    // Git tree modes can be verified without host filesystem chmod support.
    f.git(&["update-index", "--chmod=+x", "--", "script.sh"]);
    f.git(&["commit", "-m", "mode only"]);
    let mode_only = f.head();

    let (target_id, _) = f.open();
    let target = f.target(&target_id);
    let parent = f.git(&["rev-parse", &format!("{binary}^")]).trim().to_string();
    let files = target.commit_files(&binary, Some(&parent)).unwrap();
    let item = file(&files.files, "bin.dat");
    assert!(item.binary);
    assert!(item.openable);
    let slice = target
        .file_rows(&file_target(&binary, Some(&parent), item), ViewMode::Unified, IgnoreWhitespace::Off, 0, 100)
        .unwrap();
    assert!(slice.binary);
    assert_eq!(slice.row_count, 0);
    assert_eq!(slice.empty_reason, Some(revia_lib::history::HistoryEmptyReason::Binary));
    // 审查第 8 项：二进制也要能给出左右对照布局（而不是“没有可显示的文本内容”报错）。
    let layout = target
        .file_side_layout(&file_target(&binary, Some(&parent), item), IgnoreWhitespace::Off, 3)
        .unwrap();
    assert!(layout.binary);
    assert_eq!(layout.left_line_count, 0);
    assert_eq!(layout.right_line_count, 0);
    assert!(layout.segments.is_empty());
    assert!(layout.folds.is_empty());
    assert_eq!(layout.empty_reason, Some(revia_lib::history::HistoryEmptyReason::Binary));

    {
        let parent = f.git(&["rev-parse", &format!("{mode_only}^")]).trim().to_string();
        let files = target.commit_files(&mode_only, Some(&parent)).unwrap();
        let item = file(&files.files, "script.sh");
        assert!(item.mode_only);
        assert_eq!(item.old_mode.as_deref(), Some("100644"));
        assert_eq!(item.new_mode.as_deref(), Some("100755"));
        let slice = target
            .file_rows(&file_target(&mode_only, Some(&parent), item), ViewMode::Unified, IgnoreWhitespace::Off, 0, 100)
            .unwrap();
        assert_eq!(slice.row_count, 0);
        assert_eq!(slice.empty_reason, Some(revia_lib::history::HistoryEmptyReason::ModeOnly));
        assert_eq!(slice.mode_only, true);
    }
}

#[test]
fn special_paths_are_matched_literally_and_read_correctly() {
    let f = Fixture::new();
    let base = f.track("base.txt", b"base\n", "base");
    // Windows disallows colon/star, control characters and trailing spaces.
    let names = if cfg!(windows) {
        ["--help", "(glob)literal", "a[1].txt", " lead", "with space.txt",
            "tab name.txt", "replace\u{fffd}name.txt", "中文 文件.txt"]
    } else {
        [
            "--help",
            ":(glob)*",
            "a[1].txt",
            " lead and trail ",
            "with space.txt",
            "tab\tname.txt",
            "replace\u{fffd}name.txt",
            "中文 文件.txt",
        ]
    };
    for name in names {
        f.write(name, format!("content of {name}\n").as_bytes());
    }
    let commit = f.commit_all("special paths");

    let (target_id, _) = f.open();
    let target = f.target(&target_id);
    let files = target.commit_files(&commit, Some(&base)).unwrap();
    assert_eq!(files.files.len(), names.len(), "每个路径都应按字面出现一次");
    for name in names {
        let item = file(&files.files, name);
        assert!(!item.path_lossy, "合法 UTF-8 路径不能被当成有损：{name:?}");
        let slice = target
            .file_rows(&file_target(&commit, Some(&base), item), ViewMode::Unified, IgnoreWhitespace::Off, 0, 200)
            .unwrap();
        let texts = row_texts(&slice);
        assert!(
            texts.iter().any(|text| text == &format!("content of {name}")),
            "路径 {name:?} 的行内容不对：{texts:?}"
        );
    }
}

#[test]
fn symlink_content_is_the_link_target() {
    let f = Fixture::new();
    let base = f.track("a.txt", b"one\n", "first");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("a.txt", f.repo.join("link.txt")).unwrap();
        let commit = f.commit_all("symlink");
        let (target_id, _) = f.open();
        let target = f.target(&target_id);
        let files = target.commit_files(&commit, Some(&base)).unwrap();
        let item = file(&files.files, "link.txt");
        assert!(item.symlink);
        assert!(item.openable);
        let slice = target
            .file_rows(&file_target(&commit, Some(&base), item), ViewMode::Unified, IgnoreWhitespace::Off, 0, 50)
            .unwrap();
        assert!(row_texts(&slice).iter().any(|text| text == "a.txt"));
    }
}

#[cfg(unix)]
#[test]
fn no_newline_at_end_of_file_is_preserved() {
    let f = Fixture::new();
    let base = f.track("a.txt", b"one\n", "first");
    f.write("a.txt", b"one\ntwo");
    let commit = f.commit_all("no trailing newline");
    let (target_id, _) = f.open();
    let target = f.target(&target_id);
    let files = target.commit_files(&commit, Some(&base)).unwrap();
    let item = file(&files.files, "a.txt");
    let slice = target
        .file_rows(&file_target(&commit, Some(&base), item), ViewMode::Unified, IgnoreWhitespace::Off, 0, 100)
        .unwrap();
    assert!(
        slice
            .rows
            .iter()
            .any(|row| row.line_kind == Some(revia_lib::git::diff::LineKind::NoEol)),
        "缺少“文件末尾没有换行”的标记行"
    );
}

#[test]
fn submodule_is_listed_but_not_openable() {
    let f = Fixture::new();
    let base = f.track("a.txt", b"one\n", "first");
    // gitlink（mode 160000）就是子模块在树里的真实形态；不需要网络或嵌套工作区。
    f.git(&["update-index", "--add", "--cacheinfo", &format!("160000,{base},sub")]);
    f.git(&["commit", "-m", "add submodule"]);
    let commit = f.head();
    let (target_id, _) = f.open();
    let target = f.target(&target_id);
    let files = target.commit_files(&commit, Some(&base)).unwrap();
    let item = file(&files.files, "sub");
    assert!(item.submodule);
    assert!(!item.openable);
    assert_eq!(
        item.readonly_reason,
        Some(revia_lib::history::HistoryReadonlyReason::Submodule)
    );
    assert!(item.diff_id.is_none());
}

#[test]
fn releasing_a_worktree_and_clearing_the_store_invalidate_targets() {
    let f = Fixture::new();
    f.linear();
    let (first, _) = f.open();
    let (second, _) = f.open();
    assert!(f.store.target(&first).is_ok());
    f.store.close_worktree("test-worktree");
    assert!(f.store.target(&first).is_err());
    assert!(f.store.target(&second).is_err());
    assert_eq!(f.store.open_count(), 0);
    let (third, _) = f.open();
    f.store.clear();
    assert!(f.store.target(&third).is_err());
}

/// 审查第 3 项：会话已释放 / 环境已重载后，旧会话实例不能再登记新的历史目标。
///
/// 走命令层同一条路径（`AppState` 的会话表与历史目标表），不需要卡时序：
/// 先释放，再用手里那个旧会话调用 `open`。
#[test]
fn released_and_reloaded_sessions_cannot_claim_new_history_targets() {
    let f = Fixture::new();
    f.linear();
    let state = revia_lib::commands::AppState::new(f.data.clone());
    let entry = {
        let mut registry = state.registry.lock().unwrap();
        registry.add(f.repo.to_str().unwrap()).unwrap()
    };
    let worktree_id = entry
        .worktrees
        .first()
        .map(|worktree| worktree.id.clone())
        .unwrap_or_else(|| entry.id.clone());

    // 释放：先打开一个目标，再释放会话。
    let session = state.worktree_session(&worktree_id).unwrap();
    let opened = state.history.open(&session).unwrap();
    assert!(state.history.target(&opened.target_id).is_ok());
    state.release_session(&worktree_id);
    assert_eq!(state.history.open_count(), 0);
    let error = state
        .history
        .open(&session)
        .expect_err("已释放的会话不能打开历史");
    assert!(error.message.contains("会话已失效"), "{}", error.message);
    assert_eq!(state.history.open_count(), 0, "拒绝时不能登记新目标");

    // 环境重载：同样先打开一个目标，再重载。
    let session = state.worktree_session(&worktree_id).unwrap();
    let opened = state.history.open(&session).unwrap();
    assert!(state.history.target(&opened.target_id).is_ok());
    state.reload_sessions().unwrap();
    assert_eq!(state.history.open_count(), 0);
    let error = state
        .history
        .open(&session)
        .expect_err("环境重载后旧会话不能打开历史");
    assert!(error.message.contains("会话已失效"), "{}", error.message);
    assert_eq!(state.history.open_count(), 0, "拒绝时不能登记新目标");

    // 重载后取到的新会话（新的实例）仍能正常打开。
    let fresh = state.worktree_session(&worktree_id).unwrap();
    let reopened = state.history.open(&fresh).expect("新会话应能打开历史");
    assert!(state.history.target(&reopened.target_id).is_ok());
}

// ---------- 固定目标：工作区变化不影响已打开的提交 diff ----------

#[test]
fn opened_commit_diff_is_immutable_while_the_worktree_changes() {
    let f = Fixture::new();
    let base = f.track("a.txt", b"one\n", "first");
    let commit = f.track("a.txt", b"one\ntwo\n", "second");
    let (target_id, _) = f.open();
    let target = f.target(&target_id);
    let files = target.commit_files(&commit, Some(&base)).unwrap();
    let item = file(&files.files, "a.txt");
    let target_file = file_target(&commit, Some(&base), item);

    let before = target
        .file_rows(&target_file, ViewMode::Split, IgnoreWhitespace::Off, 0, 500)
        .unwrap();
    let before_layout = target
        .file_side_layout(&target_file, IgnoreWhitespace::Off, 3)
        .unwrap();
    let before_highlight = target
        .file_highlight(&target_file, IgnoreWhitespace::Off, FileVersion::New, 1, 2)
        .unwrap();

    // 改工作区、暂存，再切一个新分支（HEAD 与暂存区都变了）。
    f.write("a.txt", b"totally different\n");
    f.git(&["--literal-pathspecs", "add", "a.txt"]);
    f.git(&["checkout", "-q", "-b", "other"]);

    let after = target
        .file_rows(&target_file, ViewMode::Split, IgnoreWhitespace::Off, 0, 500)
        .unwrap();
    assert_eq!(before.rows.len(), after.rows.len());
    assert_eq!(row_texts(&before), row_texts(&after));
    let after_layout = target
        .file_side_layout(&target_file, IgnoreWhitespace::Off, 3)
        .unwrap();
    assert_eq!(before_layout.left_line_count, after_layout.left_line_count);
    assert_eq!(before_layout.right_line_count, after_layout.right_line_count);
    assert_eq!(before_layout.segments.len(), after_layout.segments.len());
    let after_highlight = target
        .file_highlight(&target_file, IgnoreWhitespace::Off, FileVersion::New, 1, 2)
        .unwrap();
    assert_eq!(before_highlight.lines.len(), after_highlight.lines.len());

    // 目标身份被篡改（换父提交）一律拒绝，而不是静默换内容。
    let mut forged = target_file.clone();
    forged.parent = Some(commit.clone());
    assert!(target
        .file_rows(&forged, ViewMode::Unified, IgnoreWhitespace::Off, 0, 10)
        .is_err());
    let mut forged = target_file;
    forged.status = HistoryFileStatus::Added;
    assert!(target
        .file_rows(&forged, ViewMode::Unified, IgnoreWhitespace::Off, 0, 10)
        .is_err());
}

/// 审查第 2 项：工作区的 `.gitattributes` 不能改变已打开的提交 diff。
///
/// 属性取自该提交自己的树（`GIT_ATTR_SOURCE`）：把工作区的 `*.txt` 标成 `-diff`
/// 并让缓存淘汰后重读，仍是同样的文本行，`commit_files` 的 `binary` 也不变。
#[test]
fn worktree_attributes_cannot_change_an_opened_commit_diff() {
    let f = Fixture::new();
    let base = f.track("a.txt", b"one\n", "first");
    let commit = f.track("a.txt", b"one\ntwo\n", "second");

    // 先准备好用于把缓存挤出去的其它提交（都在写 `.gitattributes` 之前）。
    let mut extras = Vec::new();
    for index in 0..revia_lib::history::MAX_CACHED_DIFFS + 1 {
        let path = format!("other{index}.txt");
        f.write(&path, format!("other {index}\n").as_bytes());
        let parent = f.head();
        let extra = f.commit_all(&format!("add other {index}"));
        extras.push((path, parent, extra));
    }

    let (target_id, _) = f.open();
    let target = f.target(&target_id);
    let files = target.commit_files(&commit, Some(&base)).unwrap();
    let item = file(&files.files, "a.txt");
    assert!(!item.binary);
    let target_file = file_target(&commit, Some(&base), item);
    let before = target
        .file_rows(&target_file, ViewMode::Unified, IgnoreWhitespace::Off, 0, 100)
        .unwrap();
    assert!(!before.binary);
    assert!(
        row_texts(&before).iter().any(|text| text == "two"),
        "第一次读应是文本 diff：{:?}",
        row_texts(&before)
    );

    // 工作区把 `*.txt` 标成 `-diff`（不提交），再读满缓存逼它淘汰刚才那份 diff。
    f.write(".gitattributes", b"*.txt -diff\n");
    for (path, parent, extra) in &extras {
        let files = target.commit_files(extra, Some(parent)).unwrap();
        let item = file(&files.files, path);
        target
            .file_rows(&file_target(extra, Some(parent), item), ViewMode::Unified, IgnoreWhitespace::Off, 0, 20)
            .unwrap();
    }

    // 重读：缓存已被清空，但内容仍来自提交对象，不跟着工作区属性变成二进制。
    let after = target
        .file_rows(&target_file, ViewMode::Unified, IgnoreWhitespace::Off, 0, 100)
        .unwrap();
    assert!(!after.binary, "工作区属性不得让已打开的提交 diff 变成二进制");
    assert_eq!(row_texts(&before), row_texts(&after));
    let files = target.commit_files(&commit, Some(&base)).unwrap();
    assert!(!file(&files.files, "a.txt").binary);
}

#[test]
fn caches_are_isolated_and_bounded() {
    let f = Fixture::new();
    let base = f.track("shared.txt", b"one\n", "first");
    let commit = f.track("shared.txt", b"one\ntwo\n", "second");
    let (target_a, _) = f.open();
    let (target_b, _) = f.open();
    assert_ne!(target_a, target_b);

    let git_a = f.target(&target_a);
    let git_b = f.target(&target_b);
    let files = git_a.commit_files(&commit, Some(&base)).unwrap();
    let item = file(&files.files, "shared.txt");
    let target_file = file_target(&commit, Some(&base), item);
    let slice_a = git_a
        .file_rows(&target_file, ViewMode::Unified, IgnoreWhitespace::Off, 0, 100)
        .unwrap();
    // 同一个 file target 在另一个目标上重算出来的 diff 身份不同，必须拒绝。
    assert!(git_b
        .file_rows(&target_file, ViewMode::Unified, IgnoreWhitespace::Off, 0, 100)
        .is_err());

    // 同路径不同仓库：内容不同，不串缓存。
    fs::write(f.repo2.join("shared.txt"), b"repo2 content\n").unwrap();
    f.git_in(&f.repo2, &["--literal-pathspecs", "add", "-A"]);
    f.git_in(&f.repo2, &["commit", "-m", "repo2 first"]);
    let session2 = WorktreeSession::open(
        "second-worktree",
        &f.repo2,
        f.shell_env("second-worktree"),
        Arc::new(CommandLog::default()),
    )
    .unwrap();
    let info2 = f.store.open(&session2).unwrap();
    let other = f.target(&info2.target_id);
    let other_files = other.commit_files(&info2.head.clone().unwrap(), None).unwrap();
    let other_item = file(&other_files.files, "shared.txt");
    let other_slice = other
        .file_rows(&file_target(&info2.head.clone().unwrap(), None, other_item), ViewMode::Unified, IgnoreWhitespace::Off, 0, 100)
        .unwrap();
    assert_ne!(row_texts(&slice_a), row_texts(&other_slice));

    // 忽略空白档位不共享：纯空白改动在 Off 有 hunk，在 All 下没有。
    f.write("shared.txt", b"one\n  two\n");
    let base2 = f.commit_all("indent");
    f.write("shared.txt", b"one\ntwo\n");
    let commit2 = f.commit_all("dedent");
    let files = git_a.commit_files(&commit2, Some(&base2)).unwrap();
    let item = file(&files.files, "shared.txt");
    let target_file = file_target(&commit2, Some(&base2), item);
    let off = git_a
        .file_rows(&target_file, ViewMode::Unified, IgnoreWhitespace::Off, 0, 100)
        .unwrap();
    let all = git_a
        .file_rows(&target_file, ViewMode::Unified, IgnoreWhitespace::All, 0, 100)
        .unwrap();
    let off_again = git_a
        .file_rows(&target_file, ViewMode::Unified, IgnoreWhitespace::Off, 0, 100)
        .unwrap();
    assert!(off.row_count > 0);
    assert_eq!(all.row_count, 0);
    assert_eq!(off.row_count, off_again.row_count);

    // 缓存有界：连续读很多份不同 diff 后仍然不超过限额。
    for index in 0..12 {
        let path = format!("f{index}.txt");
        f.write(&path, format!("{index}\n").as_bytes());
        let parent = f.head();
        let commit = f.commit_all(&format!("add {index}"));
        let files = git_a.commit_files(&commit, Some(&parent)).unwrap();
        let item = file(&files.files, &path);
        git_a
            .file_rows(&file_target(&commit, Some(&parent), item), ViewMode::Unified, IgnoreWhitespace::Off, 0, 10)
            .unwrap();
    }
    assert!(git_a.cache_len() <= revia_lib::history::MAX_CACHED_DIFFS);
    assert!(git_a.layout_cache_len() <= revia_lib::history::MAX_CACHED_LAYOUTS);
    assert!(f.store.open_count() <= revia_lib::history::MAX_TARGETS);
}

#[test]
fn empty_repository_is_reported_separately_from_failures() {
    let f = Fixture::new();
    let session = WorktreeSession::open(
        "empty-worktree",
        &f.repo2,
        f.shell_env("empty-worktree"),
        Arc::new(CommandLog::default()),
    )
    .unwrap();
    let info = f.store.open(&session).unwrap();
    assert!(info.empty_repository);
    assert!(info.head.is_none());
    assert!(info.refs.is_empty());
    let target = f.target(&info.target_id);
    let error = target.log_page(Some(HistoryStart::Head), false, 10, None).expect_err("空仓库不能读历史");
    assert!(error.message.contains("还没有提交"), "{}", error.message);
}

#[cfg(unix)]
#[test]
fn non_utf8_paths_stay_distinct_and_are_not_openable() {
    use std::ffi::OsStr;
    use std::io::Write;
    use std::os::unix::ffi::OsStrExt;

    let f = Fixture::new();
    // macOS 的 APFS 不允许创建非 UTF-8 文件名，因此直接写 index 与提交对象：
    // 读取仍然走真实 Git 的机器格式。
    let write_blob = |content: &[u8]| -> String {
        let mut child = f
            .command_in(&f.repo)
            .args(["hash-object", "-w", "--stdin"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.as_mut().unwrap().write_all(content).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    };
    let one = write_blob(b"one\n");
    let two = write_blob(b"two\n");
    for (blob, path) in [
        (&one, OsStr::from_bytes(b"bad\xffname.txt")),
        (&two, OsStr::from_bytes(b"bad\xfename.txt")),
    ] {
        // `--cacheinfo <mode>,<object>,<path>` 必须是一个参数，路径才能带上原始字节。
        let mut spec = std::ffi::OsString::from(format!("100644,{blob},"));
        spec.push(path);
        let output = f
            .command_in(&f.repo)
            .arg("update-index")
            .arg("--add")
            .arg("--cacheinfo")
            .arg(&spec)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "update-index 失败：{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let tree = f.git(&["write-tree"]).trim().to_string();
    let commit = f
        .git(&["commit-tree", &tree, "-m", "non-utf8 paths"])
        .trim()
        .to_string();

    let (target_id, _) = f.open();
    let target = f.target(&target_id);
    let files = target.commit_files(&commit, None).unwrap();
    assert_eq!(files.files.len(), 2, "两个不同的字节路径不能合并成一条");
    for item in &files.files {
        assert!(item.path_lossy);
        assert!(!item.openable);
        assert_eq!(
            item.readonly_reason,
            Some(revia_lib::history::HistoryReadonlyReason::NonUtf8Path)
        );
        assert!(item.diff_id.is_none());
        assert!(item.new_oid.is_some());
    }
    assert_ne!(files.files[0].new_oid, files.files[1].new_oid);

    // 用展示名伪造的请求必须被拒绝，而不是读到其中一条。
    let forged = HistoryFileTarget {
        commit: commit.clone(),
        parent: None,
        path: files.files[0].path.clone(),
        original_path: None,
        old_oid: None,
        new_oid: files.files[0].new_oid.clone(),
        status: HistoryFileStatus::Added,
        diff_id: "forged".into(),
    };
    assert!(target
        .file_rows(&forged, ViewMode::Unified, IgnoreWhitespace::Off, 0, 10)
        .is_err());
}

/// 审查第 6 项：重命名前的路径不是合法 UTF-8 时，不能标成可打开。
///
/// 否则会用有损的展示名签发 `diffId`，之后打开必然被 `verify_change` 拒绝。
#[cfg(unix)]
#[test]
fn rename_from_a_non_utf8_path_is_not_openable() {
    use std::io::Write;
    use std::process::Stdio;

    let f = Fixture::new();
    // macOS 的 APFS 不允许创建非 UTF-8 文件名，因此直接用对象与树构造提交；
    // 读取仍然走真实 Git 的机器格式。
    let write_blob = |content: &[u8]| -> String {
        let mut child = f
            .command_in(&f.repo)
            .args(["hash-object", "-w", "--stdin"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.as_mut().unwrap().write_all(content).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    };
    // `git mktree -z` 的条目是 `<mode> SP <type> SP <oid> TAB <name> NUL`，路径按原始字节。
    let mktree = |entry: &[u8]| -> String {
        let mut child = f
            .command_in(&f.repo)
            .args(["mktree", "-z"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.as_mut().unwrap().write_all(entry).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "mktree 失败：{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    };
    let entry = |blob: &str, path: &[u8]| -> Vec<u8> {
        let mut entry = format!("100644 blob {blob}\t").into_bytes();
        entry.extend_from_slice(path);
        entry.push(0);
        entry
    };

    let blob = write_blob(b"one\n");
    let base_tree = mktree(&entry(&blob, b"bad\xffname.txt"));
    let base = f
        .git(&["commit-tree", &base_tree, "-m", "非 UTF-8 路径"])
        .trim()
        .to_string();
    let tree = mktree(&entry(&blob, b"renamed.txt"));
    let commit = f
        .git(&["commit-tree", &tree, "-p", &base, "-m", "rename"])
        .trim()
        .to_string();

    let (target_id, _) = f.open();
    let target = f.target(&target_id);
    let files = target.commit_files(&commit, Some(&base)).unwrap();
    assert_eq!(files.files.len(), 1, "应识别为一次重命名：{:?}", files.files);
    let item = &files.files[0];
    assert_eq!(item.status, HistoryFileStatus::Renamed);
    assert_eq!(item.path, "renamed.txt");
    assert!(item.original_path.is_some());
    assert!(item.path_lossy, "原路径不是合法 UTF-8");
    assert!(!item.openable, "原路径有损时不能标成可打开");
    assert_eq!(
        item.readonly_reason,
        Some(revia_lib::history::HistoryReadonlyReason::NonUtf8Path)
    );
    assert!(item.diff_id.is_none());
    assert!(item.old_oid.is_some() && item.new_oid.is_some());

    // 用有损展示名伪造的请求必须被拒绝，而不是读到改名后的文件。
    let forged = HistoryFileTarget {
        commit: commit.clone(),
        parent: Some(base.clone()),
        path: item.path.clone(),
        original_path: item.original_path.clone(),
        old_oid: item.old_oid.clone(),
        new_oid: item.new_oid.clone(),
        status: item.status,
        diff_id: "forged".into(),
    };
    assert!(target
        .file_rows(&forged, ViewMode::Unified, IgnoreWhitespace::Off, 0, 10)
        .is_err());
}

// ---------- 只读副作用 ----------

fn snapshot_repo(f: &Fixture) -> Vec<String> {
    let mut state = Vec::new();
    state.push(format!("HEAD={}", f.git(&["rev-parse", "HEAD"]).trim()));
    state.push(f.git(&["for-each-ref", "--format=%(refname) %(objectname)"]));
    state.push(f.git(&["ls-files", "-s"]));
    state.push(f.git(&["status", "--porcelain=v2", "--untracked-files=all"]));
    state.push(f.git(&["config", "--list", "--show-origin"]));
    state.push(String::from_utf8_lossy(&fs::read(f.repo.join(".git/index")).unwrap()).into_owned());
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    fn walk(base: &Path, current: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        let mut entries: Vec<PathBuf> = fs::read_dir(current)
            .map(|dir| dir.filter_map(|entry| entry.ok().map(|entry| entry.path())).collect())
            .unwrap_or_default();
        entries.sort();
        for entry in entries {
            if entry.file_name().is_some_and(|name| name == ".git") {
                continue;
            }
            if entry.is_dir() {
                walk(base, &entry, out);
            } else {
                let relative = entry.strip_prefix(base).unwrap().to_string_lossy().into_owned();
                out.push((relative, fs::read(&entry).unwrap_or_default()));
            }
        }
    }
    walk(&f.repo, &f.repo, &mut files);
    for (path, bytes) in files {
        state.push(format!("worktree {path} {} bytes {}", bytes.len(), bytes.iter().fold(0u64, |acc, byte| acc.wrapping_mul(31).wrapping_add(*byte as u64))));
    }
    state
}

#[test]
fn history_reads_leave_git_state_untouched() {
    let f = Fixture::new();
    let base = f.track("a.txt", b"one\n", "first");
    let commit = f.track("a.txt", b"one\ntwo\n", "second");
    f.write("untracked.txt", b"not staged\n");
    let before = snapshot_repo(&f);

    let (target_id, info) = f.open();
    let target = f.target(&target_id);
    assert!(info.head.is_some());
    let page = target
        .log_page(Some(start_ref("refs/heads/main")), false, 50, None)
        .unwrap();
    assert!(!page.commits.is_empty());
    target.commit_detail(&commit).unwrap();
    let files = target.commit_files(&commit, Some(&base)).unwrap();
    for item in &files.files {
        if !item.openable {
            continue;
        }
        let target_file = file_target(&commit, Some(&base), item);
        target
            .file_rows(&target_file, ViewMode::Unified, IgnoreWhitespace::Off, 0, 500)
            .unwrap();
        target
            .file_rows(&target_file, ViewMode::Split, IgnoreWhitespace::All, 0, 500)
            .unwrap();
        target
            .file_side_layout(&target_file, IgnoreWhitespace::Off, 3)
            .unwrap();
        target
            .file_side_lines(&target_file, IgnoreWhitespace::Off, 3, FileVersion::Old, 1, 10, WordGranularity::Word)
            .unwrap();
        target
            .file_highlight(&target_file, IgnoreWhitespace::Off, FileVersion::New, 1, 10)
            .unwrap();
    }

    let after = snapshot_repo(&f);
    assert_eq!(before, after, "只读历史读取改变了仓库状态");
}

#[cfg(unix)]
#[test]
fn external_diff_and_textconv_are_never_run() {
    let f = Fixture::new();
    f.track("a.txt", b"one\n", "first");
    let marker = f.root.join("external-diff-ran");
    let script = f.root.join("external-diff.sh");
    fs::write(
        &script,
        format!("#!/bin/sh\ntouch '{}'\necho external\n", marker.display()),
    )
    .unwrap();
    let mut permissions = fs::metadata(&script).unwrap().permissions();
    use std::os::unix::fs::PermissionsExt;
    permissions.set_mode(0o755);
    fs::set_permissions(&script, permissions).unwrap();
    f.git(&["config", "diff.external", script.to_str().unwrap()]);
    f.git(&["config", "diff.evil.textconv", script.to_str().unwrap()]);
    fs::write(f.repo.join(".gitattributes"), b"*.txt textconv=evil\n").unwrap();
    let base = f.commit_all("attributes");
    let commit = f.track("a.txt", b"one\ntwo\n", "second");

    let (target_id, _) = f.open();
    let target = f.target(&target_id);
    let files = target.commit_files(&commit, Some(&base)).unwrap();
    let item = file(&files.files, "a.txt");
    let target_file = file_target(&commit, Some(&base), item);
    target
        .file_rows(&target_file, ViewMode::Unified, IgnoreWhitespace::Off, 0, 100)
        .unwrap();
    target
        .file_side_layout(&target_file, IgnoreWhitespace::Off, 3)
        .unwrap();
    assert!(!marker.exists(), "外部 diff / textconv 不应在只读历史里执行");
}

/// 审查第 1 项：部分克隆里本地缺失的对象不自动向远端补取。
///
/// 用本地裸仓库（`uploadpack.allowFilter=true`）做 `file://` 的 `blob:none` 克隆，
/// 让一个旧提交的旧侧内容确实不在本地，再读它的 diff：必须明确报错，
/// 并且读取前后缺失对象仍然缺失。
#[test]
fn partial_clone_missing_objects_error_instead_of_lazy_fetching() {
    let f = Fixture::new();
    let first = f.track("old.txt", b"one\n", "first");
    let second = f.track("old.txt", b"two\n", "second");
    let third = f.track("fresh.txt", b"fresh\n", "third");
    let old_blob = f.git(&["rev-parse", &format!("{first}:old.txt")]).trim().to_string();
    f.git(&["config", "uploadpack.allowFilter", "true"]);
    let clone = f.root.join("partial-clone");
    let url = url::Url::from_directory_path(revia_lib::platform::external_path(&f.repo)).unwrap().to_string();
    let output = f.git_output_in(
        &f.root,
        &["clone", "--filter=blob:none", &url, clone.to_str().unwrap()],
    );
    assert!(
        output.status.success(),
        "部分克隆失败：{}",
        String::from_utf8_lossy(&output.stderr)
    );

    // 检查本身也必须禁止补取，否则“缺失对象”的观察会把自己的检查结果改掉。
    let missing = |repo: &Path| -> String {
        let output = f
            .command_in(repo)
            .env("GIT_NO_LAZY_FETCH", "1")
            .args(["rev-list", "--objects", "--all", "--missing=print"])
            .output()
            .expect("列举缺失对象");
        assert!(
            output.status.success(),
            "列举缺失对象失败：{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    let before = missing(&clone);
    assert!(
        before.contains(&format!("?{old_blob}")),
        "旧提交的旧侧内容应确实缺失：{before}"
    );

    let session = WorktreeSession::open(
        "partial-worktree",
        &clone,
        f.shell_env("partial-worktree"),
        Arc::new(CommandLog::default()),
    )
    .unwrap();
    let info = f.store.open(&session).unwrap();
    let target = f.target(&info.target_id);

    // 本地对象齐全的提交仍能正常读：开关没有把整条读取路径弄坏。
    let files = target.commit_files(&third, Some(&second)).unwrap();
    let item = file(&files.files, "fresh.txt");
    let slice = target
        .file_rows(&file_target(&third, Some(&second), item), ViewMode::Unified, IgnoreWhitespace::Off, 0, 50)
        .unwrap();
    assert!(row_texts(&slice).iter().any(|text| text == "fresh"));

    // 旧提交的旧侧内容缺失：明确报错，不能自动向远端补取。
    let error = target
        .commit_files(&second, Some(&first))
        .expect_err("缺失对象必须报错");
    assert!(error.message.contains(&old_blob), "错误里应指名缺失对象：{}", error.message);
    let after = missing(&clone);
    assert_eq!(before, after, "只读历史读取不得补取缺失对象");
}

#[test]
fn missing_objects_error_instead_of_returning_empty_history() {
    let f = Fixture::new();
    f.linear();
    let (target_id, _) = f.open();
    let target = f.target(&target_id);
    let missing = "1111111111111111111111111111111111111111";
    let error = target.commit_detail(missing).expect_err("缺失提交必须报错");
    assert!(error.message.contains("不存在"), "{}", error.message);
    let error = target.commit_files(missing, None).expect_err("缺失提交必须报错");
    assert!(error.message.contains("不存在"), "{}", error.message);
}

// ========== M5 第二批：日志筛选、merge 改动与三栏只读内容（L04、L05、V） ==========

use revia_lib::history::{
    HistoryDateFilter, HistoryFilter, HistoryMergeMode, HistoryMergeText,
};

impl Fixture {
    /// 带额外环境变量执行 Git（作者 / 提交时间筛选的测试用）。
    fn git_with_env(&self, extra: &[(&str, &str)], args: &[&str]) -> String {
        let output = self
            .command_in(&self.repo)
            .envs(extra.iter().copied())
            .args(args.iter().map(|argument| revia_lib::platform::external_path(Path::new(argument))))
            .output()
            .expect("执行 Git");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// 用指定的作者与提交时间做一个提交。
    fn commit_as(&self, author: &str, date: &str, message: &str) -> String {
        self.git(&["--literal-pathspecs", "add", "-A"]);
        self.git_with_env(
            &[("GIT_AUTHOR_NAME", author), ("GIT_COMMITTER_DATE", date)],
            &["commit", "-m", message],
        );
        self.head()
    }

    /// 打开目标并按筛选读一页。
    fn page(
        &self,
        target: &revia_lib::history::HistoryTarget,
        start: HistoryStart,
        filter: &HistoryFilter,
        limit: u32,
    ) -> revia_lib::history::HistoryPage {
        self.all_pages(target, start, filter, limit)
            .into_iter()
            .next()
            .expect("至少一页")
    }

    /// 按筛选读完所有分页（每页 `limit` 条）。
    fn all_pages(
        &self,
        target: &revia_lib::history::HistoryTarget,
        start: HistoryStart,
        filter: &HistoryFilter,
        limit: u32,
    ) -> Vec<revia_lib::history::HistoryPage> {
        let mut pages = Vec::new();
        let mut cursor = None;
        loop {
            let page = target
                .log_page_filtered(Some(start.clone()), false, limit, cursor, filter)
                .expect("读取分页");
            let done = page.exhausted;
            cursor = page.next_cursor.clone();
            pages.push(page);
            if done {
                break;
            }
        }
        pages
    }

    fn subjects(&self, target: &revia_lib::history::HistoryTarget, filter: &HistoryFilter) -> Vec<String> {
        self.all_pages(target, all_refs_start(), filter, 50)
            .into_iter()
            .flat_map(|page| page.commits)
            .map(|commit| commit.subject)
            .collect()
    }
}

fn all_refs_start() -> HistoryStart {
    HistoryStart::Refs { full_names: Vec::new() }
}

fn text_filter(text: &str) -> HistoryFilter {
    HistoryFilter {
        text: Some(text.to_string()),
        ..HistoryFilter::default()
    }
}

#[test]
fn text_filter_matches_message_case_and_hash_prefix() {
    let f = Fixture::new();
    let c1 = f.track("a.txt", b"one\n", "first commit");
    let c2 = f.track("a.txt", b"two\n", "Second change");
    let c3 = f.track("b.txt", b"bee\n", "third commit");
    let (target_id, _) = f.open();
    let target = f.target(&target_id);

    // 子串匹配提交信息（默认不区分大小写）。
    let subjects = f.subjects(&target, &text_filter("SECOND"));
    assert_eq!(subjects, vec!["Second change"]);
    // 正文也算提交信息的一部分。
    let subjects = f.subjects(&target, &text_filter("first"));
    assert_eq!(subjects, vec!["first commit"]);

    // 区分大小写。
    let mut filter = text_filter("second");
    filter.case_sensitive = true;
    assert!(f.subjects(&target, &filter).is_empty());
    let mut filter = text_filter("Second");
    filter.case_sensitive = true;
    assert_eq!(f.subjects(&target, &filter), vec!["Second change"]);

    // 哈希前缀：小写、大写、缩写都只命中那一个提交。
    let prefix = c2.chars().take(8).collect::<String>();
    let subjects = f.subjects(&target, &text_filter(&prefix));
    assert_eq!(subjects, vec!["Second change"]);
    let subjects = f.subjects(&target, &text_filter(&prefix.to_uppercase()));
    assert_eq!(subjects, vec!["Second change"]);
    // 完整 OID 同样只命中自己。
    assert_eq!(f.subjects(&target, &text_filter(&c3)), vec!["third commit"]);

    // 哈希前缀筛选同样走分页：每页一条，逐页读到末尾。
    let pages = f.all_pages(&target, all_refs_start(), &text_filter(&prefix), 1);
    let subjects: Vec<String> = pages.into_iter().flat_map(|page| page.commits).map(|c| c.subject).collect();
    assert_eq!(subjects, vec!["Second change"]);
    assert_ne!(c1, c3);
}

#[test]
fn text_filter_scan_pages_and_span_limits_are_explicit() {
    let f = Fixture::new();
    for index in 0..6 {
        f.track("a.txt", format!("line {index}\n").as_bytes(), &format!("commit {index}"));
    }
    let (target_id, _) = f.open();
    let target = f.target(&target_id);
    // 每页两条，逐页拼起来与一次性读取一致，没有重复或遗漏。
    let paged: Vec<String> = f
        .all_pages(&target, all_refs_start(), &text_filter("commit"), 2)
        .into_iter()
        .flat_map(|page| page.commits)
        .map(|commit| commit.oid)
        .collect();
    let single: Vec<String> = f
        .page(&target, all_refs_start(), &text_filter("commit"), 50)
        .commits
        .into_iter()
        .map(|commit| commit.oid)
        .collect();
    assert_eq!(paged, single);
    assert_eq!(paged.len(), 6);

    // 不存在的文字：读到历史末尾，明确标为已到头（不是错误，也不是限额）。
    let page = f.page(&target, all_refs_start(), &text_filter("不存在的提交"), 10);
    assert!(page.commits.is_empty());
    assert!(page.exhausted);
}

#[test]
fn regex_filter_uses_git_extended_regex() {
    let f = Fixture::new();
    f.track("a.txt", b"one\n", "second change");
    f.track("a.txt", b"two\n", "third change");
    let (target_id, _) = f.open();
    let target = f.target(&target_id);

    let mut filter = text_filter("^sec.*change$");
    filter.regex = true;
    assert_eq!(f.subjects(&target, &filter), vec!["second change"]);

    // 关闭正则时同样的文字按字面匹配，`.` 不是通配。
    let mut literal = text_filter("sec.*change");
    literal.regex = false;
    assert!(f.subjects(&target, &literal).is_empty());

    // 正则 + 区分大小写。
    let mut filter = text_filter("^SEC");
    filter.regex = true;
    filter.case_sensitive = true;
    assert!(f.subjects(&target, &filter).is_empty());
    filter.case_sensitive = false;
    assert_eq!(f.subjects(&target, &filter), vec!["second change"]);
}

#[test]
fn branch_author_and_date_filters_narrow_the_same_query() {
    let f = Fixture::new();
    let first = f.track("a.txt", b"one\n", "shared base");
    // 一条只在 feature 分支上的提交。
    f.git(&["checkout", "-b", "feature"]);
    let on_feature = f.track("feature.txt", b"feature\n", "feature only");
    f.git(&["checkout", "main"]);
    let on_main = f.track("main.txt", b"main\n", "main only");
    f.git(&["tag", "v1", &first]);
    let (target_id, _) = f.open();
    let target = f.target(&target_id);

    // 默认（不选分支）包含所有引用的提交。
    let all = f.subjects(&target, &HistoryFilter::default());
    assert_eq!(all.len(), 3);

    // 只选 feature：feature 及其祖先，不含 main 独有的提交。
    let filter = HistoryFilter {
        branches: vec!["refs/heads/feature".into()],
        ..HistoryFilter::default()
    };
    let subjects = f.subjects(&target, &filter);
    assert!(subjects.contains(&"feature only".to_string()));
    assert!(subjects.contains(&"shared base".to_string()));
    assert!(!subjects.contains(&"main only".to_string()));

    // 选多个引用（标签 + main）取并集。
    let filter = HistoryFilter {
        branches: vec!["refs/tags/v1".into(), "refs/heads/main".into()],
        ..HistoryFilter::default()
    };
    let subjects = f.subjects(&target, &filter);
    assert!(subjects.contains(&"main only".to_string()));
    assert!(!subjects.contains(&"feature only".to_string()));

    // 不存在的引用明确报错，不退回当前 HEAD。
    let bogus = HistoryFilter {
        branches: vec!["refs/heads/does-not-exist".into()],
        ..HistoryFilter::default()
    };
    let error = target
        .log_page_filtered(Some(all_refs_start()), false, 10, None, &bogus)
        .expect_err("不存在的分支应报错");
    assert!(error.message.contains("引用不存在"), "{}", error.message);

    // 作者筛选：候选取自历史，按作者身份匹配。
    f.write("c.txt", b"other\n");
    let _ = f.commit_as("Alice", "2026-01-05T10:00:00+08:00", "alice change");
    let filter = HistoryFilter {
        authors: vec!["Alice".into()],
        ..HistoryFilter::default()
    };
    assert_eq!(f.subjects(&target, &filter), vec!["alice change"]);

    // 日期筛选（自定义区间）：只保留区间内的提交。
    let filter = HistoryFilter {
        date: Some(HistoryDateFilter::Range {
            since: "2026-01-01".into(),
            until: "2026-01-31".into(),
        }),
        ..HistoryFilter::default()
    };
    assert_eq!(f.subjects(&target, &filter), vec!["alice change"]);
    // 区间外没有提交。
    let filter = HistoryFilter {
        date: Some(HistoryDateFilter::Range {
            since: "2027-01-01".into(),
            until: "2027-01-31".into(),
        }),
        ..HistoryFilter::default()
    };
    assert!(f.subjects(&target, &filter).is_empty());
    let _ = (on_feature, on_main);
}

#[test]
fn path_filter_is_literal_and_covers_directories() {
    let f = Fixture::new();
    f.track("dir/a.txt", b"a\n", "dir a");
    f.track("dir/b.txt", b"b\n", "dir b");
    f.track("--help", b"help\n", "dash file");
    f.track("a[1].txt", b"bracket\n", "bracket file");
    f.track("other.txt", b"other\n", "other file");
    let (target_id, _) = f.open();
    let target = f.target(&target_id);

    let subjects = |paths: Vec<String>| {
        f.subjects(
            &target,
            &HistoryFilter { paths, ..HistoryFilter::default() },
        )
    };
    // 目录按前缀覆盖其下内容。
    assert_eq!(subjects(vec!["dir".into()]), vec!["dir b", "dir a"]);
    // 文件名以 `-` 开头按字面处理，不当成选项。
    assert_eq!(subjects(vec!["--help".into()]), vec!["dash file"]);
    // 方括号按字面处理，不当成通配。
    assert_eq!(subjects(vec!["a[1].txt".into()]), vec!["bracket file"]);
    assert_eq!(subjects(vec!["*.txt".into()]), Vec::<String>::new());
    // 多个路径取并集。
    let mut both = subjects(vec!["other.txt".into(), "--help".into()]);
    both.sort();
    assert_eq!(both, vec!["dash file".to_string(), "other file".to_string()]);

    // 空路径与带换行的路径被拒绝。
    let error = target
        .log_page_filtered(
            Some(all_refs_start()),
            false,
            10,
            None,
            &HistoryFilter { paths: vec![String::new()], ..HistoryFilter::default() },
        )
        .expect_err("空路径应报错");
    assert!(error.message.contains("路径"), "{}", error.message);
    let error = target
        .log_page_filtered(
            Some(all_refs_start()),
            false,
            10,
            None,
            &HistoryFilter { paths: vec!["a\nb".into()], ..HistoryFilter::default() },
        )
        .expect_err("含换行的路径应报错");
    assert!(error.message.contains("路径"), "{}", error.message);
}

#[test]
fn cursor_bound_to_filter_and_first_parent_traversal() {
    let f = Fixture::new();
    for index in 0..5 {
        f.track("a.txt", format!("{index}\n").as_bytes(), &format!("commit {index}"));
    }
    let (target_id, _) = f.open();
    let target = f.target(&target_id);
    let filter = text_filter("commit");
    let first = target
        .log_page_filtered(Some(all_refs_start()), false, 2, None, &filter)
        .unwrap();
    let cursor = first.next_cursor.clone().unwrap();

    // 换一组筛选继续用旧游标：明确拒绝，不能悄悄换成另一条历史。
    let other = HistoryFilter {
        authors: vec!["Alice".into()],
        ..HistoryFilter::default()
    };
    let error = target
        .log_page_filtered(Some(all_refs_start()), false, 2, Some(cursor.clone()), &other)
        .expect_err("筛选变化后旧游标必须失效");
    assert!(error.message.contains("筛选"), "{}", error.message);

    // 同一筛选同一遍历方式可以继续。
    let second = target
        .log_page_filtered(Some(all_refs_start()), false, 2, Some(cursor), &filter)
        .unwrap();
    assert_eq!(second.query_id, first.query_id);

    // 只看第一父提交 + 筛选同时生效。
    let mut filter = HistoryFilter::default();
    filter.text = None;
    let page = target
        .log_page_filtered(Some(all_refs_start()), true, 10, None, &filter)
        .unwrap();
    assert!(page.first_parent);
    assert_eq!(page.commits.len(), 5);
}

#[test]
fn filter_values_are_validated() {
    let f = Fixture::new();
    f.track("a.txt", b"one\n", "first");
    let (target_id, _) = f.open();
    let target = f.target(&target_id);
    let check = |filter: HistoryFilter| {
        target
            .log_page_filtered(Some(all_refs_start()), false, 10, None, &filter)
            .expect_err("非法筛选值应被拒绝")
            .message
    };
    let message = check(HistoryFilter {
        date: Some(HistoryDateFilter::LastDays { days: 0 }),
        ..HistoryFilter::default()
    });
    assert!(message.contains("天数"), "{message}");
    let message = check(HistoryFilter {
        date: Some(HistoryDateFilter::Range { since: "2026-13-01".into(), until: "2026-12-01".into() }),
        ..HistoryFilter::default()
    });
    assert!(message.contains("日期"), "{message}");
    let message = check(HistoryFilter {
        date: Some(HistoryDateFilter::Range { since: "2026-12-01".into(), until: "2026-01-01".into() }),
        ..HistoryFilter::default()
    });
    assert!(message.contains("起始日期"), "{message}");
    let message = check(HistoryFilter {
        date: Some(HistoryDateFilter::Range { since: "2026/12/01".into(), until: "2026-12-02".into() }),
        ..HistoryFilter::default()
    });
    assert!(message.contains("日期"), "{message}");
}

/// 造一个双父 merge：`both.txt` 相对两个父提交都有差异（改动在不同行，能自动合并），
/// `side.txt` / `main.txt` 各自只在一边有差异。
fn merge_fixture(f: &Fixture) -> (String, String) {
    let base = b"one\ntwo\nthree\nfour\nfive\n";
    f.track("both.txt", base, "base");
    f.git(&["checkout", "-b", "side"]);
    f.track("side.txt", b"side\n", "side");
    f.track("both.txt", b"one\ntwo\nthree\nfour\nSIDE\n", "side both");
    let side = f.head();
    f.git(&["checkout", "main"]);
    f.track("main.txt", b"main\n", "main");
    f.track("both.txt", b"MAIN\ntwo\nthree\nfour\nfive\n", "main both");
    let ours = f.head();
    f.git(&["merge", "--no-ff", "side", "-m", "merge side"]);
    (ours, side)
}

#[test]
fn merge_files_default_matches_diff_tree_combined_set() {
    let f = Fixture::new();
    let (ours, side) = merge_fixture(&f);
    let merge = f.head();
    let (target_id, _) = f.open();
    let target = f.target(&target_id);

    let files = target.merge_files(&merge, HistoryMergeMode::Combined).unwrap();
    assert_eq!(files.mode, HistoryMergeMode::Combined);
    assert_eq!(files.parents, vec![ours.clone(), side.clone()]);
    assert_eq!(files.groups.len(), 1);
    let paths: Vec<&str> = files.groups[0].files.iter().map(|item| item.path.as_str()).collect();
    assert_eq!(paths, vec!["both.txt"]);
    assert_eq!(files.groups[0].diff_parent.as_deref(), Some(ours.as_str()));

    // 与 Git 自己的 `diff-tree -c` 文件集合一致。
    let combined = f.git(&["diff-tree", "-c", "--no-commit-id", "-r", "--name-only", &merge]);
    let expected: Vec<&str> = combined.lines().filter(|line| !line.is_empty()).collect();
    assert_eq!(expected, vec!["both.txt"]);

    // 没有改动文件时不能被当成普通提交读取。
    let plain = f.track("plain.txt", b"plain\n", "plain");
    let error = target.merge_files(&plain, HistoryMergeMode::Combined).expect_err("不是 merge");
    assert!(error.message.contains("merge"), "{}", error.message);
}

#[test]
fn merge_files_by_parent_lists_all_changes_per_parent() {
    let f = Fixture::new();
    let (ours, side) = merge_fixture(&f);
    let merge = f.head();
    let (target_id, _) = f.open();
    let target = f.target(&target_id);

    let files = target.merge_files(&merge, HistoryMergeMode::ByParent).unwrap();
    assert_eq!(files.mode, HistoryMergeMode::ByParent);
    assert_eq!(files.groups.len(), 2);
    assert_eq!(files.groups[0].parent.as_deref(), Some(ours.as_str()));
    assert_eq!(files.groups[1].parent.as_deref(), Some(side.as_str()));
    let ours_paths: Vec<&str> = files.groups[0].files.iter().map(|item| item.path.as_str()).collect();
    assert_eq!(ours_paths, vec!["both.txt", "side.txt"]);
    let side_paths: Vec<&str> = files.groups[1].files.iter().map(|item| item.path.as_str()).collect();
    assert_eq!(side_paths, vec!["both.txt", "main.txt"]);
}

#[test]
fn octopus_merge_defaults_to_parent_groups() {
    let f = Fixture::new();
    f.track("base.txt", b"base\n", "base");
    for name in ["one", "two", "three"] {
        f.git(&["checkout", "-b", name, "main"]);
        f.track(&format!("{name}.txt"), name.as_bytes(), name);
    }
    f.git(&["checkout", "main"]);
    f.track("main.txt", b"main\n", "main");
    f.git(&["merge", "--no-ff", "one", "two", "three", "-m", "octopus"]);
    let merge = f.head();
    let (target_id, _) = f.open();
    let target = f.target(&target_id);
    let detail = target.commit_detail(&merge).unwrap();
    // octopus：当前 HEAD + 三个被并入的分支。
    assert_eq!(detail.parents.len(), 4);

    let files = target.merge_files(&merge, HistoryMergeMode::Combined).unwrap();
    // 超过两个父提交：默认按父提交分组（不假装是三栏能显示的内容）。
    assert_eq!(files.mode, HistoryMergeMode::ByParent);
    assert_eq!(files.groups.len(), 4);

    // 三栏视图明确说明不支持。
    let view = target.merge_view(&merge, "one.txt").unwrap();
    assert!(view.unavailable.is_some());
    assert!(view.sides.is_empty());
    assert!(!view.result.missing);
}

#[test]
fn merge_view_reads_three_versions_and_rejects_unrelated_paths() {
    let f = Fixture::new();
    let (_ours, _side) = merge_fixture(&f);
    let merge = f.head();
    let (target_id, _) = f.open();
    let target = f.target(&target_id);

    let view = target.merge_view(&merge, "both.txt").unwrap();
    assert!(view.unavailable.is_none());
    assert_eq!(view.parents.len(), 2);
    assert_eq!(view.sides.len(), 2);
    let parent1 = match &view.sides[0].text {
        HistoryMergeText { text, missing, .. } => (text.clone(), *missing),
    };
    let parent2 = match &view.sides[1].text {
        HistoryMergeText { text, missing, .. } => (text.clone(), *missing),
    };
    assert!(parent1.0.starts_with("MAIN\n"));
    assert!(parent2.0.ends_with("SIDE\n"));
    assert!(view.result.text.starts_with("MAIN\n"));
    assert!(view.result.text.ends_with("SIDE\n"));
    assert!(!view.result.binary);
    assert!(!view.result.lossy);

    // 只在父提交 2 里存在的文件：父提交 1 一侧要明确标为不存在，而不是空文件。
    let view = target.merge_view(&merge, "side.txt").unwrap();
    assert!(view.sides[0].text.missing);
    assert_eq!(view.sides[1].text.text, "side\n");

    // 没有改动过的路径不能作为三栏目标。
    let error = target.merge_view(&merge, "base.txt").expect_err("未改动的路径应拒绝");
    assert!(error.message.contains("没有改动"), "{}", error.message);
}

#[test]
fn filters_reject_unknown_targets_and_do_not_touch_the_repository() {
    let f = Fixture::new();
    f.track("a.txt", b"one\n", "first");
    let (target_id, _) = f.open();
    let target = f.target(&target_id);
    let before = f.git(&["status", "--porcelain=v2", "--branch"]);
    let head = f.head();
    let refs = f.git(&["for-each-ref", "--format=%(refname) %(objectname)"]);
    let _ = target
        .log_page_filtered(Some(all_refs_start()), false, 10, None, &text_filter("first"))
        .unwrap();
    let _ = target
        .log_page_filtered(Some(all_refs_start()), true, 10, None, &HistoryFilter::default())
        .unwrap();
    assert_eq!(f.head(), head);
    assert_eq!(f.git(&["status", "--porcelain=v2", "--branch"]), before);
    assert_eq!(f.git(&["for-each-ref", "--format=%(refname) %(objectname)"]), refs);
}

#[test]
fn author_filter_stays_literal_in_both_matching_modes() {
    let f = Fixture::new();
    f.track("base.txt", b"base\n", "base");
    // `+` 与邮箱里的 `.` 都必须按字面匹配，不能误命中另一个身份。
    for (file, name, email, message) in [
        ("plus.txt", "A+B", "a.b@example.test", "plus author"),
        ("triple.txt", "AAAB", "axb@example.test", "triple author"),
    ] {
        f.write(file, b"content\n");
        f.git(&["--literal-pathspecs", "add", "--", file]);
        f.git_with_env(
            &[("GIT_AUTHOR_NAME", name), ("GIT_AUTHOR_EMAIL", email)],
            &["commit", "-m", message],
        );
    }
    let (target_id, _) = f.open();
    let target = f.target(&target_id);

    // 勾选正则但没有有效文字时不生成 --grep/-E，作者仍使用字面匹配器。
    for regex in [false, true] {
        for text in [None, Some(""), Some(" \t "), Some("author")] {
            for (author, expected) in [
                ("A+B", "plus author"),
                ("AAAB", "triple author"),
                ("a.b@example.test", "plus author"),
                ("axb@example.test", "triple author"),
                ("A+B <a.b@example.test>", "plus author"),
            ] {
                let filter = HistoryFilter {
                    authors: vec![author.into()],
                    text: text.map(str::to_string),
                    regex,
                    ..HistoryFilter::default()
                };
                assert_eq!(
                    f.subjects(&target, &filter),
                    vec![expected],
                    "regex={regex}, text={text:?}, author={author}",
                );
            }
        }
    }
}
