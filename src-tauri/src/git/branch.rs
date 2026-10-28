//! 分支与远端引用概览（M5 任务 03、规格 B02）。
//!
//! 只读：一次 `for-each-ref` 读出本地分支及其上游跟踪信息，再按
//! `refs/remotes/<remote>/` 分组列出远端分支；领先 / 落后数按本地记录
//! （截至上次 fetch），不联网核对。
//!
//! 边界：
//! - 不修改 Git 配置，也不写引用；
//! - 解析失败显式报错，不把「读不到」当成「没有分支」。

use serde::Serialize;
use ts_rs::TS;

use crate::error::{AppError, Result};
use crate::git::history::is_full_oid;
use crate::git::Git;
use crate::operation::OperationKind;

/// 一个本地分支及其上游跟踪信息。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LocalBranch {
    pub name: String,
    pub full_name: String,
    pub commit: String,
    pub is_current: bool,
    /// 上游的完整引用名（`refs/remotes/origin/main`）；没有配置为 null。
    pub upstream: Option<String>,
    /// 上游的短名（`origin/main`）。
    pub upstream_short: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// 上游已被删除（`[gone]`）。
    pub upstream_gone: bool,
    /// 已经检出这个分支的 worktree 路径（含当前 worktree）。
    pub worktrees: Vec<String>,
}

/// 一个远端分支。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RemoteBranch {
    /// 短名（`origin/main`）。
    pub name: String,
    /// 去掉远端名的分支名（`main`）。
    pub branch: String,
    pub full_name: String,
    pub commit: String,
    /// 跟踪这个远端分支的本地分支名（没有同名跟踪分支时为 null）。
    pub tracking_local: Option<String>,
    /// 跟踪它的本地分支的领先 / 落后数（截至上次 fetch）。
    pub ahead: u32,
    pub behind: u32,
}

/// 按远端分组的远端分支。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RemoteGroup {
    pub name: String,
    pub branches: Vec<RemoteBranch>,
}

/// 当前仓库生效的提交身份（只读；供日志“我”筛选使用）。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UserIdentity {
    pub name: Option<String>,
    pub email: Option<String>,
}

/// 分支组件的完整只读状态。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BranchOverview {
    pub worktree_id: String,
    /// 当前分支名；游离 HEAD 为 null。
    pub current_branch: Option<String>,
    /// HEAD 指向的完整引用名；游离 HEAD 为 null。
    pub head_ref: Option<String>,
    pub detached: bool,
    /// 游离 HEAD 时显示的短哈希。
    pub short_head: Option<String>,
    pub head_commit: Option<String>,
    pub local: Vec<LocalBranch>,
    pub remotes: Vec<RemoteGroup>,
    /// 远端名列表（推送对话框“有多个远端时可选”用）。
    pub remote_names: Vec<String>,
    pub operation: Option<OperationKind>,
    pub user: UserIdentity,
}

/// 解析 `%(upstream:track)` 的 `[ahead 1, behind 2]` / `[gone]` 形式。
///
/// Git 自己写出的 ahead / behind 单词不翻译，因此可以直接解析；任何不认识的
/// 形式都按「没有领先落后信息」处理，不猜数字。
pub fn parse_track(text: &str) -> (u32, u32, bool) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return (0, 0, false);
    }
    if trimmed.contains("gone") {
        return (0, 0, true);
    }
    let mut ahead = 0;
    let mut behind = 0;
    let inner = trimmed.trim_start_matches('[').trim_end_matches(']');
    for part in inner.split(',') {
        let mut words = part.split_whitespace();
        let Some(word) = words.next() else { continue };
        let Some(value) = words.next() else { continue };
        let Ok(parsed) = value.parse::<u32>() else { continue };
        match word {
            "ahead" => ahead = parsed,
            "behind" => behind = parsed,
            _ => {}
        }
    }
    (ahead, behind, false)
}

/// 读取分支概览。
pub fn read_overview(git: &Git, worktree_id: &str) -> Result<BranchOverview> {
    let local = read_local_branches(git)?;
    let remote_groups = read_remote_groups(git, &local)?;
    let remote_names: Vec<String> = remote_groups.iter().map(|group| group.name.clone()).collect();
    let head_ref = crate::git::history::head_ref(git);
    let head_commit = crate::git::history::head_commit(git)?;
    let current_branch = head_ref
        .as_deref()
        .and_then(|name| name.strip_prefix("refs/heads/"))
        .map(str::to_string);
    let detached = head_ref.is_none() && head_commit.is_some();
    let short_head = if detached {
        head_commit.as_ref().map(|oid| oid.chars().take(7).collect())
    } else {
        None
    };
    let operation = crate::operation::detect(git, 0)?.map(|state| state.kind);
    Ok(BranchOverview {
        worktree_id: worktree_id.to_string(),
        current_branch,
        head_ref,
        detached,
        short_head,
        head_commit,
        local,
        remotes: remote_groups,
        remote_names,
        operation,
        user: UserIdentity {
            name: git.config_get("user.name"),
            email: git.config_get("user.email"),
        },
    })
}

