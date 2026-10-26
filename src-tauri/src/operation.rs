//! 进行中操作的识别（M3 F01，Q12、Q28）。
//!
//! 从当前登记 worktree 的真实 Git 元数据判断 merge、rebase、cherry-pick、
//! revert 是否进行中（含 linked worktree 与 sequencer 中间阶段），并结合
//! index 的 unmerged 条目给出未解决冲突数。
//! 只读，不修改仓库，不提供 continue/abort/skip。
//!
//! 边界：
//! - 标记路径一律由 Git 自己解析（`rev-parse --git-path <name>`），不手工拼
//!   `.git/xxx`：linked worktree 的私有 gitDir 与共享 commonDir 分工不同，
//!   猜错会把主 worktree 的进行中操作当成当前 worktree 的（F01.1）。
//! - 解析或读取失败都**显式报错**，绝不当作「无操作」（规格 F01.1）。
//! - 冲突一律禁用提交；merge 无冲突可普通提交完成合并；
//!   rebase / cherry-pick / revert 进行中禁用提交（规格 F01.2）。
//! - **rebase 期间 Git 会同时写入 `CHERRY_PICK_HEAD`**（rebase 的每一步本身就是
//!   一次 cherry-pick），这是从属标记而不是互斥异常：按 rebase 优先识别，
//!   但仍然硬门禁提交。rebase --rebase-merges 的 MERGE_HEAD 同样从属 rebase；
//!   只有不属于该操作的互斥标记组合才报错。
//! - 预期的指纹不能只含种类与冲突数：同一种操作的内容变化（换了一个 MERGE_HEAD、
//!   rebase 进行到下一步）也必须让预期失效，因此这里给出真实元数据版本。

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::error::{AppError, Result};
use crate::git::Git;

/// 进行中的操作类型（术语见 GLOSSARY：进行中的操作）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum OperationKind {
    Merge,
    Rebase,
    CherryPick,
    Revert,
}

impl OperationKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Rebase => "rebase",
            Self::CherryPick => "cherry-pick",
            Self::Revert => "revert",
        }
    }
}

/// 一次进行中操作的检测结果（面向前端提示与提交门禁）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OperationState {
    pub kind: OperationKind,
    /// 是否处于多提交序列（sequencer）中间阶段。
    pub sequencer_pending: bool,
    /// 未解决冲突的去重路径数；大于 0 时提交一律禁用。
    pub conflict_count: usize,
    /// 真实操作元数据版本：覆盖 MERGE_HEAD、rebase 状态文件、CHERRY_PICK_HEAD /
    /// REVERT_HEAD、sequencer 待办等**内容**。同一种操作内容变化时版本也变，
    /// 旧确认随之失效（F03.2）。
    pub metadata_version: String,
}

impl OperationState {
    /// 有未解决冲突时提交一律禁用（规格 F01.2）。
    pub fn has_conflicts(&self) -> bool {
        self.conflict_count > 0
    }

    /// 当前操作是否允许普通提交完成。
    ///
    /// merge 无冲突时可普通提交完成合并；rebase / cherry-pick / revert
    /// 进行中禁用提交（规格 F01.2）。
    pub fn commit_allowed(&self) -> bool {
        if self.has_conflicts() {
            return false;
        }
        matches!(self.kind, OperationKind::Merge)
    }

    /// merge 无冲突：允许「没有暂存差异」的普通提交完成合并（F03.4）。
    ///
    /// `git merge --no-commit` 一个已经包含的分支时，index 与 HEAD 相同，
    /// 但 `git commit` 仍会写出真正的 merge commit；这是合法提交，不能被
    /// 「暂存区没有改动」拦掉。
    pub fn allows_empty_diff_commit(&self) -> bool {
        self.kind == OperationKind::Merge && !self.has_conflicts()
    }
}

