//! Concurrency and Parallelism Support for AdeshLang
//!
//! This module provides:
//! - Async/await runtime integration
//! - Thread pool for parallel execution
//! - Parallel loop constructs
//! - Channels for inter-task communication
//! - Synchronization primitives
//!
//! Design goals:
//! - Simple, safe concurrent programming model
//! - Efficient parallel loops for data-parallel workloads
//! - ML/AI-friendly batch operations

#![allow(dead_code)] // Infrastructure for future async runtime features

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::builtins::RuntimeValue;
use crate::utils::collections::FastMap;

// ============================================================================
// Task System
// ============================================================================

/// Unique task identifier
pub type TaskId = u64;

/// Task state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    /// Task is ready to run
    Ready,
    /// Task is currently running
    Running,
    /// Task is waiting for something (I/O, another task, etc.)
    Waiting,
    /// Task has completed successfully
    Completed,
    /// Task has failed with an error
    Failed,
    /// Task was cancelled
    Cancelled,
}

/// A concurrent task (similar to a future/promise)
pub struct Task {
    pub id: TaskId,
    pub state: TaskState,
    /// Function name or closure to execute
    pub func_name: String,
    /// Arguments to the function
    pub args: Vec<RuntimeValue>,
    /// Result of execution (when completed)
    pub result: Option<RuntimeValue>,
    /// Error message (when failed)
    pub error: Option<String>,
    /// Tasks waiting for this task to complete
    pub waiters: Vec<TaskId>,
    /// Creation timestamp
    pub created_at: Instant,
}

impl Task {
    pub fn new(id: TaskId, func_name: String, args: Vec<RuntimeValue>) -> Self {
        Task {
            id,
            state: TaskState::Ready,
            func_name,
            args,
            result: None,
            error: None,
            waiters: Vec::new(),
            created_at: Instant::now(),
        }
    }

    /// Check if task is complete
    pub fn is_complete(&self) -> bool {
        matches!(
            self.state,
            TaskState::Completed | TaskState::Failed | TaskState::Cancelled
        )
    }
}

// ============================================================================
// Async Runtime
// ============================================================================

/// Event loop for async/await execution
pub struct AsyncRuntime {
    /// All tasks
    tasks: RwLock<FastMap<TaskId, Task>>,
    /// Ready queue (tasks that can run)
    ready_queue: Mutex<VecDeque<TaskId>>,
    /// Next task ID
    next_id: AtomicU64,
    /// Whether the runtime is running
    running: AtomicBool,
    /// Condition variable for waking the event loop
    waker: Condvar,
    /// Worker threads
    workers: Vec<JoinHandle<()>>,
    /// Number of worker threads
    num_workers: usize,
}

impl AsyncRuntime {
    /// Create a new async runtime
    pub fn new() -> Self {
        AsyncRuntime {
            tasks: RwLock::new(FastMap::default()),
            ready_queue: Mutex::new(VecDeque::new()),
            next_id: AtomicU64::new(1),
            running: AtomicBool::new(false),
            waker: Condvar::new(),
            workers: Vec::new(),
            num_workers: num_cpus(),
        }
    }

    /// Create with specific number of workers
    pub fn with_workers(num_workers: usize) -> Self {
        let mut runtime = Self::new();
        runtime.num_workers = num_workers;
        runtime
    }

    /// Spawn a new async task
    pub fn spawn(&self, func_name: String, args: Vec<RuntimeValue>) -> TaskId {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let task = Task::new(id, func_name, args);

        // Add to tasks
        {
            let mut tasks = self.tasks.write().unwrap();
            tasks.insert(id, task);
        }

        // Add to ready queue
        {
            let mut queue = self.ready_queue.lock().unwrap();
            queue.push_back(id);
        }

        // Wake the event loop
        self.waker.notify_one();

        id
    }

    /// Wait for a task to complete (blocking)
    pub fn wait(&self, task_id: TaskId) -> Result<RuntimeValue, String> {
        loop {
            // Check if task is complete
            {
                let tasks = self.tasks.read().unwrap();
                if let Some(task) = tasks.get(&task_id) {
                    if task.is_complete() {
                        return match task.state {
                            TaskState::Completed => {
                                Ok(task.result.clone().unwrap_or(RuntimeValue::Null))
                            }
                            TaskState::Failed => Err(task
                                .error
                                .clone()
                                .unwrap_or_else(|| "Task failed".to_string())),
                            TaskState::Cancelled => Err("Task was cancelled".to_string()),
                            _ => unreachable!(),
                        };
                    }
                } else {
                    return Err(format!("Task {} not found", task_id));
                }
            }

            // Sleep briefly and retry
            thread::sleep(Duration::from_micros(100));
        }
    }

