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

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify::{Event, EventKind, RecursiveMode, Watcher};
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
    pub error: Option<String>,
    /// 本次激活的代号；前端用它拒绝旧 worktree 的迟到事件。
    pub activation: u64,
    /// 当前监视目标（worktree 路径）；前端据此确认状态属于自己请求的目标。
    pub target: Option<String>,
    /// 已建立的监视路径数量。
    pub roots: usize,
}

impl WatchStatus {
    pub fn idle() -> Self {
        Self { active: false, error: None, activation: 0, target: None, roots: 0 }
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
pub fn resolve_roots(worktree: &Path, git_dir: Option<PathBuf>, common_dir: Option<PathBuf>) -> WatchRoots {
    let resolved_git_dir = git_dir.unwrap_or_else(|| resolve_dot_git(worktree));
    WatchRoots { worktree: worktree.to_path_buf(), git_dir: resolved_git_dir, common_dir }
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
            return if target.is_absolute() { target } else { worktree.join(target) };
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
        let filter = WatchFilter::from_roots(&roots);
        let mut watcher = match notify::recommended_watcher(move |result: notify::Result<Event>| {
            match result {
                Ok(event) => {
                    if filter.event_relevant(&event) {
                        let _ = tx.send(Ok(()));
                    }
                }
                Err(error) => {
                    let text = format!("文件监视运行出错：{error}");
                    if let Ok(mut guard) = callback_errors.lock() {
                        if guard.is_empty() {
                            guard.push(text.clone());
                        }
                    }
                    let _ = tx.send(Err(text));
                }
            }
        }) {
            Ok(watcher) => watcher,
            Err(error) => {
                // 启动失败就是降级，不重试、不 panic、不谎报 active。
                if let Ok(mut guard) = status.lock() {
                    guard.active = false;
                    guard.error = Some(format!("自动刷新不可用：无法启动文件监视：{error}"));
                }
                return Self { activation, stop, status, thread: None, _watcher: None };
            }
        };

        // 工作区递归：编辑器改动、新建/删除/原子替换都在这里。
        match watcher.watch(&roots.worktree, RecursiveMode::Recursive) {
            Ok(()) => root_count += 1,
            Err(error) => failures.push(format!("工作区 {}：{error}", roots.worktree.display())),
        }
        // 该 worktree 自己的 gitDir：index / HEAD / ORIG_HEAD 等私有元数据。
        //
        // 对普通仓库这是 `<worktree>/.git`，已被上面的工作区递归覆盖（含 index），无需重复注册；
        // 对 linked worktree 它是 `<common>/worktrees/<name>`，必须单独监视。
        if roots.git_dir != roots.worktree.join(".git") && !roots.git_dir.starts_with(&roots.worktree) {
            match watcher.watch(&roots.git_dir, RecursiveMode::Recursive) {
                Ok(()) => root_count += 1,
                Err(error) => failures.push(format!("Git 目录 {}：{error}", roots.git_dir.display())),
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
                    Err(error) => failures.push(format!("{}：{error}", refs.display())),
                }
            }
            for name in ["packed-refs", "HEAD", "ORIG_HEAD"] {
                let file = common.join(name);
                if file.exists() {
                    match watcher.watch(&file, RecursiveMode::NonRecursive) {
                        Ok(()) => root_count += 1,
                        Err(error) => failures.push(format!("{}：{error}", file.display())),
                    }
                }
            }
        }

        {
            let mut guard = status.lock().expect("监视状态锁");
            guard.active = root_count > 0;
            guard.roots = root_count;
            guard.error = if failures.is_empty() {
                None
            } else {
                Some(format!("自动刷新不可用：{}", failures.join("；")))
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
                            let runtime_error = thread_errors.lock().ok().and_then(|guard| guard.first().cloned());
                            if let Some(error) = runtime_error {
                                if let Ok(mut guard) = thread_status.lock() {
                                    guard.active = false;
                                    guard.error = Some(format!("自动刷新不可用：{error}"));
                                }
                                continue;
                            }
                            on_change(activation);
                        }
                        Ok(Err(error)) => {
                            if let Ok(mut guard) = thread_status.lock() {
                                guard.active = false;
                                guard.error = Some(format!("自动刷新不可用：{error}"));
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
                    guard.error = Some(format!("自动刷新不可用：无法启动监视线程：{error}"));
                }
                None
            }
        };

        Self { activation, stop, status, thread, _watcher: Some(watcher) }
    }

    pub fn activation(&self) -> u64 {
        self.activation
    }

    pub fn status(&self) -> WatchStatus {
        self.status.lock().map(|guard| guard.clone()).unwrap_or_else(|_| WatchStatus::idle())
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
#[derive(Debug, Clone)]
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

    /// 单个路径是否相关。
    ///
    /// 顺序很重要：`.git` 子树既可能属于工作区，也可能就是 gitDir / commonDir，
    /// 因此先判定共享 refs 与自有元数据，最后才回落到「工作区里的普通文件」。
    fn path_relevant(&self, raw: &Path) -> bool {
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
                return name_is_metadata(path);
            }
        }

        // 1. 共享 commonDir 的 refs 子树：任何深度都相关。
        //    仅当该目录确实不同于工作区本身时才按 commonDir 规则判定。
        if let Some(common) = self.common_dir.as_ref().filter(|common| common.as_path() != self.worktree) {
            if let Ok(relative) = path.strip_prefix(common) {
                let mut components = relative.components();
                let first = components.next().map(|part| part.as_os_str().to_string_lossy().into_owned());
                if first.as_deref() == Some("refs") {
                    return true;
                }
                // commonDir 顶层的 ref 指针文件（packed-refs / HEAD / ORIG_HEAD）。
                if first.is_some() && components.next().is_none() {
                    return first.as_deref() != Some("index") && name_is_metadata(path);
                }
                // 其余（objects/、worktrees/<other>/ 等）与本 worktree 无关。
                return false;
            }
        }

