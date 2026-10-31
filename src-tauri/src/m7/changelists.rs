//! 更改列表：分组只存应用数据；Git 原始块、提交树和写前凭据由 Rust 生成。
use crate::{
    commit::{self, CommitExpectation},
    error::{AppError, ErrorKind, Result},
    git::{self, Git},
    session::WorktreeSession,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Mutex,
};
use ts_rs::TS;

static NEXT_GROUP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
fn rejected() -> AppError {
    AppError::coded(
        ErrorKind::Rejected,
        "errors.m7.changelistChanged",
        "更改列表或文件已变化，请刷新并重新选择",
        &[],
    )
}
fn invalid() -> AppError {
    AppError::coded(
        ErrorKind::Rejected,
        "errors.m7.changelistInvalid",
        "更改列表操作无效，请核对名称和选择",
        &[],
    )
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChangeGroup {
    pub id: String,
    pub name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChangeItem {
    pub id: String,
    pub path: String,
    pub group: String,
    pub fingerprint: String,
    pub selected: bool,
    pub stale: bool,
    #[serde(default, alias = "preview")]
    pub summary: String,
    #[serde(default)]
    pub segment: Option<u32>,
    #[serde(default)]
    pub readonly_reason: Option<git::status::ReadonlyReason>,
    #[serde(default)]
    pub source: Option<git::status::ChangesKind>,
    /// None 表示整文件条目；Some 是系统 Git 的原始 hunk 序号。
    pub hunk: Option<usize>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    groups: Vec<ChangeGroup>,
    default_group: String,
    items: Vec<ChangeItem>,
}
impl Default for Saved {
    fn default() -> Self {
        Self {
            groups: vec![ChangeGroup {
                id: "default".into(),
                name: String::new(),
            }],
            default_group: "default".into(),
            items: vec![],
        }
    }
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChangelistView {
    pub worktree_id: String,
    pub identity: String,
    pub revision: String,
    pub groups: Vec<ChangeGroup>,
    pub default_group: String,
    pub items: Vec<ChangeItem>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChangelistSelection {
    pub revision: String,
    pub identity: String,
}
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ChangeAction {
    Create { name: String },
    Rename { group: String, name: String },
    Delete { group: String },
    Default { group: String },
    Move { items: Vec<String>, group: String },
    Select { items: Vec<String>, selected: bool },
}
pub struct ChangelistStore {
    directory: PathBuf,
    lock: Mutex<()>,
}
struct Scan {
    items: Vec<ChangeItem>,
    files: BTreeMap<String, git::diff::FileChange>,
    fingerprint: String,
}
impl ChangelistStore {
    pub fn new(data: &Path) -> Self {
        Self {
            directory: data.join("changelists"),
            lock: Mutex::new(()),
        }
    }
    fn path(&self, identity: &str) -> PathBuf {
        self.directory.join(format!("{identity}.json"))
    }
    fn read(&self, key: &str) -> Result<Saved> {
        match std::fs::read(self.path(key)) {
            Ok(b) => Ok(serde_json::from_slice(&b)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Saved::default()),
            Err(e) => Err(e.into()),
        }
    }
    fn write(&self, key: &str, s: &Saved) -> Result<()> {
        super::atomic_write(&self.path(key), &serde_json::to_vec_pretty(s)?)
    }
    fn refreshed(&self, session: &WorktreeSession) -> Result<(String, Saved, Scan)> {
        let key = super::edit::identity_key(session)?;
        let mut saved = self.read(&key)?;
        let scan = scan(session)?;
        let mut old: BTreeMap<_, _> = saved
            .items
            .into_iter()
            .map(|item| (item.id.clone(), item))
            .collect();
        let mut items = Vec::new();
        for fresh in &scan.items {
            let mut next = fresh.clone();
            if let Some(previous) = old.remove(&fresh.id) {
                next.group = previous.group;
                next.selected = previous.selected
                    && fresh.readonly_reason.is_none()
                    && !previous.stale
                    && previous.fingerprint == fresh.fingerprint;
            } else {
                next.group = saved.default_group.clone();
            }
            items.push(next);
        }
        // 无法对应的块保留原组、预览供人工核对，但不能继续提交。
        for mut previous in old.into_values() {
            previous.summary = previous.summary.chars().take(200).collect();
            previous.selected = false;
            previous.stale = true;
            items.push(previous);
        }
        saved.items = items;
        session.require_identity()?;
        if super::edit::identity_key(session)? != key {
            return Err(rejected());
        }
        self.write(&key, &saved)?;
        Ok((key, saved, scan))
    }
    fn to_view(
        session: &WorktreeSession,
        key: String,
        saved: Saved,
        scan: &Scan,
    ) -> Result<ChangelistView> {
        let revision = super::sha256_hex(&serde_json::to_vec(&(&key, &saved, &scan.fingerprint))?);
        Ok(ChangelistView {
            worktree_id: session.id.clone(),
            identity: key,
            revision,
            groups: saved.groups,
            default_group: saved.default_group,
            items: saved.items,
        })
    }
    pub fn view(&self, session: &WorktreeSession) -> Result<ChangelistView> {
        let _write = session.acquire_write_lock()?;
        let _lock = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let (key, saved, scan) = self.refreshed(session)?;
        Self::to_view(session, key, saved, &scan)
    }
    pub fn change(
        &self,
        session: &WorktreeSession,
        expected: &ChangelistSelection,
        action: ChangeAction,
    ) -> Result<ChangelistView> {
        let _lock = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let (key, mut saved, scan) = self.refreshed(session)?;
        let before = Self::to_view(session, key.clone(), saved.clone(), &scan)?;
        if before.revision != expected.revision || key != expected.identity {
            return Err(rejected());
        }
        let exists = |group: &str| saved.groups.iter().any(|g| g.id == group);
        match action {
            ChangeAction::Create { name } => {
                let name = name.trim();
                if name.is_empty() || name.len() > 256 {
                    return Err(invalid());
                }
                saved.groups.push(ChangeGroup {
                    id: format!(
                        "group-{}-{}",
                        super::now_ms(),
                        NEXT_GROUP.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    ),
                    name: name.into(),
                });
            }
            ChangeAction::Rename { group, name } => {
                let name = name.trim();
                if name.is_empty() || name.len() > 256 {
                    return Err(invalid());
                }
                saved
                    .groups
                    .iter_mut()
                    .find(|g| g.id == group)
                    .ok_or_else(invalid)?
                    .name = name.into();
            }
            ChangeAction::Default { group } => {
                if !exists(&group) {
                    return Err(invalid());
                }
                saved.default_group = group;
            }
            ChangeAction::Delete { group } => {
                if !exists(&group) || saved.groups.len() == 1 {
                    return Err(invalid());
                }
                if saved.default_group == group {
                    saved.default_group = saved
                        .groups
                        .iter()
                        .find(|g| g.id != group)
                        .ok_or_else(invalid)?
                        .id
                        .clone();
                }
                saved.groups.retain(|g| g.id != group);
                for item in &mut saved.items {
                    if item.group == group {
                        item.group = saved.default_group.clone();
                    }
                }
            }
            ChangeAction::Move { items, group } => {
                if !exists(&group)
                    || items
                        .iter()
                        .any(|id| !saved.items.iter().any(|i| &i.id == id))
                {
                    return Err(invalid());
                }
                for item in &mut saved.items {
                    if items.contains(&item.id) {
                        item.group = group.clone();
                    }
                }
            }
            ChangeAction::Select { items, selected } => {
                if items.iter().any(|id| {
                    !saved
                        .items
                        .iter()
                        .any(|i| &i.id == id && !i.stale && i.readonly_reason.is_none())
                }) {
                    return Err(rejected());
                }
                for item in &mut saved.items {
                    if items.contains(&item.id) {
                        item.selected = selected;
                    }
                }
            }
        }
        self.write(&key, &saved)?;
        Self::to_view(session, key, saved, &scan)
    }
    pub(crate) fn prepare<'a>(
        &'a self,
        session: &WorktreeSession,
        selection: &ChangelistSelection,
        expected: &CommitExpectation,
        amend: bool,
    ) -> Result<Prepared<'a>> {
        let _lock = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let (key, saved, scan) = self.refreshed(session)?;
        let view = Self::to_view(session, key, saved, &scan)?;
        if view.identity != selection.identity || view.revision != selection.revision {
            return Err(rejected());
        }
        let git = session.git();
        let actual = commit::capture(git)?;
        if !commit::expectations_match(&actual, expected)
            || actual.has_unmerged()
            || actual
                .operation
                .as_ref()
                .is_some_and(|operation| !operation.commit_allowed())
        {
            return Err(rejected());
        }
        let selected: Vec<_> = view
            .items
            .iter()
            .filter(|i| i.selected && !i.stale)
            .cloned()
            .collect();
        if selected.is_empty() && !amend && !actual.allows_empty_diff_commit() {
            return Err(invalid());
        }
        for item in &selected {
            if let Some(reason) = item.readonly_reason {
                return Err(reason.error());
            }
        }
        std::fs::create_dir_all(self.directory.join("commit-evidence"))?;
        // 保留全部失败与不确定结果的 index 和选项原件；只用 Git CLI 创建/改写 index。
        let directory = tempfile::Builder::new()
            .prefix("commit-")
            .tempdir_in(self.directory.join("commit-evidence"))?
            .keep();
        let index = directory.join("selected-index");
        let alternate =
            git.with_env_vars(&[("GIT_INDEX_FILE", index.to_str().ok_or_else(invalid)?)]);
        if let Some(head) = &actual.head {
            alternate.run_ok(&["read-tree", head], "初始化选择提交")?;
        } else {
            alternate.run_ok(&["read-tree", "--empty"], "初始化选择提交")?;
        }
        std::fs::write(
            directory.join("selection.json"),
            serde_json::to_vec_pretty(&view)?,
        )?;
        std::fs::write(
            directory.join("original-expectation.json"),
            serde_json::to_vec_pretty(&actual)?,
        )?;
        let original_index = git
            .run_ok(&["ls-files", "--stage", "-z"], "读取原暂存区")?
            .stdout;
        std::fs::write(directory.join("original-index-entries"), &original_index)?;
        std::fs::write(
            directory.join("original-index.patch"),
            git.run_ok(
                &["diff", "--cached", "--binary", "--full-index"],
                "保留原暂存内容",
            )?
            .stdout,
        )?;
        let paths: BTreeSet<_> = selected.iter().map(|i| i.path.clone()).collect();
        for path in &paths {
            let chosen: Vec<_> = selected.iter().filter(|i| &i.path == path).collect();
            let all = scan.items.iter().filter(|i| &i.path == path).count() == chosen.len();
            if all {
                alternate
                    .stage_file(path, None)?
                    .into_success("写入所选文件")?;
            } else {
                let file = scan.files.get(path).ok_or_else(rejected)?;
                let mut hunks: BTreeMap<usize, Vec<u32>> = BTreeMap::new();
                for item in chosen {
                    hunks
                        .entry(item.hunk.ok_or_else(rejected)?)
                        .or_default()
                        .push(item.segment.ok_or_else(rejected)?);
                }
                for (hunk, segments) in hunks {
                    let patch =
                        git::segments::rewrite_hunk_for_selection(file, hunk, &segments, true)
                            .ok_or_else(rejected)?;
                    alternate
                        .apply_index_patch(patch, false, false)?
                        .into_success("写入所选 Git 块")?;
                }
            }
        }
        let prepared = Prepared {
            git: alternate,
            directory,
            original: actual,
            original_index,
            paths: paths.into_iter().collect(),
            fingerprint: scan.fingerprint,
            ids: selected.iter().map(|i| i.id.clone()).collect(),
            store: self,
            selection: selection.clone(),
        };
        prepared.recheck(session)?;
        Ok(prepared)
    }
    pub(crate) fn finish(
        &self,
        session: &WorktreeSession,
        selection: &ChangelistSelection,
        ids: &[String],
    ) -> Result<()> {
        let _lock = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let key = super::edit::identity_key(session)?;
        if key != selection.identity {
            return Err(rejected());
        }
        let mut saved = self.read(&key)?;
        // 只移除本次仍被选择的条目；未提交分组与失配记录保留。
        saved.items.retain(|i| !ids.contains(&i.id));
        self.write(&key, &saved)
    }
}
pub(crate) struct Prepared<'a> {
    store: &'a ChangelistStore,
    selection: ChangelistSelection,
    pub git: Git,
    directory: PathBuf,
    original: CommitExpectation,
    original_index: Vec<u8>,
    paths: Vec<String>,
    fingerprint: String,
    ids: Vec<String>,
}
impl Prepared<'_> {
    pub fn recheck(&self, session: &WorktreeSession) -> Result<()> {
        if let Some(path) = session.git().git_path("index.lock") {
            let path = if path.is_absolute() {
                path
            } else {
                session.git().directory.join(path)
            };
            if path.exists() {
                return Err(rejected());
            }
        }
        if !commit::expectations_match(&commit::capture(session.git())?, &self.original)
            || scan(session)?.fingerprint != self.fingerprint
        {
            return Err(rejected());
        }
        session.require_identity()?;
        Ok(())
    }
    pub fn reconcile(&self, session: &WorktreeSession, new_head: &str) -> Result<()> {
        let git = session.git();
        session.require_identity()?;
        if commit::read_head(git)?.as_deref() != Some(new_head)
            || git
                .run_ok(&["ls-files", "--stage", "-z"], "核对原暂存区")?
                .stdout
                != self.original_index
        {
            return Err(rejected());
        }
        if self.paths.is_empty() {
            std::fs::write(self.directory.join("reconciled-head"), new_head)?;
            return self.store.finish(session, &self.selection, &self.ids);
        }
        let mut args = vec!["--literal-pathspecs", "reset", "-q", new_head, "--"];
        args.extend(self.paths.iter().map(String::as_str));
        git.run(&args)?.into_success("提交后更新所选路径暂存区")?;
        std::fs::write(self.directory.join("reconciled-head"), new_head)?;
        self.store.finish(session, &self.selection, &self.ids)
    }
}
fn scan(session: &WorktreeSession) -> Result<Scan> {
    session.require_identity()?;
    let git = session.git();
    let expected = commit::capture(git)?;
    let empty = git
        .run_with_input(&["hash-object", "-t", "tree", "--stdin"], vec![])?
        .into_success("读取空树标识")?
        .stdout_lossy()
        .trim()
        .to_string();
    let head = expected.head.as_deref().unwrap_or(&empty);
    let mut args = vec!["--literal-pathspecs", "-c", "core.quotePath=false", "diff"];
    args.extend_from_slice(git::DIFF_FLAGS);
    args.extend_from_slice(&["--no-renames", "--binary", head, "--"]);
    let raw = git.run_ok(&args, "读取更改列表 Git diff")?.stdout;
    let parsed = git::diff::parse_bytes(&raw);
    let status = git::status::read(git)?;
    let mut items = Vec::new();
    let mut files = BTreeMap::new();
    let mut whole = Vec::new();
    for file in parsed.files {
        let fingerprint = super::sha256_hex(&file.raw_patch);
        let readonly_reason = readonly_for(git, &status, &file.path);
        let source = if status.entries.iter().any(|entry| {
            entry.path == file.path && entry.changes == git::status::ChangesKind::Unstaged
        }) {
            git::status::ChangesKind::Unstaged
        } else {
            git::status::ChangesKind::Staged
        };
        let staged = git::diff::parse_bytes(&git.diff_index(Some(&file.path))?);
        let staged_signatures: Vec<_> = staged
            .files
            .iter()
            .flat_map(git::segments::segment_signatures)
            .collect();
        if file.hunks.is_empty()
            || file.binary
            || file.status != git::status::FileStatus::Modified
            || readonly_reason.is_some()
        {
            items.push(ChangeItem {
                id: super::sha256_hex(format!("{}:{fingerprint}", file.path).as_bytes()),
                path: file.path.clone(),
                group: String::new(),
                fingerprint,
                selected: false,
                stale: false,
                summary: String::new(),
                hunk: None,
                segment: None,
                readonly_reason,
                source: Some(source),
            });
        } else {
            for (i, hunk) in file.hunks.iter().enumerate() {
                for segment in git::segments::segments_of_hunk(hunk, i as u32) {
                    let patch = git::segments::rewrite_hunk_for_segment(
                        &file,
                        i,
                        segment.segment_index,
                        true,
                    )
                    .ok_or_else(rejected)?;
                    let selected_file = git::diff::parse_bytes(&patch);
                    let signatures: Vec<_> = selected_file
                        .files
                        .iter()
                        .flat_map(git::segments::segment_signatures)
                        .collect();
                    let source = if signatures
                        .iter()
                        .all(|signature| staged_signatures.contains(signature))
                    {
                        git::status::ChangesKind::Staged
                    } else {
                        source
                    };
                    let owners = git::segments::line_owners(hunk);
                    let summary = hunk
                        .lines
                        .iter()
                        .zip(owners)
                        .filter(|(_, owner)| *owner == Some(segment.segment_index))
                        .take(2)
                        .map(|(line, _)| line.text.chars().take(80).collect::<String>())
                        .collect::<Vec<_>>()
                        .join(" → ");
                    items.push(ChangeItem {
                        id: super::sha256_hex(&[file.path.as_bytes(), &patch].concat()),
                        path: file.path.clone(),
                        group: String::new(),
                        fingerprint: fingerprint.clone(),
                        selected: false,
                        stale: false,
                        summary,
                        hunk: Some(i),
                        segment: Some(segment.segment_index),
                        readonly_reason: None,
                        source: Some(source),
                    });
                }
            }
        }
        files.insert(file.path.clone(), file);
    }
    let untracked = git
        .run_ok(
            &["ls-files", "--others", "--exclude-standard", "-z"],
            "读取未跟踪文件",
        )?
        .stdout;
    for bytes in untracked.split(|b| *b == 0).filter(|b| !b.is_empty()) {
        let path = match std::str::from_utf8(bytes) {
            Ok(path) => path.to_string(),
            Err(_) => {
                let fingerprint = super::sha256_hex(bytes);
                whole.extend_from_slice(bytes);
                items.push(ChangeItem {
                    id: fingerprint.clone(),
                    path: String::from_utf8_lossy(bytes).into_owned(),
                    group: String::new(),
                    fingerprint,
                    selected: false,
                    stale: false,
                    summary: String::new(),
                    hunk: None,
                    segment: None,
                    readonly_reason: Some(git::status::ReadonlyReason::NonUtf8Path),
                    source: Some(git::status::ChangesKind::Untracked),
                });
                continue;
            }
        };
        let full = git.directory.join(&path);
        let metadata = std::fs::symlink_metadata(&full)?;
        let readonly_reason = readonly_for(git, &status, &path);
        let content = if metadata.file_type().is_symlink() {
            std::fs::read_link(&full)?
                .to_string_lossy()
                .as_bytes()
                .to_vec()
        } else if metadata.is_file() {
            std::fs::read(&full)?
        } else {
            Vec::new()
        };
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode() & 0o111
        };
        #[cfg(not(unix))]
        let mode = metadata.permissions().readonly() as u32;
        let fingerprint =
            super::sha256_hex(&[content.as_slice(), mode.to_string().as_bytes()].concat());
        whole.extend_from_slice(bytes);
        whole.extend_from_slice(fingerprint.as_bytes());
        items.push(ChangeItem {
            id: super::sha256_hex(&[bytes, fingerprint.as_bytes()].concat()),
            path,
            group: String::new(),
            fingerprint,
            selected: false,
            stale: false,
            summary: String::new(),
            hunk: None,
            segment: None,
            readonly_reason,
            source: Some(git::status::ChangesKind::Untracked),
        });
    }

    let fingerprint = super::sha256_hex(&[expected.fingerprint.as_bytes(), &raw, &whole].concat());
    Ok(Scan {
        items,
        files,
        fingerprint,
    })
}

