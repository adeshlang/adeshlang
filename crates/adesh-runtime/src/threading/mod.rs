//! Native Threading, Mutex, Synchronization, and Thread-Local Storage.

use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
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

// ============================================================================
// Worker Thread Pool
// ============================================================================

/// A unit of work submitted to the pool.
type Job = Box<dyn FnOnce() + Send + 'static>;

struct PoolQueue {
    jobs: VecDeque<Job>,
    shutdown: bool,
}

/// Decrements a batch's outstanding-job counter when dropped. Drop (rather
/// than an explicit call) means the counter is released even if the task
/// panicked mid-flight, so a panicking task can never leave its caller
/// blocked in `run_batch` forever.
struct BatchTicket {
    pending: Arc<(Mutex<usize>, Condvar)>,
}

impl Drop for BatchTicket {
    fn drop(&mut self) {
        let (lock, done) = &*self.pending;
        let mut left = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        // One ticket per submitted job; the counter never underflows here.
        *left = left.saturating_sub(1);
        if *left == 0 {
            done.notify_all();
        }
    }
}

/// Native Worker Thread Pool.
///
/// Audit fix: workers used to be spawned per parallel loop with an empty body
/// (so they exited immediately) and were never joined. This pool spawns its
/// workers exactly once, parks them on a condition variable while idle, and
/// reuses them for every batch. Each task runs inside `catch_unwind`, so a
/// panicking task can neither kill its worker thread nor poison the pool's
/// internal lock (the lock is poison-recovering anyway).
pub struct AdeshThreadPool {
    workers: Vec<Option<JoinHandle<()>>>,
    queue: Arc<Mutex<PoolQueue>>,
    wake: Arc<Condvar>,
    total_failures: Arc<AtomicUsize>,
}

