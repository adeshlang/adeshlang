//! Array operations and utilities for JIT/AOT backends
//!
//! This module provides array manipulation functions including:
//! - Basic operations: len, capacity, metadata_size
//! - Mutation: push, pop, insert, remove
//! - Access: first, last, get_index, set_index
//! - Transformation: reverse, slice, concat, flat
//! - Search: indexOf, lastIndexOf
//! - Higher-order operations: map, filter, reduce, forEach, find, some, every
//! - Construction: make_array, make_array_spread, range
//! - Utilities: spread, array_to_raw

use super::RuntimeValue;

// ============================================================================
// Basic Array Properties
// ============================================================================

/// Get length of an array, string, tuple, or object
pub(crate) fn runtime_len(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Int(0);
    }
    match &args[0] {
        RuntimeValue::String(s) => RuntimeValue::Int(s.len() as i64),
        RuntimeValue::Array(arr) => RuntimeValue::Int(arr.len() as i64),
        RuntimeValue::RawArray(_, arr) => RuntimeValue::Int(arr.len() as i64),
        RuntimeValue::DynArray { data, .. } => RuntimeValue::Int(data.len() as i64),
        RuntimeValue::Tuple(t) => RuntimeValue::Int(t.len() as i64),
        RuntimeValue::Object(obj) => RuntimeValue::Int(obj.len() as i64),
        _ => RuntimeValue::Int(0),
    }
}

/// Check whether an object contains a key: hasKey(obj, key)
pub(crate) fn runtime_has_key(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Bool(false);
    }

    let key = match &args[1] {
        RuntimeValue::String(s) => s.clone(),
        RuntimeValue::Int(n) => n.to_string(),
        RuntimeValue::Float(n) => {
            if n.fract() == 0.0 {
                (*n as i64).to_string()
            } else {
                n.to_string()
            }
        }
        RuntimeValue::Char(c) => c.to_string(),
        RuntimeValue::BigInt(n) => n.to_string(),
        RuntimeValue::U8(n) => n.to_string(),
        RuntimeValue::U16(n) => n.to_string(),
        RuntimeValue::U32(n) => n.to_string(),
        RuntimeValue::U64(n) => n.to_string(),
        RuntimeValue::U128(n) => n.to_string(),
        RuntimeValue::I8(n) => n.to_string(),
        RuntimeValue::I16(n) => n.to_string(),
        RuntimeValue::I32(n) => n.to_string(),
        RuntimeValue::I64(n) => n.to_string(),
        RuntimeValue::I128(n) => n.to_string(),
        RuntimeValue::F32(n) => {
            if n.fract() == 0.0 {
                (*n as i64).to_string()
            } else {
                n.to_string()
            }
        }
        RuntimeValue::F64(n) => {
            if n.fract() == 0.0 {
                (*n as i64).to_string()
            } else {
                n.to_string()
            }
        }
        _ => return RuntimeValue::Bool(false),
    };

    match &args[0] {
        RuntimeValue::Object(obj) => RuntimeValue::Bool(obj.contains_key(&key)),
        _ => RuntimeValue::Bool(false),
    }
}

/// Get capacity of an array (allocated space)
pub(crate) fn runtime_capacity(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Int(0);
    }
    match &args[0] {
        RuntimeValue::Array(arr) => RuntimeValue::Int(arr.capacity() as i64),
        RuntimeValue::RawArray(_, arr) => RuntimeValue::Int(arr.len() as i64), // Raw arrays have no extra capacity
        RuntimeValue::DynArray {
            data,
            tracked_capacity,
            ..
        } => {
            let cap = tracked_capacity.unwrap_or_else(|| data.capacity());
            RuntimeValue::Int(cap as i64)
        }
        _ => RuntimeValue::Int(0),
    }
}

/// Get metadata size of an array type (overhead in bytes)
pub(crate) fn runtime_metadata_size(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Int(0);
    }
    match &args[0] {
        RuntimeValue::Array(_) => RuntimeValue::Int(24), // Standard Vec: ptr(8) + len(8) + cap(8)
        RuntimeValue::RawArray(_, _) => RuntimeValue::Int(0), // Raw arrays have no metadata
        RuntimeValue::DynArray { element_type, .. } => {
            // Metadata size depends on element type
            let metadata = if element_type.starts_with("u8")
                || element_type.starts_with("i8")
                || element_type.starts_with("u16")
                || element_type.starts_with("i16")
                || element_type.starts_with("u32")
                || element_type.starts_with("i32")
                || element_type.starts_with("f32")
            {
                16 // Byte/Short/Word: 16-byte metadata
            } else {
                24 // Long/Extended/Any: 24-byte metadata
            };
            RuntimeValue::Int(metadata)
        }
        _ => RuntimeValue::Int(0),
    }
}

