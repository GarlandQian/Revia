//! 提交历史的只读 Git 读取（M5 任务 01，规格 H01–H03）。
//!
//! 边界：
//! - 只按完整对象 ID 读取；起点解析、父提交、路径与对象来自 Git 本身；
//! - 所有 diff 固定带 `--no-ext-diff --no-textconv`，不运行外部 diff/textconv；
//! - 路径参数一律放在 `--` 之后并带 `--literal-pathspecs`，不解释通配符；
//! - 路径身份来自 `-z` 机器格式的原始字节，不从显示用 patch 的标题反推；
//! - 不写暂存区、不签出、不隐式 fetch 缺失对象：读不到就报错。
//!
//! 与工作区读取（`status.rs` / `diff.rs`）分开：这里的路径与对象都绑定到
//! 具体提交，不承载“未暂存 / 已暂存 / 未跟踪”的工作区语义。

use std::collections::HashMap;

use crate::error::{AppError, Result};
use crate::git::segments::IgnoreWhitespace;
use crate::git::{Git, DIFF_FLAGS};

/// 单页最多返回多少条提交（工程限额，写入报告）。
pub const MAX_PAGE_LIMIT: u32 = 200;
/// 一次查询最多向前推进多少条提交；超过要重新从更近的起点查询，不伪装成历史末尾。
pub const MAX_QUERY_SPAN: u32 = 20_000;
/// 单个文件 diff 的原始输出上限。
pub const MAX_DIFF_BYTES: usize = 16 * 1024 * 1024;
/// 单个 blob 读取上限（左右对照与高亮需要整份内容）。
pub const MAX_BLOB_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub name: String,
    pub email: String,
    /// Unix 秒。
    pub time: i64,
    /// 形如 `+0800` 的时区。
    pub timezone: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitEntry {
    pub oid: String,
    pub parents: Vec<String>,
    pub author: Signature,
    pub committer: Signature,
    pub subject: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitDetail {
    pub oid: String,
    pub tree: String,
    pub parents: Vec<String>,
    pub author: Signature,
    pub committer: Signature,
    /// 提交声明的 `encoding` 头（没有则为 None）。
    pub encoding: Option<String>,
    /// 正文是否发生过有损解码（非 UTF-8 且无法按声明的编码转换）。
    pub message_lossy: bool,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeStatus {
    Added,
    Deleted,
    Modified,
    Renamed,
    Copied,
    TypeChanged,
}

impl ChangeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Deleted => "deleted",
            Self::Modified => "modified",
            Self::Renamed => "renamed",
            Self::Copied => "copied",
            Self::TypeChanged => "typeChanged",
        }
    }

    fn from_letter(letter: u8) -> Option<Self> {
        match letter {
            b'A' => Some(Self::Added),
            b'D' => Some(Self::Deleted),
            b'M' => Some(Self::Modified),
            b'R' => Some(Self::Renamed),
            b'C' => Some(Self::Copied),
            b'T' => Some(Self::TypeChanged),
            _ => None,
        }
    }
}

/// 一对父/子树之间的一个改动文件。路径保留原始字节，由上层决定能否无损展示。
#[derive(Debug, Clone)]
pub struct RawChange {
    pub status: ChangeStatus,
    pub old_path: Option<Vec<u8>>,
    pub new_path: Option<Vec<u8>>,
    pub old_mode: Option<String>,
    pub new_mode: Option<String>,
    pub old_oid: Option<String>,
    pub new_oid: Option<String>,
    pub added: u32,
    pub deleted: u32,
    pub binary: bool,
}

impl RawChange {
    /// 展示与请求都用这一侧作为主路径：删除取旧路径，其余取新路径。
    pub fn primary_path(&self) -> &[u8] {
        match self.status {
            ChangeStatus::Deleted => self.old_path.as_deref().unwrap_or_default(),
            _ => self.new_path.as_deref().or(self.old_path.as_deref()).unwrap_or_default(),
        }
    }

    pub fn original_path(&self) -> Option<&[u8]> {
        match self.status {
            ChangeStatus::Renamed | ChangeStatus::Copied => self.old_path.as_deref(),
            _ => None,
        }
    }

    pub fn is_symlink(&self) -> bool {
        matches!(self.old_mode.as_deref(), Some("120000")) || matches!(self.new_mode.as_deref(), Some("120000"))
    }

    pub fn is_submodule(&self) -> bool {
        matches!(self.old_mode.as_deref(), Some("160000")) || matches!(self.new_mode.as_deref(), Some("160000"))
    }

    pub fn mode_only(&self) -> bool {
        self.old_oid.is_some()
            && self.old_oid == self.new_oid
            && self.old_mode != self.new_mode
            && self.added == 0
            && self.deleted == 0
    }
}

/// 完整长度的十六进制对象 ID（SHA-1 40 位或 SHA-256 64 位）。
pub fn is_full_oid(text: &str) -> bool {
    (text.len() == 40 || text.len() == 64) && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_zero_oid(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte == b'0')
}

fn oid_option(text: &str) -> Option<String> {
    if is_zero_oid(text) {
        None
    } else if is_full_oid(text) {
        Some(text.to_ascii_lowercase())
    } else {
        None
    }
}

