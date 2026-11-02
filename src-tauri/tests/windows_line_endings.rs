//! M8 任务 03：Windows 路径与换行完整性（真实 Git、临时仓库、隔离配置）。
//!
//! 在 macOS 上同样可运行：`core.autocrlf` / `.gitattributes` 是 Git 的行为，不是平台分支。
//! Windows 专属的 verbatim / UNC 路径转换在 `platform::external_path_text` 的单元测试里覆盖。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use revia_lib::git::CommandLog;
use revia_lib::m7::edit::{self, EditNewline, EditSaveRequest};
use revia_lib::recovery::RecoveryStore;
use revia_lib::git::status::ChangesKind;
use revia_lib::session::WorktreeSession;
use revia_lib::shell::{EnvCaptureInfo, ShellEnv};

struct Fixture {
    _temp: tempfile::TempDir,
    repo: PathBuf,
    home: PathBuf,
    data: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("仓库 目录");
        let home = temp.path().join("home");
        let data = temp.path().join("app data");
        fs::create_dir_all(&repo).unwrap();
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&data).unwrap();
        fs::write(home.join(".gitconfig"), "").unwrap();
        let fixture = Self {
            _temp: temp,
            repo,
            home,
            data,
        };
        fixture.git(&["init", "-b", "main"]);
        for (key, value) in [
            ("user.name", "Revia Test"),
            ("user.email", "revia@example.invalid"),
            ("commit.gpgsign", "false"),
            ("core.autocrlf", "false"),
            ("core.quotePath", "false"),
        ] {
            fixture.git(&["config", key, value]);
        }
        fixture
    }

    fn git(&self, args: &[&str]) -> Vec<u8> {
        let output = Command::new("git")
            .current_dir(&self.repo)
            .env("HOME", &self.home)
            .env("GIT_CONFIG_GLOBAL", self.home.join(".gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?} 失败：{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    }

    fn write(&self, name: &str, bytes: &[u8]) {
        let path = self.repo.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn read(&self, name: &str) -> Vec<u8> {
        fs::read(self.repo.join(name)).unwrap()
    }

    fn commit(&self, message: &str) {
        self.git(&["add", "--all"]);
        self.git(&["commit", "-m", message]);
    }

    fn index_blob(&self, name: &str) -> Vec<u8> {
        self.git(&["cat-file", "blob", &format!(":{name}")])
    }

    fn session(&self) -> Arc<WorktreeSession> {
        let mut path_entries: Vec<String> =
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                .map(|path| path.to_string_lossy().into_owned())
                .collect();
        #[cfg(unix)]
        for extra in ["/opt/homebrew/bin", "/usr/bin", "/bin"] {
            if !path_entries.iter().any(|entry| entry == extra) {
                path_entries.push(extra.into());
            }
        }
        let path_value = std::env::join_paths(path_entries.iter().map(Path::new))
            .unwrap()
            .to_string_lossy()
            .into_owned();
        #[allow(unused_mut)]
        let mut vars = HashMap::from([
            ("HOME".to_string(), self.home.display().to_string()),
            (
                "USERPROFILE".to_string(),
                self.home.display().to_string(),
            ),
            (
                "GIT_CONFIG_GLOBAL".to_string(),
                self.home.join(".gitconfig").display().to_string(),
            ),
            ("GIT_CONFIG_NOSYSTEM".to_string(), "1".to_string()),
            ("GIT_TERMINAL_PROMPT".to_string(), "0".to_string()),
            ("PATH".to_string(), path_value),
        ]);
        #[cfg(windows)]
        for name in ["SystemRoot", "WINDIR", "COMSPEC", "PATHEXT", "TEMP", "TMP"] {
            if let Ok(value) = std::env::var(name) {
                vars.insert(name.to_string(), value);
            }
        }
        let env = ShellEnv {
            path: path_entries.clone(),
            vars,
            info: EnvCaptureInfo {
                worktree: self.repo.display().to_string(),
                shell: if cfg!(windows) {
                    "powershell.exe".into()
                } else {
                    "/bin/zsh".into()
                },
                elapsed_ms: 0,
                timed_out: false,
                used_fallback: true,
                var_count: path_entries.len(),
                path_entries,
                fallback_reason: None,
                fallback_detail: None,
            },
        };
        WorktreeSession::open("fixture", &self.repo, env, Arc::new(CommandLog::default()))
            .expect("打开会话失败")
    }
}

const PATH: &str = "中文 目录/带 空格.txt";
const COMMITTED: &[u8] = b"first\r\nsecond\r\nthird\r\n";

fn commit_crlf_file(fixture: &Fixture) {
    fixture.write(".gitattributes", b"*.txt text eol=crlf\n");
    fixture.write(PATH, COMMITTED);
    fixture.commit("初始化 CRLF 文件");
    // Git 在 index 里保存 LF，工作区按 eol=crlf 检出。
    assert_eq!(fixture.index_blob(PATH), b"first\nsecond\nthird\n");
    assert_eq!(fixture.read(PATH), COMMITTED);
}

#[test]
fn staging_crlf_file_keeps_worktree_bytes_and_git_normalizes_the_index() {
    let fixture = Fixture::new();
    commit_crlf_file(&fixture);
    let edited = b"first\r\nsecond changed\r\nthird\r\n";
    fixture.write(PATH, edited);

    let session = fixture.session();
    let snapshot = session.refresh().unwrap();
    let entry = snapshot
        .entries
        .iter()
        .find(|entry| entry.path == PATH)
        .expect("应找到 CRLF 文件");
    assert_eq!(entry.changes, ChangesKind::Unstaged);

    let staged = session.stage_file(&entry.path, &entry.fingerprint).unwrap();
    assert!(staged
        .entries
        .iter()
        .any(|entry| entry.path == PATH && entry.changes == ChangesKind::Staged));
    // 暂存不改工作区字节；index 按 .gitattributes 归一为 LF。
    assert_eq!(fixture.read(PATH), edited);
    assert_eq!(fixture.index_blob(PATH), b"first\nsecond changed\nthird\n");
}

#[test]
fn discard_restores_crlf_bytes_and_keeps_the_overwritten_content_in_recovery() {
    let fixture = Fixture::new();
    commit_crlf_file(&fixture);
    let edited = b"first\r\nsecond changed\r\nthird\r\n";
    fixture.write(PATH, edited);

    let session = fixture.session();
    let snapshot = session.refresh().unwrap();
    let entry = snapshot
        .entries
        .iter()
        .find(|entry| entry.path == PATH)
        .expect("应找到 CRLF 文件");
    let store = RecoveryStore::new(fixture.data.clone());
    let after = session
        .discard_file(&entry.path, &entry.fingerprint, &snapshot.revision.to_string(), &store)
        .expect("丢弃 CRLF 文件失败");

    assert!(after.entries.iter().all(|entry| entry.path != PATH));
    assert_eq!(fixture.read(PATH), COMMITTED);
    // 丢弃前先把完整工作区内容存入回收区（ADR-0010）。
    let records = store.list(None).unwrap();
    assert_eq!(records.len(), 1);
    assert!(records[0].paths.iter().any(|path| path == PATH));
    let inspected = store
        .inspect(&records[0].id, None, None)
        .expect("读取回收区记录失败");
    assert_eq!(inspected.files.len(), 1);
    assert_eq!(inspected.files[0].saved.bytes, edited.len() as u64);
    assert_eq!(
        inspected.files[0].saved.text.as_deref(),
        Some("first\r\nsecond changed\r\nthird\r\n")
    );
}

#[test]
fn edit_save_preserves_crlf_newlines() {
    let fixture = Fixture::new();
    commit_crlf_file(&fixture);
    let session = fixture.session();
    let editable = edit::open(&session, PATH).expect("打开编辑失败");
    assert_eq!(editable.newline, EditNewline::Crlf);
    let text = editable.text.clone().expect("文件应可编辑");
    let saved = edit::save(
        &session,
        &EditSaveRequest {
            worktree_id: session.id.clone(),
            worktree_identity: editable.worktree_identity.clone(),
            path: PATH.into(),
            base_sha256: editable.base_sha256.clone().unwrap(),
            text: text.replace("second", "second edited"),
        },
    )
    .expect("保存失败");
    assert_eq!(saved.newline, EditNewline::Crlf);
    assert_eq!(
        fixture.read(PATH),
        b"first\r\nsecond edited\r\nthird\r\n".to_vec()
    );
}