// ============================================================================
// Array Mutation
// ============================================================================

/// Push element to end of array
pub(crate) fn runtime_push(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Null;
    }
    match &args[0] {
        RuntimeValue::Array(arr) => {
            let mut new_arr = arr.clone();
            new_arr.push(args[1].clone());
            let (element_type, concrete_type) = infer_array_type(&new_arr);
            RuntimeValue::DynArray {
                data: new_arr,
                element_type,
                concrete_type,
                tracked_capacity: None,
            }
        }
        RuntimeValue::DynArray { data, .. } => {
            let mut new_arr = data.clone();
            new_arr.push(args[1].clone());
            let (element_type, concrete_type) = infer_array_type(&new_arr);
            RuntimeValue::DynArray {
                data: new_arr,
                element_type,
                concrete_type,
                tracked_capacity: None,
            }
        }
        _ => RuntimeValue::Null,
    }
}

/// Pop and return last element from array
pub(crate) fn runtime_pop(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }
    match &args[0] {
        RuntimeValue::Array(arr) => {
            if arr.is_empty() {
                RuntimeValue::Null
            } else {
                arr.last().cloned().unwrap_or(RuntimeValue::Null)
            }
        }
        _ => RuntimeValue::Null,
    }
}

/// Insert element at index: insert(arr, index, value)
pub(crate) fn runtime_insert(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 3 {
        return RuntimeValue::Null;
    }
    let index = args[1].as_int().unwrap_or(0) as usize;
    match &args[0] {
        RuntimeValue::Array(arr) => {
            let mut new_arr = arr.clone();
            if index <= new_arr.len() {
                new_arr.insert(index, args[2].clone());
            } else {
                // Out of bounds - append to end
                new_arr.push(args[2].clone());
            }
            RuntimeValue::Array(new_arr)
        }
        _ => RuntimeValue::Null,
    }
}

/// Remove element at index: remove(arr, index) -> returns (removed_element, new_array) tuple
pub(crate) fn runtime_remove(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Null;
    }
    let index = args[1].as_int().unwrap_or(0) as usize;
    match &args[0] {
        RuntimeValue::Array(arr) => {
            if index < arr.len() {
                let mut new_arr = arr.clone();
                let removed = new_arr.remove(index);
                // Return tuple of (removed_element, new_array)
                RuntimeValue::Array(vec![removed, RuntimeValue::Array(new_arr)])
            } else {
                RuntimeValue::Null
            }
        }
        _ => RuntimeValue::Null,
    }
}

// ============================================================================
// Array Element Access
// ============================================================================

/// Get first element: first(arr) -> element or null
pub(crate) fn runtime_first(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }
    match &args[0] {
        RuntimeValue::Array(arr) => arr.first().cloned().unwrap_or(RuntimeValue::Null),
        RuntimeValue::RawArray(_, arr) => arr.first().cloned().unwrap_or(RuntimeValue::Null),
        RuntimeValue::DynArray { data, .. } => data.first().cloned().unwrap_or(RuntimeValue::Null),
        RuntimeValue::Tuple(t) => t.first().cloned().unwrap_or(RuntimeValue::Null),
        RuntimeValue::String(s) => s
            .chars()
            .next()
            .map(|c| RuntimeValue::String(c.to_string()))
            .unwrap_or(RuntimeValue::Null),
        _ => RuntimeValue::Null,
    }
}

/// Get last element: last(arr) -> element or null
pub(crate) fn runtime_last(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }
    match &args[0] {
        RuntimeValue::Array(arr) => arr.last().cloned().unwrap_or(RuntimeValue::Null),
        RuntimeValue::RawArray(_, arr) => arr.last().cloned().unwrap_or(RuntimeValue::Null),
        RuntimeValue::DynArray { data, .. } => data.last().cloned().unwrap_or(RuntimeValue::Null),
        RuntimeValue::Tuple(t) => t.last().cloned().unwrap_or(RuntimeValue::Null),
        RuntimeValue::String(s) => s
            .chars()
            .last()
            .map(|c| RuntimeValue::String(c.to_string()))
            .unwrap_or(RuntimeValue::Null),
        _ => RuntimeValue::Null,
    }
}

