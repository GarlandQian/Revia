//! Worktree 运行环境生命周期（M10 任务 02）。
//!
//! 同一 module 持有「临时可读 → 完整环境解析 → 会话安装/替换 → 就绪发布 → 重载失效」：
//!
//! - `prepare()` 登记一次解析租约并给出临时环境（打开路径不跑登录 shell）；
//! - `resolve_now()` / `settle()` 决定同步解析还是后台捕获；两者都在就绪前完成安装；
//! - `install_environment()` 只在持有会话写锁且没有写操作时替换会话；写操作进行中稍后重试，
//!   不会绕过写锁；
//! - `publish` 由 `EnvCoordinator` 按租约原子完成：被重新加载作废的代次不能写缓存、ready 或事件；
//! - `reload()` 先在每个会话的写锁下标记失效，再清槽位与会话，等待中的写操作醒来时拿到
//!   结构化「环境已更新」错误，而不是在旧对象上继续写。
//!
//! 命令层只调用这些入口，不再自行组合 begin / install / complete。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::commands::{AppState, ShellResolver};
use crate::environment::{EnvCoordinator, EnvLease, WAIT_TIMEOUT};
use crate::error::{AppError, AppErrorPayload, ErrorKind, Result};
use crate::session::{EnvGate, WorktreeSession};
use crate::shell::ShellEnv;

/// 后台环境就绪事件（Q222）：Git 路径变化时前端重读；捕获失败时带 Q205 的结构化原因。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentReady {
    pub worktree_id: String,
    pub git_changed: bool,
    pub error: Option<AppErrorPayload>,
}

/// 打开会话需要的临时环境与解析租约。
pub struct PreparedEnvironment {
    pub env: ShellEnv,
    /// 记忆里的 Git 程序（None 表示没有记忆，需要按 PATH 查找）。
    pub git_path: Option<PathBuf>,
    pub needs_capture: bool,
    path: PathBuf,
    lease: EnvLease,
}

impl PreparedEnvironment {
    /// 临时环境是否已经就是完整环境（本次运行解析过这个 worktree）。
    pub fn is_complete_cache(&self) -> bool {
        !self.needs_capture
    }
}

fn write_lock_failed() -> AppError {
    AppError::coded(
        ErrorKind::Internal,
        "errors.commands.writeOperationLockFailed",
        "写操作锁失败",
        &[],
    )
}

fn reload_rejected() -> AppError {
    AppError::coded(
        ErrorKind::Rejected,
        "errors.commands.aWriteOperationProgressReloadEnvironment",
        "有正在进行的写操作，完成后再重新加载环境",
        &[],
    )
}

fn capture_environment(
    resolver: &ShellResolver,
    path: &Path,
    manual_git: Option<&Path>,
) -> (ShellEnv, PathBuf, Option<AppErrorPayload>) {
    let resolved = resolver(path);
    let git_path = manual_git
        .map(Path::to_path_buf)
        .or_else(|| resolved.which("git"))
        .unwrap_or_else(|| PathBuf::from("git"));
    let failure = resolved
        .info
        .used_fallback
        .then(|| crate::session::shell_environment_check(&resolved).detail);
    (resolved, git_path, failure)
}

