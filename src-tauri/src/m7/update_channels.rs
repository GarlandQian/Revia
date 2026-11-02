//! 从配置绑定的GitHub公开Release发现所选渠道；忽略草稿，所有页成功才返回候选。
use crate::error::{AppError, ErrorKind, Result};
use serde::Deserialize;
#[derive(Debug, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
}
#[derive(Debug, Deserialize)]
pub struct PublicRelease {
    pub tag_name: String,
    pub draft: bool,
    pub prerelease: bool,
    pub assets: Vec<ReleaseAsset>,
}
#[derive(Debug, Clone)]
pub struct ReleaseManifest {
    pub version: semver::Version,
    pub endpoint: url::Url,
}
fn invalid() -> AppError {
    AppError::coded(
        ErrorKind::Rejected,
        "errors.m7.updateReleaseIncomplete",
        "更新发布信息不完整，已保留上次状态",
        &[],
    )
}
/// 发布构建保持同一仓库发现入口，不把beta客户端固定到自己的tag。
pub fn github_repository(endpoint: &str) -> Option<(url::Url, url::Url)> {
    let endpoint = url::Url::parse(endpoint).ok()?;
    if endpoint.scheme() != "https" || endpoint.host_str() != Some("github.com") {
        return None;
    }
    let parts: Vec<_> = endpoint.path_segments()?.collect();
    if parts.len() < 5 || parts[0].is_empty() || parts[1].is_empty() || parts[2] != "releases" {
        return None;
    }
    let base = url::Url::parse(&format!("https://github.com/{}/{}", parts[0], parts[1])).ok()?;
    let api = url::Url::parse(&format!(
        "https://api.github.com/repos/{}/{}/releases?per_page=100&page=1",
        parts[0], parts[1]
    ))
    .ok()?;
    Some((base, api))
}
pub fn select(
    releases: Vec<PublicRelease>,
    allow_prerelease: bool,
    current: &semver::Version,
    base: &url::Url,
) -> Result<Option<ReleaseManifest>> {
    let mut selected: Option<ReleaseManifest> = None;
    for release in releases {
        if release.draft {
            continue;
        }
        let Ok(version) = semver::Version::parse(
            release
                .tag_name
                .strip_prefix('v')
                .unwrap_or(&release.tag_name),
        ) else {
            continue;
        };
        if version <= *current
            || (!allow_prerelease && (release.prerelease || !version.pre.is_empty()))
        {
            continue;
        }
        let Some(asset) = release
            .assets
            .into_iter()
            .find(|asset| asset.name == "latest.json")
        else {
            continue;
        };
        let endpoint = url::Url::parse(&asset.browser_download_url).map_err(|_| invalid())?;
        let mut expected = base.clone();
        expected
            .path_segments_mut()
            .map_err(|_| invalid())?
            .extend(["releases", "download", &release.tag_name, "latest.json"]);
        if endpoint != expected
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.fragment().is_some()
        {
            return Err(invalid());
        }
        if selected
            .as_ref()
            .is_none_or(|saved| version > saved.version)
        {
            selected = Some(ReleaseManifest { version, endpoint });
        }
    }
    Ok(selected)
}
pub async fn discover(
    base: &url::Url,
    list_url: url::Url,
    allow_prerelease: bool,
    current: &semver::Version,
) -> Result<Option<ReleaseManifest>> {
    let client = reqwest::Client::builder()
        .user_agent("Revia")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|_| invalid())?;
    let mut url = list_url.clone();
    let mut visited = std::collections::BTreeSet::new();
    let mut releases = Vec::new();
    loop {
        if url.origin() != list_url.origin()
            || url.path() != list_url.path()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || visited.len() >= 10_000
            || !visited.insert(url.as_str().to_string())
        {
            return Err(invalid());
        }
        let response = client
            .get(url.clone())
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|_| invalid())?
            .error_for_status()
            .map_err(|_| invalid())?;
        let next = response
            .headers()
            .get("link")
            .and_then(|h| h.to_str().ok())
            .and_then(|h| {
                h.split(',').find_map(|part| {
                    part.contains("rel=\"next\"").then(|| {
                        part.split(';')
                            .next()
                            .unwrap()
                            .trim()
                            .trim_start_matches('<')
                            .trim_end_matches('>')
                            .to_string()
                    })
                })
            });
        let mut page: Vec<PublicRelease> = response.json().await.map_err(|_| invalid())?;
        releases.append(&mut page);
        match next {
            Some(next) => url = url.join(&next).map_err(|_| invalid())?,
            None => return select(releases, allow_prerelease, current, base),
        }
    }
}
