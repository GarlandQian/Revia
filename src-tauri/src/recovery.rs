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
    /// 最近一次恢复尝试（没有则为 None）。重启后仍从这里如实展示，不用丢弃回执冒充。
    pub restore: Option<RestoreAttempt>,
    /// 记录当前内容版本；确认恢复时绑定它，任何记录/回执/尝试变化都会使旧确认失效。
    pub version: String,
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
    /// 最近一次恢复尝试（含历史中最新状态），供详情与重启后展示。
    pub restore: Option<RestoreAttempt>,
    pub version: String,
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

/// 准备阶段的产物：临时文件已经写好并 fsync，但**尚未发布**到工作区。
struct PreparedFile {
    path: String,
    full: PathBuf,
    /// Some：用临时文件原子替换；None：该路径原本不存在，发布就是删除。
    temp: Option<(TempFile, File)>,
    executable: bool,
    remove: bool,
}

impl PreparedFile {
    /// 发布：一次 rename（或删除）。失败时 TempFile 的 Drop 只清理本次创建的文件。
    fn publish(&mut self) -> Result<()> {
        if self.remove {
            match fs::symlink_metadata(&self.full) {
                Ok(meta) if meta.is_file() => fs::remove_file(&self.full)?,
                Ok(_) => return Err(AppError::rejected("目标路径不是普通文件，不能恢复")),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            return Ok(());
        }
        let (temp, handle) = self
            .temp
            .take()
            .ok_or_else(|| AppError::internal("恢复准备内容已失效"))?;
        drop(handle);
        platform::set_executable(&temp.path, self.executable)?;
        fs::rename(&temp.path, &self.full)?;
        Ok(())
    }
}

/// 准备：为每个路径创建并写好临时文件，**不触碰工作区**。
///
/// 准备失败时工作区一个字节都没有改动，因此可以如实标为「未写入」。
fn prepare_files(worktree: &Path, before: &[SavedFile]) -> Result<Vec<PreparedFile>> {
    let mut prepared = Vec::with_capacity(before.len());
    for file in before {
        let full = checked_path(worktree, &file.path)?;
        match &file.content {
            Some(bytes) => {
                if let Some(parent) = full.parent() {
                    fs::create_dir_all(parent)?;
                }
                let (temp, mut handle) = exclusive_temp_file(&full)?;
                handle.write_all(bytes)?;
                handle.sync_all()?;
                prepared.push(PreparedFile {
                    path: file.path.clone(),
                    full,
                    temp: Some((temp, handle)),
                    executable: file.executable,
                    remove: false,
                });
            }
            None => prepared.push(PreparedFile {
                path: file.path.clone(),
                full,
                temp: None,
                executable: false,
                remove: true,
            }),
        }
    }
    Ok(prepared)
}

/// 发布前校验：用**刚刚重新捕获**的真实状态判断这次发布是否仍然安全。
///
/// 检查的是**全部相关路径**，不是只看下一个要发布的那条（Q80、Q85、Q91）：
/// - 身份必须与记录一致（不拿传入的旧 identity 充数）；
/// - 任何相关路径处于未解决冲突都拒绝（Q82）；
/// - 已经发布的路径必须等于这次恢复写入的 `saved`；
/// - 尚未发布的路径（含下一个）必须仍等于丢弃完成后的 `after`。
fn verify_publish(
    pre: &RestorePreconditions,
    record: &RecoveryRecord,
    after: &[SavedFile],
    written: &[String],
) -> Result<()> {
    if !record.identity.confirmed_match(&pre.identity) {
        return Err(AppError::rejected("原 worktree 身份无法确认，不能直接恢复到新目录"));
    }
    for saved in &record.before {
        if pre.conflicts.iter().any(|path| path == &saved.path) {
            return Err(AppError::rejected("文件存在未解决的冲突，不能从回收区恢复"));
        }
    }
    for saved in &record.before {
        let observed = pre
            .files
            .iter()
            .find(|file| file.path == saved.path)
            .ok_or_else(|| AppError::rejected("无法读取目标文件的真实状态"))?;
        if written.iter().any(|path| path == &saved.path) {
            // 已经发布：必须等于本次恢复写入的内容。
            if observed != saved {
                return Err(AppError::rejected("恢复期间文件被外部改动，请重新确认"));
            }
        } else {
            // 尚未发布：必须仍等于丢弃完成后的基线。
            let baseline = after.iter().find(|file| file.path == saved.path);
            if baseline != Some(observed) {
                return Err(AppError::rejected("文件在丢弃之后又有修改，不能覆盖"));
            }
        }
    }
    Ok(())
}

/// 目标路径上方**所有** Git 仓库的真实元数据目录。
///
/// 按 `.git` 目录 / `.git` 文件指针（linked worktree、`--separate-git-dir`）解析，
/// 并读出 `commondir` 指向的共享目录；逐级向上，因此嵌套仓库的外层元数据也会被保护。
/// 不依赖目录名字叫 `.git`：真实 gitDir 可能在任意位置。
fn enclosing_git_roots(start: &Path) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    let push = |path: PathBuf, roots: &mut Vec<PathBuf>| {
        if !roots.contains(&path) {
            roots.push(path);
        }
    };
    let mut cursor = Some(start);
    while let Some(directory) = cursor {
        // 目录**自己**就是 Git 元数据（`--separate-git-dir`、裸仓库、linked worktree 私有目录）。
        if directory.join("HEAD").is_file() && directory.join("objects").is_dir() {
            if let Ok(resolved) = fs::canonicalize(directory) {
                push(resolved, &mut roots);
            }
            if let Ok(text) = fs::read_to_string(directory.join("commondir")) {
                let common = PathBuf::from(text.trim());
                let common = if common.is_absolute() { common } else { directory.join(common) };
                if let Ok(common) = fs::canonicalize(&common) {
                    push(common, &mut roots);
                }
            }
        }
        let dot_git = directory.join(".git");
        match fs::symlink_metadata(&dot_git) {
            Ok(meta) if meta.is_dir() => {
                if let Ok(resolved) = fs::canonicalize(&dot_git) {
                    push(resolved, &mut roots);
                }
            }
            Ok(meta) if meta.is_file() => {
                if let Ok(text) = fs::read_to_string(&dot_git) {
                    if let Some(rest) = text.trim().strip_prefix("gitdir:") {
                        let target = PathBuf::from(rest.trim());
                        let target = if target.is_absolute() { target } else { directory.join(target) };
                        if let Ok(resolved) = fs::canonicalize(&target) {
                            // linked worktree 的 commondir 指向共享目录。
                            if let Ok(text) = fs::read_to_string(resolved.join("commondir")) {
                                let common = PathBuf::from(text.trim());
                                let common = if common.is_absolute() { common } else { resolved.join(common) };
                                if let Ok(common) = fs::canonicalize(&common) {
                                    push(common, &mut roots);
                                }
                            }
                            push(resolved, &mut roots);
                        }
                    }
                }
            }
            _ => {}
        }
        cursor = directory.parent();
    }
    roots
}

