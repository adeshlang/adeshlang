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
                    "metadata_size" => Ok(Value::Number(da.element_type.metadata_size() as f64)),
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
            | "extend" | "set_index" | "count" | "index" | "sort" | "reverse" => {
                match obj {
                    Value::Array(_) | Value::DynArray(_) => {
                        self.call_array_method(obj, method_name, args)
                    }
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
                }
            }

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
                Value::Array(_) | Value::DynArray(_) => {
                    self.call_array_method(obj, method_name, args)
                }
                Value::Set(_) | Value::Object(_) if method_name == "contains" || method_name == "includes" => {
                    self.call_set_method(obj, method_name, args)
                }
                _ => Err(err(format!("'{}' not supported on this type", method_name))),
            },

            // Array higher-order methods
            "map" | "filter" | "reduce" | "find" | "findIndex" | "join" | "concat" | "flat"
            | "forEach" | "some" | "every" => match obj {
                Value::Array(_) | Value::DynArray(_) => {
                    self.call_array_method(obj, method_name, args)
                }
                Value::Str(_) if method_name == "join" => {
                    self.call_string_method(obj, method_name, args)
                }
                _ => Err(err(format!("'{}' not supported on this type", method_name))),
            },

            // Shared Array/Set methods
            "union" | "intersection" => match obj {
                Value::Array(_) | Value::DynArray(_) => {
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
            _ => return Err(err("array method requires array")),
        };

        match method_name {
            "push" => {
                if method_args.len() < 2 {
                    return Err(err("push(element)"));
                }
                let mut result = arr.clone();
                result.push(method_args[1].clone());
                Ok(Value::Array(result))
            }
            "pop" => {
                let mut result = arr.clone();
                result.pop();
                Ok(Value::Array(result))
            }
            "shift" => {
                let mut result = arr.clone();
                if !result.is_empty() {
                    result.remove(0);
                }
                Ok(Value::Array(result))
            }
            "unshift" => {
                if method_args.len() < 2 {
                    return Err(err("unshift(element)"));
                }
                let mut result = vec![method_args[1].clone()];
                result.extend(arr);
                Ok(Value::Array(result))
            }
            "append" => {
                let mut result = arr.clone();
                if method_args.len() > 1 {
                    result.push(method_args[1].clone());
                }
                Ok(Value::Array(result))
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

                // PERFORMANCE NOTE (Phase 3 TODO):
                // For UserFunction, call_user() creates a new Exec instance and clones
                // the entire global environment (line 1927) for EACH element.
                // This causes O(n * m) complexity where n=array length, m=globals count.
                //
                // OPTIMIZATION: Replace call_user() with Interpreter::call_user_function()
                // which reuses existing execution context (O(1) per call).
                // Requires refactoring to pass &mut Interpreter instead of &mut Exec.
                // See PHASE3_IMPLEMENTATION_NOTES.md for details.

                for v in &arr {
                    let res = match &func {
                        Value::Function(NativeFn(f)) => (f)(self, vec![v.clone()]),
                        Value::UserFunction(u) => self._call_user_fn(u, vec![v.clone()]),
                        _ => return Err(err("map requires a function")),
                    }?;
                    out.push(res);
                }
                Ok(Value::Array(out))
            }
            "filter" => {
                if method_args.len() < 2 {
                    return Err(err("filter(fn)"));
                }
                let func = method_args[1].clone();
                let mut out: Vec<Value> = Vec::new();

                // PERFORMANCE NOTE (Phase 3 TODO): Same optimization needed as map()
                // See comment in "map" case above and PHASE3_IMPLEMENTATION_NOTES.md

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
                Ok(Value::Array(out))
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
                        other => result.push(other.clone()),
                    }
                }
                Ok(Value::Array(result))
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
                Ok(Value::Array(sliced))
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
                        other => result.push(other.clone()),
                    }
                }
                Ok(Value::Array(result))
            }
            "union" => {
                if method_args.len() != 2 {
                    return Err(err("union expects 1 arg"));
                }
                let other = match &method_args[1] {
                    Value::Array(o) => o.clone(),
                    Value::Set(o) => o.clone(),
                    Value::DynArray(da) => da.data.clone(),
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
                Ok(Value::Array(out))
            }
            "intersection" => {
                if method_args.len() != 2 {
                    return Err(err("intersection expects 1 arg"));
                }
                let other = match &method_args[1] {
                    Value::Array(o) => o.clone(),
                    Value::Set(o) => o.clone(),
                    Value::DynArray(da) => da.data.clone(),
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
                Ok(Value::Array(out))
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
