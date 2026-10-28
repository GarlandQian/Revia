//! 提交历史领域：只读目标、分页查询、提交详情、改动文件与提交 diff（M5 任务 01）。
//!
//! 与工作区会话分开（规格 H01–H04）：
//! - 历史目标绑定到**打开时的** worktree 会话实例与完整提交对象，不随工作区、分支或
//!   引用移动而改变；已打开的提交 diff 在修改工作区、暂存或切分支后仍是同一份内容；
//! - 读写目标使用独立的 `target_id`，关闭后再请求会被拒绝，旧查询游标随之失效；
//! - 改动文件与对象 ID 一律来自 Git 的机器格式，路径身份不从显示 patch 反推；
//! - 不签发任何 `HunkAction` / `SegmentAction`，历史 diff 只读；
//! - 内容缓存有界，超限按淘汰清空，语义在报告里写明。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, Result};
use crate::git::highlight::{HighlightRequest, HighlightResult, Highlighter};
use crate::git::history::{self as gh, ChangeStatus, RawChange};
use crate::git::rows::{self, RenderRow, ViewMode};
use crate::git::segments::{self, ChangeSegment, FoldRange, IgnoreWhitespace};
use crate::git::diff::LineKind;
use crate::git::Git;
use crate::session::{self, FileVersion, WorktreeSession};

/// 同时保留的历史目标数（工程限额）。
pub const MAX_TARGETS: usize = 4;
/// 每个目标最多保留多少个未走完的查询（工程限额）。
pub const MAX_OPEN_QUERIES: usize = 8;
/// 每个目标最多缓存多少份文件 diff 内容（工程限额）。
pub const MAX_CACHED_DIFFS: usize = 4;
/// 每个目标最多缓存多少份显示排版（工程限额）。
pub const MAX_CACHED_LAYOUTS: usize = 8;
/// 每个目标最多缓存多少份左右对照的段与折叠区（工程限额）。
pub const MAX_CACHED_SIDES: usize = 4;
/// 一个提交最多返回多少个改动文件；超过时明确标为截断。
pub const MAX_COMMIT_FILES: usize = 5_000;
/// 左右对照支持的最大单侧行数。
pub const MAX_SIDE_LINES: u32 = 200_000;

/// 历史目标的资源限额（随目标一起返回，前端不用猜）。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HistoryLimits {
    pub max_page_size: u32,
    pub max_query_span: u32,
    pub max_open_queries: u32,
    pub max_cached_diffs: u32,
    pub max_commit_files: u32,
    pub max_diff_bytes: u32,
    pub max_blob_bytes: u32,
}

impl HistoryLimits {
    fn current() -> Self {
        Self {
            max_page_size: gh::MAX_PAGE_LIMIT,
            max_query_span: gh::MAX_QUERY_SPAN,
            max_open_queries: MAX_OPEN_QUERIES as u32,
            max_cached_diffs: MAX_CACHED_DIFFS as u32,
            max_commit_files: MAX_COMMIT_FILES as u32,
            max_diff_bytes: gh::MAX_DIFF_BYTES as u32,
            max_blob_bytes: gh::MAX_BLOB_BYTES as u32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum HistoryRefKind {
    Branch,
    RemoteBranch,
    Tag,
    Other,
}

impl HistoryRefKind {
    fn parse(value: &str) -> Self {
        match value {
            "branch" => Self::Branch,
            "remoteBranch" => Self::RemoteBranch,
            "tag" => Self::Tag,
            _ => Self::Other,
        }
    }
}

/// 一个本地引用及其指向的提交。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HistoryRef {
    pub full_name: String,
    pub short_name: String,
    pub kind: HistoryRefKind,
    pub commit: String,
    /// HEAD 当前是否指向这个引用。
    pub current: bool,
}

/// 提交上用于显示的引用标签。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HistoryRefLabel {
    pub full_name: String,
    pub short_name: String,
    pub kind: HistoryRefKind,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HistoryTargetInfo {
    pub target_id: String,
    pub worktree_id: String,
    /// 打开时 HEAD 的完整提交 ID；空仓库为 null。
    pub head: Option<String>,
    /// HEAD 指向的引用名；分离 HEAD 为 null。
    pub head_ref: Option<String>,
    /// 仓库还没有任何提交（与“读取失败”分开表示）。
    pub empty_repository: bool,
    pub refs: Vec<HistoryRef>,
    pub limits: HistoryLimits,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum HistoryStart {
    /// 本地引用全名（例如 `refs/heads/main`）。必须存在于本地引用列表，不解释 rev 语法。
    Ref {
        #[serde(rename = "fullName")]
        full_name: String,
    },
    /// 完整提交对象 ID。
    Commit { oid: String },
    /// 查询开始时解析 HEAD；空仓库明确报错。
    Head,
    /// 多个本地引用的全名（分支 / 远端分支 / 标签）。空列表表示「全部」：
    /// 查询开始时把当时每个引用（含 HEAD）解析成完整提交 ID 并固定下来。
    Refs {
        #[serde(rename = "fullNames")]
        full_names: Vec<String>,
    },
}

/// 日期筛选（L04）。最近 N 天在**查询开始时**换算成固定时间点，游标随之绑定。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum HistoryDateFilter {
    /// 最近 N 天（1–3650）。
    LastDays { days: u32 },
    /// 起止日期，形如 `2026-10-01`（含起止两天）。
    Range { since: String, until: String },
}

/// 日志筛选（L04）：前端只传结构化条件，由这里组合成固定 Git 参数。
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct HistoryFilter {
    /// 文字或哈希前缀；空字符串等同没有筛选。
    pub text: Option<String>,
    pub case_sensitive: bool,
    /// Git 的扩展正则（`--extended-regexp`）；关闭时按字面匹配。
    pub regex: bool,
    /// 分支 / 标签的完整引用名；空列表表示全部（本地、远端、标签与 HEAD 可达）。
    pub branches: Vec<String>,
    /// 作者（邮箱或 `名字 <邮箱>`）；空列表表示全部。
    pub authors: Vec<String>,
    /// 只包含这些路径（字面匹配，不跟随重命名）；空列表表示全部。
    pub paths: Vec<String>,
    pub date: Option<HistoryDateFilter>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HistorySignature {
    pub name: String,
    pub email: String,
    /// Unix 秒。
    pub time: i64,
    /// 形如 `+0800` 的时区。
    pub timezone: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HistoryCommit {
    pub oid: String,
    /// 真实的全部父提交 ID（不受“只走第一父提交”影响）。
    pub parents: Vec<String>,
    pub author: HistorySignature,
    pub committer: HistorySignature,
    pub subject: String,
    /// 指向该提交的引用标签（查询开始时的快照）。
    pub refs: Vec<HistoryRefLabel>,
}

/// 一页提交。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HistoryPage {
    pub target_id: String,
    pub query_id: String,
    /// 查询开始时解析出的完整提交 ID；引用移动后这条查询仍读原对象。
    pub start: String,
    pub first_parent: bool,
    pub commit_count: u32,
    /// 下一页游标；到达确实的历史末尾时为 null。
    pub next_cursor: Option<String>,
    /// 是否已经读到这段历史的末尾（与“超过限额”“游标过期”不同）。
    pub exhausted: bool,
    pub commits: Vec<HistoryCommit>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HistoryCommitDetail {
    pub target_id: String,
    pub oid: String,
    pub tree: String,
    pub parents: Vec<String>,
    pub author: HistorySignature,
    pub committer: HistorySignature,
    /// 完整提交正文（不截断）。
    pub message: String,
    /// 提交声明的 `encoding` 头。
    pub message_encoding: Option<String>,
    /// 正文是否发生过有损解码（无法按 UTF-8 无损读取时为 true）。
    pub message_lossy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum HistoryFileStatus {
    Added,
    Deleted,
    Modified,
    Renamed,
    Copied,
    TypeChanged,
}

impl HistoryFileStatus {
    fn from_change(status: ChangeStatus) -> Self {
        match status {
            ChangeStatus::Added => Self::Added,
            ChangeStatus::Deleted => Self::Deleted,
            ChangeStatus::Modified => Self::Modified,
            ChangeStatus::Renamed => Self::Renamed,
            ChangeStatus::Copied => Self::Copied,
            ChangeStatus::TypeChanged => Self::TypeChanged,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Deleted => "deleted",
            Self::Modified => "modified",
            Self::Renamed => "renamed",
            Self::Copied => "copied",
            Self::TypeChanged => "typeChanged",
        }
    }
}

/// 不可按文本打开的原因（历史 diff 本身始终只读）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum HistoryReadonlyReason {
    /// 路径含非 UTF-8 字节：不做有损转换后冒充另一条可打开路径。
    NonUtf8Path,
    /// 子模块（gitlink）：内容在另一个仓库里。
    Submodule,
}

impl HistoryReadonlyReason {
    pub fn description(self) -> &'static str {
        match self {
            Self::NonUtf8Path => "路径含非 UTF-8 字节，无法无损打开该提交中的文件",
            Self::Submodule => "子模块的内容在另一个仓库里，历史 diff 中不展开",
        }
    }
}

/// 空 diff 的明确原因（不用空结果吞掉错误）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum HistoryEmptyReason {
    Binary,
    ModeOnly,
    EmptyFile,
}

/// 提交相对某个父提交改动的一个文件。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HistoryFile {
    pub status: HistoryFileStatus,
    /// 主路径：删除取旧路径，其余取新路径。
    pub path: String,
    /// 重命名 / 复制前的路径。
    pub original_path: Option<String>,
    pub old_oid: Option<String>,
    pub new_oid: Option<String>,
    pub old_mode: Option<String>,
    pub new_mode: Option<String>,
    pub added: u32,
    pub deleted: u32,
    pub binary: bool,
    pub symlink: bool,
    pub submodule: bool,
    /// 只有权限变化（内容对象相同）。
    pub mode_only: bool,
    /// 路径含非 UTF-8 字节，`path` 只是有损展示名。
    pub path_lossy: bool,
    pub openable: bool,
    pub readonly_reason: Option<HistoryReadonlyReason>,
    /// 可打开时用于请求 diff 的身份；不可打开为 null。
    pub diff_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HistoryCommitFiles {
    pub target_id: String,
    pub commit: String,
    /// 调用者显式指定的父提交；根提交为 null。
    pub parent: Option<String>,
    /// 实际用于比较的基线对象：父提交或空 tree。
    pub base: String,
    /// 是否用了空 tree（根提交）。
    pub empty_tree: bool,
    pub files: Vec<HistoryFile>,
    /// 文件数超过限额时截断，并在这里如实说明。
    pub truncated: bool,
}

/// merge 提交的改动列表怎么分组（L05、Q147）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum HistoryMergeMode {
    /// 默认：只列相对每个父提交都有差异的文件（与 `git diff-tree -c` 的文件集合一致）。
    Combined,
    /// 打开「分别显示相对每个父提交的改动」后：按父提交分组列出全部改动。
    ByParent,
}

/// 一组改动：按父提交分组时 `parent` 是真实父提交；默认合并视图为 None。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HistoryMergeFileGroup {
    /// 这一组的父提交 OID；合并视图（相对所有父提交都有差异）为 None。
    pub parent: Option<String>,
    /// 这一组用于打开提交 diff 的比较父提交（合并视图取第一个父提交）。
    pub diff_parent: Option<String>,
    pub files: Vec<HistoryFile>,
}

