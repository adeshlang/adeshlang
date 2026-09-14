//! Async Runtime
//!
//! Production-grade asynchronous execution and futures support.
//!
//! This module provides:
//! - Future trait compatible with std::future
//! - Poll type for future state
//! - Integration with runtime async primitives

use std::pin::Pin;
use std::task::{Context, Poll as StdPoll};

/// Re-export from runtime stdlib for compatibility
pub use crate::runtime::stdlib_src::async_runtime::*;

/// Future trait for asynchronous computations
///
/// This is compatible with std::future::Future for interoperability.
pub trait Future {
    /// The type of value produced on completion
    type Output;

    /// Attempts to resolve the future to a final value
    ///
    /// # Safety
    /// This function is safe to call repeatedly until it returns Poll::Ready.
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> StdPoll<Self::Output>;
}

/// Poll result indicating whether a Future is ready
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Poll<T> {
    /// The future is complete with the given value
    Ready(T),
    /// The future is not yet complete
    Pending,
}

impl<T> From<Poll<T>> for StdPoll<T> {
    fn from(poll: Poll<T>) -> Self {
        match poll {
            Poll::Ready(t) => StdPoll::Ready(t),
            Poll::Pending => StdPoll::Pending,
        }
    }
}

impl<T> From<StdPoll<T>> for Poll<T> {
    fn from(poll: StdPoll<T>) -> Self {
        match poll {
            StdPoll::Ready(t) => Poll::Ready(t),
            StdPoll::Pending => Poll::Pending,
        }
    }
}

/// Errors that can occur when working with async operations
#[derive(Debug, Clone)]
pub enum AsyncError {
    /// Task join failed
    JoinError(String),
    /// Task already completed
    AlreadyCompleted,
    /// Task was cancelled
    Cancelled,
    /// Runtime error
    RuntimeError(String),
}

impl std::fmt::Display for AsyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AsyncError::JoinError(msg) => write!(f, "Async join error: {}", msg),
            AsyncError::AlreadyCompleted => write!(f, "Async operation already completed"),
            AsyncError::Cancelled => write!(f, "Async operation was cancelled"),
            AsyncError::RuntimeError(msg) => write!(f, "Async runtime error: {}", msg),
        }
    }
}

impl std::error::Error for AsyncError {}

/// A simple future that resolves immediately with a value
pub struct Ready<T> {
    value: Option<T>,
}

impl<T> Ready<T> {
    /// Creates a new Ready future
    pub fn new(value: T) -> Self {
        Ready { value: Some(value) }
    }
}

impl<T> std::future::Future for Ready<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> StdPoll<Self::Output> {
        // SAFETY: We're only accessing the Option, not moving out of Pin
        let value = unsafe { &mut self.get_unchecked_mut().value };
        StdPoll::Ready(value.take().expect("Ready polled after completion"))
    }
}

impl<T> Future for Ready<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> StdPoll<Self::Output> {
        // SAFETY: We're only accessing the Option, not moving out of Pin
        let value = unsafe { &mut self.get_unchecked_mut().value };
        StdPoll::Ready(value.take().expect("Ready polled after completion"))
    }
}

/// A future that never completes
pub struct Pending<T> {
    _phantom: std::marker::PhantomData<T>,
}

impl<T> Pending<T> {
    /// Creates a new Pending future
    pub fn new() -> Self {
        Pending {
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<T> Default for Pending<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> std::future::Future for Pending<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> StdPoll<Self::Output> {
        StdPoll::Pending
    }
}

impl<T> Future for Pending<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> StdPoll<Self::Output> {
        StdPoll::Pending
    }
}

/// Creates a future that resolves immediately
pub fn ready<T>(value: T) -> Ready<T> {
    Ready::new(value)
}

/// Creates a future that never completes
pub fn pending<T>() -> Pending<T> {
    Pending::new()
}

/// Adapter to convert our Future to std::future::Future
pub struct FutureAdapter<F> {
    inner: F,
}

impl<F> FutureAdapter<F> {
    /// Wraps a Future for use with std async ecosystem
    pub fn new(future: F) -> Self {
        FutureAdapter { inner: future }
    }
}

impl<F> std::future::Future for FutureAdapter<F>
where
    F: Future,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> StdPoll<Self::Output> {
        // SAFETY: We're pinning through to the inner future
        let inner = unsafe { self.map_unchecked_mut(|s| &mut s.inner) };
        F::poll(inner, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::task::{RawWaker, RawWakerVTable, Waker};

    fn noop_raw_waker() -> RawWaker {
        fn clone(_: *const ()) -> RawWaker {
            noop_raw_waker()
        }

        fn wake(_: *const ()) {}
        fn wake_by_ref(_: *const ()) {}
        fn drop(_: *const ()) {}

        RawWaker::new(
            std::ptr::null(),
            &RawWakerVTable::new(clone, wake, wake_by_ref, drop),
        )
    }

    fn noop_waker() -> Waker {
        // SAFETY: RawWaker uses a null data pointer and no-op vtable.
        unsafe { Waker::from_raw(noop_raw_waker()) }
    }

    #[test]
    fn test_poll_conversion() {
        let ready: StdPoll<i32> = Poll::Ready(42).into();
        assert_eq!(ready, StdPoll::Ready(42));

        let pending: StdPoll<i32> = Poll::Pending.into();
        assert_eq!(pending, StdPoll::Pending);
    }

    #[test]
    fn test_ready_future() {
        use std::task::Context;

        let mut future = ready(42);
        let waker = noop_waker();
        let mut context = Context::from_waker(&waker);

        let pinned = Pin::new(&mut future);
        let result = Future::poll(pinned, &mut context);

        assert_eq!(result, StdPoll::Ready(42));
    }

    #[test]
    fn test_pending_future() {
        use std::task::Context;

        let mut future: Pending<i32> = pending();
        let waker = noop_waker();
        let mut context = Context::from_waker(&waker);

        let pinned = Pin::new(&mut future);
        let result = Future::poll(pinned, &mut context);

        assert_eq!(result, StdPoll::Pending);
    }
}
