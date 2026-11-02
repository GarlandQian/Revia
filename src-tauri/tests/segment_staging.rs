//! M4 返修 A1/Q142：按段（块）写操作与忽略空白档位（Q140）。
//!
//! 全部用真实 Git 仓库验证：
//! - 一个 hunk 含三段时只暂存中间一段；
//! - 取消暂存与丢弃一段（反向规则）；
//! - 两段之间只隔一行上下文；
//! - CRLF；
//! - 忽略空白档位下段来自 `-w` diff，应用加 `--ignore-whitespace`；
//! - “同时忽略空行”档位拒绝按段写；
//! - 指纹档位不符时拒绝（指纹随段签发，外部改动后拒绝）。

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use revia_lib::error::ErrorKind;
use revia_lib::git::segments::{segment_credential, segments_of_hunk, IgnoreWhitespace};
use revia_lib::git::status::ChangesKind;
use revia_lib::git::{CommandLog, Git};
use revia_lib::session::WorktreeSession;
use revia_lib::shell::{EnvCaptureInfo, ShellEnv};

struct TestContext {
    _temp_dir: tempfile::TempDir,
    repo: std::path::PathBuf,
    env: HashMap<String, String>,
    paths: Vec<String>,
}

impl TestContext {
    fn new() -> Self {
        let temp_dir = tempfile::tempdir().expect("create temporary repository");
        let repo = temp_dir.path().join("repo");
        let home = temp_dir.path().join("home");
        fs::create_dir_all(&repo).unwrap();
        fs::create_dir_all(&home).unwrap();
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
            ("HOME".into(), home.display().to_string()),
            ("USERPROFILE".into(), home.display().to_string()),
            ("XDG_CONFIG_HOME".into(), home.display().to_string()),
            ("GIT_CONFIG_GLOBAL".into(), global_config.display().to_string()),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("GIT_TERMINAL_PROMPT".into(), "0".into()),
            ("PATH".into(), joined_path.to_string_lossy().into_owned()),
        ]);
        for key in ["SYSTEMROOT", "WINDIR", "TEMP", "TMP", "TMPDIR"] {
            if let Ok(value) = std::env::var(key) {
                env.insert(key.into(), value);
            }
        }
        let ctx = Self { _temp_dir: temp_dir, repo, env, paths };
        ctx.git(&["init", "-b", "main"]);
        for (key, value) in [
            ("user.name", "Revia Segment Test"),
            ("user.email", "[EMAIL]"),
            ("commit.gpgsign", "false"),
            ("core.quotePath", "false"),
            ("core.autocrlf", "false"),
            ("core.safecrlf", "false"),
        ] {
            ctx.git(&["config", key, value]);
        }
        ctx
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
            String::from_utf8_lossy(&output.stderr),
        );
        String::from_utf8(output.stdout).unwrap()
    }

    fn write(&self, path: &str, text: &str) {
        fs::write(self.repo.join(path), text).unwrap();
    }

    fn read(&self, path: &str) -> String {
        fs::read_to_string(self.repo.join(path)).unwrap()
    }

    fn track(&self, path: &str, text: &str) {
        self.write(path, text);
        self.git(&["--literal-pathspecs", "add", "--", path]);
        self.git(&["commit", "-m", "fixture"]);
    }

    fn index_bytes(&self, path: &str) -> String {
        let oid = {
            let out = self.git(&["--literal-pathspecs", "ls-files", "--stage", "--", path]);
            out.split_whitespace().nth(1).expect("index entry").to_string()
        };
        let output = self.command().args(["cat-file", "blob", &oid]).output().unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap()
    }

    fn open(&self) -> Arc<WorktreeSession> {
        let shell_env = ShellEnv {
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
                fallback_reason: None,
                fallback_detail: None,
            },
        };
        WorktreeSession::open("test-worktree", &self.repo, shell_env, Arc::new(CommandLog::default())).unwrap()
    }

    fn bare_git(&self) -> Git {
        let shell_env = ShellEnv {
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
                fallback_reason: None,
                fallback_detail: None,
            },
        };
        Git::new(&self.repo, &shell_env, Arc::new(CommandLog::default())).unwrap()
    }
}

/// 三段、段间只隔一行上下文：
/// 行1 保持；行2 改（段A）；行3 上下文；行4 改（段B）；行5 上下文；行6 改（段C）；行7 保持。
const THREE_SEGMENTS_BEFORE: &str = "line1\nalpha\nctx1\nbeta\nctx2\ngamma\nline7\n";
const THREE_SEGMENTS_AFTER: &str = "line1\nALPHA\nctx1\nBETA\nctx2\nGAMMA\nline7\n";

fn entry_fingerprint(session: &WorktreeSession, path: &str) -> (String, String) {
    let snapshot = session.refresh().unwrap();
    let entry = snapshot
        .entries
        .iter()
        .find(|entry| entry.path == path && entry.changes == ChangesKind::Unstaged)
        .expect("unstaged entry");
    (entry.fingerprint.clone(), entry.snapshot_revision.clone())
}

/// 显示时与段同批签发的写入凭证（文件指纹 + 档位）。
fn credential(fingerprint: &str, ignore: IgnoreWhitespace) -> String {
    segment_credential(fingerprint, ignore)
}

