//! AI 提交信息命令与流式请求（M9 规格 E–G 节）。
//!
//! 前端只拿请求 ID、增量文本与结构化错误；密钥不出 Rust 层，
//! 请求与返回内容不写进命令日志或诊断信息。

use std::time::Duration;

use futures_util::StreamExt;
use serde::Serialize;
use tauri::{Emitter, State};

use crate::commands::AppState;
use crate::error::{AppError, AppErrorPayload, ErrorKind, Result};

use super::payload::AiCommitMode;
use super::{delta_content, sanitize, sse_data, StreamFilter};


#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiStatus {
    /// 服务地址与模型名都非空才算已配置。
    pub configured: bool,
    pub base_url: String,
    pub model: String,
    pub language: super::AiLanguage,
    /// 提示词为空时返回默认提示词，便于界面直接编辑。
    pub prompt: String,
    pub default_prompt: String,
    pub key_set: bool,
    /// 当前地址是否已确认发送。
    pub confirmed: bool,
    /// 明文 http 且不是本机地址：确认弹窗要提示连接未加密。
    pub insecure: bool,
}

fn registry_lock<'a>(state: &'a State<'_, AppState>) -> Result<std::sync::MutexGuard<'a, crate::settings::Registry>> {
    state.registry.lock().map_err(|_| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.ai.settingsLock",
            "读取设置失败",
            &[],
        )
    })
}

fn notify_config_changed(state: &State<'_, AppState>) {
    if let Some(app) = state.app.get() {
        let _ = app.emit("revia://ai-config-changed", ());
    }
}

#[tauri::command]
pub fn ai_status(state: State<'_, AppState>) -> Result<AiStatus> {
    let settings = registry_lock(&state)?.settings().clone();
    let key_set = super::key(&settings.ai_base_url)?.is_some();
    Ok(AiStatus {
        configured: !settings.ai_base_url.is_empty() && !settings.ai_model.is_empty(),
        insecure: super::insecure_remote(&settings.ai_base_url),
        confirmed: settings
            .ai_confirmed_urls
            .iter()
            .any(|url| url == &settings.ai_base_url),
        prompt: if settings.ai_prompt.trim().is_empty() {
            super::DEFAULT_PROMPT.to_string()
        } else {
            settings.ai_prompt.clone()
        },
        default_prompt: super::DEFAULT_PROMPT.to_string(),
        base_url: settings.ai_base_url,
        model: settings.ai_model,
        language: settings.ai_language,
        key_set,
    })
}

/// 写入密钥；空字符串等于清除。只存系统凭据库，并绑定当前服务地址。
#[tauri::command]
pub fn ai_set_key(state: State<'_, AppState>, key: String) -> Result<AiStatus> {
    let base_url = current_base_url(&state)?;
    if base_url.is_empty() {
        return Err(super::not_configured());
    }
    if key.trim().is_empty() {
        super::clear_key(&base_url)?;
    } else {
        super::save_key(&base_url, key.trim())?;
    }
    notify_config_changed(&state);
    ai_status(state)
}

#[tauri::command]
pub fn ai_clear_key(state: State<'_, AppState>) -> Result<AiStatus> {
    let base_url = current_base_url(&state)?;
    super::clear_key(&base_url)?;
    notify_config_changed(&state);
    ai_status(state)
}

/// 记下当前服务地址已经确认发送（第一次使用前由界面弹窗确认）。
#[tauri::command]
pub fn ai_confirm(state: State<'_, AppState>) -> Result<AiStatus> {
    {
        let mut registry = registry_lock(&state)?;
        registry.confirm_ai_url()?;
    }
    notify_config_changed(&state);
    ai_status(state)
}