/// merge 提交的改动文件。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HistoryMergeFiles {
    pub target_id: String,
    pub commit: String,
    /// 真实父提交列表（不改写、不截断）。
    pub parents: Vec<String>,
    pub mode: HistoryMergeMode,
    pub groups: Vec<HistoryMergeFileGroup>,
    pub truncated: bool,
}

/// 三栏只读视图的一栏内容。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HistoryMergeText {
    /// 换行统一为 `\n`。
    pub text: String,
    /// 该版本里没有这个路径（与空文件不同）。
    pub missing: bool,
    pub binary: bool,
    /// 解码有损（非 UTF-8 内容）。
    pub lossy: bool,
}

/// 三栏只读视图中的一侧（父提交）。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HistoryMergeSide {
    pub parent: String,
    pub text: HistoryMergeText,
}

/// merge 提交的三栏只读内容：父提交 1 | 合并结果 | 父提交 2（L05）。
///
/// 只读：没有写入目标、没有块操作；超过两个父提交时明确说明，由按父提交分组显示。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HistoryMergeView {
    pub target_id: String,
    pub commit: String,
    pub path: String,
    pub parents: Vec<String>,
    pub sides: Vec<HistoryMergeSide>,
    pub result: HistoryMergeText,
    /// 父提交不是两个（超过两个父提交或未指认父提交）时不能显示三栏。
    pub unavailable: Option<String>,
}

/// 历史 diff 的固定目标身份：由改动文件列表签发，请求时原样带回。
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HistoryFileTarget {
    pub commit: String,
    pub parent: Option<String>,
    pub path: String,
    pub original_path: Option<String>,
    pub old_oid: Option<String>,
    pub new_oid: Option<String>,
    pub status: HistoryFileStatus,
    /// 后端签发的 diff 身份；与重新校验的结果不一致即拒绝。
    pub diff_id: String,
}

