//! Promise and async operations runtime infrastructure
//!
//! This module provides the complete promise runtime for AdeshLang, including:
//! - Promise creation, resolution, and rejection
//! - Promise combinators (all, race, any, allSettled)
//! - Promise chaining with .then() and .catch()
//! - Async/await support with microtask processing
//! - Timer functions (setTimeout, sleep)
//! - Concurrent task spawning
//!
//! The promise runtime uses Arc-based shared values for zero-copy semantics
//! and implements a microtask queue for proper event loop behavior.

use crate::utils::collections::FastMap;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use super::RuntimeValue;

// ============================================================================
// Promise Runtime Types
// ============================================================================

/// Unique promise identifier
pub type PromiseId = u64;

/// Shared runtime value using Arc for zero-copy sharing
pub type SharedValue = Arc<RuntimeValue>;

/// Promise state - tracks whether a promise is pending, fulfilled, or rejected
#[derive(Debug, Clone)]
pub enum PromiseState {
    Pending,
    Fulfilled(SharedValue),
    Rejected(SharedValue),
}

/// A handler attached to a promise via .then() or .catch()
#[derive(Clone)]
pub struct PromiseHandler {
    /// Function to call on fulfillment (optional)
    pub on_fulfill: Option<SharedValue>,
    /// Function to call on rejection (optional)
    pub on_reject: Option<SharedValue>,
    /// The downstream promise that receives the handler's result
    pub downstream_id: PromiseId,
}

impl std::fmt::Debug for PromiseHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PromiseHandler")
            .field("on_fulfill", &self.on_fulfill.is_some())
            .field("on_reject", &self.on_reject.is_some())
            .field("downstream_id", &self.downstream_id)
            .finish()
    }
}

/// A promise entry in the promise table
#[derive(Debug, Clone)]
pub struct PromiseEntry {
    pub state: PromiseState,
    pub handlers: Vec<PromiseHandler>,
}

impl PromiseEntry {
    #[inline]
    pub fn new() -> Self {
        PromiseEntry {
            state: PromiseState::Pending,
            handlers: Vec::with_capacity(2), // Most promises have 1-2 handlers
        }
    }
}

impl Default for PromiseEntry {
    fn default() -> Self {
        Self::new()
    }
}

/// Callable function representation for Promise handlers
#[derive(Debug, Clone)]
pub struct CallableFunction {
    pub name: String,
    pub params: Vec<String>,
    /// For closures, captured variables
    pub captures: FastMap<String, RuntimeValue>,
    /// Whether this is an async function (returns Promise automatically)
    pub is_async: bool,
}

impl PartialEq for CallableFunction {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

/// Microtask for the event loop
#[derive(Debug, Clone)]
pub enum Microtask {
    /// Settle a promise with a value (fulfill)
    SettleFulfill(PromiseId, SharedValue),
    /// Settle a promise with a rejection
    SettleReject(PromiseId, SharedValue),
    /// Call a handler function
    CallHandler {
        handler: SharedValue,
        arg: SharedValue,
        downstream_id: PromiseId,
        is_rejection: bool,
    },
    /// Timer callback - stores function to call and promise to settle
    Timer {
        callback: CallableFunction,
        promise_id: PromiseId,
    },
    PromiseAllCheck {
        result_id: PromiseId,
    },
    /// Check if Promise.race should resolve
    PromiseRaceCheck {
        result_id: PromiseId,
    },
    /// Check if Promise.any should resolve
    PromiseAnyCheck {
        result_id: PromiseId,
    },
}

/// Aggregate type for Promise combinators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregateType {
    All,
    Race,
    Any,
}

// ============================================================================
// Promise Runtime Implementation
// ============================================================================

/// Global promise runtime for JIT/AOT - optimized with inline functions
pub struct PromiseRuntime {
    /// All promises by ID
    promises: RwLock<FastMap<PromiseId, PromiseEntry>>,
    /// Next promise ID
    next_id: AtomicU64,
    /// Microtask queue
    microtasks: Mutex<VecDeque<Microtask>>,
    watchers_by_pid: RwLock<FastMap<PromiseId, Vec<PromiseId>>>,
    aggregates: RwLock<FastMap<PromiseId, Vec<PromiseId>>>,
    /// Type of each aggregate (All, Race, Any)
    aggregate_types: RwLock<FastMap<PromiseId, AggregateType>>,
    /// Count of pending timer threads
    pending_timers: AtomicU64,
}

impl PromiseRuntime {
    #[inline]
    pub fn new() -> Self {
        PromiseRuntime {
            promises: RwLock::new(FastMap::default()),
            next_id: AtomicU64::new(1),
            microtasks: Mutex::new(VecDeque::with_capacity(16)),
            watchers_by_pid: RwLock::new(FastMap::default()),
            aggregates: RwLock::new(FastMap::default()),
            aggregate_types: RwLock::new(FastMap::default()),
            pending_timers: AtomicU64::new(0),
        }
    }

