//! 会话：登记过的仓库 / worktree，以及它们的只读状态与显示模型。
//!
//! 约束（评审调整 1）：
//! - 除 `add_repo` 外，所有命令都只接收 Rust 层登记过的 ID，不接收任意目录路径；
//! - 文件路径必须是当前 `list_changes` 结果里的条目，否则拒绝。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::error::{AppError, Result};
use crate::git::highlight::{HighlightRequest, HighlightResult, Highlighter};
use crate::git::rows::{self, HunkAction, HunkAnchor, RenderRow, RowKind, ViewMode};
use crate::git::status::{self, ChangesKind, FileStatus, ReadonlyReason, StatusEntry};
use crate::git::{self, Git};
use crate::shell::{self, ShellEnv};
use crate::recovery::{self, DiscardOutcome, DiscardPhase, RecoverySource, RecoveryStore, WorktreeIdentity};

static NEXT_SESSION_INSTANCE: AtomicU64 = AtomicU64::new(1);

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
}

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
    pub rows: Vec<RenderRow>,
    pub fingerprint: String,
    /// 生成这段行的快照版本；前端据此拒绝晚到的旧行（Q88、T05）。
    pub revision: u64,
    /// 该文件最新的列表元数据（未跟踪文件在此时才补全）。
    pub entry: ChangeEntry,
}

/// 环境检查项（Q36：环境问题在窗口顶部提示）。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EnvironmentCheck {
    pub name: String,
    pub ok: bool,
    pub detail: String,
    pub hint: Option<String>,
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
    /// 是否回退到了最小环境。
    pub used_fallback: bool,
    pub path_entries: u32,
    pub checks: Vec<EnvironmentCheck>,
}

/// 单个文件的解析结果缓存。
struct FileModelCache {
    model: git::diff::FileChange,
    unfiled: bool,
    hunk_actions: Vec<HunkAction>,
}

struct Snapshot {
    fingerprint: String,
    entries: Vec<ChangeEntry>,
    /// key: `<changes>/<path>`
    models: HashMap<String, FileModelCache>,
    /// 显示行排版缓存：key 为 `<changes>/<path>/<view>`。
    /// 排版与文本都在这里，`get_rows` 只做切片，不再每次重建。
    layouts: HashMap<String, Vec<RenderRow>>,
    generation: u64,
}

pub struct WorktreeSession {
    pub id: String,
    pub path: PathBuf,
    pub git: Git,
    pub shell_env: ShellEnv,
    pub info: WorktreeSessionInfo,
    snapshot: Mutex<Option<Snapshot>>,
    /// 状态读取串行锁：读取与发布必须成对，避免早开始的读取后发布、覆盖较新的快照（Q88）。
    read_lock: Mutex<()>,
    highlighter: Highlighter,
    /// 已经发给高亮线程的文件内容代际，避免重复传输内容（按稳定文档键 + 版本）。
    highlight_generations: Mutex<HashMap<String, HighlightStamp>>,
    generation_counter: Mutex<u64>,
    instance_id: u64,
    /// 是否有正在进行的写操作（同一 worktree 互斥，不排队）。
    writing: Mutex<bool>,
}

/// 发给高亮线程的内容版本记录：同一文档只在版本不同时重发内容。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HighlightStamp {
    revision: u64,
    content_version: u64,
}

/// 写操作互斥 RAII Guard，离开作用域自动释放写锁。
#[derive(Debug)]
pub struct WriteGuard<'a>(&'a Mutex<bool>);
impl<'a> Drop for WriteGuard<'a> {
    fn drop(&mut self) {
        if let Ok(mut writing) = self.0.lock() {
            *writing = false;
        }
    }
}

