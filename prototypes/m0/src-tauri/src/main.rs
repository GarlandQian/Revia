//! M0 验证入口。
//!
//! 两种用法：
//! - 无参数：启动 Tauri 原型窗口。
//! - `env-check <hook|mergetool>`：命令行验证运行环境，输出 JSON，供从 Finder 双击的实测和脚本核对。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let mut args = std::env::args().skip(1);
    if let Some(first) = args.next() {
        if first == "env-check" {
            let mode = args.next().unwrap_or_else(|| "hook".to_string());
            let code = revia_m0_lib::cli::run(&mode);
            std::process::exit(code);
        }
    }
    revia_m0_lib::run()
}
