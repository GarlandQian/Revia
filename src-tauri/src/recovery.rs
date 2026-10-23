//! Durable full-file recovery records, independent of the user's Git repository.
//!
//! The saved state is immutable. A separate completion receipt is published only
//! after a successful write and an exact match with the precomputed result.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, atomic::{AtomicU64, Ordering}};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::error::{AppError, Result};
use crate::git::Git;
use crate::platform;

static NEXT_RECORD: AtomicU64 = AtomicU64::new(1);

/// 回收记录的保留期（Q9、规格实现决策 4：14 天）。
pub const RETENTION_MS: u64 = 14 * 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeIdentity {
    pub worktree_path: PathBuf,
    pub git_dir: PathBuf,
    pub common_dir: PathBuf,
    pub filesystem_ids: Vec<Option<String>>,
}

impl WorktreeIdentity {
    pub fn capture(git: &Git) -> Result<Self> {
        let read_path = |flag| -> Result<PathBuf> {
            let output = git.run_ok(&["rev-parse", "--path-format=absolute", flag], "读取 worktree 身份")?;
            Ok(fs::canonicalize(output.stdout_lossy().trim())?)
        };
        let worktree_path = fs::canonicalize(&git.directory)?;
        let git_dir = read_path("--git-dir")?;
        let common_dir = read_path("--git-common-dir")?;
        let filesystem_ids = [&worktree_path, &git_dir, &common_dir].into_iter()
            .map(|path| platform::directory_identity(path)).collect::<Result<_>>()?;
        Ok(Self { worktree_path, git_dir, common_dir, filesystem_ids })
    }

