//! M9 任务 05（Q212、Q214、Q222）：打开 worktree 时不再让登录 shell 挡住文件列表。
//!
//! 两层记忆：
//! - 本次运行内按 worktree 缓存 `shell::resolve` 的结果（`EnvCoordinator::cache`）；
//! - 跨运行把「找到过的 Git 程序与 PATH」写进应用数据目录（`EnvMemory`）。
//!
//! 打开路径先用记忆里的 Git 建会话并读状态；完整环境在后台解析，就绪后如果 Git
//! 路径变了，由命令层换一个会话并重读一次。环境未就绪时，hooks、提交、网络操作与
//! 内置终端会等它就绪（`EnvCoordinator::wait`），界面用 `environment_status` 显示等待。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppErrorPayload, ErrorKind};
use crate::shell::{EnvCaptureInfo, ShellEnv};

/// 记住的 Git 与 PATH（Q214）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RememberedEnv {
    /// 找到过的 Git 程序绝对路径。
    pub git_path: String,
    /// 解析用的 shell（展示用）。
    pub shell: String,
    /// 解析出的 PATH 条目。
    pub path_entries: Vec<String>,
    /// 记录时间（毫秒时间戳）。
    pub captured_at_ms: u64,
}

/// 跨运行的环境记忆，保存在 Revia 应用数据目录。
pub struct EnvMemory {
    path: PathBuf,
    state: Mutex<HashMap<String, RememberedEnv>>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

fn key(worktree: &Path) -> String {
    worktree.display().to_string()
}

impl EnvMemory {
    /// 读取记忆文件；损坏时当作空记忆，不影响启动。
    pub fn load(path: PathBuf) -> Self {
        let state = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<HashMap<String, RememberedEnv>>(&bytes).ok())
            .unwrap_or_default();
        Self {
            path,
            state: Mutex::new(state),
        }
    }

    pub fn get(&self, worktree: &Path) -> Option<RememberedEnv> {
        self.state
            .lock()
            .ok()
            .and_then(|guard| guard.get(&key(worktree)).cloned())
    }

    /// 任意一条记忆（用于「先取其他 worktree 的结果」）。
    pub fn any(&self) -> Option<RememberedEnv> {
        self.state
            .lock()
            .ok()
            .and_then(|guard| guard.values().next().cloned())
    }

    /// 记住这次的 Git 与 PATH。写盘失败只影响下次启动，不打断当前操作。
    pub fn remember(&self, worktree: &Path, env: &ShellEnv, git_path: &Path) {
        let entry = RememberedEnv {
            git_path: git_path.display().to_string(),
            shell: env.info.shell.clone(),
            path_entries: env.path.clone(),
            captured_at_ms: now_ms(),
        };
        let snapshot = {
            let Ok(mut guard) = self.state.lock() else {
                return;
            };
            guard.insert(key(worktree), entry);
            guard.clone()
        };
        if let Ok(bytes) = serde_json::to_vec_pretty(&snapshot) {
            let _ = crate::m7::atomic_write(&self.path, &bytes);
        }
    }

    /// 丢弃失效的 Git 记忆（S2）：这个 worktree 的记忆，以及所有指向同一个 Git 的记忆。
    ///
    /// 打开时用到的 Git 可能来自其他 worktree 的记忆（`any`），只删当前 worktree 的键
    /// 会留下那条失效记忆，下次启动又被取到。
    pub fn forget_git(&self, worktree: &Path, git_path: &Path) {
        let stale = git_path.display().to_string();
        let snapshot = {
            let Ok(mut guard) = self.state.lock() else {
                return;
            };
            guard.remove(&key(worktree));
            guard.retain(|_, remembered| remembered.git_path != stale);
            guard.clone()
        };
        if let Ok(bytes) = serde_json::to_vec_pretty(&snapshot) {
            let _ = crate::m7::atomic_write(&self.path, &bytes);
        }
    }

    /// 清空所有记忆。
    pub fn clear(&self) {
        let snapshot = {
            let Ok(mut guard) = self.state.lock() else {
                return;
            };
            guard.clear();
            guard.clone()
        };
        if let Ok(bytes) = serde_json::to_vec_pretty(&snapshot) {
            let _ = crate::m7::atomic_write(&self.path, &bytes);
        }
    }
}