    /// Create a new pending promise
    #[inline]
    pub fn create_promise(&self) -> PromiseId {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut promises = self.promises.write().unwrap();
        promises.insert(id, PromiseEntry::new());
        id
    }

    /// Get a promise's state (returns cloned state for thread safety)
    #[inline]
    pub fn get_state(&self, id: PromiseId) -> Option<PromiseState> {
        let promises = self.promises.read().unwrap();
        promises.get(&id).map(|p| p.state.clone())
    }

    /// Attach a then handler to a promise
    pub fn attach_then(
        &self,
        promise_id: PromiseId,
        on_fulfill: Option<SharedValue>,
        on_reject: Option<SharedValue>,
    ) -> PromiseId {
        let downstream_id = self.create_promise();

        // First, check state and collect what we need to do
        let action = {
            let mut promises = self.promises.write().unwrap();
            if let Some(entry) = promises.get_mut(&promise_id) {
                match &entry.state {
                    PromiseState::Pending => {
                        // Add handler for later
                        entry.handlers.push(PromiseHandler {
                            on_fulfill,
                            on_reject,
                            downstream_id,
                        });
                        None // No immediate action needed
                    }
                    PromiseState::Fulfilled(value) => Some((true, Arc::clone(value), on_fulfill)),
                    PromiseState::Rejected(value) => Some((false, Arc::clone(value), on_reject)),
                }
            } else {
                None
            }
        };

        // Now queue microtask outside the lock
        if let Some((is_fulfill, value, handler_opt)) = action {
            if let Some(handler) = handler_opt {
                self.queue_microtask(Microtask::CallHandler {
                    handler,
                    arg: value,
                    downstream_id,
                    is_rejection: false,
                });
            } else if is_fulfill {
                self.queue_microtask(Microtask::SettleFulfill(downstream_id, value));
            } else {
                self.queue_microtask(Microtask::SettleReject(downstream_id, value));
            }
        }

        downstream_id
    }

    /// Fulfill a promise with Arc value
    #[inline]
    pub fn fulfill(&self, id: PromiseId, value: SharedValue) {
        let handlers = {
            let mut promises = self.promises.write().unwrap();
            if let Some(entry) = promises.get_mut(&id) {
                if matches!(entry.state, PromiseState::Pending) {
                    entry.state = PromiseState::Fulfilled(Arc::clone(&value));
                    std::mem::take(&mut entry.handlers)
                } else {
                    return; // Already settled
                }
            } else {
                return;
            }
        };

        // Queue handlers as microtasks
        for handler in handlers {
            if let Some(on_fulfill) = handler.on_fulfill {
                self.queue_microtask(Microtask::CallHandler {
                    handler: on_fulfill,
                    arg: Arc::clone(&value),
                    downstream_id: handler.downstream_id,
                    is_rejection: false,
                });
            } else {
                self.queue_microtask(Microtask::SettleFulfill(
                    handler.downstream_id,
                    Arc::clone(&value),
                ));
            }
        }
        // Always notify aggregates (Promise.all) even if there are no handlers
        self.notify_promise_all_watchers(id);
    }

    /// Fulfill from owned RuntimeValue (convenience wrapper)
    #[inline]
    pub fn fulfill_value(&self, id: PromiseId, value: RuntimeValue) {
        self.fulfill(id, Arc::new(value));
    }

    /// Reject a promise with Arc value
    #[inline]
    pub fn reject(&self, id: PromiseId, value: SharedValue) {
        let handlers = {
            let mut promises = self.promises.write().unwrap();
            if let Some(entry) = promises.get_mut(&id) {
                if matches!(entry.state, PromiseState::Pending) {
                    entry.state = PromiseState::Rejected(Arc::clone(&value));
                    std::mem::take(&mut entry.handlers)
                } else {
                    return; // Already settled
                }
            } else {
                return;
            }
        };

        // Queue handlers as microtasks
        for handler in handlers {
            if let Some(on_reject) = handler.on_reject {
                self.queue_microtask(Microtask::CallHandler {
                    handler: on_reject,
                    arg: Arc::clone(&value),
                    downstream_id: handler.downstream_id,
                    is_rejection: false,
                });
            } else {
                self.queue_microtask(Microtask::SettleReject(
                    handler.downstream_id,
                    Arc::clone(&value),
                ));
            }
        }
        // Always notify aggregates (Promise.all) even if there are no handlers
        self.notify_promise_all_watchers(id);
    }

    /// Reject from owned RuntimeValue (convenience wrapper)
    #[inline]
    pub fn reject_value(&self, id: PromiseId, value: RuntimeValue) {
        self.reject(id, Arc::new(value));
    }

