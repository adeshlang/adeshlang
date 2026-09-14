//! Extended JSON standard library implementation for AdeshLang.
//!
//! Provides production-grade JSON functionality including:
//! - JSON.parse / JSON.parseBytes / JSON.parseFile / JSON.tryParse / JSON.isValid
//! - JSON.stringify / JSON.stringifyPretty / JSON.stringifyCompact / JSON.stringifyFile / JSON.stringifyBytes
//! - JSON.minify / JSON.pretty
//! - JSON.encode / JSON.decode / JSON.object / JSON.array / JSON.from

use crate::parsing::ast::{BuiltinEnv, Value};
use crate::stdlib::registry::BuiltinRegistry;
use rustc_hash::FxHashMap as HashMap;
use std::sync::Arc;

const DEFAULT_MAX_DEPTH: usize = 128;

/// Register all JSON standard library functions into the registry.
pub fn register(registry: &mut BuiltinRegistry) {
    registry.register(
        "JSON.parse",
        "json",
        "Parse JSON text into value",
        builtin_json_parse,
    );
    registry.register(
        "JSON.parseBytes",
        "json",
        "Parse JSON from UTF-8 bytes",
        builtin_json_parse_bytes,
    );
    registry.register(
        "JSON.parseFile",
        "json",
        "Parse JSON from file path",
        builtin_json_parse_file,
    );
    registry.register(
        "JSON.stringify",
        "json",
        "Serialize value to compact JSON string",
        builtin_json_stringify,
    );
    registry.register(
        "JSON.stringifyPretty",
        "json",
        "Serialize value to pretty-printed JSON string",
        builtin_json_stringify_pretty,
    );
    registry.register(
        "JSON.stringifyCompact",
        "json",
        "Serialize value to compact JSON string",
        builtin_json_stringify,
    );
    registry.register(
        "JSON.stringifyFile",
        "json",
        "Serialize value to JSON and write to file",
        builtin_json_stringify_file,
    );
    registry.register(
        "JSON.stringifyBytes",
        "json",
        "Serialize value to JSON UTF-8 bytes",
        builtin_json_stringify_bytes,
    );
    registry.register(
        "JSON.isValid",
        "json",
        "Check if string is valid JSON",
        builtin_json_is_valid,
    );
    registry.register(
        "JSON.tryParse",
        "json",
        "Safely parse JSON text without throwing",
        builtin_json_try_parse,
    );
    registry.register(
        "JSON.minify",
        "json",
        "Minify JSON text string",
        builtin_json_minify,
    );
    registry.register(
        "JSON.pretty",
        "json",
        "Format JSON text string with pretty printing",
        builtin_json_pretty,
    );
    registry.register(
        "JSON.encode",
        "json",
        "Encode value to JSON string",
        builtin_json_stringify,
    );
    registry.register(
        "JSON.decode",
        "json",
        "Decode JSON string to value",
        builtin_json_parse,
    );
    registry.register(
        "JSON.object",
        "json",
        "Create a new empty JSON object",
        builtin_json_object,
    );
    registry.register(
        "JSON.array",
        "json",
        "Create a new empty JSON array",
        builtin_json_array,
    );
    registry.register(
        "JSON.from",
        "json",
        "Convert value/collection to JSON value",
        builtin_json_from,
    );
}

/// Convert a serde_json::Value into an AdeshLang runtime Value, preserving numeric types.
pub fn json_to_value_ext(v: &serde_json::Value) -> Value {
    use serde_json::Value as JV;
    match v {
        JV::Null => Value::Null,
        JV::Bool(b) => Value::Bool(*b),
        JV::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::I64(i)
            } else if let Some(u) = n.as_u64() {
                Value::U64(u)
            } else if let Some(f) = n.as_f64() {
                Value::Number(f)
            } else {
                Value::Null
            }
        }
        JV::String(s) => Value::Str(s.clone()),
        JV::Array(a) => Value::Array(a.iter().map(json_to_value_ext).collect()),
        JV::Object(m) => {
            let mut hm = HashMap::default();
            for (k, vv) in m.iter() {
                hm.insert(k.clone(), json_to_value_ext(vv));
            }
            Value::Object(Arc::new(hm))
        }
    }
}

/// Helper function to convert an AdeshLang Value into serde_json::Value.
pub fn value_to_json_ext(v: &Value) -> serde_json::Value {
    crate::execution::runtime::format::value_to_json(v)
}

