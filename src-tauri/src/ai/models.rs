//! 从当前 AI 服务获取可用模型列表（M11）。
//!
//! 只使用 OpenAI 兼容的 `GET {base}/models`；请求由 Rust 层发出，密钥不出 Rust。
//! 服务响应正文、请求头与密钥都不进入错误、日志或诊断，只返回模型名。

use std::time::Duration;

use futures_util::StreamExt;
use serde::Serialize;

use crate::error::Result;

/// 模型列表请求的整体超时。
pub const MODELS_TIMEOUT: Duration = Duration::from_secs(10);
/// 响应正文上限，防止异常服务返回超大内容。
pub const MAX_MODELS_BYTES: usize = 512 * 1024;
/// 最多返回的模型数。
pub const MAX_MODELS: usize = 500;
/// 单个模型名的长度上限。
const MAX_MODEL_ID_CHARS: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiModelList {
    pub base_url: String,
    pub models: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ModelListConfig {
    pub base_url: String,
    pub key: Option<String>,
    pub timeout: Duration,
    pub max_bytes: usize,
}

/// 模型列表地址：普通 base URL 追加 `/models`；以 `/chat/completions`
/// 结尾时先去掉该后缀（M11：保持原来的地址身份规则，不自动加 `/v1`）。
pub fn endpoint(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    let base = trimmed
        .strip_suffix("/chat/completions")
        .unwrap_or(trimmed);
    format!("{base}/models")
}

/// 解析 `data[].id`：忽略其他元数据，去重、稳定排序并过滤空白/无效项。
pub fn parse(body: &str) -> Result<Vec<String>> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|_| super::models_unavailable("invalid json"))?;
    let items = value
        .get("data")
        .and_then(|data| data.as_array())
        .ok_or_else(|| super::models_unavailable("missing data array"))?;
    let mut models = std::collections::BTreeSet::new();
    for item in items {
        let Some(id) = item.get("id").and_then(|value| value.as_str()) else {
            continue;
        };
        let id = id.trim();
        if id.is_empty()
            || id.chars().count() > MAX_MODEL_ID_CHARS
            || id.chars().any(char::is_control)
        {
            continue;
        }
        models.insert(id.to_string());
        if models.len() >= MAX_MODELS {
            break;
        }
    }
    Ok(models.into_iter().collect())
}

