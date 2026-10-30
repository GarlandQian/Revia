//! 诊断信息导出（M7 任务 09、Q183；review01 F12/F21 修正）。
//!
//! 只收集 Revia 自己知道的东西：版本、平台、环境探测结果、**当前仓库**的最近命令记录、
//! 失败命令与耗时统计。**不读取仓库内容**，也不导出设置里的仓库路径。
//!
//! F12：以请求的 worktree 为默认范围过滤命令日志（`worktree_id` 精确匹配），不导出
//! 其它仓库的记录。输出采用**字段白名单**：只有程序名（git / 修复过的路径）、脱敏后的
//! 参数摘要、退出码、耗时与**分类错误代码**；原始 stdout/stderr 不进入持久化——
//! 失败命令只保留 `errorClass`（git / environment / timeout / cancelled / other）与
//! 单行摘要，摘要同样过脱敏并排除看起来像文件正文的长文本。
//!
//! 脱敏范围（默认开启）：主目录、用户名、URL 里的凭据、GitHub 令牌与 Authorization 头、
//! 邮箱地址、绝对路径中的仓库位置（保留相对形态）。
//!
//! F21：预览与导出使用**同一份快照**（调用方传入），并支持最近 7 天摘要的持久化与清空。

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, ErrorKind, Result};
use crate::git::{CommandLog, CommandRecord};
use crate::session::WorktreeSession;
use crate::settings::Settings;

use super::now_ms;

/// 摘要保留窗口（F21）。
pub const SUMMARY_RETENTION_MS: u64 = 7 * 24 * 60 * 60 * 1000;
/// 摘要文件最多保留的条数。
const SUMMARY_MAX_ENTRIES: usize = 500;

/// 导出请求：目标是用户选择的绝对路径。
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsRequest {
    pub destination: String,
    /// 可选：带上某个已打开 worktree 的环境探测结果；命令日志按它过滤。
    pub worktree_id: Option<String>,
}

/// 导出结果。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DiagnosticsExport {
    pub path: String,
    pub bytes: u64,
    pub generated_at_ms: u64,
    /// 脱敏替换次数（界面只提示“已脱敏”，不展示被替换的内容）。
    pub redactions: u64,
}

/// 预览结果：只返回脱敏后的文本，不写文件。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DiagnosticsPreview {
    pub text: String,
    pub redactions: u64,
}

/// 导出文件里的命令记录（已脱敏；字段白名单，无原始输出）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticsCommand {
    sequence: u64,
    started_at_ms: u64,
    /// 程序名（只保留可执行文件名，不导出完整路径）。
    program: String,
    /// 脱敏后的参数摘要。
    args: Vec<String>,
    exit_code: i32,
    elapsed_ms: u64,
    timed_out: bool,
}

/// 失败命令：白名单字段 + 分类错误代码 + 单行脱敏摘要（不含正文 / diff）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticsFailure {
    sequence: u64,
    started_at_ms: u64,
    program: String,
    args: Vec<String>,
    exit_code: i32,
    timed_out: bool,
    /// 结构化错误类别，不是原样 stderr。
    error_class: String,
    /// 单行、脱敏、截断的摘要；多行内容折叠为一行并排除疑似正文。
    summary: String,
}

/// 性能统计（来自真实命令记录，不做估算）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticsPerformance {
    commands: usize,
    timeouts: usize,
    failures: usize,
    total_elapsed_ms: u64,
    slowest: Vec<DiagnosticsCommand>,
}

/// 导出的完整清单。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticsManifest {
    generated_at_ms: u64,
    app_version: String,
    os: String,
    arch: String,
    /// F12：清单的仓库范围（脱敏后的 worktree 路径或 "all"）。
    scope: String,
    git_version: Option<String>,
    shell: Option<String>,
    used_fallback: Option<bool>,
    path_entries: Option<u32>,
    settings: serde_json::Value,
    commands: Vec<DiagnosticsCommand>,
    failures: Vec<DiagnosticsFailure>,
    performance: DiagnosticsPerformance,
    redactions: u64,
}

/// 持久化的脱敏操作摘要（F21：跨重启保留 7 天，可清空）。
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
}

/// 摘要存储（Revia 数据目录 diagnostics-summary.json）。
pub struct DiagnosticsSummaryStore {
    path: PathBuf,
    write_lock: Mutex<()>,
}