fn mode_option(text: &str) -> Option<String> {
    if text.is_empty() || text == "000000" {
        None
    } else {
        Some(text.to_string())
    }
}

/// 读取本地引用（含 peeled 提交）。返回 `(full_name, kind, commit)`。
pub fn list_refs(git: &Git) -> Result<Vec<(String, String, String)>> {
    let output = git.run_ok(
        &[
            "--no-optional-locks",
            "for-each-ref",
            "--format=%(refname)%00%(objecttype)%00%(objectname)%00%(*objecttype)%00%(*objectname)%00",
        ],
        "读取本地引用",
    )?;
    // 引用名不能含 NUL 或换行，因此按行切记录、按 NUL 切字段是无损的。
    let text = String::from_utf8_lossy(&output.stdout);
    let mut refs = Vec::new();
    for line in text.split('\n') {
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\0').collect();
        if fields.len() < 5 {
            continue;
        }
        let full_name = fields[0];
        if full_name.is_empty() {
            continue;
        }
        let (object_type, object_name, peeled_type, peeled_name) =
            (fields[1], fields[2], fields[3], fields[4]);
        let commit = if object_type == "commit" && is_full_oid(object_name) {
            Some(object_name.to_string())
        } else if object_type == "tag" && peeled_type == "commit" && is_full_oid(peeled_name) {
            Some(peeled_name.to_string())
        } else {
            None
        };
        let Some(commit) = commit else {
            // 指向 tree / blob 的引用不参与提交遍历。
            continue;
        };
        let kind = if full_name.starts_with("refs/heads/") {
            "branch"
        } else if full_name.starts_with("refs/remotes/") {
            "remoteBranch"
        } else if full_name.starts_with("refs/tags/") {
            "tag"
        } else {
            "other"
        };
        refs.push((full_name.to_string(), kind.to_string(), commit));
    }
    Ok(refs)
}

/// 当前 HEAD 指向的完整提交 ID；还没有提交时为 None。
pub fn head_commit(git: &Git) -> Result<Option<String>> {
    let output = git.run(&["--no-optional-locks", "rev-parse", "--verify", "--quiet", "HEAD^{commit}"])?;
    if output.exit_code != 0 {
        return Ok(None);
    }
    let oid = output.stdout_lossy().trim().to_string();
    Ok(is_full_oid(&oid).then_some(oid))
}

/// HEAD 指向的引用名（分离 HEAD 时为 None）。
pub fn head_ref(git: &Git) -> Option<String> {
    let output = git.run(&["--no-optional-locks", "symbolic-ref", "--quiet", "HEAD"]).ok()?;
    if output.exit_code != 0 {
        return None;
    }
    let name = output.stdout_lossy().trim().to_string();
    (!name.is_empty()).then_some(name)
}

/// 按仓库实际的哈希算法得到空 tree 的对象 ID（不硬编码 SHA-1 常量）。
pub fn empty_tree(git: &Git) -> Result<String> {
    let output = git.run_with_input(&["hash-object", "-t", "tree", "--stdin"], Vec::new())?;
    let output = output.into_success("计算空 tree")?;
    let oid = output.stdout_lossy().trim().to_string();
    if !is_full_oid(&oid) {
        return Err(AppError::git(format!("Git 返回的空 tree 对象 ID 不合法：{oid}")));
    }
    Ok(oid)
}

/// 对象类型（commit / tree / blob / tag）；对象不存在时返回 None。
pub fn object_type(git: &Git, oid: &str) -> Result<Option<String>> {
    if !is_full_oid(oid) {
        return Err(AppError::rejected(format!("不是完整的对象 ID：{oid}")));
    }
    let output = git.run(&["--no-optional-locks", "cat-file", "-t", oid])?;
    if output.exit_code != 0 {
        return Ok(None);
    }
    Ok(Some(output.stdout_lossy().trim().to_string()))
}

/// 某个提交树里某路径的对象 ID；路径不存在时为 None。
///
/// 用 `<commit>:<path>` 形式，路径按字面解释（不做 glob、不读工作区）。
pub fn object_at_path(git: &Git, commit: &str, path: &str) -> Result<Option<String>> {
    if !is_full_oid(commit) {
        return Err(AppError::rejected("读取提交内容时必须给出完整提交 ID"));
    }
    let spec = format!("{commit}:{path}");
    let output = git.run(&["--no-optional-locks", "rev-parse", "--verify", "--quiet", &spec])?;
    if output.exit_code != 0 {
        return Ok(None);
    }
    let oid = output.stdout_lossy().trim().to_string();
    Ok(is_full_oid(&oid).then_some(oid))
}

/// 校验一个调用者提供的提交 ID 确实是提交对象，返回规范形式。
pub fn verify_commit_oid(git: &Git, oid: &str) -> Result<String> {
    let oid = oid.trim();
    if !is_full_oid(oid) {
        return Err(AppError::rejected(format!("起点必须是完整的提交对象 ID：{oid}")));
    }
    let oid = oid.to_ascii_lowercase();
    match object_type(git, &oid)?.as_deref() {
        Some("commit") => Ok(oid),
        Some(other) => Err(AppError::rejected(format!("对象不是提交（{other}）：{oid}"))),
        None => Err(AppError::rejected(format!("对象不存在：{oid}"))),
    }
}

