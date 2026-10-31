//! M7 扩展工作流（Q176–Q199）：主题导入、工作区编辑、内置终端、克隆、
//! GitHub 账号与 PR、诊断、应用更新。
//!
//! 边界（AGENTS.md、规格 F/H）：
//! - Git 仍然只由 Rust 调用系统 CLI（ADR-0002）；前端只调用这里带类型的命令；
//! - 网络、钥匙串、PTY、更新只在外部边界做受控适配，生产入口真实执行；
//! - 诊断与草稿只写 Revia 自己的数据目录，不改用户 Git 配置，不碰用户凭据。

pub mod clone;
pub mod commands;
pub mod diagnostics;
pub mod edit;
pub mod github;
pub mod terminal;
pub mod theme;
pub mod update;
pub mod update_channels;
pub mod update_package;

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{AppError, ErrorKind, Result};

/// Unix 毫秒时间戳（诊断、草稿、缓存的排序与到期都用它）。
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// 原子写入：同目录临时文件 + rename，失败时不留半个文件。
///
/// 只用于 Revia 自己的数据目录（草稿、诊断、缓存、导入主题），
/// 不用于用户的仓库文件（那里必须保持原 inode 与权限）。
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let directory = path.parent().ok_or_else(|| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.m7.writeTargetDirectory",
            "写入目标缺少目录",
            &[],
        )
    })?;
    std::fs::create_dir_all(directory)?;
    let mut temp = tempfile::NamedTempFile::new_in(directory)?;
    use std::io::Write;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|error| error.error)?;
    Ok(())
}

/// 把用户可见的名字收敛成文件名安全的形式（诊断导出、主题 id 前缀等）。
pub fn safe_slug(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
        if out.len() >= 48 {
            break;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "item".into()
    } else {
        trimmed
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
