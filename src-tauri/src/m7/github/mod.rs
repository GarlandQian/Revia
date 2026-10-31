//! GitHub 账号、拉取请求与合并（M7 任务 06–08、Q180–Q182）。
//!
//! 边界：
//! - 网络只由这里发起；凭据只进系统钥匙串（验收实例可用独立文件存储）；
//! - 合并走 GitHub API，不改用户 Git 配置；检出 PR 仍由 Rust 调系统 Git；
//! - 任何写操作先给回执（合并/评审），或在写前核对 head 是否变化。
//!
//! 未配置 `REVIA_GITHUB_CLIENT_ID` 时只有“用访问令牌登录”可用，
//! 设备码登录会明确报“未配置”，不伪造一个客户端 id。

pub mod auth;
pub mod pr;
pub mod protection;
pub mod store;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, ErrorKind, Result};

/// 设备码登录使用的 GitHub App client id。
///
/// 生产安装包必须在构建时通过 `REVIA_GITHUB_CLIENT_ID` 注入；源码里不写死任何 id。
pub fn client_id() -> Option<String> {
    std::env::var("REVIA_GITHUB_CLIENT_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            option_env!("REVIA_GITHUB_CLIENT_ID")
                .map(str::to_string)
                .filter(|value| !value.trim().is_empty())
        })
}

/// API 根地址；只有验收实例可以用环境变量改到本地替身。
pub fn api_base() -> String {
    acceptance_override("REVIA_GITHUB_API_BASE").unwrap_or_else(|| "https://api.github.com".into())
}

/// 网页根地址（设备码登录、PR 链接）。
pub fn web_base() -> String {
    acceptance_override("REVIA_GITHUB_WEB_BASE").unwrap_or_else(|| "https://github.com".into())
}

fn acceptance_override(key: &str) -> Option<String> {
    if std::env::var("REVIA_ACCEPTANCE").as_deref() != Ok("1") {
        return None;
    }
    std::env::var(key).ok().filter(|value| !value.is_empty())
}

/// 登录方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum GithubAuthMethod {
    /// 设备码（GitHub App 用户授权）。
    DeviceFlow,
    /// 用户自己粘贴的访问令牌。
    Token,
}

/// 一个已登录的 GitHub 账号（不包含令牌）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GithubAccount {
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub auth_method: GithubAuthMethod,
    pub scopes: Vec<String>,
    pub added_at_ms: u64,
}

/// 账号与仓库绑定的总览。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GithubStatus {
    /// 是否配置了设备码登录的 client id。
    pub device_flow_configured: bool,
    pub accounts: Vec<GithubAccount>,
    /// 当前仓库绑定的账号（未绑定时为 None，界面按第一个账号显示）。
    pub repo_account: Option<String>,
}

/// 速率限制快照（列表与详情都会带回，界面在接近上限时提示）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RateLimit {
    pub remaining: Option<u32>,
    pub reset_at_ms: Option<u64>,
}

/// 账号索引（Revia 数据目录；只有账号信息，没有令牌）。
struct AccountIndex {
    path: PathBuf,
}

impl AccountIndex {
    fn load(&self) -> Vec<GithubAccount> {
        std::fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn save(&self, accounts: &[GithubAccount]) -> Result<()> {
        super::atomic_write(&self.path, &serde_json::to_vec_pretty(accounts)?)
    }
}

/// 仓库绑定（Revia 数据目录；仓库键 = 仓库绝对路径的指纹）。
struct RepoBindings {
    path: PathBuf,
}

impl RepoBindings {
    fn load(&self) -> BTreeMap<String, String> {
        std::fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn save(&self, bindings: &BTreeMap<String, String>) -> Result<()> {
        super::atomic_write(&self.path, &serde_json::to_vec_pretty(bindings)?)
    }
}

/// 访问/刷新令牌及到期时间作为同一个钥匙串条目轮换，普通账号索引不保存凭据。
#[derive(Clone, Serialize, Deserialize)]
pub struct TokenCredentials {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at_ms: Option<u64>,
    pub refresh_expires_at_ms: Option<u64>,
}

/// 令牌存储：系统钥匙串；验收实例可显式改用独立文件（仅 `REVIA_ACCEPTANCE=1`）。
pub struct TokenStore {
    acceptance_file: Option<PathBuf>,
}

impl TokenStore {
    pub fn new(data_dir: &Path) -> Self {
        let acceptance_file = if std::env::var("REVIA_ACCEPTANCE").as_deref() == Ok("1") {
            std::env::var_os("REVIA_GITHUB_TOKEN_STORE").map(|value| {
                let path = PathBuf::from(value);
                if path.is_absolute() {
                    path
                } else {
                    data_dir.join("github-tokens")
                }
            })
        } else {
            None
        };
        Self { acceptance_file }
    }