const LOG_FORMAT: &str = "--format=%H%x00%P%x00%an%x00%ae%x00%at%x00%ai%x00%cn%x00%ce%x00%ct%x00%ci%x00%s";

/// 读取一页提交（从 `start` 可达，按拓扑顺序）。`skip` 由上层游标控制。
pub fn read_page(
    git: &Git,
    start: &str,
    first_parent: bool,
    skip: u32,
    limit: u32,
) -> Result<Vec<CommitEntry>> {
    read_page_from(git, &[start.to_string()], first_parent, skip, limit)
}

/// 从固定的起点集合读取一页提交（M5 第二批：分支筛选会给出多个固定起点）。
pub fn read_page_from(
    git: &Git,
    starts: &[String],
    first_parent: bool,
    skip: u32,
    limit: u32,
) -> Result<Vec<CommitEntry>> {
    if starts.is_empty() || starts.iter().any(|start| !is_full_oid(start)) {
        return Err(AppError::internal("分页起点不是完整提交 ID"));
    }
    let skip_arg = format!("--skip={skip}");
    let count_arg = format!("--max-count={limit}");
    let mut args: Vec<&str> = vec![
        "--no-optional-locks",
        "-c",
        "core.quotePath=false",
        "--no-pager",
        "log",
        "--no-color",
        "--topo-order",
        "-z",
        LOG_FORMAT,
    ];
    if first_parent {
        args.push("--first-parent");
    }
    args.push(&skip_arg);
    args.push(&count_arg);
    for start in starts {
        args.push(start);
    }
    let output = git.run_ok(&args, "读取提交历史")?;
    parse_log_page(&output.stdout)
}

/// 当前 HEAD 树里的全部文件路径（路径筛选的候选，按字面返回）。
///
/// 只读、有上限：文件树本身不写、不读工作区；超过上限时返回前 `MAX_TREE_FILES` 条。
pub const MAX_TREE_FILES: usize = 20_000;

pub fn head_files(git: &Git) -> Result<Vec<String>> {
    let output = git.run(&[
        "--no-optional-locks",
        "-c",
        "core.quotePath=false",
        "ls-tree",
        "-r",
        "-z",
        "--name-only",
        "HEAD",
    ])?;
    if output.exit_code != 0 {
        // 空仓库（还没有 HEAD）不是错误：没有可选路径。
        return Ok(Vec::new());
    }
    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|chunk| !chunk.is_empty())
        .take(MAX_TREE_FILES)
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect())
}

/// 某个本地分支配置的上游完整引用名（没有上游时为 None）。
pub fn upstream_of(git: &Git, branch: &str) -> Result<Option<String>> {
    if branch.trim().is_empty() || branch.contains("@{") {
        return Err(AppError::rejected("分支名不能为空，也不能包含 @{"));
    }
    let spec = format!("{branch}@{{upstream}}");
    let output = git.run(&[
        "--no-optional-locks",
        "rev-parse",
        "--symbolic-full-name",
        "--verify",
        "--quiet",
        &spec,
    ])?;
    if output.exit_code != 0 {
        return Ok(None);
    }
    let name = output.stdout_lossy().trim().to_string();
    Ok((!name.is_empty()).then_some(name))
}

/// 读取两个提交之间的提交（`exclude..include`，新到旧，带条数上限）。
pub fn read_range(
    git: &Git,
    exclude: Option<&str>,
    include: &str,
    limit: u32,
) -> Result<Vec<CommitEntry>> {
    if exclude.is_some_and(|value| !is_full_oid(value)) || !is_full_oid(include) {
        return Err(AppError::internal("读取提交范围时缺少完整对象 ID"));
    }
    let range = match exclude {
        Some(exclude) => format!("{exclude}..{include}"),
        None => include.to_string(),
    };
    let count = format!("--max-count={limit}");
    let args = vec![
        "--no-optional-locks",
        "-c",
        "core.quotePath=false",
        "--no-pager",
        "log",
        "--no-color",
        "--topo-order",
        "-z",
        LOG_FORMAT,
        &count,
        &range,
    ];
    let output = git.run_ok(&args, "读取提交范围")?;
    parse_log_page(&output.stdout)
}

/// 交给 Git 的文本筛选：正则走 Git 的 `--grep`（`--extended-regexp`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrepFilter {
    pub pattern: String,
    pub case_sensitive: bool,
    /// Git 的扩展正则（`-E`）；关闭时按字面匹配（`--fixed-strings`）。
    pub regex: bool,
}

/// 已解析、可直接组合成固定 Git 参数的日志筛选（上层负责校验取值）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogFilter {
    /// 作者（邮箱或名字）；Git 按 `--author` 的正则匹配，值来自已加载的历史。
    pub authors: Vec<String>,
    /// 只要这些路径（字面匹配，`--literal-pathspecs`；不跟随重命名）。
    pub paths: Vec<String>,
    /// `--since` 的取值（ISO 时间或日期，已由上层校验）。
    pub since: Option<String>,
    /// `--until` 的取值（含当天时上层已换算为次日零点减一秒）。
    pub until: Option<String>,
    pub grep: Option<GrepFilter>,
}

