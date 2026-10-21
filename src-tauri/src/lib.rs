//! Revia：轻量的 macOS Git 客户端。
//!
//! 边界：所有 Git 调用都在 Rust 层完成（ADR-0002）；前端只调用固定、带类型的命令。

pub mod commands;
pub mod error;
pub mod git;
pub mod session;
pub mod settings;
pub mod shell;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            use tauri::Manager;
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|error| format!("找不到数据目录：{error}"))?;
            // Only the explicit soak harness may isolate persistent application settings.
            let data_dir = if commands::soak_seconds().is_some() {
                std::env::var_os("REVIA_SOAK_DATA_DIR")
                    .map(std::path::PathBuf::from)
                    .ok_or("长测必须指定独立数据目录")?
            } else {
                data_dir
            };
            commands::log_measure(&format!(
                "Rust 启动：data_dir={} autoRun={:?}",
                data_dir.display(),
                std::env::var("REVIA_MEASURE").ok()
            ));
            if commands::soak_seconds().is_some() {
                if let Some(window) = app.get_webview_window("main") {
                    window.set_title(&format!("Revia - long test PID {}", std::process::id()))?;
                }
            }
            app.manage(AppState::new(data_dir));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_repos,
            commands::list_settings,
            commands::add_repo,
            commands::select_repo,
            commands::set_diff_font_size,
            commands::open_worktree,
            commands::list_changes,
            commands::get_rows,
            commands::highlight_range,
            commands::row_index_for_line,
            commands::syntax_categories,
            commands::command_log_snapshot,
            commands::change_entry,
            commands::measure_config,
            commands::write_measurement,
            commands::debug_log,
        ])
        .run(tauri::generate_context!())
        .expect("Revia 启动失败");
}
