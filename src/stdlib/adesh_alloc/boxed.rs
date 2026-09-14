//! Box - Unique Heap Pointer
//!
//! A pointer type for heap allocation with unique ownership.

use crate::stdlib::adesh_alloc::allocator::{Allocator, global_allocator};
use crate::stdlib::adesh_core::layout::Layout;
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;

/// A unique heap-allocated pointer
pub struct Box<T> {
    ptr: NonNull<T>,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> Box<T> {
    /// Allocates memory on the heap and places `value` in it
    pub fn new(value: T) -> Self {
        let layout = Layout::from_size_align(std::mem::size_of::<T>(), std::mem::align_of::<T>())
            .expect("Invalid layout");

        let ptr = unsafe {
            let ptr = global_allocator().alloc(layout).expect("Allocation failed");
            let typed_ptr = ptr as *mut T;
            std::ptr::write(typed_ptr, value);
            NonNull::new_unchecked(typed_ptr)
        };

        Box {
            ptr,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Consumes the Box and returns the wrapped value
    pub fn into_inner(self) -> T {
        let value = unsafe { std::ptr::read(self.ptr.as_ptr()) };
        std::mem::forget(self); // Prevent drop
        value
    }

    /// Leaks the Box, returning a mutable reference
    pub fn leak(self) -> &'static mut T {
        let ptr = self.ptr;
        std::mem::forget(self);
        unsafe { &mut *ptr.as_ptr() }
    }
}

impl<T> Deref for Box<T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { self.ptr.as_ref() }
    }
}

impl<T> DerefMut for Box<T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { self.ptr.as_mut() }
    }
}

impl<T> Drop for Box<T> {
    fn drop(&mut self) {
        unsafe {
            std::ptr::drop_in_place(self.ptr.as_ptr());

            let layout =
                Layout::from_size_align(std::mem::size_of::<T>(), std::mem::align_of::<T>())
                    .expect("Invalid layout");

            global_allocator().dealloc(self.ptr.as_ptr() as *mut u8, layout);
        }
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for Box<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&**self, f)
    }
}

impl<T: std::fmt::Display> std::fmt::Display for Box<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&**self, f)
    }
}

impl<T: PartialEq> PartialEq for Box<T> {
    fn eq(&self, other: &Box<T>) -> bool {
        **self == **other
    }
}

impl<T: Eq> Eq for Box<T> {}
