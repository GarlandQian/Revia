//! 公开 minisign fixture → 实际 Node 清单工具 → 锁定 Tauri updater 下载验签。
//! 只用公开测试向量、独占产物目录与 loopback HTTP；不安装、不发布、不触及真实密钥。
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::Path,
    process::Command,
    time::Duration,
};
use tauri_plugin_updater::UpdaterExt;
const PUBKEY: &str = "untrusted comment: minisign public key E7620F1842B4E81F\nRWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
const SIGNATURE: &str = "untrusted comment: signature from minisign secret key\nRWQf6LRCGA9i59SLOFxz6NxvASXDJeRtuZykwQepbDEGt87ig1BNpWaVWuNrm73YiIiJbq71Wi+dP9eKL8OC351vwIasSSbXxwA=\ntrusted comment: timestamp:1555779966\tfile:test\nQtKMXWyYcwdpZAlPF7tE2ENJkRd1ujvKjlj1m9RtHTBnZPa5WKU5uWRs5GoP5M/VqE81QFuMKI5k/SfNQUaOAA==";
fn encode(input: &str) -> String {
    let output = Command::new("node")
        .args([
            "-e",
            "process.stdout.write(Buffer.from(process.argv[1], 'utf8').toString('base64'))",
            input,
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}
fn server(manifest: serde_json::Value) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let mut manifest = manifest;
    manifest["platforms"]["darwin-aarch64"]["url"] = format!("{address}/package").into();
    let bytes = serde_json::to_vec(&manifest).unwrap();
    let handle = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        let mut served = 0;
        while served < 2 {
            let (mut stream, _) = match listener.accept() {
                Ok(client) => client,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "fixture HTTP request timed out"
                    );
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(error) => panic!("fixture accept: {error}"),
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                assert_eq!(stream.read(&mut byte).unwrap(), 1);
                request.push(byte[0]);
            }
            let body: &[u8] = if request.starts_with(b"GET /manifest ") {
                &bytes
            } else {
                assert!(
                    request.starts_with(b"GET /package "),
                    "unexpected fixture path"
                );
                b"test"
            };
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
            stream.write_all(body).unwrap();
            served += 1;
        }
    });
    (address, handle)
}
#[test]
fn generated_legacy_metadata_is_verified_but_release_policy_requires_signed_version() {
    let root = tempfile::tempdir().unwrap();
    let fixture = fs::canonicalize(root.path()).unwrap();
    eprintln!("updater metadata fixture: {}", fixture.display());
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    fs::create_dir_all(fixture.join("Revia.app/Contents/MacOS")).unwrap();
    fs::write(
        fixture.join("Revia.app/Contents/MacOS/Revia"),
        b"fixture executable path only",
    )
    .unwrap();
    fs::create_dir(fixture.join("tools")).unwrap();
    fs::create_dir(fixture.join("src-tauri")).unwrap();
    fs::create_dir(fixture.join("artifacts")).unwrap();
    fs::copy(
        source.join("tools/write-updater-config.mjs"),
        fixture.join("tools/write-updater-config.mjs"),
    )
    .unwrap();
    fs::copy(
        source.join("src-tauri/tauri.conf.json"),
        fixture.join("src-tauri/tauri.conf.json"),
    )
    .unwrap();
    let encoded = encode(SIGNATURE);
    fs::write(fixture.join("artifacts/Revia.app.tar.gz"), b"test").unwrap();
    fs::write(fixture.join("artifacts/Revia.app.tar.gz.sig"), &encoded).unwrap();
    let generated = Command::new("node")
        .arg(revia_lib::platform::external_path(&source.join("tools/generate-latest-json.mjs")))
        .arg("--artifacts")
        .arg(revia_lib::platform::external_path(&fixture.join("artifacts")))
        .args([
            "--download-base",
            "https://example.invalid/fixture",
            "--version",
            "9.9.9",
        ])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let manifest: serde_json::Value = serde_json::from_slice(&generated.stdout).unwrap();
    assert_eq!(
        manifest["platforms"]["darwin-aarch64"]["signature"],
        encoded
    );
    for input in [PUBKEY.to_string(), encode(PUBKEY)] {
        let output = Command::new("node")
            .arg(revia_lib::platform::external_path(&fixture.join("tools/write-updater-config.mjs")))
            .env("REVIA_UPDATE_PUBKEY", input)
            .env(
                "REVIA_UPDATE_ENDPOINT",
                "https://example.invalid/fixture.json",
            )
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let overlay: serde_json::Value = serde_json::from_slice(
            &fs::read(fixture.join("src-tauri/tauri.release.conf.json")).unwrap(),
        )
        .unwrap();
        let pubkey = overlay["plugins"]["updater"]["pubkey"].as_str().unwrap();
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context
            .config_mut()
            .plugins
            .0
            .insert("updater".into(), overlay["plugins"]["updater"].clone());
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().pubkey(pubkey).build())
            .build(context)
            .unwrap();
        let (address, served) = server(manifest.clone());
        let updater = app
            .updater_builder()
            .target("darwin-aarch64")
            .timeout(Duration::from_secs(10))
            .no_proxy()
            .endpoints(vec![format!("{address}/manifest").parse().unwrap()])
            .unwrap()
            .executable_path(fixture.join("Revia.app/Contents/MacOS/Revia"))
            .build()
            .unwrap();
        tauri::async_runtime::block_on(async {
            let update = updater.check().await.unwrap().expect("fixture update");
            assert!(matches!(
                update.download(|_, _| {}, || {}).await.unwrap_err(),
                tauri_plugin_updater::Error::MissingSignedVersion
            ));
        });
        served.join().unwrap();
    }
    for input in [
        "garbage",
        "untrusted comment: bad\nRWQ=",
        "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3",
    ] {
        let output = Command::new("node")
            .arg(revia_lib::platform::external_path(&fixture.join("tools/write-updater-config.mjs")))
            .env("REVIA_UPDATE_PUBKEY", input)
            .env(
                "REVIA_UPDATE_ENDPOINT",
                "https://example.invalid/fixture.json",
            )
            .output()
            .unwrap();
        assert!(!output.status.success(), "invalid key accepted");
    }
}