    /// Check if a task is complete
    pub fn is_complete(&self, task_id: TaskId) -> bool {
        let tasks = self.tasks.read().unwrap();
        tasks.get(&task_id).map(|t| t.is_complete()).unwrap_or(true)
    }

    /// Get task result (non-blocking)
    pub fn try_get_result(&self, task_id: TaskId) -> Option<Result<RuntimeValue, String>> {
        let tasks = self.tasks.read().unwrap();
        tasks.get(&task_id).and_then(|task| {
            if task.is_complete() {
                Some(match task.state {
                    TaskState::Completed => Ok(task.result.clone().unwrap_or(RuntimeValue::Null)),
                    TaskState::Failed => Err(task
                        .error
                        .clone()
                        .unwrap_or_else(|| "Task failed".to_string())),
                    TaskState::Cancelled => Err("Task was cancelled".to_string()),
                    _ => unreachable!(),
                })
            } else {
                None
            }
        })
    }

    /// Cancel a task
    pub fn cancel(&self, task_id: TaskId) -> bool {
        let mut tasks = self.tasks.write().unwrap();
        if let Some(task) = tasks.get_mut(&task_id) {
            if !task.is_complete() {
                task.state = TaskState::Cancelled;
                return true;
            }
        }
        false
    }

    /// Get number of pending tasks
    pub fn pending_count(&self) -> usize {
        let tasks = self.tasks.read().unwrap();
        tasks.values().filter(|t| !t.is_complete()).count()
    }
}

