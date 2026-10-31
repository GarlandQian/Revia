//! GitHub公开Release的完整分页、稳定/预发布选择；全部HTTP只到自建loopback。
use revia_lib::m7::update_channels::{self, PublicRelease};
use std::{
    io::{Read, Write},
    net::TcpListener,
};
fn release(tag: &str, draft: bool, pre: bool) -> serde_json::Value {
    serde_json::json!({"tag_name":tag,"draft":draft,"prerelease":pre,"assets":[{"name":"latest.json","browser_download_url":format!("https://github.com/fixture/project/releases/download/{tag}/latest.json")}]})
}
fn select(releases: Vec<serde_json::Value>, pre: bool, current: &str) -> Option<String> {
    let releases: Vec<PublicRelease> =
        serde_json::from_value(serde_json::Value::Array(releases)).unwrap();
    update_channels::select(
        releases,
        pre,
        &current.parse().unwrap(),
        &"https://github.com/fixture/project".parse().unwrap(),
    )
    .unwrap()
    .map(|m| m.version.to_string())
}
#[test]
fn channel_includes_only_public_versions_and_beta_client_can_find_later_beta_or_stable() {
    assert_eq!(
        select(
            vec![
                release("v9.0.0", true, false),
                release("v1.1.0", false, false),
                release("v2.0.0-beta.2", false, true)
            ],
            false,
            "1.0.0"
        ),
        Some("1.1.0".into())
    );
    assert_eq!(
        select(
            vec![
                release("v1.1.0", false, false),
                release("v2.0.0-beta.2", false, true)
            ],
            true,
            "2.0.0-beta.1"
        ),
        Some("2.0.0-beta.2".into())
    );
    assert_eq!(
        select(
            vec![
                release("v2.0.0", false, false),
                release("v2.0.0-beta.2", false, true)
            ],
            true,
            "2.0.0-beta.1"
        ),
        Some("2.0.0".into())
    );
    assert_eq!(
        select(vec![release("v2.0.0-beta.2", false, false)], false, "1.0.0"),
        None
    );
    assert_eq!(
        select(vec![release("v0.9.0", false, false)], true, "1.0.0"),
        None
    );
}
#[test]
fn discovery_repository_is_fixed_while_artifact_url_is_pinned_to_announced_tag() {
    let (base, api) = update_channels::github_repository(
        "https://github.com/fixture/project/releases/download/v1.0.0-beta.1/latest-prerelease.json",
    )
    .unwrap();
    assert_eq!(base.as_str(), "https://github.com/fixture/project");
    assert_eq!(
        api.as_str(),
        "https://api.github.com/repos/fixture/project/releases?per_page=100&page=1"
    );
    let mut wrong = release("v2.0.0", false, false);
    wrong["assets"][0]["browser_download_url"] =
        "https://github.com/fixture/project/releases/latest/download/latest.json".into();
    let list = serde_json::from_value(serde_json::json!([wrong])).unwrap();
    assert!(update_channels::select(list, true, &"1.0.0".parse().unwrap(), &base).is_err());
    assert!(update_channels::github_repository(
        "https://other.invalid/fixture/project/releases/latest/download/latest.json"
    )
    .is_none());
}
fn api_server(second_failure: bool) -> (url::Url, std::thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let endpoint = format!("{address}/releases?per_page=100&page=1")
        .parse()
        .unwrap();
    let worker = std::thread::spawn(move || {
        let mut seen = Vec::new();
        for page in 1..=2 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                let mut b = [0];
                assert_eq!(stream.read(&mut b).unwrap(), 1);
                header.push(b[0]);
            }
            assert!(!String::from_utf8_lossy(&header)
                .to_ascii_lowercase()
                .contains("authorization:"));
            seen.push(
                String::from_utf8_lossy(&header)
                    .lines()
                    .next()
                    .unwrap()
                    .to_string(),
            );
            let status = if page == 2 && second_failure {
                403
            } else {
                200
            };
            let body = if page == 1 {
                serde_json::json!([release("v1.1.0", false, false)])
            } else if second_failure {
                serde_json::json!({"message":"fixture denied"})
            } else {
                serde_json::json!([release("v2.0.0-beta.2", false, true)])
            }
            .to_string();
            let link = if page == 1 {
                format!("Link: <{address}/releases?per_page=100&page=2>; rel=\"next\"\r\n")
            } else {
                String::new()
            };
            write!(stream,"HTTP/1.1 {status} OK\r\nContent-Length: {}\r\n{link}Content-Type: application/json\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
        seen
    });
    (endpoint, worker)
}
#[test]
fn real_http_full_pagination_selects_later_beta_and_failure_never_returns_partial_stable() {
    for fail in [false, true] {
        let (endpoint, worker) = api_server(fail);
        let result = tauri::async_runtime::block_on(update_channels::discover(
            &"https://github.com/fixture/project".parse().unwrap(),
            endpoint,
            true,
            &"1.0.0".parse().unwrap(),
        ));
        let paths = worker.join().unwrap();
        assert_eq!(paths.len(), 2);
        if fail {
            assert!(result.is_err());
        } else {
            assert_eq!(result.unwrap().unwrap().version.to_string(), "2.0.0-beta.2");
        }
    }
}
