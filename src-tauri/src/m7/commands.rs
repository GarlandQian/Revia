//! M7 的 Tauri 命令层：只做参数转交、会话解析与阻塞任务调度。
//!
//! 前端不拼 Git 参数、不接触文件系统与网络；每个命令都对应一个带类型的操作。

use std::path::PathBuf;
use std::sync::Arc;

use tauri::State;

use crate::commands::AppState;
use crate::error::{AppError, ErrorKind, Result};
use crate::session::WorktreeSession;
use crate::settings::{Settings, SettingsPatch};

use super::clone::CloneRequest;
use super::diagnostics::{DiagnosticsExport, DiagnosticsPreview, DiagnosticsRequest};
use super::edit::{EditDraft, EditSaveRequest, EditSaved, EditableFile};
use super::github::auth::{DevicePoll, DeviceStart};
use super::github::pr::{
    self, MergeRequest, PrCheckoutOutcome, PrCheckoutRequest, PrContext, PrCreateRequest,
    PrCreated, PrMergeOutcome, PullRequestComment, PullRequestDetail, PullRequestFile,
    PullRequestFilter, PullRequestList, ReviewReceipt, ReviewSubmitRequest,
};
use super::github::store::{MergeReceipt, ProtectionInfo, RemoteInfo, ReviewDraft};
use super::github::reviews::{self, ReviewDraftScope, ReviewDraftBundle};
use super::github::{GithubAccount, GithubState, GithubStatus};
use super::terminal::{TerminalOpenRequest, TerminalSessionInfo, TerminalTabRecord};
use super::theme::{ImportedTheme, ThemeKind};
use super::update::{AppUpdateStatus, UpdateState};

fn internal(error: impl std::fmt::Display) -> AppError {
    AppError::coded(
        ErrorKind::Internal,
        "errors.m7.taskFailed",
        "后台任务失败：{error}",
        &[("error", error.to_string())],
    )
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    // 命令本身是 async：必须 await，不能在运行时线程上 block_on（会 panic）。
    tauri::async_runtime::spawn_blocking(work).await.map_err(internal)?
}

fn session_for(state: &AppState, worktree_id: &str) -> Result<Arc<WorktreeSession>> {
    state.worktree_session(worktree_id)
}

// ---------- 颜色主题（任务 02） ----------

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportThemeRequest {
    pub path: String,
    pub kind: Option<ThemeKind>,
    pub label: Option<String>,
}

#[tauri::command]
pub async fn m7_list_imported_themes(state: State<'_, AppState>) -> Result<Vec<ImportedTheme>> {
    let store = Arc::clone(&state.m7_themes);
    blocking(move || store.list()).await
}

#[tauri::command]
pub async fn m7_import_theme(
    state: State<'_, AppState>,
    request: ImportThemeRequest,
) -> Result<ImportedTheme> {
    let store = Arc::clone(&state.m7_themes);
    blocking(move || {
        store.import(
            std::path::Path::new(&request.path),
            request.kind,
            request.label.as_deref(),
        )
    }).await
}

/// 删除导入主题；如果它正被选中，回退到对应的内置主题。
#[tauri::command]
pub async fn m7_remove_imported_theme(state: State<'_, AppState>, id: String) -> Result<Settings> {
    let store = Arc::clone(&state.m7_themes);
    let removed = store.load(&id).ok();
    store.remove(&id)?;
    let mut registry = state.registry.lock().map_err(|_| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.repositoryListLockFailed7",
            "仓库列表锁失败",
            &[],
        )
    })?;
    let settings = registry.settings().clone();
    let mut patch = SettingsPatch::default();
    if settings.theme_light == id {
        patch.theme_light = Some(crate::settings::DEFAULT_LIGHT_THEME.to_string());
    }
    if settings.theme_dark == id {
        patch.theme_dark = Some(crate::settings::DEFAULT_DARK_THEME.to_string());
    }
    let _ = removed;
    if patch.theme_light.is_none() && patch.theme_dark.is_none() {
        return Ok(settings);
    }
    registry.update_settings(&patch)
}