    fn service() -> &'static str {
        "Revia GitHub"
    }

    pub fn save(&self, login: &str, token: &str) -> Result<()> {
        if let Some(directory) = &self.acceptance_file {
            std::fs::create_dir_all(directory)?;
            let path = directory.join(format!("{}.txt", super::safe_slug(login)));
            super::atomic_write(&path, token.as_bytes())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
            }
            return Ok(());
        }
        keyring::Entry::new(Self::service(), login)
            .and_then(|entry| entry.set_password(token))
            .map_err(|error| {
                AppError::coded(
                    ErrorKind::Environment,
                    "errors.m7.githubKeychainFailed",
                    "无法把 GitHub 凭据写入系统钥匙串：{error}",
                    &[("error", error.to_string())],
                )
            })
    }

    fn load_raw(&self, login: &str) -> Result<String> {
        if let Some(directory) = &self.acceptance_file {
            let path = directory.join(format!("{}.txt", super::safe_slug(login)));
            return std::fs::read_to_string(&path).map_err(|_| not_logged_in(login));
        }
        keyring::Entry::new(Self::service(), login)
            .and_then(|entry| entry.get_password())
            .map_err(|_| not_logged_in(login))
    }

    pub fn credentials(&self, login: &str) -> Result<TokenCredentials> {
        let raw = self.load_raw(login)?;
        let credentials = if raw.starts_with('{') {
            serde_json::from_str(&raw).map_err(|_| not_logged_in(login))?
        } else {
            // 兼容旧版独立条目；没有到期时间的旧设备令牌在有刷新令牌时先轮换。
            TokenCredentials {
                access_token: raw,
                refresh_token: self.legacy_refresh(login),
                expires_at_ms: None,
                refresh_expires_at_ms: None,
            }
        };
        if credentials.access_token.is_empty()
            || credentials.access_token.chars().any(char::is_whitespace)
        {
            return Err(not_logged_in(login));
        }
        Ok(credentials)
    }

    pub fn load(&self, login: &str) -> Result<String> {
        Ok(self.credentials(login)?.access_token)
    }

    pub fn save_credentials(&self, login: &str, credentials: &TokenCredentials) -> Result<()> {
        // 单次原子文件写/钥匙串set，避免新access与旧refresh组合。
        self.save(login, &serde_json::to_string(credentials)?)?;
        self.delete_refresh(login)?;
        Ok(())
    }

    /// 旧版刷新令牌服务，仅用于迁移读取与清理。
    fn refresh_service() -> &'static str {
        "Revia GitHub Refresh"
    }

    pub fn save_refresh(&self, login: &str, refresh_token: &str) -> Result<()> {
        let mut credentials = self.credentials(login)?;
        credentials.refresh_token = (!refresh_token.is_empty()).then(|| refresh_token.to_string());
        self.save_credentials(login, &credentials)
    }

    fn legacy_refresh(&self, login: &str) -> Option<String> {
        if let Some(directory) = &self.acceptance_file {
            let path = directory.join(format!("{}.refresh.txt", super::safe_slug(login)));
            return std::fs::read_to_string(&path)
                .ok()
                .filter(|t| !t.is_empty());
        }
        keyring::Entry::new(Self::refresh_service(), login)
            .and_then(|entry| entry.get_password())
            .ok()
            .filter(|t| !t.is_empty())
    }

    pub fn load_refresh(&self, login: &str) -> Option<String> {
        self.credentials(login)
            .ok()
            .and_then(|credentials| credentials.refresh_token)
    }

    pub fn delete_refresh(&self, login: &str) -> Result<()> {
        if let Some(directory) = &self.acceptance_file {
            let path = directory.join(format!("{}.refresh.txt", super::safe_slug(login)));
            return match std::fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error.into()),
            };
        }
        Self::delete_keyring_entry(Self::refresh_service(), login)
    }

    fn delete_keyring_entry(service: &str, login: &str) -> Result<()> {
        match keyring::Entry::new(service, login).and_then(|entry| entry.delete_credential()) {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(AppError::coded(
                ErrorKind::Environment,
                "errors.m7.githubCredentialDeleteFailed",
                "无法删除 GitHub 凭据：{error}",
                &[("error", error.to_string())],
            )),
        }
    }

    pub fn delete(&self, login: &str) -> Result<()> {
        if let Some(directory) = &self.acceptance_file {
            let path = directory.join(format!("{}.txt", super::safe_slug(login)));
            match std::fs::remove_file(path) {
                Ok(()) => return Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(error) => return Err(error.into()),
            }
        }
        Self::delete_keyring_entry(Self::service(), login)
    }
}

