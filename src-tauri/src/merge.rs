//! 内置三方合并（Q143，设计见 docs/milestones/m4/merge-design.md）。
//!
//! 差异与合并结果全部由 Git 生成，Revia 不自己算合并：
//! - 三个版本按 `git ls-files -u` 给出的对象读取（同一次快照）；
//! - 改动块由 `git diff --no-index -U0` 分别求共同祖先到两侧的改动，
//!   按 Git 合并的规则（两侧改动在共同祖先里重叠或相接就是同一处）分组；
//! - 分组结果再与 `git merge-file --diff3` 的输出逐行核对：冲突处数相同、
//!   不冲突的部分逐行相同，才按块提供操作；对不上时只能接受一侧。
//!
//! 写入（应用、接受一侧）在会话层：写前校验指纹、先存回收区、写工作区、
//! `git add` 标记为已解决、写后核对。继续、中止、跳过不在 M4。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, Result};
use crate::git::Git;
use crate::operation::{self, OperationKind};

/// 冲突的两侧：你的版本（stage 2）与对方的版本（stage 3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum MergeSide {
    Ours,
    Theirs,
}

/// 栏头里一侧的身份（文案由界面按进行中的操作生成，设计第 2 节的表）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum MergeSideRole {
    /// merge：当前分支。
    CurrentBranch,
    /// merge：合并进来的分支或提交。
    MergedIn,
    /// rebase：上游（onto）。
    Upstream,
    /// rebase：正在重放的你的提交。
    YourCommit,
    /// cherry-pick、revert：HEAD。
    Head,
    /// cherry-pick、revert：被应用的提交。
    AppliedCommit,
    /// 取不到进行中的操作时：你的版本。
    Ours,
    /// 取不到进行中的操作时：对方的版本。
    Theirs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MergeSideLabel {
    pub role: MergeSideRole,
    /// 分支名或“短哈希 首行”；取不到时为空。
    pub name: Option<String>,
}

/// 一侧在冲突里的状态（照 Rebased 的 Conflicts 对话框）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ConflictSideState {
    Modified,
    Deleted,
    Added,
}

/// 不能三栏合并的原因（只能整文件接受一侧）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum MergeUnavailable {
    BothDeleted,
    OursDeleted,
    TheirsDeleted,
    Binary,
    Symlink,
    Submodule,
    /// 按块分组与 `git merge-file` 的结果对不上（例如用户配置改变了 diff 行为）。
    Inconsistent,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConflictFile {
    pub path: String,
    pub ours: ConflictSideState,
    pub theirs: ConflictSideState,
    /// 不能三栏合并时的原因；`None` 表示可以打开合并页签。
    pub unavailable: Option<MergeUnavailable>,
    /// 写前校验用（接受一侧）。
    pub fingerprint: String,
}

/// 一块改动属于哪一侧。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum MergeChunkKind {
    /// 只有你的版本改了。
    Ours,
    /// 只有对方的版本改了。
    Theirs,
    /// 两侧改得一样。
    Both,
    /// 两侧改得不一样：冲突。
    Conflict,
}

/// 一块改动在四栏里的位置（行号 0 基、左闭右开）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MergeChunk {
    pub id: u32,
    pub kind: MergeChunkKind,
    pub base_start: u32,
    pub base_end: u32,
    pub ours_start: u32,
    pub ours_end: u32,
    pub theirs_start: u32,
    pub theirs_end: u32,
    /// 初始结果里的区间。
    pub result_start: u32,
    pub result_end: u32,
    /// 初始结果是否已放进这一侧（自动应用无冲突改动时，不冲突的块为 true）。
    pub ours_applied: bool,
    pub theirs_applied: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MergeLayout {
    pub worktree_id: String,
    pub path: String,
    pub operation: Option<OperationKind>,
    pub left_label: MergeSideLabel,
    pub right_label: MergeSideLabel,
    /// 三个版本与初始结果（换行统一为 `\n`；`crlf` 记录写回时用的换行）。
    pub left_text: String,
    pub base_text: String,
    pub right_text: String,
    pub result_text: String,
    pub chunks: Vec<MergeChunk>,
    /// 不能三栏合并时的原因；此时只提供接受一侧。
    pub unavailable: Option<MergeUnavailable>,
    pub ours_state: ConflictSideState,
    pub theirs_state: ConflictSideState,
    pub crlf: bool,
    pub language: String,
    pub fingerprint: String,
}