/// Get element at index
pub(crate) fn runtime_get_index(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Null;
    }
    let index = args[1].as_int().unwrap_or(0) as usize;
    match &args[0] {
        RuntimeValue::Array(arr) => arr.get(index).cloned().unwrap_or(RuntimeValue::Null),
        RuntimeValue::RawArray(_, arr) => arr.get(index).cloned().unwrap_or(RuntimeValue::Null),
        RuntimeValue::DynArray { data, .. } => {
            data.get(index).cloned().unwrap_or(RuntimeValue::Null)
        }
        RuntimeValue::Tuple(tup) => tup.get(index).cloned().unwrap_or(RuntimeValue::Null),
        _ => RuntimeValue::Null,
    }
}

/// Set element at index
pub(crate) fn runtime_set_index(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 3 {
        return RuntimeValue::Null;
    }
    let index = args[1].as_int().unwrap_or(0) as usize;
    match &args[0] {
        RuntimeValue::Array(arr) => {
            let mut new_arr = arr.clone();
            if index < new_arr.len() {
                new_arr[index] = args[2].clone();
            }
            RuntimeValue::Array(new_arr)
        }
        RuntimeValue::RawArray(elem_type, arr) => {
            let mut new_arr = arr.clone();
            if index < new_arr.len() {
                new_arr[index] = args[2].clone();
            }
            RuntimeValue::RawArray(elem_type.clone(), new_arr)
        }
        RuntimeValue::DynArray {
            data,
            element_type,
            concrete_type,
            tracked_capacity,
        } => {
            let mut new_data = data.clone();
            if index < new_data.len() {
                new_data[index] = args[2].clone();
            }
            RuntimeValue::DynArray {
                data: new_data,
                element_type: element_type.clone(),
                concrete_type: concrete_type.clone(),
                tracked_capacity: *tracked_capacity,
            }
        }
        RuntimeValue::Tuple(_tup) => {
            // Tuples are immutable, return unchanged
            args[0].clone()
        }
        _ => RuntimeValue::Null,
    }
}

// ============================================================================
// Array Transformation
// ============================================================================

/// Reverse array: reverse(arr) -> new reversed array
pub(crate) fn runtime_reverse(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }
    match &args[0] {
        RuntimeValue::Array(arr) => {
            let mut new_arr = arr.clone();
            new_arr.reverse();
            RuntimeValue::Array(new_arr)
        }
        RuntimeValue::String(s) => RuntimeValue::String(s.chars().rev().collect()),
        _ => RuntimeValue::Null,
    }
}

/// Slice array: slice(arr, start, end?) -> new sliced array
pub(crate) fn runtime_slice_array(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Null;
    }
    let start = args[1].as_int().unwrap_or(0).max(0) as usize;

    match &args[0] {
        RuntimeValue::Array(arr) => {
            let end = args
                .get(2)
                .and_then(|v| v.as_int())
                .map(|e| (e as usize).min(arr.len()))
                .unwrap_or(arr.len());

            if start >= arr.len() {
                RuntimeValue::Array(vec![])
            } else {
                RuntimeValue::Array(arr[start..end].to_vec())
            }
        }
        RuntimeValue::String(s) => {
            let chars: Vec<char> = s.chars().collect();
            let end = args
                .get(2)
                .and_then(|v| v.as_int())
                .map(|e| (e as usize).min(chars.len()))
                .unwrap_or(chars.len());

            if start >= chars.len() {
                RuntimeValue::String(String::new())
            } else {
                RuntimeValue::String(chars[start..end].iter().collect())
            }
        }
        _ => RuntimeValue::Null,
    }
}

/// Concatenate arrays
pub(crate) fn runtime_array_concat(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Array(vec![]);
    }

    let mut result = match &args[0] {
        RuntimeValue::Array(arr) => arr.clone(),
        _ => vec![args[0].clone()],
    };

    for arg in &args[1..] {
        match arg {
            RuntimeValue::Array(arr) => result.extend(arr.clone()),
            _ => result.push(arg.clone()),
        }
    }

    RuntimeValue::Array(result)
}

