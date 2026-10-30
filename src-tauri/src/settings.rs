//! 设置与仓库登记：保存在 Revia 数据目录（Q49：`io.github.garlandqian.revia`）。
//!
//! 只写自己的数据目录，不改动用户的 Git 配置，也不往仓库目录里写东西。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, ErrorKind, Result};
use crate::git::CommandLog;
use crate::session::{RepoEntry, id_for_path};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// 登记过的仓库根路径（worktree 的路径也从这里推导）。
    pub repos: Vec<String>,
    /// 上次打开的 worktree ID。
    pub last_worktree_id: Option<String>,
    /// diff / 代码字号（⌘+ / ⌘-，Q39、Q133）。
    pub diff_font_size: u32,
    /// 浅色颜色主题（Q132）。
    pub theme_light: String,
    /// 深色颜色主题（Q132）。
    pub theme_dark: String,
    /// 代码字体名称；None 表示使用随应用打包的 JetBrains Mono（Q133）。
    pub code_font_family: Option<String>,
    /// 手动指定的 Git 程序路径；None 表示使用登录 shell `PATH` 中的 git（Q27）。
    pub git_path: Option<String>,
    /// 外部合并工具命令（M6；只传给 Git，不改用户的 Git 配置，Q28）。
    pub merge_tool: Option<String>,
    /// 外部 diff 工具（M6 T）。
    pub diff_tool: Option<String>,
    /// 界面语言；None 表示首次按系统语言，之后保存为 `zh-CN` / `en`（Q164）。
    pub language: Option<String>,
    /// cherry-pick 空提交是否创建空提交；默认跳过（M6 H02）。
    pub cherry_pick_keep_empty: bool,
    /// cherry-pick 已发布提交是否加来源后缀；默认开（M6 H02）。
    pub cherry_pick_include_source: bool,
    /// 提交工具窗口宽度（像素，Q139：可拖动分隔条）。
    pub sidebar_width: u32,
    /// 并排 / 统一视图（Q17：选择会被记住；返修 C12：不再用 localStorage）。
    pub view_mode: ViewModeSetting,
    /// 界面缩放百分比（Q139，80–150，100 表示不缩放）。
    pub ui_zoom: u32,
    /// 改动树是否按目录分组（Q139；false 是平铺）。
    pub tree_group_by_directory: bool,
    /// diff 上下文行数（Q139，Git `-U<n>`）。
    pub diff_context_lines: u32,
    /// 是否折叠未改动部分（Q139）。
    pub diff_fold_unchanged: bool,
    /// 同步滚动（Q139）。
    pub diff_sync_scroll: bool,
    /// 差异高亮方式（Q139，默认按词）。
    pub diff_highlight: DiffHighlightSetting,
    /// 显示空白字符（Q139）。
    pub diff_show_whitespace: bool,
    /// 软换行（Q139）。
    pub diff_soft_wrap: bool,
    /// 最后一处改动后跳到下一个文件（Q139）。
    pub diff_jump_to_next_file: bool,
    /// 忽略空白档位（Q140：0 关闭，1 行尾，2 全部，3 全部加空行；空行档位只读）。
    pub diff_ignore_whitespace: u32,
    /// 提交信息检查：首行长度上限（0 表示不检查）。
    pub commit_subject_limit: u32,
    /// 提交信息检查：首行与正文之间要求空行。
    pub commit_blank_line_required: bool,
    /// 提交信息检查：正文每行长度上限（0 表示不检查）。
    pub commit_body_line_limit: u32,
    /// 提交信息检查：显示右边界线。
    pub commit_show_ruler: bool,
    /// 提交时警告：CRLF 换行。
    pub commit_warn_crlf: bool,
    /// 提交时警告：游离 HEAD 或 rebase 进行中。
    pub commit_warn_detached: bool,
    /// 提交时警告：大文件阈值（字节，0 表示不警告）。
    pub commit_warn_large_file_bytes: u64,
    /// 提交时警告：在其他系统上不合法的文件名。
    pub commit_warn_invalid_filename: bool,
    /// 各类成功通知是否弹出（E.6；失败与结果待核对始终弹出）。
    pub notify: NotifyKinds,
    /// 合并时自动应用无冲突改动（Q143；关闭后初始结果用共同祖先）。
    pub merge_auto_apply: bool,
    /// 日志表格显示哪些列（F06）：作者、日期、哈希（默认隐藏）。
    pub log_columns: LogColumns,
    /// 文件历史是否显示每次提交对应的文件名/路径（F01，默认关，与 Rebased 一致）。
    pub file_history_show_names: bool,
    /// stash 差异与本地文件比较；本机 Rebased 当前参照默认选中。
    pub stash_compare_local: bool,
    /// 合并线拉直（Rebased 的 Long Edges，默认关）：关闭时跨越很多行的边折成短箭头。
    pub log_long_edges: bool,
    /// 只看第一父提交（默认关）。
    pub log_first_parent: bool,
    /// diff 预览位置（默认右侧）。
    pub log_preview_position: LogPreviewPosition,
    /// 更新方式（默认 merge；Q149）。
    pub update_mode: UpdateMode,
    /// fetch 的标签方式（默认自动，跟随 Git 配置）。
    pub fetch_tag_mode: FetchTagMode,
    /// 自动 fetch（默认关，Q152）。
    pub fetch_automatically: bool,
    /// 检查远端是否有新提交（默认关，只比较引用）。
    pub check_remote: bool,
    /// 推送被拒时自动更新（默认关）。
    pub auto_update_on_push_rejection: bool,
    /// 提交并推送时显示推送对话框（默认开）。
    pub push_dialog_on_commit: bool,
    /// 推送对话框只在受保护分支显示（默认关）。
    pub push_dialog_protected_only: bool,
    /// 受保护分支（每行一个，支持 `*` 通配；默认 master、main）。
    pub protected_branches: Vec<String>,
    /// 应用更新方式（M7、Q199）：手动检查 / 提示。F22：自动下载未实现，已移除。
    pub app_update_mode: AppUpdateMode,
    /// F22：用户主动开启预发布渠道（默认稳定版）。
    #[serde(default)]
    pub allow_prerelease: bool,
    /// 用户自定义快捷键（动作 id → 加速键）；未列出的动作用默认快捷键（Q199）。
    pub shortcuts: BTreeMap<String, String>,
}

