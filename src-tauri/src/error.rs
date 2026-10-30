//! 错误类型：Rust 层保留可展示模板与参数，跨 IPC 传结构化载荷。
//!
//! 展示规则（ADR-0013、Q164）：
//! - `code` 是稳定的字面量键（`errors.<模块>.<语义>`），前端按键查文案；
//! - `params` 只放数据（路径、退出码、Git 输出等），不放整段中文解释；
//! - 内层错误作为结构化的 `cause` 传递，前端递归格式化，不拼成字符串参数；
//! - `message` 是按中文模板渲染出的诊断文本，仅供日志、测量与旧测试使用，
//!   界面在存在 `code` 时不得直接展示它。
//!
//! 迁移前的构造函数（`rejected` / `git` / …）仍保留：迁移脚本尚未覆盖的位置
//! 与测试辅助代码可以继续使用；新代码一律使用 `coded`。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use ts_rs::TS;

#[derive(Debug)]
pub struct AppError {
    pub kind: ErrorKind,
    /// 稳定的展示键；`None` 表示仍在使用整段中文消息的旧调用点。
    pub code: Option<&'static str>,
    pub params: Vec<(&'static str, String)>,
    pub cause: Option<Box<AppErrorPayload>>,
    pub related: Vec<AppErrorPayload>,
    pub message: String,
    template: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppErrorPayload {
    pub kind: ErrorKind,
    /// 稳定展示键。旧调用点可能没有（`None`），迁移完成后应为 `Some`。
    #[serde(default)]
    #[ts(optional)]
    pub code: Option<String>,
    /// 展示参数：只含数据，不含成段的中文说明。
    #[serde(default)]
    #[ts(optional)]
    pub params: Option<BTreeMap<String, String>>,
    /// 内层错误（结构化保留，前端递归格式化）。
    #[serde(default)]
    #[ts(optional)]
    pub cause: Option<Box<AppErrorPayload>>,
    /// Additional failures from the same operation, kept structured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub related: Option<Vec<AppErrorPayload>>,
    /// 中文诊断文本，界面在有 `code` 时不直接使用。
    pub message: String,
}

/// 按模板渲染展示文本。
///
/// 支持 `{{` / `}}` 转义与 `{key}` 占位；`{cause}` 用内层错误的展示文本填充。
/// 未提供的键保持原样，便于在测试里发现漏传参数。
pub fn render_message(
    template: &str,
    params: &[(&'static str, String)],
    cause: Option<&str>,
) -> String {
    let mut out = String::with_capacity(template.len());
    let bytes = template.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let rest = &template[index..];
        if rest.starts_with("{{") {
            out.push('{');
            index += 2;
            continue;
        }
        if rest.starts_with("}}") {
            out.push('}');
            index += 2;
            continue;
        }
        if let Some(rest) = rest.strip_prefix('{') {
            let Some(end) = rest.find('}') else {
                out.push('{');
                index += 1;
                continue;
            };
            let key = &rest[..end];
            if key == "cause" {
                match cause {
                    Some(text) => out.push_str(text),
                    None => out.push_str("{cause}"),
                }
            } else if let Some((_, value)) = params.iter().find(|(name, _)| *name == key) {
                out.push_str(value);
            } else {
                out.push('{');
                out.push_str(key);
                out.push('}');
            }
            index += end + 2;
            continue;
        }
        let ch = rest.chars().next().expect("non-empty");
        out.push(ch);
        index += ch.len_utf8();
    }
    out
}

impl AppError {
    fn build(
        kind: ErrorKind,
        code: Option<&'static str>,
        template: Option<&'static str>,
        params: Vec<(&'static str, String)>,
        message: String,
    ) -> Self {
        Self {
            kind,
            code,
            params,
            cause: None,
            related: Vec::new(),
            message,
            template,
        }
    }

    /// 结构化错误：稳定键 + 中文模板 + 数据参数。
    pub fn coded(
        kind: ErrorKind,
        code: &'static str,
        template: &'static str,
        params: &[(&'static str, String)],
    ) -> Self {
        let message = render_message(template, params, None);
        Self::build(kind, Some(code), Some(template), params.to_vec(), message)
    }

    /// 以内层错误作为结构化原因。
    pub fn cause(mut self, error: AppError) -> Self {
        let cause_text = error.message.clone();
        self.cause = Some(Box::new(error.payload()));
        if let Some(template) = self.template {
            self.message = render_message(template, &self.params, Some(&cause_text));
        } else {
            self.message = format!("{}：{cause_text}", self.message);
        }
        self
    }

    pub fn related(mut self, error: &AppError) -> Self {
        self.related.push(error.payload());
        self
    }

    /// 以内层错误载荷作为结构化原因（已有载荷、无需再移动 `AppError` 时用）。
    pub fn cause_payload(mut self, cause: AppErrorPayload) -> Self {
        let cause_text = cause.message.clone();
        self.cause = Some(Box::new(cause));
        if let Some(template) = self.template {
            self.message = render_message(template, &self.params, Some(&cause_text));
        } else {
            self.message = format!("{}：{cause_text}", self.message);
        }
        self
    }

    pub fn rejected(message: impl Into<String>) -> Self {
        Self::build(ErrorKind::Rejected, None, None, Vec::new(), message.into())
    }

    pub fn git(message: impl Into<String>) -> Self {
        Self::build(ErrorKind::Git, None, None, Vec::new(), message.into())
    }

    pub fn environment(message: impl Into<String>) -> Self {
        Self::build(
            ErrorKind::Environment,
            None,
            None,
            Vec::new(),
            message.into(),
        )
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::build(ErrorKind::Internal, None, None, Vec::new(), message.into())
    }

    pub fn needs_verification(message: impl Into<String>) -> Self {
        Self::build(
            ErrorKind::NeedsVerification,
            None,
            None,
            Vec::new(),
            message.into(),
        )
    }

    /// 跨 IPC 的可序列化载荷（结构化结果里需要带上错误时用）。
    pub fn payload(&self) -> AppErrorPayload {
        let mut params = BTreeMap::new();
        for (key, value) in &self.params {
            params.insert((*key).to_string(), value.clone());
        }
        AppErrorPayload {
            kind: self.kind,
            code: self.code.map(str::to_string),
            params: Some(params),
            cause: self.cause.clone(),
            related: (!self.related.is_empty()).then(|| self.related.clone()),
            message: self.message.clone(),
        }
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
        Self::coded(
            ErrorKind::Environment,
            "errors.error.io",
            "IO 错误：{error}",
            &[("error", error.to_string())],
        )
    }
}

impl From<serde_json::Error> for AppError {
    fn from(error: serde_json::Error) -> Self {
        Self::coded(
            ErrorKind::Internal,
            "errors.error.json",
            "JSON 错误：{error}",
            &[("error", error.to_string())],
        )
    }
}

impl serde::Serialize for AppError {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        self.payload().serialize(serializer)
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
