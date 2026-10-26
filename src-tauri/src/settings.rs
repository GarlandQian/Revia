//! 设置与仓库登记：保存在 Revia 数据目录（Q49：`io.github.garlandqian.revia`）。
//!
//! 只写自己的数据目录，不改动用户的 Git 配置，也不往仓库目录里写东西。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::git::CommandLog;
use crate::session::{id_for_path, RepoEntry};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// 登记过的仓库根路径（worktree 的路径也从这里推导）。
    pub repos: Vec<String>,
    /// 上次打开的 worktree ID。
    pub last_worktree_id: Option<String>,
    /// diff 字号（⌘+ / ⌘-，Q39）。
    pub diff_font_size: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            repos: Vec::new(),
            last_worktree_id: None,
            diff_font_size: 12,
        }
    }
}

pub struct Registry {
    directory: PathBuf,
    settings: Settings,
    repos: Vec<RepoEntry>,
    command_log: Arc<CommandLog>,
}

impl Registry {
    /// 读取设置并重建仓库列表；路径不存在的仓库保留，但标记 `exists = false`。
    pub fn load(directory: PathBuf, command_log: Arc<CommandLog>) -> Self {
        let settings = read_settings(&directory).unwrap_or_default();
        let mut repos = Vec::with_capacity(settings.repos.len());
        for path in &settings.repos {
            match RepoEntry::build(Path::new(path), command_log.clone()) {
                Ok(entry) => repos.push(entry),
                Err(_) => repos.push(RepoEntry {
                    id: id_for_path(Path::new(path)),
                    path: path.clone(),
                    name: Path::new(path)
                        .file_name()
                        .map(|value| value.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.clone()),
                    exists: Path::new(path).is_dir(),
                    worktrees: Vec::new(),
                }),
            }
        }
        Self {
            directory,
            settings,
            repos,
            command_log,
        }
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn list(&self) -> Vec<RepoEntry> {
        self.repos.clone()
    }

    pub fn find(&self, id: &str) -> Option<RepoEntry> {
        self.repos.iter().find(|repo| repo.id == id).cloned()
    }

    /// 登记一个仓库或 worktree。路径由用户选择，是唯一允许接收任意路径的命令。
    pub fn add(&mut self, path: &str) -> Result<RepoEntry> {
        let candidate = PathBuf::from(path);
        if !candidate.is_dir() {
            return Err(AppError::rejected(format!("目录不存在：{path}")));
        }
        let entry = RepoEntry::build(&candidate, self.command_log.clone())?;
        let canonical = std::fs::canonicalize(Path::new(&entry.path)).unwrap_or_else(|_| PathBuf::from(&entry.path));

        let existing = self
            .repos
            .iter_mut()
            .position(|repo| Path::new(&repo.path) == canonical.as_path() || repo.id == entry.id);
        if let Some(index) = existing {
            self.repos[index] = entry.clone();
            self.persist()?;
            return Ok(entry);
        }

        self.repos.push(entry.clone());
        self.settings.repos.push(entry.path.clone());
        self.persist()?;
        Ok(entry)
    }

    /// 选择当前仓库，并记录最近打开的 worktree。
    pub fn select(&mut self, repo_id: &str, worktree_id: Option<&str>) -> Result<RepoEntry> {
        let entry = self
            .find(repo_id)
            .ok_or_else(|| AppError::rejected("仓库未登记，请先添加"))?;
        if let Some(worktree_id) = worktree_id {
            self.settings.last_worktree_id = Some(worktree_id.to_string());
        }
        self.persist()?;
        Ok(entry)
    }

    pub fn set_last_worktree(&mut self, worktree_id: &str) -> Result<()> {
        self.settings.last_worktree_id = Some(worktree_id.to_string());
        self.persist()
    }

    /// 按 ID 找 worktree 的路径：先看各仓库登记的 worktree，再看仓库自身。
    pub fn worktree_path(&self, worktree_id: &str) -> Option<PathBuf> {
        for repo in &self.repos {
            for worktree in &repo.worktrees {
                if id_for_path(Path::new(&worktree.path)) == worktree_id {
                    return Some(PathBuf::from(&worktree.path));
                }
            }
            if repo.id == worktree_id || id_for_path(Path::new(&repo.path)) == worktree_id {
                return Some(PathBuf::from(&repo.path));
            }
        }
        None
    }

    /// 目标路径落在哪个已登记 worktree 里（用于把写入归属到真实目标，而不是当前选择）。
    ///
    /// 取**最具体**的匹配：登记了外层 A 与嵌套的独立仓库 B 时，B 内的路径归属 B。
    /// 否则会拿到 A 的锁，和 B 的写操作并发（Q90）。
    pub fn worktree_containing(&self, path: &Path) -> Option<String> {
        let target = fs::canonicalize(path).ok()?;
        let mut best: Option<(usize, String)> = None;
        let mut consider = |root: &Path, id: String| {
            let root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
            if target == root || target.starts_with(&root) {
                let depth = root.components().count();
                if best.as_ref().is_none_or(|(current, _)| depth > *current) {
                    best = Some((depth, id));
                }
            }
        };
        for repo in &self.repos {
            for worktree in &repo.worktrees {
                consider(Path::new(&worktree.path), id_for_path(Path::new(&worktree.path)));
            }
            consider(Path::new(&repo.path), repo.id.clone());
        }
        best.map(|(_, id)| id)
    }

    pub fn update_font_size(&mut self, size: u32) -> Result<Settings> {
        self.settings.diff_font_size = size.clamp(9, 24);
        self.persist()?;
        Ok(self.settings.clone())
    }

    fn persist(&self) -> Result<()> {
        write_settings(&self.directory, &self.settings)
    }
}

pub fn settings_path(directory: &Path) -> PathBuf {
    directory.join("settings.json")
}

fn read_settings(directory: &Path) -> Result<Settings> {
    let path = settings_path(directory);
    let text = std::fs::read_to_string(&path).map_err(|error| AppError::internal(format!("读取设置失败：{error}")))?;
    serde_json::from_str(&text).map_err(|error| AppError::internal(format!("解析设置失败：{error}")))
}

fn write_settings(directory: &Path, settings: &Settings) -> Result<()> {
    std::fs::create_dir_all(directory)
        .map_err(|error| AppError::internal(format!("创建数据目录失败：{error}")))?;
    let path = settings_path(directory);
    let text = serde_json::to_string_pretty(settings)?;
    std::fs::write(&path, text).map_err(|error| AppError::internal(format!("写入设置失败：{error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let settings = Settings {
            repos: vec!["/tmp/example".into()],
            last_worktree_id: Some("abc".into()),
            diff_font_size: 13,
        };
        write_settings(dir.path(), &settings).unwrap();
        let read = read_settings(dir.path()).unwrap();
        assert_eq!(read.repos, settings.repos);
        assert_eq!(read.last_worktree_id.as_deref(), Some("abc"));
        assert_eq!(read.diff_font_size, 13);
    }

    #[test]
    fn missing_settings_use_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let registry = Registry::load(dir.path().to_path_buf(), Arc::new(CommandLog::default()));
        assert!(registry.list().is_empty());
        assert_eq!(registry.settings().diff_font_size, 12);
    }

    #[test]
    fn font_size_is_clamped() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = Registry::load(dir.path().to_path_buf(), Arc::new(CommandLog::default()));
        assert_eq!(registry.update_font_size(100).unwrap().diff_font_size, 24);
        assert_eq!(registry.update_font_size(1).unwrap().diff_font_size, 9);
    }
}
