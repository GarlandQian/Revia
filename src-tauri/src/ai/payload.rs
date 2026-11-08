//! AI 提交信息的发送内容（M9 规格 F 节，S5/S6/S7/P3）。
//!
//! 只读：暂存区模式用用户 index，更改列表模式在临时目录里建独立 index（ADR-0003，S7），
//! amend 相对原提交的父提交（根提交时相对空树）。diff 参数固定，不读用户的 diff 配置。
//! 敏感内容（二进制、lock 文件、子模块）根据结构化模式位和 numstat 判定，只写路径和状态；
//! 单文件与总量都截断；Git 调用不把 diff 正文写进命令日志（S6）。

use std::collections::BTreeMap;

use crate::error::Result;
use crate::git::Git;
use crate::m7::changelists::{ChangelistSelection, ChangelistStore};
use crate::session::WorktreeSession;

use super::{AiLanguage, MAX_FILE_CHARS, MAX_LISTED_FILES, MAX_TOTAL_CHARS, RECENT_LIMIT};

/// 只写路径、不写内容的 lock 文件（规格 F 节）。
const LOCK_FILES: &[&str] = &[
    "package-lock.json",
    "npm-shrinkwrap.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "bun.lock",
    "bun.lockb",
    "Cargo.lock",
    "Gemfile.lock",
    "composer.lock",
    "poetry.lock",
    "Pipfile.lock",
    "uv.lock",
    "go.sum",
    "packages.lock.json",
    "flake.lock",
    "Podfile.lock",
    "pubspec.lock",
    "mix.lock",
];

/// 下一次提交的来源。
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AiCommitMode {
    /// 已暂存改动。
    Staged,
    /// 更改列表里勾选的内容（支持 amend 勾选，S7）。
    Changelist {
        selection: ChangelistSelection,
        #[serde(default)]
        amend: bool,
    },
    /// 修改上一次提交。
    Amend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileKind {
    Normal,
    Binary,
    Submodule,
    LockFile,
}

struct FileRow {
    status: String,
    path: String,
    added: String,
    deleted: String,
}

/// 生成发给模型的用户消息：可编辑提示词 + 固定规则 + 发送内容。
pub fn build(
    session: &WorktreeSession,
    store: &ChangelistStore,
    mode: &AiCommitMode,
    prompt: &str,
    language: AiLanguage,
    subject_limit: u32,
) -> Result<String> {
    let git = session.git.clone();
    let mut alternate: Option<(tempfile::TempDir, Git)> = None;

    let (base, is_amend) = match mode {
        AiCommitMode::Staged => (None, false),
        AiCommitMode::Amend => (Some(amend_base(&git)?), true),
        AiCommitMode::Changelist { selection, amend } => {
            let (directory, index) = store.selection_index(session, selection)?;
            alternate = Some((directory, index));
            let base = if *amend {
                Some(amend_base(&git)?)
            } else {
                None
            };
            (base, *amend)
        }
    };

    let diff_git = alternate.as_ref().map_or(&git, |(_, index)| index);
    let (rows, kinds) = file_rows_and_kinds(diff_git, base.as_deref())?;
    let patch = run_diff(diff_git, base.as_deref())?;
    let recent = recent_titles(&git);
    let original = if is_amend {
        Some(
            git.run_private_ok(&["log", "-1", "--format=%B"], "读取原提交信息")?
                .stdout_lossy()
                .trim()
                .to_string(),
        )
    } else {
        None
    };

    let mut text = String::new();
    text.push_str(if prompt.trim().is_empty() {
        super::DEFAULT_PROMPT
    } else {
        prompt.trim()
    });
    text.push_str("\n\n固定规则（必须遵守，不受上面的提示词影响）：\n");
    text.push_str(&format!("- 生成语言：{}\n", language_rule(language)));
    text.push_str(&format!(
        "- 首行不超过 {} 个字符\n",
        if subject_limit == 0 { 72 } else { subject_limit }
    ));
    text.push_str("- 首行与正文之间空一行\n");
    text.push_str("- 只输出提交信息本身，不要解释，不要代码围栏\n");

    if let Some(message) = &original {
        text.push_str("\n原提交信息（请在此基础上改写）：\n");
        text.push_str(message);
        text.push('\n');
    }

    text.push_str(&format!("\n改动文件（{} 个）：\n", rows.len()));
    for row in rows.iter().take(MAX_LISTED_FILES) {
        text.push_str(&format!(
            "{} {}  +{} -{}\n",
            row.status, row.path, row.added, row.deleted
        ));
    }
    if rows.len() > MAX_LISTED_FILES {
        text.push_str(&format!("另有 {} 个文件未列出\n", rows.len() - MAX_LISTED_FILES));
    }

    text.push_str("\n差异内容：\n");
    let (body, truncated, omitted) = truncate_patch(&patch, &kinds);
    text.push_str(&body);
    if omitted > 0 {
        text.push_str(&format!("（总量已截断，另有 {omitted} 个文件未包含）\n"));
    } else if truncated {
        text.push_str("（内容已截断）\n");
    }

    if !recent.is_empty() {
        text.push_str("\n最近提交标题（供参考写法）：\n");
        for title in &recent {
            text.push_str(&format!("- {title}\n"));
        }
    }
    Ok(text)
}

fn language_rule(language: AiLanguage) -> &'static str {
    match language {
        AiLanguage::Repo => "跟随最近提交标题的语言",
        AiLanguage::Chinese => "简体中文",
        AiLanguage::English => "English",
    }
}