/// 环境进度（供 `environment_status` 与等待使用）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EnvStatus {
    /// 完整环境是否已经就绪。
    pub ready: bool,
    /// 当前会话是否仍在使用记忆里的 Git / PATH（就绪前为 true）。
    pub provisional: bool,
    /// 就绪时间（毫秒时间戳）；未就绪为 None。
    pub ready_at_ms: Option<u64>,
    /// 后台解析失败时的结构化原因（沿用 Q205 的回退提示）。
    pub error: Option<AppErrorPayload>,
}

struct SlotState {
    ready: bool,
    env: Option<ShellEnv>,
    git_path: Option<PathBuf>,
    error: Option<AppErrorPayload>,
    ready_at_ms: Option<u64>,
}

struct EnvSlot {
    /// 每次登记都换一个代号：重新加载环境后，旧的后台结果不会写进新槽位。
    generation: u64,
    state: Mutex<SlotState>,
    ready: Condvar,
}

static SLOT_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl EnvSlot {
    fn new() -> Self {
        Self {
            generation: SLOT_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            state: Mutex::new(SlotState {
                ready: false,
                env: None,
                git_path: None,
                error: None,
                ready_at_ms: None,
            }),
            ready: Condvar::new(),
        }
    }
}

/// 打开 worktree 时能立刻拿到的环境。
pub struct ProvisionalEnv {
    pub env: ShellEnv,
    /// 记忆里的 Git 程序（None 表示没有记忆，需要按 PATH 查找）。
    pub git_path: Option<PathBuf>,
    /// 是否还需要后台解析完整环境。
    pub needs_capture: bool,
}

/// 环境协调器：记忆、后台解析与等待。
pub struct EnvCoordinator {
    /// 本次运行内按 worktree 缓存的结果。
    cache: crate::shell::EnvCache,
    memory: EnvMemory,
    slots: Mutex<HashMap<String, Arc<EnvSlot>>>,
}

impl EnvCoordinator {
    pub fn new(memory_path: PathBuf) -> Self {
        Self {
            cache: crate::shell::EnvCache::new(),
            memory: EnvMemory::load(memory_path),
            slots: Mutex::new(HashMap::new()),
        }
    }

    /// 打开路径上能立刻使用的环境（Q214 的顺序）。
    ///
    /// 顺序：本次运行这个 worktree 的结果 → 本次运行其他 worktree 的结果 →
    /// 上次运行这个 worktree 的记忆 → 上次运行其他 worktree 的记忆 →
    /// 都没有时同步解析（与旧行为一致：从未找到过 Git 时照旧等待）。
    pub fn provisional(&self, worktree: &Path) -> ProvisionalEnv {
        if let Some(env) = self.cached(worktree) {
            let git_path = env.which("git");
            return ProvisionalEnv {
                env,
                git_path,
                needs_capture: false,
            };
        }
        if let Some(env) = self.any_cached() {
            return ProvisionalEnv {
                git_path: env.which("git"),
                env,
                needs_capture: true,
            };
        }
        if let Some(remembered) = self.memory.get(worktree) {
            return ProvisionalEnv {
                git_path: Some(PathBuf::from(&remembered.git_path)),
                env: env_from_remembered(&remembered, worktree),
                needs_capture: true,
            };
        }
        if let Some(remembered) = self.memory.any() {
            return ProvisionalEnv {
                git_path: Some(PathBuf::from(&remembered.git_path)),
                env: env_from_remembered(&remembered, worktree),
                needs_capture: true,
            };
        }
        // 从未找到过 Git：由调用方同步解析完整环境（只在首次运行或重新加载后发生）。
        ProvisionalEnv {
            env: ShellEnv::minimal(worktree),
            git_path: None,
            needs_capture: true,
        }
    }

    fn cached(&self, worktree: &Path) -> Option<ShellEnv> {
        self.cache.peek(worktree)
    }

    fn any_cached(&self) -> Option<ShellEnv> {
        self.cache.peek_any()
    }

    fn store_cache(&self, worktree: &Path, env: &ShellEnv) {
        self.cache.store(worktree, env.clone());
    }

