use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
/// Concurrency-safe memory primitives for AdeshLang
/// - shared<T>: safely shared immutable data across threads
/// - atomic<T>: atomic operations on primitive types
/// - mutex<T>: mutual exclusion lock
/// - rwlock<T>: reader-writer lock
use std::sync::{Arc, Mutex, RwLock};

/// Safely shared immutable data across thread boundaries
#[derive(Debug, Clone)]
pub struct Shared<T: Send + Sync> {
    inner: Arc<T>,
}

impl<T: Send + Sync> Shared<T> {
    pub fn new(value: T) -> Self {
        Shared {
            inner: Arc::new(value),
        }
    }

    pub fn get(&self) -> &T {
        &self.inner
    }
}

/// Atomic wrapper for types supporting atomic operations
#[derive(Debug)]
pub enum Atomic {
    U64(AtomicU64),
    Bool(AtomicBool),
}

impl Atomic {
    pub fn new_u64(value: u64) -> Self {
        Atomic::U64(AtomicU64::new(value))
    }

    pub fn new_bool(value: bool) -> Self {
        Atomic::Bool(AtomicBool::new(value))
    }

    pub fn load_u64(&self) -> Option<u64> {
        match self {
            Atomic::U64(a) => Some(a.load(Ordering::SeqCst)),
            _ => None,
        }
    }

    pub fn store_u64(&self, value: u64) -> bool {
        match self {
            Atomic::U64(a) => {
                a.store(value, Ordering::SeqCst);
                true
            }
            _ => false,
        }
    }

    pub fn load_bool(&self) -> Option<bool> {
        match self {
            Atomic::Bool(a) => Some(a.load(Ordering::SeqCst)),
            _ => None,
        }
    }

    pub fn store_bool(&self, value: bool) -> bool {
        match self {
            Atomic::Bool(a) => {
                a.store(value, Ordering::SeqCst);
                true
            }
            _ => false,
        }
    }

    pub fn compare_exchange_u64(&self, current: u64, new: u64) -> Option<u64> {
        match self {
            Atomic::U64(a) => a
                .compare_exchange(current, new, Ordering::SeqCst, Ordering::SeqCst)
                .ok(),
            _ => None,
        }
    }
}

/// Mutual exclusion lock for thread-safe interior mutability
#[derive(Debug)]
pub struct Mutex_<T> {
    inner: Mutex<T>,
}

impl<T> Mutex_<T> {
    pub fn new(value: T) -> Self {
        Mutex_ {
            inner: Mutex::new(value),
        }
    }

    pub fn lock(&self) -> Result<std::sync::MutexGuard<'_, T>, String> {
        self.inner.lock().map_err(|e| e.to_string())
    }

    pub fn try_lock(&self) -> Option<std::sync::MutexGuard<'_, T>> {
        self.inner.try_lock().ok()
    }
}

/// Reader-writer lock for read-heavy workloads
#[derive(Debug)]
pub struct RwLock_<T> {
    inner: RwLock<T>,
}

impl<T> RwLock_<T> {
    pub fn new(value: T) -> Self {
        RwLock_ {
            inner: RwLock::new(value),
        }
    }

    pub fn read(&self) -> Result<std::sync::RwLockReadGuard<'_, T>, String> {
        self.inner.read().map_err(|e| e.to_string())
    }

    pub fn write(&self) -> Result<std::sync::RwLockWriteGuard<'_, T>, String> {
        self.inner.write().map_err(|e| e.to_string())
    }

    pub fn try_read(&self) -> Option<std::sync::RwLockReadGuard<'_, T>> {
        self.inner.try_read().ok()
    }

    pub fn try_write(&self) -> Option<std::sync::RwLockWriteGuard<'_, T>> {
        self.inner.try_write().ok()
    }
}

/// Thread ownership tracking for safe pointer sharing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThreadId(u64);

impl ThreadId {
    pub fn current() -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        std::thread::current().id().hash(&mut hasher);
        ThreadId(hasher.finish())
    }
}

/// Marker trait for types that can be safely shared across threads
pub trait ThreadSafe: Send + Sync {}

impl<T: Send + Sync> ThreadSafe for T {}

/// Runtime check for thread-safe access (debug builds only)
#[cfg(debug_assertions)]
pub fn assert_thread_safe_access(owner: Option<ThreadId>) -> Result<(), String> {
    match owner {
        None => Ok(()),
        Some(owner_id) => {
            if ThreadId::current() == owner_id {
                Ok(())
            } else {
                Err("Attempted to access value from different thread than owner".to_string())
            }
        }
    }
}

#[cfg(not(debug_assertions))]
pub fn assert_thread_safe_access(_owner: Option<ThreadId>) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared_creation() {
        let shared = Shared::new(42u64);
        assert_eq!(*shared.get(), 42);
    }

    #[test]
    fn test_atomic_u64() {
        let atomic = Atomic::new_u64(10);
        assert_eq!(atomic.load_u64(), Some(10));
        atomic.store_u64(20);
        assert_eq!(atomic.load_u64(), Some(20));
    }

    #[test]
    fn test_atomic_bool() {
        let atomic = Atomic::new_bool(false);
        assert_eq!(atomic.load_bool(), Some(false));
        atomic.store_bool(true);
        assert_eq!(atomic.load_bool(), Some(true));
    }

    #[test]
    fn test_mutex_lock() {
        let mutex = Mutex_::new(42);
        let guard = mutex.lock().unwrap();
        assert_eq!(*guard, 42);
    }

    #[test]
    fn test_rwlock() {
        let rwlock = RwLock_::new(100);
        let read_guard = rwlock.read().unwrap();
        assert_eq!(*read_guard, 100);
    }

    #[test]
    fn test_thread_id() {
        let tid = ThreadId::current();
        assert_eq!(tid, ThreadId::current());
    }
}
