//! 提交时警告（返修 E.3，Q139）：提交前检查下列情况，发现问题时先弹提示，
//! 用户可以继续或取消。检查由 Rust 层完成，前端只显示结果。
//!
//! - CRLF 换行；
//! - 游离 HEAD 或 rebase 进行中；
//! - 大于阈值的大文件（数值可改）；
//! - 在其他系统上不合法的文件名。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::Result;
use crate::git::Git;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum CommitWarningKind {
    Crlf,
    DetachedOrRebase,
    LargeFile,
    InvalidFilename,
}

/// 文件名不合法的具体原因；由前端按完整字面量键翻译。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[ts(export)]
pub enum InvalidFilenameReason {
    Character { character: String },
    ControlCharacter,
    TrailingSpaceOrDot,
    ReservedName { component: String },
}

/// 警告的机器可读数据；Rust 不返回任何面向用户的句子。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[ts(export)]
pub enum CommitWarningDetail {
    DetachedHead,
    RebaseInProgress,
    Crlf,
    LargeFile { size_mb: u64, limit_mb: u64 },
    InvalidFilename { reason: InvalidFilenameReason },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CommitWarning {
    pub kind: CommitWarningKind,
    /// 相关文件路径；与操作整体有关的警告（游离 HEAD、rebase）为 None。
    pub path: Option<String>,
    pub detail: CommitWarningDetail,
}

/// 检查项开关（来自设置）。
#[derive(Debug, Clone, Copy)]
pub struct WarningOptions {
    pub crlf: bool,
    pub detached: bool,
    pub large_file_bytes: u64,
    pub invalid_filename: bool,
}

impl Default for WarningOptions {
    fn default() -> Self {
        Self {
            crlf: true,
            detached: true,
            large_file_bytes: 50 * 1024 * 1024,
            invalid_filename: true,
        }
    }
}

/// 暂存区里将要提交的一个文件。
#[derive(Debug, Clone)]
pub struct StagedFile {
    pub path: String,
    pub size: u64,
    /// 内容里是否有 `\r\n`。
    pub has_crlf: bool,
}

/// 按设置生成警告（纯函数，便于测试）。
pub fn evaluate(
    files: &[StagedFile],
    detached: bool,
    rebase_in_progress: bool,
    options: &WarningOptions,
) -> Vec<CommitWarning> {
    let mut warnings = Vec::new();
    if options.detached && detached {
        warnings.push(CommitWarning {
            kind: CommitWarningKind::DetachedOrRebase,
            path: None,
            detail: CommitWarningDetail::DetachedHead,
        });
    }
    if options.detached && rebase_in_progress {
        warnings.push(CommitWarning {
            kind: CommitWarningKind::DetachedOrRebase,
            path: None,
            detail: CommitWarningDetail::RebaseInProgress,
        });
    }
    for file in files {
        if options.crlf && file.has_crlf {
            warnings.push(CommitWarning {
                kind: CommitWarningKind::Crlf,
                path: Some(file.path.clone()),
                detail: CommitWarningDetail::Crlf,
            });
        }
        if options.large_file_bytes > 0 && file.size > options.large_file_bytes {
            warnings.push(CommitWarning {
                kind: CommitWarningKind::LargeFile,
                path: Some(file.path.clone()),
                detail: CommitWarningDetail::LargeFile {
                    size_mb: file.size / (1024 * 1024),
                    limit_mb: options.large_file_bytes / (1024 * 1024),
                },
            });
        }
        if options.invalid_filename {
            if let Some(reason) = invalid_path_reason(&file.path) {
                warnings.push(CommitWarning {
                    kind: CommitWarningKind::InvalidFilename,
                    path: Some(file.path.clone()),
                    detail: CommitWarningDetail::InvalidFilename { reason },
                });
            }
        }
    }
    warnings
}

/// Windows 保留设备名（不区分大小写，含带扩展名的形式）。
const RESERVED_NAMES: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9",
    "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// 在其他系统（Windows / macOS 默认文件系统）上不合法的路径部分。
pub fn invalid_path_reason(path: &str) -> Option<InvalidFilenameReason> {
    for component in path.split('/') {
        if component.is_empty() {
            continue;
        }
        if let Some(ch) = component.chars().find(|ch| matches!(ch, '<' | '>' | ':' | '"' | '\\' | '|' | '?' | '*')) {
            return Some(InvalidFilenameReason::Character { character: ch.to_string() });
        }
        if component.chars().any(|ch| (ch as u32) < 0x20) {
            return Some(InvalidFilenameReason::ControlCharacter);
        }
        if component.ends_with(' ') || component.ends_with('.') {
            return Some(InvalidFilenameReason::TrailingSpaceOrDot);
        }
        let stem = component.split('.').next().unwrap_or(component).to_ascii_lowercase();
        if RESERVED_NAMES.contains(&stem.as_str()) {
            return Some(InvalidFilenameReason::ReservedName { component: component.to_string() });
        }
        // 大小写不敏感的系统上会冲突的名字不在单文件层面判断（需要全量列表）。
    }
    None
}

/// 读取暂存区并生成警告（真实 Git 调用）。
pub fn collect(git: &Git, options: &WarningOptions) -> Result<Vec<CommitWarning>> {
    if !options.crlf && options.large_file_bytes == 0 && !options.invalid_filename && !options.detached {
        return Ok(Vec::new());
    }
    let detached = git.try_run(&["symbolic-ref", "-q", "HEAD"]).is_none();
    // 进行中的操作统一由 operation 模块识别（linked worktree、sequencer 都已处理）。
    let rebase_in_progress = crate::operation::detect(git, 0)?
        .is_some_and(|state| state.kind == crate::operation::OperationKind::Rebase);
    let mut files = Vec::new();
    if options.crlf || options.large_file_bytes > 0 {
        let output = git.run_ok(
            &["-c", "core.quotePath=false", "diff", "--cached", "--name-only", "--diff-filter=d", "-z"],
            "读取暂存文件",
        )?;
        for path in output.stdout_lossy().split('\0').filter(|path| !path.is_empty()) {
            // 上限：只为检查读前 500 个文件，避免提交前多花几秒。
            if files.len() >= 500 {
                break;
            }
            let size = git
                .try_run(&["cat-file", "-s", &format!(":0:{path}")])
                .and_then(|text| text.trim().parse::<u64>().ok())
                .unwrap_or(0);
            // 大文件不读内容：只在 2 MB 以内做 CRLF 检查。
            let has_crlf = options.crlf
                && size <= 2 * 1024 * 1024
                && git
                    .try_run(&["show", &format!(":0:{path}")])
                    .is_some_and(|text| text.contains('\r'));
            files.push(StagedFile { path: path.to_string(), size, has_crlf });
        }
    }
    Ok(evaluate(&files, detached, rebase_in_progress, options))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, size: u64, crlf: bool) -> StagedFile {
        StagedFile { path: path.into(), size, has_crlf: crlf }
    }