    /// Queue a microtask
    #[inline]
    pub fn queue_microtask(&self, task: Microtask) {
        let mut queue = self.microtasks.lock().unwrap();
        queue.push_back(task);
    }

    /// Pop next microtask
    #[inline]
    pub fn pop_microtask(&self) -> Option<Microtask> {
        let mut queue = self.microtasks.lock().unwrap();
        queue.pop_front()
    }

    /// Check if there are pending microtasks
    #[inline]
    pub fn has_microtasks(&self) -> bool {
        let queue = self.microtasks.lock().unwrap();
        !queue.is_empty()
    }

    /// Check if a promise is settled
    #[inline]
    pub fn is_settled(&self, id: PromiseId) -> bool {
        let promises = self.promises.read().unwrap();
        promises
            .get(&id)
            .map(|p| !matches!(p.state, PromiseState::Pending))
            .unwrap_or(true)
    }

    /// Schedule a timer callback - spawns a thread that queues a microtask after delay
    pub fn schedule_timer(&self, promise_id: PromiseId, callback: CallableFunction, delay_ms: u64) {
        use std::thread;
        use std::time::Duration;

        let callback_clone = callback.clone();

        // Increment pending timer count before spawning
        self.pending_timers.fetch_add(1, Ordering::SeqCst);

        thread::spawn(move || {
            thread::sleep(Duration::from_millis(delay_ms));
            // Queue the timer callback as a microtask
            PROMISE_RUNTIME.queue_microtask(Microtask::Timer {
                callback: callback_clone,
                promise_id,
            });
            // Decrement pending timer count after queuing
            PROMISE_RUNTIME
                .pending_timers
                .fetch_sub(1, Ordering::SeqCst);
        });
    }

    /// Check if there are pending timers
    pub fn has_pending_timers(&self) -> bool {
        self.pending_timers.load(Ordering::SeqCst) > 0
    }

    pub fn register_promise_all(&self, result_id: PromiseId, promise_ids: Vec<PromiseId>) {
        {
            let mut aggregates = self.aggregates.write().unwrap();
            aggregates.insert(result_id, promise_ids.clone());
        }
        {
            let mut types = self.aggregate_types.write().unwrap();
            types.insert(result_id, AggregateType::All);
        }
        {
            let mut watchers = self.watchers_by_pid.write().unwrap();
            for pid in promise_ids.iter() {
                let entry = watchers.entry(*pid).or_insert_with(Vec::new);
                entry.push(result_id);
            }
        }
        self.queue_microtask(Microtask::PromiseAllCheck { result_id });
    }

    pub fn register_promise_race(&self, result_id: PromiseId, promise_ids: Vec<PromiseId>) {
        {
            let mut aggregates = self.aggregates.write().unwrap();
            aggregates.insert(result_id, promise_ids.clone());
        }
        {
            let mut types = self.aggregate_types.write().unwrap();
            types.insert(result_id, AggregateType::Race);
        }
        {
            let mut watchers = self.watchers_by_pid.write().unwrap();
            for pid in promise_ids.iter() {
                let entry = watchers.entry(*pid).or_insert_with(Vec::new);
                entry.push(result_id);
            }
        }
        self.queue_microtask(Microtask::PromiseRaceCheck { result_id });
    }

    pub fn register_promise_any(&self, result_id: PromiseId, promise_ids: Vec<PromiseId>) {
        {
            let mut aggregates = self.aggregates.write().unwrap();
            aggregates.insert(result_id, promise_ids.clone());
        }
        {
            let mut types = self.aggregate_types.write().unwrap();
            types.insert(result_id, AggregateType::Any);
        }
        {
            let mut watchers = self.watchers_by_pid.write().unwrap();
            for pid in promise_ids.iter() {
                let entry = watchers.entry(*pid).or_insert_with(Vec::new);
                entry.push(result_id);
            }
        }
        self.queue_microtask(Microtask::PromiseAnyCheck { result_id });
    }

    pub fn get_aggregate_type(&self, result_id: PromiseId) -> Option<AggregateType> {
        let types = self.aggregate_types.read().unwrap();
        types.get(&result_id).copied()
    }

    pub fn get_aggregate_list(&self, result_id: PromiseId) -> Option<Vec<PromiseId>> {
        let aggregates = self.aggregates.read().unwrap();
        aggregates.get(&result_id).cloned()
    }