/// 读取当前服务提供的模型名。失败返回结构化错误，不包含响应正文。
pub async fn fetch(config: &ModelListConfig) -> Result<Vec<String>> {
    // 总超时从发请求前开始计时，覆盖建连、等待响应头与读取正文；
    // 只设置 connect_timeout 时，服务接受连接却不发响应头会无限等待。
    let deadline = tokio::time::Instant::now() + config.timeout;
    let client = reqwest::Client::builder()
        .connect_timeout(config.timeout)
        // 第二道保险：reqwest 自身的整体超时。
        .timeout(config.timeout)
        // 不跟随重定向：凭据只发往用户确认过的那个地址。
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| super::network_error(error.to_string()))?;
    let mut builder = client.get(endpoint(&config.base_url));
    if let Some(key) = &config.key {
        builder = builder.header("Authorization", format!("Bearer {key}"));
    }
    let response = tokio::time::timeout_at(deadline, builder.send())
        .await
        .map_err(|_| super::timeout_error())?
        .map_err(|error| {
            if error.is_timeout() {
                super::timeout_error()
            } else {
                super::network_error(super::redact(&error.to_string(), config.key.as_deref()))
            }
        })?;
    let status = response.status();
    if !status.is_success() {
        if (300..400).contains(&status.as_u16()) {
            return Err(super::models_unavailable(format!(
                "redirect not followed: HTTP {}",
                status.as_u16()
            )));
        }
        return Err(super::status_error(
            status.as_u16(),
            "",
            config.key.as_deref(),
        ));
    }

    let mut stream = response.bytes_stream();
    let mut body: Vec<u8> = Vec::new();
    loop {
        let next = tokio::time::timeout_at(deadline, stream.next()).await;
        match next {
            Err(_) => return Err(super::timeout_error()),
            Ok(None) => break,
            Ok(Some(Err(error))) => {
                if error.is_timeout() {
                    return Err(super::timeout_error());
                }
                return Err(super::network_error(super::redact(
                    &error.to_string(),
                    config.key.as_deref(),
                )));
            }
            Ok(Some(Ok(chunk))) => {
                if body.len() + chunk.len() > config.max_bytes {
                    return Err(super::models_unavailable("response too large"));
                }
                body.extend_from_slice(&chunk);
            }
        }
    }
    let text = String::from_utf8(body)
        .map_err(|_| super::models_unavailable("response is not utf-8"))?;
    parse(&text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn endpoint_appends_models_once() {
        assert_eq!(endpoint("https://api.example.com/v1"), "https://api.example.com/v1/models");
        assert_eq!(
            endpoint("https://api.example.com/v1/"),
            "https://api.example.com/v1/models"
        );
        assert_eq!(
            endpoint("http://127.0.0.1:11434/v1/chat/completions"),
            "http://127.0.0.1:11434/v1/models"
        );
    }

    #[test]
    fn parse_dedupes_sorts_and_filters() {
        let body = r#"{
            "object": "list",
            "data": [
                {"id": "z-model", "object": "model", "owned_by": "x"},
                {"id": "a-model"},
                {"id": "a-model"},
                {"id": "  spaced  "},
                {"id": "   "},
                {"id": ""},
                {"object": "model"},
                {"id": 42},
                {"id": "bad\u0000control"}
            ]
        }"#;
        assert_eq!(
            parse(body).unwrap(),
            vec!["a-model", "spaced", "z-model"]
        );
    }

    #[test]
    fn parse_rejects_malformed_shapes() {
        assert_eq!(
            parse("not json").unwrap_err().code,
            Some("errors.ai.modelsUnavailable")
        );
        assert_eq!(
            parse(r#"{"data": {}}"#).unwrap_err().code,
            Some("errors.ai.modelsUnavailable")
        );
        assert_eq!(parse(r#"{"data": []}"#).unwrap(), Vec::<String>::new());
    }

    fn serve_once<F>(respond: F) -> (String, std::thread::JoinHandle<String>)
    where
        F: FnOnce(&mut std::net::TcpStream) + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 4096];
            let read = stream.read(&mut request).unwrap_or(0);
            let request = String::from_utf8_lossy(&request[..read]).into_owned();
            respond(&mut stream);
            let _ = stream.flush();
            request
        });
        (format!("http://127.0.0.1:{port}/v1"), handle)
    }

    #[tokio::test]
    async fn fetch_gets_models_with_optional_bearer() {
        let (base_url, handle) = serve_once(|stream| {
            let body = r#"{"data":[{"id":"b"},{"id":"a"}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        let models = fetch(&ModelListConfig {
            base_url,
            key: Some("sk-test".into()),
            timeout: Duration::from_secs(5),
            max_bytes: MAX_MODELS_BYTES,
        })
        .await
        .unwrap();
        assert_eq!(models, vec!["a", "b"]);
        let request = handle.join().unwrap();
        assert!(request.starts_with("GET /v1/models "));
        assert!(request.to_lowercase().contains("authorization: bearer sk-test"));
    }

    #[tokio::test]
    async fn fetch_uses_base_of_chat_completions_without_authorization() {
        let (base_url, handle) = serve_once(|stream| {
            let body = r#"{"data":[{"id":"only"}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        let models = fetch(&ModelListConfig {
            base_url: format!("{base_url}/chat/completions"),
            key: None,
            timeout: Duration::from_secs(5),
            max_bytes: MAX_MODELS_BYTES,
        })
        .await
        .unwrap();
        assert_eq!(models, vec!["only"]);
        let request = handle.join().unwrap();
        assert!(request.starts_with("GET /v1/models "));
        assert!(!request.to_lowercase().contains("authorization:"));
    }

    #[tokio::test]
    async fn fetch_maps_status_and_never_leaks_key() {
        let (base_url, handle) = serve_once(|stream| {
            let body = "denied sk-secret";
            let response = format!(
                "HTTP/1.1 401 Unauthorized\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        let error = fetch(&ModelListConfig {
            base_url,
            key: Some("sk-secret".into()),
            timeout: Duration::from_secs(5),
            max_bytes: MAX_MODELS_BYTES,
        })
        .await
        .unwrap_err();
        assert_eq!(error.code, Some("errors.ai.auth"));
        assert!(!format!("{error:?}").contains("sk-secret"));
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn fetch_does_not_follow_redirects() {
        let target = TcpListener::bind("127.0.0.1:0").unwrap();
        let target_port = target.local_addr().unwrap().port();
        target.set_nonblocking(true).unwrap();
        let (base_url, handle) = serve_once(move |stream| {
            let response = format!(
                "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{target_port}/v1/models\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        let error = fetch(&ModelListConfig {
            base_url,
            key: Some("sk-test".into()),
            timeout: Duration::from_secs(5),
            max_bytes: MAX_MODELS_BYTES,
        })
        .await
        .unwrap_err();
        assert_eq!(error.code, Some("errors.ai.modelsUnavailable"));
        handle.join().unwrap();
        std::thread::sleep(Duration::from_millis(100));
        assert!(
            target.accept().is_err(),
            "redirect target must not receive a connection"
        );
    }

    #[tokio::test]
    async fn fetch_limits_response_size() {
        let (base_url, handle) = serve_once(|stream| {
            let body = format!(r#"{{"data":[{{"id":"{}"}}]}}"#, "a".repeat(256));
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        let error = fetch(&ModelListConfig {
            base_url,
            key: None,
            timeout: Duration::from_secs(5),
            max_bytes: 64,
        })
        .await
        .unwrap_err();
        assert_eq!(error.code, Some("errors.ai.modelsUnavailable"));
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn fetch_times_out_while_waiting_for_headers() {
        let (base_url, handle) = serve_once(|stream| {
            // 接受连接、读完请求后一直不发响应头。
            std::thread::sleep(Duration::from_millis(600));
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}");
        });
        let started = std::time::Instant::now();
        let error = fetch(&ModelListConfig {
            base_url,
            key: None,
            timeout: Duration::from_millis(50),
            max_bytes: MAX_MODELS_BYTES,
        })
        .await
        .unwrap_err();
        assert_eq!(error.code, Some("errors.ai.timeout"));
        assert!(
            started.elapsed() < Duration::from_millis(400),
            "headers must not wait for the server sleep"
        );
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn fetch_times_out() {
        let (base_url, handle) = serve_once(|stream| {
            std::thread::sleep(Duration::from_millis(200));
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}");
        });
        let error = fetch(&ModelListConfig {
            base_url,
            key: None,
            timeout: Duration::from_millis(50),
            max_bytes: MAX_MODELS_BYTES,
        })
        .await
        .unwrap_err();
        assert_eq!(error.code, Some("errors.ai.timeout"));
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn fetch_reports_malformed_json_without_body() {
        let (base_url, handle) = serve_once(|stream| {
            let body = "<html>not json</html>";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        let error = fetch(&ModelListConfig {
            base_url,
            key: None,
            timeout: Duration::from_secs(5),
            max_bytes: MAX_MODELS_BYTES,
        })
        .await
        .unwrap_err();
        assert_eq!(error.code, Some("errors.ai.modelsUnavailable"));
        assert!(!format!("{error:?}").contains("not json"));
        handle.join().unwrap();
    }
}
