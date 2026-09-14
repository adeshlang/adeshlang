//! Vec - Growable Array
//!
//! A contiguous growable array type with heap-allocated contents.

use crate::stdlib::adesh_alloc::allocator::{Allocator, global_allocator};
use crate::stdlib::adesh_core::layout::Layout;
use std::ptr::NonNull;

/// A growable, heap-allocated array
pub struct Vec<T> {
    ptr: NonNull<T>,
    len: usize,
    cap: usize,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> Vec<T> {
    /// Creates a new empty Vec
    pub fn new() -> Self {
        Vec {
            ptr: NonNull::dangling(),
            len: 0,
            cap: 0,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Creates a new Vec with the specified capacity
    pub fn with_capacity(capacity: usize) -> Self {
        if capacity == 0 {
            return Self::new();
        }

        let layout = Layout::from_size_align(
            capacity * std::mem::size_of::<T>(),
            std::mem::align_of::<T>(),
        )
        .expect("Invalid layout");

        let ptr = unsafe {
            let ptr = global_allocator().alloc(layout).expect("Allocation failed");
            NonNull::new_unchecked(ptr as *mut T)
        };

        Vec {
            ptr,
            len: 0,
            cap: capacity,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Returns the number of elements in the Vec
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns true if the Vec is empty
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the capacity of the Vec
    pub fn capacity(&self) -> usize {
        self.cap
    }

    /// Appends an element to the back of the Vec
    pub fn push(&mut self, value: T) {
        if self.len == self.cap {
            self.grow();
        }

        unsafe {
            std::ptr::write(self.ptr.as_ptr().add(self.len), value);
        }
        self.len += 1;
    }

    /// Removes and returns the last element, or None if empty
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }

        self.len -= 1;
        unsafe { Some(std::ptr::read(self.ptr.as_ptr().add(self.len))) }
    }

    /// Gets a reference to an element at the index
    pub fn get(&self, index: usize) -> Option<&T> {
        if index < self.len {
            unsafe { Some(&*self.ptr.as_ptr().add(index)) }
        } else {
            None
        }
    }

    /// Gets a mutable reference to an element at the index
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        if index < self.len {
            unsafe { Some(&mut *self.ptr.as_ptr().add(index)) }
        } else {
            None
        }
    }

    /// Returns the first element of the vector, or None if it is empty.
    pub fn first(&self) -> Option<&T> {
        self.get(0)
    }

    /// Returns the last element of the vector, or None if it is empty.
    pub fn last(&self) -> Option<&T> {
        if self.len > 0 {
            self.get(self.len - 1)
        } else {
            None
        }
    }

    /// Inserts an element at position `index` within the vector.
    pub fn insert(&mut self, index: usize, element: T) {
        assert!(index <= self.len, "insertion index out of bounds");
        if self.len == self.cap {
            self.grow();
        }
        unsafe {
            let p = self.ptr.as_ptr().add(index);
            std::ptr::copy(p, p.add(1), self.len - index);
            std::ptr::write(p, element);
        }
        self.len += 1;
    }

    /// Removes and returns the element at position `index` within the vector.
    pub fn remove(&mut self, index: usize) -> T {
        assert!(index < self.len, "removal index out of bounds");
        unsafe {
            let p = self.ptr.as_ptr().add(index);
            let val = std::ptr::read(p);
            std::ptr::copy(p.add(1), p, self.len - index - 1);
            self.len -= 1;
            val
        }
    }

    /// Removes an element from the vector and returns it, swapping it with the last element.
    pub fn swap_remove(&mut self, index: usize) -> T {
        assert!(index < self.len, "swap_remove index out of bounds");
        unsafe {
            let p = self.ptr.as_ptr();
            let val = std::ptr::read(p.add(index));
            if index < self.len - 1 {
                let last = std::ptr::read(p.add(self.len - 1));
                std::ptr::write(p.add(index), last);
            }
            self.len -= 1;
            val
        }
    }

    /// Truncates the vector, keeping the first `len` elements.
    pub fn truncate(&mut self, len: usize) {
        while self.len > len {
            self.pop();
        }
    }

    /// Reserves capacity for at least `additional` more elements.
    pub fn reserve(&mut self, additional: usize) {
        let needed = self.len + additional;
        if needed > self.cap {
            self.grow_to(needed);
        }
    }

    /// Reserves capacity for exactly `additional` more elements.
    pub fn reserve_exact(&mut self, additional: usize) {
        self.reserve(additional);
    }

    /// Shrinks the capacity of the vector as much as possible.
    pub fn shrink_to_fit(&mut self) {
        if self.cap > self.len {
            if self.len == 0 {
                let old_layout = Layout::from_size_align(
                    self.cap * std::mem::size_of::<T>(),
                    std::mem::align_of::<T>(),
                )
                .expect("Invalid layout");
                unsafe {
                    global_allocator().dealloc(self.ptr.as_ptr() as *mut u8, old_layout);
                }
                self.ptr = NonNull::dangling();
                self.cap = 0;
            } else {
                let old_layout = Layout::from_size_align(
                    self.cap * std::mem::size_of::<T>(),
                    std::mem::align_of::<T>(),
                )
                .expect("Invalid layout");
                let new_layout = Layout::from_size_align(
                    self.len * std::mem::size_of::<T>(),
                    std::mem::align_of::<T>(),
                )
                .expect("Invalid layout");
                unsafe {
                    let new_ptr = global_allocator()
                        .realloc(self.ptr.as_ptr() as *mut u8, old_layout, new_layout)
                        .expect("Reallocation failed");
                    self.ptr = NonNull::new_unchecked(new_ptr as *mut T);
                }
                self.cap = self.len;
            }
        }
    }

    /// Reverses the order of elements in the vector, in place.
    pub fn reverse(&mut self) {
        let mut i = 0;
        let mut j = self.len.saturating_sub(1);
        while i < j {
            unsafe {
                let p = self.ptr.as_ptr();
                std::ptr::swap(p.add(i), p.add(j));
            }
            i += 1;
            j -= 1;
        }
    }

    /// Sorts the slice.
    pub fn sort(&mut self)
    where
        T: Ord + std::cmp::Ord,
    {
        self.as_mut_slice().sort();
    }

    /// Sorts the slice with a comparator function.
    pub fn sort_by<F>(&mut self, compare: F)
    where
        F: FnMut(&T, &T) -> std::cmp::Ordering,
    {
        self.as_mut_slice().sort_by(compare);
    }

    /// Returns true if the vector contains an element equal to the given value.
    pub fn contains(&self, x: &T) -> bool
    where
        T: PartialEq,
    {
        self.as_slice().contains(x)
    }

    fn grow_to(&mut self, new_cap: usize) {
        let new_layout = Layout::from_size_align(
            new_cap * std::mem::size_of::<T>(),
            std::mem::align_of::<T>(),
        )
        .expect("Invalid layout");

        let new_ptr = if self.cap == 0 {
            unsafe {
                global_allocator()
                    .alloc(new_layout)
                    .expect("Allocation failed")
            }
        } else {
            let old_layout = Layout::from_size_align(
                self.cap * std::mem::size_of::<T>(),
                std::mem::align_of::<T>(),
            )
            .expect("Invalid layout");

            unsafe {
                global_allocator()
                    .realloc(self.ptr.as_ptr() as *mut u8, old_layout, new_layout)
                    .expect("Reallocation failed")
            }
        };

        self.ptr = unsafe { NonNull::new_unchecked(new_ptr as *mut T) };
        self.cap = new_cap;
    }

    /// Clears the Vec, removing all values
    pub fn clear(&mut self) {
        while self.pop().is_some() {}
    }

    /// Returns a slice of the Vec's contents
    pub fn as_slice(&self) -> &[T] {
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }

    /// Returns a mutable slice of the Vec's contents
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }

    fn grow(&mut self) {
        let new_cap = if self.cap == 0 { 4 } else { self.cap * 2 };

        let new_layout = Layout::from_size_align(
            new_cap * std::mem::size_of::<T>(),
            std::mem::align_of::<T>(),
        )
        .expect("Invalid layout");

        let new_ptr = if self.cap == 0 {
            unsafe {
                global_allocator()
                    .alloc(new_layout)
                    .expect("Allocation failed")
            }
        } else {
            let old_layout = Layout::from_size_align(
                self.cap * std::mem::size_of::<T>(),
                std::mem::align_of::<T>(),
            )
            .expect("Invalid layout");

            unsafe {
                global_allocator()
                    .realloc(self.ptr.as_ptr() as *mut u8, old_layout, new_layout)
                    .expect("Reallocation failed")
            }
        };

        self.ptr = unsafe { NonNull::new_unchecked(new_ptr as *mut T) };
        self.cap = new_cap;
    }
}

impl<T> Drop for Vec<T> {
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

impl<T> Default for Vec<T> {
    fn default() -> Self {
        Self::new()
    }
}
