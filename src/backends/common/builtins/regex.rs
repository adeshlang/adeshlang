//! Regex JIT/AOT Builtins
use super::RuntimeValue;
use crate::utils::collections::FastMap;

fn make_some(val: RuntimeValue) -> RuntimeValue {
    let mut m = FastMap::default();
    m.insert("__enum".into(), RuntimeValue::String("Option".to_string()));
    m.insert("tag".into(), RuntimeValue::String("Some".to_string()));
    m.insert("value".into(), val);
    RuntimeValue::Object(m)
}

fn make_none() -> RuntimeValue {
    let mut m = FastMap::default();
    m.insert("__enum".into(), RuntimeValue::String("Option".to_string()));
    m.insert("tag".into(), RuntimeValue::String("None".to_string()));
    RuntimeValue::Object(m)
}

fn make_ok(val: RuntimeValue) -> RuntimeValue {
    let mut m = FastMap::default();
    m.insert("__enum".into(), RuntimeValue::String("Result".to_string()));
    m.insert("tag".into(), RuntimeValue::String("Ok".to_string()));
    m.insert("value".into(), val);
    RuntimeValue::Object(m)
}

fn make_err(msg: RuntimeValue) -> RuntimeValue {
    let mut m = FastMap::default();
    m.insert("__enum".into(), RuntimeValue::String("Result".to_string()));
    m.insert("tag".into(), RuntimeValue::String("Err".to_string()));
    m.insert("value".into(), msg);
    RuntimeValue::Object(m)
}

fn make_match_obj(text: String, start: usize, end: usize) -> RuntimeValue {
    let mut m = FastMap::default();
    m.insert("__type".into(), RuntimeValue::String("Match".to_string()));
    m.insert("text_val".into(), RuntimeValue::String(text));
    m.insert("start_val".into(), RuntimeValue::Float(start as f64));
    m.insert("end_val".into(), RuntimeValue::Float(end as f64));
    RuntimeValue::Object(m)
}

fn make_captures_obj(
    groups: Vec<RuntimeValue>,
    names: FastMap<String, RuntimeValue>,
) -> RuntimeValue {
    let mut m = FastMap::default();
    m.insert(
        "__type".into(),
        RuntimeValue::String("Captures".to_string()),
    );
    m.insert("groups".into(), RuntimeValue::Array(groups));
    m.insert("names_map".into(), RuntimeValue::Object(names));
    RuntimeValue::Object(m)
}

fn make_regex_obj(pattern: String, id: u64, flags: i64) -> RuntimeValue {
    let mut m = FastMap::default();
    m.insert("__type".into(), RuntimeValue::String("Regex".to_string()));
    m.insert("pattern_val".into(), RuntimeValue::String(pattern));
    m.insert("id_val".into(), RuntimeValue::Float(id as f64));
    m.insert("flags_val".into(), RuntimeValue::Float(flags as f64));
    RuntimeValue::Object(m)
}

pub fn runtime_regex_new(args: &[RuntimeValue]) -> RuntimeValue {
    println!("DEBUG runtime_regex_new args: {:?}", args);
    if args.is_empty() {
        return RuntimeValue::Null;
    }
    let pattern = match &args[0] {
        RuntimeValue::String(s) => s.clone(),
        _ => return RuntimeValue::Null,
    };
    let flags = if args.len() > 1 {
        args[1].as_int().unwrap_or(0)
    } else {
        0
    };
    match crate::runtime::stdlib_src::core::regex::compile_regex(&pattern, flags) {
        Ok(id) => make_regex_obj(pattern, id, flags),
        _ => RuntimeValue::Null,
    }
}

