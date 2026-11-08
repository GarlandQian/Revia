//! 当前 worktree 的文件监视（Q88、Q95）。
//!
//! 边界：
//! - 只监视**当前** worktree，不为全部登记仓库常驻监视（M4 才做完整概览）；
//! - 监视范围是该 worktree 的工作区、它自己的 gitDir（linked worktree 的 `.git` 是文件，
//!   这里解析出真实的 gitDir）以及共享 commonDir 中会影响状态的元数据（refs、packed-refs、HEAD 等）；
//! - 不递归监视其他 worktree，也不监视对象库；
//! - 切换 / 关闭时整体释放 watcher、线程与监听；
//! - 失败不静默：状态一直保留到成功重建，手动与回到前台核对不受影响。
//!
//! 事件只做「有变化」的通知，具体读取仍走 `WorktreeSession::refresh()`，
//! 因此监视失败不会降低写前校验强度（ADR-0005、ADR-0009）。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify::{Event, EventKind, RecursiveMode, Watcher};

use crate::error::{AppError, AppErrorPayload, ErrorKind};
use crate::git::Git;
use crate::m7::perf::{PerfStore, WatchEventKind};

/// 监视不可用的统一结构化原因：`detail` 只放路径与系统错误等数据。
fn unavailable(detail: String) -> AppErrorPayload {
    AppError::coded(
        ErrorKind::Environment,
        "errors.watch.unavailable",
        "自动刷新不可用：{detail}",
        &[("detail", detail)],
    )
    .payload()
}
use serde::Serialize;

/// 合并窗口：把短时间内的连续事件折叠成一次刷新（编辑器保存常产生多个事件）。
pub const DEBOUNCE: Duration = Duration::from_millis(120);

/// 监视状态，供前端持续展示（Q95）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WatchStatus {
    /// 是否正在监视。
    pub active: bool,
    /// 监视失败时的原因；成功重建后清空。
    pub error: Option<AppErrorPayload>,
    /// 本次激活的代号；前端用它拒绝旧 worktree 的迟到事件。
    pub activation: u64,
    /// 当前监视目标（worktree 路径）；前端据此确认状态属于自己请求的目标。
    pub target: Option<String>,
    /// 已建立的监视路径数量。
    pub roots: usize,
}

impl WatchStatus {
    pub fn idle() -> Self {
        Self {
            active: false,
            error: None,
            activation: 0,
            target: None,
            roots: 0,
        }
    }
}

/// 监视控制句柄：持有 notify watcher 与工作线程。
pub struct WorktreeWatcher {
    activation: u64,
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<WatchStatus>>,
    thread: Option<std::thread::JoinHandle<()>>,
    /// 保留 watcher 的所有权；drop 时停止底层监视。启动失败时为 None。
    _watcher: Option<notify::RecommendedWatcher>,
}

/// 需要监视的路径集合。
///
/// 只包含**影响当前 worktree 状态**的路径：
/// - 工作区（递归）：编辑器改动、新建/删除/原子替换；
/// - 该 worktree 自己的 gitDir（递归）：它的 index / HEAD / ORIG_HEAD 等私有元数据；
/// - 共享 commonDir 的 refs 与 packed-refs（递归 refs，非递归顶层单个文件）。
///
/// 明确**不**监视：objects 对象库、其他 worktree 的私有 gitDir（`worktrees/*/`）、
/// 其他 worktree 的工作区。它们的变化不影响当前 worktree 的展示状态（Q88）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchRoots {
    pub worktree: PathBuf,
    pub git_dir: PathBuf,
    pub common_dir: Option<PathBuf>,
}

/// 解析需要监视的路径。linked worktree 的 `.git` 是文件，读取其中的 `gitdir:` 指向。
pub fn resolve_roots(
    worktree: &Path,
    git_dir: Option<PathBuf>,
    common_dir: Option<PathBuf>,
) -> WatchRoots {
    let resolved_git_dir = git_dir.unwrap_or_else(|| resolve_dot_git(worktree));
    WatchRoots {
        worktree: worktree.to_path_buf(),
        git_dir: resolved_git_dir,
        common_dir,
    }
}

/// 从工作区的 `.git` 解析真实 gitDir；目录则直接使用，文件则读取 `gitdir:` 行。
fn resolve_dot_git(worktree: &Path) -> PathBuf {
    let dot_git = worktree.join(".git");
    if dot_git.is_dir() {
        return dot_git;
    }
    if let Ok(text) = std::fs::read_to_string(&dot_git) {
        if let Some(rest) = text.trim().strip_prefix("gitdir:") {
            let target = PathBuf::from(rest.trim());
            return if target.is_absolute() {
                target
            } else {
                worktree.join(target)
            };
        }
    }
    dot_git
}

impl WorktreeWatcher {
    /// 启动监视。`on_change` 在合并窗口结束后被调用一次；它由监视线程串行调用。
    pub fn start<F>(roots: WatchRoots, activation: u64, on_change: F) -> Self
    where
        F: Fn(u64) + Send + 'static,
    {
        Self::start_with_ignore(roots, activation, None, None, on_change)
    }

