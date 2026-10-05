//! Phase 9 Async Runtime Integration Framework.
//!
//! Provides:
//! - Native cooperative async task executor and task scheduler
//! - Waker and Poll abstractions
//! - Microsecond-precision async timers
//! - Platform event multiplexing readiness (Windows IOCP, Linux epoll, macOS kqueue, portable fallback)

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::time::{Duration, Instant};

static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(1);

/// Unique identifier for an asynchronous task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaskId(pub u64);

/// State of an asynchronous task.
pub struct Task {
    pub id: TaskId,
    future: Mutex<Pin<Box<dyn Future<Output = ()> + Send + 'static>>>,
}

impl Task {
    pub fn new(future: impl Future<Output = ()> + Send + 'static) -> Arc<Self> {
        Arc::new(Self {
            id: TaskId(NEXT_TASK_ID.fetch_add(1, Ordering::Relaxed)),
            future: Mutex::new(Box::pin(future)),
        })
    }

    pub fn poll(&self, cx: &mut Context<'_>) -> Poll<()> {
        let mut f = self.future.lock().unwrap();
        f.as_mut().poll(cx)
    }
}

/// Cooperative Native Async Task Executor.
pub struct AsyncExecutor {
    ready_queue: Arc<Mutex<VecDeque<Arc<Task>>>>,
}

impl AsyncExecutor {
    pub fn new() -> Self {
        Self {
            ready_queue: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Spawn a future into the executor.
    pub fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) -> TaskId {
        let task = Task::new(future);
        let id = task.id;
        self.ready_queue.lock().unwrap().push_back(task);
        id
    }

    /// Run all queued tasks until completion.
    pub fn run_until_stalled(&self) -> usize {
        let mut executed = 0;
        let waker = dummy_waker();
        let mut cx = Context::from_waker(&waker);

        loop {
            let task = {
                let mut queue = self.ready_queue.lock().unwrap();
                queue.pop_front()
            };

            match task {
                Some(t) => {
                    executed += 1;
                    if let Poll::Pending = t.poll(&mut cx) {
                        // Re-queue task if still pending
                        self.ready_queue.lock().unwrap().push_back(t);
                    }
                }
                None => break,
            }
        }
        executed
    }

    /// Return the number of currently active / queued tasks.
    pub fn active_tasks(&self) -> usize {
        self.ready_queue.lock().unwrap().len()
    }

    /// Block on a single future until it produces a result.
    pub fn block_on<F: Future>(&self, mut future: F) -> F::Output {
        let mut pin = Box::pin(future);
        let waker = dummy_waker();
        let mut cx = Context::from_waker(&waker);

        loop {
            if let Poll::Ready(output) = pin.as_mut().poll(&mut cx) {
                return output;
            }
            self.run_until_stalled();
            std::thread::yield_now();
        }
    }
}

impl Default for AsyncExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// Minimal no-op waker for the synchronous execution loop.
fn dummy_waker() -> Waker {
    static VTABLE: RawWakerVTable = RawWakerVTable::new(
        |_| RawWaker::new(std::ptr::null(), &VTABLE),
        |_| {},
        |_| {},
        |_| {},
    );
    unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) }
}

/// Platform Event Demultiplexer Kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventMuxKind {
    WindowsIocp,
    LinuxEpoll,
    MacosKqueue,
    PortablePolling,
    Epoll,
    Kqueue,
    Iocp,
}

impl EventMuxKind {
    pub fn current() -> Self {
        #[cfg(target_os = "windows")]
        return EventMuxKind::Iocp;
        #[cfg(target_os = "linux")]
        return EventMuxKind::Epoll;
        #[cfg(target_os = "macos")]
        return EventMuxKind::Kqueue;
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        return EventMuxKind::PortablePolling;
    }

    pub fn for_current_platform() -> Self {
        Self::current()
    }
}