#[test]
fn stage_only_middle_segment_of_three_segment_hunk() {
    let ctx = TestContext::new();
    ctx.track("f.txt", THREE_SEGMENTS_BEFORE);
    // 工作区三处改动；-U3 下相邻改动共享上下文会合成一个 hunk（相距 2 行）。
    ctx.write("f.txt", THREE_SEGMENTS_AFTER);

    let session = ctx.open();
    let git = ctx.bare_git();
    let diff = git.diff_worktree(Some("f.txt")).unwrap();
    let parsed = revia_lib::git::diff::parse_bytes(&diff);
    let file = parsed.files.first().expect("diff parses");
    assert_eq!(file.hunks.len(), 1, "fixture: one hunk");
    let segments = segments_of_hunk(&file.hunks[0], 0);
    assert_eq!(segments.len(), 3, "three changes separated by one context line");

    // 只暂存中间一段（B）。
    let (fingerprint, revision) = entry_fingerprint(&session, "f.txt");
    session
        .execute_segment_op("f.txt", &credential(&fingerprint, IgnoreWhitespace::Off), 0, 1, true, IgnoreWhitespace::Off)
        .unwrap();

    let index = ctx.index_bytes("f.txt");
    // 暂存区只多了 BETA 这一行改动；其余两段仍是基线内容。
    assert!(index.contains("BETA"), "index should contain the staged segment");
    assert!(!index.contains("ALPHA"), "other segments must stay unstaged");
    assert!(!index.contains("GAMMA"), "other segments must stay unstaged");
    assert!(index.contains("alpha"), "unstaged segments keep baseline text; index={:?}", index);
    assert!(index.contains("gamma"), "unstaged segments keep baseline text");
    // 工作区不变。
    assert_eq!(ctx.read("f.txt"), THREE_SEGMENTS_AFTER);
    let _ = revision;
}

#[test]
fn unstage_and_discard_segment_use_reverse_rules() {
    let ctx = TestContext::new();
    ctx.track("f.txt", THREE_SEGMENTS_BEFORE);
    // 全部暂存后，暂存区相对 HEAD 有三段；只取消暂存中间一段。
    ctx.write("f.txt", THREE_SEGMENTS_AFTER);
    ctx.git(&["--literal-pathspecs", "add", "--", "f.txt"]);

    let session = ctx.open();
    let git = ctx.bare_git();
    let diff = git.diff_index_with_orig("f.txt", None).unwrap();
    let parsed = revia_lib::git::diff::parse_bytes(&diff);
    let file = parsed.files.first().unwrap();
    let segments = segments_of_hunk(&file.hunks[0], 0);
    assert_eq!(segments.len(), 3);

    let snapshot = session.refresh().unwrap();
    let entry = snapshot
        .entries
        .iter()
        .find(|entry| entry.path == "f.txt" && entry.changes == ChangesKind::Staged)
        .expect("staged entry");
    session
        .execute_segment_op("f.txt", &credential(&entry.fingerprint, IgnoreWhitespace::Off), 0, 1, false, IgnoreWhitespace::Off)
        .unwrap();

    let index = ctx.index_bytes("f.txt");
    // 取消暂存 B 后：A、C 的改动仍在暂存区，B 回到原文本。
    assert!(index.contains("ALPHA"));
    assert!(index.contains("GAMMA"));
    assert!(index.contains("beta"), "unsegmented line returns to original");
    assert!(!index.contains("BETA"));
}

#[test]
fn discard_segment_saves_recovery_record_and_applies_reverse() {
    let ctx = TestContext::new();
    ctx.track("f.txt", THREE_SEGMENTS_BEFORE);
    ctx.write("f.txt", THREE_SEGMENTS_AFTER);

    let session = ctx.open();
    let git = ctx.bare_git();
    let diff = git.diff_worktree(Some("f.txt")).unwrap();
    let parsed = revia_lib::git::diff::parse_bytes(&diff);
    let file = parsed.files.first().unwrap();
    let segments = segments_of_hunk(&file.hunks[0], 0);

    let store = revia_lib::recovery::RecoveryStore::new(ctx._temp_dir.path().join("recovery"));
    let (fingerprint, revision) = entry_fingerprint(&session, "f.txt");
    let next = session
        .discard_segment("f.txt", &credential(&fingerprint, IgnoreWhitespace::Off), &revision, 0, 1, IgnoreWhitespace::Off, &store)
        .unwrap();
    let _ = next;

    // 工作区：中间段回到原文本，其余两段保留。
    let worktree = ctx.read("f.txt");
    assert!(worktree.contains("ALPHA"));
    assert!(worktree.contains("GAMMA"));
    assert!(worktree.contains("beta"), "discarded segment restored");
    assert!(!worktree.contains("BETA"));
    // 回收区有记录。
    let records = store.list(None).unwrap();
    assert_eq!(records.len(), 1, "discard must save a recovery record first");
    let _ = segments;
}

#[test]
fn crlf_files_stage_segment_preserving_crlf() {
    let ctx = TestContext::new();
    ctx.track("f.txt", "line1\r\nalpha\r\nctx\r\nline4\r\n");
    ctx.write("f.txt", "line1\r\nALPHA\r\nctx\r\nline4\r\n");

    let session = ctx.open();
    let git = ctx.bare_git();
    let diff = git.diff_worktree(Some("f.txt")).unwrap();
    let parsed = revia_lib::git::diff::parse_bytes(&diff);
    let file = parsed.files.first().unwrap();
    let segments = segments_of_hunk(&file.hunks[0], 0);
    assert_eq!(segments.len(), 1);

    let (fingerprint, _) = entry_fingerprint(&session, "f.txt");
    session
        .execute_segment_op("f.txt", &credential(&fingerprint, IgnoreWhitespace::Off), 0, 0, true, IgnoreWhitespace::Off)
        .unwrap();
    let index = ctx.index_bytes("f.txt");
    assert!(index.contains("ALPHA\r\n"), "CRLF preserved in staged result");
}

