//! Bounded reads let a reaped child release output even when descendants retain writers.

use std::io::{self, Read};
use std::process::Stdio;

#[cfg(unix)]
pub(crate) struct OutputPipe(std::os::unix::net::UnixStream);

#[cfg(windows)]
pub(crate) struct OutputPipe(std::io::PipeReader);

impl OutputPipe {
    pub(crate) fn new() -> io::Result<(Self, Stdio)> {
        #[cfg(unix)]
        {
            use std::os::fd::OwnedFd;
            use std::os::unix::net::UnixStream;
            let (reader, writer) = UnixStream::pair()?;
            reader.set_read_timeout(Some(std::time::Duration::from_millis(20)))?;
            let writer: OwnedFd = writer.into();
            Ok((Self(reader), Stdio::from(writer)))
        }
        #[cfg(windows)]
        {
            let (reader, writer) = std::io::pipe()?;
            Ok((Self(reader), Stdio::from(writer)))
        }
    }
}

impl Read for OutputPipe {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        #[cfg(unix)]
        { self.0.read(bytes) }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            #[link(name = "kernel32")]
            unsafe extern "system" {
                fn PeekNamedPipe(
                    handle: *mut std::ffi::c_void,
                    buffer: *mut std::ffi::c_void,
                    size: u32,
                    read: *mut u32,
                    available: *mut u32,
                    remaining: *mut u32,
                ) -> i32;
            }
            let mut available = 0;
            // This is the sole reader; a ready byte cannot be consumed by another thread.
            let ready = unsafe {
                PeekNamedPipe(self.0.as_raw_handle(), std::ptr::null_mut(), 0,
                    std::ptr::null_mut(), &mut available, std::ptr::null_mut())
            };
            if ready == 0 {
                let error = io::Error::last_os_error();
                return if error.raw_os_error() == Some(109) { Ok(0) } else { Err(error) };
            }
            if available == 0 {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            let length = bytes.len().min(available as usize);
            self.0.read(&mut bytes[..length])
        }
    }
}
