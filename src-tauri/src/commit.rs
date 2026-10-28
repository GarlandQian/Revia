//! 固定类型的提交上下文与提交命令（M3 F03/F04，ADR-0005、Q21–Q25、Q90–Q96）。
//!
//! 约束：
//! - 前端只传登记 worktree ID、信息、amend/skip 选项与所见提交预期，
//!   不传任意 Git 参数或命令（规格 F03.1）；
//! - 提交预期覆盖**整个实际提交内容**：HEAD（含 unborn）、暂存区条目/模式/对象、
//!   暂存删除、进行中操作及其真实元数据内容；执行前重新读取比较（规格 F03.2）；
//! - 通过当前 worktree 的登录 shell 环境执行真正的 `git commit`，默认正常跑 hooks；
//!   「跳过 hooks」用命令局部机制（`-c core.hooksPath=<空目录>` + `--no-verify`）
//!   对本次调用**全部** hooks 生效，进程结束即失效，不修改用户 Git 配置（F04.2）；
//! - stdout/stderr 以固定大小分片实时读取并交付前端（不等换行、不因非 UTF-8 停读），
//!   尾部缓冲按**字节**有界；不杀子进程；不自动重跑（F04.1/F04.3/F04.4）。

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::error::{AppError, Result};
use crate::git::status::{ChangesKind, FileStatus, StatusEntry};
use crate::git::{CommandRecord, Git};
use crate::operation;

/// 最近提交信息的读取上限（规格 F02.4：最近 20 条）。
pub const RECENT_COMMIT_LIMIT: usize = 20;

/// 提交输出在 Rust 侧聚合的尾部**字节**上限（实时事件已逐片交付前端）。
const OUTPUT_TAIL_BYTES: usize = 256 * 1024;
/// 诊断用 stderr 尾部字节上限。
const STDERR_TAIL_BYTES: usize = 32 * 1024;
/// 读取管道的一次分片大小。
const READ_CHUNK_BYTES: usize = 8 * 1024;
// Git 退出后只排空有界输出，不等待后台 hook 子进程关闭继承的描述符。
const OUTPUT_DRAIN_TIMEOUT: Duration = Duration::from_millis(100);

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

// ---------------------------------------------------------------------------
// 提交预期（ADR-0005：覆盖整个实际提交内容的指纹契约）
// ---------------------------------------------------------------------------

/// 暂存区中一个路径的真实状态。
///
/// 三种条目必须区分清楚（规格 F03.2/F03.4）：
/// - 真实 index 条目：`mode`/`oid` 来自 `git ls-files --stage` 的 stage 0；
/// - 暂存删除：index 里**已经没有**这个路径，`mode` 固定 `000000`、`oid` 为 None，
///   只带被删除的 HEAD 对象作指纹线索，不伪造真实 blob；
/// - intent-to-add（`git add -N`）：index 里只有占位条目，不会被写进提交，
///   因此不算「有暂存内容」。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CommitExpectationIndexEntry {
    pub path: String,
    #[serde(default)]
    pub original_path: Option<String>,
    /// 六位八进制模式，例如 `100644`、`100755`、`120000`、`160000`；
    /// 暂存删除为 `000000`（表示 index 中缺席，不是真实条目）。
    pub mode: String,
    /// 暂存区对象 ID；unmerged（stage ≠ 0）与暂存删除为 None。
    pub oid: Option<String>,
    /// porcelain v2 的状态字母（A/M/D/R/T/U…）；本次提交不改变该条目时为 `F`。
    pub status: String,
    /// 未解决冲突（unmerged）条目。
    pub unmerged: bool,
    /// 暂存删除（index 中已无该路径）。
    #[serde(default)]
    pub deleted: bool,
    /// intent-to-add 占位条目：不会被写进提交，也不算已暂存内容。
    #[serde(default)]
    pub intent_to_add: bool,
    /// 暂存删除时被删除的 HEAD 对象（仅参与指纹，不代表 index 内容）。
    #[serde(default)]
    pub head_oid: Option<String>,
}

impl CommitExpectationIndexEntry {
    /// 这个条目是否会让本次提交真正写出内容变化。
    fn is_committable_change(&self) -> bool {
        !self.unmerged && !self.intent_to_add && self.status != "F"
    }
}

/// 提交预期：执行提交前必须与真实状态逐项一致的一组读数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CommitExpectation {
    /// 当前 HEAD 提交对象；unborn（无任何提交）为 None。
    pub head: Option<String>,
    /// HEAD 所在分支；detached 为 None。
    pub branch: Option<String>,
    /// 暂存区条目（按路径排序）：路径、模式、对象、状态、删除/占位标记。
    pub index: Vec<CommitExpectationIndexEntry>,
    /// 进行中操作（None = 无），含真实元数据版本。
    pub operation: Option<operation::OperationState>,
    /// 整组预期的指纹：前端原样带回，Rust 执行前重新计算并比较。
    pub fingerprint: String,
}

impl CommitExpectation {
    /// 本次提交会改变（新增/修改/删除/重命名）的路径。
    pub fn changed_paths(&self) -> BTreeSet<String> {
        self.index
            .iter()
            .filter(|entry| entry.is_committable_change() || entry.unmerged)
            .map(|entry| entry.path.clone())
            .collect()
    }

    /// 是否有已暂存内容（本次提交会真正写进提交的条目）。
    ///
    /// 暂存删除算内容；intent-to-add 不算（Git 也不会把它写进提交）。
    pub fn has_staged_changes(&self) -> bool {
        self.index.iter().any(CommitExpectationIndexEntry::is_committable_change)
    }

    /// index 中未解决冲突的去重路径数。
    ///
    /// 不能只看进行中操作的冲突数：`git stash pop` 之类会留下 unmerged index
    /// 却没有 MERGE_HEAD 等标记（规格 F01.2/F03.2）。
    pub fn unmerged_paths(&self) -> BTreeSet<String> {
        self.index
            .iter()
            .filter(|entry| entry.unmerged)
            .map(|entry| entry.path.clone())
            .collect()
    }

    pub fn has_unmerged(&self) -> bool {
        self.index.iter().any(|entry| entry.unmerged)
    }

