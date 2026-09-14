//! AOT Backend Memory Tracking
//!
//! Provides RAII-based memory tracking for the AOT (Cranelift) backend.
//! Replaces direct malloc/free calls with tracked allocations that integrate
//! with AdeshLang's compile-time memory safety guarantees.
//!
//! # Design
//!
//! The AOT backend generates native code that includes:
//! 1. Allocation metadata tracking (size, type, status)
//! 2. Automatic cleanup on scope exit (RAII)
//! 3. Debug mode validation (double-free, use-after-free detection)
//! 4. Integration with ownership/borrow metadata from HIR
//!
//! # Architecture
//!
//! ```text
//! HIR (with ownership metadata)
//!   ↓
//! LIR (Alloc/Free instructions with metadata)
//!   ↓
//! AOT Code Generation
//!   ↓
//! Native Code with:
//!   - adesh_rt_alloc_tracked(size, metadata_ptr) -> ptr
//!   - adesh_rt_free_tracked(ptr, metadata_ptr)
//!   - adesh_rt_scope_exit(scope_id) -> cleans up all scope allocations
//! ```

use std::collections::HashMap;

/// Allocation metadata tracked during AOT compilation
#[derive(Debug, Clone)]
pub struct AllocationMetadata {
    /// Unique allocation ID
    pub id: u64,
    /// Size in bytes
    pub size: u64,
    /// Element size (for typed allocations)
    pub elem_size: Option<u64>,
    /// Source location for debugging
    pub source_location: SourceLocation,
    /// Scope ID (for RAII cleanup)
    pub scope_id: u64,
    /// Whether this allocation escapes the current function
    pub escapes: bool,
    /// Whether this is in an unsafe block
    pub is_unsafe: bool,
}

#[derive(Debug, Clone)]
pub struct SourceLocation {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

/// Tracks allocations during AOT compilation for RAII insertion
pub struct AotMemoryTracker {
    /// Allocations by value ID
    allocations: HashMap<u64, AllocationMetadata>,
    /// Current scope ID
    current_scope: u64,
    /// Scope stack for nested scopes
    scope_stack: Vec<ScopeInfo>,
    /// Next allocation ID
    next_alloc_id: u64,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ScopeInfo {
    scope_id: u64,
    allocations: Vec<u64>, // Allocation IDs in this scope
}

impl AotMemoryTracker {
    pub fn new() -> Self {
        AotMemoryTracker {
            allocations: HashMap::new(),
            current_scope: 0,
            scope_stack: vec![ScopeInfo {
                scope_id: 0,
                allocations: Vec::new(),
            }],
            next_alloc_id: 1,
        }
    }

    /// Enter a new scope
    pub fn enter_scope(&mut self) -> u64 {
        self.current_scope += 1;
        let scope_id = self.current_scope;
        self.scope_stack.push(ScopeInfo {
            scope_id,
            allocations: Vec::new(),
        });
        scope_id
    }

