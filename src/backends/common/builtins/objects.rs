//! Object and class operations for the AdeshLang runtime.
//!
//! This module provides builtin functions for creating and manipulating objects,
//! classes, dictionaries, sets, tuples, and arrays. It includes operations for:
//! - Class instantiation and management
//! - Object creation and field access
//! - Dictionary and set operations
//! - Tuple construction
//! - Array type conversions (raw, dynamic, fixed)
//! - Method dispatch for object method calls

use super::RuntimeValue;
use super::arrays::infer_array_type;
use super::io::{runtime_input_checkbox, runtime_input_radio};
use crate::utils::collections::FastMap;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

pub static JIT_ARC_VALUES: OnceLock<Mutex<HashMap<u64, RuntimeValue>>> = OnceLock::new();

pub fn get_jit_arc_value(handle: u64) -> Option<RuntimeValue> {
    JIT_ARC_VALUES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()?
        .get(&handle)
        .cloned()
}

#[allow(dead_code)]
pub fn insert_jit_arc_value(handle: u64, value: RuntimeValue) {
    if let Some(mut guard) = JIT_ARC_VALUES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()
    {
        guard.insert(handle, value);
    }
}

#[allow(dead_code)]
pub fn remove_jit_arc_value(handle: u64) {
    if let Some(mut guard) = JIT_ARC_VALUES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()
    {
        guard.remove(&handle);
    }
}

/// Create a new instance of a class
/// args[0] = class name, args[1..] = constructor arguments
pub(crate) fn runtime_new_class(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }

    let class_name = args[0].as_string();

    // Create an object with __class__ field set
    let mut obj = FastMap::default();
    obj.insert(
        "__class__".to_string(),
        RuntimeValue::String(class_name.clone()),
    );

    // Store constructor args as properties (simplified)
    // In a full implementation, we'd call the init method
    for (i, arg) in args.iter().skip(1).enumerate() {
        obj.insert(format!("arg{}", i), arg.clone());
    }

    RuntimeValue::Object(obj)
}

/// Create a class object for storing in a variable
/// Returns an object with __type__: "class"
pub(crate) fn runtime_make_class_object(_args: &[RuntimeValue]) -> RuntimeValue {
    let mut obj = FastMap::default();
    obj.insert(
        "__type__".to_string(),
        RuntimeValue::String("class".to_string()),
    );
    RuntimeValue::Object(obj)
}

/// Set the name field on a class object
/// args[0] = object, args[1] = class name
pub(crate) fn runtime_set_class_name(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Null;
    }
    // For class objects, the field value is the class name
    if let RuntimeValue::Object(mut obj) = args[0].clone() {
        if let RuntimeValue::String(name) = &args[1] {
            obj.insert("name".to_string(), RuntimeValue::String(name.clone()));
            return RuntimeValue::Object(obj);
        }
    }
    args[0].clone()
}

/// Create an empty object or object with key-value pairs
/// If args is empty, creates empty object
/// Otherwise, expects pairs: key1, value1, key2, value2, ...
pub(crate) fn runtime_make_object(args: &[RuntimeValue]) -> RuntimeValue {
    let mut obj = FastMap::default();

    // If we have arguments, they should come in key-value pairs
    if !args.is_empty() {
        // Process arguments as key-value pairs
        for chunk in args.chunks(2) {
            if chunk.len() == 2 {
                let key = chunk[0].as_string();
                let value = chunk[1].clone();
                obj.insert(key, value);
            }
        }
    }

    RuntimeValue::Object(obj)
}

/// Create an empty dictionary
pub(crate) fn runtime_make_dict(_args: &[RuntimeValue]) -> RuntimeValue {
    RuntimeValue::Object(FastMap::default())
}

/// Create a set from arguments or an array
pub(crate) fn runtime_make_set(args: &[RuntimeValue]) -> RuntimeValue {
    let source: Vec<RuntimeValue> = if let Some(first) = args.first() {
        match first {
            RuntimeValue::Array(arr) | RuntimeValue::Set(arr) => arr.clone(),
            RuntimeValue::Tuple(tup) => tup.clone(),
            _ => args.to_vec(),
        }
    } else {
        Vec::new()
    };

    let mut unique: Vec<RuntimeValue> = Vec::new();
    for value in source {
        if !unique.iter().any(|existing| existing == &value) {
            unique.push(value);
        }
    }
    RuntimeValue::Set(unique)
}

/// Create a tuple from arguments
pub(crate) fn runtime_make_tuple(args: &[RuntimeValue]) -> RuntimeValue {
    RuntimeValue::Tuple(args.to_vec())
}