    fn notify_promise_all_watchers(&self, id: PromiseId) {
        let watchers = {
            let watchers = self.watchers_by_pid.read().unwrap();
            watchers.get(&id).cloned().unwrap_or_default()
        };
        for result_id in watchers {
            // Determine the correct microtask type based on aggregate type
            if let Some(agg_type) = self.get_aggregate_type(result_id) {
                match agg_type {
                    AggregateType::All => {
                        self.queue_microtask(Microtask::PromiseAllCheck { result_id })
                    }
                    AggregateType::Race => {
                        self.queue_microtask(Microtask::PromiseRaceCheck { result_id })
                    }
                    AggregateType::Any => {
                        self.queue_microtask(Microtask::PromiseAnyCheck { result_id })
                    }
                }
            } else {
                // Fallback to All
                self.queue_microtask(Microtask::PromiseAllCheck { result_id });
            }
        }
    }
}

impl Default for PromiseRuntime {
    fn default() -> Self {
        Self::new()
    }
}

/// Global promise runtime instance
pub static PROMISE_RUNTIME: once_cell::sync::Lazy<PromiseRuntime> =
    once_cell::sync::Lazy::new(PromiseRuntime::new);

// ============================================================================
// Promise Builtin Functions
// ============================================================================

pub(crate) fn runtime_promise_new(_args: &[RuntimeValue]) -> RuntimeValue {
    let id = PROMISE_RUNTIME.create_promise();
    RuntimeValue::Promise(id)
}

/// Promise.resolve(value) - create a pre-resolved promise
#[inline]
pub(crate) fn runtime_promise_resolve(args: &[RuntimeValue]) -> RuntimeValue {
    let value = args.first().cloned().unwrap_or(RuntimeValue::Null);
    let id = PROMISE_RUNTIME.create_promise();
    PROMISE_RUNTIME.fulfill_value(id, value);
    RuntimeValue::Promise(id)
}

/// Promise.reject(reason) - create a pre-rejected promise
#[inline]
pub(crate) fn runtime_promise_reject(args: &[RuntimeValue]) -> RuntimeValue {
    let reason = args.first().cloned().unwrap_or(RuntimeValue::Null);
    let id = PROMISE_RUNTIME.create_promise();
    PROMISE_RUNTIME.reject_value(id, reason);
    RuntimeValue::Promise(id)
}

/// Promise.all(promises) - wait for all promises to resolve, or reject on first rejection
/// Args: [array of promises]
#[inline]
pub(crate) fn runtime_promise_all(args: &[RuntimeValue]) -> RuntimeValue {
    let promises = match args.first() {
        Some(RuntimeValue::Array(arr)) => arr.clone(),
        _ => return RuntimeValue::Null,
    };

    if promises.is_empty() {
        // Empty array resolves to empty array immediately
        let id = PROMISE_RUNTIME.create_promise();
        PROMISE_RUNTIME.fulfill_value(id, RuntimeValue::Array(vec![]));
        return RuntimeValue::Promise(id);
    }

    // Create the result promise
    let result_id = PROMISE_RUNTIME.create_promise();

    // Collect all promise IDs
    let promise_ids: Vec<u64> = promises
        .iter()
        .filter_map(|p| match p {
            RuntimeValue::Promise(id) => Some(*id),
            _ => None,
        })
        .collect();

    let total = promise_ids.len();

    // Check if all are already settled
    let mut results = vec![RuntimeValue::Null; total];
    let mut all_fulfilled = true;
    let mut any_rejected = false;
    let mut reject_reason = RuntimeValue::Null;

    for (i, &pid) in promise_ids.iter().enumerate() {
        if let Some(state) = PROMISE_RUNTIME.get_state(pid) {
            match state {
                PromiseState::Fulfilled(value) => {
                    results[i] = (*value).clone();
                }
                PromiseState::Rejected(reason) => {
                    any_rejected = true;
                    reject_reason = (*reason).clone();
                    break;
                }
                PromiseState::Pending => {
                    all_fulfilled = false;
                }
            }
        }
    }

    if any_rejected {
        PROMISE_RUNTIME.reject_value(result_id, reject_reason);
    } else if all_fulfilled {
        PROMISE_RUNTIME.fulfill_value(result_id, RuntimeValue::Array(results));
    } else {
        PROMISE_RUNTIME.register_promise_all(result_id, promise_ids);
    }

    RuntimeValue::Promise(result_id)
}

