//! 会话：登记过的仓库 / worktree，以及它们的只读状态与显示模型。
//!
//! 约束（评审调整 1）：
//! - 除 `add_repo` 外，所有命令都只接收 Rust 层登记过的 ID，不接收任意目录路径；
//! - 文件路径必须是当前 `list_changes` 结果里的条目，否则拒绝。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::error::{AppError, ErrorKind, Result};
use crate::git::highlight::{HighlightRequest, HighlightResult, Highlighter};
use crate::git::rows::{self, HunkAction, HunkAnchor, RenderRow, RowKind, ViewMode};
use crate::git::segments::{self, IgnoreWhitespace};
use crate::git::status::{self, ChangesKind, FileStatus, ReadonlyReason, StatusEntry};
use crate::git::{self, Git};
use crate::merge;
use crate::recovery::{
    self, DiscardOutcome, DiscardPhase, RecoverySource, RecoveryStore, WorktreeIdentity,
};
use crate::review::{self, ReviewMarks, ReviewStore, ReviewVersions};
use crate::shell::{self, ShellEnv};

static NEXT_SESSION_INSTANCE: AtomicU64 = AtomicU64::new(1);
// 前端在同一进程内保留已接受的快照版本；重载环境或替换 Git 会重建会话，版本不能归零。
static SNAPSHOT_GENERATION: Mutex<u64> = Mutex::new(0);

/// 高亮取的是哪个版本：旧版本（HEAD 或暂存区）还是新版本（暂存区或工作区）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum FileVersion {
    Old,
    New,
}

/// 一个 worktree（来自 `git worktree list --porcelain`）。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WorktreeEntry {
    /// 由规范路径推导的稳定 ID，前端用它引用该 worktree。
    pub id: String,
    pub path: String,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub bare: bool,
    pub detached: bool,
    /// 是否是仓库的主目录。
    pub is_main: bool,
}

/// 登记过的一个仓库。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RepoEntry {
    pub id: String,
    pub path: String,
    pub name: String,
    /// 路径当前是否还存在。
    pub exists: bool,
    pub worktrees: Vec<WorktreeEntry>,
}

/// 文件列表里的一行。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChangeEntry {
    pub path: String,
    pub original_path: Option<String>,
    pub changes: ChangesKind,
    pub status: FileStatus,
    pub binary: bool,
    pub added: u32,
    pub deleted: u32,
    pub hunk_count: u32,
    pub row_count_unified: u32,
    pub row_count_split: u32,
    pub language: String,
    /// 默认收起：二进制、lock 文件、超大文件（Q32）。
    pub collapsed: bool,
    pub unmerged: bool,
    /// 是否已经统计过增删行数（未跟踪文件在打开时才统计）。
    pub metrics_loaded: bool,
    pub hunks: Vec<HunkAnchor>,
    /// 只读限制原因，None 表示支持写操作。
    pub readonly_reason: Option<ReadonlyReason>,
    /// 文件级 diff 指纹（Q80、ADR-0009）：写操作校验依据。
    pub fingerprint: String,
    /// 每次刷新都会变化，不能用内容相同代替 hunk 目标仍然有效。
    pub snapshot_revision: String,
    /// 该文件当前是否已审阅（按 HEAD + 工作区版本判定，跨组共享）。
    pub reviewed: bool,
    /// HEAD 中的文件版本（审阅标记的失效依据）。
    pub head_version: Option<String>,
    /// 工作区中的文件版本（审阅标记的失效依据）。
    pub worktree_version: Option<String>,
    /// 工作区版本是否**确认**过。
    ///
    /// 子模块等目录若无法用 Git 读出真实版本，这里为 false：该路径的审阅标记
    /// 一律不可认领，不能因为「两次都读不到」就当作版本没变（Q20、ADR-0002）。
    pub worktree_version_confirmed: bool,
}

/// 一次状态读取的结果。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChangesSnapshot {
    pub worktree_id: String,
    pub worktree_path: String,
    pub branch: Option<String>,
    pub detached: bool,
    /// 本次界面看到的 diff 指纹；写操作必须携带（ADR-0005，M2 使用）。
    pub fingerprint: String,
    /// 顶层快照版本（单调递增，空列表也有）：前端用它拒绝晚到的旧响应（Q88、Q89）。
    pub revision: u64,
    pub entries: Vec<ChangeEntry>,
    pub staged_count: u32,
    pub unstaged_count: u32,
    pub untracked_count: u32,
    /// 读取状态与解析 diff 的耗时（毫秒）。
    pub elapsed_ms: u64,
    /// 审阅进度：按文件去重（同一文件跨组只算一次）。
    pub review: review::ReviewMarks,
}

/// 左右对照布局（返修 A1，Q138）：一次取全两侧行数、段、折叠区与段操作目标。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SideBySideLayout {
    pub worktree_id: String,
    pub path: String,
    pub changes: ChangesKind,
    /// 有未解决冲突：左侧 HEAD、右侧工作区（含冲突标记），只读（设计 3.2）。
    pub conflict: bool,
    pub left_line_count: u32,
    pub right_line_count: u32,
    /// 改动段（文件全文行号，0 基左闭右开），按位置排序。
    pub segments: Vec<segments::ChangeSegment>,
    /// 每段的操作目标（与显示同批签发；前端原样传回）。
    pub segment_actions: Vec<SegmentAction>,
    /// 折叠区（Q139）。
    pub folds: Vec<segments::FoldRange>,
    /// 忽略空白档位（Q140）：同时忽略空行时块按钮变灰。
    pub ignore_whitespace: segments::IgnoreWhitespace,
    /// 段按钮是否可用（二进制、冲突、收起等不可用时为 false）。
    pub segment_actions_available: bool,
    pub fingerprint: String,
    pub snapshot_revision: String,
    /// 二进制文件不进入左右对照。
    pub binary: bool,
}

/// 段的操作目标（Q142）：与显示同批签发；前端原样传回，不自己拼补丁。
pub use segments::SegmentAction;

/// 一段显示行。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RowSlice {
    pub worktree_id: String,
    pub path: String,
    pub changes: ChangesKind,
    pub view: ViewMode,
    pub from: u32,
    pub row_count: u32,
    /// 当前过滤后统一视图的全部段首行（0 基）；分页不截断，并排视图为空。
    pub navigation_rows: Vec<u32>,
    pub rows: Vec<RenderRow>,
    pub fingerprint: String,
    /// 生成这段行的快照版本；前端据此拒绝晚到的旧行（Q88、T05）。
    pub revision: u64,
    /// 该文件最新的列表元数据（未跟踪文件在此时才补全）。
    pub entry: ChangeEntry,
    /// 这次读取**实际观察到**的审阅水位：`entry.reviewed` 对应的真实提交序号。
    ///
    /// 与 `entry` 在同一把锁内取出，所以它精确描述「这份 entry 的审阅状态新到哪一步」。
    /// 前端不能用界面上的 revision 或快照 seq 去猜（ST07-R12-2）。
    pub review_seq: u64,
}

/// 环境检查项（Q36：环境问题在窗口顶部提示）。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EnvironmentCheck {
    pub name: String,
    pub ok: bool,
    pub detail: crate::error::AppErrorPayload,
    pub hint: Option<crate::error::AppErrorPayload>,
}

/// 打开 worktree 的结果。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WorktreeSessionInfo {
    pub worktree_id: String,
    pub worktree_path: String,
    pub git_path: String,
    pub git_version: String,
    pub shell: String,
    /// 登录 shell 解析耗时。
    pub shell_elapsed_ms: u64,
    /// 读取 Git 版本耗时（M9 性能记录用）。
    pub git_version_ms: u64,
    /// 是否回退到了最小环境。
    pub used_fallback: bool,
    pub path_entries: u32,
    pub checks: Vec<EnvironmentCheck>,
}

/// 按段丢弃的段目标（返修 A1）：补丁与写前复核都用它从当前缓存切开。
#[derive(Debug, Clone, Copy)]
pub(crate) struct SegmentOverride {
    pub hunk_index: u32,
    pub segment_index: u32,
    /// 显示时所用的忽略空白档位（Q140）：段编号与那个档位的 diff 对齐。
    pub ignore_whitespace: IgnoreWhitespace,
}

/// 单个文件的解析结果缓存。
struct FileModelCache {
    model: git::diff::FileChange,
    unfiled: bool,
    hunk_actions: Vec<HunkAction>,
}

/// 左右对照的全文与段缓存（返修 A1/Q138）：两侧全文只读一次，段与折叠区按档位算一次。
struct SideLayoutCache {
    left_text: String,
    right_text: String,
    segments: Vec<segments::ChangeSegment>,
    folds: Vec<segments::FoldRange>,
}

struct Snapshot {
    fingerprint: String,
    entries: Vec<ChangeEntry>,
    /// 当前快照的审阅进度（按文件去重）。
    review: ReviewMarks,
    /// 每个路径的两个版本，用于判定标记是否仍然有效（Q20、T12）。
    versions: HashMap<String, ReviewVersions>,
    /// 版本**无法确认**的路径（例如未初始化子模块、未登记为子模块的嵌套仓库）。
    ///
    /// 这些路径不允许新增认领；已有标记只能取消（ST07-R3-1、SP07-R3-1）。
    unconfirmed: std::collections::HashSet<String>,
    /// key: `<changes>/<path>`
    models: HashMap<String, FileModelCache>,
    /// 显示行排版缓存：key 为 `<changes>/<path>/<view>`。
    /// 只缓存源行位置，`get_rows` 按请求范围生成文本与 hunk 操作目标。
    layouts: HashMap<String, rows::RowLayout>,
    /// 忽略空白档位下的模型缓存（Q140）：key 为 `<ignore>/<changes>/<path>`。
    ignore_models: HashMap<String, FileModelCache>,
    /// 左右对照的全文与段缓存（A1/Q138）：key 为 `<changes>/<path>/<ignore>/<context>`。
    side_layouts: HashMap<String, SideLayoutCache>,
    generation: u64,
}

pub struct WorktreeSession {
    pub id: String,
    pub path: PathBuf,
    pub git: Git,
    /// 完整登录环境；后台解析完成后可能在原会话上替换（Q212）。
    pub shell_env: std::sync::Mutex<ShellEnv>,
    pub info: WorktreeSessionInfo,
    snapshot: Mutex<Option<Snapshot>>,
    /// 状态读取串行锁：读取与发布必须成对，避免早开始的读取后发布、覆盖较新的快照（Q88）。
    read_lock: Mutex<()>,
    highlighter: Highlighter,
    /// 已经发给高亮线程的文件内容代际，避免重复传输内容（按稳定文档键 + 版本）。
    highlight_generations: Mutex<HashMap<String, HighlightStamp>>,
    instance_id: u64,
    changelist_mode: std::sync::atomic::AtomicBool,
    confirmation_epoch: AtomicU64,
    /// 是否有正在进行的写操作（同一 worktree 互斥，不排队）。
    /// 命令层判断“能否重新加载环境”时也要读它（返修 C3）。
    pub(crate) writing: Mutex<bool>,
    write_generation: AtomicU64,
    /// 写操作开始前等待完整环境就绪的入口（Q222、S9）；只有命令层打开的会话会设置。
    env_gate: std::sync::OnceLock<EnvGate>,
    /// Git 路径变化后被新会话取代：仍持有旧会话的写操作必须重新获取（Q222）。
    superseded: std::sync::atomic::AtomicBool,
    /// 审阅标记存储（None 表示未配置，此时界面不显示勾选框）。
    review_store: Option<Arc<ReviewStore>>,
    /// 审阅标记的**真实提交序号**：每次成功写入在会话内单调递增。
    ///
    /// 前端只用它决定同路径谁是最新持久状态，不用点击次序或回执到达次序
    /// （ST07-R9-1 / SP07-R9-3）。递增发生在 `read_lock` 内，与真实写入同序。
    review_seq: Mutex<u64>,
    /// 打开会话时捕获的持久身份：审阅标记的归属依据。
    ///
    /// 只在这里捕获一次，避免每次刷新都多起 `git rev-parse` 进程（既有性能边界）。
    /// worktree 被移动/重建后需要重新打开会话，届时身份会重新捕获（Q87）。
    identity: Option<WorktreeIdentity>,
}

/// 发给高亮线程的内容版本记录：同一文档只在版本不同时重发内容。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HighlightStamp {
    revision: u64,
    content_version: u64,
}

/// 写操作互斥 RAII Guard，离开作用域自动释放写锁。
pub struct WriteGuard<'a>(&'a Mutex<bool>, &'a AtomicU64);
impl std::fmt::Debug for WriteGuard<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("WriteGuard").field(self.0).field(self.1).finish()
    }
}
impl WriteGuard<'_> {
    pub(crate) fn holds(&self, session: &WorktreeSession) -> bool {
        std::ptr::eq(self.0, &session.writing)
    }
}
impl<'a> Drop for WriteGuard<'a> {
    fn drop(&mut self) {
        if let Ok(mut writing) = self.0.lock() {
            *writing = false;
            self.1.fetch_add(1, Ordering::Release);
        }
    }
}

/// 等待完整环境就绪；超时返回结构化错误。
pub type EnvGate = Arc<dyn Fn() -> Result<()> + Send + Sync>;

pub(crate) fn environment_reloaded() -> AppError {
    AppError::coded(
        ErrorKind::Rejected,
        "errors.environment.reloaded",
        "运行环境已更新，请重试一次",
        &[],
    )
}

/// 生产环境检查：失败时把结构化回退原因交给界面，原始详情作为独立参数（不持久化）。
pub(crate) fn shell_environment_check(shell_env: &ShellEnv) -> EnvironmentCheck {
    let hint = shell_env.info.used_fallback.then(|| {
        let code = shell_env
            .info
            .fallback_reason
            .unwrap_or("errors.environment.shellFallback");
        let detail = shell_env.info.fallback_detail.clone().unwrap_or_default();
        AppError::coded(
            ErrorKind::Environment,
            code,
            "环境解析失败，已回退到进程环境：{detail}",
            &[("detail", detail)],
        )
        .payload()
    });
    EnvironmentCheck {
        name: "shell".into(),
        ok: !shell_env.info.used_fallback,
        detail: AppError::coded(ErrorKind::Environment, "errors.environment.shellTiming", "{shell} · {elapsed} 毫秒",
            &[("shell", shell_env.info.shell.clone()), ("elapsed", shell_env.info.elapsed_ms.to_string())]).payload(),
        hint,
    }
}

impl WorktreeSession {
    /// 换成完整登录环境（Q212）：Git 子进程与内置终端都用新的 PATH / 变量。
    ///
    /// Git 程序路径变化不在原会话上处理，由命令层换一个会话并重读一次。
    pub fn apply_environment(&self, env: ShellEnv) {
        self.git.set_env(&env);
        if let Ok(mut current) = self.shell_env.lock() {
            *current = env;
        }
    }

    /// Git 版本是否满足下限（打开时已检查过，不再额外运行 Git）。
    pub fn git_version_ok(&self) -> bool {
        self.info
            .checks
            .iter()
            .find(|check| check.name == "git")
            .is_none_or(|check| check.ok)
    }

    /// 设置写操作前的环境等待入口（命令层打开会话后调用一次）。
    pub fn set_env_gate(&self, gate: EnvGate) {
        let _ = self.env_gate.set(gate);
    }

    /// 标记已被新会话取代（调用方须持有本会话的 `writing` 锁且没有写操作在进行）。
    pub fn mark_superseded(&self) {
        self.superseded.store(true, Ordering::Release);
    }

    /// 当前环境是否仍是打开时的临时环境（供状态展示）。
    pub fn shell_env_snapshot(&self) -> ShellEnv {
        self.shell_env
            .lock()
            .map(|env| env.clone())
            .unwrap_or_else(|error| error.into_inner().clone())
    }

    pub fn open(
        id: &str,
        path: &Path,
        shell_env: ShellEnv,
        command_log: Arc<git::CommandLog>,
    ) -> Result<Arc<Self>> {
        Self::open_with_review(id, path, shell_env, command_log, None)
    }

    /// 带审阅存储打开会话（生产命令层用这个）。
    pub fn open_with_review(
        id: &str,
        path: &Path,
        shell_env: ShellEnv,
        command_log: Arc<git::CommandLog>,
        review_store: Option<Arc<ReviewStore>>,
    ) -> Result<Arc<Self>> {
        Self::open_with_options(id, path, shell_env, command_log, review_store, None)
    }

    /// 带审阅存储与手动指定 Git 程序的会话（Q27、M4 F08）。
    pub fn open_with_options(
        id: &str,
        path: &Path,
        shell_env: ShellEnv,
        command_log: Arc<git::CommandLog>,
        review_store: Option<Arc<ReviewStore>>,
        git_program: Option<&Path>,
    ) -> Result<Arc<Self>> {
        let git =
            Git::with_program(path, &shell_env, command_log, git_program)?.with_worktree_id(id);
        let version_started = std::time::Instant::now();
        let git_version = git.version()?;
        let git_version_ms = version_started.elapsed().as_millis() as u64;
        let (major, minor) = git::parse_version(&git_version).unwrap_or((0, 0));
        let mut checks = Vec::new();
        checks.push(EnvironmentCheck {
            name: "git".into(),
            ok: (major, minor) >= git::MIN_GIT_VERSION,
            detail: AppError::coded(ErrorKind::Environment, "errors.environment.gitVersion", "{path}（{version}）；需要 Git {minimum} 或更高版本",
                &[("path", git.program.display().to_string()), ("version", git_version.clone()),
                  ("minimum", format!("{}.{}", git::MIN_GIT_VERSION.0, git::MIN_GIT_VERSION.1))]).payload(),
            hint: None,
        });
        checks.push(shell_environment_check(&shell_env));

        let info = WorktreeSessionInfo {
            worktree_id: id.to_string(),
            worktree_path: path.display().to_string(),
            git_path: git.program.display().to_string(),
            git_version,
            shell: shell_env.info.shell.clone(),
            shell_elapsed_ms: shell_env.info.elapsed_ms,
            git_version_ms,
            used_fallback: shell_env.info.used_fallback,
            path_entries: shell_env.path.len() as u32,
            checks,
        };

        // 身份在这里捕获一次，供审阅标记使用（不在每次刷新里重跑 Git）。
        let identity = WorktreeIdentity::capture(&git).ok();
        Ok(Arc::new(Self {
            id: id.to_string(),
            path: path.to_path_buf(),
            git,
            shell_env: std::sync::Mutex::new(shell_env),
            info,
            snapshot: Mutex::new(None),
            read_lock: Mutex::new(()),
            highlighter: Highlighter::start(),
            highlight_generations: Mutex::new(HashMap::new()),
            changelist_mode: std::sync::atomic::AtomicBool::new(false),
            instance_id: NEXT_SESSION_INSTANCE.fetch_add(1, Ordering::Relaxed),
            confirmation_epoch: AtomicU64::new(0),
            writing: Mutex::new(false),
            write_generation: AtomicU64::new(0),
            env_gate: std::sync::OnceLock::new(),
            superseded: std::sync::atomic::AtomicBool::new(false),
            review_store,
            review_seq: Mutex::new(0),
            identity: identity.clone(),
        }))
    }