pub(crate) fn runtime_get_field(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Null;
    }
    let key = args[1].as_string();
    match &args[0] {
        RuntimeValue::Object(obj) => obj.get(&key).cloned().unwrap_or(RuntimeValue::Null),
        RuntimeValue::Array(arr) => match key.as_str() {
            "len" | "length" => RuntimeValue::Int(arr.len() as i64),
            "capacity" => RuntimeValue::Int(arr.capacity() as i64),
            "metadata_size" => {
                super::arrays::runtime_metadata_size(&[RuntimeValue::Array(arr.clone())])
            }
            _ => RuntimeValue::Null,
        },
        RuntimeValue::DynArray {
            data,
            element_type,
            tracked_capacity,
            ..
        } => match key.as_str() {
            "len" | "length" => RuntimeValue::Int(data.len() as i64),
            "capacity" => {
                RuntimeValue::Int(tracked_capacity.unwrap_or_else(|| data.capacity()) as i64)
            }
            "metadata_size" => {
                let elem_clean = element_type.trim_start_matches('[').trim_end_matches(']');
                let metadata = if elem_clean.starts_with("u8")
                    || elem_clean.starts_with("i8")
                    || elem_clean.starts_with("u16")
                    || elem_clean.starts_with("i16")
                    || elem_clean.starts_with("u32")
                    || elem_clean.starts_with("i32")
                    || elem_clean.starts_with("f32")
                    || elem_clean.starts_with("bool")
                    || elem_clean.starts_with("char")
                {
                    16
                } else {
                    24
                };
                RuntimeValue::Int(metadata as i64)
            }
            _ => RuntimeValue::Null,
        },
        RuntimeValue::RawArray(_, arr) => match key.as_str() {
            "len" | "length" => RuntimeValue::Int(arr.len() as i64),
            "capacity" => RuntimeValue::Int(arr.len() as i64),
            "metadata_size" => RuntimeValue::Int(0),
            _ => RuntimeValue::Null,
        },
        RuntimeValue::Tuple(tup) => match key.as_str() {
            "len" | "length" => RuntimeValue::Int(tup.len() as i64),
            "capacity" => RuntimeValue::Int(tup.len() as i64),
            "metadata_size" => RuntimeValue::Int(0),
            _ => RuntimeValue::Null,
        },
        RuntimeValue::String(s) => match key.as_str() {
            "len" | "length" => RuntimeValue::Int(s.len() as i64),
            _ => RuntimeValue::Null,
        },
        RuntimeValue::U64(handle) => {
            if let Some(rv) = get_jit_arc_value(*handle) {
                return runtime_get_field(&[rv, args[1].clone()]);
            }
            if let Ok(guard) = crate::execution::runtime_core::arc_bridge::arc_manager().lock() {
                match key.as_str() {
                    "strong_count" => {
                        let sc = guard.strong_count(*handle).unwrap_or(0);
                        return RuntimeValue::Int(sc as i64);
                    }
                    "weak_count" => {
                        let wc = guard.weak_count(*handle).unwrap_or(0);
                        return RuntimeValue::Int(wc as i64);
                    }
                    "is_alive" => {
                        let sc = guard.strong_count(*handle).unwrap_or(0);
                        return RuntimeValue::Bool(sc > 0);
                    }
                    _ => {
                        if let Ok(val) = guard.get_value(*handle) {
                            let convert_val = |v: &crate::parsing::ast::Value| match v {
                                crate::parsing::ast::Value::Number(n) => RuntimeValue::Float(*n),
                                crate::parsing::ast::Value::I64(n) => RuntimeValue::Int(*n),
                                crate::parsing::ast::Value::U64(n) => RuntimeValue::Int(*n as i64),
                                crate::parsing::ast::Value::Str(s) => {
                                    RuntimeValue::String(s.clone())
                                }
                                crate::parsing::ast::Value::Bool(b) => RuntimeValue::Bool(*b),
                                crate::parsing::ast::Value::Object(m) => {
                                    let mut map = crate::utils::collections::FastMap::default();
                                    for (k, val_inner) in m.iter() {
                                        let v_rv = match val_inner {
                                            crate::parsing::ast::Value::Number(n) => {
                                                RuntimeValue::Float(*n)
                                            }
                                            crate::parsing::ast::Value::I64(n) => {
                                                RuntimeValue::Int(*n)
                                            }
                                            crate::parsing::ast::Value::U64(n) => {
                                                RuntimeValue::Int(*n as i64)
                                            }
                                            crate::parsing::ast::Value::Str(s) => {
                                                RuntimeValue::String(s.clone())
                                            }
                                            crate::parsing::ast::Value::Bool(b) => {
                                                RuntimeValue::Bool(*b)
                                            }
                                            _ => RuntimeValue::Null,
                                        };
                                        map.insert(k.clone(), v_rv);
                                    }
                                    RuntimeValue::Object(map)
                                }
                                _ => RuntimeValue::Null,
                            };
                            let rv = convert_val(&val);
                            return runtime_get_field(&[rv, args[1].clone()]);
                        }
                    }
                }
            }
            RuntimeValue::Null
        }
        _ => RuntimeValue::Null,
    }
}