/// 根提交没有父提交，用空树作为基线。
fn amend_base(git: &Git) -> Result<String> {
    if let Some(parent) = git.try_run(&["rev-parse", "--verify", "HEAD^"]) {
        let parent = parent.trim();
        if !parent.is_empty() {
            return Ok(parent.to_string());
        }
    }
    let empty = git.run_with_input(&["mktree"], Vec::new())?;
    Ok(empty.stdout_lossy().trim().to_string())
}

/// 固定前缀与参数，关闭可能影响输出的用户 diff 配置（S5、S6）。
fn run_diff(git: &Git, base: Option<&str>) -> Result<String> {
    let mut args: Vec<&str> = vec![
        "-c",
        "core.quotePath=false",
        "-c",
        "diff.noprefix=false",
        "-c",
        "diff.mnemonicPrefix=false",
        "-c",
        "diff.suppressBlankEmpty=false",
        "diff",
        "--cached",
        "--src-prefix=a/",
        "--dst-prefix=b/",
        "--no-ext-diff",
        "--no-textconv",
        "--no-color",
    ];
    if let Some(base) = base {
        args.push(base);
    }
    Ok(git.run_private_ok(&args, "读取 AI diff")?.stdout_lossy())
}

/// 采用 --raw -z 与 --numstat -z 结构化获取文件清单与分类（S5：模式位精确判定，不搜正文）。
fn file_rows_and_kinds(git: &Git, base: Option<&str>) -> Result<(Vec<FileRow>, BTreeMap<String, FileKind>)> {
    let mut raw_args = vec![
        "-c",
        "core.quotePath=false",
        "-c",
        "diff.noprefix=false",
        "diff",
        "--cached",
        "--raw",
        "-z",
    ];
    if let Some(base) = base {
        raw_args.push(base);
    }
    let raw = git.run_private_ok(&raw_args, "读取改动属性")?.stdout;

    let mut numstat_args = vec![
        "-c",
        "core.quotePath=false",
        "-c",
        "diff.noprefix=false",
        "diff",
        "--cached",
        "--numstat",
        "-z",
    ];
    if let Some(base) = base {
        numstat_args.push(base);
    }
    let numstat = git.run_private_ok(&numstat_args, "读取改动行数")?.stdout;
    Ok(classify_rows(&raw, &numstat))
}

