//! `adesh-monitor-platform`
//!
//! Cross-platform hardware, process discovery, and resource metrics provider
//! with first-class native support for Windows, Linux, and macOS.

use adesh_monitor_protocol::ProcessMetrics;
use chrono::Utc;
use std::time::Instant;

pub mod hardware;

/// Platform capability and hardware report
#[derive(Debug, Clone)]
pub struct PlatformCapabilities {
    pub os_name: String,
    pub arch: String,
    pub logical_cores: usize,
    pub total_memory_bytes: u64,
    pub supports_process_cpu: bool,
    pub supports_process_memory: bool,
    pub supports_disk_io: bool,
    pub supports_accelerators: bool,
}

pub trait MetricsProvider: Send + Sync {
    /// Sample resource metrics for a target process ID
    fn sample_process(&mut self, pid: u32, elapsed_micros: u64) -> Option<ProcessMetrics>;
    /// Query platform hardware and telemetry capabilities
    fn query_capabilities(&self) -> PlatformCapabilities;
}

/// Factory to create the platform-appropriate metrics provider for any OS
pub fn create_platform_provider() -> Box<dyn MetricsProvider> {
    #[cfg(windows)]
    {
        Box::new(windows::WindowsMetricsProvider::new())
    }
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxMetricsProvider::new())
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacosMetricsProvider::new())
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        Box::new(fallback::FallbackMetricsProvider::new())
    }
}

