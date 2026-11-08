//! AI 生成提交信息（M9 规格 E–G 节，Q213、Q215、Q219、Q221）。
//!
//! 只支持一个 OpenAI 兼容服务：地址与模型存在 Revia 设置里，密钥只存系统凭据库，
//! 并按服务地址绑定。请求在 Rust 层发出，前端拿不到密钥；请求与返回内容
//! 不进 Git 命令日志和诊断信息。

pub mod commands;
pub mod payload;

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, ErrorKind, Result};

/// 默认提示词：可整段改写；固定规则另行追加，不在这里（规格 E、F 节）。
pub const DEFAULT_PROMPT: &str = "你是 Git 提交信息助手。根据下面的改动内容写一条提交信息：首行简洁概括这次改动；需要时在正文说明原因和影响；参考最近提交的写法。";

/// 单个文件与总量的字符上限（规格 F 节；实现常量，不进设置）。
pub const MAX_FILE_CHARS: usize = 6_000;
pub const MAX_TOTAL_CHARS: usize = 24_000;
/// 文件清单最多列出多少项。
pub const MAX_LISTED_FILES: usize = 300;
/// 附带最近提交标题的条数。
pub const RECENT_LIMIT: usize = 10;
/// 连接超时与相邻两段数据的间隔上限（规格 E 节）。
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
pub const CHUNK_TIMEOUT: Duration = Duration::from_secs(60);

/// 密钥在系统凭据库里的 service 名；用户名是服务地址（Q221：地址即服务身份）。
const KEYRING_SERVICE: &str = "io.github.garlandqian.revia.ai";

#[cfg(test)]
static TEST_CREDENTIALS: std::sync::Mutex<Option<std::collections::HashMap<String, String>>> =
    std::sync::Mutex::new(None);

#[cfg(test)]
pub fn enable_test_credentials() {
    let mut guard = TEST_CREDENTIALS.lock().unwrap();
    *guard = Some(std::collections::HashMap::new());
}

#[cfg(test)]
pub fn disable_test_credentials() {
    let mut guard = TEST_CREDENTIALS.lock().unwrap();
    *guard = None;
}

/// 生成语言（Q213）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum AiLanguage {
    /// 跟随仓库最近提交标题的语言。
    #[default]
    Repo,
    Chinese,
    English,
}

/// 正在进行的生成请求：取消时关闭连接（规格 E 节）。
#[derive(Default)]
pub struct AiManager {
    state: Mutex<AiRequests>,
}

#[derive(Default)]
struct AiRequests {
    cancels: BTreeMap<String, tokio::sync::oneshot::Sender<()>>,
    /// 注册之前就收到的取消（前端先生成请求 ID，取消可能早于后端注册）。
    cancelled_early: std::collections::VecDeque<String>,
}

/// 早到的取消最多记住这么多条，防止无限增长。
const EARLY_CANCEL_LIMIT: usize = 64;

impl AiManager {
    /// 登记请求。该 ID 在登记之前已被取消时返回 `None`，调用方不得再发请求。
    pub fn register(&self, id: &str) -> Option<tokio::sync::oneshot::Receiver<()>> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(position) = state.cancelled_early.iter().position(|early| early == id) {
            state.cancelled_early.remove(position);
            return None;
        }
        let (sender, receiver) = tokio::sync::oneshot::channel();
        state.cancels.insert(id.to_string(), sender);
        Some(receiver)
    }

    /// 取消请求；尚未登记时记下，登记时直接拒绝。
    pub fn cancel(&self, id: &str) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(sender) = state.cancels.remove(id) {
            return sender.send(()).is_ok();
        }
        if !state.cancelled_early.iter().any(|early| early == id) {
            state.cancelled_early.push_back(id.to_string());
            while state.cancelled_early.len() > EARLY_CANCEL_LIMIT {
                state.cancelled_early.pop_front();
            }
        }
        false
    }

    pub fn finish(&self, id: &str) {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .cancels
            .remove(id);
    }
}

/// 前端生成的请求 ID：只允许字母、数字和连字符，长度 1–64。
pub fn valid_request_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
}

