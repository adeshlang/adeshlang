//! Builtin method implementations for arrays, strings, dates, and sets

#[allow(unused_imports)]
use crate::execution::runtime_core::{
    call_user, call_user_with_this, err, format::fmt, ops::equals,
};
#[allow(unused_imports)]
use crate::parsing::ast::{BuiltinEnv, NativeFn, Value};
use chrono::Datelike;
use num_traits::ToPrimitive;

use super::super::core::Exec;

impl Exec {
    /// Call builtin method on an object (e.g., string.split(), array.map())
    pub(super) fn call_builtin_method(
        &mut self,
        obj: &Value,
        method_name: &str,
        args: &[Value],
    ) -> Result<Value, String> {
        // ARC introspection: never clone receiver into method_args (that adds a spurious owner).
        if matches!(
            method_name,
            "strong_count" | "weak_count" | "is_alive" | "upgrade"
        ) && matches!(obj, Value::Share(_) | Value::Weak(_))
        {
            return crate::memory::arc::invoke_arc_method(obj, method_name).map_err(|e| err(e));
        }

        // Build args: [obj, ...rest_args]
        let mut method_args = vec![obj.clone()];
        method_args.extend_from_slice(args);

        // Dispatch to known methods based on method name and object type
        match method_name {
            // Array methods
            "length" | "len" | "capacity" | "metadata_size" => match obj {
                Value::Array(a) => match method_name {
                    "length" | "len" => Ok(Value::Number(a.len() as f64)),
                    "capacity" => Ok(Value::Number(a.capacity() as f64)),
                    "metadata_size" => Ok(Value::Number(24.0)),
                    _ => Err(err("unreachable")),
                },
                Value::DynArray(da) => match method_name {
                    "length" | "len" => Ok(Value::Number(da.data.len() as f64)),
                    "capacity" => {
                        let cap = if da.tracked_capacity > 0 {
                            da.tracked_capacity
                        } else {
                            da.data.capacity()
                        };
                        Ok(Value::Number(cap as f64))
                    }
                    "metadata_size" => {
                        let size = if da.element_type.metadata_size() == 4 {
                            16.0
                        } else {
                            24.0
                        };
                        Ok(Value::Number(size))
                    }
                    _ => Err(err("unreachable")),
                },
                Value::RawArray(_, a) => match method_name {
                    "length" | "len" => Ok(Value::Number(a.len() as f64)),
                    "capacity" => Ok(Value::Number(a.len() as f64)),
                    "metadata_size" => Ok(Value::Number(0.0)),
                    _ => Err(err("unreachable")),
                },
                Value::Tuple(t) => match method_name {
                    "length" | "len" => Ok(Value::Number(t.len() as f64)),
                    "capacity" => Ok(Value::Number(t.len() as f64)),
                    "metadata_size" => Ok(Value::Number(0.0)),
                    _ => Err(err("unreachable")),
                },
                Value::Str(s) => match method_name {
                    "length" | "len" => Ok(Value::Number(s.chars().count() as f64)),
                    _ => Err(err("not supported")),
                },
                _ => Err(err(format!("'{}' is not a method", method_name))),
            },

            // Array / Set methods
            "push" | "pop" | "shift" | "unshift" | "append" | "insert" | "remove" | "clear"
            | "extend" | "set_index" | "count" | "index" | "sort" | "reverse" => match obj {
                Value::Array(_) | Value::DynArray(_) => {
                    self.call_array_method(obj, method_name, args)
                }
                Value::RawArray(_, _) => match method_name {
                    "push" | "append" | "insert" | "extend" | "unshift" => {
                        Err(err("Cannot append to raw array (fixed size)"))
                    }
                    "pop" | "shift" | "remove" | "clear" => {
                        Err(err("Cannot pop from raw array (fixed size)"))
                    }
                    _ => self.call_array_method(obj, method_name, args),
                },
                Value::Set(_) | Value::Object(_)
                    if matches!(method_name, "insert" | "remove" | "clear") =>
                {
                    self.call_set_method(obj, method_name, args)
                }
                _ => Err(err(format!(
                    "'{}' is not a method of {}",
                    method_name,
                    fmt(obj)
                ))),
            },

            // String methods
            "split" => match obj {
                Value::Str(_) => self.call_string_method(obj, method_name, args),
                _ => Err(err(format!("'split' is only available on strings"))),
            },
            "substring" | "substr" | "charAt" | "trim" | "toLowerCase" | "toUpperCase"
            | "startsWith" | "endsWith" | "repeat" | "replace" => match obj {
                Value::Str(_) => self.call_string_method(obj, method_name, args),
                _ => Err(err(format!(
                    "'{}' is only available on strings",
                    method_name
                ))),
            },
            "indexOf" | "lastIndexOf" | "includes" | "contains" | "slice" => match obj {
                Value::Str(_) => self.call_string_method(obj, method_name, args),
                Value::Array(_) | Value::DynArray(_) | Value::RawArray(_, _) => {
                    self.call_array_method(obj, method_name, args)
                }
                Value::Set(_) | Value::Object(_)
                    if method_name == "contains" || method_name == "includes" =>
                {
                    self.call_set_method(obj, method_name, args)
                }
                _ => Err(err(format!("'{}' not supported on this type", method_name))),
            },

            // Array higher-order methods
            "map" | "filter" | "reduce" | "find" | "findIndex" | "join" | "concat" | "flat"
            | "forEach" | "some" | "every" => match obj {
                Value::Array(_) | Value::DynArray(_) | Value::RawArray(_, _) => {
                    self.call_array_method(obj, method_name, args)
                }
                Value::Str(_) if method_name == "join" => {
                    self.call_string_method(obj, method_name, args)
                }
                _ => Err(err(format!("'{}' not supported on this type", method_name))),
            },

            // Shared Array/Set methods
            "union" | "intersection" => match obj {
                Value::Array(_) | Value::DynArray(_) | Value::RawArray(_, _) => {
                    self.call_array_method(obj, method_name, args)
                }
                Value::Set(_) | Value::Object(_) => self.call_set_method(obj, method_name, args),
                _ => Err(err(format!("'{}' not supported on this type", method_name))),
            },

            // Date methods
            "toISOString" | "getTime" | "toString" | "getFullYear" | "getMonth" | "getDate"
            | "getHours" | "getMinutes" | "getSeconds" | "getMilliseconds" | "getDay"
            | "getUTCFullYear" | "getUTCMonth" | "getUTCDate" | "getUTCHours" | "getUTCMinutes"
            | "getUTCSeconds" | "getUTCMilliseconds" | "getTimezoneOffset" | "toLocaleString"
            | "toLocaleDateString" | "toLocaleTimeString" => {
                self.call_date_method(obj, method_name, args)
            }

            // Set methods
            "add" | "has" | "delete" | "size" => match obj {
                Value::Set(_) | Value::Object(_) => self.call_set_method(obj, method_name, args),
                _ => Err(err(format!("'{}' not supported on this type", method_name))),
            },

            "toFixed" => match obj {
                Value::Number(_)
                | Value::U8(_)
                | Value::U16(_)
                | Value::U32(_)
                | Value::U64(_)
                | Value::I8(_)
                | Value::I16(_)
                | Value::I32(_)
                | Value::I64(_)
                | Value::F32(_)
                | Value::F64(_) => {
                    let decimals = if let Some(d_val) = args.get(0) {
                        match d_val {
                            Value::Number(n) => *n as usize,
                            Value::U8(n) => *n as usize,
                            Value::U16(n) => *n as usize,
                            Value::U32(n) => *n as usize,
                            Value::U64(n) => *n as usize,
                            Value::I8(n) => *n as usize,
                            Value::I16(n) => *n as usize,
                            Value::I32(n) => *n as usize,
                            Value::I64(n) => *n as usize,
                            _ => 0,
                        }
                    } else {
                        0
                    };
                    let num = match obj {
                        Value::Number(n) => *n,
                        Value::U8(n) => *n as f64,
                        Value::U16(n) => *n as f64,
                        Value::U32(n) => *n as f64,
                        Value::U64(n) => *n as f64,
                        Value::I8(n) => *n as f64,
                        Value::I16(n) => *n as f64,
                        Value::I32(n) => *n as f64,
                        Value::I64(n) => *n as f64,
                        Value::F32(n) => *n as f64,
                        Value::F64(n) => *n,
                        _ => 0.0,
                    };
                    Ok(Value::Str(format!("{:.*}", decimals, num)))
                }
                _ => Err(err(format!("'toFixed' is only available on numeric types"))),
            },

            // ARC methods
            "strong_count" | "weak_count" | "is_alive" | "upgrade" => {
                crate::memory::arc::invoke_arc_method(obj, method_name).map_err(|e| err(e))
            }

            _ => Err(err(format!("Unknown method: '{}'", method_name))),
        }
    }

