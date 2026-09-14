//! Synchronization primitives for parallel execution
//!
//! Mutex, RwLock, Barrier, Semaphore, Once, ConditionVariable
//! Atomics with full memory ordering semantics

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64, AtomicU32, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, RwLock};

// ============================================================================
// Memory ordering re-exports for AdeshLang semantics
// ============================================================================

pub use Ordering::{
    AcqRel as MemoryOrderAcqRel, Acquire as MemoryOrderAcquire, Relaxed as MemoryOrderRelaxed,
    Release as MemoryOrderRelease, SeqCst as MemoryOrderSeqCst,
};

// ============================================================================
// Atomic types
// ============================================================================

/// AtomicBool with explicit memory ordering
pub struct AdeshAtomicBool {
    inner: AtomicBool,
}

impl AdeshAtomicBool {
    pub fn new(val: bool) -> Self {
        AdeshAtomicBool {
            inner: AtomicBool::new(val),
        }
    }
    pub fn load(&self, order: Ordering) -> bool {
        self.inner.load(order)
    }
    pub fn store(&self, val: bool, order: Ordering) {
        self.inner.store(val, order)
    }
    pub fn swap(&self, val: bool, order: Ordering) -> bool {
        self.inner.swap(val, order)
    }
    pub fn compare_exchange(
        &self,
        current: bool,
        new: bool,
        success: Ordering,
        failure: Ordering,
    ) -> Result<bool, bool> {
        self.inner.compare_exchange(current, new, success, failure)
    }
}

/// AtomicU64
pub struct AdeshAtomicU64 {
    inner: AtomicU64,
}

impl AdeshAtomicU64 {
    pub fn new(val: u64) -> Self {
        AdeshAtomicU64 {
            inner: AtomicU64::new(val),
        }
    }
    pub fn load(&self, order: Ordering) -> u64 {
        self.inner.load(order)
    }
    pub fn store(&self, val: u64, order: Ordering) {
        self.inner.store(val, order)
    }
    pub fn fetch_add(&self, val: u64, order: Ordering) -> u64 {
        self.inner.fetch_add(val, order)
    }
    pub fn fetch_sub(&self, val: u64, order: Ordering) -> u64 {
        self.inner.fetch_sub(val, order)
    }
    pub fn fetch_and(&self, val: u64, order: Ordering) -> u64 {
        self.inner.fetch_and(val, order)
    }
    pub fn fetch_or(&self, val: u64, order: Ordering) -> u64 {
        self.inner.fetch_or(val, order)
    }
    pub fn fetch_xor(&self, val: u64, order: Ordering) -> u64 {
        self.inner.fetch_xor(val, order)
    }
    pub fn compare_exchange(
        &self,
        current: u64,
        new: u64,
        success: Ordering,
        failure: Ordering,
    ) -> Result<u64, u64> {
        self.inner.compare_exchange(current, new, success, failure)
    }
}

/// AtomicI64
pub struct AdeshAtomicI64 {
    inner: AtomicI64,
}

impl AdeshAtomicI64 {
    pub fn new(val: i64) -> Self {
        AdeshAtomicI64 {
            inner: AtomicI64::new(val),
        }
    }
    pub fn load(&self, order: Ordering) -> i64 {
        self.inner.load(order)
    }
    pub fn store(&self, val: i64, order: Ordering) {
        self.inner.store(val, order)
    }
    pub fn fetch_add(&self, val: i64, order: Ordering) -> i64 {
        self.inner.fetch_add(val, order)
    }
}

/// AtomicU32
pub struct AdeshAtomicU32 {
    inner: AtomicU32,
}

impl AdeshAtomicU32 {
    pub fn new(val: u32) -> Self {
        AdeshAtomicU32 {
            inner: AtomicU32::new(val),
        }
    }
    pub fn load(&self, order: Ordering) -> u32 {
        self.inner.load(order)
    }
    pub fn store(&self, val: u32, order: Ordering) {
        self.inner.store(val, order)
    }
    pub fn fetch_add(&self, val: u32, order: Ordering) -> u32 {
        self.inner.fetch_add(val, order)
    }
}

/// AtomicI32
pub struct AdeshAtomicI32 {
    inner: AtomicI32,
}

impl AdeshAtomicI32 {
    pub fn new(val: i32) -> Self {
        AdeshAtomicI32 {
            inner: AtomicI32::new(val),
        }
    }
    pub fn load(&self, order: Ordering) -> i32 {
        self.inner.load(order)
    }
    pub fn store(&self, val: i32, order: Ordering) {
        self.inner.store(val, order)
    }
}