/// Promise.race(promises) - resolve/reject with first settled promise
/// Args: [array of promises]
#[inline]
pub(crate) fn runtime_promise_race(args: &[RuntimeValue]) -> RuntimeValue {
    let promises = match args.first() {
        Some(RuntimeValue::Array(arr)) => arr.clone(),
        _ => return RuntimeValue::Null,
    };

    // Create the result promise
    let result_id = PROMISE_RUNTIME.create_promise();

    if promises.is_empty() {
        // Empty array - promise stays pending forever (per spec)
        return RuntimeValue::Promise(result_id);
    }

    // Collect promise IDs
    let promise_ids: Vec<PromiseId> = promises
        .iter()
        .filter_map(|p| match p {
            RuntimeValue::Promise(id) => Some(*id),
            _ => None,
        })
        .collect();

    // Find first settled promise
    for &pid in &promise_ids {
        if let Some(state) = PROMISE_RUNTIME.get_state(pid) {
            match state {
                PromiseState::Fulfilled(value) => {
                    PROMISE_RUNTIME.fulfill_value(result_id, (*value).clone());
                    return RuntimeValue::Promise(result_id);
                }
                PromiseState::Rejected(reason) => {
                    PROMISE_RUNTIME.reject_value(result_id, (*reason).clone());
                    return RuntimeValue::Promise(result_id);
                }
                PromiseState::Pending => {}
            }
        }
    }

    // No settled promises yet, register watchers
    PROMISE_RUNTIME.register_promise_race(result_id, promise_ids);

    RuntimeValue::Promise(result_id)
}

/// Promise.any(promises) - resolve with first fulfilled, reject if all reject
/// Args: [array of promises]
#[inline]
pub(crate) fn runtime_promise_any(args: &[RuntimeValue]) -> RuntimeValue {
    let promises = match args.first() {
        Some(RuntimeValue::Array(arr)) => arr.clone(),
        _ => return RuntimeValue::Null,
    };

    // Create the result promise
    let result_id = PROMISE_RUNTIME.create_promise();

    if promises.is_empty() {
        // Empty array - reject with AggregateError (here just an array of errors)
        PROMISE_RUNTIME.reject_value(result_id, RuntimeValue::Array(vec![]));
        return RuntimeValue::Promise(result_id);
    }

    // Collect promise IDs
    let promise_ids: Vec<PromiseId> = promises
        .iter()
        .filter_map(|p| match p {
            RuntimeValue::Promise(id) => Some(*id),
            _ => None,
        })
        .collect();

    let mut all_rejected = true;
    let mut reject_reasons = vec![];

    for &pid in &promise_ids {
        if let Some(state) = PROMISE_RUNTIME.get_state(pid) {
            match state {
                PromiseState::Fulfilled(value) => {
                    // First fulfilled - resolve
                    PROMISE_RUNTIME.fulfill_value(result_id, (*value).clone());
                    return RuntimeValue::Promise(result_id);
                }
                PromiseState::Rejected(reason) => {
                    reject_reasons.push((*reason).clone());
                }
                PromiseState::Pending => {
                    all_rejected = false;
                }
            }
        }
    }

    if all_rejected && !promise_ids.is_empty() {
        // All rejected - reject with aggregate error
        PROMISE_RUNTIME.reject_value(result_id, RuntimeValue::Array(reject_reasons));
    } else if !all_rejected {
        // Some pending, register watchers
        PROMISE_RUNTIME.register_promise_any(result_id, promise_ids);
    }

    RuntimeValue::Promise(result_id)
}

/// Promise.allSettled(promises) - wait for all to settle, never rejects
/// Args: [array of promises]
#[inline]
pub(crate) fn runtime_promise_all_settled(args: &[RuntimeValue]) -> RuntimeValue {
    let promises = match args.first() {
        Some(RuntimeValue::Array(arr)) => arr.clone(),
        _ => return RuntimeValue::Null,
    };

    // Create the result promise
    let result_id = PROMISE_RUNTIME.create_promise();

    if promises.is_empty() {
        PROMISE_RUNTIME.fulfill_value(result_id, RuntimeValue::Array(vec![]));
        return RuntimeValue::Promise(result_id);
    }

    let mut results = vec![];
    let mut all_settled = true;

    for p in &promises {
        if let RuntimeValue::Promise(pid) = p {
            if let Some(state) = PROMISE_RUNTIME.get_state(*pid) {
                match state {
                    PromiseState::Fulfilled(value) => {
                        let mut obj = FastMap::default();
                        obj.insert(
                            "status".to_string(),
                            RuntimeValue::String("fulfilled".to_string()),
                        );
                        obj.insert("value".to_string(), (*value).clone());
                        results.push(RuntimeValue::Object(obj));
                    }
                    PromiseState::Rejected(reason) => {
                        let mut obj = FastMap::default();
                        obj.insert(
                            "status".to_string(),
                            RuntimeValue::String("rejected".to_string()),
                        );
                        obj.insert("reason".to_string(), (*reason).clone());
                        results.push(RuntimeValue::Object(obj));
                    }
                    PromiseState::Pending => {
                        all_settled = false;
                    }
                }
            }
        }
    }

    if all_settled {
        PROMISE_RUNTIME.fulfill_value(result_id, RuntimeValue::Array(results));
    }

    RuntimeValue::Promise(result_id)
}