fn current_base_url(state: &State<'_, AppState>) -> Result<String> {
    Ok(registry_lock(state)?.settings().ai_base_url.clone())
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiGenerateRequest {
    pub worktree_id: String,
    pub mode: AiCommitMode,
    /// 前端事先生成的请求 ID：事件与取消从一开始就能对上，不依赖命令返回。
    pub request_id: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiDelta {
    request_id: String,
    text: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiDone {
    request_id: String,
    text: String,
    cancelled: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiFailed {
    request_id: String,
    error: AppErrorPayload,
}

/// 开始一次生成：请求 ID 由前端给出，结果通过事件流式交付。
///
/// 先登记 ID 再整理内容；整理完、发请求前再查一次取消，取消早于登记时直接放弃，
/// 保证用户点了取消后改动不会被上传。
#[tauri::command]
pub async fn ai_generate(
    state: State<'_, AppState>,
    request: AiGenerateRequest,
) -> Result<()> {
    if !super::valid_request_id(&request.request_id) {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.ai.unavailable",
            "AI 请求暂时不可用",
            &[],
        ));
    }
    let id = request.request_id.clone();
    let Some(mut cancel) = state.ai.register(&id) else {
        return Ok(());
    };
    let outcome = prepare_and_send(&state, request, &id, &mut cancel).await;
    if outcome.is_err() {
        state.ai.finish(&id);
    }
    outcome.map(|send| {
        if let Some(send) = send {
            send(cancel);
        }
    })
}

type SendRequest = Box<dyn FnOnce(tokio::sync::oneshot::Receiver<()>) + Send>;

/// 读取配置并整理发送内容；已被取消时返回 `None`，不发请求。
async fn prepare_and_send(
    state: &State<'_, AppState>,
    request: AiGenerateRequest,
    id: &str,
    cancel: &mut tokio::sync::oneshot::Receiver<()>,
) -> Result<Option<SendRequest>> {
    let (base_url, model, language, prompt, subject_limit, confirmed) = {
        let registry = registry_lock(&state)?;
        let settings = registry.settings();
        (
            settings.ai_base_url.clone(),
            settings.ai_model.clone(),
            settings.ai_language,
            settings.ai_prompt.clone(),
            settings.commit_subject_limit,
            settings
                .ai_confirmed_urls
                .iter()
                .any(|url| url == &settings.ai_base_url),
        )
    };
    if base_url.is_empty() || model.is_empty() {
        return Err(super::not_configured());
    }
    if !confirmed {
        return Err(super::url_not_confirmed());
    }
    let key = super::key(&base_url)?;
    let session = state.session(&request.worktree_id)?;
    let store = std::sync::Arc::clone(&state.changelists);
    let mode = request.mode;
    let content = tauri::async_runtime::spawn_blocking(move || {
        super::payload::build(&session, &store, &mode, &prompt, language, subject_limit)
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.ai.payload",
            "整理发送内容失败",
            &[],
        )
        .cause(AppError::internal(error.to_string()))
    })??;

    // 整理内容期间用户可能已经取消：此时不发请求。
    if !matches!(cancel.try_recv(), Err(tokio::sync::oneshot::error::TryRecvError::Empty)) {
        state.ai.finish(id);
        return Ok(None);
    }
    let app = state.app.get().cloned().ok_or_else(|| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.ai.unavailable",
            "AI 请求暂时不可用",
            &[],
        )
    })?;
    let manager = std::sync::Arc::clone(&state.ai);
    let request_id = id.to_string();
    Ok(Some(Box::new(move |cancel| {
    tauri::async_runtime::spawn(async move {
        let outcome = stream(&app, &request_id, &base_url, &model, key.as_deref(), &content, cancel).await;
        match outcome {
            Ok(Outcome::Text(text)) if !text.trim().is_empty() => {
                let _ = app.emit(
                    "revia://ai-done",
                    AiDone { request_id: request_id.clone(), text, cancelled: false },
                );
            }
            Ok(Outcome::Text(_)) => {
                let _ = app.emit(
                    "revia://ai-error",
                    AiFailed { request_id: request_id.clone(), error: super::empty_error().payload() },
                );
            }
            Ok(Outcome::Cancelled) => {
                let _ = app.emit(
                    "revia://ai-done",
                    AiDone { request_id: request_id.clone(), text: String::new(), cancelled: true },
                );
            }
            Err(error) => {
                let _ = app.emit(
                    "revia://ai-error",
                    AiFailed { request_id: request_id.clone(), error: error.payload() },
                );
            }
        }
        manager.finish(&request_id);
    });
    })))
}