    /// 登记一次后台解析（同一个 worktree 复用同一个槽位），返回槽位代号。
    pub fn begin(&self, worktree_id: &str) -> u64 {
        let mut slots = self.slots.lock().unwrap_or_else(|error| error.into_inner());
        slots
            .entry(worktree_id.to_string())
            .or_insert_with(|| Arc::new(EnvSlot::new()))
            .generation
    }

    /// 后台解析完成，但只在槽位仍是登记时那一个时生效；返回是否生效。
    pub fn complete_generation(
        &self,
        worktree_id: &str,
        generation: u64,
        worktree: &Path,
        env: ShellEnv,
        git_path: PathBuf,
        error: Option<AppErrorPayload>,
    ) -> bool {
        let current = self
            .slots
            .lock()
            .ok()
            .and_then(|slots| slots.get(worktree_id).map(|slot| slot.generation));
        if current != Some(generation) {
            return false;
        }
        self.complete(worktree_id, worktree, env, git_path, error);
        true
    }

    /// 后台解析完成：写入记忆与缓存，并唤醒等待者。
    pub fn complete(
        &self,
        worktree_id: &str,
        worktree: &Path,
        env: ShellEnv,
        git_path: PathBuf,
        error: Option<AppErrorPayload>,
    ) {
        self.store_cache(worktree, &env);
        self.memory.remember(worktree, &env, &git_path);
        let slot = {
            let mut slots = self.slots.lock().unwrap_or_else(|error| error.into_inner());
            slots
                .entry(worktree_id.to_string())
                .or_insert_with(|| Arc::new(EnvSlot::new()))
                .clone()
        };
        let mut state = slot.state.lock().unwrap_or_else(|error| error.into_inner());
        state.ready = true;
        state.env = Some(env);
        state.git_path = Some(git_path);
        state.error = error;
        state.ready_at_ms = Some(now_ms());
        drop(state);
        slot.ready.notify_all();
    }

    /// 等环境就绪；没有登记过或超时返回 None。
    pub fn wait(&self, worktree_id: &str, timeout: Duration) -> Option<(ShellEnv, PathBuf)> {
        let slot = self
            .slots
            .lock()
            .ok()
            .and_then(|slots| slots.get(worktree_id).cloned())?;
        let deadline = Instant::now() + timeout;
        let mut state = slot.state.lock().unwrap_or_else(|error| error.into_inner());
        while !state.ready {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }
            let (next, _) = slot
                .ready
                .wait_timeout(state, remaining)
                .unwrap_or_else(|error| error.into_inner());
            state = next;
        }
        Some((
            state.env.clone()?,
            state.git_path.clone().unwrap_or_default(),
        ))
    }

    /// 是否已经就绪（没有登记过视为未就绪）。
    pub fn is_ready(&self, worktree_id: &str) -> bool {
        self.slots
            .lock()
            .ok()
            .and_then(|slots| slots.get(worktree_id).cloned())
            .is_some_and(|slot| {
                slot.state
                    .lock()
                    .map(|state| state.ready)
                    .unwrap_or(false)
            })
    }

    /// 等这个 worktree 的完整环境就绪；没有登记过（不由协调器管理）视为就绪。超时返回 false。
    pub fn wait_ready(&self, worktree_id: &str, timeout: Duration) -> bool {
        let tracked = self
            .slots
            .lock()
            .ok()
            .is_some_and(|slots| slots.contains_key(worktree_id));
        !tracked || self.is_ready(worktree_id) || self.wait(worktree_id, timeout).is_some()
    }

    pub fn status(&self, worktree_id: &str) -> EnvStatus {
        let slot = self
            .slots
            .lock()
            .ok()
            .and_then(|slots| slots.get(worktree_id).cloned());
        let Some(slot) = slot else {
            return EnvStatus {
                ready: false,
                provisional: true,
                ready_at_ms: None,
                error: None,
            };
        };
        let state = slot.state.lock().unwrap_or_else(|error| error.into_inner());
        EnvStatus {
            ready: state.ready,
            provisional: !state.ready,
            ready_at_ms: state.ready_at_ms,
            error: state.error.clone(),
        }
    }

    /// 记住的 Git 不可用时丢弃相关记忆（S2）。
    pub fn forget_git(&self, worktree: &Path, git_path: &Path) {
        self.memory.forget_git(worktree, git_path);
    }

    pub fn clear_memory(&self) {
        self.memory.clear();
    }

    /// 重新加载环境（设置里的「重新加载环境」）：清空本次运行的缓存、槽位与跨运行记忆（S2）。
    pub fn reset(&self) {
        self.cache.clear();
        self.clear_memory();
        if let Ok(mut slots) = self.slots.lock() {
            slots.clear();
        }
    }
}

