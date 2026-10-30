//! 拉取请求：列表、详情、文件、评论、评审草稿、检出、创建与合并（M7 任务 06–08）。
//!
//! 合并与评审都会写本地回执；合并前核对 head 是否已变化、分支保护是否已确认。
//! 检出 PR 仍然走系统 Git：先把 `refs/pull/N/head` 取到独立引用并核对提交，
//! 再复用既有检出流程（含未提交改动保护与回收）。

use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, ErrorKind, Result};
use crate::git::NETWORK_TIMEOUT;
use crate::remote_ops::{self, CheckoutResult, CheckoutStrategy, CheckoutTarget};
use crate::session::WorktreeSession;

use super::store::{self, MergeReceipt, ProtectionInfo, RemoteInfo, ReviewDraft, PROTECTION_TTL_MS};
use super::{GithubClient, GithubState, RateLimit};

/// PR 列表每页条数（GitHub 上限 100，这里用 30 便于分页）。
const PAGE_SIZE: u32 = 30;

/// PR 列表筛选。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PullRequestFilter {
    Open,
    Closed,
    All,
}

impl PullRequestFilter {
    fn as_query(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
            Self::All => "all",
        }
    }
}

/// 列表里的一个 PR。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PullRequestSummary {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub draft: bool,
    pub merged: bool,
    pub author: String,
    pub head_ref: String,
    pub base_ref: String,
    pub head_sha: String,
    pub updated_at: String,
    pub html_url: String,
    pub additions: u32,
    pub deletions: u32,
    pub changed_files: u32,
}

/// 列表结果（带速率限制）。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PullRequestList {
    pub items: Vec<PullRequestSummary>,
    pub has_more: bool,
    pub rate_limit: RateLimit,
}

/// PR 详情。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PullRequestDetail {
    pub summary: PullRequestSummary,
    pub body: String,
    pub created_at: String,
    pub mergeable: Option<bool>,
    pub mergeable_state: String,
    /// 组合状态（`success` / `pending` / `failure` / 空字符串表示没有检查）。
    pub checks_state: String,
    pub comments: u32,
    pub review_comments: u32,
    pub commits: u32,
    pub base_protected: Option<bool>,
    pub rate_limit: RateLimit,
}

/// PR 改动文件。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PullRequestFile {
    pub filename: String,
    pub previous_filename: Option<String>,
    pub status: String,
    pub additions: u32,
    pub deletions: u32,
    pub changes: u32,
    pub patch: Option<String>,
}

/// 评论类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum CommentKind {
    Issue,
    Review,
}

/// 一条 PR 评论。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PullRequestComment {
    pub id: u64,
    pub kind: CommentKind,
    pub author: String,
    pub body: String,
    pub path: Option<String>,
    pub line: Option<u32>,
    pub created_at: String,
    pub in_reply_to: Option<u64>,
}

/// 评审结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ReviewEvent {
    Comment,
    Approve,
    RequestChanges,
}

impl ReviewEvent {
    fn api_value(self) -> &'static str {
        match self {
            Self::Comment => "COMMENT",
            Self::Approve => "APPROVE",
            Self::RequestChanges => "REQUEST_CHANGES",
        }
    }
}

/// 提交评审时附带的行内评论。
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReviewCommentInput {
    pub path: String,
    /// `new` 或 `old`。
    pub side: String,
    pub line: u32,
    pub body: String,
}

/// 提交评审的请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewSubmitRequest {
    pub worktree_id: String,
    pub number: u64,
    pub event: ReviewEvent,
    pub body: String,
    pub comments: Vec<ReviewCommentInput>,
    /// 提交前核对 PR 的 head；不一致时拒绝。
    pub expected_head_sha: String,
}

/// 评审回执。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReviewReceipt {
    pub receipt: MergeReceipt,
    pub review_url: Option<String>,
}

/// 合并方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum MergeMethod {
    Merge,
    Squash,
    Rebase,
}