/// Set a field value on an object
/// NOTE: Returns a new object with the field set - does not modify in place
/// args[0] = object, args[1] = field name, args[2] = value
pub(crate) fn runtime_set_field(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 3 {
        return RuntimeValue::Null;
    }
    match &args[0] {
        RuntimeValue::Object(obj) => {
            let mut new_obj = (*obj).clone();
            let key = args[1].as_string();
            new_obj.insert(key, args[2].clone());
            RuntimeValue::Object(new_obj)
        }
        _ => RuntimeValue::Null,
    }
}

/// Set a key-value pair in a dictionary object
/// NOTE: Returns a new object with the key set - does not modify in place
/// The caller should use the returned value
/// args[0] = dictionary, args[1] = key, args[2] = value
pub(crate) fn runtime_dict_set(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 3 {
        return RuntimeValue::Null;
    }
    match &args[0] {
        RuntimeValue::Object(obj) => {
            let mut new_obj = (*obj).clone();
            let key = args[1].as_string();
            new_obj.insert(key, args[2].clone());
            RuntimeValue::Object(new_obj)
        }
        _ => RuntimeValue::Null,
    }
}

/// Optional chaining field access - returns null if object is null
/// args[0] = object, args[1] = field name
pub(crate) fn runtime_optional_get(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Null;
    }
    let obj = &args[0];
    let field = &args[1];

    // If the object is null/undefined, return null
    if matches!(obj, RuntimeValue::Null) {
        return RuntimeValue::Null;
    }

    // Get the field name
    let field_name = match field {
        RuntimeValue::String(s) => s.clone(),
        RuntimeValue::Int(i) => i.to_string(),
        _ => return RuntimeValue::Null,
    };

    // Access the field
    match obj {
        RuntimeValue::Object(map) => map.get(&field_name).cloned().unwrap_or(RuntimeValue::Null),
        RuntimeValue::Array(arr) => {
            if let Ok(idx) = field_name.parse::<usize>() {
                arr.get(idx).cloned().unwrap_or(RuntimeValue::Null)
            } else {
                RuntimeValue::Null
            }
        }
        _ => RuntimeValue::Null,
    }
}

