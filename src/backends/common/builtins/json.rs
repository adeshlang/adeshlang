//! JSON runtime operations for JIT/AOT builtins
//!
//! Provides runtime implementations for JSON.parse, JSON.stringify, JSON.stringifyPretty, etc.

use super::RuntimeValue;
use crate::utils::collections::FastMap;

const DEFAULT_MAX_DEPTH: usize = 128;

pub(crate) fn serde_json_to_runtime(v: &serde_json::Value) -> RuntimeValue {
    use serde_json::Value as JV;
    match v {
        JV::Null => RuntimeValue::Null,
        JV::Bool(b) => RuntimeValue::Bool(*b),
        JV::Number(n) => {
            if let Some(i) = n.as_i64() {
                RuntimeValue::Int(i)
            } else if let Some(u) = n.as_u64() {
                RuntimeValue::U64(u)
            } else if let Some(f) = n.as_f64() {
                RuntimeValue::Float(f)
            } else {
                RuntimeValue::Null
            }
        }
        JV::String(s) => RuntimeValue::String(s.clone()),
        JV::Array(a) => RuntimeValue::Array(a.iter().map(serde_json_to_runtime).collect()),
        JV::Object(m) => {
            let mut obj = FastMap::default();
            for (k, vv) in m.iter() {
                obj.insert(k.clone(), serde_json_to_runtime(vv));
            }
            RuntimeValue::Object(obj)
        }
    }
}

pub(crate) fn runtime_to_serde_json(v: &RuntimeValue) -> serde_json::Value {
    use serde_json::Value as JV;
    match v {
        RuntimeValue::Null => JV::Null,
        RuntimeValue::Bool(b) => JV::Bool(*b),
        RuntimeValue::Int(n) => JV::Number((*n).into()),
        RuntimeValue::Float(n) => serde_json::Number::from_f64(*n)
            .map(JV::Number)
            .unwrap_or(JV::Null),
        RuntimeValue::Char(c) => JV::String(c.to_string()),
        RuntimeValue::String(s) => JV::String(s.clone()),
        RuntimeValue::Array(a) => JV::Array(a.iter().map(runtime_to_serde_json).collect()),
        RuntimeValue::Tuple(t) => JV::Array(t.iter().map(runtime_to_serde_json).collect()),
        RuntimeValue::Set(s) => JV::Array(s.iter().map(runtime_to_serde_json).collect()),
        RuntimeValue::Object(m) => {
            let mut obj = serde_json::Map::new();
            for (k, vv) in m.iter() {
                obj.insert(k.clone(), runtime_to_serde_json(vv));
            }
            JV::Object(obj)
        }
        RuntimeValue::I8(n) => JV::Number((*n).into()),
        RuntimeValue::I16(n) => JV::Number((*n).into()),
        RuntimeValue::I32(n) => JV::Number((*n).into()),
        RuntimeValue::I64(n) => JV::Number((*n).into()),
        RuntimeValue::U8(n) => JV::Number((*n).into()),
        RuntimeValue::U16(n) => JV::Number((*n).into()),
        RuntimeValue::U32(n) => JV::Number((*n).into()),
        RuntimeValue::U64(n) => JV::Number((*n).into()),
        RuntimeValue::F32(n) => serde_json::Number::from_f64(*n as f64)
            .map(JV::Number)
            .unwrap_or(JV::Null),
        RuntimeValue::F64(n) => serde_json::Number::from_f64(*n)
            .map(JV::Number)
            .unwrap_or(JV::Null),
        _ => JV::Null,
    }
}

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

pub(crate) fn runtime_json_parse(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }
    let s = match &args[0] {
        RuntimeValue::String(x) => x.as_str(),
        _ => return RuntimeValue::Null,
    };
    match parse_json_str(s, DEFAULT_MAX_DEPTH) {
        Ok(v) => serde_json_to_runtime(&v),
        Err(_) => RuntimeValue::Null,
    }
}

pub(crate) fn runtime_json_stringify(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::String(String::new());
    }
    let jv = runtime_to_serde_json(&args[0]);
    match serde_json::to_string(&jv) {
        Ok(s) => RuntimeValue::String(s),
        Err(_) => RuntimeValue::String(String::new()),
    }
}

pub(crate) fn runtime_json_stringify_pretty(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::String(String::new());
    }
    let jv = runtime_to_serde_json(&args[0]);
    match serde_json::to_string_pretty(&jv) {
        Ok(s) => RuntimeValue::String(s),
        Err(_) => RuntimeValue::String(String::new()),
    }
}

pub(crate) fn runtime_json_is_valid(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Bool(false);
    }
    let s = match &args[0] {
        RuntimeValue::String(x) => x.as_str(),
        _ => return RuntimeValue::Bool(false),
    };
    RuntimeValue::Bool(serde_json::from_str::<serde_json::Value>(s).is_ok())
}

pub(crate) fn runtime_json_minify(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::String(String::new());
    }
    let s = match &args[0] {
        RuntimeValue::String(x) => x.as_str(),
        _ => return RuntimeValue::String(String::new()),
    };
    if let Ok(v) = parse_json_str(s, DEFAULT_MAX_DEPTH) {
        if let Ok(m) = serde_json::to_string(&v) {
            return RuntimeValue::String(m);
        }
    }
    RuntimeValue::String(String::new())
}

pub(crate) fn runtime_json_pretty(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::String(String::new());
    }
    let s = match &args[0] {
        RuntimeValue::String(x) => x.as_str(),
        _ => return RuntimeValue::String(String::new()),
    };
    if let Ok(v) = parse_json_str(s, DEFAULT_MAX_DEPTH) {
        let rv = serde_json_to_runtime(&v);
        return runtime_json_stringify_pretty(&[rv]);
    }
    RuntimeValue::String(String::new())
}

pub(crate) fn runtime_json_object(_args: &[RuntimeValue]) -> RuntimeValue {
    RuntimeValue::Object(FastMap::default())
}

pub(crate) fn runtime_json_array(_args: &[RuntimeValue]) -> RuntimeValue {
    RuntimeValue::Array(Vec::new())
}

pub(crate) fn runtime_json_from(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }
    let jv = runtime_to_serde_json(&args[0]);
    serde_json_to_runtime(&jv)
}