/// 扫描模式读到的提交：额外带回完整提交信息，供 Rust 侧做「文字或哈希前缀」匹配。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedCommit {
    pub entry: CommitEntry,
    pub message: String,
}

/// 扫描模式在同一个格式后追加完整提交信息（`%B`），供 Rust 侧匹配。
fn scan_format() -> String {
    format!("{LOG_FORMAT}%x00%B")
}

/// 组合固定的 Git 参数（不含起点与游标）。
fn log_args(args: &mut Vec<String>, filter: &LogFilter, first_parent: bool, format: &str) {
    if !filter.paths.is_empty() {
        args.push("--literal-pathspecs".into());
    }
    for value in [
        "--no-optional-locks",
        "-c",
        "core.quotePath=false",
        "--no-pager",
        "log",
        "--no-color",
        "--topo-order",
        "-z",
    ] {
        args.push(value.into());
    }
    args.push(format.to_string());
    if first_parent {
        args.push("--first-parent".into());
    }
    // `--grep` 与 `--author` 共用同一个匹配器；作者值由上层决定是否转义：
    // 有 `--extended-regexp` 时转义成字面量，否则用 `--fixed-strings` 按字面匹配。
    // 不能让作者落到 Git 的**基本**正则里：BRE 下 `\+` 是「一次或多次」，
    // 作者名里带 `+` 的提交会筛不出来。
    let mut matcher_set = false;
    if let Some(grep) = &filter.grep {
        let mut value = String::from("--grep=");
        value.push_str(&grep.pattern);
        args.push(value);
        if grep.regex {
            args.push("--extended-regexp".into());
        } else {
            args.push("--fixed-strings".into());
        }
        if !grep.case_sensitive {
            args.push("--regexp-ignore-case".into());
        }
        matcher_set = true;
    }
    if !matcher_set && !filter.authors.is_empty() {
        args.push("--fixed-strings".into());
    }
    for author in &filter.authors {
        let mut value = String::from("--author=");
        value.push_str(author);
        args.push(value);
    }
    if let Some(since) = &filter.since {
        args.push(format!("--since={since}"));
    }
    if let Some(until) = &filter.until {
        args.push(format!("--until={until}"));
    }
}