    /// 是否允许「没有暂存差异」的普通提交（无冲突 merge 收尾，F03.4）。
    pub fn allows_empty_diff_commit(&self) -> bool {
        self.operation
            .as_ref()
            .is_some_and(operation::OperationState::allows_empty_diff_commit)
    }
}

/// 当前 HEAD 对象名；unborn 时返回 None，读取失败显式报错（规格 F02.4/F03.2）。
///
/// `symbolic-ref` 成功**只**说明 HEAD 指向一个分支名，不等于 unborn：引用可能损坏
/// 或指向缺失对象。因此这里再用 `show-ref --verify` 区分：
/// - 引用确实不存在 → 合法的 unborn；
/// - 引用存在但无法解析成提交 → 仓库损坏，fail-closed。
pub fn read_head(git: &Git) -> Result<Option<String>> {
    let output = git.run(&["rev-parse", "--verify", "--quiet", "HEAD^{commit}"])?;
    match output.exit_code {
        0 => {
            let value = output.stdout_lossy().trim().to_string();
            if value.is_empty() {
                return Err(AppError::git("读取 HEAD 失败：Git 返回为空"));
            }
            Ok(Some(value))
        }
        1 => {
            let symbolic = git.run(&["symbolic-ref", "-q", "HEAD"])?;
            if symbolic.exit_code != 0 {
                return Err(AppError::git(format!(
                    "读取 HEAD 失败（无法解析为提交，且不是分支引用）：{}",
                    output.stderr.trim()
                )));
            }
            let reference = symbolic.stdout_lossy().trim().to_string();
            if reference.is_empty() {
                return Err(AppError::git("读取 HEAD 失败：分支引用为空"));
            }
            let exists = git.run(&["show-ref", "--verify", "--quiet", &reference])?;
            match exists.exit_code {
                // 引用存在却解析不出提交：损坏或对象缺失，不能当作未初始化分支。
                0 => Err(AppError::git(format!(
                    "读取 HEAD 失败：分支 {reference} 存在但无法解析成提交，请在终端检查仓库"
                ))),
                1 => Ok(None),
                other => Err(AppError::git(format!(
                    "读取 HEAD 失败（检查分支引用退出码 {other}）：{}",
                    exists.stderr.trim()
                ))),
            }
        }
        other => Err(AppError::git(format!(
            "读取 HEAD 失败（退出码 {other}）：{}",
            output.stderr.trim()
        ))),
    }
}

fn read_branch(git: &Git) -> Result<Option<String>> {
    let output = git.run(&["symbolic-ref", "--short", "-q", "HEAD"])?;
    match output.exit_code {
        0 => {
            let name = output.stdout_lossy().trim().to_string();
            if name.is_empty() { Err(AppError::git("Git 返回的分支引用为空")) }
            else { Ok(Some(name)) }
        }
        1 => Ok(None),
        _ => Err(AppError::git(format!("读取分支引用失败：{}", output.stderr.trim()))),
    }
}

