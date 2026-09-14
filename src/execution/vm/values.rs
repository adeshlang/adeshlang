//! Value representation and conversion utilities for the VM.
//!
//! Defines `VMValue`, the internal representation used by both v1 (stack) and v2 (register) VMs,
//! along with conversion functions to/from AST `Value` and builtin `RuntimeValue`.

use crate::parsing::ast::Value;
use crate::runtime::nanvalue::{HeapValue, NanValue};

/// Internal value representation for the VM.
///
/// These items are intentionally available for tests and/or for future
/// uses by the bytecode VM module. Silence dead_code warnings so the
/// project builds cleanly while the emitter/VM work continues.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(super) enum VMValue {
    Number(f64),
    Bool(bool),
    Str(String),
    BigInt(num_bigint::BigInt),
    Array(Vec<VMValue>),
    Tuple(Vec<VMValue>),
    Object(std::collections::HashMap<String, VMValue>),
    Closure { fn_offset: u32, env: Vec<VMValue> },
    U64(u64), // For pointer addresses
    Null,
}

/// Convert a VMValue to an AST Value for FFI calls.
pub(super) fn vm_to_value(v: VMValue) -> Result<Value, String> {
    match v {
        VMValue::Number(n) => Ok(Value::Number(n)),
        VMValue::Bool(b) => Ok(Value::Bool(b)),
        VMValue::Str(s) => Ok(Value::Str(s)),
        VMValue::BigInt(bi) => Ok(Value::BigInt(bi)),
        VMValue::U64(u) => Ok(Value::U64(u)),
        VMValue::Null => Ok(Value::Null),
        VMValue::Closure { .. } => Ok(Value::Null),
        VMValue::Array(arr) => Ok(Value::Array(
            arr.into_iter()
                .map(|v| vm_to_value(v).unwrap_or(Value::Null))
                .collect(),
        )),
        VMValue::Tuple(tup) => Ok(Value::Tuple(
            tup.into_iter()
                .map(|v| vm_to_value(v).unwrap_or(Value::Null))
                .collect(),
        )),
        VMValue::Object(map) => {
            let mut ast_map = crate::utils::collections::FastMap::default();
            for (k, v) in map {
                ast_map.insert(k, vm_to_value(v).unwrap_or(Value::Null));
            }
            Ok(Value::Object(std::sync::Arc::new(ast_map)))
        }
    }
}

/// Convert an AST Value to a VMValue.
pub(super) fn value_to_vm(v: Value) -> VMValue {
    match v {
        Value::Number(n) => VMValue::Number(n),
        Value::F64(n) => VMValue::Number(n),
        Value::F32(n) => VMValue::Number(n as f64),
        Value::I64(n) => VMValue::Number(n as f64),
        Value::I32(n) => VMValue::Number(n as f64),
        Value::I16(n) => VMValue::Number(n as f64),
        Value::I8(n) => VMValue::Number(n as f64),
        Value::U64(n) => VMValue::U64(n), // Preserve U64 for pointer addresses
        Value::U32(n) => VMValue::Number(n as f64),
        Value::U16(n) => VMValue::Number(n as f64),
        Value::U8(n) => VMValue::Number(n as f64),
        Value::Bool(b) => VMValue::Bool(b),
        Value::Str(s) => VMValue::Str(s),
        Value::BigInt(bi) => VMValue::BigInt(bi),
        Value::Array(arr) => VMValue::Array(arr.into_iter().map(value_to_vm).collect()),
        Value::Tuple(tup) => VMValue::Tuple(tup.into_iter().map(value_to_vm).collect()),
        Value::Object(map) => {
            let mut vm_map = std::collections::HashMap::new();
            for (k, v) in map.iter() {
                vm_map.insert(k.clone(), value_to_vm(v.clone()));
            }
            VMValue::Object(vm_map)
        }
        Value::Null => VMValue::Null,
        _ => VMValue::Null,
    }
}

/// Convert a VMValue to a builtin RuntimeValue.
pub(super) fn vm_value_to_builtin_runtime_value(
    vm_val: VMValue,
) -> crate::backends::builtins::RuntimeValue {
    use crate::backends::builtins::RuntimeValue as BRV;
    match vm_val {
        VMValue::Number(n) => {
            if n.fract() == 0.0 && n.abs() <= i64::MAX as f64 {
                BRV::Int(n as i64)
            } else {
                BRV::Float(n)
            }
        }
        VMValue::Bool(b) => BRV::Bool(b),
        VMValue::Str(s) => BRV::String(s),
        VMValue::BigInt(b) => BRV::BigInt(b),
        VMValue::U64(u) => BRV::U64(u), // Support pointer addresses
        VMValue::Array(arr) => BRV::Array(
            arr.into_iter()
                .map(vm_value_to_builtin_runtime_value)
                .collect(),
        ),
        VMValue::Tuple(tup) => BRV::Tuple(
            tup.into_iter()
                .map(vm_value_to_builtin_runtime_value)
                .collect(),
        ),
        VMValue::Object(map) => {
            let mut runtime_map = crate::utils::collections::FastMap::default();
            for (k, v) in map {
                runtime_map.insert(k, vm_value_to_builtin_runtime_value(v));
            }
            BRV::Object(runtime_map)
        }
        VMValue::Closure { .. } => BRV::Null,
        VMValue::Null => BRV::Null,
    }
}

