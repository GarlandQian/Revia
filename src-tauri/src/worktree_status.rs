//! worktree 列表的只读状态（M4 F09.2、Q126）。
//!
//! 主工具栏的 worktree 下拉每行显示：分支、路径、有无未提交改动、进行中的操作、
//! 领先和落后数（按本地记录，不联网）。全部只读，不写仓库，也不改用户的 Git 配置。

use std::path::Path;

use serde::Serialize;
use ts_rs::TS;

use crate::git::status;
use crate::git::Git;
use crate::operation::{self, OperationKind};
use crate::session::WorktreeEntry;

/// 一个 worktree 在下拉里显示的状态。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WorktreeStatusEntry {
    pub id: String,
    pub path: String,
    pub branch: Option<String>,
    pub detached: bool,
    pub is_main: bool,
    /// 路径当前是否可访问（被移动或删除时仍列出，但标记不可用）。
    pub accessible: bool,
    /// 读取状态失败的原因。
    pub error: Option<String>,
    /// 是否有未提交改动（含未跟踪文件）。
    pub dirty: bool,
    /// 进行中的操作；没有时为 None。
    pub operation: Option<OperationKind>,
    /// 进行中的操作名称（与 OperationKind 的 label 一致）。
    pub operation_label: Option<String>,
    /// 领先上游的提交数（按本地记录）。
    pub ahead: u32,
    /// 落后上游的提交数（按本地记录）。
    pub behind: u32,
    /// 上游分支名；没有配置或本地缺少引用时为 None。
    pub upstream: Option<String>,
}

/// 解析 `git rev-list --left-right --count HEAD...@{upstream}` 的输出。
///
/// 左边是 HEAD 独有的提交（领先），右边是上游独有的提交（落后）。
pub fn parse_ahead_behind(text: &str) -> Option<(u32, u32)> {
    let mut parts = text.split_whitespace();
    let ahead = parts.next()?.parse::<u32>().ok()?;
    let behind = parts.next()?.parse::<u32>().ok()?;
    Some((ahead, behind))
}

/// 读取单个 worktree 的只读状态。任何一项读取失败都如实记在 `error` 里，
/// 不让整个列表失败（一个 worktree 不可用不应挡住其他 worktree）。
pub fn read(git: &Git, entry: &WorktreeEntry) -> WorktreeStatusEntry {
    let mut result = WorktreeStatusEntry {
        id: entry.id.clone(),
        path: entry.path.clone(),
        branch: entry.branch.clone(),
        detached: entry.detached,
        is_main: entry.is_main,
        accessible: Path::new(&entry.path).is_dir(),
        error: None,
        dirty: false,
        operation: None,
        operation_label: None,
        ahead: 0,
        behind: 0,
        upstream: None,
    };
    if !result.accessible {
        result.error = Some("worktree 路径不存在或无法访问".into());
        return result;
    }
    let local = git.in_directory(Path::new(&entry.path));
    match status::read(&local) {
        Ok(state) => {
            result.branch = state.branch;
            result.detached = state.detached;
            result.dirty = !state.entries.is_empty();
        }
        Err(error) => {
            result.error = Some(error.message);
        }
    }
    if result.error.is_none() {
        match operation::conflict_count(&local).and_then(|count| operation::detect(&local, count)) {
            Ok(Some(state)) => {
                result.operation = Some(state.kind);
                result.operation_label = Some(state.kind.label().to_string());
            }
            Ok(None) => {}
            Err(error) => result.error = Some(error.message),
        }
    }
    // 领先 / 落后只按本地引用计算，不联网（Q10、Q126）。
    let upstream = local.try_run(&[
        "rev-parse",
        "--abbrev-ref",
        "--symbolic-full-name",
        "@{upstream}",
    ]);
    if let Some(name) = upstream {
        let name = name.trim().to_string();
        if !name.is_empty() {
            result.upstream = Some(name);
            if let Some(text) = local.try_run(&[
                "rev-list",
                "--left-right",
                "--count",
                "HEAD...@{upstream}",
            ]) {
                if let Some((ahead, behind)) = parse_ahead_behind(&text) {
                    result.ahead = ahead;
                    result.behind = behind;
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    use crate::git::CommandLog;
    use crate::shell::{EnvCaptureInfo, ShellEnv};

    fn shell_env(dir: &Path) -> ShellEnv {
        let path: Vec<String> = std::env::var("PATH")
            .unwrap_or_default()
            .split(':')
            .map(str::to_string)
            .collect();
        let vars = [("HOME".to_string(), std::env::var("HOME").unwrap_or_default()),
            ("PATH".to_string(), std::env::var("PATH").unwrap_or_default())]
            .into_iter()
            .collect();
        ShellEnv {
            vars,
            path: path.clone(),
            info: EnvCaptureInfo {
                worktree: dir.display().to_string(),
                shell: "/bin/zsh".into(),
                elapsed_ms: 0,
                timed_out: false,
                used_fallback: false,
                var_count: 2,
                path_entries: path,
                diagnostics: None,
            },
        }
    }

    #[test]
    fn reads_dirty_and_accessible_state() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let run = |args: &[&str]| {
            let output = Command::new("git").current_dir(&repo).args(args).output().unwrap();
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "revia@example.com"]);
        run(&["config", "user.name", "Revia Test"]);
        std::fs::write(repo.join("a.txt"), "a1\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "init"]);
        let git = Git::new(&repo, &shell_env(&repo), std::sync::Arc::new(CommandLog::default())).unwrap();
        let entry = WorktreeEntry {
            id: "wt".into(),
            path: repo.display().to_string(),
            head: None,
            branch: Some("master".into()),
            bare: false,
            detached: false,
            is_main: true,
        };
        let clean = read(&git, &entry);
        assert!(clean.accessible);
        assert!(!clean.dirty);
        assert!(clean.error.is_none());
        assert_eq!(clean.upstream, None);
        assert_eq!((clean.ahead, clean.behind), (0, 0));
        std::fs::write(repo.join("a.txt"), "a2\n").unwrap();
        assert!(read(&git, &entry).dirty);
        let missing = WorktreeEntry { path: dir.path().join("missing").display().to_string(), ..entry };
        let absent = read(&git, &missing);
        assert!(!absent.accessible);
        assert!(absent.error.is_some());
    }

    #[test]
    fn parses_ahead_behind_counts() {
        assert_eq!(parse_ahead_behind("2\t3\n"), Some((2, 3)));
        assert_eq!(parse_ahead_behind("0 0"), Some((0, 0)));
        assert_eq!(parse_ahead_behind(""), None);
        assert_eq!(parse_ahead_behind("x 1"), None);
    }
}
