//! Work-stealing parallel scheduler for AdeshLang
//!
//! High-performance Rayon-backed scheduler with:
//! - Multi-threaded work-stealing thread pool
//! - Nested parallelism flattening
//! - Lock-free work distribution in hot path
//! - Adaptive chunking & SIMD fusion
//! - Lazy initialization (zero cost when unused)

pub mod channel;
pub mod sync;
pub mod task;
pub mod worker;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

pub use channel::*;
pub use sync::*;
pub use task::*;
pub use worker::*;

/// Global scheduler instance — initialized lazily on first parallel operation
static SCHEDULER: RwLock<Option<Arc<Scheduler>>> = RwLock::new(None);

/// Whether the parallel runtime has been initialized
static PARALLEL_ENABLED: AtomicBool = AtomicBool::new(false);

/// Get or lazily initialize the global scheduler
pub fn global_scheduler() -> Arc<Scheduler> {
    {
        let reader = SCHEDULER.read().unwrap();
        if let Some(ref sched) = *reader {
            return sched.clone();
        }
    }
    let mut writer = SCHEDULER.write().unwrap();
    if let Some(ref sched) = *writer {
        return sched.clone();
    }
    PARALLEL_ENABLED.store(true, Ordering::Release);
    let sched = Arc::new(Scheduler::new());
    *writer = Some(sched.clone());
    sched
}

/// Dynamically reconfigure the global worker thread count
pub fn set_global_workers(count: usize) {
    let mut writer = SCHEDULER.write().unwrap();
    PARALLEL_ENABLED.store(true, Ordering::Release);
    let sched = Arc::new(Scheduler::new_with_workers(count));
    *writer = Some(sched);
}

/// Check if parallel runtime is active (for zero-overhead verification)
pub fn is_parallel_runtime_active() -> bool {
    PARALLEL_ENABLED.load(Ordering::Acquire)
}

/// Shutdown the global scheduler gracefully
pub fn shutdown_scheduler() {
    let mut writer = SCHEDULER.write().unwrap();
    if let Some(sched) = writer.take() {
        sched.shutdown();
    }
    PARALLEL_ENABLED.store(false, Ordering::Release);
}

/// Parallel for — divide [start, end) into chunks and execute on work-stealing workers
pub fn parallel_for<F>(start: u64, end: u64, body: F)
where
    F: Fn(u64) + Send + Sync,
{
    if start >= end {
        return;
    }
    let sched = global_scheduler();
    sched.parallel_for(start, end, body);
}

/// Parallel map across a slice
pub fn parallel_map<T, R, F>(data: &[T], op: F) -> Vec<R>
where
    T: Sync + Send,
    R: Send,
    F: Fn(&T) -> R + Sync + Send,
{
    let sched = global_scheduler();
    sched.parallel_map(data, op)
}

/// Parallel filter across a slice
pub fn parallel_filter<T, F>(data: &[T], predicate: F) -> Vec<T>
where
    T: Sync + Clone + Send,
    F: Fn(&T) -> bool + Sync + Send,
{
    let sched = global_scheduler();
    sched.parallel_filter(data, predicate)
}

/// Parallel reduce with tree reduction
pub fn parallel_reduce<T, F>(data: &[T], init: T, op: F, deterministic: bool) -> T
where
    T: Send + Sync + Clone,
    F: Fn(T, &T) -> T + Sync + Send,
{
    if data.is_empty() {
        return init;
    }
    let sched = global_scheduler();
    sched.parallel_reduce(data, init, op, deterministic)
}

/// Parallel fork-join of two tasks
pub fn parallel_join<A, B, RA, RB>(oper_a: A, oper_b: B) -> (RA, RB)
where
    A: FnOnce() -> RA + Send,
    B: FnOnce() -> RB + Send,
    RA: Send,
    RB: Send,
{
    let sched = global_scheduler();
    sched.parallel_join(oper_a, oper_b)
}

/// Number of available worker threads
pub fn num_workers() -> usize {
    if is_parallel_runtime_active() {
        global_scheduler().worker_count()
    } else {
        crate::runtime::thread::logical_cpu_count()
    }
}

/// Active parallel task count (for diagnostics)
static ACTIVE_TASKS: AtomicUsize = AtomicUsize::new(0);

pub fn active_task_count() -> usize {
    ACTIVE_TASKS.load(Ordering::Relaxed)
}

pub(crate) fn increment_active_tasks() {
    ACTIVE_TASKS.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn decrement_active_tasks() {
    ACTIVE_TASKS.fetch_sub(1, Ordering::Relaxed);
}
