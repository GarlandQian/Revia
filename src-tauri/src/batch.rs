//! 多文件批量写操作（M4 F09）：按文件校验写前指纹，串行执行，返回逐个文件的结果。
//!
//! 边界与单项操作完全一致（ADR-0005、ADR-0009、安全边界）：
//! - 每个文件在真正写入前都重新读取真实状态并核对指纹；
//! - 丢弃时逐个文件先保存回收记录，再写工作区；
//! - 只读条目（符号链接、子模块、嵌套仓库、冲突等）不参与写入，结果里说明跳过原因；
//! - 部分失败不中断其余文件，逐项如实返回；不确定的结果标为「结果待核对」。
//!
//! 批量入口在整个批次期间只取一次同 worktree 写锁，避免与其它写操作交错。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, AppErrorPayload, ErrorKind};
use crate::git::status::{ChangesKind, ReadonlyReason};
use crate::session::{ChangesSnapshot, WorktreeSession};

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

impl WorktreeSession {
    /// 批量暂存 / 取消暂存：整个批次只取一次写锁，逐文件核对指纹并串行执行。
    pub fn stage_files(&self, targets: &[BatchTarget], is_stage: bool) -> BatchOutcome {
        let guard = match self.acquire_write_lock() {
            Ok(guard) => guard,
            Err(error) => return BatchOutcome::all_failed(targets, &error),
        };
        // 只读条目在真实状态里判定：前端可能把它们一并选中，这里说明跳过原因。
        let status = crate::git::status::read(self.git()).ok();
        let mut results = Vec::with_capacity(targets.len());
        let mut snapshot: Option<ChangesSnapshot> = None;
        let mut needs_verification = false;
        for target in targets {
            // 暂存接受未暂存与未跟踪两组；取消暂存只接受已暂存一组。
            let expected_group = if is_stage {
                [ChangesKind::Unstaged, ChangesKind::Untracked]
            } else {
                [ChangesKind::Staged, ChangesKind::Staged]
            };
            let entry = status.as_ref().and_then(|status| {
                status.entries.iter().find(|entry| {
                    entry.path == target.path && expected_group.contains(&entry.changes)
                })
            });
            match entry {
                None => {
                    let error = AppError::coded(
                        ErrorKind::Rejected,
                        "errors.batch.theFileChangedConfirmAgain",
                        "文件已变化，请重新确认",
                        &[],
                    );
                    needs_verification |= error.kind == ErrorKind::NeedsVerification;
                    results.push(BatchFileResult::failed(target, &error));
                    continue;
                }
                Some(entry) => {
                    if let Some(reason) = entry.readonly_reason {
                        results.push(BatchFileResult::skipped(target, reason));
                        continue;
                    }
                }
            }
            match self.execute_staging_op_locked(&target.path, &target.fingerprint, is_stage, None)
            {
                Ok(next) => {
                    snapshot = Some(next);
                    results.push(BatchFileResult::success(target));
                }
                Err(error) => {
                    needs_verification |= error.kind == ErrorKind::NeedsVerification;
                    results.push(BatchFileResult::failed(target, &error));
                }
            }
        }
        drop(guard);
        // 批次结束后重新读取一次，界面据此与真实状态一致；失败时保留最后一次成功快照。
        if let Ok(next) = self.refresh() {
            snapshot = Some(next);
        }
        BatchOutcome {
            results,
            snapshot,
            needs_verification,
        }
    }

    /// 批量丢弃：整个批次只取一次写锁；每个文件先保存回收记录再写工作区。
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
        let mut snapshot: Option<ChangesSnapshot> = None;
        let mut needs_verification = false;
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
                self.discard_untracked_locked(&target.path, &target.fingerprint, store)
            } else {
                self.discard_with_hook_locked(
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
                Ok(next) => {
                    snapshot = Some(next);
                    results.push(BatchFileResult::success(target));
                }
                Err(error) => {
                    needs_verification |= error.kind == ErrorKind::NeedsVerification;
                    results.push(BatchFileResult::failed(target, &error));
                }
            }
        }
        drop(guard);
        if let Ok(next) = self.refresh() {
            snapshot = Some(next);
        }
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
        let path: Vec<String> = std::env::var("PATH")
            .unwrap_or_default()
            .split(':')
            .map(str::to_string)
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
                shell: "/bin/zsh".into(),
                elapsed_ms: 0,
                timed_out: false,
                used_fallback: false,
                var_count: 2,
                path_entries: path,
                diagnostics: None,
            },
        }
    }

    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q"]);
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
}