impl MergeMethod {
    fn api_value(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Squash => "squash",
            Self::Rebase => "rebase",
        }
    }
}

/// 合并请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeRequest {
    pub worktree_id: String,
    pub number: u64,
    pub method: MergeMethod,
    pub expected_head_sha: String,
    pub commit_title: Option<String>,
    pub commit_message: Option<String>,
    /// 用户已确认“受保护分支 / 检查未通过 / 状态未知”的提示。
    pub confirmed: bool,
}

/// 合并结果。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PrMergeOutcome {
    /// `merged` / `headChanged` / `conflict` / `blocked` / `unknown`。
    pub status: String,
    pub message: String,
    pub sha: Option<String>,
    pub receipt: MergeReceipt,
}

/// 检出 PR 的结果。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PrCheckoutOutcome {
    pub branch: String,
    pub head_sha: String,
    pub result: CheckoutResult,
}

/// 检出 PR 的请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrCheckoutRequest {
    pub worktree_id: String,
    pub remote: String,
    pub number: u64,
    pub expected_head_sha: String,
    pub strategy: CheckoutStrategy,
    /// 本地分支名（默认 `pr/<number>`）。
    pub local_branch: Option<String>,
}

/// 创建 PR 的请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrCreateRequest {
    pub worktree_id: String,
    pub remote: String,
    pub head: String,
    pub base: String,
    pub title: String,
    pub body: String,
    pub draft: bool,
}

/// 创建 PR 的结果。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PrCreated {
    pub number: u64,
    pub html_url: String,
}

/// 已解析的 PR 上下文：仓库、远端、账号与 API 客户端。
pub struct PrContext<'a> {
    pub session: &'a WorktreeSession,
    pub state: &'a GithubState,
    pub client: GithubClient,
    pub login: String,
    pub repo_key: String,
    pub owner: String,
    pub repo: String,
    pub remote: String,
}

impl<'a> PrContext<'a> {
    pub fn new(
        session: &'a WorktreeSession,
        state: &'a GithubState,
        remote_name: Option<&str>,
    ) -> Result<Self> {
        let remotes = store::remotes(session)?;
        let github: Vec<&RemoteInfo> = remotes
            .iter()
            .filter(|remote| remote.owner.is_some() && remote.repo.is_some())
            .collect();
        let remote = match remote_name {
            Some(name) => github
                .iter()
                .find(|remote| remote.name == name)
                .copied()
                .ok_or_else(|| {
                    AppError::coded(
                        ErrorKind::Rejected,
                        "errors.m7.githubRemoteNotGithub",
                        "远端 {remote} 不是 GitHub 仓库",
                        &[("remote", name.to_string())],
                    )
                })?,
            None => github
                .iter()
                .find(|remote| remote.name == "origin")
                .or_else(|| github.first())
                .copied()
                .ok_or_else(|| {
                    AppError::coded(
                        ErrorKind::Rejected,
                        "errors.m7.githubNoRemote",
                        "这个仓库没有 GitHub 远端",
                        &[],
                    )
                })?,
        };
        let (login, client) = state.client_for(&session.path)?;
        Ok(Self {
            session,
            state,
            client,
            login,
            repo_key: GithubState::repo_key(&session.path),
            owner: remote.owner.clone().unwrap_or_default(),
            repo: remote.repo.clone().unwrap_or_default(),
            remote: remote.name.clone(),
        })
    }

    fn repo_path(&self) -> String {
        format!("/repos/{}/{}", self.owner, self.repo)
    }
}

#[derive(Deserialize)]
struct ApiUser {
    #[serde(default)]
    login: String,
}

#[derive(Deserialize)]
struct ApiRef {
    #[serde(default)]
    #[serde(rename = "ref")]
    name: String,
    #[serde(default)]
    sha: String,
}

