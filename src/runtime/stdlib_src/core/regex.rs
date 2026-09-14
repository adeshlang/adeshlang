//! Regex Standard Library Module
//!
//! Exposes a production-grade Regular Expression system.
use crate::parsing::ast::{BuiltinEnv, NativeFn, Value};
use crate::stdlib::registry::BuiltinRegistry;
use crate::utils::collections::FastMap as HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_REGEX_ID: AtomicU64 = AtomicU64::new(1);
pub static REGEX_CACHE: once_cell::sync::Lazy<Mutex<std::collections::HashMap<u64, regex::Regex>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(std::collections::HashMap::new()));

const FLAG_IGNORE_CASE: i64 = 1;
const FLAG_MULTILINE: i64 = 2;
const FLAG_DOT_ALL: i64 = 4;
const FLAG_EXTENDED: i64 = 8;
const FLAG_UNICODE: i64 = 16;

pub fn make_some(val: Value) -> Value {
    let mut m = HashMap::default();
    m.insert("__enum".into(), Value::Str("Option".to_string()));
    m.insert("tag".into(), Value::Str("Some".to_string()));
    m.insert("value".into(), val);
    Value::Object(Arc::new(m))
}

pub fn make_none() -> Value {
    let mut m = HashMap::default();
    m.insert("__enum".into(), Value::Str("Option".to_string()));
    m.insert("tag".into(), Value::Str("None".to_string()));
    Value::Object(Arc::new(m))
}

pub fn make_ok(val: Value) -> Value {
    let mut m = HashMap::default();
    m.insert("__enum".into(), Value::Str("Result".to_string()));
    m.insert("tag".into(), Value::Str("Ok".to_string()));
    m.insert("value".into(), val);
    Value::Object(Arc::new(m))
}

pub fn make_err(msg: Value) -> Value {
    let mut m = HashMap::default();
    m.insert("__enum".into(), Value::Str("Result".to_string()));
    m.insert("tag".into(), Value::Str("Err".to_string()));
    m.insert("value".into(), msg);
    Value::Object(Arc::new(m))
}

pub fn make_match_obj(text: String, start: usize, end: usize) -> Value {
    let mut m = HashMap::default();
    m.insert("__type".into(), Value::Str("Match".to_string()));
    m.insert("text_val".into(), Value::Str(text));
    m.insert("start_val".into(), Value::Number(start as f64));
    m.insert("end_val".into(), Value::Number(end as f64));
    Value::Object(Arc::new(m))
}

pub fn make_captures_obj(groups: Vec<Value>, names: HashMap<String, Value>) -> Value {
    let mut m = HashMap::default();
    m.insert("__type".into(), Value::Str("Captures".to_string()));
    m.insert("groups".into(), Value::Array(groups));
    m.insert("names_map".into(), Value::Object(Arc::new(names)));
    Value::Object(Arc::new(m))
}

pub fn make_regex_obj(pattern: String, id: u64, flags: i64) -> Value {
    let mut m = HashMap::default();
    m.insert("__type".into(), Value::Str("Regex".to_string()));
    m.insert("pattern_val".into(), Value::Str(pattern));
    m.insert("id_val".into(), Value::Number(id as f64));
    m.insert("flags_val".into(), Value::Number(flags as f64));
    Value::Object(Arc::new(m))
}

// Compile a regex and insert it into the cache
pub fn compile_regex(pattern: &str, flags: i64) -> Result<u64, String> {
    let mut builder = regex::RegexBuilder::new(pattern);
    builder.case_insensitive((flags & FLAG_IGNORE_CASE) != 0);
    builder.multi_line((flags & FLAG_MULTILINE) != 0);
    builder.dot_matches_new_line((flags & FLAG_DOT_ALL) != 0);
    builder.ignore_whitespace((flags & FLAG_EXTENDED) != 0);
    builder.unicode(true);

    match builder.build() {
        Ok(re) => {
            let id = NEXT_REGEX_ID.fetch_add(1, Ordering::SeqCst);
            REGEX_CACHE.lock().unwrap().insert(id, re);
            Ok(id)
        }
        Err(e) => Err(e.to_string()),
    }
}