/// 读取暂存区的**完整**条目表。
///
/// 以 `git ls-files --stage -z` 的 stage 0 条目为真实 index 内容（未变化的条目
/// 也在里面，提交会把它们一并提交），再用 `git status --porcelain=v2` 补充：
/// - 状态字母与原路径（重命名源）；
/// - **暂存删除**：index 里已经没有这个路径，ls-files 不会出现它，必须单独补上，
///   否则「只暂存了删除」会被误判成没有暂存内容；
/// - **intent-to-add**：index 里有占位条目但 status 只在未暂存侧显示为新增，
///   它不会被写进提交，不能算已暂存内容。
pub fn read_index_entries(git: &Git) -> Result<Vec<CommitExpectationIndexEntry>> {
    let output = git.run_ok(
        &["-c", "core.quotePath=false", "ls-files", "--stage", "-z"],
        "读取暂存区条目",
    )?;
    let status = crate::git::status::read(git)?;
    if status.entries.iter().any(|entry| {
        (entry.changes == ChangesKind::Staged || entry.unmerged)
            && entry.readonly_reason == Some(crate::git::status::ReadonlyReason::NonUtf8Path)
    }) {
        return Err(AppError::rejected("提交包含无法无损处理的非 UTF-8 路径，请在终端处理"));
    }
    let mut staged: BTreeMap<String, StatusEntry> = BTreeMap::new();
    let mut unstaged_added: BTreeSet<String> = BTreeSet::new();
    for entry in &status.entries {
        match entry.changes {
            ChangesKind::Staged => {
                staged.insert(entry.path.clone(), entry.clone());
            }
            ChangesKind::Unstaged => {
                if entry.status == FileStatus::Added {
                    unstaged_added.insert(entry.path.clone());
                }
            }
            ChangesKind::Untracked => {}
        }
    }

    let mut entries: BTreeMap<String, CommitExpectationIndexEntry> = BTreeMap::new();
    for record in output.stdout.split(|byte| *byte == 0) {
        if record.is_empty() {
            continue;
        }
        let text = std::str::from_utf8(record)
            .map_err(|_| AppError::rejected("暂存区包含无法无损处理的非 UTF-8 路径，请在终端处理"))?;
        // <mode> <oid> <stage>\t<path>
        let Some((head, path)) = text.split_once('\t') else {
            return Err(AppError::internal("暂存区条目格式无法解析"));
        };
        let fields: Vec<&str> = head.split(' ').collect();
        if fields.len() != 3 {
            return Err(AppError::internal("暂存区条目格式无法解析"));
        }
        let (mode, oid, stage) = (fields[0], fields[1], fields[2]);
        if stage != "0" {
            // unmerged 路径会有 stage 1/2/3 多行：按路径去重后标记冲突。
            let slot = entries.entry(path.to_string()).or_insert_with(|| {
                CommitExpectationIndexEntry {
                    path: path.to_string(),
                    original_path: None,
                    mode: mode.to_string(),
                    oid: None,
                    status: "U".into(),
                    unmerged: true,
                    deleted: false,
                    intent_to_add: false,
                    head_oid: None,
                }
            });
            slot.unmerged = true;
            slot.oid = None;
            continue;
        }
        let staged_entry = staged.get(path);
        let letter = staged_entry
            .map(|entry| entry.status.letter().to_string())
            // 不在变化列表里的条目（本次提交不改变它）用 F 表示。
            .unwrap_or_else(|| "F".into());
        let original_path = staged_entry.and_then(|entry| entry.original_path.clone());
        // intent-to-add：index 有占位条目、status 没有已暂存条目、未暂存侧显示为新增。
        let intent_to_add = staged_entry.is_none() && unstaged_added.contains(path);
        entries.insert(
            path.to_string(),
            CommitExpectationIndexEntry {
                path: path.to_string(),
                original_path,
                mode: mode.to_string(),
                oid: Some(oid.to_string()),
                status: letter,
                unmerged: false,
                deleted: false,
                intent_to_add,
                head_oid: None,
            },
        );
    }

    // 暂存删除：index 中已无该路径，ls-files 不会返回它。
    for (path, entry) in &staged {
        if entry.status != FileStatus::Deleted {
            continue;
        }
        // 同一路径又被重新加入时，以真实 index 条目为准。
        if entries.contains_key(path) {
            continue;
        }
        entries.insert(
            path.clone(),
            CommitExpectationIndexEntry {
                path: path.clone(),
                original_path: entry.original_path.clone(),
                // 删除没有 index 条目：用 000000 明确表示「缺席」，
                // 不能借用 HEAD 的模式/对象冒充真实 blob。
                mode: "000000".into(),
                oid: None,
                status: "D".into(),
                unmerged: false,
                deleted: true,
                intent_to_add: false,
                head_oid: entry.oid_head.clone(),
            },
        );
    }

    let mut entries: Vec<CommitExpectationIndexEntry> = entries.into_values().collect();
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

/// 暂存区中一个路径的（模式, 对象）映射，只含真实可提交条目。
///
/// 提交成功前后用它对照 hooks 改动：`git commit` 直接由暂存区写出 tree，
/// 因此「提交前暂存区」与「新提交的 tree」逐项相等是正常的；任何差异都是
/// hooks 在提交过程中改了暂存内容（F04.3）。intent-to-add 与暂存删除都不是
/// 真实 blob，不参与对照。
pub fn index_blob_map(git: &Git) -> Result<BTreeMap<String, (String, String)>> {
    let mut map = BTreeMap::new();
    for entry in read_index_entries(git)? {
        if entry.unmerged || entry.deleted || entry.intent_to_add {
            continue;
        }
        let Some(oid) = entry.oid else { continue };
        if oid.chars().all(|ch| ch == '0') {
            continue;
        }
        map.insert(entry.path, (entry.mode, oid));
    }
    Ok(map)
}

/// 某个提交的完整 tree（路径 → (模式, 对象)）。
pub fn tree_blob_map(git: &Git, revision: &str) -> Result<BTreeMap<String, (String, String)>> {
    let output = git.run_ok(
        &[
            "-c",
            "core.quotePath=false",
            "ls-tree",
            "-r",
            "-z",
            "--full-tree",
            revision,
        ],
        "读取提交的 tree",
    )?;
    let mut map = BTreeMap::new();
    for record in output.stdout.split(|byte| *byte == 0) {
        if record.is_empty() {
            continue;
        }
        let text = std::str::from_utf8(record)
            .map_err(|_| AppError::rejected("暂存区包含无法无损处理的非 UTF-8 路径，请在终端处理"))?;
        let Some((head, path)) = text.split_once('\t') else {
            return Err(AppError::internal("tree 条目格式无法解析"));
        };
        // <mode> SP <type> SP <object>
        let fields: Vec<&str> = head.split(' ').collect();
        if fields.len() != 3 {
            return Err(AppError::internal("tree 条目格式无法解析"));
        }
        map.insert(path.to_string(), (fields[0].to_string(), fields[2].to_string()));
    }
    Ok(map)
}

/// 重新读取真实提交上下文并计算预期（比较由调用方重新调用本函数完成）。
pub fn capture(git: &Git) -> Result<CommitExpectation> {
    let head = read_head(git)?;
    let branch = read_branch(git)?;
    let index = read_index_entries(git)?;
    // 冲突数直接来自 index 的 unmerged 条目：即使没有任何进行中操作标记，
    // 存在未解决冲突也必须门禁提交（F01.2）。
    let conflicts = index
        .iter()
        .filter(|entry| entry.unmerged)
        .map(|entry| entry.path.clone())
        .collect::<BTreeSet<_>>()
        .len();
    let operation = operation::detect(git, conflicts)?;
    let mut expected = CommitExpectation {
        head,
        branch,
        index,
        operation,
        fingerprint: String::new(),
    };
    expected.fingerprint = fingerprint_of(&expected);
    Ok(expected)
}

/// 提交预期指纹：覆盖 HEAD、分支、每个 index 条目（含暂存删除/intent-to-add）、
/// 进行中操作及其**真实元数据版本**（规格 F03.2）。
pub fn fingerprint_of(expected: &CommitExpectation) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"commit-expectation-v2");
    hasher.update(b"head\0");
    hasher.update(expected.head.as_deref().unwrap_or("<unborn>").as_bytes());
    hasher.update(b"\0branch\0");
    hasher.update(expected.branch.as_deref().unwrap_or("<detached>").as_bytes());
    hasher.update(b"\0op\0");
    match &expected.operation {
        None => {
            hasher.update(b"none");
        }
        Some(state) => {
            hasher.update(state.kind.label().as_bytes());
            hasher.update(b"\0");
            hasher.update(if state.sequencer_pending { b"seq1" } else { b"seq0" });
            hasher.update(b"\0conflicts=");
            hasher.update((state.conflict_count as u64).to_le_bytes());
            hasher.update(b"\0meta=");
            hasher.update(state.metadata_version.as_bytes());
        }
    }
    hasher.update(b"\0entries=");
    hasher.update((expected.index.len() as u64).to_le_bytes());
    for entry in &expected.index {
        hasher.update(b"\0e\0");
        hasher.update((entry.path.len() as u64).to_le_bytes());
        hasher.update(entry.path.as_bytes());
        hasher.update(b"\0");
        match &entry.original_path {
            Some(orig) => {
                hasher.update(b"orig:");
                hasher.update((orig.len() as u64).to_le_bytes());
                hasher.update(orig.as_bytes());
            }
            None => {
                hasher.update(b"orig:none");
            }
        }
        hasher.update(b"\0");
        hasher.update(entry.mode.as_bytes());
        hasher.update(b"\0");
        hasher.update(entry.oid.as_deref().unwrap_or("<none>").as_bytes());
        hasher.update(b"\0");
        hasher.update(entry.status.as_bytes());
        hasher.update(if entry.unmerged { b"\0u1" } else { b"\0u0" });
        hasher.update(if entry.deleted { b"\0d1" } else { b"\0d0" });
        hasher.update(if entry.intent_to_add { b"\0i1" } else { b"\0i0" });
        hasher.update(b"\0");
        hasher.update(entry.head_oid.as_deref().unwrap_or("<none>").as_bytes());
    }
    hex(&hasher.finalize())
}