#[derive(Deserialize)]
struct ApiPull {
    number: u64,
    title: String,
    state: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    merged_at: Option<String>,
    #[serde(default)]
    user: Option<ApiUser>,
    #[serde(default)]
    head: Option<ApiRef>,
    #[serde(default)]
    base: Option<ApiRef>,
    #[serde(default)]
    updated_at: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    additions: u32,
    #[serde(default)]
    deletions: u32,
    #[serde(default)]
    changed_files: u32,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    mergeable: Option<bool>,
    #[serde(default)]
    mergeable_state: Option<String>,
    #[serde(default)]
    comments: u32,
    #[serde(default)]
    review_comments: u32,
    #[serde(default)]
    commits: u32,
}

impl ApiPull {
    fn summary(&self) -> PullRequestSummary {
        PullRequestSummary {
            number: self.number,
            title: self.title.clone(),
            state: self.state.clone(),
            draft: self.draft,
            merged: self.merged_at.is_some(),
            author: self
                .user
                .as_ref()
                .map(|user| user.login.clone())
                .unwrap_or_default(),
            head_ref: self
                .head
                .as_ref()
                .map(|head| head.name.clone())
                .unwrap_or_default(),
            base_ref: self
                .base
                .as_ref()
                .map(|base| base.name.clone())
                .unwrap_or_default(),
            head_sha: self.head.as_ref().map(|head| head.sha.clone()).unwrap_or_default(),
            updated_at: self.updated_at.clone(),
            html_url: self.html_url.clone(),
            additions: self.additions,
            deletions: self.deletions,
            changed_files: self.changed_files,
        }
    }
}

#[derive(Deserialize)]
struct ApiFile {
    filename: String,
    #[serde(default)]
    previous_filename: Option<String>,
    #[serde(default)]
    status: String,
    #[serde(default)]
    additions: u32,
    #[serde(default)]
    deletions: u32,
    #[serde(default)]
    changes: u32,
    #[serde(default)]
    patch: Option<String>,
}

#[derive(Deserialize)]
struct ApiComment {
    id: u64,
    #[serde(default)]
    user: Option<ApiUser>,
    #[serde(default)]
    body: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    line: Option<u32>,
    #[serde(default)]
    original_line: Option<u32>,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    in_reply_to_id: Option<u64>,
}

#[derive(Deserialize)]
struct ApiStatus {
    #[serde(default)]
    state: String,
}

#[derive(Deserialize)]
struct ApiMerge {
    #[serde(default)]
    merged: bool,
    #[serde(default)]
    message: String,
    #[serde(default)]
    sha: Option<String>,
}

#[derive(Deserialize)]
struct ApiReview {
    #[serde(default)]
    html_url: Option<String>,
}

/// 列出 PR。
pub fn list(
    context: &PrContext,
    filter: PullRequestFilter,
    page: u32,
) -> Result<PullRequestList> {
    let page = page.max(1);
    let path = format!(
        "{}/pulls?state={}&sort=updated&direction=desc&per_page={}&page={}",
        context.repo_path(),
        filter.as_query(),
        PAGE_SIZE,
        page
    );
    let (pulls, meta): (Vec<ApiPull>, _) = context.client.get_json(&path)?;
    Ok(PullRequestList {
        has_more: pulls.len() as u32 >= PAGE_SIZE,
        items: pulls.iter().map(ApiPull::summary).collect(),
        rate_limit: meta.rate_limit,
    })
}

/// PR 详情（含合并状态与检查状态）。
pub fn detail(context: &PrContext, number: u64) -> Result<PullRequestDetail> {
    let (pull, meta): (ApiPull, _) =
        context
            .client
            .get_json(&format!("{}/pulls/{number}", context.repo_path()))?;
    let summary = pull.summary();
    let checks_state = context
        .client
        .get_json::<ApiStatus>(&format!(
            "{}/commits/{}/status",
            context.repo_path(),
            summary.head_sha
        ))
        .map(|(status, _)| status.state)
        .unwrap_or_default();
    let base_protected = protection_state(context, &summary.base_ref, false)
        .ok()
        .map(|info| info.protected);
    Ok(PullRequestDetail {
        body: pull.body.unwrap_or_default(),
        created_at: pull.created_at.unwrap_or_default(),
        mergeable: pull.mergeable,
        mergeable_state: pull.mergeable_state.unwrap_or_else(|| "unknown".into()),
        checks_state,
        comments: pull.comments,
        review_comments: pull.review_comments,
        commits: pull.commits,
        base_protected,
        summary,
        rate_limit: meta.rate_limit,
    })
}