// Helper to get string from a Value
pub fn val_to_str(val: &Value) -> Option<String> {
    match val {
        Value::Str(s) => Some(s.clone()),
        _ => None,
    }
}

// Helper to convert any numeric Value variant to i64
pub fn val_to_i64(val: &Value) -> Option<i64> {
    match val {
        Value::Number(n) => Some(*n as i64),
        Value::I32(n) => Some(*n as i64),
        Value::I64(n) => Some(*n),
        Value::U32(n) => Some(*n as i64),
        Value::U64(n) => Some(*n as i64),
        Value::I16(n) => Some(*n as i64),
        Value::U16(n) => Some(*n as i64),
        Value::I8(n) => Some(*n as i64),
        Value::U8(n) => Some(*n as i64),
        Value::F32(n) => Some(*n as i64),
        Value::F64(n) => Some(*n as i64),
        _ => None,
    }
}

pub fn builtin_new(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Regex.new expects pattern and optional flags".to_string());
    }
    let pattern =
        val_to_str(&args[0]).ok_or_else(|| "Regex pattern must be a string".to_string())?;
    let flags = if args.len() > 1 {
        val_to_i64(&args[1]).unwrap_or(0)
    } else {
        0
    };

    match compile_regex(&pattern, flags) {
        Ok(id) => Ok(make_regex_obj(pattern, id, flags)),
        Err(e) => Err(format!("Regex compilation failed: {}", e)),
    }
}

pub fn builtin_compile(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Regex.compile expects pattern and optional flags".to_string());
    }
    let pattern =
        val_to_str(&args[0]).ok_or_else(|| "Regex pattern must be a string".to_string())?;
    let flags = if args.len() > 1 {
        val_to_i64(&args[1]).unwrap_or(0)
    } else {
        0
    };

    match compile_regex(&pattern, flags) {
        Ok(id) => Ok(make_ok(make_regex_obj(pattern, id, flags))),
        Err(e) => Ok(make_err(Value::Str(format!(
            "Regex compilation failed: {}",
            e
        )))),
    }
}

pub fn builtin_escape(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Regex.escape expects a string".to_string());
    }
    let text = val_to_str(&args[0]).ok_or_else(|| "Regex.escape expects a string".to_string())?;
    Ok(Value::Str(regex::escape(&text)))
}

pub fn register_all(registry: &mut BuiltinRegistry) {
    registry.register(
        "Regex",
        "core",
        "Regex namespace for regular expressions",
        |_env: &mut dyn BuiltinEnv, _args: Vec<Value>| {
            let mut methods = HashMap::default();

            // Creators
            methods.insert(
                "new".to_string(),
                Value::Function(NativeFn(Arc::new(builtin_new))),
            );
            methods.insert(
                "compile".to_string(),
                Value::Function(NativeFn(Arc::new(builtin_compile))),
            );
            methods.insert(
                "escape".to_string(),
                Value::Function(NativeFn(Arc::new(builtin_escape))),
            );

            // Flags Enum Constants
            methods.insert(
                "IgnoreCase".to_string(),
                Value::Number(FLAG_IGNORE_CASE as f64),
            );
            methods.insert(
                "Multiline".to_string(),
                Value::Number(FLAG_MULTILINE as f64),
            );
            methods.insert("DotAll".to_string(), Value::Number(FLAG_DOT_ALL as f64));
            methods.insert("Extended".to_string(), Value::Number(FLAG_EXTENDED as f64));
            methods.insert("Unicode".to_string(), Value::Number(FLAG_UNICODE as f64));

            methods.insert(
                "CASE_INSENSITIVE".to_string(),
                Value::Number(FLAG_IGNORE_CASE as f64),
            );
            methods.insert(
                "MULTILINE".to_string(),
                Value::Number(FLAG_MULTILINE as f64),
            );
            methods.insert("DOT_ALL".to_string(), Value::Number(FLAG_DOT_ALL as f64));
            methods.insert("EXTENDED".to_string(), Value::Number(FLAG_EXTENDED as f64));
            methods.insert("UNICODE".to_string(), Value::Number(FLAG_UNICODE as f64));

            Ok(Value::Object(Arc::new(methods)))
        },
    );
}

