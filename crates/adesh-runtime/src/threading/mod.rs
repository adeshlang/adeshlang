//! Native Threading, Mutex, Synchronization, and Thread-Local Storage.

use std::sync::{Condvar, Mutex};
use std::thread::{JoinHandle, spawn};

/// Native Adesh Mutex wrapping fast OS synchronization primitives.
pub struct AdeshMutex<T> {
    inner: Mutex<T>,
}

impl<T> AdeshMutex<T> {
    pub fn new(value: T) -> Self {
        Self {
            inner: Mutex::new(value),
        }
    }

    pub fn lock(&self) -> std::sync::MutexGuard<'_, T> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Native Adesh Condition Variable for thread rendezvous.
pub struct AdeshCondVar {
    inner: Condvar,
}

impl AdeshCondVar {
    pub fn new() -> Self {
        Self {
            inner: Condvar::new(),
        }
    }

    pub fn wait<'a, T>(&self, guard: std::sync::MutexGuard<'a, T>) -> std::sync::MutexGuard<'a, T> {
        self.inner
            .wait(guard)
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn notify_one(&self) {
        self.inner.notify_one();
    }

    pub fn notify_all(&self) {
        self.inner.notify_all();
    }
}

impl Default for AdeshCondVar {
    fn default() -> Self {
        Self::new()
    }
}

/// Native Worker Thread Pool.
pub struct AdeshThreadPool {
    workers: Vec<Option<JoinHandle<()>>>,
}

impl AdeshThreadPool {
    pub fn new(num_threads: usize) -> Self {
        let workers = (0..num_threads)
            .map(|_| {
                Some(spawn(|| {
                    // Worker loop
                }))
            })
            .collect();
        Self { workers }
    }

    pub fn thread_count(&self) -> usize {
        self.workers.len()
    }
}
