use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;

use revia_lib::git::CommandLog;
use revia_lib::session::WorktreeSession;
use revia_lib::shell::ShellEnv;

fn create_bench_fixture(root: &std::path::Path, name: &str, file_count: usize) -> PathBuf {
    let repo = root.join(name);
    fs::create_dir_all(&repo).unwrap();
    assert!(Command::new("git")
        .args(["init", "-q"])
        .current_dir(&repo)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args(["config", "user.name", "Bench"])
        .current_dir(&repo)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args(["config", "user.email", "{{EMAIL_60i11p0p}}"])
        .current_dir(&repo)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args(["config", "commit.gpgsign", "false"])
        .current_dir(&repo)
        .status()
        .unwrap()
        .success());

    // 初始文件与提交
    for i in 0..file_count {
        fs::write(repo.join(format!("file_{i}.txt")), format!("content {i}\n")).unwrap();
    }
    assert!(Command::new("git")
        .args(["add", "-A"])
        .current_dir(&repo)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args(["commit", "-q", "-m", "init"])
        .current_dir(&repo)
        .status()
        .unwrap()
        .success());

    // 生成若干改动文件
    let dirty_count = (file_count / 10).max(3);
    for i in 0..dirty_count {
        fs::write(repo.join(format!("file_{i}.txt")), format!("content {i} modified\n")).unwrap();
    }
    repo
}

#[test]
fn test_m9_measure_open_and_switch_times() {
    let _temp_guard;
    let (diffs, large) = match std::env::var("REVIA_BENCH_FIXTURES") {
        Ok(dir) => {
            let base = PathBuf::from(dir);
            let d = base.join("diffs");
            let l = base.join("large");
            if !d.exists() || !l.exists() {
                panic!(
                    "REVIA_BENCH_FIXTURES 指定的目录缺少 diffs 或 large 子仓库: {}",
                    base.display()
                );
            }
            (d, l)
        }
        Err(_) => {
            let temp = tempfile::tempdir().unwrap();
            let d = create_bench_fixture(temp.path(), "diffs", 20);
            let l = create_bench_fixture(temp.path(), "large", 100);
            _temp_guard = temp;
            (d, l)
        }
    };

    println!("\n===== M9 打开与切换仓库分段耗时实测 =====");
    let log = Arc::new(CommandLog::default());

    // 1. diffs: 首次打开
    let t0 = Instant::now();
    let env_diffs = ShellEnv::minimal(&diffs);
    let session_diffs = WorktreeSession::open("diffs-wt", &diffs, env_diffs, Arc::clone(&log)).unwrap();
    let diffs_session_ready = t0.elapsed();

    let t1 = Instant::now();
    let snapshot_diffs = session_diffs.refresh().unwrap();
    let diffs_list_data = t1.elapsed();
    let diffs_first_open = t0.elapsed();

    println!(
        "【diffs 首次打开】会话就绪: {:.2}ms | 数据列表 ({} 文件): {:.2}ms | 合计: {:.2}ms",
        diffs_session_ready.as_secs_f64() * 1000.0,
        snapshot_diffs.entries.len(),
        diffs_list_data.as_secs_f64() * 1000.0,
        diffs_first_open.as_secs_f64() * 1000.0,
    );

    // 2. large: 首次打开
    let t2 = Instant::now();
    let env_large = ShellEnv::minimal(&large);
    let session_large = WorktreeSession::open("large-wt", &large, env_large, Arc::clone(&log)).unwrap();
    let large_session_ready = t2.elapsed();

    let t3 = Instant::now();
    let snapshot_large = session_large.refresh().unwrap();
    let large_list_data = t3.elapsed();
    let large_first_open = t2.elapsed();

    println!(
        "【large 首次打开】会话就绪: {:.2}ms | 数据列表 ({} 文件): {:.2}ms | 合计: {:.2}ms",
        large_session_ready.as_secs_f64() * 1000.0,
        snapshot_large.entries.len(),
        large_list_data.as_secs_f64() * 1000.0,
        large_first_open.as_secs_f64() * 1000.0,
    );

    // 3. 切换回 diffs
    let t4 = Instant::now();
    let snapshot_diffs_switch = session_diffs.refresh().unwrap();
    let diffs_switch = t4.elapsed();

    println!(
        "【切换到 diffs】数据刷新 ({} 文件): {:.2}ms",
        snapshot_diffs_switch.entries.len(),
        diffs_switch.as_secs_f64() * 1000.0,
    );

    // 4. 切换回 large
    let t5 = Instant::now();
    let snapshot_large_switch = session_large.refresh().unwrap();
    let large_switch = t5.elapsed();

    println!(
        "【切换到 large】数据刷新 ({} 文件): {:.2}ms",
        snapshot_large_switch.entries.len(),
        large_switch.as_secs_f64() * 1000.0,
    );
    println!("===============================================================\n");
}
