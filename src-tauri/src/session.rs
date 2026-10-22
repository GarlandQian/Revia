//! 会话：登记过的仓库 / worktree，以及它们的只读状态与显示模型。
//!
//! 约束（评审调整 1）：
//! - 除 `add_repo` 外，所有命令都只接收 Rust 层登记过的 ID，不接收任意目录路径；
//! - 文件路径必须是当前 `list_changes` 结果里的条目，否则拒绝。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::error::{AppError, Result};
use crate::git::highlight::{HighlightRequest, HighlightResult, Highlighter};
use crate::git::rows::{self, HunkAnchor, RenderRow, ViewMode};
use crate::git::status::{self, ChangesKind, FileStatus, StatusEntry};
use crate::git::{self, Git};
use crate::shell::{self, ShellEnv};

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
    pub readonly_reason: Option<String>,
    /// 文件级 diff 指纹（Q80、ADR-0009）：写操作校验依据。
    pub fingerprint: String,
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
    highlighter: Highlighter,
    /// 已经发给高亮线程的文件内容代际，避免重复传输内容。
    highlight_generations: Mutex<HashMap<String, u64>>,
    generation_counter: Mutex<u64>,
    /// 是否有正在进行的写操作（同一 worktree 互斥，不排队）。
    writing: Mutex<bool>,
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
            highlighter: Highlighter::start(),
            highlight_generations: Mutex::new(HashMap::new()),
            generation_counter: Mutex::new(0),
            writing: Mutex::new(false),
        }))
    }

    /// 重新读取状态并解析 diff（`list_changes`）。
    pub fn refresh(&self) -> Result<ChangesSnapshot> {
        let started = std::time::Instant::now();
        let status = status::read(&self.git)?;

        // 一次读取两种 diff：未暂存（工作区 vs 暂存区）与已暂存（暂存区 vs HEAD）。
        let unstaged_patch = self.git.diff_worktree(None)?;
        let staged_patch = self.git.diff_index(None)?;
        let fingerprint = git::fingerprint(&[&unstaged_patch, &staged_patch]);

        let unstaged = git::diff::parse(&String::from_utf8_lossy(&unstaged_patch));
        let staged = git::diff::parse(&String::from_utf8_lossy(&staged_patch));

        let mut models: HashMap<String, FileModelCache> = HashMap::new();
        for file in unstaged.files {
            models.insert(key(ChangesKind::Unstaged, &file.path), FileModelCache { model: file, unfiled: false });
        }
        for file in staged.files {
            models.insert(key(ChangesKind::Staged, &file.path), FileModelCache { model: file, unfiled: false });
        }

        let mut entries = Vec::with_capacity(status.entries.len());
        for entry in &status.entries {
            let cached = models.get(&key(entry.changes, &entry.path));
            let model = cached.map(|cached| &cached.model);
            let language = language_for_path(&entry.path);
            let row_count_unified = model.map(|file| rows::layout(file, ViewMode::Unified).len() as u32).unwrap_or(0);
            let row_count_split = model.map(|file| rows::layout(file, ViewMode::Split).len() as u32).unwrap_or(0);
            let mut readonly_reason = entry.readonly_reason.clone();
            if readonly_reason.is_none() && self.path.join(&entry.path).is_symlink() {
                readonly_reason = Some("符号链接为只读类型，不支持在应用内暂存或取消暂存".to_string());
            }
            let fingerprint = compute_file_fingerprint(&self.git, &self.path, entry).unwrap_or_default();
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
            });
        }

        let snapshot = ChangesSnapshot {
            worktree_id: self.id.clone(),
            worktree_path: self.path.display().to_string(),
            branch: status.branch.clone(),
            detached: status.detached,
            fingerprint: fingerprint.clone(),
            staged_count: count(&entries, ChangesKind::Staged),
            unstaged_count: count(&entries, ChangesKind::Unstaged),
            untracked_count: count(&entries, ChangesKind::Untracked),
            entries: entries.clone(),
            elapsed_ms: started.elapsed().as_millis() as u64,
        };

        let generation = {
            let mut counter = self.generation_counter.lock().map_err(|_| AppError::internal("代际锁失败"))?;
            *counter += 1;
            *counter
        };
        let mut guard = self.snapshot.lock().map_err(|_| AppError::internal("状态锁失败"))?;
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
            let mut parsed = git::diff::parse(&String::from_utf8_lossy(&patch));
            let file = parsed
                .files
                .pop()
                .unwrap_or_else(|| empty_file(path, FileStatus::Added));
            snapshot
                .models
                .insert(key(changes, path), FileModelCache { model: file, unfiled: false });
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
            let layout = rows::layout(&cached.model, view);
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
        let (generation, language, fingerprint) = {
            let guard = self.snapshot.lock().map_err(|_| AppError::internal("状态锁失败"))?;
            let snapshot = guard.as_ref().ok_or_else(|| AppError::rejected("请先读取仓库状态"))?;
            if !snapshot.entries.iter().any(|entry| entry.path == path && entry.changes == changes) {
                return Err(AppError::rejected(format!("文件不在当前改动列表中：{path}")));
            }
            let language = language_for_path(path);
            (snapshot.generation, language, snapshot.fingerprint.clone())
        };

        let document_id = format!("{fingerprint}/{changes:?}/{version:?}/{path}");
        let already_sent = {
            let sent = self.highlight_generations.lock().map_err(|_| AppError::internal("高亮锁失败"))?;
            sent.get(&document_id).copied() == Some(generation)
        };

        let mut request = HighlightRequest {
            document_id: document_id.clone(),
            generation,
            language,
            from_line: from0,
            to_line: to0,
            content: None,
        };

        if !already_sent {
            request.content = self.file_content(path, changes, version)?;
        }

        let mut result = self.highlighter.highlight(request)?;
        if result.need_content {
            // 高亮线程上没有这个文件的解析状态（可能被淘汰），补发一次内容。
            let mut retry = HighlightRequest {
                document_id: document_id.clone(),
                generation,
                language: language_for_path(path),
                from_line: from0,
                to_line: to0,
                content: self.file_content(path, changes, version)?,
            };
            retry.content = retry.content.or(Some(String::new()));
            result = self.highlighter.highlight(retry)?;
        }
        if !result.need_content {
            let mut sent = self.highlight_generations.lock().map_err(|_| AppError::internal("高亮锁失败"))?;
            sent.insert(document_id, generation);
            // 对外统一成 1 基行号（与界面一致）。
            result.from_line = from_line as usize;
        }
        Ok(result)
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
        let _guard = self.acquire_write_lock()?;
        let status = status::read(&self.git)?;
        let entry = status
            .entries
            .iter()
            .find(|e| e.path == path && (e.changes == ChangesKind::Unstaged || e.changes == ChangesKind::Untracked))
            .ok_or_else(|| AppError::rejected(format!("目标文件不在当前可暂存列表中：{path}（请刷新后重试）")))?;

        if let Some(reason) = &entry.readonly_reason {
            return Err(AppError::rejected(reason.clone()));
        }
        if self.path.join(path).is_symlink() {
            return Err(AppError::rejected("符号链接为只读类型，不支持在应用内暂存或取消暂存"));
        }

        let current_fp = compute_file_fingerprint(&self.git, &self.path, entry)?;
        if current_fp != fingerprint {
            return Err(AppError::rejected("文件已变化，请重新确认"));
        }

        if let Some(orig) = &entry.original_path {
            self.git.run_ok(
                &["-c", "core.quotePath=false", "add", "-A", "--", orig, path],
                "暂存文件",
            )?;
        } else {
            self.git.run_ok(
                &["-c", "core.quotePath=false", "add", "-A", "--", path],
                "暂存文件",
            )?;
        }

        self.refresh()
    }

    /// 整文件取消暂存（Q8、Q79、Q81–Q83、ADR-0005、ADR-0009、T04）。
    pub fn unstage_file(&self, path: &str, fingerprint: &str) -> Result<ChangesSnapshot> {
        let _guard = self.acquire_write_lock()?;
        let status = status::read(&self.git)?;
        let entry = status
            .entries
            .iter()
            .find(|e| e.path == path && e.changes == ChangesKind::Staged)
            .ok_or_else(|| AppError::rejected(format!("目标文件不在当前已暂存列表中：{path}（请刷新后重试）")))?;

        if let Some(reason) = &entry.readonly_reason {
            return Err(AppError::rejected(reason.clone()));
        }

        let current_fp = compute_file_fingerprint(&self.git, &self.path, entry)?;
        if current_fp != fingerprint {
            return Err(AppError::rejected("文件已变化，请重新确认"));
        }

        if entry.status == FileStatus::Renamed {
            if let Some(orig) = &entry.original_path {
                self.git.run_ok(
                    &["-c", "core.quotePath=false", "reset", "--", orig, path],
                    "取消暂存文件",
                )?;
            } else {
                self.git.run_ok(
                    &["-c", "core.quotePath=false", "reset", "--", path],
                    "取消暂存文件",
                )?;
            }
        } else {
            self.git.run_ok(
                &["-c", "core.quotePath=false", "reset", "--", path],
                "取消暂存文件",
            )?;
        }

        self.refresh()
    }
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
pub fn compute_file_fingerprint(
    git: &Git,
    worktree_path: &Path,
    entry: &StatusEntry,
) -> Result<String> {
    let mut hasher = Sha256::new();
    // 1. changes
    hasher.update((entry.changes as u8).to_le_bytes());
    // 2. path & original_path
    hasher.update((entry.path.len() as u64).to_le_bytes());
    hasher.update(entry.path.as_bytes());
    if let Some(orig) = &entry.original_path {
        hasher.update(b"orig:");
        hasher.update((orig.len() as u64).to_le_bytes());
        hasher.update(orig.as_bytes());
    }
    // 3. status
    hasher.update(entry.status.letter().as_bytes());
    // 4. modes & oids
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

    // 5. diff patch
    let diff_bytes = match entry.changes {
        ChangesKind::Unstaged => git.diff_worktree(Some(&entry.path))?,
        ChangesKind::Staged => {
            if let Some(orig) = &entry.original_path {
                let mut args = vec!["-c", "core.quotePath=false", "diff"];
                args.extend_from_slice(git::DIFF_FLAGS);
                args.push("--staged");
                args.push("--");
                args.push(orig);
                args.push(&entry.path);
                git.run_ok(&args, "读取已暂存重命名 diff")?.stdout
            } else {
                git.diff_index(Some(&entry.path))?
            }
        }
        ChangesKind::Untracked => git.diff_untracked(&entry.path)?,
    };
    hasher.update((diff_bytes.len() as u64).to_le_bytes());
    hasher.update(&diff_bytes);

    // 6. 工作区实际物理文件状态（针对 Unstaged 与 Untracked）
    if entry.changes == ChangesKind::Unstaged || entry.changes == ChangesKind::Untracked {
        let file_path = worktree_path.join(&entry.path);
        match std::fs::symlink_metadata(&file_path) {
            Ok(meta) => {
                hasher.update(b"exists");
                hasher.update(meta.len().to_le_bytes());
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    hasher.update(meta.mode().to_le_bytes());
                }
                if meta.is_file() {
                    if let Ok(mut f) = std::fs::File::open(&file_path) {
                        use std::io::Read;
                        let mut buf = Vec::new();
                        let _ = f.read_to_end(&mut buf);
                        let file_hash = Sha256::digest(&buf);
                        hasher.update(&file_hash);
                    }
                } else if meta.file_type().is_symlink() {
                    if let Ok(target) = std::fs::read_link(&file_path) {
                        hasher.update(target.to_string_lossy().as_bytes());
                    }
                }
            }
            Err(_) => {
                hasher.update(b"missing");
            }
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