/// Convert a builtin RuntimeValue to a VMValue.
pub(super) fn builtin_runtime_value_to_vm_value(
    runtime_val: crate::backends::builtins::RuntimeValue,
) -> VMValue {
    use crate::backends::builtins::RuntimeValue as BRV;
    match runtime_val {
        BRV::Int(i) => VMValue::Number(i as f64),
        BRV::Float(f) => VMValue::Number(f),
        BRV::String(s) => VMValue::Str(s),
        BRV::Bool(b) => VMValue::Bool(b),
        BRV::BigInt(b) => VMValue::BigInt(b),
        BRV::U64(u) => VMValue::U64(u), // Support pointer addresses
        BRV::Array(arr) => VMValue::Array(
            arr.into_iter()
                .map(builtin_runtime_value_to_vm_value)
                .collect(),
        ),
        BRV::Tuple(tup) => VMValue::Tuple(
            tup.into_iter()
                .map(builtin_runtime_value_to_vm_value)
                .collect(),
        ),
        BRV::Object(map) => {
            let mut vm_map = std::collections::HashMap::new();
            for (k, v) in map {
                vm_map.insert(k, builtin_runtime_value_to_vm_value(v));
            }
            VMValue::Object(vm_map)
        }
        BRV::Null => VMValue::Null,
        _ => VMValue::Null, // Handle other variants
    }
}

/// Convert a VMValue to a NanValue (optimized for VM operations).
/// This provides better performance than converting through AST Value.
pub(super) fn vm_to_nanvalue(vm_val: &VMValue) -> NanValue {
    match vm_val {
        VMValue::Number(n) => NanValue::from_f64(*n),
        VMValue::Bool(b) => NanValue::from_bool(*b),
        VMValue::Str(s) => NanValue::from_heap(HeapValue::String(s.clone())),
        VMValue::BigInt(bi) => NanValue::from_heap(HeapValue::BigInt(bi.clone())),
        VMValue::Null => NanValue::null(),
        VMValue::Closure { .. } => NanValue::null(),
        VMValue::Array(arr) => {
            let nan_arr: Vec<NanValue> = arr.iter().map(vm_to_nanvalue).collect();
            NanValue::from_heap(HeapValue::Array(nan_arr))
        }
        VMValue::Tuple(tup) => {
            let nan_tup: Vec<NanValue> = tup.iter().map(vm_to_nanvalue).collect();
            NanValue::from_heap(HeapValue::Array(nan_tup))
        }
        VMValue::U64(u) => {
            // U64 used for pointers - convert to i48 if possible, otherwise BigInt
            if *u < (1u64 << 47) {
                NanValue::from_i48(*u as i64).unwrap()
            } else {
                NanValue::from_heap(HeapValue::BigInt(num_bigint::BigInt::from(*u)))
            }
        }
        VMValue::Object(map) => {
            // Convert HashMap<String, VMValue> to HashMap<String, NanValue>
            let mut nan_map = std::collections::HashMap::new();
            for (k, v) in map {
                nan_map.insert(k.clone(), vm_to_nanvalue(v));
            }
            NanValue::from_heap(HeapValue::Object(nan_map))
        }
    }
}

/// Convert a NanValue back to a VMValue.
#[allow(dead_code)]
pub(super) fn nanvalue_to_vm(nan_val: &NanValue) -> VMValue {
    if nan_val.is_null() {
        return VMValue::Null;
    }

    if let Some(n) = nan_val.as_f64() {
        return VMValue::Number(n);
    }

    if let Some(i) = nan_val.as_i48() {
        // Convert i48 back to f64 for VMValue (VM uses Number for integers)
        return VMValue::Number(i as f64);
    }

    if let Some(c) = nan_val.as_char() {
        // Convert char to string
        return VMValue::Str(c.to_string());
    }

    if let Some(heap_arc) = nan_val.as_heap() {
        match &*heap_arc {
            HeapValue::String(s) => VMValue::Str(s.clone()),
            HeapValue::BigInt(bi) => VMValue::BigInt(bi.clone()),
            HeapValue::Array(elements) => {
                // Convert array to object with numeric keys
                let mut vm_map = std::collections::HashMap::new();
                for (i, elem) in elements.iter().enumerate() {
                    vm_map.insert(i.to_string(), nanvalue_to_vm(elem));
                }
                VMValue::Object(vm_map)
            }
            HeapValue::Object(map) => {
                let mut vm_map = std::collections::HashMap::new();
                for (k, v) in map {
                    vm_map.insert(k.clone(), nanvalue_to_vm(v));
                }
                VMValue::Object(vm_map)
            }
        }
    } else {
        VMValue::Null
    }
}