impl Default for AsyncRuntime {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Thread Pool
// ============================================================================

/// A work-stealing thread pool for parallel execution
pub struct ThreadPool {
    /// Worker threads
    workers: Vec<Worker>,
    /// Shared work queue
    queue: Arc<Mutex<VecDeque<Job>>>,
    /// Shutdown flag
    shutdown: Arc<AtomicBool>,
    /// Number of active tasks
    active_tasks: Arc<AtomicUsize>,
}

/// A unit of work for the thread pool
type Job = Box<dyn FnOnce() + Send + 'static>;

/// A worker thread in the pool
struct Worker {
    thread: Option<JoinHandle<()>>,
}

impl ThreadPool {
    /// Create a new thread pool with specified number of threads
    pub fn new(size: usize) -> Self {
        let queue: Arc<Mutex<VecDeque<Job>>> = Arc::new(Mutex::new(VecDeque::new()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let active_tasks = Arc::new(AtomicUsize::new(0));

        let mut workers = Vec::with_capacity(size);

        for _ in 0..size {
            let queue = Arc::clone(&queue);
            let shutdown = Arc::clone(&shutdown);
            let active_tasks = Arc::clone(&active_tasks);

            let thread = thread::spawn(move || {
                loop {
                    // Check for shutdown
                    if shutdown.load(Ordering::Relaxed) {
                        break;
                    }

                    // Try to get a job
                    let job = {
                        let mut queue = queue.lock().unwrap();
                        queue.pop_front()
                    };

                    if let Some(job) = job {
                        active_tasks.fetch_add(1, Ordering::Relaxed);
                        job();
                        active_tasks.fetch_sub(1, Ordering::Relaxed);
                    } else {
                        // No work, sleep briefly
                        thread::sleep(Duration::from_micros(100));
                    }
                }
            });

            workers.push(Worker {
                thread: Some(thread),
            });
        }

        ThreadPool {
            workers,
            queue,
            shutdown,
            active_tasks,
        }
    }

    /// Create thread pool with number of CPUs
    pub fn with_cpu_count() -> Self {
        Self::new(num_cpus())
    }

    /// Execute a job on the thread pool
    pub fn execute<F>(&self, job: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let mut queue = self.queue.lock().unwrap();
        queue.push_back(Box::new(job));
    }

    /// Execute a batch of jobs and wait for all to complete
    pub fn execute_batch<F>(&self, jobs: Vec<F>)
    where
        F: FnOnce() + Send + 'static,
    {
        let count = Arc::new(AtomicUsize::new(jobs.len()));
        let done = Arc::new((Mutex::new(false), Condvar::new()));

        for job in jobs {
            let count = Arc::clone(&count);
            let done = Arc::clone(&done);

            self.execute(move || {
                job();

                if count.fetch_sub(1, Ordering::Relaxed) == 1 {
                    // Last job done
                    let (lock, cvar) = &*done;
                    let mut done = lock.lock().unwrap();
                    *done = true;
                    cvar.notify_one();
                }
            });
        }

        // Wait for all jobs to complete
        let (lock, cvar) = &*done;
        let mut done = lock.lock().unwrap();
        while !*done {
            done = cvar.wait(done).unwrap();
        }
    }

    /// Get number of active tasks
    pub fn active_count(&self) -> usize {
        self.active_tasks.load(Ordering::Relaxed)
    }

    /// Get number of queued jobs
    pub fn queued_count(&self) -> usize {
        self.queue.lock().unwrap().len()
    }

    /// Shutdown the thread pool
    pub fn shutdown(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);

        for worker in &mut self.workers {
            if let Some(thread) = worker.thread.take() {
                let _ = thread.join();
            }
        }
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        self.shutdown();
    }
}

// ============================================================================
// Parallel Iterators
// ============================================================================

/// Parallel iterator configuration
#[derive(Debug, Clone)]
pub struct ParallelConfig {
    /// Minimum chunk size for parallel iteration
    pub min_chunk_size: usize,
    /// Maximum number of chunks
    pub max_chunks: usize,
    /// Number of threads to use
    pub num_threads: usize,
}

impl Default for ParallelConfig {
    fn default() -> Self {
        ParallelConfig {
            min_chunk_size: 100,
            max_chunks: 64,
            num_threads: num_cpus(),
        }
    }
}

/// Parallel map operation
pub fn parallel_map<T, U, F>(items: Vec<T>, f: F, config: &ParallelConfig) -> Vec<U>
where
    T: Send + Sync + 'static,
    U: Send + Default + Clone + 'static + std::fmt::Debug,
    F: Fn(&T) -> U + Send + Sync + 'static,
{
    let len = items.len();

    // For small inputs, run sequentially
    if len < config.min_chunk_size {
        return items.iter().map(&f).collect();
    }

    // Determine chunk size
    let num_chunks = (len / config.min_chunk_size).min(config.max_chunks).max(1);
    let chunk_size = len.div_ceil(num_chunks);

    // Prepare result vector
    let results = Arc::new(Mutex::new(vec![U::default(); len]));
    let items = Arc::new(items);
    let f = Arc::new(f);

    // Process chunks in parallel
    let handles: Vec<_> = (0..num_chunks)
        .map(|chunk_idx| {
            let items = Arc::clone(&items);
            let results = Arc::clone(&results);
            let f = Arc::clone(&f);

            thread::spawn(move || {
                let start = chunk_idx * chunk_size;
                let end = (start + chunk_size).min(items.len());

                for i in start..end {
                    let result = f(&items[i]);
                    let mut results = results.lock().unwrap();
                    results[i] = result;
                }
            })
        })
        .collect();

    // Wait for all threads
    for handle in handles {
        let _ = handle.join();
    }

    // Extract results
    Arc::try_unwrap(results).unwrap().into_inner().unwrap()
}

/// Parallel reduce operation
pub fn parallel_reduce<T, F, G>(
    items: Vec<T>,
    identity: T,
    map_fn: F,
    reduce_fn: G,
    config: &ParallelConfig,
) -> T
where
    T: Send + Clone + Sync + 'static,
    F: Fn(&T) -> T + Send + Sync + 'static,
    G: Fn(T, T) -> T + Send + Sync + 'static,
{
    let len = items.len();

    // For small inputs, run sequentially
    if len < config.min_chunk_size {
        return items.iter().map(&map_fn).fold(identity, &reduce_fn);
    }

    // Determine chunk size
    let num_chunks = (len / config.min_chunk_size).min(config.max_chunks).max(1);
    let chunk_size = len.div_ceil(num_chunks);

    let items = Arc::new(items);
    let map_fn = Arc::new(map_fn);
    let reduce_fn = Arc::new(reduce_fn);

    // Process chunks in parallel
    let handles: Vec<_> = (0..num_chunks)
        .map(|chunk_idx| {
            let items = Arc::clone(&items);
            let map_fn = Arc::clone(&map_fn);
            let reduce_fn = Arc::clone(&reduce_fn);
            let identity = identity.clone();

            thread::spawn(move || {
                let start = chunk_idx * chunk_size;
                let end = (start + chunk_size).min(items.len());

                items[start..end]
                    .iter()
                    .map(|x| map_fn(x))
                    .fold(identity, |a, b| reduce_fn(a, b))
            })
        })
        .collect();

    // Collect and reduce partial results
    let partial_results: Vec<T> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    partial_results
        .into_iter()
        .fold(identity, |a, b| reduce_fn(a, b))
}

/// Parallel for loop (execute closure for each index in range)
pub fn parallel_for<F>(start: usize, end: usize, f: F, _config: &ParallelConfig)
where
    F: Fn(usize) + Send + Sync + 'static,
{
    let len = end - start;
    let min_chunk_size = 100; // Use constant since config lifetime is problematic
    let max_chunks = 64;

    // For small ranges, run sequentially
    if len < min_chunk_size {
        for i in start..end {
            f(i);
        }
        return;
    }

    // Determine chunk size
    let num_chunks = (len / min_chunk_size).min(max_chunks).max(1);
    let chunk_size = len.div_ceil(num_chunks);

    let f = Arc::new(f);

    // Process chunks in parallel
    let handles: Vec<_> = (0..num_chunks)
        .map(|chunk_idx| {
            let f = Arc::clone(&f);

            thread::spawn(move || {
                let chunk_start = start + chunk_idx * chunk_size;
                let chunk_end = (chunk_start + chunk_size).min(end);

                for i in chunk_start..chunk_end {
                    f(i);
                }
            })
        })
        .collect();

    // Wait for all threads
    for handle in handles {
        let _ = handle.join();
    }
}

// ============================================================================
// Channels
// ============================================================================

/// A bounded channel for communication between tasks
pub struct Channel<T> {
    buffer: Mutex<VecDeque<T>>,
    capacity: usize,
    /// Condition variable for receivers
    not_empty: Condvar,
    /// Condition variable for senders
    not_full: Condvar,
    /// Whether the channel is closed
    closed: AtomicBool,
}

impl<T> Channel<T> {
    /// Create a new bounded channel
    pub fn new(capacity: usize) -> Arc<Self> {
        Arc::new(Channel {
            buffer: Mutex::new(VecDeque::with_capacity(capacity)),
            capacity,
            not_empty: Condvar::new(),
            not_full: Condvar::new(),
            closed: AtomicBool::new(false),
        })
    }

