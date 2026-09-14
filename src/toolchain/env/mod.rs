//! Builtin Environment Trait
//!
//! Defines the interface used by native builtins to interact with the
//! interpreter event loop: promises, resolve/reject scheduling, and timers.
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use crate::parsing::ast::NativeEffect;
use crate::parsing::ast::Value;

/// Trait for builtin environment operations, used by native functions
/// to interact with the interpreter's event loop, timers, promises, etc.
pub trait BuiltinEnv {
    fn native_side_effects(&self) -> Option<Arc<Mutex<Vec<NativeEffect>>>>;
    // Create a promise with the provided executor. Returns a Value::Promise(id)
    fn create_promise_executor(&mut self, executor: Value) -> Result<Value, String>;
    // schedule resolve/reject for a promise id from this env
    fn schedule_resolve(&mut self, id: u64, v: Value);
    fn schedule_reject(&mut self, id: u64, v: Value);
    // timers API (may be unsupported in some envs)
    fn alloc_timer_id(&mut self) -> Result<u64, String>;
    fn get_timer_sender(&self) -> Option<mpsc::Sender<u64>>;
    fn register_timer(
        &mut self,
        id: u64,
        callback: Value,
        cancel_flag: Arc<std::sync::atomic::AtomicBool>,
        is_interval: bool,
        ms: u64,
    ) -> Result<(), String>;
    fn cancel_timer(&mut self, id: u64) -> Result<(), String>;
    // (event loop driver exposed as an inherent method on Interpreter)
}