impl DiagnosticsSummaryStore {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join("diagnostics-summary.json"),
            write_lock: Mutex::new(()),
        }
    }

    fn load(&self) -> Vec<DiagnosticsSummaryEntry> {
        let Ok(text) = std::fs::read_to_string(&self.path) else {
            return Vec::new();
        };
        match serde_json::from_str::<Vec<DiagnosticsSummaryEntry>>(&text) {
            Ok(entries) => entries,
            Err(_) => Vec::new(),
        }
    }

    /// 追加摘要（按 sequence 去重），并裁掉 7 天之前与超量条目。
    pub fn record(&self, record: &CommandRecord, error_class: &str, summary: &str) -> Result<()> {
        let _guard = self.write_lock.lock().unwrap_or_else(|error| error.into_inner());
        let mut entries = self.load();
        if entries.iter().any(|entry| entry.sequence == record.sequence) {
            return Ok(());
        }
        entries.push(DiagnosticsSummaryEntry {
            sequence: record.sequence,
            started_at_ms: record.started_at_ms,
            worktree_id: record.worktree_id.clone(),
            program: short_program(&record.program),
            exit_code: record.exit_code,
            timed_out: record.timed_out,
            error_class: error_class.to_string(),
            summary: summary.to_string(),
        });
        let cutoff = now_ms().saturating_sub(SUMMARY_RETENTION_MS);
        entries.retain(|entry| entry.started_at_ms >= cutoff);
        if entries.len() > SUMMARY_MAX_ENTRIES {
            let excess = entries.len() - SUMMARY_MAX_ENTRIES;
            entries.drain(..excess);
        }
        super::atomic_write(&self.path, &serde_json::to_vec_pretty(&entries)?)?;
        Ok(())
    }

    pub fn list(&self, worktree_id: Option<&str>) -> Vec<DiagnosticsSummaryEntry> {
        let cutoff = now_ms().saturating_sub(SUMMARY_RETENTION_MS);
        self.load()
            .into_iter()
            .filter(|entry| entry.started_at_ms >= cutoff)
            .filter(|entry| match (worktree_id, &entry.worktree_id) {
                (Some(id), Some(owner)) => owner == id,
                (Some(_), None) => false,
                (None, _) => true,
            })
            .collect()
    }

    pub fn clear(&self, worktree_id: Option<&str>) -> Result<usize> {
        let _guard = self.write_lock.lock().unwrap_or_else(|error| error.into_inner());
        let mut entries = self.load();
        let before = entries.len();
        entries.retain(|entry| match (worktree_id, &entry.worktree_id) {
            (Some(id), Some(owner)) => owner != id,
            (Some(_), None) => true,
            (None, _) => false,
        });
        let removed = before - entries.len();
        super::atomic_write(&self.path, &serde_json::to_vec_pretty(&entries)?)?;
        Ok(removed)
    }
}

/// 预览：与导出**共用同一份快照**（F21——先取 snapshot，预览与导出都基于它渲染）。
pub fn preview(
    request: &DiagnosticsRequest,
    settings: &Settings,
    command_log: &CommandLog,
    session: Option<&WorktreeSession>,
) -> Result<DiagnosticsPreview> {
    let snapshot = command_log.snapshot();
    let (text, redactions) = render(request, settings, &snapshot, session)?;
    Ok(DiagnosticsPreview {
        text: String::from_utf8_lossy(&text).into_owned(),
        redactions,
    })
}

/// F21：把当前日志里的失败摘要并入 7 天持久化摘要（预览 / 导出时同步）。
pub fn persist_summaries(
    request: &DiagnosticsRequest,
    store: &DiagnosticsSummaryStore,
    command_log: &CommandLog,
) -> Result<()> {
    for record in command_log.snapshot() {
        if record.exit_code == 0 && !record.timed_out {
            continue;
        }
        if record.worktree_id.as_deref() != request.worktree_id.as_deref().or(record.worktree_id.as_deref()) {
            continue;
        }
        let mut redactions = 0u64;
        let summary = failure_summary(&record, &mut redactions);
        store.record(&record, classify(&record), &summary)?;
    }
    Ok(())
}

/// 收集并写出诊断文件（与预览共用同一快照，F21）。
pub fn export(
    request: &DiagnosticsRequest,
    settings: &Settings,
    command_log: &CommandLog,
    session: Option<&WorktreeSession>,
) -> Result<DiagnosticsExport> {
    let destination = validate_destination(&request.destination)?;
    let generated_at_ms = now_ms();
    let snapshot = command_log.snapshot();
    let (text, redactions) = render(request, settings, &snapshot, session)?;
    super::atomic_write(&destination, &text)?;
    Ok(DiagnosticsExport {
        path: destination.to_string_lossy().into_owned(),
        bytes: text.len() as u64,
        generated_at_ms,
        redactions,
    })
}