/// 回收区维护状态：自动清理失败必须能被普通模式查询到，不能只写测量日志（Q9）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RecoveryMaintenance {
    /// 最近一次自动清理的时间；从未执行过为 None。
    pub last_purge_at_ms: Option<u64>,
    /// 最近一次自动清理删除的记录数。
    pub last_purge_removed: u64,
    /// 最近一次自动清理的失败原因；成功时清空。
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct RecoveryStore {
    directory: PathBuf,
    clock: Arc<dyn Fn() -> u64 + Send + Sync>,
    /// 自动清理的可查询状态（持久化到回收区目录，重启后仍能看到失败）。
    maintenance: Arc<Mutex<RecoveryMaintenance>>,
    /// 上次持久化失败：保留内存状态并在下次读取时重试，避免只读窗口里彻底丢掉提示。
    maintenance_dirty: Arc<Mutex<bool>>,
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
        let directory = data_directory.join("recovery");
        let maintenance = read_maintenance(&directory);
        Self {
            directory,
            clock,
            in_use: Arc::new(Mutex::new(HashMap::new())),
            maintenance: Arc::new(Mutex::new(maintenance)),
            maintenance_dirty: Arc::new(Mutex::new(false)),
        }
    }

    /// 标记某条记录正在被使用（保存 / 恢复 / 另存），清理会跳过它。
    pub fn use_record(&self, id: &str) -> RecordUse {
        if let Ok(mut guard) = self.in_use.lock() {
            *guard.entry(id.to_string()).or_insert(0) += 1;
        }
        RecordUse { id: id.to_string(), in_use: self.in_use.clone() }
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
        // 先原子改名再删除：目录不可写时改名会失败，此时记录内容一个字节都没动。
        // 直接 `remove_dir_all` 会先删内容再删目录，失败后只剩一个读不出来的空壳。
        let name = path
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .ok_or_else(|| AppError::internal("回收记录路径无效"))?;
        let trashed = path.with_file_name(format!(
            "trash-{}-{}-{name}",
            (self.clock)(),
            NEXT_RECORD.fetch_add(1, Ordering::Relaxed)
        ));
        fs::rename(path, &trashed)?;
        fs::remove_dir_all(&trashed)?;
        Ok(true)
    }

    /// 清理上次删除留下的临时目录（改名成功但删除中断时）。
    ///
    /// 返回未能清除的残留数量；调用方据此把失败暴露出来（不能静默跳过）。
    fn sweep_trash(&self) -> usize {
        let Ok(entries) = fs::read_dir(&self.directory) else { return 0 };
        let mut remaining = 0;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("trash-") && fs::remove_dir_all(entry.path()).is_err() {
                remaining += 1;
            }
        }
        remaining
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
            let restore = self.latest_attempt(&record.id).ok().flatten();
            let version = self.version_of(&record, outcome.as_ref()).unwrap_or_default();
            records.push(RecoverySummary {
                id: record.id, created_at_ms: record.created_at_ms, source: record.source, target: record.target,
                worktree_path: record.identity.worktree_path.display().to_string(),
                paths: record.before.iter().map(|file| file.path.clone()).collect(),
                bytes: record.before.iter().filter_map(|file| file.content.as_ref()).map(|bytes| bytes.len() as u64).sum(),
                status: match outcome { Some(DiscardOutcome::Completed { .. }) => RecoveryStatus::Completed, Some(DiscardOutcome::NotApplied) => RecoveryStatus::NotApplied, None => RecoveryStatus::NeedsVerification },
                identity_confirmed: current.is_some_and(|current| record.identity.confirmed_match(current)),
                restore,
                version,
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
            restore: self.latest_attempt(&record.id).ok().flatten(),
            version: self.version_of(&record, outcome.as_ref()).unwrap_or_default(),
        })
    }

    /// 把保存的完整工作区状态写回。只写工作区，不碰暂存区、不重放反向补丁。
    ///
    /// 固定顺序，任何一步都不允许提前改用户路径：
    /// 1. 校验确认版本与丢弃基线；
    /// 2. **先核对真实身份与全部相关状态**（不匹配时连准备动作都不做）；
    /// 3. 准备临时内容（不触碰工作区）；
    /// 4. 受控阶段后，**每个路径发布前**重新捕获全部相关路径与冲突再发布；
    /// 5. 全部写完并再次核对真实状态后，才把这次尝试记为完成。
    pub fn restore_with_hook(
        &self,
        id: &str,
        worktree: &Path,
        expected: &RestoreExpectation,
        mut capture: impl FnMut(&[String]) -> Result<RestorePreconditions>,
        mut hook: impl FnMut(RestorePhase) -> Result<()>,
    ) -> Result<RestoreOutcome> {
        let _in_use = self.use_record(id);
        let (record, outcome) = self.read(id)?;
        let version_now = self.version_of(&record, outcome.as_ref())?;
        // 确认必须绑定用户看到的那一版记录；刷新后旧确认一律失效（Q89）。
        if expected.version != version_now {
            return Err(AppError::rejected("回收记录已变化，请重新确认"));
        }
        let after = match &outcome {
            Some(DiscardOutcome::Completed { after }) => after.clone(),
            _ => {
                return Err(AppError::rejected(
                    "这条记录没有可信的丢弃完成状态，结果待核对，不能直接恢复",
                ))
            }
        };

        let paths: Vec<String> = record.before.iter().map(|file| file.path.clone()).collect();

        // 写入前就留下这次尝试：之后任何阶段中断都能据此核对（Q91）。
        let attempt_version = version_now.clone();
        let index = self.append_attempt(id, RestoreAttempt {
            at_ms: (self.clock)(),
            state: RestoreAttemptState::Pending,
            written: Vec::new(),
            version: attempt_version,
            message: None,
        })?;

        // 2. 身份与全部相关状态的第一次核对。必须在任何准备动作之前：
        //    否则请求落到另一个 worktree 时会先在那里建出目录（Q87、spec L102）。
        let first = match capture(&paths) {
            Ok(value) => value,
            Err(error) => {
                let message = format!("恢复前无法读取真实状态，未写入任何路径：{error}");
                let _ = self.finish_attempt(id, index, RestoreAttemptState::NeedsVerification, &[], Some(&message));
                return Err(AppError::needs_verification(message));
            }
        };
        if let Err(error) = verify_publish(&first, &record, &after, &[]) {
            let message = format!("{error}（恢复未写入任何路径）");
            let _ = self.finish_attempt(id, index, RestoreAttemptState::Rejected, &[], Some(&message));
            return Err(error);
        }

        // 3. 准备临时内容。此时身份已经确认过，不会往别人的目录里建东西。
        let mut prepared = match prepare_files(worktree, &record.before) {
            Ok(prepared) => prepared,
            Err(error) => {
                let message = format!("恢复准备失败，未写入任何路径：{error}");
                let _ = self.finish_attempt(id, index, RestoreAttemptState::NeedsVerification, &[], Some(&message));
                return Err(AppError::needs_verification(message));
            }
        };

        if let Err(error) = hook(RestorePhase::BeforeWrite) {
            let message = format!("恢复在写入前中断，未写入任何路径：{error}");
            let _ = self.finish_attempt(id, index, RestoreAttemptState::NeedsVerification, &[], Some(&message));
            return Err(AppError::needs_verification(message));
        }

        // 4. 逐个发布；每次发布前都重新捕获**全部**相关路径与冲突。
        let mut written: Vec<String> = Vec::with_capacity(prepared.len());
        for file in prepared.iter_mut() {
            let preconditions = match capture(&paths) {
                Ok(value) => value,
                Err(error) => {
                    let message = if written.is_empty() {
                        format!("恢复前无法读取真实状态，未写入任何路径：{error}")
                    } else {
                        format!("恢复期间无法读取真实状态，可能已写入部分路径，结果待核对：{error}")
                    };
                    let _ = self.finish_attempt(id, index, RestoreAttemptState::NeedsVerification, &written, Some(&message));
                    return Err(AppError::needs_verification(message));
                }
            };
            if let Err(error) = verify_publish(&preconditions, &record, &after, &written) {
                // 已写入路径时不能声称「未写入任何路径」：如实标待核对。
                if written.is_empty() {
                    let message = format!("{error}（恢复未写入任何路径）");
                    let _ = self.finish_attempt(id, index, RestoreAttemptState::Rejected, &[], Some(&message));
                    return Err(error);
                }
                let message = format!("{error}（已写入 {} 个路径，结果待核对）", written.len());
                let _ = self.finish_attempt(id, index, RestoreAttemptState::NeedsVerification, &written, Some(&message));
                return Err(AppError::needs_verification(message));
            }
            if let Err(error) = file.publish() {
                let message = if written.is_empty() {
                    format!("恢复写入失败，结果待核对：{error}")
                } else {
                    format!("恢复写入失败，可能已写入部分路径，结果待核对：{error}")
                };
                let _ = self.finish_attempt(id, index, RestoreAttemptState::NeedsVerification, &written, Some(&message));
                return Err(AppError::needs_verification(message));
            }
            written.push(file.path.clone());
        }

        // 5. 从这里开始工作区已经写过：任何失败都只能标待核对，并保留记录。
        if let Err(error) = hook(RestorePhase::AfterWrite) {
            let message = format!("恢复写入后中断，结果待核对：{error}");
            let _ = self.finish_attempt(id, index, RestoreAttemptState::NeedsVerification, &written, Some(&message));
            return Err(AppError::needs_verification(message));
        }
        if let Err(error) = hook(RestorePhase::BeforeComplete) {
            let message = format!("恢复完成记录阶段中断，结果待核对：{error}");
            let _ = self.finish_attempt(id, index, RestoreAttemptState::NeedsVerification, &written, Some(&message));
            return Err(AppError::needs_verification(message));
        }

        // 完成前最后一次核对真实状态；**通过之前不写 Completed**。
        let verified = match capture(&paths) {
            Ok(pre) => match verify_publish(&pre, &record, &after, &written) {
                Ok(()) => Ok(pre),
                Err(error) => Err(error.to_string()),
            },
            Err(error) => Err(error.to_string()),
        };
        let final_state = match verified {
            Ok(pre) => pre,
            Err(reason) => {
                let message = format!("恢复已写入工作区，但完成前核对失败，结果待核对：{reason}");
                let _ = self.finish_attempt(id, index, RestoreAttemptState::NeedsVerification, &written, Some(&message));
                return Err(AppError::needs_verification(message));
            }
        };

        if let Err(error) = self.finish_attempt(id, index, RestoreAttemptState::Completed, &written, None) {
            // `finish_attempt` 已把「已完成」尽力改回待核对并返回 NeedsVerification；
            // 其它错误（例如读不出留证）同样属于写后不确定，统一按待核对上报（Q91）。
            return Err(match error.kind {
                crate::error::ErrorKind::NeedsVerification => error,
                _ => AppError::needs_verification(format!(
                    "恢复已写入工作区，但结果记录失败，结果待核对：{error}"
                )),
            });
        }

        // 尾部读取同样受 Q91 约束：失败时把这次尝试改回待核对，而不是留下 Completed。
        let inspection = match self.inspect(id, Some(&final_state.identity), Some(worktree)) {
            Ok(inspection) => inspection,
            Err(error) => return Err(self.tail_failure(id, index, &written, &format!("最新状态读取失败：{error}"))),
        };
        let attempt = match self.latest_attempt(id) {
            Ok(attempt) => attempt,
            Err(error) => return Err(self.tail_failure(id, index, &written, &format!("恢复历史读取失败：{error}"))),
        };
        Ok(RestoreOutcome { inspection, attempt })
    }

    /// 写后尾部失败：把留证改回待核对并返回 `NeedsVerification`（Q91）。
    fn tail_failure(&self, id: &str, index: usize, written: &[String], reason: &str) -> AppError {
        let message = format!("恢复已写入工作区，但{reason}，结果待核对");
        let _ = self.finish_attempt(id, index, RestoreAttemptState::NeedsVerification, written, Some(&message));
        AppError::needs_verification(message)
    }

    /// 读取恢复尝试历史（按时间顺序，最新在末尾）。
    pub fn attempts(&self, id: &str) -> Result<Vec<RestoreAttempt>> {
        let path = self.journal_path(id)?;
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(self.legacy_attempt(id)?)
            }
            Err(error) => return Err(error.into()),
        };
        let journal: RestoreJournal = serde_json::from_slice(&bytes)?;
        Ok(journal.attempts)
    }

    /// 最近一次恢复尝试。
    pub fn latest_attempt(&self, id: &str) -> Result<Option<RestoreAttempt>> {
        Ok(self.attempts(id)?.pop())
    }

    /// 早期版本把唯一一次尝试写在 `restore.pending` 里；读取时兼容，不丢历史。
    fn legacy_attempt(&self, id: &str) -> Result<Vec<RestoreAttempt>> {
        let directory = self.record_directory(id)?;
        let bytes = match fs::read(directory.join("restore.pending")) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let legacy: LegacyRestoreJournal = serde_json::from_slice(&bytes)?;
        Ok(vec![RestoreAttempt {
            at_ms: legacy.at_ms,
            state: match legacy.state {
                LegacyRestoreJournalState::Pending => RestoreAttemptState::Pending,
                LegacyRestoreJournalState::Completed => RestoreAttemptState::Completed,
                LegacyRestoreJournalState::NeedsVerification => RestoreAttemptState::NeedsVerification,
                LegacyRestoreJournalState::Rejected => RestoreAttemptState::Rejected,
            },
            written: legacy.written,
            version: String::new(),
            message: None,
        }])
    }

    fn journal_path(&self, id: &str) -> Result<PathBuf> {
        Ok(self.record_directory(id)?.join("restore.json"))
    }

    /// 追加一次尝试并返回它的序号；历史永不覆盖（Q91）。
    fn append_attempt(&self, id: &str, attempt: RestoreAttempt) -> Result<usize> {
        let path = self.journal_path(id)?;
        let mut journal = self.read_journal(&path)?;
        journal.attempts.push(attempt);
        let index = journal.attempts.len() - 1;
        self.write_journal(&path, &journal)?;
        Ok(index)
    }

    fn finish_attempt(
        &self,
        id: &str,
        index: usize,
        state: RestoreAttemptState,
        written: &[String],
        message: Option<&str>,
    ) -> Result<()> {
        let path = self.journal_path(id)?;
        let mut journal = self.read_journal(&path)?;
        let attempt = journal
            .attempts
            .get_mut(index)
            .ok_or_else(|| AppError::internal("恢复留证丢失"))?;
        attempt.state = state;
        attempt.written = written.to_vec();
        attempt.message = message.map(|value| value.to_string());
        let failure = match self.write_journal_steps(&path, &journal) {
            Ok(()) => return Ok(()),
            Err(failure) => failure,
        };
        if state != RestoreAttemptState::Completed {
            return Err(failure.0);
        }
        // `write_journal_steps` 先 rename 再 sync 目录：sync 失败时磁盘上已经是 Completed。
        // 不能就这样返回普通错误把「已完成」留在盘上——尽力写回待核对再报错（Q91）。
        Err(self.downgrade_after_completion_failure(&path, index, &mut journal, written, &failure.0))
    }

    /// 完成留证写入失败后的补救：把**内存中**这次尝试改回待核对并重新落盘。
    ///
    /// 不重新读盘：记录目录此时可能正不可读（真实故障），而内存里的历史就是刚写下的内容。
    /// 只要替换成功，重开后看到的就是待核对；若连替换都失败，消息里明确说明
    /// 「磁盘上可能仍显示已完成」，让用户按待核对处理，而不是相信那个状态。
    fn downgrade_after_completion_failure(
        &self,
        path: &Path,
        index: usize,
        journal: &mut RestoreJournal,
        written: &[String],
        cause: &AppError,
    ) -> AppError {
        let message = format!("恢复已写入工作区，但完成留证失败，结果待核对：{cause}");
        let downgraded = match journal.attempts.get_mut(index) {
            Some(attempt) => {
                attempt.state = RestoreAttemptState::NeedsVerification;
                attempt.written = written.to_vec();
                attempt.message = Some(message.clone());
                self.write_journal_steps(path, journal)
            }
            None => Err((AppError::internal("恢复留证丢失"), false)),
        };
        match downgraded {
            // 待核对已落盘（目录同步可能仍失败，但替换成功即代表重开看到的是待核对）。
            Ok(()) => AppError::needs_verification(message),
            Err((second, true)) => AppError::needs_verification(format!(
                "{message}；待核对留证已替换但目录同步失败（{second}），重开应显示待核对"
            )),
            Err((second, false)) => AppError::needs_verification(format!(
                "{message}；且待核对留证也未能写入（{second}），磁盘上可能仍显示已完成，请按待核对处理"
            )),
        }
    }

    fn read_journal(&self, path: &Path) -> Result<RestoreJournal> {
        match fs::read(path) {
            Ok(bytes) => match serde_json::from_slice::<RestoreJournal>(&bytes) {
                Ok(journal) => Ok(journal),
                // 读不出来的历史不覆盖：改名留档，再从空历史继续（仍然可核对）。
                Err(_) => {
                    let backup = path.with_extension(format!("corrupt-{}", (self.clock)()));
                    let _ = fs::rename(path, backup);
                    Ok(RestoreJournal::default())
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(RestoreJournal::default()),
            Err(error) => Err(error.into()),
        }
    }

    fn write_journal(&self, path: &Path, journal: &RestoreJournal) -> Result<()> {
        self.write_journal_steps(path, journal).map_err(|(error, _)| error)
    }

    /// 写入留证的三步：写临时文件 → 原子替换 → 同步目录。
    ///
    /// 返回 `Err((错误, 替换是否已落盘))`。区分这两件事很关键：只要**替换**成功，
    /// 重开后读到的就是这次写的内容，即使随后的目录同步失败（Q91）。
    fn write_journal_steps(&self, path: &Path, journal: &RestoreJournal) -> std::result::Result<(), (AppError, bool)> {
        let temp = path.with_extension(format!("next-{}", (self.clock)()));
        write_new(&temp, journal).map_err(|error| (error, false))?;
        fs::rename(&temp, path).map_err(|error| (error.into(), false))?;
        if let Some(parent) = path.parent() {
            if let Err(error) = platform::sync_directory(parent) {
                return Err((error.into(), true));
            }
        }
        Ok(())
    }

    /// 记录内容版本：记录本身 + 丢弃回执 + 恢复历史。任何一项变化都使旧确认失效。
    pub fn version_of(&self, record: &RecoveryRecord, outcome: Option<&DiscardOutcome>) -> Result<String> {
        let mut hasher = Sha256::new();
        hasher.update(b"recovery-version-v1");
        hasher.update(serde_json::to_vec(record)?);
        hasher.update([0]);
        if let Some(outcome) = outcome {
            hasher.update(serde_json::to_vec(outcome)?);
        }
        hasher.update([0]);
        for attempt in self.attempts(&record.id)? {
            hasher.update(serde_json::to_vec(&attempt)?);
            hasher.update([0]);
        }
        Ok(hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect())
    }

    /// 生成「保存版本 vs 当前内容」的只读 diff（文本）。
    ///
    /// 任一侧不存在也能对比（保存时是删除 / 之后被删除），用 Git 的 `/dev/null`
    /// 表示空侧。两个输入都写在回收区自己的临时目录里，不在用户仓库里创建文件。
    pub fn text_diff(&self, git: &Git, id: &str, path: &str, worktree: Option<&Path>) -> Result<String> {
        let (record, _) = self.read(id)?;
        let saved = record
            .before
            .iter()
            .find(|file| file.path == path)
            .ok_or_else(|| AppError::rejected("该记录不包含这个路径"))?;
        let saved_bytes = match &saved.content {
            Some(bytes) => Some(bytes.as_slice()),
            None => None,
        };
        if let Some(bytes) = saved_bytes {
            if is_binary(bytes) {
                return Err(AppError::rejected("二进制内容不提供文本对比"));
            }
        }
        // 只有**确认不存在**才把当前侧当空侧；读不出来、类型不对、身份不可用都必须如实报错，
        // 不能伪装成「删除」画出来（spec L101）。
        let current_bytes: Option<Vec<u8>> = match worktree {
            Some(root) => {
                let full = checked_path(root, path)?;
                match fs::symlink_metadata(&full) {
                    Ok(meta) if meta.is_file() => Some(fs::read(&full)?),
                    Ok(_) => {
                        return Err(AppError::rejected("当前路径不是普通文件，无法生成文本对比"))
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                    Err(error) => {
                        return Err(AppError::environment(format!(
                            "无法读取当前内容，不能生成对比：{error}"
                        )))
                    }
                }
            }
            None => {
                return Err(AppError::rejected(
                    "当前 worktree 不可用，无法读取当前状态；保存版本仍可查看或另存",
                ))
            }
        };
        if let Some(bytes) = &current_bytes {
            if is_binary(bytes) {
                return Err(AppError::rejected("二进制内容不提供文本对比"));
            }
        }
        if saved_bytes.is_none() && current_bytes.is_none() {
            return Ok(String::new());
        }

        let scratch = self.allocate("diff")?;
        let result = (|| {
            let old_path = scratch.join("saved");
            let new_path = scratch.join("current");
            if let Some(bytes) = saved_bytes {
                fs::write(&old_path, bytes)?;
            }
            if let Some(bytes) = &current_bytes {
                fs::write(&new_path, bytes)?;
            }
            // 缺失的一侧交给 Git 的 /dev/null，让它生成「新增」或「删除」的完整 hunk。
            let missing = Path::new("/dev/null");
            let bytes = git
                .in_isolated_directory(&scratch)
                .diff_no_index(
                    if saved_bytes.is_some() { &old_path } else { missing },
                    if current_bytes.is_some() { &new_path } else { missing },
                )?;
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
        // 目标路径的每一级祖先都要通过安全检查；真实 gitDir/commonDir 由 Git 自己解析，
        // 不只认名为 `.git` 的目录（`--separate-git-dir`、linked worktree 的私有目录都要挡住）。
        // 同时保护**目标所在仓库**的元数据：记录来自 A，但目标可能落在另一个登记仓库 B。
        let destination = &self.prepare_destination(destination, &record.identity)?;

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

    /// 校验另存目标：祖先链无符号链接、不落在 Git 元数据里、目标本身尚不存在。
    ///
    /// 返回实际要写入的路径。这里不使用 `canonicalize` 后比较：一旦规范化就丢掉了
    /// 「这一级是链接」的证据，会让 `link/inner` 穿透重新成立。
    fn prepare_destination(&self, destination: &Path, identity: &WorktreeIdentity) -> Result<PathBuf> {
        let name = destination
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| AppError::rejected("另存路径包含无法无损处理的字符"))?;
        if name.is_empty() || name == "." || name == ".." {
            return Err(AppError::rejected("另存路径无效"));
        }
        let parent = destination
            .parent()
            .ok_or_else(|| AppError::rejected("另存目录无效"))?;
        if !parent.is_dir() {
            return Err(AppError::rejected("另存目录不存在"));
        }
        let mut cursor = PathBuf::new();
        for component in parent.components() {
            cursor.push(component);
            match fs::symlink_metadata(&cursor) {
                Ok(meta) if meta.file_type().is_symlink() => {
                    return Err(AppError::rejected("另存路径的上级目录是符号链接，请选择真实目录"));
                }
                Ok(meta) if meta.is_dir() => {}
                Ok(_) => return Err(AppError::rejected("另存路径的上级不是目录")),
                Err(_) => return Err(AppError::rejected("另存路径的上级目录不可访问")),
            }
        }
        let resolved_parent = fs::canonicalize(parent)?;
        // 真实 gitDir / commonDir 可能叫任何名字（--separate-git-dir），必须按解析结果判断。
        let mut protected_roots: Vec<PathBuf> = vec![identity.git_dir.clone(), identity.common_dir.clone()];
        // 目标侧：逐级解析祖先链上的 `.git`（目录或 gitdir 文件指针）与 commondir，
        // 这样嵌套仓库、另一个登记仓库的元数据都被挡住，而不是只比源记录的身份。
        protected_roots.extend(enclosing_git_roots(&resolved_parent));
        for protected in &protected_roots {
            if let Ok(root) = fs::canonicalize(protected) {
                if resolved_parent == root || resolved_parent.starts_with(&root) {
                    return Err(AppError::rejected("不能写入 Git 元数据目录"));
                }
            }
            if resolved_parent == protected.as_path() || resolved_parent.starts_with(protected) {
                return Err(AppError::rejected("不能写入 Git 元数据目录"));
            }
        }
        let target = resolved_parent.join(name);
        if fs::symlink_metadata(&target).is_ok() {
            return Err(AppError::rejected("目标路径已存在，请选择其他路径"));
        }
        Ok(target)
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
        // 上次删除留下的残留也在这一轮一起清；清不掉就报错，不能靠「本轮删了 0 条」把它藏起来。
        let leftover = self.sweep_trash();
        if leftover > 0 {
            return Err(AppError::environment(format!(
                "回收区还有 {leftover} 个未清理的残留目录，请稍后重试"
            )));
        }
        Ok(removed)
    }

    /// 自动清理入口：记录成功/失败到可查询状态，并持久化（普通模式可见）。
    pub fn auto_purge(&self) -> Result<u64> {
        let result = self.purge_expired();
        let at = (self.clock)();
        {
            let mut maintenance = self
                .maintenance
                .lock()
                .map_err(|_| AppError::internal("回收区维护状态锁失败"))?;
            maintenance.last_purge_at_ms = Some(at);
            match &result {
                Ok(removed) => {
                    maintenance.last_purge_removed = *removed;
                    maintenance.error = None;
                }
                Err(error) => {
                    maintenance.error = Some(error.to_string());
                }
            }
            if write_maintenance(&self.directory, &maintenance).is_err() {
                // 目录暂时不可写：保留内存状态，下次读取时再落盘。
                if let Ok(mut dirty) = self.maintenance_dirty.lock() {
                    *dirty = true;
                }
            } else if let Ok(mut dirty) = self.maintenance_dirty.lock() {
                *dirty = false;
            }
        }
        result
    }

    /// 读取自动清理状态（普通模式下界面据此提示失败）。
    ///
    /// 若上次落盘失败，这里会重试；内存中的失败原因不会因为一次只读窗口而丢失。
    pub fn maintenance(&self) -> RecoveryMaintenance {
        let value = self.maintenance.lock().map(|value| value.clone()).unwrap_or(RecoveryMaintenance {
            last_purge_at_ms: None,
            last_purge_removed: 0,
            error: Some("回收区维护状态不可读".into()),
        });
        let dirty = self.maintenance_dirty.lock().map(|dirty| *dirty).unwrap_or(false);
        if dirty && write_maintenance(&self.directory, &value).is_ok() {
            if let Ok(mut dirty) = self.maintenance_dirty.lock() {
                *dirty = false;
            }
        }
        value
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
        let leftover = self.sweep_trash();
        if leftover > 0 {
            return Err(AppError::environment(format!(
                "回收区还有 {leftover} 个未清理的残留目录，请稍后重试"
            )));
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
    /// 用户确认时看到的记录版本（记录内容 + 回执 + 恢复历史的摘要）。
    pub version: String,
    pub paths: Vec<String>,
    pub state: RecoveryState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RestoreAttemptState { Pending, Completed, NeedsVerification, Rejected }

/// 一次恢复尝试。历史按时间追加保留，最新一条是当前状态（Q91）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RestoreAttempt {
    pub at_ms: u64,
    pub state: RestoreAttemptState,
    /// 已发布到工作区的路径；部分完成时如实列出，不冒充全部完成。
    pub written: Vec<String>,
    /// 这次尝试核对时看到的记录版本。
    pub version: String,
    #[serde(default)]
    pub message: Option<String>,
}

/// 恢复自身的留证文件：按时间追加，永不覆盖（Q91）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RestoreJournal {
    #[serde(default)]
    attempts: Vec<RestoreAttempt>,
}

/// 早期版本写入 `restore.pending` 的单一尝试；读取时兼容，不丢历史。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyRestoreJournal {
    state: LegacyRestoreJournalState,
    written: Vec<String>,
    at_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum LegacyRestoreJournalState { Pending, Completed, NeedsVerification, Rejected }

/// 恢复结果：真实重读的检查结果 + 这次尝试的留证。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RestoreOutcome {
    pub inspection: RecoveryInspection,
    pub attempt: Option<RestoreAttempt>,
}

/// 恢复的阶段，供受控中断接缝与结果留证使用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestorePhase { BeforeWrite, AfterWrite, BeforeComplete }

/// 发布每个路径前重新捕获的真实状态（身份 + 全部相关路径 + 冲突）。
#[derive(Debug, Clone)]
pub struct RestorePreconditions {
    pub identity: WorktreeIdentity,
    pub files: Vec<SavedFile>,
    /// 当前处于未解决冲突的路径（Q82：冲突文件只读）。
    pub conflicts: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscardPhase { BeforeSave, AfterSave, BeforeWrite, AfterWrite, BeforeComplete }

/// 自动清理状态落盘：重启后仍能看到上次失败（不静默降级）。
fn write_maintenance(directory: &Path, value: &RecoveryMaintenance) -> Result<()> {
    fs::create_dir_all(directory)?;
    let path = directory.join("maintenance.json");
    let temp = directory.join(format!("maintenance-{}.json", NEXT_RECORD.fetch_add(1, Ordering::Relaxed)));
    write_new(&temp, value)?;
    fs::rename(&temp, &path)?;
    Ok(())
}

fn read_maintenance(directory: &Path) -> RecoveryMaintenance {
    fs::read(directory.join("maintenance.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or(RecoveryMaintenance { last_purge_at_ms: None, last_purge_removed: 0, error: None })
}

/// 文本对比只接受合法 UTF-8 且不含 NUL 的内容。
fn is_binary(bytes: &[u8]) -> bool {
    bytes.contains(&0) || std::str::from_utf8(bytes).is_err()
}

pub fn unchanged(before: &[SavedFile], after: &[SavedFile]) -> bool {
    // Comparing actual bytes also catches clean filters that hide worktree changes from Git.
    before == after
}