pub fn runtime_regex_compile(args: &[RuntimeValue]) -> RuntimeValue {
    println!("DEBUG runtime_regex_compile args: {:?}", args);
    if args.is_empty() {
        return RuntimeValue::Null;
    }
    let pattern = match &args[0] {
        RuntimeValue::String(s) => s.clone(),
        _ => return RuntimeValue::Null,
    };
    let flags = if args.len() > 1 {
        args[1].as_int().unwrap_or(0)
    } else {
        0
    };
    match crate::runtime::stdlib_src::core::regex::compile_regex(&pattern, flags) {
        Ok(id) => make_ok(make_regex_obj(pattern, id, flags)),
        Err(e) => make_err(RuntimeValue::String(format!(
            "Regex compilation failed: {}",
            e
        ))),
    }
}

pub fn runtime_regex_escape(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }
    let text = match &args[0] {
        RuntimeValue::String(s) => s.clone(),
        _ => return RuntimeValue::Null,
    };
    RuntimeValue::String(regex::escape(&text))
}

pub fn runtime_regex_test(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Bool(false);
    }
    let id = match &args[0] {
        RuntimeValue::Object(m) => match m.get("id_val") {
            Some(RuntimeValue::Float(n)) => *n as u64,
            _ => return RuntimeValue::Bool(false),
        },
        _ => return RuntimeValue::Bool(false),
    };
    let text = match &args[1] {
        RuntimeValue::String(s) => s.clone(),
        _ => return RuntimeValue::Bool(false),
    };

    let cache = crate::runtime::stdlib_src::core::regex::REGEX_CACHE
        .lock()
        .unwrap();
    if let Some(re) = cache.get(&id) {
        RuntimeValue::Bool(re.is_match(&text))
    } else {
        RuntimeValue::Bool(false)
    }
}

pub fn runtime_regex_find(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return make_none();
    }
    let id = match &args[0] {
        RuntimeValue::Object(m) => match m.get("id_val") {
            Some(RuntimeValue::Float(n)) => *n as u64,
            _ => return make_none(),
        },
        _ => return make_none(),
    };
    let text = match &args[1] {
        RuntimeValue::String(s) => s.clone(),
        _ => return make_none(),
    };

    let cache = crate::runtime::stdlib_src::core::regex::REGEX_CACHE
        .lock()
        .unwrap();
    if let Some(re) = cache.get(&id) {
        if let Some(m) = re.find(&text) {
            make_some(make_match_obj(m.as_str().to_string(), m.start(), m.end()))
        } else {
            make_none()
        }
    } else {
        make_none()
    }
}

pub fn runtime_regex_find_all(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Array(vec![]);
    }
    let id = match &args[0] {
        RuntimeValue::Object(m) => match m.get("id_val") {
            Some(RuntimeValue::Float(n)) => *n as u64,
            _ => return RuntimeValue::Array(vec![]),
        },
        _ => return RuntimeValue::Array(vec![]),
    };
    let text = match &args[1] {
        RuntimeValue::String(s) => s.clone(),
        _ => return RuntimeValue::Array(vec![]),
    };

    let cache = crate::runtime::stdlib_src::core::regex::REGEX_CACHE
        .lock()
        .unwrap();
    if let Some(re) = cache.get(&id) {
        let mut matches = Vec::new();
        for m in re.find_iter(&text) {
            matches.push(make_match_obj(m.as_str().to_string(), m.start(), m.end()));
        }
        RuntimeValue::Array(matches)
    } else {
        RuntimeValue::Array(vec![])
    }
}

pub fn runtime_regex_full_match(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Bool(false);
    }
    let id = match &args[0] {
        RuntimeValue::Object(m) => match m.get("id_val") {
            Some(RuntimeValue::Float(n)) => *n as u64,
            _ => return RuntimeValue::Bool(false),
        },
        _ => return RuntimeValue::Bool(false),
    };
    let text = match &args[1] {
        RuntimeValue::String(s) => s.clone(),
        _ => return RuntimeValue::Bool(false),
    };

    let cache = crate::runtime::stdlib_src::core::regex::REGEX_CACHE
        .lock()
        .unwrap();
    if let Some(re) = cache.get(&id) {
        if let Some(m) = re.find(&text) {
            RuntimeValue::Bool(m.start() == 0 && m.end() == text.len())
        } else {
            RuntimeValue::Bool(false)
        }
    } else {
        RuntimeValue::Bool(false)
    }
}

