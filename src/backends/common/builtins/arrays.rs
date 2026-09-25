//! Array operations and utilities for JIT/AOT backends
//!
//! This module provides comprehensive array manipulation functions including:
//! - Basic properties: len, capacity, metadata_size, hasKey
//! - Mutation: push, pop, shift, unshift, insert, remove, clear
//! - Access: first, last, get_index, set_index
//! - Transformation: reverse, sort, slice, concat, flat, distinct, toSet, toTuple
//! - Search: index, indexOf, lastIndexOf, includes, contains, count
//! - Aggregation: sum, min, max
//! - Higher-order operations: map, filter, reduce, forEach, find, some, every
//! - Construction: make_array, make_array_spread, range
//! - Utilities: spread, array_to_raw

use super::{CURRENT_EXCEPTION, RuntimeValue};
use std::cmp::Ordering;

#[inline]
pub(crate) fn set_array_error(msg: &str) {
    CURRENT_EXCEPTION.with(|exc| {
        *exc.borrow_mut() = Some(RuntimeValue::String(msg.to_string()));
    });
}

#[inline]
pub(crate) fn resolve_idx(idx: i64, len: usize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let resolved = if idx < 0 { len as i64 + idx } else { idx };
    if resolved >= 0 && (resolved as usize) < len {
        Some(resolved as usize)
    } else {
        None
    }
}

