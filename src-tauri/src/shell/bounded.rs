//! 有界的子进程输出读取：进程回收后只再排空短暂窗口，超过上限仍继续排空但不保存。
//!
//! `OutputPipe` 在 Unix 上带 20ms 读超时、Windows 上用 `PeekNamedPipe` 非阻塞探测；
//! 这里在进程回收后设置 `finished`，读取线程最多再排空 `DRAIN_GRACE`，因此即使 profile
//! 启动的子进程仍持有写端，环境捕获也会在允许时间内结束（R1）。

use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// 子进程回收后继续排空 stderr 的最长时间。
pub(super) const DRAIN_GRACE: Duration = Duration::from_millis(250);

pub(super) struct BoundedOutput {
    pub bytes: Vec<u8>,
    pub truncated: bool,
}

pub(super) fn read_bounded(
    mut pipe: impl Read,
    limit: usize,
    finished: &AtomicBool,
) -> BoundedOutput {
    let mut bytes = Vec::with_capacity(limit.min(8 * 1024));
    let mut truncated = false;
    let mut buffer = [0u8; 4096];
    let mut drain_deadline: Option<Instant> = None;

    loop {
        match pipe.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                if bytes.len() < limit {
                    let keep = (limit - bytes.len()).min(count);
                    bytes.extend_from_slice(&buffer[..keep]);
                    if keep < count {
                        truncated = true;
                    }
                } else {
                    truncated = true;
                }
                if finished.load(Ordering::Acquire) {
                    let deadline = *drain_deadline
                        .get_or_insert_with(|| Instant::now() + DRAIN_GRACE);
                    if Instant::now() >= deadline {
                        break;
                    }
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                if finished.load(Ordering::Acquire) {
                    let deadline = *drain_deadline
                        .get_or_insert_with(|| Instant::now() + DRAIN_GRACE);
                    if Instant::now() >= deadline {
                        break;
                    }
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => break,
        }
    }

    BoundedOutput { bytes, truncated }
}

#[cfg(all(test, unix))]
mod tests {
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use crate::platform::output_pipe::OutputPipe;

    use super::*;

    fn capture(command: &mut Command, limit: usize, finished: Arc<AtomicBool>) -> (BoundedOutput, Duration) {
        let (mut pipe, writer) = OutputPipe::new().unwrap();
        command.stdout(Stdio::null()).stderr(writer).stdin(Stdio::null());
        let mut child = command.spawn().unwrap();
        let started = Instant::now();
        let reader_finished = Arc::clone(&finished);
        let reader = std::thread::spawn(move || read_bounded(&mut pipe, limit, &reader_finished));
        std::thread::sleep(Duration::from_millis(120));
        let _ = child.kill();
        let _ = child.wait();
        finished.store(true, Ordering::Release);
        let output = reader.join().unwrap();
        (output, started.elapsed())
    }

    #[test]
    fn returns_after_reaping_even_when_a_descendant_holds_the_write_end() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file: PathBuf = directory.path().join("grandchild.pid");
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg("sleep 30 & echo $! > \"$REVIA_TEST_PID_FILE\"; printf held >&2; wait")
            .env("REVIA_TEST_PID_FILE", &pid_file);
        let finished = Arc::new(AtomicBool::new(false));
        let (output, elapsed) = capture(&mut command, 1024, finished);

        // 先清理本测试创建的后台子进程，再做断言，避免失败时遗留进程。
        if let Ok(text) = std::fs::read_to_string(&pid_file) {
            if let Ok(pid) = text.trim().parse::<u32>() {
                let _ = Command::new("kill").arg("-9").arg(pid.to_string()).status();
            }
        }
        assert!(elapsed < Duration::from_secs(3), "捕获耗时 {elapsed:?}");
        assert_eq!(output.bytes, b"held");
    }

    #[test]
    fn keeps_only_the_limit_but_drains_large_output() {
        let mut command = Command::new("sh");
        command.arg("-c").arg("i=0; while [ $i -lt 200 ]; do printf 0123456789 >&2; i=$((i+1)); done");
        let finished = Arc::new(AtomicBool::new(false));
        let (output, elapsed) = capture(&mut command, 128, finished);
        assert!(elapsed < Duration::from_secs(3), "捕获耗时 {elapsed:?}");
        assert_eq!(output.bytes.len(), 128);
        assert!(output.truncated);
    }

    #[test]
    fn normal_exit_keeps_all_output_within_the_limit() {
        let mut command = Command::new("sh");
        command.arg("-c").arg("printf 'profile failed' >&2");
        let (mut pipe, writer) = OutputPipe::new().unwrap();
        command.stdout(Stdio::null()).stderr(writer).stdin(Stdio::null());
        let mut child = command.spawn().unwrap();
        let finished = Arc::new(AtomicBool::new(false));
        let reader_finished = Arc::clone(&finished);
        let reader = std::thread::spawn(move || read_bounded(&mut pipe, 1024, &reader_finished));
        let _ = child.wait();
        finished.store(true, Ordering::Release);
        let output = reader.join().unwrap();
        assert_eq!(output.bytes, b"profile failed");
        assert!(!output.truncated);
    }

    #[test]
    fn write_side_is_closed_by_dropping_the_child() {
        // 确保上面的 capture 辅助函数没有遗留写端：子进程退出后读端应很快 EOF。
        let mut command = Command::new("sh");
        command.arg("-c").arg("exit 0");
        let (mut pipe, writer) = OutputPipe::new().unwrap();
        command.stdout(Stdio::null()).stderr(writer).stdin(Stdio::null());
        let mut child = command.spawn().unwrap();
        let finished = Arc::new(AtomicBool::new(false));
        let reader_finished = Arc::clone(&finished);
        let reader = std::thread::spawn(move || read_bounded(&mut pipe, 1024, &reader_finished));
        let _ = child.wait();
        finished.store(true, Ordering::Release);
        let output = reader.join().unwrap();
        assert!(output.bytes.is_empty());
    }
}