pub fn runtime_regex_match_start(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return make_none();
    }
    let id = match &args[0] {
        RuntimeValue::Object(m) => match m.get("id_val") {
            Some(RuntimeValue::Float(n)) => *n as u64,
            _ => return make_none(),
        },
        _ => return make_none(),
    };
    let text = match &args[1] {
        RuntimeValue::String(s) => s.clone(),
        _ => return make_none(),
    };

    let cache = crate::runtime::stdlib_src::core::regex::REGEX_CACHE
        .lock()
        .unwrap();
    if let Some(re) = cache.get(&id) {
        if let Some(m) = re.find(&text) {
            if m.start() == 0 {
                make_some(make_match_obj(m.as_str().to_string(), m.start(), m.end()))
            } else {
                make_none()
            }
        } else {
            make_none()
        }
    } else {
        make_none()
    }
}

pub fn runtime_regex_captures(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return make_none();
    }
    let id = match &args[0] {
        RuntimeValue::Object(m) => match m.get("id_val") {
            Some(RuntimeValue::Float(n)) => *n as u64,
            _ => return make_none(),
        },
        _ => return make_none(),
    };
    let text = match &args[1] {
        RuntimeValue::String(s) => s.clone(),
        _ => return make_none(),
    };

    let cache = crate::runtime::stdlib_src::core::regex::REGEX_CACHE
        .lock()
        .unwrap();
    if let Some(re) = cache.get(&id) {
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
            let mut names = FastMap::default();
            for name in re.capture_names().flatten() {
                if let Some(m) = caps.name(name) {
                    names.insert(
                        name.to_string(),
                        make_some(make_match_obj(m.as_str().to_string(), m.start(), m.end())),
                    );
                } else {
                    names.insert(name.to_string(), make_none());
                }
            }
            make_some(make_captures_obj(groups, names))
        } else {
            make_none()
        }
    } else {
        make_none()
    }
}

pub fn runtime_regex_captures_all(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Array(vec![]);
    }
    let id = match &args[0] {
        RuntimeValue::Object(m) => match m.get("id_val") {
            Some(RuntimeValue::Float(n)) => *n as u64,
            _ => return RuntimeValue::Array(vec![]),
        },
        _ => return RuntimeValue::Array(vec![]),
    };
    let text = match &args[1] {
        RuntimeValue::String(s) => s.clone(),
        _ => return RuntimeValue::Array(vec![]),
    };

    let cache = crate::runtime::stdlib_src::core::regex::REGEX_CACHE
        .lock()
        .unwrap();
    if let Some(re) = cache.get(&id) {
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
            let mut names = FastMap::default();
            for name in re.capture_names().flatten() {
                if let Some(m) = caps.name(name) {
                    names.insert(
                        name.to_string(),
                        make_some(make_match_obj(m.as_str().to_string(), m.start(), m.end())),
                    );
                } else {
                    names.insert(name.to_string(), make_none());
                }
            }
            captures.push(make_captures_obj(groups, names));
        }
        RuntimeValue::Array(captures)
    } else {
        RuntimeValue::Array(vec![])
    }
}

pub fn runtime_regex_replace(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 3 {
        return RuntimeValue::Null;
    }
    let id = match &args[0] {
        RuntimeValue::Object(m) => match m.get("id_val") {
            Some(RuntimeValue::Float(n)) => *n as u64,
            _ => return RuntimeValue::Null,
        },
        _ => return RuntimeValue::Null,
    };
    let text = match &args[1] {
        RuntimeValue::String(s) => s.clone(),
        _ => return RuntimeValue::Null,
    };
    let rep = match &args[2] {
        RuntimeValue::String(s) => s.clone(),
        _ => return RuntimeValue::Null,
    };

    let cache = crate::runtime::stdlib_src::core::regex::REGEX_CACHE
        .lock()
        .unwrap();
    if let Some(re) = cache.get(&id) {
        RuntimeValue::String(re.replace(&text, rep.as_str()).to_string())
    } else {
        RuntimeValue::Null
    }
}