    /// Create an unbounded channel
    pub fn unbounded() -> Arc<Self> {
        Self::new(usize::MAX)
    }

    /// Send a value (blocking if full)
    pub fn send(&self, value: T) -> Result<(), String> {
        let mut buffer = self.buffer.lock().unwrap();

        // Wait while buffer is full and channel is open
        while buffer.len() >= self.capacity && !self.closed.load(Ordering::Relaxed) {
            buffer = self.not_full.wait(buffer).unwrap();
        }

        if self.closed.load(Ordering::Relaxed) {
            return Err("Channel is closed".to_string());
        }

        buffer.push_back(value);
        self.not_empty.notify_one();
        Ok(())
    }

    /// Try to send a value (non-blocking)
    pub fn try_send(&self, value: T) -> Result<(), T> {
        let mut buffer = self.buffer.lock().unwrap();

        if buffer.len() >= self.capacity {
            return Err(value);
        }

        buffer.push_back(value);
        self.not_empty.notify_one();
        Ok(())
    }

    /// Receive a value (blocking if empty)
    pub fn recv(&self) -> Result<T, String> {
        let mut buffer = self.buffer.lock().unwrap();

        // Wait while buffer is empty and channel is open
        while buffer.is_empty() && !self.closed.load(Ordering::Relaxed) {
            buffer = self.not_empty.wait(buffer).unwrap();
        }

        if let Some(value) = buffer.pop_front() {
            self.not_full.notify_one();
            Ok(value)
        } else {
            Err("Channel is closed and empty".to_string())
        }
    }

    /// Try to receive a value (non-blocking)
    pub fn try_recv(&self) -> Option<T> {
        let mut buffer = self.buffer.lock().unwrap();
        let value = buffer.pop_front();
        if value.is_some() {
            self.not_full.notify_one();
        }
        value
    }