/// 应用更新方式（M7 任务 10）。与拉取时的 `update_mode`（merge / rebase）不是同一件事。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AppUpdateMode {
    /// 只在用户主动检查时查询。
    Manual,
    /// 定期检查并在有新版本时提示（默认）。
    #[default]
    Notify,
    /// 自动下载新版本，但安装与重启仍由用户确认。
    Automatic,
}

/// `f1`–`f24` 形式的按键名。
fn is_function_key(value: &str) -> bool {
    value
        .strip_prefix('f')
        .and_then(|digits| digits.parse::<u8>().ok())
        .map(|number| (1..=24).contains(&number))
        .unwrap_or(false)
}

/// 自定义快捷键的合法字符：修饰键名、字母、数字与常见符号，用 `+` 连接。
fn normalize_shortcut(value: &str) -> Option<String> {
    let trimmed = value.trim();
    // 空覆盖值表示该动作未分配；缺少覆盖项才恢复默认快捷键。
    if trimmed.is_empty() {
        return Some(String::new());
    }
    if trimmed.len() > 64 {
        return None;
    }
    let mut normalized = String::new();
    for (index, part) in trimmed.split('+').enumerate() {
        let part = part.trim();
        if part.is_empty() {
            return None;
        }
        if index > 0 {
            normalized.push('+');
        }
        let lowered = part.to_ascii_lowercase();
        if matches!(lowered.as_str(), "mod" | "cmd" | "ctrl" | "alt" | "shift" | "meta") {
            normalized.push_str(&lowered);
            continue;
        }
        // 按键部分：单个字母 / 数字 / 标点，或约定的按键名（避免 `+` 与分隔符混淆）。
        const NAMED_KEYS: [&str; 26] = [
            "plus", "minus", "equal", "comma", "period", "slash", "semicolon", "quote",
            "backquote", "bracketleft", "bracketright", "backslash", "space", "enter",
            "tab", "escape", "up", "down", "left", "right", "home", "end", "pageup",
            "pagedown", "delete", "backspace",
        ];
        let single = part.chars().count() == 1
            && (part.chars().next().unwrap().is_alphanumeric()
                || "`-=[]\\;',./".contains(part.chars().next().unwrap()));
        let named = NAMED_KEYS.contains(&lowered.as_str()) || is_function_key(&lowered);
        if !single && !named {
            return None;
        }
        normalized.push_str(&lowered);
    }
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

/// 日志表格的显示列（F06）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LogColumns {
    pub author: bool,
    pub date: bool,
    /// 哈希列默认隐藏（L02）。
    pub hash: bool,
}

