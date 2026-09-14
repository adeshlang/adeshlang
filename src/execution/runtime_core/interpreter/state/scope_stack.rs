//! Scope Stack Management
//!
//! This module provides a focused API for managing the scope hierarchy
//! in the interpreter. It handles scope allocation, variable storage,
//! and scope recycling for memory efficiency.
//!
//! ## Key Features
//!
//! - **Scope Hierarchy**: Parent-child scope relationships
//! - **Memory Efficiency**: Scope recycling via free list
//! - **Variable Management**: Declare, set, and lookup variables
//! - **Export Support**: Module export tracking
//! - **Const Tracking**: Immutable variable enforcement
//! - **Ownership Tracking**: Memory safety with borrow checker
//! - **Defer Statements**: LIFO defer execution
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crate::execution::runtime_core::interpreter::state::ScopeStack;
//! use crate::parsing::ast::Value;
//!
//! let mut scopes = ScopeStack::new();
//!
//! // Create global scope
//! let global = scopes.push(None).unwrap();
//!
//! // Declare variable
//! scopes.declare(global, "x".to_string(), Value::Number(42.0)).unwrap();
//!
//! // Create nested scope
//! let local = scopes.push(Some(global)).unwrap();
//!
//! // Lookup variable (walks scope chain)
//! let value = scopes.get(local, "x");
//! assert_eq!(value, Some(&Value::Number(42.0)));
//!
//! // Pop scope (returns to free list)
//! scopes.pop(local);
//! ```

use crate::parsing::ast::{Stmt, Value};
use crate::utils::memory::OwnershipTracker;
use rustc_hash::FxHashMap as HashMap;
use std::rc::Rc;

/// Environment (scope) for variable storage.
///
///  Most scopes only use values + enclosing; other fields use lazy initialization.
#[derive(Clone)]
pub struct Scope {
    pub values: HashMap<String, Value>,
    pub enclosing: Option<usize>,
    pub exports: HashMap<String, Value>,
    pub consts: HashMap<String, bool>,
    pub type_ann: HashMap<String, Option<String>>,
    pub ownership: HashMap<String, Rc<OwnershipTracker>>,
    pub defers: Vec<Box<Stmt>>, // Defer stack for LIFO execution
}

impl Scope {
    /// Create a new scope with optional parent.
    #[inline]
    fn new(enclosing: Option<usize>) -> Self {
        Self {
            values: HashMap::default(),
            enclosing,
            exports: HashMap::default(),
            consts: HashMap::default(),
            type_ann: HashMap::default(),
            ownership: HashMap::default(),
            defers: Vec::new(),
        }
    }

    /// Reset the scope for reuse (scope recycling).
    #[inline]
    fn reset(&mut self, enclosing: Option<usize>) {
        self.values.clear();
        self.enclosing = enclosing;
        self.exports.clear();
        self.consts.clear();
        self.type_ann.clear();
        self.ownership.clear();
        self.defers.clear();
    }
}

/// Manages the scope hierarchy with memory-efficient recycling.
///
/// Uses a free list to recycle scopes, avoiding repeated allocations.
pub struct ScopeStack {
    scopes: Vec<Scope>,
    free_list: Vec<usize>,
}

impl ScopeStack {
    /// Create a new empty scope stack.
    #[inline]
    pub fn new() -> Self {
        Self {
            scopes: Vec::new(),
            free_list: Vec::new(),
        }
    }