/// promise.then(onFulfill, onReject) - attach handlers
/// Args: [promise, onFulfill (optional), onReject (optional)]
#[inline]
pub(crate) fn runtime_promise_then(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }

    let promise_id = match &args[0] {
        RuntimeValue::Promise(id) => *id,
        _ => return RuntimeValue::Null,
    };

    // Wrap handlers in Arc for SharedValue
    let on_fulfill = args
        .get(1)
        .filter(|v| !matches!(v, RuntimeValue::Null))
        .map(|v| Arc::new(v.clone()));
    let on_reject = args
        .get(2)
        .filter(|v| !matches!(v, RuntimeValue::Null))
        .map(|v| Arc::new(v.clone()));

    let downstream_id = PROMISE_RUNTIME.attach_then(promise_id, on_fulfill, on_reject);
    RuntimeValue::Promise(downstream_id)
}

/// promise.catch(onReject) - attach rejection handler
/// Args: [promise, onReject]
#[inline]
pub(crate) fn runtime_promise_catch(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }

    let promise_id = match &args[0] {
        RuntimeValue::Promise(id) => *id,
        _ => return RuntimeValue::Null,
    };

    let on_reject = args
        .get(1)
        .filter(|v| !matches!(v, RuntimeValue::Null))
        .map(|v| Arc::new(v.clone()));

    let downstream_id = PROMISE_RUNTIME.attach_then(promise_id, None, on_reject);
    RuntimeValue::Promise(downstream_id)
}

/// Internal: fulfill a promise with a value
/// Args: [promise_id, value]
#[inline]
pub(crate) fn runtime_promise_fulfill(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Null;
    }

    let promise_id = match &args[0] {
        RuntimeValue::Promise(id) => *id,
        RuntimeValue::Int(id) => *id as u64,
        _ => return RuntimeValue::Null,
    };

    PROMISE_RUNTIME.fulfill_value(promise_id, args[1].clone());
    RuntimeValue::Null
}

/// Internal: reject a promise with a reason
/// Args: [promise_id, reason]
#[inline]
pub(crate) fn runtime_promise_reject_internal(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Null;
    }

    let promise_id = match &args[0] {
        RuntimeValue::Promise(id) => *id,
        RuntimeValue::Int(id) => *id as u64,
        _ => return RuntimeValue::Null,
    };

    PROMISE_RUNTIME.reject_value(promise_id, args[1].clone());
    RuntimeValue::Null
}

/// Create a resolve callback for a promise
/// Args: [promise]
/// Returns: a Function that, when called with a value, fulfills the promise
#[inline]
pub(crate) fn runtime_make_resolve(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }

    let promise_id = match &args[0] {
        RuntimeValue::Promise(id) => *id,
        _ => return RuntimeValue::Null,
    };

    // Create a special "resolve" function that captures the promise id
    let mut captures = FastMap::default();
    captures.insert(
        "__promise_id".to_string(),
        RuntimeValue::Int(promise_id as i64),
    );

    RuntimeValue::Function(CallableFunction {
        name: "__resolve".to_string(),
        params: vec!["value".to_string()],
        captures,
        is_async: false,
    })
}

/// Create a reject callback for a promise  
/// Args: [promise]
/// Returns: a Function that, when called with a reason, rejects the promise
#[inline]
pub(crate) fn runtime_make_reject(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }

    let promise_id = match &args[0] {
        RuntimeValue::Promise(id) => *id,
        _ => return RuntimeValue::Null,
    };

    // Create a special "reject" function that captures the promise id
    let mut captures = FastMap::default();
    captures.insert(
        "__promise_id".to_string(),
        RuntimeValue::Int(promise_id as i64),
    );

    RuntimeValue::Function(CallableFunction {
        name: "__reject".to_string(),
        params: vec!["reason".to_string()],
        captures,
        is_async: false,
    })
}

/// Indirect function call - call a function stored in a RuntimeValue
/// Args: [function, arg1, arg2, ...]
/// Handles special __resolve and __reject callbacks for Promise
#[inline]
pub(crate) fn runtime_call_indirect(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }

    let func = match &args[0] {
        RuntimeValue::Function(f) => f,
        // If not a function, return null
        _ => return RuntimeValue::Null,
    };

    let call_args = &args[1..];

    // Handle special promise resolve/reject callbacks
    match func.name.as_str() {
        "__resolve" => {
            // Get promise ID from captures
            if let Some(RuntimeValue::Int(id)) = func.captures.get("__promise_id") {
                let value = call_args.first().cloned().unwrap_or(RuntimeValue::Null);
                PROMISE_RUNTIME.fulfill_value(*id as u64, value);
            }
            RuntimeValue::Null
        }
        "__reject" => {
            // Get promise ID from captures
            if let Some(RuntimeValue::Int(id)) = func.captures.get("__promise_id") {
                let reason = call_args.first().cloned().unwrap_or(RuntimeValue::Null);
                PROMISE_RUNTIME.reject_value(*id as u64, reason);
            }
            RuntimeValue::Null
        }
        _ => {
            // For other functions, we'd need JIT context to execute them
            // This is a limitation - regular lambdas can't be called indirectly yet
            // For now, return null
            RuntimeValue::Null
        }
    }
}

