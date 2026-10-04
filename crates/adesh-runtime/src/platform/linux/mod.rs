//! Linux Platform Specifics.

#[allow(unused_imports)]
pub use super::unix::*;
use std::sync::atomic::{AtomicI32, Ordering};

/// Linux futex wait wrapper (SYS_futex).
pub fn linux_futex_wait(uaddr: &AtomicI32, val: i32) -> i32 {
    #[cfg(target_os = "linux")]
    unsafe {
        libc::syscall(
            libc::SYS_futex,
            uaddr.as_ptr(),
            libc::FUTEX_WAIT,
            val,
            std::ptr::null::<libc::timespec>(),
            std::ptr::null::<u32>(),
            0,
        ) as i32
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (uaddr, val);
        0
    }
}

/// Linux futex wake wrapper (SYS_futex).
pub fn linux_futex_wake(uaddr: &AtomicI32, count: i32) -> i32 {
    #[cfg(target_os = "linux")]
    unsafe {
        libc::syscall(
            libc::SYS_futex,
            uaddr.as_ptr(),
            libc::FUTEX_WAKE,
            count,
            std::ptr::null::<libc::timespec>(),
            std::ptr::null::<u32>(),
            0,
        ) as i32
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (uaddr, count);
        0
    }
}