    /// Create a scope stack with pre-allocated capacity.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            scopes: Vec::with_capacity(capacity),
            free_list: Vec::new(),
        }
    }

    /// Push a new scope onto the stack.
    ///
    /// Returns the scope ID. Uses recycled scope if available, otherwise allocates new.
    ///
    /// # Arguments
    ///
    /// * `parent` - Optional parent scope ID for scope chaining
    ///
    /// # Returns
    ///
    /// Scope ID that can be used for variable operations
    #[inline]
    pub fn push(&mut self, parent: Option<usize>) -> usize {
        if let Some(id) = self.free_list.pop() {
            // Reuse recycled scope
            self.scopes[id].reset(parent);
            id
        } else {
            // Allocate new scope
            let id = self.scopes.len();
            self.scopes.push(Scope::new(parent));
            id
        }
    }

    /// Pop a scope from the stack, returning it to the free list.
    ///
    /// # Arguments
    ///
    /// * `scope_id` - The scope ID to release
    ///
    /// # Note
    ///
    /// The scope is not immediately freed; it's returned to the free list
    /// for future reuse. This is more efficient than repeated allocations.
    #[inline]
    pub fn pop(&mut self, scope_id: usize) {
        if scope_id < self.scopes.len() {
            self.free_list.push(scope_id);
        }
    }

    /// Get immutable reference to a scope.
    ///
    /// # Arguments
    ///
    /// * `scope_id` - The scope ID
    ///
    /// # Returns
    ///
    /// Optional reference to the scope (None if invalid ID)
    #[inline]
    pub fn get(&self, scope_id: usize) -> Option<&Scope> {
        self.scopes.get(scope_id)
    }

    /// Get mutable reference to a scope.
    ///
    /// # Arguments
    ///
    /// * `scope_id` - The scope ID
    ///
    /// # Returns
    ///
    /// Optional mutable reference to the scope (None if invalid ID)
    #[inline]
    pub fn get_mut(&mut self, scope_id: usize) -> Option<&mut Scope> {
        self.scopes.get_mut(scope_id)
    }

    /// Declare a new variable in the given scope.
    ///
    /// # Arguments
    ///
    /// * `scope_id` - The scope to declare in
    /// * `name` - Variable name
    /// * `value` - Initial value
    ///
    /// # Returns
    ///
    /// Ok(()) on success, Err if scope doesn't exist
    #[inline]
    pub fn declare(&mut self, scope_id: usize, name: String, value: Value) -> Result<(), String> {
        if let Some(scope) = self.scopes.get_mut(scope_id) {
            scope.values.insert(name, value);
            Ok(())
        } else {
            Err(format!("Invalid scope ID: {}", scope_id))
        }
    }

    /// Set a variable in the scope chain.
    ///
    /// Walks up the scope chain to find the variable, then sets it.
    ///
    /// # Arguments
    ///
    /// * `scope_id` - Starting scope for lookup
    /// * `name` - Variable name
    /// * `value` - New value
    ///
    /// # Returns
    ///
    /// Ok(()) on success, Err if variable not found or scope invalid
    pub fn set(&mut self, scope_id: usize, name: &str, value: Value) -> Result<(), String> {
        let mut current = Some(scope_id);

        while let Some(id) = current {
            if id >= self.scopes.len() {
                return Err(format!("Invalid scope ID: {}", id));
            }

            if self.scopes[id].values.contains_key(name) {
                self.scopes[id].values.insert(name.to_string(), value);
                return Ok(());
            }

            current = self.scopes[id].enclosing;
        }

        Err(format!("Variable '{}' not found in scope chain", name))
    }

    /// Get a variable from the scope chain.
    ///
    /// Walks up the scope chain to find the variable.
    ///
    /// # Arguments
    ///
    /// * `scope_id` - Starting scope for lookup
    /// * `name` - Variable name
    ///
    /// # Returns
    ///
    /// Optional reference to the value (None if not found)
    pub fn get_variable(&self, scope_id: usize, name: &str) -> Option<&Value> {
        let mut current = Some(scope_id);

        while let Some(id) = current {
            if id >= self.scopes.len() {
                return None;
            }

            if let Some(value) = self.scopes[id].values.get(name) {
                return Some(value);
            }

            current = self.scopes[id].enclosing;
        }

        None
    }

    /// Check if a variable exists in the scope chain.
    ///
    /// # Arguments
    ///
    /// * `scope_id` - Starting scope for lookup
    /// * `name` - Variable name
    ///
    /// # Returns
    ///
    /// true if the variable exists, false otherwise
    #[inline]
    pub fn has_variable(&self, scope_id: usize, name: &str) -> bool {
        self.get_variable(scope_id, name).is_some()
    }

    /// Get the number of allocated scopes (including free list).
    #[inline]
    pub fn len(&self) -> usize {
        self.scopes.len()
    }

    /// Check if the scope stack is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.scopes.is_empty()
    }

    /// Get the number of scopes in the free list (available for reuse).
    #[inline]
    pub fn free_count(&self) -> usize {
        self.free_list.len()
    }

    /// Get direct access to the internal scopes vector.
    ///
    /// This is provided for compatibility with existing code during migration.
    ///
    /// # Safety
    ///
    /// Direct access bypasses scope stack invariants. Use with caution.
    #[inline]
    pub fn as_slice(&self) -> &[Scope] {
        &self.scopes
    }

    /// Get direct mutable access to the internal scopes vector.
    ///
    /// This is provided for compatibility with existing code during migration.
    ///
    /// # Safety
    ///
    /// Direct access bypasses scope stack invariants. Use with caution.
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut Vec<Scope> {
        &mut self.scopes
    }
}

impl Default for ScopeStack {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_creation() {
        let mut stack = ScopeStack::new();
        let global = stack.push(None);
        assert_eq!(global, 0);
        assert_eq!(stack.len(), 1);
    }

    #[test]
    fn test_variable_declaration() {
        let mut stack = ScopeStack::new();
        let scope = stack.push(None);

        stack
            .declare(scope, "x".to_string(), Value::Number(42.0))
            .unwrap();

        let value = stack.get_variable(scope, "x");
        assert!(matches!(value, Some(&Value::Number(n)) if n == 42.0));
    }

    #[test]
    fn test_scope_chain_lookup() {
        let mut stack = ScopeStack::new();

        // Create global scope with variable
        let global = stack.push(None);
        stack
            .declare(global, "x".to_string(), Value::Number(10.0))
            .unwrap();

        // Create local scope
        let local = stack.push(Some(global));

        // Lookup should find variable in parent scope
        let value = stack.get_variable(local, "x");
        assert!(matches!(value, Some(&Value::Number(n)) if n == 10.0));
    }

    #[test]
    fn test_scope_recycling() {
        let mut stack = ScopeStack::new();

        // Allocate and release a scope
        let scope1 = stack.push(None);
        stack.pop(scope1);
        assert_eq!(stack.free_count(), 1);

        // Next push should reuse the scope
        let scope2 = stack.push(None);
        assert_eq!(scope2, scope1);
        assert_eq!(stack.free_count(), 0);
    }

    #[test]
    fn test_variable_shadowing() {
        let mut stack = ScopeStack::new();

        // Global x
        let global = stack.push(None);
        stack
            .declare(global, "x".to_string(), Value::Number(10.0))
            .unwrap();

        // Local x shadows global
        let local = stack.push(Some(global));
        stack
            .declare(local, "x".to_string(), Value::Number(20.0))
            .unwrap();

        // Local lookup finds local x
        let value = stack.get_variable(local, "x");
        assert!(matches!(value, Some(&Value::Number(n)) if n == 20.0));

        // Global lookup finds global x
        let value = stack.get_variable(global, "x");
        assert!(matches!(value, Some(&Value::Number(n)) if n == 10.0));
    }
}
