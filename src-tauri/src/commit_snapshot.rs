//! 提交前快照与 hooks 改动对照（M3 F05/F04.3，Q22、ADR-0010）。
//!
//! 提交时存在**部分暂存**的文件（同一文件既有已暂存又有未暂存改动）时，
//! 在启动 commit 之前把这些文件的**完整工作区状态**存入现有回收区，
//! 来源固定为 `commitSnapshot`，与 discardFile/discardHunk 区分。
//! 快照失败则不启动 commit（规格 F05.1/F05.2）。
//!
//! 两个必须区分的基准：
//! - **提交前基准**（`SnapshotBaseline`）：快照保存后、commit 启动前这些路径的
//!   完整工作区状态；再次核对用它（保存后到启动前的外部变化必须拒绝）。
//!   这里的路径必须是能**无损**恢复的普通文件；符号链接/子模块等无法完整快照
//!   的类型一律 fail closed（不启动 commit）。
//! - **hooks 基准**（`HookBaseline`）：commit 启动前**全部 tracked 路径 +
//!   当时 dirty / untracked 路径**的类型、模式与内容摘要，加上完整暂存区条目表。
//!   它单独实现、**不跟随符号链接**、支持 Git 正常类型（普通文件、符号链接、
//!   gitlink），因此即使没有部分暂存文件也能正常提交。
//!
//! hooks 对照不能用「部分暂存文件」或「暂存内容是否消失」代替：
//! - hooks 可以改**任何**路径（包括本次提交之外、原本干净或原本未跟踪的路径）；
//! - `git commit` 成功本身会让暂存区与新 HEAD 一致，这不是 hook 改动；
//! - hook 也可能在提交成功后继续改工作区或暂存区。
//! 因此工作区侧比较「全部 tracked ∪ 当时 dirty/untracked」的**摘要**（新出现与
//! 消失的路径也算），暂存区侧比较「提交前暂存区 / 新提交的 tree / 提交后暂存区」
//! 三者。全程只保留摘要，不把仓库文件字节读进内存；读取失败显式报错，
//! 绝不悄悄当作「没有改动」。

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::error::{AppError, ErrorKind, Result};
use crate::git::Git;
use crate::git::status::ChangesKind;
use crate::recovery::{self, RecoverySource, RecoveryStore, SavedFile, WorktreeIdentity};

/// 摘要读取的分片大小（内存有界）。
const DIGEST_CHUNK_BYTES: usize = 64 * 1024;

/// 找出「部分暂存」的文件：同一路径同时出现在已暂存与未暂存两组。
pub fn partial_staged_paths(git: &Git) -> Result<Vec<String>> {
    let status = crate::git::status::read(git)?;
    let mut staged: BTreeSet<String> = BTreeSet::new();
    let mut unstaged: BTreeSet<String> = BTreeSet::new();
    for entry in &status.entries {
        match entry.changes {
            ChangesKind::Staged => {
                staged.insert(entry.path.clone());
            }
            ChangesKind::Unstaged => {
                unstaged.insert(entry.path.clone());
            }
            ChangesKind::Untracked => {}
        }
    }
    Ok(staged.intersection(&unstaged).cloned().collect())
}

/// 提交前快照：这些路径保存时的完整工作区状态（作为「保存后再次核对」的基准）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotBaseline {
    pub paths: Vec<String>,
    pub files: Vec<SavedFile>,
}

/// 保存提交前快照到回收区（规格 F05.1）。
///
/// 保存的是**完整工作区状态**（内容字节、存在状态、可执行位），不是补丁；
/// 不使用 stash/reflog。返回记录、占用标记（覆盖保存 → commit → 收尾全程，
/// 期间清理不会删掉这条备份）与保存时的完整工作区基准。
///
/// 无法无损保存的类型（符号链接、子模块、目录）在这里**显式拒绝**：
/// 宁可拒绝提交，也不用不完整的备份冒充可恢复的快照（F05.1 fail closed）。
pub fn save_snapshot(
    git: &Git,
    store: &RecoveryStore,
    identity: &WorktreeIdentity,
    paths: &[String],
) -> Result<(
    recovery::RecoveryRecord,
    recovery::RecordUse,
    SnapshotBaseline,
)> {
    if paths.is_empty() {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.commitSnapshot.thereNoFilesSnapshot",
            "没有需要快照的文件",
            &[],
        ));
    }
    store.ensure_outside(identity)?;
    let before = recovery::capture_files(&git.directory, paths).map_err(|error| {
        AppError::coded(
            ErrorKind::Rejected,
            "errors.commitSnapshot.someStagedFilesCouldFullySaved",
            "部分暂存的文件无法完整保存到回收区，已取消提交（不改动任何内容）：{cause}",
            &[],
        ).cause(error)
    })?;
    let (record, guard) = store.save(
        identity.clone(),
        RecoverySource::CommitSnapshot,
        &paths.join(", "),
        before.clone(),
        // 提交前快照不做补丁预览，没有补丁上下文；存空指纹。
        String::new(),
    )?;
    Ok((
        record,
        guard,
        SnapshotBaseline {
            paths: paths.to_vec(),
            files: before,
        },
    ))
}