    /// 带 Git 忽略判定与性能计数的启动（M9 任务 04、Q210、Q213）。
    pub fn start_with_ignore<F>(
        roots: WatchRoots,
        activation: u64,
        ignore: Option<Arc<IgnoreMatcher>>,
        perf: Option<Arc<PerfStore>>,
        on_change: F,
    ) -> Self
    where
        F: Fn(u64) + Send + 'static,
    {
        let stop = Arc::new(AtomicBool::new(false));
        let status = Arc::new(Mutex::new(WatchStatus {
            active: false,
            error: None,
            activation,
            target: Some(roots.worktree.display().to_string()),
            roots: 0,
        }));

        let (tx, rx): (Sender<Result<(), String>>, Receiver<Result<(), String>>) = channel();
        let mut root_count = 0usize;
        let mut failures: Vec<String> = Vec::new();
        let runtime_errors = Arc::new(Mutex::new(Vec::<String>::new()));

        // 运行时错误不能丢弃：记录第一条，状态对外持续为不可用（Q95）。
        let callback_errors = runtime_errors.clone();
        // core.excludesFile 所在目录：文件常被编辑器原子替换，监视目录而不是文件本身。
        let excludes_dir = ignore
            .as_ref()
            .and_then(|matcher| matcher.excludes_file.as_ref())
            .and_then(|file| file.parent().map(Path::to_path_buf))
            .filter(|dir| dir.is_dir());
        let filter = WatchFilter::from_roots(&roots);
        let mut watcher =
            match notify::recommended_watcher(move |result: notify::Result<Event>| match result {
                Ok(event) => {
                    // Q213、S1：被忽略的路径不触发刷新；统计只记次数，不发送刷新。
                    match filter.classify(&event, ignore.as_deref()) {
                        Verdict::Refresh(kind) => {
                            if let Some(perf) = &perf {
                                perf.note_watch_event(kind);
                            }
                            let _ = tx.send(Ok(()));
                        }
                        Verdict::Ignored => {
                            if let Some(perf) = &perf {
                                perf.note_watch_event(WatchEventKind::Ignored);
                            }
                        }
                        Verdict::None => {}
                    }
                }
                Err(error) => {
                    let text = error.to_string();
                    if let Ok(mut guard) = callback_errors.lock() {
                        if guard.is_empty() {
                            guard.push(text.clone());
                        }
                    }
                    let _ = tx.send(Err(text));
                }
            }) {
                Ok(watcher) => watcher,
                Err(error) => {
                    // 启动失败就是降级，不重试、不 panic、不谎报 active。
                    if let Ok(mut guard) = status.lock() {
                        guard.active = false;
                        guard.error = Some(
                            AppError::coded(
                                ErrorKind::Environment,
                                "errors.watch.startFailed",
                                "自动刷新不可用：无法启动文件监视：{error}",
                                &[("error", error.to_string())],
                            )
                            .payload(),
                        );
                    }
                    return Self {
                        activation,
                        stop,
                        status,
                        thread: None,
                        _watcher: None,
                    };
                }
            };

        // 工作区递归：编辑器改动、新建/删除/原子替换都在这里。
        match watcher.watch(&roots.worktree, RecursiveMode::Recursive) {
            Ok(()) => root_count += 1,
            Err(error) => failures.push(format!("{}: {error}", roots.worktree.display())),
        }
        // 该 worktree 自己的 gitDir：index / HEAD / ORIG_HEAD 等私有元数据。
        //
        // 对普通仓库这是 `<worktree>/.git`，已被上面的工作区递归覆盖（含 index），无需重复注册；
        // 对 linked worktree 它是 `<common>/worktrees/<name>`，必须单独监视。
        if roots.git_dir != roots.worktree.join(".git")
            && !roots.git_dir.starts_with(&roots.worktree)
        {
            match watcher.watch(&roots.git_dir, RecursiveMode::Recursive) {
                Ok(()) => root_count += 1,
                Err(error) => failures.push(format!("{}: {error}", roots.git_dir.display())),
            }
        }
        // 共享 commonDir 只注册**影响当前 worktree 状态**的元数据，不递归整棵 .git：
        // - refs/（递归）：分支/标签变化；
        // - packed-refs / HEAD / ORIG_HEAD（单个文件，非递归）。
        // 明确排除 objects/ 对象库与其他 worktree 的 worktrees/*/ 私有元数据。
        if let Some(common) = &roots.common_dir {
            let refs = common.join("refs");
            if refs.exists() {
                match watcher.watch(&refs, RecursiveMode::Recursive) {
                    Ok(()) => root_count += 1,
                    Err(error) => failures.push(format!("{}: {error}", refs.display())),
                }
            }
            // info/exclude 是这个 worktree 的忽略规则之一（linked worktree 也读公共目录的）。
            let info = common.join("info");
            if info.is_dir() {
                match watcher.watch(&info, RecursiveMode::NonRecursive) {
                    Ok(()) => root_count += 1,
                    Err(error) => failures.push(format!("{}: {error}", info.display())),
                }
            }
            for name in ["packed-refs", "HEAD", "ORIG_HEAD"] {
                let file = common.join(name);
                if file.exists() {
                    match watcher.watch(&file, RecursiveMode::NonRecursive) {
                        Ok(()) => root_count += 1,
                        Err(error) => failures.push(format!("{}: {error}", file.display())),
                    }
                }
            }
        }

        if let Some(dir) = excludes_dir.filter(|dir| !dir.starts_with(&roots.worktree)) {
            match watcher.watch(&dir, RecursiveMode::NonRecursive) {
                Ok(()) => root_count += 1,
                Err(error) => failures.push(format!("{}: {error}", dir.display())),
            }
        }

        {
            let mut guard = status.lock().expect("监视状态锁");
            guard.active = root_count > 0;
            guard.roots = root_count;
            guard.error = if failures.is_empty() {
                None
            } else {
                Some(unavailable(failures.join("; ")))
            };
        }

        let thread_stop = stop.clone();
        let thread_status = status.clone();
        let thread_errors = runtime_errors.clone();
        // 工作线程创建失败同样必须降级为「自动刷新不可用」，不能静默吞掉。
        let thread = match std::thread::Builder::new()
            .name("revia-watch".into())
            .spawn(move || {
                while !thread_stop.load(Ordering::Relaxed) {
                    match rx.recv_timeout(DEBOUNCE) {
                        Ok(Ok(())) => {
                            // 合并窗口：把接下来的连续事件一起吞掉，只通知一次。
                            while matches!(rx.recv_timeout(DEBOUNCE), Ok(Ok(()))) {
                                if thread_stop.load(Ordering::Relaxed) {
                                    return;
                                }
                            }
                            if thread_stop.load(Ordering::Relaxed) {
                                return;
                            }
                            // 运行时错误立即把状态降级，避免谎报可用。
                            let runtime_error = thread_errors
                                .lock()
                                .ok()
                                .and_then(|guard| guard.first().cloned());
                            if let Some(error) = runtime_error {
                                if let Ok(mut guard) = thread_status.lock() {
                                    guard.active = false;
                                    guard.error = Some(unavailable(error));
                                }
                                continue;
                            }
                            on_change(activation);
                        }
                        Ok(Err(error)) => {
                            if let Ok(mut guard) = thread_status.lock() {
                                guard.active = false;
                                guard.error = Some(unavailable(error));
                            }
                        }
                        Err(RecvTimeoutError::Timeout) => continue,
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
            }) {
            Ok(handle) => Some(handle),
            Err(error) => {
                if let Ok(mut guard) = status.lock() {
                    guard.active = false;
                    guard.error = Some(
                        AppError::coded(
                            ErrorKind::Environment,
                            "errors.watch.threadStartFailed",
                            "自动刷新不可用：无法启动监视线程：{error}",
                            &[("error", error.to_string())],
                        )
                        .payload(),
                    );
                }
                None
            }
        };

        Self {
            activation,
            stop,
            status,
            thread,
            _watcher: Some(watcher),
        }
    }

    pub fn activation(&self) -> u64 {
        self.activation
    }

    pub fn status(&self) -> WatchStatus {
        self.status
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_else(|_| WatchStatus::idle())
    }
}

impl Drop for WorktreeWatcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// 事件是否与当前 worktree 的状态有关。
///
/// 除了丢弃只读访问事件（避免读取自身形成刷新环），还要按路径过滤：
/// 递归监视工作区会顺带覆盖 `.git` 子树，若不筛选就会把其他 worktree 的
/// 私有 index / 对象库变化也算成本 worktree 的改动（Q88）。
#[derive(Debug)]
pub struct WatchFilter {
    /// 工作区目录；其下的 `.git` 子树不算工作区改动。
    pub worktree: PathBuf,
    /// 该 worktree 自己的 gitDir；只有 index / HEAD / ORIG_HEAD 这类私有元数据相关。
    pub git_dir: PathBuf,
    /// 共享 commonDir；只有 refs 与 packed-refs / HEAD / ORIG_HEAD 相关。
    pub common_dir: Option<PathBuf>,
}

/// 这些文件名会影响当前 worktree 的状态（index 与 ref 指针）。
const GIT_METADATA_FILES: &[&str] = &["index", "HEAD", "ORIG_HEAD", "packed-refs"];

/// 尽可能得到真实路径：路径不存在时规范化父目录再拼回文件名。
fn canonicalize_path(path: &Path) -> PathBuf {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return canonical;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => match std::fs::canonicalize(parent) {
            Ok(canonical_parent) => canonical_parent.join(name),
            Err(_) => path.to_path_buf(),
        },
        _ => path.to_path_buf(),
    }
}

impl WatchFilter {
    fn from_roots(roots: &WatchRoots) -> Self {
        // 事件里的路径是平台规范化后的真实路径（macOS 上 `/var` → `/private/var`），
        // 因此根路径也必须规范化后再做前缀比较，否则会漏掉全部事件。
        let canonical = |path: &PathBuf| canonicalize_path(path);
        Self {
            worktree: canonical(&roots.worktree),
            git_dir: canonical(&roots.git_dir),
            common_dir: roots.common_dir.as_ref().map(canonical),
        }
    }

