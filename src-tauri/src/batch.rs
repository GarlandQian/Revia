//! 多文件批量写操作（M4 F09）：按文件校验写前指纹，串行执行，返回逐个文件的结果。
//!
//! 边界与单项操作完全一致（ADR-0005、ADR-0009、安全边界）：
//! - 每个文件在真正写入前都重新读取真实状态并核对指纹；
//! - 丢弃时逐个文件先保存回收记录，再写工作区；
//! - 只读条目（符号链接、子模块、嵌套仓库、冲突等）不参与写入，结果里说明跳过原因；
//! - 部分失败不中断其余文件，逐项如实返回；不确定的结果标为「结果待核对」。
//!
//! 批量入口在整个批次期间只取一次同 worktree 写锁，避免与其它写操作交错。

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, AppErrorPayload, ErrorKind, Result};
use crate::git::diff;
use crate::git::status::{self, ChangesKind, ReadonlyReason, StatusEntry};
use crate::session::{ChangesSnapshot, WorktreeSession, compute_file_fingerprint_from_patch};

const BATCH_MAX_FILES: usize = 200;
const BATCH_MAX_ARG_BYTES: usize = 8000;
const LOCK_RETRIES: usize = 3;
const LOCK_RETRY_DELAY: Duration = Duration::from_millis(100);

/// 一个批量操作的目标：路径、所在组与用户所见指纹。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BatchTarget {
    pub path: String,
    pub changes: ChangesKind,
    /// 用户确认时看到的文件级 diff 指纹；后端写前逐文件核对（ADR-0009）。
    pub fingerprint: String,
}

/// 一个文件的执行结果。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BatchFileResult {
    pub path: String,
    pub changes: ChangesKind,
    pub ok: bool,
    /// 只读文件未执行时的结构化原因（符号链接、子模块、冲突等）。
    pub skipped: Option<AppErrorPayload>,
    /// 失败原因；成功与跳过时为 None。
    pub error: Option<AppErrorPayload>,
}

impl BatchFileResult {
    pub fn success(target: &BatchTarget) -> Self {
        Self {
            path: target.path.clone(),
            changes: target.changes,
            ok: true,
            skipped: None,
            error: None,
        }
    }

    pub fn skipped(target: &BatchTarget, reason: ReadonlyReason) -> Self {
        Self {
            path: target.path.clone(),
            changes: target.changes,
            ok: false,
            skipped: Some(reason.error().payload()),
            error: None,
        }
    }

    pub fn failed(target: &BatchTarget, error: &AppError) -> Self {
        Self {
            path: target.path.clone(),
            changes: target.changes,
            ok: false,
            skipped: None,
            error: Some(error.payload()),
        }
    }
}

/// 一次批量操作的汇总：逐文件结果 + 操作后的真实快照。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BatchOutcome {
    pub results: Vec<BatchFileResult>,
    /// 批次结束后的最新快照；读取失败时为 None（界面应重新刷新）。
    pub snapshot: Option<ChangesSnapshot>,
    /// 是否有文件的结果无法确定（需要用户核对真实状态）。
    pub needs_verification: bool,
}

impl BatchOutcome {
    pub fn all_failed(targets: &[BatchTarget], error: &AppError) -> Self {
        Self {
            results: targets
                .iter()
                .map(|target| BatchFileResult::failed(target, error))
                .collect(),
            snapshot: None,
            needs_verification: error.kind == ErrorKind::NeedsVerification,
        }
    }
}

struct VerifiedStageTarget<'a> {
    index: usize,
    target: &'a BatchTarget,
    original_path: Option<String>,
}

fn stale_target_error() -> AppError {
    AppError::coded(
        ErrorKind::Rejected,
        "errors.batch.theFileChangedConfirmAgain",
        "文件已变化，请重新确认",
        &[],
    )
}

fn stale_fingerprint_error() -> AppError {
    AppError::coded(
        ErrorKind::Rejected,
        "errors.session.theFileChangedConfirmAgain21",
        "文件已变化，请重新确认",
        &[],
    )
}

