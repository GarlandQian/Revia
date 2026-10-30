//! 诊断只投影固定字段；原始输出、用户参数、路径与账号标识不进入持久化或导出。
//! 命令完成时保存七天摘要；预览在 Rust 保存不可由前端改写的快照，导出只使用该快照。

use super::now_ms;
use crate::error::{AppError, ErrorKind, Result};
use crate::git::{CommandLog, CommandRecord};
use crate::session::WorktreeSession;
use crate::settings::Settings;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use ts_rs::TS;

pub const SUMMARY_RETENTION_MS: u64 = 7 * 24 * 60 * 60 * 1000;
const SUMMARY_MAX_ENTRIES: usize = 500;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsRequest {
    pub destination: String,
    pub worktree_id: Option<String>,
    #[serde(default)]
    pub snapshot_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DiagnosticsExport {
    pub path: String,
    pub bytes: u64,
    pub generated_at_ms: u64,
    pub redactions: u64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DiagnosticsPreview {
    pub text: String,
    pub redactions: u64,
    pub snapshot_id: String,
}

/// 只有数值和固定词表；即使读取旧版摘要，也重新投影而不采用旧 summary/program。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DiagnosticsSummaryEntry {
    pub sequence: u64,
    pub started_at_ms: u64,
    pub worktree_id: Option<String>,
    pub program: String,
    pub exit_code: i32,
    pub timed_out: bool,
    pub error_class: String,
    pub summary: String,
    #[serde(default)]
    pub elapsed_ms: u64,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone)]
struct Snapshot {
    id: String,
    worktree_id: Option<String>,
    text: String,
    redactions: u64,
    generated_at_ms: u64,
}

#[derive(Debug, Default)]
struct StoreState {
    // 不让摘要故障中断已经完成的 Git；在下次诊断操作显式报告，直到用户清空。
    failed: bool,
    next_snapshot: u64,
    snapshots: VecDeque<Snapshot>,
}

#[derive(Debug)]
pub struct DiagnosticsSummaryStore {
    path: PathBuf,
    state: Mutex<StoreState>,
}

fn store_error() -> AppError {
    AppError::coded(
        ErrorKind::Internal,
        "errors.m7.diagnosticsStoreFailed",
        "诊断摘要读写失败；请清空摘要后重试",
        &[],
    )
}

fn snapshot_error() -> AppError {
    AppError::coded(
        ErrorKind::Rejected,
        "errors.m7.diagnosticsSnapshotExpired",
        "诊断预览已失效，请刷新后再导出",
        &[],
    )
}

fn in_scope(requested: Option<&str>, owner: Option<&str>) -> bool {
    requested == owner
}

impl DiagnosticsSummaryStore {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join("diagnostics-summary.json"),
            state: Mutex::new(StoreState::default()),
        }
    }

    fn load(&self) -> Result<Vec<DiagnosticsSummaryEntry>> {
        let bytes = match std::fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(store_error().cause(error.into())),
        };
        let mut entries: Vec<DiagnosticsSummaryEntry> =
            serde_json::from_slice(&bytes).map_err(|_| store_error())?;
        let cutoff = now_ms().saturating_sub(SUMMARY_RETENTION_MS);
        entries.retain(|entry| entry.started_at_ms >= cutoff);
        for entry in &mut entries {
            entry.program = "git".into();
            entry.error_class = classify(entry.exit_code, entry.timed_out).into();
            entry.summary = entry.error_class.clone();
            entry.args = safe_args(&entry.args);
        }
        if entries.len() > SUMMARY_MAX_ENTRIES {
            entries.drain(..entries.len() - SUMMARY_MAX_ENTRIES);
        }
        // 读取时也清理过期项、迁移旧摘要，避免旧正文或过期数据继续留在磁盘。
        let sanitized = serde_json::to_vec_pretty(&entries)?;
        if sanitized != bytes {
            super::atomic_write(&self.path, &sanitized)
                .map_err(|error| store_error().cause(error))?;
        }
        Ok(entries)
    }

    /// 由统一命令日志完成入口调用，持有日志锁，和清空操作保持同一顺序。
    pub(crate) fn observe(&self, record: &CommandRecord) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let result = (|| -> Result<()> {
            let mut entries = self.load()?;
            entries.push(summary(record));
            if entries.len() > SUMMARY_MAX_ENTRIES {
                entries.remove(0);
            }
            super::atomic_write(&self.path, &serde_json::to_vec_pretty(&entries)?)
        })();
        if result.is_err() {
            state.failed = true;
        }
    }

    pub fn list(&self, worktree_id: Option<&str>) -> Result<Vec<DiagnosticsSummaryEntry>> {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.failed {
            return Err(store_error());
        }
        Ok(self
            .load()?
            .into_iter()
            .filter(|entry| in_scope(worktree_id, entry.worktree_id.as_deref()))
            .collect())
    }

    pub(crate) fn clear(&self, worktree_id: Option<&str>) -> Result<usize> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        // 无法解析旧摘要时允许显式清空；不猜测损坏文件里的归属。
        let loaded = self.load();
        let corrupt = loaded.is_err();
        let mut entries = loaded.unwrap_or_default();
        let before = entries.len();
        entries.retain(|entry| !in_scope(worktree_id, entry.worktree_id.as_deref()));
        super::atomic_write(&self.path, &serde_json::to_vec_pretty(&entries)?)?;
        state.failed = false;
        state
            .snapshots
            .retain(|snapshot| !corrupt && !in_scope(worktree_id, snapshot.worktree_id.as_deref()));
        Ok(before - entries.len())
    }
}

