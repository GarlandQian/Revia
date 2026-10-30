//! 工作区一侧的文本编辑与编辑草稿（M7 任务 03、Q177 / Q186 / Q195）。
//!
//! 只有绑定真实 worktree、路径类型安全、可无损写回的普通文本才能编辑：
//! 历史、暂存区、二进制、符号链接、子模块、冲突文件与超出大小限制的文件一律只读。
//!
//! 保存必须走 `WorktreeSession` 的写锁与身份校验，并在写前核对磁盘内容指纹；
//! 外部改动时拒绝旧保存，由前端把草稿与磁盘两版一起给用户核对。

use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, ErrorKind, Result};
use crate::session::WorktreeSession;

use super::sha256_hex;

/// 可编辑文本的大小上限（与 diff 的“大文件”警告口径一致，Q79）。
pub const MAX_EDIT_BYTES: u64 = 2 * 1024 * 1024;

/// 只读原因（界面按这些枚举值查文案，不在前端猜原因）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum EditReadonlyReason {
    /// 不是当前 worktree 里的普通文件路径。
    NotInWorktree,
    /// 符号链接（写回会改到链接目标，超出“工作区文本”范围）。
    Symlink,
    /// 目录、设备、管道等非普通文件。
    NotRegularFile,
    /// 子模块（gitlink），编辑需要单独的 Git 流程。
    Submodule,
    /// 含 NUL 字节，按二进制处理。
    Binary,
    /// 超出大小上限。
    TooLarge,
    /// 不是合法 UTF-8。
    Encoding,
    /// 冲突未解决。
    Unmerged,
    /// 文件当前不存在（已删除）。
    Missing,
}

/// 换行策略：保存时保持原文件的换行风格。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum EditNewline {
    Lf,
    Crlf,
}

/// 打开编辑的结果：可编辑时带正文与基线指纹，否则只带只读原因。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EditableFile {
    pub worktree_id: String,
    pub path: String,
    pub text: Option<String>,
    /// 基线内容指纹（保存时作为“我改的是这一版”的凭据）。
    pub base_sha256: Option<String>,
    pub readonly_reason: Option<EditReadonlyReason>,
    pub executable: bool,
    pub newline: EditNewline,
    pub has_bom: bool,
    pub size_bytes: u64,
}

/// 保存请求：基线指纹必须与磁盘当前内容一致。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditSaveRequest {
    pub worktree_id: String,
    pub path: String,
    pub base_sha256: String,
    pub text: String,
}

/// 保存成功后的可观察结果。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EditSaved {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
    pub executable: bool,
    pub newline: EditNewline,
}

/// 本机编辑草稿：绑定 worktree 身份、路径与基线版本。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EditDraft {
    pub worktree_id: String,
    pub path: String,
    pub base_sha256: String,
    pub text: String,
    pub updated_at_ms: u64,
}

/// 草稿存储（Revia 数据目录，跨重启保留；按 worktree + 路径唯一）。
pub struct EditDraftStore {
    directory: PathBuf,
}

impl EditDraftStore {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            directory: data_dir.join("edit-drafts"),
        }
    }

    pub fn save(&self, draft: &EditDraft) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(draft)?;
        super::atomic_write(&self.path_for(&draft.worktree_id, &draft.path), &bytes)
    }

    pub fn load(&self, worktree_id: &str, path: &str) -> Result<Option<EditDraft>> {
        match std::fs::read(self.path_for(worktree_id, path)) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes).ok()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn clear(&self, worktree_id: &str, path: &str) -> Result<()> {
        match std::fs::remove_file(self.path_for(worktree_id, path)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    /// 某个 worktree 的全部草稿（切换仓库、关闭确认时用）。
    pub fn list(&self, worktree_id: &str) -> Result<Vec<EditDraft>> {
        let mut drafts = Vec::new();
        let entries = match std::fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(drafts),
            Err(error) => return Err(error.into()),
        };
        for entry in entries {
            let path = entry?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else { continue };
            let Ok(draft) = serde_json::from_slice::<EditDraft>(&bytes) else {
                continue;
            };
            if draft.worktree_id == worktree_id {
                drafts.push(draft);
            }
        }
        drafts.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(drafts)
    }

    fn path_for(&self, worktree_id: &str, path: &str) -> PathBuf {
        let digest = sha256_hex(format!("{worktree_id}\0{path}").as_bytes());
        self.directory.join(format!("{digest}.json"))
    }
}