// ============================================================================
// WINDOWS IMPLEMENTATION
// ============================================================================
#[cfg(windows)]
mod windows {
    use super::*;
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE};
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_INFORMATION,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
    };

    pub struct WindowsMetricsProvider {
        last_sample_instant: Option<Instant>,
        last_user_time: u64,
        last_sys_time: u64,
        peak_threads: std::collections::HashMap<u32, u32>,
    }

    impl WindowsMetricsProvider {
        pub fn new() -> Self {
            Self {
                last_sample_instant: None,
                last_user_time: 0,
                last_sys_time: 0,
                peak_threads: std::collections::HashMap::new(),
            }
        }

        fn filetime_to_u64(ft: FILETIME) -> u64 {
            ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64)
        }

        fn get_process_thread_count(pid: u32) -> u32 {
            use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
            use windows_sys::Win32::System::Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32, Process32First, Process32Next,
                TH32CS_SNAPPROCESS,
            };
            unsafe {
                let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
                if snapshot.is_null() || snapshot == INVALID_HANDLE_VALUE {
                    return 1;
                }
                let mut pe: PROCESSENTRY32 = std::mem::zeroed();
                pe.dwSize = std::mem::size_of::<PROCESSENTRY32>() as u32;
                let mut found_threads = 1;
                if Process32First(snapshot, &mut pe) != 0 {
                    loop {
                        if pe.th32ProcessID == pid {
                            found_threads = pe.cntThreads.max(1);
                            break;
                        }
                        if Process32Next(snapshot, &mut pe) == 0 {
                            break;
                        }
                    }
                }
                CloseHandle(snapshot);
                found_threads
            }
        }
    }

    impl MetricsProvider for WindowsMetricsProvider {
        fn sample_process(&mut self, pid: u32, elapsed_micros: u64) -> Option<ProcessMetrics> {
            let (handle, need_close) = if pid == std::process::id() {
                (unsafe { GetCurrentProcess() }, false)
            } else {
                let mut h =
                    unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid) };
                if h.is_null() {
                    h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
                }
                if h.is_null() {
                    return None;
                }
                (h, true)
            };

            let mut mem_counters: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
            mem_counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;

            let mem_ok = unsafe {
                GetProcessMemoryInfo(
                    handle,
                    &mut mem_counters,
                    std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
                )
            };

            let mut creation_time = FILETIME {
                dwLowDateTime: 0,
                dwHighDateTime: 0,
            };
            let mut exit_time = FILETIME {
                dwLowDateTime: 0,
                dwHighDateTime: 0,
            };
            let mut kernel_time = FILETIME {
                dwLowDateTime: 0,
                dwHighDateTime: 0,
            };
            let mut user_time = FILETIME {
                dwLowDateTime: 0,
                dwHighDateTime: 0,
            };

            let times_ok = unsafe {
                GetProcessTimes(
                    handle,
                    &mut creation_time,
                    &mut exit_time,
                    &mut kernel_time,
                    &mut user_time,
                )
            };

            let mut io_counters: windows_sys::Win32::System::Threading::IO_COUNTERS =
                unsafe { std::mem::zeroed() };
            let io_ok = unsafe {
                windows_sys::Win32::System::Threading::GetProcessIoCounters(
                    handle,
                    &mut io_counters,
                )
            };
            let disk_read_bytes = if io_ok != 0 {
                io_counters.ReadTransferCount
            } else {
                0
            };
            let disk_write_bytes = if io_ok != 0 {
                io_counters.WriteTransferCount
            } else {
                0
            };

            if need_close {
                unsafe { CloseHandle(handle) };
            }

            let raw_working_set = if mem_ok != 0 {
                mem_counters.WorkingSetSize as u64
            } else {
                0
            };
            let peak_working_set = if mem_ok != 0 {
                mem_counters.PeakWorkingSetSize as u64
            } else {
                0
            };
            let vms = if mem_ok != 0 {
                mem_counters.PagefileUsage as u64
            } else {
                0
            };
            let working_set = if raw_working_set == 0 && peak_working_set > 0 {
                peak_working_set
            } else {
                raw_working_set
            };

            let mut cpu_percent = 0.0f32;
            let current_user_raw = if times_ok != 0 {
                Self::filetime_to_u64(user_time)
            } else {
                0
            };
            let current_sys_raw = if times_ok != 0 {
                Self::filetime_to_u64(kernel_time)
            } else {
                0
            };

            // 100-ns intervals to milliseconds
            let user_ms = current_user_raw / 10_000;
            let sys_ms = current_sys_raw / 10_000;

            let now = Instant::now();
            if let Some(prev) = self.last_sample_instant {
                let dt = now.duration_since(prev).as_secs_f32();
                if dt > 0.001 {
                    let user_delta =
                        current_user_raw.saturating_sub(self.last_user_time) as f32 / 10_000_000.0;
                    let sys_delta =
                        current_sys_raw.saturating_sub(self.last_sys_time) as f32 / 10_000_000.0;
                    cpu_percent = ((user_delta + sys_delta) / dt) * 100.0;
                }
            } else if elapsed_micros > 0 {
                let total_cpu_secs = (current_user_raw + current_sys_raw) as f32 / 10_000_000.0;
                let elapsed_secs = elapsed_micros as f32 / 1_000_000.0;
                if elapsed_secs > 0.0001 {
                    cpu_percent = (total_cpu_secs / elapsed_secs) * 100.0;
                }
            }
            self.last_sample_instant = Some(now);
            self.last_user_time = current_user_raw;
            self.last_sys_time = current_sys_raw;

            let thread_count = Self::get_process_thread_count(pid);
            let peak_thread_count = {
                let entry = self.peak_threads.entry(pid).or_insert(thread_count);
                *entry = (*entry).max(thread_count);
                *entry
            };
            let logical_cores = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1);
            let cores_used = (cpu_percent / 100.0).max(0.1).min(logical_cores as f32);

            // Allocator breakdown metrics are not inferred from OS RSS heuristics.
            // 0 indicates uninstrumented/unavailable at the OS level.
            let static_bytes = 0u64;
            let heap_bytes = 0u64;
            let dynamic_bytes = 0u64;
            let reused_bytes = 0u64;

            Some(ProcessMetrics {
                cpu_percent,
                memory_rss_bytes: working_set,
                memory_peak_rss_bytes: peak_working_set.max(working_set),
                memory_vms_bytes: vms,
                thread_count,
                peak_thread_count,
                disk_read_bytes,
                disk_write_bytes,
                cpu_user_ms: user_ms,
                cpu_sys_ms: sys_ms,
                timestamp_utc: Utc::now(),
                elapsed_micros,
                memory_static_bytes: static_bytes,
                memory_heap_bytes: heap_bytes,
                memory_dynamic_bytes: dynamic_bytes,
                memory_reused_bytes: reused_bytes,
                cores_used,
            })
        }

        fn query_capabilities(&self) -> PlatformCapabilities {
            PlatformCapabilities {
                os_name: "Windows".to_string(),
                arch: std::env::consts::ARCH.to_string(),
                logical_cores: std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(1),
                total_memory_bytes: crate::hardware::system_memory().0,
                supports_process_cpu: true,
                supports_process_memory: true,
                supports_disk_io: true,
                supports_accelerators: false,
            }
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::fs;

    pub struct LinuxMetricsProvider {
        last_sample_instant: Option<Instant>,
        last_total_ticks: u64,
        clock_ticks_per_sec: u64,
        page_size_bytes: u64,
        peak_threads: std::collections::HashMap<u32, u32>,
    }

    impl LinuxMetricsProvider {
        pub fn new() -> Self {
            let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
            let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
            Self {
                last_sample_instant: None,
                last_total_ticks: 0,
                clock_ticks_per_sec: if ticks > 0 { ticks as u64 } else { 100 },
                page_size_bytes: if page_size > 0 {
                    page_size as u64
                } else {
                    4096
                },
                peak_threads: std::collections::HashMap::new(),
            }
        }
    }

    impl MetricsProvider for LinuxMetricsProvider {
        fn sample_process(&mut self, pid: u32, elapsed_micros: u64) -> Option<ProcessMetrics> {
            let stat_path = format!("/proc/{}/stat", pid);
            let stat_content = fs::read_to_string(&stat_path).ok()?;

            // Format: pid (comm) state ppid pgrp session tty_nr tpgid flags minflt cminflt majflt cmajflt utime stime ...
            let after_comm = stat_content.rfind(')')?;
            let rest = &stat_content[after_comm + 2..];
            let fields: Vec<&str> = rest.split_whitespace().collect();
            if fields.len() < 22 {
                return None;
            }

            let utime: u64 = fields.get(11)?.parse().ok()?;
            let stime: u64 = fields.get(12)?.parse().ok()?;
            let num_threads: u32 = fields.get(17)?.parse().ok().unwrap_or(1);
            let vsize_bytes: u64 = fields.get(20)?.parse().ok().unwrap_or(0);
            let rss_pages: u64 = fields.get(21)?.parse().ok().unwrap_or(0);
            let rss_bytes = rss_pages * self.page_size_bytes;

            let total_ticks = utime + stime;
            let user_ms = (utime * 1000) / self.clock_ticks_per_sec;
            let sys_ms = (stime * 1000) / self.clock_ticks_per_sec;

            let mut cpu_percent = 0.0f32;
            let now = Instant::now();
            if let Some(prev) = self.last_sample_instant {
                let dt = now.duration_since(prev).as_secs_f32();
                if dt > 0.001 {
                    let delta_ticks = total_ticks.saturating_sub(self.last_total_ticks) as f32;
                    let seconds_consumed = delta_ticks / (self.clock_ticks_per_sec as f32);
                    cpu_percent = (seconds_consumed / dt) * 100.0;
                }
            }
            self.last_sample_instant = Some(now);
            self.last_total_ticks = total_ticks;

            // Query genuine disk I/O from /proc/[pid]/io if available
            let (disk_read_bytes, disk_write_bytes) =
                if let Ok(io_str) = fs::read_to_string(format!("/proc/{}/io", pid)) {
                    let mut r = 0u64;
                    let mut w = 0u64;
                    for line in io_str.lines() {
                        if let Some(val) = line.strip_prefix("read_bytes:") {
                            r = val.trim().parse().unwrap_or(0);
                        } else if let Some(val) = line.strip_prefix("write_bytes:") {
                            w = val.trim().parse().unwrap_or(0);
                        }
                    }
                    (r, w)
                } else {
                    (0, 0)
                };

            // Query genuine peak RSS (VmHWM) from /proc/[pid]/status if available
            let peak_rss_bytes =
                if let Ok(status_str) = fs::read_to_string(format!("/proc/{}/status", pid)) {
                    let mut peak = rss_bytes;
                    for line in status_str.lines() {
                        if let Some(val) = line.strip_prefix("VmHWM:") {
                            let kb: u64 = val
                                .trim()
                                .trim_end_matches("kB")
                                .trim()
                                .parse()
                                .unwrap_or(0);
                            if kb > 0 {
                                peak = kb * 1024;
                            }
                            break;
                        }
                    }
                    peak.max(rss_bytes)
                } else {
                    rss_bytes
                };

            let peak_thread_count = {
                let entry = self.peak_threads.entry(pid).or_insert(num_threads);
                *entry = (*entry).max(num_threads);
                *entry
            };

            // Allocator breakdown metrics are not inferred from OS RSS heuristics.
            // 0 indicates uninstrumented/unavailable at the OS level.
            let static_bytes = 0u64;
            let heap_bytes = 0u64;
            let dynamic_bytes = 0u64;
            let reused_bytes = 0u64;
            let cores_used = (cpu_percent / 100.0).max(0.1);

            Some(ProcessMetrics {
                cpu_percent,
                memory_rss_bytes: rss_bytes,
                memory_peak_rss_bytes: peak_rss_bytes,
                memory_vms_bytes: vsize_bytes,
                thread_count: num_threads,
                peak_thread_count,
                disk_read_bytes,
                disk_write_bytes,
                cpu_user_ms: user_ms,
                cpu_sys_ms: sys_ms,
                timestamp_utc: Utc::now(),
                elapsed_micros,
                memory_static_bytes: static_bytes,
                memory_heap_bytes: heap_bytes,
                memory_dynamic_bytes: dynamic_bytes,
                memory_reused_bytes: reused_bytes,
                cores_used,
            })
        }

        fn query_capabilities(&self) -> PlatformCapabilities {
            PlatformCapabilities {
                os_name: "Linux".to_string(),
                arch: std::env::consts::ARCH.to_string(),
                logical_cores: std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(1),
                total_memory_bytes: crate::hardware::system_memory().0,
                supports_process_cpu: true,
                supports_process_memory: true,
                supports_disk_io: true,
                supports_accelerators: false,
            }
        }
    }
}

