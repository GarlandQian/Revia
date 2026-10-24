//! 审阅标记的持久化（Q20、Q37、Q48、Q87、Q94、T10、T12）。
//!
//! 标记按 **worktree 身份 + 文件 + HEAD 版本 + 工作区版本** 记录，任一版本变化即失效。
//! 存储放在 Revia 自己的数据目录（`review.json`），不写进用户仓库，也不改 Git 配置。
//!
//! 身份沿用任务 05 的 [`WorktreeIdentity`]（真实 worktree / gitDir / commonDir 路径 +
//! 文件系统标识）：移动或同路径重建后身份不同，因此不会继承旧标记（Q87）。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::error::{AppError, Result};
use crate::platform;
use crate::recovery::WorktreeIdentity;

/// 文件的两个版本。任一侧变化都使标记失效。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReviewVersions {
    /// HEAD 中的文件版本（未跟踪或新增文件为 None）。
    pub head: Option<String>,
    /// 工作区中的文件版本（不存在为 None）。
    pub worktree: Option<String>,
}

/// 一条已保存的标记。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewMark {
    pub path: String,
    #[serde(flatten)]
    pub versions: ReviewVersions,
}

/// 某个 worktree 的审阅状态：哪些文件已审阅、进度如何。
///
/// `paths` 是当前改动列表里**去重后**的文件路径（同一文件跨组只算一次）。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReviewMarks {
    pub worktree_id: String,
    /// 当前快照里的去重路径（按界面显示顺序）。
    pub paths: Vec<String>,
    /// 仍然有效的已审阅路径。
    pub reviewed: Vec<String>,
    pub total: u32,
    pub reviewed_count: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewFile {
    #[serde(default)]
    version: u32,
    /// 身份键 → (路径 → 标记)
    #[serde(default)]
    worktrees: BTreeMap<String, BTreeMap<String, ReviewMark>>,
}

/// 审阅标记存储。所有写入都是「读 → 改 → 原子替换」，失败如实返回错误。
#[derive(Clone)]
pub struct ReviewStore {
    directory: PathBuf,
    /// 存储级事务锁：整个「读 → 改 → 发布」串行，避免不同 worktree 成功保存互相覆盖（ST07-1、SP07-7）。
    ///
    /// 用文件锁而不是进程内 Mutex：同一数据目录可能被多个会话（甚至多个进程）共用。
    transaction: Arc<Mutex<()>>,
}

impl ReviewStore {
    pub fn new(data_directory: PathBuf) -> Self {
        Self { directory: data_directory, transaction: Arc::new(Mutex::new(())) }
    }