/// PR 改动文件。
pub fn files(context: &PrContext, number: u64) -> Result<Vec<PullRequestFile>> {
    let (files, _): (Vec<ApiFile>, _) = context.client.get_json(&format!(
        "{}/pulls/{number}/files?per_page=100",
        context.repo_path()
    ))?;
    Ok(files
        .into_iter()
        .map(|file| PullRequestFile {
            filename: file.filename,
            previous_filename: file.previous_filename,
            status: file.status,
            additions: file.additions,
            deletions: file.deletions,
            changes: file.changes,
            patch: file.patch,
        })
        .collect())
}

/// PR 的 diff 文本（只读展示，不写入工作区）。
pub fn diff(context: &PrContext, number: u64) -> Result<String> {
    let (text, _) = context.client.get_text(
        &format!("{}/pulls/{number}", context.repo_path()),
        "application/vnd.github.v3.diff",
    )?;
    Ok(text)
}

/// PR 的评论（issue 评论 + 行内评审评论）。
pub fn comments(context: &PrContext, number: u64) -> Result<Vec<PullRequestComment>> {
    let (issue, _): (Vec<ApiComment>, _) = context
        .client
        .get_json(&format!("{}/issues/{number}/comments?per_page=100", context.repo_path()))?;
    let (review, _): (Vec<ApiComment>, _) = context.client.get_json(&format!(
        "{}/pulls/{number}/comments?per_page=100",
        context.repo_path()
    ))?;
    let mut all: Vec<PullRequestComment> = issue
        .into_iter()
        .map(|comment| map_comment(comment, CommentKind::Issue))
        .collect();
    all.extend(
        review
            .into_iter()
            .map(|comment| map_comment(comment, CommentKind::Review)),
    );
    all.sort_by(|left, right| left.created_at.cmp(&right.created_at));
    Ok(all)
}

fn map_comment(comment: ApiComment, kind: CommentKind) -> PullRequestComment {
    PullRequestComment {
        id: comment.id,
        kind,
        author: comment
            .user
            .map(|user| user.login)
            .unwrap_or_default(),
        body: comment.body,
        path: comment.path,
        line: comment.line.or(comment.original_line),
        created_at: comment.created_at,
        in_reply_to: comment.in_reply_to_id,
    }
}

/// 发表一条普通评论。
pub fn create_comment(context: &PrContext, number: u64, body: &str) -> Result<()> {
    let body = body.trim();
    if body.is_empty() {
        return Err(rejected(
            "errors.m7.githubCommentEmpty",
            "评论内容不能为空",
        ));
    }
    if body.len() > 65536 {
        return Err(rejected(
            "errors.m7.githubCommentTooLong",
            "评论内容过长",
        ));
    }
    let _: (serde_json::Value, _) = context.client.post_json(
        &format!("{}/issues/{number}/comments", context.repo_path()),
        &serde_json::json!({ "body": body }),
    )?;
    Ok(())
}

/// 本地评审草稿：列出。
pub fn list_drafts(state: &GithubState, repo_key: &str, number: u64) -> Vec<ReviewDraft> {
    state.drafts().list(repo_key, number)
}

/// 本地评审草稿：保存。
pub fn save_draft(state: &GithubState, draft: ReviewDraft) -> Result<ReviewDraft> {
    if draft.body.trim().is_empty() {
        return Err(rejected("errors.m7.githubCommentEmpty", "评论内容不能为空"));
    }
    if draft.path.trim().is_empty() || draft.line == 0 {
        return Err(rejected(
            "errors.m7.githubCommentAnchorInvalid",
            "行内评论需要文件与行号",
        ));
    }
    state.drafts().save(draft)
}