/// 规范化服务地址：只允许 http(s)，去掉末尾斜杠。
pub fn normalize_base_url(value: &str) -> Result<String> {
    let trimmed = value.trim().trim_end_matches('/').to_string();
    if trimmed.is_empty() {
        return Ok(trimmed);
    }
    if trimmed.len() > 2048 || !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.ai.urlInvalid",
            "服务地址必须以 http:// 或 https:// 开头",
            &[],
        ));
    }
    Ok(trimmed)
}

/// 请求地址：已以 `/chat/completions` 结尾时原样使用。
pub fn endpoint(base_url: &str) -> String {
    if base_url.ends_with("/chat/completions") {
        base_url.to_string()
    } else {
        format!("{base_url}/chat/completions")
    }
}

/// 地址是否非本机的明文 http（确认弹窗要提示连接未加密）。
pub fn insecure_remote(base_url: &str) -> bool {
    let Some(rest) = base_url.strip_prefix("http://") else {
        return false;
    };
    let host = rest.split('/').next().unwrap_or("");
    let host = host.rsplit('@').next().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host);
    !matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1")
}

fn keyring_entry(base_url: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, base_url).map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.ai.credentialStore",
            "无法访问系统凭据库：{cause}",
            &[],
        )
        .cause(AppError::internal(error.to_string()))
    })
}

pub fn save_key(base_url: &str, key: &str) -> Result<()> {
    if base_url.is_empty() {
        return Ok(());
    }
    #[cfg(test)]
    {
        if let Ok(mut guard) = TEST_CREDENTIALS.lock() {
            if let Some(map) = guard.as_mut() {
                map.insert(base_url.to_string(), key.to_string());
                return Ok(());
            }
        }
    }
    keyring_entry(base_url)?
        .set_password(key)
        .map_err(|error| credential_error(error.to_string()))
}

pub fn key(base_url: &str) -> Result<Option<String>> {
    if base_url.is_empty() {
        return Ok(None);
    }
    #[cfg(test)]
    {
        if let Ok(guard) = TEST_CREDENTIALS.lock() {
            if let Some(map) = guard.as_ref() {
                return Ok(map.get(base_url).cloned());
            }
        }
    }
    match keyring_entry(base_url)?.get_password() {
        Ok(value) if value.is_empty() => Ok(None),
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(credential_error(error.to_string())),
    }
}

pub fn clear_key(base_url: &str) -> Result<()> {
    if base_url.is_empty() {
        return Ok(());
    }
    #[cfg(test)]
    {
        if let Ok(mut guard) = TEST_CREDENTIALS.lock() {
            if let Some(map) = guard.as_mut() {
                map.remove(base_url);
                return Ok(());
            }
        }
    }
    match keyring_entry(base_url)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(credential_error(error.to_string())),
    }
}

fn credential_error(cause: String) -> AppError {
    AppError::coded(
        ErrorKind::Internal,
        "errors.ai.credentialStore",
        "无法访问系统凭据库",
        &[],
    )
    .cause(AppError::internal(cause))
}

pub fn not_configured() -> AppError {
    AppError::coded(
        ErrorKind::Rejected,
        "errors.ai.notConfigured",
        "还没有配置 AI 服务",
        &[],
    )
}

pub fn url_not_confirmed() -> AppError {
    AppError::coded(
        ErrorKind::Rejected,
        "errors.ai.urlNotConfirmed",
        "这个服务地址还没有确认发送",
        &[],
    )
}

pub fn timeout_error() -> AppError {
    AppError::coded(
        ErrorKind::Rejected,
        "errors.ai.timeout",
        "AI 服务响应超时",
        &[],
    )
}

pub fn network_error(cause: String) -> AppError {
    AppError::coded(
        ErrorKind::Rejected,
        "errors.ai.network",
        "无法连接 AI 服务",
        &[],
    )
    .cause(AppError::internal(redact(&cause, None)))
}

pub fn empty_error() -> AppError {
    AppError::coded(
        ErrorKind::Rejected,
        "errors.ai.empty",
        "AI 服务返回了空内容",
        &[],
    )
}