fn classify(exit_code: i32, timed_out: bool) -> &'static str {
    match (timed_out, exit_code) {
        (true, _) | (_, 124) => "timeout",
        (_, 0) => "ok",
        (_, 130) => "cancelled",
        _ => "git-failure",
    }
}

// 只留已知的子命令/无值选项。分支、路径、URL、邮件、-c 值、正文等全部省略。
fn safe_args(args: &[String]) -> Vec<String> {
    args.iter()
        .filter(|arg| {
            matches!(
                arg.as_str(),
                "status"
                    | "diff"
                    | "log"
                    | "show"
                    | "add"
                    | "reset"
                    | "restore"
                    | "commit"
                    | "fetch"
                    | "pull"
                    | "push"
                    | "clone"
                    | "checkout"
                    | "switch"
                    | "branch"
                    | "merge"
                    | "rebase"
                    | "cherry-pick"
                    | "revert"
                    | "stash"
                    | "rev-parse"
                    | "rev-list"
                    | "ls-files"
                    | "ls-tree"
                    | "cat-file"
                    | "worktree"
                    | "config"
                    | "remote"
                    | "for-each-ref"
                    | "version"
                    | "--version"
                    | "--cached"
                    | "--staged"
                    | "--stat"
                    | "--numstat"
                    | "--name-only"
                    | "--name-status"
                    | "--no-color"
                    | "--no-ext-diff"
                    | "--no-textconv"
                    | "--porcelain=v2"
                    | "--progress"
                    | "--recurse-submodules"
                    | "--abort"
                    | "--continue"
                    | "--skip"
            )
        })
        .take(32)
        .cloned()
        .collect()
}

fn summary(record: &CommandRecord) -> DiagnosticsSummaryEntry {
    let class = classify(record.exit_code, record.timed_out).to_string();
    DiagnosticsSummaryEntry {
        sequence: record.sequence,
        started_at_ms: record.started_at_ms,
        worktree_id: record.worktree_id.clone(),
        program: "git".into(),
        args: safe_args(&record.args),
        exit_code: record.exit_code,
        timed_out: record.timed_out,
        elapsed_ms: record.elapsed_ms,
        error_class: class.clone(),
        summary: class,
    }
}

/// 不序列化摘要中的内部 worktree 标识。
fn public_entry(entry: &DiagnosticsSummaryEntry) -> serde_json::Value {
    serde_json::json!({
        "sequence": entry.sequence, "startedAtMs": entry.started_at_ms,
        "program": "git", "args": safe_args(&entry.args), "exitCode": entry.exit_code,
        "timedOut": entry.timed_out, "elapsedMs": entry.elapsed_ms,
        "errorClass": classify(entry.exit_code, entry.timed_out),
    })
}

pub fn preview(
    request: &DiagnosticsRequest,
    settings: &Settings,
    command_log: &CommandLog,
    store: &DiagnosticsSummaryStore,
    session: Option<&WorktreeSession>,
) -> Result<DiagnosticsPreview> {
    command_log.with_diagnostics_snapshot(|records| {
        preview_records(request, settings, records, store, session)
    })
}