pub fn delete_draft(state: &GithubState, id: &str) -> Result<()> {
    state.drafts().delete(id)
}

/// 提交评审（附带草稿里的行内评论）。
pub fn submit_review(context: &PrContext, request: &ReviewSubmitRequest) -> Result<ReviewReceipt> {
    let body = request.body.trim();
    if request.event != ReviewEvent::Comment && body.is_empty() {
        return Err(rejected(
            "errors.m7.githubReviewBodyRequired",
            "通过或请求修改需要填写说明",
        ));
    }
    if body.len() > 65536 {
        return Err(rejected("errors.m7.githubCommentTooLong", "评论内容过长"));
    }
    let (pull, _): (ApiPull, _) = context
        .client
        .get_json(&format!("{}/pulls/{}", context.repo_path(), request.number))?;
    let head_sha = pull.head.as_ref().map(|head| head.sha.clone()).unwrap_or_default();
    if !request.expected_head_sha.is_empty() && head_sha != request.expected_head_sha {
        return Err(head_changed(&request.expected_head_sha, &head_sha));
    }
    let mut comments = Vec::new();
    for comment in &request.comments {
        if comment.body.trim().is_empty() {
            return Err(rejected("errors.m7.githubCommentEmpty", "评论内容不能为空"));
        }
        if comment.line == 0 || comment.path.trim().is_empty() {
            return Err(rejected(
                "errors.m7.githubCommentAnchorInvalid",
                "行内评论需要文件与行号",
            ));
        }
        let side = match comment.side.as_str() {
            "old" => "LEFT",
            _ => "RIGHT",
        };
        comments.push(serde_json::json!({
            "path": comment.path,
            "line": comment.line,
            "side": side,
            "body": comment.body,
        }));
    }
    let payload = serde_json::json!({
        "body": body,
        "event": request.event.api_value(),
        "comments": comments,
    });
    let (review, _): (ApiReview, _) = context.client.post_json(
        &format!("{}/pulls/{}/reviews", context.repo_path(), request.number),
        &payload,
    )?;
    let receipt = context.state.receipts().append(MergeReceipt {
        id: String::new(),
        repo_key: context.repo_key.clone(),
        number: request.number,
        action: format!("review:{}", request.event.api_value().to_lowercase()),
        head_sha: Some(head_sha),
        result_sha: None,
        protected_branch: false,
        protection_source: "api".into(),
        notes: vec![format!("account={}", context.login)],
        created_at_ms: super::super::now_ms(),
    })?;
    // 提交成功后清掉本 PR 的本地草稿：它们已经变成远端评论。
    let _ = context
        .state
        .drafts()
        .clear(&context.repo_key, request.number);
    Ok(ReviewReceipt {
        receipt,
        review_url: review.html_url,
    })
}

/// 创建 PR。
pub fn create(context: &PrContext, request: &PrCreateRequest) -> Result<PrCreated> {
    let title = request.title.trim();
    if title.is_empty() {
        return Err(rejected("errors.m7.githubPrTitleEmpty", "PR 标题不能为空"));
    }
    if title.len() > 256 {
        return Err(rejected("errors.m7.githubPrTitleTooLong", "PR 标题过长"));
    }
    if request.body.len() > 65536 {
        return Err(rejected("errors.m7.githubPrBodyTooLong", "PR 描述过长"));
    }
    validate_branch(request.head.trim())?;
    validate_branch(request.base.trim())?;
    let payload = serde_json::json!({
        "title": title,
        "head": request.head.trim(),
        "base": request.base.trim(),
        "body": request.body,
        "draft": request.draft,
    });
    let (pull, _): (ApiPull, _) = context
        .client
        .post_json(&format!("{}/pulls", context.repo_path()), &payload)?;
    Ok(PrCreated {
        number: pull.number,
        html_url: pull.html_url,
    })
}

