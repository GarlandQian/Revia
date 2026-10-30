//! GitHub 登录：设备码授权与访问令牌（M7 任务 06、Q180）。
//!
//! 令牌只写系统钥匙串；界面与日志只看到账号名、头像与授权范围。
//! 设备码登录需要构建时注入的 client id，未配置时明确报错，不退化成伪造流程。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, ErrorKind, Result};

use super::{client_id, web_base, DeviceFlow, GithubAccount, GithubAuthMethod, GithubClient, GithubState};

/// 设备码授权开始后给用户的信息。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DeviceStart {
    pub flow_id: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in_ms: u64,
    pub interval_ms: u64,
}

/// 轮询结果。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum DevicePoll {
    /// 还没授权；界面按 `interval_ms` 继续轮询。
    Pending {
        #[serde(rename = "intervalMs")]
        interval_ms: u64,
        #[serde(rename = "expiresInMs")]
        expires_in_ms: u64,
    },
    Authorized {
        account: GithubAccount,
    },
    Expired,
    Denied,
}

#[derive(Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    #[serde(default)]
    verification_uri: String,
    expires_in: u64,
    #[serde(default = "default_interval")]
    interval: u64,
}

fn default_interval() -> u64 {
    5
}

#[derive(Deserialize)]
struct TokenResponse {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    interval: Option<u64>,
    /// F16：GitHub App 用户令牌会过期；refresh_token 及其过期时间一并接收。
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    refresh_token_expires_in: Option<u64>,
}

#[derive(Deserialize)]
struct UserResponse {
    login: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    avatar_url: Option<String>,
}

/// 表单编码（设备码接口要求 `application/x-www-form-urlencoded`）。
fn urlencode(pairs: &[(&str, &str)]) -> String {
    let mut out = String::new();
    for (index, (key, value)) in pairs.iter().enumerate() {
        if index > 0 {
            out.push('&');
        }
        out.push_str(&encode_component(key));
        out.push('=');
        out.push_str(&encode_component(value));
    }
    out
}

fn encode_component(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn http_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("Revia")
        .build()
        .map_err(|error| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.m7.githubClientFailed",
                "无法初始化 GitHub 客户端：{error}",
                &[("error", error.to_string())],
            )
        })
}

fn missing_client_id() -> AppError {
    AppError::coded(
        ErrorKind::Rejected,
        "errors.m7.githubDeviceFlowUnconfigured",
        "此安装包没有配置 GitHub 登录，请改用访问令牌登录",
        &[],
    )
}

fn network_error(error: reqwest::Error) -> AppError {
    AppError::coded(
        ErrorKind::Environment,
        "errors.m7.githubNetwork",
        "无法连接 GitHub：{error}",
        &[("error", error.to_string())],
    )
}

/// 开始设备码授权。
pub fn start_device_flow(state: &GithubState) -> Result<DeviceStart> {
    let client_id = client_id().ok_or_else(missing_client_id)?;
    let client = http_client()?;
    let response = client
        .post(format!("{}/login/device/code", web_base()))
        .header("Accept", "application/json")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(urlencode(&[("client_id", &client_id), ("scope", "repo read:user")]))
        .send()
        .map_err(network_error)?;
    if !response.status().is_success() {
        return Err(AppError::coded(
            ErrorKind::Environment,
            "errors.m7.githubDeviceFlowFailed",
            "GitHub 拒绝了设备码请求（HTTP {status}）",
            &[("status", response.status().as_u16().to_string())],
        ));
    }
    let body: DeviceCodeResponse = response.json().map_err(network_error)?;
    let now = super::super::now_ms();
    let flow_id = super::super::sha256_hex(format!("{}\0{now}", body.device_code).as_bytes())[..16].to_string();
    let interval_ms = body.interval.max(1) * 1000;
    let expires_at_ms = now + body.expires_in * 1000;
    let mut flows = state.flows.lock().unwrap_or_else(|error| error.into_inner());
    // 只保留一个进行中的流程：新流程会替换旧流程，避免同时轮询多个设备码。
    flows.clear();
    flows.insert(
        flow_id.clone(),
        DeviceFlow {
            device_code: body.device_code,
            interval_ms,
            expires_at_ms,
            next_poll_ms: now + interval_ms,
        },
    );
    Ok(DeviceStart {
        flow_id,
        user_code: body.user_code,
        verification_uri: if body.verification_uri.is_empty() {
            format!("{}/login/device", web_base())
        } else {
            body.verification_uri
        },
        expires_in_ms: body.expires_in * 1000,
        interval_ms,
    })
}

/// F16：取消设备码授权——移除进行中的流程，后续轮询按"未知流程"结束；
/// 已填的用户码不再有效。这是本地取消，不访问网络。
pub fn cancel_device_flow(state: &GithubState, flow_id: &str) -> Result<()> {
    let removed = state
        .flows
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(flow_id)
        .is_some();
    if removed {
        Ok(())
    } else {
        Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.githubDeviceFlowUnknown",
            "登录流程已结束，请重新开始",
            &[],
        ))
    }
}