/// 解析并校验 worktree 内的相对路径（拒绝绝对路径、`..` 与保留名字）。
fn resolve_path(session: &WorktreeSession, path: &str) -> Result<PathBuf> {
    if path.trim().is_empty() {
        return Err(readonly(EditReadonlyReason::NotInWorktree));
    }
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        return Err(readonly(EditReadonlyReason::NotInWorktree));
    }
    for component in candidate.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            _ => return Err(readonly(EditReadonlyReason::NotInWorktree)),
        }
    }
    let full = session.path.join(candidate);
    // 父目录必须仍在 worktree 内（防止符号链接目录把写入带到别处）。
    let parent = full.parent().unwrap_or(&session.path);
    let canonical_parent = std::fs::canonicalize(parent)
        .map_err(|_| readonly(EditReadonlyReason::Missing))?;
    let canonical_root =
        std::fs::canonicalize(&session.path).map_err(|_| readonly(EditReadonlyReason::NotInWorktree))?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err(readonly(EditReadonlyReason::NotInWorktree));
    }
    Ok(full)
}

fn readonly(reason: EditReadonlyReason) -> AppError {
    let (code, message) = match reason {
        EditReadonlyReason::NotInWorktree => (
            "errors.m7.editPathOutsideWorktree",
            "只能编辑当前 worktree 里的文件",
        ),
        EditReadonlyReason::Symlink => (
            "errors.m7.editSymlinkReadonly",
            "符号链接只读，不能在这里编辑",
        ),
        EditReadonlyReason::NotRegularFile => (
            "errors.m7.editNotRegularFile",
            "不是普通文件，不能编辑",
        ),
        EditReadonlyReason::Submodule => (
            "errors.m7.editSubmoduleReadonly",
            "子模块只读，请在其 worktree 中打开",
        ),
        EditReadonlyReason::Binary => (
            "errors.m7.editBinaryReadonly",
            "二进制文件只读，不能在文本视图里编辑",
        ),
        EditReadonlyReason::TooLarge => (
            "errors.m7.editTooLargeReadonly",
            "文件过大，超过可编辑上限（2 MB）",
        ),
        EditReadonlyReason::Encoding => (
            "errors.m7.editEncodingReadonly",
            "文件不是 UTF-8 文本，只读",
        ),
        EditReadonlyReason::Unmerged => (
            "errors.m7.editUnmergedReadonly",
            "冲突未解决，先在合并视图处理",
        ),
        EditReadonlyReason::Missing => (
            "errors.m7.editFileMissing",
            "文件已不存在，请刷新后重新打开",
        ),
    };
    AppError::coded(ErrorKind::Rejected, code, message, &[])
}

/// 解码磁盘内容（BOM、换行、UTF-8）。
#[derive(Debug)]
struct Decoded {
    text: String,
    newline: EditNewline,
    has_bom: bool,
    sha256: String,
}

fn decode(bytes: &[u8]) -> Result<Decoded> {
    let sha256 = sha256_hex(bytes);
    let (body, has_bom) = match bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        Some(rest) => (rest, true),
        None => (bytes, false),
    };
    if body.iter().take(8192).any(|byte| *byte == 0) {
        return Err(readonly(EditReadonlyReason::Binary));
    }
    let text = std::str::from_utf8(body)
        .map_err(|_| readonly(EditReadonlyReason::Encoding))?
        .to_string();
    let newline = if text.contains("\r\n") {
        EditNewline::Crlf
    } else {
        EditNewline::Lf
    };
    Ok(Decoded {
        text,
        newline,
        has_bom,
        sha256,
    })
}

fn is_executable(metadata: &std::fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        false
    }
}