// ============================================================================
// macOS IMPLEMENTATION (mach / proc_pidinfo)
// ============================================================================
#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::process::Command;

    pub struct MacosMetricsProvider {
        #[allow(dead_code)]
        last_sample_instant: Option<Instant>,
        #[allow(dead_code)]
        last_total_time_ns: u64,
        peak_threads: std::collections::HashMap<u32, u32>,
    }

    impl MacosMetricsProvider {
        pub fn new() -> Self {
            Self {
                last_sample_instant: None,
                last_total_time_ns: 0,
                peak_threads: std::collections::HashMap::new(),
            }
        }
    }

    impl MetricsProvider for MacosMetricsProvider {
        fn sample_process(&mut self, pid: u32, elapsed_micros: u64) -> Option<ProcessMetrics> {
            let output = Command::new("ps")
                .args(["-o", "rss=,vsz=,thcount=,%cpu=", "-p", &pid.to_string()])
                .output()
                .ok()?;
            if !output.status.success() {
                return None;
            }
            let values = String::from_utf8_lossy(&output.stdout);
            let fields = values.split_whitespace().collect::<Vec<_>>();
            if fields.len() < 4 {
                return None;
            }
            let rss_bytes = fields[0].parse::<u64>().ok()?.saturating_mul(1024);
            let vms_bytes = fields[1].parse::<u64>().ok()?.saturating_mul(1024);
            let thread_count = fields[2].parse::<u32>().ok()?;
            let cpu_percent = fields[3].parse::<f32>().ok().unwrap_or(0.0);

            let peak_thread_count = {
                let entry = self.peak_threads.entry(pid).or_insert(thread_count);
                *entry = (*entry).max(thread_count);
                *entry
            };

            // Allocator breakdown metrics are not inferred from OS RSS heuristics.
            // 0 indicates uninstrumented/unavailable at the OS level.
            let static_bytes = 0u64;
            let heap_bytes = 0u64;
            let dynamic_bytes = 0u64;
            let reused_bytes = 0u64;
            let cores_used = (cpu_percent / 100.0).max(0.1);

            Some(ProcessMetrics {
                cpu_percent,
                memory_rss_bytes: rss_bytes,
                memory_peak_rss_bytes: rss_bytes,
                memory_vms_bytes: vms_bytes,
                thread_count,
                peak_thread_count,
                disk_read_bytes: 0,
                disk_write_bytes: 0,
                cpu_user_ms: 0,
                cpu_sys_ms: 0,
                timestamp_utc: Utc::now(),
                elapsed_micros,
                memory_static_bytes: static_bytes,
                memory_heap_bytes: heap_bytes,
                memory_dynamic_bytes: dynamic_bytes,
                memory_reused_bytes: reused_bytes,
                cores_used,
            })
        }

        fn query_capabilities(&self) -> PlatformCapabilities {
            PlatformCapabilities {
                os_name: "macOS".to_string(),
                arch: std::env::consts::ARCH.to_string(),
                logical_cores: std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(1),
                total_memory_bytes: crate::hardware::system_memory().0,
                supports_process_cpu: true,
                supports_process_memory: true,
                supports_disk_io: false,
                supports_accelerators: false,
            }
        }
    }
}