    /// 单个路径的相关性。
    ///
    /// 顺序很重要：`.git` 子树既可能属于工作区，也可能就是 gitDir / commonDir，
    /// 因此先判定共享 refs 与自有元数据，最后才回落到「工作区里的普通文件」。
    fn path_kind(&self, raw: &Path) -> PathKind {
        // 事件路径与根路径都可能是真实路径或未规范化的临时路径（macOS 的 /var 与 /private/var）。
        // 新建/删除的路径本身可能还不存在，此时规范化它的父目录再拼回文件名。
        let path_buf = canonicalize_path(raw);
        let path: &Path = &path_buf;
        let name_is_metadata = |path: &Path| {
            path.file_name()
                .map(|value| value.to_string_lossy().into_owned())
                .is_some_and(|name| GIT_METADATA_FILES.contains(&name.as_str()))
        };

        // 当前 linked worktree 的 gitDir 位于 commonDir 内，必须先识别它自己的元数据。
        if let Ok(relative) = path.strip_prefix(&self.git_dir) {
            let mut components = relative.components();
            let first = components.next();
            if first.is_some() && components.next().is_none() {
                return if name_is_metadata(path) {
                    PathKind::GitMetadata
                } else {
                    PathKind::Irrelevant
                };
            }
        }

        // 1. 共享 commonDir 的 refs 子树：任何深度都相关。
        //    仅当该目录确实不同于工作区本身时才按 commonDir 规则判定。
        if let Some(common) = self
            .common_dir
            .as_ref()
            .filter(|common| common.as_path() != self.worktree)
        {
            if let Ok(relative) = path.strip_prefix(common) {
                let mut components = relative.components();
                let first = components
                    .next()
                    .map(|part| part.as_os_str().to_string_lossy().into_owned());
                if first.as_deref() == Some("refs") {
                    return PathKind::GitMetadata;
                }
                // commonDir 顶层的 ref 指针文件（packed-refs / HEAD / ORIG_HEAD）。
                if first.is_some() && components.next().is_none() {
                    return if first.as_deref() != Some("index") && name_is_metadata(path) {
                        PathKind::GitMetadata
                    } else {
                        PathKind::Irrelevant
                    };
                }
                if relative == Path::new("info/exclude") || relative == Path::new("info").join("exclude") {
                    return PathKind::IgnoreRule;
                }
                // 其余（objects/、worktrees/<other>/ 等）与本 worktree 无关。
                return PathKind::Irrelevant;
            }
        }

        // 2. 该 worktree 自己的 gitDir 顶层元数据与 info/exclude
        if let Ok(relative) = path.strip_prefix(&self.git_dir) {
            if relative == Path::new("info/exclude") || relative == Path::new("info").join("exclude") {
                return PathKind::IgnoreRule;
            }
            let mut components = relative.components();
            let first = components.next();
            let single = first.is_some() && components.next().is_none();
            if single {
                return if name_is_metadata(path) {
                    PathKind::GitMetadata
                } else {
                    PathKind::Irrelevant
                };
            }
            // 更深的层级（例如 index.lock 之外的内容）不影响展示状态。
            return PathKind::Irrelevant;
        }

        // 3. 工作区里的普通文件改动；排除 `.git` 子树（含其他 worktree 的私有元数据）。
        if let Ok(relative) = path.strip_prefix(&self.worktree) {
            let mut components = relative.components();
            let first = components
                .next()
                .map(|part| part.as_os_str().to_string_lossy().into_owned());
            if first.as_deref() != Some(".git") {
                return PathKind::Worktree;
            }
            if relative == Path::new(".git/info/exclude") || relative == Path::new(".git").join("info").join("exclude") {
                return PathKind::IgnoreRule;
            }
            // 工作区内的 .git：只接受本 worktree 自己的顶层元数据文件。
            let single = components.next().is_some() && components.next().is_none();
            return if single && name_is_metadata(path) {
                PathKind::GitMetadata
            } else {
                PathKind::Irrelevant
            };
        }

        PathKind::Irrelevant
    }

