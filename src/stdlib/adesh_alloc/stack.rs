//! Stack - Last-In First-Out (LIFO) Data Structure
//!
//! Implemented over Vec for optimal O(1) push/pop operations.

use crate::stdlib::adesh_alloc::vec::Vec;

pub struct Stack<T> {
    inner: Vec<T>,
}

impl<T> Stack<T> {
    pub fn new() -> Self {
        Stack { inner: Vec::new() }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Stack {
            inner: Vec::with_capacity(capacity),
        }
    }

    pub fn push(&mut self, value: T) {
        self.inner.push(value);
    }

    pub fn pop(&mut self) -> Option<T> {
        self.inner.pop()
    }

    pub fn peek(&self) -> Option<&T> {
        self.inner.last()
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

impl<T> Default for Stack<T> {
    fn default() -> Self {
        Self::new()
    }
}