// ---------- 工作区编辑（任务 03） ----------

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditOpenRequest {
    pub worktree_id: String,
    pub path: String,
}

#[tauri::command]
pub async fn m7_open_editable(
    state: State<'_, AppState>,
    request: EditOpenRequest,
) -> Result<EditableFile> {
    let session = session_for(&state, &request.worktree_id)?;
    blocking(move || super::edit::open(&session, &request.path)).await
}

#[tauri::command]
pub async fn m7_save_editable(
    state: State<'_, AppState>,
    request: EditSaveRequest,
) -> Result<EditSaved> {
    let session = session_for(&state, &request.worktree_id)?;
    blocking(move || super::edit::save(&session, &request)).await
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditDraftRef {
    pub worktree_id: String,
    pub path: String,
    #[serde(default)]
    pub worktree_identity: Option<String>,
}

#[tauri::command]
pub async fn m7_save_edit_draft(state: State<'_, AppState>, draft: EditDraft) -> Result<EditDraft> {
    let store = Arc::clone(&state.m7_edit_drafts);
    let session = session_for(&state, &draft.worktree_id)?;
    blocking(move || {
        if draft.worktree_identity != super::edit::identity_key(&session)? {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.session.worktreeIdentityChanged",
                "worktree 身份已变化，请重新打开仓库后再操作",
                &[],
            ));
        }
        store.save(&draft)?;
        Ok(draft)
    }).await
}

#[tauri::command]
pub async fn m7_load_edit_draft(
    state: State<'_, AppState>,
    request: EditDraftRef,
) -> Result<Option<EditDraft>> {
    let store = Arc::clone(&state.m7_edit_drafts);
    let session = session_for(&state, &request.worktree_id)?;
    blocking(move || {
        store.load(
            &request.worktree_id,
            &request.path,
            &super::edit::identity_key(&session)?,
        )
    })
    .await
}

#[tauri::command]
pub async fn m7_clear_edit_draft(state: State<'_, AppState>, request: EditDraftRef) -> Result<()> {
    let store = Arc::clone(&state.m7_edit_drafts);
    let identity = request.worktree_identity.ok_or_else(|| {
        AppError::coded(
            ErrorKind::Rejected,
            "errors.session.worktreeIdentityChanged",
            "worktree 身份已变化，请重新打开仓库后再操作",
            &[],
        )
    })?;
    blocking(move || store.clear(&request.worktree_id, &request.path, &identity)).await
}

#[tauri::command]
pub async fn m7_list_edit_drafts(
    state: State<'_, AppState>,
    worktree_id: String,
) -> Result<Vec<EditDraft>> {
    let store = Arc::clone(&state.m7_edit_drafts);
    // 不认领已删除/替换仓库的旧草稿；原文件保留，不能自动继承到新身份。
    let Some(session) = session_for(&state, &worktree_id).ok() else {
        return Ok(Vec::new());
    };
    blocking(move || match super::edit::identity_key(&session) {
        Ok(identity) => store.list(&worktree_id, &identity),
        Err(_) => Ok(Vec::new()),
    })
    .await
}

// ---------- 内置终端（任务 04） ----------

#[tauri::command]
pub async fn m7_terminal_open(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    request: TerminalOpenRequest,
) -> Result<TerminalSessionInfo> {
    let session = session_for(&state, &request.worktree_id)?;
    let manager = Arc::clone(&state.m7_terminal);
    let cwd = request.cwd.clone();
    blocking(move || {
        session.require_identity()?;
        manager.open(
            &app,
            &session.id,
            &session.path,
            &session.shell_env,
            cwd.as_deref(),
            request.rows,
            request.cols,
        )
    }).await
}

#[tauri::command]
pub async fn m7_terminal_list(state: State<'_, AppState>) -> Result<Vec<TerminalSessionInfo>> {
    Ok(state.m7_terminal.list())
}