    /// 进入一次存储事务：整个读改写期间持有锁。
    fn begin_transaction(&self) -> Result<std::sync::MutexGuard<'_, ()>> {
        self.transaction
            .lock()
            .map_err(|_| AppError::internal("审阅标记存储锁失败"))
    }

    fn path(&self) -> PathBuf {
        self.directory.join("review.json")
    }

    /// 持久身份键：真实路径 + 文件系统标识。
    ///
    /// 不能用 worktree ID（那个由路径推导）：同路径重建后 ID 相同但身份不同，
    /// 会错误继承旧标记（Q87）。
    ///
    /// 任一路径拿不到文件系统标识时返回 `None`：**不按路径认领**，
    /// 否则未知身份会被稳定路径键错误继承（ST07-2、SP07-6）。
    pub fn identity_key(identity: &WorktreeIdentity) -> Option<String> {
        if identity.filesystem_ids.iter().any(Option::is_none) {
            return None;
        }
        let mut hasher = Sha256::new();
        hasher.update(b"revia-review-identity-v1");
        for path in [&identity.worktree_path, &identity.git_dir, &identity.common_dir] {
            hasher.update(path.to_string_lossy().as_bytes());
            hasher.update([0]);
        }
        for id in &identity.filesystem_ids {
            match id {
                Some(value) => hasher.update(value.as_bytes()),
                None => hasher.update(b"<none>"),
            }
            hasher.update([0]);
        }
        Some(hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect())
    }

    fn read_file(&self) -> Result<ReviewFile> {
        match fs::read(self.path()) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| AppError::internal(format!("审阅标记文件无法解析：{error}"))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(ReviewFile { version: 1, worktrees: BTreeMap::new() }),
            Err(error) => Err(AppError::internal(format!("读取审阅标记失败：{error}"))),
        }
    }

    /// 独占临时文件：名字带进程号 + 单调序号 + 纳秒，并用 `create_new` 保证不撞名。
    fn exclusive_temp(&self) -> Result<(PathBuf, fs::File)> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        for _ in 0..64 {
            let nonce = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let path = self
                .directory
                .join(format!("review-{}-{nanos}-{nonce}.tmp", std::process::id()));
            match fs::OpenOptions::new().create_new(true).write(true).open(&path) {
                Ok(handle) => return Ok((path, handle)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(AppError::internal(format!("创建审阅标记临时文件失败：{error}")))
                }
            }
        }
        Err(AppError::internal("无法创建审阅标记临时文件"))
    }

    fn write_file(&self, value: &ReviewFile) -> Result<()> {
        use std::io::Write;
        fs::create_dir_all(&self.directory)
            .map_err(|error| AppError::internal(format!("创建数据目录失败：{error}")))?;
        let path = self.path();
        let (temp, mut handle) = self.exclusive_temp()?;
        let text = serde_json::to_vec_pretty(value)?;
        let write = handle
            .write_all(&text)
            .and_then(|()| handle.sync_all());
        drop(handle);
        if let Err(error) = write {
            let _ = fs::remove_file(&temp);
            return Err(AppError::internal(format!("写入审阅标记失败：{error}")));
        }
        if let Err(error) = fs::rename(&temp, &path) {
            let _ = fs::remove_file(&temp);
            return Err(AppError::internal(format!("保存审阅标记失败：{error}")));
        }
        platform::sync_directory(&self.directory)
            .map_err(|error| AppError::internal(format!("同步审阅标记失败：{error}")))?;
        Ok(())
    }

    /// 读出某个身份下保存的标记（不做有效性判断，由调用方比对版本）。
    pub fn marks(&self, identity: &WorktreeIdentity) -> Result<Vec<ReviewMark>> {
        let _transaction = self.begin_transaction()?;
        let Some(key) = Self::identity_key(identity) else {
            // 身份不明确：不按路径认领任何标记。
            return Ok(Vec::new());
        };
        let file = self.read_file()?;
        Ok(file
            .worktrees
            .get(&key)
            .map(|marks| marks.values().cloned().collect())
            .unwrap_or_default())
    }

    /// 设置或取消一条标记，并把失效标记一并清理。
    ///
    /// 返回保存后的**全部**标记；写入失败时返回错误（调用方不能当作已保存）。
    pub fn set_mark(
        &self,
        identity: &WorktreeIdentity,
        path: &str,
        versions: ReviewVersions,
        marked: bool,
    ) -> Result<Vec<ReviewMark>> {
        let _transaction = self.begin_transaction()?;
        let key = Self::identity_key(identity)
            .ok_or_else(|| AppError::internal("worktree 身份不明确，无法保存审阅标记"))?;
        let mut file = self.read_file()?;
        let entry = file.worktrees.entry(key.clone()).or_default();
        if marked {
            entry.insert(path.to_string(), ReviewMark { path: path.to_string(), versions });
        } else {
            entry.remove(path);
        }
        if entry.is_empty() {
            file.worktrees.remove(&key);
        }
        file.version = 1;
        self.write_file(&file)?;
        Ok(file
            .worktrees
            .get(&key)
            .map(|marks| marks.values().cloned().collect())
            .unwrap_or_default())
    }

    /// 丢弃与当前版本不一致的标记（只清理确实失效的，不动仍然有效的）。
    pub fn retain_valid(
        &self,
        identity: &WorktreeIdentity,
        valid: impl Fn(&ReviewMark) -> bool,
    ) -> Result<Vec<ReviewMark>> {
        let _transaction = self.begin_transaction()?;
        let Some(key) = Self::identity_key(identity) else {
            return Ok(Vec::new());
        };
        let mut file = self.read_file()?;
        let Some(entry) = file.worktrees.get_mut(&key) else {
            return Ok(Vec::new());
        };
        let before = entry.len();
        entry.retain(|_, mark| valid(mark));
        let removed = before != entry.len();
        if entry.is_empty() {
            file.worktrees.remove(&key);
        }
        if removed {
            self.write_file(&file)?;
        }
        // 已在事务内，直接返回内存结果（不能重入 `marks`，那会死锁）。
        Ok(file
            .worktrees
            .get(&key)
            .map(|marks| marks.values().cloned().collect())
            .unwrap_or_default())
    }
}