impl WorktreeSession {
    pub fn open(id: &str, path: &Path, shell_env: ShellEnv, command_log: Arc<git::CommandLog>) -> Result<Arc<Self>> {
        let git = Git::new(path, &shell_env, command_log)?;
        let git_version = git.version()?;
        let (major, minor) = git::parse_version(&git_version).unwrap_or((0, 0));
        let mut checks = Vec::new();
        checks.push(EnvironmentCheck {
            name: "git".into(),
            ok: (major, minor) >= git::MIN_GIT_VERSION,
            detail: format!("{}（{}）", git.program.display(), git_version),
            hint: Some(format!(
                "需要 Git {}.{} 或更高版本",
                git::MIN_GIT_VERSION.0, git::MIN_GIT_VERSION.1
            )),
        });
        checks.push(EnvironmentCheck {
            name: "shell".into(),
            ok: !shell_env.info.used_fallback,
            detail: format!("{} · {} 毫秒", shell_env.info.shell, shell_env.info.elapsed_ms),
            hint: Some("登录 shell 环境解析回退到了最小环境，hooks 与外部工具可能找不到".into()),
        });

        let info = WorktreeSessionInfo {
            worktree_id: id.to_string(),
            worktree_path: path.display().to_string(),
            git_path: git.program.display().to_string(),
            git_version,
            shell: shell_env.info.shell.clone(),
            shell_elapsed_ms: shell_env.info.elapsed_ms,
            used_fallback: shell_env.info.used_fallback,
            path_entries: shell_env.path.len() as u32,
            checks,
        };

        Ok(Arc::new(Self {
            id: id.to_string(),
            path: path.to_path_buf(),
            git,
            shell_env,
            info,
            snapshot: Mutex::new(None),
            read_lock: Mutex::new(()),
            highlighter: Highlighter::start(),
            highlight_generations: Mutex::new(HashMap::new()),
            generation_counter: Mutex::new(0),
            instance_id: NEXT_SESSION_INSTANCE.fetch_add(1, Ordering::Relaxed),
            writing: Mutex::new(false),
        }))
    }

    /// 重新读取状态并解析 diff（`list_changes`）。
    ///
    /// 读取与发布在 `read_lock` 内成对完成：早开始的读取不会晚于较新的读取发布，
    /// 因此发布出的 generation 始终与内容同步递增（Q88、T05）。
    pub fn refresh(&self) -> Result<ChangesSnapshot> {
        let _read = self.read_lock.lock().map_err(|_| AppError::internal("读取锁失败"))?;
        self.refresh_locked()
    }