#[test]
fn whitespace_ignore_segment_from_w_diff_applies_with_ignore_whitespace() {
    let ctx = TestContext::new();
    // 基线：缩进 4 空格。
    ctx.track("f.txt", "line1\n    alpha\n    alpha2\nctx\n");
    // 工作区：缩进全部改为 8 空格 + alpha2 行有一个真实改动（文本变化）。
    ctx.write("f.txt", "line1\n        alpha\n        BETA2\nctx\n");

    let session = ctx.open();
    let git = ctx.bare_git();
    // -w 下只剩真实改动。
    let diff = git.diff_worktree_with_ignore(Some("f.txt"), IgnoreWhitespace::All).unwrap();
    let parsed = revia_lib::git::diff::parse_bytes(&diff);
    let file = parsed.files.first().unwrap();
    let segments = segments_of_hunk(&file.hunks[0], 0);
    assert_eq!(segments.len(), 1, "-w diff keeps only the real change");
    // 真实改动行是 BETA2（新文本）。
    let segment = &segments[0];
    let _ = segment;
    let texts: Vec<&str> = file.hunks[0]
        .lines
        .iter()
        .filter(|line| matches!(line.kind, revia_lib::git::diff::LineKind::Addition))
        .map(|line| line.text.as_str())
        .collect();
    assert!(texts.iter().any(|text| text.contains("BETA2")), "real change is the text edit");

    // 普通指纹（真实状态），按段暂存这一段。
    let (fingerprint, _) = entry_fingerprint(&session, "f.txt");
    session
        .execute_segment_op("f.txt", &credential(&fingerprint, IgnoreWhitespace::All), 0, segment.segment_index, true, IgnoreWhitespace::All)
        .unwrap();

    // 暂存区：BETA2 的文本改动已进暂存（新行内容取自工作区，带工作区的缩进）；
    // 缩进改动（alpha 行 8 空格 vs 基线 4 空格）全部留在未暂存。
    let index = ctx.index_bytes("f.txt");
    assert!(index.contains("BETA2"), "real change staged");
    assert!(index.contains("    alpha\n"), "indent-only lines stay as baseline");
    // 纯空白改动留在原处：alpha 行的缩进差异仍是未暂存改动。
    let unstaged = git.diff_worktree(Some("f.txt")).unwrap();
    assert!(
        unstaged.windows(5).any(|window| window == b"alpha"),
        "unstaged diff still lists alpha line; unstaged={:?} index={:?}",
        String::from_utf8_lossy(&unstaged),
        String::from_utf8_lossy(index.as_bytes()),
    );
}

#[test]
fn blank_lines_level_rejects_segment_writes() {
    let ctx = TestContext::new();
    ctx.track("f.txt", "line1\nalpha\nctx\n");
    ctx.write("f.txt", "line1\nALPHA\nctx\n");

    let session = ctx.open();
    let (fingerprint, _) = entry_fingerprint(&session, "f.txt");
    let error = session
        .execute_segment_op("f.txt", &credential(&fingerprint, IgnoreWhitespace::AllAndBlankLines), 0, 0, true, IgnoreWhitespace::AllAndBlankLines)
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Rejected, "{error}");
    // 工作区与暂存区都没变。
    assert_eq!(ctx.read("f.txt"), "line1\nALPHA\nctx\n");
}

#[test]
fn stale_fingerprint_is_rejected_and_segment_id_must_match_display() {
    let ctx = TestContext::new();
    ctx.track("f.txt", "line1\nalpha\nctx\n");
    ctx.write("f.txt", "line1\nALPHA\nctx\n");
    let session = ctx.open();
    let (fingerprint, _) = entry_fingerprint(&session, "f.txt");
    // 外部改动后指纹不符：拒绝。
    ctx.write("f.txt", "line1\nALPHA2\nctx\n");
    let error = session
        .execute_segment_op("f.txt", &credential(&fingerprint, IgnoreWhitespace::Off), 0, 0, true, IgnoreWhitespace::Off)
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Rejected, "{error}");
}

#[test]
fn pure_whitespace_change_stays_when_staging_segment_with_w() {
    // 规划方验证过的场景：缩进全改 + 一行真实改动；-w 只剩真实改动，
    // `git apply --cached --ignore-whitespace` 后暂存区只多这一行。
    let ctx = TestContext::new();
    ctx.track("f.txt", "a\n    keep-indent\nb\n");
    ctx.write("f.txt", "a\n        keep-indent\nREAL CHANGE\n");

    let git = ctx.bare_git();
    let diff = git.diff_worktree_with_ignore(Some("f.txt"), IgnoreWhitespace::All).unwrap();
    let parsed = revia_lib::git::diff::parse_bytes(&diff);
    let file = parsed.files.first().unwrap();
    let additions: Vec<&str> = file
        .hunks
        .iter()
        .flat_map(|hunk| hunk.lines.iter())
        .filter(|line| line.kind == revia_lib::git::diff::LineKind::Addition)
        .map(|line| line.text.as_str())
        .collect();
    assert!(
        additions.iter().any(|text| text.contains("REAL CHANGE")),
        "real change survives -w: {additions:?}"
    );
    assert!(
        !additions.iter().any(|text| text.contains("keep-indent")),
        "indent-only change disappears under -w"
    );
}

// ---------- 返修 A1/Q138：左右对照布局与按侧取行 ----------

use revia_lib::git::rows::{RowKind, ViewMode};
use revia_lib::git::segments::{SideLine, WordGranularity};
use revia_lib::session::FileVersion;

/// 两处改动相距很远（中间 20 行未改动），-U3 下会得到两个 hunk，也产生可折叠区间。
const SPREAD_BEFORE: &str = "head\nkeep\n\
line3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\n\
line11\nline12\nline13\nline14\nline15\nline16\nline17\nline18\nline19\nline20\n\
tail\n";
const SPREAD_AFTER: &str = "head\nCHANGED\n\
line3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\n\
line11\nline12\nline13\nline14\nline15\nline16\nline17\nline18\nline19\nline20\n\
TAIL\n";

