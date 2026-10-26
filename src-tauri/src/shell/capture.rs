//! Own one login-shell capture without creating files in the user's worktree.

use std::fs;
use std::io::{self, Read};
use std::os::fd::OwnedFd;
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use wait_timeout::ChildExt;

use super::{base_env, SHELL_TIMEOUT};

const DIAGNOSTIC_BYTES: usize = 2000;
const CAPTURE_BYTES: usize = DIAGNOSTIC_BYTES + 4;
const DRAIN_INTERVAL: Duration = Duration::from_millis(20);
static NEXT_CAPTURE: AtomicU64 = AtomicU64::new(1);

// The destination is an argument, never interpolated into shell source.
const DUMP_SCRIPT: &str = r#"typeset -a __revia_keep
__revia_keep=(PATH HOME SHELL USER LOGNAME LANG LC_ALL LC_CTYPE TMPDIR SSH_AUTH_SOCK \
DISPLAY XDG_CONFIG_HOME GIT_SSH GIT_SSH_COMMAND GIT_ASKPASS \
FNM_DIR FNM_NODE_DIST_MIRROR NVM_DIR NVM_BIN NVM_INC NVM_CD_FLAGS NVM_RC_VERSION \
VOLTA_HOME BUN_INSTALL DENO_INSTALL PNPM_HOME ASDF_DIR ASDF_DATA_DIR PYENV_ROOT)
umask 077
exec {__revia_fd}> "$1" || exit 1
for __revia_k in $__revia_keep; do
  if (( ${+parameters[$__revia_k]} )); then
    print -rn -- "$__revia_k=${(P)__revia_k}"$'\0' >&$__revia_fd || exit 1
  fi
done
exec {__revia_fd}>&-
kill -USR1 $$
"#;

pub(super) struct CapturedEnvironment {
    pub environment: Vec<u8>,
    pub diagnostics: Option<String>,
}

pub(super) fn run(
    shell: &Path,
    home: &Path,
    worktree: &Path,
) -> Result<CapturedEnvironment, String> {
    let files = CaptureFiles::create()
        .map_err(|error| format!("创建环境捕获临时目录失败：{error}"))?;
    let (mut diagnostics, stderr) = CaptureDiagnostics::new()
        .map_err(|error| format!("创建 shell 诊断通道失败：{error}"))?;
    let child = Command::new(shell)
        .current_dir(worktree)
        .args(["-l", "-i", "-c", DUMP_SCRIPT, "revia-env-capture"])
        .arg(files.environment_path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr))
        .env_clear()
        .envs(base_env(home))
        .spawn()
        .map_err(|error| format!("启动 {} 失败：{error}", shell.display()))?;
    // Declared after files: terminate/reap the shell before cleaning its capture.
    let mut child = CaptureChild { child, reaped: false };
    child.wait(&mut diagnostics)?;

    // USR1 is the existing completion protocol; a successful exit code is not required.
    let environment = fs::read(files.environment_path())
        .map_err(|error| format!("读取环境转储失败：{error}"))?;
    Ok(CapturedEnvironment { environment, diagnostics: diagnostics.finish() })
}

struct CaptureFiles {
    directory: PathBuf,
}