pub(crate) fn compare_runtime_values(a: &RuntimeValue, b: &RuntimeValue) -> Ordering {
    match (a, b) {
        (RuntimeValue::Int(x), RuntimeValue::Int(y)) => x.cmp(y),
        (RuntimeValue::Float(x), RuntimeValue::Float(y)) => {
            x.partial_cmp(y).unwrap_or(Ordering::Equal)
        }
        (RuntimeValue::Int(x), RuntimeValue::Float(y)) => {
            (*x as f64).partial_cmp(y).unwrap_or(Ordering::Equal)
        }
        (RuntimeValue::Float(x), RuntimeValue::Int(y)) => {
            x.partial_cmp(&(*y as f64)).unwrap_or(Ordering::Equal)
        }
        (RuntimeValue::Bool(x), RuntimeValue::Bool(y)) => x.cmp(y),
        (RuntimeValue::String(x), RuntimeValue::String(y)) => x.cmp(y),
        (RuntimeValue::Char(x), RuntimeValue::Char(y)) => x.cmp(y),
        (RuntimeValue::U8(x), RuntimeValue::U8(y)) => x.cmp(y),
        (RuntimeValue::U16(x), RuntimeValue::U16(y)) => x.cmp(y),
        (RuntimeValue::U32(x), RuntimeValue::U32(y)) => x.cmp(y),
        (RuntimeValue::U64(x), RuntimeValue::U64(y)) => x.cmp(y),
        (RuntimeValue::I8(x), RuntimeValue::I8(y)) => x.cmp(y),
        (RuntimeValue::I16(x), RuntimeValue::I16(y)) => x.cmp(y),
        (RuntimeValue::I32(x), RuntimeValue::I32(y)) => x.cmp(y),
        (RuntimeValue::I64(x), RuntimeValue::I64(y)) => x.cmp(y),
        (RuntimeValue::F32(x), RuntimeValue::F32(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (RuntimeValue::F64(x), RuntimeValue::F64(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        _ => {
            if let (Some(x), Some(y)) = (a.as_float(), b.as_float()) {
                x.partial_cmp(&y).unwrap_or(Ordering::Equal)
            } else {
                Ordering::Equal
            }
        }
    }
}

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
        RuntimeValue::Set(s) => RuntimeValue::Int(s.len() as i64),
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
        RuntimeValue::RawArray(_, arr) => RuntimeValue::Int(arr.len() as i64),
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
        RuntimeValue::Array(arr) => {
            if arr.is_empty() {
                return RuntimeValue::Int(16);
            }
            let is_mixed = arr.iter().any(|v| match v {
                RuntimeValue::String(_)
                | RuntimeValue::Object(_)
                | RuntimeValue::Array(_)
                | RuntimeValue::DynArray { .. }
                | RuntimeValue::Tuple(_)
                | RuntimeValue::Null => true,
                _ => false,
            }) && arr.iter().any(|v| match v {
                RuntimeValue::Int(_)
                | RuntimeValue::Float(_)
                | RuntimeValue::U8(_)
                | RuntimeValue::I8(_)
                | RuntimeValue::U16(_)
                | RuntimeValue::I16(_)
                | RuntimeValue::U32(_)
                | RuntimeValue::I32(_)
                | RuntimeValue::F32(_)
                | RuntimeValue::Bool(_) => true,
                _ => false,
            });
            if is_mixed {
                return RuntimeValue::Int(24);
            }
            let has_large_elem = arr.iter().any(|v| match v {
                RuntimeValue::Float(_)
                | RuntimeValue::F64(_)
                | RuntimeValue::I64(_)
                | RuntimeValue::U64(_)
                | RuntimeValue::I128(_)
                | RuntimeValue::U128(_)
                | RuntimeValue::BigInt(_) => true,
                RuntimeValue::Int(n) => *n < i32::MIN as i64 || *n > i32::MAX as i64,
                _ => false,
            });
            if has_large_elem {
                RuntimeValue::Int(24)
            } else {
                RuntimeValue::Int(16)
            }
        }
        RuntimeValue::RawArray(_, _) => RuntimeValue::Int(0),
        RuntimeValue::DynArray {
            element_type,
            concrete_type,
            ..
        } => {
            let ty_str = if !concrete_type.is_empty() {
                concrete_type.as_str()
            } else {
                element_type.as_str()
            };
            let clean = ty_str.trim_start_matches('[').trim_end_matches(']');
            let metadata = if clean.starts_with("u8")
                || clean.starts_with("i8")
                || clean.starts_with("u16")
                || clean.starts_with("i16")
                || clean.starts_with("u32")
                || clean.starts_with("i32")
                || clean.starts_with("f32")
                || clean.starts_with("bool")
                || clean.starts_with("char")
            {
                16
            } else {
                24
            };
            RuntimeValue::Int(metadata)
        }
        RuntimeValue::String(_) => RuntimeValue::Int(24),
        RuntimeValue::Tuple(_) => RuntimeValue::Int(0),
        RuntimeValue::U64(handle) => {
            if let Some(rv) = super::objects::get_jit_arc_value(*handle) {
                runtime_metadata_size(&[rv])
            } else {
                RuntimeValue::Int(0)
            }
        }
        _ => RuntimeValue::Int(0),
    }
}

// ============================================================================
// Array Mutation (Immutable Value Semantics)
// ============================================================================

/// Push/append element to end of array
pub(crate) fn runtime_push(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return args.first().cloned().unwrap_or(RuntimeValue::Null);
    }
    match &args[0] {
        RuntimeValue::RawArray(_, _) => {
            set_array_error("Cannot append to raw array (fixed size)");
            args[0].clone()
        }
        RuntimeValue::DynArray {
            data,
            element_type,
            concrete_type,
            tracked_capacity,
        } => {
            if let Some(cap) = tracked_capacity {
                if data.len() >= *cap {
                    set_array_error(&format!(
                        "Cannot append to fixed-capacity array (capacity: {})",
                        cap
                    ));
                    return args[0].clone();
                }
            }
            let mut new_data = data.clone();
            new_data.push(args[1].clone());
            RuntimeValue::DynArray {
                data: new_data,
                element_type: element_type.clone(),
                concrete_type: concrete_type.clone(),
                tracked_capacity: *tracked_capacity,
            }
        }
        RuntimeValue::Array(arr) => {
            let mut new_arr = arr.clone();
            new_arr.push(args[1].clone());
            RuntimeValue::Array(new_arr)
        }
        _ => RuntimeValue::Null,
    }
}

/// Pop element from array: pop() or pop(index) -> returns updated array
pub(crate) fn runtime_pop(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }
    match &args[0] {
        RuntimeValue::RawArray(_, _) => {
            set_array_error("Cannot pop from raw array (fixed size)");
            args[0].clone()
        }
        RuntimeValue::DynArray {
            data,
            element_type,
            concrete_type,
            tracked_capacity,
        } => {
            let mut new_data = data.clone();
            if args.len() > 1 {
                if let Some(idx_raw) = args[1].as_int() {
                    if let Some(idx) = resolve_idx(idx_raw, new_data.len()) {
                        new_data.remove(idx);
                    }
                }
            } else {
                new_data.pop();
            }
            RuntimeValue::DynArray {
                data: new_data,
                element_type: element_type.clone(),
                concrete_type: concrete_type.clone(),
                tracked_capacity: *tracked_capacity,
            }
        }
        RuntimeValue::Array(arr) => {
            let mut new_arr = arr.clone();
            if args.len() > 1 {
                if let Some(idx_raw) = args[1].as_int() {
                    if let Some(idx) = resolve_idx(idx_raw, new_arr.len()) {
                        new_arr.remove(idx);
                    }
                }
            } else {
                new_arr.pop();
            }
            RuntimeValue::Array(new_arr)
        }
        _ => RuntimeValue::Null,
    }
}

/// Shift: remove first element from array and return updated array
pub(crate) fn runtime_shift(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }
    match &args[0] {
        RuntimeValue::RawArray(_, _) => {
            set_array_error("Cannot shift raw array (fixed size)");
            args[0].clone()
        }
        RuntimeValue::DynArray {
            data,
            element_type,
            concrete_type,
            tracked_capacity,
        } => {
            let mut new_data = data.clone();
            if !new_data.is_empty() {
                new_data.remove(0);
            }
            RuntimeValue::DynArray {
                data: new_data,
                element_type: element_type.clone(),
                concrete_type: concrete_type.clone(),
                tracked_capacity: *tracked_capacity,
            }
        }
        RuntimeValue::Array(arr) => {
            let mut new_arr = arr.clone();
            if !new_arr.is_empty() {
                new_arr.remove(0);
            }
            RuntimeValue::Array(new_arr)
        }
        _ => RuntimeValue::Null,
    }
}