    /// Call string builtin method
    pub(super) fn call_string_method(
        &mut self,
        obj: &Value,
        method_name: &str,
        args: &[Value],
    ) -> Result<Value, String> {
        let mut method_args = vec![obj.clone()];
        method_args.extend_from_slice(args);

        match method_name {
            "split" => {
                if method_args.len() < 2 {
                    return Err(err("split(delimiter)"));
                }
                let s = match &method_args[0] {
                    Value::Str(s) => s.clone(),
                    _ => return Err(err("split requires string")),
                };
                let delim = match &method_args[1] {
                    Value::Str(d) => d.clone(),
                    _ => return Err(err("split delimiter must be string")),
                };

                let parts: Vec<Value> =
                    s.split(&delim).map(|p| Value::Str(p.to_string())).collect();
                Ok(Value::Array(parts))
            }
            "substring" | "substr" => {
                if method_args.len() < 2 {
                    return Err(err("substring(start[, end])"));
                }
                let s = match &method_args[0] {
                    Value::Str(s) => s.clone(),
                    _ => return Err(err("substring requires string")),
                };
                let start = match &method_args[1] {
                    Value::Number(n) => *n as usize,
                    _ => return Err(err("substring indices must be numbers")),
                };
                let end = if method_args.len() > 2 {
                    match &method_args[2] {
                        Value::Number(n) => Some(*n as usize),
                        _ => None,
                    }
                } else {
                    None
                };

                let chars: Vec<char> = s.chars().collect();
                let end = end.unwrap_or(chars.len());
                let substring: String = chars
                    .iter()
                    .skip(start)
                    .take(end.saturating_sub(start))
                    .collect();
                Ok(Value::Str(substring))
            }
            "charAt" => {
                if method_args.len() < 2 {
                    return Err(err("charAt(index)"));
                }
                let s = match &method_args[0] {
                    Value::Str(s) => s.clone(),
                    _ => return Err(err("charAt requires string")),
                };
                let idx = match &method_args[1] {
                    Value::Number(n) => *n as usize,
                    Value::I32(n) => *n as usize,
                    Value::I64(n) => *n as usize,
                    Value::U32(n) => *n as usize,
                    Value::U64(n) => *n as usize,
                    Value::I8(n) => *n as usize,
                    Value::U8(n) => *n as usize,
                    Value::I16(n) => *n as usize,
                    Value::U16(n) => *n as usize,
                    _ => return Err(err("charAt index must be number")),
                };

                if let Some(ch) = s.chars().nth(idx) {
                    Ok(Value::Str(ch.to_string()))
                } else {
                    Ok(Value::Str(String::new()))
                }
            }
            "indexOf" => {
                if method_args.len() < 2 {
                    return Err(err("indexOf(searchString)"));
                }
                let s = match &method_args[0] {
                    Value::Str(s) => s.clone(),
                    _ => return Err(err("indexOf requires string")),
                };
                let search = match &method_args[1] {
                    Value::Str(search) => search.clone(),
                    _ => return Err(err("indexOf search must be string")),
                };

                match s.find(&search) {
                    Some(pos) => Ok(Value::Number(pos as f64)),
                    None => Ok(Value::Number(-1.0)),
                }
            }
            "lastIndexOf" => {
                if method_args.len() < 2 {
                    return Err(err("lastIndexOf(searchString)"));
                }
                let s = match &method_args[0] {
                    Value::Str(s) => s.clone(),
                    _ => return Err(err("lastIndexOf requires string")),
                };
                let search = match &method_args[1] {
                    Value::Str(search) => search.clone(),
                    _ => return Err(err("lastIndexOf search must be string")),
                };

                match s.rfind(&search) {
                    Some(pos) => Ok(Value::Number(pos as f64)),
                    None => Ok(Value::Number(-1.0)),
                }
            }
            "includes" | "contains" => {
                if method_args.len() < 2 {
                    return Err(err("includes(searchString)"));
                }
                let s = match &method_args[0] {
                    Value::Str(s) => s.clone(),
                    _ => return Err(err("includes requires string")),
                };
                let search = match &method_args[1] {
                    Value::Str(search) => search.clone(),
                    _ => return Err(err("includes search must be string")),
                };

                Ok(Value::Bool(s.contains(&search)))
            }
            "startsWith" => {
                if method_args.len() < 2 {
                    return Err(err("startsWith(searchString)"));
                }
                let s = match &method_args[0] {
                    Value::Str(s) => s.clone(),
                    _ => return Err(err("startsWith requires string")),
                };
                let search = match &method_args[1] {
                    Value::Str(search) => search.clone(),
                    _ => return Err(err("startsWith search must be string")),
                };

                Ok(Value::Bool(s.starts_with(&search)))
            }
            "endsWith" => {
                if method_args.len() < 2 {
                    return Err(err("endsWith(searchString)"));
                }
                let s = match &method_args[0] {
                    Value::Str(s) => s.clone(),
                    _ => return Err(err("endsWith requires string")),
                };
                let search = match &method_args[1] {
                    Value::Str(search) => search.clone(),
                    _ => return Err(err("endsWith search must be string")),
                };

                Ok(Value::Bool(s.ends_with(&search)))
            }
            "trim" => {
                let s = match &method_args[0] {
                    Value::Str(s) => s.clone(),
                    _ => return Err(err("trim requires string")),
                };
                Ok(Value::Str(s.trim().to_string()))
            }
            "toLowerCase" => {
                let s = match &method_args[0] {
                    Value::Str(s) => s.clone(),
                    _ => return Err(err("toLowerCase requires string")),
                };
                Ok(Value::Str(s.to_lowercase()))
            }
            "toUpperCase" => {
                let s = match &method_args[0] {
                    Value::Str(s) => s.clone(),
                    _ => return Err(err("toUpperCase requires string")),
                };
                Ok(Value::Str(s.to_uppercase()))
            }
            "replace" => {
                if method_args.len() < 3 {
                    return Err(err("replace(search, replacement)"));
                }
                let s = match &method_args[0] {
                    Value::Str(s) => s.clone(),
                    _ => return Err(err("replace requires string")),
                };
                let search = match &method_args[1] {
                    Value::Str(search) => search.clone(),
                    _ => return Err(err("replace search must be string")),
                };
                let replacement = match &method_args[2] {
                    Value::Str(r) => r.clone(),
                    _ => return Err(err("replace replacement must be string")),
                };

                Ok(Value::Str(s.replacen(&search, &replacement, 1)))
            }
            "repeat" => {
                if method_args.len() < 2 {
                    return Err(err("repeat(count)"));
                }
                let s = match &method_args[0] {
                    Value::Str(s) => s.clone(),
                    _ => return Err(err("repeat requires string")),
                };
                let count = match &method_args[1] {
                    Value::Number(n) => *n as usize,
                    _ => return Err(err("repeat count must be number")),
                };

                Ok(Value::Str(s.repeat(count)))
            }
            "slice" => {
                if method_args.len() < 2 {
                    return Err(err("slice(start[, end])"));
                }
                let s = match &method_args[0] {
                    Value::Str(s) => s.clone(),
                    _ => return Err(err("slice requires string")),
                };
                let start = match &method_args[1] {
                    Value::Number(n) => *n as i32,
                    Value::I32(n) => *n as i32,
                    Value::I64(n) => *n as i32,
                    Value::U32(n) => *n as i32,
                    Value::U64(n) => *n as i32,
                    Value::I8(n) => *n as i32,
                    Value::U8(n) => *n as i32,
                    Value::I16(n) => *n as i32,
                    Value::U16(n) => *n as i32,
                    _ => return Err(err("slice start must be number")),
                };
                let end = if method_args.len() > 2 {
                    match &method_args[2] {
                        Value::Number(n) => Some(*n as i32),
                        Value::I32(n) => Some(*n as i32),
                        Value::I64(n) => Some(*n as i32),
                        Value::U32(n) => Some(*n as i32),
                        Value::U64(n) => Some(*n as i32),
                        Value::I8(n) => Some(*n as i32),
                        Value::U8(n) => Some(*n as i32),
                        Value::I16(n) => Some(*n as i32),
                        Value::U16(n) => Some(*n as i32),
                        _ => None,
                    }
                } else {
                    None
                };

                let chars: Vec<char> = s.chars().collect();
                let len = chars.len() as i32;
                let start_idx = (if start < 0 {
                    (len + start).max(0)
                } else {
                    start.min(len)
                }) as usize;
                let end_idx = if let Some(e) = end {
                    (if e < 0 { (len + e).max(0) } else { e.min(len) }) as usize
                } else {
                    chars.len()
                };

                let sliced: String = chars
                    .iter()
                    .skip(start_idx)
                    .take(end_idx.saturating_sub(start_idx))
                    .collect();
                Ok(Value::Str(sliced))
            }
            "join" => {
                // String join (concatenate with separator)
                if method_args.len() < 2 {
                    return Ok(Value::Str(fmt(obj)));
                }
                let sep = match &method_args[1] {
                    Value::Str(s) => s.clone(),
                    _ => return Err(err("join separator must be string")),
                };
                Ok(Value::Str(format!("{}{}", fmt(obj), sep)))
            }
            _ => Err(err(format!("Unknown string method: '{}'", method_name))),
        }
    }

