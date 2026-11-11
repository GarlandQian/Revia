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

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiKeyRequest {
    pub key: String,
    /// 界面操作时看到并已提交的服务地址；与已保存地址不一致时拒绝。
    pub base_url: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiServiceRequest {
    pub base_url: String,
}

/// 写入密钥；空字符串等于清除。只存系统凭据库，并绑定当前服务地址。
///
/// 校验地址与凭据写入都在设置锁内完成：并发的地址更新不会让密钥落到旧地址。
#[tauri::command]
pub fn ai_set_key(state: State<'_, AppState>, request: AiKeyRequest) -> Result<AiStatus> {
    {
        let registry = registry_lock(&state)?;
        let base_url = registry.settings().ai_base_url.clone();
        if base_url.is_empty() {
            return Err(super::not_configured());
        }
        if base_url != request.base_url {
            return Err(super::service_changed());
        }
        if request.key.trim().is_empty() {
            super::clear_key(&base_url)?;
        } else {
            super::save_key(&base_url, request.key.trim())?;
        }
    }
    notify_config_changed(&state);
    ai_status(state)
}

#[tauri::command]
pub fn ai_clear_key(state: State<'_, AppState>, request: AiServiceRequest) -> Result<AiStatus> {
    {
        let registry = registry_lock(&state)?;
        let base_url = registry.settings().ai_base_url.clone();
        if base_url.is_empty() {
            return Err(super::not_configured());
        }
        if base_url != request.base_url {
            return Err(super::service_changed());
        }
        super::clear_key(&base_url)?;
    }
    notify_config_changed(&state);
    ai_status(state)
}

/// 读取当前服务提供的模型列表。
///
/// 只依赖已保存的非空服务地址，不要求已选模型或已配置；地址必须与界面看到的一致，
/// 避免把密钥发往刚被其他操作改掉的地址。
#[tauri::command]
pub async fn ai_list_models(
    state: State<'_, AppState>,
    request: AiServiceRequest,
) -> Result<super::models::AiModelList> {
    let expected = super::normalize_base_url(&request.base_url)?;
    let (base_url, key) = {
        let registry = registry_lock(&state)?;
        let saved = registry.settings().ai_base_url.clone();
        if saved.is_empty() {
            return Err(super::not_configured());
        }
        if saved != expected {
            return Err(super::service_changed());
        }
        let key = super::key(&saved)?;
        (saved, key)
    };
    let models = super::models::fetch(&super::models::ModelListConfig {
        base_url: base_url.clone(),
        key,
        timeout: super::models::MODELS_TIMEOUT,
        max_bytes: super::models::MAX_MODELS_BYTES,
    })
    .await?;
    Ok(super::models::AiModelList { base_url, models })
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

    use tauri::Manager;

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

    fn test_app(dir: &std::path::Path) -> tauri::App<tauri::test::MockRuntime> {
        tauri::test::mock_builder()
            .manage(AppState::new(dir.to_path_buf()))
            .invoke_handler(tauri::generate_handler![
                super::ai_status,
                super::ai_set_key,
                super::ai_clear_key,
                super::ai_list_models
            ])
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap()
    }

    fn test_webview(
        app: &tauri::App<tauri::test::MockRuntime>,
    ) -> tauri::WebviewWindow<tauri::test::MockRuntime> {
        tauri::WebviewWindowBuilder::new(app, "main", Default::default())
            .build()
            .unwrap()
    }

    fn invoke(
        webview: &tauri::WebviewWindow<tauri::test::MockRuntime>,
        cmd: &str,
        body: serde_json::Value,
    ) -> std::result::Result<serde_json::Value, serde_json::Value> {
        tauri::test::get_ipc_response(
            webview,
            tauri::webview::InvokeRequest {
                cmd: cmd.into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                url: "tauri://localhost".parse().unwrap(),
                body: tauri::ipc::InvokeBody::Json(body),
                headers: Default::default(),
                invoke_key: tauri::test::INVOKE_KEY.to_string(),
            },
        )
        .map(|body| body.deserialize::<serde_json::Value>().unwrap())
    }

    fn seed_settings(dir: &std::path::Path, base_url: &str) {
        let log = std::sync::Arc::new(crate::git::CommandLog::default());
        let mut registry = crate::settings::Registry::load(dir.to_path_buf(), log);
        registry
            .update_settings(&crate::settings::SettingsPatch {
                ai_base_url: Some(base_url.to_string()),
                ai_model: Some("model-a".into()),
                ..Default::default()
            })
            .unwrap();
    }

    fn error_code(value: &serde_json::Value) -> Option<&str> {
        value.get("code").and_then(|code| code.as_str())
    }

    #[test]
    fn ai_set_key_rejects_stale_address_without_writing_credentials() {
        let _credentials = super::super::test_credentials::TestCredentials::install();
        let dir = tempfile::tempdir().unwrap();
        let url_a = "https://api.stale.test/v1";
        let url_b = "https://api.other.test/v1";
        seed_settings(dir.path(), url_a);
        super::super::save_key(url_a, "sk-old").unwrap();
        let app = test_app(dir.path());
        let webview = test_webview(&app);

        let response = invoke(
            &webview,
            "ai_set_key",
            serde_json::json!({ "request": { "key": "sk-new", "baseUrl": url_b } }),
        );
        let error = response.unwrap_err();
        assert_eq!(error_code(&error), Some("errors.ai.serviceChanged"));
        assert_eq!(super::super::key(url_a).unwrap().as_deref(), Some("sk-old"));
        assert_eq!(super::super::key(url_b).unwrap(), None);
    }

    #[test]
    fn ai_clear_key_rejects_stale_address() {
        let _credentials = super::super::test_credentials::TestCredentials::install();
        let dir = tempfile::tempdir().unwrap();
        let url_a = "https://api.clear.test/v1";
        seed_settings(dir.path(), url_a);
        super::super::save_key(url_a, "sk-old").unwrap();
        let app = test_app(dir.path());
        let webview = test_webview(&app);

        let response = invoke(
            &webview,
            "ai_clear_key",
            serde_json::json!({ "request": { "baseUrl": "https://api.other.test/v1" } }),
        );
        let error = response.unwrap_err();
        assert_eq!(error_code(&error), Some("errors.ai.serviceChanged"));
        assert_eq!(super::super::key(url_a).unwrap().as_deref(), Some("sk-old"));
    }

    #[test]
    fn ai_list_models_rejects_stale_address_without_network() {
        let _credentials = super::super::test_credentials::TestCredentials::install();
        let dir = tempfile::tempdir().unwrap();
        let url_a = "https://api.list.test/v1";
        seed_settings(dir.path(), url_a);
        let app = test_app(dir.path());
        let webview = test_webview(&app);

        let response = invoke(
            &webview,
            "ai_list_models",
            serde_json::json!({ "request": { "baseUrl": "https://api.other.test/v1" } }),
        );
        let error = response.unwrap_err();
        assert_eq!(error_code(&error), Some("errors.ai.serviceChanged"));
    }

    #[test]
    fn ai_list_models_reads_models_with_saved_key() {
        let _credentials = super::super::test_credentials::TestCredentials::install();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let base_url = format!("http://127.0.0.1:{port}/v1");
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 2048];
            let read = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            assert!(request.starts_with("GET /v1/models "), "{request}");
            assert!(request.to_lowercase().contains("authorization: bearer sk-list"));
            let body = r#"{"data":[{"id":"m1"},{"id":"m2"},{"id":"m1"}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
            stream.flush().unwrap();
        });

        let dir = tempfile::tempdir().unwrap();
        seed_settings(dir.path(), &base_url);
        super::super::save_key(&base_url, "sk-list").unwrap();
        let app = test_app(dir.path());
        let webview = test_webview(&app);

        let response = invoke(
            &webview,
            "ai_list_models",
            serde_json::json!({ "request": { "baseUrl": base_url } }),
        )
        .unwrap();
        assert_eq!(response["baseUrl"], base_url);
        assert_eq!(response["models"], serde_json::json!(["m1", "m2"]));
        handle.join().unwrap();
    }

    #[test]
    fn ai_set_key_holds_settings_lock_while_writing_credentials() {
        let credentials = super::super::test_credentials::TestCredentials::install();
        let dir = tempfile::tempdir().unwrap();
        let base_url = "https://api.lock.test/v1";
        seed_settings(dir.path(), base_url);
        let app = test_app(dir.path());
        let webview = test_webview(&app);

        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        credentials.set_hook(move || {
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
        });

        let request = serde_json::json!({ "request": { "key": "sk-lock", "baseUrl": base_url } });
        let handle = std::thread::spawn(move || invoke(&webview, "ai_set_key", request));
        started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("credential write did not start");
        let state = app.state::<AppState>();
        match state.registry.try_lock() {
            Err(std::sync::TryLockError::WouldBlock) => {}
            other => panic!("settings lock not held during credential write: {}", other.is_ok()),
        }
        release_tx.send(()).unwrap();
        let response = handle.join().unwrap();
        assert!(response.is_ok(), "{response:?}");
        assert_eq!(super::super::key(base_url).unwrap().as_deref(), Some("sk-lock"));
    }
}