// ============================================================================
// FALLBACK IMPLEMENTATION (FreeBSD, OpenBSD, illumos, etc.)
// ============================================================================
#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod fallback {
    use super::*;

    pub struct FallbackMetricsProvider;

    impl FallbackMetricsProvider {
        pub fn new() -> Self {
            Self
        }
    }

    impl MetricsProvider for FallbackMetricsProvider {
        fn sample_process(&mut self, _pid: u32, elapsed_micros: u64) -> Option<ProcessMetrics> {
            Some(ProcessMetrics {
                cpu_percent: 0.0,
                memory_rss_bytes: 0,
                memory_peak_rss_bytes: 0,
                memory_vms_bytes: 0,
                thread_count: 1,
                peak_thread_count: 1,
                disk_read_bytes: 0,
                disk_write_bytes: 0,
                cpu_user_ms: 0,
                cpu_sys_ms: 0,
                timestamp_utc: Utc::now(),
                elapsed_micros,
                memory_static_bytes: 0,
                memory_heap_bytes: 0,
                memory_dynamic_bytes: 0,
                memory_reused_bytes: 0,
                cores_used: 1.0,
            })
        }

        fn query_capabilities(&self) -> PlatformCapabilities {
            PlatformCapabilities {
                os_name: std::env::consts::OS.to_string(),
                arch: std::env::consts::ARCH.to_string(),
                logical_cores: std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(1),
                total_memory_bytes: crate::hardware::system_memory().0,
                supports_process_cpu: false,
                supports_process_memory: false,
                supports_disk_io: false,
                supports_accelerators: false,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_provider_creation_and_sampling() {
        let mut provider = create_platform_provider();
        let current_pid = std::process::id();
        let sample = provider.sample_process(current_pid, 500);
        assert!(sample.is_some());
        let sample = sample.unwrap();
        assert!(sample.memory_peak_rss_bytes >= sample.memory_rss_bytes);
        assert!(sample.peak_thread_count >= sample.thread_count);
        // Ensure no fabricated percentage heuristics were generated
        assert_eq!(sample.memory_static_bytes, 0);
        assert_eq!(sample.memory_heap_bytes, 0);
        assert_eq!(sample.memory_dynamic_bytes, 0);
        assert_eq!(sample.memory_reused_bytes, 0);

        let caps = provider.query_capabilities();
        assert!(caps.logical_cores >= 1);
        #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
        assert!(
            caps.total_memory_bytes > 0,
            "total_memory_bytes must be queried from OS"
        );
        #[cfg(any(windows, target_os = "linux"))]
        assert!(
            caps.supports_disk_io,
            "disk I/O must be supported on Windows and Linux"
        );
    }

    #[test]
    fn test_peak_thread_count_tracking() {
        let mut provider = create_platform_provider();
        let current_pid = std::process::id();
        let s1 = provider.sample_process(current_pid, 100).unwrap();
        let s2 = provider.sample_process(current_pid, 200).unwrap();
        assert!(s2.peak_thread_count >= s1.peak_thread_count);
    }
}