impl SnapshotBaseline {
    /// 重新读取这些路径的真实工作区状态；读取失败显式报错（不当作「未变化」）。
    pub fn capture_now(&self, git: &Git) -> Result<Vec<SavedFile>> {
        recovery::capture_files(&git.directory, &self.paths)
    }

    /// 当前状态是否仍与保存时一致（保存后到启动前再次核对用，F05.2）。
    pub fn matches_now(&self, git: &Git) -> Result<bool> {
        Ok(self.capture_now(git)? == self.files)
    }
}

// ---------------------------------------------------------------------------
// hooks 改动对照（F04.3）
// ---------------------------------------------------------------------------

/// 工作区条目的类型（对照用；不跟随符号链接）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum WorktreeEntryKind {
    /// 普通文件。
    File,
    /// 符号链接（摘要取链接目标，不读取目标内容）。
    Symlink,
    /// 子模块工作区（gitlink）。
    Submodule,
    /// 目录（未跟踪目录等）。
    Directory,
    /// 路径不存在。
    Missing,
}

/// 一个路径在某一时刻的工作区摘要：只保留摘要，不保留文件字节。
#[derive(Debug, Clone, PartialEq, Eq)]
struct WorktreeDigest {
    kind: WorktreeEntryKind,
    executable: bool,
    /// 文件字节、链接目标或子模块 HEAD/index/工作区的摘要；普通目录与缺失为 None。
    content: Option<String>,
}