impl Default for LogColumns {
    fn default() -> Self {
        Self {
            author: true,
            date: true,
            hash: false,
        }
    }
}

/// diff 预览位置（F06）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LogPreviewPosition {
    Right,
    Bottom,
}

impl Default for LogPreviewPosition {
    fn default() -> Self {
        Self::Right
    }
}

/// 更新方式（B04）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateMode {
    Merge,
    Rebase,
    /// 按分支默认：只读读取该分支与仓库的 rebase 相关配置决定（不改配置）。
    BranchDefault,
}

impl Default for UpdateMode {
    fn default() -> Self {
        Self::Merge
    }
}

/// fetch 的标签方式（B03）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FetchTagMode {
    /// 跟随 Git 配置，不加参数。
    Auto,
    /// 同步删除远端已删除的标签和分支。
    Sync,
    /// 取回全部标签。
    Always,
    /// 不取标签。
    Never,
}

impl Default for FetchTagMode {
    fn default() -> Self {
        Self::Auto
    }
}

/// 默认受保护分支（B05、B08）。
pub const DEFAULT_PROTECTED_BRANCHES: &[&str] = &["master", "main"];

/// 分支名是否匹配受保护分支的某个模式（`*` 通配，`?` 单字符）。
pub fn matches_protected(name: &str, patterns: &[String]) -> bool {
    patterns
        .iter()
        .any(|pattern| glob_matches(pattern.trim(), name))
}

/// 极简 glob：只支持 `*` 与 `?`，其余字符按字面比较（分支名里没有转义需求）。
fn glob_matches(pattern: &str, text: &str) -> bool {
    if pattern.is_empty() {
        return false;
    }
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut p, mut t) = (0usize, 0usize);
    let (mut star, mut resume) = (None::<usize>, 0usize);
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            resume = t;
            p += 1;
        } else if let Some(position) = star {
            p = position + 1;
            resume += 1;
            t = resume;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}

/// 各类成功结果是否弹出通知（E.6）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NotifyKinds {
    /// 暂存、取消暂存。
    pub stage: bool,
    /// 丢弃。
    pub discard: bool,
    /// 提交。
    pub commit: bool,
    /// 回收区：恢复、另存、清理。
    pub recovery: bool,
}

impl Default for NotifyKinds {
    fn default() -> Self {
        Self {
            stage: true,
            discard: true,
            commit: true,
            recovery: true,
        }
    }
}

/// 并排 / 统一视图（Q17）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ViewModeSetting {
    Split,
    Unified,
}

impl Default for ViewModeSetting {
    fn default() -> Self {
        Self::Split
    }
}

/// 差异高亮方式（Q139）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiffHighlightSetting {
    Word,
    Line,
    Char,
    None,
}

