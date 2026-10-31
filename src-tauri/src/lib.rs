//! Revia：轻量的 macOS Git 客户端。
//!
//! 边界：所有 Git 调用都在 Rust 层完成（ADR-0002）；前端只调用固定、带类型的命令。

pub mod acceptance;
pub mod autostash;
pub mod batch;
pub mod commands;
pub mod commit;
pub mod commit_flow;
pub mod commit_snapshot;
pub mod commit_warnings;
pub mod error;
pub mod external_tools;
pub mod file_history;
pub mod git;
pub mod history;
mod history_sequence;
pub mod m7;
pub mod merge;
pub mod operation;
pub mod operation_flow;
pub mod platform;
pub mod recovery;
pub mod remote_ops;
pub mod review;
pub mod rewrite;
pub mod session;
pub mod settings;
pub mod shell;
pub mod stash;
pub mod stash_archive;
pub mod stash_transaction;
pub mod watch;
pub mod worktree_status;
pub mod write_guard;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
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
                    None if commands::soak_seconds().is_some() => {
                        return Err("长测必须指定独立数据目录".into());
                    }
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
            let data_dir_for_update = data_dir.clone();
            let mut state = AppState::new(data_dir);
            state.acceptance = acceptance_dir
                .map(acceptance::Acceptance::new)
                .transpose()?
                .map(std::sync::Arc::new);

            // 回收区到期自动清理：启动时一次，之后每小时一次（Q9、规格实现决策 4）。
            // 失败不静默：状态持久化到回收区目录，普通模式下也能查询并显示（不依赖测量日志）。
            let recovery = state.recovery.clone();
            let stash_archive = state.stash_archive.clone();
            let startup_purge = commands::purge_expired_now(&recovery);
            match &startup_purge {
                Ok(removed) => commands::log_measure(&format!("回收区启动清理：删除 {removed} 条")),
                Err(error) => commands::log_measure(&format!("回收区启动清理失败：{error}")),
            }
            match stash_archive.purge_expired() {
                Ok(0) => {}
                Ok(removed) => {
                    commands::log_measure(&format!("stash 归档启动清理：删除 {removed} 条"))
                }
                Err(error) => commands::log_measure(&format!("stash 归档启动清理失败：{error}")),
            }
            let periodic = recovery.clone();
            let periodic_stash_archive = stash_archive.clone();
            std::thread::Builder::new()
                .name("revia-recovery-purge".into())
                .spawn(move || {
                    loop {
                        std::thread::sleep(std::time::Duration::from_secs(3600));
                        match commands::purge_expired_now(&periodic) {
                            Ok(0) => {}
                            Ok(removed) => {
                                commands::log_measure(&format!("回收区定期清理：删除 {removed} 条"))
                            }
                            Err(error) => {
                                commands::log_measure(&format!("回收区定期清理失败：{error}"))
                            }
                        }
                        match periodic_stash_archive.purge_expired() {
                            Ok(0) => {}
                            Ok(removed) => commands::log_measure(&format!(
                                "stash 归档定期清理：删除 {removed} 条"
                            )),
                            Err(error) => {
                                commands::log_measure(&format!("stash 归档定期清理失败：{error}"))
                            }
                        }
                    }
                })
                .ok();

            crate::m7::update::spawn_auto_check(app.handle().clone(), data_dir_for_update, std::sync::Arc::clone(&state.m7_update));
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_repos,
            commands::list_settings,
            commands::add_repo,
            commands::select_repo,
            commands::set_diff_font_size,
            commands::update_settings,
            commands::reload_environment,
            commands::list_worktree_status,
            commands::open_worktree,
            commands::watch_worktree,
            commands::stop_watch,
            commands::watch_status,
            commands::list_changes,
            commands::set_review_mark,
            commands::get_rows,
            commands::side_by_side_layout,
            commands::side_lines,
            commands::stage_segment,
            commands::unstage_segment,
            commands::discard_segment,
            commands::highlight_range,
            commands::highlight_text,
            commands::row_index_for_line,
            commands::syntax_categories,
            commands::command_log_snapshot,
            commands::stage_file,
            commands::unstage_file,
            commands::stage_files,
            commands::unstage_files,
            commands::discard_files,
            commands::stage_hunk,
            commands::unstage_hunk,
            commands::discard_file,
            commands::discard_hunk,
            commands::list_recovery,
            commands::inspect_recovery,
            commands::restore_recovery,
            commands::save_recovery_as,
            commands::recovery_text_diff,
            commands::recovery_maintenance,
            commands::recovery_save_as_owner,
            commands::clear_recovery,
            commands::purge_recovery,
            commands::change_entry,
            commands::measure_config,
            commands::write_measurement,
            commands::debug_log,
            commands::set_exit_blocked,
            commands::request_exit,
            commands::commit_context,
            commands::amend_head_message,
            commands::save_commit_draft,
            commands::clear_commit_draft,
            commands::execute_commit,
            commands::commit_warnings,
            commands::list_conflicts,
            commands::merge_open,
            commands::merge_apply,
            commands::merge_accept_side,
            commands::history_open,
            commands::history_close,
            commands::history_refs,
            commands::history_log_page,
            commands::history_commit,
            commands::history_commit_files,
            commands::history_file_rows,
            commands::history_file_side_layout,
            commands::history_file_side_lines,
            commands::history_file_highlight,
            commands::history_merge_files,
            commands::history_merge_view,
            commands::branches_overview,
            commands::repo_files,
            commands::checkout_branch,
            commands::create_branch,
            commands::fetch_remotes,
            commands::update_project,
            commands::push_preview,
            commands::push_branch,
            commands::check_remote,
            commands::stash_list,
            commands::stash_preflight,
            commands::stash_inspect,
            commands::stash_save,
            commands::stash_apply,
            commands::stash_drop,
            commands::stash_clear,
            commands::stash_archives,
            commands::stash_archive_restore,
            commands::autostash_pending,
            commands::autostash_restore,
            commands::autostash_forget,
            commands::operation_state,
            commands::operation_execute,
            commands::rewrite_preflight,
            commands::rewrite_range,
            commands::rewrite_cherry_pick,
            commands::rewrite_revert,
            commands::rewrite_reset,
            commands::rewrite_reword,
            commands::rewrite_interactive,
            commands::file_history,
            commands::file_blame,
            commands::file_previous_revision,
            commands::external_tool_choices,
            commands::external_tool_launch,
            commands::system_language,
            m7::commands::m7_list_imported_themes,
            m7::commands::m7_import_theme,
            m7::commands::m7_remove_imported_theme,
            m7::commands::m7_changelist_view,
            m7::commands::m7_changelist_change,
            m7::commands::m7_changelist_warnings,
            m7::commands::m7_open_editable,
            m7::commands::m7_save_editable,
            m7::commands::m7_save_edit_draft,
            m7::commands::m7_load_edit_draft,
            m7::commands::m7_clear_edit_draft,
            m7::commands::m7_list_edit_drafts,
            m7::commands::m7_terminal_open,
            m7::commands::m7_terminal_list,
            m7::commands::m7_terminal_write,
            m7::commands::m7_terminal_resize,
            m7::commands::m7_terminal_snapshot,
            m7::commands::m7_terminal_close,
            m7::commands::m7_terminal_close_worktree,
            m7::commands::m7_terminal_tabs,
            m7::commands::m7_terminal_save_tabs,
            m7::commands::m7_clone_start,
            m7::commands::m7_clone_cancel,
            m7::commands::m7_clone_active,
            m7::commands::m7_clone_register,
            m7::commands::m7_github_status,
            m7::commands::m7_github_device_start,
            m7::commands::m7_github_device_poll,
            m7::commands::m7_github_login_token,
            m7::commands::m7_github_logout,
            m7::commands::m7_github_bind_repo,
            m7::commands::m7_github_remotes,
            m7::commands::m7_github_pr_list,
            m7::commands::m7_github_pr_detail,
            m7::commands::m7_github_pr_files,
            m7::commands::m7_github_pr_diff,
            m7::commands::m7_github_pr_comments,
            m7::commands::m7_github_pr_comment_create,
            m7::commands::m7_github_device_cancel,
            m7::commands::m7_github_repo_key,
            m7::commands::m7_github_review_drafts,
            m7::commands::m7_github_review_draft_save,
            m7::commands::m7_github_review_draft_delete,
            m7::commands::m7_github_review_submit,
            m7::commands::m7_github_pr_checkout,
            m7::commands::m7_github_pr_create,
            m7::commands::m7_github_pr_merge,
            m7::commands::m7_github_receipts,
            m7::commands::m7_github_protection_refresh,
            m7::commands::m7_export_diagnostics,
            m7::commands::m7_preview_diagnostics,
            m7::commands::m7_diagnostics_summary,
            m7::commands::m7_diagnostics_clear,
            m7::commands::m7_update_status,
            m7::commands::m7_update_check,
            m7::commands::m7_update_download,
            m7::commands::m7_update_install,
            m7::commands::m7_update_restart,
        ])
        .build(tauri::generate_context!())
        .expect("Revia 启动失败");
    platform::install_exit_interceptor(app.handle()).expect("原生退出保护安装失败");
    app.run(|app, event| {
        use tauri::{Emitter, Manager};
        // Runtime 退出的最后一道保护；macOS terminate: 在平台层先转交同一前端驱动。
        // 前端窗口已销毁时放行，避免进程无法结束。
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            let Some(window) = app.get_webview_window("main") else {
                return;
            };
            // 只有前端明确报告「有在途写或待核对结果」时才阻止。
            let blocked = app
                .try_state::<AppState>()
                .map(|state| state.exit_blocked())
                .unwrap_or(false);
            if blocked {
                api.prevent_exit();
                let _ = window.emit("revia://exit-requested", ());
            }
        }
    });
}