impl WorktreeSession {
    #[allow(clippy::too_many_arguments)]
    pub fn execute_changelist_commit_flow(
        self: &std::sync::Arc<Self>,
        store: &ChangelistStore,
        selection: &ChangelistSelection,
        drafts: &crate::commit::DraftStore,
        recovery: &crate::recovery::RecoveryStore,
        hooks_off_root: PathBuf,
        mut options: crate::commit_flow::CommitOptions,
        operation_id: String,
        emit: std::sync::Arc<dyn Fn(crate::commit_flow::CommitOutputEvent) + Send + Sync>,
    ) -> Result<crate::commit_flow::CommitOutcome> {
        let _write = self.acquire_write_lock()?;
        let prepared = store.prepare(self, selection, &options.expected, options.amend)?;
        // amend 二次确认先对真实原预期校验，再转成这次独立 index 的凭据。
        if let Some(confirmation) = &mut options.amend_confirmation {
            if confirmation.expectation_fingerprint != options.expected.fingerprint
                || confirmation.message != options.message
                || confirmation.amend != options.amend
                || confirmation.skip_hooks != options.skip_hooks
            {
                return Err(rejected());
            }
            confirmation.expectation_fingerprint = commit::capture(&prepared.git)?.fingerprint;
        }
        options.expected = commit::capture(&prepared.git)?;
        let outcome = self.run_commit_flow(
            drafts,
            recovery,
            hooks_off_root,
            options,
            operation_id,
            emit,
            Some(&prepared),
        )?;
        Ok(outcome)
    }
}

fn readonly_for(
    git: &Git,
    status: &git::status::WorktreeStatus,
    path: &str,
) -> Option<git::status::ReadonlyReason> {
    if let Some(reason) = status
        .entries
        .iter()
        .filter(|entry| entry.path == path)
        .find_map(|entry| entry.readonly_reason)
    {
        return Some(reason);
    }
    let full = git.directory.join(path);
    if full.is_symlink() {
        return Some(git::status::ReadonlyReason::Symlink);
    }
    if full.is_dir() || full.join(".git").exists() {
        return Some(git::status::ReadonlyReason::NestedRepo);
    }
    None
}