impl Default for DiffHighlightSetting {
    fn default() -> Self {
        Self::Word
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            repos: Vec::new(),
            last_worktree_id: None,
            diff_font_size: 12,
            theme_light: DEFAULT_LIGHT_THEME.into(),
            theme_dark: DEFAULT_DARK_THEME.into(),
            code_font_family: None,
            git_path: None,
            merge_tool: None,
            diff_tool: None,
            language: None,
            cherry_pick_keep_empty: false,
            cherry_pick_include_source: true,
            sidebar_width: 330,
            view_mode: ViewModeSetting::Split,
            ui_zoom: 100,
            tree_group_by_directory: false,
            diff_context_lines: 3,
            diff_fold_unchanged: false,
            diff_sync_scroll: true,
            diff_highlight: DiffHighlightSetting::Word,
            diff_show_whitespace: false,
            diff_soft_wrap: false,
            diff_jump_to_next_file: true,
            diff_ignore_whitespace: 0,
            commit_subject_limit: 72,
            commit_blank_line_required: false,
            commit_body_line_limit: 0,
            commit_show_ruler: false,
            commit_warn_crlf: true,
            commit_warn_detached: true,
            commit_warn_large_file_bytes: 50 * 1024 * 1024,
            commit_warn_invalid_filename: true,
            notify: NotifyKinds::default(),
            merge_auto_apply: true,
            log_columns: LogColumns::default(),
            file_history_show_names: false,
            stash_compare_local: true,
            log_long_edges: false,
            log_first_parent: false,
            log_preview_position: LogPreviewPosition::default(),
            update_mode: UpdateMode::default(),
            fetch_tag_mode: FetchTagMode::default(),
            fetch_automatically: false,
            check_remote: false,
            auto_update_on_push_rejection: false,
            push_dialog_on_commit: true,
            push_dialog_protected_only: false,
            protected_branches: DEFAULT_PROTECTED_BRANCHES
                .iter()
                .map(|name| name.to_string())
                .collect(),
            app_update_mode: AppUpdateMode::default(),
            allow_prerelease: false,
            shortcuts: BTreeMap::new(),
        }
    }
}

/// 随应用打包的四套颜色主题（Q132）。
pub const LIGHT_THEMES: &[&str] = &["light-2026", "one-light"];
pub const DARK_THEMES: &[&str] = &["dark-2026", "one-dark-pro"];
pub const DEFAULT_LIGHT_THEME: &str = "one-light";
pub const DEFAULT_DARK_THEME: &str = "one-dark-pro";