/// Unshift: insert element at start of array and return updated array
pub(crate) fn runtime_unshift(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }
    let val_to_insert = if args.len() > 1 {
        args[1].clone()
    } else {
        RuntimeValue::Null
    };

    match &args[0] {
        RuntimeValue::RawArray(_, _) => {
            set_array_error("Cannot unshift raw array (fixed size)");
            args[0].clone()
        }
        RuntimeValue::DynArray {
            data,
            element_type,
            concrete_type,
            tracked_capacity,
        } => {
            if let Some(cap) = tracked_capacity {
                if data.len() >= *cap {
                    set_array_error(&format!(
                        "Cannot unshift to fixed-capacity array (capacity: {})",
                        cap
                    ));
                    return args[0].clone();
                }
            }
            let mut new_data = Vec::with_capacity(data.len() + 1);
            new_data.push(val_to_insert);
            new_data.extend(data.iter().cloned());
            RuntimeValue::DynArray {
                data: new_data,
                element_type: element_type.clone(),
                concrete_type: concrete_type.clone(),
                tracked_capacity: *tracked_capacity,
            }
        }
        RuntimeValue::Array(arr) => {
            let mut new_arr = Vec::with_capacity(arr.len() + 1);
            new_arr.push(val_to_insert);
            new_arr.extend(arr.iter().cloned());
            RuntimeValue::Array(new_arr)
        }
        _ => RuntimeValue::Null,
    }
}

/// Insert element at index: insert(arr, index, value) or arr.insert(index, value)
pub(crate) fn runtime_insert(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 3 {
        return args.first().cloned().unwrap_or(RuntimeValue::Null);
    }
    let index_raw = args[1].as_int().unwrap_or(0);
    let val = args[2].clone();

    match &args[0] {
        RuntimeValue::RawArray(_, _) => {
            set_array_error("Cannot insert into raw array (fixed size)");
            args[0].clone()
        }
        RuntimeValue::DynArray {
            data,
            element_type,
            concrete_type,
            tracked_capacity,
        } => {
            if let Some(cap) = tracked_capacity {
                if data.len() >= *cap {
                    set_array_error(&format!(
                        "Cannot insert into fixed-capacity array (capacity: {})",
                        cap
                    ));
                    return args[0].clone();
                }
            }
            let mut new_data = data.clone();
            let idx = if index_raw < 0 {
                (new_data.len() as i64 + index_raw).max(0) as usize
            } else {
                (index_raw as usize).min(new_data.len())
            };
            new_data.insert(idx, val);
            RuntimeValue::DynArray {
                data: new_data,
                element_type: element_type.clone(),
                concrete_type: concrete_type.clone(),
                tracked_capacity: *tracked_capacity,
            }
        }
        RuntimeValue::Array(arr) => {
            let mut new_arr = arr.clone();
            let idx = if index_raw < 0 {
                (new_arr.len() as i64 + index_raw).max(0) as usize
            } else {
                (index_raw as usize).min(new_arr.len())
            };
            new_arr.insert(idx, val);
            RuntimeValue::Array(new_arr)
        }
        _ => RuntimeValue::Null,
    }
}

/// Remove element at index: remove(arr, index) or arr.remove(index)
pub(crate) fn runtime_remove(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return args.first().cloned().unwrap_or(RuntimeValue::Null);
    }
    let index_raw = args[1].as_int().unwrap_or(0);

    match &args[0] {
        RuntimeValue::RawArray(_, _) => {
            set_array_error("Cannot remove from raw array (fixed size)");
            args[0].clone()
        }
        RuntimeValue::DynArray {
            data,
            element_type,
            concrete_type,
            tracked_capacity,
        } => {
            let mut new_data = data.clone();
            if let Some(idx) = resolve_idx(index_raw, new_data.len()) {
                new_data.remove(idx);
            }
            RuntimeValue::DynArray {
                data: new_data,
                element_type: element_type.clone(),
                concrete_type: concrete_type.clone(),
                tracked_capacity: *tracked_capacity,
            }
        }
        RuntimeValue::Array(arr) => {
            let mut new_arr = arr.clone();
            if let Some(idx) = resolve_idx(index_raw, new_arr.len()) {
                new_arr.remove(idx);
            }
            RuntimeValue::Array(new_arr)
        }
        _ => RuntimeValue::Null,
    }
}

/// Clear array
pub(crate) fn runtime_clear(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }
    match &args[0] {
        RuntimeValue::RawArray(_, _) => {
            set_array_error("Cannot clear raw array (fixed size)");
            args[0].clone()
        }
        RuntimeValue::DynArray {
            element_type,
            concrete_type,
            tracked_capacity,
            ..
        } => RuntimeValue::DynArray {
            data: vec![],
            element_type: element_type.clone(),
            concrete_type: concrete_type.clone(),
            tracked_capacity: *tracked_capacity,
        },
        RuntimeValue::Array(_) => RuntimeValue::Array(vec![]),
        _ => RuntimeValue::Null,
    }
}

