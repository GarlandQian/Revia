//! Optional external diff/merge tools (M6 T).
//!
//! Revia never edits Git config.  A tool selected in Revia settings (or already
//! present as `diff.tool` / `merge.tool`) is passed to Git as fixed argv.  The
//! inherited worktree environment is used, no shell is involved, and filenames
//! are never expanded by Revia.
//!
//! External tools can write.  The command holds the session write lock, records
//! a recovery entry first, and after the process exits re-reads the conflict
//! state.  A zero exit status is not treated as "conflict resolved".

use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, ErrorKind, Result};
use crate::git::Git;
use crate::git::status;
use crate::recovery::{self, ProtectedRecovery, RecoverySource, RecoveryStore};
use crate::session::WorktreeSession;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ExternalToolKind {
    Diff,
    Merge,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExternalToolChoices {
    pub diff: Vec<String>,
    pub merge: Vec<String>,
    pub configured_diff: Option<String>,
    pub configured_merge: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExternalToolSetup {
    pub choices: ExternalToolChoices,
    pub context: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalToolRequest {
    pub expected_context: String,
    pub kind: ExternalToolKind,
    pub path: String,
    /// Selected tool; falls back to Revia settings / repository Git config.
    #[serde(default)]
    pub tool: Option<String>,
    /// Compare this commit's parent with the commit (diff only).
    #[serde(default)]
    pub revision: Option<String>,
    /// Which current change to diff (staged vs unstaged).  Required for a
    /// working-tree diff so the user sees the view they asked for instead of
    /// silently switching to the index.
    #[serde(default)]
    pub changes_kind: Option<status::ChangesKind>,
    #[serde(default)]
    pub expected_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExternalToolOutcome {
    pub tool: String,
    pub exit_code: Option<i32>,
    pub recovery_id: Option<String>,
    /// Re-read after the process exits; never inferred from exit status.
    pub still_conflicted: bool,
    pub path: String,
    /// Captured stdout/stderr (bounded).  GUI tools normally print little;
    /// terminal tools still report failures instead of only an exit code.
    pub stdout: String,
    pub stderr: String,
}

fn safe_tool_name(tool: &str) -> bool {
    !tool.is_empty()
        && tool.len() <= 128
        && tool
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b' '))
}

fn parse_tool_help(text: &str) -> Vec<String> {
    // `--tool-help` prints available tools before any section header, then a
    // "valid, but not currently available" section.  Newer Git versions may
    // add an explicit available section too.  Track both forms instead of
    // matching only one header.
    let mut in_tools = false;
    let mut tools = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            in_tools = false;
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if lower.contains("may be set to one of the following") {
            in_tools = true;
            continue;
        }
        if lower.starts_with("the following tools") {
            in_tools = lower.contains("available") && !lower.contains("not currently available");
            continue;
        }
        if !in_tools {
            continue;
        }
        // Tool entries are indented, while explanatory prose starts at the
        // margin.  Requiring leading whitespace avoids treating the trailing
        // "Some of the tools..." paragraph as a tool name.
        if !line.starts_with([' ', '\t']) {
            in_tools = false;
            continue;
        }
        let name = trimmed.split_whitespace().next().unwrap_or("");
        let name = name.trim_matches(|ch: char| {
            !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_' && ch != '.'
        });
        if !name.is_empty() && safe_tool_name(name) {
            tools.push(name.to_string());
        }
    }
    tools.sort();
    tools.dedup();
    tools
}

pub fn choices(
    git: &Git,
    configured_diff: Option<&str>,
    configured_merge: Option<&str>,
) -> ExternalToolChoices {
    let diff = git
        .run(&["difftool", "--tool-help"])
        .ok()
        .map(|output| parse_tool_help(&output.stdout_lossy()))
        .unwrap_or_default();
    let merge = git
        .run(&["mergetool", "--tool-help"])
        .ok()
        .map(|output| parse_tool_help(&output.stdout_lossy()))
        .unwrap_or_default();
    // Revia settings win; otherwise honor the repository's existing
    // `diff.tool` / `merge.tool` configuration without writing Git config.
    let configured_diff = configured_tool(git, "diff.tool").or_else(|| configured_diff.map(str::to_string));
    let configured_merge = configured_tool(git, "merge.tool").or_else(|| configured_merge.map(str::to_string));
    ExternalToolChoices {
        diff,
        merge,
        configured_diff,
        configured_merge,
    }
}

fn configured_tool(git: &Git, key: &str) -> Option<String> {
    let output = git.run(&["config", "--get", key]).ok()?;
    if output.exit_code != 0 {
        return None;
    }
    let value = output.stdout_lossy().trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn resolve_tool(
    git: &Git,
    kind: ExternalToolKind,
    requested: Option<&str>,
    configured: Option<&str>,
) -> Result<String> {
    let tool = requested
        .map(str::trim)
        .filter(|tool| !tool.is_empty())
        .map(str::to_string)
        .or_else(|| configured_tool(git, match kind { ExternalToolKind::Diff => "diff.tool", ExternalToolKind::Merge => "merge.tool" }))
        .or_else(|| configured.map(str::trim).filter(|tool| !tool.is_empty()).map(str::to_string))
        .ok_or_else(|| {
            AppError::coded(
                ErrorKind::Rejected,
                "errors.externalTools.noToolConfigured",
                "尚未配置外部工具，请先在设置中选择",
                &[],
            )
        })?;
    if !safe_tool_name(&tool) {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.externalTools.invalidToolName",
            "外部工具名称无效：{tool}",
            &[("tool", tool)],
        ));
    }
    Ok(tool)
}