/// 合并页签与冲突对话框的写结果。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MergeOutcome {
    pub path: String,
    pub recovery_record_id: Option<String>,
}

/// 一个冲突路径在暂存区里的条目（stage 1/2/3）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConflictStages {
    pub base: Option<StageEntry>,
    pub ours: Option<StageEntry>,
    pub theirs: Option<StageEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageEntry {
    pub mode: String,
    pub oid: String,
}

impl ConflictStages {
    pub fn side(&self, side: MergeSide) -> Option<&StageEntry> {
        match side {
            MergeSide::Ours => self.ours.as_ref(),
            MergeSide::Theirs => self.theirs.as_ref(),
        }
    }

    fn state(&self, side: MergeSide) -> ConflictSideState {
        match (self.side(side), &self.base) {
            (None, _) => ConflictSideState::Deleted,
            (Some(_), None) => ConflictSideState::Added,
            (Some(_), Some(_)) => ConflictSideState::Modified,
        }
    }
}

/// 读取未解决冲突的暂存区条目（`git ls-files -u -z`）；`path` 为空时读全部。
pub fn read_stages(git: &Git, path: Option<&str>) -> Result<BTreeMap<String, ConflictStages>> {
    let mut args = vec!["--literal-pathspecs", "-c", "core.quotePath=false", "ls-files", "-u", "-z"];
    if let Some(path) = path {
        args.extend(["--", path]);
    }
    let output = git.run_ok(&args, "读取冲突条目")?;
    let mut stages: BTreeMap<String, ConflictStages> = BTreeMap::new();
    // 每条：`<mode> <oid> <stage>\t<path>`
    for entry in output.stdout.split(|byte| *byte == 0).filter(|entry| !entry.is_empty()) {
        let entry = String::from_utf8_lossy(entry);
        let Some((meta, path)) = entry.split_once('\t') else { continue };
        let mut fields = meta.split_whitespace();
        let (Some(mode), Some(oid), Some(stage)) = (fields.next(), fields.next(), fields.next()) else {
            continue;
        };
        let item = Some(StageEntry { mode: mode.to_string(), oid: oid.to_string() });
        let slot = stages.entry(path.to_string()).or_default();
        match stage {
            "1" => slot.base = item,
            "2" => slot.ours = item,
            "3" => slot.theirs = item,
            _ => {}
        }
    }
    Ok(stages)
}

fn stages_of(git: &Git, path: &str) -> Result<ConflictStages> {
    read_stages(git, Some(path))?
        .remove(path)
        .ok_or_else(|| AppError::rejected(format!("{path} 没有未解决的冲突")))
}

fn blob(git: &Git, entry: Option<&StageEntry>) -> Result<Option<Vec<u8>>> {
    let Some(entry) = entry else { return Ok(None) };
    Ok(Some(git.run_ok(&["cat-file", "blob", &entry.oid], "读取冲突版本")?.stdout))
}

/// 与 Git 相同的二进制判断：前 8000 字节里有 NUL。
fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8000).any(|byte| *byte == 0)
}

/// 不能三栏合并的原因（只看模式与是否二进制，不读工作区）。
fn unavailable(stages: &ConflictStages, blobs: &[Option<&Vec<u8>>]) -> Option<MergeUnavailable> {
    match (&stages.ours, &stages.theirs) {
        (None, None) => return Some(MergeUnavailable::BothDeleted),
        (None, Some(_)) => return Some(MergeUnavailable::OursDeleted),
        (Some(_), None) => return Some(MergeUnavailable::TheirsDeleted),
        _ => {}
    }
    let has_mode = |mode: &str| [&stages.base, &stages.ours, &stages.theirs].into_iter().flatten().any(|entry| entry.mode == mode);
    if has_mode("160000") {
        return Some(MergeUnavailable::Submodule);
    }
    if has_mode("120000") {
        return Some(MergeUnavailable::Symlink);
    }
    if blobs.iter().flatten().any(|bytes| is_binary(bytes)) {
        return Some(MergeUnavailable::Binary);
    }
    None
}