    /// Exit current scope and return allocation IDs that need cleanup
    pub fn exit_scope(&mut self) -> Vec<u64> {
        if let Some(scope) = self.scope_stack.pop() {
            // Return allocations that need cleanup (those that don't escape)
            scope
                .allocations
                .iter()
                .filter(|&&alloc_id| {
                    self.allocations
                        .get(&alloc_id)
                        .map(|meta| !meta.escapes)
                        .unwrap_or(false)
                })
                .copied()
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Register a new allocation
    pub fn register_allocation(
        &mut self,
        _value_id: u64,
        size: u64,
        elem_size: Option<u64>,
        source_location: SourceLocation,
        is_unsafe: bool,
    ) -> u64 {
        let alloc_id = self.next_alloc_id;
        self.next_alloc_id += 1;

        let metadata = AllocationMetadata {
            id: alloc_id,
            size,
            elem_size,
            source_location,
            scope_id: self.current_scope,
            escapes: false, // Will be updated by escape analysis
            is_unsafe,
        };

        self.allocations.insert(alloc_id, metadata);

        // Add to current scope
        if let Some(scope) = self.scope_stack.last_mut() {
            scope.allocations.push(alloc_id);
        }

        alloc_id
    }

    /// Mark an allocation as escaping (returned from function, stored in global, etc.)
    pub fn mark_escaping(&mut self, alloc_id: u64) {
        if let Some(metadata) = self.allocations.get_mut(&alloc_id) {
            metadata.escapes = true;
        }
    }

    /// Get metadata for an allocation
    pub fn get_metadata(&self, alloc_id: u64) -> Option<&AllocationMetadata> {
        self.allocations.get(&alloc_id)
    }

    /// Check if an allocation has been freed (for double-free detection)
    pub fn is_freed(&self, alloc_id: u64) -> bool {
        // In AOT, we track via metadata; explicit free removes from map
        !self.allocations.contains_key(&alloc_id)
    }

    /// Mark an allocation as freed (explicit free() call)
    pub fn mark_freed(&mut self, alloc_id: u64) {
        self.allocations.remove(&alloc_id);
    }
}

impl Default for AotMemoryTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Runtime metadata structure (must match C layout for FFI)
#[repr(C)]
pub struct RuntimeAllocationMetadata {
    pub id: u64,
    pub size: u64,
    pub elem_size: u64, // 0 if untyped
    pub scope_id: u64,
    pub source_file: *const u8,
    pub source_line: u32,
    pub source_column: u32,
    pub is_freed: u8, // 0 = active, 1 = freed
}

/// C runtime functions that need to be linked with AOT-compiled code
///
/// These functions should be provided by adesh_runtime.c:
///
/// ```c
/// void* adesh_rt_alloc_tracked(size_t size, RuntimeAllocationMetadata* metadata);
/// void adesh_rt_free_tracked(void* ptr, RuntimeAllocationMetadata* metadata);
/// void adesh_rt_scope_exit(uint64_t scope_id);
/// void adesh_rt_validate_ptr(void* ptr, RuntimeAllocationMetadata* metadata);
/// ```
pub mod runtime_signatures {
    /// Signature for adesh_rt_alloc_tracked
    /// Arguments: (size: i64, metadata_ptr: i64) -> ptr: i64
    pub const ALLOC_TRACKED_SIG: (&str, &[&str], &str) =
        ("adesh_rt_alloc_tracked", &["i64", "i64"], "i64");

    /// Signature for adesh_rt_free_tracked
    /// Arguments: (ptr: i64, metadata_ptr: i64) -> void (returns 0)
    pub const FREE_TRACKED_SIG: (&str, &[&str], &str) =
        ("adesh_rt_free_tracked", &["i64", "i64"], "i64");

    /// Signature for adesh_rt_scope_exit
    /// Arguments: (scope_id: i64) -> void (returns 0)
    pub const SCOPE_EXIT_SIG: (&str, &[&str], &str) = ("adesh_rt_scope_exit", &["i64"], "i64");

    /// Signature for adesh_rt_validate_ptr (debug mode)
    /// Arguments: (ptr: i64, metadata_ptr: i64) -> is_valid: i32
    pub const VALIDATE_PTR_SIG: (&str, &[&str], &str) =
        ("adesh_rt_validate_ptr", &["i64", "i64"], "i32");

    /// Direct malloc fast path (bypasses FFI tracking metadata frames in release mode)
    pub const MALLOC_RAW_SIG: (&str, &[&str], &str) = ("malloc", &["i64"], "i64");

    /// Direct free fast path (bypasses FFI tracking metadata frames in release mode)
    pub const FREE_RAW_SIG: (&str, &[&str], &str) = ("free", &["i64"], "i64");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_tracking() {
        let mut tracker = AotMemoryTracker::new();

        // Root scope
        assert_eq!(tracker.current_scope, 0);

        // Enter nested scope
        let scope1 = tracker.enter_scope();
        assert_eq!(scope1, 1);

        // Allocate in nested scope
        let alloc1 = tracker.register_allocation(
            100,
            1024,
            None,
            SourceLocation {
                file: "test.adesh".to_string(),
                line: 10,
                column: 5,
            },
            false,
        );

        // Exit scope should return allocation for cleanup
        let cleanup = tracker.exit_scope();
        assert_eq!(cleanup.len(), 1);
        assert_eq!(cleanup[0], alloc1);
    }

    #[test]
    fn test_escaping_allocation() {
        let mut tracker = AotMemoryTracker::new();

        let _scope1 = tracker.enter_scope();
        let alloc1 = tracker.register_allocation(
            100,
            1024,
            None,
            SourceLocation {
                file: "test.adesh".to_string(),
                line: 10,
                column: 5,
            },
            false,
        );

        // Mark as escaping (e.g., returned from function)
        tracker.mark_escaping(alloc1);

        // Exit scope should NOT return this allocation for cleanup
        let cleanup = tracker.exit_scope();
        assert_eq!(cleanup.len(), 0);
    }

    #[test]
    fn test_double_free_detection() {
        let mut tracker = AotMemoryTracker::new();

        let alloc1 = tracker.register_allocation(
            100,
            1024,
            None,
            SourceLocation {
                file: "test.adesh".to_string(),
                line: 10,
                column: 5,
            },
            false,
        );

        assert!(!tracker.is_freed(alloc1));

        tracker.mark_freed(alloc1);
        assert!(tracker.is_freed(alloc1));
    }
}