    /// 判定一个事件：是否需要刷新、是否已被忽略（Q213、S1）。
    ///
    /// - `.git` 内部元数据：一定刷新；同时清空忽略缓存（index 变化后「已跟踪」判定会变）；
    /// - 忽略规则文件（`.gitignore`、`<gitDir>/info/exclude`、`core.excludesFile`）：清空缓存并刷新一次；
    /// - 工作区里的普通文件：交给 Git 的 `check-ignore` 判定，命中就跳过刷新。
    fn classify(&self, event: &Event, ignore: Option<&IgnoreMatcher>) -> Verdict {
        if matches!(event.kind, EventKind::Access(_)) {
            return Verdict::None;
        }
        // 有些平台（如 FSEvents）可能不携带路径；此时保守放行，避免漏刷新。
        if event.paths.is_empty() {
            return Verdict::Refresh(WatchEventKind::Normal);
        }
        let excludes = ignore.and_then(|m| m.excludes_file.as_deref());
        let kinds: Vec<(PathKind, PathBuf)> = event
            .paths
            .iter()
            .map(|path| {
                let canonical = canonicalize_path(path);
                // core.excludesFile 通常在仓库之外，按路径单独识别。
                let kind = if excludes.is_some_and(|file| canonical == file) {
                    PathKind::IgnoreRule
                } else {
                    self.path_kind(path)
                };
                (kind, canonical)
            })
            .collect();
        if kinds.iter().all(|(kind, _)| *kind == PathKind::Irrelevant) {
            return Verdict::None;
        }
        if kinds.iter().any(|(kind, _)| *kind == PathKind::GitMetadata) {
            if let Some(matcher) = ignore {
                matcher.invalidate();
            }
            return Verdict::Refresh(WatchEventKind::GitInternal);
        }
        let paths: Vec<PathBuf> = kinds
            .iter()
            .filter(|(kind, _)| *kind == PathKind::Worktree || *kind == PathKind::IgnoreRule)
            .map(|(_, path)| path.clone())
            .collect();
        if kinds.iter().any(|(kind, _)| *kind == PathKind::IgnoreRule)
            || paths.iter().any(|path| is_ignore_rule_file(path, excludes))
        {
            if let Some(matcher) = ignore {
                matcher.invalidate();
            }
            return Verdict::Refresh(WatchEventKind::Normal);
        }
        match ignore {
            // 没有 Git 判据时保守刷新：宁多刷一次，不漏改动。
            None => Verdict::Refresh(WatchEventKind::Normal),
            Some(matcher) => {
                if matcher.all_ignored(&paths) {
                    Verdict::Ignored
                } else {
                    Verdict::Refresh(WatchEventKind::Normal)
                }
            }
        }
    }

    /// 测试用：旧布尔语义（是否与当前 worktree 状态有关）。
    #[cfg(test)]
    fn path_relevant(&self, raw: &Path) -> bool {
        self.path_kind(raw) != PathKind::Irrelevant
    }

    /// 测试用：事件是否值得刷新（忽略命中时为 false）。
    #[cfg(test)]
    fn event_relevant(&self, event: &Event) -> bool {
        !matches!(self.classify(event, None), Verdict::None)
    }
}

/// 路径与当前 worktree 状态的关系。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathKind {
    Irrelevant,
    /// `.git` 内部元数据：index、HEAD、refs 等。
    GitMetadata,
    /// 忽略规则文件（.gitignore、<gitDir>/info/exclude 等）。
    IgnoreRule,
    /// 工作区里的普通文件（可能被忽略）。
    Worktree,
}

/// 一次事件判定的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// 与当前 worktree 无关，或只读访问。
    None,
    /// 被忽略路径的事件（S1：跳过刷新，只记入统计）。
    Ignored,
    /// 需要刷新，并计入对应类别的监视统计。
    Refresh(WatchEventKind),
}

/// Git 的默认全局忽略文件位置（未配置 `core.excludesFile` 时）。
fn default_excludes_file() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| crate::shell::home_dir().ok().map(|home| home.join(".config")))?;
    Some(base.join("git").join("ignore"))
}

/// 忽略规则文件：内容变化会让 `check-ignore` 的结论失效。
fn is_ignore_rule_file(path: &Path, excludes_file: Option<&Path>) -> bool {
    if path.file_name().is_some_and(|name| name == ".gitignore") {
        return true;
    }
    if path.ends_with("info/exclude") {
        return true;
    }
    if let Some(excludes) = excludes_file {
        if canonicalize_path(path) == *excludes {
            return true;
        }
    }
    false
}

/// 用 Git 自己的判定过滤被忽略的路径（Q213）。
///
/// 判定标准与 `git status` 完全一致：只看 Git 的结果，不在这里重新实现通配规则。
/// 结论缓存在内存里；`.git` 元数据或忽略规则文件变化时清空（index 变化会让
/// 「已跟踪因此不忽略」的结论失效）。
pub struct IgnoreMatcher {
    git: Git,
    /// 规范化后的工作区根；事件路径也是规范化的，比较前必须同口径。
    root: PathBuf,
    pub excludes_file: Option<PathBuf>,
    state: Mutex<IgnoreCache>,
}

#[derive(Debug, Default)]
struct IgnoreCache {
    /// 已确认被忽略的路径（文件或目录）。
    ignored: HashSet<PathBuf>,
    /// 已确认不被忽略的路径。
    tracked_or_normal: HashSet<PathBuf>,
}

/// 缓存上限：超过就整体清空，避免长时间运行无限增长。
const IGNORE_CACHE_LIMIT: usize = 4096;

impl IgnoreMatcher {
    /// `git` 的目录必须是工作区根目录（`check-ignore` 按工作区相对路径工作）。
    pub fn new(git: Git) -> Self {
        let root = canonicalize_path(&git.directory);
        // 未配置 core.excludesFile 时，Git 默认读 `$XDG_CONFIG_HOME/git/ignore`（或 `~/.config/git/ignore`）。
        let excludes_file = git
            .try_run(&["config", "--path", "core.excludesFile"])
            .and_then(|out| {
                let text = out.trim();
                (!text.is_empty()).then(|| PathBuf::from(text))
            })
            .or_else(default_excludes_file)
            .map(|path| canonicalize_path(&path));
        Self {
            git,
            root,
            excludes_file,
            state: Mutex::new(IgnoreCache::default()),
        }
    }

    pub fn invalidate(&self) {
        if let Ok(mut cache) = self.state.lock() {
            cache.ignored.clear();
            cache.tracked_or_normal.clear();
        }
    }