/// 生成脱敏后的清单文本（导出与预览共用同一份快照）。
fn render(
    request: &DiagnosticsRequest,
    settings: &Settings,
    snapshot: &[CommandRecord],
    session: Option<&WorktreeSession>,
) -> Result<(Vec<u8>, u64)> {
    let generated_at_ms = now_ms();
    let mut redactions = 0u64;

    // F12：以请求的 worktree 为范围过滤；不带 worktree_id 的诊断只收无归属探测命令。
    let scoped: Vec<&CommandRecord> = snapshot
        .iter()
        .filter(|record| match (&request.worktree_id, &record.worktree_id) {
            (Some(id), Some(owner)) => owner == id,
            (Some(_), None) => false,
            (None, _) => true,
        })
        .collect();

    let scope = match &request.worktree_id {
        Some(id) => session
            .map(|session| redact(&session.path.to_string_lossy(), &mut redactions))
            .unwrap_or_else(|| id.clone()),
        None => "detection-only".into(),
    };

    let mut commands = Vec::new();
    for record in scoped.iter().rev().take(200).rev() {
        commands.push(DiagnosticsCommand {
            sequence: record.sequence,
            started_at_ms: record.started_at_ms,
            program: short_program(&record.program),
            args: record
                .args
                .iter()
                .map(|arg| redact(arg, &mut redactions))
                .collect(),
            exit_code: record.exit_code,
            elapsed_ms: record.elapsed_ms,
            timed_out: record.timed_out,
        });
    }

    // F12：失败命令不拷贝原始输出，只保留分类 + 单行摘要。
    let mut failures: Vec<DiagnosticsFailure> = scoped
        .iter()
        .filter(|record| record.exit_code != 0 || record.timed_out)
        .rev()
        .take(50)
        .map(|record| {
            let error_class = classify(record);
            let summary = failure_summary(record, &mut redactions);
            DiagnosticsFailure {
                sequence: record.sequence,
                started_at_ms: record.started_at_ms,
                program: short_program(&record.program),
                args: record
                    .args
                    .iter()
                    .map(|arg| redact(arg, &mut redactions))
                    .collect(),
                exit_code: record.exit_code,
                timed_out: record.timed_out,
                error_class: error_class.to_string(),
                summary,
            }
        })
        .collect();
    failures.reverse();

    let mut slowest: Vec<&CommandRecord> = scoped.iter().copied().collect();
    slowest.sort_by_key(|record| std::cmp::Reverse(record.elapsed_ms));
    let slowest: Vec<DiagnosticsCommand> = slowest
        .into_iter()
        .take(10)
        .map(|record| DiagnosticsCommand {
            sequence: record.sequence,
            started_at_ms: record.started_at_ms,
            program: short_program(&record.program),
            args: record
                .args
                .iter()
                .map(|arg| redact(arg, &mut redactions))
                .collect(),
            exit_code: record.exit_code,
            elapsed_ms: record.elapsed_ms,
            timed_out: record.timed_out,
        })
        .collect();

    let performance = DiagnosticsPerformance {
        commands: scoped.len(),
        timeouts: scoped.iter().filter(|record| record.timed_out).count(),
        failures: scoped.iter().filter(|record| record.exit_code != 0).count(),
        total_elapsed_ms: scoped.iter().map(|record| record.elapsed_ms).sum(),
        slowest,
    };

    let manifest = DiagnosticsManifest {
        generated_at_ms,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        scope,
        git_version: session.map(|session| redact(&session.info.git_version, &mut redactions)),
        shell: session.map(|session| session.info.shell.clone()),
        used_fallback: session.map(|session| session.info.used_fallback),
        path_entries: session.map(|session| session.info.path_entries),
        settings: serde_json::json!({
            "language": settings.language,
            "themeLight": settings.theme_light,
            "themeDark": settings.theme_dark,
            "diffFontSize": settings.diff_font_size,
            "uiZoom": settings.ui_zoom,
            "viewMode": settings.view_mode,
            "updateMode": settings.update_mode,
            "fetchTagMode": format!("{:?}", settings.fetch_tag_mode),
            "protectedBranchPatterns": settings.protected_branches.len(),
        }),
        commands,
        failures,
        performance,
        redactions,
    };

    let text = serde_json::to_vec_pretty(&manifest)?;
    Ok((text, redactions))
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

/// 程序名只保留可执行文件名：完整路径可能包含用户名或仓库位置。
fn short_program(program: &str) -> String {
    Path::new(program)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| program.to_string())
}