/// 列出未解决冲突的文件、两侧状态与指纹（冲突对话框用）。
pub fn list_conflicts(git: &Git) -> Result<Vec<ConflictFile>> {
    let operation = operation_version(git)?;
    read_stages(git, None)?
        .into_iter()
        .map(|(path, stages)| {
            let blobs = [blob(git, stages.base.as_ref())?, blob(git, stages.ours.as_ref())?, blob(git, stages.theirs.as_ref())?];
            let reason = unavailable(&stages, &[blobs[0].as_ref(), blobs[1].as_ref(), blobs[2].as_ref()]);
            Ok(ConflictFile {
                ours: stages.state(MergeSide::Ours),
                theirs: stages.state(MergeSide::Theirs),
                unavailable: reason,
                fingerprint: fingerprint_with(git, &path, &stages, &operation)?,
                path,
            })
        })
        .collect()
}

/// 进行中操作的元数据版本（指纹的一部分：换了一步 rebase 也让旧页签失效）。
fn operation_version(git: &Git) -> Result<String> {
    Ok(operation::detect(git, 0)?.map(|state| state.metadata_version).unwrap_or_default())
}

/// 合并相关的指纹：三个版本的模式与对象、工作区文件、进行中的操作。
pub fn fingerprint(git: &Git, path: &str) -> Result<String> {
    let stages = stages_of(git, path)?;
    fingerprint_with(git, path, &stages, &operation_version(git)?)
}

fn fingerprint_with(git: &Git, path: &str, stages: &ConflictStages, operation: &str) -> Result<String> {
    let stage_text = [&stages.base, &stages.ours, &stages.theirs]
        .iter()
        .map(|entry| entry.as_ref().map(|entry| format!("{} {}", entry.mode, entry.oid)).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    let worktree = worktree_state(&git.directory.join(path))?;
    Ok(crate::git::fingerprint(&[stage_text.as_bytes(), &worktree, operation.as_bytes()]))
}

/// 工作区里这个路径的状态（不跟随符号链接）。
fn worktree_state(full: &Path) -> Result<Vec<u8>> {
    match std::fs::symlink_metadata(full) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(b"missing".to_vec()),
        Err(error) => Err(AppError::rejected(format!("读取工作区文件失败：{error}"))),
        Ok(meta) if meta.file_type().is_symlink() => {
            let target = std::fs::read_link(full).map_err(|error| AppError::rejected(format!("读取符号链接失败：{error}")))?;
            Ok([b"link:".as_slice(), target.to_string_lossy().as_bytes()].concat())
        }
        Ok(meta) if meta.is_dir() => Ok(b"dir".to_vec()),
        Ok(meta) => {
            let mut bytes = std::fs::read(full).map_err(|error| AppError::rejected(format!("读取工作区文件失败：{error}")))?;
            bytes.push(if is_executable(&meta) { b'x' } else { b'-' });
            Ok(bytes)
        }
    }
}

#[cfg(unix)]
fn is_executable(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_meta: &std::fs::Metadata) -> bool {
    false
}