/// Implement __call_method to route object method calls to appropriate builtins
/// args[0] = object, args[1] = method name, args[2..] = method arguments
pub(crate) fn runtime_call_method(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Null;
    }

    let obj = &args[0];
    let method_name = args[1].as_string();

    let make_none = || {
        let mut m = FastMap::default();
        m.insert("__enum".into(), RuntimeValue::String("Option".to_string()));
        m.insert("tag".into(), RuntimeValue::String("None".to_string()));
        RuntimeValue::Object(m)
    };

    if let RuntimeValue::Object(m) = obj {
        if let Some(RuntimeValue::String(ty)) = m.get("__type") {
            match ty.as_str() {
                "Regex" => match method_name.as_str() {
                    "pattern" => {
                        return m.get("pattern_val").cloned().unwrap_or(RuntimeValue::Null);
                    }
                    "flags" => {
                        return m.get("flags_val").cloned().unwrap_or(RuntimeValue::Null);
                    }
                    "test" | "isMatch" => {
                        let r_args: Vec<RuntimeValue> = args.iter().skip(2).cloned().collect();
                        let combined_args = std::iter::once(obj.clone())
                            .chain(r_args)
                            .collect::<Vec<_>>();
                        return super::runtime_regex_test(&combined_args);
                    }
                    "find" => {
                        let r_args: Vec<RuntimeValue> = args.iter().skip(2).cloned().collect();
                        let combined_args = std::iter::once(obj.clone())
                            .chain(r_args)
                            .collect::<Vec<_>>();
                        return super::runtime_regex_find(&combined_args);
                    }
                    "findAll" | "findIter" => {
                        let r_args: Vec<RuntimeValue> = args.iter().skip(2).cloned().collect();
                        let combined_args = std::iter::once(obj.clone())
                            .chain(r_args)
                            .collect::<Vec<_>>();
                        return super::runtime_regex_find_all(&combined_args);
                    }
                    "fullMatch" => {
                        let r_args: Vec<RuntimeValue> = args.iter().skip(2).cloned().collect();
                        let combined_args = std::iter::once(obj.clone())
                            .chain(r_args)
                            .collect::<Vec<_>>();
                        return super::runtime_regex_full_match(&combined_args);
                    }
                    "matchStart" | "match" => {
                        let r_args: Vec<RuntimeValue> = args.iter().skip(2).cloned().collect();
                        let combined_args = std::iter::once(obj.clone())
                            .chain(r_args)
                            .collect::<Vec<_>>();
                        return super::runtime_regex_match_start(&combined_args);
                    }
                    "captures" => {
                        let r_args: Vec<RuntimeValue> = args.iter().skip(2).cloned().collect();
                        let combined_args = std::iter::once(obj.clone())
                            .chain(r_args)
                            .collect::<Vec<_>>();
                        return super::runtime_regex_captures(&combined_args);
                    }
                    "capturesAll" | "capturesIter" => {
                        let r_args: Vec<RuntimeValue> = args.iter().skip(2).cloned().collect();
                        let combined_args = std::iter::once(obj.clone())
                            .chain(r_args)
                            .collect::<Vec<_>>();
                        return super::runtime_regex_captures_all(&combined_args);
                    }
                    "replace" => {
                        let r_args: Vec<RuntimeValue> = args.iter().skip(2).cloned().collect();
                        let combined_args = std::iter::once(obj.clone())
                            .chain(r_args)
                            .collect::<Vec<_>>();
                        return super::runtime_regex_replace(&combined_args);
                    }
                    "replaceAll" => {
                        let r_args: Vec<RuntimeValue> = args.iter().skip(2).cloned().collect();
                        let combined_args = std::iter::once(obj.clone())
                            .chain(r_args)
                            .collect::<Vec<_>>();
                        return super::runtime_regex_replace_all(&combined_args);
                    }
                    "split" => {
                        let r_args: Vec<RuntimeValue> = args.iter().skip(2).cloned().collect();
                        let combined_args = std::iter::once(obj.clone())
                            .chain(r_args)
                            .collect::<Vec<_>>();
                        return super::runtime_regex_split(&combined_args);
                    }
                    "splitN" => {
                        let r_args: Vec<RuntimeValue> = args.iter().skip(2).cloned().collect();
                        let combined_args = std::iter::once(obj.clone())
                            .chain(r_args)
                            .collect::<Vec<_>>();
                        return super::runtime_regex_split_n(&combined_args);
                    }
                    _ => {}
                },
                "Match" => match method_name.as_str() {
                    "text" => {
                        return m.get("text_val").cloned().unwrap_or(RuntimeValue::Null);
                    }
                    "start" => {
                        return m.get("start_val").cloned().unwrap_or(RuntimeValue::Null);
                    }
                    "end" => {
                        return m.get("end_val").cloned().unwrap_or(RuntimeValue::Null);
                    }
                    "range" => {
                        let start = m
                            .get("start_val")
                            .cloned()
                            .unwrap_or(RuntimeValue::Float(0.0));
                        let end = m
                            .get("end_val")
                            .cloned()
                            .unwrap_or(RuntimeValue::Float(0.0));
                        return RuntimeValue::Tuple(vec![start, end]);
                    }
                    "len" => {
                        let start = match m.get("start_val") {
                            Some(RuntimeValue::Float(f)) => *f,
                            _ => 0.0,
                        };
                        let end = match m.get("end_val") {
                            Some(RuntimeValue::Float(f)) => *f,
                            _ => 0.0,
                        };
                        return RuntimeValue::Float(end - start);
                    }
                    "isEmpty" => {
                        let start = match m.get("start_val") {
                            Some(RuntimeValue::Float(f)) => *f,
                            _ => 0.0,
                        };
                        let end = match m.get("end_val") {
                            Some(RuntimeValue::Float(f)) => *f,
                            _ => 0.0,
                        };
                        return RuntimeValue::Bool(start == end);
                    }
                    _ => {}
                },
                "Captures" => match method_name.as_str() {
                    "len" => {
                        let size = match m.get("groups") {
                            Some(RuntimeValue::Array(a)) => a.len(),
                            _ => 0,
                        };
                        return RuntimeValue::Float(size as f64);
                    }
                    "get" | "group" => {
                        if args.len() < 3 {
                            return make_none();
                        }
                        let idx = args[2].as_int().unwrap_or(0) as usize;
                        match m.get("groups") {
                            Some(RuntimeValue::Array(a)) => {
                                if idx < a.len() {
                                    return a[idx].clone();
                                }
                            }
                            _ => {}
                        }
                        return make_none();
                    }
                    "name" => {
                        if args.len() < 3 {
                            return make_none();
                        }
                        let name = args[2].as_string();
                        match m.get("names_map") {
                            Some(RuntimeValue::Object(nm)) => {
                                if let Some(v) = nm.get(&name) {
                                    return v.clone();
                                }
                            }
                            _ => {}
                        }
                        return make_none();
                    }
                    "names" => {
                        match m.get("names_map") {
                            Some(RuntimeValue::Object(nm)) => {
                                let keys =
                                    nm.keys().map(|k| RuntimeValue::String(k.clone())).collect();
                                return RuntimeValue::Array(keys);
                            }
                            _ => {}
                        }
                        return RuntimeValue::Array(vec![]);
                    }
                    "full" => {
                        match m.get("groups") {
                            Some(RuntimeValue::Array(a)) => {
                                if !a.is_empty() {
                                    return a[0].clone();
                                }
                            }
                            _ => {}
                        }
                        return make_none();
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        // Option/Result support in JIT/AOT
        if let Some(RuntimeValue::String(enum_name)) = m.get("__enum") {
            let tag = match m.get("tag") {
                Some(RuntimeValue::String(s)) => s.clone(),
                _ => String::new(),
            };
            let value = m.get("value").cloned().unwrap_or(RuntimeValue::Null);
            if enum_name == "Option" {
                match method_name.as_str() {
                    "isSome" => return RuntimeValue::Bool(tag == "Some"),
                    "isNone" => return RuntimeValue::Bool(tag == "None"),
                    "unwrap" => {
                        if tag == "Some" {
                            return value;
                        } else {
                            panic!("Called Option.unwrap() on a None value");
                        }
                    }
                    "unwrapOr" => {
                        if tag == "Some" {
                            return value;
                        } else {
                            if args.len() > 2 {
                                return args[2].clone();
                            } else {
                                return RuntimeValue::Null;
                            }
                        }
                    }
                    _ => {}
                }
            } else if enum_name == "Result" {
                match method_name.as_str() {
                    "isOk" => return RuntimeValue::Bool(tag == "Ok"),
                    "isErr" => return RuntimeValue::Bool(tag == "Err"),
                    "unwrap" => {
                        if tag == "Ok" {
                            return value;
                        } else {
                            panic!("Called Result.unwrap() on an Err value");
                        }
                    }
                    "unwrapOr" => {
                        if tag == "Ok" {
                            return value;
                        } else {
                            if args.len() > 2 {
                                return args[2].clone();
                            } else {
                                return RuntimeValue::Null;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Build method args: [obj, ...rest_args]
    let method_args: Vec<RuntimeValue> = std::iter::once(obj.clone())
        .chain(args.iter().skip(2).cloned())
        .collect();

    // Dispatch to known methods based on method name
    // Note: We can't access the registry from a static function,
    // so we manually dispatch to implemented methods
    match method_name.as_str() {
        // Array methods
        "append" | "push" => super::runtime_push(&method_args),
        "pop" => super::runtime_pop(&method_args),
        "shift" => super::runtime_shift(&method_args),
        "unshift" => super::runtime_unshift(&method_args),
        "insert" => super::runtime_insert(&method_args),
        "remove" => super::runtime_remove(&method_args),
        "clear" => super::runtime_clear(&method_args),
        "extend" | "concat" => super::runtime_array_concat(&method_args),
        "set_index" => super::runtime_set_index(&method_args),
        "get_index" => super::runtime_get_index(&method_args),
        "map" => super::runtime_map(&method_args),
        "filter" => super::runtime_filter(&method_args),
        "reduce" => super::runtime_reduce(&method_args),
        "count" => super::runtime_count(&method_args),
        "index" | "indexOf" => match &method_args[0] {
            RuntimeValue::String(_) => super::runtime_index_of(&method_args),
            _ => super::runtime_array_index_of(&method_args),
        },
        "lastIndexOf" => match &method_args[0] {
            RuntimeValue::String(_) => super::runtime_last_index_of(&method_args),
            _ => super::runtime_array_last_index_of(&method_args),
        },
        "sort" => super::runtime_sort(&method_args),
        "reverse" => super::runtime_reverse(&method_args),
        "slice" => match &method_args[0] {
            RuntimeValue::String(_) => super::runtime_slice(&method_args),
            _ => super::runtime_slice_array(&method_args),
        },
        "join" => super::runtime_join(&method_args),
        "includes" | "contains" => match &method_args[0] {
            RuntimeValue::String(_) => super::runtime_includes(&method_args),
            _ => super::runtime_array_includes(&method_args),
        },
        "flat" => super::runtime_array_flat(&method_args),
        "sum" => super::runtime_sum(&method_args),
        "min" => super::runtime_array_min(&method_args),
        "max" => super::runtime_array_max(&method_args),
        "distinct" => super::runtime_distinct(&method_args),
        "toSet" => super::runtime_to_set(&method_args),
        "toTuple" => super::runtime_to_tuple(&method_args),
        "metadata_size" => super::runtime_metadata_size(&method_args),
        "capacity" => super::runtime_capacity(&method_args),
        "first" => super::runtime_first(&method_args),
        "last" => super::runtime_last(&method_args),
        "find" => super::runtime_find(&method_args),
        "findIndex" => super::runtime_find_index(&method_args),
        "forEach" => super::runtime_for_each(&method_args),
        "some" => super::runtime_some(&method_args),
        "every" => super::runtime_every(&method_args),

        // Date methods
        "toISOString" => super::runtime_date_to_iso_string(&method_args),
        "getTime" => super::runtime_date_get_time(&method_args),
        "toString" => super::runtime_date_to_string(&method_args),
        "getFullYear" => super::runtime_date_get_full_year(&method_args),
        "getMonth" => super::runtime_date_get_month(&method_args),
        "getDate" => super::runtime_date_get_date(&method_args),
        "getHours" => super::runtime_date_get_hours(&method_args),
        "getMinutes" => super::runtime_date_get_minutes(&method_args),
        "getSeconds" => super::runtime_date_get_seconds(&method_args),
        "getMilliseconds" => super::runtime_date_get_milliseconds(&method_args),
        "getDay" => super::runtime_date_get_day(&method_args),
        "getUTCFullYear" => super::runtime_date_get_utc_full_year(&method_args),
        "getUTCMonth" => super::runtime_date_get_utc_month(&method_args),
        "getUTCDate" => super::runtime_date_get_utc_date(&method_args),
        "getUTCHours" => super::runtime_date_get_utc_hours(&method_args),
        "getUTCMinutes" => super::runtime_date_get_utc_minutes(&method_args),
        "getUTCSeconds" => super::runtime_date_get_utc_seconds(&method_args),
        "getUTCMilliseconds" => super::runtime_date_get_utc_milliseconds(&method_args),
        "getTimezoneOffset" => super::runtime_date_get_timezone_offset(&method_args),
        "toLocaleString" => super::runtime_date_to_locale_string(&method_args),
        "toLocaleDateString" => super::runtime_date_to_locale_date_string(&method_args),
        "toLocaleTimeString" => super::runtime_date_to_locale_time_string(&method_args),
        // Set methods
        "union" => super::runtime_set_union(&method_args),
        "intersection" => super::runtime_set_intersection(&method_args),
        "add" => super::runtime_set_add(&method_args),
        "has" => super::runtime_set_has(&method_args),
        "delete" => super::runtime_set_delete(&method_args),
        // Promise methods
        "then" => super::runtime_promise_then(&method_args),
        "catch" => super::runtime_promise_catch(&method_args),
        // String methods
        "split" => super::runtime_split(&method_args),
        "substring" | "substr" => super::runtime_substr(&method_args),
        "charAt" => super::runtime_char_at(&method_args),
        "startsWith" => super::runtime_starts_with(&method_args),
        "endsWith" => super::runtime_ends_with(&method_args),
        "trim" => super::runtime_trim(&method_args),
        "trimStart" | "trimLeft" => super::runtime_trim_start(&method_args),
        "trimEnd" | "trimRight" => super::runtime_trim_end(&method_args),
        "toLowerCase" | "lower" => super::runtime_to_lower_case(&method_args),
        "toUpperCase" | "upper" => super::runtime_to_upper_case(&method_args),
        "replace" => super::runtime_replace(&method_args),
        "repeat" => super::runtime_repeat(&method_args),
        // Input methods
        "mock" => super::io::runtime_input_mock(&method_args),
        "checkbox" => runtime_input_checkbox(&method_args),
        "radio" => runtime_input_radio(&method_args),
        // If not found, try generic field access fallback or return null
        _ => {
            // Universal Stdlib Method Fallback for any module/object method call (e.g. FS.readFile, Path.join, Crypto.sha256, etc.)
            if let RuntimeValue::Object(m) = obj {
                let target_name = if let Some(type_str) = m
                    .get("__name")
                    .or_else(|| m.get("__type"))
                    .map(|v| v.as_string())
                {
                    format!("{}.{}", type_str, method_name)
                } else {
                    method_name.clone()
                };
                let res = super::stdlib_bridge::dispatch_stdlib_builtin(&target_name, &method_args);
                if res != RuntimeValue::Null {
                    return res;
                }
            }
            let res = super::stdlib_bridge::dispatch_stdlib_builtin(&method_name, &method_args);
            if res != RuntimeValue::Null {
                return res;
            }
            RuntimeValue::Null
        }
    }
}

/// Convert an array to a raw array with typed elements
/// args[0] = array, optional args[1] = element type
pub(crate) fn runtime_array_to_raw(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }
    let opt_elem_type = if args.len() > 1 {
        Some(args[1].as_string())
    } else {
        None
    };
    match &args[0] {
        RuntimeValue::DynArray {
            data, element_type, ..
        } => {
            let ty = opt_elem_type.unwrap_or_else(|| element_type.clone());
            let mut coerced = Vec::with_capacity(data.len());
            for v in data {
                coerced.push(coerce_elem_to_type(v, &ty).unwrap_or_else(|| v.clone()));
            }
            RuntimeValue::RawArray(ty, coerced)
        }
        RuntimeValue::Array(arr) => {
            let ty = opt_elem_type.unwrap_or_else(|| infer_array_type(arr).0);
            let mut coerced = Vec::with_capacity(arr.len());
            for v in arr {
                coerced.push(coerce_elem_to_type(v, &ty).unwrap_or_else(|| v.clone()));
            }
            RuntimeValue::RawArray(ty, coerced)
        }
        RuntimeValue::RawArray(elem_type, data) => {
            if let Some(ty) = opt_elem_type {
                let mut coerced = Vec::with_capacity(data.len());
                for v in data {
                    coerced.push(coerce_elem_to_type(v, &ty).unwrap_or_else(|| v.clone()));
                }
                RuntimeValue::RawArray(ty, coerced)
            } else {
                args[0].clone()
            }
        }
        _ => args[0].clone(), // Already raw or other type
    }
}

/// Convert an array to a dynamic array
/// args[0] = array, optional args[1] = element type
pub(crate) fn runtime_array_to_dynamic(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }
    let opt_elem_type = if args.len() > 1 {
        Some(args[1].as_string())
    } else {
        None
    };
    match &args[0] {
        RuntimeValue::RawArray(elem_type, data) => {
            let target_elem_type = opt_elem_type.unwrap_or_else(|| elem_type.clone());
            let concrete_type = format!("[{}]", target_elem_type);
            let mut coerced = Vec::with_capacity(data.len());
            for v in data {
                coerced
                    .push(coerce_elem_to_type(v, &target_elem_type).unwrap_or_else(|| v.clone()));
            }
            RuntimeValue::DynArray {
                data: coerced,
                element_type: target_elem_type,
                concrete_type,
                tracked_capacity: None,
            }
        }
        RuntimeValue::Array(data) => {
            if let Some(target_elem_type) = opt_elem_type {
                let concrete_type = format!("[{}]", target_elem_type);
                let mut coerced = Vec::with_capacity(data.len());
                for v in data {
                    coerced.push(
                        coerce_elem_to_type(v, &target_elem_type).unwrap_or_else(|| v.clone()),
                    );
                }
                RuntimeValue::DynArray {
                    data: coerced,
                    element_type: target_elem_type,
                    concrete_type,
                    tracked_capacity: None,
                }
            } else {
                let (element_type, concrete_type) = infer_array_type(data);
                RuntimeValue::DynArray {
                    data: data.clone(),
                    element_type,
                    concrete_type,
                    tracked_capacity: None,
                }
            }
        }
        RuntimeValue::DynArray {
            data,
            element_type,
            tracked_capacity,
            ..
        } => {
            if let Some(target_elem_type) = opt_elem_type {
                let concrete_type = format!("[{}]", target_elem_type);
                let mut coerced = Vec::with_capacity(data.len());
                for v in data {
                    coerced.push(
                        coerce_elem_to_type(v, &target_elem_type).unwrap_or_else(|| v.clone()),
                    );
                }
                RuntimeValue::DynArray {
                    data: coerced,
                    element_type: target_elem_type,
                    concrete_type,
                    tracked_capacity: *tracked_capacity,
                }
            } else {
                args[0].clone()
            }
        }
        _ => args[0].clone(), // Already dynamic or other type
    }
}

/// Coerce an element to a specific type
fn coerce_elem_to_type(v: &RuntimeValue, t: &str) -> Option<RuntimeValue> {
    match t.to_lowercase().as_str() {
        "u8" => v.as_int().map(|n| RuntimeValue::U8(n as u8)),
        "u16" => v.as_int().map(|n| RuntimeValue::U16(n as u16)),
        "u32" => v.as_int().map(|n| RuntimeValue::U32(n as u32)),
        "u64" => v.as_int().map(|n| RuntimeValue::U64(n as u64)),
        "u128" => v.as_int().map(|n| RuntimeValue::U128(n as u128)),
        "i8" => v.as_int().map(|n| RuntimeValue::I8(n as i8)),
        "i16" => v.as_int().map(|n| RuntimeValue::I16(n as i16)),
        "i32" => v.as_int().map(|n| RuntimeValue::I32(n as i32)),
        "i64" => v.as_int().map(|n| RuntimeValue::I64(n as i64)),
        "i128" => v.as_int().map(|n| RuntimeValue::I128(n as i128)),
        "f32" => v.as_float().map(|n| RuntimeValue::F32(n as f32)),
        "f64" | "float" => v.as_float().map(|n| RuntimeValue::F64(n)),
        "string" | "str" => Some(RuntimeValue::String(v.as_string())),
        "bool" | "boolean" => v.as_bool().map(RuntimeValue::Bool),
        _ => Some(v.clone()),
    }
}

/// Convert an array to a fixed-size dynamic array with type coercion
/// args[0] = array, args[1] = element type, args[2] = capacity
pub(crate) fn runtime_array_to_fixed(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 3 || matches!(&args[0], RuntimeValue::Null) {
        return RuntimeValue::Null;
    }
    let elem_type = args[1].as_string();
    let n = args[2].as_int().unwrap_or(0).max(0) as usize;
    let elements: Vec<RuntimeValue> = match &args[0] {
        RuntimeValue::Array(arr) => arr.clone(),
        RuntimeValue::RawArray(_, arr) => arr.clone(),
        RuntimeValue::DynArray { data, .. } => data.clone(),
        _ => vec![],
    };
    let mut coerced: Vec<RuntimeValue> = Vec::with_capacity(elements.len());
    for v in elements.iter() {
        let cv = coerce_elem_to_type(v, &elem_type).unwrap_or(RuntimeValue::Null);
        coerced.push(cv);
    }
    RuntimeValue::DynArray {
        data: coerced,
        element_type: elem_type.clone(),
        concrete_type: format!("[{}]", elem_type),
        tracked_capacity: Some(n),
    }
}

/// Convert an array to a fixed-size raw array with type coercion and padding
/// args[0] = array, args[1] = element type, args[2] = size
pub(crate) fn runtime_array_to_fixed_raw(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 3 || matches!(&args[0], RuntimeValue::Null) {
        return RuntimeValue::Null;
    }
    let elem_type = args[1].as_string();
    let n = args[2].as_int().unwrap_or(0).max(0) as usize;
    let elements: Vec<RuntimeValue> = match &args[0] {
        RuntimeValue::Array(arr) => arr.clone(),
        RuntimeValue::RawArray(_, arr) => arr.clone(),
        RuntimeValue::DynArray { data, .. } => data.clone(),
        _ => vec![],
    };
    let mut coerced: Vec<RuntimeValue> = Vec::with_capacity(elements.len());
    for v in elements.iter() {
        let cv = coerce_elem_to_type(v, &elem_type).unwrap_or(RuntimeValue::Null);
        coerced.push(cv);
    }
    if coerced.len() < n {
        let pad_count = n - coerced.len();
        let default = match elem_type.to_lowercase().as_str() {
            "u8" => RuntimeValue::U8(0),
            "u16" => RuntimeValue::U16(0),
            "u32" => RuntimeValue::U32(0),
            "u64" => RuntimeValue::U64(0),
            "u128" => RuntimeValue::U128(0),
            "i8" => RuntimeValue::I8(0),
            "i16" => RuntimeValue::I16(0),
            "i32" => RuntimeValue::I32(0),
            "i64" => RuntimeValue::I64(0),
            "i128" => RuntimeValue::I128(0),
            "f32" => RuntimeValue::F32(0.0),
            "f64" | "float" => RuntimeValue::F64(0.0),
            "string" | "str" => RuntimeValue::String(String::new()),
            "bool" | "boolean" => RuntimeValue::Bool(false),
            _ => RuntimeValue::Null,
        };
        for _ in 0..pad_count {
            coerced.push(default.clone());
        }
    } else if coerced.len() > n {
        coerced.truncate(n);
    }
    RuntimeValue::RawArray(elem_type.clone(), coerced)
}