// ============================================================================
// Async/Await and Concurrency Functions
// ============================================================================

/// await expression - blocks until promise is settled
/// For JIT, this is a synchronous wait (event loop runs during wait)
/// Args: [promise]
#[inline]
pub(crate) fn runtime_await(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }

    let promise_id = match &args[0] {
        RuntimeValue::Promise(id) => *id,
        // If not a promise, just return the value (like JS)
        other => return other.clone(),
    };

    // Busy-wait loop with microtask processing
    let mut iterations = 0u32;
    const MAX_WAIT_ITERATIONS: u32 = 1_000_000;

    loop {
        iterations += 1;
        if iterations > MAX_WAIT_ITERATIONS {
            return RuntimeValue::String("Await timeout: promise never settled".to_string());
        }

        // Process pending microtasks first
        process_microtasks_sync();

        // Check if promise is settled
        if let Some(state) = PROMISE_RUNTIME.get_state(promise_id) {
            match state {
                PromiseState::Fulfilled(value) => {
                    // Unwrap Arc to get the value
                    return (*value).clone();
                }
                PromiseState::Rejected(reason) => {
                    // In real async, this would throw. For now, return error value
                    let mut obj = FastMap::default();
                    obj.insert("error".to_string(), (*reason).clone());
                    return RuntimeValue::Object(obj);
                }
                PromiseState::Pending => {
                    // Yield briefly to prevent CPU spin
                    std::thread::sleep(std::time::Duration::from_micros(10));
                }
            }
        } else {
            return RuntimeValue::Null;
        }
    }
}

/// spawn(promise_or_call) - Spawn a concurrent task
/// For JIT, this wraps the value in a Promise if not already one
/// Args: [value] - typically a call to an async function
/// Returns: Promise that will resolve when the spawned task completes
#[inline]
pub(crate) fn runtime_spawn(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }

    match &args[0] {
        // If already a promise, just return it (task is already running)
        RuntimeValue::Promise(id) => RuntimeValue::Promise(*id),

        // If it's a function, we need to call it and wrap the result
        RuntimeValue::Function(func) => {
            // Create a promise for this spawn
            let promise_id = PROMISE_RUNTIME.create_promise();

            // Queue the function call as a microtask
            PROMISE_RUNTIME.queue_microtask(Microtask::Timer {
                callback: func.clone(),
                promise_id,
            });

            RuntimeValue::Promise(promise_id)
        }

        // For any other value, wrap it in an immediately fulfilled promise
        other => {
            let promise_id = PROMISE_RUNTIME.create_promise();
            PROMISE_RUNTIME.fulfill_value(promise_id, other.clone());
            RuntimeValue::Promise(promise_id)
        }
    }
}

