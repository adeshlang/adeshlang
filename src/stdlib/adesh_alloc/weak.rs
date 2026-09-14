//! Weak - Weak Reference to Arc
//!
//! A weak reference does not prevent the Arc from being dropped.

use crate::stdlib::adesh_alloc::arc::Arc;
use std::ptr::NonNull;
use std::sync::atomic::Ordering;

// Use the same ArcInner structure from arc module
use super::arc::ArcInner;

/// Weak reference to an Arc
pub struct Weak<T> {
    pub(crate) ptr: NonNull<ArcInner<T>>,
    pub(crate) _phantom: std::marker::PhantomData<ArcInner<T>>,
}

unsafe impl<T: Send + Sync> Send for Weak<T> {}
unsafe impl<T: Send + Sync> Sync for Weak<T> {}

impl<T> Weak<T> {
    /// Attempts to upgrade the weak reference to an Arc
    ///
    /// Returns None if the value has already been dropped
    pub fn upgrade(&self) -> Option<Arc<T>> {
        let inner = unsafe { self.ptr.as_ref() };

        let mut n = inner.strong.load(Ordering::Relaxed);
        loop {
            if n == 0 {
                return None;
            }
            if n > isize::MAX as usize {
                std::process::abort();
            }

            match inner
                .strong
                .compare_exchange_weak(n, n + 1, Ordering::Acquire, Ordering::Relaxed)
            {
                Ok(_) => {
                    return Some(Arc {
                        ptr: self.ptr,
                        _phantom: std::marker::PhantomData,
                    });
                }
                Err(old) => n = old,
            }
        }
    }

    /// Gets the number of strong references
    pub fn strong_count(&self) -> usize {
        unsafe { self.ptr.as_ref().strong.load(Ordering::Acquire) }
    }

    /// Gets the number of weak references
    pub fn weak_count(&self) -> usize {
        unsafe { self.ptr.as_ref().weak.load(Ordering::Acquire) }
    }
}

impl<T> Clone for Weak<T> {
    fn clone(&self) -> Self {
        unsafe {
            let mut count = self.ptr.as_ref().weak.load(Ordering::Relaxed);
            loop {
                if count > isize::MAX as usize {
                    std::process::abort();
                }
                match self.ptr.as_ref().weak.compare_exchange_weak(
                    count,
                    count + 1,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => break,
                    Err(actual) => count = actual,
                }
            }
        }
        Weak {
            ptr: self.ptr,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<T> Drop for Weak<T> {
    fn drop(&mut self) {
        unsafe {
            if self.ptr.as_ref().weak.fetch_sub(1, Ordering::Release) == 1 {
                std::sync::atomic::fence(Ordering::Acquire);
                let _ = Box::from_raw(self.ptr.as_ptr());
            }
        }
    }
}