    /// Close the channel
    pub fn close(&self) {
        self.closed.store(true, Ordering::Relaxed);
        self.not_empty.notify_all();
        self.not_full.notify_all();
    }

    /// Check if channel is closed
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    /// Get number of items in buffer
    pub fn len(&self) -> usize {
        self.buffer.lock().unwrap().len()
    }

    /// Check if channel is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ============================================================================
// Synchronization Primitives
// ============================================================================

/// A simple mutex wrapper for AdeshLang values
pub struct AdeshMutex {
    inner: Mutex<RuntimeValue>,
}

impl AdeshMutex {
    pub fn new(value: RuntimeValue) -> Arc<Self> {
        Arc::new(AdeshMutex {
            inner: Mutex::new(value),
        })
    }

    pub fn lock(&self) -> std::sync::MutexGuard<'_, RuntimeValue> {
        self.inner.lock().unwrap()
    }

    pub fn try_lock(&self) -> Option<std::sync::MutexGuard<'_, RuntimeValue>> {
        self.inner.try_lock().ok()
    }
}

/// A read-write lock for AdeshLang values
pub struct AdeshRwLock {
    inner: RwLock<RuntimeValue>,
}

impl AdeshRwLock {
    pub fn new(value: RuntimeValue) -> Arc<Self> {
        Arc::new(AdeshRwLock {
            inner: RwLock::new(value),
        })
    }

    pub fn read(&self) -> std::sync::RwLockReadGuard<'_, RuntimeValue> {
        self.inner.read().unwrap()
    }

    pub fn write(&self) -> std::sync::RwLockWriteGuard<'_, RuntimeValue> {
        self.inner.write().unwrap()
    }
}

/// Atomic counter for AdeshLang
pub struct AtomicCounter {
    value: AtomicU64,
}

impl AtomicCounter {
    pub fn new(initial: u64) -> Arc<Self> {
        Arc::new(AtomicCounter {
            value: AtomicU64::new(initial),
        })
    }

    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }

    pub fn set(&self, value: u64) {
        self.value.store(value, Ordering::Relaxed);
    }

    pub fn increment(&self) -> u64 {
        self.value.fetch_add(1, Ordering::Relaxed)
    }

    pub fn decrement(&self) -> u64 {
        self.value.fetch_sub(1, Ordering::Relaxed)
    }

    pub fn add(&self, delta: u64) -> u64 {
        self.value.fetch_add(delta, Ordering::Relaxed)
    }

    pub fn compare_exchange(&self, expected: u64, new: u64) -> Result<u64, u64> {
        self.value
            .compare_exchange(expected, new, Ordering::Relaxed, Ordering::Relaxed)
    }
}

/// A barrier for synchronizing multiple threads
pub struct Barrier {
    count: AtomicUsize,
    total: usize,
    generation: AtomicUsize,
    cvar: Condvar,
    mutex: Mutex<()>,
}

impl Barrier {
    pub fn new(count: usize) -> Arc<Self> {
        Arc::new(Barrier {
            count: AtomicUsize::new(0),
            total: count,
            generation: AtomicUsize::new(0),
            cvar: Condvar::new(),
            mutex: Mutex::new(()),
        })
    }

    /// Wait at the barrier
    pub fn wait(&self) {
        let current_gen = self.generation.load(Ordering::Relaxed);

        if self.count.fetch_add(1, Ordering::Relaxed) + 1 == self.total {
            // Last one to arrive
            self.count.store(0, Ordering::Relaxed);
            self.generation.fetch_add(1, Ordering::Relaxed);
            self.cvar.notify_all();
        } else {
            // Wait for others
            let mut guard = self.mutex.lock().unwrap();
            while self.generation.load(Ordering::Relaxed) == current_gen {
                guard = self.cvar.wait(guard).unwrap();
            }
        }
    }
}

// ============================================================================
// ML/AI Data Structures
// ============================================================================

/// N-dimensional tensor for ML workloads
#[derive(Debug, Clone)]
pub struct Tensor {
    /// Shape of the tensor
    pub shape: Vec<usize>,
    /// Flat data storage
    pub data: Vec<f64>,
    /// Total number of elements
    pub size: usize,
}

