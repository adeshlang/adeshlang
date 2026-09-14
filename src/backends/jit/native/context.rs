//! Native JIT Execution Context
//!
//! This module manages the runtime context for executing JIT-compiled code.

use cranelift_jit::JITModule;
use cranelift_module::FuncId;
use std::collections::HashMap;

/// Native JIT execution context
///
/// Holds compiled functions and provides execution interface.
pub struct NativeJitContext {
    /// Map from function name to Cranelift FuncId
    functions: HashMap<String, FuncId>,
    /// Reference to the JIT module (for obtaining function pointers)
    /// Note: We store function pointers directly in practice
    function_ptrs: HashMap<String, *const u8>,
}

impl NativeJitContext {
    /// Create a new execution context
    pub fn new(functions: HashMap<String, FuncId>, module: &JITModule) -> Self {
        let mut function_ptrs = HashMap::new();

        // Get function pointers for all compiled functions
        for (name, func_id) in &functions {
            // Try to get the function pointer, skip if it failed to compile
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                module.get_finalized_function(*func_id)
            })) {
                Ok(ptr) => {
                    function_ptrs.insert(name.clone(), ptr);
                }
                Err(_) => {
                    if cfg!(debug_assertions) {
                        eprintln!("Warning: Could not get function pointer for {}", name);
                    }
                }
            }
        }

        NativeJitContext {
            functions,
            function_ptrs,
        }
    }

    /// Get a function pointer by name
    pub fn get_function(&self, name: &str) -> Option<*const u8> {
        self.function_ptrs.get(name).copied()
    }

    /// Check if a function exists
    pub fn has_function(&self, name: &str) -> bool {
        self.function_ptrs.contains_key(name)
    }

    /// Get function ID by name
    pub fn get_function_id(&self, name: &str) -> Option<FuncId> {
        self.functions.get(name).copied()
    }
}