/// 检出 PR：取 `refs/pull/N/head`，核对提交后复用既有检出流程。
pub fn checkout(
    context: &PrContext,
    request: &PrCheckoutRequest,
    recovery: &crate::recovery::RecoveryStore,
    archive: &crate::stash_archive::StashArchiveStore,
) -> Result<PrCheckoutOutcome> {
    let (pull, _): (ApiPull, _) = context
        .client
        .get_json(&format!("{}/pulls/{}", context.repo_path(), request.number))?;
    let head_sha = pull.head.as_ref().map(|head| head.sha.clone()).unwrap_or_default();
    if head_sha != request.expected_head_sha {
        return Err(head_changed(&request.expected_head_sha, &head_sha));
    }
    let branch = request
        .local_branch
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("pr/{}", request.number));
    validate_branch(&branch)?;

    // 取到独立命名空间，避免直接写 heads 时与已检出分支冲突。
    let tracking_ref = format!("refs/revia/pr/{}", request.number);
    let refspec = format!("+refs/pull/{}/head:{tracking_ref}", request.number);
    let fetched = context.session.git.run_network(
        &["fetch", &request.remote, &refspec],
        NETWORK_TIMEOUT,
    )?;
    if fetched.exit_code != 0 {
        return Err(AppError::coded(
            ErrorKind::Git,
            "errors.m7.githubFetchPrFailed",
            "取 PR 失败：{error}",
            &[("error", fetched.stderr.trim().to_string())],
        ));
    }
    let resolved = context
        .session
        .git
        .run_ok(&["rev-parse", &tracking_ref], "核对 PR 提交")?;
    let resolved_sha = String::from_utf8_lossy(&resolved.stdout).trim().to_string();
    if resolved_sha != request.expected_head_sha {
        return Err(head_changed(&request.expected_head_sha, &resolved_sha));
    }

    let current = context
        .session
        .git
        .run_ok(&["rev-parse", "--abbrev-ref", "HEAD"], "读取当前分支")?;
    let current_branch = String::from_utf8_lossy(&current.stdout).trim().to_string();
    let exists = context
        .session
        .git
        .run_ok(
            &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")],
            "检查本地分支",
        )
        .map(|output| !output.stdout.is_empty())
        .unwrap_or(false);
    if exists && current_branch == branch {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.githubPrBranchCheckedOut",
            "本地分支 {branch} 已检出，请先切换到其他分支再检出 PR",
            &[("branch", branch)],
        ));
    }
    let branch_args: Vec<String> = if exists {
        vec!["branch".into(), "-f".into(), branch.clone(), resolved_sha.clone()]
    } else {
        vec!["branch".into(), branch.clone(), resolved_sha.clone()]
    };
    let refs: Vec<&str> = branch_args.iter().map(String::as_str).collect();
    let created = context.session.git.run_write(&refs)?;
    if created.exit_code != 0 {
        return Err(AppError::coded(
            ErrorKind::Git,
            "errors.m7.githubPrBranchFailed",
            "创建本地分支失败：{error}",
            &[("error", created.stderr.trim().to_string())],
        ));
    }
    let result = remote_ops::checkout(
        context.session,
        recovery,
        &CheckoutTarget::Local { name: branch.clone() },
        request.strategy,
        archive,
    )?;
    Ok(PrCheckoutOutcome {
        branch,
        head_sha: resolved_sha,
        result,
    })
}