/// 轮询一次设备码授权状态。
pub fn poll_device_flow(state: &GithubState, flow_id: &str) -> Result<DevicePoll> {
    let client_id = client_id().ok_or_else(missing_client_id)?;
    let now = super::super::now_ms();
    let flow = {
        let flows = state.flows.lock().unwrap_or_else(|error| error.into_inner());
        let flow = flows.get(flow_id).ok_or_else(|| {
            AppError::coded(
                ErrorKind::Rejected,
                "errors.m7.githubDeviceFlowUnknown",
                "登录流程已结束，请重新开始",
                &[],
            )
        })?;
        DeviceFlow {
            device_code: flow.device_code.clone(),
            interval_ms: flow.interval_ms,
            expires_at_ms: flow.expires_at_ms,
            next_poll_ms: flow.next_poll_ms,
        }
    };
    if now >= flow.expires_at_ms {
        state
            .flows
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(flow_id);
        return Ok(DevicePoll::Expired);
    }
    if now < flow.next_poll_ms {
        return Ok(DevicePoll::Pending {
            interval_ms: flow.next_poll_ms - now,
            expires_in_ms: flow.expires_at_ms - now,
        });
    }
    let client = http_client()?;
    let response = client
        .post(format!("{}/login/oauth/access_token", web_base()))
        .header("Accept", "application/json")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(urlencode(&[
            ("client_id", &client_id),
            ("device_code", &flow.device_code),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ]))
        .send()
        .map_err(network_error)?;
    let body: TokenResponse = response.json().map_err(network_error)?;
    let mut flows = state.flows.lock().unwrap_or_else(|error| error.into_inner());
    let Some(slot) = flows.get_mut(flow_id) else {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.githubDeviceFlowUnknown",
            "登录流程已结束，请重新开始",
            &[],
        ));
    };
    match body.error.as_deref() {
        None => {
            let token = body.access_token.ok_or_else(|| {
                AppError::coded(
                    ErrorKind::Environment,
                    "errors.m7.githubDeviceFlowFailed",
                    "GitHub 没有返回访问令牌",
                    &[],
                )
            })?;
            drop(flows);
            // F16：设备码流程的 refresh_token 一并传给登录持久化。
            let account = login_with_token_full(
                state,
                &token,
                body.scope.as_deref(),
                GithubAuthMethod::DeviceFlow,
                body.refresh_token.as_deref(),
            )?;
            state
                .flows
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .remove(flow_id);
            Ok(DevicePoll::Authorized { account })
        }
        Some("authorization_pending") => {
            slot.next_poll_ms = now + slot.interval_ms;
            Ok(DevicePoll::Pending {
                interval_ms: slot.interval_ms,
                expires_in_ms: slot.expires_at_ms.saturating_sub(now),
            })
        }
        Some("slow_down") => {
            // GitHub 要求放慢：优先用响应里的新间隔，否则在旧间隔上加 5 秒。
            slot.interval_ms = body
                .interval
                .map(|seconds| seconds.max(1) * 1000)
                .unwrap_or(slot.interval_ms + 5000);
            slot.next_poll_ms = now + slot.interval_ms;
            Ok(DevicePoll::Pending {
                interval_ms: slot.interval_ms,
                expires_in_ms: slot.expires_at_ms.saturating_sub(now),
            })
        }
        Some("expired_token") => {
            drop(flows);
            state
                .flows
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .remove(flow_id);
            Ok(DevicePoll::Expired)
        }
        Some("access_denied") => {
            drop(flows);
            state
                .flows
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .remove(flow_id);
            Ok(DevicePoll::Denied)
        }
        Some(other) => Err(AppError::coded(
            ErrorKind::Environment,
            "errors.m7.githubDeviceFlowFailed",
            "GitHub 登录失败：{error}",
            &[("error", other.to_string())],
        )),
    }
}

/// 用访问令牌登录（也用于设备码拿到令牌后的账号登记）。
pub fn login_with_token(
    state: &GithubState,
    token: &str,
    scopes_hint: Option<&str>,
    method: GithubAuthMethod,
) -> Result<GithubAccount> {
    login_with_token_full(state, token, scopes_hint, method, None)
}

/// F16：带刷新令牌的完整登录路径；设备码流程拿到 refresh_token 时走这里。
pub fn login_with_token_full(
    state: &GithubState,
    token: &str,
    scopes_hint: Option<&str>,
    method: GithubAuthMethod,
    refresh: Option<&str>,
) -> Result<GithubAccount> {
    let token = token.trim();
    if token.is_empty() {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.githubTokenEmpty",
            "访问令牌不能为空",
            &[],
        ));
    }
    if token.chars().any(|ch| ch.is_whitespace()) {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.githubTokenInvalid",
            "访问令牌格式不正确",
            &[],
        ));
    }
    let client = GithubClient::new(super::api_base(), token.to_string())?;
    let (user, meta): (UserResponse, _) = client.get_json("/user")?;
    let scopes = if meta.scopes.is_empty() {
        scopes_hint
            .map(|value| {
                value
                    .split(',')
                    .map(|scope| scope.trim().to_string())
                    .filter(|scope| !scope.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    } else {
        meta.scopes
    };
    let account = GithubAccount {
        login: user.login,
        name: user.name,
        avatar_url: user.avatar_url,
        auth_method: method,
        scopes,
        added_at_ms: super::super::now_ms(),
    };
    state.tokens.save(&account.login, token)?;
    // F16：刷新令牌（如有）一并持久化，过期轮换时使用。
    state.tokens.save_refresh(&account.login, refresh.unwrap_or(""))?;
    state.upsert_account(account.clone())?;
    Ok(account)
}

/// 退出账号：删钥匙串条目、账号记录与仓库绑定。
pub fn logout(state: &GithubState, login: &str) -> Result<()> {
    state.remove_account(login)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_or_whitespace_tokens_are_rejected_before_any_network_call() {
        let dir = tempfile::tempdir().unwrap();
        let state = GithubState::new(dir.path());
        assert_eq!(
            login_with_token(&state, "  ", None, GithubAuthMethod::Token)
                .unwrap_err()
                .code,
            Some("errors.m7.githubTokenEmpty")
        );
        assert_eq!(
            login_with_token(&state, "abc def", None, GithubAuthMethod::Token)
                .unwrap_err()
                .code,
            Some("errors.m7.githubTokenInvalid")
        );
    }
}