/// 两侧栏头（设计第 2 节的表）。
pub fn side_labels(git: &Git, kind: Option<OperationKind>) -> (MergeSideLabel, MergeSideLabel) {
    let label = |role, name: Option<String>| MergeSideLabel { role, name };
    let describe = |rev: &str| -> Option<String> {
        let oid = git.try_run(&["rev-parse", "--verify", "-q", rev])?;
        let text = git.try_run(&["log", "-1", "--format=%h %s", oid.trim()])?;
        Some(text.trim().to_string())
    };
    match kind {
        Some(OperationKind::Merge) => {
            let merged = git
                .try_run(&["rev-parse", "--verify", "-q", "MERGE_HEAD"])
                .and_then(|oid| {
                    let oid = oid.trim().to_string();
                    git.try_run(&["name-rev", "--name-only", "--refs=refs/heads/*", "--no-undefined", &oid])
                        .map(|name| name.trim().to_string())
                        .or_else(|| Some(oid.chars().take(8).collect()))
                });
            (label(MergeSideRole::CurrentBranch, git.current_branch()), label(MergeSideRole::MergedIn, merged))
        }
        Some(OperationKind::Rebase) => {
            let onto = rebase_onto(git).and_then(|oid| describe(&oid));
            (label(MergeSideRole::Upstream, onto), label(MergeSideRole::YourCommit, describe("REBASE_HEAD")))
        }
        Some(OperationKind::CherryPick) => (label(MergeSideRole::Head, None), label(MergeSideRole::AppliedCommit, describe("CHERRY_PICK_HEAD"))),
        Some(OperationKind::Revert) => (label(MergeSideRole::Head, None), label(MergeSideRole::AppliedCommit, describe("REVERT_HEAD"))),
        None => (label(MergeSideRole::Ours, None), label(MergeSideRole::Theirs, None)),
    }
}

/// rebase 的 onto 提交（路径由 Git 解析，linked worktree 也正确）。
fn rebase_onto(git: &Git) -> Option<String> {
    ["rebase-merge/onto", "rebase-apply/onto"].iter().find_map(|name| {
        let path = git.try_run(&["rev-parse", "--path-format=absolute", "--git-path", name])?;
        let text = std::fs::read_to_string(PathBuf::from(path.trim_end_matches('\n'))).ok()?;
        Some(text.trim().to_string()).filter(|oid| !oid.is_empty())
    })
}

/// 临时目录：放在 Revia 数据目录下，用完删除（设计第 3 节）。
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn create(data_dir: &Path) -> Result<Self> {
        let unique = format!(
            "merge-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|value| value.as_nanos())
                .unwrap_or_default()
        );
        let path = data_dir.join("tmp").join(unique);
        std::fs::create_dir_all(&path).map_err(|error| AppError::internal(format!("创建临时目录失败：{error}")))?;
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, name: &str, bytes: &[u8]) -> Result<PathBuf> {
        let path = self.path.join(name);
        std::fs::write(&path, bytes).map_err(|error| AppError::internal(format!("写临时文件失败：{error}")))?;
        Ok(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// 一侧相对共同祖先的一处改动（行号 0 基、左闭右开）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Change {
    side: MergeSide,
    base_start: u32,
    base_end: u32,
    side_start: u32,
    side_end: u32,
}

impl Change {
    fn delta(&self) -> i64 {
        (self.side_end - self.side_start) as i64 - (self.base_end - self.base_start) as i64
    }
}

/// diff 与 merge-file 用同一种算法，不受用户 diff 配置影响。
const DIFF_ALGORITHM: &str = "--diff-algorithm=myers";

/// 共同祖先到一侧的改动（`git diff --no-index -U0`）。
fn side_changes(git: &Git, base: &Path, side_path: &Path, side: MergeSide) -> Result<Vec<Change>> {
    let base_arg = base.display().to_string();
    let side_arg = side_path.display().to_string();
    let output = git.run_allow_difference(
        &[
            "diff",
            "--no-index",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            DIFF_ALGORITHM,
            "--no-indent-heuristic",
            "-U0",
            "--",
            &base_arg,
            &side_arg,
        ],
        "读取合并改动",
    )?;
    let range = |start: u32, count: u32| {
        // `-U0` 里计数为 0 的一侧写的是“在第 N 行之后”。
        let start = if count == 0 { start } else { start - 1 };
        (start, start + count)
    };
    Ok(crate::git::diff::parse_bytes(&output.stdout)
        .files
        .iter()
        .flat_map(|file| &file.hunks)
        .map(|hunk| {
            let (base_start, base_end) = range(hunk.old_start, hunk.old_count);
            let (side_start, side_end) = range(hunk.new_start, hunk.new_count);
            Change { side, base_start, base_end, side_start, side_end }
        })
        .collect())
}

/// 按行切开（每行带自己的行尾），与 Git 的行一致。
fn lines(bytes: &[u8]) -> Vec<&[u8]> {
    bytes.split_inclusive(|byte| *byte == b'\n').collect()
}

/// 一份文本与每行的起点（多一个元素是文末），按行号取连续的一段。
struct Text<'a> {
    bytes: &'a [u8],
    starts: Vec<usize>,
}