#[tauri::command]
pub async fn m7_terminal_write(
    state: State<'_, AppState>,
    session_id: u64,
    data: String,
) -> Result<()> {
    let owner = state.m7_terminal.owner(session_id)?;
    let session = session_for(&state, &owner)?;
    if *session.writing.lock().unwrap_or_else(|error| error.into_inner()) {
        return Err(AppError::coded(ErrorKind::Rejected, "errors.m7.terminalWorktreeBusy",
            "此 worktree 正在执行写操作，请完成后再输入终端", &[]));
    }
    state.m7_terminal.write_input(session_id, &data)
}

#[tauri::command]
pub async fn m7_terminal_resize(
    state: State<'_, AppState>,
    session_id: u64,
    rows: u16,
    cols: u16,
) -> Result<()> {
    state.m7_terminal.resize(session_id, rows, cols)
}

#[tauri::command]
pub async fn m7_terminal_snapshot(state: State<'_, AppState>, session_id: u64) -> Result<String> {
    state.m7_terminal.snapshot(session_id)
}

#[tauri::command]
pub async fn m7_terminal_close(state: State<'_, AppState>, session_id: u64) -> Result<()> {
    state.m7_terminal.close(session_id)
}

#[tauri::command]
pub async fn m7_terminal_close_worktree(
    state: State<'_, AppState>,
    worktree_id: String,
) -> Result<Vec<u64>> {
    Ok(state.m7_terminal.close_worktree(&worktree_id))
}

#[tauri::command]
pub async fn m7_terminal_tabs(state: State<'_, AppState>) -> Result<Vec<TerminalTabRecord>> {
    let store = Arc::clone(&state.m7_terminal_tabs);
    blocking(move || store.load()).await
}

#[tauri::command]
pub async fn m7_terminal_save_tabs(
    state: State<'_, AppState>,
    tabs: Vec<TerminalTabRecord>,
) -> Result<()> {
    let store = Arc::clone(&state.m7_terminal_tabs);
    blocking(move || store.save(tabs)).await
}

// ---------- 克隆（任务 05） ----------

#[tauri::command]
pub async fn m7_clone_start(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    request: CloneRequest,
) -> Result<u64> {
    let home = crate::shell::home_dir()?;
    let target = PathBuf::from(request.target.trim());
    let parent = target.parent().unwrap_or(&home).to_path_buf();
    let shell_env = crate::shell::resolve(&home, &parent);
    let git_program = crate::commands::settings_snapshot(&state)?.git_path.map(PathBuf::from);
    state.m7_clone.start(app, request, shell_env, git_program)
}

#[tauri::command]
pub async fn m7_clone_cancel(state: State<'_, AppState>, job_id: u64) -> Result<()> {
    state.m7_clone.cancel(job_id)
}

#[tauri::command]
pub async fn m7_clone_active(state: State<'_, AppState>) -> Result<Vec<u64>> {
    Ok(state.m7_clone.active())
}

/// 克隆成功后登记仓库（与“添加仓库”使用同一套校验）。
#[tauri::command]
pub async fn m7_clone_register(
    state: State<'_, AppState>,
    path: String,
) -> Result<crate::session::RepoEntry> {
    let mut registry = state.registry.lock().map_err(|_| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.repositoryListLockFailed6",
            "仓库列表锁失败",
            &[],
        )
    })?;
    registry.add(&path)
}

// ---------- GitHub（任务 06–08） ----------

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubRepoRequest {
    pub worktree_id: String,
    #[serde(default)]
    pub remote: Option<String>,
}

#[tauri::command]
pub async fn m7_github_status(
    state: State<'_, AppState>,
    worktree_id: Option<String>,
) -> Result<GithubStatus> {
    let github = Arc::clone(&state.m7_github);
    let repo_path = match worktree_id {
        Some(id) => Some(state.worktree_session(&id)?.path.clone()),
        None => None,
    };
    Ok(GithubStatus {
        device_flow_configured: super::github::client_id().is_some(),
        accounts: github.accounts(),
        repo_account: repo_path
            .as_deref()
            .and_then(|path| github.repo_account(path)),
    })
}

