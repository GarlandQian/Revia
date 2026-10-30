//! Per-file history and blame (M6 F01–F03).
//!
//! History uses `git log --follow` so renames are followed.  Git's `--follow`
//! itself crosses a delete/recreate of the same path, so each fixed tip is
//! followed only as far as its current file's creation. Blame is
//! line-porcelain from Git; the UI never parses a human-formatted blame.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, ErrorKind, Result};
use crate::git::Git;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileHistoryEntry {
    pub oid: String,
    pub short_oid: String,
    pub author: String,
    pub author_time_ms: u64,
    pub subject: String,
    /// `M`, `A`, `D`, `R`, `C`, `T` ...
    pub status: String,
    /// Path at this commit (the new path for a rename).
    pub path: String,
    pub old_path: Option<String>,
    pub patch: String,
    /// The patch was cut at the 2 MiB safety cap.
    pub patch_truncated: bool,
    /// A read-only limitation or Git failure, never a lossy replacement patch.
    #[serde(default)]
    #[ts(optional)]
    pub patch_error: Option<crate::error::AppErrorPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileHistoryPage {
    pub entries: Vec<FileHistoryEntry>,
    pub has_more: bool,
    pub next_skip: u32,
    pub path: String,
    /// Commit the pagination is anchored to.  Follow-up pages pass this back
    /// so newly arriving commits cannot shift the window mid-scroll.
    pub anchor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHistoryRequest {
    pub path: String,
    #[serde(default)]
    pub all_branches: bool,
    #[serde(default)]
    pub skip: u32,
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// OID from the first page.  When present the query starts there instead
    /// of the current HEAD so the paging window is stable.
    #[serde(default)]
    pub anchor: Option<String>,
}

fn default_limit() -> u32 {
    40
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BlameLine {
    pub line: u32,
    pub oid: String,
    pub short_oid: String,
    pub author: String,
    pub author_time_ms: u64,
    pub summary: String,
    pub content: String,
    /// Git reports all-zero OIDs for uncommitted lines.
    pub uncommitted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BlameResult {
    pub path: String,
    pub revision: String,
    pub previous: Option<FileRevision>,
    pub lines: Vec<BlameLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlameRequest {
    pub path: String,
    /// Optional commit; empty/None means HEAD.
    #[serde(default)]
    pub revision: Option<String>,
    #[serde(default)]
    pub ignore_whitespace: bool,
    #[serde(default)]
    pub detect_moves: bool,
    #[serde(default)]
    pub detect_copies: bool,
}

fn parse_timestamp(value: Option<&str>) -> u64 {
    value
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0)
        .saturating_mul(1000)
}

fn short(oid: &str) -> String {
    oid.chars().take(8).collect()
}

pub fn history(git: &Git, request: &FileHistoryRequest) -> Result<FileHistoryPage> {
    if request.path.is_empty() || request.path.contains('\0') {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.fileHistory.invalidPath",
            "文件路径无效",
            &[],
        ));
    }
    let limit = request.limit.clamp(1, 200);
    let skip = request.skip;
    // Freeze every start commit once. Re-expanding --all on later pages
    // would introduce newly moved branch tips and shift the pagination.
    let mut starts: Vec<String> = if let Some(anchor) = request.anchor.as_deref() {
        let parsed: Vec<String> = serde_json::from_str(anchor).or_else(|_| {
            if matches!(anchor.len(), 40 | 64) && anchor.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                Ok(vec![anchor.to_string()])
            } else {
                Err(<serde_json::Error as serde::de::Error>::custom("invalid history anchor"))
            }
        }).map_err(|_| AppError::coded(ErrorKind::Rejected, "errors.fileHistory.invalidAnchor", "文件历史起点无效，请重新打开", &[]))?;
        if parsed.iter().any(|oid| !matches!(oid.len(), 40 | 64) || !oid.bytes().all(|byte| byte.is_ascii_hexdigit())) {
            return Err(AppError::coded(ErrorKind::Rejected, "errors.fileHistory.invalidAnchor", "文件历史起点无效，请重新打开", &[]));
        }
        parsed
    } else if request.all_branches {
        let output = git.run_ok(&["for-each-ref", "--format=%(objectname)", "refs/heads", "refs/remotes"], "读取文件历史全部分支起点")?;
        let mut branches: Vec<_> = output.stdout_lossy().lines().map(str::to_string).collect();
        // A detached current checkout remains an applicable start, while
        // refs/stash, tags and other purpose-specific refs are not branches.
        let head = git.run(&["rev-parse", "--verify", "--quiet", "HEAD"])?;
        if head.exit_code == 0 { branches.push(head.stdout_lossy().trim().to_string()); }
        branches
    } else {
        let output = git.run_ok(&["rev-parse", "--verify", "HEAD"], "读取文件历史起点")?;
        vec![output.stdout_lossy().trim().to_string()]
    };
    starts.sort();
    starts.dedup();
    let anchor = serde_json::to_string(&starts)?;
    if starts.is_empty() {
        return Ok(FileHistoryPage { entries: Vec::new(), has_more: false, next_skip: 0, path: request.path.clone(), anchor });
    }
    // Classify each tip's file life independently. A deletion on one branch
    // cannot truncate a surviving branch's ancestors. Read from the start on
    // later pages too, so a page boundary cannot hide the creation boundary.
    let max_count = format!(
        "--max-count={}",
        skip.saturating_add(limit).saturating_add(1)
    );
    let count = skip.saturating_add(limit).saturating_add(1) as usize;
    let mut streams = Vec::new();
    for start in &starts {
        streams.push(history_rows(git, &request.path, start, &max_count)?);
    }
    let mut rows = merge_history_prefix(streams, count);
    let total = rows.len();
    let start = (skip as usize).min(total);
    let end = start.saturating_add(limit as usize).min(total);
    let has_more = end < total;
    let mut page_entries: Vec<_> = rows.drain(start..end).map(|row| row.0).collect();
    for entry in &mut page_entries {
        let mut patch_args = vec![
            "-c", "core.quotePath=false", "--literal-pathspecs", "show", "--format=", "--no-color",
            "--no-ext-diff", "--no-textconv", "--src-prefix=a/", "--dst-prefix=b/", "--find-renames",
            &entry.oid, "--", &entry.path,
        ];
        if let Some(old) = &entry.old_path { patch_args.push(old); }
        let patch = (|| -> Result<()> {
            let output = git.run(&patch_args)?.into_success("读取文件历史差异")?;
            let (patch, truncated) = crate::git::readonly_patch(&output.stdout, 2 * 1024 * 1024)
                .ok_or_else(|| AppError::coded(ErrorKind::Rejected,
                    "errors.fileHistory.patchUnavailable", "该文件差异不是可无损显示的 UTF-8 文本，仅保留历史记录", &[]))?;
            entry.patch_truncated = truncated;
            entry.patch = patch;
            Ok(())
        })();
        if let Err(error) = patch { entry.patch_error = Some(error.payload()); }
    }
    Ok(FileHistoryPage {
        next_skip: skip.saturating_add(page_entries.len() as u32), has_more,
        path: request.path.clone(), anchor, entries: page_entries,
    })
}

/// Merge only the exposed heads of fixed Git history streams. Commit dates
/// need not be monotonic: sorting growing prefixes globally would move a late
/// ancestor ahead of an already returned page. This order preserves each
/// stream's Git prefix and is independent of the requested page size.
fn merge_history_prefix(streams: Vec<Vec<(FileHistoryEntry, u64)>>, count: usize) -> Vec<(FileHistoryEntry, u64)> {
    use std::cmp::Reverse;
    use std::collections::{BinaryHeap, HashSet};
    let mut streams: Vec<_> = streams.into_iter().map(|rows| rows.into_iter().peekable()).collect();
    let key = |index: usize, row: &(FileHistoryEntry, u64)| {
        (row.1, Reverse(row.0.oid.clone()), Reverse(row.0.path.clone()), Reverse(index))
    };
    let mut heads = BinaryHeap::new();
    for (index, stream) in streams.iter_mut().enumerate() {
        if let Some(row) = stream.peek() { heads.push(key(index, row)); }
    }
    let mut seen = HashSet::new();
    let mut rows = Vec::new();
    while rows.len() < count {
        let Some((_, _, _, Reverse(index))) = heads.pop() else { break; };
        let stream = &mut streams[index];
        if let Some(row) = stream.next() {
            if seen.insert((row.0.oid.clone(), row.0.path.clone())) { rows.push(row); }
        }
        if let Some(row) = stream.peek() { heads.push(key(index, row)); }
    }
    // A stream has no repeated (OID,path), so count rows per stream suffice
    // for the first count distinct union rows, even with shared ancestors.
    rows
}

fn history_rows(git: &Git, path: &str, start: &str, max_count: &str) -> Result<Vec<(FileHistoryEntry, u64)>> {
    let args = vec![
        "--literal-pathspecs",
        "log",
        "-z",
        "--follow",
        "--name-status",
        "--date=unix",
        "--format=%x1e%H%x00%h%x00%an%x00%at%x00%s%x00%ct",
        max_count, start, "--", path,
    ];
    let output = git.run(&args)?;
    if output.exit_code != 0 {
        return Err(AppError::coded(
            ErrorKind::Git,
            "errors.fileHistory.failedReadFileHistory",
            "读取文件历史失败：{detail}",
            &[("detail", output.stderr.trim().to_string())],
        ));
    }
    // These bytes also carry path credentials used by later Git reads. Never
    // turn an undecodable path into a different, valid UTF-8 filename.
    let text = std::str::from_utf8(&output.stdout).map_err(|_| non_utf8_path())?;
    let mut entries = Vec::new();
    let mut fields = text.split('\0').peekable();
    while let Some(header) = fields.next() {
        if header.is_empty() { continue; }
        // The marker is meaningful only where a commit header is expected.
        // Paths, authors and subjects may themselves contain U+001E. Consume
        // each NUL field by its role before looking for another header.
        let oid = header.trim_start_matches('\n').strip_prefix('\u{1e}')
            .filter(|oid| matches!(oid.len(), 40 | 64) && oid.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .ok_or_else(invalid_history_output)?.to_string();
        let short_oid = fields.next().ok_or_else(invalid_history_output)?.to_string();
        let author = fields.next().ok_or_else(invalid_history_output)?.to_string();
        let time = parse_timestamp(Some(fields.next().ok_or_else(invalid_history_output)?));
        let subject = fields.next().ok_or_else(invalid_history_output)?.to_string();
        let commit_time = parse_timestamp(Some(fields.next().ok_or_else(invalid_history_output)?));
        let mut path = path.to_string();
        let mut old_path = None;
        let mut file_status = "M".to_string();
        while let Some(field) = fields.peek() {
            let code = field.trim_start_matches('\n');
            if code.is_empty() { fields.next(); continue; }
            if code.starts_with('\u{1e}') { break; }
            let renamed = code.starts_with('R') || code.starts_with('C');
            if !(matches!(code, "A" | "M" | "D" | "T" | "U" | "X" | "B")
                || (renamed && code.len() > 1 && code[1..].bytes().all(|byte| byte.is_ascii_digit()))) {
                return Err(invalid_history_output());
            }
            file_status = code[..1].to_string();
            fields.next();
            let first = fields.next().filter(|path| !path.is_empty()).ok_or_else(invalid_history_output)?;
            if renamed {
                old_path = Some(first.to_string());
                path = fields.next().filter(|path| !path.is_empty()).ok_or_else(invalid_history_output)?.to_string();
            } else { path = first.to_string(); }
        }
        // An addition starts this file's life; a rename continues the old
        // path. Do not infer a global boundary from another branch's deletion.
        let creation = file_status == "A";
        entries.push((FileHistoryEntry {
            oid,
            short_oid,
            author,
            author_time_ms: time,
            subject,
            status: file_status,
            path,
            old_path,
            patch: String::new(),
            patch_truncated: false,
            patch_error: None,
        }, commit_time));
        if creation { break; }
    }
    Ok(entries)
}

fn invalid_history_output() -> AppError {
    AppError::coded(ErrorKind::Rejected, "errors.fileHistory.invalidHistoryOutput",
        "文件历史输出格式无法无损解析，请刷新后重试", &[])
}

fn blame_failure(stderr: &str) -> AppError {
    AppError::coded(ErrorKind::Git, "errors.fileHistory.failedReadBlame", "读取 blame 失败：{detail}",
        &[("detail", stderr.trim().to_string())])
}

fn non_utf8_path() -> AppError {
    AppError::coded(ErrorKind::Rejected, "errors.readonly.nonUtf8Path",
        "非 UTF-8 路径无法无损处理，仅供只读查看", &[])
}

pub fn blame(git: &Git, request: &BlameRequest) -> Result<BlameResult> {
    let revision = request
        .revision
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    // Pin the selected commit or the current branch anchor before reading.
    // The all-branches history browser is never a blame version credential.
    let resolved = revision.map(|value| {
        let output = git.run(&["rev-parse", "--verify", "--end-of-options", &format!("{value}^{{commit}}")])?;
        if output.exit_code != 0 { return Err(blame_failure(&output.stderr)); }
        Ok(output.stdout_lossy().trim().to_string())
    }).transpose()?;
    let revision = resolved.as_deref();
    let working_head = if revision.is_none() {
        Some(git.run_ok(&["rev-parse", "--verify", "HEAD"], "固定工作副本历史")?.stdout_lossy().trim().to_string())
    } else { None };
    let mut args = vec!["blame", "--line-porcelain"];
    if request.ignore_whitespace {
        args.push("-w");
    }
    if request.detect_moves {
        args.push("-M");
    }
    if request.detect_copies {
        args.push("-C");
    }
    if let Some(revision) = revision {
        args.push(revision);
    }
    args.extend_from_slice(&["--", &request.path]);
    let output = git.run(&args)?;
    if output.exit_code != 0 { return Err(blame_failure(&output.stderr)); }
    let text = std::str::from_utf8(&output.stdout).ok().filter(|text| !text.contains('\0'))
        .ok_or_else(|| AppError::coded(ErrorKind::Rejected,
            "errors.fileHistory.blameUnavailable", "该文件不是可无损显示的 UTF-8 文本，无法显示 blame", &[]))?;
    let mut lines = Vec::new();
    let mut current_oid = String::new();
    let mut current_line = 0u32;
    let mut author = String::new();
    let mut author_time_ms = 0u64;
    let mut summary = String::new();
    for line in text.lines() {
        if line.starts_with('\t') {
            let oid = current_oid.clone();
            let uncommitted = oid.chars().all(|value| value == '0');
            lines.push(BlameLine {
                line: current_line,
                short_oid: if uncommitted {
                    String::new()
                } else {
                    short(&oid)
                },
                oid,
                author: author.clone(),
                author_time_ms,
                summary: summary.clone(),
                content: line[1..].to_string(),
                uncommitted,
            });
            continue;
        }
        let mut fields = line.split(' ');
        let first = fields.next().unwrap_or_default();
        if first.len() >= 40 && first.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            current_oid = first.to_string();
            let _original_line = fields.next();
            current_line = fields
                .next()
                .and_then(|value| value.parse().ok())
                .unwrap_or(0);
            author.clear();
            author_time_ms = 0;
            summary.clear();
            continue;
        }
        if let Some(value) = line.strip_prefix("author ") {
            author = value.to_string();
        } else if let Some(value) = line.strip_prefix("author-time ") {
            author_time_ms = parse_timestamp(Some(value));
        } else if let Some(value) = line.strip_prefix("summary ") {
            summary = value.to_string();
        }
    }
    let previous = if let Some(oid) = &working_head {
        if git.run_ok(&["rev-parse", "--verify", "HEAD"], "核对工作副本历史")?.stdout_lossy().trim() != oid {
            return Err(crate::write_guard::stale());
        }
        let output = git.run_ok(&["--literal-pathspecs", "log", "-1", "--format=%H", oid, "--", &request.path], "读取当前分支文件基准")?;
        let oid = output.stdout_lossy().trim().to_string();
        (!oid.is_empty()).then(|| FileRevision { oid, path: request.path.clone() })
    } else {
        previous_revision(git, revision.unwrap(), &request.path)?
    };
    Ok(BlameResult {
        path: request.path.clone(),
        // Empty means the working copy was blamed directly.
        revision: revision.unwrap_or_default().to_string(),
        previous,
        lines,
    })
}

/// A concrete previous version.  `path` changes at a rename boundary, so the
/// caller must not keep blaming the new name at the old commit.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileRevision {
    pub oid: String,
    pub path: String,
}

