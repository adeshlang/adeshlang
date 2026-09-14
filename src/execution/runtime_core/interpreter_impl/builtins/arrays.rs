//! Array Builtin Methods
//!
//! This module contains built-in array manipulation methods for the language.
//! Provides implementations for simple array operations that don't require
//! callback functions or interpreter context.
//!
//! ## Methods Implemented
//!
//! ### Mutating Methods (immutable in the language)
//! - `push(element)`: Add element to end
//! - `pop()`: Remove last element
//! - `shift()`: Remove first element
//! - `unshift(element)`: Add element to start
//! - `append(element)`: Alias for push
//!
//! ### Search Methods
//! - `indexOf(value)`: Find index of value (-1 if not found)
//! - `includes(value)`: Check if array contains value
//! - `contains(value)`: Alias for includes
//!
//! ### Transformation Methods
//! - `join(separator)`: Join array elements into string
//! - `concat(...items)`: Concatenate arrays/values
//! - `slice(start, end)`: Extract subarray
//! - `flat()`: Flatten array one level
//!
//! ## Note
//!
//! Array methods with higher-order functions (map, filter, reduce, find, findIndex,
//! forEach, some, every) remain in interpreter_core.rs as they require access to
//! the full interpreter context for function execution.

use super::super::super::format::fmt;
use super::super::super::interpreter::err;
use super::super::super::ops::equals;
use crate::parsing::ast::Value;
use num_traits::ToPrimitive;
use std::cmp::Ordering;

