//! Zero-GC channels compatible with ownership (move semantics)

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};

/// Channel error types
#[derive(Debug, Clone)]
pub enum ChannelError {
    Closed,
    Empty,
    Full,
    Disconnected,
}

struct BoundedState<T> {
    queue: VecDeque<T>,
    closed: bool,
}

/// Bounded channel — backpressure by design
pub struct BoundedChannel<T> {
    state: Mutex<BoundedState<T>>,
    capacity: usize,
    not_empty: Condvar,
    not_full: Condvar,
}

impl<T> BoundedChannel<T> {
    pub fn new(capacity: usize) -> (Sender<T>, Receiver<T>) {
        let effective_cap = capacity.max(1);
        let inner = Arc::new(BoundedChannel {
            state: Mutex::new(BoundedState {
                queue: VecDeque::with_capacity(effective_cap.min(1024)),
                closed: false,
            }),
            capacity: effective_cap,
            not_empty: Condvar::new(),
            not_full: Condvar::new(),
        });
        (
            Sender {
                inner: Arc::clone(&inner),
            },
            Receiver { inner },
        )
    }

    fn send_inner(&self, value: T) -> Result<(), ChannelError> {
        let mut state = self.state.lock().unwrap();
        if state.closed {
            return Err(ChannelError::Closed);
        }
        while state.queue.len() >= self.capacity {
            state = self.not_full.wait(state).unwrap();
            if state.closed {
                return Err(ChannelError::Closed);
            }
        }
        state.queue.push_back(value);
        self.not_empty.notify_one();
        Ok(())
    }

    fn recv_inner(&self) -> Result<T, ChannelError> {
        let mut state = self.state.lock().unwrap();
        while state.queue.is_empty() {
            if state.closed {
                return Err(ChannelError::Closed);
            }
            state = self.not_empty.wait(state).unwrap();
        }
        let val = state.queue.pop_front().unwrap();
        self.not_full.notify_one();
        Ok(val)
    }

    fn try_recv_inner(&self) -> Result<T, ChannelError> {
        let mut state = self.state.lock().unwrap();
        state.queue.pop_front().ok_or_else(|| {
            if state.closed {
                ChannelError::Closed
            } else {
                ChannelError::Empty
            }
        })
    }

    fn close_inner(&self) {
        let mut state = self.state.lock().unwrap();
        state.closed = true;
        self.not_empty.notify_all();
        self.not_full.notify_all();
    }
}

/// Unbounded channel
pub struct UnboundedChannel<T> {
    state: Mutex<BoundedState<T>>,
    not_empty: Condvar,
}

impl<T> UnboundedChannel<T> {
    pub fn new() -> (UnboundedSender<T>, UnboundedReceiver<T>) {
        let inner = Arc::new(UnboundedChannel {
            state: Mutex::new(BoundedState {
                queue: VecDeque::new(),
                closed: false,
            }),
            not_empty: Condvar::new(),
        });
        (
            UnboundedSender {
                inner: Arc::clone(&inner),
            },
            UnboundedReceiver { inner },
        )
    }
}

pub struct Sender<T> {
    inner: Arc<BoundedChannel<T>>,
}

impl<T> Sender<T> {
    pub fn send(&self, value: T) -> Result<(), ChannelError> {
        self.inner.send_inner(value)
    }

    pub fn close(&self) {
        self.inner.close_inner();
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Sender {
            inner: Arc::clone(&self.inner),
        }
    }
}

pub struct Receiver<T> {
    inner: Arc<BoundedChannel<T>>,
}

impl<T> Receiver<T> {
    pub fn recv(&self) -> Result<T, ChannelError> {
        self.inner.recv_inner()
    }

    pub fn try_recv(&self) -> Result<T, ChannelError> {
        self.inner.try_recv_inner()
    }
}

pub struct UnboundedSender<T> {
    inner: Arc<UnboundedChannel<T>>,
}

impl<T> UnboundedSender<T> {
    pub fn send(&self, value: T) -> Result<(), ChannelError> {
        let mut state = self.inner.state.lock().unwrap();
        if state.closed {
            return Err(ChannelError::Closed);
        }
        state.queue.push_back(value);
        self.inner.not_empty.notify_one();
        Ok(())
    }

    pub fn close(&self) {
        let mut state = self.inner.state.lock().unwrap();
        state.closed = true;
        self.inner.not_empty.notify_all();
    }
}

pub struct UnboundedReceiver<T> {
    inner: Arc<UnboundedChannel<T>>,
}

impl<T> UnboundedReceiver<T> {
    pub fn recv(&self) -> Result<T, ChannelError> {
        let mut state = self.inner.state.lock().unwrap();
        while state.queue.is_empty() {
            if state.closed {
                return Err(ChannelError::Closed);
            }
            state = self.inner.not_empty.wait(state).unwrap();
        }
        Ok(state.queue.pop_front().unwrap())
    }
}

/// Create a bounded channel pair
pub fn channel<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    BoundedChannel::new(capacity)
}

/// Create an unbounded channel pair
pub fn unbounded_channel<T>() -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    UnboundedChannel::new()
}
