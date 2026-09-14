//! Escape Analysis for Stack Allocation (Optimizing JIT Tier 2)
//!
//! This module provides escape analysis to determine when arrays can be safely
//! allocated on the stack instead of the heap, enabling significant performance improvements.
//!
//! ## Escape Analysis Strategy:
//!
//! ### NoEscape Cases (Stack-Allocatable):
//! ```
//! fn process() {
//!     let temp = [1, 2, 3, 4, 5];  // ← Stack-allocated
//!     let sum = temp.reduce(|a, b| a + b);
//!     return sum;  // Array doesn't escape
//! }
//! ```
//!
//! ### Escape Cases (Heap-Required):
//! ```
//! fn create_array() {
//!     let arr = [1, 2, 3];
//!     return arr;  // ← Escapes via return
//! }
//!
//! fn store_in_object() {
//!     let arr = [1, 2, 3];
//!     let obj = { data: arr };  // ← Escapes into heap object
//!     return obj;
//! }
//! ```
//!
//! ## Implementation Phases:
//!
//! ### Phase 1: Intraprocedural Analysis (Current)
//! - Analyze single function scope
//! - Track return values
//! - Track heap stores
//! - Conservative for function calls
//!
//! ### Phase 2: Interprocedural Analysis (Future)
//! - Cross-function dataflow
//! - Alias analysis
//! - Precise escape tracking
//!
//! ### Phase 3: Profile-Guided (Future)
//! - Runtime profiling of escape patterns
//! - Speculative stack allocation with deopt
//! - Adaptive threshold tuning
//!
//! ## Code Generation:
//!
//! ### Stack-Allocated Array (NoEscape):
//! ```asm
//! ; Allocate on stack
//! sub rsp, 24           ; Reserve stack space (e.g., 6 i32s)
//! mov QWORD PTR [rsp], 1
//! mov QWORD PTR [rsp+4], 2
//! ...
//! ; Automatic cleanup on return
//! add rsp, 24
//! ret
//! ```
//!
//! ### Heap-Allocated Array (Escapes):
//! ```asm
//! ; Allocate on heap
//! mov rdi, 24           ; Size
//! call malloc
//! mov r8, rax           ; Save pointer
//! ; Initialize...
//! ; Heap / ARC ownership tracking
//! ```
//!
//! ## Performance Impact:
//! - Stack allocation: ~5-10 cycles
//! - Heap allocation: ~50-100+ cycles
//! - Zero heap allocation & zero ref-count overhead for stack arrays
//! - Better cache locality
//! - Automatic deterministic cleanup upon scope exit

use std::collections::HashMap;

/// Escape status for a value
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapeStatus {
    /// Does not escape: can be stack-allocated
    NoEscape,
    /// Escapes: must be heap-allocated
    Escapes,
    /// Unknown/conservative: assume escapes
    Unknown,
}

/// Intraprocedural Escape Analyzer
#[derive(Debug, Clone, Default)]
pub struct EscapeAnalyzer {
    /// Variable name -> EscapeStatus
    statuses: HashMap<String, EscapeStatus>,
}

impl EscapeAnalyzer {
    pub fn new() -> Self {
        Self {
            statuses: HashMap::new(),
        }
    }

    /// Record a new local variable with initial status
    pub fn record_var(&mut self, name: &str, status: EscapeStatus) {
        self.statuses.insert(name.to_string(), status);
    }

    /// Mark a variable as escaping (e.g. returned, stored in heap, passed to unknown func)
    pub fn mark_escaped(&mut self, name: &str) {
        self.statuses
            .insert(name.to_string(), EscapeStatus::Escapes);
    }

    /// Mark a variable as non-escaping (stack allocatable)
    pub fn mark_no_escape(&mut self, name: &str) {
        self.statuses
            .insert(name.to_string(), EscapeStatus::NoEscape);
    }

    /// Check if a variable can be stack-allocated
    pub fn can_stack_allocate(&self, name: &str) -> bool {
        match self.statuses.get(name) {
            Some(EscapeStatus::NoEscape) => true,
            _ => false,
        }
    }

    /// Get escape status
    pub fn get_status(&self, name: &str) -> EscapeStatus {
        self.statuses
            .get(name)
            .copied()
            .unwrap_or(EscapeStatus::Unknown)
    }

    /// Analyze a list of statement variable names and return expression references
    pub fn analyze_local_flow(
        &mut self,
        declared_locals: &[&str],
        returned_vars: &[&str],
        heap_stored_vars: &[&str],
    ) {
        for &local in declared_locals {
            if returned_vars.contains(&local) || heap_stored_vars.contains(&local) {
                self.mark_escaped(local);
            } else {
                self.mark_no_escape(local);
            }
        }
    }
}

/// Code generation helpers for stack vs heap allocation
pub mod codegen {
    /// Generate stack allocation code (pseudocode for JIT backend)
    pub fn generate_stack_array(elem_type: &str, size: usize, name: &str) -> String {
        format!(
            r#"
// Stack-allocated array (escape analysis: NoEscape)
// Total size: {size} * sizeof({elem_type}) bytes
let {name}: [{elem_type}; {size}] = alloca({size} * sizeof({elem_type}));
// Benefits:
// - No heap allocation (~50+ cycles saved)
// - Zero ref-count tracking overhead
// - Automatic cleanup (stack unwind)
// - Better cache locality
"#,
            name = name,
            elem_type = elem_type,
            size = size
        )
    }

    /// Generate heap allocation code
    pub fn generate_heap_array(elem_type: &str, size: usize, name: &str) -> String {
        format!(
            r#"
// Heap-allocated array (escape analysis: Escapes)
let {name}: Box<[{elem_type}]> = Box::new([default(); {size}]);
// Requirements:
// - Deterministic single-owner or ARC management
// - Can outlive stack frame
// - May be shared across threads
"#,
            name = name,
            elem_type = elem_type,
            size = size
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_analyzer_creation() {
        let analyzer = EscapeAnalyzer::new();
        assert!(!analyzer.can_stack_allocate("test"));
    }

    #[test]
    fn test_escape_status_tracking() {
        let mut analyzer = EscapeAnalyzer::new();
        analyzer.record_var("temp_arr", EscapeStatus::NoEscape);
        analyzer.record_var("ret_arr", EscapeStatus::Escapes);

        assert!(analyzer.can_stack_allocate("temp_arr"));
        assert_eq!(analyzer.get_status("temp_arr"), EscapeStatus::NoEscape);

        assert!(!analyzer.can_stack_allocate("ret_arr"));
        assert_eq!(analyzer.get_status("ret_arr"), EscapeStatus::Escapes);
    }

    #[test]
    fn test_local_flow_analysis() {
        let mut analyzer = EscapeAnalyzer::new();
        let locals = ["local1", "local2", "escaped1"];
        let returns = ["escaped1"];
        let heap_stores = [];

        analyzer.analyze_local_flow(&locals, &returns, &heap_stores);

        assert!(analyzer.can_stack_allocate("local1"));
        assert!(analyzer.can_stack_allocate("local2"));
        assert!(!analyzer.can_stack_allocate("escaped1"));
    }

    #[test]
    fn test_codegen_helpers() {
        let stack_code = codegen::generate_stack_array("i32", 10, "local_arr");
        assert!(stack_code.contains("alloca"));
        assert!(stack_code.contains("NoEscape"));

        let heap_code = codegen::generate_heap_array("i32", 10, "heap_arr");
        assert!(heap_code.contains("Box"));
        assert!(heap_code.contains("Escapes"));
    }
}