fn validate_path(path: &str) -> Result<()> {
    let escapes = Path::new(path).is_absolute() || path.split(['/', '\\']).any(|part| part == "..");
    if path.is_empty() || path.contains('\0') || escapes {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.externalTools.invalidPath",
            "文件路径无效",
            &[],
        ));
    }
    Ok(())
}

fn validate_current_change(
    session: &WorktreeSession,
    path: &str,
    requested: Option<status::ChangesKind>,
) -> Result<status::ChangesKind> {
    validate_path(path)?;
    let state = status::read(&session.git)?;
    let Some(entry) = state.entries.iter().find(|entry| entry.path == path) else {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.externalTools.pathNotInCurrentChanges",
            "该文件不在当前改动中，请刷新后重试",
            &[],
        ));
    };
    if entry.changes == status::ChangesKind::Untracked {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.externalTools.untrackedHasNoDiff",
            "未跟踪文件没有可比较的 Git 版本",
            &[],
        ));
    }
    // Route explicitly by the requested view.  When both a staged and an
    // unstaged entry exist for the same path, guessing would hide one of them.
    let requested = requested.unwrap_or(entry.changes);
    if !state
        .entries
        .iter()
        .any(|entry| entry.path == path && entry.changes == requested)
    {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.externalTools.pathNotInCurrentChanges",
            "该文件不在当前改动中，请刷新后重试",
            &[],
        ));
    }
    Ok(requested)
}

fn validate_revision_path(session: &WorktreeSession, parent: &str, revision: &str, path: &str) -> Result<()> {
    validate_path(path)?;
    let output = session.git.run_ok(&["--literal-pathspecs", "diff-tree", "--no-commit-id", "--name-only", "--no-renames", "-r", "-z", parent, revision, "--", path], "读取外部差异路径")?;
    if !output.stdout.split(|byte| *byte == 0).any(|entry| entry == path.as_bytes()) {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.externalTools.pathNotInRevision",
            "该文件在所选提交中不存在，请刷新后重试",
            &[],
        ));
    }
    Ok(())
}

fn protect_external(
    session: &WorktreeSession,
    store: &RecoveryStore,
    path: &str,
    paths: &[String],
) -> Result<Option<ProtectedRecovery>> {
    crate::write_guard::protect(session, store, RecoverySource::ExternalTool, path, paths)
}