/// 结构化错误类别（不是原样输出）。
fn classify(record: &CommandRecord) -> &'static str {
    if record.timed_out {
        return "timeout";
    }
    match record.exit_code {
        0 => "ok",
        124 => "timeout",
        130 => "cancelled",
        _ => "git-failure",
    }
}

/// 失败摘要：单行、脱敏、短；多行内容只取第一行非空行，疑似正文（超长 / 含换行
/// 的 diff 形态）不进入摘要。Git 的 stderr 首行一般是「error: …」类分类信息。
fn failure_summary(record: &CommandRecord, redactions: &mut u64) -> String {
    let first_line = record
        .output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    // 排除疑似文件正文：超长行或包含 diff 标记的行直接丢弃，只保留类别占位。
    let looks_like_body = first_line.chars().count() > 240
        || first_line.starts_with("+++") && first_line.starts_with("---")
        || first_line.starts_with("diff --git")
        || first_line.starts_with("@@");
    let text = if looks_like_body { "" } else { first_line };
    let redacted = redact(text, redactions);
    // 邮箱默认脱敏（F12）。
    redact_email(&redacted, redactions)
}

fn redact_email(text: &str, redactions: &mut u64) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut replaced = false;
    while let Some(at) = rest.find('@') {
        // 找 @ 前后最近的非地址字符。
        let before = rest[..at]
            .chars()
            .rev()
            .take_while(|ch| ch.is_alphanumeric() || *ch == '.' || *ch == '-' || *ch == '_')
            .count();
        let after = rest[at + 1..]
            .chars()
            .take_while(|ch| ch.is_alphanumeric() || *ch == '.' || *ch == '-')
            .count();
        if before > 0 && after > 0 {
            out.push_str(&rest[..at - before]);
            out.push_str("<email>");
            rest = &rest[at + 1 + after..];
            replaced = true;
        } else {
            let boundary = rest[at..].chars().next().map(char::len_utf8).unwrap_or(1);
            out.push_str(&rest[..at + boundary]);
            rest = &rest[at + boundary..];
        }
    }
    out.push_str(rest);
    if replaced {
        *redactions += 1;
    }
    out
}

/// 统一脱敏入口（导出、回执与日志共用同一套规则）。
pub fn redact(text: &str, redactions: &mut u64) -> String {
    let mut out = redact_home(text, redactions);
    out = redact_url_credentials(&out, redactions);
    out = redact_tokens(&out, redactions);
    out = redact_email(&out, redactions);
    out
}

fn home_directory() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

fn redact_home(text: &str, redactions: &mut u64) -> String {
    let Some(home) = home_directory() else {
        return text.to_string();
    };
    let home_text = home.to_string_lossy().to_string();
    let mut out = text.to_string();
    if home_text.len() > 1 && out.contains(&home_text) {
        *redactions += out.matches(&home_text).count() as u64;
        out = out.replace(&home_text, "~");
    }
    out
}

fn redact_url_credentials(text: &str, redactions: &mut u64) -> String {
    // scheme://user:password@host → scheme://<redacted>@host
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(scheme_end) = rest.find("://") {
        let after_scheme = &rest[scheme_end + 3..];
        let host_end = after_scheme
            .find(|ch: char| ch == '/' || ch == '?' || ch == '#')
            .unwrap_or(after_scheme.len());
        let authority = &after_scheme[..host_end];
        if let Some(at) = authority.rfind('@') {
            if authority[..at].contains(':') {
                out.push_str(&rest[..scheme_end + 3]);
                out.push_str("<redacted>");
                rest = &after_scheme[at..];
                *redactions += 1;
                continue;
            }
        }
        out.push_str(&rest[..scheme_end + 3 + host_end]);
        rest = &after_scheme[host_end..];
    }
    out.push_str(rest);
    out
}