/// 判断两份预期是否一致：逐项比较 + 指纹比较（指纹由本模块计算，必须一起变）。
pub fn expectations_match(a: &CommitExpectation, b: &CommitExpectation) -> bool {
    a.head == b.head
        && a.branch == b.branch
        && a.index == b.index
        && a.operation == b.operation
        && a.fingerprint == b.fingerprint
}

// ---------------------------------------------------------------------------
// 提交信息模板与最近提交信息（F02.3/F02.4）
// ---------------------------------------------------------------------------

/// `commit.template` 的内容；未设置时为 None。
///
/// 用 `git config --path` 让 Git 自己展开 `~/`（不自己猜路径语义），
/// 并区分「未设置」（退出码 1）与「读取失败」（其它退出码显式报错，
/// 不再被 `config_get` 静默吞掉）。只读取，不修改 Git 配置。
pub fn read_template(git: &Git) -> Result<Option<String>> {
    let output = git.run(&["config", "-z", "--path", "--get", "commit.template"])?;
    match output.exit_code {
        0 => {}
        1 => return Ok(None),
        other => {
            return Err(AppError::environment(format!(
                "读取 commit.template 配置失败（退出码 {other}）：{}",
                output.stderr.trim()
            )))
        }
    }
    let value = output.stdout.strip_suffix(&[0])
        .ok_or_else(|| AppError::environment("commit.template 配置缺少记录结束符"))?;
    let value = std::str::from_utf8(value)
        .map_err(|_| AppError::environment("commit.template 路径无法无损处理"))?;
    if value.is_empty() {
        return Ok(None);
    }
    // `--path` 已把 `~/` 展开成绝对路径；其余相对路径按仓库根解析（Git 的语义）。
    let path = PathBuf::from(value);
    let path = if path.is_absolute() { path } else { git.directory.join(path) };
    let text = std::fs::read_to_string(&path).map_err(|error| {
        AppError::environment(format!("读取提交信息模板失败（{}）：{error}", path.display()))
    })?;
    Ok(Some(text))
}

/// 最近 N 条提交信息（完整多行原文，规格 F02.4）。
///
/// 用 `-z`：每条记录以 NUL 结束，字段以 0x01 分隔，多行与 Unicode 原样保留。
pub fn read_recent_messages(git: &Git, limit: usize) -> Result<Vec<String>> {
    // 根仓库无 HEAD：没有最近提交，返回空而不是报错（规格 F02.4）。
    if read_head(git)?.is_none() {
        return Ok(Vec::new());
    }
    let output = git.run_ok(
        &[
            "-c",
            "core.quotePath=false",
            "log",
            "-z",
            "-n",
            &limit.to_string(),
            "--format=%H%x01%B",
        ],
        "读取最近提交信息",
    )?;
    let mut messages = Vec::new();
    for record in output.stdout.split(|byte| *byte == 0) {
        if record.is_empty() {
            continue;
        }
        let Some(position) = record.iter().position(|byte| *byte == 0x01) else {
            continue;
        };
        let text = String::from_utf8_lossy(&record[position + 1..]).into_owned();
        messages.push(text);
    }
    Ok(messages)
}

/// 固定提交对象的完整信息，保留正文自带的尾随换行。
pub fn read_last_message(git: &Git, head: &str) -> Result<String> {
    let output = git.run_ok(
        &["log", "-1", "-z", "--format=%B", head, "--"], "读取最近提交信息"
    )?;
    let message = output.stdout.strip_suffix(&[0])
        .ok_or_else(|| AppError::environment("最近提交信息缺少记录结束符"))?;
    Ok(String::from_utf8_lossy(message).into_owned())
}

// ---------------------------------------------------------------------------
// 本地上游包含判断（F06.2/F06.3）
// ---------------------------------------------------------------------------

/// HEAD 相对本地上游跟踪分支的发布状态（只用本地记录，不 fetch）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum UpstreamStatus {
    /// 没有配置上游跟踪分支（或 HEAD 未出生 / detached）。
    NoUpstream,
    /// **配置了**上游，但本地没有它的引用，无法判断包含关系（不猜测远端状态）。
    Unknown,
    /// HEAD 尚不在上游（本地领先或分叉——分叉也视为未包含，不猜测）。
    NotContained,
    /// HEAD 已包含于上游（本地上游记录里已有这个提交）。
    Contained,
}

/// 一次上游读数：状态 + 上游名 + 读数版本。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpstreamRead {
    pub status: UpstreamStatus,
    /// 本地上游跟踪分支名；未配置为 None。
    pub upstream: Option<String>,
    /// 读数版本：HEAD + 分支 + 上游名 + 上游对象名。
    /// 任何一项变化都使旧的 amend 二次确认失效（规格 F06.2）。
    pub version: String,
}

/// 上游配置的读取结果：必须区分「未配置」「已配置但本地没有引用」与读取错误。
enum UpstreamConfig {
    NotConfigured,
    Configured { name: String, object: Option<String> },
}

