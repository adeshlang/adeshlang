//! RingBuffer - Fixed capacity streaming circular buffer

use crate::stdlib::adesh_alloc::vec::Vec;

/// A circular buffer with fixed capacity
pub struct RingBuffer<T> {
    data: Vec<Option<T>>,
    head: usize,
    tail: usize,
    len: usize,
    cap: usize,
}

impl<T> RingBuffer<T> {
    /// Creates a RingBuffer with the specified capacity
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "Capacity must be greater than 0");
        let mut data = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            data.push(None);
        }
        RingBuffer {
            data,
            head: 0,
            tail: 0,
            len: 0,
            cap: capacity,
        }
    }

    /// Pushes an element into the buffer, overwriting the oldest element if full
    pub fn push(&mut self, value: T) -> Option<T> {
        let mut overwritten = None;
        if self.len == self.cap {
            // Overwrite head
            overwritten = std::mem::replace(self.data.get_mut(self.head).unwrap(), Some(value));
            self.head = (self.head + 1) % self.cap;
            self.tail = (self.tail + 1) % self.cap;
        } else {
            *self.data.get_mut(self.tail).unwrap() = Some(value);
            self.tail = (self.tail + 1) % self.cap;
            self.len += 1;
        }
        overwritten
    }

    /// Pops the oldest element from the buffer
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        let val = std::mem::replace(self.data.get_mut(self.head).unwrap(), None);
        self.head = (self.head + 1) % self.cap;
        self.len -= 1;
        val
    }

    /// Peeks at the oldest element
    pub fn peek(&self) -> Option<&T> {
        if self.len == 0 {
            None
        } else {
            self.data.get(self.head).unwrap().as_ref()
        }
    }

    /// Returns the length
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns the capacity
    pub fn capacity(&self) -> usize {
        self.cap
    }

    /// Returns true if empty
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns true if full
    pub fn is_full(&self) -> bool {
        self.len == self.cap
    }
}
