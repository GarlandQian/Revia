//! 解析 `git status --porcelain=v2 -z`（机器可读格式，Q7/技术方向）。
//!
//! 只读取状态，不修改仓库。忽略文件（`!`）不进入结果（Q30 只显示未跟踪文件）。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::Result;
use crate::git::Git;

/// 改动所属的分组。术语见 GLOSSARY：未暂存改动 / 已暂存改动 / 未跟踪文件。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ChangesKind {
    Unstaged,
    Staged,
    Untracked,
}

/// 文件在某一组里的状态，对应界面上的单字母（Q31）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum FileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    /// 未跟踪文件（只出现在未跟踪分组）。
    Untracked,
    /// 冲突：M1 只显示状态，不显示 diff。
    Conflict,
    TypeChanged,
}

/// 只读限制原因（Q81、Q82、规格第 2 节）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ReadonlyReason {
    Symlink,
    Submodule,
    NestedRepo,
    Conflict,
    NonUtf8Path,
    Unreadable,
}

impl ReadonlyReason {
    pub fn description(&self) -> &'static str {
        match self {
            Self::Symlink => "符号链接为只读类型，不支持在应用内暂存或取消暂存",
            Self::Submodule => "子模块为只读类型，不支持在应用内暂存或取消暂存",
            Self::NestedRepo => "嵌套仓库为只读类型，不支持在应用内暂存",
            Self::Conflict => "文件存在未解决的冲突，不支持通过普通操作暂存",
            Self::NonUtf8Path => "非 UTF-8 路径无法无损处理，仅供只读查看",
            Self::Unreadable => "文件无法读取或无访问权限，仅供只读查看",
        }
    }
}

impl FileStatus {
    pub fn letter(self) -> &'static str {
        match self {
            FileStatus::Modified => "M",
            FileStatus::Added => "A",
            FileStatus::Deleted => "D",
            FileStatus::Renamed => "R",
            FileStatus::Untracked => "?",
            FileStatus::Conflict => "U",
            FileStatus::TypeChanged => "T",
        }
    }
}

/// 仓库当前分支与每个文件的状态。
#[derive(Debug, Clone, Default, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WorktreeStatus {
    pub branch: Option<String>,
    pub detached: bool,
    pub entries: Vec<StatusEntry>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StatusEntry {
    pub path: String,
    /// 重命名的原路径。
    pub original_path: Option<String>,
    pub changes: ChangesKind,
    pub status: FileStatus,
    /// 是否处于 unmerged（冲突）状态。
    pub unmerged: bool,
    /// 只读限制原因，None 表示支持写操作。
    pub readonly_reason: Option<ReadonlyReason>,
    #[serde(skip)]
    pub mode_head: Option<String>,
    #[serde(skip)]
    pub mode_index: Option<String>,
    #[serde(skip)]
    pub mode_worktree: Option<String>,
    #[serde(skip)]
    pub oid_head: Option<String>,
    #[serde(skip)]
    pub oid_index: Option<String>,
}

/// 读取并解析状态。
pub fn read(git: &Git) -> Result<WorktreeStatus> {
    let output = git.run_ok(
        &[
            "-c",
            "core.quotePath=false",
            "status",
            "--porcelain=v2",
            "-z",
            "--untracked-files=all",
            "--branch",
        ],
        "读取仓库状态",
    )?;
    Ok(parse(&output.stdout))
}