fn timed_out_error() -> AppError {
    AppError::coded(
        ErrorKind::NeedsVerification,
        "errors.session.theGitCommandTimedOutInterrupted2",
        "Git 命令执行超时或中断，写入结果待核对；请重新核对真实状态后再继续",
        &[],
    )
}

fn lock_exhausted_error(detail: String) -> AppError {
    AppError::coded(
        ErrorKind::Rejected,
        "errors.session.theGitIndexLockedAnotherProcess2",
        "Git 索引被其他进程占用（index.lock），已重试 {LOCK_RETRIES} 次：{detail}",
        &[
            ("LOCK_RETRIES", format!("{LOCK_RETRIES}")),
            ("detail", detail),
        ],
    )
}

fn staging_git_error(is_stage: bool, exit_code: i32, stderr: &str) -> AppError {
    let (code, template) = if is_stage {
        (
            "errors.session.failedStageFile",
            "暂存文件失败（退出码 {code}）：{detail}",
        )
    } else {
        (
            "errors.session.failedUnstageFile",
            "取消暂存文件失败（退出码 {code}）：{detail}",
        )
    };
    AppError::coded(
        ErrorKind::Git,
        code,
        template,
        &[
            ("code", format!("{exit_code}")),
            ("detail", stderr.trim().to_string()),
        ],
    )
}

