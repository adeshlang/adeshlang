//! Compilation context for function lowering
//!
//! This module contains the FunctionCompileContext which tracks mappings between
//! LIR entities and Cranelift entities during the compilation of a single function.

use cranelift::prelude::*;
use cranelift_module::{DataId, FuncId};
use std::collections::HashMap;

use super::types::AotValueType;
use crate::backends::common::lir::{BlockId, ValueId};

/// Context for compiling a single function
/// Tracks mappings between LIR and Cranelift entities
#[allow(dead_code)]
pub(super) struct FunctionCompileContext {
    /// Map from LIR ValueId to Cranelift Value
    pub(super) value_map: HashMap<ValueId, Value>,
    /// Map from LIR ValueId to its runtime type
    pub(super) value_types: HashMap<ValueId, AotValueType>,
    /// Map from LIR BlockId to Cranelift Block
    pub(super) block_map: HashMap<BlockId, Block>,
    /// Map from LIR variable names to Cranelift Variables
    pub(super) var_map: HashMap<String, Variable>,
    /// Map from variable names to their runtime types
    pub(super) var_types: HashMap<String, AotValueType>,
    /// Next variable index for Cranelift Variable allocation
    pub(super) next_var_index: usize,
    /// String data IDs
    pub(super) string_data: HashMap<String, DataId>,
    /// Printf function reference
    pub(super) printf_func: Option<FuncId>,
    /// Puts function reference  
    pub(super) puts_func: Option<FuncId>,
    /// Malloc function reference (for array allocation)
    pub(super) malloc_func: Option<FuncId>,
    /// Memcpy function reference (for array copying)
    pub(super) memcpy_func: Option<FuncId>,
    /// Free function reference (for deallocation)
    pub(super) free_func: Option<FuncId>,
    /// Heap policy guard function (assert heap allowed)
    pub(super) heap_guard_func: Option<FuncId>,
    /// Global array buffer for static arrays (temporary workaround for malloc issues)
    pub(super) array_buffer: Option<DataId>,
    /// Global array offset counter (runtime tracked)
    pub(super) array_offset: Option<DataId>,
    /// Next offset in the global array buffer (compile-time tracking, unused for now)
    #[allow(dead_code)]
    pub(super) next_array_offset: usize,
    /// User-defined functions
    pub(super) user_funcs: HashMap<String, FuncId>,
    /// Track object literals for print options (ValueId -> properties)
    pub(super) object_properties: HashMap<ValueId, HashMap<String, (ValueId, AotValueType)>>,
    /// Track constant strings by ValueId for option extraction
    pub(super) const_strings: HashMap<ValueId, String>,
    /// Track constant booleans by ValueId
    pub(super) const_bools: HashMap<ValueId, bool>,
    /// Track constant integer values by ValueId
    pub(super) const_ints: HashMap<ValueId, i64>,
    /// argc value for main function
    pub(super) argc_value: Option<Value>,
    /// argv value for main function
    pub(super) argv_value: Option<Value>,
    /// Global argc variable
    pub(super) argc_global: Option<DataId>,
    /// Global argv variable
    pub(super) argv_global: Option<DataId>,
    /// Optional capacity override for arrays produced via fixed-size annotation
    pub(super) array_capacity: HashMap<ValueId, i64>,
    /// Capacity override tracked per variable name for propagation on LoadVar
    pub(super) var_array_capacity: HashMap<String, i64>,
    /// Map LIR ValueId for function references to FuncId (for callbacks)
    pub(super) func_values: HashMap<ValueId, FuncId>,
    /// Map string index to DataId for ConstString instructions
    pub(super) string_indices: HashMap<usize, DataId>,
    /// Map ValueId to string DataId for values that reference strings
    pub(super) string_data_for_value: HashMap<ValueId, DataId>,
    /// Track object properties by variable name (for objects stored in variables)
    pub(super) var_object_properties: HashMap<String, HashMap<String, (ValueId, AotValueType)>>,
}

impl FunctionCompileContext {
    #[allow(dead_code)]
    pub(super) fn new() -> Self {
        Self {
            value_map: HashMap::new(),
            value_types: HashMap::new(),
            block_map: HashMap::new(),
            var_map: HashMap::new(),
            var_types: HashMap::new(),
            next_var_index: 0,
            string_data: HashMap::new(),
            printf_func: None,
            puts_func: None,
            malloc_func: None,
            memcpy_func: None,
            free_func: None,
            heap_guard_func: None,
            array_buffer: None,
            array_offset: None,
            next_array_offset: 0,
            user_funcs: HashMap::new(),
            object_properties: HashMap::new(),
            const_strings: HashMap::new(),
            const_bools: HashMap::new(),
            const_ints: HashMap::new(),
            argc_value: None,
            argv_value: None,
            argc_global: None,
            argv_global: None,
            array_capacity: HashMap::new(),
            var_array_capacity: HashMap::new(),
            func_values: HashMap::new(),
            string_indices: HashMap::new(),
            string_data_for_value: HashMap::new(),
            var_object_properties: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    pub(super) fn alloc_variable(&mut self) -> Variable {
        let var = Variable::new(self.next_var_index);
        self.next_var_index += 1;
        var
    }

    #[allow(dead_code)]
    pub(super) fn get_or_create_var(&mut self, name: String) -> Variable {
        if let Some(&var) = self.var_map.get(&name) {
            var
        } else {
            let var = self.alloc_variable();
            self.var_map.insert(name, var);
            var
        }
    }
}
