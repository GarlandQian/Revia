//! 测试只继承平台的程序搜索路径；Git配置与用户目录始终指向本次fixture。
use revia_lib::shell::{EnvCaptureInfo, ShellEnv};
use std::{collections::HashMap, path::Path};

pub fn shell_env(home: &Path, worktree: &Path, global_config: &Path) -> ShellEnv {
    let path: Vec<String> = std::env::var_os("PATH")
        .map(|value| {
            std::env::split_paths(&value)
                .map(|p| p.to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    let path_value = std::env::join_paths(path.iter().map(Path::new)).expect("fixture PATH");
    let vars = HashMap::from([
        ("HOME".into(), home.to_string_lossy().into_owned()),
        ("USERPROFILE".into(), home.to_string_lossy().into_owned()),
        (
            "GIT_CONFIG_GLOBAL".into(),
            global_config.to_string_lossy().into_owned(),
        ),
        ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
        ("GIT_TERMINAL_PROMPT".into(), "0".into()),
        ("PATH".into(), path_value.to_string_lossy().into_owned()),
    ]);
    // 只传递Windows运行程序所需的系统位置与扩展名，不继承凭据或Git配置。
    #[cfg(windows)]
    let vars = {
        let mut vars = vars;
        for name in ["SystemRoot", "WINDIR", "COMSPEC", "PATHEXT"] {
            if let Ok(value) = std::env::var(name) {
                vars.insert(name.into(), value);
            }
        }
        vars
    };
    ShellEnv {
        path: path.clone(),
        info: EnvCaptureInfo {
            worktree: worktree.to_string_lossy().into_owned(),
            shell: if cfg!(windows) { "cmd.exe" } else { "/bin/sh" }.into(),
            elapsed_ms: 0,
            timed_out: false,
            used_fallback: true,
            var_count: vars.len(),
            path_entries: path,
            fallback_reason: None,
            fallback_detail: None,
        },
        vars,
    }
}