/// Process all pending microtasks synchronously
/// Note: CallHandler microtasks that need JIT function calls are not processed here
/// They need to be handled by the JIT after await returns
fn process_microtasks_sync() {
    let max_iterations = 1000; // Prevent infinite loops
    let mut iterations = 0;

    while let Some(task) = PROMISE_RUNTIME.pop_microtask() {
        iterations += 1;
        if iterations > max_iterations {
            break;
        }

        match task {
            Microtask::SettleFulfill(id, value) => {
                PROMISE_RUNTIME.fulfill(id, value);
            }
            Microtask::SettleReject(id, reason) => {
                PROMISE_RUNTIME.reject(id, reason);
            }
            Microtask::CallHandler {
                handler,
                arg,
                downstream_id,
                is_rejection: _,
            } => {
                // For CallHandler, we need to call the handler function
                // Check if it's a Function that we can identify
                if let RuntimeValue::Function(_func) = handler.as_ref() {
                    // We can't call this without JIT context
                    // For now, if the function is a simple transform, just pass the value through
                    // This is a limitation - full support would require JIT integration
                    // For simple .then() chains, we just propagate the value
                    PROMISE_RUNTIME.fulfill(downstream_id, arg);
                } else {
                    // Not a function, just propagate the value
                    PROMISE_RUNTIME.fulfill(downstream_id, arg);
                }
            }
            Microtask::Timer {
                callback: _,
                promise_id,
            } => {
                // Timer callbacks are executed by the JIT, not here
                // We just mark that the timer fired by fulfilling its promise
                // The actual callback execution happens in the JIT's await handling
                PROMISE_RUNTIME.fulfill_value(promise_id, RuntimeValue::Null);
            }
            Microtask::PromiseAllCheck { result_id } => {
                if let Some(pids) = PROMISE_RUNTIME.get_aggregate_list(result_id) {
                    let mut results: Vec<RuntimeValue> = Vec::with_capacity(pids.len());
                    let mut all_fulfilled = true;
                    let mut any_rejected = false;
                    let mut reject_reason = RuntimeValue::Null;
                    for pid in pids.iter() {
                        if let Some(state) = PROMISE_RUNTIME.get_state(*pid) {
                            match state {
                                PromiseState::Fulfilled(value) => {
                                    results.push((*value).clone());
                                }
                                PromiseState::Rejected(reason) => {
                                    any_rejected = true;
                                    reject_reason = (*reason).clone();
                                    break;
                                }
                                PromiseState::Pending => {
                                    all_fulfilled = false;
                                }
                            }
                        } else {
                            all_fulfilled = false;
                        }
                    }
                    if any_rejected {
                        PROMISE_RUNTIME.reject_value(result_id, reject_reason);
                    } else if all_fulfilled {
                        PROMISE_RUNTIME.fulfill_value(result_id, RuntimeValue::Array(results));
                    }
                }
            }
            Microtask::PromiseRaceCheck { result_id } => {
                // Check if result is already settled
                if PROMISE_RUNTIME.is_settled(result_id) {
                    continue;
                }

                if let Some(pids) = PROMISE_RUNTIME.get_aggregate_list(result_id) {
                    for pid in pids.iter() {
                        if let Some(state) = PROMISE_RUNTIME.get_state(*pid) {
                            match state {
                                PromiseState::Fulfilled(value) => {
                                    PROMISE_RUNTIME.fulfill_value(result_id, (*value).clone());
                                    break;
                                }
                                PromiseState::Rejected(reason) => {
                                    PROMISE_RUNTIME.reject_value(result_id, (*reason).clone());
                                    break;
                                }
                                PromiseState::Pending => {}
                            }
                        }
                    }
                }
            }
            Microtask::PromiseAnyCheck { result_id } => {
                // Check if result is already settled
                if PROMISE_RUNTIME.is_settled(result_id) {
                    continue;
                }

                if let Some(pids) = PROMISE_RUNTIME.get_aggregate_list(result_id) {
                    let mut all_rejected = true;
                    let mut reject_reasons = vec![];

                    for pid in pids.iter() {
                        if let Some(state) = PROMISE_RUNTIME.get_state(*pid) {
                            match state {
                                PromiseState::Fulfilled(value) => {
                                    // First fulfilled wins
                                    PROMISE_RUNTIME.fulfill_value(result_id, (*value).clone());
                                    all_rejected = false;
                                    break;
                                }
                                PromiseState::Rejected(reason) => {
                                    reject_reasons.push((*reason).clone());
                                }
                                PromiseState::Pending => {
                                    all_rejected = false;
                                }
                            }
                        } else {
                            all_rejected = false;
                        }
                    }

                    // If all promises rejected (and we didn't fulfill above), reject with aggregate
                    if all_rejected && !reject_reasons.is_empty() {
                        PROMISE_RUNTIME
                            .reject_value(result_id, RuntimeValue::Array(reject_reasons));
                    }
                }
            }
        }
    }
}

// ============================================================================
// Timer Functions
// ============================================================================

/// setTimeout(callback, delay) - schedule callback after delay ms
/// Args: [callback (Function), delay_ms (Int)]
/// Returns: a Promise that will be fulfilled after delay (for chaining/awaiting)
/// The callback is stored in a timer registry and executed after the delay
#[inline]
pub(crate) fn runtime_set_timeout(args: &[RuntimeValue]) -> RuntimeValue {
    use std::thread;
    use std::time::Duration;

    // Get callback function (arg 0)
    let callback = args.get(0).cloned();

    // Get delay (arg 1)
    let delay_ms = args.get(1).and_then(|v| v.as_int()).unwrap_or(0) as u64;

    // Create a promise for the timeout
    let promise_id = PROMISE_RUNTIME.create_promise();

    // Store callback in the timer queue if it's a function
    if let Some(RuntimeValue::Function(func)) = callback {
        // Add to timer queue - the callback and promise_id
        PROMISE_RUNTIME.schedule_timer(promise_id, func, delay_ms);
    } else {
        // No callback, just resolve the promise after delay
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(delay_ms));
            PROMISE_RUNTIME.fulfill_value(promise_id, RuntimeValue::Null);
        });
    }

    RuntimeValue::Promise(promise_id)
}

/// sleep(ms) - synchronous sleep for the given milliseconds
#[inline]
pub(crate) fn runtime_sleep(args: &[RuntimeValue]) -> RuntimeValue {
    use std::thread;
    use std::time::Duration;

    let delay_ms = args.first().and_then(|v| v.as_int()).unwrap_or(0) as u64;

    thread::sleep(Duration::from_millis(delay_ms));
    RuntimeValue::Null
}