/// 读取当前分支配置的上游名与本地对象名。
///
/// 判据是 `branch.<name>.remote` + `branch.<name>.merge`（Git 认为「配置了上游」
/// 的依据），而不是 `@{upstream}` 的退出码：后者在本地缺少上游引用时会失败，
/// 直接当成「未配置」会漏掉「已配置但本地没有引用」这种必须如实说未知的情况。
fn read_upstream_config(git: &Git, branch: &str) -> Result<UpstreamConfig> {
    let remote = git.run(&["config", "--get", &format!("branch.{branch}.remote")])?;
    match remote.exit_code {
        0 => {}
        1 => return Ok(UpstreamConfig::NotConfigured),
        other => {
            return Err(AppError::environment(format!(
                "读取分支上游配置失败（退出码 {other}）：{}",
                remote.stderr.trim()
            )))
        }
    }
    let remote_name = remote.stdout_lossy().trim().to_string();
    let merge = git.run(&["config", "--get", &format!("branch.{branch}.merge")])?;
    match merge.exit_code {
        0 => {}
        1 => return Ok(UpstreamConfig::NotConfigured),
        other => {
            return Err(AppError::environment(format!(
                "读取分支上游合并配置失败（退出码 {other}）：{}",
                merge.stderr.trim()
            )))
        }
    }
    let merge_ref = merge.stdout_lossy().trim().to_string();
    if remote_name.is_empty() || merge_ref.is_empty() {
        return Ok(UpstreamConfig::NotConfigured);
    }

    // 上游名以 Git 自己的解析为准（`@{upstream}` 的名称解析不要求对象存在）。
    let name_output = git.run(&[
        "rev-parse",
        "--abbrev-ref",
        "--symbolic-full-name",
        "@{upstream}",
    ])?;
    let name = if name_output.exit_code == 0 {
        let value = name_output.stdout_lossy().trim().to_string();
        if value.is_empty() { None } else { Some(value) }
    } else {
        None
    };
    let Some(name) = name else {
        // 配置存在但 Git 解析不出上游名（例如 fetch refspec 缺失）：如实说未知。
        let short = merge_ref.strip_prefix("refs/heads/").unwrap_or(&merge_ref);
        return Ok(UpstreamConfig::Configured {
            name: format!("{remote_name}/{short}"),
            object: None,
        });
    };
    // 本地没有上游引用（从未 fetch 过该分支）：对象名缺失，但配置是存在的。
    let object = git.run(&["rev-parse", "--verify", "--quiet", "@{upstream}"])?;
    let object = match object.exit_code {
        0 => {
            let value = object.stdout_lossy().trim().to_string();
            if value.is_empty() { None } else { Some(value) }
        }
        1 => None,
        other => {
            return Err(AppError::environment(format!(
                "读取上游引用失败（退出码 {other}）：{}",
                object.stderr.trim()
            )))
        }
    };
    Ok(UpstreamConfig::Configured { name, object })
}

/// 判断 HEAD 是否已包含于本地上游跟踪分支（规格 F06.2/F06.3）。
///
/// 只读本地引用与已有对象，不 fetch、不访问网络。读取错误显式报错
/// （`Unknown` 只表示「已配置上游但本地没有它的引用」），绝不把读取失败
/// 当作「未包含」或「未配置」。
pub fn upstream_read(git: &Git) -> Result<UpstreamRead> {
    let head = read_head(git)?;
    let branch = read_branch(git)?;
    let config = match (&head, &branch) {
        (Some(_), Some(name)) => read_upstream_config(git, name)?,
        _ => UpstreamConfig::NotConfigured,
    };
    let (upstream, object, configured) = match config {
        UpstreamConfig::NotConfigured => (None, None, false),
        UpstreamConfig::Configured { name, object } => (Some(name), object, true),
    };

    let mut hasher = Sha256::new();
    hasher.update(b"upstream-v2\0");
    hasher.update(head.as_deref().unwrap_or("<unborn>").as_bytes());
    hasher.update(b"\0");
    hasher.update(branch.as_deref().unwrap_or("<detached>").as_bytes());
    hasher.update(b"\0");
    hasher.update(upstream.as_deref().unwrap_or("<none>").as_bytes());
    hasher.update(b"\0");
    hasher.update(object.as_deref().unwrap_or("<missing>").as_bytes());
    let version = hex(&hasher.finalize());

    let status = if !configured || head.is_none() {
        UpstreamStatus::NoUpstream
    } else if object.is_none() {
        UpstreamStatus::Unknown
    } else {
        let head_object = head.as_deref().ok_or_else(|| AppError::internal("HEAD 缺失"))?;
        let upstream_object = object.as_deref().ok_or_else(|| AppError::internal("上游对象缺失"))?;
        let output = git.run(&["merge-base", "--is-ancestor", head_object, upstream_object])?;
        match output.exit_code {
            0 => UpstreamStatus::Contained,
            1 => UpstreamStatus::NotContained,
            other => {
                return Err(AppError::git(format!(
                    "判断上游包含关系失败（退出码 {other}）：{}",
                    output.stderr.trim()
                )))
            }
        }
    };
    Ok(UpstreamRead { status, upstream, version })
}

/// 只要状态（内部与门禁摘要用）。
#[allow(dead_code)]
pub fn upstream_status(git: &Git) -> Result<UpstreamStatus> {
    Ok(upstream_read(git)?.status)
}

// ---------------------------------------------------------------------------
// 每 worktree 持久草稿（F02.5）
// ---------------------------------------------------------------------------

/// 一份提交草稿。
///
/// `skip_hooks` 是**本次命令**的选项，不属于持久状态：写入时一律存 false，
/// 读取时也强制为 false，避免重启或切换 worktree 后自动跳过 hooks（F04.2）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CommitDraft {
    pub message: String,
    /// 草稿写入时是否选择了 amend（信息预填自最近一次提交）。
    #[serde(default)]
    pub amend: bool,
    /// 历史字段：始终为 false，仅为兼容旧文件保留。
    #[serde(default)]
    pub skip_hooks: bool,
    /// 草稿保存时间（毫秒），诊断用。
    #[serde(default)]
    pub saved_at_ms: u64,
    #[serde(default)]
    pub revision: String,
}

impl CommitDraft {
    /// 草稿是否等于「没有任何输入」（全空时不写盘，避免留下空草稿）。
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.message.trim().is_empty() && !self.amend
    }
}

/// 条件清除草稿的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftClear {
    /// 磁盘上的草稿仍等于这次提交的输入，已删除。
    Cleared,
    /// 磁盘上没有草稿。
    Absent,
    /// 磁盘上的草稿已被重新编辑（内容不同），保留不动。
    Changed,
}