/// 计算一个路径的工作区摘要（**不跟随**符号链接，内存有界）。
///
/// 任何 IO 错误都显式上报：读不到不等于「没变化」。
///
/// `gitlink` 为真表示该路径在 index 里是子模块（mode 160000）：此时工作区里
/// 它是一个目录，按其真实 HEAD、index 与文件摘要比较，父仓库 index 未变时
/// 也能报告 hook 在子模块内的 checkout 或编辑；报告仍归于父仓库的子模块路径。
fn digest_path(git: &Git, path: &str, gitlink: bool, depth: usize) -> Result<WorktreeDigest> {
    let full = safe_join(&git.directory, path)?;
    let metadata = match std::fs::symlink_metadata(&full) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(WorktreeDigest {
                kind: WorktreeEntryKind::Missing,
                executable: false,
                content: None,
            });
        }
        Err(error) => {
            return Err(AppError::coded(
                ErrorKind::Environment,
                "errors.commitSnapshot.failedReadWorkingTreeStatus",
                "读取工作区状态失败（{0}）：{error}",
                &[
                    ("0", format!("{:}", full.display())),
                    ("error", format!("{}", error)),
                ],
            ));
        }
    };
    if metadata.file_type().is_symlink() {
        let target = std::fs::read_link(&full).map_err(|error| {
            AppError::coded(
                ErrorKind::Environment,
                "errors.commitSnapshot.failedReadSymbolicLink",
                "读取符号链接失败（{0}）：{error}",
                &[
                    ("0", format!("{:}", full.display())),
                    ("error", format!("{}", error)),
                ],
            )
        })?;
        let mut hasher = Sha256::new();
        hasher.update(target.as_os_str().as_encoded_bytes());
        return Ok(WorktreeDigest {
            kind: WorktreeEntryKind::Symlink,
            executable: false,
            content: Some(hex(&hasher.finalize())),
        });
    }
    if metadata.is_dir() {
        return Ok(WorktreeDigest {
            kind: if gitlink {
                WorktreeEntryKind::Submodule
            } else {
                WorktreeEntryKind::Directory
            },
            executable: false,
            content: if gitlink {
                digest_submodule(git, &full, depth + 1)?
            } else {
                None
            },
        });
    }
    if !metadata.is_file() {
        // 设备文件、FIFO 等：不进内容，只用类型参与比较。
        return Ok(WorktreeDigest {
            kind: WorktreeEntryKind::Missing,
            executable: false,
            content: None,
        });
    }
    let mut file = std::fs::File::open(&full).map_err(|error| {
        AppError::coded(
            ErrorKind::Environment,
            "errors.commitSnapshot.failedReadWorkingTreeFile",
            "读取工作区文件失败（{0}）：{error}",
            &[
                ("0", format!("{:}", full.display())),
                ("error", format!("{}", error)),
            ],
        )
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; DIGEST_CHUNK_BYTES];
    loop {
        let read = file.read(&mut buffer).map_err(|error| {
            AppError::coded(
                ErrorKind::Environment,
                "errors.commitSnapshot.failedReadWorkingTreeFile2",
                "读取工作区文件失败（{0}）：{error}",
                &[
                    ("0", format!("{:}", full.display())),
                    ("error", format!("{}", error)),
                ],
            )
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(WorktreeDigest {
        kind: WorktreeEntryKind::File,
        executable: crate::platform::executable(&metadata),
        content: Some(hex(&hasher.finalize())),
    })
}

/// 子模块只读摘要；未初始化的空检出不向上查找父仓库。
fn digest_submodule(git: &Git, root: &Path, depth: usize) -> Result<Option<String>> {
    if depth > 32 {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.commitSnapshot.theSubmoduleNestedTooDeeplyCompare",
            "子模块嵌套过深，无法可靠对照 hooks 改动，请在终端提交",
            &[],
        ));
    }
    match std::fs::symlink_metadata(root.join(".git")) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(AppError::coded(
                ErrorKind::Environment,
                "errors.commitSnapshot.failedReadSubmoduleGitEntry",
                "读取子模块 Git 入口失败（{0}）：{error}",
                &[
                    ("0", format!("{:}", root.display())),
                    ("error", format!("{}", error)),
                ],
            ));
        }
        Ok(metadata)
            if metadata.file_type().is_symlink() || !(metadata.is_file() || metadata.is_dir()) =>
        {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.commitSnapshot.theSubmoduleGitEntryTypeHandled",
                "子模块 Git 入口类型无法安全处理",
                &[],
            ));
        }
        Ok(_) => {}
    }
    // 去掉父仓库的 Git 定位环境；所有文件仍走 safe_join，不跟随链接父路径。
    let submodule = git.in_isolated_directory(root);
    let prefix = submodule.run_ok(&["rev-parse", "--show-prefix"], "确认子模块根目录")?;
    if prefix.stdout.as_slice() != b"\n" {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.commitSnapshot.theSubmodulePathNoLongerMaps",
            "子模块路径不再对应其根目录，无法对照 hooks 改动",
            &[],
        ));
    }
    let head = crate::commit::read_head(&submodule)?;
    let index = submodule.run_ok(
        &["--no-optional-locks", "ls-files", "--stage", "-z"],
        "读取子模块暂存区",
    )?;
    let mut gitlinks = BTreeSet::new();
    for record in index
        .stdout
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let text = std::str::from_utf8(record).map_err(|_| {
            AppError::coded(
                ErrorKind::Rejected,
                "errors.commitSnapshot.theSubmoduleContainsNonUtf8",
                "子模块包含无法无损处理的非 UTF-8 路径，请在终端提交",
                &[],
            )
        })?;
        let (fields, path) = text.split_once('\t').ok_or_else(|| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commitSnapshot.cannotParseSubmoduleIndexEntryFormat",
                "子模块暂存区条目格式无法解析",
                &[],
            )
        })?;
        if fields.split(' ').next() == Some("160000") {
            gitlinks.insert(path.to_string());
        }
    }
    let listing = submodule.run_ok(
        &[
            "--no-optional-locks",
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ],
        "读取子模块文件列表",
    )?;
    let mut paths = BTreeSet::new();
    for record in listing
        .stdout
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        paths.insert(
            std::str::from_utf8(record)
                .map_err(|_| {
                    AppError::coded(
                        ErrorKind::Rejected,
                        "errors.commitSnapshot.theSubmoduleContainsNonUtf82",
                        "子模块包含无法无损处理的非 UTF-8 路径，请在终端提交",
                        &[],
                    )
                })?
                .to_string(),
        );
    }
    let mut hasher = Sha256::new();
    hash_digest_field(&mut hasher, head.as_deref().unwrap_or("").as_bytes());
    hash_digest_field(&mut hasher, &index.stdout);
    for path in paths {
        let digest = digest_path(&submodule, &path, gitlinks.contains(&path), depth)?;
        hash_digest_field(&mut hasher, path.as_bytes());
        hasher.update([digest.kind as u8, u8::from(digest.executable)]);
        hash_digest_field(
            &mut hasher,
            digest.content.as_deref().unwrap_or("").as_bytes(),
        );
    }
    Ok(Some(hex(&hasher.finalize())))
}