impl<'a> Text<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        let mut starts = vec![0];
        starts.extend(lines(bytes).iter().scan(0, |offset, line| {
            *offset += line.len();
            Some(*offset)
        }));
        Self { bytes, starts }
    }

    fn line_count(&self) -> u32 {
        (self.starts.len() - 1) as u32
    }

    fn slice(&self, start: u32, end: u32) -> &'a [u8] {
        let bytes: &'a [u8] = self.bytes;
        &bytes[self.starts[start as usize]..self.starts[end.max(start) as usize]]
    }
}

/// 把两侧的改动按 Git 合并的规则分成块。
fn group_chunks(ours: &Text, theirs: &Text, changes: Vec<Change>) -> Vec<MergeChunk> {
    let mut changes = changes;
    changes.sort_by_key(|change| (change.base_start, change.base_end));
    let mut groups: Vec<Vec<Change>> = Vec::new();
    for change in changes {
        // 重叠或相接（Git 的 xdl_merge：一侧改动结束处紧挨另一侧开始也算冲突）。
        let joins = groups.last().is_some_and(|group| {
            let end = group.iter().map(|item| item.base_end).max().unwrap_or(0);
            change.base_start <= end
        });
        if joins {
            groups.last_mut().unwrap().push(change);
        } else {
            groups.push(vec![change]);
        }
    }
    let (mut ours_delta, mut theirs_delta) = (0i64, 0i64);
    let mut chunks = Vec::with_capacity(groups.len());
    for (id, group) in groups.iter().enumerate() {
        let base_start = group.iter().map(|change| change.base_start).min().unwrap_or(0);
        let base_end = group.iter().map(|change| change.base_end).max().unwrap_or(0);
        let delta_of = |side| group.iter().filter(|change| change.side == side).map(Change::delta).sum::<i64>();
        let has = |side| group.iter().any(|change| change.side == side);
        let ours_start = (base_start as i64 + ours_delta) as u32;
        let theirs_start = (base_start as i64 + theirs_delta) as u32;
        ours_delta += delta_of(MergeSide::Ours);
        theirs_delta += delta_of(MergeSide::Theirs);
        let ours_end = (base_end as i64 + ours_delta) as u32;
        let theirs_end = (base_end as i64 + theirs_delta) as u32;
        let kind = match (has(MergeSide::Ours), has(MergeSide::Theirs)) {
            (true, false) => MergeChunkKind::Ours,
            (false, true) => MergeChunkKind::Theirs,
            _ if ours.slice(ours_start, ours_end) == theirs.slice(theirs_start, theirs_end) => MergeChunkKind::Both,
            _ => MergeChunkKind::Conflict,
        };
        chunks.push(MergeChunk {
            id: id as u32,
            kind,
            base_start,
            base_end,
            ours_start,
            ours_end,
            theirs_start,
            theirs_end,
            result_start: 0,
            result_end: 0,
            ours_applied: false,
            theirs_applied: false,
        });
    }
    chunks
}