/// HTTP 状态映射为结构化错误；服务端说明里的密钥先去掉。
pub fn status_error(status: u16, body: &str, key: Option<&str>) -> AppError {
    let detail: String = redact(body.trim(), key).chars().take(400).collect();
    let (code, message) = match status {
        401 | 403 => ("errors.ai.auth", "认证失败，请检查密钥"),
        404 => ("errors.ai.modelMissing", "找不到模型或接口"),
        429 => ("errors.ai.rateLimited", "请求过于频繁，请稍后再试"),
        500..=599 => ("errors.ai.server", "AI 服务出错"),
        _ => ("errors.ai.server", "AI 服务返回了错误"),
    };
    AppError::coded(ErrorKind::Rejected, code, message, &[])
        .cause(AppError::internal(format!("HTTP {status}: {detail}")))
}

/// 去掉文本里可能出现的密钥。
pub fn redact(text: &str, key: Option<&str>) -> String {
    match key.filter(|value| !value.is_empty()) {
        Some(key) => text.replace(key, "***"),
        None => text.to_string(),
    }
}

/// 清理模型输出：去掉思考段与包裹全文的代码围栏。
pub fn sanitize(raw: &str) -> String {
    let mut text = strip_think(raw);
    let trimmed = text.trim();
    if let Some(rest) = trimmed.strip_prefix("```") {
        let rest = rest.split_once('\n').map_or("", |(_, body)| body);
        text = rest.trim_end().trim_end_matches("```").trim().to_string();
    } else {
        text = trimmed.to_string();
    }
    text.trim().to_string()
}

fn strip_think(raw: &str) -> String {
    let mut text = raw.to_string();
    loop {
        let lower = text.to_lowercase();
        let Some(start) = lower.find("<think") else {
            break;
        };
        let Some(open_end) = lower[start..].find('>').map(|offset| start + offset + 1) else {
            text.truncate(start);
            break;
        };
        match lower[open_end..].find("</think") {
            Some(offset) => {
                let close_end = lower[open_end + offset..]
                    .find('>')
                    .map(|end| open_end + offset + end + 1)
                    .unwrap_or(text.len());
                text.replace_range(start..close_end, "");
            }
            None => {
                text.truncate(start);
                break;
            }
        }
    }
    text
}

/// 流式清理：只在可见文本确实增长时给出新增片段。
#[derive(Default)]
pub struct StreamFilter {
    raw: String,
    visible: String,
}

impl StreamFilter {
    pub fn push(&mut self, delta: &str) -> Option<String> {
        self.raw.push_str(delta);
        let mut next = sanitize(&self.raw);
        // 末尾可能是半个思考标签：先不显示，等下一段补全后再决定去掉还是保留。
        let hold = partial_tag_len(&next);
        if hold > 0 {
            next.truncate(next.len() - hold);
            next = next.trim_end().to_string();
        }
        if next.len() > self.visible.len() && next.starts_with(&self.visible) {
            let suffix = next[self.visible.len()..].to_string();
            self.visible = next;
            Some(suffix)
        } else {
            None
        }
    }

    pub fn finish(&mut self) -> String {
        self.visible = sanitize(&self.raw);
        self.visible.clone()
    }
}

/// 结尾是半个 `<think>` / `</think>` 时返回需要压住的字节数。
fn partial_tag_len(text: &str) -> usize {
    let lower = text.to_lowercase();
    for tag in ["<think", "</think"] {
        for length in (1..tag.len()).rev() {
            if lower.ends_with(&tag[..length]) {
                return length;
            }
        }
    }
    0
}

/// SSE 的一行 `data:` 负载；注释、空行与事件名返回 None。
pub fn sse_data(line: &str) -> Option<&str> {
    line.trim_end_matches(['\r', '\n'])
        .strip_prefix("data:")
        .map(str::trim)
}

