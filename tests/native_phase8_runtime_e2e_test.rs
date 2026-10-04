//! End-to-End Native Runtime Tests (Phase 8).
//!
//! Verifies:
//! 1. Platform virtual memory management (allocation, writing, freeing).
//! 2. Platform dynamic library loading abstraction.
//! 3. Platform high-resolution monotonic timer.
//! 4. Concurrency primitives: Thread, Mutex, RWLock, Semaphore, Once, SpinLock.
//! 5. Runtime atomic operations.

#![allow(dead_code, unused_imports)]

use adesh_runtime::platform::{DynamicLibrary, PlatformTimer, VirtualMemory, system_error_code};
use adesh_runtime::threading::{AdeshAtomicI64, AtomicOrdering, Mutex, Once, RWLock, Semaphore, SpinLock, Thread};
use std::sync::Arc;
use std::time::Duration;

#[test]
fn test_platform_virtual_memory() {
    let size = 65536; // 64 KB
    let ptr = VirtualMemory::allocate(size, false);
    assert!(!ptr.is_null(), "virtual memory allocation must succeed");

    // Write to memory
    unsafe {
        for i in 0..1024 {
            *ptr.add(i) = (i % 256) as u8;
        }
        for i in 0..1024 {
            assert_eq!(*ptr.add(i), (i % 256) as u8);
        }
    }

    let freed = VirtualMemory::free(ptr, size);
    assert!(freed, "virtual memory freeing must succeed");
}

#[test]
fn test_platform_dynamic_library_loading() {
    #[cfg(target_os = "windows")]
    let lib_name = "kernel32.dll";
    #[cfg(not(target_os = "windows"))]
    let lib_name = "libc.so.6";

    let lib = DynamicLibrary::load(lib_name).expect("load system dynamic library");
    
    #[cfg(target_os = "windows")]
    let sym_name = "GetCurrentProcessId";
    #[cfg(not(target_os = "windows"))]
    let sym_name = "getpid";

    let sym_ptr = lib.symbol(sym_name).expect("find symbol");
    assert!(!sym_ptr.is_null());

    let pid_fn: unsafe extern "C" fn() -> u32 = unsafe { std::mem::transmute(sym_ptr) };
    let pid = unsafe { pid_fn() };
    assert_eq!(pid, std::process::id());
}

#[test]
fn test_platform_timer_and_system_error() {
    let t0 = PlatformTimer::now();
    std::thread::sleep(Duration::from_millis(15));
    let elapsed = PlatformTimer::elapsed_nanos(t0);
    assert!(elapsed >= 10_000_000, "elapsed time must be at least 10ms, got {}ns", elapsed);

    let _err = system_error_code();
}

#[test]
fn test_threading_mutex_and_spawn() {
    let counter: Arc<Mutex<i64>> = Arc::new(Mutex::new(0i64));
    let mut handles: Vec<Thread> = Vec::new();

    for _ in 0..8 {
        let c: Arc<Mutex<i64>> = Arc::clone(&counter);
        handles.push(Thread::spawn(move || {
            for _ in 0..1000 {
                let mut guard = c.lock();
                *guard += 1;
            }
        }));
    }

    for h in handles {
        h.join().expect("thread join");
    }

    assert_eq!(*counter.lock(), 8000);
}

#[test]
fn test_threading_rwlock_and_spin_lock() {
    let rwlock = Arc::new(RWLock::new(42i64));
    let spinlock = Arc::new(SpinLock::new());

    // Readers
    let r1 = rwlock.read();
    let r2 = rwlock.read();
    assert_eq!(*r1, 42);
    assert_eq!(*r2, 42);
    drop(r1);
    drop(r2);

    // Writer
    {
        let mut w = rwlock.write();
        *w = 99;
    }
    assert_eq!(*rwlock.read(), 99);

    // SpinLock
    {
        spinlock.lock();
        // critical section
        spinlock.unlock();
    }
}

#[test]
fn test_threading_semaphore_and_once() {
    let sem: Arc<Semaphore> = Arc::new(Semaphore::new(2));
    let once: Arc<Once> = Arc::new(Once::new());
    let init_counter: Arc<AdeshAtomicI64> = Arc::new(AdeshAtomicI64::new(0));

    let mut handles: Vec<Thread> = Vec::new();
    for _ in 0..4 {
        let s: Arc<Semaphore> = Arc::clone(&sem);
        let o: Arc<Once> = Arc::clone(&once);
        let c: Arc<AdeshAtomicI64> = Arc::clone(&init_counter);
        handles.push(Thread::spawn(move || {
            s.acquire();
            o.call_once(|| {
                c.fetch_add(1, AtomicOrdering::SeqCst);
            });
            s.release();
        }));
    }

    for h in handles {
        h.join().expect("join");
    }

    assert_eq!(init_counter.load(AtomicOrdering::SeqCst), 1);
}