fn preview_records(
    request: &DiagnosticsRequest,
    settings: &Settings,
    records: &[CommandRecord],
    store: &DiagnosticsSummaryStore,
    session: Option<&WorktreeSession>,
) -> Result<DiagnosticsPreview> {
    let records: Vec<_> = records
        .iter()
        .filter(|record| {
            in_scope(
                request.worktree_id.as_deref(),
                record.worktree_id.as_deref(),
            )
        })
        .collect();
    let mut state = store
        .state
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if state.failed {
        return Err(store_error());
    }
    let entries: Vec<_> = store
        .load()?
        .into_iter()
        .filter(|entry| in_scope(request.worktree_id.as_deref(), entry.worktree_id.as_deref()))
        .collect();
    let mut combined = entries.clone();
    for record in &records {
        let entry = summary(record);
        // sequence 只在进程内单调，跨重启还需比较时间和所属目标。
        if !combined.iter().any(|old| {
            old.sequence == entry.sequence
                && old.started_at_ms == entry.started_at_ms
                && old.worktree_id == entry.worktree_id
        }) {
            combined.push(entry);
        }
    }
    combined.sort_by_key(|entry| entry.started_at_ms);
    let generated_at_ms = now_ms();
    let redactions = records
        .iter()
        .map(|record| {
            record
                .args
                .len()
                .saturating_sub(safe_args(&record.args).len()) as u64
                + u64::from(!record.output.is_empty() || !record.stderr_excerpt.is_empty())
                + u64::from(!record.directory.is_empty())
        })
        .sum();
    let mut slowest = combined.clone();
    slowest.sort_by_key(|entry| std::cmp::Reverse(entry.elapsed_ms));
    let manifest = serde_json::json!({
        "generatedAtMs": generated_at_ms, "appVersion": env!("CARGO_PKG_VERSION"),
        "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
        "scope": if request.worktree_id.is_some() { "current-worktree" } else { "environment-only" },
        "environment": session.map(|session| serde_json::json!({
            "usedFallback": session.info.used_fallback, "pathEntries": session.info.path_entries,
            "gitVersion": session.info.git_version.split_whitespace()
                .find(|part| part.split('.').count() >= 2 && part.chars().all(|ch| ch.is_ascii_digit() || ch == '.')),
        })),
        "settings": { "language": settings.language.as_deref().filter(|value| matches!(*value, "en" | "zh-CN")),
            "diffFontSize": settings.diff_font_size, "uiZoom": settings.ui_zoom,
            "viewMode": settings.view_mode, "updateMode": settings.update_mode,
            "protectedBranchPatterns": settings.protected_branches.len() },
        "commands": records.iter().rev().take(200).map(|record| public_entry(&summary(record))).collect::<Vec<_>>(),
        "history": entries.iter().map(public_entry).collect::<Vec<_>>(),
        "failures": combined.iter().filter(|entry| entry.exit_code != 0 || entry.timed_out)
            .rev().take(50).map(public_entry).collect::<Vec<_>>(),
        "performance": { "commands": combined.len(),
            "timeouts": combined.iter().filter(|entry| entry.timed_out).count(),
            "failures": combined.iter().filter(|entry| entry.exit_code != 0).count(),
            "totalElapsedMs": combined.iter().map(|entry| entry.elapsed_ms).sum::<u64>(),
            "slowest": slowest.iter().take(10).map(public_entry).collect::<Vec<_>>() },
        "redactions": redactions,
    });
    let text = serde_json::to_string_pretty(&manifest)?;
    state.next_snapshot += 1;
    // 内容只存于 Rust；调用方只能引用这一份字节，不能提交替换内容。
    let id = format!(
        "{}-{}",
        state.next_snapshot,
        super::sha256_hex(text.as_bytes())
    );
    state.snapshots.push_back(Snapshot {
        id: id.clone(),
        worktree_id: request.worktree_id.clone(),
        text: text.clone(),
        redactions,
        generated_at_ms,
    });
    while state.snapshots.len() > 8 {
        state.snapshots.pop_front();
    }
    Ok(DiagnosticsPreview {
        text,
        redactions,
        snapshot_id: id,
    })
}

pub fn export(
    request: &DiagnosticsRequest,
    store: &DiagnosticsSummaryStore,
) -> Result<DiagnosticsExport> {
    let destination = validate_destination(&request.destination)?;
    let state = store
        .state
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let snapshot = state
        .snapshots
        .iter()
        .find(|snapshot| {
            Some(&snapshot.id) == request.snapshot_id.as_ref()
                && snapshot.worktree_id == request.worktree_id
        })
        .ok_or_else(snapshot_error)?;
    super::atomic_write(&destination, snapshot.text.as_bytes())?;
    Ok(DiagnosticsExport {
        path: destination.to_string_lossy().into_owned(),
        bytes: snapshot.text.len() as u64,
        generated_at_ms: snapshot.generated_at_ms,
        redactions: snapshot.redactions,
    })
}

fn validate_destination(text: &str) -> Result<PathBuf> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.diagnosticsPathEmpty",
            "请选择诊断文件的保存位置",
            &[],
        ));
    }
    let path = PathBuf::from(trimmed);
    if !path.is_absolute() {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.diagnosticsPathNotAbsolute",
            "诊断文件必须保存到绝对路径",
            &[],
        ));
    }
    let parent = path.parent().ok_or_else(|| {
        AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.diagnosticsPathInvalid",
            "诊断文件的目录不存在",
            &[],
        )
    })?;
    if !parent.is_dir() {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.diagnosticsPathInvalid",
            "诊断文件的目录不存在",
            &[],
        ));
    }
    if path.is_dir() {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.diagnosticsPathIsDirectory",
            "诊断文件的目标是一个目录",
            &[],
        ));
    }
    Ok(path)
}
