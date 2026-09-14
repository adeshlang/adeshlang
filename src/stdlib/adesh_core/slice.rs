//! Slice Operations
//!
//! Operations on contiguous sequences of elements.
//! No allocations, just views into existing memory.

use crate::stdlib::adesh_core::iter::Iterator;
use std::marker::PhantomData;

/// A contiguous sequence of elements
#[derive(Clone, Copy)]
pub struct Slice<T> {
    ptr: *const T,
    len: usize,
}

impl<T> Slice<T> {
    /// Creates a new slice from a pointer and length
    ///
    /// # Safety
    /// The caller must ensure that:
    /// - `ptr` is valid for `len` elements
    /// - The memory pointed to is not mutated during the slice's lifetime
    pub unsafe fn from_raw_parts(ptr: *const T, len: usize) -> Self {
        Slice { ptr, len }
    }

    /// Returns the length of the slice
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns true if the slice is empty
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns a reference to the element at the given index, if in bounds
    pub fn get(&self, index: usize) -> Option<&T> {
        if index < self.len {
            unsafe { Some(&*self.ptr.add(index)) }
        } else {
            None
        }
    }

    /// Returns a pointer to the first element
    pub fn as_ptr(&self) -> *const T {
        self.ptr
    }

    /// Splits the slice at the given index
    pub fn split_at(&self, mid: usize) -> (Slice<T>, Slice<T>) {
        assert!(mid <= self.len, "split_at index out of bounds");
        unsafe {
            (
                Slice::from_raw_parts(self.ptr, mid),
                Slice::from_raw_parts(self.ptr.add(mid), self.len - mid),
            )
        }
    }

    /// Returns the first element, if any
    pub fn first(&self) -> Option<&T> {
        if self.len > 0 {
            unsafe { Some(&*self.ptr) }
        } else {
            None
        }
    }

    /// Returns the last element, if any
    pub fn last(&self) -> Option<&T> {
        if self.len > 0 {
            unsafe { Some(&*self.ptr.add(self.len - 1)) }
        } else {
            None
        }
    }

    /// Checks if the slice contains a value
    pub fn contains(&self, value: &T) -> bool
    where
        T: PartialEq,
    {
        self.iter().any(|x| x == value)
    }

    /// Finds index of a value in the slice
    pub fn index_of(&self, value: &T) -> Option<usize>
    where
        T: PartialEq,
    {
        for i in 0..self.len {
            if unsafe { &*self.ptr.add(i) } == value {
                return Some(i);
            }
        }
        None
    }

    /// Returns an iterator over the slice elements
    pub fn iter(&self) -> SliceIter<'_, T> {
        SliceIter {
            ptr: self.ptr,
            end: unsafe { self.ptr.add(self.len) },
            _marker: PhantomData,
        }
    }
}

/// An iterator over slice elements
pub struct SliceIter<'a, T> {
    ptr: *const T,
    end: *const T,
    _marker: PhantomData<&'a T>,
}

impl<'a, T> crate::stdlib::adesh_core::iter::Iterator for SliceIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.ptr == self.end {
            None
        } else {
            unsafe {
                let old = self.ptr;
                self.ptr = self.ptr.add(1);
                Some(&*old)
            }
        }
    }
}

/// A mutable contiguous sequence of elements
pub struct SliceMut<T> {
    ptr: *mut T,
    len: usize,
}

impl<T> SliceMut<T> {
    /// Creates a new mutable slice from a pointer and length
    ///
    /// # Safety
    /// The caller must ensure that:
    /// - `ptr` is valid for `len` elements
    /// - No other references to this memory exist
    pub unsafe fn from_raw_parts_mut(ptr: *mut T, len: usize) -> Self {
        SliceMut { ptr, len }
    }

    /// Returns the length of the slice
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns true if the slice is empty
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns a mutable reference to the element at the given index, if in bounds
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        if index < self.len {
            unsafe { Some(&mut *self.ptr.add(index)) }
        } else {
            None
        }
    }

    /// Returns a mutable pointer to the first element
    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.ptr
    }

    /// Returns the first mutable element, if any
    pub fn first_mut(&mut self) -> Option<&mut T> {
        if self.len > 0 {
            unsafe { Some(&mut *self.ptr) }
        } else {
            None
        }
    }

    /// Returns the last mutable element, if any
    pub fn last_mut(&mut self) -> Option<&mut T> {
        if self.len > 0 {
            unsafe { Some(&mut *self.ptr.add(self.len - 1)) }
        } else {
            None
        }
    }

    /// Splits the mutable slice at the given index
    pub fn split_at_mut(&mut self, mid: usize) -> (SliceMut<T>, SliceMut<T>) {
        assert!(mid <= self.len, "split_at_mut index out of bounds");
        unsafe {
            (
                SliceMut::from_raw_parts_mut(self.ptr, mid),
                SliceMut::from_raw_parts_mut(self.ptr.add(mid), self.len - mid),
            )
        }
    }
}