/// 由 `--raw -z` 与 `--numstat -z` 的原始字节得出文件清单与分类；路径可以不是合法 UTF-8。
fn classify_rows(raw: &[u8], numstat: &[u8]) -> (Vec<FileRow>, BTreeMap<String, FileKind>) {
    let counts = parse_numstat(numstat);

    let mut rows = Vec::new();
    let mut kinds = BTreeMap::new();
    let tokens: Vec<&[u8]> = raw.split(|byte| *byte == 0).collect();
    let mut index = 0;
    while index < tokens.len() {
        let meta = String::from_utf8_lossy(tokens[index]).into_owned();
        if !meta.starts_with(':') {
            index += 1;
            continue;
        }
        let parts: Vec<&str> = meta[1..].split_whitespace().collect();
        if parts.len() < 5 {
            index += 1;
            continue;
        }
        let old_mode = parts[0];
        let new_mode = parts[1];
        let status = parts[4];

        let path;
        if status.starts_with('R') || status.starts_with('C') {
            if index + 2 >= tokens.len() {
                break;
            }
            path = String::from_utf8_lossy(tokens[index + 2]).into_owned();
            index += 3;
        } else {
            if index + 1 >= tokens.len() {
                break;
            }
            path = String::from_utf8_lossy(tokens[index + 1]).into_owned();
            index += 2;
        }

        let is_submodule = old_mode == "160000" || new_mode == "160000";
        let (added, deleted) = counts
            .get(&path)
            .cloned()
            .unwrap_or_else(|| ("0".into(), "0".into()));
        let is_binary = (added == "-" && deleted == "-") && !is_submodule;
        let filename = path.rsplit('/').next().unwrap_or(&path);
        let is_lock = LOCK_FILES.contains(&filename);

        let kind = if is_submodule {
            FileKind::Submodule
        } else if is_binary {
            FileKind::Binary
        } else if is_lock {
            FileKind::LockFile
        } else {
            FileKind::Normal
        };

        kinds.insert(path.clone(), kind);
        rows.push(FileRow {
            status: status.chars().next().unwrap_or('?').to_string(),
            path,
            added,
            deleted,
        });
    }

    (rows, kinds)
}

fn parse_numstat(bytes: &[u8]) -> BTreeMap<String, (String, String)> {
    let mut counts = BTreeMap::new();
    let tokens: Vec<&[u8]> = bytes.split(|byte| *byte == 0).collect();
    let mut index = 0;
    while index < tokens.len() {
        let token = String::from_utf8_lossy(tokens[index]).into_owned();
        if token.is_empty() {
            index += 1;
            continue;
        }
        let mut parts = token.splitn(3, '\t');
        let added = parts.next().unwrap_or("-").to_string();
        let deleted = parts.next().unwrap_or("-").to_string();
        let rest = parts.next().unwrap_or("");
        if rest.is_empty() {
            if index + 2 < tokens.len() {
                let path = String::from_utf8_lossy(tokens[index + 2]).into_owned();
                counts.insert(path, (added, deleted));
            }
            index += 3;
        } else {
            counts.insert(rest.to_string(), (added, deleted));
            index += 1;
        }
    }
    counts
}

/// 最近提交标题：严格只取首行，不被标题内换行合并（P3）。
fn recent_titles(git: &Git) -> Vec<String> {
    // `-z` 让每条记录以 NUL 结束；`%B%x00` 会在每条后面多出换行，导致第 2 条起首行为空。
    let Ok(output) = git.run_private_ok(&["log", "-z", &format!("-{RECENT_LIMIT}"), "--format=%B"], "读取最近提交") else {
        return Vec::new();
    };
    output
        .stdout
        .split(|byte| *byte == 0)
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .filter_map(|msg| {
            let line = msg.trim_start_matches(['\r', '\n']).lines().next().unwrap_or("").trim().to_string();
            (!line.is_empty()).then_some(line)
        })
        .collect()
}