/// 把完整环境装进会话；Git 路径变了就换成新会话（Q222）。返回是否换了会话。
///
/// 只在持有该会话 `writing` 锁且没有写操作时动手，写操作进行中就稍后再试，
/// 不会绕过写锁（Q90）。会话已被关闭、重新加载或换了路径时放弃，迟到的结果不登记到别处。
///
/// 租约有效性在**会话表锁内**核对（M10 返修 SP4）：与 `reload()` 的「作废 + 清会话表」
/// 互斥，重载后迟到的安装不会把旧会话重新登记回来。
fn install_environment(
    env: &EnvCoordinator,
    lease: &EnvLease,
    sessions: &Mutex<HashMap<String, Arc<WorktreeSession>>>,
    worktree_id: &str,
    expected: &Arc<WorktreeSession>,
    resolved: &ShellEnv,
    git_path: &Path,
    open_next: impl Fn(ShellEnv) -> Result<Arc<WorktreeSession>>,
) -> bool {
    loop {
        let Ok(mut map) = sessions.lock() else {
            return false;
        };
        // 重新加载已作废这次解析：不得再改会话表（否则旧会话会在重载后重新出现）。
        if !env.is_lease_valid(lease) {
            return false;
        }
        // 只装进发起解析时的那个会话：它已被关闭、重新加载或替换时放弃。
        let Some(current) = map
            .get(worktree_id)
            .cloned()
            .filter(|current| Arc::ptr_eq(current, expected))
        else {
            return false;
        };
        let Ok(writing) = current.writing.lock() else {
            return false;
        };
        if !*writing {
            if current.git.program == git_path {
                current.apply_environment(resolved.clone());
                return false;
            }
            return match open_next(resolved.clone()) {
                Ok(next) => {
                    current.mark_superseded();
                    map.insert(worktree_id.to_string(), next);
                    true
                }
                Err(_) => {
                    // 新 Git 打不开：保留可用的旧 Git，只换环境。
                    current.apply_environment(resolved.clone());
                    false
                }
            };
        }
        drop(writing);
        drop(map);
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// Worktree 运行环境的生产入口。
pub struct WorktreeEnvironment<'a> {
    state: &'a AppState,
}

impl<'a> WorktreeEnvironment<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    /// 打开前登记：临时环境 + 一次解析租约。之后必须调用 `resolve_now()` 或 `settle()` 收尾。
    pub fn prepare(&self, worktree_id: &str, path: &Path) -> PreparedEnvironment {
        let provisional = self.state.env.provisional(path);
        PreparedEnvironment {
            env: provisional.env,
            git_path: provisional.git_path,
            needs_capture: provisional.needs_capture,
            path: path.to_path_buf(),
            lease: self.state.env.begin(worktree_id),
        }
    }

    /// 同步解析完整登录环境并用租约发布，返回环境与要用的 Git 程序。
    ///
    /// 发布失败说明这次解析的槽位已作废（重新加载过）：返回结构化失效错误，
    /// 调用方不得再登记会话，也不自动重放写入（M10 返修 SP4）。
    pub fn resolve_now(
        &self,
        prepared: &PreparedEnvironment,
        manual_git: Option<&Path>,
    ) -> Result<(ShellEnv, PathBuf)> {
        let (resolved, git_path, failure) =
            capture_environment(self.state.resolver(), &prepared.path, manual_git);
        if !self.state.env.publish(
            &prepared.lease,
            &prepared.path,
            resolved.clone(),
            git_path.clone(),
            failure,
        ) {
            return Err(crate::session::environment_reloaded());
        }
        Ok((resolved, git_path))
    }

    /// 打开（或复用）worktree 会话的**唯一入口**。
    ///
    /// 依次完成：临时环境与解析租约 →（必要时）同步解析 → 打开会话 → 绑定环境门 →
    /// 就绪发布 / 后台捕获 → 登记会话。调用方不再需要知道这些步骤的顺序。
    ///
    /// 耗时的 shell / Git 读取在生命周期锁外完成；登记时再次校验租约。
    /// 重载可以立即作废在途打开，也不会让一个慢 worktree 阻塞其它已打开会话。
    pub fn open(&self, worktree_id: &str) -> Result<Arc<WorktreeSession>> {
        let _lifecycle = self.state.lifecycle.lock().map_err(|_| crate::commands::session_lock_error())?;
        if let Some(session) = self
            .state
            .sessions
            .lock()
            .map_err(|_| crate::commands::session_lock_error())?
            .get(worktree_id)
        {
            return Ok(session.clone());
        }
        let path = self.worktree_path(worktree_id)?;
        let manual_git = self.manual_git()?;
        // Q212：打开路径上不跑登录 shell，先用记忆里的 Git 与环境（不持会话表锁）。
        let prepared = self.prepare(worktree_id, &path);
        drop(_lifecycle);
        let open = |env: ShellEnv, program: Option<&Path>| {
            WorktreeSession::open_with_options(
                worktree_id,
                &path,
                env,
                Arc::clone(&self.state.command_log),
                Some(Arc::clone(&self.state.review)),
                program,
            )
        };
        // 已经同步解析过完整环境（不再在后台重复跑登录 shell，S2）。
        let mut synced = false;
        let session = match manual_git.clone().or_else(|| prepared.git_path.clone()) {
            Some(program) => {
                let opened = open(prepared.env.clone(), Some(&program));
                let usable = opened.as_ref().is_ok_and(|session| session.git_version_ok());
                if usable || manual_git.is_some() {
                    opened?
                } else {
                    // S2：记住的 Git 不存在、无法执行或版本过低：丢弃记忆，退回同步等待完整环境。
                    self.state.env.forget_git(&path, &program);
                    let (resolved, found) = self.resolve_now(&prepared, None)?;
                    synced = true;
                    open(resolved, Some(&found))?
                }
            }
            None => {
                // 从未找到过 Git：照旧同步等待完整环境就绪（规格 D、Q222）。
                let (resolved, found) = self.resolve_now(&prepared, None)?;
                synced = true;
                open(resolved, Some(&found))?
            }
        };
        // 只在提交结果时串行：重载期间完成的旧读取不能重新登记会话。
        let _lifecycle = self.state.lifecycle.lock()
            .map_err(|_| crate::commands::session_lock_error())?;
        if !self.state.env.is_lease_valid(&prepared.lease) {
            return Err(crate::session::environment_reloaded());
        }
        if let Some(existing) = self.state.sessions.lock()
            .map_err(|_| crate::commands::session_lock_error())?.get(worktree_id) {
            return Ok(Arc::clone(existing));
        }
        session.set_env_gate(self.gate(worktree_id));
        // 本次运行的缓存已经是完整环境：在登记前同步发布就绪，发布失败即不登记。
        if prepared.is_complete_cache() {
            let git_path = prepared
                .git_path
                .clone()
                .unwrap_or_else(|| session.git.program.clone());
            if !self.state.env.publish(
                &prepared.lease,
                &prepared.path,
                prepared.env.clone(),
                git_path,
                None,
            ) {
                return Err(crate::session::environment_reloaded());
            }
        }
        // 双重检查：并发的第一个请求只发布一个会话。
        let mut sessions = self
            .state
            .sessions
            .lock()
            .map_err(|_| crate::commands::session_lock_error())?;
        if let Some(existing) = sessions.get(worktree_id) {
            return Ok(existing.clone());
        }
        sessions.insert(worktree_id.to_string(), Arc::clone(&session));
        drop(sessions);
        // 后台捕获：安装成功（或既定回退）先于 ready，重载作废的代次不发布。
        if !synced && prepared.needs_capture {
            self.spawn_capture(prepared, &session, manual_git);
        }
        Ok(session)
    }

    fn worktree_path(&self, worktree_id: &str) -> Result<PathBuf> {
        let registry = self.state.registry.lock().map_err(|_| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.repositoryListLockFailed",
                "仓库列表锁失败",
                &[],
            )
        })?;
        registry.worktree_path(worktree_id).ok_or_else(|| {
            AppError::coded(
                ErrorKind::Rejected,
                "errors.commands.theWorktreeRegisteredAddRepositoryFirst",
                "worktree 未登记，请先添加仓库",
                &[],
            )
        })
    }

    /// 设置里手动指定的 Git 程序（Q27）。
    fn manual_git(&self) -> Result<Option<PathBuf>> {
        let registry = self.state.registry.lock().map_err(|_| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.repositoryListLockFailed2",
                "仓库列表锁失败",
                &[],
            )
        })?;
        Ok(registry
            .settings()
            .git_path
            .clone()
            .map(PathBuf::from))
    }

    /// 写操作开始前等待完整环境（会话的 `acquire_write_lock` 调用）。
    pub fn gate(&self, worktree_id: &str) -> EnvGate {
        let env = Arc::clone(&self.state.env);
        let id = worktree_id.to_string();
        Arc::new(move || {
            if env.wait_ready(&id, WAIT_TIMEOUT) {
                Ok(())
            } else {
                Err(crate::environment::still_loading())
            }
        })
    }

    /// 等环境就绪后再执行网络操作与内置终端（Q212）。
    pub fn await_ready(&self, worktree_id: &str) -> Result<()> {
        if self.state.env.wait_ready(worktree_id, WAIT_TIMEOUT) {
            Ok(())
        } else {
            Err(crate::environment::still_loading())
        }
    }

    /// 环境未就绪时打开失败不立即报错：等它就绪后重试一次（Q212）。
    pub fn session_retrying(&self, worktree_id: &str) -> Result<Arc<WorktreeSession>> {
        match self.state.session(worktree_id) {
            Ok(session) => Ok(session),
            Err(first) => {
                if self.state.env.wait(worktree_id, WAIT_TIMEOUT).is_none() {
                    return Err(first);
                }
                self.state.session(worktree_id)
            }
        }
    }

    /// 重新加载环境：作废租约、清缓存与记忆、让旧会话失效并释放历史目标。
    ///
    /// 有任何会话正在写操作时拒绝，且**拒绝时不改变任何会话**（M10 返修 SP2）：
    /// 先拿齐所有会话的写锁（`try_lock`），全部拿到之后才统一标记失效；
    /// 这之后到清除会话表之间没有写入窗口。
    /// 标记失效必须在**持有该会话写锁**时完成：等待完整环境的写操作醒来后会重新检查
    /// `superseded`，因此不会在旧对象上继续写。
    pub fn reload(&self) -> Result<()> {
        let _lifecycle = self
            .state
            .lifecycle
            .lock()
            .map_err(|_| crate::commands::session_lock_error())?;
        let mut sessions = self
            .state
            .sessions
            .lock()
            .map_err(|_| crate::commands::session_lock_error())?;
        let mut guards = Vec::with_capacity(sessions.len());
        for session in sessions.values() {
            // 写操作进行中时 `writing` 锁是空闲的、只有标志为 true（`WriteGuard` 不持有锁），
            // 所以这里拿锁不会等，拿到后再看标志就是权威判断。
            let guard = session.writing.lock().map_err(|_| write_lock_failed())?;
            if *guard {
                return Err(reload_rejected());
            }
            guards.push(guard);
        }
        // 写锁全部在手里：此刻没有写操作，新的写操作也无法开始。
        for session in sessions.values() {
            session.mark_superseded();
        }
        drop(guards);
        // 会话表锁一直持有：后台安装的租约核对与这里互斥，重载后不会重新登记旧会话。
        self.state.env.reset();
        let instances: Vec<(String, u64)> = sessions
            .iter()
            .map(|(worktree_id, session)| (worktree_id.clone(), session.instance_id()))
            .collect();
        sessions.clear();
        drop(sessions);
        for (worktree_id, instance_id) in instances {
            self.state
                .history
                .invalidate_session(&worktree_id, instance_id);
        }
        // 环境重载后旧的历史目标（含其 Git 程序与 shell 环境）不再有效。
        self.state.history.clear();
        Ok(())
    }

    /// 后台解析完整登录环境（Q212）。
    ///
    /// 先把环境装进会话（Git 路径变了就换会话），再宣布就绪：
    /// 等在 `acquire_write_lock` 里的写操作醒来时，用的已经是完整环境。
    fn spawn_capture(
        &self,
        prepared: PreparedEnvironment,
        expected: &Arc<WorktreeSession>,
        manual_git: Option<PathBuf>,
    ) {
        let worktree_id = prepared.lease.worktree_id().to_string();
        let path = prepared.path.clone();
        let lease = prepared.lease;
        let expected = Arc::clone(expected);
        let resolver = Arc::clone(self.state.resolver());
        let env = Arc::clone(&self.state.env);
        let sessions = Arc::clone(&self.state.sessions);
        let command_log = Arc::clone(&self.state.command_log);
        let review = Arc::clone(&self.state.review);
        let gate = self.gate(&worktree_id);
        let app = self.state.app.get().cloned();
        std::thread::Builder::new()
            .name("revia-env".into())
            .spawn(move || {
                let (resolved, git_path, failure) =
                    capture_environment(&resolver, &path, manual_git.as_deref());
                let changed = install_environment(
                    &env,
                    &lease,
                    &sessions,
                    &worktree_id,
                    &expected,
                    &resolved,
                    &git_path,
                    |next_env| {
                        let next = WorktreeSession::open_with_options(
                            &worktree_id,
                            &path,
                            next_env,
                            Arc::clone(&command_log),
                            Some(Arc::clone(&review)),
                            Some(&git_path),
                        )?;
                        next.set_env_gate(Arc::clone(&gate));
                        Ok(next)
                    },
                );
                let payload = failure.clone();
                // 重新加载环境后，旧的结果不能宣布就绪，也不能发事件（槽位已作废）。
                if !env.publish(&lease, &path, resolved, git_path, failure) {
                    return;
                }
                if let Some(app) = app {
                    use tauri::Emitter;
                    let _ = app.emit(
                        "revia://environment-ready",
                        EnvironmentReady {
                            worktree_id,
                            git_changed: changed,
                            error: payload,
                        },
                    );
                }
            })
            .ok();
    }
}