    /// 会话实例代号：进程内唯一，历史目标据此拒绝旧实例的目标。
    pub fn set_changelist_mode(&self, enabled: bool) { self.changelist_mode.store(enabled, Ordering::Release); }

    pub fn instance_id(&self) -> u64 {
        self.instance_id
    }

    pub fn confirmation_version(&self) -> (u64, u64) {
        (self.instance_id, self.confirmation_epoch.load(Ordering::Acquire))
    }

    /// 重新读取状态并解析 diff（`list_changes`）。
    ///
    /// 读取与发布在 `read_lock` 内成对完成：早开始的读取不会晚于较新的读取发布，
    /// 因此发布出的 generation 始终与内容同步递增（Q88、T05）。
    pub fn refresh(&self) -> Result<ChangesSnapshot> {
        self.refresh_with_hook(|| {})
    }

    /// 受控读取接缝：真实 Git 取数后、发布前通知，用于验证环境替换时的迟到读取。
    pub fn refresh_with_hook(&self, before_publish: impl FnOnce()) -> Result<ChangesSnapshot> {
        self.confirmation_epoch.fetch_add(1, Ordering::AcqRel);
        self.read_with_hook(before_publish)
    }

    /// Internal revalidation does not create a new user confirmation epoch.
    pub(crate) fn read_for_write(&self) -> Result<ChangesSnapshot> {
        self.read_with_hook(|| {})
    }