/// 设置修改请求：只传要改的字段，其余保持不变。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsPatch {
    pub theme_light: Option<String>,
    pub theme_dark: Option<String>,
    pub code_font_family: Option<String>,
    pub diff_font_size: Option<u32>,
    pub git_path: Option<String>,
    pub merge_tool: Option<String>,
    pub diff_tool: Option<String>,
    pub language: Option<String>,
    pub cherry_pick_keep_empty: Option<bool>,
    pub cherry_pick_include_source: Option<bool>,
    pub sidebar_width: Option<u32>,
    pub view_mode: Option<ViewModeSetting>,
    pub ui_zoom: Option<u32>,
    pub tree_group_by_directory: Option<bool>,
    pub diff_context_lines: Option<u32>,
    pub diff_fold_unchanged: Option<bool>,
    pub diff_sync_scroll: Option<bool>,
    pub diff_highlight: Option<DiffHighlightSetting>,
    pub diff_show_whitespace: Option<bool>,
    pub diff_soft_wrap: Option<bool>,
    pub diff_jump_to_next_file: Option<bool>,
    pub diff_ignore_whitespace: Option<u32>,
    pub commit_subject_limit: Option<u32>,
    pub commit_blank_line_required: Option<bool>,
    pub commit_body_line_limit: Option<u32>,
    pub commit_show_ruler: Option<bool>,
    pub commit_warn_crlf: Option<bool>,
    pub commit_warn_detached: Option<bool>,
    pub commit_warn_large_file_bytes: Option<u64>,
    pub commit_warn_invalid_filename: Option<bool>,
    pub notify: Option<NotifyKinds>,
    pub merge_auto_apply: Option<bool>,
    pub log_columns: Option<LogColumns>,
    pub file_history_show_names: Option<bool>,
    pub stash_compare_local: Option<bool>,
    pub log_long_edges: Option<bool>,
    pub log_first_parent: Option<bool>,
    pub log_preview_position: Option<LogPreviewPosition>,
    pub update_mode: Option<UpdateMode>,
    pub fetch_tag_mode: Option<FetchTagMode>,
    pub fetch_automatically: Option<bool>,
    pub check_remote: Option<bool>,
    pub auto_update_on_push_rejection: Option<bool>,
    pub push_dialog_on_commit: Option<bool>,
    pub push_dialog_protected_only: Option<bool>,
    pub protected_branches: Option<Vec<String>>,
    pub app_update_mode: Option<AppUpdateMode>,
    /// F22：预发布渠道开关。
    #[serde(default)]
    pub allow_prerelease: Option<bool>,
    pub shortcuts: Option<BTreeMap<String, String>>,
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
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.settings.directoryDoesExist",
                "目录不存在：{path}",
                &[("path", format!("{}", path))],
            ));
        }
        let entry = RepoEntry::build(&candidate, self.command_log.clone())?;
        let canonical = std::fs::canonicalize(Path::new(&entry.path))
            .unwrap_or_else(|_| PathBuf::from(&entry.path));

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
        let entry = self.find(repo_id).ok_or_else(|| {
            AppError::coded(
                ErrorKind::Rejected,
                "errors.settings.theRepositoryRegisteredAddFirst",
                "仓库未登记，请先添加",
                &[],
            )
        })?;
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
                consider(
                    Path::new(&worktree.path),
                    id_for_path(Path::new(&worktree.path)),
                );
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

    /// 应用一次设置修改；取值不合法时拒绝并保留原设置。
    pub fn update_settings(&mut self, patch: &SettingsPatch) -> Result<Settings> {
        let mut next = self.settings.clone();
        if let Some(theme) = &patch.theme_light {
            if !self.theme_allowed(theme, true) {
                return Err(AppError::coded(
                    ErrorKind::Rejected,
                    "errors.settings.unknownLightColorTheme",
                    "未知的浅色颜色主题：{theme}",
                    &[("theme", format!("{}", theme))],
                ));
            }
            next.theme_light = theme.clone();
        }
        if let Some(theme) = &patch.theme_dark {
            if !self.theme_allowed(theme, false) {
                return Err(AppError::coded(
                    ErrorKind::Rejected,
                    "errors.settings.unknownDarkColorTheme",
                    "未知的深色颜色主题：{theme}",
                    &[("theme", format!("{}", theme))],
                ));
            }
            next.theme_dark = theme.clone();
        }
        if let Some(font) = &patch.code_font_family {
            let trimmed = font.trim();
            next.code_font_family = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        if let Some(size) = patch.diff_font_size {
            next.diff_font_size = size.clamp(9, 24);
        }
        if let Some(path) = &patch.git_path {
            let trimmed = path.trim();
            if trimmed.is_empty() {
                next.git_path = None;
            } else {
                let candidate = Path::new(trimmed);
                if !candidate.is_file() {
                    return Err(AppError::coded(
                        ErrorKind::Rejected,
                        "errors.settings.theGitProgramPathExecutableFile",
                        "Git 程序路径不是可执行文件：{trimmed}",
                        &[("trimmed", format!("{}", trimmed))],
                    ));
                }
                next.git_path = Some(trimmed.to_string());
            }
        }
        if let Some(tool) = &patch.merge_tool {
            let trimmed = tool.trim();
            next.merge_tool = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        if let Some(tool) = &patch.diff_tool {
            let trimmed = tool.trim();
            next.diff_tool = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        if let Some(language) = &patch.language {
            let trimmed = language.trim();
            if trimmed.is_empty() {
                next.language = None;
            } else if matches!(trimmed, "zh-CN" | "en") {
                next.language = Some(trimmed.to_string());
            } else {
                return Err(AppError::coded(
                    ErrorKind::Rejected,
                    "errors.settings.unknownLanguage",
                    "未知的界面语言：{language}",
                    &[("language", trimmed.to_string())],
                ));
            }
        }
        if let Some(value) = patch.cherry_pick_keep_empty {
            next.cherry_pick_keep_empty = value;
        }
        if let Some(value) = patch.cherry_pick_include_source {
            next.cherry_pick_include_source = value;
        }
        if let Some(width) = patch.sidebar_width {
            next.sidebar_width = width.clamp(220, 720);
        }
        if let Some(mode) = patch.view_mode {
            next.view_mode = mode;
        }
        if let Some(zoom) = patch.ui_zoom {
            next.ui_zoom = zoom.clamp(80, 150);
        }
        if let Some(value) = patch.tree_group_by_directory {
            next.tree_group_by_directory = value;
        }
        if let Some(value) = patch.diff_context_lines {
            next.diff_context_lines = value.clamp(0, 64);
        }
        if let Some(value) = patch.diff_fold_unchanged {
            next.diff_fold_unchanged = value;
        }
        if let Some(value) = patch.diff_sync_scroll {
            next.diff_sync_scroll = value;
        }
        if let Some(value) = patch.diff_highlight {
            next.diff_highlight = value;
        }
        if let Some(value) = patch.diff_show_whitespace {
            next.diff_show_whitespace = value;
        }
        if let Some(value) = patch.diff_soft_wrap {
            next.diff_soft_wrap = value;
        }
        if let Some(value) = patch.diff_jump_to_next_file {
            next.diff_jump_to_next_file = value;
        }
        if let Some(value) = patch.diff_ignore_whitespace {
            next.diff_ignore_whitespace = value.clamp(0, 3);
        }
        if let Some(value) = patch.commit_subject_limit {
            next.commit_subject_limit = value.clamp(0, 200);
        }
        if let Some(value) = patch.commit_blank_line_required {
            next.commit_blank_line_required = value;
        }
        if let Some(value) = patch.commit_body_line_limit {
            next.commit_body_line_limit = value.clamp(0, 200);
        }
        if let Some(value) = patch.commit_show_ruler {
            next.commit_show_ruler = value;
        }
        if let Some(value) = patch.commit_warn_crlf {
            next.commit_warn_crlf = value;
        }
        if let Some(value) = patch.commit_warn_detached {
            next.commit_warn_detached = value;
        }
        if let Some(value) = patch.commit_warn_large_file_bytes {
            next.commit_warn_large_file_bytes = value;
        }
        if let Some(value) = patch.commit_warn_invalid_filename {
            next.commit_warn_invalid_filename = value;
        }
        if let Some(value) = patch.notify {
            next.notify = value;
        }
        if let Some(value) = patch.merge_auto_apply {
            next.merge_auto_apply = value;
        }
        if let Some(value) = patch.log_columns {
            next.log_columns = value;
        }
        if let Some(value) = patch.stash_compare_local { next.stash_compare_local = value; }
        if let Some(value) = patch.file_history_show_names {
            next.file_history_show_names = value;
        }
        if let Some(value) = patch.log_long_edges {
            next.log_long_edges = value;
        }
        if let Some(value) = patch.log_first_parent {
            next.log_first_parent = value;
        }
        if let Some(value) = patch.log_preview_position {
            next.log_preview_position = value;
        }
        if let Some(value) = patch.update_mode {
            next.update_mode = value;
        }
        if let Some(value) = patch.fetch_tag_mode {
            next.fetch_tag_mode = value;
        }
        if let Some(value) = patch.fetch_automatically {
            next.fetch_automatically = value;
        }
        if let Some(value) = patch.check_remote {
            next.check_remote = value;
        }
        if let Some(value) = patch.auto_update_on_push_rejection {
            next.auto_update_on_push_rejection = value;
        }
        if let Some(value) = patch.push_dialog_on_commit {
            next.push_dialog_on_commit = value;
        }
        if let Some(value) = patch.push_dialog_protected_only {
            next.push_dialog_protected_only = value;
        }
        if let Some(value) = &patch.protected_branches {
            next.protected_branches = normalize_patterns(value)?;
        }
        if let Some(value) = patch.app_update_mode {
            next.app_update_mode = value;
        }
        if let Some(value) = patch.allow_prerelease {
            next.allow_prerelease = value;
        }
        if let Some(value) = &patch.shortcuts {
            let mut shortcuts = BTreeMap::new();
            for (action, accelerator) in value {
                let action = action.trim();
                if action.is_empty() || action.len() > 64 {
                    return Err(AppError::coded(
                        ErrorKind::Rejected,
                        "errors.settings.unknownShortcutAction",
                        "未知的快捷键动作：{action}",
                        &[("action", action.to_string())],
                    ));
                }
                let Some(normalized) = normalize_shortcut(accelerator) else {
                    return Err(AppError::coded(
                        ErrorKind::Rejected,
                        "errors.settings.shortcutInvalid",
                        "快捷键格式不正确：{shortcut}",
                        &[("shortcut", accelerator.clone())],
                    ));
                };
                shortcuts.insert(action.to_string(), normalized);
            }
            if shortcuts.len() > 64 {
                return Err(AppError::coded(
                    ErrorKind::Rejected,
                    "errors.settings.tooManyShortcuts",
                    "自定义快捷键数量过多",
                    &[],
                ));
            }
            next.shortcuts = shortcuts;
        }
        self.settings = next;
        self.persist()?;
        Ok(self.settings.clone())
    }

    /// 颜色主题可以是内置的，也可以是已导入且深浅色匹配的主题（M7 任务 02）。
    fn theme_allowed(&self, id: &str, light: bool) -> bool {
        if (if light { LIGHT_THEMES } else { DARK_THEMES }).contains(&id) {
            return true;
        }
        if !id.starts_with("imported-") {
            return false;
        }
        crate::m7::theme::ThemeStore::new(&self.directory)
            .list()
            .map(|themes| {
                themes.iter().any(|theme| {
                    theme.id == id
                        && (theme.kind == crate::m7::theme::ThemeKind::Light) == light
                })
            })
            .unwrap_or(false)
    }

    fn persist(&self) -> Result<()> {
        write_settings(&self.directory, &self.settings)
    }
}

/// 受保护分支模式：去空白、去空行、拒绝含空白的模式（分支名里没有空格）。
fn normalize_patterns(values: &[String]) -> Result<Vec<String>> {
    let mut next = Vec::new();
    for value in values {
        let pattern = value.trim();
        if pattern.is_empty() {
            continue;
        }
        if pattern.chars().any(char::is_whitespace) {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.settings.protectedBranchPatternsContainWhitespace",
                "受保护分支模式不能包含空白：{pattern}",
                &[("pattern", format!("{}", pattern))],
            ));
        }
        if !next.iter().any(|existing: &String| existing == pattern) {
            next.push(pattern.to_string());
        }
    }
    Ok(next)
}