/// 解析当前 worktree 的真实 gitDir 与 commonDir（回收区身份与监视根共用）。
pub fn resolve_dirs(git: &Git) -> Result<(PathBuf, PathBuf)> {
    let read = |flag: &str| -> Result<PathBuf> {
        let output = git.run_ok(
            &["rev-parse", "--path-format=absolute", flag],
            "读取 Git 元数据目录",
        )?;
        let text = output.stdout_lossy();
        // 只去掉 Git 追加的一个换行符；路径自身可以包含换行（Unix 合法）。
        let value = text.strip_suffix('\n').unwrap_or(&text);
        if value.is_empty() {
            return Err(AppError::environment(format!(
                "Git 未返回 {flag} 的真实路径"
            )));
        }
        Ok(PathBuf::from(value))
    };
    Ok((read("--git-dir")?, read("--git-common-dir")?))
}

/// 让 Git 解析一个状态标记的真实路径（`rev-parse --git-path`）。
fn marker_path(git: &Git, name: &str) -> Result<PathBuf> {
    let output = git.run_ok(
        &["rev-parse", "--path-format=absolute", "--git-path", name],
        "解析 Git 状态标记路径",
    )?;
    let text = output.stdout_lossy();
    let value = text.strip_suffix('\n').unwrap_or(&text).to_string();
    if value.is_empty() {
        return Err(AppError::environment(format!(
            "Git 未返回状态标记 {name} 的路径"
        )));
    }
    let path = PathBuf::from(value);
    Ok(if path.is_absolute() { path } else { git.directory.join(path) })
}

/// 标记是否存在；元数据不可访问（权限等 IO 错误）与「不存在」必须区分。
fn marker_state(git: &Git, name: &str) -> Result<bool> {
    let path = marker_path(git, name)?;
    match fs::symlink_metadata(&path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(AppError::environment(format!(
            "读取操作标记失败（{}）：{error}",
            path.display()
        ))),
    }
}

/// 把一个标记的内容摘要写进 hasher。
///
/// 文件按固定大小分片读取（内存有界）；目录只读取其中的小状态文件，
/// 不递归到对象库之类的大目录。
fn hash_marker(hasher: &mut Sha256, path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_file() {
        hash_file(hasher, path)?;
        return Ok(());
    }
    if metadata.is_dir() {
        // sequencer / rebase-merge / rebase-apply 目录：只取状态文本文件。
        const STATE_FILES: &[&str] = &[
            "todo",
            "git-rebase-todo",
            "done",
            "head-name",
            "onto",
            "orig-head",
            "msgnum",
            "end",
            "abort-safety",
            "interactive",
            "rewritten-list",
            "post-rewrite",
        ];
        for name in STATE_FILES {
            let candidate = path.join(name);
            match fs::symlink_metadata(&candidate) {
                Ok(meta) if meta.is_file() => {
                    hasher.update(b"\0file:");
                    hasher.update(name.as_bytes());
                    hasher.update(b"\0");
                    hash_file(hasher, &candidate)?;
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(AppError::environment(format!(
                        "读取操作状态文件失败（{}）：{error}",
                        candidate.display()
                    )))
                }
            }
        }
    }
    Ok(())
}

/// 分片读取一个文件并写入 hasher（内存有界，不整文件读入）。
fn hash_file(hasher: &mut Sha256, path: &Path) -> Result<()> {
    use std::io::Read;
    const CHUNK: usize = 64 * 1024;
    let mut file = fs::File::open(path)?;
    let mut buffer = vec![0u8; CHUNK];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(())
}