#[tauri::command]
pub async fn m7_github_device_start(state: State<'_, AppState>) -> Result<DeviceStart> {
    let github = Arc::clone(&state.m7_github);
    blocking(move || super::github::auth::start_device_flow(&github)).await
}

#[tauri::command]
pub async fn m7_github_device_open() -> Result<()> {
    blocking(crate::platform::open_github_device_page).await
}

#[tauri::command]
pub async fn m7_github_device_poll(
    state: State<'_, AppState>,
    flow_id: String,
) -> Result<DevicePoll> {
    let github = Arc::clone(&state.m7_github);
    blocking(move || super::github::auth::poll_device_flow(&github, &flow_id)).await
}

#[tauri::command]
pub async fn m7_github_login_token(
    state: State<'_, AppState>,
    token: String,
) -> Result<GithubAccount> {
    let github = Arc::clone(&state.m7_github);
    blocking(move || {
        super::github::auth::login_with_token(
            &github,
            &token,
            None,
            super::github::GithubAuthMethod::Token,
        )
    }).await
}

#[tauri::command]
pub async fn m7_github_logout(state: State<'_, AppState>, login: String) -> Result<()> {
    let github = Arc::clone(&state.m7_github);
    blocking(move || super::github::auth::logout(&github, &login)).await
}

#[tauri::command]
pub async fn m7_github_bind_repo(
    state: State<'_, AppState>,
    worktree_id: String,
    login: String,
) -> Result<()> {
    let session = session_for(&state, &worktree_id)?;
    let github = Arc::clone(&state.m7_github);
    blocking(move || github.bind_repo(&session.path, &login)).await
}

