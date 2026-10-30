//! GitHub 相关的本地状态：评审草稿、合并回执、分支保护缓存、远端解析。
//!
//! 这些文件只存在 Revia 数据目录，不写仓库、不改 Git 配置。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::Result;
use crate::session::WorktreeSession;

use super::parse_github_repo;

/// 一条本地评审草稿（未提交的评论）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReviewDraft {
    pub id: String,
    pub repo_key: String,
    pub number: u64,
    /// F09：草稿绑定的 PR head SHA——PR 更新后旧草稿不再被误当当前版本提交。
    #[serde(default)]
    #[ts(optional)]
    pub head_sha: Option<String>,
    pub path: String,
    /// `new`（新增行）或 `old`（删除行）。
    pub side: String,
    pub line: u32,
    pub body: String,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

/// 合并/评审的本地回执：即使网络结果不确定，也能核对做过什么。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MergeReceipt {
    pub id: String,
    pub repo_key: String,
    pub number: u64,
    /// `merge` / `squash` / `rebase`，评审回执为 `review:<event>`。
    pub action: String,
    pub head_sha: Option<String>,
    pub result_sha: Option<String>,
    pub protected_branch: bool,
    /// `api`（已核对）/ `unknown`（无法核对，按受保护处理）。
    pub protection_source: String,
    pub notes: Vec<String>,
    pub created_at_ms: u64,
}

/// 分支保护状态（本地缓存，24 小时内复用）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProtectionInfo {
    pub branch: String,
    pub protected: bool,
    /// `api` 表示来自 GitHub；`unknown` 表示无法确认（按受保护处理）。
    pub source: String,
    pub checked_at_ms: u64,
}

/// 24 小时后视为过期：过期按“未知”处理，要求用户确认。
pub const PROTECTION_TTL_MS: u64 = 24 * 60 * 60 * 1000;

/// 一个仓库的远端。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RemoteInfo {
    pub name: String,
    pub url: String,
    pub owner: Option<String>,
    pub repo: Option<String>,
}

/// 读取仓库的远端列表（`git remote -v`，只读）。
pub fn remotes(session: &WorktreeSession) -> Result<Vec<RemoteInfo>> {
    let output = session.git.run_ok(&["remote", "-v"], "读取远端列表")?;
    let mut seen: BTreeMap<String, RemoteInfo> = BTreeMap::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let mut parts = line.split_whitespace();
        let (Some(name), Some(url)) = (parts.next(), parts.next()) else {
            continue;
        };
        if seen.contains_key(name) {
            continue;
        }
        let parsed = parse_github_repo(url);
        seen.insert(
            name.to_string(),
            RemoteInfo {
                name: name.to_string(),
                url: url.to_string(),
                owner: parsed.as_ref().map(|(owner, _)| owner.clone()),
                repo: parsed.map(|(_, repo)| repo),
            },
        );
    }
    Ok(seen.into_values().collect())
}

/// 评审草稿存储。
pub struct ReviewDraftStore {
    path: PathBuf,
}

impl ReviewDraftStore {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join("github-review-drafts.json"),
        }
    }

    pub fn list(&self, repo_key: &str, number: u64) -> Vec<ReviewDraft> {
        self.load()
            .into_iter()
            .filter(|draft| draft.repo_key == repo_key && draft.number == number)
            .collect()
    }

    pub fn save(&self, mut draft: ReviewDraft) -> Result<ReviewDraft> {
        let mut all = self.load();
        if draft.id.is_empty() {
            draft.id = super::super::sha256_hex(
                format!(
                    "{}\0{}\0{}\0{}\0{}",
                    draft.repo_key, draft.number, draft.path, draft.side, draft.line
                )
                .as_bytes(),
            )[..16]
                .to_string();
            draft.created_at_ms = super::super::now_ms();
        }
        draft.updated_at_ms = super::super::now_ms();
        all.retain(|existing| existing.id != draft.id);
        all.push(draft.clone());
        all.sort_by_key(|draft| draft.created_at_ms);
        all.truncate(2000);
        self.store(&all)?;
        Ok(draft)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let mut all = self.load();
        all.retain(|draft| draft.id != id);
        self.store(&all)
    }

    pub fn clear(&self, repo_key: &str, number: u64) -> Result<()> {
        let mut all = self.load();
        all.retain(|draft| !(draft.repo_key == repo_key && draft.number == number));
        self.store(&all)
    }

    fn load(&self) -> Vec<ReviewDraft> {
        std::fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn store(&self, drafts: &[ReviewDraft]) -> Result<()> {
        super::super::atomic_write(&self.path, &serde_json::to_vec_pretty(drafts)?)
    }
}

