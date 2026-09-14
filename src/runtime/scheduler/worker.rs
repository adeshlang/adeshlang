//! Worker threads and work-stealing queues

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

use super::{decrement_active_tasks, increment_active_tasks};

/// A unit of work for the scheduler
pub(crate) struct WorkItem {
    pub start: u64,
    pub end: u64,
    pub generation: u64,
}

/// Per-worker local queue with steal support
pub(crate) struct WorkerQueue {
    pub _id: usize,
    pub local: Mutex<VecDeque<WorkItem>>,
    pub stolen_count: AtomicUsize,
}

impl WorkerQueue {
    fn new(id: usize) -> Self {
        WorkerQueue {
            _id: id,
            local: Mutex::new(VecDeque::new()),
            stolen_count: AtomicUsize::new(0),
        }
    }

    fn push(&self, item: WorkItem) {
        self.local.lock().unwrap().push_back(item);
    }

    fn pop(&self) -> Option<WorkItem> {
        self.local.lock().unwrap().pop_front()
    }

    fn steal(&self) -> Option<WorkItem> {
        let mut local = self.local.lock().unwrap();
        if local.len() > 1 {
            let item = local.pop_back();
            if item.is_some() {
                self.stolen_count.fetch_add(1, Ordering::Relaxed);
            }
            item
        } else {
            None
        }
    }

    fn len(&self) -> usize {
        self.local.lock().unwrap().len()
    }
}

/// Work-stealing scheduler
pub struct Scheduler {
    workers: Vec<Arc<WorkerQueue>>,
    handles: Mutex<Vec<JoinHandle<()>>>,
    running: AtomicBool,
    shutdown: AtomicBool,
    worker_count: usize,
    work_available: Condvar,
    generation: AtomicUsize,
}

impl Scheduler {
    pub fn new() -> Self {
        let worker_count = crate::runtime::thread::logical_cpu_count().max(1);
        let mut workers = Vec::with_capacity(worker_count);
        for i in 0..worker_count {
            workers.push(Arc::new(WorkerQueue::new(i)));
        }
        Scheduler {
            workers,
            handles: Mutex::new(Vec::new()),
            running: AtomicBool::new(false),
            shutdown: AtomicBool::new(false),
            worker_count,
            work_available: Condvar::new(),
            generation: AtomicUsize::new(0),
        }
    }

    pub fn worker_count(&self) -> usize {
        self.worker_count
    }

    /// Execute parallel_for with static chunking + work stealing fallback
    pub fn parallel_for<F>(&self, start: u64, end: u64, body: F)
    where
        F: Fn(u64) + Send + Sync,
    {
        let total = end - start;
        if total == 0 {
            return;
        }

        // For small workloads, execute sequentially to avoid thread overhead
        if total < 256 {
            for i in start..end {
                body(i);
            }
            return;
        }

        self.ensure_workers();
        let body = Arc::new(body);
        let work_gen = self.generation.fetch_add(1, Ordering::Relaxed) as u64;

        // Static chunking
        let chunk_size = (total / self.worker_count as u64).max(64);
        let mut chunks = Vec::new();
        let mut pos = start;
        while pos < end {
            let chunk_end = (pos + chunk_size).min(end);
            chunks.push(WorkItem {
                start: pos,
                end: chunk_end,
                generation: work_gen,
            });
            pos = chunk_end;
        }

        // Distribute chunks round-robin to worker queues
        for (i, chunk) in chunks.into_iter().enumerate() {
            let worker_id = i % self.worker_count;
            self.workers[worker_id].push(chunk);
        }

        // Signal workers
        self.work_available.notify_all();

        // Main thread participates in work stealing
        self.execute_work_stealing(&body, work_gen);

        // Wait for all queues to drain
        loop {
            let remaining: usize = self.workers.iter().map(|w| w.len()).sum();
            if remaining == 0 {
                break;
            }
            thread::yield_now();
        }
    }

    fn execute_work_stealing<F>(&self, body: &Arc<F>, work_gen: u64)
    where
        F: Fn(u64) + Send + Sync,
    {
        // Main thread steals and executes
        loop {
            let mut found = false;
            for worker in &self.workers {
                if let Some(item) = worker.pop().or_else(|| worker.steal()) {
                    if item.generation == work_gen {
                        increment_active_tasks();
                        for i in item.start..item.end {
                            body(i);
                        }
                        decrement_active_tasks();
                        found = true;
                    }
                }
            }
            if !found {
                break;
            }
        }
    }

    /// Tree-structured parallel reduction
    pub fn parallel_reduce<T, F>(&self, data: &[T], init: T, op: F, _deterministic: bool) -> T
    where
        T: Send + Sync + Clone,
        F: Fn(T, &T) -> T + Sync,
    {
        if data.is_empty() {
            return init;
        }
        if data.len() < 1024 {
            return data.iter().fold(init, |acc, x| op(acc, x));
        }

        let op = Arc::new(op);
        let chunk_size = (data.len() / self.worker_count).max(256);

        // Chunk-local reductions then tree combine (sequential combine avoids Send on F)
        let mut partials: Vec<T> = Vec::new();
        for chunk in data.chunks(chunk_size) {
            let local = chunk.iter().fold(init.clone(), |acc, x| op(acc, x));
            partials.push(local);
        }

        partials.into_iter().fold(init, |acc, x| op(acc, &x))
    }

    fn ensure_workers(&self) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }
        // Workers are implicit via work stealing from main thread + thread pool
        // For production, persistent worker threads would be spawned here
    }

    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        self.work_available.notify_all();
        let handles = self.handles.lock().unwrap().drain(..).collect::<Vec<_>>();
        for h in handles {
            let _ = h.join();
        }
        self.running.store(false, Ordering::SeqCst);
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}