/// 历史里的一个显示行：与工作区的 `RenderRow` 相同排版，但不含任何写操作目标。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HistoryRow {
    pub kind: rows::RowKind,
    pub hunk_index: u32,
    pub section: Option<String>,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    pub line_kind: Option<LineKind>,
    pub text: Option<String>,
    pub left: Option<rows::RowCell>,
    pub right: Option<rows::RowCell>,
    pub modified: bool,
    pub segment_index: Option<u32>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HistoryRowSlice {
    pub target_id: String,
    pub diff_id: String,
    pub view: ViewMode,
    pub from: u32,
    pub row_count: u32,
    pub rows: Vec<HistoryRow>,
    pub binary: bool,
    pub mode_only: bool,
    pub empty_reason: Option<HistoryEmptyReason>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HistorySideLayout {
    pub target_id: String,
    pub diff_id: String,
    pub left_line_count: u32,
    pub right_line_count: u32,
    /// 改动段：与工作区左右对照同一份数据模型，但**没有**段操作目标。
    pub segments: Vec<ChangeSegment>,
    pub folds: Vec<FoldRange>,
    pub ignore_whitespace: IgnoreWhitespace,
    pub binary: bool,
    /// 该侧对象不存在（新增文件的旧侧、删除文件的新侧）；与空文件不同。
    pub left_missing: bool,
    pub right_missing: bool,
    pub left_lossy: bool,
    pub right_lossy: bool,
    pub symlink: bool,
    pub mode_only: bool,
    pub empty_reason: Option<HistoryEmptyReason>,
}

/// 一份已解析并缓存的提交文件 diff。
struct CachedDiff {
    diff_id: String,
    model: crate::git::diff::FileChange,
    binary: bool,
    mode_only: bool,
    symlink: bool,
    old_oid: Option<String>,
    new_oid: Option<String>,
    old_missing: bool,
    new_missing: bool,
    texts: Option<Arc<Texts>>,
}

struct Texts {
    old: String,
    new: String,
    old_lossy: bool,
    new_lossy: bool,
}

struct RowLayoutCache {
    layout: rows::RowLayout,
}

struct SideCache {
    segments: Vec<ChangeSegment>,
    folds: Vec<FoldRange>,
}

struct QueryState {
    id: String,
    /// 查询开始时固定的起点集合（引用移动后仍读这些对象）。
    starts: Vec<String>,
    first_parent: bool,
    next_skip: u32,
    refs: Vec<HistoryRef>,
    /// 筛选条件的规范形式：续页必须带同一组筛选，否则分页就没有意义。
    filter_key: String,
    /// 「文字或哈希前缀」筛选（关闭正则时）需要 Rust 逐批匹配，命中结果与已扫描进度缓存
    /// 在这里；上限同样受 `MAX_QUERY_SPAN` 约束。
    scan: Option<ScanState>,
}

/// 扫描式文本筛选的进度：命中的提交（已带引用标签）与已扫描的提交数。
struct ScanState {
    matched: Vec<HistoryCommit>,
    scanned: u32,
    exhausted: bool,
}

struct TargetInner {
    closed: bool,
    queries: Vec<QueryState>,
    query_counter: u64,
    content: HashMap<String, CachedDiff>,
    layouts: HashMap<String, RowLayoutCache>,
    sides: HashMap<String, SideCache>,
}

pub struct HistoryTarget {
    pub id: String,
    pub worktree_id: String,
    pub session_instance: u64,
    git: Git,
    highlighter: Highlighter,
    inner: Mutex<TargetInner>,
}

impl HistoryTarget {
    fn with_inner<T>(&self, f: impl FnOnce(&mut TargetInner) -> Result<T>) -> Result<T> {
        let mut inner = self.inner.lock().map_err(|_| AppError::internal("历史目标锁失败"))?;
        if inner.closed {
            return Err(AppError::rejected("历史目标已关闭，请重新打开提交历史"));
        }
        f(&mut inner)
    }

    /// 当前缓存的 diff 份数（回归测试检查有界性用）。
    pub fn cache_len(&self) -> usize {
        self.inner.lock().map(|inner| inner.content.len()).unwrap_or(0)
    }

    pub fn layout_cache_len(&self) -> usize {
        self.inner.lock().map(|inner| inner.layouts.len()).unwrap_or(0)
    }

    /// 重新读取本地引用（新查询与标签使用最新引用）。
    pub fn refs(&self) -> Result<Vec<HistoryRef>> {
        read_refs(&self.git)
    }

    // ---------- H01：分页读取 ----------

    /// 不带筛选的分页读取（任务 01 的契约，M5 首批测试继续使用）。
    pub fn log_page(
        &self,
        start: Option<HistoryStart>,
        first_parent: bool,
        limit: u32,
        cursor: Option<String>,
    ) -> Result<HistoryPage> {
        self.log_page_filtered(start, first_parent, limit, cursor, &HistoryFilter::default())
    }

    /// 带筛选的分页读取（M5 第二批 L04）。
    ///
    /// 起点与筛选在查询开始时固定：续页必须来自同一目标、同一遍历方式、同一筛选，
    /// 否则明确拒绝。引用在两页之间移动时，仍读查询开始时解析出的完整提交 ID。
    pub fn log_page_filtered(
        &self,
        start: Option<HistoryStart>,
        first_parent: bool,
        limit: u32,
        cursor: Option<String>,
        filter: &HistoryFilter,
    ) -> Result<HistoryPage> {
        if limit == 0 {
            return Err(AppError::rejected("limit 必须在 1 到 200 之间"));
        }
        if limit > gh::MAX_PAGE_LIMIT {
            return Err(AppError::rejected(format!(
                "limit 不能超过 {}",
                gh::MAX_PAGE_LIMIT
            )));
        }
        let resolved = self.resolve_filter(filter)?;
        let filter_key = resolved.key.clone();
        self.with_inner(|inner| {
            let (query_id, starts, skip, refs) = match cursor {
                Some(cursor) => {
                    let (cursor_target, query_id, skip) = parse_cursor(&cursor)?;
                    // 查询编号在每个目标内从 `q1` 起，光看 `qN:skip` 会让另一个目标的
                    // 同名查询被当成合法续页；游标必须带上签发它的目标。
                    if cursor_target != self.id {
                        return Err(AppError::rejected("分页游标属于另一个历史目标，请重新开始查询"));
                    }
                    let query = inner
                        .queries
                        .iter()
                        .find(|query| query.id == query_id)
                        .ok_or_else(|| AppError::rejected("分页游标已过期，请重新开始查询"))?;
                    if query.next_skip != skip {
                        return Err(AppError::rejected("分页游标与查询进度不一致，请重新开始查询"));
                    }
                    if query.first_parent != first_parent {
                        return Err(AppError::rejected("分页游标与本次查询的遍历方式不一致"));
                    }
                    if query.filter_key != filter_key {
                        return Err(AppError::rejected("分页游标与本次筛选条件不一致，请重新开始查询"));
                    }
                    (
                        query.id.clone(),
                        query.starts.clone(),
                        query.next_skip,
                        query.refs.clone(),
                    )
                }
                None => {
                    let start = start.ok_or_else(|| AppError::rejected("首次查询必须指定起点"))?;
                    let starts = self.resolve_starts(&start, filter)?;
                    let refs = read_refs(&self.git)?;
                    inner.query_counter += 1;
                    let id = format!("q{}", inner.query_counter);
                    inner.queries.push(QueryState {
                        id: id.clone(),
                        starts: starts.clone(),
                        first_parent,
                        next_skip: 0,
                        refs: refs.clone(),
                        filter_key: filter_key.clone(),
                        scan: resolved.text_match.as_ref().map(|_| ScanState {
                            matched: Vec::new(),
                            scanned: 0,
                            exhausted: false,
                        }),
                    });
                    while inner.queries.len() > MAX_OPEN_QUERIES {
                        inner.queries.remove(0);
                    }
                    (id, starts, 0, refs)
                }
            };

            let end = skip.saturating_add(limit);
            if end > gh::MAX_QUERY_SPAN {
                return Err(AppError::rejected(format!(
                    "历史分页超过本次查询上限（{} 条），请从更近的起点重新查询",
                    gh::MAX_QUERY_SPAN
                )));
            }
            let (page_commits, exhausted) = match &resolved.text_match {
                Some(match_mode) => self.scan_page(inner, &query_id, &starts, first_parent, skip, limit, &resolved, match_mode)?,
                None => {
                    let commits = gh::read_page_filtered(
                        &self.git,
                        &starts,
                        first_parent,
                        skip,
                        limit,
                        &resolved.git_filter,
                    )?;
                    let page: Vec<HistoryCommit> = commits
                        .iter()
                        .map(|commit| describe_commit(commit, &refs))
                        .collect();
                    let exhausted = page.len() < limit as usize;
                    (page, exhausted)
                }
            };
            let next_skip = skip + page_commits.len() as u32;
            if let Some(query) = inner.queries.iter_mut().find(|query| query.id == query_id) {
                query.next_skip = next_skip;
            }
            let next_cursor = (!exhausted).then(|| format!("{}/{query_id}:{next_skip}", self.id));
            let start_label = starts.first().cloned().unwrap_or_default();
            Ok(HistoryPage {
                target_id: self.id.clone(),
                query_id,
                start: start_label,
                first_parent,
                commit_count: page_commits.len() as u32,
                next_cursor,
                exhausted,
                commits: page_commits,
            })
        })
    }

    /// 扫描式文本筛选（文字或哈希前缀）：由 Rust 逐批读取固定范围并匹配。
    ///
    /// 命中结果缓存在查询状态里，续页只是切片；扫描进度受 `MAX_QUERY_SPAN` 限制，
    /// 超过时明确报错，不伪装成历史末尾。
    fn scan_page(
        &self,
        inner: &mut TargetInner,
        query_id: &str,
        starts: &[String],
        first_parent: bool,
        skip: u32,
        limit: u32,
        resolved: &ResolvedFilter,
        match_mode: &TextMatch,
    ) -> Result<(Vec<HistoryCommit>, bool)> {
        const BATCH: u32 = 200;
        loop {
            let (have, scanned, exhausted) = {
                let query = inner
                    .queries
                    .iter()
                    .find(|query| query.id == query_id)
                    .ok_or_else(|| AppError::rejected("分页游标已过期，请重新开始查询"))?;
                let scan = query
                    .scan
                    .as_ref()
                    .ok_or_else(|| AppError::internal("扫描式查询缺少进度"))?;
                (scan.matched.len() as u32, scan.scanned, scan.exhausted)
            };
            if skip.saturating_add(limit) <= have || exhausted {
                let query = inner
                    .queries
                    .iter()
                    .find(|query| query.id == query_id)
                    .expect("查询已在上面的分支里确认存在");
                let scan = query.scan.as_ref().expect("扫描式查询缺少进度");
                let from = skip as usize;
                let to = (from + limit as usize).min(scan.matched.len());
                let page = if from < to { scan.matched[from..to].to_vec() } else { Vec::new() };
                let ended = exhausted && to == scan.matched.len();
                return Ok((page, ended));
            }
            if scanned >= gh::MAX_QUERY_SPAN {
                return Err(AppError::rejected(format!(
                    "已扫描 {} 条提交仍未取满本次查询上限（{} 条），请从更近的起点或更具体的筛选重新查询",
                    scanned,
                    gh::MAX_QUERY_SPAN
                )));
            }
            let batch = gh::read_page_scanned(
                &self.git,
                starts,
                first_parent,
                scanned,
                BATCH.min(gh::MAX_QUERY_SPAN - scanned),
                &resolved.git_filter,
            )?;
            let batch_len = batch.len() as u32;
            let page_refs = inner
                .queries
                .iter()
                .find(|query| query.id == query_id)
                .map(|query| query.refs.clone())
                .unwrap_or_default();
            let query = inner
                .queries
                .iter_mut()
                .find(|query| query.id == query_id)
                .ok_or_else(|| AppError::rejected("分页游标已过期，请重新开始查询"))?;
            let scan = query
                .scan
                .as_mut()
                .ok_or_else(|| AppError::internal("扫描式查询缺少进度"))?;
            for item in &batch {
                if match_mode.matches(item) {
                    scan.matched.push(describe_commit(&item.entry, &page_refs));
                }
            }
            scan.scanned += batch_len;
            if batch_len == 0 {
                scan.exhausted = true;
            }
        }
    }

    /// 解析查询起点为完整提交 ID 集合；不接受任意 rev 表达式。
    ///
    /// 分支筛选（L04）是起点集合的唯一来源：选了具体分支 / 标签就只走这些引用，
    /// 没选就是「全部」（所有本地分支、远端分支、标签与 HEAD）。`start` 仍需给出，
    /// 作为 H01 的显式锚点，并在「全部」时确认调用者的意图。
    fn resolve_starts(&self, start: &HistoryStart, filter: &HistoryFilter) -> Result<Vec<String>> {
        if !filter.branches.is_empty() {
            return self.resolve_starts(
                &HistoryStart::Refs { full_names: filter.branches.clone() },
                &HistoryFilter::default(),
            );
        }
        match start {
            HistoryStart::Ref { full_name } => {
                if full_name.is_empty() {
                    return Err(AppError::rejected("引用名不能为空"));
                }
                let refs = read_refs(&self.git)?;
                refs.iter()
                    .find(|item| &item.full_name == full_name)
                    .map(|item| vec![item.commit.clone()])
                    .ok_or_else(|| AppError::rejected(format!("引用不存在或不是提交：{full_name}")))
            }
            HistoryStart::Commit { oid } => Ok(vec![gh::verify_commit_oid(&self.git, oid)?]),
            HistoryStart::Head => Ok(vec![gh::head_commit(&self.git)?
                .ok_or_else(|| AppError::rejected("仓库还没有提交，无法读取历史"))?]),
            HistoryStart::Refs { full_names } => {
                let refs = read_refs(&self.git)?;
                let mut starts: Vec<String> = Vec::new();
                if full_names.is_empty() {
                    // 「全部」：所有本地分支、远端分支、标签与 HEAD 可达的提交。
                    for item in &refs {
                        if !starts.contains(&item.commit) {
                            starts.push(item.commit.clone());
                        }
                    }
                    if let Some(head) = gh::head_commit(&self.git)? {
                        if !starts.contains(&head) {
                            starts.push(head);
                        }
                    }
                    if starts.is_empty() {
                        return Err(AppError::rejected("仓库还没有提交，无法读取历史"));
                    }
                    return Ok(starts);
                }
                for name in full_names {
                    let item = refs
                        .iter()
                        .find(|item| &item.full_name == name)
                        .ok_or_else(|| AppError::rejected(format!("引用不存在或不是提交：{name}")))?;
                    if !starts.contains(&item.commit) {
                        starts.push(item.commit.clone());
                    }
                }
                Ok(starts)
            }
        }
    }

    /// 解析并校验筛选条件：作者、路径、日期、文本。取值都在这里定型，
    /// 前端传什么都不会变成任意 Git 参数。
    fn resolve_filter(&self, filter: &HistoryFilter) -> Result<ResolvedFilter> {
        let mut git_filter = gh::LogFilter::default();
        let mut key = String::new();
        let text = filter.text.as_deref().unwrap_or("").trim().to_string();
        let use_regex = filter.regex;
        // 正则模式交给 Git 的 `--grep -E`；关闭正则时由 Rust 逐批匹配提交信息或哈希前缀。
        let mut text_match = None;
        if !text.is_empty() {
            if use_regex {
                git_filter.grep = Some(gh::GrepFilter {
                    pattern: text.clone(),
                    case_sensitive: filter.case_sensitive,
                    regex: true,
                });
                key.push_str(&format!("re:{:?}:{text};", filter.case_sensitive));
            } else {
                text_match = Some(TextMatch {
                    pattern: text.clone(),
                    case_sensitive: filter.case_sensitive,
                    hash_prefix: looks_like_hash(&text),
                });
                key.push_str(&format!("tx:{:?}:{text};", filter.case_sensitive));
            }
        }
        // 只有实际生成了 ERE 文本筛选时才转义作者；文字为空时，即使界面勾选
        // 正则，`log_args` 仍用 `--fixed-strings`，必须传未经转义的字面值。
        let author_uses_regex = git_filter.grep.as_ref().is_some_and(|grep| grep.regex);
        for author in &filter.authors {
            let value = author.trim();
            if value.is_empty() {
                continue;
            }
            let pattern = if author_uses_regex { regex_escape(value) } else { value.to_string() };
            git_filter.authors.push(pattern);
            key.push_str(&format!("au:{value};"));
        }
        for path in &filter.paths {
            let value = path.trim();
            if value.is_empty() {
                return Err(AppError::rejected("路径筛选不能包含空路径"));
            }
            // 路径按字面匹配（`--literal-pathspecs`），并由 `--` 与选项隔开，
            // 因此以 `-` 开头或含 `*`、`[`、`:()` 的路径都不会被当成选项或魔改模式。
            if value.contains('\n') {
                return Err(AppError::rejected(format!("无法按字面匹配含换行的路径：{value}")));
            }
            git_filter.paths.push(value.to_string());
            key.push_str(&format!("pa:{value};"));
        }
        for branch in &filter.branches {
            if branch.trim().is_empty() {
                return Err(AppError::rejected("分支筛选不能包含空引用名"));
            }
            key.push_str(&format!("br:{branch};"));
        }
        if let Some(date) = &filter.date {
            match date {
                HistoryDateFilter::LastDays { days } => {
                    if *days == 0 || *days > 3650 {
                        return Err(AppError::rejected("最近天数必须在 1 到 3650 之间"));
                    }
                    let seconds = now_unix().saturating_sub(i64::from(*days) * 86_400);
                    git_filter.since = Some(seconds.to_string());
                    key.push_str(&format!("dt:last:{days};"));
                }
                HistoryDateFilter::Range { since, until } => {
                    validate_date(since)?;
                    validate_date(until)?;
                    if since > until {
                        return Err(AppError::rejected("起始日期不能晚于结束日期"));
                    }
                    git_filter.since = Some(format!("{since} 00:00:00"));
                    git_filter.until = Some(format!("{until} 23:59:59"));
                    key.push_str(&format!("dt:range:{since}:{until};"));
                }
            }
        }
        Ok(ResolvedFilter { git_filter, text_match, key })
    }

    // ---------- H02：详情与改动文件 ----------

    pub fn commit_detail(&self, oid: &str) -> Result<HistoryCommitDetail> {
        let oid = gh::verify_commit_oid(&self.git, oid)?;
        let detail = gh::read_commit(&self.git, &oid)?;
        Ok(HistoryCommitDetail {
            target_id: self.id.clone(),
            oid: detail.oid,
            tree: detail.tree,
            parents: detail.parents,
            author: signature(&detail.author),
            committer: signature(&detail.committer),
            message: detail.message,
            message_encoding: detail.encoding,
            message_lossy: detail.message_lossy,
        })
    }

    pub fn commit_files(&self, commit: &str, parent: Option<&str>) -> Result<HistoryCommitFiles> {
        let commit = gh::verify_commit_oid(&self.git, commit)?;
        let detail = gh::read_commit(&self.git, &commit)?;
        let parent = match parent {
            Some(parent) => Some(gh::verify_commit_oid(&self.git, parent)?),
            None => None,
        };
        let (base, empty_tree) = self.resolve_base(&detail.parents, parent.as_deref())?;
        let changes = gh::read_changes(&self.git, &base, &commit)?;
        let truncated = changes.len() > MAX_COMMIT_FILES;
        let mut files = Vec::new();
        for change in changes.iter().take(MAX_COMMIT_FILES) {
            files.push(self.describe_change(&commit, parent.as_deref(), change));
        }
        Ok(HistoryCommitFiles {
            target_id: self.id.clone(),
            commit,
            parent,
            base,
            empty_tree,
            files,
            truncated,
        })
    }

    /// merge 提交的改动文件（L05、Q147）。
    ///
    /// 默认（`Combined`）只列相对**每个**父提交都有差异的文件，文件集合与
    /// `git diff-tree -c` 一致；超过两个父提交时改为按父提交分组（`mode` 如实返回）。
    /// `ByParent` 按父提交分组列出全部改动。
    pub fn merge_files(&self, commit: &str, mode: HistoryMergeMode) -> Result<HistoryMergeFiles> {
        let commit = gh::verify_commit_oid(&self.git, commit)?;
        let detail = gh::read_commit(&self.git, &commit)?;
        if detail.parents.len() < 2 {
            return Err(AppError::rejected("这个提交不是 merge 提交，没有多个父提交"));
        }
        let mut per_parent: Vec<Vec<RawChange>> = Vec::new();
        for parent in &detail.parents {
            let changes = gh::read_changes(&self.git, parent, &commit)?;
            per_parent.push(changes);
        }
        let grouped = mode == HistoryMergeMode::ByParent || detail.parents.len() > 2;
        if !grouped {
            let first_parent = detail.parents[0].clone();
            let mut files = Vec::new();
            for change in &per_parent[0] {
                let path = change.primary_path();
                if per_parent[1..]
                    .iter()
                    .all(|changes| changes.iter().any(|other| other.primary_path() == path))
                {
                    files.push(self.describe_change(&commit, Some(&first_parent), change));
                }
            }
            let truncated = files.len() > MAX_COMMIT_FILES;
            files.truncate(MAX_COMMIT_FILES);
            return Ok(HistoryMergeFiles {
                target_id: self.id.clone(),
                commit,
                parents: detail.parents,
                mode: HistoryMergeMode::Combined,
                groups: vec![HistoryMergeFileGroup {
                    parent: None,
                    diff_parent: Some(first_parent),
                    files,
                }],
                truncated,
            });
        }
        let mut groups = Vec::new();
        let mut truncated = false;
        for (index, changes) in per_parent.iter().enumerate() {
            let parent = detail.parents[index].clone();
            let mut files = Vec::new();
            for change in changes.iter().take(MAX_COMMIT_FILES) {
                files.push(self.describe_change(&commit, Some(&parent), change));
            }
            if changes.len() > MAX_COMMIT_FILES {
                truncated = true;
            }
            groups.push(HistoryMergeFileGroup {
                parent: Some(parent.clone()),
                diff_parent: Some(parent),
                files,
            });
        }
        Ok(HistoryMergeFiles {
            target_id: self.id.clone(),
            commit,
            parents: detail.parents,
            mode: HistoryMergeMode::ByParent,
            groups,
            truncated,
        })
    }

    /// merge 提交的三栏只读内容：父提交 1 | 合并结果 | 父提交 2（L05）。
    ///
    /// 内容按对象 ID 从 Git 读取（不读工作区），路径必须属于该提交的改动文件。
    pub fn merge_view(&self, commit: &str, path: &str) -> Result<HistoryMergeView> {
        let commit = gh::verify_commit_oid(&self.git, commit)?;
        let detail = gh::read_commit(&self.git, &commit)?;
        if path.is_empty() {
            return Err(AppError::rejected("路径不能为空"));
        }
        // 路径身份：必须来自该提交相对某个父提交的改动文件。
        let mut known = false;
        for parent in &detail.parents {
            let changes = gh::read_changes(&self.git, parent, &commit)?;
            if changes.iter().any(|change| {
                std::str::from_utf8(change.primary_path()).ok() == Some(path)
                    || change
                        .original_path()
                        .and_then(|value| std::str::from_utf8(value).ok())
                        == Some(path)
            }) {
                known = true;
                break;
            }
        }
        if !known {
            return Err(AppError::rejected(format!(
                "该提交的这个文件没有改动，不能作为三栏比较目标：{path}"
            )));
        }
        let unavailable = (detail.parents.len() != 2)
            .then(|| format!("该提交有 {} 个父提交，三栏只读视图只支持两个父提交", detail.parents.len()));
        let result = self.read_tree_text(&commit, path)?;
        let mut sides = Vec::new();
        if unavailable.is_none() {
            for parent in &detail.parents {
                sides.push(HistoryMergeSide {
                    parent: parent.clone(),
                    text: self.read_tree_text(parent, path)?,
                });
            }
        }
        Ok(HistoryMergeView {
            target_id: self.id.clone(),
            commit,
            path: path.to_string(),
            parents: detail.parents,
            sides,
            result,
            unavailable,
        })
    }

    /// 读取某个提交树里某路径的文本内容（只读、按对象 ID）。
    fn read_tree_text(&self, commit: &str, path: &str) -> Result<HistoryMergeText> {
        let Some(oid) = gh::object_at_path(&self.git, commit, path)? else {
            return Ok(HistoryMergeText {
                text: String::new(),
                missing: true,
                binary: false,
                lossy: false,
            });
        };
        match gh::object_type(&self.git, &oid)?.as_deref() {
            Some("blob") => {}
            Some(other) => {
                return Ok(HistoryMergeText {
                    text: format!("（{other}，不能在文本视图中显示）"),
                    missing: false,
                    binary: true,
                    lossy: false,
                });
            }
            None => {
                return Err(AppError::git(format!("缺少对象：{oid}")));
            }
        }
        let bytes = gh::blob(&self.git, &oid)?;
        if bytes.len() as u64 > gh::MAX_BLOB_BYTES {
            return Ok(HistoryMergeText {
                text: format!("（文件超过 {} MB，不能在三栏视图中显示）", gh::MAX_BLOB_BYTES / (1024 * 1024)),
                missing: false,
                binary: true,
                lossy: false,
            });
        }
        if gh::looks_binary(&bytes) {
            return Ok(HistoryMergeText {
                text: String::new(),
                missing: false,
                binary: true,
                lossy: false,
            });
        }
        let lossy = std::str::from_utf8(&bytes).is_err();
        let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
        Ok(HistoryMergeText { text, missing: false, binary: false, lossy })
    }

    /// 比较基线：显式父提交必须是该提交的真实父提交；根提交明确使用空 tree。
    fn resolve_base(&self, parents: &[String], parent: Option<&str>) -> Result<(String, bool)> {
        match parent {
            Some(parent) => {
                if !parents.iter().any(|candidate| candidate == parent) {
                    return Err(AppError::rejected(format!(
                        "指定的父提交不是该提交的父提交：{parent}"
                    )));
                }
                Ok((parent.to_string(), false))
            }
            None => {
                if !parents.is_empty() {
                    return Err(AppError::rejected(
                        "该提交有父提交，读取改动文件时必须显式指定一个父提交",
                    ));
                }
                Ok((gh::empty_tree(&self.git)?, true))
            }
        }
    }

    fn describe_change(
        &self,
        commit: &str,
        parent: Option<&str>,
        change: &RawChange,
    ) -> HistoryFile {
        // 重命名 / 复制的主路径与原路径任一不是合法 UTF-8，都算不可无损打开：
        // 否则会用有损的展示名签发 diffId，之后打开时身份校验必然拒绝。
        let path = display_path(change.primary_path());
        let original_path = change.original_path().map(display_path);
        let path_lossy = std::str::from_utf8(change.primary_path()).is_err()
            || change
                .original_path()
                .is_some_and(|original| std::str::from_utf8(original).is_err());
        let symlink = change.is_symlink();
        let submodule = change.is_submodule();
        let mode_only = change.mode_only();
        let readonly_reason = if path_lossy {
            Some(HistoryReadonlyReason::NonUtf8Path)
        } else if submodule {
            Some(HistoryReadonlyReason::Submodule)
        } else {
            None
        };
        let openable = readonly_reason.is_none();
        let diff_id = openable.then(|| {
            diff_identity(
                &self.id,
                commit,
                parent,
                HistoryFileStatus::from_change(change.status),
                &path,
                original_path.as_deref(),
                change.old_oid.as_deref(),
                change.new_oid.as_deref(),
            )
        });
        HistoryFile {
            status: HistoryFileStatus::from_change(change.status),
            path,
            original_path,
            old_oid: change.old_oid.clone(),
            new_oid: change.new_oid.clone(),
            old_mode: change.old_mode.clone(),
            new_mode: change.new_mode.clone(),
            added: change.added,
            deleted: change.deleted,
            binary: change.binary,
            symlink,
            submodule,
            mode_only,
            path_lossy,
            openable,
            readonly_reason,
            diff_id,
        }
    }

    // ---------- H03：提交 diff ----------

    pub fn file_rows(
        &self,
        file: &HistoryFileTarget,
        view: ViewMode,
        ignore: IgnoreWhitespace,
        from: u32,
        to: u32,
    ) -> Result<HistoryRowSlice> {
        self.with_inner(|inner| {
            let diff_id = self.ensure_diff(inner, file, ignore)?;
            let cached = inner
                .content
                .get(&diff_key(&diff_id, ignore))
                .ok_or_else(|| AppError::internal("提交 diff 缓存丢失"))?;
            let key = format!("{diff_id}/{}/{view:?}", ignore.as_str());
            if !inner.layouts.contains_key(&key) {
                if inner.layouts.len() >= MAX_CACHED_LAYOUTS {
                    inner.layouts.clear();
                }
                inner.layouts.insert(
                    key.clone(),
                    RowLayoutCache {
                        layout: rows::RowLayout::new(&cached.model, view),
                    },
                );
            }
            let layout = inner
                .layouts
                .get(&key)
                .ok_or_else(|| AppError::internal("提交 diff 排版缓存丢失"))?;
            let row_count = layout.layout.len() as u32;
            let start = from.min(row_count);
            let end = to.min(row_count).max(start);
            let rendered = layout.layout.render_range(&cached.model, start as usize..end as usize);
            let empty_reason = empty_reason(cached);
            Ok(HistoryRowSlice {
                target_id: self.id.clone(),
                diff_id,
                view,
                from: start,
                row_count,
                rows: rendered.iter().map(history_row).collect(),
                binary: cached.binary,
                mode_only: cached.mode_only,
                empty_reason,
            })
        })
    }

    pub fn file_side_layout(
        &self,
        file: &HistoryFileTarget,
        ignore: IgnoreWhitespace,
        context_lines: u32,
    ) -> Result<HistorySideLayout> {
        self.with_inner(|inner| {
            let diff_id = self.ensure_diff(inner, file, ignore)?;
            // 先判断二进制再读文本：二进制没有文本两侧，不能先走 ensure_texts 而报错，
            // 否则永远走不到下面的 `binary = true` 分支。
            let (binary, mode_only, symlink, left_missing, right_missing, empty_reason) = {
                let cached = inner
                    .content
                    .get(&diff_key(&diff_id, ignore))
                    .ok_or_else(|| AppError::internal("提交 diff 缓存丢失"))?;
                (
                    cached.binary,
                    cached.mode_only,
                    cached.symlink,
                    cached.old_missing,
                    cached.new_missing,
                    empty_reason(cached),
                )
            };
            if binary {
                return Ok(HistorySideLayout {
                    target_id: self.id.clone(),
                    diff_id,
                    left_line_count: 0,
                    right_line_count: 0,
                    segments: Vec::new(),
                    folds: Vec::new(),
                    ignore_whitespace: ignore,
                    binary: true,
                    left_missing,
                    right_missing,
                    left_lossy: false,
                    right_lossy: false,
                    symlink,
                    mode_only,
                    empty_reason,
                });
            }
            let texts = self.ensure_texts(inner, &diff_id)?;
            let left_count = segments::text_line_count(&texts.old);
            let right_count = segments::text_line_count(&texts.new);
            if left_count > MAX_SIDE_LINES || right_count > MAX_SIDE_LINES {
                return Err(AppError::rejected(format!(
                    "文件超过历史左右对照上限（{MAX_SIDE_LINES} 行）"
                )));
            }
            let cached = inner
                .content
                .get(&diff_key(&diff_id, ignore))
                .ok_or_else(|| AppError::internal("提交 diff 缓存丢失"))?;
            let side_key = format!("{diff_id}/{}/{context_lines}", ignore.as_str());
            if !inner.sides.contains_key(&side_key) {
                if !segments::texts_match_diff(&cached.model, &texts.old, &texts.new) {
                    return Err(AppError::internal(
                        "提交内容与 Git 给出的差异不一致，已拒绝显示",
                    ));
                }
                let mut all: Vec<ChangeSegment> = Vec::new();
                for (index, hunk) in cached.model.hunks.iter().enumerate() {
                    all.extend(segments::segments_of_hunk(hunk, index as u32));
                }
                let folds = segments::compute_folds(&cached.model, left_count, right_count, context_lines);
                if inner.sides.len() >= MAX_CACHED_SIDES {
                    inner.sides.clear();
                }
                inner.sides.insert(side_key.clone(), SideCache { segments: all, folds });
            }
            let side = inner
                .sides
                .get(&side_key)
                .ok_or_else(|| AppError::internal("提交左右对照缓存丢失"))?;
            Ok(HistorySideLayout {
                target_id: self.id.clone(),
                diff_id,
                left_line_count: left_count,
                right_line_count: right_count,
                segments: side.segments.clone(),
                folds: side.folds.clone(),
                ignore_whitespace: ignore,
                binary: false,
                left_missing: cached.old_missing,
                right_missing: cached.new_missing,
                left_lossy: texts.old_lossy,
                right_lossy: texts.new_lossy,
                symlink: cached.symlink,
                mode_only: cached.mode_only,
                empty_reason,
            })
        })
    }

    pub fn file_side_lines(
        &self,
        file: &HistoryFileTarget,
        ignore: IgnoreWhitespace,
        context_lines: u32,
        version: FileVersion,
        from: u32,
        to: u32,
        granularity: segments::WordGranularity,
    ) -> Result<Vec<segments::SideLine>> {
        self.with_inner(|inner| {
            let diff_id = self.ensure_diff(inner, file, ignore)?;
            let texts = self.ensure_texts(inner, &diff_id)?;
            let cached = inner
                .content
                .get(&diff_key(&diff_id, ignore))
                .ok_or_else(|| AppError::internal("提交 diff 缓存丢失"))?;
            if cached.binary {
                return Err(AppError::rejected("二进制文件没有可显示的历史文本行"));
            }
            let side_key = format!("{diff_id}/{}/{context_lines}", ignore.as_str());
            let segment_list = match inner.sides.get(&side_key) {
                Some(side) => side.segments.clone(),
                None => {
                    let mut all: Vec<ChangeSegment> = Vec::new();
                    for (index, hunk) in cached.model.hunks.iter().enumerate() {
                        all.extend(segments::segments_of_hunk(hunk, index as u32));
                    }
                    all
                }
            };
            let left_lines: Vec<&str> = segments::display_lines(&texts.old).collect();
            let right_lines: Vec<&str> = segments::display_lines(&texts.new).collect();
            let lines = match version {
                FileVersion::Old => &left_lines,
                FileVersion::New => &right_lines,
            };
            let from = from.max(1);
            let to = to.max(from);
            let mut changed_lines: std::collections::HashSet<u32> = std::collections::HashSet::new();
            let mut word_ranges: HashMap<u32, Vec<(u32, u32)>> = HashMap::new();
            for segment in &segment_list {
                let (start, end) = match version {
                    FileVersion::Old => (segment.left_start, segment.left_end),
                    FileVersion::New => (segment.right_start, segment.right_end),
                };
                for line in start + 1..=end {
                    changed_lines.insert(line);
                }
                if end < from || start >= to {
                    continue;
                }
                let (left_map, right_map) =
                    segments::segment_word_ranges(&left_lines, &right_lines, segment, granularity);
                word_ranges.extend(match version {
                    FileVersion::Old => left_map,
                    FileVersion::New => right_map,
                });
            }
            Ok(segments::side_lines_from_text(lines, from, to, &changed_lines, &word_ranges))
        })
    }

    pub fn file_highlight(
        &self,
        file: &HistoryFileTarget,
        ignore: IgnoreWhitespace,
        version: FileVersion,
        from_line: u32,
        to_line: u32,
    ) -> Result<HighlightResult> {
        let (diff_id, text) = self.with_inner(|inner| {
            let diff_id = self.ensure_diff(inner, file, ignore)?;
            let texts = self.ensure_texts(inner, &diff_id)?;
            let cached = inner
                .content
                .get(&diff_key(&diff_id, ignore))
                .ok_or_else(|| AppError::internal("提交 diff 缓存丢失"))?;
            if cached.binary {
                return Err(AppError::rejected("二进制文件不支持语法高亮"));
            }
            let text = match version {
                FileVersion::Old => texts.old.clone(),
                FileVersion::New => texts.new.clone(),
            };
            Ok((diff_id, text))
        })?;
        let from0 = from_line.saturating_sub(1) as usize;
        let to0 = to_line as usize;
        self.highlighter.highlight(HighlightRequest {
            document_id: format!("history/{}/{diff_id}/{version:?}", self.id),
            generation: stable_hash(&diff_id),
            language: session::language_for_path(&file.path),
            from_line: from0,
            to_line: to0,
            content: Some(text),
            revision: 0,
        })
    }

    /// 确保这份固定目标的 diff 已解析并缓存，返回后端计算的 diff 身份。
    fn ensure_diff(
        &self,
        inner: &mut TargetInner,
        file: &HistoryFileTarget,
        ignore: IgnoreWhitespace,
    ) -> Result<String> {
        // 先用请求里的身份字段重算 diff 身份，任何字段被改动都立即拒绝（不需要 Git 调用）。
        let recomputed = diff_identity(
            &self.id,
            &file.commit,
            file.parent.as_deref(),
            file.status,
            &file.path,
            file.original_path.as_deref(),
            file.old_oid.as_deref(),
            file.new_oid.as_deref(),
        );
        if recomputed != file.diff_id {
            return Err(AppError::rejected(
                "提交 diff 的目标身份与后端记录不一致，请重新打开该文件",
            ));
        }
        // 快速路径：同一个目标下已经解析过这份固定目标。`diff_id` 里含目标实例身份，
        // 因此别的目标 / 别的提交 / 别的父项 / 别的空白档位都不会命中。
        let cached_key = diff_key(&file.diff_id, ignore);
        if inner.content.contains_key(&cached_key) {
            return Ok(file.diff_id.clone());
        }
        let commit = gh::verify_commit_oid(&self.git, &file.commit)?;
        let parent = match &file.parent {
            Some(parent) => {
                let parent = gh::verify_commit_oid(&self.git, parent)?;
                Some(parent)
            }
            None => None,
        };
        let detail = gh::read_commit(&self.git, &commit)?;
        let (base, _) = self.resolve_base(&detail.parents, parent.as_deref())?;
        let change = self.verify_change(&base, &commit, file)?;
        let diff_id = diff_identity(
            &self.id,
            &commit,
            parent.as_deref(),
            HistoryFileStatus::from_change(change.status),
            &file.path,
            file.original_path.as_deref(),
            change.old_oid.as_deref(),
            change.new_oid.as_deref(),
        );
        if diff_id != file.diff_id {
            return Err(AppError::rejected(
                "提交 diff 的目标身份与后端记录不一致，请重新打开该文件",
            ));
        }
        let key = diff_key(&diff_id, ignore);
        if !inner.content.contains_key(&key) {
            let cached = self.read_diff(&commit, &base, &change, &diff_id, ignore)?;
            if inner.content.len() >= MAX_CACHED_DIFFS {
                inner.content.clear();
                inner.layouts.clear();
                inner.sides.clear();
            }
            inner.content.insert(key, cached);
        }
        Ok(diff_id)
    }

    /// 用 Git 的改动文件列表核对请求里的文件身份；不一致一律拒绝。
    fn verify_change(
        &self,
        base: &str,
        commit: &str,
        file: &HistoryFileTarget,
    ) -> Result<RawChange> {
        if let Some(oid) = &file.old_oid {
            if !gh::is_full_oid(oid) {
                return Err(AppError::rejected("旧对象 ID 不合法"));
            }
        }
        if let Some(oid) = &file.new_oid {
            if !gh::is_full_oid(oid) {
                return Err(AppError::rejected("新对象 ID 不合法"));
            }
        }
        let changes = gh::read_changes(&self.git, base, commit)?;
        let wanted_original = file.original_path.as_deref().map(str::as_bytes);
        changes
            .into_iter()
            .find(|change| {
                change.primary_path() == file.path.as_bytes()
                    && change.original_path() == wanted_original
                    && change.old_oid.as_deref() == file.old_oid.as_deref()
                    && change.new_oid.as_deref() == file.new_oid.as_deref()
                    && HistoryFileStatus::from_change(change.status) == file.status
            })
            .ok_or_else(|| {
                AppError::rejected(format!(
                    "文件不属于该提交与父提交的改动范围，或内容身份已变化：{}",
                    file.path
                ))
            })
    }

    fn read_diff(
        &self,
        commit: &str,
        base: &str,
        change: &RawChange,
        diff_id: &str,
        ignore: IgnoreWhitespace,
    ) -> Result<CachedDiff> {
        let primary = std::str::from_utf8(change.primary_path())
            .map_err(|_| AppError::rejected(HistoryReadonlyReason::NonUtf8Path.description()))?
            .to_string();
        let mut paths: Vec<&str> = Vec::new();
        if let Some(original) = change.original_path() {
            let original = std::str::from_utf8(original)
                .map_err(|_| AppError::rejected(HistoryReadonlyReason::NonUtf8Path.description()))?;
            paths.push(original);
        }
        paths.push(&primary);
        let patch = gh::file_patch(&self.git, base, commit, &paths, ignore)?;
        let parsed = crate::git::diff::parse_bytes(&patch);
        let model = select_patch_file(parsed.files, change)?;
        let binary = change.binary || model.binary;
        let mode_only = change.mode_only();
        let empty = model.hunks.is_empty();
        // 两侧都真实存在、内容对象不同、又不是二进制 / 仅权限变化，却没有任何 hunk：
        // 说明 Git 没有给出可显示的内容（不能当成空 diff 吞掉）。新增 / 删除文件本来
        // 只有一侧，空文件也不会产生 hunk，这些情况不在此列。
        let both_present = change.old_oid.is_some() && change.new_oid.is_some();
        if empty
            && both_present
            && !binary
            && !mode_only
            && change.old_oid != change.new_oid
            && !ignore_allows_empty(ignore)
        {
            return Err(AppError::git(format!(
                "Git 没有返回该文件的差异，但对象 ID 不同：{primary}"
            )));
        }
        Ok(CachedDiff {
            diff_id: diff_id.to_string(),
            model,
            binary,
            mode_only,
            symlink: change.is_symlink(),
            old_oid: change.old_oid.clone(),
            new_oid: change.new_oid.clone(),
            old_missing: change.old_oid.is_none(),
            new_missing: change.new_oid.is_none(),
            texts: None,
        })
    }

    /// 按对象 ID 读取两侧文本；缺失一侧是“没有对象”，不是空文件。
    fn ensure_texts(&self, inner: &mut TargetInner, diff_id: &str) -> Result<Arc<Texts>> {
        let keys: Vec<String> = inner
            .content
            .iter()
            .filter(|(_, cached)| cached.diff_id == diff_id)
            .map(|(key, _)| key.clone())
            .collect();
        let Some(key) = keys.first().cloned() else {
            return Err(AppError::internal("提交 diff 缓存丢失"));
        };
        let cached = inner
            .content
            .get_mut(&key)
            .ok_or_else(|| AppError::internal("提交 diff 缓存丢失"))?;
        if cached.texts.is_none() {
            if cached.binary {
                return Err(AppError::rejected("二进制文件没有可显示的文本内容"));
            }
            let old = self.read_blob_text(cached.old_missing, cached.old_oid.as_deref())?;
            let new = self.read_blob_text(cached.new_missing, cached.new_oid.as_deref())?;
            cached.texts = Some(Arc::new(Texts {
                old: old.0,
                new: new.0,
                old_lossy: old.1,
                new_lossy: new.1,
            }));
        }
        cached
            .texts
            .clone()
            .ok_or_else(|| AppError::internal("提交 diff 文本缓存丢失"))
    }

    fn read_blob_text(&self, missing: bool, oid: Option<&str>) -> Result<(String, bool)> {
        if missing {
            return Ok((String::new(), false));
        }
        let oid = oid.ok_or_else(|| AppError::internal("提交 diff 缺少对象 ID"))?;
        let bytes = gh::blob(&self.git, oid)?;
        Ok(match String::from_utf8(bytes.clone()) {
            Ok(text) => (text, false),
            Err(_) => (String::from_utf8_lossy(&bytes).into_owned(), true),
        })
    }
}

fn signature(value: &gh::Signature) -> HistorySignature {
    HistorySignature {
        name: value.name.clone(),
        email: value.email.clone(),
        time: value.time,
        timezone: value.timezone.clone(),
    }
}

fn display_path(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn parse_cursor(cursor: &str) -> Result<(&str, &str, u32)> {
    // 形如 `<target_id>/<query_id>:<skip>`。
    let (target_id, rest) = cursor
        .split_once('/')
        .ok_or_else(|| AppError::rejected("分页游标格式不正确"))?;
    let (query_id, skip) = rest
        .split_once(':')
        .ok_or_else(|| AppError::rejected("分页游标格式不正确"))?;
    if target_id.is_empty() || query_id.is_empty() {
        return Err(AppError::rejected("分页游标格式不正确"));
    }
    let skip = skip
        .parse::<u32>()
        .map_err(|_| AppError::rejected("分页游标格式不正确"))?;
    Ok((target_id, query_id, skip))
}

fn diff_identity(
    target_id: &str,
    commit: &str,
    parent: Option<&str>,
    status: HistoryFileStatus,
    path: &str,
    original_path: Option<&str>,
    old_oid: Option<&str>,
    new_oid: Option<&str>,
) -> String {
    crate::git::fingerprint(&[
        target_id.as_bytes(),
        commit.as_bytes(),
        parent.unwrap_or("").as_bytes(),
        status.as_str().as_bytes(),
        path.as_bytes(),
        original_path.unwrap_or("").as_bytes(),
        old_oid.unwrap_or("").as_bytes(),
        new_oid.unwrap_or("").as_bytes(),
    ])
}

fn diff_key(diff_id: &str, ignore: IgnoreWhitespace) -> String {
    format!("{diff_id}/{}", ignore.as_str())
}

fn ignore_allows_empty(ignore: IgnoreWhitespace) -> bool {
    !matches!(ignore, IgnoreWhitespace::Off)
}

fn empty_reason(cached: &CachedDiff) -> Option<HistoryEmptyReason> {
    if cached.binary {
        return Some(HistoryEmptyReason::Binary);
    }
    if cached.mode_only {
        return Some(HistoryEmptyReason::ModeOnly);
    }
    // 只有“一侧本来就没有对象、另一侧是空内容”才是空文件；忽略空白后没有 hunk
    // 不算空文件，避免把被选项过滤掉的改动说成空。
    if cached.model.hunks.is_empty() && (cached.old_missing || cached.new_missing) {
        return Some(HistoryEmptyReason::EmptyFile);
    }
    None
}

/// 已解析的筛选：交给 Git 的固定参数 + Rust 侧文本匹配 + 规范形式。
struct ResolvedFilter {
    git_filter: gh::LogFilter,
    /// 「文字或哈希前缀」筛查（关闭正则时）：由 Rust 逐批匹配。
    text_match: Option<TextMatch>,
    /// 续页校验用：同一组筛选才算同一条查询。
    key: String,
}

/// 关闭正则时的文本匹配：提交信息（完整含正文）或哈希前缀。
struct TextMatch {
    pattern: String,
    case_sensitive: bool,
    /// 输入看起来像哈希前缀时，同时按对象 ID 前缀匹配（L04）。
    hash_prefix: bool,
}

impl TextMatch {
    fn matches(&self, item: &gh::ScannedCommit) -> bool {
        if self.hash_prefix && item.entry.oid.starts_with(&self.pattern.to_ascii_lowercase()) {
            return true;
        }
        if self.case_sensitive {
            item.message.contains(&self.pattern)
        } else {
            item.message.to_lowercase().contains(&self.pattern.to_lowercase())
        }
    }
}

/// 输入是否看起来是哈希前缀（至少 4 位十六进制字符）。
fn looks_like_hash(text: &str) -> bool {
    text.len() >= 4 && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// 把字面文字转成等价的扩展正则（作者筛选在正则模式下也按字面匹配）。
fn regex_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if "()[]{}.*+?^$|\\".contains(ch) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// 校验 `YYYY-MM-DD`（只接受自建格式，不把前端文字直接塞进 Git 参数）。
fn validate_date(text: &str) -> Result<()> {
    let bytes = text.as_bytes();
    let shaped = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit());
    if !shaped {
        return Err(AppError::rejected(format!("日期必须形如 2026-10-27：{text}")));
    }
    let month: u32 = text[5..7].parse().unwrap_or(0);
    let day: u32 = text[8..10].parse().unwrap_or(0);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(AppError::rejected(format!("日期不合法：{text}")));
    }
    Ok(())
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs() as i64)
        .unwrap_or_default()
}