/// A mergetool may exit 0 after Git stages the file, so `git status` alone can
/// report a clean index while the conflict markers are still there.  Read a
/// bounded prefix and require a complete marker block before treating the file
/// as unresolved.  The caller still re-reads Git's unmerged state first; this
/// is a safety net, not a substitute for checking the worktree.
fn has_conflict_markers(path: &Path) -> bool {
    const MAX_SCAN: u64 = 8 * 1024 * 1024;
    let Ok(file) = File::open(path) else {
        return false;
    };
    let mut reader = BufReader::new(file.take(MAX_SCAN));
    let mut start = false;
    let mut middle = false;
    let mut line = String::new();
    loop {
        line.clear();
        let Ok(read) = reader.read_line(&mut line) else {
            return false;
        };
        if read == 0 {
            return false;
        }
        let value = line.trim_end_matches(['\r', '\n']);
        if value.starts_with("<<<<<<<") {
            start = true;
            middle = false;
        } else if start && value.starts_with("=======") {
            middle = true;
        } else if start && middle && value.starts_with(">>>>>>>") {
            return true;
        }
    }
}

pub fn launch(
    session: &WorktreeSession,
    recovery_store: &RecoveryStore,
    request: &ExternalToolRequest,
    configured_tool: Option<&str>,
) -> Result<ExternalToolOutcome> {
    launch_with_timeout(session, recovery_store, request, configured_tool, Duration::from_secs(30 * 60))
}