impl Tensor {
    /// Create a new tensor with given shape
    pub fn zeros(shape: Vec<usize>) -> Self {
        let size: usize = shape.iter().product();
        Tensor {
            shape,
            data: vec![0.0; size],
            size,
        }
    }

    /// Create a tensor filled with a value
    pub fn fill(shape: Vec<usize>, value: f64) -> Self {
        let size: usize = shape.iter().product();
        Tensor {
            shape,
            data: vec![value; size],
            size,
        }
    }

    /// Create from existing data
    pub fn from_data(shape: Vec<usize>, data: Vec<f64>) -> Result<Self, String> {
        let size: usize = shape.iter().product();
        if data.len() != size {
            return Err(format!(
                "Data length {} doesn't match shape {:?} (expected {})",
                data.len(),
                shape,
                size
            ));
        }
        Ok(Tensor { shape, data, size })
    }

    /// Get element at index
    pub fn get(&self, indices: &[usize]) -> Option<f64> {
        let flat_idx = self.flat_index(indices)?;
        self.data.get(flat_idx).copied()
    }

    /// Set element at index
    pub fn set(&mut self, indices: &[usize], value: f64) -> Result<(), String> {
        let flat_idx = self
            .flat_index(indices)
            .ok_or_else(|| "Index out of bounds".to_string())?;
        self.data[flat_idx] = value;
        Ok(())
    }

    /// Convert multi-dimensional index to flat index
    fn flat_index(&self, indices: &[usize]) -> Option<usize> {
        if indices.len() != self.shape.len() {
            return None;
        }

        let mut flat = 0;
        let mut stride = 1;

        for (i, &idx) in indices.iter().enumerate().rev() {
            if idx >= self.shape[i] {
                return None;
            }
            flat += idx * stride;
            stride *= self.shape[i];
        }

        Some(flat)
    }

    /// Element-wise addition
    pub fn add(&self, other: &Tensor) -> Result<Tensor, String> {
        if self.shape != other.shape {
            return Err("Shape mismatch for addition".to_string());
        }

        let data: Vec<f64> = self
            .data
            .iter()
            .zip(&other.data)
            .map(|(a, b)| a + b)
            .collect();

        Ok(Tensor {
            shape: self.shape.clone(),
            data,
            size: self.size,
        })
    }

    /// Element-wise multiplication
    pub fn mul(&self, other: &Tensor) -> Result<Tensor, String> {
        if self.shape != other.shape {
            return Err("Shape mismatch for multiplication".to_string());
        }

        let data: Vec<f64> = self
            .data
            .iter()
            .zip(&other.data)
            .map(|(a, b)| a * b)
            .collect();

        Ok(Tensor {
            shape: self.shape.clone(),
            data,
            size: self.size,
        })
    }

    /// Scalar multiplication
    pub fn scale(&self, scalar: f64) -> Tensor {
        let data: Vec<f64> = self.data.iter().map(|x| x * scalar).collect();

        Tensor {
            shape: self.shape.clone(),
            data,
            size: self.size,
        }
    }

    /// Sum all elements
    pub fn sum(&self) -> f64 {
        self.data.iter().sum()
    }

    /// Mean of all elements
    pub fn mean(&self) -> f64 {
        self.sum() / self.size as f64
    }

    /// Matrix multiplication (for 2D tensors)
    pub fn matmul(&self, other: &Tensor) -> Result<Tensor, String> {
        if self.shape.len() != 2 || other.shape.len() != 2 {
            return Err("matmul requires 2D tensors".to_string());
        }

        let (m, k1) = (self.shape[0], self.shape[1]);
        let (k2, n) = (other.shape[0], other.shape[1]);

        if k1 != k2 {
            return Err(format!("Shape mismatch: ({}, {}) x ({}, {})", m, k1, k2, n));
        }

        let mut result = Tensor::zeros(vec![m, n]);

        for i in 0..m {
            for j in 0..n {
                let mut sum = 0.0;
                for k in 0..k1 {
                    sum += self.data[i * k1 + k] * other.data[k * n + j];
                }
                result.data[i * n + j] = sum;
            }
        }

        Ok(result)
    }