/// 合并 PR：核对 head、保护状态与合并状态，写回执。
pub fn merge(context: &PrContext, request: &MergeRequest) -> Result<PrMergeOutcome> {
    let (pull, _): (ApiPull, _) = context
        .client
        .get_json(&format!("{}/pulls/{}", context.repo_path(), request.number))?;
    let summary = pull.summary();
    let head_sha = summary.head_sha.clone();
    if !request.expected_head_sha.is_empty() && head_sha != request.expected_head_sha {
        let receipt = context.state.receipts().append(MergeReceipt {
            id: String::new(),
            repo_key: context.repo_key.clone(),
            number: request.number,
            action: request.method.api_value().to_string(),
            head_sha: Some(request.expected_head_sha.clone()),
            result_sha: None,
            protected_branch: false,
            protection_source: "api".into(),
            notes: vec!["headChanged".into()],
            created_at_ms: super::super::now_ms(),
        })?;
        return Err(head_changed_error(&request.expected_head_sha, &head_sha, receipt));
    }
    let protection = protection_state(context, &summary.base_ref, true)?;
    let state = pull.mergeable_state.clone().unwrap_or_else(|| "unknown".into());
    let requires_confirmation =
        protection.protected || !matches!(state.as_str(), "clean" | "has_hooks");
    if requires_confirmation && !request.confirmed {
        let receipt = context.state.receipts().append(MergeReceipt {
            id: String::new(),
            repo_key: context.repo_key.clone(),
            number: request.number,
            action: request.method.api_value().to_string(),
            head_sha: Some(head_sha.clone()),
            result_sha: None,
            protected_branch: protection.protected,
            protection_source: protection.source.clone(),
            notes: vec![format!("mergeableState={state}")],
            created_at_ms: super::super::now_ms(),
        })?;
        return Err(AppError::coded(
            ErrorKind::Rejected,
            if protection.protected {
                "errors.m7.githubProtectedBranchConfirm"
            } else {
                "errors.m7.githubMergeNeedsConfirm"
            },
            "合并前需要确认：{reason}",
            &[
                ("reason", format!("{state}/{source}", source = protection.source)),
                ("receiptId", receipt.id),
            ],
        ));
    }
    let payload = serde_json::json!({
        "merge_method": request.method.api_value(),
        "sha": head_sha,
        "commit_title": request.commit_title,
        "commit_message": request.commit_message,
    });
    let response = context
        .client
        .put_json::<ApiMerge, _>(
            &format!("{}/pulls/{}/merge", context.repo_path(), request.number),
            &payload,
        );
    let (status, message, sha) = match response {
        Ok((merged, _)) if merged.merged => ("merged".to_string(), merged.message, merged.sha),
        Ok((merged, _)) => ("unknown".to_string(), merged.message, merged.sha),
        Err(error) => {
            let code = error.code.unwrap_or("errors.m7.githubRequestFailed");
            let status = match code {
                "errors.m7.githubNotFound" => "headChanged",
                "errors.m7.githubValidationFailed" => "conflict",
                "errors.m7.githubMergeBlocked" => "blocked",
                _ => "unknown",
            };
            let receipt = context.state.receipts().append(MergeReceipt {
                id: String::new(),
                repo_key: context.repo_key.clone(),
                number: request.number,
                action: request.method.api_value().to_string(),
                head_sha: Some(head_sha),
                result_sha: None,
                protected_branch: protection.protected,
                protection_source: protection.source.clone(),
                notes: vec![format!("error={code}"), format!("mergeableState={state}")],
                created_at_ms: super::super::now_ms(),
            })?;
            return Ok(PrMergeOutcome {
                status: status.to_string(),
                message: error.message,
                sha: None,
                receipt,
            });
        }
    };
    let receipt = context.state.receipts().append(MergeReceipt {
        id: String::new(),
        repo_key: context.repo_key.clone(),
        number: request.number,
        action: request.method.api_value().to_string(),
        head_sha: Some(head_sha),
        result_sha: sha.clone(),
        protected_branch: protection.protected,
        protection_source: protection.source.clone(),
        notes: vec![format!("mergeableState={state}"), format!("account={}", context.login)],
        created_at_ms: super::super::now_ms(),
    })?;
    Ok(PrMergeOutcome {
        status,
        message,
        sha,
        receipt,
    })
}