/// 探测当前 worktree 的进行中操作。
///
/// 返回 (kind, sequencer 中间阶段, 元数据版本)。
fn probe(git: &Git) -> Result<Option<(OperationKind, bool, String)>> {
    // 顺序即优先级：rebase 期间 CHERRY_PICK_HEAD 是从属标记，不能当成冲突。
    const MARKERS: &[(&str, OperationKind)] = &[
        ("rebase-merge", OperationKind::Rebase),
        ("rebase-apply", OperationKind::Rebase),
        ("MERGE_HEAD", OperationKind::Merge),
        ("CHERRY_PICK_HEAD", OperationKind::CherryPick),
        ("REVERT_HEAD", OperationKind::Revert),
    ];
    let mut present: Vec<(&str, OperationKind, PathBuf)> = Vec::new();
    for (name, kind) in MARKERS {
        if marker_state(git, name)? {
            present.push((name, *kind, marker_path(git, name)?));
        }
    }
    let sequencer_present = marker_state(git, "sequencer")?;
    let sequencer_dir = if sequencer_present {
        Some(marker_path(git, "sequencer")?)
    } else {
        None
    };

    if present.is_empty() && sequencer_dir.is_none() {
        return Ok(None);
    }

    // 互斥异常：只有「merge 与 rebase 同时存在」这类 Git 自身不可能的组合才报错。
    let kinds: BTreeSet<&str> = present.iter().map(|(_, kind, _)| kind.label()).collect();
    // rebase --rebase-merges 可以合法同时保留 MERGE_HEAD；整个阶段仍按 rebase 禁提交。
    if !kinds.contains("rebase") && kinds.len() > 1 {
        return Err(AppError::environment("Git 元数据中存在互斥操作标记，请在终端处理"));
    }

    let (kind, sequencer_pending) = match present.first() {
        Some((_, kind, _)) => (*kind, sequencer_present),
        None => {
            // 只有 sequencer：多提交序列（cherry-pick / revert 序列），用待办首行区分。
            let directory = sequencer_dir
                .as_ref()
                .ok_or_else(|| AppError::internal("sequencer 路径缺失"))?;
            let todo = directory.join("todo");
            let first = match fs::read_to_string(&todo) {
                Ok(text) => text.lines().next().unwrap_or("").trim().to_string(),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Err(AppError::environment(format!(
                        "sequencer 目录缺少待办文件：{}",
                        todo.display()
                    )))
                }
                Err(error) => {
                    return Err(AppError::environment(format!(
                        "读取 sequencer 待办失败（{}）：{error}",
                        todo.display()
                    )))
                }
            };
            let kind = if first.starts_with("revert") {
                OperationKind::Revert
            } else if first.starts_with("pick") {
                OperationKind::CherryPick
            } else {
                return Err(AppError::environment(format!(
                    "无法识别 sequencer 待办内容（{}）",
                    todo.display()
                )));
            };
            (kind, true)
        }
    };

    // 元数据版本：所有**存在**的标记内容（含从属标记）一起哈希，
    // 这样同一种操作的内容变化（换了 MERGE_HEAD、rebase 进行到下一步）也会变。
    let mut hasher = Sha256::new();
    hasher.update(b"operation-metadata-v1");
    for (name, _, path) in &present {
        hasher.update(b"\0marker:");
        hasher.update(name.as_bytes());
        hasher.update(b"\0");
        hash_marker(&mut hasher, path)?;
    }
    if let Some(directory) = &sequencer_dir {
        hasher.update(b"\0marker:sequencer\0");
        hash_marker(&mut hasher, directory)?;
    }
    let metadata_version: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();

    Ok(Some((kind, sequencer_pending, metadata_version)))
}

/// 检测当前 worktree 的进行中操作（`conflict_count` 来自 index 的 unmerged 条目）。
pub fn detect(git: &Git, conflict_count: usize) -> Result<Option<OperationState>> {
    Ok(probe(git)?.map(|(kind, sequencer_pending, metadata_version)| OperationState {
        kind,
        sequencer_pending,
        conflict_count,
        metadata_version,
    }))
}

/// 未解决冲突的去重路径数（直接复用既有 status 解析）。
pub fn conflict_count(git: &Git) -> Result<usize> {
    let status = crate::git::status::read(git)?;
    Ok(status
        .entries
        .iter()
        .filter(|entry| entry.unmerged)
        .map(|entry| entry.path.clone())
        .collect::<std::collections::HashSet<_>>()
        .len())
}