// ============================================================================
// Mutex, RwLock, Barrier, Semaphore
// ============================================================================

pub struct AdeshMutex<T> {
    inner: Mutex<T>,
}

impl<T> AdeshMutex<T> {
    pub fn new(val: T) -> Self {
        AdeshMutex {
            inner: Mutex::new(val),
        }
    }
    pub fn lock(&self) -> std::sync::LockResult<std::sync::MutexGuard<'_, T>> {
        self.inner.lock()
    }
    pub fn try_lock(&self) -> std::sync::TryLockResult<std::sync::MutexGuard<'_, T>> {
        self.inner.try_lock()
    }
}

pub struct AdeshRwLock<T> {
    inner: RwLock<T>,
}

impl<T> AdeshRwLock<T> {
    pub fn new(val: T) -> Self {
        AdeshRwLock {
            inner: RwLock::new(val),
        }
    }
    pub fn read(&self) -> std::sync::LockResult<std::sync::RwLockReadGuard<'_, T>> {
        self.inner.read()
    }
    pub fn write(&self) -> std::sync::LockResult<std::sync::RwLockWriteGuard<'_, T>> {
        self.inner.write()
    }
}

/// Synchronization barrier for N threads
pub struct AdeshBarrier {
    count: usize,
    arrived: Mutex<usize>,
    generation: Mutex<u64>,
    condvar: Condvar,
}

impl AdeshBarrier {
    pub fn new(count: usize) -> Self {
        AdeshBarrier {
            count,
            arrived: Mutex::new(0),
            generation: Mutex::new(0),
            condvar: Condvar::new(),
        }
    }

    pub fn wait(&self) {
        let mut arrived = self.arrived.lock().unwrap();
        *arrived += 1;
        if *arrived >= self.count {
            *arrived = 0;
            *self.generation.lock().unwrap() += 1;
            self.condvar.notify_all();
        } else {
            let work_gen = *self.generation.lock().unwrap();
            loop {
                arrived = self.condvar.wait(arrived).unwrap();
                if *self.generation.lock().unwrap() != work_gen {
                    break;
                }
            }
        }
    }
}

/// Counting semaphore
pub struct AdeshSemaphore {
    permits: Mutex<usize>,
    condvar: Condvar,
}

impl AdeshSemaphore {
    pub fn new(permits: usize) -> Self {
        AdeshSemaphore {
            permits: Mutex::new(permits),
            condvar: Condvar::new(),
        }
    }

    pub fn acquire(&self) {
        let mut permits = self.permits.lock().unwrap();
        while *permits == 0 {
            permits = self.condvar.wait(permits).unwrap();
        }
        *permits -= 1;
    }

    pub fn release(&self) {
        let mut permits = self.permits.lock().unwrap();
        *permits += 1;
        self.condvar.notify_one();
    }
}

/// One-time initialization
pub struct AdeshOnce {
    inner: std::sync::Once,
}

impl AdeshOnce {
    pub fn new() -> Self {
        AdeshOnce {
            inner: std::sync::Once::new(),
        }
    }

    pub fn call_once<F: FnOnce()>(&self, f: F) {
        self.inner.call_once(f);
    }
}

impl Default for AdeshOnce {
    fn default() -> Self {
        Self::new()
    }
}

/// Condition variable
pub struct AdeshCondvar {
    inner: Condvar,
}

impl AdeshCondvar {
    pub fn new() -> Self {
        AdeshCondvar {
            inner: Condvar::new(),
        }
    }

    pub fn wait<'a, T>(
        &self,
        guard: std::sync::MutexGuard<'a, T>,
    ) -> std::sync::LockResult<std::sync::MutexGuard<'a, T>> {
        self.inner.wait(guard)
    }

    pub fn notify_one(&self) {
        self.inner.notify_one();
    }

    pub fn notify_all(&self) {
        self.inner.notify_all();
    }
}

/// Cache-line padded accumulator to prevent false sharing
#[repr(align(64))]
pub struct PaddedAccumulator<T: Default> {
    pub value: T,
    _pad: [u8; 0],
}

impl<T: Default> PaddedAccumulator<T> {
    pub fn new() -> Self {
        PaddedAccumulator {
            value: T::default(),
            _pad: [],
        }
    }
}
