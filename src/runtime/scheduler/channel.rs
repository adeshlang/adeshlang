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

/// Bounded channel — backpressure by design
pub struct BoundedChannel<T> {
    queue: Mutex<VecDeque<T>>,
    capacity: usize,
    closed: Mutex<bool>,
    not_empty: Condvar,
    not_full: Condvar,
}

impl<T> BoundedChannel<T> {
    pub fn new(capacity: usize) -> (Sender<T>, Receiver<T>) {
        let inner = Arc::new(BoundedChannel {
            queue: Mutex::new(VecDeque::with_capacity(capacity.min(1024))),
            capacity,
            closed: Mutex::new(false),
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
        let mut queue = self.queue.lock().unwrap();
        if *self.closed.lock().unwrap() {
            return Err(ChannelError::Closed);
        }
        while queue.len() >= self.capacity {
            queue = self.not_full.wait(queue).unwrap();
            if *self.closed.lock().unwrap() {
                return Err(ChannelError::Closed);
            }
        }
        queue.push_back(value);
        self.not_empty.notify_one();
        Ok(())
    }

    fn recv_inner(&self) -> Result<T, ChannelError> {
        let mut queue = self.queue.lock().unwrap();
        while queue.is_empty() {
            if *self.closed.lock().unwrap() {
                return Err(ChannelError::Closed);
            }
            queue = self.not_empty.wait(queue).unwrap();
        }
        let val = queue.pop_front().unwrap();
        self.not_full.notify_one();
        Ok(val)
    }

    fn try_recv_inner(&self) -> Result<T, ChannelError> {
        let mut queue = self.queue.lock().unwrap();
        queue.pop_front().ok_or(ChannelError::Empty)
    }

    fn close_inner(&self) {
        *self.closed.lock().unwrap() = true;
        self.not_empty.notify_all();
        self.not_full.notify_all();
    }
}

/// Unbounded channel
pub struct UnboundedChannel<T> {
    queue: Mutex<VecDeque<T>>,
    closed: Mutex<bool>,
    not_empty: Condvar,
}

impl<T> UnboundedChannel<T> {
    pub fn new() -> (UnboundedSender<T>, UnboundedReceiver<T>) {
        let inner = Arc::new(UnboundedChannel {
            queue: Mutex::new(VecDeque::new()),
            closed: Mutex::new(false),
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
        let mut queue = self.inner.queue.lock().unwrap();
        if *self.inner.closed.lock().unwrap() {
            return Err(ChannelError::Closed);
        }
        queue.push_back(value);
        self.inner.not_empty.notify_one();
        Ok(())
    }
}

pub struct UnboundedReceiver<T> {
    inner: Arc<UnboundedChannel<T>>,
}

impl<T> UnboundedReceiver<T> {
    pub fn recv(&self) -> Result<T, ChannelError> {
        let mut queue = self.inner.queue.lock().unwrap();
        while queue.is_empty() {
            if *self.inner.closed.lock().unwrap() {
                return Err(ChannelError::Closed);
            }
            queue = self.inner.not_empty.wait(queue).unwrap();
        }
        Ok(queue.pop_front().unwrap())
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