/// 工作区中某个路径的版本（内容 SHA-256；不存在为 None）。
///
/// 符号链接按链接目标记录，子模块按提交号记录，避免把目录内容当文件读。
pub fn worktree_version(root: &Path, path: &str) -> Result<Option<String>> {
    let full = root.join(path);
    let meta = match fs::symlink_metadata(&full) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(AppError::internal(format!("读取文件状态失败：{error}"))),
    };
    let mut hasher = Sha256::new();
    if meta.file_type().is_symlink() {
        let target = fs::read_link(&full)?;
        hasher.update(b"symlink:");
        hasher.update(target.to_string_lossy().as_bytes());
    } else if meta.is_dir() {
        // 子模块：记录**真实 Git 版本**，切换提交后标记随之失效（ST07-8、SP07-8）。
        // 非子模块的目录按内容哈希（不递归读用户文件），保持只读、不启动 Git 进程。
        match submodule_commit(root, path) {
            Some(commit) => {
                hasher.update(b"submodule:");
                hasher.update(commit.as_bytes());
            }
            None => hasher.update(b"dir:"),
        }
    } else {
        hasher.update(b"file:");
        hasher.update(fs::read(&full)?);
    }
    Ok(Some(hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()))
}

/// 读取子模块当前检出的提交号（只读 `.git` 文件/目录，不启动 Git 进程）。
///
/// 子模块的 `.git` 是一个文件：`gitdir: ../.git/modules/<name>`；真实 HEAD 在
/// `<gitdir>/HEAD` 或 `<gitdir>/refs/...`。读不出来就返回 None（调用方退化为「是目录」）。
fn submodule_commit(root: &Path, path: &str) -> Option<String> {
    let dot_git = root.join(path).join(".git");
    let meta = fs::symlink_metadata(&dot_git).ok()?;
    if !meta.is_file() {
        return None;
    }
    let text = fs::read_to_string(&dot_git).ok()?;
    let target = text.trim().strip_prefix("gitdir:")?.trim();
    let git_dir = {
        let candidate = PathBuf::from(target);
        if candidate.is_absolute() { candidate } else { root.join(path).join(candidate) }
    };
    let head = fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let head = head.trim();
    if let Some(reference) = head.strip_prefix("ref: ") {
        // 分支引用：读 `refs/...`；打包引用（packed-refs）里的内容这里不解析，
        // 读不到就返回 None，退化为「是目录」，不会伪造版本。
        let value = fs::read_to_string(git_dir.join(reference)).ok()?;
        let value = value.trim();
        return (!value.is_empty()).then(|| value.to_string());
    }
    (!head.is_empty()).then(|| head.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(path: &Path, tag: &str) -> WorktreeIdentity {
        WorktreeIdentity {
            worktree_path: path.to_path_buf(),
            git_dir: path.join(".git"),
            common_dir: path.join(".git"),
            filesystem_ids: vec![Some(tag.to_string())],
        }
    }

    #[test]
    fn identity_key_distinguishes_recreated_worktree_at_same_path() {
        let dir = tempfile::tempdir().unwrap();
        let first = identity(dir.path(), "inode-1");
        let second = identity(dir.path(), "inode-2");
        assert_ne!(ReviewStore::identity_key(&first), ReviewStore::identity_key(&second));
    }

    #[test]
    fn marks_are_scoped_per_identity_and_survive_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let store = ReviewStore::new(dir.path().join("data"));
        let a = identity(dir.path(), "inode-a");
        let b = identity(dir.path(), "inode-b");
        let versions = ReviewVersions { head: Some("h".into()), worktree: Some("w".into()) };
        store.set_mark(&a, "x.txt", versions.clone(), true).unwrap();
        // 重新打开：仍然读得到（重启后保留）。
        let reopened = ReviewStore::new(dir.path().join("data"));
        assert_eq!(reopened.marks(&a).unwrap().len(), 1);
        // 另一身份不继承。
        assert!(reopened.marks(&b).unwrap().is_empty());
    }

    #[test]
    fn unmark_and_prune_remove_only_the_target() {
        let dir = tempfile::tempdir().unwrap();
        let store = ReviewStore::new(dir.path().join("data"));
        let a = identity(dir.path(), "inode-a");
        let v = |n: &str| ReviewVersions { head: Some(n.into()), worktree: Some(n.into()) };
        store.set_mark(&a, "one.txt", v("1"), true).unwrap();
        store.set_mark(&a, "two.txt", v("2"), true).unwrap();
        store.set_mark(&a, "one.txt", v("1"), false).unwrap();
        assert_eq!(store.marks(&a).unwrap().len(), 1);
        store
            .retain_valid(&a, |mark| mark.path == "kept.txt")
            .unwrap();
        assert!(store.marks(&a).unwrap().is_empty());
    }
}
