//! Revia：轻量的 macOS Git 客户端。
//!
//! 边界：所有 Git 调用都在 Rust 层完成（ADR-0002）；前端只调用固定、带类型的命令。

pub mod acceptance;
pub mod commands;
pub mod error;
pub mod git;
pub mod session;
pub mod settings;
pub mod shell;
pub mod watch;

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
            // Explicit diagnostic runs can isolate settings without affecting normal launches.
            let measuring = commands::soak_seconds().is_some()
                || commands::is_measure_auto_run()
                || std::env::var_os("REVIA_SMOKE").is_some();
            let acceptance_dir = acceptance::data_directory(&data_dir)?;
            let data_dir = if let Some(directory) = &acceptance_dir {
                directory.clone()
            } else if measuring {
                match std::env::var_os("REVIA_MEASURE_DATA_DIR").map(std::path::PathBuf::from) {
                    Some(path) if path.is_absolute() => path,
                    Some(_) => return Err("测量数据目录必须是绝对路径".into()),
                    None if commands::soak_seconds().is_some() => return Err("长测必须指定独立数据目录".into()),
                    None => data_dir,
                }
            } else {
                data_dir
            };
            commands::log_measure(&format!(
                "Rust 启动：data_dir={} autoRun={:?}",
                data_dir.display(),
                std::env::var("REVIA_MEASURE").ok()
            ));
            if commands::is_measure_auto_run()
                && commands::soak_seconds().is_none()
                && std::env::var_os("REVIA_SMOKE").is_none()
            {
                if let Some(window) = app.get_webview_window("main") {
                    window.set_title(&format!("Revia - 性能测量 PID {}", std::process::id()))?;
                }
            }
            if commands::soak_seconds().is_some() {
                if let Some(window) = app.get_webview_window("main") {
                    window.set_title(&format!("Revia - 长测 PID {}", std::process::id()))?;
                    // 诊断时可选：窗口保持在最前并出现在所有桌面空间，避免被其他窗口遮挡导致长测因不可见而中断。
                    if std::env::var_os("REVIA_SOAK_ON_TOP").is_some() {
                        window.set_always_on_top(true)?;
                        #[cfg(target_os = "macos")]
                        window.set_visible_on_all_workspaces(true)?;
                    }
                }
            }
            let mut state = AppState::new(data_dir);
            state.acceptance = acceptance_dir.map(acceptance::Acceptance::new).transpose()?.map(std::sync::Arc::new);
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_repos,
            commands::list_settings,
            commands::add_repo,
            commands::select_repo,
            commands::set_diff_font_size,
            commands::open_worktree,
            commands::watch_worktree,
            commands::stop_watch,
            commands::watch_status,
            commands::list_changes,
            commands::get_rows,
            commands::highlight_range,
            commands::row_index_for_line,
            commands::syntax_categories,
            commands::command_log_snapshot,
            commands::stage_file,
            commands::unstage_file,
            commands::stage_hunk,
            commands::unstage_hunk,
            commands::change_entry,
            commands::measure_config,
            commands::write_measurement,
            commands::debug_log,
        ])
        .run(tauri::generate_context!())
        .expect("Revia 启动失败");
}