/// 打开一个文件进入编辑：返回正文与基线指纹，或只读原因。
pub fn open(session: &WorktreeSession, path: &str) -> Result<EditableFile> {
    session.require_identity()?;
    let mut file = EditableFile {
        worktree_id: session.id.clone(),
        path: path.to_string(),
        text: None,
        base_sha256: None,
        readonly_reason: None,
        executable: false,
        newline: EditNewline::Lf,
        has_bom: false,
        size_bytes: 0,
    };
    let full = match resolve_path(session, path) {
        Ok(full) => full,
        Err(error) => {
            file.readonly_reason = Some(reason_of(&error).unwrap_or(EditReadonlyReason::NotInWorktree));
            return Ok(file);
        }
    };
    let metadata = match std::fs::symlink_metadata(&full) {
        Ok(metadata) => metadata,
        Err(_) => {
            file.readonly_reason = Some(EditReadonlyReason::Missing);
            return Ok(file);
        }
    };
    if metadata.file_type().is_symlink() {
        file.readonly_reason = Some(EditReadonlyReason::Symlink);
        return Ok(file);
    }
    if metadata.is_dir() {
        file.readonly_reason = Some(EditReadonlyReason::Submodule);
        return Ok(file);
    }
    if !metadata.is_file() {
        file.readonly_reason = Some(EditReadonlyReason::NotRegularFile);
        return Ok(file);
    }
    file.size_bytes = metadata.len();
    file.executable = is_executable(&metadata);
    // 冲突文件与只读条目按 Git 状态判断，不看文件内容。
    if let Ok(state) = crate::git::status::read(&session.git) {
        if let Some(entry) = state.entries.iter().find(|entry| entry.path == path) {
            if entry.unmerged {
                file.readonly_reason = Some(EditReadonlyReason::Unmerged);
                return Ok(file);
            }
        }
    }
    if metadata.len() > MAX_EDIT_BYTES {
        file.readonly_reason = Some(EditReadonlyReason::TooLarge);
        return Ok(file);
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    match std::fs::File::open(&full).and_then(|mut handle| handle.read_to_end(&mut bytes)) {
        Ok(_) => {}
        Err(_) => {
            file.readonly_reason = Some(EditReadonlyReason::Missing);
            return Ok(file);
        }
    }
    let decoded = match decode(&bytes) {
        Ok(decoded) => decoded,
        Err(error) => {
            file.readonly_reason = Some(reason_of(&error).unwrap_or(EditReadonlyReason::Encoding));
            return Ok(file);
        }
    };
    file.text = Some(decoded.text);
    file.base_sha256 = Some(decoded.sha256);
    file.newline = decoded.newline;
    file.has_bom = decoded.has_bom;
    Ok(file)
}

fn reason_of(error: &AppError) -> Option<EditReadonlyReason> {
    match error.code? {
        "errors.m7.editPathOutsideWorktree" => Some(EditReadonlyReason::NotInWorktree),
        "errors.m7.editSymlinkReadonly" => Some(EditReadonlyReason::Symlink),
        "errors.m7.editNotRegularFile" => Some(EditReadonlyReason::NotRegularFile),
        "errors.m7.editSubmoduleReadonly" => Some(EditReadonlyReason::Submodule),
        "errors.m7.editBinaryReadonly" => Some(EditReadonlyReason::Binary),
        "errors.m7.editTooLargeReadonly" => Some(EditReadonlyReason::TooLarge),
        "errors.m7.editEncodingReadonly" => Some(EditReadonlyReason::Encoding),
        "errors.m7.editUnmergedReadonly" => Some(EditReadonlyReason::Unmerged),
        "errors.m7.editFileMissing" => Some(EditReadonlyReason::Missing),
        _ => None,
    }
}

/// 显式保存：写前核对身份、路径类型与磁盘指纹，保留权限、BOM 与换行策略。
pub fn save(session: &WorktreeSession, request: &EditSaveRequest) -> Result<EditSaved> {
    // 写锁与身份校验沿用既有写门禁（Q90、ADR-0005）：同一 worktree 不并发写。
    let _write = session.acquire_write_lock()?;
    session.require_identity()?;
    if request.text.contains('\0') {
        return Err(readonly(EditReadonlyReason::Binary));
    }
    let full = resolve_path(session, &request.path)?;
    let metadata = std::fs::symlink_metadata(&full).map_err(|_| readonly(EditReadonlyReason::Missing))?;
    if metadata.file_type().is_symlink() {
        return Err(readonly(EditReadonlyReason::Symlink));
    }
    if !metadata.is_file() {
        return Err(readonly(EditReadonlyReason::NotRegularFile));
    }
    if metadata.len() > MAX_EDIT_BYTES {
        return Err(readonly(EditReadonlyReason::TooLarge));
    }
    let mut current = Vec::with_capacity(metadata.len() as usize);
    std::fs::File::open(&full)
        .and_then(|mut handle| handle.read_to_end(&mut current))
        .map_err(|_| readonly(EditReadonlyReason::Missing))?;
    let decoded = decode(&current)?;
    if decoded.sha256 != request.base_sha256 {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.editFileChangedOnDisk",
            "文件已在磁盘上被修改，保存已拒绝；请先比较草稿与磁盘内容",
            &[("diskSha256", decoded.sha256)],
        ));
    }
    let encoded = encode(&request.text, decoded.newline, decoded.has_bom);
    if encoded.len() as u64 > MAX_EDIT_BYTES {
        return Err(readonly(EditReadonlyReason::TooLarge));
    }
    let result = (|| -> Result<()> {
        let mut handle = std::fs::OpenOptions::new().write(true).truncate(true).open(&full)?;
        handle.write_all(&encoded)?;
        handle.sync_all()?;
        Ok(())
    })();
    if let Err(error) = result {
        return Err(AppError::coded(
            ErrorKind::NeedsVerification,
            "errors.m7.editWriteFailedKeepDraft",
            "写入文件失败，草稿已保留；请核对磁盘内容后再试：{error}",
            &[("error", error.to_string())],
        ));
    }
    // 写完立刻回读核对：读回失败或内容不符时结果待核对，不自动重发。
    let mut after = Vec::new();
    match std::fs::File::open(&full).and_then(|mut handle| handle.read_to_end(&mut after)) {
        Ok(_) if after == encoded => {}
        Ok(_) => {
            return Err(AppError::coded(
                ErrorKind::NeedsVerification,
                "errors.m7.editWriteUnverified",
                "写入后回读内容不一致，结果待核对；草稿已保留",
                &[],
            ))
        }
        Err(error) => {
            return Err(AppError::coded(
                ErrorKind::NeedsVerification,
                "errors.m7.editWriteUnverifiedCause",
                "写入后无法回读文件，结果待核对；草稿已保留：{error}",
                &[("error", error.to_string())],
            ))
        }
    }
    let metadata = std::fs::metadata(&full)?;
    Ok(EditSaved {
        path: request.path.clone(),
        sha256: sha256_hex(&after),
        bytes: after.len() as u64,
        executable: is_executable(&metadata),
        newline: decoded.newline,
    })
}