#[inline]
pub(crate) fn coerce_to_typed_val(val: RuntimeValue, ty_str: &str) -> RuntimeValue {
    let clean_ty = ty_str
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(';')
        .next()
        .unwrap_or(ty_str)
        .trim();
    let int_opt = match val {
        RuntimeValue::Int(n) => Some(n),
        RuntimeValue::U8(n) => Some(n as i64),
        RuntimeValue::U16(n) => Some(n as i64),
        RuntimeValue::U32(n) => Some(n as i64),
        RuntimeValue::U64(n) => Some(n as i64),
        RuntimeValue::I8(n) => Some(n as i64),
        RuntimeValue::I16(n) => Some(n as i64),
        RuntimeValue::I32(n) => Some(n as i64),
        RuntimeValue::I64(n) => Some(n),
        _ => None,
    };
    if let Some(n) = int_opt {
        match clean_ty {
            "u8" => RuntimeValue::U8(n as u8),
            "u16" => RuntimeValue::U16(n as u16),
            "u32" => RuntimeValue::U32(n as u32),
            "u64" => RuntimeValue::U64(n as u64),
            "i8" => RuntimeValue::I8(n as i8),
            "i16" => RuntimeValue::I16(n as i16),
            "i32" => RuntimeValue::I32(n as i32),
            "i64" => RuntimeValue::I64(n),
            _ => RuntimeValue::Int(n),
        }
    } else {
        val
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
        RuntimeValue::RawArray(elem_ty, arr) => {
            let elem = arr.first().cloned().unwrap_or(RuntimeValue::Null);
            coerce_to_typed_val(elem, elem_ty.as_str())
        }
        RuntimeValue::DynArray {
            data,
            concrete_type,
            element_type,
            ..
        } => {
            let elem = data.first().cloned().unwrap_or(RuntimeValue::Null);
            let ty_str = if !concrete_type.is_empty() {
                concrete_type.as_str()
            } else {
                element_type.as_str()
            };
            coerce_to_typed_val(elem, ty_str)
        }
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
        RuntimeValue::RawArray(elem_ty, arr) => {
            let elem = arr.last().cloned().unwrap_or(RuntimeValue::Null);
            coerce_to_typed_val(elem, elem_ty.as_str())
        }
        RuntimeValue::DynArray {
            data,
            concrete_type,
            element_type,
            ..
        } => {
            let elem = data.last().cloned().unwrap_or(RuntimeValue::Null);
            let ty_str = if !concrete_type.is_empty() {
                concrete_type.as_str()
            } else {
                element_type.as_str()
            };
            coerce_to_typed_val(elem, ty_str)
        }
        RuntimeValue::Tuple(t) => t.last().cloned().unwrap_or(RuntimeValue::Null),
        RuntimeValue::String(s) => s
            .chars()
            .last()
            .map(|c| RuntimeValue::String(c.to_string()))
            .unwrap_or(RuntimeValue::Null),
        _ => RuntimeValue::Null,
    }
}

/// Get element at index (with negative index resolution)
pub(crate) fn runtime_get_index(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Null;
    }
    let index_raw = args[1].as_int().unwrap_or(0);
    match &args[0] {
        RuntimeValue::Array(arr) => resolve_idx(index_raw, arr.len())
            .and_then(|i| arr.get(i))
            .cloned()
            .unwrap_or(RuntimeValue::Null),
        RuntimeValue::RawArray(elem_ty, arr) => {
            let elem = resolve_idx(index_raw, arr.len())
                .and_then(|i| arr.get(i))
                .cloned()
                .unwrap_or(RuntimeValue::Null);
            coerce_to_typed_val(elem, elem_ty.as_str())
        }
        RuntimeValue::DynArray {
            data,
            concrete_type,
            element_type,
            ..
        } => {
            let elem = resolve_idx(index_raw, data.len())
                .and_then(|i| data.get(i))
                .cloned()
                .unwrap_or(RuntimeValue::Null);
            let ty_str = if !concrete_type.is_empty() {
                concrete_type.as_str()
            } else {
                element_type.as_str()
            };
            coerce_to_typed_val(elem, ty_str)
        }
        RuntimeValue::Tuple(tup) => resolve_idx(index_raw, tup.len())
            .and_then(|i| tup.get(i))
            .cloned()
            .unwrap_or(RuntimeValue::Null),
        RuntimeValue::String(s) => {
            let chars: Vec<char> = s.chars().collect();
            resolve_idx(index_raw, chars.len())
                .and_then(|i| chars.get(i))
                .map(|c| RuntimeValue::String(c.to_string()))
                .unwrap_or(RuntimeValue::Null)
        }
        RuntimeValue::Object(obj) => {
            let key = match &args[1] {
                RuntimeValue::String(k) => k.clone(),
                RuntimeValue::Int(n) => n.to_string(),
                _ => String::new(),
            };
            obj.get(&key).cloned().unwrap_or(RuntimeValue::Null)
        }
        _ => RuntimeValue::Null,
    }
}

