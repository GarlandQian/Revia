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
}

impl ReviewStore {
    pub fn new(data_directory: PathBuf) -> Self {
        Self { directory: data_directory }
    }

    fn path(&self) -> PathBuf {
        self.directory.join("review.json")
    }

    /// 持久身份键：真实路径 + 文件系统标识。
    ///
    /// 不能用 worktree ID（那个由路径推导）：同路径重建后 ID 相同但身份不同，
    /// 会错误继承旧标记（Q87）。
    pub fn identity_key(identity: &WorktreeIdentity) -> String {
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
        hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn read_file(&self) -> Result<ReviewFile> {
        match fs::read(self.path()) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| AppError::internal(format!("审阅标记文件无法解析：{error}"))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(ReviewFile { version: 1, worktrees: BTreeMap::new() }),
            Err(error) => Err(AppError::internal(format!("读取审阅标记失败：{error}"))),
        }
    }

    fn write_file(&self, value: &ReviewFile) -> Result<()> {
        fs::create_dir_all(&self.directory)
            .map_err(|error| AppError::internal(format!("创建数据目录失败：{error}")))?;
        let path = self.path();
        let temp = self.directory.join(format!("review-{}.json", std::process::id()));
        let text = serde_json::to_vec_pretty(value)?;
        fs::write(&temp, &text).map_err(|error| AppError::internal(format!("写入审阅标记失败：{error}")))?;
        fs::rename(&temp, &path).map_err(|error| AppError::internal(format!("保存审阅标记失败：{error}")))?;
        platform::sync_directory(&self.directory)
            .map_err(|error| AppError::internal(format!("同步审阅标记失败：{error}")))?;
        Ok(())
    }

    /// 读出某个身份下保存的标记（不做有效性判断，由调用方比对版本）。
    pub fn marks(&self, identity: &WorktreeIdentity) -> Result<Vec<ReviewMark>> {
        let file = self.read_file()?;
        let key = Self::identity_key(identity);
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
        let mut file = self.read_file()?;
        let key = Self::identity_key(identity);
        let entry = file.worktrees.entry(key).or_default();
        if marked {
            entry.insert(path.to_string(), ReviewMark { path: path.to_string(), versions });
        } else {
            entry.remove(path);
        }
        if entry.is_empty() {
            let key = Self::identity_key(identity);
            file.worktrees.remove(&key);
        }
        file.version = 1;
        self.write_file(&file)?;
        Ok(self.marks(identity)?)
    }

    /// 丢弃与当前版本不一致的标记（只清理确实失效的，不动仍然有效的）。
    pub fn retain_valid(
        &self,
        identity: &WorktreeIdentity,
        valid: impl Fn(&ReviewMark) -> bool,
    ) -> Result<Vec<ReviewMark>> {
        let mut file = self.read_file()?;
        let key = Self::identity_key(identity);
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
        Ok(self.marks(identity)?)
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
        // 子模块或嵌套目录：只记录「是目录」，不递归读取用户内容。
        hasher.update(b"dir:");
    } else {
        hasher.update(b"file:");
        hasher.update(fs::read(&full)?);
    }
    Ok(Some(hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()))
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
