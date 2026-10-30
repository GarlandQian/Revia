//! 诊断信息导出（M7 任务 09、Q183）。
//!
//! 只收集 Revia 自己知道的东西：版本、平台、环境探测结果、最近命令记录、
//! 失败命令与耗时统计。**不读取仓库内容**，也不导出设置里的仓库路径。
//!
//! 导出前统一脱敏：主目录、用户名、URL 里的凭据、GitHub 令牌与 Authorization 头。

use std::path::{Path, PathBuf};

use serde::Serialize;
use ts_rs::TS;

use crate::error::{AppError, ErrorKind, Result};
use crate::git::{CommandLog, CommandRecord};
use crate::session::WorktreeSession;
use crate::settings::Settings;

use super::now_ms;

/// 导出请求：目标是用户选择的绝对路径。
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsRequest {
    pub destination: String,
    /// 可选：带上某个已打开 worktree 的环境探测结果。
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

/// 导出文件里的命令记录（已脱敏）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticsCommand {
    sequence: u64,
    started_at_ms: u64,
    program: String,
    args: Vec<String>,
    exit_code: i32,
    elapsed_ms: u64,
    timed_out: bool,
    stderr_excerpt: String,
}

/// 失败命令（附一小段输出，便于定位）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticsFailure {
    sequence: u64,
    program: String,
    args: Vec<String>,
    exit_code: i32,
    timed_out: bool,
    output_excerpt: String,
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

/// 预览结果：只返回脱敏后的文本，不写文件。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DiagnosticsPreview {
    pub text: String,
    pub redactions: u64,
}

/// 预览（任务 09）：与导出一致的内容，但不落盘。
pub fn preview(
    _request: &DiagnosticsRequest,
    settings: &Settings,
    command_log: &CommandLog,
    session: Option<&WorktreeSession>,
) -> Result<DiagnosticsPreview> {
    let (text, redactions) = render(settings, command_log, session)?;
    Ok(DiagnosticsPreview {
        text: String::from_utf8_lossy(&text).into_owned(),
        redactions,
    })
}

/// 收集并写出诊断文件。
pub fn export(
    request: &DiagnosticsRequest,
    settings: &Settings,
    command_log: &CommandLog,
    session: Option<&WorktreeSession>,
) -> Result<DiagnosticsExport> {
    let destination = validate_destination(&request.destination)?;
    let generated_at_ms = now_ms();
    let (text, redactions) = render(settings, command_log, session)?;
    super::atomic_write(&destination, &text)?;
    Ok(DiagnosticsExport {
        path: destination.to_string_lossy().into_owned(),
        bytes: text.len() as u64,
        generated_at_ms,
        redactions,
    })
}

