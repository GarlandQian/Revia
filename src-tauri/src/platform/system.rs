//! 集中平台系统采样与机器配置（M9，Q55、Q210、Q217、S3、T1）。
//!
//! 周期采样不启动任何子进程：macOS 用 libproc 系统调用，Windows 用 Win32 API。
//! 一次性电脑配置仅在导出诊断信息时按需调用；Windows 下启动的子进程统一经过 hide_console。

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ProcessCpuMemory {
    pub pid: u32,
    pub cpu_seconds: f64,
    pub rss_bytes: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SystemSampleSnapshot {
    pub main: ProcessCpuMemory,
    pub children: Vec<ProcessCpuMemory>,
}

/// 采样当前进程树（主进程与全部子进程）的累计 CPU 秒数和内存，不启动任何子进程。
pub fn sample_process_tree() -> Option<SystemSampleSnapshot> {
    #[cfg(target_os = "macos")]
    {
        macos::sample()
    }
    #[cfg(windows)]
    {
        windows::sample()
    }
    #[cfg(all(not(target_os = "macos"), not(windows)))]
    {
        None
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;

    #[repr(C)]
    #[derive(Default)]
    struct ProcTaskInfo {
        pub pti_virtual_size: u64,
        pub pti_resident_size: u64,
        pub pti_total_user: u64,
        pub pti_total_system: u64,
        pub pti_threads_user: u64,
        pub pti_threads_system: u64,
        pub pti_policy: i32,
        pub pti_faults: i32,
        pub pti_pageins: i32,
        pub pti_cow_faults: i32,
        pub pti_messages_sent: i32,
        pub pti_messages_received: i32,
        pub pti_syscalls_mach: i32,
        pub pti_syscalls_unix: i32,
        pub pti_csw: i32,
        pub pti_threadnum: i32,
        pub pti_numrunning: i32,
        pub pti_priority: i32,
    }

    const PROC_PIDTASKINFO: i32 = 4;

    unsafe extern "C" {
        fn proc_pidinfo(
            pid: i32,
            flavor: i32,
            arg: u64,
            buffer: *mut std::ffi::c_void,
            buffersize: i32,
        ) -> i32;
        fn proc_listchildpids(
            ppid: i32,
            buffer: *mut std::ffi::c_void,
            buffersize: i32,
        ) -> i32;
        fn mach_timebase_info(info: *mut MachTimebaseInfo) -> i32;
    }

    #[repr(C)]
    #[derive(Default)]
    struct MachTimebaseInfo {
        numer: u32,
        denom: u32,
    }

    /// `pti_total_*` 的单位是 mach 绝对时间（Apple Silicon 上不是纳秒），按时基换算成纳秒。
    fn ticks_to_nanos(ticks: u64) -> f64 {
        static TIMEBASE: std::sync::OnceLock<(u32, u32)> = std::sync::OnceLock::new();
        let (numer, denom) = *TIMEBASE.get_or_init(|| {
            let mut info = MachTimebaseInfo::default();
            let ok = unsafe { mach_timebase_info(&mut info) } == 0;
            if ok && info.numer > 0 && info.denom > 0 {
                (info.numer, info.denom)
            } else {
                (1, 1)
            }
        });
        ticks as f64 * f64::from(numer) / f64::from(denom)
    }

    fn get_task_info(pid: u32) -> Option<ProcessCpuMemory> {
        let mut info = ProcTaskInfo::default();
        let size = std::mem::size_of::<ProcTaskInfo>() as i32;
        let ret = unsafe {
            proc_pidinfo(
                pid as i32,
                PROC_PIDTASKINFO,
                0,
                &mut info as *mut _ as *mut std::ffi::c_void,
                size,
            )
        };
        if ret == size {
            let ticks = info.pti_total_user.saturating_add(info.pti_total_system);
            let cpu_seconds = ticks_to_nanos(ticks) / 1_000_000_000.0;
            Some(ProcessCpuMemory {
                pid,
                cpu_seconds,
                rss_bytes: info.pti_resident_size,
            })
        } else {
            None
        }
    }

    fn list_children(ppid: u32) -> Vec<u32> {
        let mut pids = vec![0i32; 128];
        // 返回值是写入的进程个数（不是字节数）。
        let count = unsafe {
            proc_listchildpids(
                ppid as i32,
                pids.as_mut_ptr() as *mut std::ffi::c_void,
                (pids.len() * std::mem::size_of::<i32>()) as i32,
            )
        };
        if count <= 0 {
            return Vec::new();
        }
        pids.truncate((count as usize).min(pids.len()));
        pids.into_iter().filter(|&p| p > 0).map(|p| p as u32).collect()
    }

    pub fn sample() -> Option<SystemSampleSnapshot> {
        let root = std::process::id();
        let main = get_task_info(root)?;

        let mut all_children = Vec::new();
        let mut queue = list_children(root);
        while let Some(child_pid) = queue.pop() {
            if !all_children.contains(&child_pid) {
                all_children.push(child_pid);
                let grand = list_children(child_pid);
                queue.extend(grand);
            }
        }

        let mut children = Vec::new();
        for cpid in all_children {
            if let Some(cinfo) = get_task_info(cpid) {
                children.push(cinfo);
            }
        }

        Some(SystemSampleSnapshot { main, children })
    }
}

#[cfg(windows)]
mod windows {
    use super::*;

    #[repr(C)]
    #[derive(Copy, Clone)]
    struct ProcessEntry32W {
        dw_size: u32,
        cnt_usage: u32,
        th32_process_id: u32,
        th32_default_heap_id: usize,
        th32_module_id: u32,
        cnt_threads: u32,
        th32_parent_process_id: u32,
        pc_pri_class_base: i32,
        dw_flags: u32,
        sz_exe_file: [u16; 260],
    }

    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    #[repr(C)]
    struct FileTime {
        dw_low_date_time: u32,
        dw_high_date_time: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateToolhelp32Snapshot(dw_flags: u32, th32_process_id: u32) -> *mut std::ffi::c_void;
        fn Process32FirstW(h_snapshot: *mut std::ffi::c_void, lppe: *mut ProcessEntry32W) -> i32;
        fn Process32NextW(h_snapshot: *mut std::ffi::c_void, lppe: *mut ProcessEntry32W) -> i32;
        fn CloseHandle(h_object: *mut std::ffi::c_void) -> i32;
        fn OpenProcess(dw_desired_access: u32, b_inherit_handle: i32, dw_process_id: u32) -> *mut std::ffi::c_void;
        fn GetProcessTimes(
            h_process: *mut std::ffi::c_void,
            lp_creation_time: *mut FileTime,
            lp_exit_time: *mut FileTime,
            lp_kernel_time: *mut FileTime,
            lp_user_time: *mut FileTime,
        ) -> i32;
    }

    #[link(name = "psapi")]
    unsafe extern "system" {
        fn GetProcessMemoryInfo(
            h_process: *mut std::ffi::c_void,
            ppsmem_counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }

    const TH32CS_SNAPPROCESS: u32 = 0x00000002;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

    fn file_time_to_seconds(ft: &FileTime) -> f64 {
        let intervals = ((ft.dw_high_date_time as u64) << 32) | (ft.dw_low_date_time as u64);
        intervals as f64 / 10_000_000.0
    }

    fn get_process_info(pid: u32) -> Option<ProcessCpuMemory> {
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return None;
            }
            let mut creation = FileTime { dw_low_date_time: 0, dw_high_date_time: 0 };
            let mut exit = FileTime { dw_low_date_time: 0, dw_high_date_time: 0 };
            let mut kernel = FileTime { dw_low_date_time: 0, dw_high_date_time: 0 };
            let mut user = FileTime { dw_low_date_time: 0, dw_high_date_time: 0 };
            let time_ok = GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user);

            let mut mem = ProcessMemoryCounters {
                cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
                page_fault_count: 0,
                peak_working_set_size: 0,
                working_set_size: 0,
                quota_peak_paged_pool_usage: 0,
                quota_paged_pool_usage: 0,
                quota_peak_non_paged_pool_usage: 0,
                quota_non_paged_pool_usage: 0,
                pagefile_usage: 0,
                peak_pagefile_usage: 0,
            };
            let mem_ok = GetProcessMemoryInfo(handle, &mut mem, mem.cb);
            CloseHandle(handle);

            if time_ok != 0 && mem_ok != 0 {
                let cpu_seconds = file_time_to_seconds(&kernel) + file_time_to_seconds(&user);
                Some(ProcessCpuMemory {
                    pid,
                    cpu_seconds,
                    rss_bytes: mem.working_set_size as u64,
                })
            } else {
                None
            }
        }
    }

    pub fn sample() -> Option<SystemSampleSnapshot> {
        let root = std::process::id();
        let main = get_process_info(root)?;

        let mut pairs = Vec::new();
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            // 失败时返回 INVALID_HANDLE_VALUE（-1），不是空指针。
            if snapshot.is_null() || snapshot as isize == -1 {
                return Some(SystemSampleSnapshot { main, children: Vec::new() });
            }
            let mut entry = ProcessEntry32W {
                dw_size: std::mem::size_of::<ProcessEntry32W>() as u32,
                cnt_usage: 0,
                th32_process_id: 0,
                th32_default_heap_id: 0,
                th32_module_id: 0,
                cnt_threads: 0,
                th32_parent_process_id: 0,
                pc_pri_class_base: 0,
                dw_flags: 0,
                sz_exe_file: [0; 260],
            };
            if Process32FirstW(snapshot, &mut entry) != 0 {
                loop {
                    pairs.push((entry.th32_process_id, entry.th32_parent_process_id));
                    if Process32NextW(snapshot, &mut entry) == 0 {
                        break;
                    }
                }
            }
            CloseHandle(snapshot);
        }

        let mut children_pids = Vec::new();
        let mut queue = vec![root];
        while let Some(parent) = queue.pop() {
            for &(pid, ppid) in &pairs {
                if ppid == parent && pid != root && !children_pids.contains(&pid) {
                    children_pids.push(pid);
                    queue.push(pid);
                }
            }
        }

        let mut children = Vec::new();
        for cpid in children_pids {
            if let Some(cinfo) = get_process_info(cpid) {
                children.push(cinfo);
            }
        }

        Some(SystemSampleSnapshot { main, children })
    }
}