    fn read_with_hook(&self, before_publish: impl FnOnce()) -> Result<ChangesSnapshot> {
        let _read = self.read_lock.lock().map_err(|_| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.session.readLockFailed",
                "读取锁失败",
                &[],
            )
        })?;
        self.refresh_locked(before_publish)
    }

    fn refresh_locked(&self, before_publish: impl FnOnce()) -> Result<ChangesSnapshot> {
        let mut timing = crate::commands::MeasureSpan::new("session.refresh");
        let started = std::time::Instant::now();
        // 在开始取数时分配版本，并与会话失效互斥。旧会话即使迟到返回，版本也低于新会话。
        // 两把锁都在 Git 取数前释放，不把不同 worktree 的读取串行化。
        let generation = {
            let _writing = self.writing.lock().map_err(|_| {
                AppError::coded(ErrorKind::Internal, "errors.session.writeOperationLockFailed", "写操作锁失败", &[])
            })?;
            if self.superseded.load(Ordering::Acquire) {
                return Err(environment_reloaded());
            }
            let mut counter = SNAPSHOT_GENERATION.lock().map_err(|_| {
                AppError::coded(ErrorKind::Internal, "errors.session.generationLockFailed", "代际锁失败", &[])
            })?;
            *counter += 1;
            *counter
        };
        let changelist_mode = self.changelist_mode.load(Ordering::Acquire);
        let status = status::read(&self.git)?;
        timing.mark("status");

        // `git diff` 会分几次读同一个文件（先算 blob 哈希、再取内容）。如果外部正在**直接保存**
        // （先截断再写入），同一次调用可能拿到「index 行的 blob 来自 A 状态、hunk 文本来自 B 状态」
        // 这种内部不一致的输出。发布它等于伪造快照，指纹也会与实际内容不符。
        // 因此在 diff 前后各取一次工作区文件的 stat；只要有一个变了，这次读取就不发布。
        let watched: Vec<&str> = status
            .entries
            .iter()
            .filter(|entry| changelist_mode || entry.changes != ChangesKind::Staged)
            .map(|entry| entry.path.as_str())
            .collect();
        let before = self.worktree_stats(&watched);

        // 一次读取两种 diff：未暂存（工作区 vs 暂存区）与已暂存（暂存区 vs HEAD）。
        let unstaged_patch = self.git.diff_worktree(None)?;
        let staged_patch = self.git.diff_index(None)?;
        let changelist_patch = if changelist_mode { self.git.diff_changelist(None, IgnoreWhitespace::Off)? } else { Vec::new() };

        timing.mark("git_diffs");
        let after = self.worktree_stats(&watched);
        if before != after {
            // 读取期间工作区变了：这次结果可能是混合的，按「读取失败」处理，不发布。
            return Err(AppError::coded(
                ErrorKind::Git,
                "errors.session.filesChangingWhileStatusBeingRead",
                "读取状态期间文件正在变化，请稍后重试",
                &[],
            ));
        }
        let fingerprint = if changelist_mode { git::fingerprint(&[&unstaged_patch, &staged_patch, &changelist_patch]) } else { git::fingerprint(&[&unstaged_patch, &staged_patch]) };

        let unstaged = git::diff::parse_bytes(&unstaged_patch);
        let staged = git::diff::parse_bytes(&staged_patch);
        timing.mark("parse");

        let mut models: HashMap<String, FileModelCache> = HashMap::new();
        for file in unstaged.files {
            models.insert(
                key(ChangesKind::Unstaged, &file.path),
                FileModelCache {
                    model: file,
                    unfiled: false,
                    hunk_actions: Vec::new(),
                },
            );
        }
        for file in staged.files {
            models.insert(
                key(ChangesKind::Staged, &file.path),
                FileModelCache {
                    model: file,
                    unfiled: false,
                    hunk_actions: Vec::new(),
                },
            );
        }

        let mut entries = Vec::with_capacity(status.entries.len());
        for entry in &status.entries {
            let cached = models.get(&key(entry.changes, &entry.path));
            let model = cached.map(|cached| &cached.model);
            let language = language_for_path(&entry.path);
            let layout_summary = model.map(rows::layout_summary).unwrap_or_default();
            let row_count_unified = layout_summary.unified_count as u32;
            let row_count_split = layout_summary.split_count as u32;

            let mut readonly_reason = entry.readonly_reason;
            let full_path = self.path.join(&entry.path);
            if readonly_reason.is_none() && full_path.is_symlink() {
                readonly_reason = Some(ReadonlyReason::Symlink);
            }
            if readonly_reason.is_none()
                && entry.changes == ChangesKind::Untracked
                && (full_path.join(".git").exists() || entry.path.ends_with('/'))
            {
                readonly_reason = Some(ReadonlyReason::NestedRepo);
            }

            // 指纹来自展示所用的同一份 Git 输出（FileChange.raw_patch），不在刷新中按文件另起 Git 进程（审查意见 3）
            let raw_diff = model.map(|m| m.raw_patch.as_slice()).unwrap_or(&[]);
            let (fingerprint, fp_err) =
                match compute_file_fingerprint_from_patch(entry, raw_diff, &self.path) {
                    Ok(fp) => (fp, None),
                    Err(e) => (String::new(), Some(e)),
                };

            // 读取失败不能变成假空指纹，必须标记只读并记录原因
            if fp_err.is_some() && readonly_reason.is_none() {
                readonly_reason = Some(ReadonlyReason::Unreadable);
            }

            // 工作区版本：普通文件按内容、符号链接按目标、子模块按 Git 真实提交号。
            // 目录（子模块）拿不到真实版本时视为「无法确认」，不生成可认领的版本。
            let worktree_version =
                review::worktree_version(Some(&self.git), &self.path, &entry.path)
                    .ok()
                    .flatten();
            let worktree_version_confirmed =
                worktree_version.is_some() || !self.path.join(&entry.path).is_dir();

            entries.push(ChangeEntry {
                path: entry.path.clone(),
                original_path: entry.original_path.clone(),
                changes: entry.changes,
                status: entry.status,
                binary: model.map(|file| file.binary).unwrap_or(false),
                added: model.map(|file| file.added).unwrap_or(0),
                deleted: model.map(|file| file.deleted).unwrap_or(0),
                hunk_count: model.map(|file| file.hunks.len() as u32).unwrap_or(0),
                row_count_unified,
                row_count_split,
                language,
                collapsed: model
                    .map(|file| is_collapsed(&entry.path, file))
                    .unwrap_or_else(|| looks_like_lock(&entry.path)),
                unmerged: entry.unmerged,
                // 未跟踪文件要读了内容才知道行数，打开时补全。
                metrics_loaded: entry.changes != ChangesKind::Untracked,
                hunks: layout_summary.anchors,
                readonly_reason,
                fingerprint,
                snapshot_revision: String::new(),
                reviewed: false,
                head_version: head_version_for(entry),
                worktree_version: worktree_version.clone(),
                worktree_version_confirmed: worktree_version_confirmed,
            });
        }

        if changelist_mode {
            for file in git::diff::parse_bytes(&changelist_patch).files {
                let template = entries.iter().find(|entry| entry.path == file.path || entry.original_path.as_deref() == Some(file.path.as_str())).cloned();
                let status_entry = status.entries.iter().find(|entry| entry.path == file.path || entry.original_path.as_deref() == Some(file.path.as_str())).cloned();
                let (Some(mut entry), Some(mut status_entry)) = (template, status_entry) else { continue };
                entry.path = file.path.clone(); entry.original_path = None; entry.changes = ChangesKind::Changelist; entry.status = file.status;
                status_entry.path = file.path.clone(); status_entry.original_path = None; status_entry.changes = ChangesKind::Changelist; status_entry.status = file.status;
                entry.fingerprint = compute_file_fingerprint_from_patch(&status_entry, &file.raw_patch, &self.path)?;
                entry.worktree_version = review::worktree_version(Some(&self.git), &self.path, &file.path).ok().flatten();
                entry.worktree_version_confirmed = entry.worktree_version.is_some() || !self.path.join(&file.path).is_dir();
                fill_entry(&mut entry, Some(&file));
                models.insert(key(ChangesKind::Changelist, &file.path), FileModelCache {model:file,unfiled:false,hunk_actions:Vec::new()});
                entries.push(entry);
            }
        }

        before_publish();
        if self.superseded.load(Ordering::Acquire) {
            return Err(environment_reloaded());
        }
        let mut guard = self.snapshot.lock().map_err(|_| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.session.statusLockFailed",
                "状态锁失败",
                &[],
            )
        })?;
        timing.mark("entries_and_snapshot_lock");
        let revision = generation.to_string();
        for entry in &mut entries {
            entry.snapshot_revision = revision.clone();
            if let Some(cached) = models.get_mut(&key(entry.changes, &entry.path)) {
                cached.hunk_actions = self.hunk_actions(entry, &cached.model);
            }
        }

        // 用打开会话时捕获的身份，但每次刷新都**重新确认它现在仍然成立**：
        // 同路径重建后文件系统标识会变，此时不能继续认领旧标记（Q87、ST07-2）。
        // `revalidate` 只重读文件系统标识，不启动 Git 进程（既有性能断言不变）。
        let (review, versions, unconfirmed) = match self.confirmed_identity() {
            Some(identity) => self.review_state(&identity, &entries)?,
            None => (
                self.empty_review(&entries),
                HashMap::new(),
                std::collections::HashSet::new(),
            ),
        };
        timing.mark("review_identity_and_state");
        let reviewed: std::collections::HashSet<&str> =
            review.reviewed.iter().map(String::as_str).collect();
        for entry in &mut entries {
            entry.reviewed = reviewed.contains(entry.path.as_str());
        }

        let snapshot = ChangesSnapshot {
            worktree_id: self.id.clone(),
            worktree_path: self.path.display().to_string(),
            branch: status.branch.clone(),
            detached: status.detached,
            fingerprint: fingerprint.clone(),
            revision: generation,
            staged_count: count(&entries, ChangesKind::Staged),
            unstaged_count: count(&entries, ChangesKind::Unstaged),
            untracked_count: count(&entries, ChangesKind::Untracked),
            entries: entries.clone(),
            elapsed_ms: started.elapsed().as_millis() as u64,
            review: review.clone(),
        };
        *guard = Some(Snapshot {
            fingerprint,
            entries,
            review,
            versions,
            unconfirmed,
            models,
            layouts: HashMap::new(),
            ignore_models: HashMap::new(),
            side_layouts: HashMap::new(),
            generation,
        });
        Ok(snapshot)
    }

    /// 取一段显示行（普通档位，忽略空白关闭）。
    pub fn rows(
        &self,
        path: &str,
        changes: ChangesKind,
        view: ViewMode,
        from: u32,
        to: u32,
    ) -> Result<RowSlice> {
        self.rows_with_ignore(path, changes, view, IgnoreWhitespace::Off, from, to)
    }

    /// 取一段显示行。
    ///
    /// `ignore`（返修 A1/Q140）：忽略空白档位下用对应 Git 参数的 diff 排版，
    /// 块按钮与段的编号与那份 diff 一致；普通档位用刷新时的缓存。
    pub fn rows_with_ignore(
        &self,
        path: &str,
        changes: ChangesKind,
        view: ViewMode,
        ignore: IgnoreWhitespace,
        from: u32,
        to: u32,
    ) -> Result<RowSlice> {
        let mut guard = self.snapshot.lock().map_err(|_| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.session.statusLockFailed2",
                "状态锁失败",
                &[],
            )
        })?;
        let snapshot = guard.as_mut().ok_or_else(|| {
            AppError::coded(
                ErrorKind::Rejected,
                "errors.session.readRepositoryStatusFirst",
                "请先读取仓库状态",
                &[],
            )
        })?;

        if !snapshot
            .entries
            .iter()
            .any(|entry| entry.path == path && entry.changes == changes)
        {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.session.theFileCurrentChangeListRefresh",
                "文件不在当前改动列表中：{path}（请刷新后重试）",
                &[("path", format!("{}", path))],
            ));
        }

        // 未跟踪文件的内容在首次打开时才读，并补全列表元数据。
        let needs_load = snapshot
            .models
            .get(&key(changes, path))
            .map(|cached| cached.unfiled)
            .unwrap_or(true);
        if needs_load {
            let patch = self.git.diff_untracked(path)?;
            let mut parsed = git::diff::parse_bytes(&patch);
            let file = parsed
                .files
                .pop()
                .unwrap_or_else(|| empty_file(path, FileStatus::Added));
            snapshot.models.insert(
                key(changes, path),
                FileModelCache {
                    model: file,
                    unfiled: false,
                    hunk_actions: Vec::new(),
                },
            );
            if let Some(entry) = snapshot
                .entries
                .iter_mut()
                .find(|entry| entry.path == path && entry.changes == changes)
            {
                fill_entry(
                    entry,
                    snapshot
                        .models
                        .get(&key(changes, path))
                        .map(|cached| &cached.model),
                );
            }
        }

        // 忽略空白档位：按需读一份带参数的 diff 并缓存（Q140）。
        let ignore_key = self.ensure_ignore_model(snapshot, path, changes, ignore)?;
        let cached = match &ignore_key {
            None => snapshot.models.get(&key(changes, path)),
            Some(ignore_key) => snapshot.ignore_models.get(ignore_key),
        }
        .ok_or_else(|| {
            AppError::coded(
                ErrorKind::Rejected,
                "errors.session.thereNoChangesFile",
                "没有该文件的改动内容：{path}",
                &[("path", format!("{}", path))],
            )
        })?;
        let layout_key = format!("{changes:?}/{path}/{view:?}/{ignore:?}");
        if !snapshot.layouts.contains_key(&layout_key) {
            // 简单上限：缓存最多 8 份排版（镜像来回切换的常见用法）。
            if snapshot.layouts.len() >= 8 {
                snapshot.layouts.clear();
            }
            snapshot.layouts.insert(
                layout_key.clone(),
                rows::RowLayout::new(&cached.model, view),
            );
        }
        let layout = snapshot.layouts.get(&layout_key).ok_or_else(|| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.session.layoutCacheLost",
                "排版缓存丢失",
                &[],
            )
        })?;
        let row_count = layout.len() as u32;
        let navigation_rows = layout.navigation_rows().to_vec();
        let from = from.min(row_count);
        let to = to.min(row_count).max(from);
        let mut rows = layout.render_range(&cached.model, from as usize..to as usize);
        let hunk_actions = cached.hunk_actions.clone();
        let entry = snapshot
            .entries
            .iter()
            .find(|entry| entry.path == path && entry.changes == changes)
            .cloned()
            .ok_or_else(|| {
                AppError::coded(
                    ErrorKind::Internal,
                    "errors.session.listMetadataLost",
                    "列表元数据丢失",
                    &[],
                )
            })?;
        let revision = snapshot.generation;
        // 块（段）操作目标（返修 A1/Q142）：与显示同批签发，前端原样传回。
        let segment_actions_available = entry.changes != ChangesKind::Changelist && ignore.allows_segment_actions()
            && entry.readonly_reason.is_none()
            && !entry.unmerged
            && !entry.binary
            && entry.original_path.is_none()
            && matches!(
                entry.status,
                FileStatus::Modified | FileStatus::Added | FileStatus::Deleted
            );
        for row in &mut rows {
            if row.kind == RowKind::Hunk {
                row.hunk_action = hunk_actions.get(row.hunk_index as usize).cloned();
            }
            if let (Some(segment_index), true) = (row.segment_index, segment_actions_available) {
                row.segment_action = Some(segments::SegmentAction {
                    worktree_id: self.id.clone(),
                    path: path.to_string(),
                    changes,
                    hunk_index: row.hunk_index,
                    segment_index,
                    fingerprint: entry.fingerprint.clone(),
                    ignore_whitespace: ignore,
                    credential: segments::segment_credential(&entry.fingerprint, ignore),
                    snapshot_revision: revision.to_string(),
                });
            }
        }

        Ok(RowSlice {
            worktree_id: self.id.clone(),
            path: path.to_string(),
            changes,
            view,
            from,
            row_count,
            navigation_rows,
            rows,
            fingerprint: snapshot.fingerprint.clone(),
            revision,
            entry,
            // 与 `entry` 同一次加锁读取：快照的审阅状态就是这个序号对应的真值。
            review_seq: snapshot.review.seq,
        })
    }

    /// 行号对应的显示行下标（切换视图时保持位置用）。
    pub fn row_index_for_line(
        &self,
        path: &str,
        changes: ChangesKind,
        view: ViewMode,
        line: u32,
        version: FileVersion,
    ) -> Result<u32> {
        self.row_index_for_line_with_ignore(
            path,
            changes,
            view,
            line,
            version,
            IgnoreWhitespace::Off,
            None,
            None,
        )
    }

    /// 视图切换使用与取行相同的过滤模型；版本校验和模型选择处于同一次快照锁内。
    pub fn row_index_for_line_with_ignore(
        &self,
        path: &str,
        changes: ChangesKind,
        view: ViewMode,
        line: u32,
        version: FileVersion,
        ignore: IgnoreWhitespace,
        expected_fingerprint: Option<&str>,
        expected_snapshot_revision: Option<&str>,
    ) -> Result<u32> {
        let mut guard = self.snapshot.lock().map_err(|_| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.session.statusLockFailed3",
                "状态锁失败",
                &[],
            )
        })?;
        let snapshot = guard.as_mut().ok_or_else(|| {
            AppError::coded(
                ErrorKind::Rejected,
                "errors.session.readRepositoryStatusFirst2",
                "请先读取仓库状态",
                &[],
            )
        })?;
        let entry = snapshot
            .entries
            .iter()
            .find(|entry| entry.path == path && entry.changes == changes)
            .ok_or_else(|| {
                AppError::coded(
                    ErrorKind::Rejected,
                    "errors.session.thereNoChangesFile2",
                    "没有该文件的改动内容：{path}",
                    &[("path", format!("{}", path))],
                )
            })?;
        if expected_fingerprint.is_some_and(|expected| entry.fingerprint != expected)
            || expected_snapshot_revision
                .is_some_and(|expected| entry.snapshot_revision != expected)
        {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.session.theFileChangedRefreshTryAgain",
                "文件已变化，请刷新后重试",
                &[],
            ));
        }
        let ignore_key = self.ensure_ignore_model(snapshot, path, changes, ignore)?;
        let cached = match &ignore_key {
            None => snapshot.models.get(&key(changes, path)),
            Some(ignore_key) => snapshot.ignore_models.get(ignore_key),
        }
        .ok_or_else(|| {
            AppError::coded(
                ErrorKind::Rejected,
                "errors.session.thereNoChangesFile3",
                "没有该文件的改动内容：{path}",
                &[("path", format!("{}", path))],
            )
        })?;
        Ok(rows::row_index_for_line(
            &cached.model,
            view,
            line,
            version == FileVersion::Old,
        ))
    }

    /// 左右对照布局（返修 A1/Q138）：两侧全文、段、折叠区与段操作目标同批签发。
    ///
    /// 段仍来自同一次 `git diff` 的解析结果（ADR-0004）；忽略空白档位下
    /// 重新读一份带对应参数的 diff（Q140），指纹仍按普通 diff 计算。
    pub fn side_layout(
        &self,
        path: &str,
        changes: ChangesKind,
        ignore: IgnoreWhitespace,
        context_lines: u32,
    ) -> Result<SideBySideLayout> {
        let mut guard = self.snapshot.lock().map_err(|_| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.session.statusLockFailed4",
                "状态锁失败",
                &[],
            )
        })?;
        let snapshot = guard.as_mut().ok_or_else(|| {
            AppError::coded(
                ErrorKind::Rejected,
                "errors.session.readRepositoryStatusFirst3",
                "请先读取仓库状态",
                &[],
            )
        })?;
        let entry = snapshot
            .entries
            .iter()
            .find(|entry| entry.path == path && entry.changes == changes)
            .cloned()
            .ok_or_else(|| {
                AppError::coded(
                    ErrorKind::Rejected,
                    "errors.session.theFileCurrentChangeListRefresh2",
                    "文件不在当前改动列表中：{path}（请刷新后重试）",
                    &[("path", format!("{}", path))],
                )
            })?;
        let worktree_id = self.id.clone();
        let revision = snapshot.generation.to_string();
        if entry.binary {
            return Ok(SideBySideLayout {
                worktree_id,
                path: path.to_string(),
                changes,
                conflict: entry.unmerged,
                left_line_count: 0,
                right_line_count: 0,
                segments: Vec::new(),
                segment_actions: Vec::new(),
                folds: Vec::new(),
                ignore_whitespace: ignore,
                segment_actions_available: false,
                fingerprint: entry.fingerprint.clone(),
                snapshot_revision: revision,
                binary: true,
            });
        }

        let cache_key = self.ensure_side_cache(snapshot, path, changes, ignore, context_lines)?;
        let cached = snapshot.side_layouts.get(&cache_key).ok_or_else(|| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.session.sideSideCacheLost",
                "左右对照缓存丢失",
                &[],
            )
        })?;
        let segments = cached.segments.clone();
        let actions_available = entry.changes != ChangesKind::Changelist && ignore.allows_segment_actions()
            && entry.readonly_reason.is_none()
            && !entry.unmerged
            && !entry.binary
            && entry.original_path.is_none()
            && !segments.is_empty()
            && matches!(
                entry.status,
                FileStatus::Modified | FileStatus::Added | FileStatus::Deleted
            );
        let segment_actions = if actions_available {
            let credential = segments::segment_credential(&entry.fingerprint, ignore);
            segments
                .iter()
                .map(|segment| SegmentAction {
                    worktree_id: worktree_id.clone(),
                    path: path.to_string(),
                    changes,
                    hunk_index: segment.hunk_index,
                    segment_index: segment.segment_index,
                    fingerprint: entry.fingerprint.clone(),
                    ignore_whitespace: ignore,
                    credential: credential.clone(),
                    snapshot_revision: revision.clone(),
                })
                .collect()
        } else {
            Vec::new()
        };
        Ok(SideBySideLayout {
            worktree_id,
            path: path.to_string(),
            changes,
            conflict: entry.unmerged,
            left_line_count: segments::text_line_count(&cached.left_text),
            right_line_count: segments::text_line_count(&cached.right_text),
            segments,
            segment_actions,
            folds: cached.folds.clone(),
            ignore_whitespace: ignore,
            segment_actions_available: actions_available,
            fingerprint: entry.fingerprint.clone(),
            snapshot_revision: revision,
            binary: false,
        })
    }

    /// 按需取左右对照一侧的行（虚拟滚动用），带改动标记与词级区间。
    ///
    /// 行号是**文件全文**的 1 基行号，`from`/`to` 两端包含。
    pub fn side_lines(
        &self,
        path: &str,
        changes: ChangesKind,
        ignore: IgnoreWhitespace,
        context_lines: u32,
        version: FileVersion,
        from: u32,
        to: u32,
        granularity: segments::WordGranularity,
    ) -> Result<Vec<segments::SideLine>> {
        let mut guard = self.snapshot.lock().map_err(|_| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.session.statusLockFailed5",
                "状态锁失败",
                &[],
            )
        })?;
        let snapshot = guard.as_mut().ok_or_else(|| {
            AppError::coded(
                ErrorKind::Rejected,
                "errors.session.readRepositoryStatusFirst4",
                "请先读取仓库状态",
                &[],
            )
        })?;
        if !snapshot
            .entries
            .iter()
            .any(|entry| entry.path == path && entry.changes == changes)
        {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.session.theFileCurrentChangeListRefresh3",
                "文件不在当前改动列表中：{path}（请刷新后重试）",
                &[("path", format!("{}", path))],
            ));
        }
        let cache_key = self.ensure_side_cache(snapshot, path, changes, ignore, context_lines)?;
        let cached = snapshot.side_layouts.get(&cache_key).ok_or_else(|| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.session.sideSideCacheLost2",
                "左右对照缓存丢失",
                &[],
            )
        })?;
        let left_lines: Vec<&str> = segments::display_lines(&cached.left_text).collect();
        let right_lines: Vec<&str> = segments::display_lines(&cached.right_text).collect();
        let lines = match version {
            FileVersion::Old => &left_lines,
            FileVersion::New => &right_lines,
        };
        let from = from.max(1);
        let to = to.max(from);

        let mut changed_lines: std::collections::HashSet<u32> = std::collections::HashSet::new();
        let mut word_ranges: std::collections::HashMap<u32, Vec<(u32, u32)>> =
            std::collections::HashMap::new();
        for segment in &cached.segments {
            let (start, end) = match version {
                FileVersion::Old => (segment.left_start, segment.left_end),
                FileVersion::New => (segment.right_start, segment.right_end),
            };
            // 请求外的行既不返回，也不需要重复分配改动标记。
            if end < from || start >= to {
                continue;
            }
            for line in (start + 1).max(from)..=end.min(to) {
                changed_lines.insert(line);
            }
            // 段的行区间始终按左右各自的全文行号算，与请求的是哪一侧无关。
            let (left_map, right_map) =
                segments::segment_word_ranges(&left_lines, &right_lines, segment, granularity);
            word_ranges.extend(match version {
                FileVersion::Old => left_map,
                FileVersion::New => right_map,
            });
        }
        Ok(segments::side_lines_from_text(
            lines,
            from,
            to,
            &changed_lines,
            &word_ranges,
        ))
    }

    /// 忽略空白档位下按需读一份带对应参数的 diff 并缓存（Q140），返回缓存键；
    /// 不忽略时返回 `None`，调用方用刷新时的普通模型。
    fn ensure_ignore_model(
        &self,
        snapshot: &mut Snapshot,
        path: &str,
        changes: ChangesKind,
        ignore: IgnoreWhitespace,
    ) -> Result<Option<String>> {
        if matches!(ignore, IgnoreWhitespace::Off) {
            return Ok(None);
        }
        let ignore_key = format!("{ignore:?}/{changes:?}/{path}");
        if !snapshot.ignore_models.contains_key(&ignore_key) {
            let original_path = snapshot
                .entries
                .iter()
                .find(|entry| entry.path == path && entry.changes == changes)
                .and_then(|entry| entry.original_path.clone());
            let file = self.read_ignore_model(path, changes, original_path.as_deref(), ignore)?;
            snapshot.ignore_models.insert(
                ignore_key.clone(),
                FileModelCache {
                    model: file,
                    unfiled: false,
                    hunk_actions: Vec::new(),
                },
            );
        }
        Ok(Some(ignore_key))
    }

    /// 读取并解析某一档忽略空白下的 diff（不经缓存）。
    fn read_ignore_model(
        &self,
        path: &str,
        changes: ChangesKind,
        original_path: Option<&str>,
        ignore: IgnoreWhitespace,
    ) -> Result<git::diff::FileChange> {
        let patch = match changes {
            ChangesKind::Unstaged => self.git.diff_worktree_with_ignore(Some(path), ignore)?,
            ChangesKind::Staged => self
                .git
                .diff_index_with_ignore(path, original_path, ignore)?,
            ChangesKind::Untracked => self.git.diff_untracked(path)?,
            ChangesKind::Changelist => self.git.diff_changelist(Some(path), ignore)?,
        };
        let mut parsed = git::diff::parse_bytes(&patch);
        Ok(parsed
            .files
            .pop()
            .unwrap_or_else(|| empty_file(path, FileStatus::Added)))
    }

    /// 确保左右对照的全文与段已缓存，返回缓存键。
    fn ensure_side_cache(
        &self,
        snapshot: &mut Snapshot,
        path: &str,
        changes: ChangesKind,
        ignore: IgnoreWhitespace,
        context_lines: u32,
    ) -> Result<String> {
        let mut timing = crate::commands::MeasureSpan::new("session.side_cache");
        let cache_key = format!("{changes:?}/{path}/{ignore:?}/{context_lines}");
        if snapshot.side_layouts.contains_key(&cache_key) {
            timing.mark("cache_hit");
            return Ok(cache_key);
        }
        let unmerged = snapshot
            .entries
            .iter()
            .any(|entry| entry.path == path && entry.changes == changes && entry.unmerged);
        let cached = if unmerged {
            // 冲突文件（设计 3.2）：HEAD 对工作区（含冲突标记），段来自 `git diff HEAD`，只读。
            let left_text = self.git.file_at("HEAD", path).unwrap_or_default();
            let right_text = self.git.working_file(path).unwrap_or_default();
            let model = git::diff::parse_bytes(&self.git.diff_head(path)?)
                .files
                .pop();
            Self::build_side_layout(left_text, right_text, model.as_ref(), context_lines)?
        } else {
            let (left_text, right_text) = match changes {
                ChangesKind::Unstaged => (
                    self.git.file_at("", path).unwrap_or_default(),
                    self.git.working_file(path).unwrap_or_default(),
                ),
                ChangesKind::Staged => (
                    self.git.file_at("HEAD", path).unwrap_or_default(),
                    self.git.file_at("", path).unwrap_or_default(),
                ),
                ChangesKind::Changelist => (self.git.file_at("HEAD", path).unwrap_or_default(), self.git.working_file(path).unwrap_or_default()),
                ChangesKind::Untracked => (
                    String::new(),
                    self.git.working_file(path).unwrap_or_default(),
                ),
            };
            timing.mark("texts");
            let model = match self.ensure_ignore_model(snapshot, path, changes, ignore)? {
                None => snapshot.models.get(&key(changes, path)),
                Some(ignore_key) => snapshot.ignore_models.get(&ignore_key),
            }
            .map(|cached| &cached.model);
            Self::build_side_layout(left_text, right_text, model, context_lines)?
        };
        timing.mark("layout_built");
        // 派生完成后再改缓存，不必复制快照里的整个 diff 模型。
        // 有界缓存：文件与档位来回切换时不无限累积。
        if snapshot.side_layouts.len() >= 16 {
            snapshot.side_layouts.clear();
        }
        snapshot.side_layouts.insert(cache_key.clone(), cached);
        Ok(cache_key)
    }

    /// 核对两侧全文与 diff 一致后，派生供快照缓存的段与折叠区。
    fn build_side_layout(
        left_text: String,
        right_text: String,
        model: Option<&git::diff::FileChange>,
        context_lines: u32,
    ) -> Result<SideLayoutCache> {
        let mut timing = crate::commands::MeasureSpan::new("session.build_side_layout");
        // 全文与 diff 不是同一次读取：对不上说明期间文件被改过，不缓存，等刷新后重取。
        if let Some(model) = model {
            if !segments::texts_match_diff(model, &left_text, &right_text) {
                return Err(AppError::coded(
                    ErrorKind::Rejected,
                    "errors.session.theFileChangedRefreshTryAgain2",
                    "文件已变化，请刷新后重试",
                    &[],
                ));
            }
        }
        timing.mark("texts_match_diff");
        let (segments, folds) = match model {
            Some(model) => {
                let mut segments: Vec<segments::ChangeSegment> = Vec::new();
                for (index, hunk) in model.hunks.iter().enumerate() {
                    segments.extend(segments::segments_of_hunk(hunk, index as u32));
                }
                let folds = segments::compute_folds_from_segments(
                    &segments,
                    segments::text_line_count(&left_text),
                    segments::text_line_count(&right_text),
                    context_lines,
                );
                (segments, folds)
            }
            None => (Vec::new(), Vec::new()),
        };
        timing.mark("segments_and_folds");
        Ok(SideLayoutCache {
            left_text,
            right_text,
            segments,
            folds,
        })
    }

    /// 按行范围取高亮结果；内容由 Rust 自己读取，前端不接触文件内容。
    ///
    /// 行号约定与界面一致：**1 基、两端包含**（`from_line = 1, to_line = 400` 表示前 400 行）。
    /// 内部解析器的行范围是 0 基、左闭右开，在这里换算。
    pub fn highlight(
        &self,
        path: &str,
        changes: ChangesKind,
        version: FileVersion,
        from_line: u32,
        to_line: u32,
    ) -> Result<HighlightResult> {
        let from0 = from_line.saturating_sub(1) as usize;
        let to0 = to_line as usize;

        // 稳定文档键：不含全局快照指纹，持续刷新不会无限新建文档（Q88）。
        let document_id = format!("{}/{changes:?}/{version:?}/{path}", self.id);
        let language = language_for_path(path);

        let mut attempt = 0u8;
        loop {
            attempt += 1;
            // 取快照的不可变证据：版本、文件指纹、以及**展示所用的那份 diff 原始字节**。
            let (revision, entry_fingerprint, expected_raw, orig_path) = {
                let guard = self.snapshot.lock().map_err(|_| {
                    AppError::coded(
                        ErrorKind::Internal,
                        "errors.session.statusLockFailed6",
                        "状态锁失败",
                        &[],
                    )
                })?;
                let snapshot = guard.as_ref().ok_or_else(|| {
                    AppError::coded(
                        ErrorKind::Rejected,
                        "errors.session.readRepositoryStatusFirst5",
                        "请先读取仓库状态",
                        &[],
                    )
                })?;
                let entry = snapshot
                    .entries
                    .iter()
                    .find(|entry| entry.path == path && entry.changes == changes)
                    .ok_or_else(|| {
                        AppError::coded(
                            ErrorKind::Rejected,
                            "errors.session.theFileCurrentChangeList",
                            "文件不在当前改动列表中：{path}",
                            &[("path", format!("{}", path))],
                        )
                    })?;
                let raw = snapshot
                    .models
                    .get(&key(changes, path))
                    .map(|cached| cached.model.raw_patch.clone())
                    .unwrap_or_default();
                (
                    snapshot.generation,
                    entry.fingerprint.clone(),
                    raw,
                    entry.original_path.clone(),
                )
            };

            // 内容版本由文件身份 + 该版本内容指纹确定：同一内容不重建解析状态。
            let content = self.file_content(path, changes, version)?;
            let content_version =
                content_version_of(&document_id, &entry_fingerprint, content.as_deref());

            let already_sent = {
                let sent = self.highlight_generations.lock().map_err(|_| {
                    AppError::coded(
                        ErrorKind::Internal,
                        "errors.session.highlightLockFailed",
                        "高亮锁失败",
                        &[],
                    )
                })?;
                sent.get(&document_id).copied()
                    == Some(HighlightStamp {
                        revision,
                        content_version,
                    })
            };

            // 首次为这一版内容取高亮前，必须证明这些字节确实属于快照展示的版本：
            // 重新读取该文件当前的相关 diff，与快照里那份字节逐字节比较。
            // 仅比对快照代际或 mtime/size 不能排除「未刷新前外部等长改写」（Q88、T05、ADR-0004）。
            if !already_sent
                && !self.diff_matches_snapshot(
                    path,
                    changes,
                    &expected_raw,
                    orig_path.as_deref(),
                )?
            {
                return Err(AppError::coded(
                    ErrorKind::Rejected,
                    "errors.session.theFileChangedConfirmAgain",
                    "文件已变化，请重新确认",
                    &[],
                ));
            }

            let mut request = HighlightRequest {
                document_id: document_id.clone(),
                generation: content_version,
                language: language.clone(),
                from_line: from0,
                to_line: to0,
                content: None,
                revision,
            };

            if !already_sent {
                request.content = content.clone();
            }

            let mut result = self.highlighter.highlight(request)?;
            if result.need_content {
                // 高亮线程上没有这个文件的解析状态（可能被淘汰），补发一次内容。
                let retry = HighlightRequest {
                    document_id: document_id.clone(),
                    generation: content_version,
                    language: language.clone(),
                    from_line: from0,
                    to_line: to0,
                    content: Some(content.clone().unwrap_or_default()),
                    revision,
                };
                result = self.highlighter.highlight(retry)?;
            }
            if result.need_content {
                return Err(AppError::coded(
                    ErrorKind::Internal,
                    "errors.session.theHighlightingThreadDidAcceptFile",
                    "高亮线程没有接受文件内容",
                    &[],
                ));
            }

            // 读取内容期间快照可能已前进：本次结果属于旧版本，要求调用方重新请求，
            // 绝不把旧内容标成新版本（Q88、T05）。
            let now = self
                .snapshot
                .lock()
                .map_err(|_| {
                    AppError::coded(
                        ErrorKind::Internal,
                        "errors.session.statusLockFailed7",
                        "状态锁失败",
                        &[],
                    )
                })?
                .as_ref()
                .map(|snapshot| snapshot.generation)
                .unwrap_or(revision);
            if now != revision {
                if attempt < 2 {
                    continue;
                }
                return Err(AppError::coded(
                    ErrorKind::Rejected,
                    "errors.session.theFileChangedConfirmAgain2",
                    "文件已变化，请重新确认",
                    &[],
                ));
            }

            {
                let mut sent = self.highlight_generations.lock().map_err(|_| {
                    AppError::coded(
                        ErrorKind::Internal,
                        "errors.session.highlightLockFailed2",
                        "高亮锁失败",
                        &[],
                    )
                })?;
                // 有界清理：只保留当前版本的记录，避免刷新累积旧键。
                sent.retain(|key, _| key == &document_id);
                sent.insert(
                    document_id.clone(),
                    HighlightStamp {
                        revision,
                        content_version,
                    },
                );
            }
            // 对外统一成 1 基行号（与界面一致）。
            result.from_line = from_line as usize;
            result.revision = revision;
            result.content_version = content_version;
            return Ok(result);
        }
    }

    /// 该文件当前的相关 diff 是否与快照记录的那份字节完全一致。
    ///
    /// 只有一致时，读取到的版本内容才与界面上展示的 diff / 写指纹属于同一版本；
    /// 不修改暂存区，也不把无关 worktree 内容混入已暂存侧指纹（ADR-0009）。
    fn diff_matches_snapshot(
        &self,
        path: &str,
        changes: ChangesKind,
        expected_raw: &[u8],
        orig_path: Option<&str>,
    ) -> Result<bool> {
        let current = match changes {
            ChangesKind::Unstaged => self.git.diff_worktree(Some(path))?,
            ChangesKind::Staged => self.git.diff_index_with_orig(path, orig_path)?,
            ChangesKind::Untracked => self.git.diff_untracked(path)?,
            ChangesKind::Changelist => self.git.diff_changelist(Some(path), IgnoreWhitespace::Off)?,
        };
        Ok(current == expected_raw)
    }

    /// 某个版本的文件内容（旧版本可能是 HEAD 或暂存区，新版本可能是暂存区或工作区）。
    fn file_content(
        &self,
        path: &str,
        changes: ChangesKind,
        version: FileVersion,
    ) -> Result<Option<String>> {
        let scope = match (changes, version) {
            (ChangesKind::Unstaged, FileVersion::Old) | (ChangesKind::Staged, FileVersion::New) => {
                "index"
            }
            (ChangesKind::Staged | ChangesKind::Changelist, FileVersion::Old) => "HEAD",
            (ChangesKind::Unstaged | ChangesKind::Changelist, FileVersion::New) => "worktree",
            // 未跟踪文件没有旧版本
            (ChangesKind::Untracked, FileVersion::Old) => "none",
            (ChangesKind::Untracked, FileVersion::New) => "worktree",
        };
        Ok(match scope {
            "index" => self.git.file_at("", path),
            "HEAD" => self.git.file_at("HEAD", path),
            "worktree" => self.git.working_file(path),
            _ => None,
        })
    }

    pub fn snapshot_fingerprint(&self) -> Option<String> {
        self.snapshot
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|snapshot| snapshot.fingerprint.clone()))
    }

    /// 当前高亮线程保留的文档数（验证缓存有界；测试与诊断用）。
    /// 合并页签按内容取高亮（Q143）：三个版本与可编辑的结果都不是某个 diff 的一侧，
    /// 由界面带上内容与版本号，解析状态仍放在同一个高亮线程里。
    pub fn highlight_text(&self, request: HighlightRequest) -> Result<HighlightResult> {
        self.highlighter.highlight(request)
    }

    pub fn highlight_document_count(&self) -> usize {
        self.highlighter.document_count()
    }

    /// 该 worktree 自己的 gitDir（linked worktree 的 `.git` 是文件，由 Git 解析出真实目录）。
    pub fn git_dir_path(&self) -> Option<PathBuf> {
        let text = self
            .git
            .try_run(&["rev-parse", "--path-format=absolute", "--git-dir"])?;
        let path = text.trim();
        (!path.is_empty()).then(|| PathBuf::from(path))
    }

    /// 共享 commonDir（主仓库的 .git）；单独仓库与 gitDir 相同。
    pub fn common_dir_path(&self) -> Option<PathBuf> {
        self.git.common_dir()
    }

    /// M1 没有写操作，这里只用于报告与测试。
    pub fn is_binary(&self, path: &str, changes: ChangesKind) -> bool {
        self.snapshot
            .lock()
            .ok()
            .and_then(|guard| {
                guard.as_ref().and_then(|snapshot| {
                    snapshot
                        .models
                        .get(&key(changes, path))
                        .map(|cached| cached.model.binary)
                })
            })
            .unwrap_or(false)
    }

    /// 当前快照里的条目（批量操作按用户确认的原快照逐项核对）。
    pub(crate) fn snapshot_entries(&self) -> Option<Vec<ChangeEntry>> {
        self.snapshot
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|snapshot| snapshot.entries.clone()))
    }

    /// 本会话使用的 Git 上下文（批量操作读取真实状态用）。
    pub(crate) fn git(&self) -> &Git {
        &self.git
    }

    /// A read observation does not mark the repository as writing or block PTY input.
    /// Its result may be published only if no repository write overlapped the read.
    pub fn idle_observation_version(&self) -> Result<u64> {
        self.publish_idle_observation(None, || Ok(self.write_generation.load(Ordering::Acquire)))
    }

    /// Keep the final metadata publication atomic with respect to write admission.
    /// The callback only publishes application metadata; it must not run Git or async work.
    pub fn publish_idle_observation<T>(&self, expected: Option<u64>, publish: impl FnOnce() -> Result<T>) -> Result<T> {
        let writing = self.writing.lock().map_err(|_| AppError::coded(ErrorKind::Internal, "errors.session.writeOperationLockFailed", "Write lock failed", &[]))?;
        if *writing || expected.is_some_and(|version| version != self.write_generation.load(Ordering::Acquire)) {
            return Err(AppError::coded(ErrorKind::Rejected, "errors.session.aWriteOperationProgressWorktreeWait", "Worktree write overlapped this observation", &[]));
        }
        self.require_identity()?;
        publish()
    }

    /// 获取当前 worktree 的独占写操作锁（不排队，发现被占用立即拒绝并发写，Q90/T06）。
    pub fn acquire_write_lock(&self) -> Result<WriteGuard<'_>> {
        // 所有写操作统一在这里等待完整环境（hooks、凭据、git-lfs 等都需要完整 PATH，Q222、S9）。
        if let Some(gate) = self.env_gate.get() {
            gate()?;
        }
        let mut writing = self.writing.lock().map_err(|_| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.session.writeOperationLockFailed",
                "写操作锁失败",
                &[],
            )
        })?;
        if *writing {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.session.aWriteOperationProgressWorktreeWait",
                "当前 worktree 有正在进行的写操作，请稍候",
                &[],
            ));
        }
        // 环境安装与会话替换都在持有 `writing` 锁时进行，这里的判断是权威的。
        if self.superseded.load(Ordering::Acquire) {
            return Err(environment_reloaded());
        }
        self.require_identity()?;
        self.write_generation.fetch_add(1, Ordering::Release);
        *writing = true;
        Ok(WriteGuard(&self.writing, &self.write_generation))
    }

    /// 整文件暂存（Q8、Q79、Q81–Q83、ADR-0005、ADR-0009）。
    pub fn stage_file(&self, path: &str, fingerprint: &str) -> Result<ChangesSnapshot> {
        self.execute_staging_op(path, fingerprint, true, None)
    }

    /// 整文件取消暂存（Q8、Q79、Q81–Q83、ADR-0005、ADR-0009、T04）。
    pub fn unstage_file(&self, path: &str, fingerprint: &str) -> Result<ChangesSnapshot> {
        self.execute_staging_op(path, fingerprint, false, None)
    }

    pub fn stage_hunk(
        &self,
        path: &str,
        fingerprint: &str,
        hunk_id: &str,
    ) -> Result<ChangesSnapshot> {
        self.execute_staging_op(path, fingerprint, true, Some(hunk_id))
    }

    pub fn unstage_hunk(
        &self,
        path: &str,
        fingerprint: &str,
        hunk_id: &str,
    ) -> Result<ChangesSnapshot> {
        self.execute_staging_op(path, fingerprint, false, Some(hunk_id))
    }

    /// 按段写（返修 A1/Q142）：暂存、取消暂存一段；补丁由同一个 Git hunk
    /// 按段切开得到（同 git add -p），不用前端传补丁（ADR-0004）。
    ///
    /// `credential` 是显示时与段同批签发的凭证（文件指纹 + 忽略空白档位，Q140）；
    /// 档位与请求不符就拒绝。
    pub fn execute_segment_op(
        &self,
        path: &str,
        credential: &str,
        hunk_index: u32,
        segment_index: u32,
        is_stage: bool,
        ignore_whitespace: IgnoreWhitespace,
    ) -> Result<ChangesSnapshot> {
        let expected_fp = segment_fingerprint(credential, ignore_whitespace)?;
        let _guard = self.acquire_write_lock()?;
        self.execute_segment_op_locked(
            path,
            expected_fp,
            hunk_index,
            segment_index,
            is_stage,
            ignore_whitespace,
        )
    }

    pub(crate) fn execute_segment_op_locked(
        &self,
        path: &str,
        expected_fp: &str,
        hunk_index: u32,
        segment_index: u32,
        is_stage: bool,
        ignore_whitespace: IgnoreWhitespace,
    ) -> Result<ChangesSnapshot> {
        let mut last_lock_err: Option<String> = None;
        const LOCK_RETRIES: usize = 3;
        const LOCK_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(100);

        for attempt in 0..=LOCK_RETRIES {
            // 每次尝试（含重试）前重新读取真实状态并核对指纹（审查意见 2）。
            let status = status::read(&self.git)?;
            let changes = if is_stage {
                ChangesKind::Unstaged
            } else {
                ChangesKind::Staged
            };
            let entry = status
                .entries
                .iter()
                .find(|entry| entry.path == path && entry.changes == changes)
                .ok_or_else(|| {
                    AppError::coded(
                        ErrorKind::Rejected,
                        "errors.session.theFileChangedConfirmAgain3",
                        "文件已变化，请重新确认",
                        &[],
                    )
                })?;
            if let Some(reason) = &entry.readonly_reason {
                return Err(reason.error());
            }
            // Q81：Git 识别的改名只做整文件操作。
            if entry.original_path.is_some() {
                return Err(AppError::coded(
                    ErrorKind::Rejected,
                    "errors.session.renamedFilesOnlyHandledAsWhole",
                    "改名的文件只能整个文件操作",
                    &[],
                ));
            }
            let diff_bytes = if is_stage {
                self.git.diff_worktree(Some(path))?
            } else {
                self.git
                    .diff_index_with_orig(path, entry.original_path.as_deref())?
            };
            let current_fp = compute_file_fingerprint_from_patch(entry, &diff_bytes, &self.path)?;
            if current_fp != expected_fp {
                return Err(AppError::coded(
                    ErrorKind::Rejected,
                    "errors.session.theFileChangedConfirmAgain4",
                    "文件已变化，请重新确认",
                    &[],
                ));
            }

            // 段号是显示时按同一档位的 diff 签发的：普通档位直接用指纹核对过的这份 diff，
            // 忽略空白档位重新读同一档位的 diff（内容未变，结果与显示时相同）。
            let model = if matches!(ignore_whitespace, IgnoreWhitespace::Off) {
                git::diff::parse_bytes(&diff_bytes).files.into_iter().next()
            } else {
                Some(self.read_ignore_model(path, changes, None, ignore_whitespace)?)
            }
            .ok_or_else(|| {
                AppError::coded(
                    ErrorKind::Rejected,
                    "errors.session.theFileChangedConfirmAgain5",
                    "文件已变化，请重新确认",
                    &[],
                )
            })?;
            let patch = segments::rewrite_hunk_for_segment(
                &model,
                hunk_index as usize,
                segment_index,
                is_stage,
            )
            .ok_or_else(|| {
                AppError::coded(
                    ErrorKind::Rejected,
                    "errors.session.aPatchGeneratedBlockRefreshConfirm",
                    "当前块无法生成补丁，请刷新后重新确认",
                    &[],
                )
            })?;
            let expected_remaining =
                segments::signatures_without(&model, hunk_index as usize, segment_index)
                    .ok_or_else(|| {
                        AppError::coded(
                            ErrorKind::Rejected,
                            "errors.session.aPatchGeneratedBlockRefreshConfirm2",
                            "当前块无法生成补丁，请刷新后重新确认",
                            &[],
                        )
                    })?;

            let output = self.git.apply_index_patch(
                patch,
                !is_stage,
                !matches!(ignore_whitespace, IgnoreWhitespace::Off),
            )?;
            if output.timed_out || output.exit_code < 0 {
                return Err(AppError::coded(
                    ErrorKind::NeedsVerification,
                    "errors.session.theGitCommandTimedOutInterrupted",
                    "Git 命令执行超时或中断，写入结果待核对；请重新核对真实状态后再继续",
                    &[],
                ));
            }
            if git::is_lock_error(&output) {
                let err_text = output.stderr.trim().to_string();
                if attempt < LOCK_RETRIES {
                    last_lock_err = Some(err_text);
                    std::thread::sleep(LOCK_RETRY_DELAY * (attempt as u32 + 1));
                    continue;
                }
                let detail = last_lock_err.unwrap_or(err_text);
                return Err(AppError::coded(
                    ErrorKind::Rejected,
                    "errors.session.theGitIndexLockedAnotherProcess",
                    "Git 索引被其他进程占用（index.lock），已重试 {LOCK_RETRIES} 次：{detail}",
                    &[
                        ("LOCK_RETRIES", format!("{}", LOCK_RETRIES)),
                        ("detail", format!("{}", detail)),
                    ],
                ));
            }
            if output.exit_code != 0 {
                let (code, template) = if is_stage {
                    (
                        "errors.session.failedStageHunk",
                        "暂存 hunk 失败（退出码 {code}）：{detail}",
                    )
                } else {
                    (
                        "errors.session.failedUnstageHunk",
                        "取消暂存 hunk 失败（退出码 {code}）：{detail}",
                    )
                };
                return Err(AppError::coded(
                    ErrorKind::Git,
                    code,
                    template,
                    &[
                        ("code", format!("{}", output.exit_code)),
                        ("detail", format!("{}", output.stderr.trim())),
                    ],
                ));
            }
            // 写后核对（设计 3.2）：按同一档位重新读来源一侧的 diff，目标段应已移走、
            // 其余段不变；对不上标“结果待核对”（Q91）。
            let verified = self
                .read_ignore_model(path, changes, None, ignore_whitespace)
                .map(|after| segments::segment_signatures(&after) == expected_remaining);
            let refreshed = self.refresh();
            return match (verified, refreshed) {
                (Ok(true), Ok(snapshot)) => Ok(snapshot),
                (Ok(false), _) => Err(AppError::coded(
                    ErrorKind::NeedsVerification,
                    "errors.session.theWriteRanButChangesRead",
                    "写入已执行，但重新读取的改动与预期不符，结果待核对；请检查这个文件的暂存内容",
                    &[],
                )),
                (Err(error), _) | (_, Err(error)) => Err(AppError::coded(
                    ErrorKind::NeedsVerification,
                    "errors.session.theWriteRanButReadingNew",
                    "写入已执行，重新读取状态失败：{cause}",
                    &[],
                ).cause(error)),
            };
        }
        unreachable!("锁重试循环总会返回")
    }

    pub fn discard_file(
        &self,
        path: &str,
        fingerprint: &str,
        revision: &str,
        store: &RecoveryStore,
    ) -> Result<ChangesSnapshot> {
        self.discard_with_hook(path, fingerprint, revision, None, store, |_| Ok(()))
    }

    pub fn discard_hunk(
        &self,
        path: &str,
        fingerprint: &str,
        revision: &str,
        hunk_id: &str,
        store: &RecoveryStore,
    ) -> Result<ChangesSnapshot> {
        self.discard_with_hook(
            path,
            fingerprint,
            revision,
            Some(hunk_id),
            store,
            |_| Ok(()),
        )
    }

    fn check_discard_target(
        &self,
        path: &str,
        fingerprint: &str,
        revision: Option<&str>,
    ) -> Result<StatusEntry> {
        {
            let guard = self.snapshot.lock().map_err(|_| {
                AppError::coded(
                    ErrorKind::Internal,
                    "errors.session.statusLockFailed8",
                    "状态锁失败",
                    &[],
                )
            })?;
            let snapshot = guard.as_ref().ok_or_else(|| {
                AppError::coded(
                    ErrorKind::Rejected,
                    "errors.session.readRepositoryStatusFirst6",
                    "请先读取仓库状态",
                    &[],
                )
            })?;
            // 批量丢弃时，用户确认的原快照版本已在整批开始时逐项核对；
            // 批次内每次刷新都会推进版本，因此单文件复检只看真实指纹（ADR-0009）。
            if !snapshot.entries.iter().any(|entry| {
                entry.path == path
                    && entry.changes == ChangesKind::Unstaged
                    && entry.fingerprint == fingerprint
                    && revision.is_none_or(|revision| entry.snapshot_revision == revision)
            }) {
                return Err(AppError::coded(
                    ErrorKind::Rejected,
                    "errors.session.theFileChangedConfirmAgain6",
                    "文件已变化，请重新确认",
                    &[],
                ));
            }
        }
        let status = status::read(&self.git)?;
        let entry = status
            .entries
            .into_iter()
            .find(|entry| entry.path == path && entry.changes == ChangesKind::Unstaged)
            .ok_or_else(|| {
                AppError::coded(
                    ErrorKind::Rejected,
                    "errors.session.theFileChangedConfirmAgain7",
                    "文件已变化，请重新确认",
                    &[],
                )
            })?;
        if let Some(reason) = &entry.readonly_reason {
            return Err(reason.error());
        }
        // intent-to-add（`git add -N`）在暂存区还没有 blob：既没有可恢复的提交基线，
        // 反向应用又会直接删除文件。按「未跟踪文件没有删除入口」的既有边界明确拒绝。
        if entry.status == FileStatus::Added
            && entry
                .oid_index
                .as_deref()
                .map_or(true, |oid| oid.chars().all(|ch| ch == '0'))
        {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.session.aNewFileNoRecoverableOriginal",
                "新增文件没有可恢复的原始版本，不能在应用内丢弃",
                &[],
            ));
        }
        let diff = self.git.diff_worktree(Some(path))?;
        if compute_file_fingerprint_from_patch(&entry, &diff, &self.path)? != fingerprint {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.session.theFileChangedConfirmAgain8",
                "文件已变化，请重新确认",
                &[],
            ));
        }
        Ok(entry)
    }

    /// Test seams surround real storage and real Git; normal operations pass a no-op.
    pub fn discard_with_hook(
        &self,
        path: &str,
        fingerprint: &str,
        revision: &str,
        hunk_id: Option<&str>,
        store: &RecoveryStore,
        hook: impl FnMut(DiscardPhase) -> Result<()>,
    ) -> Result<ChangesSnapshot> {
        let _write = self.acquire_write_lock()?;
        self.discard_with_hook_locked(
            path,
            fingerprint,
            Some(revision),
            hunk_id,
            None,
            store,
            hook,
        )
    }

    /// 丢弃一段（返修 A1/Q142）：补丁由缓存的同一个 Git hunk 按段切开（反向），
    /// 其余流程（回收区、写前校验、写后核对）与单块丢弃一致。
    pub fn discard_segment(
        &self,
        path: &str,
        credential: &str,
        revision: &str,
        hunk_index: u32,
        segment_index: u32,
        ignore_whitespace: IgnoreWhitespace,
        store: &RecoveryStore,
    ) -> Result<ChangesSnapshot> {
        let fingerprint = segment_fingerprint(credential, ignore_whitespace)?;
        let _write = self.acquire_write_lock()?;
        self.discard_with_hook_locked(
            path,
            fingerprint,
            Some(revision),
            None,
            Some(SegmentOverride {
                hunk_index,
                segment_index,
                ignore_whitespace,
            }),
            store,
            |_| Ok(()),
        )
    }

    /// 已持有同 worktree 写锁时的丢弃实现（批量操作用）。
    ///
    /// `revision` 为 None 时不比对快照版本，但仍会重新读取真实状态并核对指纹。
    /// `segment_override`（返修 A1）：按段丢弃时的段目标；补丁与写前复核都
    /// 从**当前缓存**的同一个 hunk 按段切开，段变了（上下文偏移）即拒绝。
    pub(crate) fn discard_with_hook_locked(
        &self,
        path: &str,
        fingerprint: &str,
        revision: Option<&str>,
        hunk_id: Option<&str>,
        segment_override: Option<SegmentOverride>,
        store: &RecoveryStore,
        mut hook: impl FnMut(DiscardPhase) -> Result<()>,
    ) -> Result<ChangesSnapshot> {
        let entry = self.check_discard_target(path, fingerprint, revision)?;
        let identity = WorktreeIdentity::capture(&self.git)?;
        store.ensure_outside(&identity)?;
        let mut paths = vec![entry.path.clone()];
        if let Some(original) = &entry.original_path {
            paths.push(original.clone());
        }
        paths.sort();
        paths.dedup();
        let before = recovery::capture_files(&self.path, &paths)?;
        let patch = match &segment_override {
            Some(segment) => self.segment_patch(path, segment, false)?,
            None => match hunk_id {
                Some(id) => self.hunk_patch(path, fingerprint, id, true)?,
                None => self.git.discard_patch(path)?,
            },
        };
        if patch.is_empty() {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.session.theFileChangedConfirmAgain9",
                "文件已变化，请重新确认",
                &[],
            ));
        }
        // 预览与保存必须使用同一套「字节如何转换」的上下文（属性 + 相关配置）。
        let context = recovery::context_fingerprint(&self.git, &paths)?;
        let expected_after = store.preview(&self.git, &self.path, &paths, &before, &patch)?;
        hook(DiscardPhase::BeforeSave)?;
        // A complete, durably published record is the prerequisite for any worktree write.
        // 占用标记覆盖「保存 → 写入 → 结果记录」全程：期间清理不能删掉这条唯一备份（Q91）。
        let (record, _record_in_use) = store.save(
            identity.clone(),
            if hunk_id.is_some() || segment_override.is_some() {
                RecoverySource::DiscardHunk
            } else {
                RecoverySource::DiscardFile
            },
            path,
            before.clone(),
            context.clone(),
        )?;
        hook(DiscardPhase::AfterSave).map_err(discard_pending)?;
        hook(DiscardPhase::BeforeWrite).map_err(discard_pending)?;
        {
            // Do not let an internal refresh replace hunk credentials between the final check and Git.
            let _read = self.read_lock.lock().map_err(|_| {
                AppError::coded(
                    ErrorKind::Internal,
                    "errors.session.readLockFailed2",
                    "读取锁失败",
                    &[],
                )
            })?;
            let validation = (|| {
                self.check_discard_target(path, fingerprint, revision)?;
                if WorktreeIdentity::capture(&self.git)? != identity
                    || !recovery::unchanged(&before, &recovery::capture_files(&self.path, &paths)?)
                {
                    return Err(AppError::coded(
                        ErrorKind::Rejected,
                        "errors.session.theFileChangedConfirmAgain10",
                        "文件已变化，请重新确认",
                        &[],
                    ));
                }
                let current_patch = match &segment_override {
                    // 按段丢弃：重新从当前缓存切开同一段比对；段变了（上下文偏移）即拒绝。
                    Some(segment) => self.segment_patch(path, segment, false).unwrap_or_default(),
                    None => match hunk_id {
                        Some(id) => self.hunk_patch(path, fingerprint, id, true)?,
                        None => self.git.discard_patch(path)?,
                    },
                };
                if current_patch != patch {
                    return Err(AppError::coded(
                        ErrorKind::Rejected,
                        "errors.session.theFileChangedConfirmAgain11",
                        "文件已变化，请重新确认",
                        &[],
                    ));
                }
                // 属性/转换上下文变化时，同一份补丁会写出不同的字节（例如未选中的行也被改了换行）。
                // 目标字节与补丁都相同并不能证明写入等价，必须在此之前拒绝。
                if recovery::context_fingerprint(&self.git, &paths)? != context {
                    return Err(AppError::coded(
                        ErrorKind::Rejected,
                        "errors.session.theFileSGitAttributesLine",
                        "文件的 Git 属性或换行转换配置已变化，请重新确认后再丢弃",
                        &[],
                    ));
                }
                Ok(())
            })();
            if let Err(error) = validation {
                store
                    .finish(&record, &DiscardOutcome::NotApplied)
                    .map_err(discard_pending)?;
                return Err(error);
            }
            let output = self
                .git
                .apply_worktree_patch(patch)
                .map_err(discard_pending)?;
            if output.timed_out || output.exit_code != 0 {
                return Err(discard_pending(merge_git_error(&output)));
            }
        }
        hook(DiscardPhase::AfterWrite).map_err(discard_pending)?;
        let actual_after = recovery::capture_files(&self.path, &paths).map_err(discard_pending)?;
        if actual_after != expected_after {
            return Err(discard_pending(merge_verification_error(path)));
        }
        hook(DiscardPhase::BeforeComplete).map_err(discard_pending)?;
        store
            .finish(
                &record,
                &DiscardOutcome::Completed {
                    after: expected_after,
                },
            )
            .map_err(discard_pending)?;
        self.refresh().map_err(|error| {
            AppError::coded(
                ErrorKind::NeedsVerification,
                "errors.session.theDiscardRanSavedButReading",
                "丢弃已执行并保存，最新状态读取失败：{cause}",
                &[],
            )
            .cause(error)
        })
    }

    /// 丢弃一个未跟踪文件：先保存完整内容到回收区，再删除文件（M4 F03.4）。
    ///
    /// 未跟踪文件没有 Git 基线，丢弃等于删除；安全边界要求先存回收区，
    /// 因此这里保存 `before` 内容、把 `after` 记为路径不存在，再删除。
    pub(crate) fn discard_untracked_locked(
        &self,
        path: &str,
        fingerprint: &str,
        store: &RecoveryStore,
    ) -> Result<ChangesSnapshot> {
        let status = status::read(&self.git)?;
        let entry = status
            .entries
            .into_iter()
            .find(|entry| entry.path == path && entry.changes == ChangesKind::Untracked)
            .ok_or_else(|| {
                AppError::coded(
                    ErrorKind::Rejected,
                    "errors.session.theFileChangedConfirmAgain12",
                    "文件已变化，请重新确认",
                    &[],
                )
            })?;
        if let Some(reason) = &entry.readonly_reason {
            return Err(reason.error());
        }
        // 未跟踪条目的指纹按「空 diff + 工作区字节」计算（刷新时尚未读取内容，见 refresh）；
        // 这里必须用同一份输入，否则会把同一文件误判为已变化。
        if compute_file_fingerprint_from_patch(&entry, &[], &self.path)? != fingerprint {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.session.theFileChangedConfirmAgain13",
                "文件已变化，请重新确认",
                &[],
            ));
        }
        let identity = WorktreeIdentity::capture(&self.git)?;
        store.ensure_outside(&identity)?;
        let paths = vec![entry.path.clone()];
        let before = recovery::capture_files(&self.path, &paths)?;
        let context = recovery::context_fingerprint(&self.git, &paths)?;
        let (record, _record_in_use) = store.save(
            identity,
            RecoverySource::DiscardFile,
            path,
            before.clone(),
            context,
        )?;
        let full = recovery::checked_path(&self.path, path)?;
        // 保存回收记录之后、删除之前复核内容：期间被外部写入的内容不能被删掉而不在回收记录里（返修 C1）。
        // NotFound 视为未被外部改动：回收记录已包含「路径不存在」的原状态。
        let current = recovery::capture_files(&self.path, &paths)?;
        if !recovery::unchanged(&before, &current) {
            store
                .finish(&record, &DiscardOutcome::NotApplied)
                .map_err(discard_pending)?;
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.session.theFileChangedConfirmAgain14",
                "文件已变化，请重新确认",
                &[],
            ));
        }
        match std::fs::remove_file(&full) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                store
                    .finish(&record, &DiscardOutcome::NotApplied)
                    .map_err(discard_pending)?;
                return Err(AppError::coded(
                    ErrorKind::Rejected,
                    "errors.session.failedDeleteUntrackedFile",
                    "删除未跟踪文件失败：{error}",
                    &[("error", format!("{}", error))],
                ));
            }
        }
        let after = vec![recovery::SavedFile {
            path: entry.path.clone(),
            content: None,
            executable: false,
        }];
        store
            .finish(&record, &DiscardOutcome::Completed { after })
            .map_err(discard_pending)?;
        self.refresh().map_err(|error| {
            AppError::coded(
                ErrorKind::NeedsVerification,
                "errors.session.theFileDeletedSavedButReading",
                "文件已删除并保存，最新状态读取失败：{cause}",
                &[],
            ).cause(error)
        })
    }

    // ---------- 内置三方合并（Q143，设计见 merge-design.md） ----------

    /// 未解决冲突的文件、两侧状态与指纹（冲突对话框用）。
    pub fn list_conflicts(&self) -> Result<Vec<merge::ConflictFile>> {
        merge::list_conflicts(&self.git)
    }

    /// 合并页签的数据：三个版本、按块分好的改动与初始结果。
    pub fn merge_layout(
        &self,
        data_dir: &Path,
        path: &str,
        auto_apply: bool,
    ) -> Result<merge::MergeLayout> {
        merge::build_layout(
            &self.git,
            data_dir,
            &self.id,
            path,
            auto_apply,
            language_for_path(path),
        )
    }

    /// “应用”合并结果：写前校验 → 存回收区 → 再核对 → 写工作区 → `git add` → 写后核对。
    pub fn merge_apply(
        &self,
        path: &str,
        result: &str,
        expected_fingerprint: &str,
        store: &RecoveryStore,
    ) -> Result<merge::MergeOutcome> {
        let _write = self.acquire_write_lock()?;
        let stages = merge::read_stages(&self.git, Some(path))?
            .remove(path)
            .ok_or_else(|| {
                AppError::coded(
                    ErrorKind::Rejected,
                    "errors.session.thisFileNoLongerUnresolvedConflicts",
                    "这个文件已经没有未解决的冲突，请刷新后重试",
                    &[],
                )
            })?;
        if merge::fingerprint(&self.git, path)? != expected_fingerprint {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.session.theFileChangedReopenMergeTab",
                "文件已变化，请重新打开合并页签",
                &[],
            ));
        }
        let ours = stages.ours.as_ref().ok_or_else(|| {
            AppError::coded(
                ErrorKind::Rejected,
                "errors.session.thisFileDeletedYourVersionYou",
                "你的版本里已删除这个文件，只能接受一侧",
                &[],
            )
        })?;
        let full = recovery::checked_path(&self.path, path)?;
        if std::fs::symlink_metadata(&full).is_ok_and(|meta| !meta.is_file()) {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.session.thisPathRegularFileWorkingTree",
                "工作区里这个路径不是普通文件，只能接受一侧",
                &[],
            ));
        }
        // 换行风格保持与你的版本一致（设计第 4 节）。
        let ours_bytes = self
            .git
            .run_ok(&["cat-file", "blob", &ours.oid], "读取你的版本")?
            .stdout;
        let bytes = merge::result_bytes(result, ours_bytes.windows(2).any(|pair| pair == b"\r\n"));

        let identity = WorktreeIdentity::capture(&self.git)?;
        store.ensure_outside(&identity)?;
        let paths = vec![path.to_string()];
        let before = recovery::capture_files(&self.path, &paths)?;
        let context = recovery::context_fingerprint(&self.git, &paths)?;
        let (record, _record_in_use) = store.save(
            identity,
            RecoverySource::MergeApply,
            path,
            before.clone(),
            context,
        )?;
        // 保存之后、写入之前再核对：期间被外部改动的内容不能被覆盖（同返修 C1）。
        let still_same = merge::fingerprint(&self.git, path)
            .is_ok_and(|current| current == expected_fingerprint)
            && recovery::capture_files(&self.path, &paths)
                .is_ok_and(|now| recovery::unchanged(&before, &now));
        if !still_same {
            store
                .finish(&record, &DiscardOutcome::NotApplied)
                .map_err(discard_pending)?;
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.session.theFileChangedReopenMergeTab2",
                "文件已变化，请重新打开合并页签",
                &[],
            ));
        }
        // 写已有文件时截断重写，可执行位不变；文件不在时按你的版本的模式创建。
        let existed = full.exists();
        std::fs::write(&full, &bytes).map_err(merge_pending)?;
        if !existed && ours.mode == "100755" {
            set_executable(&full).map_err(merge_pending)?;
        }
        // 用户明确点了“应用”：`git add` 标记为已解决（Q82）。
        let output = self
            .git
            .run(&["--literal-pathspecs", "add", "--", path])
            .map_err(merge_pending)?;
        if output.timed_out || output.exit_code != 0 {
            return Err(merge_pending(merge_git_error(&output)));
        }
        // 写后核对：不再是未解决冲突；工作区就是写入的结果；暂存区对象等于按属性计算的工作区对象。
        let resolved =
            merge::read_stages(&self.git, Some(path)).is_ok_and(|stages| stages.is_empty());
        let written = std::fs::read(&full).is_ok_and(|now| now == bytes);
        let staged = merge::worktree_blob_id(&self.git, path)
            .is_ok_and(|blob| merge::index_blob_id(&self.git, path) == Some(blob));
        if !(resolved && written && staged) {
            return Err(merge_pending(merge_verification_error(path)));
        }
        let after = recovery::capture_files(&self.path, &paths).map_err(merge_pending)?;
        store
            .finish(&record, &DiscardOutcome::Completed { after })
            .map_err(merge_pending)?;
        self.refresh().map_err(|error| {
            AppError::coded(
                ErrorKind::NeedsVerification,
                "errors.session.theMergeResultWrittenSavedBut",
                "合并结果已写入并保存，最新状态读取失败：{cause}",
                &[],
            )
            .cause(error)
        })?;
        Ok(merge::MergeOutcome {
            path: path.to_string(),
            recovery_record_id: Some(record.id.clone()),
        })
    }

    /// 整文件接受一侧（可多选）：`git checkout --ours|--theirs` 后 `git add`；
    /// 那一侧删除了文件时用 `git rm`。每个文件都写前校验、先存回收区、写后核对。
    pub fn merge_accept_side(
        &self,
        targets: &[(String, String)],
        side: merge::MergeSide,
        store: &RecoveryStore,
    ) -> Result<Vec<merge::MergeOutcome>> {
        let _write = self.acquire_write_lock()?;
        // 整批先核对：任何一个文件变了就一个都不写。
        for (path, fingerprint) in targets {
            if merge::fingerprint(&self.git, path)? != *fingerprint {
                return Err(AppError::coded(
                    ErrorKind::Rejected,
                    "errors.session.hasChangedConfirmAgain",
                    "{path} 已变化，请重新确认",
                    &[("path", format!("{}", path))],
                ));
            }
        }
        let mut outcomes = Vec::new();
        for (path, fingerprint) in targets {
            let done = |outcomes: &Vec<merge::MergeOutcome>| {
                outcomes
                    .iter()
                    .map(|outcome| outcome.path.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let stages = merge::read_stages(&self.git, Some(path))?
                .remove(path.as_str())
                .ok_or_else(|| {
                    AppError::coded(
                        ErrorKind::Rejected,
                        "errors.session.noLongerUnresolvedConflicts",
                        "{path} 已经没有未解决的冲突",
                        &[("path", format!("{}", path))],
                    )
                })?;
            let expected_blob = merge::side_blob_id(&stages, side);
            let identity = WorktreeIdentity::capture(&self.git)?;
            store.ensure_outside(&identity)?;
            let paths = vec![path.clone()];
            let before = recovery::capture_files(&self.path, &paths)?;
            let context = recovery::context_fingerprint(&self.git, &paths)?;
            let (record, _record_in_use) = store.save(
                identity,
                RecoverySource::MergeApply,
                path,
                before.clone(),
                context,
            )?;
            let still_same = merge::fingerprint(&self.git, path)
                .is_ok_and(|current| current == *fingerprint)
                && recovery::capture_files(&self.path, &paths)
                    .is_ok_and(|now| recovery::unchanged(&before, &now));
            if !still_same {
                store
                    .finish(&record, &DiscardOutcome::NotApplied)
                    .map_err(discard_pending)?;
                if outcomes.is_empty() {
                    return Err(AppError::coded(
                        ErrorKind::Rejected,
                        "errors.session.hasChangedWrittenNothing",
                        "{path} 已变化，没有写入",
                        &[("path", format!("{}", path))],
                    ));
                }
                return Err(AppError::coded(
                    ErrorKind::Rejected,
                    "errors.session.hasChangedWrittenAlreadyCompleted",
                    "{path} 已变化，没有写入；已完成：{done}",
                    &[("done", done(&outcomes)), ("path", format!("{}", path))],
                ));
            }
            let output = if expected_blob.is_some() {
                let checkout = self
                    .git
                    .run(&[
                        "--literal-pathspecs",
                        "checkout",
                        merge::checkout_flag(side),
                        "--",
                        path,
                    ])
                    .map_err(merge_pending)?;
                if checkout.timed_out || checkout.exit_code != 0 {
                    return Err(merge_pending(merge_git_error(&checkout)));
                }
                self.git
                    .run(&["--literal-pathspecs", "add", "--", path])
                    .map_err(merge_pending)?
            } else {
                self.git
                    .run(&["--literal-pathspecs", "rm", "--quiet", "--", path])
                    .map_err(merge_pending)?
            };
            if output.timed_out || output.exit_code != 0 {
                return Err(merge_pending(merge_git_error(&output)));
            }
            // 写后核对：不再冲突；暂存区就是那一侧的对象，或那一侧删除后路径已不在。
            let resolved =
                merge::read_stages(&self.git, Some(path)).is_ok_and(|stages| stages.is_empty());
            let index_ok = merge::index_blob_id(&self.git, path) == expected_blob;
            let worktree_ok =
                expected_blob.is_some() || std::fs::symlink_metadata(self.path.join(path)).is_err();
            if !(resolved && index_ok && worktree_ok) {
                return Err(merge_pending(merge_verification_error(path)));
            }
            let after = recovery::capture_files(&self.path, &paths).map_err(merge_pending)?;
            store
                .finish(&record, &DiscardOutcome::Completed { after })
                .map_err(merge_pending)?;
            outcomes.push(merge::MergeOutcome {
                path: path.clone(),
                recovery_record_id: Some(record.id.clone()),
            });
        }
        self.refresh().map_err(|error| {
            AppError::coded(
                ErrorKind::NeedsVerification,
                "errors.session.aSideAcceptedSavedButReading",
                "已接受一侧并保存，最新状态读取失败：{cause}",
                &[],
            )
            .cause(error)
        })?;
        Ok(outcomes)
    }

    /// 从回收记录恢复完整工作区状态（Q29、Q85、ADR-0010）。
    ///
    /// 复用同 worktree 的非排队写锁；每个路径发布前重新读取**真实**身份、全部相关状态
    /// 与未解决冲突，只写工作区，不碰暂存区，也不重放反向补丁。
    pub fn restore_record(
        &self,
        id: &str,
        expected: &recovery::RestoreExpectation,
        store: &RecoveryStore,
    ) -> Result<recovery::RestoreOutcome> {
        self.restore_record_with_hook(id, expected, store, |_| Ok(()))
    }

    /// 带受控阶段接缝的恢复（正常调用传入空钩子）。
    pub fn restore_record_with_hook(
        &self,
        id: &str,
        expected: &recovery::RestoreExpectation,
        store: &RecoveryStore,
        hook: impl FnMut(recovery::RestorePhase) -> Result<()>,
    ) -> Result<recovery::RestoreOutcome> {
        let _write = self.acquire_write_lock()?;
        let identity = WorktreeIdentity::capture(&self.git)?;
        store.ensure_outside(&identity)?;
        // 每次发布前都重新捕获：不把这里的旧身份当作发布时的真实身份（Q80、Q82、Q87）。
        let capture = |paths: &[String]| -> Result<recovery::RestorePreconditions> {
            let identity = WorktreeIdentity::capture(&self.git)?;
            let files = recovery::capture_files(&self.path, paths)?;
            let status = status::read(&self.git)?;
            let conflicts = status
                .entries
                .iter()
                .filter(|entry| entry.unmerged)
                .map(|entry| entry.path.clone())
                .collect::<Vec<_>>();
            Ok(recovery::RestorePreconditions {
                identity,
                files,
                conflicts,
            })
        };
        store.restore_with_hook(id, &self.path, expected, capture, hook)
    }

    /// 「保存版本 vs 当前内容」的只读 diff（由 Rust 侧真实 Git 生成）。
    pub fn recovery_text_diff(
        &self,
        id: &str,
        path: &str,
        store: &RecoveryStore,
    ) -> Result<String> {
        store.text_diff(&self.git, id, path, Some(&self.path))
    }

    /// 恢复的只读检查（不写工作区）。
    pub fn inspect_record(
        &self,
        id: &str,
        store: &RecoveryStore,
    ) -> Result<recovery::RecoveryInspection> {
        // 身份捕获失败（目录被移动/删除、Git 元数据缺失）不能让详情不可用：
        // 回退为「当前侧不可用」，保存版本照常展示（Q87）。
        match WorktreeIdentity::capture(&self.git) {
            Ok(identity) => store.inspect(id, Some(&identity), Some(&self.path)),
            Err(_) => store.inspect(id, None, None),
        }
    }

    /// 工作区文件的轻量快照（大小 + 纳秒 mtime + inode），用于判断一次读取期间文件是否变化。
    fn worktree_stats(&self, paths: &[&str]) -> Vec<Option<(u64, i128, u64)>> {
        paths
            .iter()
            .map(|path| {
                let full = self.path.join(path);
                let meta = std::fs::symlink_metadata(&full).ok()?;
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|duration| duration.as_nanos() as i128)
                    .unwrap_or(-1);
                #[cfg(unix)]
                let inode = {
                    use std::os::unix::fs::MetadataExt;
                    meta.ino()
                };
                #[cfg(not(unix))]
                let inode = 0u64;
                Some((meta.len(), mtime, inode))
            })
            .collect()
    }

    /// 计算当前快照的审阅状态：去重路径、仍然有效的标记、进度。
    ///
    /// 有效性判定用**HEAD 版本 + 工作区版本**：任一变化即失效（Q20、T12）。
    /// 只暂存/取消暂存不改变这两个版本，因此标记保留。
    fn review_state(
        &self,
        identity: &WorktreeIdentity,
        entries: &[ChangeEntry],
    ) -> Result<(
        ReviewMarks,
        HashMap<String, ReviewVersions>,
        std::collections::HashSet<String>,
    )> {
        let mut paths: Vec<String> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut versions: HashMap<String, ReviewVersions> = HashMap::new();
        let mut unconfirmed: std::collections::HashSet<String> = std::collections::HashSet::new();
        for entry in entries {
            if seen.insert(entry.path.clone()) {
                paths.push(entry.path.clone());
            }
            // 同一文件跨组共享：两组的 head/worktree 版本相同，取一次即可。
            versions
                .entry(entry.path.clone())
                .or_insert_with(|| ReviewVersions {
                    head: entry.head_version.clone(),
                    worktree: entry.worktree_version.clone(),
                });
            if !entry.worktree_version_confirmed {
                unconfirmed.insert(entry.path.clone());
            }
        }

        let store = self.review_store.as_ref();
        let mut reviewed: Vec<String> = Vec::new();
        let mut marks_out: Vec<review::ReviewMark> = Vec::new();
        if let Some(store) = store {
            // 只清理确实失效的标记；有效的保留。
            // 存储损坏/不可读/清理写入失败都如实向上报错，不能伪装成「全部未审阅」（Q36）。
            let (valid, removed) = store.retain_valid(identity, |mark| {
                // 版本无法确认的路径一律不可认领。
                !unconfirmed.contains(&mark.path)
                    && versions
                        .get(&mark.path)
                        .is_some_and(|current| *current == mark.versions)
            })?;
            // 失效也是一次改变真值的事件：推进可比较的进度，
            // 让「失效之前发出的成功回执」无法再覆盖失效后的真值（ST07-R10-3 / SP07-R10-3）。
            if removed {
                self.bump_review_seq()?;
            }
            let reviewed_set: std::collections::HashSet<&str> =
                valid.iter().map(|mark| mark.path.as_str()).collect();
            reviewed = paths
                .iter()
                .filter(|path| reviewed_set.contains(path.as_str()))
                .cloned()
                .collect();
            // 只报**仍然有效**的标记，并带上它自己实际绑定的版本：
            // 前端记账用这些版本，不能拿界面当前条目的版本补（ST07-R9-3）。
            marks_out = paths
                .iter()
                .filter_map(|path| valid.iter().find(|mark| mark.path == *path).cloned())
                .collect();
        }
        let seq = *self.review_seq.lock().map_err(|_| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.session.reviewSequenceLockFailed",
                "审阅序号锁失败",
                &[],
            )
        })?;
        let marks = ReviewMarks {
            worktree_id: self.id.clone(),
            paths: paths.clone(),
            reviewed: reviewed.clone(),
            total: paths.len() as u32,
            reviewed_count: reviewed.len() as u32,
            seq,
            marks: marks_out,
        };
        Ok((marks, versions, unconfirmed))
    }

    /// 推进并返回审阅的真实提交序号。
    ///
    /// 所有改变审阅真值的事件（新增 / 取消 / 失效清理）都必须经过它，
    /// 这样序号大小始终等于磁盘上的持久化先后（ST07-R9-1、ST07-R10-3）。
    fn bump_review_seq(&self) -> Result<u64> {
        let mut seq = self.review_seq.lock().map_err(|_| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.session.reviewSequenceLockFailed2",
                "审阅序号锁失败",
                &[],
            )
        })?;
        *seq += 1;
        Ok(*seq)
    }

    /// 当前仍然成立的身份；不成立（同路径重建、路径消失、标识未知）时返回 None。
    fn confirmed_identity(&self) -> Option<WorktreeIdentity> {
        self.identity.as_ref()?.revalidate(&self.git)
    }

    /// 提交草稿归属用的身份（M3 F02.5）：
    ///
    /// - 身份成立：返回当前重新确认过的身份；
    /// - 打开会话时就没能捕获身份，或现在已不成立（同路径重建、移动）：显式报错，
    ///   不从同路径的旧身份继承，也不保存新草稿（与审阅标记同一边界，Q87）。
    pub fn captured_identity(&self) -> Result<Option<WorktreeIdentity>> {
        match &self.identity {
            Some(identity) => Ok(identity.revalidate(&self.git)),
            None => Err(AppError::coded(
                ErrorKind::Environment,
                "errors.session.theWorktreeIdentityUnavailableReadWrite",
                "worktree 身份不可用，不能读写提交草稿",
                &[],
            )),
        }
    }

    /// Never adopt a replacement repository through an existing session.
    pub fn require_identity(&self) -> Result<WorktreeIdentity> {
        self.confirmed_identity().ok_or_else(|| AppError::coded(
            ErrorKind::Rejected, "errors.session.worktreeIdentityChanged",
            "worktree 身份已变化，请重新打开仓库后再操作", &[],
        ))
    }

    /// 身份不可用时的审阅状态：全部未审阅，进度按去重路径统计。
    fn empty_review(&self, entries: &[ChangeEntry]) -> ReviewMarks {
        let mut paths: Vec<String> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for entry in entries {
            if seen.insert(entry.path.clone()) {
                paths.push(entry.path.clone());
            }
        }
        let seq = self.review_seq.lock().map(|seq| *seq).unwrap_or_default();
        ReviewMarks {
            worktree_id: self.id.clone(),
            total: paths.len() as u32,
            paths,
            reviewed: Vec::new(),
            reviewed_count: 0,
            seq,
            marks: Vec::new(),
        }
    }

    /// 切换某个文件的审阅标记（勾选框与 ⌥⌘R 共用）。
    ///
    /// 返回更新后的审阅进度；持久化失败时返回错误，不会伪称保存成功。
    pub fn set_review_mark(
        &self,
        path: &str,
        marked: bool,
        expected_versions: Option<ReviewVersions>,
    ) -> Result<ReviewMarks> {
        let store = self.review_store.as_ref().ok_or_else(|| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.session.theReviewMarkStoreUnavailable",
                "审阅标记存储不可用",
                &[],
            )
        })?;
        let _read = self.read_lock.lock().map_err(|_| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.session.readLockFailed3",
                "读取锁失败",
                &[],
            )
        })?;
        let (versions, snapshot_paths, snapshot_versions, unconfirmed, current_review, confirmed) = {
            let guard = self.snapshot.lock().map_err(|_| {
                AppError::coded(
                    ErrorKind::Internal,
                    "errors.session.statusLockFailed9",
                    "状态锁失败",
                    &[],
                )
            })?;
            let snapshot = guard.as_ref().ok_or_else(|| {
                AppError::coded(
                    ErrorKind::Rejected,
                    "errors.session.readRepositoryStatusFirst7",
                    "请先读取仓库状态",
                    &[],
                )
            })?;
            (
                snapshot.versions.get(path).cloned(),
                snapshot.review.paths.clone(),
                snapshot.versions.clone(),
                snapshot.unconfirmed.clone(),
                snapshot.review.clone(),
                !snapshot.unconfirmed.contains(path),
            )
        };
        let versions = versions.ok_or_else(|| {
            AppError::coded(
                ErrorKind::Rejected,
                "errors.session.theFileCurrentChangeList2",
                "文件不在当前改动列表中",
                &[],
            )
        })?;
        // 版本无法确认的路径：不能新增认领（Q20）。取消已有标记仍然允许，
        // 否则用户无法清理旧记录（ST07-R3-1、SP07-R3-1）。
        if marked && !confirmed {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.session.thisFileSVersionConfirmedMarked",
                "这个文件的版本无法确认，不能标记为已审阅",
                &[],
            ));
        }
        // 用户看到的版本必须与执行时一致：刷新先发布了新版本时拒绝，
        // 不能把用户没看过的内容持久标成已审阅（ST07-3、SP07-5）。
        if let Some(expected) = &expected_versions {
            if expected != &versions {
                return Err(AppError::coded(
                    ErrorKind::Rejected,
                    "errors.session.theFileChangedConfirmAgainBefore",
                    "文件已变化，请重新确认后再标记",
                    &[],
                ));
            }
        }
        let identity = self.confirmed_identity().ok_or_else(|| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.session.theWorktreeIdentityAmbiguousSaveReview",
                "worktree 身份不明确，无法保存审阅标记",
                &[],
            )
        })?;
        let saved = store.set_mark(&identity, path, versions.clone(), marked)?;
        // 真实提交序号：在 `read_lock` 内、写入成功之后递增，因此序号大小
        // 与磁盘上的持久化先后严格一致（ST07-R9-1 / SP07-R9-3）。
        let seq = self.bump_review_seq()?;

        // 更新内存快照里的审阅状态，避免为一次勾选重跑整轮 diff。
        let mut guard = self.snapshot.lock().map_err(|_| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.session.statusLockFailed10",
                "状态锁失败",
                &[],
            )
        })?;
        if let Some(snapshot) = guard.as_mut() {
            let mut reviewed = snapshot.review.reviewed.clone();
            if marked {
                if !reviewed.iter().any(|item| item == path) {
                    reviewed.push(path.to_string());
                }
            } else {
                reviewed.retain(|item| item != path);
            }
            // 保持界面显示顺序。
            let ordered: Vec<String> = snapshot_paths
                .iter()
                .filter(|item| reviewed.iter().any(|kept| kept == *item))
                .cloned()
                .collect();
            let reviewed_set: std::collections::HashSet<&str> =
                ordered.iter().map(String::as_str).collect();
            for entry in &mut snapshot.entries {
                entry.reviewed = reviewed_set.contains(entry.path.as_str());
            }
            // 每条标记都带它**实际保存的版本**：聚合路径的版本来源只能是这里，
            // 不能是请求时快照或界面当前条目（ST07-R9-3 / SP07-R9-2）。
            let ordered_set: std::collections::HashSet<&str> =
                ordered.iter().map(String::as_str).collect();
            // 回执必须是**完整**的已确认真值：对当前改动列表里的每个可确认路径，
            // 都给出它这一刻的 reviewed（true/false）与执行时绑定的版本
            // （ST07-R11-1 / SP07-R11-2）。
            //
            // 只报正标记会让「聚合回执里某个路径已经未审」这件事丢失：
            // B 的取消回执被暂缓时，A 的回执只含正标记 A，前端仍以为 B 已审，
            // 即使 B 的取消已经持久化。取消目标同理（ST07-R10-1）。
            let marks_out: Vec<review::ReviewMark> = snapshot_paths
                .iter()
                .filter(|item| !unconfirmed.contains(*item))
                .filter_map(|item| {
                    let versions = snapshot_versions.get(item)?.clone();
                    Some(review::ReviewMark {
                        path: item.clone(),
                        reviewed: ordered_set.contains(item.as_str()),
                        versions,
                    })
                })
                .collect();
            // `saved` 只用于确认写入成功；真值以当前快照 + 版本表为准。
            drop(saved);
            snapshot.review = ReviewMarks {
                worktree_id: self.id.clone(),
                paths: snapshot_paths,
                reviewed_count: ordered.len() as u32,
                total: current_review.total,
                reviewed: ordered,
                seq,
                marks: marks_out,
            };
            return Ok(snapshot.review.clone());
        }
        Ok(ReviewMarks {
            seq,
            ..current_review
        })
    }

    fn hunk_actions(&self, entry: &ChangeEntry, file: &git::diff::FileChange) -> Vec<HunkAction> {
        if !supports_hunk_actions(entry, file) {
            return Vec::new();
        }
        if file.hunks.iter().any(|hunk| {
            !file
                .raw_patch
                .get(hunk.raw_range.clone())
                .is_some_and(|raw| raw.starts_with(b"@@ "))
        }) {
            return Vec::new();
        }
        file.hunks
            .iter()
            .enumerate()
            .map(|(index, hunk)| {
                let hunk_id = git::fingerprint(&[
                    &self.instance_id.to_le_bytes(),
                    self.id.as_bytes(),
                    entry.path.as_bytes(),
                    format!("{:?}", entry.changes).as_bytes(),
                    entry.fingerprint.as_bytes(),
                    entry.snapshot_revision.as_bytes(),
                    &(index as u64).to_le_bytes(),
                    &file.raw_patch[hunk.raw_range.clone()],
                ]);
                HunkAction {
                    worktree_id: self.id.clone(),
                    path: entry.path.clone(),
                    changes: entry.changes,
                    fingerprint: entry.fingerprint.clone(),
                    snapshot_revision: entry.snapshot_revision.clone(),
                    hunk_id,
                }
            })
            .collect()
    }

    fn hunk_patch(
        &self,
        path: &str,
        expected_fp: &str,
        hunk_id: &str,
        is_stage: bool,
    ) -> Result<Vec<u8>> {
        let guard = self.snapshot.lock().map_err(|_| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.session.statusLockFailed11",
                "状态锁失败",
                &[],
            )
        })?;
        let snapshot = guard.as_ref().ok_or_else(|| {
            AppError::coded(
                ErrorKind::Rejected,
                "errors.session.readRepositoryStatusFirst8",
                "请先读取仓库状态",
                &[],
            )
        })?;
        let changes = if is_stage {
            ChangesKind::Unstaged
        } else {
            ChangesKind::Staged
        };
        let entry = snapshot
            .entries
            .iter()
            .find(|entry| {
                entry.path == path && entry.changes == changes && entry.fingerprint == expected_fp
            })
            .ok_or_else(|| {
                AppError::coded(
                    ErrorKind::Rejected,
                    "errors.session.theFileChangedConfirmAgain15",
                    "文件已变化，请重新确认",
                    &[],
                )
            })?;
        let cached = snapshot.models.get(&key(changes, path)).ok_or_else(|| {
            AppError::coded(
                ErrorKind::Rejected,
                "errors.session.theFileChangedConfirmAgain16",
                "文件已变化，请重新确认",
                &[],
            )
        })?;
        if !supports_hunk_actions(entry, &cached.model) {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.session.thisFileDoesSupportHunkOperations",
                "这个文件不支持 hunk 操作",
                &[],
            ));
        }
        let index = cached
            .hunk_actions
            .iter()
            .position(|action| {
                action.hunk_id == hunk_id
                    && action.fingerprint == expected_fp
                    && action.snapshot_revision == snapshot.generation.to_string()
            })
            .ok_or_else(|| {
                AppError::coded(
                    ErrorKind::Rejected,
                    "errors.session.theFileChangedConfirmAgain17",
                    "文件已变化，请重新确认",
                    &[],
                )
            })?;
        cached.model.hunk_patch(index).ok_or_else(|| {
            AppError::coded(
                ErrorKind::Rejected,
                "errors.session.aPatchGeneratedHunkRefreshConfirm",
                "当前 hunk 无法生成补丁，请刷新后重新确认",
                &[],
            )
        })
    }

    /// 从当前缓存的 hunk 按段切开补丁（返修 A1/Q142）。
    ///
    /// `forward`：暂存（true，保留 +/-）；丢弃与取消暂存用反向（false）。
    /// 段来自显示时同批签发的 hunk_index/segment_index；模型缓存被刷新后
    /// hunk 可能偏移，切不出补丁或与显示不一致时由调用方拒绝。
    fn segment_patch(
        &self,
        path: &str,
        segment: &SegmentOverride,
        forward: bool,
    ) -> Result<Vec<u8>> {
        let guard = self.snapshot.lock().map_err(|_| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.session.statusLockFailed12",
                "状态锁失败",
                &[],
            )
        })?;
        let snapshot = guard.as_ref().ok_or_else(|| {
            AppError::coded(
                ErrorKind::Rejected,
                "errors.session.readRepositoryStatusFirst9",
                "请先读取仓库状态",
                &[],
            )
        })?;
        let entry = snapshot
            .entries
            .iter()
            .find(|entry| entry.path == path && entry.changes == ChangesKind::Unstaged)
            .ok_or_else(|| {
                AppError::coded(
                    ErrorKind::Rejected,
                    "errors.session.theFileChangedConfirmAgain18",
                    "文件已变化，请重新确认",
                    &[],
                )
            })?;
        // 忽略空白档位下：段编号来自带档位的 diff，补丁必须从同一档位的 hunk 切出
        // （Q140）。普通档位直接用刷新时的缓存模型。
        if entry.original_path.is_some() {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.session.renamedFilesOnlyHandledAsWhole2",
                "改名的文件只能整个文件操作",
                &[],
            ));
        }
        let model = if matches!(segment.ignore_whitespace, IgnoreWhitespace::Off) {
            snapshot
                .models
                .get(&key(ChangesKind::Unstaged, path))
                .map(|cached| cached.model.clone())
        } else {
            Some(self.read_ignore_model(
                path,
                ChangesKind::Unstaged,
                None,
                segment.ignore_whitespace,
            )?)
        };
        let model = model.ok_or_else(|| {
            AppError::coded(
                ErrorKind::Rejected,
                "errors.session.theFileChangedConfirmAgain19",
                "文件已变化，请重新确认",
                &[],
            )
        })?;
        segments::rewrite_hunk_for_segment(
            &model,
            segment.hunk_index as usize,
            segment.segment_index,
            forward,
        )
        .ok_or_else(|| {
            AppError::coded(
                ErrorKind::Rejected,
                "errors.session.aPatchGeneratedBlockRefreshConfirm3",
                "当前块无法生成补丁，请刷新后重新确认",
                &[],
            )
        })
    }

    /// 统一的写操作执行入口：锁重试在操作层进行，每次重试前重新读取状态并核对指纹（审查意见 1、2、7）。
    fn execute_staging_op(
        &self,
        path: &str,
        expected_fp: &str,
        is_stage: bool,
        hunk_id: Option<&str>,
    ) -> Result<ChangesSnapshot> {
        let _guard = self.acquire_write_lock()?;
        self.execute_staging_op_locked(path, expected_fp, is_stage, hunk_id)
    }

    /// 已持有同 worktree 写锁时的暂存 / 取消暂存实现（批量操作用）。
    pub(crate) fn execute_staging_op_locked(
        &self,
        path: &str,
        expected_fp: &str,
        is_stage: bool,
        hunk_id: Option<&str>,
    ) -> Result<ChangesSnapshot> {
        let mut last_lock_err: Option<String> = None;
        const LOCK_RETRIES: usize = 3;
        const LOCK_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(100);

        for attempt in 0..=LOCK_RETRIES {
            // 每次尝试（包含重试）前，都必须重新读取真实状态并核对目标当前版本！（审查意见 2）
            let status = status::read(&self.git)?;
            let entry = if is_stage {
                status.entries.iter().find(|e| {
                    e.path == path
                        && (e.changes == ChangesKind::Unstaged
                            || e.changes == ChangesKind::Untracked)
                })
            } else {
                status
                    .entries
                    .iter()
                    .find(|e| e.path == path && e.changes == ChangesKind::Staged)
            };

            let entry = entry.ok_or_else(|| {
                AppError::coded(
                    ErrorKind::Rejected,
                    "errors.session.theFileChangedConfirmAgain20",
                    "文件已变化，请重新确认",
                    &[],
                )
            })?;

            if let Some(reason) = &entry.readonly_reason {
                return Err(reason.error());
            }

            // 检查工作区符号链接或嵌套仓库
            let full_path = self.path.join(path);
            if full_path.is_symlink() {
                return Err(AppError::coded(
                    ErrorKind::Rejected,
                    "errors.session.symbolicLinksReadOnlyHereStaging",
                    "符号链接为只读类型，不支持在应用内暂存或取消暂存",
                    &[],
                ));
            }
            if entry.changes == ChangesKind::Untracked
                && (full_path.join(".git").exists() || path.ends_with('/'))
            {
                return Err(AppError::coded(
                    ErrorKind::Rejected,
                    "errors.session.nestedRepositoriesReadOnlyHereStaging",
                    "嵌套仓库为只读类型，不支持在应用内暂存",
                    &[],
                ));
            }

            // 针对该单文件读取当前 diff 并在写前核对指纹
            let diff_bytes = if is_stage {
                if entry.changes == ChangesKind::Untracked {
                    Vec::new()
                } else {
                    self.git.diff_worktree(Some(path))?
                }
            } else {
                self.git
                    .diff_index_with_orig(path, entry.original_path.as_deref())?
            };

            let current_fp = compute_file_fingerprint_from_patch(entry, &diff_bytes, &self.path)?;
            if current_fp != expected_fp {
                return Err(AppError::coded(
                    ErrorKind::Rejected,
                    "errors.session.theFileChangedConfirmAgain21",
                    "文件已变化，请重新确认",
                    &[],
                ));
            }

            // 执行单次写操作（run_once，不绕过核对进行隐式重试）
            let output = if let Some(hunk_id) = hunk_id {
                // 状态读取期间可能刷新过快照；在真正应用前才解析仍有效的原始目标。
                let patch = self.hunk_patch(path, expected_fp, hunk_id, is_stage)?;
                self.git.apply_index_patch(patch, !is_stage, false)?
            } else if is_stage {
                // 暂存只取工作区这一侧；`original_path` 只用于「取消暂存已识别的改名」。
                // 对 RM 的未暂存条目，原路径已不在工作区或暂存区里，一并传入会让
                // `git add -A --` 因 pathspec 不匹配而失败（整文件暂存改名条目时发现）。
                self.git.stage_file(path, None)?
            } else {
                self.git
                    .unstage_file(path, entry.original_path.as_deref())?
            };

            // 结果不确定时标为“结果待核对”（审查意见 1）
            if output.timed_out || output.exit_code < 0 {
                return Err(AppError::coded(
                    ErrorKind::NeedsVerification,
                    "errors.session.theGitCommandTimedOutInterrupted2",
                    "Git 命令执行超时或中断，写入结果待核对；请重新核对真实状态后再继续",
                    &[],
                ));
            }

            // 检查锁占用
            if git::is_lock_error(&output) {
                let err_text = output.stderr.trim().to_string();
                if attempt < LOCK_RETRIES {
                    last_lock_err = Some(err_text);
                    std::thread::sleep(LOCK_RETRY_DELAY * (attempt as u32 + 1));
                    continue; // 重新进入循环：重新读取状态并重新核对指纹！
                } else {
                    let detail = last_lock_err.unwrap_or(err_text);
                    return Err(AppError::coded(
                        ErrorKind::Rejected,
                        "errors.session.theGitIndexLockedAnotherProcess2",
                        "Git 索引被其他进程占用（index.lock），已重试 {LOCK_RETRIES} 次：{detail}",
                        &[
                            ("LOCK_RETRIES", format!("{}", LOCK_RETRIES)),
                            ("detail", format!("{}", detail)),
                        ],
                    ));
                }
            }

            if output.exit_code != 0 {
                let (code, template) = match (is_stage, hunk_id.is_some()) {
                    (true, true) => (
                        "errors.session.failedStageHunk2",
                        "暂存 hunk 失败（退出码 {code}）：{detail}",
                    ),
                    (false, true) => (
                        "errors.session.failedUnstageHunk2",
                        "取消暂存 hunk 失败（退出码 {code}）：{detail}",
                    ),
                    (true, false) => (
                        "errors.session.failedStageFile",
                        "暂存文件失败（退出码 {code}）：{detail}",
                    ),
                    (false, false) => (
                        "errors.session.failedUnstageFile",
                        "取消暂存文件失败（退出码 {code}）：{detail}",
                    ),
                };
                return Err(AppError::coded(
                    ErrorKind::Git,
                    code,
                    template,
                    &[
                        ("code", format!("{}", output.exit_code)),
                        ("detail", format!("{}", output.stderr.trim())),
                    ],
                ));
            }

            // 执行成功，跳出重试循环
            break;
        }

        // 写入成功后重新读取状态；若读取失败，也属于结果待核对（审查意见 1）
        match self.refresh() {
            Ok(snap) => Ok(snap),
            Err(e) => Err(AppError::coded(
                ErrorKind::NeedsVerification,
                "errors.session.theFileModifiedButReadingLatest",
                "文件已操作但读取最新状态失败（结果待核对）：{cause}",
                &[],
            ).cause(e)),
        }
    }
}