/// Set element at index (with bounds check & negative index resolution)
pub(crate) fn runtime_set_index(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 3 {
        return args.first().cloned().unwrap_or(RuntimeValue::Null);
    }
    let index_raw = args[1].as_int().unwrap_or(0);
    let val = args[2].clone();

    match &args[0] {
        RuntimeValue::Array(arr) => {
            let mut new_arr = arr.clone();
            if let Some(idx) = resolve_idx(index_raw, new_arr.len()) {
                new_arr[idx] = val;
                RuntimeValue::Array(new_arr)
            } else {
                set_array_error("index out of bounds");
                args[0].clone()
            }
        }
        RuntimeValue::RawArray(elem_type, arr) => {
            let mut new_arr = arr.clone();
            if let Some(idx) = resolve_idx(index_raw, new_arr.len()) {
                new_arr[idx] = val;
                RuntimeValue::RawArray(elem_type.clone(), new_arr)
            } else {
                set_array_error("index out of bounds");
                args[0].clone()
            }
        }
        RuntimeValue::DynArray {
            data,
            element_type,
            concrete_type,
            tracked_capacity,
        } => {
            let mut new_data = data.clone();
            if let Some(idx) = resolve_idx(index_raw, new_data.len()) {
                new_data[idx] = val;
                RuntimeValue::DynArray {
                    data: new_data,
                    element_type: element_type.clone(),
                    concrete_type: concrete_type.clone(),
                    tracked_capacity: *tracked_capacity,
                }
            } else {
                set_array_error("index out of bounds");
                args[0].clone()
            }
        }
        RuntimeValue::Tuple(_) => args[0].clone(),
        _ => RuntimeValue::Null,
    }
}

// ============================================================================
// Array Transformation & Aggregation
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
        RuntimeValue::DynArray {
            data,
            element_type,
            concrete_type,
            tracked_capacity,
        } => {
            let mut new_data = data.clone();
            new_data.reverse();
            RuntimeValue::DynArray {
                data: new_data,
                element_type: element_type.clone(),
                concrete_type: concrete_type.clone(),
                tracked_capacity: *tracked_capacity,
            }
        }
        RuntimeValue::RawArray(elem_type, arr) => {
            let mut new_arr = arr.clone();
            new_arr.reverse();
            RuntimeValue::RawArray(elem_type.clone(), new_arr)
        }
        RuntimeValue::String(s) => RuntimeValue::String(s.chars().rev().collect()),
        _ => RuntimeValue::Null,
    }
}

/// Sort array elements in ascending order
pub(crate) fn runtime_sort(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }
    match &args[0] {
        RuntimeValue::Array(arr) => {
            let mut new_arr = arr.clone();
            new_arr.sort_by(compare_runtime_values);
            RuntimeValue::Array(new_arr)
        }
        RuntimeValue::DynArray {
            data,
            element_type,
            concrete_type,
            tracked_capacity,
        } => {
            let mut new_data = data.clone();
            new_data.sort_by(compare_runtime_values);
            RuntimeValue::DynArray {
                data: new_data,
                element_type: element_type.clone(),
                concrete_type: concrete_type.clone(),
                tracked_capacity: *tracked_capacity,
            }
        }
        RuntimeValue::RawArray(elem_type, arr) => {
            let mut new_arr = arr.clone();
            new_arr.sort_by(compare_runtime_values);
            RuntimeValue::RawArray(elem_type.clone(), new_arr)
        }
        _ => args[0].clone(),
    }
}

