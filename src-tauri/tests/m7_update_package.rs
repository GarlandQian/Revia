//! 实际Tauri下载/验签→缓存→新updater离线恢复→只替换独占fixture .app。
use revia_lib::m7::update_package::{self, DownloadedUpdate};
use std::{
    fs,
    io::{Cursor, Read, Write},
    net::TcpListener,
    path::Path,
    process::Command,
};
use tauri_plugin_updater::UpdaterExt;
fn encode(text: &str) -> String {
    let out = Command::new("node")
        .args([
            "-e",
            "process.stdout.write(Buffer.from(process.argv[1]).toString('base64'))",
            text,
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap()
}
fn package(root: &Path) -> Vec<u8> {
    let staged = root.join("stage/Revia.app/Contents/MacOS");
    fs::create_dir_all(&staged).unwrap();
    fs::write(staged.join("Revia"), b"new fixture application").unwrap();
    let out = Command::new("tar")
        .args(["-czf"])
        .arg(root.join("fixture.tar.gz"))
        .arg("-C")
        .arg(root.join("stage"))
        .arg("Revia.app")
        .env("COPYFILE_DISABLE", "1")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    fs::read(root.join("fixture.tar.gz")).unwrap()
}
fn manifest(bytes: &[u8], version: &str) -> (String, serde_json::Value) {
    let key = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let public = encode(&key.pk.to_box().unwrap().to_string());
    let comment = format!("timestamp:1\tfile:fixture.tar.gz\tversion:{version}");
    let sig = minisign::sign(
        Some(&key.pk),
        &key.sk,
        Cursor::new(bytes),
        Some(&comment),
        None,
    )
    .unwrap();
    (
        public,
        serde_json::json!({"version":version,"platforms":{"darwin-aarch64":{"signature":encode(&sig.to_string()),"url":"http://127.0.0.1:9/package"}}}),
    )
}
fn make_app(public: &str) -> tauri::App<tauri::test::MockRuntime> {
    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    context.config_mut().plugins.0.insert("updater".into(),serde_json::json!({"pubkey":public,"endpoints":["http://127.0.0.1:9/manifest"],"dangerousInsecureTransportProtocol":true,"requireSignedVersion":true}));
    tauri::test::mock_builder()
        .plugin(tauri_plugin_updater::Builder::new().pubkey(public).build())
        .build(context)
        .unwrap()
}
fn make_updater(
    app: &tauri::App<tauri::test::MockRuntime>,
    root: &Path,
) -> tauri_plugin_updater::Updater {
    app.updater_builder()
        .target("darwin-aarch64")
        .no_proxy()
        .endpoints(vec!["http://127.0.0.1:9/manifest".parse().unwrap()])
        .unwrap()
        .executable_path(root.join("installed/Revia.app/Contents/MacOS/Revia"))
        .build()
        .unwrap()
}
fn server(
    mut manifest: serde_json::Value,
    package: Vec<u8>,
) -> (String, std::thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    manifest["platforms"]["darwin-aarch64"]["url"] = format!("{address}/package").into();
    let body = serde_json::to_vec(&manifest).unwrap();
    let worker = std::thread::spawn(move || {
        let mut paths = Vec::new();
        for _ in 0..2 {
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
            let first = String::from_utf8_lossy(&header)
                .lines()
                .next()
                .unwrap()
                .to_string();
            let bytes = if first.starts_with("GET /manifest ") {
                &body
            } else {
                assert!(first.starts_with("GET /package "));
                &package
            };
            paths.push(first);
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",bytes.len()).unwrap();
            stream.write_all(bytes).unwrap();
        }
        paths
    });
    (address, worker)
}
#[cfg(target_os = "macos")]
#[test]
fn actual_signed_download_survives_restart_and_installs_offline_only_into_fixture_app() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = package(dir.path());
    let (public, manifest) = manifest(&bytes, "9.9.9");
    let app = make_app(&public);
    let data = dir.path().join("appdata");
    fs::create_dir(&data).unwrap();
    let target = dir.path().join("installed/Revia.app/Contents/MacOS/Revia");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, b"old fixture application").unwrap();
    let (address, requests) = server(manifest, bytes.clone());
    let online = app
        .updater_builder()
        .target("darwin-aarch64")
        .no_proxy()
        .endpoints(vec![format!("{address}/manifest").parse().unwrap()])
        .unwrap()
        .executable_path(&target)
        .build()
        .unwrap();
    let saved = tauri::async_runtime::block_on(async {
        let update = online.check().await.unwrap().unwrap();
        let downloaded = update.download(|_, _| {}, || {}).await.unwrap();
        assert_eq!(downloaded, bytes);
        update_package::save(&online, &update, &data, &downloaded).unwrap()
    });
    assert_eq!(requests.join().unwrap().len(), 2); // HTTP服务已结束，后续必须完全离线。
    let metadata = serde_json::to_vec(&saved).unwrap();
    let restarted: DownloadedUpdate = serde_json::from_slice(&metadata).unwrap();
    let offline = make_updater(&app, dir.path());
    let (update, cached) = update_package::restore(&offline, &data, &restarted, false).unwrap();
    assert_eq!(cached, bytes);
    assert_eq!(fs::read(&target).unwrap(), b"old fixture application");
    update.install(cached).unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"new fixture application");
    println!("FIXTURE_APP={}", target.display());
}
#[test]
fn bytes_metadata_version_and_channel_tampering_never_produce_an_installable_package() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = package(dir.path());
    let (public, manifest) = manifest(&bytes, "9.9.9-beta.1");
    let app = make_app(&public);
    let updater = make_updater(&app, dir.path());
    let update = updater.restore_update(manifest).unwrap().unwrap();
    let saved = update_package::save(&updater, &update, dir.path(), &bytes).unwrap();
    assert!(update_package::restore(&updater, dir.path(), &saved, false).is_err());
    assert!(update_package::restore(&updater, dir.path(), &saved, true).is_ok());
    let mut wrong = saved.clone();
    wrong.version = "10.0.0".into();
    wrong.manifest["version"] = "10.0.0".into();
    assert!(update_package::restore(&updater, dir.path(), &wrong, true).is_err());
    let path = dir
        .path()
        .join("update-packages")
        .join(format!("{}.package", saved.sha256));
    let mut damaged = bytes.clone();
    damaged.push(1);
    fs::write(&path, &damaged).unwrap();
    assert!(update_package::restore(&updater, dir.path(), &saved, true).is_err());
    let mut hash_forged = saved.clone();
    hash_forged.sha256 = revia_lib::m7::sha256_hex(&damaged);
    fs::write(
        dir.path()
            .join("update-packages")
            .join(format!("{}.package", hash_forged.sha256)),
        damaged,
    )
    .unwrap();
    assert!(update_package::restore(&updater, dir.path(), &hash_forged, true).is_err());
    wrong.sha256 = "../../outside".into();
    assert!(update_package::restore(&updater, dir.path(), &wrong, true).is_err());
}
#[test]
fn wrong_key_missing_version_and_failed_atomic_write_leave_original_package() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = package(dir.path());
    let (public, metadata) = manifest(&bytes, "9.9.9");
    let app = make_app(&public);
    let updater = make_updater(&app, dir.path());
    let update = updater.restore_update(metadata.clone()).unwrap().unwrap();
    let saved = update_package::save(&updater, &update, dir.path(), &bytes).unwrap();
    let (other_key, _) = manifest(&bytes, "9.9.9");
    let foreign = make_app(&other_key);
    let foreign_updater = make_updater(&foreign, dir.path());
    assert!(update_package::restore(&foreign_updater, dir.path(), &saved, false).is_err());
    let key = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let signature = minisign::sign(
        Some(&key.pk),
        &key.sk,
        Cursor::new(&bytes),
        Some("timestamp:1"),
        None,
    )
    .unwrap();
    let unsigned_app = make_app(&encode(&key.pk.to_box().unwrap().to_string()));
    let unsigned_updater = make_updater(&unsigned_app, dir.path());
    let mut unsigned = metadata;
    unsigned["platforms"]["darwin-aarch64"]["signature"] = encode(&signature.to_string()).into();
    let unsigned_update = unsigned_updater.restore_update(unsigned).unwrap().unwrap();
    assert!(update_package::save(&unsigned_updater, &unsigned_update, dir.path(), &bytes).is_err());
    assert_eq!(
        update_package::restore(&updater, dir.path(), &saved, false)
            .unwrap()
            .1,
        bytes
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let directory = dir.path().join("update-packages");
        let mode = fs::metadata(&directory).unwrap().permissions();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o555)).unwrap();
        let failed = update_package::save(&updater, &update, dir.path(), &bytes);
        fs::set_permissions(&directory, mode).unwrap();
        assert!(failed.is_err());
        assert_eq!(
            update_package::restore(&updater, dir.path(), &saved, false)
                .unwrap()
                .1,
            bytes
        );
    }
}
