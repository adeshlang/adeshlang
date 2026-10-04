//! Native Threading, Mutex, Synchronization, and Thread-Local Storage.

use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex as StdMutex, OnceLock};
use std::thread::{JoinHandle, spawn};

/// Native Adesh Mutex wrapping fast OS synchronization primitives.
pub struct AdeshMutex<T> {
    inner: StdMutex<T>,
}

impl<T> AdeshMutex<T> {
    pub fn new(value: T) -> Self {
        Self {
            inner: StdMutex::new(value),
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
    pending: Arc<(StdMutex<usize>, Condvar)>,
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
    queue: Arc<StdMutex<PoolQueue>>,
    wake: Arc<Condvar>,
    total_failures: Arc<AtomicUsize>,
}

fn worker_loop(queue: Arc<StdMutex<PoolQueue>>, wake: Arc<Condvar>, failures: Arc<AtomicUsize>) {
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
        let queue = Arc::new(StdMutex::new(PoolQueue {
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
        let pending = Arc::new((StdMutex::new(jobs.len()), Condvar::new()));
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

// ============================================================================
// Section 25: Production Native Threading Primitives
// ============================================================================

/// Native Adesh Thread handle.
pub struct AdeshThread {
    handle: Option<JoinHandle<()>>,
}

impl AdeshThread {
    pub fn spawn<F>(f: F) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        Self {
            handle: Some(spawn(f)),
        }
    }

    pub fn join(mut self) -> Result<(), ()> {
        if let Some(h) = self.handle.take() {
            h.join().map_err(|_| ())
        } else {
            Ok(())
        }
    }
}

/// Native Adesh Reader-Writer Lock.
pub struct AdeshRwLock<T> {
    inner: std::sync::RwLock<T>,
}

impl<T> AdeshRwLock<T> {
    pub fn new(value: T) -> Self {
        Self {
            inner: std::sync::RwLock::new(value),
        }
    }

    pub fn read(&self) -> std::sync::RwLockReadGuard<'_, T> {
        self.inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn write(&self) -> std::sync::RwLockWriteGuard<'_, T> {
        self.inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Native Counting Semaphore.
pub struct AdeshSemaphore {
    count: StdMutex<usize>,
    cond: Condvar,
}

impl AdeshSemaphore {
    pub fn new(initial: usize) -> Self {
        Self {
            count: StdMutex::new(initial),
            cond: Condvar::new(),
        }
    }

    pub fn acquire(&self) {
        let mut count = self.count.lock().unwrap_or_else(|p| p.into_inner());
        while *count == 0 {
            count = self.cond.wait(count).unwrap_or_else(|p| p.into_inner());
        }
        *count -= 1;
    }

    pub fn release(&self) {
        let mut count = self.count.lock().unwrap_or_else(|p| p.into_inner());
        *count += 1;
        self.cond.notify_one();
    }
}

/// Native Once initialization primitive.
pub struct AdeshOnce {
    inner: std::sync::Once,
}

impl AdeshOnce {
    pub const fn new() -> Self {
        Self {
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

/// Native SpinLock with pause instruction for low-latency synchronization.
pub struct AdeshSpinLock {
    locked: std::sync::atomic::AtomicBool,
}

impl AdeshSpinLock {
    pub const fn new() -> Self {
        Self {
            locked: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub fn lock(&self) {
        while self.locked.swap(true, Ordering::Acquire) {
            while self.locked.load(Ordering::Relaxed) {
                std::hint::spin_loop();
            }
        }
    }

    pub fn unlock(&self) {
        self.locked.store(false, Ordering::Release);
    }
}

impl Default for AdeshSpinLock {
    fn default() -> Self {
        Self::new()
    }
}

/// Native 64-bit Atomic Variable supporting memory models.
pub struct AdeshAtomicI64 {
    inner: std::sync::atomic::AtomicI64,
}

impl AdeshAtomicI64 {
    pub const fn new(val: i64) -> Self {
        Self {
            inner: std::sync::atomic::AtomicI64::new(val),
        }
    }

    #[inline]
    pub fn load(&self, order: Ordering) -> i64 {
        self.inner.load(order)
    }

    #[inline]
    pub fn store(&self, val: i64, order: Ordering) {
        self.inner.store(val, order);
    }

    #[inline]
    pub fn fetch_add(&self, val: i64, order: Ordering) -> i64 {
        self.inner.fetch_add(val, order)
    }

    #[inline]
    pub fn compare_exchange(
        &self,
        current: i64,
        new: i64,
        success: Ordering,
        failure: Ordering,
    ) -> Result<i64, i64> {
        self.inner.compare_exchange(current, new, success, failure)
    }
}

// Re-exports conforming to Section 25 names
pub type Thread = AdeshThread;
pub type Mutex<T> = AdeshMutex<T>;
pub type RWLock<T> = AdeshRwLock<T>;
pub type ConditionVariable = AdeshCondVar;
pub type Semaphore = AdeshSemaphore;
pub type Once = AdeshOnce;
pub type SpinLock = AdeshSpinLock;
pub type Atomic = AdeshAtomicI64;
pub use std::sync::atomic::Ordering as AtomicOrdering;

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