/// Slice array: slice(arr, start, end?) -> new sliced array
pub(crate) fn runtime_slice_array(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }
    let start_raw = args.get(1).and_then(|v| v.as_int()).unwrap_or(0);
    let end_raw = args.get(2).and_then(|v| v.as_int());

    match &args[0] {
        RuntimeValue::Array(arr) => {
            let len = arr.len() as i64;
            let start_idx = (if start_raw < 0 {
                (len + start_raw).max(0)
            } else {
                start_raw.min(len)
            }) as usize;
            let end_idx = if let Some(e) = end_raw {
                (if e < 0 { (len + e).max(0) } else { e.min(len) }) as usize
            } else {
                arr.len()
            };
            if start_idx >= arr.len() || start_idx >= end_idx {
                RuntimeValue::Array(vec![])
            } else {
                RuntimeValue::Array(arr[start_idx..end_idx.min(arr.len())].to_vec())
            }
        }
        RuntimeValue::DynArray {
            data,
            element_type,
            concrete_type,
            ..
        } => {
            let len = data.len() as i64;
            let start_idx = (if start_raw < 0 {
                (len + start_raw).max(0)
            } else {
                start_raw.min(len)
            }) as usize;
            let end_idx = if let Some(e) = end_raw {
                (if e < 0 { (len + e).max(0) } else { e.min(len) }) as usize
            } else {
                data.len()
            };
            let sliced = if start_idx >= data.len() || start_idx >= end_idx {
                vec![]
            } else {
                data[start_idx..end_idx.min(data.len())].to_vec()
            };
            RuntimeValue::DynArray {
                data: sliced,
                element_type: element_type.clone(),
                concrete_type: concrete_type.clone(),
                tracked_capacity: None,
            }
        }
        RuntimeValue::String(s) => {
            let chars: Vec<char> = s.chars().collect();
            let len = chars.len() as i64;
            let start_idx = (if start_raw < 0 {
                (len + start_raw).max(0)
            } else {
                start_raw.min(len)
            }) as usize;
            let end_idx = if let Some(e) = end_raw {
                (if e < 0 { (len + e).max(0) } else { e.min(len) }) as usize
            } else {
                chars.len()
            };
            if start_idx >= chars.len() || start_idx >= end_idx {
                RuntimeValue::String(String::new())
            } else {
                RuntimeValue::String(chars[start_idx..end_idx.min(chars.len())].iter().collect())
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
        RuntimeValue::DynArray { data, .. } => data.clone(),
        RuntimeValue::RawArray(_, arr) => arr.clone(),
        RuntimeValue::Tuple(tup) => tup.clone(),
        _ => vec![args[0].clone()],
    };

    for arg in &args[1..] {
        match arg {
            RuntimeValue::Array(arr) => result.extend(arr.clone()),
            RuntimeValue::DynArray { data, .. } => result.extend(data.clone()),
            RuntimeValue::RawArray(_, arr) => result.extend(arr.clone()),
            RuntimeValue::Tuple(tup) => result.extend(tup.clone()),
            _ => result.push(arg.clone()),
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

/// Flatten array by depth
pub(crate) fn runtime_array_flat(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Array(vec![]);
    }

    let array = match &args[0] {
        RuntimeValue::Array(arr) => arr.clone(),
        RuntimeValue::DynArray { data, .. } => data.clone(),
        _ => return args[0].clone(),
    };

    let depth = args.get(1).and_then(|v| v.as_int()).unwrap_or(1);
    if depth <= 0 {
        return RuntimeValue::Array(array);
    }

    let mut result = Vec::new();
    for item in array {
        match item {
            RuntimeValue::Array(inner) => result.extend(inner),
            RuntimeValue::DynArray { data, .. } => result.extend(data),
            _ => result.push(item),
        }
    }

    RuntimeValue::Array(result)
}

/// Count occurrences of item in array
pub(crate) fn runtime_count(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Int(0);
    }
    let needle = &args[1];
    let count = match &args[0] {
        RuntimeValue::Array(arr) => arr
            .iter()
            .filter(|v| runtime_values_equal(v, needle))
            .count(),
        RuntimeValue::RawArray(_, arr) => arr
            .iter()
            .filter(|v| runtime_values_equal(v, needle))
            .count(),
        RuntimeValue::DynArray { data, .. } => data
            .iter()
            .filter(|v| runtime_values_equal(v, needle))
            .count(),
        RuntimeValue::Tuple(tup) => tup
            .iter()
            .filter(|v| runtime_values_equal(v, needle))
            .count(),
        _ => 0,
    };
    RuntimeValue::Int(count as i64)
}

/// Sum elements in numeric array
pub(crate) fn runtime_sum(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Int(0);
    }
    let items = match &args[0] {
        RuntimeValue::Array(arr) => arr.as_slice(),
        RuntimeValue::DynArray { data, .. } => data.as_slice(),
        RuntimeValue::RawArray(_, arr) => arr.as_slice(),
        RuntimeValue::Tuple(tup) => tup.as_slice(),
        _ => return RuntimeValue::Int(0),
    };

    let mut sum_f64 = 0.0;
    let mut all_int = true;
    let mut sum_i64: i64 = 0;
    for item in items {
        if let Some(n) = item.as_int() {
            sum_i64 = sum_i64.wrapping_add(n);
            sum_f64 += n as f64;
        } else if let Some(f) = item.as_float() {
            all_int = false;
            sum_f64 += f;
        }
    }
    if all_int {
        RuntimeValue::Int(sum_i64)
    } else {
        RuntimeValue::Float(sum_f64)
    }
}

/// Minimum element in array
pub(crate) fn runtime_array_min(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }
    let items = match &args[0] {
        RuntimeValue::Array(arr) => arr.as_slice(),
        RuntimeValue::DynArray { data, .. } => data.as_slice(),
        RuntimeValue::RawArray(_, arr) => arr.as_slice(),
        RuntimeValue::Tuple(tup) => tup.as_slice(),
        _ => return RuntimeValue::Null,
    };
    if items.is_empty() {
        return RuntimeValue::Null;
    }
    let mut best = items[0].clone();
    for item in &items[1..] {
        if compare_runtime_values(item, &best) == Ordering::Less {
            best = item.clone();
        }
    }
    best
}

/// Maximum element in array
pub(crate) fn runtime_array_max(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }
    let items = match &args[0] {
        RuntimeValue::Array(arr) => arr.as_slice(),
        RuntimeValue::DynArray { data, .. } => data.as_slice(),
        RuntimeValue::RawArray(_, arr) => arr.as_slice(),
        RuntimeValue::Tuple(tup) => tup.as_slice(),
        _ => return RuntimeValue::Null,
    };
    if items.is_empty() {
        return RuntimeValue::Null;
    }
    let mut best = items[0].clone();
    for item in &items[1..] {
        if compare_runtime_values(item, &best) == Ordering::Greater {
            best = item.clone();
        }
    }
    best
}