pub fn not_logged_in(login: &str) -> AppError {
    AppError::coded(
        ErrorKind::Rejected,
        "errors.m7.githubNotLoggedIn",
        "GitHub 账号 {login} 的凭据已不可用，请重新登录",
        &[("login", login.to_string())],
    )
}

/// 一个进行中的设备码授权流程。
pub struct DeviceFlow {
    pub device_code: String,
    pub interval_ms: u64,
    pub expires_at_ms: u64,
    pub next_poll_ms: u64,
}

/// GitHub 相关状态（进程内 + Revia 数据目录）。
pub struct GithubState {
    accounts: AccountIndex,
    bindings: RepoBindings,
    drafts: store::ReviewDraftStore,
    receipts: store::ReceiptStore,
    pub tokens: TokenStore,
    pub flows: Mutex<BTreeMap<String, DeviceFlow>>,
    credential_lock: Mutex<()>,
}

impl GithubState {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            accounts: AccountIndex {
                path: data_dir.join("github-accounts.json"),
            },
            bindings: RepoBindings {
                path: data_dir.join("github-repos.json"),
            },
            drafts: store::ReviewDraftStore::new(data_dir),
            receipts: store::ReceiptStore::new(data_dir),
            tokens: TokenStore::new(data_dir),
            flows: Mutex::new(BTreeMap::new()),
            credential_lock: Mutex::new(()),
        }
    }

    pub fn drafts(&self) -> &store::ReviewDraftStore {
        &self.drafts
    }

    pub fn receipts(&self) -> &store::ReceiptStore {
        &self.receipts
    }

    pub fn accounts(&self) -> Vec<GithubAccount> {
        self.accounts.load()
    }

    pub fn upsert_account(&self, account: GithubAccount) -> Result<()> {
        let mut accounts = self.accounts.load();
        accounts.retain(|existing| existing.login != account.login);
        accounts.push(account);
        accounts.sort_by(|left, right| left.login.cmp(&right.login));
        self.accounts.save(&accounts)
    }

    pub fn remove_account(&self, login: &str) -> Result<()> {
        let _guard = self
            .credential_lock
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        self.tokens.delete(login)?;
        self.tokens.delete_refresh(login)?;
        let mut accounts = self.accounts.load();
        accounts.retain(|account| account.login != login);
        self.accounts.save(&accounts)?;
        let mut bindings = self.bindings.load();
        bindings.retain(|_, bound| bound != login);
        self.bindings.save(&bindings)?;
        Ok(())
    }

    pub fn repo_key(repo_path: &Path) -> String {
        let canonical =
            std::fs::canonicalize(repo_path).unwrap_or_else(|_| repo_path.to_path_buf());
        super::sha256_hex(canonical.to_string_lossy().as_bytes())
    }

    pub fn bind_repo(&self, repo_path: &Path, login: &str) -> Result<()> {
        if !self.accounts().iter().any(|account| account.login == login) {
            return Err(not_logged_in(login));
        }
        let mut bindings = self.bindings.load();
        bindings.insert(Self::repo_key(repo_path), login.to_string());
        self.bindings.save(&bindings)
    }

    pub fn repo_account(&self, repo_path: &Path) -> Option<String> {
        let bindings = self.bindings.load();
        bindings
            .get(&Self::repo_key(repo_path))
            .cloned()
            .filter(|login| {
                self.accounts()
                    .iter()
                    .any(|account| &account.login == login)
            })
    }

    /// 取出当前仓库要用的账号与客户端；未绑定时用唯一账号。
    pub fn client_for(&self, repo_path: &Path) -> Result<(String, GithubClient)> {
        // 刷新会使旧refresh失效；同一状态内串行选择与轮换，不自动重放任何业务写请求。
        let _guard = self
            .credential_lock
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let accounts = self.accounts();
        if accounts.is_empty() {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.m7.githubNoAccount",
                "还没有登录 GitHub 账号",
                &[],
            ));
        }
        let login = match self.repo_account(repo_path) {
            Some(login) => login,
            None if accounts.len() == 1 => accounts[0].login.clone(),
            None => {
                return Err(AppError::coded(
                    ErrorKind::Rejected,
                    "errors.m7.githubRepoUnbound",
                    "当前仓库有多个 GitHub 账号，请先为它选择账号",
                    &[],
                ));
            }
        };
        let account = accounts
            .iter()
            .find(|account| account.login == login)
            .ok_or_else(|| not_logged_in(&login))?;
        let token = auth::access_token_for(self, &login, account.auth_method)?;
        Ok((login, GithubClient::new(api_base(), token)?))
    }
}

