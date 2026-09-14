//! Closure Management
//!
//! This module provides APIs for managing closures, including capture
//! analysis, closure creation, and environment binding. It helps ensure
//! that closures properly capture their lexical environment.
//!
//! ## Key Features
//!
//! - **Capture Analysis**: Identifies variables captured by closures
//! - **Environment Binding**: Associates closures with their capture environments
//! - **Scope Preservation**: Ensures captured variables remain accessible
//! - **Capture Metadata**: Tracks what variables are captured and how
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crate::execution::runtime_core::interpreter::state::ClosureManager;
//! use crate::parsing::ast::Value;
//!
//! let mut closures = ClosureManager::new();
//!
//! // Register a closure with its capture environment
//! closures.register_closure(
//!     "my_closure",
//!     42, // closure scope ID
//!     vec!["x".to_string(), "y".to_string()], // captured variables
//! );
//!
//! // Check if variable is captured
//! assert!(closures.is_captured("my_closure", "x"));
//! ```

use rustc_hash::FxHashMap as HashMap;

/// Information about a closure's captures.
#[derive(Clone, Debug)]
pub struct ClosureInfo {
    /// The scope ID where the closure is defined
    pub closure_scope: usize,
    /// Names of captured variables
    pub captured_vars: Vec<String>,
    /// Optional name for debugging
    pub name: Option<String>,
}

impl ClosureInfo {
    /// Create new closure info.
    ///
    /// # Arguments
    ///
    /// * `closure_scope` - The scope ID where the closure is defined
    /// * `captured_vars` - Names of captured variables
    #[inline]
    pub fn new(closure_scope: usize, captured_vars: Vec<String>) -> Self {
        Self {
            closure_scope,
            captured_vars,
            name: None,
        }
    }

    /// Create closure info with a name.
    ///
    /// # Arguments
    ///
    /// * `name` - Optional closure name for debugging
    /// * `closure_scope` - The scope ID where the closure is defined
    /// * `captured_vars` - Names of captured variables
    #[inline]
    pub fn with_name(name: String, closure_scope: usize, captured_vars: Vec<String>) -> Self {
        Self {
            closure_scope,
            captured_vars,
            name: Some(name),
        }
    }

    /// Check if a variable is captured by this closure.
    ///
    /// # Arguments
    ///
    /// * `var_name` - Variable name to check
    ///
    /// # Returns
    ///
    /// true if the variable is captured, false otherwise
    #[inline]
    pub fn captures(&self, var_name: &str) -> bool {
        self.captured_vars.iter().any(|v| v == var_name)
    }

    /// Get the number of captured variables.
    #[inline]
    pub fn capture_count(&self) -> usize {
        self.captured_vars.len()
    }
}

/// Manages closure metadata and capture analysis.
///
/// Tracks which variables are captured by closures to ensure proper
/// environment preservation and access.
pub struct ClosureManager {
    /// Map from closure ID to closure information
    closures: HashMap<String, ClosureInfo>,
    /// Next closure ID for anonymous closures
    next_id: usize,
}

impl ClosureManager {
    /// Create a new closure manager.
    #[inline]
    pub fn new() -> Self {
        Self {
            closures: HashMap::default(),
            next_id: 0,
        }
    }

    /// Register a closure with its capture information.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier for the closure
    /// * `closure_scope` - The scope ID where the closure is defined
    /// * `captured_vars` - Names of captured variables
    ///
    /// # Returns
    ///
    /// The closure ID
    pub fn register_closure(
        &mut self,
        id: &str,
        closure_scope: usize,
        captured_vars: Vec<String>,
    ) -> String {
        let info = ClosureInfo::new(closure_scope, captured_vars);
        self.closures.insert(id.to_string(), info);
        id.to_string()
    }

    /// Register a named closure with its capture information.
    ///
    /// # Arguments
    ///
    /// * `name` - Closure name for debugging
    /// * `closure_scope` - The scope ID where the closure is defined
    /// * `captured_vars` - Names of captured variables
    ///
    /// # Returns
    ///
    /// The closure ID
    pub fn register_named_closure(
        &mut self,
        name: String,
        closure_scope: usize,
        captured_vars: Vec<String>,
    ) -> String {
        let info = ClosureInfo::with_name(name.clone(), closure_scope, captured_vars);
        self.closures.insert(name.clone(), info);
        name
    }

    /// Register an anonymous closure.
    ///
    /// Generates a unique ID for the closure.
    ///
    /// # Arguments
    ///
    /// * `closure_scope` - The scope ID where the closure is defined
    /// * `captured_vars` - Names of captured variables
    ///
    /// # Returns
    ///
    /// The generated closure ID
    pub fn register_anonymous_closure(
        &mut self,
        closure_scope: usize,
        captured_vars: Vec<String>,
    ) -> String {
        let id = format!("__closure_{}", self.next_id);
        self.next_id += 1;
        self.register_closure(&id, closure_scope, captured_vars);
        id
    }

    /// Get closure information by ID.
    ///
    /// # Arguments
    ///
    /// * `id` - Closure ID
    ///
    /// # Returns
    ///
    /// Optional reference to closure info
    #[inline]
    pub fn get(&self, id: &str) -> Option<&ClosureInfo> {
        self.closures.get(id)
    }

    /// Check if a variable is captured by a closure.
    ///
    /// # Arguments
    ///
    /// * `id` - Closure ID
    /// * `var_name` - Variable name to check
    ///
    /// # Returns
    ///
    /// true if the variable is captured, false otherwise
    pub fn is_captured(&self, id: &str, var_name: &str) -> bool {
        self.closures
            .get(id)
            .map(|info| info.captures(var_name))
            .unwrap_or(false)
    }

