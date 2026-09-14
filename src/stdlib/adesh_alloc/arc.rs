//! Arc - Atomic Reference Counted Smart Pointer
//!
//! Thread-safe reference-counted pointer with no garbage collection.

use std::ops::Deref;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Internal structure for Arc
pub(crate) struct ArcInner<T> {
    pub(crate) strong: AtomicUsize,
    pub(crate) weak: AtomicUsize,
    pub(crate) data: T,
}

/// Atomic Reference Counted pointer
pub struct Arc<T> {
    pub(crate) ptr: NonNull<ArcInner<T>>,
    pub(crate) _phantom: std::marker::PhantomData<ArcInner<T>>,
}

unsafe impl<T: Send + Sync> Send for Arc<T> {}
unsafe impl<T: Send + Sync> Sync for Arc<T> {}

impl<T> Arc<T> {
    /// Creates a new Arc with the given value
    ///
    /// # Examples
    /// ```
    /// use adeshlang::stdlib::adesh_alloc::Arc;
    /// let arc = Arc::new(42);
    /// assert_eq!(*arc, 42);
    /// ```
    pub fn new(data: T) -> Self {
        let inner = Box::new(ArcInner {
            strong: AtomicUsize::new(1),
            weak: AtomicUsize::new(1),
            data,
        });

        Arc {
            // SAFETY: Box::into_raw never returns null, so new_unchecked is safe
            ptr: unsafe { NonNull::new_unchecked(Box::into_raw(inner)) },
            _phantom: std::marker::PhantomData,
        }
    }

    /// Gets the number of strong references
    ///
    /// # Examples
    /// ```
    /// use adeshlang::stdlib::adesh_alloc::Arc;
    /// let arc = Arc::new(5);
    /// assert_eq!(Arc::strong_count(&arc), 1);
    /// ```
    pub fn strong_count(this: &Self) -> usize {
        this.inner().strong.load(Ordering::Acquire)
    }

    /// Gets the number of weak references
    ///
    /// # Examples
    /// ```
    /// use adeshlang::stdlib::adesh_alloc::Arc;
    /// let arc = Arc::new(5);
    /// assert_eq!(Arc::weak_count(&arc), 0);
    /// ```
    pub fn weak_count(this: &Self) -> usize {
        this.inner().weak.load(Ordering::Acquire)
    }

    /// Returns a mutable reference if there is exactly one strong reference
    ///
    /// Returns None if the reference count is not 1.
    ///
    /// # Safety
    /// This is safe because if there's only one strong reference, we have
    /// exclusive access to the data.
    ///
    /// # Examples
    /// ```
    /// use adeshlang::stdlib::adesh_alloc::Arc;
    /// let mut arc = Arc::new(5);
    /// *Arc::get_mut(&mut arc).unwrap() = 10;
    /// assert_eq!(*arc, 10);
    /// ```
    pub fn get_mut(this: &mut Self) -> Option<&mut T> {
        if Self::strong_count(this) == 1 {
            // SAFETY: We have exclusive access (strong_count == 1)
            // This is the only mutable reference to the data
            unsafe { Some(&mut (*this.ptr.as_ptr()).data) }
        } else {
            None
        }
    }

    /// Creates a weak reference to this Arc
    ///
    /// Weak references do not prevent the value from being dropped.
    ///
    /// # Examples
    /// ```
    /// use adeshlang::stdlib::adesh_alloc::Arc;
    /// let arc = Arc::new(5);
    /// let weak = Arc::downgrade(&arc);
    /// drop(arc);
    /// assert!(weak.upgrade().is_none());
    /// ```
    pub fn downgrade(this: &Self) -> crate::stdlib::adesh_alloc::weak::Weak<T> {
        let mut count = this.inner().weak.load(Ordering::Relaxed);
        loop {
            if count > isize::MAX as usize {
                std::process::abort();
            }
            match this.inner().weak.compare_exchange_weak(
                count,
                count + 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => count = actual,
            }
        }
        crate::stdlib::adesh_alloc::weak::Weak {
            ptr: this.ptr,
            _phantom: std::marker::PhantomData,
        }
    }

    #[inline]
    fn inner(&self) -> &ArcInner<T> {
        // SAFETY: ptr is always valid as long as Arc exists
        // The pointer came from Box::into_raw which guarantees validity
        unsafe { self.ptr.as_ref() }
    }
}

impl<T> Clone for Arc<T> {
    /// Creates a new Arc pointer to the same allocation
    ///
    /// This increments the strong reference count.
    ///
    /// # Panics
    /// Panics if the reference count overflows (extremely unlikely in practice).
    fn clone(&self) -> Self {
        let mut count = self.inner().strong.load(Ordering::Relaxed);
        loop {
            if count > isize::MAX as usize {
                std::process::abort();
            }
            match self.inner().strong.compare_exchange_weak(
                count,
                count + 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => count = actual,
            }
        }

        Arc {
            ptr: self.ptr,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<T> Deref for Arc<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.inner().data
    }
}

impl<T> Drop for Arc<T> {
    fn drop(&mut self) {
        // Decrement the strong count
        // Use Release ordering to ensure all previous writes are visible to the thread that will drop
        if self.inner().strong.fetch_sub(1, Ordering::Release) != 1 {
            // Not the last reference, just return
            return;
        }

        // This fence is needed to prevent reordering of uses of the data and
        // the deletion of the data. We need Acquire ordering to ensure we see
        // all writes from other threads.
        std::sync::atomic::fence(Ordering::Acquire);

        // SAFETY: We're the last strong reference (fetch_sub returned 1)
        // The fence above ensures all previous writes are visible
        // We have exclusive access to drop the data
        unsafe {
            std::ptr::drop_in_place(&mut (*self.ptr.as_ptr()).data);
        }

        // Decrement the weak count (releasing the implicit weak reference)
        if self.inner().weak.fetch_sub(1, Ordering::Release) == 1 {
            std::sync::atomic::fence(Ordering::Acquire);
            unsafe {
                let _ = Box::from_raw(self.ptr.as_ptr());
            }
        }
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for Arc<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&**self, f)
    }
}

impl<T: PartialEq> PartialEq for Arc<T> {
    fn eq(&self, other: &Arc<T>) -> bool {
        **self == **other
    }
}

impl<T: Eq> Eq for Arc<T> {}