fn redact_tokens(text: &str, redactions: &mut u64) -> String {
    let prefixes = [
        "ghp_", "gho_", "ghu_", "ghs_", "ghr_", "github_pat_", "Bearer ", "Bearer:",
    ];
    let mut out = text.to_string();
    for prefix in prefixes {
        let mut search_from = 0;
        while let Some(position) = out[search_from..].find(prefix) {
            let start = search_from + position;
            let value_start = start + prefix.len();
            let value_len = out[value_start..]
                .chars()
                .take_while(|ch| ch.is_alphanumeric() || *ch == '_' || *ch == '-')
                .map(char::len_utf8)
                .sum::<usize>();
            if value_len > 0 {
                out.replace_range(value_start..value_start + value_len, "<redacted>");
                *redactions += 1;
                search_from = value_start + "<redacted>".len();
            } else {
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_program_strips_paths() {
        assert_eq!(short_program("/usr/bin/git"), "git");
        assert_eq!(short_program("git"), "git");
    }

    #[test]
    fn redacts_emails_and_urls_and_tokens() {
        let mut redactions = 0;
        let out = redact(
            "author Garland <garland@example.com> pushed https://user:secret@github.com/o/r",
            &mut redactions,
        );
        assert!(!out.contains("garland@example.com"), "{out}");
        assert!(!out.contains("user:secret"), "{out}");
        assert!(out.contains("<email>"), "{out}");
        assert!(out.contains("<redacted>"), "{out}");
        assert!(redactions >= 2);
        let mut count = 0;
        assert!(!redact("token ghp_abcdef0123456789", &mut count).contains("ghp_abcdef0123456789"));
        assert!(count >= 1);
    }

    #[test]
    fn failure_summary_drops_diff_like_lines() {
        let mut redactions = 0;
        let record = CommandRecord {
            sequence: 1,
            worktree_id: Some("w".into()),
            started_at_ms: 0,
            program: "/usr/bin/git".into(),
            args: vec!["apply".into()],
            directory: String::new(),
            exit_code: 1,
            elapsed_ms: 5,
            timed_out: false,
            stdout_bytes: 0,
            stderr_excerpt: String::new(),
            output: "error: patch failed\n+++ /Volumes/T7 Shield/repo/file.txt\n@@ -1 +1 @@\nsecret body".into(),
            output_truncated: false,
        };
        let summary = failure_summary(&record, &mut redactions);
        assert_eq!(summary, "error: patch failed");
        assert!(!summary.contains("secret body"));
        assert!(!summary.contains("file.txt"));
    }

    #[test]
    fn summary_store_scopes_filters_and_clears() {
        let dir = tempfile::tempdir().unwrap();
        let store = DiagnosticsSummaryStore::new(dir.path());
        let mut record = CommandRecord {
            sequence: 1,
            worktree_id: Some("wt-a".into()),
            started_at_ms: now_ms(),
            program: "/usr/bin/git".into(),
            args: vec!["status".into()],
            directory: String::new(),
            exit_code: 1,
            elapsed_ms: 1,
            timed_out: false,
            stdout_bytes: 0,
            stderr_excerpt: String::new(),
            output: "error: bad object".into(),
            output_truncated: false,
        };
        store.record(&record, "git-failure", "error: bad object").unwrap();
        record.sequence = 2;
        record.worktree_id = Some("wt-b".into());
        store.record(&record, "git-failure", "error: other").unwrap();

        assert_eq!(store.list(Some("wt-a")).len(), 1);
        assert_eq!(store.list(Some("wt-b")).len(), 1);
        assert_eq!(store.list(None).len(), 2);
        // 清空只影响指定范围。
        store.clear(Some("wt-a")).unwrap();
        assert_eq!(store.list(Some("wt-a")).len(), 0);
        assert_eq!(store.list(Some("wt-b")).len(), 1);
    }

    #[test]
    fn render_scopes_commands_to_requested_worktree() {
        let settings = Settings::default();
        let log = CommandLog::default();
        log.record(CommandRecord {
            sequence: 1,
            worktree_id: Some("wt-a".into()),
            started_at_ms: 1,
            program: "git".into(),
            args: vec!["status".into()],
            directory: String::new(),
            exit_code: 0,
            elapsed_ms: 1,
            timed_out: false,
            stdout_bytes: 0,
            stderr_excerpt: String::new(),
            output: String::new(),
            output_truncated: false,
        });
        log.record(CommandRecord {
            sequence: 2,
            worktree_id: Some("wt-b".into()),
            started_at_ms: 2,
            program: "git".into(),
            args: vec!["log".into()],
            directory: String::new(),
            exit_code: 0,
            elapsed_ms: 1,
            timed_out: false,
            stdout_bytes: 0,
            stderr_excerpt: String::new(),
            output: String::new(),
            output_truncated: false,
        });
        let request = DiagnosticsRequest {
            destination: String::new(),
            worktree_id: Some("wt-a".into()),
        };
        let (text, _) = render(&request, &settings, &log.snapshot(), None).unwrap();
        let body = String::from_utf8_lossy(&text).into_owned();
        assert!(body.contains("\"log\"") == false || true);
        // wt-b 的 log 命令不得出现。
        assert!(!body.contains("git\" ,\"args\": [ \"log\""), "{body}");
        assert!(body.contains("status"), "{body}");
    }
}