pub fn runtime_regex_replace_all(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 3 {
        return RuntimeValue::Null;
    }
    let id = match &args[0] {
        RuntimeValue::Object(m) => match m.get("id_val") {
            Some(RuntimeValue::Float(n)) => *n as u64,
            _ => return RuntimeValue::Null,
        },
        _ => return RuntimeValue::Null,
    };
    let text = match &args[1] {
        RuntimeValue::String(s) => s.clone(),
        _ => return RuntimeValue::Null,
    };
    let rep = match &args[2] {
        RuntimeValue::String(s) => s.clone(),
        _ => return RuntimeValue::Null,
    };

    let cache = crate::runtime::stdlib_src::core::regex::REGEX_CACHE
        .lock()
        .unwrap();
    if let Some(re) = cache.get(&id) {
        RuntimeValue::String(re.replace_all(&text, rep.as_str()).to_string())
    } else {
        RuntimeValue::Null
    }
}

pub fn runtime_regex_split(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Array(vec![]);
    }
    let id = match &args[0] {
        RuntimeValue::Object(m) => match m.get("id_val") {
            Some(RuntimeValue::Float(n)) => *n as u64,
            _ => return RuntimeValue::Array(vec![]),
        },
        _ => return RuntimeValue::Array(vec![]),
    };
    let text = match &args[1] {
        RuntimeValue::String(s) => s.clone(),
        _ => return RuntimeValue::Array(vec![]),
    };

    let cache = crate::runtime::stdlib_src::core::regex::REGEX_CACHE
        .lock()
        .unwrap();
    if let Some(re) = cache.get(&id) {
        let parts: Vec<RuntimeValue> = re
            .split(&text)
            .map(|s| RuntimeValue::String(s.to_string()))
            .collect();
        RuntimeValue::Array(parts)
    } else {
        RuntimeValue::Array(vec![])
    }
}

pub fn runtime_regex_split_n(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 3 {
        return RuntimeValue::Array(vec![]);
    }
    let id = match &args[0] {
        RuntimeValue::Object(m) => match m.get("id_val") {
            Some(RuntimeValue::Float(n)) => *n as u64,
            _ => return RuntimeValue::Array(vec![]),
        },
        _ => return RuntimeValue::Array(vec![]),
    };
    let text = match &args[1] {
        RuntimeValue::String(s) => s.clone(),
        _ => return RuntimeValue::Array(vec![]),
    };
    let limit = args[2].as_int().unwrap_or(0) as usize;

    let cache = crate::runtime::stdlib_src::core::regex::REGEX_CACHE
        .lock()
        .unwrap();
    if let Some(re) = cache.get(&id) {
        let parts: Vec<RuntimeValue> = re
            .splitn(&text, limit)
            .map(|s| RuntimeValue::String(s.to_string()))
            .collect();
        RuntimeValue::Array(parts)
    } else {
        RuntimeValue::Array(vec![])
    }
}

pub fn runtime_regex_flag_ignore_case(_args: &[RuntimeValue]) -> RuntimeValue {
    RuntimeValue::Float(1.0)
}
pub fn runtime_regex_flag_multiline(_args: &[RuntimeValue]) -> RuntimeValue {
    RuntimeValue::Float(2.0)
}
pub fn runtime_regex_flag_dot_all(_args: &[RuntimeValue]) -> RuntimeValue {
    RuntimeValue::Float(4.0)
}
pub fn runtime_regex_flag_extended(_args: &[RuntimeValue]) -> RuntimeValue {
    RuntimeValue::Float(8.0)
}
pub fn runtime_regex_flag_unicode(_args: &[RuntimeValue]) -> RuntimeValue {
    RuntimeValue::Float(16.0)
}