    fn refresh_locked(&self) -> Result<ChangesSnapshot> {
        let started = std::time::Instant::now();
        let status = status::read(&self.git)?;

        // 一次读取两种 diff：未暂存（工作区 vs 暂存区）与已暂存（暂存区 vs HEAD）。
        let unstaged_patch = self.git.diff_worktree(None)?;
        let staged_patch = self.git.diff_index(None)?;
        let fingerprint = git::fingerprint(&[&unstaged_patch, &staged_patch]);

        let unstaged = git::diff::parse_bytes(&unstaged_patch);
        let staged = git::diff::parse_bytes(&staged_patch);

        let mut models: HashMap<String, FileModelCache> = HashMap::new();
        for file in unstaged.files {
            models.insert(key(ChangesKind::Unstaged, &file.path), FileModelCache { model: file, unfiled: false, hunk_actions: Vec::new() });
        }
        for file in staged.files {
            models.insert(key(ChangesKind::Staged, &file.path), FileModelCache { model: file, unfiled: false, hunk_actions: Vec::new() });
        }

        let mut entries = Vec::with_capacity(status.entries.len());
        for entry in &status.entries {
            let cached = models.get(&key(entry.changes, &entry.path));
            let model = cached.map(|cached| &cached.model);
            let language = language_for_path(&entry.path);
            let row_count_unified = model.map(|file| rows::layout(file, ViewMode::Unified).len() as u32).unwrap_or(0);
            let row_count_split = model.map(|file| rows::layout(file, ViewMode::Split).len() as u32).unwrap_or(0);

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
            let (fingerprint, fp_err) = match compute_file_fingerprint_from_patch(entry, raw_diff, &self.path) {
                Ok(fp) => (fp, None),
                Err(e) => (String::new(), Some(e)),
            };

            // 读取失败不能变成假空指纹，必须标记只读并记录原因
            if fp_err.is_some() && readonly_reason.is_none() {
                readonly_reason = Some(ReadonlyReason::Unreadable);
            }

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
                collapsed: model.map(|file| is_collapsed(&entry.path, file)).unwrap_or_else(|| looks_like_lock(&entry.path)),
                unmerged: entry.unmerged,
                // 未跟踪文件要读了内容才知道行数，打开时补全。
                metrics_loaded: entry.changes != ChangesKind::Untracked,
                hunks: model.map(rows::hunk_anchors).unwrap_or_default(),
                readonly_reason,
                fingerprint,
                snapshot_revision: String::new(),
            });
        }

        let mut guard = self.snapshot.lock().map_err(|_| AppError::internal("状态锁失败"))?;
        let generation = {
            let mut counter = self.generation_counter.lock().map_err(|_| AppError::internal("代际锁失败"))?;
            *counter += 1;
            *counter
        };
        let revision = generation.to_string();
        for entry in &mut entries {
            entry.snapshot_revision = revision.clone();
            if let Some(cached) = models.get_mut(&key(entry.changes, &entry.path)) {
                cached.hunk_actions = self.hunk_actions(entry, &cached.model);
            }
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
        };
        *guard = Some(Snapshot {
            fingerprint,
            entries,
            models,
            layouts: HashMap::new(),
            generation,
        });
        Ok(snapshot)
    }

    /// 取一段显示行。
    pub fn rows(
        &self,
        path: &str,
        changes: ChangesKind,
        view: ViewMode,
        from: u32,
        to: u32,
    ) -> Result<RowSlice> {
        let mut guard = self.snapshot.lock().map_err(|_| AppError::internal("状态锁失败"))?;
        let snapshot = guard.as_mut().ok_or_else(|| AppError::rejected("请先读取仓库状态"))?;

        if !snapshot.entries.iter().any(|entry| entry.path == path && entry.changes == changes) {
            return Err(AppError::rejected(format!(
                "文件不在当前改动列表中：{path}（请刷新后重试）"
            )));
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
            snapshot
                .models
                .insert(key(changes, path), FileModelCache { model: file, unfiled: false, hunk_actions: Vec::new() });
            if let Some(entry) = snapshot
                .entries
                .iter_mut()
                .find(|entry| entry.path == path && entry.changes == changes)
            {
                fill_entry(entry, snapshot.models.get(&key(changes, path)).map(|cached| &cached.model));
            }
        }

        let cached = snapshot
            .models
            .get(&key(changes, path))
            .ok_or_else(|| AppError::rejected(format!("没有该文件的改动内容：{path}")))?;
        let layout_key = format!("{changes:?}/{path}/{view:?}");
        if !snapshot.layouts.contains_key(&layout_key) {
            let mut layout = rows::layout(&cached.model, view);
            for row in &mut layout {
                if row.kind == RowKind::Hunk {
                    row.hunk_action = cached.hunk_actions.get(row.hunk_index as usize).cloned();
                }
            }
            // 简单上限：缓存最多 8 份排版（镜像来回切换的常见用法）。
            if snapshot.layouts.len() >= 8 {
                snapshot.layouts.clear();
            }
            snapshot.layouts.insert(layout_key.clone(), layout);
        }
        let layout = snapshot
            .layouts
            .get(&layout_key)
            .ok_or_else(|| AppError::internal("排版缓存丢失"))?;
        let row_count = layout.len() as u32;
        let from = from.min(row_count);
        let to = to.min(row_count).max(from);
        let rows: Vec<RenderRow> = layout[from as usize..to as usize].to_vec();
        let entry = snapshot
            .entries
            .iter()
            .find(|entry| entry.path == path && entry.changes == changes)
            .cloned()
            .ok_or_else(|| AppError::internal("列表元数据丢失"))?;

        Ok(RowSlice {
            worktree_id: self.id.clone(),
            path: path.to_string(),
            changes,
            view,
            from,
            row_count,
            rows,
            fingerprint: snapshot.fingerprint.clone(),
            revision: snapshot.generation,
            entry,
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
        let guard = self.snapshot.lock().map_err(|_| AppError::internal("状态锁失败"))?;
        let snapshot = guard.as_ref().ok_or_else(|| AppError::rejected("请先读取仓库状态"))?;
        let cached = snapshot
            .models
            .get(&key(changes, path))
            .ok_or_else(|| AppError::rejected(format!("没有该文件的改动内容：{path}")))?;
        Ok(rows::row_index_for_line(
            &cached.model,
            view,
            line,
            version == FileVersion::Old,
        ))
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
                let guard = self.snapshot.lock().map_err(|_| AppError::internal("状态锁失败"))?;
                let snapshot = guard.as_ref().ok_or_else(|| AppError::rejected("请先读取仓库状态"))?;
                let entry = snapshot
                    .entries
                    .iter()
                    .find(|entry| entry.path == path && entry.changes == changes)
                    .ok_or_else(|| AppError::rejected(format!("文件不在当前改动列表中：{path}")))?;
                let raw = snapshot
                    .models
                    .get(&key(changes, path))
                    .map(|cached| cached.model.raw_patch.clone())
                    .unwrap_or_default();
                (snapshot.generation, entry.fingerprint.clone(), raw, entry.original_path.clone())
            };

            // 内容版本由文件身份 + 该版本内容指纹确定：同一内容不重建解析状态。
            let content = self.file_content(path, changes, version)?;
            let content_version = content_version_of(&document_id, &entry_fingerprint, content.as_deref());

            let already_sent = {
                let sent = self.highlight_generations.lock().map_err(|_| AppError::internal("高亮锁失败"))?;
                sent.get(&document_id).copied()
                    == Some(HighlightStamp { revision, content_version })
            };

            // 首次为这一版内容取高亮前，必须证明这些字节确实属于快照展示的版本：
            // 重新读取该文件当前的相关 diff，与快照里那份字节逐字节比较。
            // 仅比对快照代际或 mtime/size 不能排除「未刷新前外部等长改写」（Q88、T05、ADR-0004）。
            if !already_sent && !self.diff_matches_snapshot(path, changes, &expected_raw, orig_path.as_deref())? {
                return Err(AppError::rejected("文件已变化，请重新确认"));
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
                return Err(AppError::internal("高亮线程没有接受文件内容"));
            }

            // 读取内容期间快照可能已前进：本次结果属于旧版本，要求调用方重新请求，
            // 绝不把旧内容标成新版本（Q88、T05）。
            let now = self
                .snapshot
                .lock()
                .map_err(|_| AppError::internal("状态锁失败"))?
                .as_ref()
                .map(|snapshot| snapshot.generation)
                .unwrap_or(revision);
            if now != revision {
                if attempt < 2 {
                    continue;
                }
                return Err(AppError::rejected("文件已变化，请重新确认"));
            }

            {
                let mut sent = self.highlight_generations.lock().map_err(|_| AppError::internal("高亮锁失败"))?;
                // 有界清理：只保留当前版本的记录，避免刷新累积旧键。
                sent.retain(|key, _| key == &document_id);
                sent.insert(document_id.clone(), HighlightStamp { revision, content_version });
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
        };
        Ok(current == expected_raw)
    }

    /// 某个版本的文件内容（旧版本可能是 HEAD 或暂存区，新版本可能是暂存区或工作区）。
    fn file_content(&self, path: &str, changes: ChangesKind, version: FileVersion) -> Result<Option<String>> {
        let scope = match (changes, version) {
            (ChangesKind::Unstaged, FileVersion::Old) | (ChangesKind::Staged, FileVersion::New) => "index",
            (ChangesKind::Staged, FileVersion::Old) => "HEAD",
            (ChangesKind::Unstaged, FileVersion::New) => "worktree",
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
    pub fn highlight_document_count(&self) -> usize {
        self.highlighter.document_count()
    }

    /// 该 worktree 自己的 gitDir（linked worktree 的 `.git` 是文件，由 Git 解析出真实目录）。
    pub fn git_dir_path(&self) -> Option<PathBuf> {
        let text = self.git.try_run(&["rev-parse", "--path-format=absolute", "--git-dir"])?;
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
                guard
                    .as_ref()
                    .and_then(|snapshot| snapshot.models.get(&key(changes, path)).map(|cached| cached.model.binary))
            })
            .unwrap_or(false)
    }

    /// 获取当前 worktree 的独占写操作锁（不排队，发现被占用立即拒绝并发写，Q90/T06）。
    pub fn acquire_write_lock(&self) -> Result<WriteGuard<'_>> {
        let mut writing = self
            .writing
            .lock()
            .map_err(|_| AppError::internal("写操作锁失败"))?;
        if *writing {
            return Err(AppError::rejected("当前 worktree 有正在进行的写操作，请稍候"));
        }
        *writing = true;
        Ok(WriteGuard(&self.writing))
    }

    /// 整文件暂存（Q8、Q79、Q81–Q83、ADR-0005、ADR-0009）。
    pub fn stage_file(&self, path: &str, fingerprint: &str) -> Result<ChangesSnapshot> {
        self.execute_staging_op(path, fingerprint, true, None)
    }

    /// 整文件取消暂存（Q8、Q79、Q81–Q83、ADR-0005、ADR-0009、T04）。
    pub fn unstage_file(&self, path: &str, fingerprint: &str) -> Result<ChangesSnapshot> {
        self.execute_staging_op(path, fingerprint, false, None)
    }

    pub fn stage_hunk(&self, path: &str, fingerprint: &str, hunk_id: &str) -> Result<ChangesSnapshot> {
        self.execute_staging_op(path, fingerprint, true, Some(hunk_id))
    }

    pub fn unstage_hunk(&self, path: &str, fingerprint: &str, hunk_id: &str) -> Result<ChangesSnapshot> {
        self.execute_staging_op(path, fingerprint, false, Some(hunk_id))
    }

    pub fn discard_file(&self, path: &str, fingerprint: &str, revision: &str, store: &RecoveryStore) -> Result<ChangesSnapshot> {
        self.discard_with_hook(path, fingerprint, revision, None, store, |_| Ok(()))
    }

    pub fn discard_hunk(&self, path: &str, fingerprint: &str, revision: &str, hunk_id: &str, store: &RecoveryStore) -> Result<ChangesSnapshot> {
        self.discard_with_hook(path, fingerprint, revision, Some(hunk_id), store, |_| Ok(()))
    }

    fn check_discard_target(&self, path: &str, fingerprint: &str, revision: &str) -> Result<StatusEntry> {
        {
            let guard = self.snapshot.lock().map_err(|_| AppError::internal("状态锁失败"))?;
            let snapshot = guard.as_ref().ok_or_else(|| AppError::rejected("请先读取仓库状态"))?;
            if !snapshot.entries.iter().any(|entry| entry.path == path && entry.changes == ChangesKind::Unstaged
                && entry.fingerprint == fingerprint && entry.snapshot_revision == revision) {
                return Err(AppError::rejected("文件已变化，请重新确认"));
            }
        }
        let status = status::read(&self.git)?;
        let entry = status.entries.into_iter().find(|entry| entry.path == path && entry.changes == ChangesKind::Unstaged)
            .ok_or_else(|| AppError::rejected("文件已变化，请重新确认"))?;
        if let Some(reason) = &entry.readonly_reason { return Err(AppError::rejected(reason.description())); }
        // intent-to-add（`git add -N`）在暂存区还没有 blob：既没有可恢复的提交基线，
        // 反向应用又会直接删除文件。按「未跟踪文件没有删除入口」的既有边界明确拒绝。
        if entry.status == FileStatus::Added
            && entry.oid_index.as_deref().map_or(true, |oid| oid.chars().all(|ch| ch == '0'))
        {
            return Err(AppError::rejected("新增文件没有可恢复的原始版本，不能在应用内丢弃"));
        }
        let diff = self.git.diff_worktree(Some(path))?;
        if compute_file_fingerprint_from_patch(&entry, &diff, &self.path)? != fingerprint {
            return Err(AppError::rejected("文件已变化，请重新确认"));
        }
        Ok(entry)
    }

    /// Test seams surround real storage and real Git; normal operations pass a no-op.
    pub fn discard_with_hook(
        &self, path: &str, fingerprint: &str, revision: &str, hunk_id: Option<&str>,
        store: &RecoveryStore, mut hook: impl FnMut(DiscardPhase) -> Result<()>,
    ) -> Result<ChangesSnapshot> {
        let _write = self.acquire_write_lock()?;
        let entry = self.check_discard_target(path, fingerprint, revision)?;
        let identity = WorktreeIdentity::capture(&self.git)?;
        store.ensure_outside(&identity)?;
        let mut paths = vec![entry.path.clone()];
        if let Some(original) = &entry.original_path { paths.push(original.clone()); }
        paths.sort();
        paths.dedup();
        let before = recovery::capture_files(&self.path, &paths)?;
        let patch = match hunk_id {
            Some(id) => self.hunk_patch(path, fingerprint, id, true)?,
            None => self.git.discard_patch(path)?,
        };
        if patch.is_empty() { return Err(AppError::rejected("文件已变化，请重新确认")); }
        let expected_after = store.preview(&self.git, &self.path, &before, &patch)?;
        hook(DiscardPhase::BeforeSave)?;
        // A complete, durably published record is the prerequisite for any worktree write.
        let record = store.save(identity.clone(), if hunk_id.is_some() { RecoverySource::DiscardHunk } else { RecoverySource::DiscardFile }, path, before.clone())?;
        hook(DiscardPhase::AfterSave).map_err(discard_pending)?;
        hook(DiscardPhase::BeforeWrite).map_err(discard_pending)?;
        {
            // Do not let an internal refresh replace hunk credentials between the final check and Git.
            let _read = self.read_lock.lock().map_err(|_| AppError::internal("读取锁失败"))?;
            let validation = (|| {
                self.check_discard_target(path, fingerprint, revision)?;
                if WorktreeIdentity::capture(&self.git)? != identity || !recovery::unchanged(&before, &recovery::capture_files(&self.path, &paths)?) {
                    return Err(AppError::rejected("文件已变化，请重新确认"));
                }
                let current_patch = match hunk_id {
                    Some(id) => self.hunk_patch(path, fingerprint, id, true)?,
                    None => self.git.discard_patch(path)?,
                };
                if current_patch != patch { return Err(AppError::rejected("文件已变化，请重新确认")); }
                Ok(())
            })();
            if let Err(error) = validation {
                store.finish(&record, &DiscardOutcome::NotApplied).map_err(discard_pending)?;
                return Err(error);
            }
            let output = self.git.apply_worktree_patch(patch).map_err(discard_pending)?;
            if output.timed_out || output.exit_code != 0 {
                return Err(discard_pending(format!("Git 退出码 {}：{}", output.exit_code, output.stderr.trim())));
            }
        }
        hook(DiscardPhase::AfterWrite).map_err(discard_pending)?;
        let actual_after = recovery::capture_files(&self.path, &paths).map_err(discard_pending)?;
        if actual_after != expected_after {
            return Err(discard_pending("写后状态与已校验结果不同，可能有外部修改"));
        }
        hook(DiscardPhase::BeforeComplete).map_err(discard_pending)?;
        store.finish(&record, &DiscardOutcome::Completed { after: expected_after }).map_err(discard_pending)?;
        self.refresh().map_err(|error| AppError::needs_verification(format!("丢弃已执行并保存，最新状态读取失败：{error}")))
    }

    fn hunk_actions(&self, entry: &ChangeEntry, file: &git::diff::FileChange) -> Vec<HunkAction> {
        if !supports_hunk_actions(entry, file) {
            return Vec::new();
        }
        if file.hunks.iter().any(|hunk| {
            !file.raw_patch.get(hunk.raw_range.clone()).is_some_and(|raw| raw.starts_with(b"@@ "))
        }) {
            return Vec::new();
        }
        file.hunks.iter().enumerate().map(|(index, hunk)| {
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
        }).collect()
    }

    fn hunk_patch(&self, path: &str, expected_fp: &str, hunk_id: &str, is_stage: bool) -> Result<Vec<u8>> {
        let guard = self.snapshot.lock().map_err(|_| AppError::internal("状态锁失败"))?;
        let snapshot = guard.as_ref().ok_or_else(|| AppError::rejected("请先读取仓库状态"))?;
        let changes = if is_stage { ChangesKind::Unstaged } else { ChangesKind::Staged };
        let entry = snapshot.entries.iter()
            .find(|entry| entry.path == path && entry.changes == changes && entry.fingerprint == expected_fp)
            .ok_or_else(|| AppError::rejected("文件已变化，请重新确认"))?;
        let cached = snapshot.models.get(&key(changes, path))
            .ok_or_else(|| AppError::rejected("文件已变化，请重新确认"))?;
        if !supports_hunk_actions(entry, &cached.model) {
            return Err(AppError::rejected("这个文件不支持 hunk 操作"));
        }
        let index = cached.hunk_actions.iter().position(|action| {
            action.hunk_id == hunk_id
                && action.fingerprint == expected_fp
                && action.snapshot_revision == snapshot.generation.to_string()
        }).ok_or_else(|| AppError::rejected("文件已变化，请重新确认"))?;
        cached.model.hunk_patch(index)
            .ok_or_else(|| AppError::rejected("当前 hunk 无法生成补丁，请刷新后重新确认"))
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
        let mut last_lock_err: Option<String> = None;
        const LOCK_RETRIES: usize = 3;
        const LOCK_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(100);

        for attempt in 0..=LOCK_RETRIES {
            // 每次尝试（包含重试）前，都必须重新读取真实状态并核对目标当前版本！（审查意见 2）
            let status = status::read(&self.git)?;
            let entry = if is_stage {
                status
                    .entries
                    .iter()
                    .find(|e| e.path == path && (e.changes == ChangesKind::Unstaged || e.changes == ChangesKind::Untracked))
            } else {
                status
                    .entries
                    .iter()
                    .find(|e| e.path == path && e.changes == ChangesKind::Staged)
            };

            let entry = entry.ok_or_else(|| AppError::rejected("文件已变化，请重新确认"))?;

            if let Some(reason) = &entry.readonly_reason {
                return Err(AppError::rejected(reason.description()));
            }

            // 检查工作区符号链接或嵌套仓库
            let full_path = self.path.join(path);
            if full_path.is_symlink() {
                return Err(AppError::rejected("符号链接为只读类型，不支持在应用内暂存或取消暂存"));
            }
            if entry.changes == ChangesKind::Untracked && (full_path.join(".git").exists() || path.ends_with('/')) {
                return Err(AppError::rejected("嵌套仓库为只读类型，不支持在应用内暂存"));
            }

            // 针对该单文件读取当前 diff 并在写前核对指纹
            let diff_bytes = if is_stage {
                if entry.changes == ChangesKind::Untracked {
                    Vec::new()
                } else {
                    self.git.diff_worktree(Some(path))?
                }
            } else {
                self.git.diff_index_with_orig(path, entry.original_path.as_deref())?
            };

            let current_fp = compute_file_fingerprint_from_patch(entry, &diff_bytes, &self.path)?;
            if current_fp != expected_fp {
                return Err(AppError::rejected("文件已变化，请重新确认"));
            }

            // 执行单次写操作（run_once，不绕过核对进行隐式重试）
            let output = if let Some(hunk_id) = hunk_id {
                // 状态读取期间可能刷新过快照；在真正应用前才解析仍有效的原始目标。
                let patch = self.hunk_patch(path, expected_fp, hunk_id, is_stage)?;
                self.git.apply_index_patch(patch, !is_stage)?
            } else if is_stage {
                self.git.stage_file(path, entry.original_path.as_deref())?
            } else {
                self.git.unstage_file(path, entry.original_path.as_deref())?
            };

            // 结果不确定时标为“结果待核对”（审查意见 1）
            if output.timed_out || output.exit_code < 0 {
                return Err(AppError::needs_verification(
                    "Git 命令执行超时或中断，写入结果待核对；请重新核对真实状态后再继续",
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
                    return Err(AppError::rejected(format!(
                        "Git 索引被其他进程占用（index.lock），已重试 {LOCK_RETRIES} 次：{detail}"
                    )));
                }
            }

            if output.exit_code != 0 {
                return Err(AppError::git(format!(
                    "{}失败（退出码 {}）：{}",
                    match (is_stage, hunk_id.is_some()) {
                        (true, true) => "暂存 hunk",
                        (false, true) => "取消暂存 hunk",
                        (true, false) => "暂存文件",
                        (false, false) => "取消暂存文件",
                    },
                    output.exit_code,
                    output.stderr.trim()
                )));
            }

            // 执行成功，跳出重试循环
            break;
        }

        // 写入成功后重新读取状态；若读取失败，也属于结果待核对（审查意见 1）
        match self.refresh() {
            Ok(snap) => Ok(snap),
            Err(e) => Err(AppError::needs_verification(format!(
                "文件已操作但读取最新状态失败（结果待核对）：{e}"
            ))),
        }
    }
}