/// Flatten array by one level
pub(crate) fn runtime_array_flat(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Array(vec![]);
    }

    let array = match &args[0] {
        RuntimeValue::Array(arr) => arr,
        _ => return args[0].clone(),
    };

    let depth = args.get(1).and_then(|v| v.as_int()).unwrap_or(1);

    if depth <= 0 {
        return RuntimeValue::Array(array.clone());
    }

    let mut result = Vec::new();
    for item in array {
        if depth > 0 {
            match item {
                RuntimeValue::Array(inner) => result.extend(inner.clone()),
                _ => result.push(item.clone()),
            }
        } else {
            result.push(item.clone());
        }
    }

    RuntimeValue::Array(result)
}

// ============================================================================
// Array Search
// ============================================================================

/// Find first index of element in array
pub(crate) fn runtime_array_index_of(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Int(-1);
    }

    let array = match &args[0] {
        RuntimeValue::Array(arr) => arr,
        _ => return RuntimeValue::Int(-1),
    };

    let needle = &args[1];

    for (i, val) in array.iter().enumerate() {
        if runtime_values_equal(val, needle) {
            return RuntimeValue::Int(i as i64);
        }
    }

    RuntimeValue::Int(-1)
}

/// Find last index of element in array
pub(crate) fn runtime_array_last_index_of(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Int(-1);
    }

    let array = match &args[0] {
        RuntimeValue::Array(arr) => arr,
        _ => return RuntimeValue::Int(-1),
    };

    let needle = &args[1];

    for (i, val) in array.iter().enumerate().rev() {
        if runtime_values_equal(val, needle) {
            return RuntimeValue::Int(i as i64);
        }
    }

    RuntimeValue::Int(-1)
}

// ============================================================================
// Higher-Order Array Operations (Placeholders for callback support)
// ============================================================================

/// forEach - execute function for each element (placeholder)
pub(crate) fn runtime_for_each(_args: &[RuntimeValue]) -> RuntimeValue {
    // Placeholder - actual callback execution needs runtime context
    RuntimeValue::Null
}

/// find - return first element that matches predicate (placeholder)
pub(crate) fn runtime_find(_args: &[RuntimeValue]) -> RuntimeValue {
    // Placeholder - needs callback execution
    RuntimeValue::Null
}

/// findIndex - return index of first element that matches predicate (placeholder)
pub(crate) fn runtime_find_index(_args: &[RuntimeValue]) -> RuntimeValue {
    // Placeholder - needs callback execution
    RuntimeValue::Int(-1)
}

/// some - check if at least one element matches predicate (placeholder)
pub(crate) fn runtime_some(_args: &[RuntimeValue]) -> RuntimeValue {
    // Placeholder - needs callback execution
    RuntimeValue::Bool(false)
}

/// every - check if all elements match predicate (placeholder)
pub(crate) fn runtime_every(_args: &[RuntimeValue]) -> RuntimeValue {
    // Placeholder - needs callback execution
    RuntimeValue::Bool(true)
}

/// map - transform each element using a callback (placeholder)
pub(crate) fn runtime_map(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Null;
    }

    let arr = match &args[0] {
        RuntimeValue::Array(a) => a,
        RuntimeValue::DynArray {
            data,
            concrete_type,
            element_type,
            tracked_capacity,
            ..
        } => {
            // For DynArray, we need to return same type
            // For now, return unchanged - JIT doesn't support callbacks yet
            return RuntimeValue::DynArray {
                data: data.clone(),
                concrete_type: concrete_type.clone(),
                element_type: element_type.clone(),
                tracked_capacity: *tracked_capacity,
            };
        }
        _ => return RuntimeValue::Null,
    };

    // For JIT map: callbacks are not directly supported
    // Return the original array unchanged as a workaround
    RuntimeValue::Array(arr.clone())
}

/// filter - keep only elements matching predicate (placeholder)
pub(crate) fn runtime_filter(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Null;
    }

    let arr = match &args[0] {
        RuntimeValue::Array(a) => a,
        RuntimeValue::DynArray {
            data,
            concrete_type,
            element_type,
            tracked_capacity,
            ..
        } => {
            // For DynArray, return with same type
            // For now, return unchanged - JIT doesn't support callbacks yet
            return RuntimeValue::DynArray {
                data: data.clone(),
                concrete_type: concrete_type.clone(),
                element_type: element_type.clone(),
                tracked_capacity: *tracked_capacity,
            };
        }
        _ => return RuntimeValue::Null,
    };

    // For JIT filter: callbacks are not directly supported
    // Return the original array unchanged as a workaround
    RuntimeValue::Array(arr.clone())
}

