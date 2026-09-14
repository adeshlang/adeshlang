//! Async Runtime Context
//!
//! Contains state for async operations including promises, timers,
//! and microtask queue management.

use crate::parsing::ast::{InterpreterEnv, NativeEffect, Value};
use crate::utils::timer::TimerManager;
use rustc_hash::FxHashMap as HashMap;
use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, mpsc};

// Use promise types from the promises module (they're private to runtime_core)
use super::super::promises::PromiseEntry;

/// Entry for a timer (setTimeout/setInterval)
#[derive(Clone)]
pub struct TimerEntry {
    pub callback: Value,
    pub cancel_flag: Arc<AtomicBool>,
    pub is_interval: bool,
}

/// Async runtime context for promises, timers, and microtasks.
///
/// Manages asynchronous execution including:
/// - Promise state and handlers
/// - Timer scheduling and callbacks
/// - Microtask queue for promise resolution
pub struct AsyncContext {
    /// Microtask queue (promises, timers)
    pub(crate) microtasks: VecDeque<Box<dyn FnOnce(&mut dyn InterpreterEnv) + 'static>>,

    /// Timer event receiver
    pub(crate) timer_rx: Option<mpsc::Receiver<u64>>,

    /// Timer manager for scheduling
    pub(crate) timer_manager: Option<Arc<TimerManager>>,

    /// Active timers registry
    pub(crate) timer_entries: HashMap<u64, TimerEntry>,

    /// Next timer ID
    pub(crate) next_timer_id: u64,

    /// Promise table
    pub(crate) promises: HashMap<u64, PromiseEntry>,

    /// Native side effects queue
    pub(crate) native_side_effects: Arc<Mutex<Vec<NativeEffect>>>,
}

impl AsyncContext {
    /// Creates a new async context
    pub fn new() -> Self {
        Self {
            microtasks: VecDeque::new(),
            timer_rx: None,
            timer_manager: None,
            timer_entries: HashMap::default(),
            next_timer_id: 0,
            promises: HashMap::default(),
            native_side_effects: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Gets the next timer ID and increments counter
    #[inline]
    pub fn next_timer_id(&mut self) -> u64 {
        let id = self.next_timer_id;
        self.next_timer_id += 1;
        id
    }

    /// Queues a microtask
    #[inline]
    pub fn queue_microtask(&mut self, task: Box<dyn FnOnce(&mut dyn InterpreterEnv) + 'static>) {
        self.microtasks.push_back(task);
    }

    /// Checks if there are pending microtasks
    #[inline]
    pub fn has_microtasks(&self) -> bool {
        !self.microtasks.is_empty()
    }

    /// Gets a reference to native side effects
    #[inline]
    pub fn native_side_effects(&self) -> &Arc<Mutex<Vec<NativeEffect>>> {
        &self.native_side_effects
    }

    /// Get timer entry count for debugging
    #[allow(dead_code)]
    pub(crate) fn timer_entry_count(&self) -> usize {
        self.timer_entries.len()
    }

    /// Get promise count for debugging
    #[allow(dead_code)]
    pub(crate) fn promise_count(&self) -> usize {
        self.promises.len()
    }

    /// Check if timer manager is configured
    #[allow(dead_code)]
    pub(crate) fn has_timer_manager(&self) -> bool {
        self.timer_manager.is_some()
    }

    /// Check if timer receiver is configured
    #[allow(dead_code)]
    pub(crate) fn has_timer_receiver(&self) -> bool {
        self.timer_rx.is_some()
    }
}

impl Default for AsyncContext {
    fn default() -> Self {
        Self::new()
    }
}
