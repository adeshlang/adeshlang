//! Queue - First-In First-Out (FIFO) Data Structure
//!
//! Implemented over VecDeque for optimal O(1) push/pop operations.

use crate::stdlib::adesh_alloc::vecdeque::VecDeque;

pub struct Queue<T> {
    inner: VecDeque<T>,
}

impl<T> Queue<T> {
    pub fn new() -> Self {
        Queue {
            inner: VecDeque::new(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Queue {
            inner: VecDeque::with_capacity(capacity),
        }
    }

    pub fn enqueue(&mut self, value: T) {
        self.inner.push_back(value);
    }

    pub fn dequeue(&mut self) -> Option<T> {
        self.inner.pop_front()
    }

    pub fn front(&self) -> Option<&T> {
        self.inner.front()
    }

    pub fn back(&self) -> Option<&T> {
        self.inner.back()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn clear(&mut self) {
        self.inner.clear();
    }

    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }
}

impl<T> Default for Queue<T> {
    fn default() -> Self {
        Self::new()
    }
}