    /// 这些路径是否**全部**被忽略（任何一个没被忽略就要刷新）。
    pub fn all_ignored(&self, paths: &[PathBuf]) -> bool {
        if paths.is_empty() {
            return false;
        }
        // 与事件路径同口径：都规范化后再比较（macOS 的 /var 与 /private/var）。
        let paths: Vec<PathBuf> = paths.iter().map(|path| canonicalize_path(path)).collect();
        let mut pending: Vec<PathBuf> = Vec::new();
        {
            let Ok(cache) = self.state.lock() else {
                return false;
            };
            for path in &paths {
                if ancestor_ignored(&cache.ignored, path) || cache.ignored.contains(path) {
                    continue;
                }
                if cache.tracked_or_normal.contains(path) {
                    return false;
                }
                pending.push(path.clone());
            }
        }
        if pending.is_empty() {
            return true;
        }
        let Some(ignored) = self.query(&pending) else {
            return false;
        };
        if let Ok(mut cache) = self.state.lock() {
            if cache.ignored.len() + cache.tracked_or_normal.len() > IGNORE_CACHE_LIMIT {
                cache.ignored.clear();
                cache.tracked_or_normal.clear();
            }
            for path in &pending {
                if ignored.contains(path) {
                    cache.ignored.insert(path.clone());
                } else {
                    cache.tracked_or_normal.insert(path.clone());
                }
            }
        }
        pending.iter().all(|path| ignored.contains(path))
    }

    /// 一次 Git 调用判定一批路径；失败返回 None（调用方保守刷新）。
    fn query(&self, paths: &[PathBuf]) -> Option<HashSet<PathBuf>> {
        let root = &self.root;
        let mut input = Vec::new();
        let mut relative: HashMap<PathBuf, PathBuf> = HashMap::new();
        for path in paths {
            let relative_path = path.strip_prefix(root).ok()?.to_path_buf();
            input.extend_from_slice(relative_path.as_os_str().as_encoded_bytes());
            input.push(0);
            relative.insert(relative_path, path.clone());
        }
        let output = self.git.run_stdin(&["check-ignore", "--stdin", "-z"], input).ok()?;
        // 0 = 有被忽略的路径，1 = 一个都没有；其他退出码说明判定失败。
        if output.exit_code != 0 && output.exit_code != 1 {
            return None;
        }
        let mut ignored = HashSet::new();
        for chunk in output.stdout.split(|byte| *byte == 0) {
            if chunk.is_empty() {
                continue;
            }
            let name = PathBuf::from(String::from_utf8_lossy(chunk).into_owned());
            if let Some(original) = relative.get(&name) {
                ignored.insert(original.clone());
            }
        }
        Some(ignored)
    }
}

/// 是否有更上层的目录已被判定为忽略（忽略目录下的所有内容都不刷新）。
fn ancestor_ignored(ignored: &HashSet<PathBuf>, path: &Path) -> bool {
    let mut current = path.parent();
    while let Some(parent) = current {
        if ignored.contains(parent) {
            return true;
        }
        current = parent.parent();
    }
    false
}

struct WatchInstallState {
    current: Option<Arc<WorktreeWatcher>>,
    last: WatchStatus,
    accepted: u64,
    installation: u64,
}

/// 事件进度独立于激活身份；旧 watcher 的迟到通知不能覆盖新目标的通知。
#[derive(Debug, Clone, Copy, Default)]
pub struct WatchEvents {
    pub sequence: u64,
    pub activation: Option<u64>,
}

impl WatchEvents {
    pub fn record(&mut self, activation: u64) {
        if self.activation.is_some_and(|latest| activation < latest) {
            return;
        }
        self.sequence += 1;
        self.activation = Some(activation);
    }
}

/// 当前激活的监视器集合（只保留一个）。
pub struct WatchController {
    state: Mutex<WatchInstallState>,
    /// 测试接缝：非空时直接报告监视失败，不启动真实监视（Q95 确定性证据）。
    forced_failure: Mutex<Option<String>>,
}

impl Default for WatchController {
    fn default() -> Self {
        Self {
            state: Mutex::new(WatchInstallState {
                current: None,
                last: WatchStatus::idle(),
                accepted: 0,
                installation: 0,
            }),
            forced_failure: Mutex::new(None),
        }
    }
}

impl WatchController {
    pub fn new() -> Self {
        Self::default()
    }

    /// 强制下一次及后续激活失败（仅测试与诊断使用）。
    pub fn force_failure(&self, reason: Option<String>) {
        if let Ok(mut guard) = self.forced_failure.lock() {
            *guard = reason;
        }
    }

    /// 切换到新的 worktree。
    ///
    /// `requested` 是**前端发起时**的激活代号（A→B→A 时前端会给出更大的值）。
    /// 后端把请求按到达顺序受理，但只有代号不低于已受理最大值时才真正安装；
    /// 迟到进入的旧请求（代号更小）会被直接拒绝，不会把旧意图装成最新监视。
    pub fn activate<F>(&self, roots: WatchRoots, requested: u64, on_change: F) -> WatchStatus
    where
        F: Fn(u64) + Send + 'static,
    {
        self.activate_filtered(roots, requested, None, None, on_change)
    }

    /// 带 Git 忽略判定与性能计数的激活（M9 任务 04）。
    pub fn activate_filtered<F>(
        &self,
        roots: WatchRoots,
        requested: u64,
        ignore: Option<Arc<IgnoreMatcher>>,
        perf: Option<Arc<PerfStore>>,
        on_change: F,
    ) -> WatchStatus
    where
        F: Fn(u64) + Send + 'static,
    {
        self.activate_with_start(roots, requested, ignore, perf, on_change, || {})
    }