/// 核对分支保护状态：有缓存用缓存，过期则重新取；取不到时按“未知”处理。
pub fn protection_state(
    context: &PrContext,
    branch: &str,
    refresh_if_stale: bool,
) -> Result<ProtectionInfo> {
    if branch.trim().is_empty() {
        return Ok(ProtectionInfo {
            branch: branch.to_string(),
            protected: false,
            source: "unknown".into(),
            checked_at_ms: super::super::now_ms(),
        });
    }
    let now = super::super::now_ms();
    if let Some(cached) = context.state.receipts().protection(&context.repo_key, branch) {
        if !refresh_if_stale || now.saturating_sub(cached.checked_at_ms) < PROTECTION_TTL_MS {
            return Ok(cached);
        }
    }
    let path = format!("{}/branches/{branch}/protection", context.repo_path());
    let info = match context.client.get_json::<serde_json::Value>(&path) {
        Ok(_) => ProtectionInfo {
            branch: branch.to_string(),
            protected: true,
            source: "api".into(),
            checked_at_ms: now,
        },
        Err(error) => match error.code {
            Some("errors.m7.githubNotFound") => ProtectionInfo {
                branch: branch.to_string(),
                protected: false,
                source: "api".into(),
                checked_at_ms: now,
            },
            // 权限不足或网络问题时不能断言“未受保护”。
            _ => ProtectionInfo {
                branch: branch.to_string(),
                protected: true,
                source: "unknown".into(),
                checked_at_ms: now,
            },
        },
    };
    let _ = context.state.receipts().store_protection(&context.repo_key, &info);
    Ok(info)
}

fn head_changed(expected: &str, actual: &str) -> AppError {
    AppError::coded(
        ErrorKind::Rejected,
        "errors.m7.githubHeadChanged",
        "PR 的提交已变化（期望 {expected}，实际 {actual}），请刷新后重试",
        &[
            ("expected", expected.to_string()),
            ("actual", actual.to_string()),
        ],
    )
}

fn head_changed_error(expected: &str, actual: &str, receipt: MergeReceipt) -> AppError {
    AppError::coded(
        ErrorKind::Rejected,
        "errors.m7.githubHeadChanged",
        "PR 的提交已变化（期望 {expected}，实际 {actual}），请刷新后重试",
        &[
            ("expected", expected.to_string()),
            ("actual", actual.to_string()),
            ("receiptId", receipt.id),
        ],
    )
}

fn rejected(code: &'static str, message: &'static str) -> AppError {
    AppError::coded(ErrorKind::Rejected, code, message, &[])
}

/// 分支名形状校验（不查 Git，只拒绝明显非法或像选项的名字）。
fn validate_branch(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 255
        || name.starts_with('-')
        || name.starts_with('/')
        || name.ends_with('/')
        || name.contains("..")
        || name.contains("//")
        || name
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace() || "~^:?*[\\".contains(ch))
    {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.githubBranchNameInvalid",
            "分支名不合法：{name}",
            &[("name", name.to_string())],
        ));
    }
    Ok(())
}

/// 读一个目录下的文件名（供界面在 PR 列表里显示本地草稿数量等；只读）。
pub fn repo_key_for(path: &Path) -> String {
    GithubState::repo_key(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_names_are_validated() {
        assert!(validate_branch("pr/12").is_ok());
        assert!(validate_branch("feature/a-b_c").is_ok());
        assert!(validate_branch("-x").is_err());
        assert!(validate_branch("a..b").is_err());
        assert!(validate_branch("a b").is_err());
        assert!(validate_branch("a~b").is_err());
        assert!(validate_branch("").is_err());
    }

    #[test]
    fn merge_confirmation_rule_requires_confirm_for_protected_or_non_clean() {
        let needs = |protected: bool, state: &str| {
            (!protected && !matches!(state, "clean" | "has_hooks")) || protected
        };
        assert!(needs(true, "clean"));
        assert!(needs(false, "unknown"));
        assert!(needs(false, "blocked"));
        assert!(!needs(false, "clean"));
        assert!(!needs(false, "has_hooks"));
    }
}