/// Call simple array builtin method (no callbacks)
///
/// Methods that require callbacks (map, filter, etc.) must remain in interpreter_core
pub fn call_simple_array_method(
    arr_ref: &[Value],
    method_name: &str,
    args: &[Value],
) -> Result<Value, String> {
    fn value_to_f64(value: &Value) -> Option<f64> {
        match value {
            Value::Number(n) => Some(*n),
            Value::BigInt(bi) => bi.to_f64(),
            Value::U8(n) => Some(*n as f64),
            Value::U16(n) => Some(*n as f64),
            Value::U32(n) => Some(*n as f64),
            Value::U64(n) => Some(*n as f64),
            Value::U128(n) => Some(*n as f64),
            Value::I8(n) => Some(*n as f64),
            Value::I16(n) => Some(*n as f64),
            Value::I32(n) => Some(*n as f64),
            Value::I64(n) => Some(*n as f64),
            Value::I128(n) => Some(*n as f64),
            Value::F32(n) => Some(*n as f64),
            Value::F64(n) => Some(*n),
            _ => None,
        }
    }

    fn compare_values(a: &Value, b: &Value) -> Ordering {
        match (a, b) {
            (Value::Number(na), Value::Number(nb)) => na.partial_cmp(nb).unwrap_or(Ordering::Equal),
            (Value::F64(na), Value::F64(nb)) => na.partial_cmp(nb).unwrap_or(Ordering::Equal),
            (Value::F32(na), Value::F32(nb)) => na.partial_cmp(nb).unwrap_or(Ordering::Equal),
            (Value::I64(na), Value::I64(nb)) => na.cmp(nb),
            (Value::I32(na), Value::I32(nb)) => na.cmp(nb),
            (Value::I16(na), Value::I16(nb)) => na.cmp(nb),
            (Value::I8(na), Value::I8(nb)) => na.cmp(nb),
            (Value::U64(na), Value::U64(nb)) => na.cmp(nb),
            (Value::U32(na), Value::U32(nb)) => na.cmp(nb),
            (Value::U16(na), Value::U16(nb)) => na.cmp(nb),
            (Value::U8(na), Value::U8(nb)) => na.cmp(nb),
            (Value::BigInt(na), Value::BigInt(nb)) => na.cmp(nb),
            (Value::Str(sa), Value::Str(sb)) => sa.cmp(sb),
            (Value::Bool(ba), Value::Bool(bb)) => ba.cmp(bb),
            (Value::Char(ca), Value::Char(cb)) => ca.cmp(cb),
            _ => match (value_to_f64(a), value_to_f64(b)) {
                (Some(na), Some(nb)) => na.partial_cmp(&nb).unwrap_or(Ordering::Equal),
                _ => fmt(a).cmp(&fmt(b)),
            },
        }
    }

    match method_name {
        "length" | "len" => Ok(Value::Number(arr_ref.len() as f64)),
        "isEmpty" | "is_empty" => Ok(Value::Bool(arr_ref.is_empty())),
        "first" => Ok(arr_ref.first().cloned().unwrap_or(Value::Null)),
        "last" => Ok(arr_ref.last().cloned().unwrap_or(Value::Null)),
        "push" => {
            if args.is_empty() {
                return Err(err("push(element)".to_string()));
            }
            let mut result = arr_ref.to_vec();
            result.push(args[0].clone());
            Ok(Value::Array(result))
        }
        "pop" => {
            let mut result = arr_ref.to_vec();
            if !args.is_empty() {
                let idx = match &args[0] {
                    Value::Number(n) => *n as i32,
                    Value::I64(n) => *n as i32,
                    Value::U64(n) => *n as i32,
                    Value::I32(n) => *n,
                    Value::U32(n) => *n as i32,
                    _ => result.len() as i32 - 1,
                };
                if idx >= 0 && (idx as usize) < result.len() {
                    result.remove(idx as usize);
                }
            } else {
                result.pop();
            }
            Ok(Value::Array(result))
        }
        "shift" => {
            let mut result = arr_ref.to_vec();
            if !result.is_empty() {
                result.remove(0);
            }
            Ok(Value::Array(result))
        }
        "unshift" => {
            if args.is_empty() {
                return Err(err("unshift(element)".to_string()));
            }
            let mut result = Vec::with_capacity(arr_ref.len() + 1);
            result.push(args[0].clone());
            result.extend_from_slice(arr_ref);
            Ok(Value::Array(result))
        }
        "insert" => {
            if args.len() < 2 {
                return Err(err("insert(index, value)".to_string()));
            }
            let mut result = arr_ref.to_vec();
            let idx = match &args[0] {
                Value::Number(n) => *n as i32,
                Value::I64(n) => *n as i32,
                Value::U64(n) => *n as i32,
                Value::I32(n) => *n,
                Value::U32(n) => *n as i32,
                _ => 0,
            };
            let index = idx.max(0) as usize;
            if index <= result.len() {
                result.insert(index, args[1].clone());
            } else {
                result.push(args[1].clone());
            }
            Ok(Value::Array(result))
        }
        "remove" => {
            if args.is_empty() {
                return Err(err("remove(index)".to_string()));
            }
            let mut result = arr_ref.to_vec();
            let idx = match &args[0] {
                Value::Number(n) => *n as i32,
                Value::I64(n) => *n as i32,
                Value::U64(n) => *n as i32,
                Value::I32(n) => *n,
                Value::U32(n) => *n as i32,
                _ => 0,
            };
            if idx >= 0 && (idx as usize) < result.len() {
                result.remove(idx as usize);
            }
            Ok(Value::Array(result))
        }
        "clear" => Ok(Value::Array(Vec::new())),
        "append" => {
            let mut result = arr_ref.to_vec();
            if !args.is_empty() {
                result.push(args[0].clone());
            }
            Ok(Value::Array(result))
        }
        "extend" => {
            if args.is_empty() {
                return Err(err("extend(array)".to_string()));
            }
            let mut result = arr_ref.to_vec();
            match &args[0] {
                Value::Array(other) => result.extend(other.clone()),
                Value::DynArray(da) => result.extend(da.data.clone()),
                other => result.push(other.clone()),
            }
            Ok(Value::Array(result))
        }
        "set_index" => {
            if args.len() < 2 {
                return Err(err("set_index(index, value)".to_string()));
            }
            let mut result = arr_ref.to_vec();
            let idx = match &args[0] {
                Value::Number(n) => *n as i32,
                Value::I64(n) => *n as i32,
                Value::U64(n) => *n as i32,
                Value::I32(n) => *n,
                Value::U32(n) => *n as i32,
                _ => -1,
            };
            if idx >= 0 && (idx as usize) < result.len() {
                result[idx as usize] = args[1].clone();
            }
            Ok(Value::Array(result))
        }
        "count" => {
            if args.is_empty() {
                return Err(err("count(value)".to_string()));
            }
            let needle = &args[0];
            let mut count = 0;
            for v in arr_ref {
                if equals(v, needle) {
                    count += 1;
                }
            }
            Ok(Value::Number(count as f64))
        }
        "index" => {
            if args.is_empty() {
                return Err(err("index(value)".to_string()));
            }
            let needle = &args[0];
            for (i, v) in arr_ref.iter().enumerate() {
                if equals(v, needle) {
                    return Ok(Value::Number(i as f64));
                }
            }
            Ok(Value::Number(-1.0))
        }
        "join" => {
            let sep = if !args.is_empty() {
                match &args[0] {
                    Value::Str(s) => s.clone(),
                    _ => ",".to_string(),
                }
            } else {
                ",".to_string()
            };
            let joined = arr_ref
                .iter()
                .map(|v| fmt(v))
                .collect::<Vec<_>>()
                .join(&sep);
            Ok(Value::Str(joined))
        }
        "concat" => {
            let mut result = arr_ref.to_vec();
            for arg in args {
                match arg {
                    Value::Array(other) => result.extend(other.clone()),
                    Value::DynArray(da) => result.extend(da.data.clone()),
                    other => result.push(other.clone()),
                }
            }
            Ok(Value::Array(result))
        }
        "reverse" => {
            let mut result = arr_ref.to_vec();
            result.reverse();
            Ok(Value::Array(result))
        }
        "sort" => {
            let mut result = arr_ref.to_vec();
            result.sort_by(|a, b| compare_values(a, b));
            Ok(Value::Array(result))
        }
        "slice" => {
            let start = if !args.is_empty() {
                match &args[0] {
                    Value::Number(n) => *n as i32,
                    _ => 0,
                }
            } else {
                0
            };
            let end = if args.len() > 1 {
                match &args[1] {
                    Value::Number(n) => Some(*n as i32),
                    _ => None,
                }
            } else {
                None
            };

            let len = arr_ref.len() as i32;
            let start_idx = (if start < 0 {
                (len + start).max(0)
            } else {
                start.min(len)
            }) as usize;
            let end_idx = if let Some(e) = end {
                (if e < 0 { (len + e).max(0) } else { e.min(len) }) as usize
            } else {
                arr_ref.len()
            };

            let sliced: Vec<Value> = arr_ref
                .iter()
                .skip(start_idx)
                .take(end_idx.saturating_sub(start_idx))
                .cloned()
                .collect();
            Ok(Value::Array(sliced))
        }
        "indexOf" => {
            let search = if !args.is_empty() {
                args[0].clone()
            } else {
                return Ok(Value::Number(-1.0));
            };
            for (i, v) in arr_ref.iter().enumerate() {
                if equals(v, &search) {
                    return Ok(Value::Number(i as f64));
                }
            }
            Ok(Value::Number(-1.0))
        }
        "includes" | "contains" => {
            let search = if !args.is_empty() {
                args[0].clone()
            } else {
                return Ok(Value::Bool(false));
            };
            for v in arr_ref {
                if equals(v, &search) {
                    return Ok(Value::Bool(true));
                }
            }
            Ok(Value::Bool(false))
        }
        "lastIndexOf" => {
            let search = args.first().ok_or_else(|| err("lastIndexOf(value)"))?;
            Ok(Value::Number(
                arr_ref
                    .iter()
                    .rposition(|v| equals(v, search))
                    .map(|i| i as f64)
                    .unwrap_or(-1.0),
            ))
        }
        "distinct" => {
            let mut result = Vec::new();
            for value in arr_ref {
                if !result.iter().any(|v| equals(v, value)) {
                    result.push(value.clone());
                }
            }
            Ok(Value::Array(result))
        }
        "sum" => {
            let mut total = 0.0;
            for value in arr_ref {
                total += value
                    .as_f64()
                    .ok_or_else(|| err("sum requires numeric elements"))?;
            }
            Ok(Value::Number(total))
        }
        "min" | "max" => {
            let mut values = arr_ref.iter().filter_map(Value::as_f64);
            let first = values
                .next()
                .ok_or_else(|| err("min/max requires numeric elements"))?;
            let result = values.fold(first, |a, b| {
                if method_name == "min" {
                    a.min(b)
                } else {
                    a.max(b)
                }
            });
            Ok(Value::Number(result))
        }
        "toSet" => {
            let mut result = Vec::new();
            for value in arr_ref {
                if !result.iter().any(|v| equals(v, value)) {
                    result.push(value.clone());
                }
            }
            Ok(Value::Set(result))
        }
        "toTuple" => Ok(Value::Tuple(arr_ref.to_vec())),
        "flat" => {
            let mut result = Vec::new();
            for v in arr_ref {
                match v {
                    Value::Array(inner) => result.extend(inner.clone()),
                    Value::DynArray(da) => result.extend(da.data.clone()),
                    other => result.push(other.clone()),
                }
            }
            Ok(Value::Array(result))
        }
        _ => Err(err(format!(
            "Unknown simple array method: '{}' (callback methods remain in interpreter_core)",
            method_name
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push() {
        let arr = vec![Value::Number(1.0), Value::Number(2.0)];
        let result = call_simple_array_method(&arr, "push", &[Value::Number(3.0)]).unwrap();
        match result {
            Value::Array(v) => assert_eq!(v.len(), 3),
            _ => panic!("Expected array"),
        }
    }

    #[test]
    fn test_pop() {
        let arr = vec![Value::Number(1.0), Value::Number(2.0)];
        let result = call_simple_array_method(&arr, "pop", &[]).unwrap();
        match result {
            Value::Array(v) => assert_eq!(v.len(), 1),
            _ => panic!("Expected array"),
        }
    }

    #[test]
    fn test_join() {
        let arr = vec![Value::Number(1.0), Value::Number(2.0), Value::Number(3.0)];
        let result =
            call_simple_array_method(&arr, "join", &[Value::Str("-".to_string())]).unwrap();
        match result {
            Value::Str(s) => assert_eq!(s, "1-2-3"),
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_slice() {
        let arr = vec![
            Value::Number(1.0),
            Value::Number(2.0),
            Value::Number(3.0),
            Value::Number(4.0),
        ];
        let result =
            call_simple_array_method(&arr, "slice", &[Value::Number(1.0), Value::Number(3.0)])
                .unwrap();
        match result {
            Value::Array(v) => assert_eq!(v.len(), 2),
            _ => panic!("Expected array"),
        }
    }

    #[test]
    fn test_index_of() {
        let arr = vec![
            Value::Number(10.0),
            Value::Number(20.0),
            Value::Number(30.0),
        ];
        let result = call_simple_array_method(&arr, "indexOf", &[Value::Number(20.0)]).unwrap();
        match result {
            Value::Number(n) => assert_eq!(n, 1.0),
            _ => panic!("Expected number"),
        }
    }

    #[test]
    fn test_includes() {
        let arr = vec![Value::Number(10.0), Value::Number(20.0)];
        let result = call_simple_array_method(&arr, "includes", &[Value::Number(20.0)]).unwrap();
        match result {
            Value::Bool(b) => assert!(b),
            _ => panic!("Expected bool"),
        }
    }

    #[test]
    fn test_flat() {
        let arr = vec![
            Value::Number(1.0),
            Value::Array(vec![Value::Number(2.0), Value::Number(3.0)]),
            Value::Number(4.0),
        ];
        let result = call_simple_array_method(&arr, "flat", &[]).unwrap();
        match result {
            Value::Array(v) => assert_eq!(v.len(), 4),
            _ => panic!("Expected array"),
        }
    }
}