fn encode(text: &str, newline: EditNewline, has_bom: bool) -> Vec<u8> {
    let body = match newline {
        EditNewline::Lf => text.replace("\r\n", "\n"),
        EditNewline::Crlf => text.replace("\r\n", "\n").replace('\n', "\r\n"),
    };
    let mut bytes = Vec::with_capacity(body.len() + 3);
    if has_bom {
        bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    bytes.extend_from_slice(body.as_bytes());
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_preserves_crlf_and_bom() {
        assert_eq!(encode("a\nb\n", EditNewline::Crlf, false), b"a\r\nb\r\n");
        assert_eq!(encode("a\r\nb", EditNewline::Lf, false), b"a\nb");
        assert_eq!(
            encode("a", EditNewline::Lf, true),
            vec![0xEF, 0xBB, 0xBF, b'a']
        );
    }

    #[test]
    fn decode_reports_binary_and_encoding() {
        assert_eq!(
            decode(&[0x00, 0x01]).unwrap_err().code,
            Some("errors.m7.editBinaryReadonly")
        );
        assert_eq!(
            decode(&[0xFF, 0xFE]).unwrap_err().code,
            Some("errors.m7.editEncodingReadonly")
        );
        let decoded = decode("\u{feff}a\r\nb".as_bytes()).unwrap();
        assert!(decoded.has_bom);
        assert_eq!(decoded.newline, EditNewline::Crlf);
        assert_eq!(decoded.text, "a\r\nb");
    }
}