fn hash_digest_field(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

/// 拼接仓库内路径；拒绝绝对路径与 `..` 等越界分量（只读用途也要守住边界）。
fn safe_join(root: &Path, path: &str) -> Result<std::path::PathBuf> {
    let relative = Path::new(path);
    if path.is_empty()
        || (cfg!(windows) && path.contains('\\'))
        || relative
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
        || path.split('/').any(|part| {
            part.is_empty() || part == "." || part == ".." || part.eq_ignore_ascii_case(".git")
        })
    {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.commitSnapshot.theFilePathHandledSafely",
            "文件路径无法安全处理",
            &[],
        ));
    }
    let full = root.join(relative);
    if let Some(parent) = full.parent() {
        if parent != root {
            let parent_relative = parent.strip_prefix(root).map_err(|_| {
                AppError::coded(
                    ErrorKind::Rejected,
                    "errors.commitSnapshot.theFilePathEscapesRoot",
                    "文件路径越界",
                    &[],
                )
            })?;
            crate::recovery::checked_path(
                root,
                parent_relative.to_str().ok_or_else(|| {
                    AppError::coded(
                        ErrorKind::Rejected,
                        "errors.commitSnapshot.theFilePathHandledLosslessly",
                        "文件路径无法无损处理",
                        &[],
                    )
                })?,
            )?;
        }
    }
    Ok(full)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// hooks 运行后与提交前基准的对照：列出 hooks 修改的工作区/暂存区文件（规格 F04.3）。
///
/// 不自动撤销任何改动；只报告。
#[derive(Debug, Default)]
pub struct HookChanges {
    /// 工作区内容/类型/可执行位/存在状态发生变化的路径。
    pub worktree: Vec<String>,
    /// 暂存区条目集合、模式或对象发生变化的路径。
    pub index: Vec<String>,
}

impl HookChanges {
    pub fn is_empty(&self) -> bool {
        self.worktree.is_empty() && self.index.is_empty()
    }
}

/// 提交前基准：全部 tracked 与当时 dirty/untracked 路径的摘要 + 提交前暂存区。
pub struct HookBaseline {
    /// 提交前暂存区（路径 → (模式, 对象)），只含真实可提交条目。
    index: BTreeMap<String, (String, String)>,
    /// 提交前工作区摘要：全部 tracked 路径 ∪ 当时 dirty/untracked 路径。
    worktree: BTreeMap<String, WorktreeDigest>,
}

/// 在 commit 启动前捕获基准。
pub fn capture_hook_baseline(
    git: &Git,
    expectation: &crate::commit::CommitExpectation,
) -> Result<HookBaseline> {
    let index = crate::commit::index_blob_map(git)?;
    // 全部 tracked 路径：`ls-files -z` 只输出路径，内存开销与文件数成正比，
    // 不读文件字节；随后逐个算摘要（分片读取，内存有界）。
    let tracked = git.run_ok(
        &["-c", "core.quotePath=false", "ls-files", "-z"],
        "读取已跟踪文件列表",
    )?;
    let mut paths: BTreeSet<String> = BTreeSet::new();
    for record in tracked.stdout.split(|byte| *byte == 0) {
        if record.is_empty() {
            continue;
        }
        paths.insert(
            std::str::from_utf8(record)
                .map_err(|_| {
                    AppError::coded(
                        ErrorKind::Rejected,
                        "errors.commitSnapshot.theHooksComparisonContainsNonUtf",
                        "hook 对照包含无法无损处理的非 UTF-8 路径，请在终端提交",
                        &[],
                    )
                })?
                .to_string(),
        );
    }
    // 本次提交涉及的路径（含重命名源）一定在内；显式并进来，避免列表与预期不一致。
    for path in expectation.changed_paths() {
        paths.insert(path);
    }
    for entry in &expectation.index {
        if let Some(original) = &entry.original_path {
            paths.insert(original.clone());
        }
    }
    // 当时 dirty / untracked 的路径（未跟踪文件不在 ls-files 里）。
    let status = crate::git::status::read(git)?;
    for entry in &status.entries {
        paths.insert(entry.path.clone());
        if let Some(original) = &entry.original_path {
            paths.insert(original.clone());
        }
    }

    let mut worktree = BTreeMap::new();
    for path in paths {
        let gitlink = index.get(&path).is_some_and(|(mode, _)| mode == "160000");
        let digest = digest_path(git, &path, gitlink, 0)?;
        // 缺失的 tracked 路径也要记（它被删除也是一种状态）。
        worktree.insert(path, digest);
    }
    Ok(HookBaseline { index, worktree })
}