/// 把整段 patch 拆成文件块，根据结构化属性和长度裁剪。
fn truncate_patch(patch: &str, kinds: &BTreeMap<String, FileKind>) -> (String, bool, usize) {
    let mut blocks: Vec<&str> = Vec::new();
    let mut start = 0;
    let mut offset = 0;
    for line in patch.split_inclusive('\n') {
        if line.starts_with("diff --git ") && offset > start {
            blocks.push(&patch[start..offset]);
            start = offset;
        }
        offset += line.len();
    }
    if start < patch.len() {
        blocks.push(&patch[start..]);
    }

    let mut body = String::new();
    let mut truncated = false;
    let mut omitted = 0;
    let mut used = 0usize;
    for block in blocks {
        let path = block_path(block);
        let kind = kinds.get(&path).copied().unwrap_or_else(|| {
            let name = path.rsplit('/').next().unwrap_or(&path);
            if LOCK_FILES.contains(&name) {
                FileKind::LockFile
            } else {
                FileKind::Normal
            }
        });

        let content = match kind {
            FileKind::Binary => format!("{path}（二进制，内容省略）\n"),
            FileKind::Submodule => format!("{path}（子模块，内容省略）\n"),
            FileKind::LockFile => format!("{path}（lock 文件，内容省略）\n"),
            FileKind::Normal => block.to_string(),
        };

        let mut chars = content.chars().count();
        let mut text = content;
        if chars > MAX_FILE_CHARS {
            text = format!(
                "{}（本文件已截断）\n",
                text.chars().take(MAX_FILE_CHARS).collect::<String>()
            );
            chars = MAX_FILE_CHARS + 10;
            truncated = true;
        }
        let remaining = MAX_TOTAL_CHARS.saturating_sub(used);
        if remaining == 0 {
            omitted += 1;
            continue;
        }
        if chars > remaining {
            text = format!(
                "{}（内容已截断）\n",
                text.chars().take(remaining).collect::<String>()
            );
            truncated = true;
        }
        used += text.chars().count();
        body.push_str(&text);
    }
    (body, truncated, omitted)
}

