//! High-performance Rayon-backed work-stealing parallel scheduler

use rayon::ThreadPool;
use rayon::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::{decrement_active_tasks, increment_active_tasks};

/// Work-stealing scheduler backed by a tuned Rayon thread pool
pub struct Scheduler {
    pool: Arc<ThreadPool>,
    worker_count: usize,
    running: AtomicBool,
}

impl Scheduler {
    /// Create a scheduler with default CPU core worker count
    pub fn new() -> Self {
        let count = crate::runtime::thread::logical_cpu_count().max(1);
        Self::new_with_workers(count)
    }

    /// Create a scheduler with a specific worker thread count
    pub fn new_with_workers(worker_count: usize) -> Self {
        let count = worker_count.max(1);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(count)
            .thread_name(|idx| format!("adesh-worker-{idx}"))
            .panic_handler(|panic_info| {
                eprintln!("[adesh-parallel-worker panic]: {:?}", panic_info);
            })
            .build()
            .unwrap_or_else(|e| {
                eprintln!("[adesh scheduler warning]: Failed to build custom threadpool: {e}, falling back to default");
                rayon::ThreadPoolBuilder::new().build().unwrap()
            });

        Scheduler {
            pool: Arc::new(pool),
            worker_count: count,
            running: AtomicBool::new(true),
        }
    }

    pub fn worker_count(&self) -> usize {
        self.worker_count
    }

    /// Execute closure inside the scheduler's thread pool
    #[inline]
    pub fn install<R, F>(&self, op: F) -> R
    where
        F: FnOnce() -> R + Send,
        R: Send,
    {
        self.pool.install(op)
    }

    /// Execute parallel_for with adaptive chunking and work stealing
    pub fn parallel_for<F>(&self, start: u64, end: u64, body: F)
    where
        F: Fn(u64) + Send + Sync,
    {
        if start >= end {
            return;
        }
        let total = end - start;
        // For tiny workloads, execute sequentially to eliminate scheduling overhead
        if total < 32 {
            for i in start..end {
                body(i);
            }
            return;
        }

        increment_active_tasks();
        self.pool.install(|| {
            (start..end).into_par_iter().for_each(|i| {
                body(i);
            });
        });
        decrement_active_tasks();
    }

    /// Execute parallel map over a slice, returning a new vector with preserved order
    pub fn parallel_map<T, R, F>(&self, data: &[T], op: F) -> Vec<R>
    where
        T: Sync + Send,
        R: Send,
        F: Fn(&T) -> R + Sync + Send,
    {
        if data.is_empty() {
            return Vec::new();
        }
        if data.len() < 32 {
            return data.iter().map(op).collect();
        }

        increment_active_tasks();
        let result = self.pool.install(|| data.par_iter().map(op).collect());
        decrement_active_tasks();
        result
    }

    /// Execute parallel filter over a slice, preserving order
    pub fn parallel_filter<T, F>(&self, data: &[T], predicate: F) -> Vec<T>
    where
        T: Sync + Clone + Send,
        F: Fn(&T) -> bool + Sync + Send,
    {
        if data.is_empty() {
            return Vec::new();
        }
        if data.len() < 32 {
            return data.iter().filter(|x| predicate(x)).cloned().collect();
        }

        increment_active_tasks();
        let result = self
            .pool
            .install(|| data.par_iter().filter(|x| predicate(x)).cloned().collect());
        decrement_active_tasks();
        result
    }

    /// Parallel reduction with tree combine
    pub fn parallel_reduce<T, F>(&self, data: &[T], init: T, op: F, _deterministic: bool) -> T
    where
        T: Send + Sync + Clone,
        F: Fn(T, &T) -> T + Sync + Send,
    {
        if data.is_empty() {
            return init;
        }
        if data.len() < 32 {
            return data.iter().fold(init, |acc, x| op(acc, x));
        }

        increment_active_tasks();
        let result = self.pool.install(|| {
            data.par_iter()
                .fold(|| init.clone(), |acc, x| op(acc, x))
                .reduce(|| init.clone(), |a, b| op(a, &b))
        });
        decrement_active_tasks();
        result
    }

    /// Fork-join parallel execution of two operations
    pub fn parallel_join<A, B, RA, RB>(&self, oper_a: A, oper_b: B) -> (RA, RB)
    where
        A: FnOnce() -> RA + Send,
        B: FnOnce() -> RB + Send,
        RA: Send,
        RB: Send,
    {
        self.pool.install(|| rayon::join(oper_a, oper_b))
    }

    pub fn shutdown(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}