#[tauri::command]
pub async fn m7_github_remotes(
    state: State<'_, AppState>,
    worktree_id: String,
) -> Result<Vec<RemoteInfo>> {
    let session = session_for(&state, &worktree_id)?;
    blocking(move || super::github::store::remotes(&session)).await
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrListRequest {
    pub worktree_id: String,
    #[serde(default)]
    pub remote: Option<String>,
    pub filter: PullRequestFilter,
    pub page: u32,
}

#[tauri::command]
pub async fn m7_github_pr_list(
    state: State<'_, AppState>,
    request: PrListRequest,
) -> Result<PullRequestList> {
    let session = session_for(&state, &request.worktree_id)?;
    let github = Arc::clone(&state.m7_github);
    blocking(move || {
        let context = PrContext::new(&session, &github, request.remote.as_deref())?;
        pr::list(&context, request.filter, request.page)
    }).await
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrNumberRequest {
    pub worktree_id: String,
    #[serde(default)]
    pub login: Option<String>,
    #[serde(default)]
    pub remote: Option<String>,
    pub number: u64,
    /// F09：按此 head 过滤审阅草稿；PR 更新后旧 head 草稿不进入当前列表。
    #[serde(default)]
    pub head_sha: Option<String>,
}

#[tauri::command]
pub async fn m7_github_pr_detail(
    state: State<'_, AppState>,
    request: PrNumberRequest,
) -> Result<PullRequestDetail> {
    let session = session_for(&state, &request.worktree_id)?;
    let github = Arc::clone(&state.m7_github);
    blocking(move || {
        let context = PrContext::new(&session, &github, request.remote.as_deref())?;
        pr::detail(&context, request.number)
    }).await
}

#[tauri::command]
pub async fn m7_github_pr_files(
    state: State<'_, AppState>,
    request: PrNumberRequest,
) -> Result<Vec<PullRequestFile>> {
    let session = session_for(&state, &request.worktree_id)?;
    let github = Arc::clone(&state.m7_github);
    blocking(move || {
        let context = PrContext::new(&session, &github, request.remote.as_deref())?;
        pr::files(&context, request.number)
    }).await
}

#[tauri::command]
pub async fn m7_github_pr_diff(
    state: State<'_, AppState>,
    request: PrNumberRequest,
) -> Result<String> {
    let session = session_for(&state, &request.worktree_id)?;
    let github = Arc::clone(&state.m7_github);
    blocking(move || {
        let context = PrContext::new(&session, &github, request.remote.as_deref())?;
        pr::diff(&context, request.number)
    }).await
}

#[tauri::command]
pub async fn m7_github_pr_comments(
    state: State<'_, AppState>,
    request: PrNumberRequest,
) -> Result<Vec<PullRequestComment>> {
    let session = session_for(&state, &request.worktree_id)?;
    let github = Arc::clone(&state.m7_github);
    blocking(move || {
        let context = PrContext::new(&session, &github, request.remote.as_deref())?;
        pr::comments(&context, request.number)
    }).await
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrCommentRequest {
    pub worktree_id: String,
    pub draft_scope: ReviewDraftScope,
    #[serde(default)]
    pub remote: Option<String>,
    pub number: u64,
    pub body: String,
}

#[tauri::command]
pub async fn m7_github_pr_comment_create(
    state: State<'_, AppState>,
    request: PrCommentRequest,
) -> Result<()> {
    let session = session_for(&state, &request.worktree_id)?;
    let github = Arc::clone(&state.m7_github);
    blocking(move || {
        let context = PrContext::new(&session, &github, request.remote.as_deref())?;
        pr::create_comment(&context, request.number, &request.body, &request.draft_scope)
    }).await
}

#[tauri::command]
pub async fn m7_github_review_drafts(state: State<'_, AppState>, request: PrNumberRequest) -> Result<ReviewDraftBundle> {
    let session = session_for(&state, &request.worktree_id)?;
    let github = Arc::clone(&state.m7_github);
    blocking(move || reviews::list(&session, &github,
        request.login.as_deref().ok_or_else(reviews::changed)?,
        request.remote.as_deref().ok_or_else(reviews::changed)?, request.number,
        request.head_sha.as_deref().ok_or_else(reviews::changed)?)).await
}
#[tauri::command]
pub async fn m7_github_review_draft_save(state: State<'_, AppState>, scope: ReviewDraftScope, draft: ReviewDraft) -> Result<ReviewDraft> {
    let session = session_for(&state, &scope.worktree_id)?;
    let github = Arc::clone(&state.m7_github);
    blocking(move || reviews::save(&session, &github, &scope, draft)).await
}
#[tauri::command]
pub async fn m7_github_review_draft_delete(state: State<'_, AppState>, scope: ReviewDraftScope, id: String) -> Result<()> {
    let session = session_for(&state, &scope.worktree_id)?;
    let github = Arc::clone(&state.m7_github);
    blocking(move || reviews::delete(&session, &github, &scope, &id)).await
}

#[tauri::command]
pub async fn m7_github_review_submit(
    state: State<'_, AppState>,
    request: ReviewSubmitRequest,
) -> Result<ReviewReceipt> {
    let session = session_for(&state, &request.worktree_id)?;
    let github = Arc::clone(&state.m7_github);
    blocking(move || {
        // F07：审阅必须落在用户所选的远端仓库上；请求缺 remote 时拒绝而不是回退 origin。
        let context = PrContext::new(&session, &github, request.remote.as_deref())?;
        pr::submit_review(&context, &request)
    }).await
}

#[tauri::command]
pub async fn m7_github_pr_checkout(
    state: State<'_, AppState>,
    request: PrCheckoutRequest,
) -> Result<PrCheckoutOutcome> {
    let session = session_for(&state, &request.worktree_id)?;
    let github = Arc::clone(&state.m7_github);
    let recovery = state.recovery.clone();
    let archive = state.stash_archive.clone();
    blocking(move || {
        let context = PrContext::new(&session, &github, Some(&request.remote))?;
        pr::checkout(&context, &request, &recovery, &archive)
    }).await
}

#[tauri::command]
pub async fn m7_github_pr_create(
    state: State<'_, AppState>,
    request: PrCreateRequest,
) -> Result<PrCreated> {
    let session = session_for(&state, &request.worktree_id)?;
    let github = Arc::clone(&state.m7_github);
    blocking(move || {
        let context = PrContext::new(&session, &github, Some(&request.remote))?;
        pr::create(&context, &request)
    }).await
}

#[tauri::command]
pub async fn m7_github_pr_merge(
    state: State<'_, AppState>,
    request: MergeRequest,
) -> Result<PrMergeOutcome> {
    let session = session_for(&state, &request.worktree_id)?;
    let github = Arc::clone(&state.m7_github);
    blocking(move || {
        // F07：合并必须落在用户所选的远端仓库上；请求缺 remote 时拒绝而不是回退 origin。
        let context = PrContext::new(&session, &github, request.remote.as_deref())?;
        pr::merge(&context, &request)
    }).await
}

/// F09：前端保存草稿需要与后端一致的 repoKey（worktree 规范路径 SHA）。
/// F16：取消进行中的设备码授权。
#[tauri::command]
pub async fn m7_github_device_cancel(state: State<'_, AppState>, flow_id: String) -> Result<()> {
    let github = Arc::clone(&state.m7_github);
    blocking(move || super::github::auth::cancel_device_flow(&github, &flow_id)).await
}

#[tauri::command]
pub async fn m7_github_repo_key(state: State<'_, AppState>, worktree_id: String) -> Result<String> {
    let session = session_for(&state, &worktree_id)?;
    Ok(GithubState::repo_key(&session.path))
}

#[tauri::command]
pub async fn m7_github_receipts(
    state: State<'_, AppState>,
    worktree_id: String,
) -> Result<Vec<MergeReceipt>> {
    let repo_key = GithubState::repo_key(&session_for(&state, &worktree_id)?.path);
    Ok(state.m7_github.receipts().list(&repo_key))
}

#[tauri::command]
pub async fn m7_github_protection_refresh(
    state: State<'_, AppState>,
    request: GithubRepoRequest,
    branch: String,
) -> Result<ProtectionInfo> {
    let session = session_for(&state, &request.worktree_id)?;
    let github = Arc::clone(&state.m7_github);
    blocking(move || {
        let context = PrContext::new(&session, &github, request.remote.as_deref())?;
        pr::protection_state(&context, &branch, true)
    }).await
}

// ---------- 诊断（任务 09） ----------

#[tauri::command]
pub async fn m7_preview_diagnostics(
    state: State<'_, AppState>,
    request: DiagnosticsRequest,
) -> Result<DiagnosticsPreview> {
    let settings = crate::commands::settings_snapshot(&state)?;
    let session = match &request.worktree_id {
        Some(id) => Some(session_for(&state, id)?),
        None => None,
    };
    let command_log = Arc::clone(&state.command_log);
    let summaries = Arc::clone(&state.m7_diagnostics);
    blocking(move || {
        super::diagnostics::preview(
            &request,
            &settings,
            &command_log,
            &summaries,
            session.as_deref(),
        )
    })
    .await
}

/// F21：读取持久化的脱敏摘要（按当前 worktree 过滤，7 天内）。
#[tauri::command]
pub async fn m7_diagnostics_summary(
    state: State<'_, AppState>,
    worktree_id: Option<String>,
) -> Result<Vec<crate::m7::diagnostics::DiagnosticsSummaryEntry>> {
    let store = Arc::clone(&state.m7_diagnostics);
    blocking(move || store.list(worktree_id.as_deref())).await
}

/// 清空当前 worktree 摘要；无 worktree 时只清环境探测，并同步清除内存与预览。
#[tauri::command]
pub async fn m7_diagnostics_clear(
    state: State<'_, AppState>,
    worktree_id: Option<String>,
) -> Result<usize> {
    let log = Arc::clone(&state.command_log);
    blocking(move || log.clear_diagnostics(worktree_id.as_deref())).await
}

#[tauri::command]
pub async fn m7_export_diagnostics(
    state: State<'_, AppState>,
    request: DiagnosticsRequest,
) -> Result<DiagnosticsExport> {
    let store = Arc::clone(&state.m7_diagnostics);
    blocking(move || super::diagnostics::export(&request, &store)).await
}

// ---------- 应用更新（任务 10） ----------

fn update_mode(state: &AppState) -> Result<crate::settings::AppUpdateMode> {
    let registry = state.registry.lock().map_err(|_| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.repositoryListLockFailed6",
            "仓库列表锁失败",
            &[],
        )
    })?;
    Ok(registry.settings().app_update_mode)
}

#[tauri::command]
pub async fn m7_update_status(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<AppUpdateStatus> {
    let mode = update_mode(&state)?;
    Ok(state.m7_update.status(&app, mode))
}

#[tauri::command]
pub async fn m7_update_check(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    force: bool,
) -> Result<AppUpdateStatus> {
    let mode = update_mode(&state)?;
    let update: Arc<UpdateState> = Arc::clone(&state.m7_update);
    super::update::check(app, &update, mode, force).await
}

#[tauri::command]
pub async fn m7_update_download(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<AppUpdateStatus> {
    let mode = update_mode(&state)?;
    let update: Arc<UpdateState> = Arc::clone(&state.m7_update);
    super::update::download_update(app, &update, mode).await
}

#[tauri::command]
pub async fn m7_update_install(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<AppUpdateStatus> {
    let mode = update_mode(&state)?;
    let update: Arc<UpdateState> = Arc::clone(&state.m7_update);
    super::update::download_and_install(app, &update, mode).await
}

#[tauri::command]
pub async fn m7_update_restart(app: tauri::AppHandle) -> Result<()> {
    super::update::restart(&app);
    Ok(())
}

// ---------- 更改列表（任务13） ----------
#[derive(Debug,serde::Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ChangelistRequest { pub worktree_id:String }
#[derive(Debug,serde::Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ChangelistChangeRequest { pub worktree_id:String, pub expected:super::changelists::ChangelistSelection, pub action:super::changelists::ChangeAction }
#[tauri::command]
pub async fn m7_changelist_view(state:State<'_,AppState>,request:ChangelistRequest)->Result<super::changelists::ChangelistView> {
    let session=session_for(&state,&request.worktree_id)?;let store=Arc::clone(&state.changelists);
    blocking(move||store.view(&session)).await
}
#[tauri::command]
pub async fn m7_changelist_change(state:State<'_,AppState>,request:ChangelistChangeRequest)->Result<super::changelists::ChangelistView> {
    let session=session_for(&state,&request.worktree_id)?;let store=Arc::clone(&state.changelists);
    blocking(move||{let _write=session.acquire_write_lock()?;store.change(&session,&request.expected,request.action)}).await
}
#[derive(Debug,serde::Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ChangelistWarningsRequest {pub worktree_id:String,pub selection:super::changelists::ChangelistSelection,#[serde(default)]pub amend:bool}
#[tauri::command]
pub async fn m7_changelist_warnings(state:State<'_,AppState>,request:ChangelistWarningsRequest)->Result<Vec<crate::commit_warnings::CommitWarning>> {
    let session=session_for(&state,&request.worktree_id)?;let store=Arc::clone(&state.changelists);
    let settings=state.registry.lock().map_err(|_|internal("settings lock"))?.settings().clone();
    blocking(move||{let _write=session.acquire_write_lock()?;let prepared=store.prepare(&session,&request.selection,&crate::commit::capture(session.git())?,request.amend)?;
        crate::commit_warnings::collect(&prepared.git,&crate::commit_warnings::WarningOptions{crlf:settings.commit_warn_crlf,detached:settings.commit_warn_detached,large_file_bytes:settings.commit_warn_large_file_bytes,invalid_filename:settings.commit_warn_invalid_filename})
    }).await
}

// ---------- 原生玻璃（Liquid Glass） ----------

#[tauri::command]
pub fn m7_glass_support() -> crate::platform::window_glass::GlassSupport {
    crate::platform::window_glass::support()
}

#[tauri::command]
pub fn m7_glass_layout(
    window: tauri::WebviewWindow,
    layout: crate::platform::window_glass::GlassLayout,
) -> Result<()> {
    crate::platform::window_glass::apply(&window, layout).map_err(internal)
}