    /// Parallel matrix multiplication
    pub fn matmul_parallel(
        &self,
        other: &Tensor,
        _config: &ParallelConfig,
    ) -> Result<Tensor, String> {
        if self.shape.len() != 2 || other.shape.len() != 2 {
            return Err("matmul requires 2D tensors".to_string());
        }

        let (m, k1) = (self.shape[0], self.shape[1]);
        let (k2, n) = (other.shape[0], other.shape[1]);

        if k1 != k2 {
            return Err(format!("Shape mismatch: ({}, {}) x ({}, {})", m, k1, k2, n));
        }

        // For simplicity, use a sequential implementation wrapped in threads
        // A full parallel implementation would use scoped threads or rayon
        let result_data = Arc::new(Mutex::new(vec![0.0; m * n]));
        let self_data = Arc::new(self.data.clone());
        let other_data = Arc::new(other.data.clone());

        // Determine chunk size
        let num_threads = num_cpus().min(m).max(1);
        let chunk_size = m.div_ceil(num_threads);

        let handles: Vec<_> = (0..num_threads)
            .map(|thread_idx| {
                let result_data = Arc::clone(&result_data);
                let self_data = Arc::clone(&self_data);
                let other_data = Arc::clone(&other_data);

                thread::spawn(move || {
                    let start_row = thread_idx * chunk_size;
                    let end_row = (start_row + chunk_size).min(m);

                    for i in start_row..end_row {
                        for j in 0..n {
                            let mut sum = 0.0;
                            for k in 0..k1 {
                                sum += self_data[i * k1 + k] * other_data[k * n + j];
                            }
                            let mut result = result_data.lock().unwrap();
                            result[i * n + j] = sum;
                        }
                    }
                })
            })
            .collect();

        // Wait for all threads
        for handle in handles {
            let _ = handle.join();
        }

        Ok(Tensor {
            shape: vec![m, n],
            data: Arc::try_unwrap(result_data).unwrap().into_inner().unwrap(),
            size: m * n,
        })
    }
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Get number of CPU cores
fn num_cpus() -> usize {
    thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thread_pool() {
        let pool = ThreadPool::new(4);
        let counter = Arc::new(AtomicUsize::new(0));

        let mut jobs = Vec::new();
        for _ in 0..100 {
            let counter = Arc::clone(&counter);
            jobs.push(move || {
                counter.fetch_add(1, Ordering::Relaxed);
            });
        }

        pool.execute_batch(jobs);

        assert_eq!(counter.load(Ordering::Relaxed), 100);
    }

    #[test]
    fn test_parallel_map() {
        let items: Vec<i32> = (0..1000).collect();
        let config = ParallelConfig::default();

        let results: Vec<i32> = parallel_map(items, |x| x * 2, &config);

        for (i, r) in results.iter().enumerate() {
            assert_eq!(*r, i as i32 * 2);
        }
    }

    #[test]
    fn test_channel() {
        let chan = Channel::new(10);

        // Send some values
        chan.send(1).unwrap();
        chan.send(2).unwrap();
        chan.send(3).unwrap();

        // Receive them
        assert_eq!(chan.recv().unwrap(), 1);
        assert_eq!(chan.recv().unwrap(), 2);
        assert_eq!(chan.recv().unwrap(), 3);
    }

    #[test]
    fn test_tensor_ops() {
        let t1 = Tensor::fill(vec![2, 3], 1.0);
        let t2 = Tensor::fill(vec![2, 3], 2.0);

        let sum = t1.add(&t2).unwrap();
        assert_eq!(sum.sum(), 18.0);

        let scaled = t1.scale(5.0);
        assert_eq!(scaled.sum(), 30.0);
    }

    #[test]
    fn test_matmul() {
        // 2x3 matrix
        let a = Tensor::from_data(vec![2, 3], vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]).unwrap();
        // 3x2 matrix
        let b = Tensor::from_data(vec![3, 2], vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]).unwrap();

        let c = a.matmul(&b).unwrap();

        assert_eq!(c.shape, vec![2, 2]);
        assert_eq!(c.get(&[0, 0]), Some(22.0)); // 1*1 + 2*3 + 3*5
        assert_eq!(c.get(&[0, 1]), Some(28.0)); // 1*2 + 2*4 + 3*6
    }

    #[test]
    fn test_async_runtime() {
        let runtime = AsyncRuntime::new();

        let task_id = runtime.spawn("test".to_string(), vec![RuntimeValue::Int(42)]);

        // Task should be created
        assert!(!runtime.is_complete(task_id));
    }
}