/// Unique elements in array
pub(crate) fn runtime_distinct(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Array(vec![]);
    }
    let items = match &args[0] {
        RuntimeValue::Array(arr) => arr.as_slice(),
        RuntimeValue::DynArray { data, .. } => data.as_slice(),
        RuntimeValue::RawArray(_, arr) => arr.as_slice(),
        RuntimeValue::Tuple(tup) => tup.as_slice(),
        _ => return args[0].clone(),
    };
    let mut result = Vec::new();
    for item in items {
        if !result.iter().any(|v| runtime_values_equal(v, item)) {
            result.push(item.clone());
        }
    }
    match &args[0] {
        RuntimeValue::DynArray {
            element_type,
            concrete_type,
            ..
        } => RuntimeValue::DynArray {
            data: result,
            element_type: element_type.clone(),
            concrete_type: concrete_type.clone(),
            tracked_capacity: None,
        },
        _ => RuntimeValue::Array(result),
    }
}

/// Convert array to Set
pub(crate) fn runtime_to_set(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Set(vec![]);
    }
    let items = match &args[0] {
        RuntimeValue::Array(arr) => arr.as_slice(),
        RuntimeValue::DynArray { data, .. } => data.as_slice(),
        RuntimeValue::RawArray(_, arr) => arr.as_slice(),
        RuntimeValue::Tuple(tup) => tup.as_slice(),
        _ => return RuntimeValue::Set(vec![args[0].clone()]),
    };
    let mut result = Vec::new();
    for item in items {
        if !result.iter().any(|v| runtime_values_equal(v, item)) {
            result.push(item.clone());
        }
    }
    RuntimeValue::Set(result)
}

/// Convert array to Tuple
pub(crate) fn runtime_to_tuple(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Tuple(vec![]);
    }
    let items = match &args[0] {
        RuntimeValue::Array(arr) => arr.clone(),
        RuntimeValue::DynArray { data, .. } => data.clone(),
        RuntimeValue::RawArray(_, arr) => arr.clone(),
        RuntimeValue::Tuple(tup) => tup.clone(),
        _ => vec![args[0].clone()],
    };
    RuntimeValue::Tuple(items)
}

// ============================================================================
// Array Search
// ============================================================================

/// Find first index of element in array
pub(crate) fn runtime_array_index_of(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Int(-1);
    }
    let needle = &args[1];
    let idx = match &args[0] {
        RuntimeValue::Array(arr) => arr.iter().position(|v| runtime_values_equal(v, needle)),
        RuntimeValue::RawArray(_, arr) => arr.iter().position(|v| runtime_values_equal(v, needle)),
        RuntimeValue::DynArray { data, .. } => {
            data.iter().position(|v| runtime_values_equal(v, needle))
        }
        RuntimeValue::Tuple(tup) => tup.iter().position(|v| runtime_values_equal(v, needle)),
        _ => None,
    };
    RuntimeValue::Int(idx.map(|i| i as i64).unwrap_or(-1))
}

/// Find last index of element in array
pub(crate) fn runtime_array_last_index_of(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Int(-1);
    }
    let needle = &args[1];
    let idx = match &args[0] {
        RuntimeValue::Array(arr) => arr.iter().rposition(|v| runtime_values_equal(v, needle)),
        RuntimeValue::RawArray(_, arr) => arr.iter().rposition(|v| runtime_values_equal(v, needle)),
        RuntimeValue::DynArray { data, .. } => {
            data.iter().rposition(|v| runtime_values_equal(v, needle))
        }
        RuntimeValue::Tuple(tup) => tup.iter().rposition(|v| runtime_values_equal(v, needle)),
        _ => None,
    };
    RuntimeValue::Int(idx.map(|i| i as i64).unwrap_or(-1))
}

/// Check if array contains element
pub(crate) fn runtime_array_includes(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Bool(false);
    }
    let needle = &args[1];
    let found = match &args[0] {
        RuntimeValue::Array(arr) => arr.iter().any(|v| runtime_values_equal(v, needle)),
        RuntimeValue::RawArray(_, arr) => arr.iter().any(|v| runtime_values_equal(v, needle)),
        RuntimeValue::DynArray { data, .. } => data.iter().any(|v| runtime_values_equal(v, needle)),
        RuntimeValue::Tuple(tup) => tup.iter().any(|v| runtime_values_equal(v, needle)),
        RuntimeValue::String(s) => s.contains(&needle.as_string()),
        _ => false,
    };
    RuntimeValue::Bool(found)
}

// ============================================================================
// Higher-Order Array Operations
// ============================================================================

pub(crate) fn runtime_for_each(_args: &[RuntimeValue]) -> RuntimeValue {
    RuntimeValue::Null
}

pub(crate) fn runtime_find(_args: &[RuntimeValue]) -> RuntimeValue {
    RuntimeValue::Null
}

pub(crate) fn runtime_find_index(_args: &[RuntimeValue]) -> RuntimeValue {
    RuntimeValue::Int(-1)
}

pub(crate) fn runtime_some(_args: &[RuntimeValue]) -> RuntimeValue {
    RuntimeValue::Bool(false)
}

pub(crate) fn runtime_every(_args: &[RuntimeValue]) -> RuntimeValue {
    RuntimeValue::Bool(true)
}

pub(crate) fn runtime_map(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }
    args[0].clone()
}

pub(crate) fn runtime_filter(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }
    args[0].clone()
}