#[tauri::command]
pub fn ai_cancel(state: State<'_, AppState>, request_id: String) -> bool {
    state.ai.cancel(&request_id)
}

#[derive(Debug, Clone)]
pub struct AiStreamConfig {
    pub base_url: String,
    pub model: String,
    pub key: Option<String>,
    pub connect_timeout: Duration,
    pub chunk_timeout: Duration,
}

#[derive(Debug)]
enum Outcome {
    Text(String),
    Cancelled,
}

async fn stream(
    app: &tauri::AppHandle,
    request_id: &str,
    base_url: &str,
    model: &str,
    key: Option<&str>,
    content: &str,
    cancel: tokio::sync::oneshot::Receiver<()>,
) -> Result<Outcome> {
    let app_handle = app.clone();
    let id = request_id.to_string();
    let config = AiStreamConfig {
        base_url: base_url.to_string(),
        model: model.to_string(),
        key: key.map(str::to_string),
        connect_timeout: super::CONNECT_TIMEOUT,
        chunk_timeout: super::CHUNK_TIMEOUT,
    };
    stream_inner(
        &config,
        content,
        cancel,
        move |text| {
            let _ = app_handle.emit(
                "revia://ai-delta",
                AiDelta {
                    request_id: id.clone(),
                    text,
                },
            );
        },
    )
    .await
}

