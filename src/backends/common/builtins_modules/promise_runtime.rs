//! Promise Runtime Infrastructure (Optimized with Arc)

use crate::utils::collections::FastMap;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use super::types::{CallableFunction, PromiseId, RuntimeValue};

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