/// 核对段凭证（Q140）：档位只能是可按块写的两档以内，且与签发时一致；返回文件指纹。
fn segment_fingerprint(credential: &str, ignore: IgnoreWhitespace) -> Result<&str> {
    if !ignore.allows_segment_actions() {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.session.theModeAlsoIgnoresBlankLines",
            "同时忽略空行的档位只用于查看；关闭忽略空白后才能按块操作",
            &[],
        ));
    }
    segments::credential_fingerprint(credential, ignore).ok_or_else(|| {
        AppError::coded(
            ErrorKind::Rejected,
            "errors.session.theIgnoreWhitespaceSettingChangedConfirm",
            "忽略空白的设置已变化，请重新确认这个块",
            &[],
        )
    })
}

/// 合并写入在回收区保存之后失败：结果待核对，不自动继续（Q91）。
fn merge_pending(error: impl Into<AppError>) -> AppError {
    AppError::coded(
        ErrorKind::NeedsVerification,
        "errors.session.theOriginalContentSavedRecoveryArea",
        "原内容已保存到回收区，合并结果待核对：{cause}",
        &[],
    ).cause(error.into())
}

fn merge_git_error(output: &git::GitOutput) -> AppError {
    AppError::coded(ErrorKind::Git, "errors.mod.failedExitCode", "Git 命令失败（退出码 {0}）：{1}", &[("0", output.exit_code.to_string()), ("1", output.stderr.trim().to_string())])
}

