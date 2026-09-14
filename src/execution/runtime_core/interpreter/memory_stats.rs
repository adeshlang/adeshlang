//! Memory statistics structures for runtime memory reporting.
//!
//! Provides data structures for collecting and reporting memory usage
//! statistics during program execution.

use std::collections::HashMap;

/// Variable memory information for detailed stats.
#[derive(Debug, Clone)]
pub struct VariableMemoryInfo {
    pub name: String,
    pub type_name: String,
    pub size_bytes: usize,
    pub is_const: bool,
}

/// Array metadata statistics.
#[derive(Debug, Clone, Default)]
pub struct ArrayMemoryInfo {
    /// Name of the array variable
    pub name: String,
    /// Element type (Byte, Word, Long, etc.)
    pub element_type: String,
    /// Current length
    pub length: usize,
    /// Current capacity
    pub capacity: usize,
    /// Metadata overhead in bytes
    pub metadata_bytes: usize,
    /// Data size in bytes
    pub data_bytes: usize,
    /// Total memory footprint
    pub total_bytes: usize,
}

/// Memory statistics for a program.
#[derive(Debug, Clone, Default)]
pub struct ProgramMemoryStats {
    /// Total bytes used by user variables
    pub total_variable_bytes: usize,
    /// Number of user-defined variables
    pub variable_count: usize,
    /// Detailed info per variable
    pub variables: Vec<VariableMemoryInfo>,
    /// Array-specific memory statistics
    pub arrays: Vec<ArrayMemoryInfo>,
    /// Total array metadata overhead
    pub total_array_metadata_bytes: usize,
    /// Total array data bytes
    pub total_array_data_bytes: usize,
    /// Memory by type category
    pub by_type: HashMap<String, (usize, usize)>, // type -> (count, bytes)
    /// Number of environments/scopes
    pub scope_count: usize,
    /// Number of promises
    pub promise_count: usize,
    /// Number of active timers
    pub timer_count: usize,
    /// Number of cached methods
    pub method_cache_count: usize,

    // New optimized breakdown statistics
    pub compiler_memory: usize,
    pub runtime_memory: usize,
    pub heap_memory: usize,
    pub stack_memory: usize,
    pub static_data: usize,
    pub object_memory: usize,
    pub array_memory: usize,
    pub string_memory: usize,
    pub function_metadata: usize,
    pub type_metadata: usize,
    pub symbol_metadata: usize,

    // Method cache statistics
    pub method_cache_hits: usize,
    pub method_cache_lookups: usize,
}
