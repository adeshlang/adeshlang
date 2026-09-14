//! VecDeque - Contiguous Circular Buffer
//!
//! A double-ended queue implemented with a growable ring buffer.

use crate::stdlib::adesh_alloc::allocator::{Allocator, global_allocator};
use crate::stdlib::adesh_core::layout::Layout;
use std::ptr::NonNull;

/// A double-ended queue implemented with a growable ring buffer
pub struct VecDeque<T> {
    ptr: NonNull<T>,
    cap: usize,
    head: usize,
    len: usize,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> VecDeque<T> {
    /// Creates an empty VecDeque
    pub fn new() -> Self {
        VecDeque {
            ptr: NonNull::dangling(),
            cap: 0,
            head: 0,
            len: 0,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Creates an empty VecDeque with space for at least `capacity` elements
    pub fn with_capacity(capacity: usize) -> Self {
        if capacity == 0 {
            return Self::new();
        }
        let cap = capacity.next_power_of_two();
        let layout =
            Layout::from_size_align(cap * std::mem::size_of::<T>(), std::mem::align_of::<T>())
                .expect("Invalid layout");

        let ptr = unsafe {
            let p = global_allocator().alloc(layout).expect("Allocation failed");
            NonNull::new_unchecked(p as *mut T)
        };

        VecDeque {
            ptr,
            cap,
            head: 0,
            len: 0,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Returns the number of elements in the VecDeque
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns true if the VecDeque is empty
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the capacity
    pub fn capacity(&self) -> usize {
        self.cap
    }

    /// Clears the queue
    pub fn clear(&mut self) {
        while self.pop_back().is_some() {}
        self.head = 0;
    }

    /// Gets a reference to the element at the index
    pub fn get(&self, index: usize) -> Option<&T> {
        if index < self.len {
            let real_idx = (self.head + index) & (self.cap - 1);
            unsafe { Some(&*self.ptr.as_ptr().add(real_idx)) }
        } else {
            None
        }
    }

    /// Gets a mutable reference to the element at the index
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        if index < self.len {
            let real_idx = (self.head + index) & (self.cap - 1);
            unsafe { Some(&mut *self.ptr.as_ptr().add(real_idx)) }
        } else {
            None
        }
    }

    /// Prepends an element to the front
    pub fn push_front(&mut self, value: T) {
        if self.len == self.cap {
            self.grow();
        }
        self.head = if self.head == 0 {
            self.cap - 1
        } else {
            self.head - 1
        };
        unsafe {
            std::ptr::write(self.ptr.as_ptr().add(self.head), value);
        }
        self.len += 1;
    }

    /// Appends an element to the back
    pub fn push_back(&mut self, value: T) {
        if self.len == self.cap {
            self.grow();
        }
        let back_idx = (self.head + self.len) & (self.cap - 1);
        unsafe {
            std::ptr::write(self.ptr.as_ptr().add(back_idx), value);
        }
        self.len += 1;
    }

    /// Removes and returns the first element
    pub fn pop_front(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        unsafe {
            let val = std::ptr::read(self.ptr.as_ptr().add(self.head));
            self.head = (self.head + 1) & (self.cap - 1);
            self.len -= 1;
            Some(val)
        }
    }

    /// Removes and returns the last element
    pub fn pop_back(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        let back_idx = (self.head + self.len - 1) & (self.cap - 1);
        unsafe {
            let val = std::ptr::read(self.ptr.as_ptr().add(back_idx));
            self.len -= 1;
            Some(val)
        }
    }

    /// Returns a reference to the front element
    pub fn front(&self) -> Option<&T> {
        self.get(0)
    }

    /// Returns a reference to the back element
    pub fn back(&self) -> Option<&T> {
        if self.len > 0 {
            self.get(self.len - 1)
        } else {
            None
        }
    }

    fn grow(&mut self) {
        let new_cap = if self.cap == 0 { 4 } else { self.cap * 2 };
        let mut new_buf = VecDeque::with_capacity(new_cap);
        while let Some(item) = self.pop_front() {
            new_buf.push_back(item);
        }
        let old_layout = Layout::from_size_align(
            self.cap * std::mem::size_of::<T>(),
            std::mem::align_of::<T>(),
        )
        .expect("Invalid layout");
        if self.cap != 0 {
            unsafe {
                global_allocator().dealloc(self.ptr.as_ptr() as *mut u8, old_layout);
            }
        }
        self.ptr = new_buf.ptr;
        self.cap = new_buf.cap;
        self.head = new_buf.head;
        self.len = new_buf.len;
        std::mem::forget(new_buf);
    }
}

impl<T> Drop for VecDeque<T> {
    fn drop(&mut self) {
        self.clear();
        if self.cap != 0 {
            let layout = Layout::from_size_align(
                self.cap * std::mem::size_of::<T>(),
                std::mem::align_of::<T>(),
            )
            .expect("Invalid layout");
            unsafe {
                global_allocator().dealloc(self.ptr.as_ptr() as *mut u8, layout);
            }
        }
    }
}

impl<T> Default for VecDeque<T> {
    fn default() -> Self {
        Self::new()
    }
}
