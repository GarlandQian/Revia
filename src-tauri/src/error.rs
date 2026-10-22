//! 错误类型：Rust 层内部保留细节，跨 IPC 只传可展示的字符串。
//!
//! 前端按 kind 分别展示（Q36）：操作被拒绝 / Git 命令失败 / 环境问题。

use std::fmt;

#[derive(Debug)]
pub struct AppError {
    pub kind: ErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// 操作被拒绝（参数不合法、状态过期等）。
    Rejected,
    /// Git 命令失败。
    Git,
    /// 环境问题（找不到 shell、解析环境失败等）。
    Environment,
    /// 内部错误。
    Internal,
    /// 结果待核对（超时、中断等导致写入结果不确定，Q91/规格第 3 节）。
    NeedsVerification,
}

impl AppError {
    pub fn rejected(message: impl Into<String>) -> Self {
        Self { kind: ErrorKind::Rejected, message: message.into() }
    }

    pub fn git(message: impl Into<String>) -> Self {
        Self { kind: ErrorKind::Git, message: message.into() }
    }

    pub fn environment(message: impl Into<String>) -> Self {
        Self { kind: ErrorKind::Environment, message: message.into() }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self { kind: ErrorKind::Internal, message: message.into() }
    }

    pub fn needs_verification(message: impl Into<String>) -> Self {
        Self { kind: ErrorKind::NeedsVerification, message: message.into() }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        AppError::environment(format!("IO 错误：{error}"))
    }
}

impl From<serde_json::Error> for AppError {
    fn from(error: serde_json::Error) -> Self {
        AppError::internal(format!("JSON 错误：{error}"))
    }
}

impl serde::Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.message)
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