impl HookBaseline {
    /// 与当前状态对照，列出 hooks 修改过的文件。读取失败显式报错（不算「无变化」）。
    ///
    /// `committed_head`：本次提交成功后的新 HEAD（失败为 None）。
    /// - 工作区：`git commit` 不会改工作区，因此摘要的任何差异（含新出现/消失的
    ///   路径）都是 hooks 或外部改动，如实报告。
    /// - 暂存区：提交成功时 `git commit` 自己会把暂存区写成新 tree，因此
    ///   「提交前暂存区 / 新 tree / 提交后暂存区」三者任何不一致都是 hook 改动；
    ///   提交失败时直接比较提交前与当前的暂存区。
    pub fn diff_against(&self, git: &Git, committed_head: Option<&str>) -> Result<HookChanges> {
        // 当前路径集合：基准路径 ∪ 当前 tracked ∪ 当前 dirty/untracked。
        let mut paths: BTreeSet<String> = self.worktree.keys().cloned().collect();
        let tracked_now = git.run_ok(
            &["-c", "core.quotePath=false", "ls-files", "-z"],
            "读取已跟踪文件列表",
        )?;
        for record in tracked_now.stdout.split(|byte| *byte == 0) {
            if record.is_empty() {
                continue;
            }
            paths.insert(
                std::str::from_utf8(record)
                    .map_err(|_| {
                        AppError::coded(
                            ErrorKind::Rejected,
                            "errors.commitSnapshot.theHooksComparisonContainsNonUtf2",
                            "hook 对照包含无法无损处理的非 UTF-8 路径，请在终端提交",
                            &[],
                        )
                    })?
                    .to_string(),
            );
        }
        let status_now = crate::git::status::read(git)?;
        for entry in &status_now.entries {
            paths.insert(entry.path.clone());
            if let Some(original) = &entry.original_path {
                paths.insert(original.clone());
            }
        }
        // 先读当前暂存区（gitlink 判定与 index 对照都用到）。
        let index_now = crate::commit::index_blob_map(git)?;

        let mut worktree_changes = Vec::new();
        for path in &paths {
            let gitlink = index_now
                .get(path)
                .is_some_and(|(mode, _)| mode == "160000");
            let now = digest_path(git, path, gitlink, 0)?;
            match self.worktree.get(path) {
                Some(before) if *before == now => {}
                // 基准里没有这个路径：提交过程中新出现的改动（hook 新建文件等）。
                None if now.kind != WorktreeEntryKind::Missing => {
                    worktree_changes.push(path.clone());
                }
                None => {}
                // 基准里有、现在不同（含被删除）：hook 或外部改动。
                Some(_) => worktree_changes.push(path.clone()),
            }
        }

        let mut index_changes: BTreeSet<String> = BTreeSet::new();
        collect_index_difference(&self.index, &index_now, &mut index_changes);
        if let Some(head) = committed_head {
            // 新提交的 tree 与提交前/提交后的暂存区都必须一致。
            let tree = crate::commit::tree_blob_map(git, head)?;
            collect_index_difference(&tree, &self.index, &mut index_changes);
            collect_index_difference(&tree, &index_now, &mut index_changes);
        }
        Ok(HookChanges {
            worktree: worktree_changes,
            index: index_changes.into_iter().collect(),
        })
    }
}

/// 把两份「路径 → (模式, 对象)」表的差异累加进 `out`。
fn collect_index_difference(
    left: &BTreeMap<String, (String, String)>,
    right: &BTreeMap<String, (String, String)>,
    out: &mut BTreeSet<String>,
) {
    for (path, value) in left {
        if right.get(path) != Some(value) {
            out.insert(path.clone());
        }
    }
    for path in right.keys() {
        if !left.contains_key(path) {
            out.insert(path.clone());
        }
    }
}