impl CaptureFiles {
    fn create() -> io::Result<Self> {
        // Keep paths absolute even if a login script changes the current directory.
        let root = fs::canonicalize(std::env::temp_dir())?;
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        for _ in 0..64 {
            let nonce = NEXT_CAPTURE.fetch_add(1, Ordering::Relaxed);
            let directory = root.join(format!("revia-env-{}-{stamp}-{nonce}", std::process::id()));
            match fs::DirBuilder::new().mode(0o700).create(&directory) {
                Ok(()) => return Ok(Self { directory }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(io::ErrorKind::AlreadyExists, "无法分配环境捕获临时目录"))
    }

    fn environment_path(&self) -> PathBuf {
        self.directory.join("environment")
    }
}

impl Drop for CaptureFiles {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

struct CaptureDiagnostics {
    reader: UnixStream,
    bytes: Vec<u8>,
    error: Option<String>,
}

impl CaptureDiagnostics {
    fn new() -> io::Result<(Self, OwnedFd)> {
        let (reader, writer) = UnixStream::pair()?;
        Ok((Self { reader, bytes: Vec::with_capacity(CAPTURE_BYTES), error: None }, writer.into()))
    }

    fn retain_prefix(&mut self, bytes: &[u8]) {
        let keep = bytes.len().min(CAPTURE_BYTES - self.bytes.len());
        self.bytes.extend_from_slice(&bytes[..keep]);
    }

    /// Drain even after the retained prefix is full so stderr cannot block the shell.
    fn drain_for(&mut self, timeout: Duration) -> bool {
        if let Err(error) = self.reader.set_read_timeout(Some(timeout)) {
            self.error = Some(error.to_string());
            return false;
        }
        let mut bytes = [0u8; 8192];
        match self.reader.read(&mut bytes) {
            Ok(0) => false,
            Ok(read) => {
                self.retain_prefix(&bytes[..read]);
                true
            }
            Err(error) if matches!(error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
            ) => true,
            Err(error) => {
                self.error = Some(error.to_string());
                false
            }
        }
    }

    fn finish(mut self) -> Option<String> {
        // A descendant may still own stderr. Read only the missing prefix, never wait for EOF.
        if self.bytes.len() < CAPTURE_BYTES && self.error.is_none() {
            match self.reader.set_nonblocking(true) {
                Ok(()) => {
                    let mut bytes = [0u8; CAPTURE_BYTES];
                    while self.bytes.len() < CAPTURE_BYTES {
                        let remaining = CAPTURE_BYTES - self.bytes.len();
                        match self.reader.read(&mut bytes[..remaining]) {
                            Ok(0) => break,
                            Ok(read) => self.retain_prefix(&bytes[..read]),
                            Err(error) if matches!(error.kind(),
                                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
                            ) => break,
                            Err(error) => {
                                self.error = Some(error.to_string());
                                break;
                            }
                        }
                    }
                }
                Err(error) => self.error = Some(error.to_string()),
            }
        }
        let text = String::from_utf8_lossy(&self.bytes);
        let text = match self.error {
            Some(error) => format!("shell 诊断输出读取不完整：{error}\n{}", text.trim()),
            None => text.trim().to_string(),
        };
        if text.is_empty() {
            return None;
        }
        if text.len() <= DIAGNOSTIC_BYTES {
            return Some(text);
        }
        let mut end = DIAGNOSTIC_BYTES;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        Some(format!("{}…", &text[..end]))
    }
}

struct CaptureChild {
    child: Child,
    reaped: bool,
}

impl CaptureChild {
    fn wait(&mut self, diagnostics: &mut CaptureDiagnostics) -> Result<(), String> {
        let started = Instant::now();
        let status = loop {
            let remaining = SHELL_TIMEOUT.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                break None;
            }
            if let Some(status) = self.child.try_wait()
                .map_err(|error| format!("等待 shell 失败：{error}"))?
            {
                break Some(status);
            }
            if !diagnostics.drain_for(remaining.min(DRAIN_INTERVAL)) {
                // EOF or an unreadable channel does not mean the shell has exited.
                let remaining = SHELL_TIMEOUT.saturating_sub(started.elapsed());
                if remaining.is_zero() {
                    break None;
                }
                break self.child.wait_timeout(remaining)
                    .map_err(|error| format!("等待 shell 失败：{error}"))?;
            }
        };
        match status {
            Some(_) => {
                self.reaped = true;
                Ok(())
            }
            None => Err(format!(
                "登录 shell 在 {} 毫秒内没有返回，已终止并使用最小环境",
                SHELL_TIMEOUT.as_millis()
            )),
        }
    }
}

impl Drop for CaptureChild {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let home = root.join("home");
        // Spaces and quotes must remain ordinary path bytes, not shell source.
        let worktree = root.join("worktree ' with spaces");
        fs::create_dir(&home).unwrap();
        fs::create_dir(&worktree).unwrap();
        (temp, home, worktree)
    }

    #[test]
    fn capture_preserves_worktree_files_and_bounds_large_stderr() {
        let (_temp, home, worktree) = fixture();
        fs::write(worktree.join(".revia-env.dump"), b"user-owned").unwrap();
        fs::write(home.join(".zshrc"),
            "print -rn -- 'diagnostic prefix' >&2\n/usr/bin/head -c 1048576 /dev/zero >&2\nexport PNPM_HOME=fixture-only\n").unwrap();
        let captured = run(Path::new("/bin/zsh"), &home, &worktree).unwrap();
        assert!(captured.environment.split(|byte| *byte == 0).any(|item| item == b"PNPM_HOME=fixture-only"));
        let diagnostic = captured.diagnostics.unwrap();
        assert!(diagnostic.starts_with("diagnostic prefix"));
        assert!(diagnostic.len() <= DIAGNOSTIC_BYTES + "…".len());
        assert_eq!(fs::read(worktree.join(".revia-env.dump")).unwrap(), b"user-owned");
        assert_eq!(fs::read_dir(&worktree).unwrap().count(), 1);
    }

    #[test]
    fn capture_does_not_wait_for_inherited_stderr_eof() {
        let (_temp, home, worktree) = fixture();
        fs::write(home.join(".zshrc"), "/bin/sleep 3 &!\nprint -rn -- ready >&2\n").unwrap();
        let started = Instant::now();
        let captured = run(Path::new("/bin/zsh"), &home, &worktree).unwrap();
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(captured.diagnostics.unwrap().contains("ready"));
    }

    #[test]
    fn capture_timeout_reaps_shell_and_releases_private_files() {
        let (_temp, home, worktree) = fixture();
        fs::write(home.join(".zshrc"), "while true; do :; done\n").unwrap();
        let started = Instant::now();
        let error = run(Path::new("/bin/zsh"), &home, &worktree).err().expect("shell must time out");
        assert!(error.contains("没有返回"));
        assert!(started.elapsed() < SHELL_TIMEOUT + Duration::from_secs(2));
        let files = CaptureFiles::create().unwrap();
        let directory = files.directory.clone();
        assert!(directory.is_absolute());
        drop(files);
        assert!(!directory.exists());
    }
}