fn worker_loop(queue: Arc<Mutex<PoolQueue>>, wake: Arc<Condvar>, failures: Arc<AtomicUsize>) {
    loop {
        // Park until a job arrives or the pool is shut down.
        let job = {
            let mut guard = queue
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            loop {
                if let Some(job) = guard.jobs.pop_front() {
                    break job;
                }
                if guard.shutdown {
                    return;
                }
                guard = wake
                    .wait(guard)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
        };
        // Defense in depth: tasks already contain their own catch_unwind (see
        // `run_batch`); this outer one guarantees a panic can never escape
        // into the parked worker loop and silently kill the worker.
        if catch_unwind(AssertUnwindSafe(job)).is_err() {
            failures.fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl AdeshThreadPool {
    pub fn new(num_threads: usize) -> Self {
        let threads = num_threads.max(1);
        let queue = Arc::new(Mutex::new(PoolQueue {
            jobs: VecDeque::new(),
            shutdown: false,
        }));
        let wake = Arc::new(Condvar::new());
        let total_failures = Arc::new(AtomicUsize::new(0));

        let workers = (0..threads)
            .map(|_| {
                let queue = Arc::clone(&queue);
                let wake = Arc::clone(&wake);
                let failures = Arc::clone(&total_failures);
                Some(spawn(move || worker_loop(queue, wake, failures)))
            })
            .collect();

        Self {
            workers,
            queue,
            wake,
            total_failures,
        }
    }

    pub fn thread_count(&self) -> usize {
        self.workers.len()
    }

    /// Total number of tasks that have panicked in this pool since it was
    /// created (across all batches).
    pub fn failed_tasks(&self) -> usize {
        self.total_failures.load(Ordering::Relaxed)
    }

    /// Submit `jobs` and block until every one of them has finished executing.
    /// Returns how many of the submitted jobs panicked (0 = all succeeded).
    pub fn run_batch(&self, jobs: Vec<Job>) -> usize {
        if jobs.is_empty() {
            return 0;
        }
        let pending = Arc::new((Mutex::new(jobs.len()), Condvar::new()));
        let batch_panics = Arc::new(AtomicUsize::new(0));

        for job in jobs {
            let ticket = BatchTicket {
                pending: Arc::clone(&pending),
            };
            let failures = Arc::clone(&self.total_failures);
            let panics = Arc::clone(&batch_panics);
            let task: Job = Box::new(move || {
                // Dropping `ticket` at scope exit releases the batch slot,
                // whether this task finishes normally or panics.
                let _release = ticket;
                // Panic containment per task: a failing task must not take
                // the worker (and with it the whole pool) down.
                if catch_unwind(AssertUnwindSafe(job)).is_err() {
                    panics.fetch_add(1, Ordering::Relaxed);
                    failures.fetch_add(1, Ordering::Relaxed);
                }
            });
            {
                let mut guard = self
                    .queue
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                guard.jobs.push_back(task);
            }
            self.wake.notify_one();
        }

        // Block until every task of this batch has finished (successfully or
        // not); the BatchTickets guarantee the counter reaches zero.
        let (lock, done) = &*pending;
        let mut left = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        while *left > 0 {
            left = done
                .wait(left)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        batch_panics.load(Ordering::Relaxed)
    }

    /// Apply `callback` to every index in `[start, end)` using the pool's
    /// workers, splitting the range into `chunk`-sized tasks. Blocks until all
    /// tasks finish and returns the number of panicked tasks (0 = success).
    pub fn parallel_for_each(
        &self,
        start: i64,
        end: i64,
        chunk: i64,
        callback: extern "C" fn(i64),
    ) -> usize {
        let step = chunk.max(1);
        let mut jobs: Vec<Job> = Vec::new();
        let mut lo = start;
        while lo < end {
            let hi = lo.saturating_add(step).min(end);
            jobs.push(Box::new(move || {
                for i in lo..hi {
                    callback(i);
                }
            }));
            lo = hi;
        }
        self.run_batch(jobs)
    }

    /// The process-wide shared pool: created lazily on first use and reused
    /// for every parallel loop. Workers park while idle, so the pool is
    /// intentionally never joined or dropped (daemon-style); the OS reclaims
    /// its threads at process exit.
    pub fn global() -> &'static AdeshThreadPool {
        static GLOBAL: OnceLock<AdeshThreadPool> = OnceLock::new();
        GLOBAL.get_or_init(|| {
            let threads = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
                .clamp(2, 8);
            AdeshThreadPool::new(threads)
        })
    }
}

impl Drop for AdeshThreadPool {
    fn drop(&mut self) {
        // Tell every worker to exit and join it. Jobs still queued are simply
        // dropped; their BatchTickets still fire, so any concurrent
        // `run_batch` caller is released instead of deadlocking.
        {
            let mut guard = self
                .queue
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            guard.shutdown = true;
        }
        self.wake.notify_all();
        for worker in self.workers.iter_mut() {
            if let Some(handle) = worker.take() {
                let _ = handle.join();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AdeshThreadPool, Job};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn pool_reuses_workers_and_contains_task_panics() {
        let pool = AdeshThreadPool::new(2);
        let completed = Arc::new(AtomicUsize::new(0));
        let first_batch: Vec<Job> = (0..8)
            .map(|_| {
                let completed = Arc::clone(&completed);
                Box::new(move || {
                    completed.fetch_add(1, Ordering::Relaxed);
                }) as Job
            })
            .collect();
        assert_eq!(pool.run_batch(first_batch), 0);
        assert_eq!(completed.load(Ordering::Relaxed), 8);

        let second_batch: Vec<Job> = vec![Box::new(|| panic!("contained worker-task panic")), {
            let completed = Arc::clone(&completed);
            Box::new(move || {
                completed.fetch_add(1, Ordering::Relaxed);
            })
        }];
        assert_eq!(pool.run_batch(second_batch), 1);
        assert_eq!(pool.failed_tasks(), 1);
        assert_eq!(completed.load(Ordering::Relaxed), 9);
        assert_eq!(pool.thread_count(), 2);
    }
}