#[test]
fn side_layout_reports_labels_line_counts_segments_and_folds() {
    let ctx = TestContext::new();
    ctx.track("spread.txt", SPREAD_BEFORE);
    ctx.write("spread.txt", SPREAD_AFTER);
    let session = ctx.open();
    session.refresh().unwrap();

    let layout = session
        .side_layout("spread.txt", ChangesKind::Unstaged, IgnoreWhitespace::Off, 3)
        .unwrap();
    assert!(!layout.conflict && !layout.binary);
    assert_eq!(layout.left_line_count, 21);
    assert_eq!(layout.right_line_count, 21);
    assert_eq!(layout.segments.len(), 2, "两处改动各成一段");
    assert!(layout.segment_actions_available);
    assert_eq!(layout.segment_actions.len(), 2);
    assert_eq!(layout.segment_actions[0].hunk_index, 0);
    assert_eq!(layout.segment_actions[0].segment_index, 0);
    assert_eq!(layout.segment_actions[1].hunk_index, 1);
    assert!(!layout.folds.is_empty(), "中间 20 行未改动应可折叠");
    // 第一段：第 2 行（0 基 1..2）。
    assert_eq!((layout.segments[0].left_start, layout.segments[0].left_end), (1, 2));
    assert_eq!((layout.segments[0].right_start, layout.segments[0].right_end), (1, 2));
    // 第二段：第 21 行。
    assert_eq!((layout.segments[1].left_start, layout.segments[1].left_end), (20, 21));
}

#[test]
fn side_layout_uses_head_and_index_labels_for_staged() {
    let ctx = TestContext::new();
    ctx.track("s.txt", "a\nb\n");
    ctx.write("s.txt", "a\nB\n");
    ctx.git(&["--literal-pathspecs", "add", "--", "s.txt"]);
    let session = ctx.open();
    session.refresh().unwrap();
    let layout = session
        .side_layout("s.txt", ChangesKind::Staged, IgnoreWhitespace::Off, 3)
        .unwrap();
    assert_eq!(layout.changes, ChangesKind::Staged);
    assert_eq!(layout.segments.len(), 1);
}

#[test]
fn side_layout_for_untracked_has_empty_left_side() {
    let ctx = TestContext::new();
    ctx.write("new.txt", "one\ntwo\n");
    let session = ctx.open();
    session.refresh().unwrap();
    let layout = session
        .side_layout("new.txt", ChangesKind::Untracked, IgnoreWhitespace::Off, 3)
        .unwrap();
    assert_eq!(layout.changes, ChangesKind::Untracked);
    assert_eq!(layout.left_line_count, 0);
    assert_eq!(layout.right_line_count, 2);
}

#[test]
fn side_lines_mark_changed_lines_and_words() {
    let ctx = TestContext::new();
    ctx.track("w.txt", "const count = ref(0);\nkeep\n");
    ctx.write("w.txt", "const count = ref(1);\nkeep\n");
    let session = ctx.open();
    session.refresh().unwrap();

    let left = session
        .side_lines("w.txt", ChangesKind::Unstaged, IgnoreWhitespace::Off, 3, FileVersion::Old, 1, 2, WordGranularity::Word)
        .unwrap();
    let right = session
        .side_lines("w.txt", ChangesKind::Unstaged, IgnoreWhitespace::Off, 3, FileVersion::New, 1, 2, WordGranularity::Word)
        .unwrap();
    assert_eq!(left.len(), 2);
    assert_eq!(right.len(), 2);
    assert_eq!(left[0].text, "const count = ref(0);");
    assert!(left[0].changed && !left[1].changed, "只有改动的行带标记");
    let words: Vec<SideLine> = vec![left[0].clone(), right[0].clone()];
    for line in &words {
        assert_eq!(line.words.len(), 1, "只标出改动的那个字：{line:?}");
        let (start, end) = line.words[0];
        let marked = &line.text[start as usize..end as usize];
        assert!(marked == "0" || marked == "1", "改动字词是 0 或 1，实际 {marked}");
    }
    // 按行高亮时不返回词级区间。
    let plain = session
        .side_lines("w.txt", ChangesKind::Unstaged, IgnoreWhitespace::Off, 3, FileVersion::Old, 1, 1, WordGranularity::Line)
        .unwrap();
    assert!(plain[0].changed);
    assert!(plain[0].words.is_empty());
}

#[test]
fn side_lines_pages_preserve_changed_flags_and_words_at_segment_edges() {
    let ctx = TestContext::new();
    ctx.track("page.txt", "head\nconst a = 1;\nconst b = 2;\nconst c = 3;\nkeep\nlast");
    ctx.write("page.txt", "head\nconst a = 4;\nconst b = 5;\nconst c = 6;\nkeep\nLAST");
    let session = ctx.open();
    session.refresh().unwrap();
    for version in [FileVersion::Old, FileVersion::New] {
        for granularity in [WordGranularity::Word, WordGranularity::Char, WordGranularity::Line] {
            let all = session.side_lines("page.txt", ChangesKind::Unstaged, IgnoreWhitespace::Off, 3,
                version, 1, 6, granularity).unwrap();
            assert_eq!(all.iter().map(|line| line.changed).collect::<Vec<_>>(), [false, true, true, true, false, true]);
            for (from, to) in [(0, 0), (2, 2), (3, 3), (2, 4), (4, 6), (5, 5), (6, 20), (8, 10)] {
                let page = session.side_lines("page.txt", ChangesKind::Unstaged, IgnoreWhitespace::Off, 3,
                    version, from, to, granularity).unwrap();
                let first = from.max(1);
                let last = to.max(first);
                let expected = all.iter().filter(|line| line.line >= first && line.line <= last).collect::<Vec<_>>();
                assert_eq!(serde_json::to_value(&page).unwrap(), serde_json::to_value(&expected).unwrap(),
                    "range {from}..={to}, {version:?}, {granularity:?}");
            }
        }
    }
}