/// 把 Git 读到的提交转成对外的行（供推送预览等复用）。
pub fn describe_commit_public(commit: &gh::CommitEntry) -> HistoryCommit {
    describe_commit(commit, &[])
}

/// 把 Git 读到的提交转成对外的行（引用标签来自查询开始时的引用快照）。
fn describe_commit(commit: &gh::CommitEntry, refs: &[HistoryRef]) -> HistoryCommit {
    HistoryCommit {
        oid: commit.oid.clone(),
        parents: commit.parents.clone(),
        author: signature(&commit.author),
        committer: signature(&commit.committer),
        subject: commit.subject.clone(),
        refs: refs
            .iter()
            .filter(|label| label.commit == commit.oid)
            .map(|label| HistoryRefLabel {
                full_name: label.full_name.clone(),
                short_name: label.short_name.clone(),
                kind: label.kind,
            })
            .collect(),
    }
}

fn history_row(row: &RenderRow) -> HistoryRow {
    HistoryRow {
        kind: row.kind,
        hunk_index: row.hunk_index,
        section: row.section.clone(),
        old_line: row.old_line,
        new_line: row.new_line,
        line_kind: row.line_kind,
        text: row.text.clone(),
        left: row.left.clone(),
        right: row.right.clone(),
        modified: row.modified,
        segment_index: row.segment_index,
    }
}

