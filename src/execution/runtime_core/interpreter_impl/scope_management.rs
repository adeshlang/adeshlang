//! Scope Management Module
//!
//! Handles scope hierarchy, variable declarations, and scope chain resolution.
//!
//! This module is extracted from the monolithic interpreter_core.rs to improve
//! code organization and maintainability.
//!
//! # Overview
//!
//! The language uses a scope chain model similar to JavaScript and Python. Each scope
//! (environment) contains:
//! - Variable bindings (name -> value)
//! - Constant flags (name -> is_const)
//! - Type annotations (name -> type)
//! - Ownership trackers for non-Copy values
//! - Defer statements (executed in LIFO order on scope exit)
//!
//! # Scope Lifecycle
//!
//! 1. **Acquisition**: Scopes are acquired via `acquire_scope()`
//!    - Recycled from free list if available
//!    - Created new if free list is empty
//!
//! 2. **Usage**: Variables are declared and looked up
//!    - `define_at()` declares mutable variables
//!    - `define_at_const()` declares const/let variables
//!    - `get()`, `get_fast()` perform scope chain lookups
//!
//! 3. **Release**: Scopes are released via `release_scope()`
//!    - Cleared and returned to free list
//!    - Global scope (index 0) is never released
//!
//! # Lookup Strategy
//!
//! Variable lookup follows the scope chain from current to global:
//! ```text
//! Current Scope -> Parent Scope -> ... -> Global Scope
//! ```
//!
//! Lookup stops at the first scope containing the variable.
//! If no scope contains the variable, lookup returns None.
//!
//! # Performance Optimizations
//!
//! - **Scope Recycling**: Free list prevents repeated allocations
//! - **Inline Lookups**: `get()` and `get_fast()` are inlined
//! - **Depth Limits**: MAX_LOOKUP_DEPTH prevents infinite loops
//! - **Fast Hash**: Uses FxHashMap for O(1) average lookup
//!
//! # Memory Safety
//!
//! - Non-Copy values have ownership trackers (Rc<OwnershipTracker>)
//! - Move semantics enforced at runtime
//! - Ownership transferred on assignment/move
//! - Copy values (int, bool) have no ownership tracking overhead
//!
//! # Implementation Notes
//!
//! The scope management methods remain in interpreter_core.rs due to tight
//! coupling with the Interpreter struct. This module provides documentation
//! and conceptual organization.
//!
//! For actual implementation, see:
//! - `Interpreter::acquire_scope()` (~line 1097)
//! - `Interpreter::release_scope()` (~line 1112)
//! - `Interpreter::define_at()` (~line 3280)
//! - `Interpreter::define_at_const()` (~line 3294)
//! - `Interpreter::get()` (~line 3330)
//! - `Interpreter::get_fast()` (~line 3317)
//! - `Interpreter::get_tracker()` (~line 3351)
//! - `Interpreter::get_with_env()` (~line 3370)

use crate::execution::runtime_core::interpreter::env::Env;
use crate::parsing::ast::Value;

/// Maximum depth for scope chain lookups.
///
/// Prevents infinite loops from circular scope chains (should never happen
/// but provides safety). Most real code has chain depth < 10.
pub const MAX_LOOKUP_DEPTH: usize = 100;

/// Maximum depth for scope chain traversal during captures.
///
/// Used when capturing environment snapshots for closures.
pub const MAX_CHAIN_DEPTH: usize = 100;

/// Helper function to check if a value is Copy (doesn't need ownership tracking).
///
/// Copy values include:
/// - Null
/// - Bool
/// - Number (f64)
/// - Char
/// - Fixed-width numeric types (U8, I32, F32, etc.)
///
/// Non-copy values (need ownership tracking):
/// - Str (heap-allocated)
/// - Array (heap-allocated)
/// - Object (heap-allocated)
/// - Functions (may have captured state)
/// - Instances (heap-allocated fields)
#[inline]
pub fn is_copy_value(v: &Value) -> bool {
    matches!(
        v,
        Value::Null
            | Value::Bool(_)
            | Value::Number(_)
            | Value::Char(_)
            | Value::U8(_)
            | Value::U16(_)
            | Value::U32(_)
            | Value::U64(_)
            | Value::U128(_)
            | Value::I8(_)
            | Value::I16(_)
            | Value::I32(_)
            | Value::I64(_)
            | Value::I128(_)
            | Value::F32(_)
            | Value::F64(_)
            // Enum constructors (like Some, None) are copyable
            | Value::EnumCtor(_, _)
    )
}

