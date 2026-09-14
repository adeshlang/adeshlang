//! Work-stealing parallel scheduler for AdeshLang
//!
//! Lightweight scheduler with:
//! - One worker per CPU core (configurable)
//! - Worker-local queues with work stealing
//! - No global lock in hot path
//! - Lazy initialization (zero cost when unused)
//! - Nested parallelism flattening

pub mod channel;
pub mod sync;
pub mod task;
pub mod worker;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

pub use channel::*;
pub use sync::*;
pub use task::*;
pub use worker::*;

/// Global scheduler instance — initialized only when parallel ops are used
static SCHEDULER: OnceLock<Arc<Scheduler>> = OnceLock::new();

/// Whether the parallel runtime has been initialized
static PARALLEL_ENABLED: AtomicBool = AtomicBool::new(false);

/// Get or lazily initialize the global scheduler
pub fn global_scheduler() -> Arc<Scheduler> {
    SCHEDULER
        .get_or_init(|| {
            PARALLEL_ENABLED.store(true, Ordering::Release);
            Arc::new(Scheduler::new())
        })
        .clone()
}

/// Check if parallel runtime is active (for zero-overhead verification)
pub fn is_parallel_runtime_active() -> bool {
    PARALLEL_ENABLED.load(Ordering::Acquire)
}

/// Shutdown the global scheduler gracefully
pub fn shutdown_scheduler() {
    if let Some(sched) = SCHEDULER.get() {
        sched.shutdown();
    }
}

/// Parallel for — divide [start, end) into chunks and execute on workers
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

/// Parallel reduce with tree reduction
pub fn parallel_reduce<T, F>(data: &[T], init: T, op: F, deterministic: bool) -> T
where
    T: Send + Sync + Clone,
    F: Fn(T, &T) -> T + Sync,
{
    if data.is_empty() {
        return init;
    }
    let sched = global_scheduler();
    sched.parallel_reduce(data, init, op, deterministic)
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