    /// Call array builtin method
    pub(super) fn call_array_method(
        &mut self,
        obj: &Value,
        method_name: &str,
        args: &[Value],
    ) -> Result<Value, String> {
        let mut method_args = vec![obj.clone()];
        method_args.extend_from_slice(args);

        let arr = match obj {
            Value::Array(a) => a.clone(),
            Value::DynArray(da) => da.data.clone(),
            Value::RawArray(_, elems) => elems.clone(),
            _ => return Err(err("array method requires array")),
        };

        let wrap_result = |result: Vec<Value>| -> Value {
            match obj {
                Value::DynArray(da) => {
                    Value::DynArray(Box::new(crate::parsing::ast::DynamicArray {
                        data: result,
                        element_type: da.element_type.clone(),
                        concrete_type: da.concrete_type.clone(),
                        tracked_capacity: da.tracked_capacity,
                    }))
                }
                Value::RawArray(ty, _) => Value::RawArray(ty.clone(), result),
                _ => Value::Array(result),
            }
        };

        match method_name {
            "push" => {
                if let Value::DynArray(da) = obj {
                    if da.tracked_capacity > 0 && da.data.len() >= da.tracked_capacity {
                        return Err(err(format!(
                            "Cannot append to fixed-capacity array (capacity: {})",
                            da.tracked_capacity
                        )));
                    }
                }
                if method_args.len() < 2 {
                    return Err(err("push(element)"));
                }
                let mut result = arr.clone();
                result.push(method_args[1].clone());
                Ok(wrap_result(result))
            }
            "pop" => {
                if arr.is_empty() {
                    return Err(err("Cannot pop from empty array"));
                }
                let mut result = arr.clone();
                result.pop();
                Ok(wrap_result(result))
            }
            "shift" => {
                let mut result = arr.clone();
                if !result.is_empty() {
                    result.remove(0);
                }
                Ok(wrap_result(result))
            }
            "unshift" => {
                if let Value::DynArray(da) = obj {
                    if da.tracked_capacity > 0 && da.data.len() >= da.tracked_capacity {
                        return Err(err(format!(
                            "Cannot append to fixed-capacity array (capacity: {})",
                            da.tracked_capacity
                        )));
                    }
                }
                if method_args.len() < 2 {
                    return Err(err("unshift(element)"));
                }
                let mut result = vec![method_args[1].clone()];
                result.extend(arr);
                Ok(wrap_result(result))
            }
            "append" => {
                if let Value::DynArray(da) = obj {
                    if da.tracked_capacity > 0 && da.data.len() >= da.tracked_capacity {
                        return Err(err(format!(
                            "Cannot append to fixed-capacity array (capacity: {})",
                            da.tracked_capacity
                        )));
                    }
                }
                let mut result = arr.clone();
                if method_args.len() > 1 {
                    result.push(method_args[1].clone());
                }
                Ok(wrap_result(result))
            }
            "set_index" => {
                if method_args.len() < 3 {
                    return Err(err("set_index(index, value)"));
                }
                let idx = match &method_args[1] {
                    Value::Number(n) => {
                        if (n - n.trunc()).abs() > 1e-12 || *n < 0.0 {
                            return Err(err("index must be integer and non-negative"));
                        }
                        *n as usize
                    }
                    Value::BigInt(b) => b.to_usize().unwrap_or(usize::MAX),
                    Value::I8(n) => {
                        if *n < 0 {
                            return Err(err("index out of bounds"));
                        } else {
                            *n as usize
                        }
                    }
                    Value::I16(n) => {
                        if *n < 0 {
                            return Err(err("index out of bounds"));
                        } else {
                            *n as usize
                        }
                    }
                    Value::I32(n) => {
                        if *n < 0 {
                            return Err(err("index out of bounds"));
                        } else {
                            *n as usize
                        }
                    }
                    Value::I64(n) => {
                        if *n < 0 {
                            return Err(err("index out of bounds"));
                        } else {
                            *n as usize
                        }
                    }
                    Value::U8(n) => *n as usize,
                    Value::U16(n) => *n as usize,
                    Value::U32(n) => *n as usize,
                    Value::U64(n) => *n as usize,
                    _ => return Err(err("set_index index must be number")),
                };
                if idx >= arr.len() {
                    return Err(err("index out of bounds"));
                }
                let mut result = arr.clone();
                result[idx] = method_args[2].clone();
                Ok(wrap_result(result))
            }
            "insert" => {
                if let Value::DynArray(da) = obj {
                    if da.tracked_capacity > 0 && da.data.len() >= da.tracked_capacity {
                        return Err(err(format!(
                            "Cannot append to fixed-capacity array (capacity: {})",
                            da.tracked_capacity
                        )));
                    }
                }
                if method_args.len() < 3 {
                    return Err(err("insert(index, element)"));
                }
                let idx = match &method_args[1] {
                    Value::Number(n) => *n as usize,
                    _ => return Err(err("insert index must be number")),
                };
                if idx > arr.len() {
                    return Err(err("index out of bounds"));
                }
                let mut result = arr.clone();
                result.insert(idx, method_args[2].clone());
                Ok(wrap_result(result))
            }
            "remove" => {
                if method_args.len() < 2 {
                    return Err(err("remove(index)"));
                }
                let idx = match &method_args[1] {
                    Value::Number(n) => *n as usize,
                    _ => return Err(err("remove index must be number")),
                };
                if idx >= arr.len() {
                    return Err(err("index out of bounds"));
                }
                let mut result = arr.clone();
                result.remove(idx);
                Ok(wrap_result(result))
            }
            "clear" => Ok(wrap_result(Vec::new())),
            "extend" => {
                let mut result = arr.clone();
                for i in 1..method_args.len() {
                    match &method_args[i] {
                        Value::Array(other) => result.extend(other.clone()),
                        Value::DynArray(da) => result.extend(da.data.clone()),
                        Value::RawArray(_, raw) => result.extend(raw.clone()),
                        other => result.push(other.clone()),
                    }
                }
                if let Value::DynArray(da) = obj {
                    if da.tracked_capacity > 0 && result.len() > da.tracked_capacity {
                        return Err(err(format!(
                            "Cannot append to fixed-capacity array (capacity: {})",
                            da.tracked_capacity
                        )));
                    }
                }
                Ok(wrap_result(result))
            }
            "sort" => {
                let mut result = arr.clone();
                result.sort_by(|a, b| match (a, b) {
                    (Value::Number(n1), Value::Number(n2)) => {
                        n1.partial_cmp(n2).unwrap_or(std::cmp::Ordering::Equal)
                    }
                    (Value::Str(s1), Value::Str(s2)) => s1.cmp(s2),
                    _ => std::cmp::Ordering::Equal,
                });
                Ok(wrap_result(result))
            }
            "reverse" => {
                let mut result = arr.clone();
                result.reverse();
                Ok(wrap_result(result))
            }
            "count" => {
                if method_args.len() < 2 {
                    return Ok(Value::Number(arr.len() as f64));
                }
                let target = &method_args[1];
                let count = arr.iter().filter(|v| equals(v, target)).count();
                Ok(Value::Number(count as f64))
            }
            "index" => {
                if method_args.len() < 2 {
                    return Err(err("index expects 1 arg"));
                }
                if let Value::Number(n) = &method_args[1] {
                    let i = *n as usize;
                    return Ok(arr.get(i).cloned().unwrap_or(Value::Null));
                }
                Err(err("index expects numeric arg"))
            }
            "join" => {
                let sep = if method_args.len() > 1 {
                    match &method_args[1] {
                        Value::Str(s) => s.clone(),
                        _ => ",".to_string(),
                    }
                } else {
                    ",".to_string()
                };
                let joined = arr.iter().map(|v| fmt(v)).collect::<Vec<_>>().join(&sep);
                Ok(Value::Str(joined))
            }
            "map" => {
                if method_args.len() < 2 {
                    return Err(err("map(fn)"));
                }
                let func = method_args[1].clone();
                let mut out: Vec<Value> = Vec::with_capacity(arr.len());

                for v in &arr {
                    let res = match &func {
                        Value::Function(NativeFn(f)) => (f)(self, vec![v.clone()]),
                        Value::UserFunction(u) => self._call_user_fn(u, vec![v.clone()]),
                        _ => return Err(err("map requires a function")),
                    }?;
                    out.push(res);
                }
                Ok(wrap_result(out))
            }
            "filter" => {
                if method_args.len() < 2 {
                    return Err(err("filter(fn)"));
                }
                let func = method_args[1].clone();
                let mut out: Vec<Value> = Vec::new();

                for v in &arr {
                    let res = match &func {
                        Value::Function(NativeFn(f)) => (f)(self, vec![v.clone()]),
                        Value::UserFunction(u) => self._call_user_fn(u, vec![v.clone()]),
                        _ => return Err(err("filter requires a function")),
                    }?;
                    if res.truthy() {
                        out.push(v.clone());
                    }
                }
                Ok(wrap_result(out))
            }
            "reduce" => {
                if method_args.len() < 2 || method_args.len() > 3 {
                    return Err(err("reduce(fn, init?)"));
                }
                if arr.is_empty() && method_args.len() == 2 {
                    return Err(err("reduce of empty array with no initial value"));
                }
                let func = method_args[1].clone();
                let mut acc = if method_args.len() == 3 {
                    method_args[2].clone()
                } else {
                    arr[0].clone()
                };
                let start_idx = if method_args.len() == 3 { 0 } else { 1 };
                for v in arr.iter().skip(start_idx) {
                    let res = match &func {
                        Value::Function(NativeFn(f)) => (f)(self, vec![acc.clone(), v.clone()]),
                        Value::UserFunction(u) => {
                            self._call_user_fn(u, vec![acc.clone(), v.clone()])
                        }
                        _ => return Err(err("reduce requires a function")),
                    }?;
                    acc = res;
                }
                Ok(acc)
            }
            "find" => {
                if method_args.len() < 2 {
                    return Err(err("find(fn)"));
                }
                let func = method_args[1].clone();
                for v in &arr {
                    let res = match &func {
                        Value::Function(NativeFn(f)) => (f)(self, vec![v.clone()]),
                        Value::UserFunction(u) => self._call_user_fn(u, vec![v.clone()]),
                        _ => return Err(err("find requires a function")),
                    }?;
                    if res.truthy() {
                        return Ok(v.clone());
                    }
                }
                Ok(Value::Null)
            }
            "findIndex" => {
                if method_args.len() < 2 {
                    return Err(err("findIndex(fn)"));
                }
                let func = method_args[1].clone();
                for (i, v) in arr.iter().enumerate() {
                    let res = match &func {
                        Value::Function(NativeFn(f)) => (f)(self, vec![v.clone()]),
                        Value::UserFunction(u) => self._call_user_fn(u, vec![v.clone()]),
                        _ => return Err(err("findIndex requires a function")),
                    }?;
                    if res.truthy() {
                        return Ok(Value::Number(i as f64));
                    }
                }
                Ok(Value::Number(-1.0))
            }
            "concat" => {
                let mut result = arr.clone();
                for i in 1..method_args.len() {
                    match &method_args[i] {
                        Value::Array(other) => result.extend(other.clone()),
                        Value::DynArray(da) => result.extend(da.data.clone()),
                        Value::RawArray(_, raw) => result.extend(raw.clone()),
                        other => result.push(other.clone()),
                    }
                }
                Ok(wrap_result(result))
            }
            "slice" => {
                let start = if method_args.len() > 1 {
                    match &method_args[1] {
                        Value::Number(n) => *n as i32,
                        _ => 0,
                    }
                } else {
                    0
                };
                let end = if method_args.len() > 2 {
                    match &method_args[2] {
                        Value::Number(n) => Some(*n as i32),
                        _ => None,
                    }
                } else {
                    None
                };

                let len = arr.len() as i32;
                let start_idx = (if start < 0 {
                    (len + start).max(0)
                } else {
                    start.min(len)
                }) as usize;
                let end_idx = if let Some(e) = end {
                    (if e < 0 { (len + e).max(0) } else { e.min(len) }) as usize
                } else {
                    arr.len()
                };

                let sliced: Vec<Value> = arr
                    .iter()
                    .skip(start_idx)
                    .take(end_idx.saturating_sub(start_idx))
                    .cloned()
                    .collect();
                Ok(wrap_result(sliced))
            }
            "indexOf" => {
                let search = if method_args.len() > 1 {
                    method_args[1].clone()
                } else {
                    return Ok(Value::Number(-1.0));
                };
                for (i, v) in arr.iter().enumerate() {
                    if equals(v, &search) {
                        return Ok(Value::Number(i as f64));
                    }
                }
                Ok(Value::Number(-1.0))
            }
            "includes" | "contains" => {
                let search = if method_args.len() > 1 {
                    method_args[1].clone()
                } else {
                    return Ok(Value::Bool(false));
                };
                for v in &arr {
                    if equals(v, &search) {
                        return Ok(Value::Bool(true));
                    }
                }
                Ok(Value::Bool(false))
            }
            "forEach" => {
                if method_args.len() < 2 {
                    return Err(err("forEach(fn)"));
                }
                let func = method_args[1].clone();
                for v in &arr {
                    let _ = match &func {
                        Value::Function(NativeFn(f)) => (f)(self, vec![v.clone()]),
                        Value::UserFunction(u) => self._call_user_fn(u, vec![v.clone()]),
                        _ => return Err(err("forEach requires a function")),
                    }?;
                }
                Ok(Value::Null)
            }
            "some" => {
                if method_args.len() < 2 {
                    return Err(err("some(fn)"));
                }
                let func = method_args[1].clone();
                for v in &arr {
                    let res = match &func {
                        Value::Function(NativeFn(f)) => (f)(self, vec![v.clone()]),
                        Value::UserFunction(u) => self._call_user_fn(u, vec![v.clone()]),
                        _ => return Err(err("some requires a function")),
                    }?;
                    if res.truthy() {
                        return Ok(Value::Bool(true));
                    }
                }
                Ok(Value::Bool(false))
            }
            "every" => {
                if method_args.len() < 2 {
                    return Err(err("every(fn)"));
                }
                let func = method_args[1].clone();
                for v in &arr {
                    let res = match &func {
                        Value::Function(NativeFn(f)) => (f)(self, vec![v.clone()]),
                        Value::UserFunction(u) => self._call_user_fn(u, vec![v.clone()]),
                        _ => return Err(err("every requires a function")),
                    }?;
                    if !res.truthy() {
                        return Ok(Value::Bool(false));
                    }
                }
                Ok(Value::Bool(true))
            }
            "flat" => {
                let mut result = Vec::new();
                for v in &arr {
                    match v {
                        Value::Array(inner) => result.extend(inner.clone()),
                        Value::DynArray(da) => result.extend(da.data.clone()),
                        Value::RawArray(_, raw) => result.extend(raw.clone()),
                        other => result.push(other.clone()),
                    }
                }
                Ok(wrap_result(result))
            }
            "union" => {
                if method_args.len() != 2 {
                    return Err(err("union expects 1 arg"));
                }
                let other = match &method_args[1] {
                    Value::Array(o) => o.clone(),
                    Value::Set(o) => o.clone(),
                    Value::DynArray(da) => da.data.clone(),
                    Value::RawArray(_, raw) => raw.clone(),
                    _ => return Err(err("union expects array or set")),
                };
                let mut out = Vec::new();
                for v in &arr {
                    if !out.iter().any(|x| equals(x, v)) {
                        out.push(v.clone());
                    }
                }
                for v in &other {
                    if !out.iter().any(|x| equals(x, v)) {
                        out.push(v.clone());
                    }
                }
                Ok(wrap_result(out))
            }
            "intersection" => {
                if method_args.len() != 2 {
                    return Err(err("intersection expects 1 arg"));
                }
                let other = match &method_args[1] {
                    Value::Array(o) => o.clone(),
                    Value::Set(o) => o.clone(),
                    Value::DynArray(da) => da.data.clone(),
                    Value::RawArray(_, raw) => raw.clone(),
                    _ => return Err(err("intersection expects array or set")),
                };
                let mut out = Vec::new();
                for v in &arr {
                    if other.iter().any(|x| equals(x, v)) {
                        if !out.iter().any(|x| equals(x, v)) {
                            out.push(v.clone());
                        }
                    }
                }
                Ok(wrap_result(out))
            }
            _ => Err(err(format!("Unknown array method: '{}'", method_name))),
        }
    }