/// 从 `choices[0].delta.content` 取文本；思考字段（如 `reasoning_content`）不读。
pub fn delta_content(value: &serde_json::Value) -> Option<&str> {
    value
        .get("choices")?
        .get(0)?
        .get("delta")?
        .get("content")?
        .as_str()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_appends_once() {
        assert_eq!(
            endpoint("https://api.openai.com/v1"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            endpoint("http://localhost:11434/v1/chat/completions"),
            "http://localhost:11434/v1/chat/completions"
        );
    }

    #[test]
    fn normalize_rejects_non_http() {
        assert_eq!(normalize_base_url(" https://x.dev/v1/ ").unwrap(), "https://x.dev/v1");
        assert!(normalize_base_url("ftp://x.dev").is_err());
        assert_eq!(normalize_base_url("  ").unwrap(), "");
    }

    #[test]
    fn insecure_only_for_remote_plain_http() {
        assert!(!insecure_remote("http://localhost:11434/v1"));
        assert!(!insecure_remote("http://127.0.0.1:8080"));
        assert!(insecure_remote("http://example.com/v1"));
        assert!(!insecure_remote("https://example.com/v1"));
    }

    #[test]
    fn sanitize_strips_think_and_fences() {
        assert_eq!(sanitize("<think>secret</think>feat: add"), "feat: add");
        assert_eq!(sanitize("```\nfeat: add\n\nbody\n```"), "feat: add\n\nbody");
        assert_eq!(sanitize("  \nfeat: x\n  "), "feat: x");
    }

    #[test]
    fn stream_filter_emits_growth_only() {
        let mut filter = StreamFilter::default();
        assert_eq!(filter.push("feat: "), Some("feat:".into()));
        assert_eq!(filter.push("<thi"), None);
        assert_eq!(filter.push("nk>hidden</think>add"), Some(" add".into()));
        assert_eq!(filter.finish(), "feat: add");
    }

    #[test]
    fn sse_lines_and_delta() {
        assert_eq!(sse_data("data: {\"a\":1}\r\n"), Some("{\"a\":1}"));
        assert_eq!(sse_data(": ping"), None);
        let value: serde_json::Value =
            serde_json::from_str(r#"{"choices":[{"delta":{"content":"hi","reasoning_content":"x"}}]}"#)
                .unwrap();
        assert_eq!(delta_content(&value), Some("hi"));
    }

    #[test]
    fn status_errors_map_and_redact() {
        let error = status_error(401, "bad key sk-secret", Some("sk-secret"));
        assert_eq!(error.code, Some("errors.ai.auth"));
        assert!(!format!("{error:?}").contains("sk-secret"));
        assert_eq!(status_error(429, "", None).code, Some("errors.ai.rateLimited"));
        assert_eq!(status_error(404, "", None).code, Some("errors.ai.modelMissing"));
        assert_eq!(status_error(503, "", None).code, Some("errors.ai.server"));
    }

    #[test]
    fn credential_mock_write_clear_and_url_swap() {
        enable_test_credentials();
        let url_a = "https://api.a.com/v1";
        let url_b = "https://api.b.com/v1";

        save_key(url_a, "sk-secret-a").unwrap();
        assert_eq!(key(url_a).unwrap().as_deref(), Some("sk-secret-a"));
        assert_eq!(key(url_b).unwrap(), None);

        clear_key(url_a).unwrap();
        assert_eq!(key(url_a).unwrap(), None);

        let dir = tempfile::tempdir().unwrap();
        let log = std::sync::Arc::new(crate::git::CommandLog::default());
        let mut registry = crate::settings::Registry::load(dir.path().to_path_buf(), log);
        registry
            .update_settings(&crate::settings::SettingsPatch {
                ai_base_url: Some(url_a.to_string()),
                ai_model: Some("gpt-4".into()),
                ..Default::default()
            })
            .unwrap();
        save_key(url_a, "sk-secret-a").unwrap();
        registry.confirm_ai_url().unwrap();
        assert!(registry.settings().ai_confirmed_urls.contains(&url_a.to_string()));

        registry
            .update_settings(&crate::settings::SettingsPatch {
                ai_base_url: Some(url_b.to_string()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(key(url_a).unwrap(), None);
        assert_eq!(key(url_b).unwrap(), None);
        assert!(registry.settings().ai_confirmed_urls.is_empty());

        disable_test_credentials();
    }
}