/// 按块拼出结果：`apply` 为 true 时放进不冲突的块，冲突块保留共同祖先的内容。
/// 同时填好每块在结果里的区间与初始状态。
fn assemble(base: &Text, ours: &Text, theirs: &Text, chunks: &mut [MergeChunk], apply: bool) -> Vec<u8> {
    let mut out = Vec::new();
    let mut line_count = 0u32;
    let mut cursor = 0u32;
    let push = |out: &mut Vec<u8>, bytes: &[u8], line_count: &mut u32| {
        out.extend_from_slice(bytes);
        *line_count += lines(bytes).len() as u32;
    };
    for chunk in chunks.iter_mut() {
        push(&mut out, base.slice(cursor, chunk.base_start), &mut line_count);
        chunk.result_start = line_count;
        let content = match (chunk.kind, apply) {
            (MergeChunkKind::Ours | MergeChunkKind::Both, true) => ours.slice(chunk.ours_start, chunk.ours_end),
            (MergeChunkKind::Theirs, true) => theirs.slice(chunk.theirs_start, chunk.theirs_end),
            _ => base.slice(chunk.base_start, chunk.base_end),
        };
        push(&mut out, content, &mut line_count);
        chunk.result_end = line_count;
        chunk.ours_applied = apply && matches!(chunk.kind, MergeChunkKind::Ours | MergeChunkKind::Both);
        chunk.theirs_applied = apply && matches!(chunk.kind, MergeChunkKind::Theirs | MergeChunkKind::Both);
        cursor = chunk.base_end;
    }
    push(&mut out, base.slice(cursor, base.line_count()), &mut line_count);
    out
}

const MARKER_SIZE: usize = 24;

/// 运行 `git merge-file -p --diff3`：返回（输出，冲突数）。
fn merge_file(git: &Git, ours: &Path, base: &Path, theirs: &Path) -> Result<(Vec<u8>, u32)> {
    let marker = format!("--marker-size={MARKER_SIZE}");
    let args = [
        "merge-file",
        "-p",
        "--diff3",
        DIFF_ALGORITHM,
        &marker,
        "-L",
        "ours",
        "-L",
        "base",
        "-L",
        "theirs",
        &ours.display().to_string(),
        &base.display().to_string(),
        &theirs.display().to_string(),
    ]
    .map(String::from);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = git.run(&args)?;
    // 退出码就是冲突数（上限 127）；更大表示出错。
    if output.timed_out || !(0..128).contains(&output.exit_code) {
        return Err(AppError::git(format!("git merge-file 执行失败：{}", output.stderr.trim())));
    }
    Ok((output.stdout, output.exit_code as u32))
}

/// 把 merge-file 的输出里每处冲突换成共同祖先那一段，返回（各行，冲突数）。
fn merge_file_with_base(output: &[u8]) -> (Vec<Vec<u8>>, u32) {
    let marker = |ch: u8| {
        let prefix = vec![ch; MARKER_SIZE];
        move |line: &[u8]| line.starts_with(&prefix) && matches!(line.get(MARKER_SIZE), Some(b' ' | b'\n' | b'\r') | None)
    };
    let (open, base_mark, separator, close) = (marker(b'<'), marker(b'|'), marker(b'='), marker(b'>'));
    let mut result = Vec::new();
    let mut conflicts = 0;
    let mut section: Option<u8> = None; // 冲突里的哪一段：o / b / t
    for line in lines(output) {
        match section {
            None if open(line) => {
                conflicts += 1;
                section = Some(b'o');
            }
            None => result.push(strip_eol(line).to_vec()),
            Some(_) if base_mark(line) => section = Some(b'b'),
            Some(_) if separator(line) => section = Some(b't'),
            Some(_) if close(line) => section = None,
            Some(b'b') => result.push(strip_eol(line).to_vec()),
            Some(_) => {}
        }
    }
    (result, conflicts)
}

fn strip_eol(line: &[u8]) -> &[u8] {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    line.strip_suffix(b"\r").unwrap_or(line)
}

fn to_display(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace("\r\n", "\n")
}