    #[test]
    fn crlf_and_large_files_are_reported_only_when_enabled() {
        let files = [file("a.txt", 60 * 1024 * 1024, true)];
        let warnings = evaluate(&files, false, false, &WarningOptions::default());
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings.iter().any(|warning| warning.kind == CommitWarningKind::Crlf));
        assert!(warnings.iter().any(|warning| warning.kind == CommitWarningKind::LargeFile));

        let disabled = WarningOptions { crlf: false, large_file_bytes: 0, ..WarningOptions::default() };
        assert!(evaluate(&files, false, false, &disabled).is_empty());
    }

    #[test]
    fn detached_and_rebase_are_reported() {
        let warnings = evaluate(&[], true, true, &WarningOptions::default());
        assert_eq!(warnings.len(), 2);
        assert!(warnings.iter().all(|warning| warning.path.is_none()));
    }

    #[test]
    fn invalid_filenames_are_detected() {
        assert!(invalid_path_reason("src/ok.ts").is_none());
        assert!(invalid_path_reason("src/a:b.ts").is_some());
        assert!(invalid_path_reason("src/q?.ts").is_some());
        assert!(invalid_path_reason("src/trailing .ts ").is_some());
        assert!(invalid_path_reason("docs/CON.txt").is_some());
        assert!(invalid_path_reason("docs/nul").is_some());
        assert!(invalid_path_reason("docs/console.txt").is_none(), "CON 前缀不算保留名");
    }
}
