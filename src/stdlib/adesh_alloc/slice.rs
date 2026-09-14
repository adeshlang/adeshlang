//! Slice - Safe Non-Owning Contiguous View
//!
//! A non-owning view into a contiguous sequence of elements.

use std::marker::PhantomData;

/// A non-owning view over a slice of data
pub struct Slice<'a, T> {
    ptr: *const T,
    len: usize,
    _phantom: PhantomData<&'a T>,
}

impl<'a, T> Slice<'a, T> {
    /// Creates a new Slice from a raw pointer and length
    pub fn from_raw_parts(ptr: *const T, len: usize) -> Self {
        Slice {
            ptr,
            len,
            _phantom: PhantomData,
        }
    }

    /// Creates a Slice from a Rust slice
    pub fn from_slice(slice: &'a [T]) -> Self {
        Slice {
            ptr: slice.as_ptr(),
            len: slice.len(),
            _phantom: PhantomData,
        }
    }

    /// Returns the length of the slice
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns true if the slice is empty
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Gets a reference to the element at the index
    pub fn get(&self, index: usize) -> Option<&'a T> {
        if index < self.len {
            unsafe { Some(&*self.ptr.add(index)) }
        } else {
            None
        }
    }

    /// Returns the first element of the slice
    pub fn first(&self) -> Option<&'a T> {
        self.get(0)
    }

    /// Returns the last element of the slice
    pub fn last(&self) -> Option<&'a T> {
        if self.len > 0 {
            self.get(self.len - 1)
        } else {
            None
        }
    }

    /// Returns true if the slice contains an element equal to `x`
    pub fn contains(&self, x: &T) -> bool
    where
        T: PartialEq,
    {
        for i in 0..self.len {
            if self.get(i) == Some(x) {
                return true;
            }
        }
        false
    }

    /// Splits the slice into two sub-slices at index `at`
    pub fn split_at(&self, at: usize) -> (Slice<'a, T>, Slice<'a, T>) {
        let mid = at.min(self.len);
        let left = Slice::from_raw_parts(self.ptr, mid);
        let right_ptr = unsafe { self.ptr.add(mid) };
        let right = Slice::from_raw_parts(right_ptr, self.len - mid);
        (left, right)
    }

    /// Returns the underlying raw slice
    pub fn as_slice(&self) -> &'a [T] {
        if self.len == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
        }
    }
}

impl<'a, T> Clone for Slice<'a, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, T> Copy for Slice<'a, T> {}
