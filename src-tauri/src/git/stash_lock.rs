//! Fixed Git update-ref protocol for deleting refs/stash. Core Git owns both
//! loose and packed-ref locks; the callback only validates the locked reflog.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use wait_timeout::ChildExt;

use super::{CommandRecord, Git, LOG_OUTPUT_LIMIT, mask_credentials};
use crate::error::{AppError, ErrorKind, Result};
use crate::platform::output_pipe::OutputPipe;

fn protocol_error() -> AppError {
    AppError::coded(
        ErrorKind::Git,
        "errors.stashTransaction.protocol",
        "Git 的 stash 删除事务未按预期完成，请刷新后核对",
        &[],
    )
}

fn acknowledgement(
    pipe: &mut OutputPipe,
    expected: &[u8],
    started: Instant,
    limit: Duration,
    output: &mut Vec<u8>,
) -> Result<()> {
    let mut line = Vec::new();
    loop {
        if started.elapsed() >= limit {
            return Err(protocol_error());
        }
        let mut byte = [0];
        match pipe.read(&mut byte) {
            Ok(0) => return Err(protocol_error()),
            Ok(_) => {
                if output.len() < LOG_OUTPUT_LIMIT {
                    output.push(byte[0]);
                }
                line.push(byte[0]);
                if line.len() > 1024 {
                    return Err(protocol_error());
                }
                if byte[0] == b'\n' {
                    return if line == expected {
                        Ok(())
                    } else {
                        Err(protocol_error())
                    };
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::Interrupted
                ) =>
            {
                std::thread::sleep(Duration::from_millis(2))
            }
            Err(error) => return Err(error.into()),
        }
    }
}

pub(crate) fn delete_reference(
    git: &Git,
    expected_oid: &str,
    validate: impl FnOnce() -> Result<()>,
) -> Result<()> {
    // Full hex OIDs only: callers cannot supply another ref, protocol line or
    // shell command. Hooks/environment remain the user's normal Git behavior.
    if !matches!(expected_oid.len(), 40 | 64)
        || !expected_oid.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(protocol_error());
    }
    let started = Instant::now();
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let limit = Duration::from_secs(120);
    let args = ["update-ref", "--no-deref", "--stdin"];
    let (mut stdout, stdout_writer) = OutputPipe::new()?;
    let (mut stderr, stderr_writer) = OutputPipe::new()?;
    let mut command = Command::new(&git.program);
    command
        .current_dir(&git.directory)
        .args(args)
        .env_clear()
        .envs(git.env.iter().cloned())
        .stdin(Stdio::piped())
        .stdout(stdout_writer)
        .stderr(stderr_writer);
    let mut child = command.spawn()?;
    drop(command);
    let done = Arc::new(AtomicBool::new(false));
    let finished = done.clone();
    let reader = std::thread::spawn(move || {
        let mut output = Vec::new();
        let mut bytes = [0; 4096];
        loop {
            match stderr.read(&mut bytes) {
                Ok(0) => break,
                Ok(count) => {
                    let take = count.min(LOG_OUTPUT_LIMIT.saturating_sub(output.len()));
                    output.extend_from_slice(&bytes[..take]);
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(_) => break,
            }
            if finished.load(Ordering::Acquire) {
                break;
            }
        }
        output
    });
    let mut output = Vec::new();
    let mut commit_started = false;
    let result = (|| {
        let mut input = child.stdin.take().ok_or_else(protocol_error)?;
        input
            .write_all(format!("start\ndelete refs/stash {expected_oid}\nprepare\n").as_bytes())?;
        input.flush()?;
        acknowledgement(&mut stdout, b"start: ok\n", started, limit, &mut output)?;
        acknowledgement(&mut stdout, b"prepare: ok\n", started, limit, &mut output)?;
        // reference-transaction hook stdout is redirected to stderr by Git;
        // its output cannot spoof this acknowledgement (Git hook.h contract).
        if let Err(error) = validate() {
            input.write_all(b"abort\n")?;
            input.flush()?;
            acknowledgement(&mut stdout, b"abort: ok\n", started, limit, &mut output)?;
            return Err(error);
        }
        commit_started = true;
        input.write_all(b"commit\n")?;
        input.flush()?;
        acknowledgement(&mut stdout, b"commit: ok\n", started, limit, &mut output)
    })();
    let waited = child.wait_timeout(limit.saturating_sub(started.elapsed()));
    let timed_out = !matches!(waited, Ok(Some(_)));
    if timed_out {
        let _ = child.kill();
    }
    let status = child.wait();
    done.store(true, Ordering::Release);
    let stderr = reader.join().unwrap_or_default();
    let exit = status
        .as_ref()
        .ok()
        .and_then(|status| status.code())
        .unwrap_or(-1);
    let stderr_text = String::from_utf8_lossy(&stderr);
    let (logged_output, output_truncated) = super::cap_output_text(
        &mask_credentials(&format!("{}{}", String::from_utf8_lossy(&output), stderr_text)),
        stderr.len() == LOG_OUTPUT_LIMIT,
    );
    git.log.record(CommandRecord {
        sequence: 0,
        worktree_id: git.worktree_id.clone(),
        started_at_ms: at,
        program: git.program.display().to_string(),
        args: args.iter().map(|arg| arg.to_string()).collect(),
        directory: git.directory.display().to_string(),
        exit_code: exit,
        elapsed_ms: started.elapsed().as_millis() as u64,
        timed_out,
        stdout_bytes: output.len(),
        stderr_excerpt: mask_credentials(&stderr_text).chars().take(2000).collect(),
        output: logged_output,
        output_truncated,
    });
    let result = if timed_out || exit != 0 {
        Err(AppError::coded(
            if commit_started {
                ErrorKind::Git
            } else {
                ErrorKind::Rejected
            },
            "errors.stashTransaction.backend",
            "无法安全更新 stash：{detail}",
            &[("detail", stderr_text.trim().to_string())],
        ))
    } else {
        result
    };
    let result = result.and_then(|()| {
        // Core Git reports a reflog unlink failure as a warning. A successful
        // protocol receipt alone therefore cannot prove the deletion complete.
        // An intervening external stash is also an uncertain receipt, never a
        // reason to delete again.
        let reference = git.run(&["show-ref", "--verify", "--quiet", "refs/stash"])?;
        let log = git.run(&["reflog", "exists", "refs/stash"])?;
        if reference.exit_code != 1 || log.exit_code != 1 {
            return Err(protocol_error());
        }
        Ok(())
    });
    if commit_started || timed_out {
        result.map_err(crate::recovery::pending_write)
    } else {
        result
    }
}