/// 从解析结果里挑出与请求对象身份一致的那份文件。
///
/// 身份来自 `diff-tree --raw`（对象 ID、路径、状态），这里只核对是否与 Git 给出的
/// 补丁是同一份内容；不靠补丁标题反推身份。
fn select_patch_file(
    mut files: Vec<crate::git::diff::FileChange>,
    change: &RawChange,
) -> Result<crate::git::diff::FileChange> {
    if files.is_empty() {
        // 空补丁（空文件、仅权限变化）也要有确定的模型。
        return Ok(empty_model(change));
    }
    if files.len() == 1 {
        let mut file = files.pop().unwrap_or_else(|| empty_model(change));
        override_identity(&mut file, change);
        return Ok(file);
    }
    let expected_old = change.old_oid.as_deref().unwrap_or("");
    let expected_new = change.new_oid.as_deref().unwrap_or("");
    let matches: Vec<usize> = files
        .iter()
        .enumerate()
        .filter(|(_, file)| index_line_matches(&file.raw_patch, expected_old, expected_new))
        .map(|(index, _)| index)
        .collect();
    if matches.len() != 1 {
        return Err(AppError::git(format!(
            "Git 返回的差异无法与改动对象对上（匹配 {} 个），已拒绝显示",
            matches.len()
        )));
    }
    let mut file = files.swap_remove(matches[0]);
    override_identity(&mut file, change);
    Ok(file)
}