    /// Get the closure scope for a closure.
    ///
    /// # Arguments
    ///
    /// * `id` - Closure ID
    ///
    /// # Returns
    ///
    /// Optional closure scope ID
    #[inline]
    pub fn get_closure_scope(&self, id: &str) -> Option<usize> {
        self.closures.get(id).map(|info| info.closure_scope)
    }

    /// Get the list of captured variables for a closure.
    ///
    /// # Arguments
    ///
    /// * `id` - Closure ID
    ///
    /// # Returns
    ///
    /// Optional reference to the list of captured variable names
    #[inline]
    pub fn get_captured_vars(&self, id: &str) -> Option<&[String]> {
        self.closures
            .get(id)
            .map(|info| info.captured_vars.as_slice())
    }

    /// Remove a closure from tracking.
    ///
    /// # Arguments
    ///
    /// * `id` - Closure ID
    ///
    /// # Returns
    ///
    /// The removed closure info, if it existed
    #[inline]
    pub fn remove(&mut self, id: &str) -> Option<ClosureInfo> {
        self.closures.remove(id)
    }

    /// Clear all closure tracking.
    ///
    /// This is useful for resetting state in REPL or test scenarios.
    #[inline]
    pub fn clear(&mut self) {
        self.closures.clear();
        self.next_id = 0;
    }

    /// Get the number of tracked closures.
    #[inline]
    pub fn len(&self) -> usize {
        self.closures.len()
    }

    /// Check if no closures are being tracked.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.closures.is_empty()
    }

    /// Analyze captured variables in a closure.
    ///
    /// This is a helper method that can be used during AST traversal
    /// to identify which variables from outer scopes are referenced.
    ///
    /// # Note
    ///
    /// This is a simplified API. Full capture analysis would require
    /// AST traversal which is beyond the scope of this module.
    ///
    /// # Arguments
    ///
    /// * `referenced_vars` - Set of all variable names referenced in closure body
    /// * `declared_vars` - Set of all variable names declared in closure body
    ///
    /// # Returns
    ///
    /// List of captured variable names (referenced but not declared)
    pub fn analyze_captures(
        &self,
        referenced_vars: &[String],
        declared_vars: &[String],
    ) -> Vec<String> {
        referenced_vars
            .iter()
            .filter(|var| !declared_vars.contains(var))
            .cloned()
            .collect()
    }
}

impl Default for ClosureManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_closure_registration() {
        let mut mgr = ClosureManager::new();

        let id = mgr.register_closure("my_closure", 42, vec!["x".to_string(), "y".to_string()]);

        assert_eq!(id, "my_closure");
        assert!(mgr.is_captured("my_closure", "x"));
        assert!(mgr.is_captured("my_closure", "y"));
        assert!(!mgr.is_captured("my_closure", "z"));
    }

    #[test]
    fn test_anonymous_closure() {
        let mut mgr = ClosureManager::new();

        let id1 = mgr.register_anonymous_closure(10, vec!["a".to_string()]);
        let id2 = mgr.register_anonymous_closure(20, vec!["b".to_string()]);

        assert_ne!(id1, id2);
        assert!(mgr.is_captured(&id1, "a"));
        assert!(mgr.is_captured(&id2, "b"));
    }

    #[test]
    fn test_closure_scope() {
        let mut mgr = ClosureManager::new();

        mgr.register_closure("fn1", 100, vec![]);
        mgr.register_closure("fn2", 200, vec![]);

        assert_eq!(mgr.get_closure_scope("fn1"), Some(100));
        assert_eq!(mgr.get_closure_scope("fn2"), Some(200));
        assert_eq!(mgr.get_closure_scope("fn3"), None);
    }

    #[test]
    fn test_captured_vars() {
        let mut mgr = ClosureManager::new();

        mgr.register_closure(
            "my_fn",
            50,
            vec!["x".to_string(), "y".to_string(), "z".to_string()],
        );

        let vars = mgr.get_captured_vars("my_fn").unwrap();
        assert_eq!(vars.len(), 3);
        assert!(vars.contains(&"x".to_string()));
        assert!(vars.contains(&"y".to_string()));
        assert!(vars.contains(&"z".to_string()));
    }

    #[test]
    fn test_named_closure() {
        let mut mgr = ClosureManager::new();

        let id =
            mgr.register_named_closure("my_named_fn".to_string(), 42, vec!["captured".to_string()]);

        let info = mgr.get(&id).unwrap();
        assert_eq!(info.name, Some("my_named_fn".to_string()));
        assert_eq!(info.closure_scope, 42);
        assert_eq!(info.capture_count(), 1);
    }

    #[test]
    fn test_analyze_captures() {
        let mgr = ClosureManager::new();

        let referenced = vec!["x".to_string(), "y".to_string(), "z".to_string()];
        let declared = vec!["z".to_string()];

        let captured = mgr.analyze_captures(&referenced, &declared);

        assert_eq!(captured.len(), 2);
        assert!(captured.contains(&"x".to_string()));
        assert!(captured.contains(&"y".to_string()));
        assert!(!captured.contains(&"z".to_string()));
    }

    #[test]
    fn test_remove_closure() {
        let mut mgr = ClosureManager::new();

        mgr.register_closure("temp", 10, vec![]);
        assert_eq!(mgr.len(), 1);

        let removed = mgr.remove("temp");
        assert!(removed.is_some());
        assert_eq!(mgr.len(), 0);
    }

    #[test]
    fn test_clear() {
        let mut mgr = ClosureManager::new();

        mgr.register_closure("fn1", 10, vec![]);
        mgr.register_closure("fn2", 20, vec![]);
        assert_eq!(mgr.len(), 2);

        mgr.clear();
        assert_eq!(mgr.len(), 0);
        assert!(mgr.is_empty());
    }
}