/// 回执与保护状态存储。
pub struct ReceiptStore {
    receipts: PathBuf,
    protection: PathBuf,
}

impl ReceiptStore {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            receipts: data_dir.join("github-receipts.json"),
            protection: data_dir.join("github-protection.json"),
        }
    }

    pub fn append(&self, mut receipt: MergeReceipt) -> Result<MergeReceipt> {
        if receipt.id.is_empty() {
            receipt.id = super::super::sha256_hex(
                format!(
                    "{}\0{}\0{}\0{}",
                    receipt.repo_key,
                    receipt.number,
                    receipt.action,
                    receipt.created_at_ms
                )
                .as_bytes(),
            )[..16]
                .to_string();
        }
        let mut all = self.load_receipts();
        all.push(receipt.clone());
        all.sort_by_key(|receipt| receipt.created_at_ms);
        all.truncate(500);
        super::super::atomic_write(&self.receipts, &serde_json::to_vec_pretty(&all)?)?;
        Ok(receipt)
    }

    pub fn list(&self, repo_key: &str) -> Vec<MergeReceipt> {
        self.load_receipts()
            .into_iter()
            .filter(|receipt| receipt.repo_key == repo_key)
            .collect()
    }

    /// F10：某仓库所有已缓存“确实受保护”的分支名（去重），供推送 / 历史保护取并集。
    pub fn protected_branches(&self, repo_key: &str) -> Vec<String> {
        let mut branches: Vec<String> = self
            .load_protection()
            .get(repo_key)
            .map(|map| {
                map.iter()
                    .filter(|(_, info)| info.protected)
                    .map(|(branch, _)| branch.clone())
                    .collect()
            })
            .unwrap_or_default();
        branches.sort();
        branches.dedup();
        branches
    }

    pub fn protection(&self, repo_key: &str, branch: &str) -> Option<ProtectionInfo> {
        self.load_protection()
            .get(repo_key)
            .and_then(|branches| branches.get(branch))
            .cloned()
    }

    pub fn store_protection(&self, repo_key: &str, info: &ProtectionInfo) -> Result<()> {
        let mut all = self.load_protection();
        all.entry(repo_key.to_string())
            .or_default()
            .insert(info.branch.clone(), info.clone());
        super::super::atomic_write(&self.protection, &serde_json::to_vec_pretty(&all)?)
    }

    fn load_receipts(&self) -> Vec<MergeReceipt> {
        std::fs::read(&self.receipts)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn load_protection(&self) -> BTreeMap<String, BTreeMap<String, ProtectionInfo>> {
        std::fs::read(&self.protection)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drafts_round_trip_and_clear_by_pr() {
        let dir = tempfile::tempdir().unwrap();
        let store = ReviewDraftStore::new(dir.path());
        let draft = ReviewDraft {
            id: String::new(),
            repo_key: "repo".into(),
            number: 7,
            head_sha: Some("abc123".into()),
            path: "src/a.ts".into(),
            side: "new".into(),
            line: 12,
            body: "这里要加注释".into(),
            created_at_ms: 0,
            updated_at_ms: 0,
        };
        let saved = store.save(draft).unwrap();
        assert!(!saved.id.is_empty());
        assert_eq!(store.list("repo", 7).len(), 1);
        assert_eq!(store.list("repo", 8).len(), 0);
        store.clear("repo", 7).unwrap();
        assert!(store.list("repo", 7).is_empty());
    }

    #[test]
    fn receipts_and_protection_are_per_repo() {
        let dir = tempfile::tempdir().unwrap();
        let store = ReceiptStore::new(dir.path());
        let receipt = MergeReceipt {
            id: String::new(),
            repo_key: "a".into(),
            number: 1,
            action: "squash".into(),
            head_sha: Some("abc".into()),
            result_sha: Some("def".into()),
            protected_branch: true,
            protection_source: "api".into(),
            notes: vec![],
            created_at_ms: 10,
        };
        store.append(receipt).unwrap();
        assert_eq!(store.list("a").len(), 1);
        assert_eq!(store.list("b").len(), 0);
        store
            .store_protection(
                "a",
                &ProtectionInfo {
                    branch: "main".into(),
                    protected: true,
                    source: "api".into(),
                    checked_at_ms: 5,
                },
            )
            .unwrap();
        assert!(store.protection("a", "main").unwrap().protected);
        assert!(store.protection("a", "dev").is_none());
    }
}