fn override_identity(file: &mut crate::git::diff::FileChange, change: &RawChange) {
    file.path = display_path(change.primary_path());
    file.old_path = change.original_path().map(display_path);
    file.status = match change.status {
        ChangeStatus::Added => crate::git::status::FileStatus::Added,
        ChangeStatus::Deleted => crate::git::status::FileStatus::Deleted,
        ChangeStatus::Renamed | ChangeStatus::Copied => crate::git::status::FileStatus::Renamed,
        ChangeStatus::TypeChanged => crate::git::status::FileStatus::TypeChanged,
        ChangeStatus::Modified => crate::git::status::FileStatus::Modified,
    };
}

fn empty_model(change: &RawChange) -> crate::git::diff::FileChange {
    crate::git::diff::FileChange {
        path: display_path(change.primary_path()),
        old_path: change.original_path().map(display_path),
        status: match change.status {
            ChangeStatus::Added => crate::git::status::FileStatus::Added,
            ChangeStatus::Deleted => crate::git::status::FileStatus::Deleted,
            ChangeStatus::Renamed | ChangeStatus::Copied => crate::git::status::FileStatus::Renamed,
            ChangeStatus::TypeChanged => crate::git::status::FileStatus::TypeChanged,
            ChangeStatus::Modified => crate::git::status::FileStatus::Modified,
        },
        binary: change.binary,
        hunks: Vec::new(),
        added: change.added,
        deleted: change.deleted,
        old_lines: 0,
        new_lines: 0,
        raw_patch: Vec::new(),
    }
}