    fn activate_with_start<F, S>(
        &self,
        roots: WatchRoots,
        requested: u64,
        ignore: Option<Arc<IgnoreMatcher>>,
        perf: Option<Arc<PerfStore>>,
        on_change: F,
        before_start: S,
    ) -> WatchStatus
    where
        F: Fn(u64) + Send + 'static,
        S: FnOnce(),
    {
        let target = roots.worktree.display().to_string();
        let superseded = || WatchStatus {
            active: false,
            error: Some(
                AppError::coded(
                    ErrorKind::Rejected,
                    "errors.watch.superseded",
                    "自动刷新不可用：监视请求已被更新的切换取代",
                    &[],
                )
                .payload(),
            ),
            activation: requested,
            target: Some(target.clone()),
            roots: 0,
        };
        let installation = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if requested < state.accepted {
                return superseded();
            }
            state.accepted = requested;
            state.installation += 1;
            state.installation
        };
        before_start();
        let failure = self
            .forced_failure
            .lock()
            .ok()
            .and_then(|guard| guard.clone());
        let watcher = failure
            .is_none()
            .then(|| {
                Arc::new(WorktreeWatcher::start_with_ignore(
                    roots, requested, ignore, perf, on_change,
                ))
            });
        let status = match &watcher {
            Some(watcher) => watcher.status(),
            None => WatchStatus {
                active: false,
                error: failure.map(unavailable),
                activation: requested,
                target: Some(target.clone()),
                roots: 0,
            },
        };
        // 受理与最终安装共用同一把锁；慢启动不能覆盖更新的激活或停止请求。
        let previous = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if state.installation != installation {
                drop(state);
                drop(watcher);
                return superseded();
            }
            state.last = status.clone();
            std::mem::replace(&mut state.current, watcher)
        };
        drop(previous);
        status
    }

    /// 显式诊断接缝：停止当前通知，但保留实际失败状态直到重新激活。
    pub fn fail_current(&self, reason: String) {
        let watcher = self
            .state
            .lock()
            .ok()
            .and_then(|state| state.current.clone());
        if let Some(watcher) = watcher {
            watcher.stop.store(true, Ordering::Relaxed);
            if let Ok(mut status) = watcher.status.lock() {
                status.active = false;
                status.error = Some(unavailable(reason));
            }
        }
    }

    /// 停止监视（离开 worktree 或应用退出）。
    pub fn stop(&self) -> WatchStatus {
        let status = WatchStatus::idle();
        let previous = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.installation += 1;
            state.last = status.clone();
            state.current.take()
        };
        drop(previous);
        status
    }

    pub fn status(&self) -> WatchStatus {
        self.state
            .lock()
            .ok()
            .map(|state| {
                state
                    .current
                    .as_ref()
                    .map(|watcher| watcher.status())
                    .unwrap_or_else(|| state.last.clone())
            })
            .unwrap_or_else(WatchStatus::idle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_dot_git_reads_gitdir_file_for_linked_worktree() {
        let dir = tempfile::tempdir().unwrap();
        let worktree = dir.path().join("linked");
        std::fs::create_dir_all(&worktree).unwrap();
        let real_git_dir = dir.path().join("main/.git/worktrees/linked");
        std::fs::create_dir_all(&real_git_dir).unwrap();
        std::fs::write(
            worktree.join(".git"),
            format!("gitdir: {}\n", real_git_dir.display()),
        )
        .unwrap();

        let roots = resolve_roots(&worktree, None, None);
        assert_eq!(roots.git_dir, real_git_dir);
    }

    #[test]
    fn resolve_dot_git_uses_directory_when_present() {
        let dir = tempfile::tempdir().unwrap();
        let worktree = dir.path().join("main");
        std::fs::create_dir_all(worktree.join(".git")).unwrap();

        let roots = resolve_roots(&worktree, None, None);
        assert_eq!(roots.git_dir, worktree.join(".git"));
    }

    fn filter_for(root: &Path) -> WatchFilter {
        WatchFilter::from_roots(&resolve_roots(root, None, None))
    }

    #[test]
    fn access_events_are_filtered() {
        use notify::event::{AccessKind, AccessMode};
        let dir = tempfile::tempdir().unwrap();
        let filter = filter_for(dir.path());
        let access = Event::new(EventKind::Access(AccessKind::Close(AccessMode::Read)))
            .add_path(dir.path().join("x"));
        assert!(!filter.event_relevant(&access));
        let modify = Event::new(EventKind::Modify(notify::event::ModifyKind::Any))
            .add_path(dir.path().join("x"));
        assert!(filter.event_relevant(&modify));
    }

    #[test]
    fn filter_excludes_other_worktrees_and_objects() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("repo");
        std::fs::create_dir_all(root.join(".git/worktrees/other")).unwrap();
        std::fs::create_dir_all(root.join(".git/objects")).unwrap();
        std::fs::create_dir_all(root.join(".git/refs/heads")).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        // 真实运行时 commonDir 会由 Git 解析后传入（普通仓库就是 <root>/.git）。
        let filter = WatchFilter::from_roots(&resolve_roots(&root, None, Some(root.join(".git"))));

        let modify = |path: PathBuf| {
            Event::new(EventKind::Modify(notify::event::ModifyKind::Any)).add_path(path)
        };

        assert!(
            filter.event_relevant(&modify(root.join("src/app.ts"))),
            "工作区文件改动必须相关；worktree={:?} git_dir={:?} common={:?}",
            filter.worktree,
            filter.git_dir,
            filter.common_dir
        );
        assert!(
            filter.event_relevant(&modify(root.join(".git/index"))),
            "自己的 index 必须相关"
        );
        assert!(
            filter.event_relevant(&modify(root.join(".git/refs/heads/main"))),
            "refs 必须相关"
        );
        assert!(
            !filter.event_relevant(&modify(root.join(".git/worktrees/other/index"))),
            "其他 worktree 的私有 index 不得相关"
        );
        assert!(
            !filter.event_relevant(&modify(root.join(".git/objects/ab/cdef"))),
            "对象库不得相关"
        );
    }

    #[test]
    fn linked_filter_keeps_own_metadata_and_shared_refs() {
        let dir = tempfile::tempdir().unwrap();
        let common = dir.path().join("main/.git");
        let linked = dir.path().join("linked");
        let own = common.join("worktrees/linked");
        for path in [
            &own,
            &common.join("worktrees/other"),
            &common.join("refs/heads"),
            &linked,
        ] {
            std::fs::create_dir_all(path).unwrap();
        }
        let filter = WatchFilter::from_roots(&resolve_roots(
            &linked,
            Some(own.clone()),
            Some(common.clone()),
        ));
        assert!(filter.path_relevant(&own.join("index")));
        assert!(filter.path_relevant(&own.join("HEAD")));
        assert!(filter.path_relevant(&common.join("refs/heads/main")));
        assert!(!filter.path_relevant(&common.join("worktrees/other/index")));
        assert!(!filter.path_relevant(&common.join("objects/ab/cdef")));
    }

    #[test]
    fn event_sequence_advances_within_activation_and_rejects_old_notifications() {
        let mut events = WatchEvents::default();
        events.record(1);
        assert_eq!((events.sequence, events.activation), (1, Some(1)));
        events.record(1);
        assert_eq!((events.sequence, events.activation), (2, Some(1)));
        events.record(2);
        events.record(1);
        assert_eq!((events.sequence, events.activation), (3, Some(2)));
    }

    #[test]
    fn early_request_cannot_install_after_new_target_or_stop() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let controller = Arc::new(WatchController::new());
        for (activation, stop) in [(1, false), (3, true)] {
            let (entered_tx, entered_rx) = channel();
            let (release_tx, release_rx) = channel();
            let pending = controller.clone();
            let roots = resolve_roots(&a, None, None);
            let worker = std::thread::spawn(move || {
                pending.activate_with_start(
                    roots,
                    activation,
                    None,
                    None,
                    |_| {},
                    || {
                        entered_tx.send(()).unwrap();
                        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    },
                )
            });
            entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            if stop {
                controller.stop();
            } else {
                assert!(
                    controller
                        .activate(resolve_roots(&b, None, None), 2, |_| {})
                        .active
                );
            }
            release_tx.send(()).unwrap();
            assert!(!worker.join().unwrap().active, "早受理的请求不得晚安装");
            let current = controller.status();
            if stop {
                assert_eq!(current, WatchStatus::idle());
            } else {
                assert_eq!(current.activation, 2);
                assert_eq!(current.target, Some(b.display().to_string()));
            }
        }
    }

    #[test]
    fn watcher_reports_failure_for_missing_worktree() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist");
        let roots = resolve_roots(&missing, None, None);
        let watcher = WorktreeWatcher::start(roots, 7, |_| {});
        let status = watcher.status();
        assert_eq!(status.activation, 7);
        assert!(!status.active, "不存在的目录不应报告为已监视");
        assert!(status.error.is_some(), "监视失败必须给出原因，不能静默");
    }

    #[test]
    fn controller_replaces_previous_watcher_on_switch() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first");
        let second = dir.path().join("second");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();

        let controller = WatchController::new();
        let status_a = controller.activate(resolve_roots(&first, None, None), 1, |_| {});
        assert!(status_a.active);
        let status_b = controller.activate(resolve_roots(&second, None, None), 2, |_| {});
        assert!(status_b.active);
        assert!(
            status_b.activation > status_a.activation,
            "切换必须产生新的激活代号"
        );
        assert_eq!(
            status_b.target.as_deref(),
            Some(second.display().to_string().as_str())
        );

        let stopped = controller.stop();
        assert!(!stopped.active);
    }

    /// 迟到进入的旧激活请求不得替换当前监视（A→B→A 的旧 A）。
    #[test]
    fn late_activation_request_does_not_replace_current_watcher() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();

        let controller = WatchController::new();
        // 前端先发起 A(1)，再发起 B(2)，最后又回到 A(3)。
        let status_a1 = controller.activate(resolve_roots(&a, None, None), 1, |_| {});
        assert!(status_a1.active);
        let status_b = controller.activate(resolve_roots(&b, None, None), 2, |_| {});
        assert_eq!(
            status_b.target.as_deref(),
            Some(b.display().to_string().as_str())
        );

        // 迟到进入的旧 A(1) 请求：必须被拒绝，不能把监视切回 A。
        let late = controller.activate(resolve_roots(&a, None, None), 1, |_| {});
        assert!(!late.active, "迟到请求不得报告为已安装");
        assert!(late.error.is_some(), "迟到请求必须给出明确原因");
        assert_eq!(
            controller.status().target.as_deref(),
            Some(b.display().to_string().as_str()),
            "当前监视仍应是 B，而不是被旧 A 覆盖"
        );

        // 真正的新 A(3) 才能切换过去。
        let status_a3 = controller.activate(resolve_roots(&a, None, None), 3, |_| {});
        assert!(status_a3.active);
        assert_eq!(
            status_a3.target.as_deref(),
            Some(a.display().to_string().as_str())
        );
    }

    /// 切换后旧目录的改动不得再触发当前目标的刷新通知。
    #[test]
    fn superseded_watcher_stops_emitting_notifications() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();

        let hits = Arc::new(Mutex::new(Vec::<u64>::new()));
        let controller = WatchController::new();
        let sink = hits.clone();
        let status_a = controller.activate(resolve_roots(&a, None, None), 1, move |activation| {
            sink.lock().unwrap().push(activation);
        });
        assert!(status_a.active);

        let sink = hits.clone();
        let status_b = controller.activate(resolve_roots(&b, None, None), 2, move |activation| {
            sink.lock().unwrap().push(activation);
        });
        assert!(status_b.active);

        // 旧目录 A 的写入必须不再产生通知（旧 watcher 已被替换并释放）。
        std::fs::write(a.join("old.txt"), b"changed in a").unwrap();
        std::thread::sleep(Duration::from_millis(600));

        // 新目录 B 的写入必须产生属于 B 的通知。
        std::fs::write(b.join("existing.txt"), b"initial in b").unwrap();
        std::thread::sleep(Duration::from_millis(300));
        std::fs::write(b.join("existing.txt"), b"changed in b").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if hits
                .lock()
                .unwrap()
                .iter()
                .any(|activation| *activation == 2)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        let observed = hits.lock().unwrap().clone();
        assert!(
            !observed.iter().any(|activation| *activation == 1),
            "旧目录 A 的改动不得触发通知，实际收到：{observed:?}"
        );
        assert!(
            observed.iter().any(|activation| *activation == 2),
            "当前目录 B 的改动必须触发通知，实际收到：{observed:?}"
        );
    }

    #[test]
    fn forced_failure_is_reported_and_cleared_on_stop() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("repo");
        std::fs::create_dir_all(&path).unwrap();

        let controller = WatchController::new();
        controller.force_failure(Some("测试注入的监视失败".into()));
        let status = controller.activate(resolve_roots(&path, None, None), 1, |_| {});
        assert!(!status.active);
        assert_eq!(
            status.error.as_ref().map(|e| e.message.as_str()),
            Some("自动刷新不可用：测试注入的监视失败")
        );

        // 手动刷新不改变监视失败状态：不能假装监视已恢复。
        assert!(!controller.status().active);
        assert!(controller.status().error.is_some());
    }
}