fn merge_verification_error(path: &str) -> AppError {
    AppError::coded(ErrorKind::NeedsVerification, "errors.session.mergeVerificationFailed", "写后核对没有通过：{path}", &[("path", path.to_string())])
}

#[cfg(unix)]
fn set_executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(permissions.mode() | 0o111);
    std::fs::set_permissions(path, permissions)
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

fn discard_pending(error: impl Into<AppError>) -> AppError {
    AppError::coded(
        ErrorKind::NeedsVerification,
        "errors.session.theContentSavedRecoveryAreaDiscard",
        "内容已保存到回收区，丢弃结果待核对；不会自动继续：{cause}",
        &[],
    ).cause(error.into())
}

fn supports_hunk_actions(entry: &ChangeEntry, file: &git::diff::FileChange) -> bool {
    // Q81：Git 识别的重命名只做整文件操作，不提供 hunk 操作。
    //
    // porcelain type-2 的 `RM`（已暂存改名 + 新路径上有未暂存修改）会同时出现
    // staged `R` 和 unstaged `M` 两个条目，二者都保留 `original_path`。
    // 只看 status 会把后者当成普通 M 而暴露 hunk；这里按 Git 给出的原路径元数据拒绝，
    // 既不猜测「删除 + 未跟踪」配对，也不影响真正的普通 M 文件。
    let renamed_by_git = entry.original_path.is_some() || file.old_path.is_some();
    entry.changes != ChangesKind::Untracked
        && entry.changes != ChangesKind::Changelist
        && !renamed_by_git
        && entry.readonly_reason.is_none()
        && !entry.unmerged
        && !entry.binary
        && !file.binary
        && !file.hunks.is_empty()
        && matches!(
            entry.status,
            FileStatus::Modified | FileStatus::Added | FileStatus::Deleted
        )
        && matches!(
            file.status,
            FileStatus::Modified | FileStatus::Added | FileStatus::Deleted
        )
}