// ---------------- 机器配置（导出时一次性调用，S10、T1） ----------------

pub fn platform_cpu_model() -> Option<String> {
    #[cfg(windows)]
    {
        std::env::var("PROCESSOR_IDENTIFIER").ok()
    }
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (!text.is_empty()).then_some(text)
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        None
    }
}

pub fn platform_os_version() -> Option<String> {
    #[cfg(windows)]
    {
        let version = windows_version::OsVersion::current();
        Some(format!("{}.{}.{}", version.major, version.minor, version.build))
    }
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (!text.is_empty()).then_some(text)
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        None
    }
}

pub fn platform_gpu() -> Option<String> {
    #[cfg(windows)]
    {
        let mut command = std::process::Command::new("powershell");
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(Get-CimInstance Win32_VideoController | Select-Object -First 1 -ExpandProperty Name)",
        ]);
        crate::platform::hide_console(&mut command);
        let output = command.output().ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (!text.is_empty()).then_some(text)
    }
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("system_profiler")
            .args(["SPDisplaysDataType", "-json"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
        let name = value
            .get("SPDisplaysDataType")?
            .as_array()?
            .first()?
            .get("sppci_model")?
            .as_str()?
            .to_string();
        (!name.is_empty()).then_some(name)
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        None
    }
}

/// 读取 WebView2 运行时版本（S10：使用正确的官方 Clients GUID 并同时查询 HKLM 和 HKCU）。
pub fn platform_webview_version() -> Option<String> {
    #[cfg(windows)]
    {
        // 微软官方指定的 WebView2 Evergreen 运行时 GUID
        const WEBVIEW2_GUID: &str = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
        let paths = [
            format!(r"HKLM\SOFTWARE\Microsoft\EdgeUpdate\Clients\{WEBVIEW2_GUID}"),
            format!(r"HKCU\SOFTWARE\Microsoft\EdgeUpdate\Clients\{WEBVIEW2_GUID}"),
            format!(r"HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{WEBVIEW2_GUID}"),
            format!(r"HKCU\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{WEBVIEW2_GUID}"),
        ];
        for reg_path in paths {
            let mut command = std::process::Command::new("reg");
            command.args(["query", &reg_path, "/v", "pv"]);
            crate::platform::hide_console(&mut command);
            if let Ok(output) = command.output() {
                if output.status.success() {
                    let text = String::from_utf8_lossy(&output.stdout);
                    if let Some(part) = text.split_whitespace().find(|part| part.split('.').count() >= 3) {
                        return Some(part.to_string());
                    }
                }
            }
        }
        None
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(all(test, target_os = "macos"))]
mod macos_tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// N1：CPU 秒数要按 mach 时基换算，忙等 300ms 至少要计入 150ms。
    #[test]
    fn cpu_seconds_track_real_cpu_time() {
        let before = sample_process_tree().expect("sample").main.cpu_seconds;
        let started = Instant::now();
        let mut counter = 0u64;
        while started.elapsed() < Duration::from_millis(300) {
            counter = std::hint::black_box(counter.wrapping_add(1));
        }
        let after = sample_process_tree().expect("sample").main.cpu_seconds;
        assert!(after - before >= 0.15, "cpu delta {} too small", after - before);
    }

    /// N2：`proc_listchildpids` 返回个数；只有 1 个子进程时也要列出来。
    #[test]
    fn single_child_process_is_listed() {
        let mut child = std::process::Command::new("/bin/sleep").arg("5").spawn().expect("spawn sleep");
        let pid = child.id();
        let found = sample_process_tree()
            .expect("sample")
            .children
            .iter()
            .any(|process| process.pid == pid);
        let _ = child.kill();
        let _ = child.wait();
        assert!(found, "child {pid} missing from process tree");
    }
}