/// patch 的 `index <old>..<new>` 行是否与改动对象一致（前缀比较，Git 会缩写）。
fn index_line_matches(patch: &[u8], old_oid: &str, new_oid: &str) -> bool {
    for line in patch.split(|byte| *byte == b'\n') {
        let Some(rest) = line.strip_prefix(b"index ") else {
            continue;
        };
        let text = String::from_utf8_lossy(rest);
        let Some((old, rest)) = text.split_once("..") else {
            continue;
        };
        let new = rest.split(' ').next().unwrap_or("");
        return oid_prefix_matches(old, old_oid) && oid_prefix_matches(new, new_oid);
    }
    false
}

fn oid_prefix_matches(prefix: &str, oid: &str) -> bool {
    if oid.is_empty() {
        return prefix.bytes().all(|byte| byte == b'0');
    }
    !prefix.is_empty() && oid.starts_with(prefix)
}

fn stable_hash(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in text.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// 读取本地引用，标记 HEAD 指向哪一个。
pub fn read_refs(git: &Git) -> Result<Vec<HistoryRef>> {
    let current_ref = gh::head_ref(git);
    let raw = gh::list_refs(git)?;
    Ok(raw
        .into_iter()
        .map(|(full_name, kind, commit)| {
            let short_name = match kind.as_str() {
                "branch" => full_name.trim_start_matches("refs/heads/").to_string(),
                "remoteBranch" => full_name.trim_start_matches("refs/remotes/").to_string(),
                "tag" => full_name.trim_start_matches("refs/tags/").to_string(),
                _ => full_name.clone(),
            };
            let current = current_ref.as_deref() == Some(full_name.as_str());
            HistoryRef {
                full_name,
                short_name,
                kind: HistoryRefKind::parse(&kind),
                commit,
                current,
            }
        })
        .collect())
}

/// 应用级历史目标表：打开、关闭与容量控制。
pub struct HistoryStore {
    inner: Mutex<StoreInner>,
    highlighter: Highlighter,
}

struct StoreInner {
    next_id: u64,
    targets: Vec<(String, std::sync::Arc<HistoryTarget>)>,
    /// 每个 worktree 已失效的最大会话实例号（释放 / 环境重载后登记）。
    ///
    /// `open` 读取 Git 可能很慢，期间 `release_session` 或 `reload_sessions`
    /// 可能已经把会话和它的历史目标清掉；只靠关闭目标表拦不住“先读 Git、后登记”
    /// 的旧实例。这里在登记前校验，失效的会话不能再认领新目标。
    invalidated_sessions: HashMap<String, u64>,
}

/// 让一个历史目标失效并清空它的缓存（表锁已释放后调用）。
fn retire(target: &HistoryTarget) {
    if let Ok(mut state) = target.inner.lock() {
        state.closed = true;
        state.content.clear();
        state.layouts.clear();
        state.sides.clear();
        state.queries.clear();
    }
}

impl Default for HistoryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl HistoryStore {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(StoreInner {
                next_id: 0,
                targets: Vec::new(),
                invalidated_sessions: HashMap::new(),
            }),
            highlighter: Highlighter::start(),
        }
    }

    /// 为一个已登记 worktree 打开历史目标（同一 worktree 允许有多个，各有独立实例身份）。
    pub fn open(&self, session: &WorktreeSession) -> Result<HistoryTargetInfo> {
        // 历史读取用它自己的 Git 副本：部分克隆里缺失对象不自动向远端补取
        // （契约检查“不隐式 fetch 缺失对象”）。工作区读取不受影响。
        let git = session.git.with_env_vars(&[("GIT_NO_LAZY_FETCH", "1")]);
        let head = gh::head_commit(&git)?;
        let head_ref = gh::head_ref(&git);
        let refs = read_refs(&git)?;
        let empty_repository = head.is_none() && refs.is_empty();
        let mut inner = self.inner.lock().map_err(|_| AppError::internal("历史目标表锁失败"))?;
        // 读 Git 期间会话可能已被释放 / 环境重载：失效的实例不能再登记目标。
        // 检查放在目标表锁内、登记之前，因此不会与 release / clear 交错成功。
        if let Some(invalidated) = inner.invalidated_sessions.get(&session.id) {
            if session.instance_id() <= *invalidated {
                return Err(AppError::rejected(
                    "该 worktree 的会话已失效（已释放或环境已重载），请重新打开提交历史",
                ));
            }
        }
        inner.next_id += 1;
        let target_id = format!("h{}-{}", session.instance_id(), inner.next_id);
        let target = std::sync::Arc::new(HistoryTarget {
            id: target_id.clone(),
            worktree_id: session.id.clone(),
            session_instance: session.instance_id(),
            git,
            highlighter: self.highlighter.clone_handle(),
            inner: Mutex::new(TargetInner {
                closed: false,
                queries: Vec::new(),
                query_counter: 0,
                content: HashMap::new(),
                layouts: HashMap::new(),
                sides: HashMap::new(),
            }),
        });
        // 先在表锁内移出被淘汰的目标，释放表锁后再标记它们失效：等待目标锁
        // 不能卡住其他 worktree 的历史请求。
        let mut evicted = Vec::new();
        while inner.targets.len() >= MAX_TARGETS {
            let (_, target) = inner.targets.remove(0);
            evicted.push(target);
        }
        inner.targets.push((target_id.clone(), target));
        drop(inner);
        for target in evicted {
            retire(&target);
        }
        Ok(HistoryTargetInfo {
            target_id,
            worktree_id: session.id.clone(),
            head,
            head_ref,
            empty_repository,
            refs,
            limits: HistoryLimits::current(),
        })
    }

    /// 取得一个仍然打开的历史目标；已关闭或已淘汰时明确拒绝。
    pub fn target(&self, target_id: &str) -> Result<std::sync::Arc<HistoryTarget>> {
        let inner = self.inner.lock().map_err(|_| AppError::internal("历史目标表锁失败"))?;
        inner
            .targets
            .iter()
            .find(|(id, _)| id == target_id)
            .map(|(_, target)| target.clone())
            .ok_or_else(|| AppError::rejected("历史目标已关闭，请重新打开提交历史"))
    }

    /// 关闭一个历史目标（幂等）。关闭后该目标的请求与游标立即失效。
    ///
    /// 先从表里移除并释放表锁，再去等目标自身的锁：目标可能正在跑 Git
    /// （最长 120 秒超时），不能在等它的时候挡住其他 worktree 的历史请求。
    pub fn close(&self, target_id: &str) -> bool {
        let removed = {
            let mut inner = match self.inner.lock() {
                Ok(inner) => inner,
                Err(_) => return false,
            };
            inner
                .targets
                .iter()
                .position(|(id, _)| id == target_id)
                .map(|position| inner.targets.remove(position).1)
        };
        let Some(target) = removed else {
            return false;
        };
        retire(&target);
        true
    }

    /// 标记某个 worktree 的会话实例已失效：之后用它打开历史一律拒绝。
    ///
    /// 只保留每个 worktree 的最大失效实例号（实例号全局单调递增），不随释放次数增长。
    pub fn invalidate_session(&self, worktree_id: &str, instance_id: u64) {
        if let Ok(mut inner) = self.inner.lock() {
            let entry = inner
                .invalidated_sessions
                .entry(worktree_id.to_string())
                .or_insert(instance_id);
            *entry = (*entry).max(instance_id);
        }
    }

    /// 释放某个 worktree 的全部历史目标（会话释放 / 环境重载）。
    pub fn close_worktree(&self, worktree_id: &str) {
        let ids: Vec<String> = {
            let inner = match self.inner.lock() {
                Ok(inner) => inner,
                Err(_) => return,
            };
            inner
                .targets
                .iter()
                .filter(|(_, target)| target.worktree_id == worktree_id)
                .map(|(id, _)| id.clone())
                .collect()
        };
        for id in ids {
            self.close(&id);
        }
    }

    /// 清空全部历史目标（重新加载环境时）。
    pub fn clear(&self) {
        let ids: Vec<String> = {
            let inner = match self.inner.lock() {
                Ok(inner) => inner,
                Err(_) => return,
            };
            inner.targets.iter().map(|(id, _)| id.clone()).collect()
        };
        for id in ids {
            self.close(&id);
        }
    }

    pub fn open_count(&self) -> usize {
        self.inner.lock().map(|inner| inner.targets.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_index_line_binds_object_ids() {
        let patch = b"diff --git a/x b/x\nindex 1111111..2222222 100644\n--- a/x\n+++ b/x\n";
        assert!(index_line_matches(patch, "1111111aaaa", "2222222bbbb"));
        assert!(!index_line_matches(patch, "9999999aaaa", "2222222bbbb"));
        assert!(!index_line_matches(patch, "1111111aaaa", "9999999bbbb"));
        assert!(
            index_line_matches(b"index 0000000..abc1234 100644\n", "", "abc1234def"),
            "新增文件的旧对象是全零缩写"
        );
        assert!(!index_line_matches(b"", "a", "b"));
        assert!(!index_line_matches(b"index 1111111..2222222\n", "", "2222222"));
    }

    #[test]
    fn diff_identity_covers_every_target_field() {
        let base = diff_identity(
            "target",
            "commit",
            Some("parent"),
            HistoryFileStatus::Modified,
            "a.txt",
            None,
            Some("old"),
            Some("new"),
        );
        assert_eq!(
            base,
            diff_identity(
                "target",
                "commit",
                Some("parent"),
                HistoryFileStatus::Modified,
                "a.txt",
                None,
                Some("old"),
                Some("new"),
            )
        );
        for other in [
            diff_identity("other", "commit", Some("parent"), HistoryFileStatus::Modified, "a.txt", None, Some("old"), Some("new")),
            diff_identity("target", "other", Some("parent"), HistoryFileStatus::Modified, "a.txt", None, Some("old"), Some("new")),
            diff_identity("target", "commit", Some("other"), HistoryFileStatus::Modified, "a.txt", None, Some("old"), Some("new")),
            diff_identity("target", "commit", None, HistoryFileStatus::Modified, "a.txt", None, Some("old"), Some("new")),
            diff_identity("target", "commit", Some("parent"), HistoryFileStatus::Added, "a.txt", None, Some("old"), Some("new")),
            diff_identity("target", "commit", Some("parent"), HistoryFileStatus::Modified, "b.txt", None, Some("old"), Some("new")),
            diff_identity("target", "commit", Some("parent"), HistoryFileStatus::Modified, "a.txt", Some("o.txt"), Some("old"), Some("new")),
            diff_identity("target", "commit", Some("parent"), HistoryFileStatus::Modified, "a.txt", None, None, Some("new")),
            diff_identity("target", "commit", Some("parent"), HistoryFileStatus::Modified, "a.txt", None, Some("old"), None),
        ] {
            assert_ne!(base, other);
        }
        // 空白档位不进 diff 身份，但进缓存键，不同档位不共享缓存。
        assert_ne!(diff_key(&base, IgnoreWhitespace::Off), diff_key(&base, IgnoreWhitespace::All));
    }
}