/// 用记住的 PATH 合成一个可用的环境；`HOME` 仍然从进程取。
fn env_from_remembered(remembered: &RememberedEnv, worktree: &Path) -> ShellEnv {
    let mut vars = HashMap::new();
    for key in ["HOME", "USERPROFILE", "HOMEDRIVE", "HOMEPATH"] {
        if let Ok(value) = std::env::var(key) {
            vars.insert(key.to_string(), value);
        }
    }
    let mut env = ShellEnv {
        vars,
        path: remembered.path_entries.clone(),
        info: EnvCaptureInfo {
            worktree: worktree.display().to_string(),
            shell: remembered.shell.clone(),
            elapsed_ms: 0,
            timed_out: false,
            used_fallback: false,
            var_count: remembered.path_entries.len(),
            path_entries: remembered.path_entries.clone(),
            fallback_reason: None,
            fallback_detail: None,
        },
    };
    env.vars.insert("PATH".to_string(), env.path_string());
    env
}

pub const WAIT_TIMEOUT: Duration = Duration::from_secs(30);

/// 环境仍未就绪时给前端的提示（Q205 的措辞沿用「回退到最小环境」）。
pub fn still_loading() -> AppError {
    AppError::coded(
        ErrorKind::Environment,
        "errors.environment.stillLoading",
        "登录环境仍在加载，请稍候",
        &[],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_memory() -> (tempfile::TempDir, EnvCoordinator) {
        let dir = tempfile::tempdir().unwrap();
        let coordinator = EnvCoordinator::new(dir.path().join("env-memory.json"));
        (dir, coordinator)
    }

    #[test]
    fn remembered_env_survives_restart_and_is_used_first() {
        let dir = tempfile::tempdir().unwrap();
        let worktree = dir.path().join("repo");
        std::fs::create_dir_all(&worktree).unwrap();
        let path = dir.path().join("env-memory.json");
        let env = ShellEnv {
            vars: HashMap::new(),
            path: vec!["/opt/homebrew/bin".into(), "/usr/bin".into()],
            info: EnvCaptureInfo {
                worktree: worktree.display().to_string(),
                shell: "/bin/zsh".into(),
                elapsed_ms: 3,
                timed_out: false,
                used_fallback: false,
                var_count: 0,
                path_entries: vec![],
                fallback_reason: None,
                fallback_detail: None,
            },
        };
        EnvCoordinator::new(path.clone()).complete(
            "wt-1",
            &worktree,
            env,
            PathBuf::from("/opt/homebrew/bin/git"),
            None,
        );
        // 模拟重启：新的协调器从磁盘读回记忆。
        let restarted = EnvCoordinator::new(path);
        let provisional = restarted.provisional(&worktree);
        assert!(provisional.needs_capture);
        assert_eq!(
            provisional.git_path,
            Some(PathBuf::from("/opt/homebrew/bin/git"))
        );
        assert_eq!(provisional.env.path.len(), 2);
    }

    /// S2：失效的 Git 来自其他 worktree 的记忆时，那条记忆也要丢弃，其他记忆保留。
    #[test]
    fn forgetting_a_stale_git_drops_every_entry_that_points_to_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("env-memory.json");
        let other = dir.path().join("other");
        let healthy = dir.path().join("healthy");
        let opening = dir.path().join("opening");
        let stale_git = dir.path().join("deleted-git");
        let good_git = dir.path().join("good-git");
        let memory = EnvMemory::load(path.clone());
        memory.remember(&other, &ShellEnv::minimal(&other), &stale_git);
        memory.remember(&healthy, &ShellEnv::minimal(&healthy), &good_git);

        memory.forget_git(&opening, &stale_git);

        let reloaded = EnvMemory::load(path);
        assert!(reloaded.get(&other).is_none());
        assert_eq!(
            reloaded.get(&healthy).map(|entry| entry.git_path),
            Some(good_git.display().to_string())
        );
        assert_eq!(
            reloaded.any().map(|entry| entry.git_path),
            Some(good_git.display().to_string())
        );
    }

    #[test]
    fn wait_returns_none_before_ready_and_value_after_complete() {
        let (dir, coordinator) = temp_memory();
        let worktree = dir.path().join("repo");
        std::fs::create_dir_all(&worktree).unwrap();
        coordinator.begin("wt-1");
        assert!(!coordinator.is_ready("wt-1"));
        assert!(coordinator
            .wait("wt-1", Duration::from_millis(20))
            .is_none());
        let env = ShellEnv::minimal(&worktree);
        coordinator.complete(
            "wt-1",
            &worktree,
            env,
            PathBuf::from("/usr/bin/git"),
            None,
        );
        let (ready_env, git_path) = coordinator
            .wait("wt-1", Duration::from_millis(20))
            .expect("环境应已就绪");
        assert_eq!(git_path, PathBuf::from("/usr/bin/git"));
        assert_eq!(ready_env.info.shell, ShellEnv::minimal(&worktree).info.shell);
        assert!(coordinator.status("wt-1").ready);
    }

    #[test]
    fn late_completion_for_one_worktree_does_not_mark_another_ready() {
        let (dir, coordinator) = temp_memory();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        coordinator.begin("a");
        coordinator.begin("b");
        coordinator.complete("a", &a, ShellEnv::minimal(&a), PathBuf::from("git"), None);
        assert!(coordinator.is_ready("a"));
        assert!(!coordinator.is_ready("b"));
        // b 的迟到结果只登记到 b。
        coordinator.complete("b", &b, ShellEnv::minimal(&b), PathBuf::from("git"), None);
        assert!(coordinator.is_ready("b"));
    }

    #[test]
    fn switched_git_path_is_visible_after_completion() {
        let (dir, coordinator) = temp_memory();
        let worktree = dir.path().join("repo");
        std::fs::create_dir_all(&worktree).unwrap();
        // 先写一条记忆，模拟上次运行记住的 Git。
        coordinator.complete(
            "wt-1",
            &worktree,
            ShellEnv::minimal(&worktree),
            PathBuf::from("/opt/homebrew/bin/git"),
            None,
        );
        let coordinator = EnvCoordinator::new(dir.path().join("env-memory.json"));
        coordinator.begin("wt-1");
        let provisional = coordinator.provisional(&worktree);
        assert!(provisional.needs_capture);
        assert_eq!(
            provisional.git_path,
            Some(PathBuf::from("/opt/homebrew/bin/git"))
        );
        // 后台解析找到新的 Git（记住的那个被删除或换了版本）。
        coordinator.complete(
            "wt-1",
            &worktree,
            ShellEnv::minimal(&worktree),
            PathBuf::from("/usr/local/bin/git"),
            None,
        );
        let (_, git_path) = coordinator.wait("wt-1", Duration::from_millis(50)).unwrap();
        assert_eq!(git_path, PathBuf::from("/usr/local/bin/git"));
        // 记忆已更新：下次启动直接用新路径。
        let reloaded = EnvCoordinator::new(dir.path().join("env-memory.json"));
        assert_eq!(
            reloaded.provisional(&worktree).git_path,
            Some(PathBuf::from("/usr/local/bin/git"))
        );
    }

    #[test]
    fn reset_clears_cached_environments() {
        let (dir, coordinator) = temp_memory();
        let worktree = dir.path().join("repo");
        std::fs::create_dir_all(&worktree).unwrap();
        coordinator.begin("wt-1");
        coordinator.complete(
            "wt-1",
            &worktree,
            ShellEnv::minimal(&worktree),
            PathBuf::from("git"),
            None,
        );
        coordinator.reset();
        assert!(!coordinator.is_ready("wt-1"));
    }
}