/// 解析 porcelain v2 的 NUL 分隔输出。
pub fn parse(bytes: &[u8]) -> WorktreeStatus {
    let mut status = WorktreeStatus::default();
    let mut fields = bytes.split(|byte| *byte == 0).peekable();

    while let Some(field) = fields.next() {
        if field.is_empty() {
            continue;
        }
        let text = String::from_utf8_lossy(field).into_owned();

        if let Some(rest) = text.strip_prefix("# branch.head ") {
            let name = rest.trim().to_string();
            if name == "(detached)" {
                status.detached = true;
            } else {
                status.branch = Some(name);
            }
            continue;
        }
        if text.starts_with("# ") {
            continue;
        }

        let kind = text.as_bytes()[0];
        match kind {
            b'1' | b'2' => {
                // 1 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>
                // 2 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <X><score> <path>
                let head_fields = if kind == b'1' { 8 } else { 9 };
                let (head, path) = split_head(&text, head_fields);
                let xy = head.get(1).copied().unwrap_or("..");
                let x = normalize(xy.as_bytes().first().copied().unwrap_or(b'.') as char);
                let y = normalize(xy.as_bytes().get(1).copied().unwrap_or(b'.') as char);
                let original_path = if kind == b'2' {
                    fields
                        .next()
                        .map(|value| String::from_utf8_lossy(value).into_owned())
                } else {
                    None
                };

                let is_symlink = head.get(3).is_some_and(|m| *m == "120000")
                    || head.get(4).is_some_and(|m| *m == "120000")
                    || head.get(5).is_some_and(|m| *m == "120000");
                let is_submodule = head.get(2).is_some_and(|s| s.starts_with('S'))
                    || head.get(3).is_some_and(|m| *m == "160000")
                    || head.get(4).is_some_and(|m| *m == "160000")
                    || head.get(5).is_some_and(|m| *m == "160000");
                let has_non_utf8 = path.contains('\u{FFFD}')
                    || original_path.as_deref().is_some_and(|orig| orig.contains('\u{FFFD}'));
                let readonly_reason = if is_symlink {
                    Some(ReadonlyReason::Symlink)
                } else if is_submodule {
                    Some(ReadonlyReason::Submodule)
                } else if has_non_utf8 {
                    Some(ReadonlyReason::NonUtf8Path)
                } else {
                    None
                };

                let mode_head = head.get(3).map(|s| s.to_string());
                let mode_index = head.get(4).map(|s| s.to_string());
                let mode_worktree = head.get(5).map(|s| s.to_string());
                let oid_head = head.get(6).map(|s| s.to_string());
                let oid_index = head.get(7).map(|s| s.to_string());

                if x != '.' {
                    status.entries.push(StatusEntry {
                        path: path.to_string(),
                        original_path: original_path.clone(),
                        changes: ChangesKind::Staged,
                        status: status_from_letter(x),
                        unmerged: false,
                        readonly_reason: readonly_reason.clone(),
                        mode_head: mode_head.clone(),
                        mode_index: mode_index.clone(),
                        mode_worktree: mode_worktree.clone(),
                        oid_head: oid_head.clone(),
                        oid_index: oid_index.clone(),
                    });
                }
                if y != '.' {
                    status.entries.push(StatusEntry {
                        path: path.to_string(),
                        original_path,
                        changes: ChangesKind::Unstaged,
                        status: status_from_letter(y),
                        unmerged: false,
                        readonly_reason,
                        mode_head,
                        mode_index,
                        mode_worktree,
                        oid_head,
                        oid_index,
                    });
                }
            }
            b'u' => {
                // u <XY> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>
                let (_head, path) = split_head(&text, 10);
                status.entries.push(StatusEntry {
                    path: path.to_string(),
                    original_path: None,
                    changes: ChangesKind::Unstaged,
                    status: FileStatus::Conflict,
                    unmerged: true,
                    readonly_reason: Some(ReadonlyReason::Conflict),
                    mode_head: None,
                    mode_index: None,
                    mode_worktree: None,
                    oid_head: None,
                    oid_index: None,
                });
            }
            b'?' => {
                let path = text[2..].to_string();
                let is_non_utf8 = path.contains('\u{FFFD}');
                let is_nested_repo = path.ends_with('/') || path.ends_with("/.git");
                let readonly_reason = if is_non_utf8 {
                    Some(ReadonlyReason::NonUtf8Path)
                } else if is_nested_repo {
                    Some(ReadonlyReason::NestedRepo)
                } else {
                    None
                };
                status.entries.push(StatusEntry {
                    path,
                    original_path: None,
                    changes: ChangesKind::Untracked,
                    status: FileStatus::Untracked,
                    unmerged: false,
                    readonly_reason,
                    mode_head: None,
                    mode_index: None,
                    mode_worktree: None,
                    oid_head: None,
                    oid_index: None,
                });
            }
            // `!` 被忽略的文件：M1 不显示（Q30）
            _ => {}
        }
    }

    status
}