#[test]
fn unified_rows_carry_one_segment_index_per_block() {
    let ctx = TestContext::new();
    ctx.track("f.txt", THREE_SEGMENTS_BEFORE);
    ctx.write("f.txt", THREE_SEGMENTS_AFTER);
    let session = ctx.open();
    session.refresh().unwrap();
    let slice = session
        .rows_with_ignore("f.txt", ChangesKind::Unstaged, ViewMode::Unified, IgnoreWhitespace::Off, 0, u32::MAX)
        .unwrap();
    let mut indexes: Vec<u32> = slice
        .rows
        .iter()
        .filter_map(|row| row.segment_index)
        .collect();
    indexes.sort();
    assert_eq!(indexes, vec![0, 1, 2], "三段各有一个首行");
    assert_eq!(slice.entry.hunk_count, 1);
    assert_eq!(slice.navigation_rows, vec![2, 5, 8], "一个 Git hunk 内的三段分别导航");
    let page = session.rows_with_ignore("f.txt", ChangesKind::Unstaged, ViewMode::Unified,
        IgnoreWhitespace::Off, 4, 7).unwrap();
    assert_eq!(page.navigation_rows, slice.navigation_rows, "局部分页仍保留全部段首");
    for row in &slice.rows {
        if let Some(index) = row.segment_index {
            assert_eq!(row.segment_action.as_ref().map(|action| action.segment_index), Some(index));
        } else {
            assert!(row.segment_action.is_none());
        }
    }
}

