//! Execution Context
//!
//! Contains core execution state for the language interpreter including
//! scope management, call stack tracking, and module context.

use super::super::Env;
use crate::parsing::error::LangError;

/// Core execution state for the interpreter.
///
/// Manages environment scopes, call stack, and module context.
/// Separated from Interpreter to allow focused testing and clearer ownership.
pub struct ExecutionContext {
    /// Environment scopes (variable bindings organized hierarchically)
    pub(crate) envs: Vec<Env>,

    /// Free list for scope recycling (memory optimization)
    pub(crate) free_envs: Vec<usize>,

    /// Global scope index
    pub(crate) global: usize,

    /// Current module being executed
    pub(crate) current_module: Option<String>,

    /// Current class context for visibility checking
    pub(crate) current_class_context: Option<String>,

    /// Pending throw error for exception handling
    pub(crate) pending_throw: Option<LangError>,

    /// Track if in pre-registration phase (decorator system)
    pub(crate) in_pre_registration: bool,

    /// Call stack depth tracking (debug builds only)
    #[cfg(debug_assertions)]
    pub(crate) exec_depth: usize,

    /// Call stack for debugging (debug builds only)
    #[cfg(debug_assertions)]
    pub(crate) call_stack: Vec<String>,
}

impl ExecutionContext {
    /// Creates a new execution context with default state
    pub fn new() -> Self {
        Self {
            envs: Vec::new(),
            free_envs: Vec::new(),
            global: 0,
            current_module: None,
            current_class_context: None,
            pending_throw: None,
            in_pre_registration: false,
            #[cfg(debug_assertions)]
            exec_depth: 0,
            #[cfg(debug_assertions)]
            call_stack: Vec::new(),
        }
    }

    /// Gets the global scope index
    #[inline]
    pub fn global(&self) -> usize {
        self.global
    }

    /// Sets the global scope index
    #[inline]
    pub fn set_global(&mut self, global: usize) {
        self.global = global;
    }

    /// Gets the current module name
    #[inline]
    pub fn current_module(&self) -> Option<&str> {
        self.current_module.as_deref()
    }

    /// Sets the current module name
    #[inline]
    pub fn set_current_module(&mut self, module: Option<String>) {
        self.current_module = module;
    }

    /// Gets the current class context
    #[inline]
    pub fn current_class_context(&self) -> Option<&str> {
        self.current_class_context.as_deref()
    }

    /// Sets the current class context
    #[inline]
    pub fn set_current_class_context(&mut self, context: Option<String>) {
        self.current_class_context = context;
    }

    /// Checks if a throw is pending
    #[inline]
    pub fn has_pending_throw(&self) -> bool {
        self.pending_throw.is_some()
    }

    /// Takes the pending throw error
    #[inline]
    pub fn take_pending_throw(&mut self) -> Option<LangError> {
        self.pending_throw.take()
    }

    /// Sets a pending throw error
    #[inline]
    pub fn set_pending_throw(&mut self, error: LangError) {
        self.pending_throw = Some(error);
    }

    /// Clears the pending throw
    #[inline]
    pub fn clear_pending_throw(&mut self) {
        self.pending_throw = None;
    }

    /// Checks if in pre-registration phase
    #[inline]
    pub fn in_pre_registration(&self) -> bool {
        self.in_pre_registration
    }

    /// Sets pre-registration phase flag
    #[inline]
    pub fn set_pre_registration(&mut self, value: bool) {
        self.in_pre_registration = value;
    }

    /// Gets current execution depth (debug builds only)
    #[cfg(debug_assertions)]
    #[inline]
    pub fn exec_depth(&self) -> usize {
        self.exec_depth
    }

    /// Increments execution depth (debug builds only)
    #[cfg(debug_assertions)]
    #[inline]
    pub fn increment_depth(&mut self) {
        self.exec_depth = self.exec_depth.saturating_add(1);
    }

    /// Decrements execution depth (debug builds only)
    #[cfg(debug_assertions)]
    #[inline]
    pub fn decrement_depth(&mut self) {
        self.exec_depth = self.exec_depth.saturating_sub(1);
    }

    /// Pushes a call onto the call stack (debug builds only)
    #[cfg(debug_assertions)]
    #[inline]
    pub fn push_call(&mut self, name: String) {
        self.call_stack.push(name);
    }

    /// Pops a call from the call stack (debug builds only)
    #[cfg(debug_assertions)]
    #[inline]
    pub fn pop_call(&mut self) -> Option<String> {
        self.call_stack.pop()
    }

    /// Gets a reference to the call stack (debug builds only)
    #[cfg(debug_assertions)]
    #[inline]
    pub fn call_stack(&self) -> &[String] {
        &self.call_stack
    }

    /// Get the number of active environments for debugging
    #[allow(dead_code)]
    pub(crate) fn env_count(&self) -> usize {
        self.envs.len()
    }

    /// Get the number of free environment slots for debugging
    #[allow(dead_code)]
    pub(crate) fn free_env_count(&self) -> usize {
        self.free_envs.len()
    }
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self::new()
    }
}