    /// Call date builtin method
    pub(super) fn call_date_method(
        &mut self,
        obj: &Value,
        method_name: &str,
        _args: &[Value],
    ) -> Result<Value, String> {
        match obj {
            Value::BigInt(ts) => match method_name {
                "getTime" => Ok(Value::Number(ts.to_f64().unwrap_or(0.0))),
                "toISOString" => {
                    let ms = ts.to_i64().unwrap_or(0);
                    let secs = (ms / 1000) as i64;
                    let nanos = ((ms % 1000) * 1_000_000) as i32;
                    let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(secs, nanos as u32)
                        .unwrap_or(chrono::DateTime::<chrono::Utc>::UNIX_EPOCH);
                    let iso = dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
                    Ok(Value::Str(iso))
                }
                "toString" => {
                    let ms = ts.to_i64().unwrap_or(0);
                    let secs = (ms / 1000) as i64;
                    let nanos = ((ms % 1000) * 1_000_000) as i32;
                    let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(secs, nanos as u32)
                        .unwrap_or(chrono::DateTime::<chrono::Utc>::UNIX_EPOCH);
                    Ok(Value::Str(dt.to_string()))
                }
                "getFullYear" => {
                    let ms = ts.to_i64().unwrap_or(0);
                    let secs = (ms / 1000) as i64;
                    let nanos = ((ms % 1000) * 1_000_000) as i32;
                    let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(secs, nanos as u32)
                        .unwrap_or(chrono::DateTime::<chrono::Utc>::UNIX_EPOCH);
                    Ok(Value::Number(dt.year() as f64))
                }
                "getMonth" => {
                    let ms = ts.to_i64().unwrap_or(0);
                    let secs = (ms / 1000) as i64;
                    let nanos = ((ms % 1000) * 1_000_000) as i32;
                    let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(secs, nanos as u32)
                        .unwrap_or(chrono::DateTime::<chrono::Utc>::UNIX_EPOCH);
                    Ok(Value::Number((dt.month() - 1) as f64))
                }
                _ => Err(err(format!("Unimplemented date method: '{}'", method_name))),
            },
            _ => Err(err("Date methods require BigInt timestamp")),
        }
    }

    /// Call set builtin method
    pub(super) fn call_set_method(
        &mut self,
        _obj: &Value,
        method_name: &str,
        _args: &[Value],
    ) -> Result<Value, String> {
        Err(err(format!(
            "Set method not yet implemented: '{}'",
            method_name
        )))
    }
}