fn read_local_branches(git: &Git) -> Result<Vec<LocalBranch>> {
    let output = git.run_ok(
        &[
            "--no-optional-locks",
            "for-each-ref",
            "--format=%(refname)%00%(refname:short)%00%(objectname)%00%(upstream)%00%(upstream:short)%00%(upstream:track)%00%(HEAD)%00",
            "refs/heads/",
        ],
        "读取本地分支",
    )?;
    let worktrees = git.worktrees().unwrap_or_default();
    let mut branches = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).split('\n') {
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\0').collect();
        if fields.len() < 7 {
            continue;
        }
        let full_name = fields[0];
        if !full_name.starts_with("refs/heads/") || !is_full_oid(fields[2]) {
            continue;
        }
        let name = fields[1].to_string();
        let (ahead, behind, upstream_gone) = parse_track(fields[5]);
        let occupied: Vec<String> = worktrees
            .iter()
            .filter(|entry| entry.branch.as_deref() == Some(name.as_str()))
            .map(|entry| entry.path.clone())
            .collect();
        branches.push(LocalBranch {
            name,
            full_name: full_name.to_string(),
            commit: fields[2].to_string(),
            is_current: fields[6].trim() == "*",
            upstream: (!fields[3].is_empty()).then(|| fields[3].to_string()),
            upstream_short: (!fields[4].is_empty()).then(|| fields[4].to_string()),
            ahead,
            behind,
            upstream_gone,
            worktrees: occupied,
        });
    }
    // 当前分支置顶（B02），其余按名字排序：Git 的 for-each-ref 已按引用名排序。
    branches.sort_by(|a, b| b.is_current.cmp(&a.is_current));
    Ok(branches)
}

fn read_remote_groups(git: &Git, local: &[LocalBranch]) -> Result<Vec<RemoteGroup>> {
    let output = git.run_ok(
        &[
            "--no-optional-locks",
            "for-each-ref",
            "--format=%(refname)%00%(refname:short)%00%(objectname)%00",
            "refs/remotes/",
        ],
        "读取远端分支",
    )?;
    let mut groups: Vec<RemoteGroup> = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).split('\n') {
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\0').collect();
        if fields.len() < 3 {
            continue;
        }
        let full_name = fields[0];
        let short = fields[1];
        if !is_full_oid(fields[2]) {
            continue;
        }
        // `origin/HEAD` 是指向默认分支的符号引用，不是可检出的分支。
        if short.ends_with("/HEAD") {
            continue;
        }
        let Some((remote, branch)) = short.split_once('/') else {
            continue;
        };
        let tracking = local
            .iter()
            .find(|candidate| candidate.upstream.as_deref() == Some(full_name));
        let entry = RemoteBranch {
            name: short.to_string(),
            branch: branch.to_string(),
            full_name: full_name.to_string(),
            commit: fields[2].to_string(),
            tracking_local: tracking.map(|candidate| candidate.name.clone()),
            ahead: tracking.map(|candidate| candidate.ahead).unwrap_or(0),
            behind: tracking.map(|candidate| candidate.behind).unwrap_or(0),
        };
        match groups.iter_mut().find(|group| group.name == remote) {
            Some(group) => group.branches.push(entry),
            None => groups.push(RemoteGroup {
                name: remote.to_string(),
                branches: vec![entry],
            }),
        }
    }
    groups.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(groups)
}

/// 校验新分支名（B02）：用 Git 自己的规则 `check-ref-format --branch`。
///
/// Git 会拒绝空名、`..`、以 `-` 开头、以 `.` 或 `/` 结尾等；返回的错误原文
/// 直接展示给用户，前端不自己实现一套规则。
pub fn validate_branch_name(git: &Git, name: &str) -> Result<()> {
    if name.trim().is_empty() {
        return Err(AppError::rejected("分支名不能为空"));
    }
    let output = git.run(&["check-ref-format", "--branch", name])?;
    if output.exit_code != 0 {
        return Err(AppError::rejected(format!(
            "分支名不合法：{}",
            output.stderr.trim()
        )));
    }
    Ok(())
}

/// 本地分支是否已存在（完整引用比较，不做前缀匹配）。
pub fn branch_exists(git: &Git, name: &str) -> Result<bool> {
    let full_name = format!("refs/heads/{name}");
    let output = git.run(&["show-ref", "--verify", "--quiet", &full_name])?;
    Ok(output.exit_code == 0)
}