pub(crate) fn runtime_reduce(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() >= 3 {
        args[2].clone()
    } else if !args.is_empty() {
        match &args[0] {
            RuntimeValue::Array(a) => a.first().cloned().unwrap_or(RuntimeValue::Null),
            RuntimeValue::DynArray { data, .. } => {
                data.first().cloned().unwrap_or(RuntimeValue::Null)
            }
            _ => RuntimeValue::Null,
        }
    } else {
        RuntimeValue::Null
    }
}

// ============================================================================
// Array Construction & Spread
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

    let mut i = 0;
    while i + 1 < args.len() {
        let value = &args[i];
        let is_spread = match &args[i + 1] {
            RuntimeValue::Bool(b) => *b,
            _ => false,
        };

        if is_spread {
            match value {
                RuntimeValue::Array(arr) => result.extend(arr.iter().cloned()),
                RuntimeValue::DynArray { data, .. } => result.extend(data.iter().cloned()),
                RuntimeValue::RawArray(_, arr) => result.extend(arr.iter().cloned()),
                RuntimeValue::Tuple(tup) => result.extend(tup.iter().cloned()),
                _ => result.push(value.clone()),
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

/// Spread operator - returns array as-is for JIT
pub(crate) fn runtime_spread(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Array(vec![]);
    }
    args[0].clone()
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Infer the element type and concrete type for an array
pub(crate) fn infer_array_type(data: &[RuntimeValue]) -> (String, String) {
    if data.is_empty() {
        return ("number".to_string(), "number".to_string());
    }

    // Check if all elements are integers (either Int or typed integer variants)
    let all_ints = data.iter().all(|v| match v {
        RuntimeValue::Int(_)
        | RuntimeValue::U8(_)
        | RuntimeValue::U16(_)
        | RuntimeValue::U32(_)
        | RuntimeValue::U64(_)
        | RuntimeValue::I8(_)
        | RuntimeValue::I16(_)
        | RuntimeValue::I32(_)
        | RuntimeValue::I64(_) => true,
        _ => false,
    });

    if all_ints {
        let int_vals: Vec<i64> = data
            .iter()
            .map(|v| match v {
                RuntimeValue::Int(n) => *n,
                RuntimeValue::U8(n) => *n as i64,
                RuntimeValue::U16(n) => *n as i64,
                RuntimeValue::U32(n) => *n as i64,
                RuntimeValue::U64(n) => *n as i64,
                RuntimeValue::I8(n) => *n as i64,
                RuntimeValue::I16(n) => *n as i64,
                RuntimeValue::I32(n) => *n as i64,
                RuntimeValue::I64(n) => *n,
                _ => 0,
            })
            .collect();

        let min = *int_vals.iter().min().unwrap_or(&0);
        let max = *int_vals.iter().max().unwrap_or(&0);

        let concrete = if min >= 0 {
            if max <= u8::MAX as i64 {
                "u8"
            } else if max <= u16::MAX as i64 {
                "u16"
            } else if max <= u32::MAX as i64 {
                "u32"
            } else {
                "u64"
            }
        } else if min >= i8::MIN as i64 && max <= i8::MAX as i64 {
            "i8"
        } else if min >= i16::MIN as i64 && max <= i16::MAX as i64 {
            "i16"
        } else if min >= i32::MIN as i64 && max <= i32::MAX as i64 {
            "i32"
        } else {
            "i64"
        };

        return (concrete.to_string(), concrete.to_string());
    }

    // Check if all elements are floats
    if data.iter().all(|v| {
        matches!(
            v,
            RuntimeValue::Float(_) | RuntimeValue::F64(_) | RuntimeValue::F32(_)
        )
    }) {
        return ("f64".to_string(), "f64".to_string());
    }

    // Check if all elements are booleans
    if data.iter().all(|v| matches!(v, RuntimeValue::Bool(_))) {
        return ("bool".to_string(), "bool".to_string());
    }

    // Check if all elements are strings
    if data.iter().all(|v| matches!(v, RuntimeValue::String(_))) {
        return ("string".to_string(), "string".to_string());
    }

    // Check if all elements are chars
    if data.iter().all(|v| matches!(v, RuntimeValue::Char(_))) {
        return ("char".to_string(), "char".to_string());
    }

    ("number".to_string(), "number".to_string())
}

/// Compare two runtime values for equality
pub(crate) fn runtime_values_equal(a: &RuntimeValue, b: &RuntimeValue) -> bool {
    match (a, b) {
        (RuntimeValue::Int(x), RuntimeValue::Int(y)) => x == y,
        (RuntimeValue::Float(x), RuntimeValue::Float(y)) => x == y,
        (RuntimeValue::Int(x), RuntimeValue::Float(y)) => (*x as f64) == *y,
        (RuntimeValue::Float(x), RuntimeValue::Int(y)) => *x == (*y as f64),
        (RuntimeValue::Bool(x), RuntimeValue::Bool(y)) => x == y,
        (RuntimeValue::String(x), RuntimeValue::String(y)) => x == y,
        (RuntimeValue::Char(x), RuntimeValue::Char(y)) => x == y,
        (RuntimeValue::Null, RuntimeValue::Null) => true,
        _ => {
            if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
                x == y
            } else if let (Some(x), Some(y)) = (a.as_float(), b.as_float()) {
                x == y
            } else {
                false
            }
        }
    }
}