/// reduce - accumulate array elements into a single value (placeholder)
pub(crate) fn runtime_reduce(args: &[RuntimeValue]) -> RuntimeValue {
    // args[0] = array, args[1] = callback function, args[2] = initial value
    // Note: In JIT mode, callbacks are not directly supported
    // Return the initial value if provided, else return the array or null
    if args.len() >= 3 {
        // Initial value provided
        args[2].clone()
    } else if args.len() >= 1 {
        // No initial value - try to return first element or null
        match &args[0] {
            RuntimeValue::Array(a) => {
                if a.is_empty() {
                    RuntimeValue::Null
                } else {
                    a[0].clone()
                }
            }
            RuntimeValue::DynArray { data, .. } => {
                if data.is_empty() {
                    RuntimeValue::Null
                } else {
                    data[0].clone()
                }
            }
            _ => RuntimeValue::Null,
        }
    } else {
        RuntimeValue::Null
    }
}

// ============================================================================
// Array Construction
// ============================================================================

/// Create array from arguments
pub(crate) fn runtime_make_array(args: &[RuntimeValue]) -> RuntimeValue {
    let data = args.to_vec();
    let (element_type, concrete_type) = infer_array_type(&data);

    RuntimeValue::DynArray {
        data,
        element_type,
        concrete_type,
        tracked_capacity: None,
    }
}

/// Create array with spread support: args are pairs of (value, is_spread)
pub(crate) fn runtime_make_array_spread(args: &[RuntimeValue]) -> RuntimeValue {
    let mut result = Vec::new();

    // Process pairs: (value, is_spread_marker)
    let mut i = 0;
    while i + 1 < args.len() {
        let value = &args[i];
        let is_spread = match &args[i + 1] {
            RuntimeValue::Bool(b) => *b,
            _ => false,
        };

        if is_spread {
            // Spread: flatten the array into result
            if let RuntimeValue::Array(arr) = value {
                result.extend(arr.iter().cloned());
            } else if let RuntimeValue::DynArray { data, .. } = value {
                result.extend(data.iter().cloned());
            } else {
                result.push(value.clone());
            }
        } else {
            result.push(value.clone());
        }
        i += 2;
    }

    let (element_type, concrete_type) = infer_array_type(&result);

    RuntimeValue::DynArray {
        data: result,
        element_type,
        concrete_type,
        tracked_capacity: None,
    }
}

/// Create range array: range(start, end, step_or_inclusive?)
pub(crate) fn runtime_range(args: &[RuntimeValue]) -> RuntimeValue {
    let start = args.first().and_then(|v| v.as_int()).unwrap_or(0);
    let end = args.get(1).and_then(|v| v.as_int()).unwrap_or(0);
    // Third arg can be either step (int) or inclusive (bool)
    let (step, inclusive) = match args.get(2) {
        Some(RuntimeValue::Bool(b)) => (1, *b),
        Some(RuntimeValue::Int(s)) => (*s, false),
        _ => (1, false),
    };

    if step == 0 {
        return RuntimeValue::Array(Vec::new());
    }

    let mut result = Vec::new();
    let mut i = start;

    if step > 0 {
        let limit = if inclusive { end + 1 } else { end };
        while i < limit {
            result.push(RuntimeValue::Int(i));
            i += step;
        }
    } else {
        let limit = if inclusive { end - 1 } else { end };
        while i > limit {
            result.push(RuntimeValue::Int(i));
            i += step;
        }
    }

    let (element_type, concrete_type) = infer_array_type(&result);
    RuntimeValue::DynArray {
        data: result,
        element_type,
        concrete_type,
        tracked_capacity: None,
    }
}

// ============================================================================
// Array Utilities
// ============================================================================

/// Spread operator - returns array as-is for JIT
pub(crate) fn runtime_spread(args: &[RuntimeValue]) -> RuntimeValue {
    // Spread returns the array/iterable as-is for JIT (used in array literals)
    if args.is_empty() {
        return RuntimeValue::Array(vec![]);
    }
    // Return the value as-is - actual spreading happens at array/call construction
    args[0].clone()
}