/// Enforce recursion depth limit to prevent stack overflow on deeply nested JSON.
fn check_depth(v: &serde_json::Value, current: usize, max: usize) -> Result<(), String> {
    if current > max {
        return Err(format!(
            "JSON parse error: Exceeded maximum nesting depth of {}",
            max
        ));
    }
    match v {
        serde_json::Value::Array(arr) => {
            for elem in arr {
                check_depth(elem, current + 1, max)?;
            }
        }
        serde_json::Value::Object(obj) => {
            for (_k, elem) in obj {
                check_depth(elem, current + 1, max)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Helper to parse text string into serde_json::Value with detailed error format.
fn parse_json_str(s: &str, max_depth: usize) -> Result<serde_json::Value, String> {
    let v: serde_json::Value = serde_json::from_str(s).map_err(|e| {
        format!(
            "JSON parse error: {} at line {}, column {}",
            e,
            e.line(),
            e.column()
        )
    })?;
    check_depth(&v, 1, max_depth)?;
    Ok(v)
}

/// Unwraps runtime references (Ref, Share) to get the underlying concrete Value.
pub fn unwrap_val(v: &Value) -> Value {
    match v {
        Value::Ref(inner, _) => unwrap_val(inner),
        Value::Share(sr) => {
            let inner_val = unsafe { &(*sr.ptr).value };
            unwrap_val(inner_val)
        }
        _ => v.clone(),
    }
}

pub fn builtin_json_parse(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("JSON.parse expects at least 1 argument".to_string());
    }
    let val = unwrap_val(&args[0]);
    match val {
        Value::Str(s) => {
            let max_depth = if args.len() > 1 {
                match unwrap_val(&args[1]) {
                    Value::Number(n) => n as usize,
                    Value::I64(i) => i as usize,
                    Value::U64(u) => u as usize,
                    _ => DEFAULT_MAX_DEPTH,
                }
            } else {
                DEFAULT_MAX_DEPTH
            };
            let v = parse_json_str(&s, max_depth)?;
            Ok(json_to_value_ext(&v))
        }
        Value::Object(_)
        | Value::Array(_)
        | Value::DynArray(_)
        | Value::RawArray(..)
        | Value::Tuple(_)
        | Value::Struct(_)
        | Value::Instance(_)
        | Value::Number(_)
        | Value::I64(_)
        | Value::U64(_)
        | Value::Bool(_)
        | Value::Null => Ok(val),
        _ => {
            let s = crate::execution::runtime_core::format::fmt(&val);
            let v = parse_json_str(&s, DEFAULT_MAX_DEPTH)?;
            Ok(json_to_value_ext(&v))
        }
    }
}

pub fn builtin_json_parse_bytes(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.is_empty() {
        return Err("JSON.parseBytes expects at least 1 argument".to_string());
    }
    let val = unwrap_val(&args[0]);
    let bytes: Vec<u8> = match val {
        Value::Str(s) => s.as_bytes().to_vec(),
        Value::Array(arr) => arr
            .iter()
            .filter_map(|v| match unwrap_val(v) {
                Value::U8(b) => Some(b),
                Value::I64(i) => Some(i as u8),
                Value::Number(n) => Some(n as u8),
                _ => None,
            })
            .collect(),
        Value::RawArray(_, arr) => arr
            .iter()
            .filter_map(|v| match unwrap_val(v) {
                Value::U8(b) => Some(b),
                Value::I64(i) => Some(i as u8),
                Value::Number(n) => Some(n as u8),
                _ => None,
            })
            .collect(),
        _ => return Err("JSON.parseBytes expects a string or array of byte numbers".to_string()),
    };
    let s = String::from_utf8(bytes)
        .map_err(|e| format!("JSON parseBytes error: Invalid UTF-8 sequence: {}", e))?;
    let v = parse_json_str(&s, DEFAULT_MAX_DEPTH)?;
    Ok(json_to_value_ext(&v))
}

pub fn builtin_json_parse_file(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.is_empty() {
        return Err("JSON.parseFile expects at least 1 argument".to_string());
    }
    let val = unwrap_val(&args[0]);
    let path = match &val {
        Value::Str(x) => x.as_str(),
        _ => return Err("JSON.parseFile expects a string file path".to_string()),
    };
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("JSON.parseFile error reading file '{}': {}", path, e))?;
    let v = parse_json_str(&content, DEFAULT_MAX_DEPTH)?;
    Ok(json_to_value_ext(&v))
}

pub fn builtin_json_stringify(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.is_empty() {
        return Err("JSON.stringify expects at least 1 argument".to_string());
    }
    let val = unwrap_val(&args[0]);
    let jv = value_to_json_ext(&val);
    let s = serde_json::to_string(&jv).map_err(|e| format!("JSON stringify error: {}", e))?;
    Ok(Value::Str(s))
}

pub fn builtin_json_stringify_pretty(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.is_empty() {
        return Err("JSON.stringifyPretty expects at least 1 argument".to_string());
    }
    let val = unwrap_val(&args[0]);
    let jv = value_to_json_ext(&val);
    let s = serde_json::to_string_pretty(&jv)
        .map_err(|e| format!("JSON stringifyPretty error: {}", e))?;
    Ok(Value::Str(s))
}

pub fn builtin_json_stringify_file(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("JSON.stringifyFile expects path and value arguments".to_string());
    }
    let path_val = unwrap_val(&args[0]);
    let path = match &path_val {
        Value::Str(s) => s.as_str(),
        _ => return Err("JSON.stringifyFile expects a string file path".to_string()),
    };
    let is_pretty = if args.len() > 2 {
        match unwrap_val(&args[2]) {
            Value::Bool(b) => b,
            _ => false,
        }
    } else {
        false
    };

    let s = if is_pretty {
        let res = builtin_json_stringify_pretty(_env, vec![args[1].clone()])?;
        match res {
            Value::Str(st) => st,
            _ => String::new(),
        }
    } else {
        let res = builtin_json_stringify(_env, vec![args[1].clone()])?;
        match res {
            Value::Str(st) => st,
            _ => String::new(),
        }
    };

    std::fs::write(path, s)
        .map_err(|e| format!("JSON.stringifyFile error writing to '{}': {}", path, e))?;
    Ok(Value::Bool(true))
}

pub fn builtin_json_stringify_bytes(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.is_empty() {
        return Err("JSON.stringifyBytes expects at least 1 argument".to_string());
    }
    let res = builtin_json_stringify(_env, args)?;
    let s = match res {
        Value::Str(st) => st,
        _ => String::new(),
    };
    let bytes_val: Vec<Value> = s.as_bytes().iter().map(|b| Value::U8(*b)).collect();
    Ok(Value::Array(bytes_val))
}

pub fn builtin_json_is_valid(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Ok(Value::Bool(false));
    }
    let val = unwrap_val(&args[0]);
    match val {
        Value::Str(s) => {
            let valid = serde_json::from_str::<serde_json::Value>(&s).is_ok();
            Ok(Value::Bool(valid))
        }
        Value::Object(_)
        | Value::Array(_)
        | Value::DynArray(_)
        | Value::RawArray(..)
        | Value::Tuple(_)
        | Value::Struct(_)
        | Value::Instance(_)
        | Value::Number(_)
        | Value::I64(_)
        | Value::U64(_)
        | Value::Bool(_)
        | Value::Null => Ok(Value::Bool(true)),
        _ => Ok(Value::Bool(false)),
    }
}