        // 2. 该 worktree 自己的 gitDir 顶层元数据（linked worktree 位于 commonDir/worktrees/<name>）。
        if let Ok(relative) = path.strip_prefix(&self.git_dir) {
            let mut components = relative.components();
            let first = components.next();
            let single = first.is_some() && components.next().is_none();
            if single {
                return name_is_metadata(path);
            }
            // 更深的层级（例如 index.lock 之外的内容）不影响展示状态。
            return false;
        }

        // 3. 工作区里的普通文件改动；排除 `.git` 子树（含其他 worktree 的私有元数据）。
        if let Ok(relative) = path.strip_prefix(&self.worktree) {
            let mut components = relative.components();
            let first = components.next().map(|part| part.as_os_str().to_string_lossy().into_owned());
            if first.as_deref() != Some(".git") {
                return true;
            }
            // 工作区内的 .git：只接受本 worktree 自己的顶层元数据文件。
            let single = components.next().is_some() && components.next().is_none();
            return single && name_is_metadata(path);
        }

        false
    }

    fn event_relevant(&self, event: &Event) -> bool {
        if matches!(event.kind, EventKind::Access(_)) {
            return false;
        }
        // 有些平台（如 FSEvents）可能不携带路径；此时保守放行，避免漏刷新。
        if event.paths.is_empty() {
            return true;
        }
        event.paths.iter().any(|path| self.path_relevant(path))
    }
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
        self.activate_with_start(roots, requested, on_change, || {})
    }

    fn activate_with_start<F, S>(&self, roots: WatchRoots, requested: u64, on_change: F, before_start: S) -> WatchStatus
    where
        F: Fn(u64) + Send + 'static,
        S: FnOnce(),
    {
        let target = roots.worktree.display().to_string();
        let superseded = || WatchStatus {
            active: false,
            error: Some("自动刷新不可用：监视请求已被更新的切换取代".into()),
            activation: requested,
            target: Some(target.clone()),
            roots: 0,
        };
        let installation = {
            let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if requested < state.accepted {
                return superseded();
            }
            state.accepted = requested;
            state.installation += 1;
            state.installation
        };
        before_start();
        let failure = self.forced_failure.lock().ok().and_then(|guard| guard.clone());
        let watcher = failure.is_none().then(|| Arc::new(WorktreeWatcher::start(roots, requested, on_change)));
        let status = match &watcher {
            Some(watcher) => watcher.status(),
            None => WatchStatus {
                active: false,
                error: failure,
                activation: requested,
                target: Some(target.clone()),
                roots: 0,
            },
        };
        // 受理与最终安装共用同一把锁；慢启动不能覆盖更新的激活或停止请求。
        let previous = {
            let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
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
        let watcher = self.state.lock().ok().and_then(|state| state.current.clone());
        if let Some(watcher) = watcher {
            watcher.stop.store(true, Ordering::Relaxed);
            if let Ok(mut status) = watcher.status.lock() {
                status.active = false;
                status.error = Some(reason);
            }
        }
    }

    /// 停止监视（离开 worktree 或应用退出）。
    pub fn stop(&self) -> WatchStatus {
        let status = WatchStatus::idle();
        let previous = {
            let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            state.installation += 1;
            state.last = status.clone();
            state.current.take()
        };
        drop(previous);
        status
    }

    pub fn status(&self) -> WatchStatus {
        self.state.lock().ok()
            .map(|state| state.current.as_ref().map(|watcher| watcher.status()).unwrap_or_else(|| state.last.clone()))
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
        std::fs::write(worktree.join(".git"), format!("gitdir: {}\n", real_git_dir.display())).unwrap();

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
            filter.worktree, filter.git_dir, filter.common_dir
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
        for path in [&own, &common.join("worktrees/other"), &common.join("refs/heads"), &linked] {
            std::fs::create_dir_all(path).unwrap();
        }
        let filter = WatchFilter::from_roots(&resolve_roots(&linked, Some(own.clone()), Some(common.clone())));
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
                pending.activate_with_start(roots, activation, |_| {}, || {
                    entered_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                })
            });
            entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            if stop {
                controller.stop();
            } else {
                assert!(controller.activate(resolve_roots(&b, None, None), 2, |_| {}).active);
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
        assert!(status_b.activation > status_a.activation, "切换必须产生新的激活代号");
        assert_eq!(status_b.target.as_deref(), Some(second.display().to_string().as_str()));

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
        assert_eq!(status_b.target.as_deref(), Some(b.display().to_string().as_str()));

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
        assert_eq!(status_a3.target.as_deref(), Some(a.display().to_string().as_str()));
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
            if hits.lock().unwrap().iter().any(|activation| *activation == 2) {
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
        controller.force_failure(Some("自动刷新不可用：测试注入的监视失败".into()));
        let status = controller.activate(resolve_roots(&path, None, None), 1, |_| {});
        assert!(!status.active);
        assert_eq!(status.error.as_deref(), Some("自动刷新不可用：测试注入的监视失败"));

        // 手动刷新不改变监视失败状态：不能假装监视已恢复。
        assert!(!controller.status().active);
        assert!(controller.status().error.is_some());
    }
}