#[test]
fn unified_navigation_follows_filtered_layout_and_preserves_write_credentials() {
    let ctx = TestContext::new();
    let mut lines: Vec<String> = (1..=60).map(|line| format!("line{line}")).collect();
    lines[3] = "tail space".into();
    lines[18] = "inside space".into();
    lines[48] = "value before".into();
    let before = format!("{}\n", lines.join("\n"));
    ctx.track("ws.txt", &before);
    lines[3] = "tail space   ".into();
    lines[18] = "inside    space".into();
    lines[48] = "value after".into();
    lines.insert(34, String::new());
    let after = format!("{}\n", lines.join("\n"));
    ctx.write("ws.txt", &after);
    let head = ctx.git(&["rev-parse", "HEAD"]);
    let session = ctx.open();
    let snapshot = session.refresh().unwrap();
    let original = snapshot.entries.iter().find(|entry| entry.path == "ws.txt").unwrap();
    assert_eq!(original.hunk_count, 4);

    for (ignore, expected_count) in [
        (IgnoreWhitespace::Off, 4),
        (IgnoreWhitespace::Eol, 3),
        (IgnoreWhitespace::All, 2),
        (IgnoreWhitespace::AllAndBlankLines, 1),
    ] {
        let all = session.rows_with_ignore("ws.txt", ChangesKind::Unstaged, ViewMode::Unified,
            ignore, 0, u32::MAX).unwrap();
        let expected: Vec<u32> = all.rows.iter().enumerate()
            .filter_map(|(index, row)| row.segment_index.map(|_| index as u32)).collect();
        assert_eq!(all.navigation_rows.len(), expected_count, "{ignore:?}");
        assert_eq!(all.navigation_rows, expected, "导航与过滤后正文的每段首行一致");
        assert_eq!(serde_json::to_value(&all.entry).unwrap(), serde_json::to_value(original).unwrap(),
            "显示导航不得改写原始列表元数据");
        assert_eq!(all.revision, snapshot.revision);
        for (from, to) in [(0, 1), (all.row_count, all.row_count)] {
            let page = session.rows_with_ignore("ws.txt", ChangesKind::Unstaged, ViewMode::Unified,
                ignore, from, to).unwrap();
            assert_eq!(page.navigation_rows, all.navigation_rows, "分页仍携带完整导航");
        }
        for &row_index in &all.navigation_rows {
            let page = session.rows_with_ignore("ws.txt", ChangesKind::Unstaged, ViewMode::Unified,
                ignore, row_index, row_index + 1).unwrap();
            let row = page.rows.first().unwrap();
            assert!(row.segment_index.is_some(), "导航必须落在当前排版的段首");
            assert_eq!(page.navigation_rows, all.navigation_rows);
            if ignore.allows_segment_actions() {
                let action = row.segment_action.as_ref().expect("可操作档位保留签发凭据");
                assert_eq!(action.worktree_id, "test-worktree");
                assert_eq!(action.path, original.path);
                assert_eq!(action.changes, original.changes);
                assert_eq!(action.fingerprint, original.fingerprint);
                assert_eq!(action.snapshot_revision, original.snapshot_revision);
                assert_eq!(action.ignore_whitespace, ignore);
                assert_eq!(action.credential, credential(&original.fingerprint, ignore));
                assert_eq!(Some(action.segment_index), row.segment_index);
            } else {
                assert!(row.segment_action.is_none(), "同时忽略空行仍只读");
            }
        }
    }
    assert_eq!(ctx.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(ctx.index_bytes("ws.txt"), before);
    assert_eq!(ctx.read("ws.txt"), after);
}

#[test]
fn view_anchor_row_index_matches_filtered_rows_and_rejects_stale_versions() {
    let request: revia_lib::commands::RowIndexRequest = serde_json::from_value(serde_json::json!({
        "worktreeId": "test-worktree", "path": "ws.txt", "changes": "unstaged",
        "view": "unified", "line": 50, "version": "new",
    })).unwrap();
    assert_eq!(request.ignore_whitespace, IgnoreWhitespace::Off, "旧请求缺省不忽略空白");
    assert!(request.expected_fingerprint.is_none() && request.expected_snapshot_revision.is_none());

    let ctx = TestContext::new();
    let mut lines: Vec<String> = (1..=60).map(|line| format!("line{line}")).collect();
    lines[3] = "tail space".into();
    lines[18] = "inside space".into();
    lines[48] = "value before".into();
    let before = format!("{}\n", lines.join("\n"));
    ctx.track("ws.txt", &before);
    lines[3] = "tail space   ".into();
    lines[18] = "inside    space".into();
    lines[48] = "value after".into();
    lines.insert(34, String::new());
    let after = format!("{}\n", lines.join("\n"));
    ctx.write("ws.txt", &after);
    let head = ctx.git(&["rev-parse", "HEAD"]);
    let session = ctx.open();
    let snapshot = session.refresh().unwrap();
    let original = snapshot.entries.iter().find(|entry| entry.path == "ws.txt").unwrap();
    let ordinary = session.row_index_for_line("ws.txt", ChangesKind::Unstaged,
        ViewMode::Unified, 50, FileVersion::New).unwrap();

    for (ignore, expected_hunks) in [
        (IgnoreWhitespace::Eol, 3),
        (IgnoreWhitespace::All, 2),
        (IgnoreWhitespace::AllAndBlankLines, 1),
    ] {
        // 定位先于取行，覆盖该档位缓存尚未建立的生产路径。
        let anchor = session.row_index_for_line_with_ignore("ws.txt", ChangesKind::Unstaged,
            ViewMode::Unified, 50, FileVersion::New, ignore,
            Some(&original.fingerprint), Some(&original.snapshot_revision)).unwrap();
        let rows = session.rows_with_ignore("ws.txt", ChangesKind::Unstaged,
            ViewMode::Unified, ignore, 0, u32::MAX).unwrap();
        assert_eq!(rows.navigation_rows.len(), expected_hunks, "{ignore:?}");
        assert_eq!(rows.revision, snapshot.revision);
        assert_eq!(rows.entry.fingerprint, original.fingerprint);
        assert_eq!(rows.rows[anchor as usize].new_line, Some(50));
        assert_eq!(rows.rows[anchor as usize].text.as_deref(), Some("value after"));
        assert_ne!(anchor, ordinary, "过滤前面的空白块后不能沿用普通排版的索引");
        for (version, tail_line) in [(FileVersion::Old, 60), (FileVersion::New, 61)] {
            let (last_index, last_line) = rows.rows.iter().enumerate().rev().find_map(|(index, row)| {
                if row.kind != RowKind::Line { return None; }
                let line = match version {
                    FileVersion::Old => row.old_line,
                    FileVersion::New => row.new_line,
                };
                line.map(|line| (index as u32, line))
            }).unwrap();
            assert!(last_index > 0 && last_line < tail_line, "fixture 尾部在最后 hunk 之后");
            let actual = session.row_index_for_line_with_ignore("ws.txt", ChangesKind::Unstaged,
                ViewMode::Unified, tail_line, version, ignore,
                Some(&original.fingerprint), Some(&original.snapshot_revision)).unwrap();
            assert_eq!(actual, last_index, "{ignore:?} / {version:?} 尾部保持在最后可见行，不回到文件头");
        }
        for (index, row) in rows.rows.iter().enumerate() {
            if row.kind != RowKind::Line { continue; }
            for (version, line) in [(FileVersion::Old, row.old_line), (FileVersion::New, row.new_line)] {
                if let Some(line) = line {
                    let actual = session.row_index_for_line_with_ignore("ws.txt", ChangesKind::Unstaged,
                        ViewMode::Unified, line, version, ignore,
                        Some(&original.fingerprint), Some(&original.snapshot_revision)).unwrap();
                    assert_eq!(actual, index as u32, "{ignore:?} / {version:?} / {line}");
                }
            }
        }
    }

    let wrong_fingerprint = session.row_index_for_line_with_ignore("ws.txt", ChangesKind::Unstaged,
        ViewMode::Unified, 50, FileVersion::New, IgnoreWhitespace::All,
        Some("stale-fingerprint"), Some(&original.snapshot_revision)).unwrap_err();
    assert_eq!(wrong_fingerprint.kind, ErrorKind::Rejected);
    let refreshed = session.refresh().unwrap();
    assert_ne!(refreshed.revision, snapshot.revision);
    let stale_revision = session.row_index_for_line_with_ignore("ws.txt", ChangesKind::Unstaged,
        ViewMode::Unified, 50, FileVersion::New, IgnoreWhitespace::All,
        Some(&original.fingerprint), Some(&original.snapshot_revision)).unwrap_err();
    assert_eq!(stale_revision.kind, ErrorKind::Rejected);
    let current = refreshed.entries.iter().find(|entry| entry.path == "ws.txt").unwrap();
    session.row_index_for_line_with_ignore("ws.txt", ChangesKind::Unstaged,
        ViewMode::Unified, 50, FileVersion::New, IgnoreWhitespace::All,
        Some(&current.fingerprint), Some(&current.snapshot_revision)).unwrap();
    assert_eq!(session.row_index_for_line("ws.txt", ChangesKind::Unstaged,
        ViewMode::Unified, 50, FileVersion::New).unwrap(), ordinary, "旧入口继续使用普通排版");
    assert_eq!(ctx.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(ctx.index_bytes("ws.txt"), before);
    assert_eq!(ctx.read("ws.txt"), after);
}

#[test]
fn unified_navigation_is_empty_when_all_changes_are_ignored() {
    let ctx = TestContext::new();
    ctx.track("ws.txt", "a\n    indented\nb\n");
    ctx.write("ws.txt", "a\n        indented\nb\n");
    let session = ctx.open();
    session.refresh().unwrap();
    let plain = session.rows_with_ignore("ws.txt", ChangesKind::Unstaged, ViewMode::Unified,
        IgnoreWhitespace::Off, 0, u32::MAX).unwrap();
    assert_eq!(plain.navigation_rows.len(), 1);
    for ignore in [IgnoreWhitespace::All, IgnoreWhitespace::AllAndBlankLines] {
        let filtered = session.rows_with_ignore("ws.txt", ChangesKind::Unstaged, ViewMode::Unified,
            ignore, 0, u32::MAX).unwrap();
        assert!(filtered.navigation_rows.is_empty());
        assert!(filtered.rows.is_empty());
        assert_eq!(filtered.row_count, 0);
        assert_eq!(serde_json::to_value(&filtered.entry).unwrap(), serde_json::to_value(&plain.entry).unwrap());
    }
    let restored = session.rows_with_ignore("ws.txt", ChangesKind::Unstaged, ViewMode::Unified,
        IgnoreWhitespace::Off, 0, u32::MAX).unwrap();
    assert_eq!(restored.navigation_rows, plain.navigation_rows);
}

#[test]
fn ignore_whitespace_layout_filters_indent_only_changes() {
    let ctx = TestContext::new();
    ctx.track("ws.txt", "a\n    indented\nb\n");
    ctx.write("ws.txt", "a\n        indented\nb\n");
    let session = ctx.open();
    session.refresh().unwrap();

    let plain = session
        .side_layout("ws.txt", ChangesKind::Unstaged, IgnoreWhitespace::Off, 3)
        .unwrap();
    assert_eq!(plain.segments.len(), 1, "普通档位能看到缩进改动");
    let filtered = session
        .side_layout("ws.txt", ChangesKind::Unstaged, IgnoreWhitespace::All, 3)
        .unwrap();
    assert!(filtered.segments.is_empty(), "-w 档位下缩进改动被忽略");
    assert!(filtered.segment_actions.is_empty());
    // 文件指纹不随档位变化，写入凭证则绑定签发时的档位（Q140）。
    assert_eq!(plain.fingerprint, filtered.fingerprint);
    assert_ne!(
        plain.segment_actions[0].credential,
        credential(&filtered.fingerprint, IgnoreWhitespace::All),
    );
}

#[test]
fn blank_line_ignore_level_disables_segment_buttons() {
    let ctx = TestContext::new();
    ctx.track("b.txt", "a\n\nb\n");
    ctx.write("b.txt", "a\n\n\nb\n");
    let session = ctx.open();
    session.refresh().unwrap();
    let layout = session
        .side_layout("b.txt", ChangesKind::Unstaged, IgnoreWhitespace::AllAndBlankLines, 3)
        .unwrap();
    assert!(!layout.segment_actions_available, "同时忽略空行时块按钮变灰");
    assert!(layout.segment_actions.is_empty());
}

// ---------- 规划方接手（Q141、Q142）：审查发现的问题与设计第 5 节缺的测试 ----------

#[test]
fn credential_from_another_whitespace_level_is_rejected() {
    let ctx = TestContext::new();
    ctx.track("f.txt", "a\n    keep\nb\n");
    ctx.write("f.txt", "a\n        keep\nB\n");
    let session = ctx.open();
    let (fingerprint, _) = entry_fingerprint(&session, "f.txt");
    // 段按 -w 签发，却按“不忽略”写入：段号在两份 diff 里含义不同，必须拒绝。
    let error = session
        .execute_segment_op("f.txt", &credential(&fingerprint, IgnoreWhitespace::All), 0, 0, true, IgnoreWhitespace::Off)
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Rejected, "{error}");
    assert_eq!(ctx.index_bytes("f.txt"), "a\n    keep\nb\n", "暂存区不变");
    assert_eq!(ctx.read("f.txt"), "a\n        keep\nB\n", "工作区不变");
}

#[test]
fn ignore_whitespace_segment_after_a_hunk_that_shifts_lines() {
    // 前一个 hunk 多出 5 行，后一个 hunk 的旧侧与新侧行号不同。
    let mut before = String::from("top\n");
    for line in 0..30 {
        before.push_str(&format!("    line{line}\n"));
    }
    let after = before
        .replacen("top\n", "top\nn1\nn2\nn3\nn4\nn5\n", 1)
        .replace("    line20\n", "        line20\n")
        .replace("    line21\n", "    LINE21\n");
    let ctx = TestContext::new();
    ctx.track("f.txt", &before);
    ctx.write("f.txt", &after);
    let session = ctx.open();
    let layout = {
        session.refresh().unwrap();
        session.side_layout("f.txt", ChangesKind::Unstaged, IgnoreWhitespace::All, 3).unwrap()
    };
    assert_eq!(layout.segments.len(), 2, "-w 下只剩新增 5 行与 LINE21 两段");
    let action = layout.segment_actions[1].clone();
    session
        .execute_segment_op("f.txt", &action.credential, action.hunk_index, action.segment_index, true, IgnoreWhitespace::All)
        .unwrap();
    let index = ctx.index_bytes("f.txt");
    assert!(index.contains("    LINE21\n"), "真实改动进入暂存区");
    assert!(index.contains("\n    line20\n"), "缩进改动留在未暂存");
    assert!(!index.contains("n1\n"), "另一段留在未暂存");

    // 再按 -w 取消暂存同一段。
    session.refresh().unwrap();
    let staged = session.side_layout("f.txt", ChangesKind::Staged, IgnoreWhitespace::All, 3).unwrap();
    let action = staged.segment_actions[0].clone();
    session
        .execute_segment_op("f.txt", &action.credential, action.hunk_index, action.segment_index, false, IgnoreWhitespace::All)
        .unwrap();
    assert_eq!(ctx.index_bytes("f.txt"), before, "取消暂存后暂存区回到基线");
}

#[test]
fn no_newline_at_end_of_file_segments_stage_and_discard() {
    let ctx = TestContext::new();
    ctx.track("f.txt", "first\nmiddle\nlast");
    ctx.write("f.txt", "FIRST\nmiddle\nLAST");
    let session = ctx.open();
    session.refresh().unwrap();
    let layout = session.side_layout("f.txt", ChangesKind::Unstaged, IgnoreWhitespace::Off, 3).unwrap();
    assert_eq!(layout.segments.len(), 2, "末行修改是一段，不被无换行标记拆开");
    let last = layout.segment_actions[1].clone();
    session
        .execute_segment_op("f.txt", &last.credential, last.hunk_index, last.segment_index, true, IgnoreWhitespace::Off)
        .unwrap();
    assert_eq!(ctx.index_bytes("f.txt"), "first\nmiddle\nLAST");

    // 丢弃第一段：末行保持无换行。
    session.refresh().unwrap();
    let layout = session.side_layout("f.txt", ChangesKind::Unstaged, IgnoreWhitespace::Off, 3).unwrap();
    let first = layout.segment_actions[0].clone();
    let store = revia_lib::recovery::RecoveryStore::new(ctx._temp_dir.path().join("recovery"));
    session
        .discard_segment("f.txt", &first.credential, &first.snapshot_revision, first.hunk_index, first.segment_index, IgnoreWhitespace::Off, &store)
        .unwrap();
    assert_eq!(ctx.read("f.txt"), "first\nmiddle\nLAST");
}

#[test]
fn segments_at_file_head_and_tail_stage_and_discard() {
    let ctx = TestContext::new();
    let before: String = (1..=12).map(|line| format!("l{line}\n")).collect();
    let after = before.replacen("l1\n", "L1\n", 1).replace("l12\n", "l12\nextra\n");
    ctx.track("f.txt", &before);
    ctx.write("f.txt", &after);
    let session = ctx.open();
    session.refresh().unwrap();
    let layout = session.side_layout("f.txt", ChangesKind::Unstaged, IgnoreWhitespace::Off, 3).unwrap();
    assert_eq!(layout.segments.len(), 2);
    // 文件头一段（前面没有上下文）：暂存。
    let head = layout.segment_actions[0].clone();
    session
        .execute_segment_op("f.txt", &head.credential, head.hunk_index, head.segment_index, true, IgnoreWhitespace::Off)
        .unwrap();
    assert!(ctx.index_bytes("f.txt").starts_with("L1\n"));
    assert!(!ctx.index_bytes("f.txt").contains("extra"));
    // 文件尾一段（后面没有上下文）：丢弃。
    session.refresh().unwrap();
    let layout = session.side_layout("f.txt", ChangesKind::Unstaged, IgnoreWhitespace::Off, 3).unwrap();
    let tail = layout.segment_actions[0].clone();
    let store = revia_lib::recovery::RecoveryStore::new(ctx._temp_dir.path().join("recovery"));
    session
        .discard_segment("f.txt", &tail.credential, &tail.snapshot_revision, tail.hunk_index, tail.segment_index, IgnoreWhitespace::Off, &store)
        .unwrap();
    assert_eq!(ctx.read("f.txt"), before.replacen("l1\n", "L1\n", 1));
}

#[test]
fn rejected_segment_write_changes_neither_side() {
    let ctx = TestContext::new();
    ctx.track("f.txt", THREE_SEGMENTS_BEFORE);
    ctx.write("f.txt", THREE_SEGMENTS_AFTER);
    let session = ctx.open();
    let (fingerprint, revision) = entry_fingerprint(&session, "f.txt");
    // 不存在的段号：切不出补丁，暂存区与工作区都不动。
    let error = session
        .execute_segment_op("f.txt", &credential(&fingerprint, IgnoreWhitespace::Off), 0, 7, true, IgnoreWhitespace::Off)
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Rejected, "{error}");
    let store = revia_lib::recovery::RecoveryStore::new(ctx._temp_dir.path().join("recovery"));
    let error = session
        .discard_segment("f.txt", &credential(&fingerprint, IgnoreWhitespace::Off), &revision, 0, 7, IgnoreWhitespace::Off, &store)
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Rejected, "{error}");
    assert_eq!(ctx.index_bytes("f.txt"), THREE_SEGMENTS_BEFORE);
    assert_eq!(ctx.read("f.txt"), THREE_SEGMENTS_AFTER);
}