/// 从 `+++ b/<path>` / `--- a/<path>` 或首行 `diff --git a/<path> b/<path>` 取相对路径。
fn block_path(block: &str) -> String {
    for line in block.lines() {
        for prefix in ["+++ b/", "--- a/"] {
            if let Some(path) = line.strip_prefix(prefix) {
                let p = path.trim();
                if p != "/dev/null" {
                    return p.to_string();
                }
            }
        }
    }
    block
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("diff --git a/"))
        .and_then(|rest| rest.split_once(" b/").map(|(_, right)| right.trim().to_string()))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::ShellEnv;
    use std::fs;
    use std::process::Command;

    fn test_repo() -> (tempfile::TempDir, std::sync::Arc<WorktreeSession>, ChangelistStore) {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        assert!(Command::new("git")
            .args(["init", "-q"])
            .current_dir(&repo)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(&repo)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["config", "user.email", "{{EMAIL_sfg94g6w}}"])
            .current_dir(&repo)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["config", "commit.gpgsign", "false"])
            .current_dir(&repo)
            .status()
            .unwrap()
            .success());

        let env = ShellEnv::minimal(&repo);
        let session = WorktreeSession::open(
            "wt-test",
            &repo,
            env,
            std::sync::Arc::new(crate::git::CommandLog::default()),
        )
        .unwrap();
        let store = ChangelistStore::new(&dir.path().join("changelists"));
        (dir, session, store)
    }

    #[test]
    fn staged_mode_includes_staged_changes_and_diff_not_in_log() {
        let (_dir, session, store) = test_repo();
        let repo = &session.path;
        fs::write(repo.join("file1.txt"), "hello secret content\n").unwrap();
        session.git.run_ok(&["add", "file1.txt"], "add").unwrap();

        let payload = build(
            &session,
            &store,
            &AiCommitMode::Staged,
            "",
            AiLanguage::Repo,
            72,
        )
        .unwrap();

        assert!(payload.contains("file1.txt"));
        assert!(payload.contains("+hello secret content"));
        assert!(payload.contains("固定规则"));

        // S6：断言 Git 命令日志里没有包含发送的 diff 正文
        let log_records = session.git.log.snapshot();
        for record in log_records {
            assert!(
                !record.output.contains("hello secret content"),
                "CommandLog leaked diff content: {}",
                record.output
            );
        }
    }

    #[test]
    fn changelist_mode_preserves_user_index_and_supports_amend() {
        let (_dir, session, store) = test_repo();
        let repo = &session.path;
        fs::write(repo.join("a.txt"), "a1\n").unwrap();
        fs::write(repo.join("b.txt"), "b1\n").unwrap();
        session.git.run_ok(&["add", "a.txt"], "add a").unwrap();
        session.git.run_ok(&["commit", "-m", "commit 1"], "commit").unwrap();

        fs::write(repo.join("a.txt"), "a2\n").unwrap();
        fs::write(repo.join("b.txt"), "b2\n").unwrap();
        session.git.run_ok(&["add", "a.txt"], "add a2").unwrap();

        let user_index_before = session
            .git
            .run_ok(&["status", "--porcelain"], "status")
            .unwrap()
            .stdout_lossy();

        let initial_view = store.view(&session).unwrap();
        let b_item = initial_view
            .items
            .iter()
            .find(|i| i.path == "b.txt")
            .unwrap();

        let mut sel = ChangelistSelection {
            identity: initial_view.identity,
            revision: initial_view.revision,
        };
        // 勾选 b
        let view_b = store
            .change(
                &session,
                &sel,
                crate::m7::changelists::ChangeAction::Select {
                    items: vec![b_item.id.clone()],
                    selected: true,
                },
            )
            .unwrap();
        sel.identity = view_b.identity;
        sel.revision = view_b.revision;

        // 1. 普通更改列表提交模式
        let payload_normal = build(
            &session,
            &store,
            &AiCommitMode::Changelist {
                selection: sel.clone(),
                amend: false,
            },
            "",
            AiLanguage::Chinese,
            72,
        )
        .unwrap();
        assert!(payload_normal.contains("b.txt"));
        assert!(!payload_normal.contains("a.txt"));

        // 2. S7：更改列表模式下勾选 amend，使用勾选内容相对父提交取 diff
        let payload_amend = build(
            &session,
            &store,
            &AiCommitMode::Changelist {
                selection: sel,
                amend: true,
            },
            "",
            AiLanguage::Chinese,
            72,
        )
        .unwrap();
        assert!(payload_amend.contains("b.txt"));
        assert!(payload_amend.contains("commit 1"));

        // 核对用户 index 前后一致
        let user_index_after = session
            .git
            .run_ok(&["status", "--porcelain"], "status")
            .unwrap()
            .stdout_lossy();
        assert_eq!(user_index_before, user_index_after);
    }

    #[test]
    fn special_files_binary_lock_submodule_rename_non_utf8() {
        let (_dir, session, store) = test_repo();
        let repo = &session.path;
        fs::write(repo.join("old.txt"), "old content with 160000 number\n").unwrap();
        session.git.run_ok(&["add", "old.txt"], "add").unwrap();
        session.git.run_ok(&["commit", "-m", "init"], "commit").unwrap();

        session.git.run_ok(&["mv", "old.txt", "new.txt"], "mv").unwrap();

        // lock 文件
        fs::write(
            repo.join("package-lock.json"),
            "{\n  \"lockfileVersion\": 3\n}\n",
        )
        .unwrap();
        session.git.run_ok(&["add", "package-lock.json"], "add").unwrap();

        // 二进制文件
        fs::write(repo.join("image.bin"), b"\x00\x01\x02\x03\xff").unwrap();
        session.git.run_ok(&["add", "image.bin"], "add").unwrap();

        // 普通文本文件含有 "160000"，测试 S5：不能因正文含 160000 误判为子模块！
        fs::write(repo.join("price.txt"), "price is 160000 dollars\n").unwrap();
        session.git.run_ok(&["add", "price.txt"], "add").unwrap();

        // 模拟真实子模块（创建子仓库并 git submodule add）
        let sub_dir = repo.join("sub_repo");
        fs::create_dir_all(&sub_dir).unwrap();
        Command::new("git").args(["init", "-q"]).current_dir(&sub_dir).status().unwrap();
        Command::new("git").args(["config", "user.name", "Sub"]).current_dir(&sub_dir).status().unwrap();
        Command::new("git").args(["config", "user.email", "{{EMAIL_sfg94g6w}}"]).current_dir(&sub_dir).status().unwrap();
        fs::write(sub_dir.join("sub.txt"), "sub\n").unwrap();
        Command::new("git").args(["add", "sub.txt"]).current_dir(&sub_dir).status().unwrap();
        Command::new("git").args(["commit", "-m", "sub init"]).current_dir(&sub_dir).status().unwrap();

        session.git.run_ok(&["-c", "protocol.file.allow=always", "submodule", "add", "./sub_repo", "mysub"], "submodule add").unwrap();

        let payload = build(
            &session,
            &store,
            &AiCommitMode::Staged,
            "",
            AiLanguage::Repo,
            72,
        )
        .unwrap();

        assert!(payload.contains("package-lock.json（lock 文件，内容省略）"));
        assert!(!payload.contains("lockfileVersion"));
        assert!(payload.contains("image.bin（二进制，内容省略）"));
        assert!(payload.contains("new.txt"));

        // S5：子模块精确识别，且普通文件包含 160000 不被误判
        assert!(payload.contains("mysub（子模块，内容省略）"));
        assert!(payload.contains("+price is 160000 dollars"));
        assert!(!payload.contains("price.txt（子模块"));
    }

    /// S13：更改列表只勾选同一文件的部分块时，只发送勾选的块，用户 index 不变。
    #[test]
    fn changelist_partial_hunk_selection_sends_only_selected_hunk() {
        let (_dir, session, store) = test_repo();
        let repo = &session.path;
        let original: String = (1..=60).map(|line| format!("line {line}\n")).collect();
        fs::write(repo.join("multi.txt"), &original).unwrap();
        session.git.run_ok(&["add", "multi.txt"], "add").unwrap();
        session.git.run_ok(&["commit", "-m", "base"], "commit").unwrap();
        let changed = original
            .replace("line 2\n", "line 2 FIRST-CHANGE\n")
            .replace("line 55\n", "line 55 SECOND-CHANGE\n");
        fs::write(repo.join("multi.txt"), changed).unwrap();
        let index_before = session.git.run_ok(&["write-tree"], "write-tree").unwrap().stdout_lossy();

        let view = store.view(&session).unwrap();
        let mut items: Vec<_> = view.items.iter().filter(|item| item.path == "multi.txt").collect();
        items.sort_by_key(|item| item.segment);
        assert_eq!(items.len(), 2, "two hunks become two selectable items");
        let selection = ChangelistSelection { identity: view.identity.clone(), revision: view.revision.clone() };
        let selected = store
            .change(
                &session,
                &selection,
                crate::m7::changelists::ChangeAction::Select { items: vec![items[0].id.clone()], selected: true },
            )
            .unwrap();
        let selection = ChangelistSelection { identity: selected.identity, revision: selected.revision };

        let payload = build(
            &session,
            &store,
            &AiCommitMode::Changelist { selection, amend: false },
            "",
            AiLanguage::Repo,
            72,
        )
        .unwrap();
        assert!(payload.contains("FIRST-CHANGE"));
        assert!(!payload.contains("SECOND-CHANGE"));
        let index_after = session.git.run_ok(&["write-tree"], "write-tree").unwrap().stdout_lossy();
        assert_eq!(index_before, index_after, "user index must stay untouched");
    }

    /// S13：Git 输出里的非 UTF-8 路径（任何平台都可能出现）照样分类与计数，不错位。
    #[test]
    fn non_utf8_paths_in_git_output_are_classified() {
        let raw = b":100644 100644 aaaaaaa bbbbbbb M\0caf\xe9.txt\0:100644 100644 ccccccc ddddddd M\0yarn.lock\0:000000 100644 0000000 eeeeeee A\0bin\xff.dat\0";
        let numstat = b"2\t1\tcaf\xe9.txt\x003\t0\tyarn.lock\0-\t-\tbin\xff.dat\0";
        let (rows, kinds) = classify_rows(raw, numstat);
        assert_eq!(rows.len(), 3);
        let latin = String::from_utf8_lossy(b"caf\xe9.txt").into_owned();
        assert_eq!(rows[0].path, latin);
        assert_eq!((rows[0].added.as_str(), rows[0].deleted.as_str()), ("2", "1"));
        assert!(matches!(kinds.get(&latin), Some(FileKind::Normal)));
        assert!(matches!(kinds.get("yarn.lock"), Some(FileKind::LockFile)));
        let binary = String::from_utf8_lossy(b"bin\xff.dat").into_owned();
        assert!(matches!(kinds.get(&binary), Some(FileKind::Binary)));
    }

    /// S13：非 UTF-8 文件名（Unix 上可以是任意字节）不会让整理失败，也按原样列出。
    #[cfg(unix)]
    #[test]
    fn non_utf8_file_name_is_listed() {
        use std::os::unix::ffi::OsStrExt;
        let (_dir, session, store) = test_repo();
        let repo = &session.path;
        let name = std::ffi::OsStr::from_bytes(b"caf\xe9-latin1.txt");
        if fs::write(repo.join(name), "latin1 name\n").is_err() {
            // APFS 等文件系统拒绝非 UTF-8 名称：无从构造，明确记录跳过而不是冒充通过。
            eprintln!("SKIPPED non_utf8_file_name_is_listed: filesystem rejects non-UTF-8 names");
            return;
        }
        session.git.run_ok(&["add", "-A"], "add").unwrap();
        let payload = build(&session, &store, &AiCommitMode::Staged, "", AiLanguage::Repo, 72).unwrap();
        assert!(payload.contains("latin1.txt"), "file list keeps the entry");
        assert!(payload.contains("+latin1 name"));
    }

    #[test]
    fn truncation_and_listed_files_limit_and_recent_subject() {
        let (_dir, session, store) = test_repo();
        let repo = &session.path;
        // P3：多行提交标题只取首行
        fs::write(repo.join("c.txt"), "c\n").unwrap();
        session.git.run_ok(&["add", "c.txt"], "add").unwrap();
        session.git.run_ok(&["commit", "-m", "first line\n\nsecond body line"], "commit").unwrap();
        // 多条提交都要取到标题（曾因记录分隔多出换行，只剩 1 条）
        fs::write(repo.join("d.txt"), "d\n").unwrap();
        session.git.run_ok(&["add", "d.txt"], "add").unwrap();
        session.git.run_ok(&["commit", "-m", "newer title\n\nnewer body"], "commit").unwrap();

        // 超过 300 个文件测试清单上限
        for i in 0..305 {
            fs::write(repo.join(format!("dummy_{i}.txt")), "x\n").unwrap();
        }
        session.git.run_ok(&["add", "-A"], "add dummies").unwrap();

        let payload = build(
            &session,
            &store,
            &AiCommitMode::Staged,
            "",
            AiLanguage::Repo,
            72,
        )
        .unwrap();

        assert!(payload.contains("另有 5 个文件未列出"));
        assert!(payload.contains("- first line"));
        assert!(payload.contains("- newer title"));
        assert!(!payload.contains("second body line"));
        assert!(!payload.contains("newer body"));

        // 截断测试
        let long_file = format!(
            "diff --git a/long.txt b/long.txt\n--- a/long.txt\n+++ b/long.txt\n@@ -1,1 +1,1 @@\n+{}\n",
            "a".repeat(7000)
        );
        let mut kinds = BTreeMap::new();
        kinds.insert("long.txt".to_string(), FileKind::Normal);
        let (body, truncated, _) = truncate_patch(&long_file, &kinds);
        assert!(truncated);
        assert!(body.contains("（本文件已截断）"));
    }
}