pub fn builtin_json_try_parse(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.is_empty() {
        return Ok(Value::Null);
    }
    match builtin_json_parse(_env, args) {
        Ok(v) => Ok(v),
        Err(_) => Ok(Value::Null),
    }
}

pub fn builtin_json_minify(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("JSON.minify expects a string argument".to_string());
    }
    let val = unwrap_val(&args[0]);
    let s = match &val {
        Value::Str(x) => x.as_str(),
        _ => return Err("JSON.minify expects a string argument".to_string()),
    };
    let v = parse_json_str(s, DEFAULT_MAX_DEPTH)?;
    let minified = serde_json::to_string(&v).map_err(|e| format!("JSON minify error: {}", e))?;
    Ok(Value::Str(minified))
}

pub fn builtin_json_pretty(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("JSON.pretty expects a string argument".to_string());
    }
    let val = unwrap_val(&args[0]);
    let s = match &val {
        Value::Str(x) => x.as_str(),
        _ => return Err("JSON.pretty expects a string argument".to_string()),
    };
    let v = parse_json_str(s, DEFAULT_MAX_DEPTH)?;
    let val_lang = json_to_value_ext(&v);
    builtin_json_stringify_pretty(_env, vec![val_lang])
}

pub fn builtin_json_object(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    Ok(Value::Object(Arc::new(HashMap::default())))
}

pub fn builtin_json_array(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    Ok(Value::Array(Vec::new()))
}

pub fn builtin_json_from(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Ok(Value::Null);
    }
    let jv = value_to_json_ext(&args[0]);
    Ok(json_to_value_ext(&jv))
}
