//! Durable full-file recovery records, independent of the user's Git repository.
//!
//! The saved state is immutable. A separate completion receipt is published only
//! after a successful write and an exact match with the precomputed result.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, Result};
use crate::git::Git;
use crate::platform;

static NEXT_RECORD: AtomicU64 = AtomicU64::new(1);

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

#[derive(Clone)]
pub struct RecoveryStore {
    directory: PathBuf,
    clock: Arc<dyn Fn() -> u64 + Send + Sync>,
}

impl RecoveryStore {
    pub fn new(data_directory: PathBuf) -> Self {
        Self::with_clock(data_directory, Arc::new(|| SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64))
    }

    pub fn with_clock(data_directory: PathBuf, clock: Arc<dyn Fn() -> u64 + Send + Sync>) -> Self {
        Self { directory: data_directory.join("recovery"), clock }
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

    pub fn save(&self, identity: WorktreeIdentity, source: RecoverySource, target: &str, before: Vec<SavedFile>) -> Result<RecoveryRecord> {
        self.ensure_outside(&identity)?;
        let directory = self.allocate("saving")?;
        let id = directory.file_name().unwrap().to_string_lossy().replacen("saving-", "record-", 1);
        let record = RecoveryRecord { id, created_at_ms: (self.clock)(), source, target: target.into(), identity, before };
        write_new(&directory.join("saved.json"), &record)?;
        platform::sync_directory(&directory)?;
        fs::rename(&directory, self.directory.join(&record.id))?;
        platform::sync_directory(&self.directory)?;
        if let Some(parent) = self.directory.parent() { platform::sync_directory(parent)?; }
        Ok(record)
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

    /// Compute the exact expected result with the same Git patch, away from the worktree.
    ///
    /// `git apply` runs the working-tree conversion (`core.autocrlf`, `core.eol`,
    /// `.gitattributes` filters) **only inside a repository**. Previewing outside one
    /// would either fail on `eol=crlf` text or silently produce different bytes, so the
    /// scratch is a throw-away repository carrying the same attributes and newline config.
    pub fn preview(&self, git: &Git, worktree: &Path, before: &[SavedFile], patch: &[u8]) -> Result<Vec<SavedFile>> {
        let scratch = self.allocate("preview")?;
        let result = (|| {
            init_scratch_repository(&scratch, git)?;
            for file in before {
                copy_attribute_chain(worktree, &scratch, &file.path)?;
                let path = checked_path(&scratch, &file.path)?;
                if let Some(bytes) = &file.content {
                    fs::create_dir_all(path.parent().unwrap())?;
                    fs::write(&path, bytes)?;
                    platform::set_executable(&path, file.executable)?;
                }
            }
            let scratch_git = git.in_directory(&scratch);
            scratch_git.apply_worktree_patch(patch.to_vec())?.into_success("校验丢弃补丁")?;
            capture_files(&scratch, &before.iter().map(|file| file.path.clone()).collect::<Vec<_>>())
        })();
        let _ = fs::remove_dir_all(scratch);
        result
    }
}

/// 在预览目录里建立一个丢弃用的临时仓库，并沿用真实仓库的换行相关配置。
///
/// 仓库级配置（例如 `core.autocrlf=true`）不会随全局配置继承，必须显式复制，
/// 否则预览写入的字节会与真实 worktree 的结果不一致。
fn init_scratch_repository(scratch: &Path, git: &Git) -> Result<()> {
    git.in_directory(scratch).run_ok(&["init", "-q"], "初始化预览仓库")?;
    for key in ["core.autocrlf", "core.eol", "core.safecrlf", "core.ignorecase", "core.filemode"] {
        if let Some(value) = git.config_get(key) {
            git.in_directory(scratch)
                .run_ok(&["config", key, &value], "复制预览仓库配置")?;
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
    // 根目录与每一级父目录都可能定义属性；逐级复制，后级覆盖前级。
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
    // 根级属性即使父目录为空也要复制。
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscardPhase { BeforeSave, AfterSave, BeforeWrite, AfterWrite, BeforeComplete }

pub fn unchanged(before: &[SavedFile], after: &[SavedFile]) -> bool {
    // Comparing actual bytes also catches clean filters that hide worktree changes from Git.
    before == after
}

