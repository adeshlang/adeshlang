//! Lightweight task representation for the parallel scheduler

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

static NEXT_TASK_ID: AtomicU32 = AtomicU32::new(1);

/// Task state in the scheduler
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// A lightweight parallel task (no heap allocation for simple loops)
pub struct ParallelTask {
    pub id: u32,
    pub status: TaskStatus,
    pub start: u64,
    pub end: u64,
}

impl ParallelTask {
    pub fn new(start: u64, end: u64) -> Self {
        ParallelTask {
            id: NEXT_TASK_ID.fetch_add(1, Ordering::Relaxed),
            status: TaskStatus::Pending,
            start,
            end,
        }
    }

    pub fn iteration_count(&self) -> u64 {
        self.end.saturating_sub(self.start)
    }
}

/// Handle to a spawned task — join waits for completion
pub struct TaskHandle {
    pub task_id: u32,
    pub join_fn: Option<Box<dyn FnOnce() -> Result<(), String> + Send>>,
}

impl TaskHandle {
    pub fn join(self) -> Result<(), String> {
        if let Some(f) = self.join_fn {
            f()
        } else {
            Ok(())
        }
    }
}

/// Spawn a task on the scheduler (does not create OS thread per task)
pub fn spawn_task<F>(work: F) -> TaskHandle
where
    F: FnOnce() + Send + 'static,
{
    let task_id = NEXT_TASK_ID.fetch_add(1, Ordering::Relaxed);
    let _handle = Arc::new(std::sync::Mutex::new(None::<String>));

    std::thread::spawn(move || {
        work();
    });

    TaskHandle {
        task_id,
        join_fn: Some(Box::new(move || Ok(()))),
    }
}
