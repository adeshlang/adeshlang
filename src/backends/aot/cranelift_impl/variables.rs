//! Variable operations for Cranelift backend
//!
//! This module handles variable load/store operations, module loading, and copy operations.

use cranelift::prelude::*;
use cranelift_codegen::ir::InstBuilder;
use std::collections::{HashMap, HashSet};

use crate::backends::aot::cranelift::AotValueType;
use crate::backends::aot::lir::{LirInst, ValueId};

/// Helper to map AotValueType to Cranelift type
pub(crate) fn aot_type_to_cranelift(aot_type: &AotValueType) -> Type {
    match aot_type {
        AotValueType::Float | AotValueType::F64 => types::F64,
        AotValueType::F32 => types::F32,
        AotValueType::Bool => types::I8,
        AotValueType::U8 | AotValueType::I8 => types::I8,
        AotValueType::U16 | AotValueType::I16 => types::I16,
        AotValueType::U32 | AotValueType::I32 => types::I32,
        AotValueType::U128 | AotValueType::I128 => types::I128,
        _ => types::I64, // Default: Int, U64, I64, Array, Pointer, etc.
    }
}

/// Lower variable operations
///
/// Returns Ok(true) if the instruction was handled, Ok(false) if it wasn't a variable operation,
/// or Err if an error occurred.
pub(crate) fn lower_variable_instruction(
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<ValueId, Value>,
    value_types: &mut HashMap<ValueId, AotValueType>,
    runtime_handle_values: &mut HashSet<ValueId>,
    var_runtime_handles: &mut HashSet<String>,
    var_types: &mut HashMap<String, AotValueType>,
    array_capacity: &mut HashMap<ValueId, i64>,
    var_array_capacity: &mut HashMap<String, i64>,
    var_map: &mut HashMap<String, Variable>,
    next_var_index: &mut usize,
    object_properties: &mut HashMap<ValueId, HashMap<String, (ValueId, AotValueType)>>,
    var_object_properties: &mut HashMap<String, HashMap<String, (ValueId, AotValueType)>>,
    const_bools: &mut HashMap<ValueId, bool>,
    const_strings: &mut HashMap<ValueId, String>,
    inst: &LirInst,
) -> Result<bool, String> {
    match inst {
        // Load variable
        LirInst::LoadVar(dst, name) => {
            // Get the type from var_types if available, default to I64
            let aot_type = var_types.get(name).cloned();

            // Map AotValueType to correct Cranelift type
            let cranelift_type = aot_type
                .as_ref()
                .map(|t| aot_type_to_cranelift(t))
                .unwrap_or(types::I64);

            let var = get_or_create_var(var_map, next_var_index, name, builder, cranelift_type);
            let v = builder.use_var(var);

            value_map.insert(*dst, v);
            // Propagate the type
            if let Some(aot_t) = aot_type {
                value_types.insert(*dst, aot_t);
            }
            if runtime_handle_values.contains(dst) {
                runtime_handle_values.remove(dst);
            }
            if value_types.get(dst) == Some(&AotValueType::Handle) {
                runtime_handle_values.insert(*dst);
            }
            if var_runtime_handles.contains(name) {
                runtime_handle_values.insert(*dst);
                value_types.insert(*dst, AotValueType::Handle);
            }
            // Propagate capacity override if tracked for this variable
            if let Some(&cap) = var_array_capacity.get(name) {
                array_capacity.insert(*dst, cap);
            }
            // Propagate object properties if stored for this variable
            if let Some(props) = var_object_properties.get(name).cloned() {
                object_properties.insert(*dst, props);
            }
            Ok(true)
        }

        // Store variable
        LirInst::StoreVar(name, src) => {
            // Get the source value
            let val = value_map
                .get(src)
                .copied()
                .ok_or_else(|| format!("Value {} not found for StoreVar", src))?;

            // Get the actual Cranelift type of the value, not the AotValueType
            let actual_type = builder.func.dfg.value_type(val);

            // Get or create variable with the actual value type
            let var = get_or_create_var(var_map, next_var_index, name, builder, actual_type);

            // Store value directly - types now guaranteed to match
            builder.def_var(var, val);

            // Track the variable's semantic type for later use
            let src_type = value_types.get(src).cloned().unwrap_or(AotValueType::Int);
            var_types.insert(name.clone(), src_type);
            if runtime_handle_values.contains(src)
                || value_types.get(src) == Some(&AotValueType::Handle)
            {
                var_runtime_handles.insert(name.clone());
            } else {
                var_runtime_handles.remove(name);
            }

            // If source has a capacity override, track it on the variable name
            if let Some(&cap) = array_capacity.get(src) {
                var_array_capacity.insert(name.clone(), cap);
            } else {
                // Clear any previous capacity override if this assignment removes it
                var_array_capacity.remove(name);
            }
            // Propagate object properties to variable tracking
            if let Some(props) = object_properties.get(src).cloned() {
                var_object_properties.insert(name.clone(), props);
            }
            Ok(true)
        }

        // Load module (placeholder)
        LirInst::LoadModule(dst, _alias) => {
            // Module loading: return 0 for now (needs runtime)
            let v = builder.ins().iconst(types::I64, 0);
            value_map.insert(*dst, v);
            Ok(true)
        }

        // Copy value
        LirInst::Copy(dst, src) => {
            let val = value_map
                .get(src)
                .copied()
                .ok_or_else(|| format!("Value {} not found for Copy", src))?;
            value_map.insert(*dst, val);
            // Propagate type
            if let Some(src_type) = value_types.get(src).cloned() {
                value_types.insert(*dst, src_type);
            }
            if runtime_handle_values.contains(dst) {
                runtime_handle_values.remove(dst);
            }
            if runtime_handle_values.contains(src) {
                runtime_handle_values.insert(*dst);
            }
            // Propagate object properties
            if let Some(props) = object_properties.get(src).cloned() {
                object_properties.insert(*dst, props);
            }
            // Propagate constant information for print options
            if let Some(bool_val) = const_bools.get(src).copied() {
                const_bools.insert(*dst, bool_val);
            }
            if let Some(str_val) = const_strings.get(src).cloned() {
                const_strings.insert(*dst, str_val);
            }
            Ok(true)
        }

        _ => Ok(false), // Not a variable operation
    }
}

/// Helper function to get or create a variable
fn get_or_create_var(
    var_map: &mut HashMap<String, Variable>,
    next_var_index: &mut usize,
    name: &str,
    builder: &mut FunctionBuilder,
    cranelift_type: Type,
) -> Variable {
    if let Some(&var) = var_map.get(name) {
        var
    } else {
        let var = Variable::new(*next_var_index);
        *next_var_index += 1;
        builder.declare_var(var, cranelift_type);
        var_map.insert(name.to_string(), var);
        var
    }
}
