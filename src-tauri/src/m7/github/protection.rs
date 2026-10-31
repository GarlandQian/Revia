//! 只读同步完整受保护分支集合；缓存绑定真实仓库身份、账号与远端，不保存策略规则。
use super::{
    pr::PrContext,
    store::{ProtectionInfo, ProtectionSnapshot, PROTECTION_TTL_MS},
    GithubState,
};
use crate::{
    error::{AppError, ErrorKind, Result},
    session::WorktreeSession,
};
use serde::Deserialize;
use std::collections::BTreeSet;

fn scoped_key(
    session: &WorktreeSession,
    login: &str,
    remote: &str,
    owner: &str,
    repo: &str,
) -> Result<String> {
    let identity = crate::m7::edit::identity_key(session)?;
    Ok(crate::m7::sha256_hex(&serde_json::to_vec(&(
        identity,
        login.to_ascii_lowercase(),
        remote,
        owner.to_ascii_lowercase(),
        repo.to_ascii_lowercase(),
    ))?))
}
pub fn scope_key(context: &PrContext<'_>) -> Result<String> {
    scoped_key(
        context.session,
        &context.login,
        &context.remote,
        &context.owner,
        &context.repo,
    )
}
fn info_for(branch: &str, snapshot: Option<&ProtectionSnapshot>) -> ProtectionInfo {
    match snapshot {
        Some(saved) => ProtectionInfo {
            branch: branch.into(),
            protected: saved.branches.iter().any(|name| name == branch),
            source: if saved.stale
                || crate::m7::now_ms().saturating_sub(saved.checked_at_ms) >= PROTECTION_TTL_MS
            {
                "stale"
            } else {
                "api"
            }
            .into(),
            checked_at_ms: saved.checked_at_ms,
        },
        None => ProtectionInfo {
            branch: branch.into(),
            protected: true,
            source: "unknown".into(),
            checked_at_ms: 0,
        },
    }
}
/// force用于用户刷新或合并前复核；完整分页成功后才整体替换，失败不把404当成空保护集合。
pub fn state(context: &PrContext<'_>, branch: &str, force: bool) -> Result<ProtectionInfo> {
    if branch.trim().is_empty() {
        return Ok(info_for(branch, None));
    }
    let key = scope_key(context)?;
    let store = context.state.receipts();
    let cached = store.protection_snapshot(&key)?;
    if !force
        && cached.as_ref().is_some_and(|saved| {
            !saved.stale
                && crate::m7::now_ms().saturating_sub(saved.checked_at_ms) < PROTECTION_TTL_MS
        })
    {
        return Ok(info_for(branch, cached.as_ref()));
    }
    let generation = store.begin_protection_sync(&key);
    let fetched = fetch_branches(context);
    let snapshot =
        store.finish_protection_sync(&key, generation, fetched.ok(), crate::m7::now_ms())?;
    Ok(info_for(branch, snapshot.as_ref()))
}
fn incomplete() -> AppError {
    AppError::coded(
        ErrorKind::Rejected,
        "errors.m7.githubProtectionIncomplete",
        "分支保护响应不完整，已保留上次结果",
        &[],
    )
}
#[derive(Deserialize)]
struct ProtectedBranch {
    name: String,
    protected: bool,
}
fn fetch_branches(context: &PrContext<'_>) -> Result<Vec<String>> {
    let endpoint = format!(
        "{}/repos/{}/{}/branches",
        context.client.base().trim_end_matches('/'),
        context.owner,
        context.repo
    );
    let endpoint = reqwest::Url::parse(&endpoint).map_err(|_| incomplete())?;
    let mut url = endpoint.clone();
    url.set_query(Some("protected=true&per_page=100&page=1"));
    let mut visited = BTreeSet::new();
    let mut branches = BTreeSet::new();
    loop {
        // 分页仅限同一API来源和同一仓库的branches端点，不能带凭据跳到另一地址。
        if url.origin() != endpoint.origin()
            || url.path() != endpoint.path()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || !url
                .query_pairs()
                .any(|(k, v)| k == "protected" && v == "true")
            || visited.len() >= 10_000
            || !visited.insert(url.as_str().to_string())
        {
            return Err(incomplete());
        }
        let (page, meta): (Vec<ProtectedBranch>, _) = context.client.get_json(url.as_str())?;
        for branch in page {
            if !branch.protected || branch.name.is_empty() {
                return Err(incomplete());
            }
            super::pr::validate_branch(&branch.name)?;
            branches.insert(branch.name);
        }
        match meta.next_page {
            Some(next) => url = url.join(&next).map_err(|_| incomplete())?,
            None => return Ok(branches.into_iter().collect()),
        }
    }
}
/// 推送按实际目标远端消费；历史确认没有指定远端时取当前账号各GitHub远端的有效并集。
/// 这里只读本地缓存，不为普通本地历史/推送访问或刷新账号凭据。
pub fn effective(
    session: &WorktreeSession,
    state: &GithubState,
    remote: Option<&str>,
    local: &[String],
) -> Result<Vec<String>> {
    let accounts = state.accounts();
    let login = state.repo_account(&session.path).or_else(|| {
        if accounts.len() == 1 {
            Some(accounts[0].login.clone())
        } else {
            None
        }
    });
    let Some(login) = login else {
        return Ok(local.to_vec());
    };
    let mut all: BTreeSet<String> = local.iter().cloned().collect();
    for entry in super::store::remotes(session)? {
        if remote.is_some_and(|name| entry.name != name) {
            continue;
        }
        let (Some(owner), Some(repo)) = (&entry.owner, &entry.repo) else {
            continue;
        };
        let key = scoped_key(session, &login, &entry.name, owner, repo)?;
        if let Some(snapshot) = state.receipts().protection_snapshot(&key)? {
            all.extend(snapshot.branches);
        }
    }
    Ok(all.into_iter().collect())
}
pub fn history_branch(
    session: &WorktreeSession,
    state: &GithubState,
    local: &[String],
) -> Result<Option<String>> {
    let protected = effective(session, state, None, local)?;
    let head = session
        .git
        .run(&["symbolic-ref", "--quiet", "--short", "HEAD"])?;
    if head.exit_code == 1 {
        return Ok(None);
    }
    let branch = head
        .into_success("读取历史操作当前分支")?
        .stdout_lossy()
        .trim()
        .to_string();
    Ok(crate::remote_ops::is_protected(&branch, &protected).then_some(branch))
}