/// 解析 GitHub 仓库标识（owner/name），只接受 GitHub 的远端地址。
pub fn parse_github_repo(url: &str) -> Option<(String, String)> {
    let trimmed = url.trim().trim_end_matches('/');
    let rest = if let Some(rest) = trimmed.strip_prefix("https://github.com/") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("http://github.com/") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("ssh://git@github.com/") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("git@github.com:") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("github.com/") {
        rest
    } else {
        return None;
    };
    let rest = rest.trim_end_matches(".git").trim_end_matches('/');
    let mut parts = rest.splitn(2, '/');
    let owner = parts.next()?.trim();
    let repo = parts.next()?.trim();
    if owner.is_empty()
        || repo.is_empty()
        || !owner
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        || !repo
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return None;
    }
    Some((owner.to_string(), repo.to_string()))
}

/// HTTP 客户端：只做 JSON 请求与错误归类。
pub struct GithubClient {
    http: reqwest::blocking::Client,
    base: String,
    token: String,
}

/// 一次请求的元信息（速率限制与授权范围）。
#[derive(Debug, Clone, Default)]
pub struct ApiMeta {
    pub rate_limit: RateLimit,
    pub scopes: Vec<String>,
    /// F19：Link header 里的下一页完整 URL（没有下一页为 None）。
    pub next_page: Option<String>,
}

impl GithubClient {
    pub fn new(base: String, token: String) -> Result<Self> {
        let http = reqwest::blocking::Client::builder()
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
            })?;
        Ok(Self { http, base, token })
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<(T, ApiMeta)> {
        let (bytes, meta) = self.send(
            reqwest::Method::GET,
            path,
            None,
            "application/vnd.github+json",
        )?;
        let value = serde_json::from_slice(&bytes).map_err(|error| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.m7.githubBadResponse",
                "GitHub 返回了无法解析的数据：{error}",
                &[("error", error.to_string())],
            )
        })?;
        Ok((value, meta))
    }

    pub fn post_json<T: serde::de::DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<(T, ApiMeta)> {
        let body = serde_json::to_vec(body)?;
        let (bytes, meta) = self.send(
            reqwest::Method::POST,
            path,
            Some(body),
            "application/vnd.github+json",
        )?;
        let value = serde_json::from_slice(&bytes).map_err(|error| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.m7.githubBadResponse",
                "GitHub 返回了无法解析的数据：{error}",
                &[("error", error.to_string())],
            )
        })?;
        Ok((value, meta))
    }

    pub fn put_json<T: serde::de::DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<(T, ApiMeta)> {
        let body = serde_json::to_vec(body)?;
        let (bytes, meta) = self.send(
            reqwest::Method::PUT,
            path,
            Some(body),
            "application/vnd.github+json",
        )?;
        let value = serde_json::from_slice(&bytes).map_err(|error| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.m7.githubBadResponse",
                "GitHub 返回了无法解析的数据：{error}",
                &[("error", error.to_string())],
            )
        })?;
        Ok((value, meta))
    }

    /// 取纯文本响应（PR 的 diff / patch）。
    pub fn get_text(&self, path: &str, accept: &str) -> Result<(String, ApiMeta)> {
        let (bytes, meta) = self.send(reqwest::Method::GET, path, None, accept)?;
        Ok((String::from_utf8_lossy(&bytes).into_owned(), meta))
    }

    fn send(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Vec<u8>>,
        accept: &str,
    ) -> Result<(Vec<u8>, ApiMeta)> {
        let url = if path.starts_with("http") {
            path.to_string()
        } else {
            format!("{}{}", self.base, path)
        };
        let mut request = self
            .http
            .request(method, &url)
            .header("Accept", accept)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(body) = body {
            request = request
                .header("Content-Type", "application/json")
                .body(body);
        }
        let response = request.send().map_err(|error| {
            AppError::coded(
                ErrorKind::Environment,
                "errors.m7.githubNetwork",
                "无法连接 GitHub：{error}",
                &[("error", error.to_string())],
            )
        })?;
        let status = response.status();
        let meta = meta_of(response.headers());
        let bytes = response.bytes().map_err(|error| {
            AppError::coded(
                ErrorKind::Environment,
                "errors.m7.githubNetwork",
                "读取 GitHub 响应失败：{error}",
                &[("error", error.to_string())],
            )
        })?;
        if status.is_success() {
            return Ok((bytes.to_vec(), meta));
        }
        Err(api_error(status.as_u16(), &bytes, &meta))
    }
}