/// Existing real-Git regressions use a short deadline on the production path.
pub fn launch_with_timeout(
    session: &WorktreeSession, recovery_store: &RecoveryStore, request: &ExternalToolRequest,
    configured_tool: Option<&str>, timeout: Duration,
) -> Result<ExternalToolOutcome> {
    let _write = session.acquire_write_lock()?;
    require_context(session, &request.expected_context)?;
    let revision = request
        .revision
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        // Historical targets are commit identities, never revision options or
        // mutable ref expressions. Validate before any diff command can run.
        .map(|value| crate::git::history::read_commit(&session.git, value))
        .transpose()?;
    let parent = revision.as_ref().map(|commit| {
        let parent = commit.parents.first().ok_or_else(|| AppError::coded(
            ErrorKind::Rejected, "errors.externalTools.revisionHasNoParent",
            "该提交没有可比对的父提交", &[]))?;
        crate::git::history::verify_commit_oid(&session.git, parent)
    }).transpose()?;
    let revision = revision.as_ref().map(|commit| commit.oid.as_str());
    let changes_kind = if request.kind == ExternalToolKind::Diff && revision.is_none() {
        Some(validate_current_change(
            session,
            &request.path,
            request.changes_kind,
        )?)
    } else {
        None
    };
    if request.kind == ExternalToolKind::Diff {
        if let (Some(parent), Some(revision)) = (parent.as_deref(), revision) {
            validate_revision_path(session, parent, revision, &request.path)?;
        }
    } else {
        validate_current_change(session, &request.path, request.changes_kind)?;
        let state = status::read(&session.git)?;
        let entry = state
            .entries
            .iter()
            .find(|entry| entry.path == request.path)
            .ok_or_else(|| {
                AppError::coded(
                    ErrorKind::Rejected,
                    "errors.externalTools.pathNoLongerExists",
                    "该文件已不在当前改动中，请刷新后重试",
                    &[],
                )
            })?;
        if !entry.unmerged {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.externalTools.fileNotConflicted",
                "该文件当前不是冲突文件",
                &[],
            ));
        }
    }
    verify_current_target(session, request)?;
    let tool = resolve_tool(
        &session.git,
        request.kind,
        request.tool.as_deref(),
        configured_tool,
    )?;
    let mut protected_paths = Vec::new();
    if request.kind != ExternalToolKind::Diff || revision.is_none() {
        protected_paths.push(request.path.clone());
        if request.kind == ExternalToolKind::Merge {
            // Git's own backup can replace an existing user file even when the
            // external tool only writes MERGED. Include absence in the record.
            protected_paths.push(format!("{}.orig", request.path));
        }
    }
    let protected_state = crate::write_guard::fingerprint(&session.git, &protected_paths)?;
    let recovery_record = if protected_paths.is_empty() {
        // Historical diff has no current worktree content to snapshot; the
        // commit itself is the immutable source.
        None
    } else {
        protect_external(session, recovery_store, &request.path, &protected_paths)?
    };
    let recovery_id = recovery_record.as_ref().map(|record| record.id.clone());
    verify_current_target(session, request)?;
    session.require_identity()?;
    require_context(session, &request.expected_context)?;
    crate::write_guard::require(&session.git, &protected_paths, &protected_state)?;

    let mut args: Vec<String> = match request.kind {
        ExternalToolKind::Diff => vec![
            "--literal-pathspecs".into(),
            "difftool".into(),
            "--no-prompt".into(),
            format!("--tool={tool}"),
        ],
        ExternalToolKind::Merge => vec![
            "--literal-pathspecs".into(),
            // LOCAL/BASE/REMOTE/BACKUP names otherwise live beside the user's
            // file. Git allocates a fresh directory for these temporary files.
            "-c".into(),
            "mergetool.writeToTemp=true".into(),
            "mergetool".into(),
            "--no-prompt".into(),
            format!("--tool={tool}"),
        ],
    };
    if request.kind == ExternalToolKind::Diff {
        if changes_kind == Some(status::ChangesKind::Staged) {
            args.push("--cached".into());
        }
        if let (Some(parent), Some(revision)) = (parent, revision) {
            args.push(parent);
            args.push(revision.to_string());
        }
    }
    args.push("--".into());
    args.push(request.path.clone());
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();

    // Run through the controlled Git executor: same worktree environment,
    // command log entry, timeout recovery, and a hard cap on captured output.
    const MAX_TOOL_OUTPUT: usize = 1024 * 1024;
    let output = session
        .git
        .run_external_tool(&refs, timeout, MAX_TOOL_OUTPUT).map_err(recovery::pending_write)?;
    let stdout = output.stdout_lossy();
    let stderr = output.stderr.clone();
    if let Some(record) = &recovery_record {
        recovery_store.finish_snapshot(record)?;
    }

    if output.exit_code < 0 {
        return Err(recovery::pending_write(AppError::coded(ErrorKind::Git,
            "errors.mod.failedExitCode", "Git 命令失败（退出码 {0}）：{1}",
            &[("0", output.exit_code.to_string()), ("1", stderr.trim().to_string())])));
    }
    if output.timed_out {
        return Err(AppError::coded(ErrorKind::NeedsVerification, "errors.externalTools.timedOut", "外部工具已超时，文件可能已变化，请核对结果", &[]));
    }
    let after = status::read(&session.git).map_err(recovery::pending_write)?;
    let unmerged = after
        .entries
        .iter()
        .any(|entry| entry.path == request.path && entry.unmerged);
    let markers = request.kind == ExternalToolKind::Merge
        && !unmerged
        && has_conflict_markers(&session.path.join(&request.path));
    let still_conflicted = unmerged || markers;
    Ok(ExternalToolOutcome {
        tool,
        exit_code: Some(output.exit_code),
        recovery_id,
        still_conflicted,
        path: request.path.clone(),
        stdout,
        stderr,
    })
}

fn verify_current_target(session: &WorktreeSession, request: &ExternalToolRequest) -> Result<()> {
    if request.kind == ExternalToolKind::Diff && request.revision.as_deref().is_some_and(|value| !value.trim().is_empty()) { return Ok(()); }
    let current = if request.kind == ExternalToolKind::Merge {
        crate::merge::fingerprint(&session.git, &request.path)?
    } else {
        session.read_for_write()?.entries.iter().find(|entry| entry.path == request.path && Some(entry.changes) == request.changes_kind)
            .map(|entry| entry.fingerprint.clone()).unwrap_or_default()
    };
    if !current.is_empty() && request.expected_fingerprint.as_deref() == Some(current.as_str()) { return Ok(()); }
    Err(AppError::coded(ErrorKind::Rejected, "errors.externalTools.staleTarget", "文件版本已变化，请重新打开外部工具确认", &[]))
}

pub fn confirmation_context(session: &WorktreeSession) -> Result<String> {
    Ok(crate::write_guard::bind_confirmation(session, &crate::write_guard::context(&session.git)?))
}

fn require_context(session: &WorktreeSession, expected: &str) -> Result<()> {
    if confirmation_context(session)? != expected {
        return Err(crate::write_guard::stale());
    }
    Ok(())
}