/// Scope chain walker for debugging.
///
/// Returns a vector of scope indices from current to global.
/// Useful for debugging scope issues and understanding closure captures.
pub fn walk_scope_chain(envs: &[Env], start: usize) -> Vec<usize> {
    let mut chain = Vec::new();
    let mut current = Some(start);
    let mut depth = 0;

    while let Some(idx) = current {
        chain.push(idx);
        if idx < envs.len() {
            current = envs[idx].enclosing;
        } else {
            break;
        }
        depth += 1;
        if depth > MAX_LOOKUP_DEPTH {
            eprintln!("[WARNING] Scope chain depth exceeded {}", MAX_LOOKUP_DEPTH);
            break;
        }
    }

    chain
}

/// Scope statistics for monitoring and debugging.
///
/// Tracks scope usage metrics for performance tuning.
#[derive(Debug, Default)]
pub struct ScopeStats {
    /// Total number of scopes created
    pub total_created: usize,
    /// Current number of active scopes
    pub active_scopes: usize,
    /// Number of scopes in free list
    pub free_scopes: usize,
    /// Peak number of active scopes
    pub peak_scopes: usize,
    /// Number of scope recycling hits
    pub recycle_hits: usize,
}

impl ScopeStats {
    /// Create new scope statistics tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a scope acquisition.
    pub fn record_acquire(&mut self, from_free_list: bool) {
        if from_free_list {
            self.recycle_hits += 1;
            if self.free_scopes > 0 {
                self.free_scopes -= 1;
            }
        } else {
            self.total_created += 1;
        }
        self.active_scopes += 1;
        if self.active_scopes > self.peak_scopes {
            self.peak_scopes = self.active_scopes;
        }
    }

    /// Record a scope release.
    pub fn record_release(&mut self) {
        if self.active_scopes > 0 {
            self.active_scopes -= 1;
        }
        self.free_scopes += 1;
    }

    /// Calculate recycling efficiency (0.0 to 1.0).
    ///
    /// Returns the ratio of recycled scopes to total scope acquisitions.
    /// Higher values indicate better memory reuse.
    pub fn recycle_efficiency(&self) -> f64 {
        let total_acquisitions = self.total_created + self.recycle_hits;
        if total_acquisitions == 0 {
            return 0.0;
        }
        self.recycle_hits as f64 / total_acquisitions as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_copy_value() {
        assert!(is_copy_value(&Value::Null));
        assert!(is_copy_value(&Value::Bool(true)));
        assert!(is_copy_value(&Value::Number(42.0)));
        assert!(is_copy_value(&Value::Char('a')));
        assert!(is_copy_value(&Value::U8(255)));
        assert!(is_copy_value(&Value::I32(-42)));
        assert!(is_copy_value(&Value::F32(3.14)));
        assert!(!is_copy_value(&Value::Str("hello".into())));
    }

    #[test]
    fn test_scope_stats() {
        let mut stats = ScopeStats::new();

        // Acquire from new
        stats.record_acquire(false);
        assert_eq!(stats.total_created, 1);
        assert_eq!(stats.active_scopes, 1);

        // Release
        stats.record_release();
        assert_eq!(stats.active_scopes, 0);
        assert_eq!(stats.free_scopes, 1);

        // Acquire from free list
        stats.record_acquire(true);
        assert_eq!(stats.recycle_hits, 1);
        assert_eq!(stats.free_scopes, 0);
    }
}