pub fn settings_path(directory: &Path) -> PathBuf {
    directory.join("settings.json")
}

pub fn read_settings(directory: &Path) -> Result<Settings> {
    let path = settings_path(directory);
    let text = std::fs::read_to_string(&path).map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.settings.failedReadSettingsCause",
            "读取设置失败：{error}",
            &[("error", error.to_string())],
        )
    })?;
    serde_json::from_str(&text).map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.settings.failedParseSettingsCause",
            "解析设置失败：{error}",
            &[("error", error.to_string())],
        )
    })
}

fn write_settings(directory: &Path, settings: &Settings) -> Result<()> {
    std::fs::create_dir_all(directory).map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.settings.failedCreateDataDirectoryCause",
            "创建数据目录失败：{error}",
            &[("error", error.to_string())],
        )
    })?;
    let path = settings_path(directory);
    let text = serde_json::to_string_pretty(settings)?;
    std::fs::write(&path, text).map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.settings.failedWriteSettingsCause",
            "写入设置失败：{error}",
            &[("error", error.to_string())],
        )
    })
}

/// Platform-specific first-run language selection stays in the platform shell.
pub fn system_language() -> String {
    crate::platform::system_language()
}

pub use crate::platform::language_from_locale;

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
            theme_light: "light-2026".into(),
            theme_dark: "dark-2026".into(),
            code_font_family: Some("Fira Code".into()),
            git_path: None,
            merge_tool: Some("code".into()),
            sidebar_width: 420,
            tree_group_by_directory: true,
            diff_context_lines: 5,
            diff_ignore_whitespace: 2,
            commit_subject_limit: 60,
            commit_warn_large_file_bytes: 1024,
            notify: NotifyKinds {
                discard: false,
                ..NotifyKinds::default()
            },
            ..Settings::default()
        };
        write_settings(dir.path(), &settings).unwrap();
        let read = read_settings(dir.path()).unwrap();
        assert_eq!(read.repos, settings.repos);
        assert_eq!(read.last_worktree_id.as_deref(), Some("abc"));
        assert_eq!(read.diff_font_size, 13);
        assert_eq!(read.theme_light, "light-2026");
        assert_eq!(read.theme_dark, "dark-2026");
        assert_eq!(read.code_font_family.as_deref(), Some("Fira Code"));
        assert_eq!(read.merge_tool.as_deref(), Some("code"));
        assert_eq!(read.sidebar_width, 420);
        assert!(read.tree_group_by_directory);
        assert_eq!(read.diff_context_lines, 5);
        assert_eq!(read.diff_ignore_whitespace, 2);
        assert_eq!(read.commit_subject_limit, 60);
        assert_eq!(read.commit_warn_large_file_bytes, 1024);
        assert!(!read.notify.discard && read.notify.stage);
    }

    #[test]
    fn old_settings_files_get_theme_defaults() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            settings_path(dir.path()),
            r#"{"repos":[],"diffFontSize":11}"#,
        )
        .unwrap();
        let read = read_settings(dir.path()).unwrap();
        assert_eq!(read.diff_font_size, 11);
        assert_eq!(read.theme_light, DEFAULT_LIGHT_THEME);
        assert_eq!(read.theme_dark, DEFAULT_DARK_THEME);
        assert!(read.stash_compare_local);
    }

    #[test]
    fn unassigned_shortcut_override_survives_settings_save_and_reload() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = Registry::load(dir.path().to_path_buf(), Arc::new(CommandLog::default()));
        let updated = registry.update_settings(&SettingsPatch {
            shortcuts: Some(BTreeMap::from([("commit".into(), String::new()), ("cloneRepo".into(), "mod+k".into())])),
            ..SettingsPatch::default()
        }).unwrap();
        assert_eq!(updated.shortcuts.get("commit").map(String::as_str), Some(""));
        let reloaded = Registry::load(dir.path().to_path_buf(), Arc::new(CommandLog::default()));
        assert_eq!(reloaded.settings().shortcuts, updated.shortcuts);
        assert_eq!(read_settings(dir.path()).unwrap().shortcuts, updated.shortcuts);
        registry.update_settings(&SettingsPatch {
            shortcuts: Some(BTreeMap::new()), ..SettingsPatch::default()
        }).unwrap();
        assert!(read_settings(dir.path()).unwrap().shortcuts.is_empty());
    }

    #[test]
    fn update_settings_validates_theme_and_clamps_font() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry =
            Registry::load(dir.path().to_path_buf(), Arc::new(CommandLog::default()));
        let updated = registry
            .update_settings(&SettingsPatch {
                theme_light: Some("light-2026".into()),
                theme_dark: Some("one-dark-pro".into()),
                diff_font_size: Some(100),
                stash_compare_local: Some(false),
                ..SettingsPatch::default()
            })
            .unwrap();
        assert_eq!(updated.theme_light, "light-2026");
        assert_eq!(updated.diff_font_size, 24);
        assert!(!updated.stash_compare_local);
        assert!(!read_settings(dir.path()).unwrap().stash_compare_local);
        let rejected = registry.update_settings(&SettingsPatch {
            theme_light: Some("one-dark-pro".into()),
            ..SettingsPatch::default()
        });
        assert!(rejected.is_err());
        assert_eq!(registry.settings().theme_light, "light-2026");
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
        let mut registry =
            Registry::load(dir.path().to_path_buf(), Arc::new(CommandLog::default()));
        assert_eq!(registry.update_font_size(100).unwrap().diff_font_size, 24);
        assert_eq!(registry.update_font_size(1).unwrap().diff_font_size, 9);
    }
}