fn meta_of(headers: &reqwest::header::HeaderMap) -> ApiMeta {
    let number = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<u64>().ok())
    };
    let next_page = headers
        .get("link")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            // Link: <https://api.github.com/...?page=2>; rel="next", <...>; rel="last"
            value.split(',').find_map(|part| {
                let part = part.trim();
                if part.contains("rel=\"next\"") {
                    part.split(';').next().map(|url| {
                        url.trim()
                            .trim_start_matches('<')
                            .trim_end_matches('>')
                            .to_string()
                    })
                } else {
                    None
                }
            })
        });
    ApiMeta {
        next_page,
        rate_limit: RateLimit {
            remaining: number("x-ratelimit-remaining").map(|value| value as u32),
            reset_at_ms: number("x-ratelimit-reset").map(|value| value * 1000),
        },
        scopes: headers
            .get("x-oauth-scopes")
            .and_then(|value| value.to_str().ok())
            .map(|value| {
                value
                    .split(',')
                    .map(|scope| scope.trim().to_string())
                    .filter(|scope| !scope.is_empty())
                    .collect()
            })
            .unwrap_or_default(),
    }
}

fn api_error(status: u16, body: &[u8], meta: &ApiMeta) -> AppError {
    let detail = serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("message")
                .and_then(|message| message.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| String::from_utf8_lossy(body).chars().take(300).collect());
    match status {
        401 => AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.githubUnauthorized",
            "GitHub 凭据已失效，请重新登录：{detail}",
            &[("detail", detail)],
        ),
        403 | 429 if meta.rate_limit.remaining == Some(0) => AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.githubRateLimited",
            "GitHub 速率限制已用尽，请稍后再试：{detail}",
            &[
                ("detail", detail),
                (
                    "retryAtMs",
                    meta.rate_limit.reset_at_ms.unwrap_or(0).to_string(),
                ),
            ],
        ),
        404 => AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.githubNotFound",
            "GitHub 上找不到对应资源：{detail}",
            &[("detail", detail)],
        ),
        422 => AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.githubValidationFailed",
            "GitHub 拒绝了这次请求：{detail}",
            &[("detail", detail)],
        ),
        _ => AppError::coded(
            ErrorKind::Environment,
            "errors.m7.githubRequestFailed",
            "GitHub 请求失败（HTTP {status}）：{detail}",
            &[("status", status.to_string()), ("detail", detail)],
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_github_remote_urls() {
        for url in [
            "https://github.com/owner/repo.git",
            "https://github.com/owner/repo",
            "git@github.com:owner/repo.git",
            "ssh://git@github.com/owner/repo.git",
        ] {
            assert_eq!(
                parse_github_repo(url),
                Some(("owner".to_string(), "repo".to_string())),
                "{url}"
            );
        }
        assert_eq!(parse_github_repo("https://gitlab.com/owner/repo.git"), None);
        assert_eq!(parse_github_repo("https://github.com/owner"), None);
    }

    #[test]
    fn api_errors_are_structured() {
        let meta = ApiMeta::default();
        assert_eq!(
            api_error(401, b"{\"message\":\"Bad credentials\"}", &meta).code,
            Some("errors.m7.githubUnauthorized")
        );
        let limited = ApiMeta {
            next_page: None,
            rate_limit: RateLimit {
                remaining: Some(0),
                reset_at_ms: Some(1_700_000_000_000),
            },
            scopes: Vec::new(),
        };
        let error = api_error(403, b"{}", &limited);
        assert_eq!(error.code, Some("errors.m7.githubRateLimited"));
        assert!(
            error
                .params
                .iter()
                .any(|(key, value)| *key == "retryAtMs" && value == "1700000000000")
        );
    }
}