/// 构造合并页签的数据。
pub fn build_layout(git: &Git, data_dir: &Path, worktree_id: &str, path: &str, auto_apply: bool, language: String) -> Result<MergeLayout> {
    let stages = stages_of(git, path)?;
    let operation = operation::detect(git, 0)?;
    let fingerprint = fingerprint_with(
        git,
        path,
        &stages,
        &operation.as_ref().map(|state| state.metadata_version.clone()).unwrap_or_default(),
    )?;
    let kind = operation.as_ref().map(|state| state.kind);
    let (left_label, right_label) = side_labels(git, kind);
    let base_bytes = blob(git, stages.base.as_ref())?.unwrap_or_default();
    let ours_bytes = blob(git, stages.ours.as_ref())?;
    let theirs_bytes = blob(git, stages.theirs.as_ref())?;
    let mut layout = MergeLayout {
        worktree_id: worktree_id.to_string(),
        path: path.to_string(),
        operation: kind,
        left_label,
        right_label,
        left_text: String::new(),
        base_text: String::new(),
        right_text: String::new(),
        result_text: String::new(),
        chunks: Vec::new(),
        unavailable: unavailable(&stages, &[Some(&base_bytes), ours_bytes.as_ref(), theirs_bytes.as_ref()]),
        ours_state: stages.state(MergeSide::Ours),
        theirs_state: stages.state(MergeSide::Theirs),
        crlf: false,
        language,
        fingerprint,
    };
    if layout.unavailable.is_some() {
        return Ok(layout);
    }
    let (ours_bytes, theirs_bytes) = (ours_bytes.unwrap_or_default(), theirs_bytes.unwrap_or_default());
    let Some((chunks, result)) = chunk_merge(git, data_dir, &base_bytes, &ours_bytes, &theirs_bytes, auto_apply)? else {
        layout.unavailable = Some(MergeUnavailable::Inconsistent);
        return Ok(layout);
    };
    layout.crlf = ours_bytes.windows(2).any(|pair| pair == b"\r\n");
    layout.left_text = to_display(&ours_bytes);
    layout.base_text = to_display(&base_bytes);
    layout.right_text = to_display(&theirs_bytes);
    layout.result_text = to_display(&result);
    layout.chunks = chunks;
    Ok(layout)
}

/// 按块分好三份内容并拼出初始结果；与 `git merge-file` 对不上时返回 `None`。
pub fn chunk_merge(
    git: &Git,
    data_dir: &Path,
    base_bytes: &[u8],
    ours_bytes: &[u8],
    theirs_bytes: &[u8],
    auto_apply: bool,
) -> Result<Option<(Vec<MergeChunk>, Vec<u8>)>> {
    let dir = TempDir::create(data_dir)?;
    let base_path = dir.write("base", base_bytes)?;
    let ours_path = dir.write("ours", ours_bytes)?;
    let theirs_path = dir.write("theirs", theirs_bytes)?;
    let mut changes = side_changes(git, &base_path, &ours_path, MergeSide::Ours)?;
    changes.extend(side_changes(git, &base_path, &theirs_path, MergeSide::Theirs)?);
    let (base_lines, ours_lines, theirs_lines) = (Text::new(base_bytes), Text::new(ours_bytes), Text::new(theirs_bytes));
    let mut chunks = group_chunks(&ours_lines, &theirs_lines, changes);

    // 与 Git 的合并结果核对：冲突处数相同，冲突块保留共同祖先时逐行相同。
    let (merged, conflict_count) = merge_file(git, &ours_path, &base_path, &theirs_path)?;
    let (expected, marked) = merge_file_with_base(&merged);
    let check = assemble(&base_lines, &ours_lines, &theirs_lines, &mut chunks, true);
    let assembled: Vec<&[u8]> = lines(&check).into_iter().map(strip_eol).collect();
    let conflicts = chunks.iter().filter(|chunk| chunk.kind == MergeChunkKind::Conflict).count() as u32;
    if conflicts != conflict_count.min(127) || conflicts != marked || assembled != expected {
        return Ok(None);
    }
    let result = assemble(&base_lines, &ours_lines, &theirs_lines, &mut chunks, auto_apply);
    Ok(Some((chunks, result)))
}

/// 合并结果按“你的版本”的换行写回（设计第 4 节）。
pub fn result_bytes(text: &str, crlf: bool) -> Vec<u8> {
    let text = text.replace("\r\n", "\n");
    if crlf { text.replace('\n', "\r\n").into_bytes() } else { text.into_bytes() }
}