#[test]
fn conflicted_file_rejects_segment_writes() {
    let ctx = TestContext::new();
    ctx.track("c.txt", "base\n");
    ctx.git(&["checkout", "-b", "other"]);
    ctx.write("c.txt", "theirs\n");
    ctx.git(&["commit", "-am", "theirs"]);
    ctx.git(&["checkout", "main"]);
    ctx.write("c.txt", "ours\n");
    ctx.git(&["commit", "-am", "ours"]);
    let output = ctx.command().args(["merge", "other"]).output().unwrap();
    assert!(!output.status.success(), "fixture: merge conflicts");
    let worktree = ctx.read("c.txt");
    let session = ctx.open();
    let snapshot = session.refresh().unwrap();
    let entry = snapshot.entries.iter().find(|entry| entry.path == "c.txt").expect("conflict entry");
    let layout = session.side_layout("c.txt", entry.changes, IgnoreWhitespace::Off, 3).unwrap();
    assert!(layout.conflict);
    assert!(!layout.segments.is_empty(), "冲突文件只读对照 HEAD 与工作区，仍标出改动");
    assert!(layout.segment_actions.is_empty(), "冲突文件不签发块按钮");
    let error = session
        .execute_segment_op("c.txt", &credential(&entry.fingerprint, IgnoreWhitespace::Off), 0, 0, true, IgnoreWhitespace::Off)
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Rejected, "{error}");
    assert_eq!(ctx.read("c.txt"), worktree);
}