/// 每 worktree 单独的持久草稿存储（规格 F02.5）。
///
/// 目录按 worktree **身份**（真实路径 + gitDir + commonDir + 文件系统标识）绑定，
/// 同路径重建后身份不同，不继承旧草稿（与审阅标记同一边界，Q87）。
///
/// `lock` 在 clone 之间共享：保存与「比较后清除」必须互斥，否则
/// 「读到旧稿 → 另一处保存新稿 → 按旧稿删除」会把刚写的新稿删掉。
#[derive(Clone)]
pub struct DraftStore {
    directory: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl DraftStore {
    pub fn new(data_directory: PathBuf) -> Self {
        Self {
            directory: data_directory.join("commit-drafts"),
            lock: Arc::new(Mutex::new(())),
        }
    }

    /// 身份 → 存储键：真实身份的全部字段 + 文件系统标识一起哈希。
    pub fn identity_key(identity: &crate::recovery::WorktreeIdentity) -> Result<String> {
        if !identity.filesystem_ids.iter().all(Option::is_some) {
            return Err(AppError::environment(
                "worktree 文件系统标识不可用，不能绑定提交草稿",
            ));
        }
        let mut hasher = Sha256::new();
        hasher.update(b"commit-draft-v1");
        hasher.update(identity.worktree_path.display().to_string().as_bytes());
        hasher.update(b"\0");
        hasher.update(identity.git_dir.display().to_string().as_bytes());
        hasher.update(b"\0");
        hasher.update(identity.common_dir.display().to_string().as_bytes());
        hasher.update(b"\0");
        for id in &identity.filesystem_ids {
            hasher.update(id.as_deref().unwrap_or("").as_bytes());
            hasher.update(b"\0");
        }
        Ok(hex(&hasher.finalize()))
    }

    fn draft_path(&self, identity: &crate::recovery::WorktreeIdentity) -> Result<PathBuf> {
        let key = Self::identity_key(identity)?;
        Ok(self.directory.join(format!("{key}.json")))
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, ()>> {
        self.lock
            .lock()
            .map_err(|_| AppError::internal("提交草稿锁失败"))
    }

    fn read_file(path: &Path) -> Result<Option<CommitDraft>> {
        match std::fs::read(path) {
            Ok(bytes) => {
                let mut draft: CommitDraft = serde_json::from_slice(&bytes).map_err(|error| {
                    AppError::environment(format!("提交草稿损坏（{}）：{error}", path.display()))
                })?;
                // 跳过 hooks 是本次命令的选项，永不从磁盘恢复。
                draft.skip_hooks = false;
                Ok(Some(draft))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(AppError::environment(format!(
                "读取提交草稿失败（{}）：{error}",
                path.display()
            ))),
        }
    }

    /// 读取该身份的草稿；没有草稿返回 None；损坏或读取失败如实报错。
    pub fn load(&self, identity: &crate::recovery::WorktreeIdentity) -> Result<Option<CommitDraft>> {
        let path = self.draft_path(identity)?;
        Self::read_file(&path)
    }

    /// 保存草稿：原子写（临时文件 + rename + 同步）。失败如实报错。
    ///
    /// 与 `clear_if_matches` 共用同一把锁：两次写入按调用顺序落盘，
    /// 不会出现「先发后到把磁盘改回旧内容」。
    pub fn save(
        &self,
        identity: &crate::recovery::WorktreeIdentity,
        draft: &CommitDraft,
    ) -> Result<()> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SAVE_COUNTER: AtomicU64 = AtomicU64::new(1);
        let path = self.draft_path(identity)?;
        let _guard = self.lock()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut draft = draft.clone();
        draft.saved_at_ms = now_ms();
        // 跳过 hooks 只对本次命令生效，不进入持久草稿。
        draft.skip_hooks = false;
        let temp = path.with_extension(format!(
            "next-{}-{}",
            std::process::id(),
            SAVE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        {
            let mut file = std::fs::File::create(&temp)?;
            file.write_all(&serde_json::to_vec(&draft)?)?;
            file.sync_all()?;
        }
        std::fs::rename(&temp, &path)?;
        if let Some(parent) = path.parent() {
            crate::platform::sync_directory(parent)?;
        }
        Ok(())
    }

    /// 清除草稿（前端明确丢弃输入时）。不存在视为已清除。
    pub fn clear(&self, identity: &crate::recovery::WorktreeIdentity) -> Result<()> {
        let path = self.draft_path(identity)?;
        let _guard = self.lock()?;
        match std::fs::remove_file(&path) {
            Ok(()) => path.parent().map(crate::platform::sync_directory).unwrap_or(Ok(())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(AppError::environment(format!(
                "清除提交草稿失败（{}）：{error}",
                path.display()
            ))),
        }
    }

    /// 条件清除：只有磁盘上的草稿仍**逐字**等于这次提交的输入时才删除。
    ///
    /// 在锁内完成「读 → 比较 → 删」，因此与并发的 `save` 不会交错：
    /// 期间被重新编辑过（内容或 amend 选项不同）的草稿一律保留。
    pub fn clear_if_matches(
        &self,
        identity: &crate::recovery::WorktreeIdentity,
        expected_message: &str,
        expected_amend: bool,
        expected_revision: &str,
    ) -> Result<DraftClear> {
        let path = self.draft_path(identity)?;
        let _guard = self.lock()?;
        let Some(saved) = Self::read_file(&path)? else {
            return Ok(DraftClear::Absent);
        };
        // 逐字比较原文：不能用 trim 后的文本，否则「提交后再补一句」的新稿会被误删。
        if saved.message != expected_message || saved.amend != expected_amend || saved.revision != expected_revision {
            return Ok(DraftClear::Changed);
        }
        match std::fs::remove_file(&path) {
            Ok(()) => {
                if let Some(parent) = path.parent() { crate::platform::sync_directory(parent)?; }
                Ok(DraftClear::Cleared)
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(DraftClear::Absent),
            Err(error) => Err(AppError::environment(format!(
                "清除提交草稿失败（{}）：{error}",
                path.display()
            ))),
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ---------------------------------------------------------------------------
// 真实 git commit 执行（F03/F04）
// ---------------------------------------------------------------------------

/// 一次提交的固定命令选项（前端不能扩展成任意参数）。
pub struct CommitCommand<'a> {
    pub message: &'a str,
    pub amend: bool,
    pub skip_hooks: bool,
    /// 跳过 hooks 时使用的**空** hooks 目录（调用方按次创建）。
    /// 命令局部生效：只影响本次 `git commit`，不改用户 Git 配置。
    pub hooks_off_dir: Option<&'a Path>,
}

/// 一次提交执行的进程级结果。
#[derive(Debug)]
pub enum CommitRun {
    /// Git 正常退出（无论成功失败）：真实退出码 + 合并输出尾部。
    Finished {
        success: bool,
        exit_code: i32,
        output: String,
    },
    /// 启动失败（命令尚未启动，无结果风险）。
    SpawnFailed(String),
    /// 等待退出失败（进程已启动但结果不确定；不杀进程、不重跑）。
    Uncertain(String),
}

/// 流式提交监听器：每收到一片可交付文本回调一次（在读取线程上执行）。
///
/// 交付单位是固定大小读取到的片段，因此长行与无换行输出也能实时看到；`stderr` 只标记来源流，不改变交付顺序（F04.1）。
pub trait CommitOutputSink: Send + Sync + 'static {
    fn on_output(&self, text: &str, stderr: bool);
}

/// 聚合尾部输出的共享缓冲：按**字节**有界（F04.1 不无限保留内存）。
#[derive(Default)]
struct OutputTail {
    text: Mutex<String>,
    stderr: Mutex<String>,
    bytes: Mutex<usize>,
}

impl OutputTail {
    fn push(&self, text: &str, stderr: bool) {
        if let Ok(mut bytes) = self.bytes.lock() {
            *bytes = bytes.saturating_add(text.len());
        }
        if let Ok(mut tail) = self.text.lock() {
            tail.push_str(text);
            trim_front_bytes(&mut tail, OUTPUT_TAIL_BYTES);
        }
        if stderr {
            if let Ok(mut tail) = self.stderr.lock() {
                tail.push_str(text);
                    trim_front_bytes(&mut tail, STDERR_TAIL_BYTES);
            }
        }
    }

    fn text(&self) -> String {
        self.text.lock().map(|tail| tail.clone()).unwrap_or_default()
    }

    fn stderr_text(&self) -> String {
        self.stderr.lock().map(|tail| tail.clone()).unwrap_or_default()
    }

    fn byte_count(&self) -> usize {
        self.bytes.lock().map(|bytes| *bytes).unwrap_or_default()
    }
}

/// 从头部裁掉超出的字节（保留尾部），并落到字符边界上，避免半个 UTF-8 字符。
fn trim_front_bytes(text: &mut String, limit: usize) {
    if text.len() <= limit {
        return;
    }
    let mut cut = text.len() - limit;
    while cut < text.len() && !text.is_char_boundary(cut) {
        cut += 1;
    }
    text.drain(..cut);
}

impl CommitOutputSink for OutputTail {
    fn on_output(&self, text: &str, stderr: bool) {
        self.push(text, stderr);
    }
}

/// 固定大小读取并立即交付，包括无换行的进度/交互提示；只跨分片保留不完整的 UTF-8。
fn read_stream<R: Read>(
    mut pipe: R,
    sink: Arc<dyn CommitOutputSink>,
    tail: Arc<OutputTail>,
    is_stderr: bool,
    finished: Arc<AtomicBool>,
) -> std::io::Result<()> {
    let mut chunk = [0u8; READ_CHUNK_BYTES];
    let mut pending = Vec::with_capacity(READ_CHUNK_BYTES + 3);
    let deliver = |text: &str| {
        sink.on_output(text, is_stderr);
        tail.push(text, is_stderr);
    };
    let mut draining = None;
    let mut drained = 0;
    loop {
        if finished.load(Ordering::Acquire) {
            let started = draining.get_or_insert_with(Instant::now);
            if started.elapsed() >= OUTPUT_DRAIN_TIMEOUT || drained >= OUTPUT_TAIL_BYTES {
                break;
            }
        }
        let read = match pipe.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) if matches!(error.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {
                if finished.load(Ordering::Acquire) { break; }
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(error) => return Err(error),
        };
        if draining.is_some() { drained += read; }
        pending.extend_from_slice(&chunk[..read]);
        loop {
            match std::str::from_utf8(&pending) {
                Ok(text) => {
                    if !text.is_empty() { deliver(text); }
                    pending.clear();
                    break;
                }
                Err(error) => {
                    let valid = error.valid_up_to();
                    if valid > 0 {
                        // valid_up_to 保证此前的字节可无损解码。
                        deliver(std::str::from_utf8(&pending[..valid]).unwrap_or_default());
                    }
                    match error.error_len() {
                        Some(invalid) => {
                            deliver("\u{fffd}");
                            pending.drain(..valid + invalid);
                        }
                        None => {
                            pending.drain(..valid);
                            break;
                        }
                    }
                }
            }
        }
    }
    if !pending.is_empty() { deliver(&String::from_utf8_lossy(&pending)); }
    Ok(())
}

/// 用 worktree 会话的登录 shell 环境执行真正的 `git commit`。
///
/// - 信息用 `-F -` 从 stdin 传入（保留多行与 Unicode，不经过 shell 解释）；
/// - amend → `--amend`；
/// - skip_hooks → `-c core.hooksPath=<空目录>` + `--no-verify`：命令局部机制，
///   本次调用**全部** hooks（pre-commit、prepare-commit-msg、commit-msg、
///   post-commit、post-rewrite…）都找不到可执行文件，进程结束即失效，
///   不修改用户 Git 配置（规格 F04.2）；
/// - stdout/stderr 各自起线程分片实时读取（避免单流读满管道死锁），交给 sink；
///   进程结束前绝不 kill；无超时——hooks 可以合法运行很久（F04.4）；
/// - 返回 Finished（真实退出码 + 输出尾部）、SpawnFailed 或 Uncertain
///   （等待失败，结果待核对，规格 F03.5）。
pub fn execute_commit(
    git: &Git,
    command: CommitCommand<'_>,
    sink: Arc<dyn CommitOutputSink>,
) -> CommitRun {
    let mut args: Vec<String> = Vec::new();
    if command.skip_hooks {
        if let Some(directory) = command.hooks_off_dir {
            args.push("-c".into());
            args.push(format!("core.hooksPath={}", directory.display()));
        }
    }
    args.push("commit".into());
    if command.amend {
        args.push("--amend".into());
    }
    if command.skip_hooks {
        args.push("--no-verify".into());
    }
    args.push("--cleanup=strip".into());
    args.push("-F".into());
    args.push("-".into());

    // 在启动 Git 前准备可中断读取；通道失败属于未启动，不会留下无人排空的进程。
    let pipes = (|| -> std::io::Result<_> {
        let (stdout, stdout_writer) = crate::platform::output_pipe::OutputPipe::new()?;
        let (stderr, stderr_writer) = crate::platform::output_pipe::OutputPipe::new()?;
        Ok((stdout, stdout_writer, stderr, stderr_writer))
    })();
    let (stdout, stdout_writer, stderr, stderr_writer) = match pipes {
        Ok(pipes) => pipes,
        Err(error) => return CommitRun::SpawnFailed(format!("创建提交输出通道失败：{error}")),
    };
    let tail = Arc::new(OutputTail::default());
    let finished = Arc::new(AtomicBool::new(false));
    let mut builder = Command::new(&git.program);
    builder
        .current_dir(&git.directory)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(stdout_writer)
        .stderr(stderr_writer)
        .env_clear();
    for (key, value) in git.env.iter() {
        builder.env(key, value);
    }

    let started = std::time::Instant::now();
    let mut child = match builder.spawn() {
        Ok(child) => child,
        Err(error) => {
            return CommitRun::SpawnFailed(match error.kind() {
                std::io::ErrorKind::NotFound => {
                    format!("启动 git commit 失败：找不到 {}", git.program.display())
                }
                _ => format!("启动 git commit 失败：{error}"),
            });
        }
    };

    drop(builder); // 父进程不能持有输出写端，否则 EOF 永远不会到达。

    // stdin：一次性写入完整提交信息后关闭。写失败不杀进程（Git 会因读不到
    // 信息自行失败，真实输出已流回结果区）。
    let stdin = child.stdin.take();
    let message_bytes = command.message.as_bytes().to_vec();
    let stdin_result = std::thread::spawn(move || -> std::io::Result<()> {
        let mut pipe = stdin.ok_or_else(|| std::io::Error::other("stdin 管道不可用"))?;
        pipe.write_all(&message_bytes)?;
        pipe.flush()?;
        Ok(())
    });

    let mut readers = Vec::with_capacity(2);
    for (pipe, is_stderr) in [(stdout, false), (stderr, true)] {
        let sink = sink.clone();
        let tail = tail.clone();
        let finished = finished.clone();
        readers.push(std::thread::spawn(move || read_stream(pipe, sink, tail, is_stderr, finished)));
    }

    // 始终持有 Child 和写门，直到真正回收进程。等待出错不是 Git 已退出的证据；
    // 只能重试等待，不能释放写锁让一次 listChanges 把仍活着的提交核销。
    let mut wait_error = None;
    let status = loop {
        match child.wait() {
            Ok(status) => break status,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => {
                if wait_error.is_none() {
                    let note = format!("等待 git commit 终态失败，继续保留写入保护并等待：{error}\n");
                    sink.on_output(&note, true);
                    tail.push(&note, true);
                    wait_error = Some(error.to_string());
                }
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
    };
    finished.store(true, Ordering::Release);
    let elapsed = started.elapsed();

    let stdin_error = match stdin_result.join() {
        Ok(Ok(())) => None,
        Ok(Err(error)) => Some(error.to_string()),
        Err(_) => Some("提交信息写入线程中断".into()),
    };
    let mut output_error = None;
    for reader in readers {
        match reader.join() {
            Ok(Ok(())) => {}
            Ok(Err(error)) => { output_error = Some(error.to_string()); }
            Err(_) => { output_error = Some("输出读取线程中断".to_string()); }
        }
    }

    let exit_code = status.code().unwrap_or(-1);
    let masked_output = crate::git::cap_output_text(
        &crate::git::mask_credentials(&tail.text()),
        tail.byte_count() > crate::git::LOG_OUTPUT_LIMIT,
    );
    // 命令日志：直接 spawn 的提交也要留证（git/mod.rs 的「每条命令都记入」）。
    git.log.record(CommandRecord {
        sequence: 0,
        worktree_id: git.worktree_id.clone(),
        started_at_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_millis() as u64)
            .unwrap_or_default(),
        program: git.program.display().to_string(),
        args: args.iter().map(|arg| crate::git::mask_credentials(arg)).collect(),
        directory: git.directory.display().to_string(),
        exit_code,
        elapsed_ms: elapsed.as_millis() as u64,
        timed_out: false,
        stdout_bytes: tail.byte_count(),
        stderr_excerpt: crate::git::mask_credentials(&tail.stderr_text().chars().take(400).collect::<String>()),
        output: masked_output.0,
        output_truncated: masked_output.1,
    });

    if let Some(error) = wait_error.or(output_error) {
        return CommitRun::Uncertain(format!("提交进程已结束，但结果记录需要核对：{error}\n{}", tail.text()));
    }
    if status.code().is_none() {
        return CommitRun::Uncertain(format!("git commit 非正常终止，不能确定是否已创建提交，请核对 HEAD：\n{}", tail.text()));
    }
    if status.success() {
        if let Some(error) = stdin_error {
            return CommitRun::Uncertain(format!("无法确认提交信息完整写入，结果待核对：{error}\n{}", tail.text()));
        }
    }
    CommitRun::Finished { success: status.success(), exit_code, output: tail.text() }
}

/// 提交后读取真实新 HEAD（成功反馈与审阅刷新用）。失败显式报错。
pub fn read_head_after(git: &Git) -> Result<String> {
    read_head(git)?.ok_or_else(|| AppError::git("提交后读取 HEAD 失败：Git 报告无 HEAD"))
}

/// 跳过 hooks 用的空目录：每次提交在 `root` 下建一个新的空目录，
/// 结束后尽力删除。返回的目录一定为空（Git 在其中找不到任何 hook）。
pub fn prepare_hooks_off_dir(root: &Path) -> Result<PathBuf> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let base = root.join("no-hooks");
    std::fs::create_dir_all(&base)?;
    let directory = base.join(format!(
        "{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&directory).map_err(|error| {
        AppError::environment(format!(
            "无法准备跳过 hooks 的空目录（{}）：{error}",
            directory.display()
        ))
    })?;
    Ok(directory)
}

/// 清理跳过 hooks 用的临时目录（失败不影响提交结果，不向上报错）。
pub fn release_hooks_off_dir(directory: &Path) {
    let _ = std::fs::remove_dir(directory);
}