/// Find the rename source in the first-parent diff that produced `oid`.
fn rename_source(git: &Git, parent: &str, oid: &str, path: &str) -> Result<Option<String>> {
    let output = git.run(&[
        "diff-tree",
        "--no-commit-id",
        "-r",
        "-M",
        "-z",
        "--name-status",
        "--no-ext-diff",
        parent,
        oid,
    ])?;
    if output.exit_code != 0 {
        return Ok(None);
    }
    let tokens: Vec<&[u8]> = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|token| !token.is_empty())
        .collect();
    let mut index = 0;
    while index < tokens.len() {
        let status = std::str::from_utf8(tokens[index]).map_err(|_| non_utf8_path())?;
        index += 1;
        if status.starts_with('R') || status.starts_with('C') {
            if index + 1 >= tokens.len() {
                break;
            }
            let old = std::str::from_utf8(tokens[index]).map_err(|_| non_utf8_path())?;
            let new = std::str::from_utf8(tokens[index + 1]).map_err(|_| non_utf8_path())?;
            index += 2;
            if new == path {
                return Ok(Some(old.to_string()));
            }
        } else {
            index += 1;
        }
    }
    Ok(None)
}

/// Previous revision of a file at a commit.  The file may have a different
/// path at the parent because history follows renames.  `None` means the file
/// did not exist at the first parent.
pub fn previous_revision(git: &Git, oid: &str, path: &str) -> Result<Option<FileRevision>> {
    let output = git.run(&["rev-parse", "--verify", &format!("{oid}^")])?;
    if output.exit_code != 0 {
        return Ok(None);
    }
    let parent = output.stdout_lossy().trim().to_string();
    let exists = git.run(&["cat-file", "-e", &format!("{parent}:{path}")])?;
    if exists.exit_code == 0 {
        return Ok(Some(FileRevision {
            oid: parent,
            path: path.to_string(),
        }));
    }
    if let Some(old_path) = rename_source(git, &parent, oid, path)? {
        let old_exists = git.run(&["cat-file", "-e", &format!("{parent}:{old_path}")])?;
        if old_exists.exit_code == 0 {
            return Ok(Some(FileRevision {
                oid: parent,
                path: old_path,
            }));
        }
    }
    Ok(None)
}
