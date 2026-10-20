//! Revia M0 原型：diff 渲染与运行环境验证。
//!
//! 边界：所有 Git 调用都在这一侧的 Rust 里完成（ADR-0002），前端只调用固定、带类型的命令。

pub mod cli;
pub mod commands;
pub mod diff;
pub mod env;
pub mod error;
pub mod git;
pub mod highlight;
pub mod measure;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 启动即打印一次关键路径，方便在没有窗口时核对日志落点。
    println!(
        "[revia-m0] 启动：cwd={:?} measureLog={:?} autoRun={} repo={:?}",
        std::env::current_dir().ok(),
        measure::target_path(),
        measure::auto_run_requested(),
        measure::measure_repo(),
    );
    let _ = std::io::Write::flush(&mut std::io::stdout());
    tauri::Builder::default()
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::resolve_worktree,
            commands::get_diff,
            commands::get_file_versions,
            commands::highlight_syntect,
            commands::syntect_support,
            commands::write_measurement,
            commands::measure_config,
            commands::debug_log,
        ])
        .run(tauri::generate_context!())
        .expect("Revia M0 原型启动失败");
}
