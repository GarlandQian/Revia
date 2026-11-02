//! 本地审阅草稿的真实目标归属；本地存取不访问账号凭据或API。
use super::{
    pr::PrContext,
    store::{self, ReviewDraft},
    GithubState,
};
use crate::{
    error::{AppError, ErrorKind, Result},
    session::WorktreeSession,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReviewDraftScope {
    pub worktree_id: String,
    pub login: String,
    pub remote: String,
    pub number: u64,
    pub head_sha: String,
    pub key: String,
    pub target_key: String,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReviewDraftBundle {
    pub scope: ReviewDraftScope,
    pub drafts: Vec<ReviewDraft>,
    pub older: Vec<ReviewDraft>,
}
pub fn changed() -> AppError {
    AppError::coded(
        ErrorKind::Rejected,
        "errors.m7.githubDraftTargetChanged",
        "草稿的账号、远端、仓库或提交版本已变化；已保留原稿",
        &[],
    )
}
impl ReviewDraftScope {
    pub fn resolve(
        session: &WorktreeSession,
        login: &str,
        remote: &str,
        number: u64,
        head: &str,
    ) -> Result<Self> {
        if login.is_empty()
            || number == 0
            || head.len() != 40
            || !head.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err(changed());
        }
        let identity = crate::m7::edit::identity_key(session)?;
        let remotes = store::remotes(session)?;
        let entry = remotes
            .iter()
            .find(|entry| entry.name == remote)
            .ok_or_else(changed)?;
        let (Some(owner), Some(repo)) = (&entry.owner, &entry.repo) else {
            return Err(changed());
        };
        let target_key = crate::m7::sha256_hex(&serde_json::to_vec(&(
            identity,
            login.to_ascii_lowercase(),
            remote,
            owner.to_ascii_lowercase(),
            repo.to_ascii_lowercase(),
            number,
        ))?);
        let key = crate::m7::sha256_hex(&serde_json::to_vec(&(&target_key, head))?);
        Ok(Self {
            worktree_id: session.id.clone(),
            login: login.into(),
            remote: remote.into(),
            number,
            head_sha: head.into(),
            key,
            target_key,
        })
    }
    pub fn verify(&self, session: &WorktreeSession) -> Result<()> {
        let actual = Self::resolve(
            session,
            &self.login,
            &self.remote,
            self.number,
            &self.head_sha,
        )?;
        if &actual != self {
            return Err(changed());
        }
        Ok(())
    }
    pub fn verify_publish(&self, context: &PrContext<'_>, number: u64, head: &str) -> Result<()> {
        self.verify(context.session)?;
        if self.login != context.login
            || self.remote != context.remote
            || self.number != number
            || self.head_sha != head
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub fn list(
    session: &WorktreeSession,
    state: &GithubState,
    login: &str,
    remote: &str,
    number: u64,
    head: &str,
) -> Result<ReviewDraftBundle> {
    let scope = ReviewDraftScope::resolve(session, login, remote, number, head)?;
    let all = state.drafts().list_target(&scope.target_key)?;
    let (drafts, older) = all
        .into_iter()
        .partition(|draft| draft.scope_key.as_deref() == Some(&scope.key));
    Ok(ReviewDraftBundle {
        scope,
        drafts,
        older,
    })
}
pub fn save(
    session: &WorktreeSession,
    state: &GithubState,
    scope: &ReviewDraftScope,
    mut draft: ReviewDraft,
) -> Result<ReviewDraft> {
    scope.verify(session)?;
    if draft.body.len() > 65536
        || !matches!(
            draft.kind.as_str(),
            "inline" | "reviewBody" | "commentBody" | "inlineForm"
        )
    {
        return Err(changed());
    }
    if draft.kind == "inline"
        && (draft.path.is_empty() || draft.line == 0 || draft.body.trim().is_empty())
    {
        return Err(changed());
    }
    draft.repo_key = GithubState::repo_key(&session.path);
    draft.number = scope.number;
    draft.head_sha = Some(scope.head_sha.clone());
    draft.scope_key = Some(scope.key.clone());
    draft.target_key = Some(scope.target_key.clone());
    // 不使用前端id认领其他目标的草稿；正文与行内锚点在各自版本内有稳定id。
    draft.id = crate::m7::sha256_hex(&serde_json::to_vec(&(
        &scope.key,
        &draft.kind,
        if draft.kind == "inline" {
            draft.path.as_str()
        } else {
            ""
        },
        if draft.kind == "inline" {
            draft.line
        } else {
            0
        },
        if draft.kind == "inline" {
            draft.side.as_str()
        } else {
            ""
        },
    ))?);
    state.drafts().save(draft)
}
pub fn delete(
    session: &WorktreeSession,
    state: &GithubState,
    scope: &ReviewDraftScope,
    id: &str,
) -> Result<()> {
    scope.verify(session)?;
    state.drafts().delete_scoped(&scope.key, id)
}