fn verify_stage_targets<'a>(
    session: &WorktreeSession,
    indexed_targets: &[(usize, &'a BatchTarget)],
    is_stage: bool,
    results: &mut [Option<BatchFileResult>],
) -> Result<Vec<VerifiedStageTarget<'a>>> {
    let status = status::read(session.git())?;
    let expected_group = if is_stage {
        [ChangesKind::Unstaged, ChangesKind::Untracked]
    } else {
        [ChangesKind::Staged, ChangesKind::Staged]
    };

    let mut pending: Vec<(usize, &'a BatchTarget, &StatusEntry)> =
        Vec::with_capacity(indexed_targets.len());
    let mut diff_paths: Vec<&str> = Vec::new();
    let mut diff_path_seen: HashSet<&str> = HashSet::new();

    for &(idx, target) in indexed_targets {
        let entry = status.entries.iter().find(|entry| {
            entry.path == target.path && expected_group.contains(&entry.changes)
        });
        let Some(entry) = entry else {
            results[idx] = Some(BatchFileResult::failed(target, &stale_target_error()));
            continue;
        };
        if let Some(reason) = entry.readonly_reason {
            results[idx] = Some(BatchFileResult::skipped(target, reason));
            continue;
        }
        let full_path = session.path.join(&target.path);
        if full_path.is_symlink() {
            let error = AppError::coded(
                ErrorKind::Rejected,
                "errors.session.symbolicLinksReadOnlyHereStaging",
                "符号链接为只读类型，不支持在应用内暂存或取消暂存",
                &[],
            );
            results[idx] = Some(BatchFileResult::failed(target, &error));
            continue;
        }
        if entry.changes == ChangesKind::Untracked
            && (full_path.join(".git").exists() || target.path.ends_with('/'))
        {
            let error = AppError::coded(
                ErrorKind::Rejected,
                "errors.session.nestedRepositoriesReadOnlyHereStaging",
                "嵌套仓库为只读类型，不支持在应用内暂存",
                &[],
            );
            results[idx] = Some(BatchFileResult::failed(target, &error));
            continue;
        }
        if entry.changes != ChangesKind::Untracked && diff_path_seen.insert(target.path.as_str()) {
            diff_paths.push(target.path.as_str());
        }
        pending.push((idx, target, entry));
    }

    let bulk_patches = if diff_paths.len() > 3 {
        let mut changed_during_bulk: HashSet<String> = HashSet::new();
        let raw = if is_stage {
            let before = session.worktree_stats_for(&diff_paths);
            let bytes = session.git().diff_worktree(None)?;
            let after = session.worktree_stats_for(&diff_paths);
            for (path, (lhs, rhs)) in diff_paths.iter().zip(before.iter().zip(after.iter())) {
                if lhs != rhs {
                    changed_during_bulk.insert((*path).to_string());
                }
            }
            bytes
        } else {
            session.git().diff_index(None)?
        };
        let mut patches: HashMap<String, Vec<u8>> = HashMap::new();
        for file in diff::parse_bytes(&raw).files {
            patches.insert(file.path, file.raw_patch);
        }
        Some((patches, changed_during_bulk))
    } else {
        None
    };

    let mut verified = Vec::with_capacity(pending.len());
    for (idx, target, entry) in pending {
        let single_diff: Vec<u8>;
        let diff_bytes: &[u8] = if entry.changes == ChangesKind::Untracked {
            &[]
        } else if let Some((patches, changed_during_bulk)) = &bulk_patches {
            if changed_during_bulk.contains(&target.path) {
                match session.git().diff_worktree(Some(&target.path)) {
                    Ok(bytes) => {
                        single_diff = bytes;
                        &single_diff
                    }
                    Err(error) => {
                        results[idx] = Some(BatchFileResult::failed(target, &error));
                        continue;
                    }
                }
            } else {
                patches.get(&target.path).map(Vec::as_slice).unwrap_or(&[])
            }
        } else if is_stage {
            match session.git().diff_worktree(Some(&target.path)) {
                Ok(bytes) => {
                    single_diff = bytes;
                    &single_diff
                }
                Err(error) => {
                    results[idx] = Some(BatchFileResult::failed(target, &error));
                    continue;
                }
            }
        } else {
            match session
                .git()
                .diff_index_with_orig(&target.path, entry.original_path.as_deref())
            {
                Ok(bytes) => {
                    single_diff = bytes;
                    &single_diff
                }
                Err(error) => {
                    results[idx] = Some(BatchFileResult::failed(target, &error));
                    continue;
                }
            }
        };

        let current_fp = match compute_file_fingerprint_from_patch(entry, diff_bytes, &session.path)
        {
            Ok(fp) => fp,
            Err(error) => {
                results[idx] = Some(BatchFileResult::failed(target, &error));
                continue;
            }
        };
        if current_fp != target.fingerprint {
            results[idx] = Some(BatchFileResult::failed(target, &stale_fingerprint_error()));
            continue;
        }
        verified.push(VerifiedStageTarget {
            index: idx,
            target,
            original_path: entry.original_path.clone(),
        });
    }

    Ok(verified)
}

fn chunk_verified_targets<'a>(
    verified: Vec<VerifiedStageTarget<'a>>,
    is_stage: bool,
) -> Vec<Vec<VerifiedStageTarget<'a>>> {
    let mut chunks = Vec::new();
    let mut current = Vec::new();
    let mut current_bytes = 0usize;
    for item in verified {
        let extra_orig = if is_stage {
            0
        } else {
            item.original_path.as_ref().map_or(0, |path| path.len() + 1)
        };
        let item_bytes = item.target.path.len() + 1 + extra_orig;
        if !current.is_empty()
            && (current.len() >= BATCH_MAX_FILES || current_bytes + item_bytes > BATCH_MAX_ARG_BYTES)
        {
            chunks.push(std::mem::take(&mut current));
            current_bytes = 0;
        }
        current_bytes += item_bytes;
        current.push(item);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn collect_git_paths<'a>(chunk: &'a [VerifiedStageTarget<'a>], is_stage: bool) -> Vec<&'a str> {
    let mut paths = Vec::with_capacity(chunk.len());
    let mut seen = HashSet::with_capacity(chunk.len());
    for item in chunk {
        if !is_stage {
            if let Some(orig) = item.original_path.as_deref() {
                if seen.insert(orig) {
                    paths.push(orig);
                }
            }
        }
        if seen.insert(item.target.path.as_str()) {
            paths.push(item.target.path.as_str());
        }
    }
    paths
}

impl WorktreeSession {
    /// 批量暂存 / 取消暂存：整个批次只取一次写锁，写前按真实状态逐项核对指纹，合并调用 Git，最后统一刷新一次。
    pub fn stage_files(&self, targets: &[BatchTarget], is_stage: bool) -> BatchOutcome {
        let guard = match self.acquire_write_lock() {
            Ok(guard) => guard,
            Err(error) => return BatchOutcome::all_failed(targets, &error),
        };
        if targets.is_empty() {
            drop(guard);
            let snapshot = self.refresh().ok();
            return BatchOutcome {
                results: Vec::new(),
                snapshot,
                needs_verification: false,
            };
        }

        let mut results: Vec<Option<BatchFileResult>> = vec![None; targets.len()];
        let indexed: Vec<(usize, &BatchTarget)> = targets.iter().enumerate().collect();
        let verified = match verify_stage_targets(self, &indexed, is_stage, &mut results) {
            Ok(verified) => verified,
            Err(error) => {
                drop(guard);
                let needs_verification = error.kind == ErrorKind::NeedsVerification;
                let final_results = results
                    .into_iter()
                    .enumerate()
                    .map(|(idx, slot)| {
                        slot.unwrap_or_else(|| BatchFileResult::failed(&targets[idx], &error))
                    })
                    .collect();
                return BatchOutcome {
                    results: final_results,
                    snapshot: self.refresh().ok(),
                    needs_verification,
                };
            }
        };

        let mut needs_verification = false;
        let mut aborted_uncertain = false;
        let chunks = chunk_verified_targets(verified, is_stage);
        let mut chunk_iter = chunks.into_iter();
        while let Some(mut chunk) = chunk_iter.next() {
            if aborted_uncertain {
                let err = timed_out_error();
                for item in chunk {
                    results[item.index] = Some(BatchFileResult::failed(item.target, &err));
                }
                continue;
            }
            let mut last_lock_err: Option<String> = None;
            for attempt in 0..=LOCK_RETRIES {
                if attempt > 0 {
                    std::thread::sleep(LOCK_RETRY_DELAY * (attempt as u32));
                    let remaining: Vec<(usize, &BatchTarget)> =
                        chunk.iter().map(|item| (item.index, item.target)).collect();
                    match verify_stage_targets(self, &remaining, is_stage, &mut results) {
                        Ok(reverified) => {
                            chunk = reverified;
                            if chunk.is_empty() {
                                break;
                            }
                        }
                        Err(error) => {
                            needs_verification |= error.kind == ErrorKind::NeedsVerification;
                            for item in chunk {
                                if results[item.index].is_none() {
                                    results[item.index] =
                                        Some(BatchFileResult::failed(item.target, &error));
                                }
                            }
                            break;
                        }
                    }
                }

                let paths = collect_git_paths(&chunk, is_stage);
                let output = if is_stage {
                    self.git().stage_files(&paths)
                } else {
                    self.git().unstage_files(&paths)
                };
                let output = match output {
                    Ok(output) => output,
                    Err(error) => {
                        needs_verification |= error.kind == ErrorKind::NeedsVerification;
                        for item in &chunk {
                            results[item.index] =
                                Some(BatchFileResult::failed(item.target, &error));
                        }
                        break;
                    }
                };

                if output.timed_out || output.exit_code < 0 {
                    let err = timed_out_error();
                    needs_verification = true;
                    aborted_uncertain = true;
                    for item in &chunk {
                        results[item.index] = Some(BatchFileResult::failed(item.target, &err));
                    }
                    break;
                }

                if crate::git::is_lock_error(&output) {
                    let err_text = output.stderr.trim().to_string();
                    if attempt < LOCK_RETRIES {
                        last_lock_err = Some(err_text);
                        continue;
                    }
                    let err = lock_exhausted_error(last_lock_err.unwrap_or(err_text));
                    for item in &chunk {
                        results[item.index] = Some(BatchFileResult::failed(item.target, &err));
                    }
                    break;
                }

                if output.exit_code != 0 {
                    if chunk.len() > 1 {
                        for item in &chunk {
                            match self.execute_staging_op_without_refresh_locked(
                                &item.target.path,
                                &item.target.fingerprint,
                                is_stage,
                                None,
                            ) {
                                Ok(()) => {
                                    results[item.index] =
                                        Some(BatchFileResult::success(item.target));
                                }
                                Err(error) => {
                                    needs_verification |=
                                        error.kind == ErrorKind::NeedsVerification;
                                    results[item.index] =
                                        Some(BatchFileResult::failed(item.target, &error));
                                }
                            }
                        }
                    } else if let Some(item) = chunk.first() {
                        let err = staging_git_error(is_stage, output.exit_code, &output.stderr);
                        results[item.index] = Some(BatchFileResult::failed(item.target, &err));
                    }
                    break;
                }

                for item in &chunk {
                    results[item.index] = Some(BatchFileResult::success(item.target));
                }
                break;
            }
        }
        for remaining in chunk_iter {
            let err = timed_out_error();
            for item in remaining {
                results[item.index] = Some(BatchFileResult::failed(item.target, &err));
            }
        }

        drop(guard);
        let fallback_error = stale_target_error();
        let final_results: Vec<BatchFileResult> = results
            .into_iter()
            .enumerate()
            .map(|(idx, slot)| {
                slot.unwrap_or_else(|| BatchFileResult::failed(&targets[idx], &fallback_error))
            })
            .collect();
        let any_succeeded = final_results.iter().any(|result| result.ok);
        let snapshot = match self.refresh() {
            Ok(next) => Some(next),
            Err(_) => {
                if any_succeeded {
                    needs_verification = true;
                }
                None
            }
        };
        BatchOutcome {
            results: final_results,
            snapshot,
            needs_verification,
        }
    }

    /// 批量丢弃：整个批次只取一次写锁；每个文件先保存回收记录再写工作区，最后统一刷新一次。
    ///
    /// `snapshot_revision` 是用户确认时看到的快照版本：整批目标先按它逐项核对，
    /// 之后每个文件仍会重新读取真实状态并核对指纹（外部修改会被拒绝）。
    pub fn discard_files(
        &self,
        targets: &[BatchTarget],
        snapshot_revision: &str,
        store: &crate::recovery::RecoveryStore,
    ) -> BatchOutcome {
        let guard = match self.acquire_write_lock() {
            Ok(guard) => guard,
            Err(error) => return BatchOutcome::all_failed(targets, &error),
        };
        let confirmed = self.snapshot_entries();
        let mut results = Vec::with_capacity(targets.len());
        let mut needs_verification = false;
        let mut any_succeeded = false;
        for target in targets {
            // 用户确认的是「当时那份快照里的这些文件」：整批先按原快照逐项核对。
            let expected = if target.changes == ChangesKind::Untracked {
                ChangesKind::Untracked
            } else {
                ChangesKind::Unstaged
            };
            let confirmed_ok = confirmed.as_ref().is_some_and(|entries| {
                entries.iter().any(|entry| {
                    entry.path == target.path
                        && entry.changes == expected
                        && entry.fingerprint == target.fingerprint
                        && entry.snapshot_revision == snapshot_revision
                })
            });
            if !confirmed_ok {
                let error = AppError::coded(
                    ErrorKind::Rejected,
                    "errors.batch.theFileChangedConfirmAgain2",
                    "文件已变化，请重新确认",
                    &[],
                );
                needs_verification |= error.kind == ErrorKind::NeedsVerification;
                results.push(BatchFileResult::failed(target, &error));
                continue;
            }
            let entry = confirmed.as_ref().and_then(|entries| {
                entries
                    .iter()
                    .find(|entry| entry.path == target.path && entry.changes == expected)
            });
            if let Some(reason) = entry.and_then(|entry| entry.readonly_reason) {
                results.push(BatchFileResult::skipped(target, reason));
                continue;
            }
            let outcome = if expected == ChangesKind::Untracked {
                self.discard_untracked_no_refresh_locked(&target.path, &target.fingerprint, store)
            } else {
                self.discard_with_hook_no_refresh_locked(
                    &target.path,
                    &target.fingerprint,
                    None,
                    None,
                    None,
                    store,
                    |_| Ok(()),
                )
            };
            match outcome {
                Ok(()) => {
                    any_succeeded = true;
                    results.push(BatchFileResult::success(target));
                }
                Err(error) => {
                    needs_verification |= error.kind == ErrorKind::NeedsVerification;
                    results.push(BatchFileResult::failed(target, &error));
                }
            }
        }
        drop(guard);
        let snapshot = match self.refresh() {
            Ok(next) => Some(next),
            Err(_) => {
                if any_succeeded {
                    needs_verification = true;
                }
                None
            }
        };
        BatchOutcome {
            results,
            snapshot,
            needs_verification,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use crate::git::CommandLog;
    use crate::recovery::RecoveryStore;
    use crate::session::WorktreeSession;
    use crate::shell::{EnvCaptureInfo, ShellEnv};

    fn git(dir: &Path, args: &[&str]) {
        let output = Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .expect("运行 git");
        assert!(
            output.status.success(),
            "git {args:?} 失败：{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn shell_env(dir: &Path) -> ShellEnv {
        let path: Vec<String> = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        let vars = [
            (
                "HOME".to_string(),
                std::env::var("HOME").unwrap_or_default(),
            ),
            (
                "PATH".to_string(),
                std::env::var("PATH").unwrap_or_default(),
            ),
        ]
        .into_iter()
        .collect();
        ShellEnv {
            vars,
            path: path.clone(),
            info: EnvCaptureInfo {
                worktree: dir.display().to_string(),
                shell: "fixture".into(),
                elapsed_ms: 0,
                timed_out: false,
                used_fallback: false,
                var_count: 2,
                path_entries: path,
                fallback_reason: None,
                fallback_detail: None,
            },
        }
    }

    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q"]);
        git(&repo, &["config", "core.autocrlf", "false"]);
        git(&repo, &["config", "user.email", "revia@example.com"]);
        git(&repo, &["config", "user.name", "Revia Test"]);
        std::fs::write(repo.join("a.txt"), "a1\n").unwrap();
        std::fs::write(repo.join("b.txt"), "b1\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", "init"]);
        (dir, repo)
    }

    fn open(repo: &Path) -> std::sync::Arc<WorktreeSession> {
        WorktreeSession::open_with_review(
            "wt",
            repo,
            shell_env(repo),
            std::sync::Arc::new(CommandLog::default()),
            None,
        )
        .unwrap()
    }

    fn targets(snapshot: &ChangesSnapshot) -> Vec<BatchTarget> {
        snapshot
            .entries
            .iter()
            .filter(|entry| entry.changes != ChangesKind::Staged)
            .map(|entry| BatchTarget {
                path: entry.path.clone(),
                changes: entry.changes,
                fingerprint: entry.fingerprint.clone(),
            })
            .collect()
    }

    #[test]
    fn batch_stage_and_unstage_apply_each_file() {
        let (_dir, repo) = fixture();
        std::fs::write(repo.join("a.txt"), "a2\n").unwrap();
        std::fs::write(repo.join("b.txt"), "b2\n").unwrap();
        let session = open(&repo);
        let snapshot = session.refresh().unwrap();
        let targets = targets(&snapshot);
        assert_eq!(targets.len(), 2);
        let staged = session.stage_files(&targets, true);
        assert!(
            staged.results.iter().all(|result| result.ok),
            "{:?}",
            staged.results
        );
        let snapshot = staged.snapshot.unwrap();
        assert_eq!(snapshot.staged_count, 2);
        let unstage = snapshot
            .entries
            .iter()
            .filter(|entry| entry.changes == ChangesKind::Staged)
            .map(|entry| BatchTarget {
                path: entry.path.clone(),
                changes: entry.changes,
                fingerprint: entry.fingerprint.clone(),
            })
            .collect::<Vec<_>>();
        let unstaged = session.stage_files(&unstage, false);
        assert!(
            unstaged.results.iter().all(|result| result.ok),
            "{:?}",
            unstaged.results
        );
        assert_eq!(unstaged.snapshot.unwrap().staged_count, 0);
    }

    #[test]
    fn batch_stage_reports_stale_fingerprint_and_continues() {
        let (_dir, repo) = fixture();
        std::fs::write(repo.join("a.txt"), "a2\n").unwrap();
        std::fs::write(repo.join("b.txt"), "b2\n").unwrap();
        let session = open(&repo);
        let snapshot = session.refresh().unwrap();
        let mut targets = targets(&snapshot);
        targets[0].fingerprint = "stale".into();
        let outcome = session.stage_files(&targets, true);
        assert!(!outcome.results[0].ok);
        assert!(outcome.results[1].ok, "一个失败不应中断其余文件");
        assert!(outcome.results[0].error.is_some());
    }

    #[test]
    fn batch_discard_saves_recovery_for_each_file() {
        let (dir, repo) = fixture();
        std::fs::write(repo.join("a.txt"), "a2\n").unwrap();
        std::fs::write(repo.join("b.txt"), "b2\n").unwrap();
        let session = open(&repo);
        let snapshot = session.refresh().unwrap();
        let targets = targets(&snapshot);
        let store = RecoveryStore::new(dir.path().join("data"));
        let outcome = session.discard_files(&targets, &snapshot.revision.to_string(), &store);
        assert!(
            outcome.results.iter().all(|result| result.ok),
            "{:?}",
            outcome.results
        );
        assert_eq!(std::fs::read_to_string(repo.join("a.txt")).unwrap(), "a1\n");
        assert_eq!(std::fs::read_to_string(repo.join("b.txt")).unwrap(), "b1\n");
        let records = store.list(None).unwrap();
        assert_eq!(records.len(), 2, "每个文件都应先保存一条回收记录");
    }

    #[test]
    fn batch_discard_deletes_untracked_files_after_saving() {
        let (dir, repo) = fixture();
        std::fs::write(repo.join("new.txt"), "new content\n").unwrap();
        let session = open(&repo);
        let snapshot = session.refresh().unwrap();
        let targets = targets(&snapshot);
        let target = targets
            .iter()
            .find(|target| target.path == "new.txt")
            .unwrap();
        let store = RecoveryStore::new(dir.path().join("data"));
        let outcome = session.discard_files(
            std::slice::from_ref(target),
            &snapshot.revision.to_string(),
            &store,
        );
        assert!(outcome.results[0].ok, "{:?}", outcome.results);
        assert!(!repo.join("new.txt").exists(), "未跟踪文件应被删除");
        let records = store.list(None).unwrap();
        assert_eq!(records.len(), 1);
    }

    #[test]
    #[cfg(unix)]
    fn batch_skips_readonly_entries_with_reason() {
        let (_dir, repo) = fixture();
        std::fs::write(repo.join("a.txt"), "a2\n").unwrap();
        // 已跟踪的符号链接被改写时才是只读条目（未跟踪符号链接按普通未跟踪文件处理）。
        std::os::unix::fs::symlink("a.txt", repo.join("link.txt")).unwrap();
        git(&repo, &["add", "link.txt"]);
        git(&repo, &["commit", "-q", "-m", "link"]);
        std::fs::remove_file(repo.join("link.txt")).unwrap();
        std::os::unix::fs::symlink("b.txt", repo.join("link.txt")).unwrap();
        let session = open(&repo);
        let snapshot = session.refresh().unwrap();
        let targets = targets(&snapshot);
        assert!(targets.iter().any(|target| target.path == "link.txt"));
        let outcome = session.stage_files(&targets, true);
        let link = outcome
            .results
            .iter()
            .find(|result| result.path == "link.txt")
            .unwrap();
        assert!(!link.ok);
        assert!(link.skipped.is_some(), "只读文件应说明跳过原因");
        assert!(link.error.is_none());
        let a = outcome
            .results
            .iter()
            .find(|result| result.path == "a.txt")
            .unwrap();
        assert!(a.ok);
    }

    #[test]
    fn batch_stage_and_unstage_many_files_uses_constant_git_calls() {
        let (_dir, repo) = fixture();
        for index in 0..6 {
            let name = format!("tracked_{index}.txt");
            std::fs::write(repo.join(&name), format!("v1 {index}\n")).unwrap();
        }
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", "tracked"]);

        for index in 0..6 {
            std::fs::write(repo.join(format!("tracked_{index}.txt")), format!("v2 {index}\n"))
                .unwrap();
        }
        for index in 0..4 {
            std::fs::write(
                repo.join(format!("untracked_{index}.txt")),
                format!("new {index}\n"),
            )
            .unwrap();
        }

        let session = open(&repo);
        let snapshot = session.refresh().unwrap();
        let targets = targets(&snapshot);
        assert_eq!(targets.len(), 10);

        let before_stage = session.git().log.snapshot().len();
        let staged = session.stage_files(&targets, true);
        let stage_git_calls = session.git().log.snapshot().len() - before_stage;
        assert!(
            staged.results.iter().all(|result| result.ok),
            "{:?}",
            staged.results
        );
        assert!(
            stage_git_calls <= 12,
            "10 files batch stage spawned {stage_git_calls} git commands (expected constant O(1))"
        );
        let after_stage = staged.snapshot.unwrap();
        assert_eq!(after_stage.staged_count, 10);

        let unstage_targets: Vec<BatchTarget> = after_stage
            .entries
            .iter()
            .filter(|entry| entry.changes == ChangesKind::Staged)
            .map(|entry| BatchTarget {
                path: entry.path.clone(),
                changes: entry.changes,
                fingerprint: entry.fingerprint.clone(),
            })
            .collect();
        assert_eq!(unstage_targets.len(), 10);

        let before_unstage = session.git().log.snapshot().len();
        let unstaged = session.stage_files(&unstage_targets, false);
        let unstage_git_calls = session.git().log.snapshot().len() - before_unstage;
        assert!(
            unstaged.results.iter().all(|result| result.ok),
            "{:?}",
            unstaged.results
        );
        assert!(
            unstage_git_calls <= 12,
            "10 files batch unstage spawned {unstage_git_calls} git commands (expected constant O(1))"
        );
        assert_eq!(unstaged.snapshot.unwrap().staged_count, 0);
    }

    #[test]
    fn batch_stage_preserves_order_with_mixed_stale_and_untracked() {
        let (_dir, repo) = fixture();
        std::fs::write(repo.join("a.txt"), "a2\n").unwrap();
        std::fs::write(repo.join("b.txt"), "b2\n").unwrap();
        std::fs::write(repo.join("u1.txt"), "u1\n").unwrap();
        std::fs::write(repo.join("u2.txt"), "u2\n").unwrap();

        let session = open(&repo);
        let snapshot = session.refresh().unwrap();
        let mut batch_targets = targets(&snapshot);
        assert_eq!(batch_targets.len(), 4);
        let u1_pos = batch_targets
            .iter()
            .position(|target| target.path == "u1.txt")
            .unwrap();
        let a_pos = batch_targets
            .iter()
            .position(|target| target.path == "a.txt")
            .unwrap();
        batch_targets[u1_pos].fingerprint = "stale-untracked".into();
        batch_targets[a_pos].fingerprint = "stale-tracked".into();

        let expected_paths: Vec<String> =
            batch_targets.iter().map(|target| target.path.clone()).collect();
        let outcome = session.stage_files(&batch_targets, true);
        let actual_paths: Vec<String> = outcome
            .results
            .iter()
            .map(|result| result.path.clone())
            .collect();
        assert_eq!(actual_paths, expected_paths);
        assert!(!outcome.results[u1_pos].ok);
        assert!(!outcome.results[a_pos].ok);
        for (idx, result) in outcome.results.iter().enumerate() {
            if idx != u1_pos && idx != a_pos {
                assert!(result.ok, "unexpected failure at {idx}: {result:?}");
            }
        }
    }
}