fn discard_pending(error: impl std::fmt::Display) -> AppError {
    AppError::needs_verification(format!("内容已保存到回收区，丢弃结果待核对；不会自动继续：{error}"))
}

fn supports_hunk_actions(entry: &ChangeEntry, file: &git::diff::FileChange) -> bool {
    entry.changes != ChangesKind::Untracked
        && entry.readonly_reason.is_none()
        && !entry.unmerged
        && !entry.binary
        && !file.binary
        && !file.hunks.is_empty()
        && matches!(entry.status, FileStatus::Modified | FileStatus::Added | FileStatus::Deleted)
        && matches!(file.status, FileStatus::Modified | FileStatus::Added | FileStatus::Deleted)
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
            entry.row_count_unified = rows::layout(file, ViewMode::Unified).len() as u32;
            entry.row_count_split = rows::layout(file, ViewMode::Split).len() as u32;
            entry.collapsed = is_collapsed(&entry.path, file);
            entry.hunks = rows::hunk_anchors(file);
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
    entries.iter().filter(|entry| entry.changes == changes).count() as u32
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

/// 计算单个文件变更条目的 diff 指纹（Q80、ADR-0009）。
///
/// 指纹来自展示所用的同一份 Git 输出切片（diff_bytes），列表刷新时不启动额外 Git 进程。
pub fn compute_file_fingerprint_from_patch(
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
    if entry.changes == ChangesKind::Untracked || (entry.changes == ChangesKind::Unstaged && lacks_text_hunks) {
        let file_path = worktree_path.join(&entry.path);
        let meta = match std::fs::symlink_metadata(&file_path) {
            Ok(meta) => Some(meta),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && entry.changes == ChangesKind::Unstaged => None,
            Err(error) => return Err(AppError::rejected(format!("读取文件属性失败（{}）：{error}", entry.path))),
        };
        hasher.update(b"worktree_meta");
        if let Some(meta) = meta {
        hasher.update(b"present");
        hasher.update(meta.len().to_le_bytes());

        if meta.is_file() {
            let mut f = std::fs::File::open(&file_path)
                .map_err(|e| AppError::rejected(format!("打开文件失败（{}）：{}", entry.path, e)))?;
            use std::io::Read;
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)
                .map_err(|e| AppError::rejected(format!("读取文件内容失败（{}）：{}", entry.path, e)))?;
            let file_hash = Sha256::digest(&buf);
            hasher.update(&file_hash);
        } else if meta.file_type().is_symlink() {
            let target = std::fs::read_link(&file_path)
                .map_err(|e| AppError::rejected(format!("读取符号链接失败（{}）：{}", entry.path, e)))?;
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
    name.ends_with(".lock") || matches!(name.as_str(), "pnpm-lock.yaml" | "package-lock.json" | "yarn.lock")
}

/// 扩展名 → 语法名（同时交给 syntect 与前端做提示）。
pub fn language_for_path(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    if looks_like_lock(path) {
        return "yaml".into();
    }
    let extension = name.rsplit_once('.').map(|(_, ext)| ext).unwrap_or(name.as_str());
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
        let toplevel = git
            .toplevel()
            .ok_or_else(|| AppError::rejected(format!("不是 Git 仓库：{}", path.display())))?;
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
    digest.iter().take(8).map(|byte| format!("{byte:02x}")).collect()
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
        let Some(entry) = current.as_mut() else { continue };
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
        assert!(is_collapsed("logs/app.log", &file(LARGE_FILE_LINES + 1, 0, false)));
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