    pub fn confirmed_match(&self, other: &Self) -> bool {
        self.filesystem_ids.iter().all(Option::is_some) && self == other
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedFile {
    pub path: String,
    /// None is an absent path; an empty vector is a present empty file.
    pub content: Option<Vec<u8>>,
    pub executable: bool,
}

pub fn checked_path(root: &Path, path: &str) -> Result<PathBuf> {
    // 反斜杠只在 Windows 是路径分隔符；在 Unix 上它是合法的文件名字符，
    // 不能因为平台差异把普通文本文件误判为只读（路径仍按 `/` 分段并逐段校验）。
    let separator_bypass = cfg!(windows) && path.contains('\\');
    if path.is_empty() || separator_bypass || Path::new(path).components().any(|part| !matches!(part, Component::Normal(_)))
        || path.split('/').any(|part| part.is_empty() || part == "." || part == ".." || part.eq_ignore_ascii_case(".git"))
    {
        return Err(AppError::rejected("文件路径无法安全处理"));
    }
    let mut current = root.to_path_buf();
    for part in path.split('/') {
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => return Err(AppError::rejected("符号链接及其下的路径为只读类型")),
            Ok(_) => {},
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
            Err(error) => return Err(error.into()),
        }
    }
    Ok(current)
}

pub fn capture_files(root: &Path, paths: &[String]) -> Result<Vec<SavedFile>> {
    paths.iter().map(|path| {
        let full = checked_path(root, path)?;
        match fs::symlink_metadata(&full) {
            Ok(meta) if meta.is_file() => Ok(SavedFile { path: path.clone(), content: Some(fs::read(full)?), executable: platform::executable(&meta) }),
            Ok(_) => Err(AppError::rejected("目标不是普通文件，不能丢弃")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(SavedFile { path: path.clone(), content: None, executable: false }),
            Err(error) => Err(error.into()),
        }
    }).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RecoverySource { DiscardFile, DiscardHunk }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryRecord {
    pub id: String,
    pub created_at_ms: u64,
    pub source: RecoverySource,
    pub target: String,
    pub identity: WorktreeIdentity,
    pub before: Vec<SavedFile>,
    /// 保存时影响字节转换的有效上下文（属性 + 相关配置）指纹。
    /// 写入前必须复核；变化即拒绝，避免用旧上下文写入未选中的内容。
    #[serde(default)]
    pub context_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiscardOutcome {
    Completed { after: Vec<SavedFile> },
    NotApplied,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RecoveryStatus { Completed, NotApplied, NeedsVerification }

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RecoverySummary {
    pub id: String,
    pub created_at_ms: u64,
    pub source: RecoverySource,
    pub target: String,
    pub worktree_path: String,
    pub paths: Vec<String>,
    pub bytes: u64,
    pub status: RecoveryStatus,
    pub identity_confirmed: bool,
}


/// 一条记录当前的可用状态（Q29、Q85、Q87）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RecoveryState {
    /// 与丢弃完成后的状态一致：可以直接恢复。
    Ready,
    /// 之后又有了修改：只能只读对比 / 另存，不能覆盖。
    LaterChanges,
    /// 没有可信的「丢弃完成后」基线（待核对或未执行）。
    NoBaseline,
    /// 原 worktree 身份无法确认（移动、重建、其他 worktree）。
    IdentityMismatch,
}

/// 单个路径在某一侧的状态摘要。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RecoveryFileState {
    pub exists: bool,
    pub bytes: u64,
    pub executable: bool,
    /// 是否按二进制处理（含 NUL 或不是合法 UTF-8）。
    pub binary: bool,
    /// 文本内容，供只读对比；二进制为 None。
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RecoveryFileView {
    pub path: String,
    pub saved: RecoveryFileState,
    /// 当前工作区里的状态；原 worktree 不可用或该路径读取失败时为 None。
    pub current: Option<RecoveryFileState>,
    /// 当前侧是否无法读取（用于如实提示，不用保存内容冒充）。
    pub current_unavailable: bool,
}

/// 恢复前的完整检查结果：界面据此决定是否允许直接恢复。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RecoveryInspection {
    pub id: String,
    pub target: String,
    pub source: RecoverySource,
    pub created_at_ms: u64,
    pub worktree_path: String,
    pub paths: Vec<String>,
    pub state: RecoveryState,
    pub files: Vec<RecoveryFileView>,
}

fn file_state(file: &SavedFile) -> RecoveryFileState {
    match &file.content {
        None => RecoveryFileState { exists: false, bytes: 0, executable: false, binary: false, text: None },
        Some(bytes) => {
            let binary = bytes.contains(&0) || std::str::from_utf8(bytes).is_err();
            RecoveryFileState {
                exists: true,
                bytes: bytes.len() as u64,
                executable: file.executable,
                binary,
                text: if binary { None } else { Some(String::from_utf8_lossy(bytes).into_owned()) },
            }
        }
    }
}

/// 本次恢复创建的临时文件；只清理这些路径，绝不删除预先存在的用户文件。
struct TempFile {
    path: PathBuf,
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// 在目标文件同目录下独占创建一个唯一临时文件（Q80、ADR-0010）。
///
/// 不用固定后缀名：用户可能正好有同名文件。`create_new` 保证只有新建成功才继续，
/// 已存在的任何路径都不会被触碰，也不存在「先删除再创建」的窗口。
fn exclusive_temp_file(target: &Path) -> Result<(TempFile, File)> {
    let parent = target.parent().ok_or_else(|| AppError::rejected("目标路径无效"))?;
    for _ in 0..64 {
        let nonce = NEXT_RECORD.fetch_add(1, Ordering::Relaxed);
        let name = format!(
            ".revia-restore-{}-{}-{}",
            std::process::id(),
            (SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos()),
            nonce
        );
        let path = parent.join(name);
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
        match options.open(&path) {
            Ok(handle) => return Ok((TempFile { path }, handle)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(AppError::environment("无法创建恢复用临时文件"))
}

/// 恢复的单个路径：写入后立即返回本次创建/删除的路径，供阶段留证。
fn restore_one(worktree: &Path, file: &SavedFile) -> Result<()> {
    let full = checked_path(worktree, &file.path)?;
    match &file.content {
        Some(bytes) => {
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent)?;
            }
            let (temp, mut handle) = exclusive_temp_file(&full)?;
            handle.write_all(bytes)?;
            handle.sync_all()?;
            drop(handle);
            platform::set_executable(&temp.path, file.executable)?;
            // rename 成功时 temp 已不存在，Drop 中的清理是无害的空操作。
            fs::rename(&temp.path, &full)?;
            Ok(())
        }
        None => {
            match fs::symlink_metadata(&full) {
                // 保存时该路径不存在，恢复就是让它重新不存在。
                Ok(meta) if meta.is_file() => fs::remove_file(&full)?,
                Ok(_) => return Err(AppError::rejected("目标路径不是普通文件，不能恢复")),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            Ok(())
        }
    }
}

/// 把保存的工作区状态写回。调用方必须先完成身份与「无后续变化」校验。
///
/// 多路径按顺序写入；中途失败时已写入的路径保持新内容、其余保持原样，
/// 由调用方标为「结果待核对」，**不声称事务原子**。
fn restore_files(worktree: &Path, before: &[SavedFile], mut hook: impl FnMut(RestorePhase) -> Result<()>) -> Result<Vec<String>> {
    hook(RestorePhase::BeforeWrite)?;
    let mut written = Vec::with_capacity(before.len());
    for file in before {
        restore_one(worktree, file)?;
        written.push(file.path.clone());
    }
    hook(RestorePhase::AfterWrite)?;
    Ok(written)
}

#[derive(Clone)]
pub struct RecoveryStore {
    directory: PathBuf,
    clock: Arc<dyn Fn() -> u64 + Send + Sync>,
    /// 在途记录 ID → 持有者数量。清理与删除在**同一把锁内**完成检查与删除，
    /// 因此不存在「检查完释放锁再删」的 TOCTOU 窗口（Q91）。
    in_use: Arc<Mutex<HashMap<String, usize>>>,
}

/// 在途占用标记；离开作用域自动释放一个持有者。
///
/// 使用计数而非集合：同一记录被多个持有者保护时，任何一个提前释放都不会解除保护。
pub struct RecordUse {
    id: String,
    in_use: Arc<Mutex<HashMap<String, usize>>>,
}

impl Drop for RecordUse {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.in_use.lock() {
            match guard.get_mut(&self.id) {
                Some(count) if *count > 1 => *count -= 1,
                Some(_) => { guard.remove(&self.id); }
                None => {}
            }
        }
    }
}

impl RecoveryStore {
    pub fn new(data_directory: PathBuf) -> Self {
        Self::with_clock(data_directory, Arc::new(|| SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64))
    }

    pub fn with_clock(data_directory: PathBuf, clock: Arc<dyn Fn() -> u64 + Send + Sync>) -> Self {
        Self { directory: data_directory.join("recovery"), clock, in_use: Arc::new(Mutex::new(HashMap::new())) }
    }

    /// 标记某条记录正在被使用（保存 / 恢复 / 另存），清理会跳过它。
    pub fn use_record(&self, id: &str) -> RecordUse {
        if let Ok(mut guard) = self.in_use.lock() {
            *guard.entry(id.to_string()).or_insert(0) += 1;
        }
        RecordUse { id: id.to_string(), in_use: self.in_use.clone() }
    }

    /// 登记占用（用于尚未发布、只知道目录名的保存中记录）。
    fn use_directory(&self, directory_name: &str) -> RecordUse {
        self.use_record(directory_name)
    }

    fn allocate(&self, prefix: &str) -> Result<PathBuf> {
        fs::create_dir_all(&self.directory)?;
        for _ in 0..100 {
            let id = format!("{prefix}-{}-{}-{}", (self.clock)(), std::process::id(), NEXT_RECORD.fetch_add(1, Ordering::Relaxed));
            let directory = self.directory.join(id);
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            { use std::os::unix::fs::DirBuilderExt; builder.mode(0o700); }
            match builder.create(&directory) {
                Ok(()) => return Ok(directory),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(AppError::environment("无法分配回收记录目录"))
    }

    pub fn ensure_outside(&self, identity: &WorktreeIdentity) -> Result<()> {
        let mut parent = self.directory.as_path();
        let mut missing = Vec::new();
        while !parent.exists() {
            missing.push(parent.file_name().ok_or_else(|| AppError::environment("回收区路径无效"))?);
            parent = parent.parent().ok_or_else(|| AppError::environment("回收区路径无效"))?;
        }
        let mut directory = fs::canonicalize(parent)?;
        for name in missing.into_iter().rev() { directory.push(name); }
        if [&identity.worktree_path, &identity.git_dir, &identity.common_dir].iter().any(|root| directory.starts_with(root)) {
            return Err(AppError::rejected("回收区不能位于用户仓库中"));
        }
        Ok(())
    }

    /// 保存完整记录，并返回覆盖「保存 → 写入 → 结果记录」全过程的占用标记。
    ///
    /// 占用在记录目录创建后、发布前登记：清理只认 `record-*`，但仍提前占位，
    /// 保证发布后立刻发生的清理不会删掉这条唯一的备份（Q91）。
    pub fn save(&self, identity: WorktreeIdentity, source: RecoverySource, target: &str, before: Vec<SavedFile>, context_fingerprint: String) -> Result<(RecoveryRecord, RecordUse)> {
        self.ensure_outside(&identity)?;
        let directory = self.allocate("saving")?;
        let id = directory.file_name().unwrap().to_string_lossy().replacen("saving-", "record-", 1);
        let guard = self.use_record(&id);
        let record = RecoveryRecord { id: id.clone(), created_at_ms: (self.clock)(), source, target: target.into(), identity, before, context_fingerprint };
        write_new(&directory.join("saved.json"), &record)?;
        platform::sync_directory(&directory)?;
        fs::rename(&directory, self.directory.join(&record.id))?;
        platform::sync_directory(&self.directory)?;
        if let Some(parent) = self.directory.parent() { platform::sync_directory(parent)?; }
        Ok((record, guard))
    }

    /// 在**同一把锁内**判断并删除一条记录，避免检查与删除之间的竞态。
    fn remove_if_unused(&self, id: &str, path: &Path) -> Result<bool> {
        let guard = self
            .in_use
            .lock()
            .map_err(|_| AppError::internal("回收区占用锁失败"))?;
        if guard.get(id).copied().unwrap_or(0) > 0 {
            return Ok(false);
        }
        fs::remove_dir_all(path)?;
        Ok(true)
    }

    pub fn finish(&self, record: &RecoveryRecord, outcome: &DiscardOutcome) -> Result<()> {
        let directory = self.record_directory(&record.id)?;
        // A partial receipt after a crash must never look like a completed write.
        write_new(&directory.join("outcome.pending"), outcome)?;
        fs::rename(directory.join("outcome.pending"), directory.join("outcome.json"))?;
        platform::sync_directory(&directory)
    }

    fn record_directory(&self, id: &str) -> Result<PathBuf> {
        if !id.starts_with("record-") || !id.bytes().all(|value| value.is_ascii_alphanumeric() || value == b'-') {
            return Err(AppError::rejected("回收记录 ID 无效"));
        }
        checked_path(&self.directory, id)
    }

    pub fn read(&self, id: &str) -> Result<(RecoveryRecord, Option<DiscardOutcome>)> {
        let directory = self.record_directory(id)?;
        let record = serde_json::from_slice(&fs::read(directory.join("saved.json"))?)?;
        let outcome = match fs::read(directory.join("outcome.json")) {
            Ok(bytes) => Some(serde_json::from_slice(&bytes)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        Ok((record, outcome))
    }

    pub fn list(&self, current: Option<&WorktreeIdentity>) -> Result<Vec<RecoverySummary>> {
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut records = Vec::new();
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with("record-") { continue; }
            let (record, outcome) = match self.read(&name) {
                Ok(value) => value,
                Err(error) => return Err(AppError::environment(format!("回收记录 {name} 无法读取：{error}"))),
            };
            records.push(RecoverySummary {
                id: record.id, created_at_ms: record.created_at_ms, source: record.source, target: record.target,
                worktree_path: record.identity.worktree_path.display().to_string(),
                paths: record.before.iter().map(|file| file.path.clone()).collect(),
                bytes: record.before.iter().filter_map(|file| file.content.as_ref()).map(|bytes| bytes.len() as u64).sum(),
                status: match outcome { Some(DiscardOutcome::Completed { .. }) => RecoveryStatus::Completed, Some(DiscardOutcome::NotApplied) => RecoveryStatus::NotApplied, None => RecoveryStatus::NeedsVerification },
                identity_confirmed: current.is_some_and(|current| record.identity.confirmed_match(current)),
            });
        }
        records.sort_by(|a, b| b.created_at_ms.cmp(&a.created_at_ms).then_with(|| b.id.cmp(&a.id)));
        Ok(records)
    }


    /// 检查一条记录是否可以直接恢复，并给出只读对比所需的两侧状态。
    ///
    /// 判定基准是**丢弃完成后**的状态（`outcome.after`），不是保存的 `before`（Q29、ADR-0010）。
    pub fn inspect(
        &self,
        id: &str,
        identity: Option<&WorktreeIdentity>,
        worktree: Option<&Path>,
    ) -> Result<RecoveryInspection> {
        let (record, outcome) = self.read(id)?;
        let identity_matches = identity.is_some_and(|current| record.identity.confirmed_match(current));

        let after = match &outcome {
            Some(DiscardOutcome::Completed { after }) => Some(after),
            _ => None,
        };

        let mut files = Vec::with_capacity(record.before.len());
        let mut current_matches_after = after.is_some();
        for saved in &record.before {
            // 当前侧不可用时**不**用保存内容冒充：明确标为不可读（Q87）。
            let current: Option<SavedFile> = match worktree {
                Some(root) => match capture_files(root, std::slice::from_ref(&saved.path)) {
                    Ok(captured) => captured.into_iter().next(),
                    Err(_) => None,
                },
                None => None,
            };
            match (&current, after) {
                (Some(current), Some(after)) => {
                    if after.iter().find(|file| file.path == saved.path) != Some(current) {
                        current_matches_after = false;
                    }
                }
                _ => current_matches_after = false,
            }
            files.push(RecoveryFileView {
                path: saved.path.clone(),
                saved: file_state(saved),
                current: current.as_ref().map(file_state),
                current_unavailable: current.is_none(),
            });
        }

        let any_current_unavailable = files.iter().any(|file| file.current_unavailable);
        let state = if !identity_matches || any_current_unavailable {
            RecoveryState::IdentityMismatch
        } else if after.is_none() {
            RecoveryState::NoBaseline
        } else if !current_matches_after {
            RecoveryState::LaterChanges
        } else {
            RecoveryState::Ready
        };

        Ok(RecoveryInspection {
            id: record.id.clone(),
            target: record.target.clone(),
            source: record.source.clone(),
            created_at_ms: record.created_at_ms,
            worktree_path: record.identity.worktree_path.display().to_string(),
            paths: record.before.iter().map(|file| file.path.clone()).collect(),
            state,
            files,
        })
    }

    /// 把保存的完整工作区状态写回。只写工作区，不碰暂存区、不重放反向补丁。
    ///
    /// `expected` 来自用户确认时看到的那一次检查（含记录、相关路径、状态与所见版本）；
    /// 期间记录内容、身份或当前状态变化都会使这次恢复失效（Q89、Q91）。
    pub fn restore(
        &self,
        id: &str,
        identity: &WorktreeIdentity,
        worktree: &Path,
        expected: &RestoreExpectation,
    ) -> Result<RecoveryInspection> {
        self.restore_with_hook(id, identity, worktree, expected, |_| Ok(()))
    }

    /// 带受控阶段接缝的恢复；正常调用传入空钩子。
    pub fn restore_with_hook(
        &self,
        id: &str,
        identity: &WorktreeIdentity,
        worktree: &Path,
        expected: &RestoreExpectation,
        mut hook: impl FnMut(RestorePhase) -> Result<()>,
    ) -> Result<RecoveryInspection> {
        let _in_use = self.use_record(id);
        let inspection = self.inspect(id, Some(identity), Some(worktree))?;
        if inspection.paths != expected.paths {
            return Err(AppError::rejected("回收记录的相关路径已变化，请重新确认"));
        }
        if inspection.state != expected.state {
            return Err(AppError::rejected("回收记录状态已变化，请重新确认"));
        }
        match inspection.state {
            RecoveryState::Ready => {}
            RecoveryState::IdentityMismatch => {
                return Err(AppError::rejected("原 worktree 身份无法确认，不能直接恢复到新目录"))
            }
            RecoveryState::NoBaseline => {
                return Err(AppError::rejected("这条记录没有可信的丢弃完成状态，结果待核对，不能直接恢复"))
            }
            RecoveryState::LaterChanges => {
                return Err(AppError::rejected("文件在丢弃之后又有修改，不能覆盖；请改用只读对比或另存"))
            }
        }

        // 恢复自身的结果留证：先落一份 pending，写入开始前就存在。
        let journal = self.begin_restore_journal(id)?;
        let (record, _) = self.read(id)?;
        // 写入前最后一次核对真实身份与相关字节（防止检查与写入之间被外部改动）。
        let refreshed = self.inspect(id, Some(identity), Some(worktree))?;
        if refreshed.state != RecoveryState::Ready || refreshed.paths != expected.paths {
            let _ = self.write_restore_journal(&journal, RestoreJournalState::Rejected, &[]);
            return Err(AppError::rejected("恢复前复核失败，文件或身份已变化，请重新确认"));
        }

        let written = match restore_files(worktree, &record.before, &mut hook) {
            Ok(written) => written,
            Err(error) => {
                // 可能已经写入了一部分：保留记录并标为待核对，不自动续跑。
                let _ = self.write_restore_journal(&journal, RestoreJournalState::NeedsVerification, &[]);
                return Err(AppError::needs_verification(format!(
                    "恢复过程中断，可能已写入部分路径，结果待核对；请重新读取真实状态：{error}"
                )));
            }
        };
        hook(RestorePhase::BeforeComplete)?;
        self.write_restore_journal(&journal, RestoreJournalState::Completed, &written)?;
        // 返回恢复后的真实状态，供界面确认。
        self.inspect(id, Some(identity), Some(worktree))
    }

    fn begin_restore_journal(&self, id: &str) -> Result<PathBuf> {
        let directory = self.record_directory(id)?;
        let path = directory.join("restore.pending");
        let journal = RestoreJournal {
            state: RestoreJournalState::Pending,
            written: Vec::new(),
            at_ms: (self.clock)(),
        };
        write_new(&path, &journal)?;
        platform::sync_directory(&directory)?;
        Ok(path)
    }

    fn write_restore_journal(&self, path: &Path, state: RestoreJournalState, written: &[String]) -> Result<()> {
        let journal = RestoreJournal { state, written: written.to_vec(), at_ms: (self.clock)() };
        // 原子替换，避免读到半份回执。
        let temp = path.with_extension("next");
        let _ = fs::remove_file(&temp);
        write_new(&temp, &journal)?;
        fs::rename(&temp, path)?;
        if let Some(parent) = path.parent() {
            platform::sync_directory(parent)?;
        }
        Ok(())
    }

    /// 读取恢复留证（供查看与「重启后仍待核对」）。
    pub fn restore_journal(&self, id: &str) -> Result<Option<RestoreJournal>> {
        let directory = self.record_directory(id)?;
        match fs::read(directory.join("restore.pending")) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// 生成「保存版本 vs 当前内容」的只读 diff（文本）。
    ///
    /// 由 Rust 侧调用真实 Git（`--no-index`）产出，前端只渲染；
    /// 两个输入都写在回收区自己的临时目录里，不在用户仓库里创建文件。
    pub fn text_diff(&self, git: &Git, id: &str, path: &str, worktree: Option<&Path>) -> Result<String> {
        let (record, _) = self.read(id)?;
        let saved = record
            .before
            .iter()
            .find(|file| file.path == path)
            .ok_or_else(|| AppError::rejected("该记录不包含这个路径"))?;
        let saved_bytes = saved
            .content
            .as_ref()
            .ok_or_else(|| AppError::rejected("这条记录里该路径不存在内容，无法对比"))?;
        if saved_bytes.contains(&0) || std::str::from_utf8(saved_bytes).is_err() {
            return Err(AppError::rejected("二进制内容不提供文本对比"));
        }
        let worktree = worktree.ok_or_else(|| AppError::rejected("当前 worktree 不可用，无法生成对比"))?;
        let current = capture_files(worktree, std::slice::from_ref(&path.to_string()))?
            .into_iter()
            .next()
            .ok_or_else(|| AppError::rejected("读取当前内容失败"))?;
        let current_bytes = current
            .content
            .ok_or_else(|| AppError::rejected("当前路径不存在，无法生成文本对比"))?;
        if current_bytes.contains(&0) || std::str::from_utf8(&current_bytes).is_err() {
            return Err(AppError::rejected("二进制内容不提供文本对比"));
        }

        let scratch = self.allocate("diff")?;
        let result = (|| {
            let old_path = scratch.join("saved");
            let new_path = scratch.join("current");
            fs::write(&old_path, saved_bytes)?;
            fs::write(&new_path, &current_bytes)?;
            let bytes = git
                .in_isolated_directory(&scratch)
                .diff_no_index(&old_path, &new_path)?;
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        })();
        let _ = fs::remove_dir_all(scratch);
        result
    }

    /// 把保存版本另存到用户选择的新路径；目标必须不存在（含符号链接等）。
    pub fn save_as(&self, id: &str, path: &str, destination: &Path) -> Result<u64> {
        let _in_use = self.use_record(id);
        let (record, _) = self.read(id)?;
        let saved = record
            .before
            .iter()
            .find(|file| file.path == path)
            .ok_or_else(|| AppError::rejected("该记录不包含这个路径"))?;
        let bytes = saved
            .content
            .as_ref()
            .ok_or_else(|| AppError::rejected("这条记录里该路径不存在内容，无法另存"))?;

        // 目标必须是一个正常的外部路径：祖先链不得含符号链接、不得落在 Git 元数据里。
        // 只校验目标本身不存在（create_new 保证），不因目标已存在而先截断。
        let name = destination
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| AppError::rejected("另存路径包含无法无损处理的字符"))?;
        if name.is_empty() || name == "." || name == ".." || name.eq_ignore_ascii_case(".git") {
            return Err(AppError::rejected("另存路径无效"));
        }
        let parent = destination
            .parent()
            .ok_or_else(|| AppError::rejected("另存目录无效"))?;
        if !parent.is_dir() {
            return Err(AppError::rejected("另存目录不存在"));
        }
        // 逐段核对**原始路径**的每一级祖先：任一级是符号链接就拒绝（绝不穿透到别的目录写入）。
        // 这里不做 canonicalize 后比较：一旦规范化就丢掉了「这一级是链接」的证据。
        // 目标本身不要求存在，因此只检查它的上级；create_new 保证目标不会被截断或替换。
        let mut cursor = PathBuf::new();
        for component in parent.components() {
            cursor.push(component);
            if let std::path::Component::Normal(name) = component {
                if name.to_string_lossy().eq_ignore_ascii_case(".git") {
                    return Err(AppError::rejected("不能写入 Git 元数据目录"));
                }
            }
            match fs::symlink_metadata(&cursor) {
                Ok(meta) if meta.file_type().is_symlink() => {
                    return Err(AppError::rejected("另存路径的上级目录是符号链接，请选择真实目录"));
                }
                Ok(meta) if meta.is_dir() => {}
                Ok(_) => return Err(AppError::rejected("另存路径的上级不是目录")),
                Err(_) => return Err(AppError::rejected("另存路径的上级目录不可访问")),
            }
        }

        // 用 create_new 一步完成「不存在才创建」：不会先截断再发现已存在。
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
        let mut handle = match options.open(destination) {
            Ok(handle) => handle,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(AppError::rejected("目标路径已存在，请选择其他路径"));
            }
            Err(error) => return Err(error.into()),
        };
        handle.write_all(bytes)?;
        handle.sync_all()?;
        platform::set_executable(destination, saved.executable)?;
        Ok(bytes.len() as u64)
    }

    /// 删除超过保留期的记录；返回删除数量。
    ///
    /// 在途保存 / 恢复占用的记录一律跳过；删除失败不静默。
    pub fn purge_expired(&self) -> Result<u64> {
        let now = (self.clock)();
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(error.into()),
        };
        let mut removed = 0;
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with("record-") {
                continue;
            }
            let (record, _) = match self.read(&name) {
                Ok(value) => value,
                // 读不出来的记录不删：宁可保留可见，也不误删未知数据。
                Err(_) => continue,
            };
            if now.saturating_sub(record.created_at_ms) < RETENTION_MS {
                continue;
            }
            if self.remove_if_unused(&name, &entry.path())? {
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// 清空回收区（用户明确确认后调用）。在途记录保留。
    pub fn clear(&self) -> Result<u64> {
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(error.into()),
        };
        let mut removed = 0;
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with("record-") {
                continue;
            }
            if self.remove_if_unused(&name, &entry.path())? {
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// Compute the exact expected result with the same Git patch, away from the worktree.
    ///
    /// `git apply` 只在仓库内执行工作区转换（`core.autocrlf`、`core.eol`、`.gitattributes`、
    /// `info/attributes`、`core.attributesFile`、`filter.*`）。预览必须在与真实仓库**等价**的
    /// 上下文里运行，否则会误拒正常文本，或算出与真实写入不同的字节。
    pub fn preview(&self, git: &Git, worktree: &Path, paths: &[String], before: &[SavedFile], patch: &[u8]) -> Result<Vec<SavedFile>> {
        let scratch = self.allocate("preview")?;
        let result = (|| {
            prepare_scratch(git, worktree, &scratch, paths)?;
            for file in before {
                let path = checked_path(&scratch, &file.path)?;
                if let Some(bytes) = &file.content {
                    fs::create_dir_all(path.parent().unwrap())?;
                    fs::write(&path, bytes)?;
                    platform::set_executable(&path, file.executable)?;
                }
            }
            let isolated = git.in_isolated_directory(&scratch);
            isolated.apply_worktree_patch(patch.to_vec())?.into_success("校验丢弃补丁")?;
            capture_files(&scratch, &before.iter().map(|file| file.path.clone()).collect::<Vec<_>>())
        })();
        let _ = fs::remove_dir_all(scratch);
        result
    }
}

/// 保存时记录的有效转换上下文指纹；写入前复核，变化即拒绝。
pub fn context_fingerprint(git: &Git, paths: &[String]) -> Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(b"ctx-v1");
    for key in CONTEXT_CONFIG_KEYS {
        hasher.update(key.as_bytes());
        match git.config_get(key) {
            Some(value) => hasher.update(value.as_bytes()),
            None => hasher.update(b"<unset>"),
        }
        hasher.update([0]);
    }
    for (key, value) in git.config_entries("filter.")? {
        hasher.update(key.as_bytes());
        hasher.update([0]);
        hasher.update((value.len() as u64).to_le_bytes());
        hasher.update(value.as_bytes());
        hasher.update([0]);
    }
    for path in paths {
        hasher.update(path.as_bytes());
        hasher.update([0]);
        hasher.update(&check_attr(git, path)?);
        hasher.update([0]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// 这些配置直接决定 index 与工作区之间的字节转换。
const CONTEXT_CONFIG_KEYS: &[&str] = &[
    "core.autocrlf",
    "core.eol",
    "core.safecrlf",
    "core.attributesFile",
    "core.ignorecase",
    "core.filemode",
];

/// 该路径的**有效属性**（Git 自己按全部来源与优先级算出）。
fn check_attr(git: &Git, path: &str) -> Result<Vec<u8>> {
    Ok(git
        .run_ok(&["check-attr", "-a", "-z", "--", path], "读取有效属性")?
        .stdout)
}

/// 在隔离目录里重建与真实仓库等价的属性与转换上下文。
fn prepare_scratch(git: &Git, worktree: &Path, scratch: &Path, paths: &[String]) -> Result<()> {
    let isolated = git.in_isolated_directory(scratch);
    isolated.run_ok(&["init", "-q"], "初始化预览仓库")?;

    // 1. 仓库级换行/属性配置（全局配置由同一份 HOME/GIT_CONFIG_GLOBAL 自然继承）。
    for key in CONTEXT_CONFIG_KEYS {
        if let Some(value) = git.config_get(key) {
            isolated.run_ok(&["config", key, &value], "复制预览仓库配置")?;
        }
    }
    // 2. 本地 filter.* 全部子键：转换是否发生、如何发生都由它决定。
    //    值可能是多行 shell 命令，必须原样作为单个参数传递。
    for (key, value) in git.config_entries("filter.")? {
        isolated.run_ok(&["config", "--add", &key, &value], "复制预览 filter 配置")?;
    }
    // 3. 工作区 `.gitattributes` 链（根到目标父目录）。
    let mut attribute_dirs: Vec<PathBuf> = Vec::new();
    for path in paths {
        copy_attribute_chain(worktree, scratch, path)?;
        if let Some(parent) = Path::new(path).parent() {
            let mut prefix = PathBuf::new();
            for component in parent.components() {
                if let Component::Normal(name) = component {
                    prefix.push(name);
                    if !attribute_dirs.contains(&prefix) {
                        attribute_dirs.push(prefix.clone());
                    }
                }
            }
        }
    }
    // 4. 该仓库实际生效的 info/attributes。
    //    linked worktree 的私有 gitDir 下没有 info/，Git 用的是 commonDir 下那份；
    //    这里交给 Git 自己解析，不猜目录布局。返回值可能是相对工作目录的路径。
    let info_attributes = git.git_path("info/attributes").map(|path| {
        if path.is_absolute() { path } else { worktree.join(path) }
    });
    if let Some(source) = info_attributes.as_ref().filter(|path| path.is_file()) {
        let destination = scratch.join(".git").join("info");
        fs::create_dir_all(&destination)?;
        fs::copy(source, destination.join("attributes"))?;
    }
    // 5. 仓库级 core.attributesFile：复制内容并把配置指到副本，避免预览读到会变化的原件。
    if let Some(configured) = git.config_get("core.attributesFile") {
        let source = {
            let raw = PathBuf::from(&configured);
            if raw.is_absolute() { raw } else { worktree.join(raw) }
        };
        if source.is_file() {
            let copy = scratch.join(".attributes-file");
            fs::copy(&source, &copy)?;
            isolated.run_ok(
                &["config", "core.attributesFile", &copy.display().to_string()],
                "复制预览属性文件",
            )?;
        }
    }
    // 6. 直接核对：预览上下文必须与真实仓库算出完全相同的有效属性。
    for path in paths {
        if check_attr(git, path)? != check_attr(&isolated, path)? {
            return Err(AppError::rejected(
                "无法等价复现该文件的 Git 属性上下文，已停止丢弃（不会修改工作区）",
            ));
        }
    }
    Ok(())
}

/// 把目标路径经过的每一级 `.gitattributes` 复制到预览仓库，保持属性语义一致。
fn copy_attribute_chain(worktree: &Path, scratch: &Path, relative: &str) -> Result<()> {
    let parent = match Path::new(relative).parent() {
        Some(parent) => parent.to_path_buf(),
        None => return Ok(()),
    };
    let mut prefix = PathBuf::new();
    for component in parent.components() {
        if let Component::Normal(name) = component {
            prefix.push(name);
            let source = worktree.join(&prefix).join(".gitattributes");
            if source.is_file() {
                let destination = scratch.join(&prefix).join(".gitattributes");
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&source, &destination)?;
            }
        }
    }
    let root_attributes = worktree.join(".gitattributes");
    if root_attributes.is_file() && !scratch.join(".gitattributes").exists() {
        fs::copy(&root_attributes, scratch.join(".gitattributes"))?;
    }
    Ok(())
}

fn write_new(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
    let mut file: File = options.open(path)?;
    file.write_all(&serde_json::to_vec(value)?)?;
    file.sync_all()?;
    Ok(())
}

/// 用户确认恢复时看到的内容；任一项变化都使确认失效。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreExpectation {
    pub paths: Vec<String>,
    pub state: RecoveryState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RestoreJournalState { Pending, Completed, NeedsVerification, Rejected }

/// 恢复自身的阶段结果（与丢弃的 Completed 回执相互独立）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreJournal {
    pub state: RestoreJournalState,
    pub written: Vec<String>,
    pub at_ms: u64,
}

/// 恢复的阶段，供受控中断接缝与结果留证使用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestorePhase { BeforeWrite, AfterWrite, BeforeComplete }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscardPhase { BeforeSave, AfterSave, BeforeWrite, AfterWrite, BeforeComplete }

pub fn unchanged(before: &[SavedFile], after: &[SavedFile]) -> bool {
    // Comparing actual bytes also catches clean filters that hide worktree changes from Git.
    before == after
}