fn fill_entry(entry: &mut ChangeEntry, model: Option<&git::diff::FileChange>) {
    let language = language_for_path(&entry.path);
    entry.language = language;
    match model {
        Some(file) => {
            entry.binary = file.binary;
            entry.added = file.added;
            entry.deleted = file.deleted;
            entry.hunk_count = file.hunks.len() as u32;
            let layout_summary = rows::layout_summary(file);
            entry.row_count_unified = layout_summary.unified_count as u32;
            entry.row_count_split = layout_summary.split_count as u32;
            entry.collapsed = is_collapsed(&entry.path, file);
            entry.hunks = layout_summary.anchors;
        }
        None => {
            entry.collapsed = looks_like_lock(&entry.path);
        }
    }
    entry.metrics_loaded = true;
}

fn key(changes: ChangesKind, path: &str) -> String {
    format!("{changes:?}/{path}")
}

/// 内容版本：由稳定文档键与该版本内容指纹确定，同一内容不重建解析状态（Q88）。
fn content_version_of(document_id: &str, entry_fingerprint: &str, content: Option<&str>) -> u64 {
    let mut hasher = Sha256::new();
    hasher.update(document_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(entry_fingerprint.as_bytes());
    hasher.update(b"\0");
    match content {
        Some(text) => {
            hasher.update(b"content");
            hasher.update((text.len() as u64).to_le_bytes());
            hasher.update(text.as_bytes());
        }
        None => hasher.update(b"missing"),
    }
    let digest = hasher.finalize();
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    u64::from_le_bytes(bytes)
}

fn count(entries: &[ChangeEntry], changes: ChangesKind) -> u32 {
    entries
        .iter()
        .filter(|entry| entry.changes == changes)
        .count() as u32
}

fn empty_file(path: &str, status: FileStatus) -> git::diff::FileChange {
    git::diff::FileChange {
        path: path.to_string(),
        old_path: None,
        status,
        binary: false,
        hunks: Vec::new(),
        added: 0,
        deleted: 0,
        old_lines: 0,
        new_lines: 0,
        raw_patch: Vec::new(),
    }
}

/// 默认收起规则：二进制、lock 文件、超大文件（Q32）。
fn is_collapsed(path: &str, file: &git::diff::FileChange) -> bool {
    if file.binary {
        return true;
    }
    if looks_like_lock(path) {
        return true;
    }
    file.old_lines.max(file.new_lines) > LARGE_FILE_LINES
}

/// HEAD 中的文件版本：用 Git 给出的 blob OID（新增/未跟踪文件为 None）。
///
/// 只暂存或取消暂存不改变 HEAD 的 OID，因此审阅标记仍然有效（Q20、T12）。
fn head_version_for(entry: &StatusEntry) -> Option<String> {
    let oid = entry.oid_head.as_deref()?;
    if oid.is_empty() || oid.bytes().all(|byte| byte == b'0') {
        return None;
    }
    Some(format!("blob:{}", oid))
}

/// 计算单个文件变更条目的 diff 指纹（Q80、ADR-0009）。
///
/// 指纹来自展示所用的同一份 Git 输出切片（diff_bytes），列表刷新时不启动额外 Git 进程。

fn compute_file_fingerprint_from_patch(
    entry: &StatusEntry,
    diff_bytes: &[u8],
    worktree_path: &Path,
) -> Result<String> {
    let mut hasher = Sha256::new();
    // 1. 变更组
    hasher.update((entry.changes as u8).to_le_bytes());
    // 2. 路径及原路径
    hasher.update((entry.path.len() as u64).to_le_bytes());
    hasher.update(entry.path.as_bytes());
    if let Some(orig) = &entry.original_path {
        hasher.update(b"orig:");
        hasher.update((orig.len() as u64).to_le_bytes());
        hasher.update(orig.as_bytes());
    }
    // 3. 状态代码
    hasher.update(entry.status.letter().as_bytes());
    // 4. Git 状态中的模式与 OID（避免平台分支散落）
    if let Some(m) = &entry.mode_head {
        hasher.update(m.as_bytes());
    }
    if let Some(m) = &entry.mode_index {
        hasher.update(m.as_bytes());
    }
    if let Some(m) = &entry.mode_worktree {
        hasher.update(m.as_bytes());
    }
    if let Some(oid) = &entry.oid_head {
        hasher.update(oid.as_bytes());
    }
    if let Some(oid) = &entry.oid_index {
        hasher.update(oid.as_bytes());
    }

    // 5. diff 输出（展示所用同一份输出）
    hasher.update((diff_bytes.len() as u64).to_le_bytes());
    hasher.update(diff_bytes);

    // Binary and mode-only diffs contain no content-bearing hunks. Bind their
    // actual worktree bytes too; textual hunks already bind the Git file version.
    let lacks_text_hunks = !diff_bytes.windows(4).any(|bytes| bytes == b"\n@@ ");
    if entry.changes == ChangesKind::Untracked
        || (matches!(entry.changes, ChangesKind::Unstaged | ChangesKind::Changelist) && lacks_text_hunks)
    {
        let file_path = worktree_path.join(&entry.path);
        let meta = match std::fs::symlink_metadata(&file_path) {
            Ok(meta) => Some(meta),
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && matches!(entry.changes, ChangesKind::Unstaged | ChangesKind::Changelist) =>
            {
                None
            }
            Err(error) => {
                return Err(AppError::coded(
                    ErrorKind::Rejected,
                    "errors.session.failedReadFileAttributes",
                    "读取文件属性失败（{0}）：{error}",
                    &[
                        ("0", format!("{:}", entry.path)),
                        ("error", format!("{}", error)),
                    ],
                ));
            }
        };
        hasher.update(b"worktree_meta");
        if let Some(meta) = meta {
            hasher.update(b"present");
            hasher.update(meta.len().to_le_bytes());

            if meta.is_file() {
                let mut f = std::fs::File::open(&file_path).map_err(|e| {
                    AppError::coded(
                        ErrorKind::Rejected,
                        "errors.session.failedOpenFile",
                        "打开文件失败（{0}）：{1}",
                        &[("0", format!("{:}", entry.path)), ("1", format!("{:}", e))],
                    )
                })?;
                use std::io::Read;
                let mut buf = Vec::new();
                f.read_to_end(&mut buf).map_err(|e| {
                    AppError::coded(
                        ErrorKind::Rejected,
                        "errors.session.failedReadFileContents",
                        "读取文件内容失败（{0}）：{1}",
                        &[("0", format!("{:}", entry.path)), ("1", format!("{:}", e))],
                    )
                })?;
                let file_hash = Sha256::digest(&buf);
                hasher.update(&file_hash);
            } else if meta.file_type().is_symlink() {
                let target = std::fs::read_link(&file_path).map_err(|e| {
                    AppError::coded(
                        ErrorKind::Rejected,
                        "errors.session.failedReadSymbolicLink",
                        "读取符号链接失败（{0}）：{1}",
                        &[("0", format!("{:}", entry.path)), ("1", format!("{:}", e))],
                    )
                })?;
                hasher.update(target.to_string_lossy().as_bytes());
            }
        } else {
            hasher.update(b"absent");
        }
    }

    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

pub const LARGE_FILE_LINES: u32 = 2000;

fn looks_like_lock(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    name.ends_with(".lock")
        || matches!(
            name.as_str(),
            "pnpm-lock.yaml" | "package-lock.json" | "yarn.lock"
        )
}

/// 扩展名 → 语法名（同时交给 syntect 与前端做提示）。
pub fn language_for_path(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    if looks_like_lock(path) {
        return "yaml".into();
    }
    let extension = name
        .rsplit_once('.')
        .map(|(_, ext)| ext)
        .unwrap_or(name.as_str());
    match extension {
        "ts" | "mts" | "cts" => "ts",
        "tsx" => "tsx",
        "js" | "mjs" | "cjs" => "js",
        "jsx" => "jsx",
        "vue" => "vue",
        "swift" => "swift",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "md" | "markdown" => "markdown",
        "sh" | "zsh" | "bash" => "shellscript",
        "rs" => "rust",
        "py" => "python",
        "css" => "css",
        "html" | "htm" => "html",
        "toml" => "toml",
        "go" => "go",
        "java" => "java",
        "kt" => "kotlin",
        "c" | "h" => "c",
        "cc" | "cpp" | "hpp" => "cpp",
        _ => "txt",
    }
    .to_string()
}

impl RepoEntry {
    pub fn build(path: &Path, command_log: Arc<git::CommandLog>) -> Result<Self> {
        let home = shell::home_dir()?;
        let shell_env = shell::resolve(&home, path);
        let git = Git::new(path, &shell_env, command_log)?;
        let toplevel = git.toplevel().ok_or_else(|| {
            AppError::coded(
                ErrorKind::Rejected,
                "errors.session.notGitRepository",
                "不是 Git 仓库：{0}",
                &[("0", format!("{:}", path.display()))],
            )
        })?;
        let worktrees = git.worktrees().unwrap_or_default();
        let main_path = worktrees
            .iter()
            .find(|entry| entry.is_main)
            .map(|entry| entry.path.clone())
            .unwrap_or_else(|| toplevel.display().to_string());
        let name = Path::new(&main_path)
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| main_path.clone());
        Ok(Self {
            id: id_for_path(&toplevel),
            path: toplevel.display().to_string(),
            name,
            exists: true,
            worktrees,
        })
    }
}

/// 仓库 / worktree 的 ID：规范路径的 SHA-256 前 16 位（稳定、可重复）。
pub fn id_for_path(path: &Path) -> String {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut hasher = Sha256::new();
    hasher.update(canonical.display().to_string().as_bytes());
    let digest = hasher.finalize();
    digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// 解析 `git worktree list --porcelain` 的输出。
pub fn parse_worktrees(text: &str) -> Vec<WorktreeEntry> {
    let mut entries: Vec<WorktreeEntry> = Vec::new();
    let mut current: Option<WorktreeEntry> = None;
    for line in text.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            let worktree_path = path.trim().to_string();
            current = Some(WorktreeEntry {
                id: id_for_path(Path::new(&worktree_path)),
                path: worktree_path,
                head: None,
                branch: None,
                bare: false,
                detached: false,
                is_main: entries.is_empty(),
            });
            continue;
        }
        let Some(entry) = current.as_mut() else {
            continue;
        };
        if let Some(head) = line.strip_prefix("HEAD ") {
            entry.head = Some(head.trim().to_string());
        } else if let Some(branch) = line.strip_prefix("branch ") {
            entry.branch = Some(
                branch
                    .trim()
                    .strip_prefix("refs/heads/")
                    .unwrap_or(branch.trim())
                    .to_string(),
            );
        } else if line.trim() == "bare" {
            entry.bare = true;
        } else if line.trim() == "detached" {
            entry.detached = true;
        }
    }
    if let Some(entry) = current.take() {
        entries.push(entry);
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::diff::FileChange;
    use crate::shell::EnvCaptureInfo;

    fn file(added: u32, deleted: u32, binary: bool) -> FileChange {
        FileChange {
            path: "a.ts".into(),
            old_path: None,
            status: FileStatus::Modified,
            binary,
            hunks: Vec::new(),
            added,
            deleted,
            old_lines: added + 3,
            new_lines: deleted + 3,
            raw_patch: Vec::new(),
        }
    }

    fn temp_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("repo")).unwrap();
        let status = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir.path().join("repo"))
            .status()
            .unwrap();
        assert!(status.success());
        dir
    }

    #[test]
    fn apply_environment_swaps_git_and_terminal_env() {
        let dir = temp_repo();
        let repo = dir.path().join("repo");
        let env = ShellEnv::minimal(&repo);
        let session = WorktreeSession::open(
            "wt-1",
            &repo,
            env,
            std::sync::Arc::new(crate::git::CommandLog::default()),
        )
        .unwrap();
        let mut resolved = ShellEnv::minimal(&repo);
        resolved.vars.insert("REVIA_TEST_PATH".into(), "/custom/bin".into());
        resolved.path = vec!["/custom/bin".into()];
        session.apply_environment(resolved);
        // hooks 与提交走 git 子进程环境（Q212）。
        let git_env = session.git.env.read().unwrap();
        assert!(git_env
            .iter()
            .any(|(key, value)| key == "REVIA_TEST_PATH" && value == "/custom/bin"));
        assert!(git_env
            .iter()
            .any(|(key, value)| key == "PATH" && value.contains("/custom/bin")));
        drop(git_env);
        // 内置终端用会话里的环境快照。
        assert_eq!(
            session
                .shell_env_snapshot()
                .vars
                .get("REVIA_TEST_PATH")
                .map(String::as_str),
            Some("/custom/bin")
        );
    }

    #[test]
    fn shell_check_exposes_structured_fallback_reason_and_raw_detail() {
        let env = ShellEnv {
            vars: HashMap::new(),
            path: Vec::new(),
            info: EnvCaptureInfo {
                worktree: "C:\\repo".into(),
                shell: "pwsh.exe".into(),
                elapsed_ms: 120,
                timed_out: false,
                used_fallback: true,
                var_count: 0,
                path_entries: Vec::new(),
                fallback_reason: Some("errors.environment.shellProfileFailed"),
                fallback_detail: Some("profile C:\\Users\\x\\profile.ps1: blocked".into()),
            },
        };
        let check = shell_environment_check(&env);
        assert!(!check.ok);
        let hint = check.hint.expect("回退应带结构化提示");
        assert_eq!(hint.code.as_deref(), Some("errors.environment.shellProfileFailed"));
        assert_eq!(
            hint.params.as_ref().and_then(|params| params.get("detail")).map(String::as_str),
            Some("profile C:\\Users\\x\\profile.ps1: blocked")
        );
    }

    #[test]
    fn id_is_stable_for_same_path() {
        let path = Path::new("/tmp");
        assert_eq!(id_for_path(path), id_for_path(path));
        assert_eq!(id_for_path(path).len(), 16);
    }

    #[test]
    fn language_comes_from_extension() {
        assert_eq!(language_for_path("src/app.ts"), "ts");
        assert_eq!(language_for_path("src/App.vue"), "vue");
        assert_eq!(language_for_path("Model.swift"), "swift");
        assert_eq!(language_for_path("pnpm-lock.yaml"), "yaml");
        assert_eq!(language_for_path("LICENSE"), "txt");
    }

    #[test]
    fn collapse_rules_cover_lock_binary_and_large() {
        assert!(is_collapsed("pnpm-lock.yaml", &file(1, 1, false)));
        assert!(is_collapsed("logo.png", &file(1, 1, true)));
        assert!(is_collapsed(
            "logs/app.log",
            &file(LARGE_FILE_LINES + 1, 0, false)
        ));
        assert!(!is_collapsed("src/app.ts", &file(10, 4, false)));
    }

    #[test]
    fn parses_main_and_linked_worktrees() {
        let text = "\
worktree /tmp/repo
HEAD abc123
branch refs/heads/main

worktree /tmp/other
HEAD def456
detached
";
        let entries = parse_worktrees(text);
        assert_eq!(entries.len(), 2);
        assert!(entries[0].is_main);
        assert_eq!(entries[0].branch.as_deref(), Some("main"));
        assert!(entries[1].detached);
        assert!(!entries[1].is_main);
    }
}