/// 写入后暂存区应有的对象：按这个路径的属性（过滤、换行转换）计算工作区文件的对象 ID。
pub fn worktree_blob_id(git: &Git, path: &str) -> Result<String> {
    let full = git.directory.join(path).display().to_string();
    let output = git.run_ok(&["hash-object", "--path", path, "--", &full], "计算合并结果的对象")?;
    Ok(output.stdout_lossy().trim().to_string())
}

/// 暂存区 stage 0 的对象 ID；路径不在暂存区时为 None。
pub fn index_blob_id(git: &Git, path: &str) -> Option<String> {
    let spec = format!(":0:{path}");
    git.try_run(&["rev-parse", "--verify", "-q", &spec]).map(|oid| oid.trim().to_string())
}

/// 整文件接受一侧时要用的 stage（与 `git checkout --ours/--theirs` 一致）。
pub fn checkout_flag(side: MergeSide) -> &'static str {
    match side {
        MergeSide::Ours => "--ours",
        MergeSide::Theirs => "--theirs",
    }
}

/// 接受一侧后暂存区应有的对象（该侧被删除时为 None）。
pub fn side_blob_id(stages: &ConflictStages, side: MergeSide) -> Option<String> {
    stages.side(side).map(|entry| entry.oid.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(side: MergeSide, base: (u32, u32), side_range: (u32, u32)) -> Change {
        Change { side, base_start: base.0, base_end: base.1, side_start: side_range.0, side_end: side_range.1 }
    }

    #[test]
    fn touching_changes_from_both_sides_form_one_conflict() {
        let ours = Text::new(b"a\nB\nc\nd\n");
        let theirs = Text::new(b"a\nb\nC\nd\n");
        let chunks = group_chunks(
            &ours,
            &theirs,
            vec![change(MergeSide::Ours, (1, 2), (1, 2)), change(MergeSide::Theirs, (2, 3), (2, 3))],
        );
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].kind, MergeChunkKind::Conflict);
        assert_eq!((chunks[0].base_start, chunks[0].base_end), (1, 3));
        assert_eq!((chunks[0].ours_start, chunks[0].ours_end), (1, 3));
    }

    #[test]
    fn separated_changes_keep_side_offsets() {
        // 你的版本在第 1 行后插入两行；对方改了第 4 行。
        let ours = Text::new(b"a\nx\ny\nb\nc\nd\n");
        let theirs = Text::new(b"a\nb\nc\nD\n");
        let chunks = group_chunks(
            &ours,
            &theirs,
            vec![change(MergeSide::Ours, (1, 1), (1, 3)), change(MergeSide::Theirs, (3, 4), (3, 4))],
        );
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[1].kind, MergeChunkKind::Theirs);
        assert_eq!((chunks[1].ours_start, chunks[1].ours_end), (5, 6), "对方块在你的版本里的位置要算上前面插入的两行");
    }

    #[test]
    fn identical_changes_are_not_conflicts() {
        let ours = Text::new(b"a\nX\nc\n");
        let theirs = Text::new(b"a\nX\nc\n");
        let chunks = group_chunks(
            &ours,
            &theirs,
            vec![change(MergeSide::Ours, (1, 2), (1, 2)), change(MergeSide::Theirs, (1, 2), (1, 2))],
        );
        assert_eq!(chunks[0].kind, MergeChunkKind::Both);
    }

    #[test]
    fn merge_file_output_conflicts_become_base_lines() {
        let marker = |ch: char| ch.to_string().repeat(MARKER_SIZE);
        let output = format!(
            "a\n{} ours\nO\n{} base\nB\n{}\nT\n{} theirs\nz\n",
            marker('<'),
            marker('|'),
            marker('='),
            marker('>')
        );
        let (result, conflicts) = merge_file_with_base(output.as_bytes());
        assert_eq!(conflicts, 1);
        assert_eq!(result, vec![b"a".to_vec(), b"B".to_vec(), b"z".to_vec()]);
    }

    #[test]
    fn result_keeps_ours_line_endings() {
        assert_eq!(result_bytes("a\nb\n", true), b"a\r\nb\r\n");
        assert_eq!(result_bytes("a\r\nb\n", false), b"a\nb\n");
    }
}