/// 带筛选读取一页（文本筛选用 Git 的 `--grep`）。
pub fn read_page_filtered(
    git: &Git,
    starts: &[String],
    first_parent: bool,
    skip: u32,
    limit: u32,
    filter: &LogFilter,
) -> Result<Vec<CommitEntry>> {
    if starts.is_empty() || starts.iter().any(|start| !is_full_oid(start)) {
        return Err(AppError::internal("分页起点不是完整提交 ID"));
    }
    let mut args: Vec<String> = Vec::new();
    log_args(&mut args, filter, first_parent, LOG_FORMAT);
    args.push(format!("--skip={skip}"));
    args.push(format!("--max-count={limit}"));
    for start in starts {
        args.push(start.clone());
    }
    if !filter.paths.is_empty() {
        args.push("--".into());
        for path in &filter.paths {
            args.push(path.clone());
        }
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = git.run_ok(&refs, "读取提交历史")?;
    parse_log_page(&output.stdout)
}

/// 扫描模式：同上，但每条记录额外带回完整提交信息（`%B`），供 Rust 侧匹配。
///
/// 只有「文字或哈希前缀」筛选（关闭正则时）使用这一条路径：哈希前缀无法用
/// Git 的 `--grep` 表达，因此由 Rust 在固定起点、固定筛选下逐批读取并匹配。
pub fn read_page_scanned(
    git: &Git,
    starts: &[String],
    first_parent: bool,
    skip: u32,
    limit: u32,
    filter: &LogFilter,
) -> Result<Vec<ScannedCommit>> {
    if starts.is_empty() || starts.iter().any(|start| !is_full_oid(start)) {
        return Err(AppError::internal("分页起点不是完整提交 ID"));
    }
    let mut args: Vec<String> = Vec::new();
    let format = scan_format();
    log_args(&mut args, filter, first_parent, &format);
    args.push(format!("--skip={skip}"));
    args.push(format!("--max-count={limit}"));
    for start in starts {
        args.push(start.clone());
    }
    if !filter.paths.is_empty() {
        args.push("--".into());
        for path in &filter.paths {
            args.push(path.clone());
        }
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = git.run_ok(&refs, "读取提交历史")?;
    parse_scanned_page(&output.stdout)
}

/// 解析扫描模式的输出（每条记录多一个提交信息字段）。
pub fn parse_scanned_page(bytes: &[u8]) -> Result<Vec<ScannedCommit>> {
    const FIELDS: usize = 12;
    let mut commits = Vec::new();
    let mut fields: Vec<&[u8]> = bytes.split(|byte| *byte == 0).collect();
    if fields.last().is_some_and(|field| field.is_empty()) {
        fields.pop();
    }
    if fields.len() % FIELDS != 0 {
        return Err(AppError::internal(format!(
            "提交扫描字段数不是 {FIELDS} 的整数倍：{}",
            fields.len()
        )));
    }
    for chunk in fields.chunks(FIELDS) {
        let oid = String::from_utf8_lossy(chunk[0]).into_owned();
        if !is_full_oid(&oid) {
            return Err(AppError::internal(format!("提交扫描里的对象 ID 不合法：{oid}")));
        }
        let parents = String::from_utf8_lossy(chunk[1])
            .split_whitespace()
            .map(|parent| parent.to_string())
            .collect();
        commits.push(ScannedCommit {
            entry: CommitEntry {
                oid,
                parents,
                author: signature_from_fields(chunk[2], chunk[3], chunk[4], chunk[5]),
                committer: signature_from_fields(chunk[6], chunk[7], chunk[8], chunk[9]),
                subject: String::from_utf8_lossy(chunk[10]).into_owned(),
            },
            message: String::from_utf8_lossy(chunk[11]).into_owned(),
        });
    }
    Ok(commits)
}

/// 解析 `git log -z --format=<NUL 分隔字段>` 的输出。
pub fn parse_log_page(bytes: &[u8]) -> Result<Vec<CommitEntry>> {
    const FIELDS: usize = 11;
    let mut commits = Vec::new();
    let mut fields: Vec<&[u8]> = bytes.split(|byte| *byte == 0).collect();
    // `-z` 只在最后一条记录后补一个 NUL，按字段分组时末尾会多出一个空字段。
    // 只去掉这一个：末尾提交的主题本身可以是空字符串（`--allow-empty-message`），
    // 多弹一个就会把它的字段吃掉，字段数不再是 FIELDS 的整数倍。
    if fields.last().is_some_and(|field| field.is_empty()) {
        fields.pop();
    }
    if fields.len() % FIELDS != 0 {
        return Err(AppError::internal(format!(
            "提交日志字段数不是 {FIELDS} 的整数倍：{}",
            fields.len()
        )));
    }
    for chunk in fields.chunks(FIELDS) {
        let oid = String::from_utf8_lossy(chunk[0]).into_owned();
        if !is_full_oid(&oid) {
            return Err(AppError::internal(format!("提交日志里的对象 ID 不合法：{oid}")));
        }
        let parents = String::from_utf8_lossy(chunk[1])
            .split_whitespace()
            .map(|parent| parent.to_string())
            .collect();
        commits.push(CommitEntry {
            oid,
            parents,
            author: signature_from_fields(chunk[2], chunk[3], chunk[4], chunk[5]),
            committer: signature_from_fields(chunk[6], chunk[7], chunk[8], chunk[9]),
            subject: String::from_utf8_lossy(chunk[10]).into_owned(),
        });
    }
    Ok(commits)
}

fn signature_from_fields(name: &[u8], email: &[u8], time: &[u8], iso: &[u8]) -> Signature {
    let time = String::from_utf8_lossy(time).trim().parse::<i64>().unwrap_or(0);
    let iso = String::from_utf8_lossy(iso).into_owned();
    // `%ai` / `%ci` 形如 `2026-10-27 14:22:08 +0800`，最后一截是时区。
    let timezone = iso.split_whitespace().last().unwrap_or("").to_string();
    Signature {
        name: String::from_utf8_lossy(name).into_owned(),
        email: String::from_utf8_lossy(email).into_owned(),
        time,
        timezone,
    }
}

/// 读取一个提交对象的完整内容（正文不截断）。
pub fn read_commit(git: &Git, oid: &str) -> Result<CommitDetail> {
    let oid = verify_commit_oid(git, oid)?;
    let output = git.run_ok(&["--no-optional-locks", "cat-file", "commit", &oid], "读取提交对象")?;
    parse_commit(&oid, &output.stdout)
}

/// 解析 `git cat-file commit` 的原始字节。
pub fn parse_commit(oid: &str, bytes: &[u8]) -> Result<CommitDetail> {
    let split = find_header_end(bytes);
    let (headers, message) = match split {
        Some(index) => (&bytes[..index], &bytes[index + 2..]),
        None => (bytes, &[][..]),
    };
    let mut tree = None;
    let mut parents = Vec::new();
    let mut author = None;
    let mut committer = None;
    let mut encoding = None;
    for line in headers.split(|byte| *byte == b'\n') {
        // 续行（gpgsig 等）以空格开头，不属于我们要读的头部。
        if line.first() == Some(&b' ') {
            continue;
        }
        let Some(space) = line.iter().position(|byte| *byte == b' ') else {
            continue;
        };
        let (key, value) = (&line[..space], &line[space + 1..]);
        match key {
            b"tree" => tree = Some(String::from_utf8_lossy(value).trim().to_string()),
            b"parent" => parents.push(String::from_utf8_lossy(value).trim().to_string()),
            b"author" => author = Some(parse_signature_line(value)),
            b"committer" => committer = Some(parse_signature_line(value)),
            b"encoding" => encoding = Some(String::from_utf8_lossy(value).trim().to_string()),
            _ => {}
        }
    }
    let tree = tree.ok_or_else(|| AppError::git(format!("提交对象缺少 tree 头：{oid}")))?;
    let author = author.ok_or_else(|| AppError::git(format!("提交对象缺少 author 头：{oid}")))?;
    let committer = committer.ok_or_else(|| AppError::git(format!("提交对象缺少 committer 头：{oid}")))?;
    let (message, message_lossy) = decode_message(message, encoding.as_deref());
    Ok(CommitDetail {
        oid: oid.to_string(),
        tree,
        parents,
        author,
        committer,
        encoding,
        message_lossy,
        message,
    })
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(2).position(|window| window == b"\n\n")
}

fn parse_signature_line(value: &[u8]) -> Signature {
    let text = String::from_utf8_lossy(value).into_owned();
    // `Name <email> 1234567890 +0800`
    let lowered = text.trim_end();
    let (head, tail) = match lowered.rsplit_once('>') {
        Some((head, tail)) => (format!("{head}>"), tail.trim()),
        None => (lowered.to_string(), ""),
    };
    let (name, email) = match head.rfind('<') {
        Some(index) => (
            head[..index].trim().to_string(),
            head[index + 1..head.len().saturating_sub(1)].to_string(),
        ),
        None => (head.trim().to_string(), String::new()),
    };
    let mut parts = tail.split_whitespace();
    let time = parts.next().and_then(|value| value.parse::<i64>().ok()).unwrap_or(0);
    let timezone = parts.next().unwrap_or("").to_string();
    Signature { name, email, time, timezone }
}

fn decode_message(bytes: &[u8], encoding: Option<&str>) -> (String, bool) {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return (text.to_string(), false);
    }
    // 无法在不引入转码库的前提下转换其它编码：如实标记有损。
    let _ = encoding;
    (String::from_utf8_lossy(bytes).into_owned(), true)
}

/// 读取提交相对一个明确父提交（根提交传空 tree 的对象 ID）的改动文件。
///
/// 属性取自该提交自己的树（`GIT_ATTR_SOURCE`）：工作区里的 `.gitattributes`
/// 不能改变已打开提交的“文本 / 二进制”判断。
pub fn read_changes(git: &Git, base: &str, commit: &str) -> Result<Vec<RawChange>> {
    if !is_full_oid(base) || !is_full_oid(commit) {
        return Err(AppError::internal("读取改动文件时缺少完整对象 ID"));
    }
    let git = git.with_env_vars(&[("GIT_ATTR_SOURCE", commit)]);
    let output = git.run_ok(
        &[
            "--no-optional-locks",
            "-c",
            "core.quotePath=false",
            "--no-pager",
            "diff-tree",
            "-r",
            "-z",
            "--raw",
            "--numstat",
            "-M",
            "--no-commit-id",
            "--no-ext-diff",
            "--no-textconv",
            base,
            commit,
        ],
        "读取提交改动文件",
    )?;
    parse_changes(&output.stdout)
}

/// 解析 `git diff-tree -r -z --raw --numstat -M` 的输出。
///
/// `--raw` 记录先输出，`--numstat` 记录后输出；两者都按 NUL 切字段。
/// 路径保留原始字节，任何字符都不做 trim 或转义解释。
pub fn parse_changes(bytes: &[u8]) -> Result<Vec<RawChange>> {
    let mut raw: Vec<RawChange> = Vec::new();
    let mut counts: HashMap<Vec<u8>, (u32, u32, bool)> = HashMap::new();
    let mut fields: Vec<&[u8]> = bytes.split(|byte| *byte == 0).collect();
    while fields.last().is_some_and(|field| field.is_empty()) {
        fields.pop();
    }
    let mut index = 0;
    while index < fields.len() {
        let field = fields[index];
        if field.first() != Some(&b':') {
            break;
        }
        let header = String::from_utf8_lossy(field);
        let parts: Vec<&str> = header[1..].split(' ').collect();
        if parts.len() < 5 {
            return Err(AppError::internal(format!("改动记录格式不对：{header}")));
        }
        let status_letter = parts[4].bytes().next().unwrap_or(b'M');
        let status = ChangeStatus::from_letter(status_letter)
            .ok_or_else(|| AppError::internal(format!("未知的改动状态：{}", parts[4])))?;
        let old_mode = mode_option(parts[0]);
        let new_mode = mode_option(parts[1]);
        let old_oid = oid_option(parts[2]);
        let new_oid = oid_option(parts[3]);
        index += 1;
        let pair = matches!(status, ChangeStatus::Renamed | ChangeStatus::Copied);
        let path_count = if pair { 2 } else { 1 };
        if index + path_count > fields.len() {
            return Err(AppError::internal("改动记录缺少路径字段"));
        }
        let (old_path, new_path) = if pair {
            let old = fields[index].to_vec();
            let new = fields[index + 1].to_vec();
            index += 2;
            (Some(old), Some(new))
        } else {
            let path = fields[index].to_vec();
            index += 1;
            match status {
                ChangeStatus::Added => (None, Some(path)),
                ChangeStatus::Deleted => (Some(path), None),
                _ => (Some(path.clone()), Some(path)),
            }
        };
        raw.push(RawChange {
            status,
            old_path,
            new_path,
            old_mode,
            new_mode,
            old_oid,
            new_oid,
            added: 0,
            deleted: 0,
            binary: false,
        });
    }
    // 其余字段是 numstat 记录。
    while index < fields.len() {
        let field = fields[index];
        index += 1;
        if field.is_empty() {
            continue;
        }
        let Some(first_tab) = field.iter().position(|byte| *byte == b'\t') else {
            continue;
        };
        let rest = &field[first_tab + 1..];
        let Some(second_tab) = rest.iter().position(|byte| *byte == b'\t') else {
            continue;
        };
        let added = &field[..first_tab];
        let deleted = &rest[..second_tab];
        let path = &rest[second_tab + 1..];
        let binary = added == b"-" && deleted == b"-";
        let added_count = String::from_utf8_lossy(added).parse::<u32>().unwrap_or(0);
        let deleted_count = String::from_utf8_lossy(deleted).parse::<u32>().unwrap_or(0);
        let key = if path.is_empty() {
            // 重命名 / 复制：数值记录后跟旧路径、新路径两个字段。
            if index + 1 < fields.len() {
                let new_path = fields[index + 1].to_vec();
                index += 2;
                new_path
            } else {
                continue;
            }
        } else {
            path.to_vec()
        };
        counts.insert(key, (added_count, deleted_count, binary));
    }
    for change in &mut raw {
        let key = change.new_path.as_ref().or(change.old_path.as_ref()).cloned().unwrap_or_default();
        if let Some((added, deleted, binary)) = counts.get(&key) {
            change.added = *added;
            change.deleted = *deleted;
            change.binary = *binary;
        }
    }
    Ok(raw)
}

/// 读取一个文件的提交 diff（只含该文件）。路径必须是后端确认过的身份。
pub fn file_patch(
    git: &Git,
    base: &str,
    commit: &str,
    paths: &[&str],
    ignore: IgnoreWhitespace,
) -> Result<Vec<u8>> {
    if !is_full_oid(base) || !is_full_oid(commit) {
        return Err(AppError::internal("读取提交 diff 时缺少完整对象 ID"));
    }
    if paths.is_empty() {
        return Err(AppError::internal("读取提交 diff 时缺少路径"));
    }
    // 与改动文件列表一致：属性取自这个提交自己的树，工作区的 `.gitattributes`
    // 不能把已打开的提交 diff 从文本变成二进制。
    let git = git.with_env_vars(&[("GIT_ATTR_SOURCE", commit)]);
    let mut args: Vec<&str> = vec![
        "--no-optional-locks",
        "--literal-pathspecs",
        "-c",
        "core.quotePath=false",
        "--no-pager",
        "diff",
    ];
    args.extend_from_slice(DIFF_FLAGS);
    args.extend_from_slice(ignore.diff_args());
    if ignore.ignore_blank_lines() {
        args.push("--ignore-blank-lines");
    }
    args.extend_from_slice(&["-M", base, commit, "--"]);
    args.extend_from_slice(paths);
    let output = git.run_ok(&args, "读取提交 diff")?;
    if output.stdout.len() > MAX_DIFF_BYTES {
        return Err(AppError::rejected(format!(
            "文件差异超过 {} MiB，暂不支持在历史中显示",
            MAX_DIFF_BYTES / (1024 * 1024)
        )));
    }
    Ok(output.stdout)
}

/// 读取一个 blob 的原始字节；缺失对象或超过上限都明确报错。
pub fn blob(git: &Git, oid: &str) -> Result<Vec<u8>> {
    if !is_full_oid(oid) {
        return Err(AppError::rejected(format!("不是完整的对象 ID：{oid}")));
    }
    let size_output = git.run(&["--no-optional-locks", "cat-file", "-s", oid])?;
    if size_output.exit_code != 0 {
        return Err(AppError::git(format!(
            "读取对象大小失败（对象可能缺失）：{oid}：{}",
            size_output.stderr.trim()
        )));
    }
    let size = size_output.stdout_lossy().trim().parse::<u64>().unwrap_or(0);
    if size > MAX_BLOB_BYTES {
        return Err(AppError::rejected(format!(
            "文件大小 {size} 字节超过历史查看上限 {} MiB",
            MAX_BLOB_BYTES / (1024 * 1024)
        )));
    }
    let output = git.run_ok(&["--no-optional-locks", "cat-file", "blob", oid], "读取文件内容")?;
    Ok(output.stdout)
}

/// 内容里是否出现 NUL 字节（与 Git 判断二进制的方式大致一致）。
pub fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8000).any(|byte| *byte == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nul(records: &[&[u8]]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for record in records {
            bytes.extend_from_slice(record);
            bytes.push(0);
        }
        bytes
    }

    #[test]
    fn parses_raw_and_numstat_with_rename() {
        let bytes = nul(&[
            b":100644 100755 422c2b7ab3b3c668038da977e4e93a5fc623169c de980441c3ab03a8c07dda1ad27b8a11f39deb1e R066",
            b"a.txt",
            b"renamed.txt",
            b"1\t0\t",
            b"a.txt",
            b"renamed.txt",
        ]);
        let changes = parse_changes(&bytes).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].status, ChangeStatus::Renamed);
        assert_eq!(changes[0].old_path.as_deref(), Some(b"a.txt".as_slice()));
        assert_eq!(changes[0].new_path.as_deref(), Some(b"renamed.txt".as_slice()));
        assert_eq!(changes[0].old_mode.as_deref(), Some("100644"));
        assert_eq!(changes[0].new_mode.as_deref(), Some("100755"));
        assert_eq!(changes[0].added, 1);
        assert_eq!(changes[0].deleted, 0);
    }

    #[test]
    fn keeps_special_path_bytes_verbatim() {
        let bytes = nul(&[
            b":000000 100644 0000000000000000000000000000000000000000 5626abf0f72e58d7a153368ba57db4c673c0e171 A",
            b" lead and trail ",
            b":000000 100644 0000000000000000000000000000000000000000 5626abf0f72e58d7a153368ba57db4c673c0e171 A",
            b"bad\xffname.txt",
            b"- - \t".as_slice(),
        ]);
        // 上面故意留了不完整记录：解析应只取合法部分，不 panic。
        let changes = parse_changes(&bytes).unwrap();
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].primary_path(), b" lead and trail ");
        assert_eq!(changes[1].primary_path(), b"bad\xffname.txt");
    }

    #[test]
    fn numstat_binary_and_tab_path() {
        let bytes = nul(&[
            b":000000 100644 0000000000000000000000000000000000000000 5626abf0f72e58d7a153368ba57db4c673c0e171 A",
            b"tab\tname.txt",
            b"-\t-\ttab\tname.txt",
        ]);
        let changes = parse_changes(&bytes).unwrap();
        assert_eq!(changes.len(), 1);
        assert!(changes[0].binary);
        assert_eq!(changes[0].primary_path(), b"tab\tname.txt");
    }

    #[test]
    fn parses_commit_headers_and_message() {
        let raw = b"tree 528ddf0de85b0b04e0fe4615a4c43b3c8efc1717\n\
parent 1111111111111111111111111111111111111111\n\
parent 2222222222222222222222222222222222222222\n\
author A U Thor <a@example.com> 1793082908 +0800\n\
committer C O Mitter <c@example.com> 1793082999 -0500\n\
\n\
subject line\n\
\n\
body\n";
        let oid = "0123456789012345678901234567890123456789";
        let detail = parse_commit(oid, raw).unwrap();
        assert_eq!(detail.parents.len(), 2);
        assert_eq!(detail.author.name, "A U Thor");
        assert_eq!(detail.author.email, "a@example.com");
        assert_eq!(detail.author.time, 1793082908);
        assert_eq!(detail.author.timezone, "+0800");
        assert_eq!(detail.committer.timezone, "-0500");
        assert_eq!(detail.message, "subject line\n\nbody\n");
        assert!(!detail.message_lossy);
    }

    #[test]
    fn decodes_lossy_message_and_skips_gpgsig_continuation() {
        let raw = b"tree 528ddf0de85b0b04e0fe4615a4c43b3c8efc1717\n\
author A <a@example.com> 1 +0000\n\
committer A <a@example.com> 1 +0000\n\
encoding ISO-8859-1\n\
gpgsig -----BEGIN PGP SIGNATURE-----\n \
fake\n\
\n\
caf\xe9\n";
        let oid = "0123456789012345678901234567890123456789";
        let detail = parse_commit(oid, raw).unwrap();
        assert_eq!(detail.encoding.as_deref(), Some("ISO-8859-1"));
        assert!(detail.message_lossy);
        assert!(detail.message.starts_with("caf"));
    }

    #[test]
    fn parses_nul_framed_log_page() {
        let oid = "0123456789012345678901234567890123456789";
        let bytes = nul(&[
            oid.as_bytes(),
            b"1111111111111111111111111111111111111111 2222222222222222222222222222222222222222",
            b"Name",
            b"n@example.com",
            b"123",
            b"2026-10-27 14:22:08 +0800",
            b"C",
            b"c@example.com",
            b"456",
            b"2026-10-27 14:22:09 -0500",
            b"subject",
        ]);
        let commits = parse_log_page(&bytes).unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].parents.len(), 2);
        assert_eq!(commits[0].author.time, 123);
        assert_eq!(commits[0].committer.timezone, "-0500");
        assert_eq!(commits[0].subject, "subject");
    }

    #[test]
    fn keeps_an_empty_subject_on_the_last_record() {
        let oid = "0123456789012345678901234567890123456789";
        let bytes = nul(&[
            oid.as_bytes(),
            b"",
            b"Name",
            b"[EMAIL]",
            b"123",
            b"2026-10-27 14:22:08 +0800",
            b"C",
            b"[EMAIL]",
            b"456",
            b"2026-10-27 14:22:09 -0500",
            b"",
        ]);
        let commits = parse_log_page(&bytes).unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].subject, "");
        assert!(commits[0].parents.is_empty());
    }

    #[test]
    fn rejects_short_or_non_hex_oids() {
        assert!(!is_full_oid("--help"));
        assert!(!is_full_oid("HEAD~1"));
        assert!(!is_full_oid("abc123"));
        assert!(is_full_oid("0123456789012345678901234567890123456789"));
        assert!(is_full_oid(&"a".repeat(64)));
    }
}