/// 生成脱敏后的清单文本（导出与预览共用）。
fn render(
    settings: &Settings,
    command_log: &CommandLog,
    session: Option<&WorktreeSession>,
) -> Result<(Vec<u8>, u64)> {
    let generated_at_ms = now_ms();
    let records = command_log.snapshot();
    let mut redactions = 0u64;

    let mut commands = Vec::new();
    for record in records.iter().rev().take(200).rev() {
        commands.push(redact_command(record, &mut redactions));
    }
    let mut failures: Vec<DiagnosticsFailure> = records
        .iter()
        .filter(|record| record.exit_code != 0 || record.timed_out)
        .rev()
        .take(50)
        .map(|record| DiagnosticsFailure {
            sequence: record.sequence,
            program: redact(&record.program, &mut redactions),
            args: record
                .args
                .iter()
                .map(|arg| redact(arg, &mut redactions))
                .collect(),
            exit_code: record.exit_code,
            timed_out: record.timed_out,
            output_excerpt: redact(&excerpt(&record.output, 2048), &mut redactions),
        })
        .collect();
    failures.reverse();

    let mut slowest: Vec<&CommandRecord> = records.iter().collect();
    slowest.sort_by_key(|record| std::cmp::Reverse(record.elapsed_ms));
    let slowest: Vec<DiagnosticsCommand> = slowest
        .into_iter()
        .take(10)
        .map(|record| redact_command(record, &mut redactions))
        .collect();

    let performance = DiagnosticsPerformance {
        commands: records.len(),
        timeouts: records.iter().filter(|record| record.timed_out).count(),
        failures: records.iter().filter(|record| record.exit_code != 0).count(),
        total_elapsed_ms: records.iter().map(|record| record.elapsed_ms).sum(),
        slowest,
    };

    let manifest = DiagnosticsManifest {
        generated_at_ms,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        git_version: session.map(|session| redact(&session.info.git_version, &mut redactions)),
        shell: session.map(|session| redact(&session.info.shell, &mut redactions)),
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

fn redact_command(record: &CommandRecord, redactions: &mut u64) -> DiagnosticsCommand {
    DiagnosticsCommand {
        sequence: record.sequence,
        started_at_ms: record.started_at_ms,
        program: redact(&record.program, redactions),
        args: record
            .args
            .iter()
            .map(|arg| redact(arg, redactions))
            .collect(),
        exit_code: record.exit_code,
        elapsed_ms: record.elapsed_ms,
        timed_out: record.timed_out,
        stderr_excerpt: redact(&excerpt(&record.stderr_excerpt, 1024), redactions),
    }
}

fn excerpt(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    text.chars().take(limit).collect::<String>() + "…"
}

/// 统一脱敏入口（导出、回执与日志共用同一套规则）。
pub fn redact(text: &str, redactions: &mut u64) -> String {
    let mut out = redact_home(text, redactions);
    out = redact_url_credentials(&out, redactions);
    out = redact_tokens(&out, redactions);
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
    if let Some(name) = home.file_name().and_then(|value| value.to_str()) {
        // 用户名可能出现在路径片段里（例如 /Users/<name>/），只替换成占位符。
        if name.len() >= 2 {
            let count = out.matches(name).count() as u64;
            if count > 0 {
                *redactions += count;
                out = out.replace(name, "<user>");
            }
        }
    }
    out
}

/// 去掉 `scheme://user:token@host` 里的凭据部分。
fn redact_url_credentials(text: &str, redactions: &mut u64) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find("://") {
        let (head, tail) = rest.split_at(index + 3);
        out.push_str(head);
        // 凭据段以 `@` 结束，且必须在路径分隔符之前。
        let stop = tail
            .find(|ch: char| matches!(ch, '/' | ' ' | '\n' | '"' | '\t'))
            .unwrap_or(tail.len());
        if let Some(at) = tail[..stop].find('@') {
            out.push_str("<redacted>@");
            *redactions += 1;
            rest = &tail[at + 1..];
        } else {
            rest = tail;
        }
    }
    out.push_str(rest);
    out
}

/// 令牌前缀与 Authorization 头。
fn redact_tokens(text: &str, redactions: &mut u64) -> String {
    const PREFIXES: [&str; 8] = [
        "ghp_",
        "gho_",
        "ghu_",
        "ghs_",
        "ghr_",
        "github_pat_",
        "ghp-",
        "x-access-token:",
    ];
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    let bytes = text.as_bytes();
    while index < bytes.len() {
        let rest = &text[index..];
        let mut matched = None;
        for prefix in PREFIXES {
            if rest.starts_with(prefix) {
                matched = Some(prefix);
                break;
            }
        }
        if let Some(prefix) = matched {
            out.push_str(prefix);
            out.push_str("<redacted>");
            *redactions += 1;
            index += prefix.len();
            while index < bytes.len() {
                let ch = bytes[index] as char;
                if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                    index += 1;
                } else {
                    break;
                }
            }
            continue;
        }
        if rest.to_ascii_lowercase().starts_with("bearer ") {
            out.push_str("Bearer <redacted>");
            *redactions += 1;
            index += "bearer ".len();
            while index < bytes.len() {
                let ch = bytes[index] as char;
                if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.' {
                    index += 1;
                } else {
                    break;
                }
            }
            continue;
        }
        let ch = rest.chars().next().unwrap_or_default();
        out.push(ch);
        index += ch.len_utf8();
    }
    out
}

/// 供测试与界面提示：脱敏后的文本不会包含主目录。
pub fn redacted_home_marker(path: &Path) -> String {
    let mut redactions = 0;
    redact(&path.to_string_lossy(), &mut redactions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_tokens_and_credentials() {
        let mut count = 0;
        let text = "clone https://alice:secret@example.com/repo.git ghp_abcdef1234567890 Bearer abc.def";
        let out = redact(text, &mut count);
        assert!(!out.contains("secret"));
        assert!(!out.contains("abcdef1234567890"));
        assert!(!out.contains("abc.def"));
        assert!(count >= 3);
        assert!(out.contains("https://<redacted>@example.com/repo.git"));
    }

    #[test]
    fn redacts_home_directory_and_username() {
        let mut count = 0;
        let home = home_directory().unwrap_or_else(|| PathBuf::from("/home/tester"));
        let path = home.join("project/repo");
        let out = redact(&path.to_string_lossy(), &mut count);
        assert!(out.starts_with('~'), "{out}");
        assert!(count >= 1);
    }

    #[test]
    fn destination_must_be_absolute_existing_directory() {
        assert!(validate_destination("relative.json").is_err());
        let dir = tempfile::tempdir().unwrap();
        assert!(validate_destination(&dir.path().to_string_lossy()).is_err());
        assert!(validate_destination(
            &dir.path().join("diag.json").to_string_lossy()
        )
        .is_ok());
    }
}
