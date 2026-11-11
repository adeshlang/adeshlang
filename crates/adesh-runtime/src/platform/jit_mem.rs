//! Secure In-Memory Executable Page Allocator & Lifetime Manager.
//!
//! Provides strict W^X (Write XOR Execute) memory pages for native JIT execution.
//! Pages are initially allocated as Read-Write (`PAGE_READWRITE` / `PROT_READ | PROT_WRITE`)
//! so machine code can be written and relocations patched.
//! Once code generation is complete, the pages are transitioned to Read-Execute
//! (`PAGE_EXECUTE_READ` / `PROT_READ | PROT_EXEC`) and the CPU instruction cache is synchronized.

use std::ptr::null_mut;

#[derive(Debug)]
pub enum JitMemoryError {
    AllocationFailed,
    ProtectionChangeFailed,
    ZeroSize,
}

impl std::fmt::Display for JitMemoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AllocationFailed => write!(f, "Failed to allocate virtual memory for JIT code"),
            Self::ProtectionChangeFailed => {
                write!(f, "Failed to transition JIT memory protection to RX (W^X violation)")
            }
            Self::ZeroSize => write!(f, "Cannot allocate 0 bytes of JIT memory"),
        }
    }
}

impl std::error::Error for JitMemoryError {}

/// An RAII-managed block of executable memory with strict W^X enforcement.
pub struct JitMemoryBlock {
    ptr: *mut u8,
    size: usize,
    is_executable: bool,
}

// Send and Sync safety: JIT memory blocks are plain raw memory buffers.
unsafe impl Send for JitMemoryBlock {}
unsafe impl Sync for JitMemoryBlock {}

impl JitMemoryBlock {
    /// Return the system page size (typically 4096 bytes).
    pub fn page_size() -> usize {
        #[cfg(windows)]
        {
            4096
        }
        #[cfg(unix)]
        unsafe {
            libc::sysconf(libc::_SC_PAGESIZE) as usize
        }
        #[cfg(not(any(windows, unix)))]
        {
            4096
        }
    }

    /// Allocate a writable memory block large enough to hold `size` bytes.
    /// Memory is mapped with Read-Write permissions.
    pub fn allocate_writable(size: usize) -> Result<Self, JitMemoryError> {
        if size == 0 {
            return Err(JitMemoryError::ZeroSize);
        }

        let page_size = Self::page_size();
        let rounded_size = (size + page_size - 1) & !(page_size - 1);

        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::System::Memory::{
                MEM_COMMIT, MEM_RESERVE, PAGE_READWRITE, VirtualAlloc,
            };
            let ptr = VirtualAlloc(
                null_mut(),
                rounded_size,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_READWRITE,
            );
            if ptr.is_null() {
                return Err(JitMemoryError::AllocationFailed);
            }
            Ok(Self {
                ptr: ptr as *mut u8,
                size: rounded_size,
                is_executable: false,
            })
        }

        #[cfg(unix)]
        unsafe {
            let ptr = libc::mmap(
                null_mut(),
                rounded_size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            );
            if ptr == libc::MAP_FAILED {
                return Err(JitMemoryError::AllocationFailed);
            }
            Ok(Self {
                ptr: ptr as *mut u8,
                size: rounded_size,
                is_executable: false,
            })
        }

        #[cfg(not(any(windows, unix)))]
        {
            Err(JitMemoryError::AllocationFailed)
        }
    }

    /// Seal memory permissions from Writable to Executable (Read + Execute).
    /// Enforces W^X and flushes the instruction cache.
    pub fn seal_executable(&mut self) -> Result<(), JitMemoryError> {
        if self.is_executable {
            return Ok(());
        }

        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::System::Memory::{PAGE_EXECUTE_READ, VirtualProtect};

            unsafe extern "system" {
                fn FlushInstructionCache(
                    hProcess: *mut std::ffi::c_void,
                    lpBaseAddress: *const std::ffi::c_void,
                    dwSize: usize,
                ) -> i32;
                fn GetCurrentProcess() -> *mut std::ffi::c_void;
            }

            let mut old_protect = 0;
            let success = VirtualProtect(
                self.ptr as _,
                self.size,
                PAGE_EXECUTE_READ,
                &mut old_protect,
            );
            if success == 0 {
                return Err(JitMemoryError::ProtectionChangeFailed);
            }
            FlushInstructionCache(GetCurrentProcess(), self.ptr as _, self.size);
        }

        #[cfg(unix)]
        unsafe {
            let ret = libc::mprotect(
                self.ptr as *mut _,
                self.size,
                libc::PROT_READ | libc::PROT_EXEC,
            );
            if ret != 0 {
                return Err(JitMemoryError::ProtectionChangeFailed);
            }
            #[cfg(target_arch = "aarch64")]
            {
                #[cfg(target_os = "macos")]
                libc::sys_icache_invalidate(self.ptr, self.size);
                #[cfg(target_os = "linux")]
                {
                    let start = self.ptr as usize;
                    let end = start + self.size;
                    std::arch::aarch64::__clear_cache(start as *mut _, end as *mut _);
                }
            }
        }

        self.is_executable = true;
        Ok(())
    }

    /// Access mutable pointer to write code while the block is in Writable mode.
    ///
    /// # Safety
    /// Calling this while `is_executable` is true will panic, maintaining strict W^X.
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        assert!(
            !self.is_executable,
            "Cannot obtain mutable pointer to executable JIT memory"
        );
        self.ptr
    }

    /// Get executable function pointer or code buffer start.
    pub fn as_ptr(&self) -> *const u8 {
        self.ptr as *const u8
    }

    /// Get allocated size in bytes.
    pub fn len(&self) -> usize {
        self.size
    }

    /// Check if the block is currently sealed as executable.
    pub fn is_executable(&self) -> bool {
        self.is_executable
    }
}

impl Drop for JitMemoryBlock {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::System::Memory::{MEM_RELEASE, VirtualFree};
            VirtualFree(self.ptr as _, 0, MEM_RELEASE);
        }

        #[cfg(unix)]
        unsafe {
            libc::munmap(self.ptr as _, self.size);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jit_memory_lifecycle() {
        let mut mem = JitMemoryBlock::allocate_writable(64).expect("allocation failed");
        assert!(!mem.is_executable());
        assert!(mem.len() >= 64);

        // Can write into it
        unsafe {
            let slice = std::slice::from_raw_parts_mut(mem.as_mut_ptr(), 64);
            slice[0] = 0xC3; // RET instruction on x86
        }

        // Seal to executable
        mem.seal_executable().expect("sealing failed");
        assert!(mem.is_executable());

        // Call the RET instruction
        type EmptyFn = extern "C" fn();
        let f: EmptyFn = unsafe { std::mem::transmute(mem.as_ptr()) };
        f(); // Should cleanly return without crashing!
    }
}
