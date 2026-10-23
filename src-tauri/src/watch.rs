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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
    /// 已建立的监视路径数量。
    pub roots: usize,
}

impl WatchStatus {
    pub fn idle() -> Self {
        Self { active: false, error: None, activation: 0, roots: 0 }
    }
}

/// 监视控制句柄：持有 notify watcher 与工作线程。
pub struct WorktreeWatcher {
    activation: u64,
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<WatchStatus>>,
    thread: Option<std::thread::JoinHandle<()>>,
    /// 保留 watcher 的所有权；drop 时停止底层监视。
    _watcher: notify::RecommendedWatcher,
}

/// 需要监视的路径集合。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchRoots {
    /// 工作区目录（递归）。
    pub worktree: PathBuf,
    /// 该 worktree 自己的 gitDir（递归，包含 index/HEAD）。
    pub git_dir: PathBuf,
    /// 共享 commonDir（非递归，只关心 refs 与 packed-refs 等顶层元数据）。
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
            roots: 0,
        }));

        let (tx, rx): (Sender<()>, Receiver<()>) = channel();
        let mut root_count = 0usize;
        let mut failures: Vec<String> = Vec::new();

        let mut watcher = match notify::recommended_watcher(move |result: notify::Result<Event>| {
            match result {
                Ok(event) => {
                    if is_relevant(&event) {
                        let _ = tx.send(());
                    }
                }
                Err(_) => {
                    // 事件层面的错误不改变整体可用性，交由路径级错误处理。
                }
            }
        }) {
            Ok(watcher) => watcher,
            Err(error) => {
                if let Ok(mut guard) = status.lock() {
                    guard.error = Some(format!("无法启动文件监视：{error}"));
                }
                return Self { activation, stop, status, thread: None, _watcher: notify::recommended_watcher(|_| {}).expect("回退监视器不可用") };
            }
        };

        // 工作区递归：编辑器改动、新建/删除/重命名都在这里。
        match watcher.watch(&roots.worktree, RecursiveMode::Recursive) {
            Ok(()) => root_count += 1,
            Err(error) => failures.push(format!("工作区 {}：{error}", roots.worktree.display())),
        }
        // 该 worktree 的 gitDir 递归：index、HEAD 等直接元数据。
        if roots.git_dir != roots.worktree {
            match watcher.watch(&roots.git_dir, RecursiveMode::Recursive) {
                Ok(()) => root_count += 1,
                Err(error) => failures.push(format!("Git 目录 {}：{error}", roots.git_dir.display())),
            }
        }
        // 共享 commonDir 只非递归：refs/ 与 packed-refs 的变化由 refs 目录自身递归覆盖。
        if let Some(common) = &roots.common_dir {
            let refs = common.join("refs");
            for path in [refs.as_path(), common.as_path()].iter() {
                if path.exists() {
                    match watcher.watch(path, RecursiveMode::Recursive) {
                        Ok(()) => root_count += 1,
                        Err(error) => failures.push(format!("{}：{error}", path.display())),
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
        let thread = std::thread::Builder::new()
            .name("revia-watch".into())
            .spawn(move || {
                while !thread_stop.load(Ordering::Relaxed) {
                    match rx.recv_timeout(DEBOUNCE) {
                        Ok(()) => {
                            // 合并窗口：把接下来的连续事件一起吞掉，只通知一次。
                            while rx.recv_timeout(DEBOUNCE).is_ok() {
                                if thread_stop.load(Ordering::Relaxed) {
                                    return;
                                }
                            }
                            if thread_stop.load(Ordering::Relaxed) {
                                return;
                            }
                            on_change(activation);
                        }
                        Err(RecvTimeoutError::Timeout) => continue,
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
            })
            .ok();

        Self { activation, stop, status, thread, _watcher: watcher }
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

/// 过滤只读访问事件，避免读取自身形成刷新环。
fn is_relevant(event: &Event) -> bool {
    !matches!(event.kind, EventKind::Access(_))
}

/// 当前激活的监视器集合（只保留一个）。
pub struct WatchController {
    current: Mutex<Option<Arc<WorktreeWatcher>>>,
    counter: AtomicU64,
    /// 测试接缝：非空时直接报告监视失败，不启动真实监视（Q95 确定性证据）。
    forced_failure: Mutex<Option<String>>,
    /// 最近一次激活 / 停止的结果；没有 watcher 时仍保留失败原因（Q95）。
    last: Mutex<WatchStatus>,
}

impl Default for WatchController {
    fn default() -> Self {
        Self {
            current: Mutex::new(None),
            counter: AtomicU64::new(0),
            forced_failure: Mutex::new(None),
            last: Mutex::new(WatchStatus::idle()),
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

    /// 切换到新的 worktree：先释放旧监视，再启动新的。
    pub fn activate<F>(&self, roots: WatchRoots, on_change: F) -> WatchStatus
    where
        F: Fn(u64) + Send + 'static,
    {
        let activation = self.counter.fetch_add(1, Ordering::Relaxed) + 1;
        if let Some(reason) = self.forced_failure.lock().ok().and_then(|guard| guard.clone()) {
            if let Ok(mut guard) = self.current.lock() {
                guard.take();
            }
            let status = WatchStatus { active: false, error: Some(reason), activation, roots: 0 };
            self.remember(&status);
            return status;
        }
        let watcher = Arc::new(WorktreeWatcher::start(roots, activation, on_change));
        let status = watcher.status();
        if let Ok(mut guard) = self.current.lock() {
            // 旧 watcher 在这里被 drop：停止线程与底层监视。
            *guard = Some(watcher);
        }
        self.remember(&status);
        status
    }

    fn remember(&self, status: &WatchStatus) {
        if let Ok(mut guard) = self.last.lock() {
            *guard = status.clone();
        }
    }

    /// 停止监视（离开 worktree 或应用退出）。
    pub fn stop(&self) -> WatchStatus {
        if let Ok(mut guard) = self.current.lock() {
            guard.take();
        }
        let status = WatchStatus::idle();
        self.remember(&status);
        status
    }

    pub fn status(&self) -> WatchStatus {
        self.current
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|watcher| watcher.status()))
            .or_else(|| self.last.lock().ok().map(|guard| guard.clone()))
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

    #[test]
    fn access_events_are_filtered() {
        use notify::event::{AccessKind, AccessMode};
        let access = Event::new(EventKind::Access(AccessKind::Close(AccessMode::Read)))
            .add_path(PathBuf::from("/tmp/x"));
        assert!(!is_relevant(&access));
        let modify = Event::new(EventKind::Modify(notify::event::ModifyKind::Any))
            .add_path(PathBuf::from("/tmp/x"));
        assert!(is_relevant(&modify));
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
        let status_a = controller.activate(resolve_roots(&first, None, None), |_| {});
        assert!(status_a.active);
        let status_b = controller.activate(resolve_roots(&second, None, None), |_| {});
        assert!(status_b.active);
        assert!(status_b.activation > status_a.activation, "切换必须产生新的激活代号");

        let stopped = controller.stop();
        assert!(!stopped.active);
    }

    #[test]
    fn forced_failure_is_reported_and_cleared_on_stop() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("repo");
        std::fs::create_dir_all(&path).unwrap();

        let controller = WatchController::new();
        controller.force_failure(Some("自动刷新不可用：测试注入的监视失败".into()));
        let status = controller.activate(resolve_roots(&path, None, None), |_| {});
        assert!(!status.active);
        assert_eq!(status.error.as_deref(), Some("自动刷新不可用：测试注入的监视失败"));

        // 手动刷新不改变监视失败状态：不能假装监视已恢复。
        assert!(!controller.status().active);
        assert!(controller.status().error.is_some());
    }
}
