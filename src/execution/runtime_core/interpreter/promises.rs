//! Promise implementation for async/await support.
//!
//! Provides promise state management and handler tracking for
//! the microtask queue implementation.

use crate::parsing::ast::Value;

/// Promise state during execution.
#[derive(Clone)]
pub(crate) enum PromiseState {
    Pending,
    Fulfilled(Value),
    Rejected(Value),
}

/// Handler for promise then/catch chaining.
pub(crate) struct ThenHandler {
    pub(crate) on_fulfill: Option<Value>,
    pub(crate) on_reject: Option<Value>,
    pub(crate) downstream: u64, // promise id to settle after handler runs
}

/// Promise entry in the promise registry.
pub(crate) struct PromiseEntry {
    pub(crate) state: PromiseState,
    pub(crate) handlers: Vec<ThenHandler>,
}
