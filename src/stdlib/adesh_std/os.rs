//! OS Bindings
//!
//! Platform-specific functionality abstraction

/// Platform type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Linux,
    Windows,
    MacOS,
    Unix,
    Unknown,
}

impl Platform {
    /// Returns the current platform
    pub fn current() -> Platform {
        #[cfg(target_os = "linux")]
        return Platform::Linux;

        #[cfg(target_os = "windows")]
        return Platform::Windows;

        #[cfg(target_os = "macos")]
        return Platform::MacOS;

        #[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
        return Platform::Unix;

        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos", unix)))]
        return Platform::Unknown;
    }
}

/// Environment variable operations
pub mod env {
    /// Gets an environment variable
    pub fn var(key: &str) -> Option<String> {
        std::env::var(key).ok()
    }

    /// Sets an environment variable
    pub fn set_var(key: &str, value: &str) {
        unsafe {
            std::env::set_var(key, value);
        }
    }

    /// Removes an environment variable
    pub fn remove_var(key: &str) {
        unsafe {
            std::env::remove_var(key);
        }
    }
}
