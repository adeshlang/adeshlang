//! Platform backend for threading.
//!
//! Portable API lives in `runtime::thread`. OS-specific operations stay here.
//!
//! Native threads: Linux, Windows, macOS.
//! WASM: APIs that need OS threads fail with a clear error (no fake concurrency).

pub fn logical_cpu_count() -> usize {
    #[cfg(target_arch = "wasm32")]
    {
        1
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        num_cpus::get().max(1)
    }
}

pub fn threading_supported() -> bool {
    !cfg!(target_arch = "wasm32")
}

pub fn unsupported_msg(feature: &str) -> String {
    format!(
        "thread.{} is not available on this platform ({}-{})",
        feature,
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

/// Best-effort OS thread name for debuggers/profilers.
/// Rust `thread::Builder::name` is always set in addition to this.
pub fn set_os_thread_name(name: &str) {
    let truncated: String = name.chars().take(15).collect();
    #[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
    {
        if let Ok(c) = std::ffi::CString::new(truncated.clone()) {
            unsafe {
                libc::pthread_setname_np(libc::pthread_self(), c.as_ptr());
            }
        }
    }
    #[cfg(all(target_os = "macos", not(target_arch = "wasm32")))]
    {
        if let Ok(c) = std::ffi::CString::new(truncated.clone()) {
            unsafe {
                libc::pthread_setname_np(c.as_ptr());
            }
        }
    }
    let _ = truncated;
}

/// Optional CPU affinity. Portable programs must not require this.
pub fn set_current_affinity(cpu_mask: u64) -> Result<(), String> {
    if cpu_mask == 0 {
        return Err("affinity mask must be non-zero".into());
    }
    #[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
    {
        unsafe {
            let mut set = std::mem::zeroed::<libc::cpu_set_t>();
            for cpu in 0..64u64 {
                if (cpu_mask & (1u64 << cpu)) != 0 {
                    libc::CPU_SET(cpu as usize, &mut set);
                }
            }
            let rc = libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set);
            if rc != 0 {
                return Err(format!(
                    "set_affinity failed: {}",
                    std::io::Error::last_os_error()
                ));
            }
        }
        return Ok(());
    }
    #[cfg(windows)]
    {
        return windows_set_affinity(cpu_mask);
    }
    #[cfg(not(any(windows, all(target_os = "linux", not(target_arch = "wasm32")))))]
    {
        let _ = cpu_mask;
        Err(unsupported_msg("set_affinity"))
    }
}

#[cfg(windows)]
fn windows_set_affinity(cpu_mask: u64) -> Result<(), String> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> isize;
        fn SetThreadAffinityMask(thread: isize, mask: usize) -> usize;
    }
    unsafe {
        let prev = SetThreadAffinityMask(GetCurrentThread(), cpu_mask as usize);
        if prev == 0 {
            return Err(format!(
                "set_affinity failed: {}",
                std::io::Error::last_os_error()
            ));
        }
    }
    Ok(())
}

pub fn get_current_affinity() -> Result<u64, String> {
    #[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
    {
        unsafe {
            let mut set = std::mem::zeroed::<libc::cpu_set_t>();
            let rc = libc::sched_getaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &mut set);
            if rc != 0 {
                return Err(format!(
                    "get_affinity failed: {}",
                    std::io::Error::last_os_error()
                ));
            }
            let mut mask = 0u64;
            for cpu in 0..64usize {
                if libc::CPU_ISSET(cpu, &set) {
                    mask |= 1u64 << cpu;
                }
            }
            return Ok(mask);
        }
    }
    #[cfg(not(all(target_os = "linux", not(target_arch = "wasm32"))))]
    {
        Err(unsupported_msg("get_affinity"))
    }
}

/// Nice-value style priority on Unix. Not faked on Windows.
pub fn set_current_priority(priority: i32) -> Result<(), String> {
    #[cfg(unix)]
    {
        unsafe {
            let rc = libc::setpriority(libc::PRIO_PROCESS, 0, priority);
            if rc != 0 {
                return Err(format!(
                    "set_priority failed: {}",
                    std::io::Error::last_os_error()
                ));
            }
        }
        return Ok(());
    }
    #[cfg(not(unix))]
    {
        let _ = priority;
        Err(unsupported_msg("set_priority"))
    }
}

pub fn get_current_priority() -> Result<i32, String> {
    #[cfg(unix)]
    {
        unsafe {
            let p = libc::getpriority(libc::PRIO_PROCESS, 0);
            return Ok(p);
        }
    }
    #[cfg(not(unix))]
    {
        Err(unsupported_msg("get_priority"))
    }
}