/// Convert array to raw array representation
#[allow(dead_code)]
pub(crate) fn runtime_array_to_raw(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }

    match &args[0] {
        RuntimeValue::Array(arr) => {
            // Detect element type
            let elem_type = if arr.is_empty() {
                "any".to_string()
            } else {
                match &arr[0] {
                    RuntimeValue::Int(_) => "i64".to_string(),
                    RuntimeValue::Float(_) => "f64".to_string(),
                    RuntimeValue::Bool(_) => "bool".to_string(),
                    RuntimeValue::String(_) => "string".to_string(),
                    RuntimeValue::U8(_) => "u8".to_string(),
                    RuntimeValue::U16(_) => "u16".to_string(),
                    RuntimeValue::U32(_) => "u32".to_string(),
                    RuntimeValue::U64(_) => "u64".to_string(),
                    RuntimeValue::I8(_) => "i8".to_string(),
                    RuntimeValue::I16(_) => "i16".to_string(),
                    RuntimeValue::I32(_) => "i32".to_string(),
                    RuntimeValue::I64(_) => "i64".to_string(),
                    RuntimeValue::F32(_) => "f32".to_string(),
                    RuntimeValue::F64(_) => "f64".to_string(),
                    _ => "any".to_string(),
                }
            };
            RuntimeValue::RawArray(elem_type, arr.clone())
        }
        RuntimeValue::DynArray {
            data, element_type, ..
        } => RuntimeValue::RawArray(element_type.clone(), data.clone()),
        _ => args[0].clone(),
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Infer the element type and concrete type for an array
pub(super) fn infer_array_type(data: &[RuntimeValue]) -> (String, String) {
    if data.is_empty() {
        return ("any".to_string(), "any".to_string());
    }

    // Check if all elements are the same type
    let first_type = match &data[0] {
        RuntimeValue::U8(_) => "u8",
        RuntimeValue::U16(_) => "u16",
        RuntimeValue::U32(_) => "u32",
        RuntimeValue::U64(_) => "u64",
        RuntimeValue::U128(_) => "u128",
        RuntimeValue::I8(_) => "i8",
        RuntimeValue::I16(_) => "i16",
        RuntimeValue::I32(_) => "i32",
        RuntimeValue::I64(_) => "i64",
        RuntimeValue::I128(_) => "i128",
        RuntimeValue::F32(_) => "f32",
        RuntimeValue::F64(_) => "f64",
        RuntimeValue::Int(_) => "number",
        RuntimeValue::Float(_) => "number",
        RuntimeValue::Bool(_) => "bool",
        RuntimeValue::String(_) => "string",
        _ => "any",
    };

    // Check if all elements match the first type
    let all_same = data.iter().all(|v| {
        let v_type = match v {
            RuntimeValue::U8(_) => "u8",
            RuntimeValue::U16(_) => "u16",
            RuntimeValue::U32(_) => "u32",
            RuntimeValue::U64(_) => "u64",
            RuntimeValue::U128(_) => "u128",
            RuntimeValue::I8(_) => "i8",
            RuntimeValue::I16(_) => "i16",
            RuntimeValue::I32(_) => "i32",
            RuntimeValue::I64(_) => "i64",
            RuntimeValue::I128(_) => "i128",
            RuntimeValue::F32(_) => "f32",
            RuntimeValue::F64(_) => "f64",
            RuntimeValue::Int(_) => "number",
            RuntimeValue::Float(_) => "number",
            RuntimeValue::Bool(_) => "bool",
            RuntimeValue::String(_) => "string",
            _ => "any",
        };
        v_type == first_type
    });

    if all_same {
        (first_type.to_string(), first_type.to_string())
    } else {
        ("any".to_string(), "number".to_string()) // Mixed types default to number
    }
}

/// Compare two runtime values for equality (used by indexOf/lastIndexOf)
fn runtime_values_equal(a: &RuntimeValue, b: &RuntimeValue) -> bool {
    match (a, b) {
        (RuntimeValue::Int(x), RuntimeValue::Int(y)) => x == y,
        (RuntimeValue::Float(x), RuntimeValue::Float(y)) => x == y,
        (RuntimeValue::Int(x), RuntimeValue::Float(y)) => (*x as f64) == *y,
        (RuntimeValue::Float(x), RuntimeValue::Int(y)) => *x == (*y as f64),
        (RuntimeValue::Bool(x), RuntimeValue::Bool(y)) => x == y,
        (RuntimeValue::String(x), RuntimeValue::String(y)) => x == y,
        (RuntimeValue::Null, RuntimeValue::Null) => true,
        _ => false,
    }
}