/// 把一条记录切成前 `head_fields` 个字段和剩余部分（剩余部分是路径，可能含空格）。
fn split_head(text: &str, head_fields: usize) -> (Vec<&str>, &str) {
    let mut parts = text.splitn(head_fields + 1, ' ');
    let head: Vec<&str> = (0..head_fields).filter_map(|_| parts.next()).collect();
    (head, parts.next().unwrap_or(""))
}

fn normalize(letter: char) -> char {
    match letter {
        'A' | 'M' | 'D' | 'R' | 'C' | 'T' | '.' => letter,
        'U' | 'u' => 'U',
        _ => 'M',
    }
}

fn status_from_letter(letter: char) -> FileStatus {
    match letter {
        'A' => FileStatus::Added,
        'D' => FileStatus::Deleted,
        'R' | 'C' => FileStatus::Renamed,
        'T' => FileStatus::TypeChanged,
        'U' => FileStatus::Conflict,
        _ => FileStatus::Modified,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nul(records: &[&str]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for record in records {
            bytes.extend_from_slice(record.as_bytes());
            bytes.push(0);
        }
        bytes
    }

    #[test]
    fn parses_branch_and_modified_file() {
        let bytes = nul(&["# branch.oid abc", "# branch.head main", "1 .M N... 100644 100644 100644 aaa bbb src/app.ts"]);
        let status = parse(&bytes);
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert!(!status.detached);
        assert_eq!(status.entries.len(), 1);
        assert_eq!(status.entries[0].path, "src/app.ts");
        assert_eq!(status.entries[0].changes, ChangesKind::Unstaged);
        assert_eq!(status.entries[0].status, FileStatus::Modified);
    }

    #[test]
    fn parses_staged_and_unstaged_for_same_file() {
        let bytes = nul(&["1 MM N... 100644 100644 100644 aaa bbb a.txt"]);
        let status = parse(&bytes);
        assert_eq!(status.entries.len(), 2, "同一文件应同时出现在两组");
        assert_eq!(status.entries[0].changes, ChangesKind::Staged);
        assert_eq!(status.entries[1].changes, ChangesKind::Unstaged);
        assert_eq!(status.entries[0].path, "a.txt");
    }

    #[test]
    fn parses_rename_with_original_path() {
        let bytes = nul(&["2 R. N... 100644 100644 100644 aaa bbb R100 new.ts", "old.ts"]);
        let status = parse(&bytes);
        assert_eq!(status.entries.len(), 1);
        assert_eq!(status.entries[0].status, FileStatus::Renamed);
        assert_eq!(status.entries[0].path, "new.ts");
        assert_eq!(status.entries[0].original_path.as_deref(), Some("old.ts"));
    }

    #[test]
    fn parses_untracked_and_ignores_ignored() {
        let bytes = nul(&["? 新文件.md", "! node_modules/"]);
        let status = parse(&bytes);
        assert_eq!(status.entries.len(), 1);
        assert_eq!(status.entries[0].status, FileStatus::Untracked);
        assert_eq!(status.entries[0].changes, ChangesKind::Untracked);
    }

    #[test]
    fn parses_conflict_and_detached_head() {
        let bytes = nul(&[
            "# branch.head (detached)",
            "u UU N... 100644 100644 100644 100644 aaa bbb ccc README.md",
        ]);
        let status = parse(&bytes);
        assert!(status.detached);
        assert_eq!(status.entries[0].status, FileStatus::Conflict);
        assert!(status.entries[0].unmerged);
    }

    #[test]
    fn status_letters_match_glossary() {
        assert_eq!(FileStatus::Modified.letter(), "M");
        assert_eq!(FileStatus::Untracked.letter(), "?");
        assert_eq!(FileStatus::Conflict.letter(), "U");
    }
}