#[cfg(test)]
mod ignore_tests {
    use super::*;
    use crate::git::CommandLog;
    use crate::shell::{EnvCaptureInfo, ShellEnv};
    use std::process::Command;

    fn test_shell_env(dir: &Path) -> ShellEnv {
        let path: Vec<String> = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|entry| entry.to_string_lossy().into_owned())
            .collect();
        let vars = [
            (
                "HOME".to_string(),
                std::env::var("HOME").unwrap_or_default(),
            ),
            (
                "PATH".to_string(),
                std::env::var("PATH").unwrap_or_default(),
            ),
        ]
        .into_iter()
        .collect();
        ShellEnv {
            vars,
            path: path.clone(),
            info: EnvCaptureInfo {
                worktree: dir.display().to_string(),
                shell: "fixture".into(),
                elapsed_ms: 0,
                timed_out: false,
                used_fallback: false,
                var_count: 2,
                path_entries: path,
                fallback_reason: None,
                fallback_detail: None,
            },
        }
    }

    fn git_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::create_dir_all(repo.join("node_modules/pkg")).unwrap();
        std::fs::write(repo.join(".gitignore"), "node_modules/\n*.log\n").unwrap();
        std::fs::write(repo.join("node_modules/pkg/index.js"), "x").unwrap();
        std::fs::write(repo.join("debug.log"), "x").unwrap();
        std::fs::write(repo.join("src/app.ts"), "x").unwrap();
        for args in [
            vec!["init", "-q"],
            vec!["add", ".gitignore", "src/app.ts"],
        ] {
            let status = Command::new("git").args(&args).current_dir(&repo).status().unwrap();
            assert!(status.success());
        }
        dir
    }

    fn matcher(repo: &Path) -> IgnoreMatcher {
        let env = test_shell_env(repo);
        let git = Git::new(repo, &env, Arc::new(CommandLog::default())).unwrap();
        IgnoreMatcher::new(git)
    }

    #[test]
    fn git_decides_ignored_and_normal_paths() {
        let dir = git_repo();
        let repo = dir.path().join("repo");
        let matcher = matcher(&repo);
        assert!(matcher.all_ignored(&[repo.join("node_modules/pkg/index.js")]));
        assert!(matcher.all_ignored(&[repo.join("debug.log")]));
        assert!(!matcher.all_ignored(&[repo.join("src/app.ts")]));
        // 混合批次：只要有一个没被忽略，就必须刷新。
        assert!(!matcher.all_ignored(&[
            repo.join("debug.log"),
            repo.join("src/app.ts")
        ]));
        // 已跟踪文件不会被忽略。
        assert!(!matcher.all_ignored(&[repo.join(".gitignore")]));
    }

    #[test]
    fn invalidate_requeries_after_rules_change() {
        let dir = git_repo();
        let repo = dir.path().join("repo");
        let matcher = matcher(&repo);
        let target = repo.join("src/other.ts");
        std::fs::write(&target, "x").unwrap();
        assert!(!matcher.all_ignored(&[target.clone()]));
        // 规则变化后缓存失效，重新问 Git。
        std::fs::write(repo.join(".gitignore"), "src/other.ts\n").unwrap();
        matcher.invalidate();
        assert!(matcher.all_ignored(&[target]));
    }

    #[test]
    fn ignore_rule_and_metadata_events_always_refresh() {
        let dir = git_repo();
        let repo = dir.path().join("repo");
        let roots = resolve_roots(&repo, None, None);
        let filter = WatchFilter::from_roots(&roots);
        let modify = |path: PathBuf| Event {
            kind: EventKind::Modify(notify::event::ModifyKind::Any),
            paths: vec![path],
            attrs: Default::default(),
        };
        assert_eq!(
            filter.classify(&modify(repo.join(".gitignore")), None),
            Verdict::Refresh(WatchEventKind::Normal)
        );
        assert_eq!(
            filter.classify(&modify(repo.join(".git/index")), None),
            Verdict::Refresh(WatchEventKind::GitInternal)
        );
        // 没有 Git 判据时保守刷新。
        assert_eq!(
            filter.classify(&modify(repo.join("debug.log")), None),
            Verdict::Refresh(WatchEventKind::Normal)
        );
    }

    #[test]
    fn ignored_events_do_not_refresh_when_matcher_is_present() {
        let dir = git_repo();
        let repo = dir.path().join("repo");
        let roots = resolve_roots(&repo, None, None);
        let filter = WatchFilter::from_roots(&roots);
        let matcher = matcher(&repo);
        let modify = |path: PathBuf| Event {
            kind: EventKind::Modify(notify::event::ModifyKind::Any),
            paths: vec![path],
            attrs: Default::default(),
        };
        assert_eq!(
            filter.classify(&modify(repo.join("node_modules/pkg/index.js")), Some(&matcher)),
            Verdict::Ignored
        );
        assert_eq!(
            filter.classify(&modify(repo.join("src/app.ts")), Some(&matcher)),
            Verdict::Refresh(WatchEventKind::Normal)
        );
        // S1：.git/info/exclude 变更清空缓存并刷新
        assert_eq!(
            filter.classify(&modify(repo.join(".git/info/exclude")), Some(&matcher)),
            Verdict::Refresh(WatchEventKind::Normal)
        );
    }

    /// S1：仓库外的 core.excludesFile 变化也清空缓存并刷新；其他仓库外的路径照旧不相关。
    #[test]
    fn excludes_file_outside_worktree_invalidates_and_refreshes() {
        let dir = git_repo();
        let repo = dir.path().join("repo");
        let global = dir.path().join("global-ignore");
        std::fs::write(&global, "src/extra.ts\n").unwrap();
        // 只写测试仓库自己的配置（fixture），不碰用户的 Git 配置。
        let status = Command::new("git")
            .args(["config", "core.excludesFile", global.to_str().unwrap()])
            .current_dir(&repo)
            .status()
            .unwrap();
        assert!(status.success());
        let roots = resolve_roots(&repo, None, None);
        let filter = WatchFilter::from_roots(&roots);
        let matcher = matcher(&repo);
        assert_eq!(matcher.excludes_file.as_deref(), Some(canonicalize_path(&global).as_path()));
        let target = repo.join("src/extra.ts");
        std::fs::write(&target, "x").unwrap();
        assert!(matcher.all_ignored(&[target.clone()]));

        let modify = |path: PathBuf| Event {
            kind: EventKind::Modify(notify::event::ModifyKind::Any),
            paths: vec![path],
            attrs: Default::default(),
        };
        std::fs::write(&global, "").unwrap();
        assert_eq!(
            filter.classify(&modify(global.clone()), Some(&matcher)),
            Verdict::Refresh(WatchEventKind::Normal)
        );
        // 缓存已清空：重新问 Git，规则删除后不再被忽略。
        assert!(!matcher.all_ignored(&[target]));
        assert_eq!(
            filter.classify(&modify(dir.path().join("unrelated.txt")), Some(&matcher)),
            Verdict::None
        );
    }
}
