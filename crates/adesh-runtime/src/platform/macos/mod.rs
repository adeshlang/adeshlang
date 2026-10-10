//! macOS Platform Specifics.

pub use super::unix::*;

/// macOS Mach semaphore / synchronization primitives.
#[cfg(target_os = "macos")]
pub fn macos_mach_timebase_info() -> (u32, u32) {
    #[repr(C)]
    struct mach_timebase_info_data_t {
        numer: u32,
        denom: u32,
    }
    unsafe extern "C" {
        fn mach_timebase_info(info: *mut mach_timebase_info_data_t) -> i32;
    }
    let mut info = mach_timebase_info_data_t { numer: 0, denom: 0 };
    unsafe {
        mach_timebase_info(&mut info);
    }
    (info.numer, info.denom)
}

#[cfg(not(target_os = "macos"))]
pub fn macos_mach_timebase_info() -> (u32, u32) {
    (1, 1)
}