// Intercept Regex, Match and Captures methods in the interpreter
pub fn get_regex_prop(obj: &HashMap<String, Value>, key: &str) -> Option<Result<Value, String>> {
    let type_val = obj.get("__type")?;
    let type_str = match type_val {
        Value::Str(s) => s.as_str(),
        _ => return None,
    };

    match type_str {
        "Regex" => {
            let pattern = obj.get("pattern_val").cloned().unwrap_or(Value::Null);
            let id = match obj.get("id_val") {
                Some(Value::Number(n)) => *n as u64,
                _ => return Some(Err("Invalid Regex ID".to_string())),
            };
            let flags = match obj.get("flags_val") {
                Some(Value::Number(n)) => *n as i64,
                _ => 0,
            };

            match key {
                "pattern" => Some(Ok(Value::Function(NativeFn(Arc::new(
                    move |_env, _args| Ok(pattern.clone()),
                ))))),
                "flags" => Some(Ok(Value::Function(NativeFn(Arc::new(
                    move |_env, _args| Ok(Value::Number(flags as f64)),
                ))))),
                "test" | "isMatch" => Some(Ok(Value::Function(NativeFn(Arc::new(
                    move |_env, args| {
                        if args.is_empty() {
                            return Err("test() expects a string".to_string());
                        }
                        let text = val_to_str(&args[0])
                            .ok_or_else(|| "test() expects a string".to_string())?;
                        let cache = REGEX_CACHE.lock().unwrap();
                        let re = cache
                            .get(&id)
                            .ok_or_else(|| "Regex not found in cache".to_string())?;
                        Ok(Value::Bool(re.is_match(&text)))
                    },
                ))))),
                "find" => Some(Ok(Value::Function(NativeFn(Arc::new(
                    move |_env, args| {
                        if args.is_empty() {
                            return Err("find() expects a string".to_string());
                        }
                        let text = val_to_str(&args[0])
                            .ok_or_else(|| "find() expects a string".to_string())?;
                        let cache = REGEX_CACHE.lock().unwrap();
                        let re = cache
                            .get(&id)
                            .ok_or_else(|| "Regex not found in cache".to_string())?;
                        if let Some(m) = re.find(&text) {
                            Ok(make_some(make_match_obj(
                                m.as_str().to_string(),
                                m.start(),
                                m.end(),
                            )))
                        } else {
                            Ok(make_none())
                        }
                    },
                ))))),
                "findAll" | "findIter" => Some(Ok(Value::Function(NativeFn(Arc::new(
                    move |_env, args| {
                        if args.is_empty() {
                            return Err("findAll() expects a string".to_string());
                        }
                        let text = val_to_str(&args[0])
                            .ok_or_else(|| "findAll() expects a string".to_string())?;
                        let cache = REGEX_CACHE.lock().unwrap();
                        let re = cache
                            .get(&id)
                            .ok_or_else(|| "Regex not found in cache".to_string())?;
                        let mut matches = Vec::new();
                        for m in re.find_iter(&text) {
                            matches.push(make_match_obj(
                                m.as_str().to_string(),
                                m.start(),
                                m.end(),
                            ));
                        }
                        Ok(Value::Array(matches))
                    },
                ))))),
                "fullMatch" => Some(Ok(Value::Function(NativeFn(Arc::new(
                    move |_env, args| {
                        if args.is_empty() {
                            return Err("fullMatch() expects a string".to_string());
                        }
                        let text = val_to_str(&args[0])
                            .ok_or_else(|| "fullMatch() expects a string".to_string())?;
                        let cache = REGEX_CACHE.lock().unwrap();
                        let re = cache
                            .get(&id)
                            .ok_or_else(|| "Regex not found in cache".to_string())?;
                        if let Some(m) = re.find(&text) {
                            Ok(Value::Bool(m.start() == 0 && m.end() == text.len()))
                        } else {
                            Ok(Value::Bool(false))
                        }
                    },
                ))))),
                "matchStart" | "match" => Some(Ok(Value::Function(NativeFn(Arc::new(
                    move |_env, args| {
                        if args.is_empty() {
                            return Err("matchStart() expects a string".to_string());
                        }
                        let text = val_to_str(&args[0])
                            .ok_or_else(|| "matchStart() expects a string".to_string())?;
                        let cache = REGEX_CACHE.lock().unwrap();
                        let re = cache
                            .get(&id)
                            .ok_or_else(|| "Regex not found in cache".to_string())?;
                        if let Some(m) = re.find(&text) {
                            if m.start() == 0 {
                                Ok(make_some(make_match_obj(
                                    m.as_str().to_string(),
                                    m.start(),
                                    m.end(),
                                )))
                            } else {
                                Ok(make_none())
                            }
                        } else {
                            Ok(make_none())
                        }
                    },
                ))))),
                "captures" => Some(Ok(Value::Function(NativeFn(Arc::new(
                    move |_env, args| {
                        if args.is_empty() {
                            return Err("captures() expects a string".to_string());
                        }
                        let text = val_to_str(&args[0])
                            .ok_or_else(|| "captures() expects a string".to_string())?;
                        let cache = REGEX_CACHE.lock().unwrap();
                        let re = cache
                            .get(&id)
                            .ok_or_else(|| "Regex not found in cache".to_string())?;
                        if let Some(caps) = re.captures(&text) {
                            let mut groups = Vec::new();
                            for i in 0..caps.len() {
                                if let Some(m) = caps.get(i) {
                                    groups.push(make_some(make_match_obj(
                                        m.as_str().to_string(),
                                        m.start(),
                                        m.end(),
                                    )));
                                } else {
                                    groups.push(make_none());
                                }
                            }
                            let mut names = HashMap::default();
                            for name in re.capture_names().flatten() {
                                if let Some(m) = caps.name(name) {
                                    names.insert(
                                        name.to_string(),
                                        make_some(make_match_obj(
                                            m.as_str().to_string(),
                                            m.start(),
                                            m.end(),
                                        )),
                                    );
                                } else {
                                    names.insert(name.to_string(), make_none());
                                }
                            }
                            Ok(make_some(make_captures_obj(groups, names)))
                        } else {
                            Ok(make_none())
                        }
                    },
                ))))),
                "capturesAll" | "capturesIter" => Some(Ok(Value::Function(NativeFn(Arc::new(
                    move |_env, args| {
                        if args.is_empty() {
                            return Err("capturesAll() expects a string".to_string());
                        }
                        let text = val_to_str(&args[0])
                            .ok_or_else(|| "capturesAll() expects a string".to_string())?;
                        let cache = REGEX_CACHE.lock().unwrap();
                        let re = cache
                            .get(&id)
                            .ok_or_else(|| "Regex not found in cache".to_string())?;
                        let mut captures = Vec::new();
                        for caps in re.captures_iter(&text) {
                            let mut groups = Vec::new();
                            for i in 0..caps.len() {
                                if let Some(m) = caps.get(i) {
                                    groups.push(make_some(make_match_obj(
                                        m.as_str().to_string(),
                                        m.start(),
                                        m.end(),
                                    )));
                                } else {
                                    groups.push(make_none());
                                }
                            }
                            let mut names = HashMap::default();
                            for name in re.capture_names().flatten() {
                                if let Some(m) = caps.name(name) {
                                    names.insert(
                                        name.to_string(),
                                        make_some(make_match_obj(
                                            m.as_str().to_string(),
                                            m.start(),
                                            m.end(),
                                        )),
                                    );
                                } else {
                                    names.insert(name.to_string(), make_none());
                                }
                            }
                            captures.push(make_captures_obj(groups, names));
                        }
                        Ok(Value::Array(captures))
                    },
                ))))),
                "replace" => Some(Ok(Value::Function(NativeFn(Arc::new(
                    move |_env, args| {
                        if args.len() < 2 {
                            return Err(
                                "replace(text, replacement) expects 2 arguments".to_string()
                            );
                        }
                        let text = val_to_str(&args[0])
                            .ok_or_else(|| "replace() expects a string".to_string())?;
                        let rep = val_to_str(&args[1])
                            .ok_or_else(|| "replace() replacement must be a string".to_string())?;
                        let cache = REGEX_CACHE.lock().unwrap();
                        let re = cache
                            .get(&id)
                            .ok_or_else(|| "Regex not found in cache".to_string())?;
                        Ok(Value::Str(re.replace(&text, rep.as_str()).to_string()))
                    },
                ))))),
                "replaceAll" => Some(Ok(Value::Function(NativeFn(Arc::new(
                    move |_env, args| {
                        if args.len() < 2 {
                            return Err(
                                "replaceAll(text, replacement) expects 2 arguments".to_string()
                            );
                        }
                        let text = val_to_str(&args[0])
                            .ok_or_else(|| "replaceAll() expects a string".to_string())?;
                        let rep = val_to_str(&args[1]).ok_or_else(|| {
                            "replaceAll() replacement must be a string".to_string()
                        })?;
                        let cache = REGEX_CACHE.lock().unwrap();
                        let re = cache
                            .get(&id)
                            .ok_or_else(|| "Regex not found in cache".to_string())?;
                        Ok(Value::Str(re.replace_all(&text, rep.as_str()).to_string()))
                    },
                ))))),
                "split" => Some(Ok(Value::Function(NativeFn(Arc::new(
                    move |_env, args| {
                        if args.is_empty() {
                            return Err("split() expects a string".to_string());
                        }
                        let text = val_to_str(&args[0])
                            .ok_or_else(|| "split() expects a string".to_string())?;
                        let cache = REGEX_CACHE.lock().unwrap();
                        let re = cache
                            .get(&id)
                            .ok_or_else(|| "Regex not found in cache".to_string())?;
                        let parts: Vec<Value> =
                            re.split(&text).map(|s| Value::Str(s.to_string())).collect();
                        Ok(Value::Array(parts))
                    },
                ))))),
                "splitN" => Some(Ok(Value::Function(NativeFn(Arc::new(
                    move |_env, args| {
                        if args.len() < 2 {
                            return Err("splitN(text, limit) expects 2 arguments".to_string());
                        }
                        let text = val_to_str(&args[0])
                            .ok_or_else(|| "splitN() expects a string".to_string())?;
                        let limit = match &args[1] {
                            Value::Number(n) => *n as usize,
                            _ => 0,
                        };
                        let cache = REGEX_CACHE.lock().unwrap();
                        let re = cache
                            .get(&id)
                            .ok_or_else(|| "Regex not found in cache".to_string())?;
                        let parts: Vec<Value> = re
                            .splitn(&text, limit)
                            .map(|s| Value::Str(s.to_string()))
                            .collect();
                        Ok(Value::Array(parts))
                    },
                ))))),
                _ => None,
            }
        }
        "Match" => {
            let text_val = obj.get("text_val").cloned().unwrap_or(Value::Null);
            let start_val = obj.get("start_val").cloned().unwrap_or(Value::Null);
            let end_val = obj.get("end_val").cloned().unwrap_or(Value::Null);

            match key {
                "text" => Some(Ok(Value::Function(NativeFn(Arc::new(
                    move |_env, _args| Ok(text_val.clone()),
                ))))),
                "start" => Some(Ok(Value::Function(NativeFn(Arc::new(
                    move |_env, _args| Ok(start_val.clone()),
                ))))),
                "end" => Some(Ok(Value::Function(NativeFn(Arc::new(
                    move |_env, _args| Ok(end_val.clone()),
                ))))),
                "range" => {
                    let start = start_val.clone();
                    let end = end_val.clone();
                    Some(Ok(Value::Function(NativeFn(Arc::new(
                        move |_env, _args| Ok(Value::Tuple(vec![start.clone(), end.clone()])),
                    )))))
                }
                "len" => {
                    let s = match start_val {
                        Value::Number(n) => n,
                        _ => 0.0,
                    };
                    let e = match end_val {
                        Value::Number(n) => n,
                        _ => 0.0,
                    };
                    Some(Ok(Value::Function(NativeFn(Arc::new(
                        move |_env, _args| Ok(Value::Number(e - s)),
                    )))))
                }
                "isEmpty" => {
                    let s = match start_val {
                        Value::Number(n) => n,
                        _ => 0.0,
                    };
                    let e = match end_val {
                        Value::Number(n) => n,
                        _ => 0.0,
                    };
                    Some(Ok(Value::Function(NativeFn(Arc::new(
                        move |_env, _args| Ok(Value::Bool(s == e)),
                    )))))
                }
                _ => None,
            }
        }
        "Captures" => {
            let groups = obj.get("groups").cloned().unwrap_or(Value::Null);
            let names_map = obj.get("names_map").cloned().unwrap_or(Value::Null);

            match key {
                "len" => {
                    let size = match &groups {
                        Value::Array(a) => a.len(),
                        _ => 0,
                    };
                    Some(Ok(Value::Function(NativeFn(Arc::new(
                        move |_env, _args| Ok(Value::Number(size as f64)),
                    )))))
                }
                "get" | "group" => {
                    let grps = groups.clone();
                    Some(Ok(Value::Function(NativeFn(Arc::new(
                        move |_env, args| {
                            if args.is_empty() {
                                return Err("get() expects an index".to_string());
                            }
                            let idx = match &args[0] {
                                Value::Number(n) => *n as usize,
                                _ => return Err("Index must be a number".to_string()),
                            };
                            match &grps {
                                Value::Array(a) => {
                                    if idx < a.len() {
                                        Ok(a[idx].clone())
                                    } else {
                                        Ok(make_none())
                                    }
                                }
                                _ => Ok(make_none()),
                            }
                        },
                    )))))
                }
                "name" => {
                    let n_map = names_map.clone();
                    Some(Ok(Value::Function(NativeFn(Arc::new(
                        move |_env, args| {
                            if args.is_empty() {
                                return Err("name() expects a name".to_string());
                            }
                            let name = val_to_str(&args[0])
                                .ok_or_else(|| "name() expects a name string".to_string())?;
                            match &n_map {
                                Value::Object(m) => {
                                    if let Some(v) = m.get(&name) {
                                        Ok(v.clone())
                                    } else {
                                        Ok(make_none())
                                    }
                                }
                                _ => Ok(make_none()),
                            }
                        },
                    )))))
                }
                "names" => {
                    let n_map = names_map.clone();
                    Some(Ok(Value::Function(NativeFn(Arc::new(
                        move |_env, _args| match &n_map {
                            Value::Object(m) => {
                                let keys: Vec<Value> =
                                    m.keys().map(|k| Value::Str(k.clone())).collect();
                                Ok(Value::Array(keys))
                            }
                            _ => Ok(Value::Array(vec![])),
                        },
                    )))))
                }
                "full" => {
                    let grps = groups.clone();
                    Some(Ok(Value::Function(NativeFn(Arc::new(
                        move |_env, _args| match &grps {
                            Value::Array(a) => {
                                if !a.is_empty() {
                                    Ok(a[0].clone())
                                } else {
                                    Ok(make_none())
                                }
                            }
                            _ => Ok(make_none()),
                        },
                    )))))
                }
                _ => None,
            }
        }
        _ => None,
    }
}