async fn stream_inner(
    config: &AiStreamConfig,
    content: &str,
    mut cancel: tokio::sync::oneshot::Receiver<()>,
    mut on_delta: impl FnMut(String) + Send,
) -> Result<Outcome> {
    let client = reqwest::Client::builder()
        .connect_timeout(config.connect_timeout)
        .build()
        .map_err(|error| super::network_error(error.to_string()))?;
    let body = serde_json::json!({
        "model": config.model,
        "stream": true,
        "messages": [{ "role": "user", "content": content }],
    });
    let mut builder = client.post(super::endpoint(&config.base_url)).json(&body);
    if let Some(key) = &config.key {
        builder = builder.header("Authorization", format!("Bearer {key}"));
    }
    let response = tokio::select! {
        _ = &mut cancel => return Ok(Outcome::Cancelled),
        response = builder.send() => response.map_err(|error| {
            if error.is_timeout() { super::timeout_error() }
            else { super::network_error(super::redact(&error.to_string(), config.key.as_deref())) }
        })?,
    };
    let status = response.status();
    if !status.is_success() {
        let detail = tokio::time::timeout(Duration::from_secs(15), response.text())
            .await
            .ok()
            .and_then(std::result::Result::ok)
            .unwrap_or_default();
        return Err(super::status_error(status.as_u16(), &detail, config.key.as_deref()));
    }

    let mut stream = response.bytes_stream();
    let mut buffer: Vec<u8> = Vec::new();
    let mut filter = StreamFilter::default();
    'read: loop {
        let chunk = tokio::select! {
            _ = &mut cancel => return Ok(Outcome::Cancelled),
            chunk = tokio::time::timeout(config.chunk_timeout, stream.next()) => match chunk {
                Err(_) => return Err(super::timeout_error()),
                Ok(None) => break,
                Ok(Some(Err(error))) => {
                    return Err(super::network_error(super::redact(&error.to_string(), config.key.as_deref())))
                }
                Ok(Some(Ok(bytes))) => bytes,
            },
        };
        buffer.extend_from_slice(&chunk);
        while let Some(position) = buffer.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = buffer.drain(..=position).collect();
            let line = String::from_utf8_lossy(&line).into_owned();
            let Some(data) = sse_data(&line) else {
                continue;
            };
            if data == "[DONE]" {
                break 'read;
            }
            let Ok(value) = serde_json::from_str::<serde_json::Value>(data) else {
                continue;
            };
            if let Some(delta) = delta_content(&value) {
                if let Some(text) = filter.push(delta) {
                    on_delta(text);
                }
            }
        }
    }
    let text = filter.finish();
    Ok(Outcome::Text(sanitize(&text)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;

    #[tokio::test]
    async fn stream_sse_success_with_think_stripped() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let base_url = format!("http://127.0.0.1:{port}/v1");

        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut req = [0u8; 2048];
            let read_bytes = stream.read(&mut req).unwrap();
            let req_str = String::from_utf8_lossy(&req[..read_bytes]);
            assert!(req_str.contains("POST /v1/chat/completions"));
            assert!(req_str.contains("authorization: Bearer test-key"));
            // S13: 请求体包含 stream: true，且不发送 temperature
            assert!(req_str.contains("\"stream\":true"));
            assert!(!req_str.contains("\"temperature\""));

            let sse_body = concat!(
                "HTTP/1.1 200 OK\r\n",
                "Content-Type: text/event-stream\r\n",
                "Connection: close\r\n\r\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\"feat: add foo\\n\\n\",\"reasoning_content\":\"hidden think\"}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\"<think>ignore</think>bar details\"}}]}\n\n",
                "data: [DONE]\n\n"
            );
            stream.write_all(sse_body.as_bytes()).unwrap();
            stream.flush().unwrap();
        });

        let (_tx, rx) = tokio::sync::oneshot::channel();
        let mut deltas = Vec::new();
        let config = AiStreamConfig {
            base_url,
            model: "gpt-test".into(),
            key: Some("test-key".into()),
            connect_timeout: Duration::from_secs(5),
            chunk_timeout: Duration::from_secs(5),
        };
        let outcome = stream_inner(
            &config,
            "some content",
            rx,
            |delta| deltas.push(delta),
        )
        .await
        .unwrap();

        match outcome {
            Outcome::Text(text) => {
                assert_eq!(text, "feat: add foo\n\nbar details");
                assert!(!text.contains("hidden think"));
                assert!(!text.contains("ignore"));
            }
            Outcome::Cancelled => panic!("should not be cancelled"),
        }
        assert!(!deltas.is_empty());
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn stream_without_key_omits_authorization() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let base_url = format!("http://127.0.0.1:{port}/v1");

        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut req = [0u8; 2048];
            let read_bytes = stream.read(&mut req).unwrap();
            let req_str = String::from_utf8_lossy(&req[..read_bytes]);
            assert!(!req_str.to_lowercase().contains("authorization:"));

            let sse_body = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"msg\"}}]}\n\ndata: [DONE]\n\n";
            stream.write_all(sse_body.as_bytes()).unwrap();
            stream.flush().unwrap();
        });

        let (_tx, rx) = tokio::sync::oneshot::channel();
        let config = AiStreamConfig {
            base_url,
            model: "gpt-test".into(),
            key: None,
            connect_timeout: Duration::from_secs(5),
            chunk_timeout: Duration::from_secs(5),
        };
        let outcome = stream_inner(
            &config,
            "content",
            rx,
            |_| {},
        )
        .await
        .unwrap();

        match outcome {
            Outcome::Text(text) => assert_eq!(text, "msg"),
            Outcome::Cancelled => panic!("cancelled"),
        }
        handle.join().unwrap();
    }

    // 多线程运行时：等服务端线程时，连接任务仍能被驱动去关闭套接字（与生产的 Tauri 运行时一致）。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn stream_cancel_triggers_cancellation() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let base_url = format!("http://127.0.0.1:{port}/v1");

        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = stream.set_read_timeout(Some(Duration::from_millis(300)));
            let mut req = [0u8; 1024];
            let _ = stream.read(&mut req);
            let sse_header = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"start\"}}]}\n\n";
            let _ = stream.write_all(sse_header.as_bytes());
            let _ = stream.flush();
            // S13：客户端取消后必须关闭连接——服务端读到 EOF（或连接被重置），而不是一直等到超时。
            let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
            let mut buf = [0u8; 64];
            match stream.read(&mut buf) {
                Ok(0) => true,
                Err(error) => error.kind() == std::io::ErrorKind::ConnectionReset,
                Ok(_) => false,
            }
        });

        let (tx, rx) = tokio::sync::oneshot::channel();
        let tx = Arc::new(std::sync::Mutex::new(Some(tx)));
        let tx_clone = Arc::clone(&tx);

        let config = AiStreamConfig {
            base_url,
            model: "gpt-test".into(),
            key: None,
            connect_timeout: Duration::from_secs(5),
            chunk_timeout: Duration::from_secs(5),
        };
        let outcome = stream_inner(
            &config,
            "content",
            rx,
            move |_| {
                if let Ok(mut guard) = tx_clone.lock() {
                    if let Some(sender) = guard.take() {
                        let _ = sender.send(());
                    }
                }
            },
        )
        .await
        .unwrap();

        assert!(matches!(outcome, Outcome::Cancelled));
        assert!(handle.join().unwrap(), "connection must be closed after cancel");
    }

    #[tokio::test]
    async fn stream_status_error_mappings() {
        for (status_code, error_class, expected_code) in [
            (401, "Unauthorized", "errors.ai.auth"),
            (429, "Too Many Requests", "errors.ai.rateLimited"),
            (404, "Not Found", "errors.ai.modelMissing"),
            (500, "Internal Error", "errors.ai.server"),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let base_url = format!("http://127.0.0.1:{port}/v1");

            let handle = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut req = [0u8; 1024];
                let _ = stream.read(&mut req);
                let response = format!(
                    "HTTP/1.1 {status_code} {error_class}\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nerror occurred with sk-secret-123\r\n"
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            });

            let (_tx, rx) = tokio::sync::oneshot::channel();
            let config = AiStreamConfig {
                base_url,
                model: "gpt-test".into(),
                key: Some("sk-secret-123".into()),
                connect_timeout: Duration::from_secs(5),
                chunk_timeout: Duration::from_secs(5),
            };
            let err = stream_inner(
                &config,
                "content",
                rx,
                |_| {},
            )
            .await
            .unwrap_err();

            assert_eq!(err.code, Some(expected_code));
            assert!(!format!("{err:?}").contains("sk-secret-123"));
            handle.join().unwrap();
        }
    }

    #[tokio::test]
    async fn stream_chunk_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let base_url = format!("http://127.0.0.1:{port}/v1");

        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut req = [0u8; 1024];
            let _ = stream.read(&mut req);
            let response = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"part1\"}}]}\n\n";
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
            std::thread::sleep(Duration::from_millis(150));
        });

        let (_tx, rx) = tokio::sync::oneshot::channel();
        let config = AiStreamConfig {
            base_url,
            model: "gpt-test".into(),
            key: None,
            connect_timeout: Duration::from_secs(5),
            chunk_timeout: Duration::from_millis(50),
        };
        let err = stream_inner(
            &config,
            "content",
            rx,
            |_| {},
        )
        .await
        .unwrap_err();

        assert_eq!(err.code, Some("errors.ai.timeout"));
        handle.join().unwrap();
    }

    #[test]
    fn ai_manager_registers_and_cancels_before_response() {
        let manager = super::super::AiManager::default();
        let rx = manager.register("req-early").expect("registered");
        assert!(manager.cancel("req-early"));
        // 验证接收端立即收到取消信号
        assert!(rx.blocking_recv().is_ok());
    }

    /// S12：取消早于后端登记（前端先有 ID）时，登记被拒绝，请求不会发出。
    #[test]
    fn cancel_before_register_prevents_request() {
        let manager = super::super::AiManager::default();
        assert!(!manager.cancel("req-before"));
        assert!(manager.register("req-before").is_none());
        // 早到的取消只作用一次，同一 ID 之后可以重新登记。
        assert!(manager.register("req-before").is_some());
    }

    /// 早到的取消有上限，不会无限增长。
    #[test]
    fn early_cancels_are_bounded() {
        let manager = super::super::AiManager::default();
        for index in 0..200 {
            manager.cancel(&format!("req-{index}"));
        }
        assert!(manager.register("req-0").is_some());
        assert!(manager.register("req-199").is_none());
    }

    #[test]
    fn request_ids_are_validated() {
        assert!(super::super::valid_request_id("8f0c6a0e-1b2c-4d5e-9f00-112233445566"));
        assert!(!super::super::valid_request_id(""));
        assert!(!super::super::valid_request_id("../etc"));
        assert!(!super::super::valid_request_id(&"a".repeat(65)));
    }
}
