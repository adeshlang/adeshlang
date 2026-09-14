//! System Args and Env
//!
//! Exposes helpers for working with program arguments and environment variables:
//! - `args`, `execName`, `argsCount`, `argsSlice`, `argsJoin`, `argsIndexOf`
//! - `parseArgs`, `argGet`, `argHas` for flag parsing
//! - `env*` for process and runtime environment access and `.env` files
use crate::parsing::ast::{BuiltinEnv, Value};
use crate::stdlib::registry::BuiltinRegistry;
use rustc_hash::FxHashMap as HashMap;

pub fn register(registry: &mut BuiltinRegistry) {
    registry.register("execName", "system", "Executable name", builtin_exec_name);
    registry.register("args", "system", "Raw argv as array", builtin_args);
    registry.register(
        "parseArgs",
        "system",
        "Parse argv into flags and positionals",
        builtin_parse_args,
    );
    registry.register(
        "argGet",
        "system",
        "Get flag value with default",
        builtin_arg_get,
    );
    registry.register("argHas", "system", "Check if flag present", builtin_arg_has);
    registry.register("env", "system", "Get environment variable", builtin_env);
    registry.register(
        "envGet",
        "system",
        "Get environment variable with default",
        builtin_env_get,
    );
    registry.register(
        "envSet",
        "system",
        "Set environment variable",
        builtin_env_set,
    );
    registry.register(
        "envRemove",
        "system",
        "Remove environment variable",
        builtin_env_remove,
    );
    registry.register(
        "envClear",
        "system",
        "Clear runtime environment variables",
        builtin_env_clear,
    );
    registry.register(
        "envSetCurrentDir",
        "system",
        "Set current working directory",
        builtin_env_set_current_dir,
    );
    registry.register(
        "envUserDir",
        "system",
        "Get platform user directory",
        builtin_env_user_dir,
    );
    registry.register(
        "envPlatformInfo",
        "system",
        "Get platform and OS metadata",
        builtin_env_platform_info,
    );
    registry.register(
        "envHas",
        "system",
        "Check environment variable presence",
        builtin_env_has,
    );
    registry.register(
        "envAll",
        "system",
        "Get all environment variables",
        builtin_env_all,
    );
    registry.register("argsCount", "system", "Argument count", builtin_args_count);
    registry.register(
        "argsSlice",
        "system",
        "Slice argv from index",
        builtin_args_slice,
    );
    registry.register(
        "argsIndexOf",
        "system",
        "Find index of arg",
        builtin_args_index_of,
    );
    registry.register(
        "argsJoin",
        "system",
        "Join argv with separator",
        builtin_args_join,
    );
    registry.register(
        "envFromFile",
        "system",
        "Read .env file to object",
        builtin_env_from_file,
    );
    registry.register(
        "envFileGet",
        "system",
        "Get key from .env file",
        builtin_env_file_get,
    );
    registry.register(
        "envRuntimeGet",
        "system",
        "Get runtime env variable",
        builtin_env_runtime_get,
    );
    registry.register(
        "envRuntimeHas",
        "system",
        "Check runtime env presence",
        builtin_env_runtime_has,
    );
    registry.register(
        "envRuntimeAll",
        "system",
        "Get runtime env object",
        builtin_env_runtime_all,
    );
    registry.register(
        "envRuntimeLoad",
        "system",
        "Load runtime env from object or file",
        builtin_env_runtime_load,
    );
    registry.register(
        "envSystemInfo",
        "system",
        "Comprehensive system information",
        builtin_env_system_info,
    );
    registry.register(
        "envCount",
        "system",
        "Count environment variables",
        builtin_env_count,
    );
    registry.register(
        "envFilter",
        "system",
        "Filter environment variables by prefix",
        builtin_env_filter,
    );
    registry.register(
        "envGetMany",
        "system",
        "Get multiple environment variables",
        builtin_env_get_many,
    );
    registry.register(
        "envSetIfAbsent",
        "system",
        "Set environment variable only if absent",
        builtin_env_set_if_absent,
    );
    registry.register(
        "envCommandLine",
        "system",
        "Full command line string",
        builtin_env_command_line,
    );
    registry.register(
        "envUserInfo",
        "system",
        "Current user details",
        builtin_env_user_info,
    );
    registry.register(
        "envFromFile",
        "system",
        "Read .env file to object",
        builtin_env_from_file,
    );
    registry.register(
        "envFileGet",
        "system",
        "Get key from .env file",
        builtin_env_file_get,
    );
    // Omit envLoad to avoid mutating process environment from scripts
}

fn builtin_exec_name(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let s = crate::execution::runtime::get_program_name();
    Ok(Value::Str(s))
}

fn builtin_args(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let arr = crate::execution::runtime::get_program_args();
    let vals: Vec<Value> = arr.into_iter().map(Value::Str).collect();
    Ok(Value::Array(vals))
}

fn builtin_parse_args(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let src = crate::execution::runtime::get_program_args();
    let mut flags: HashMap<String, Value> = HashMap::default();
    let mut positionals: Vec<Value> = Vec::new();
    let mut i = 0usize;
    while i < src.len() {
        let s = &src[i];
        if s.starts_with("--") {
            let rest = &s[2..];
            if let Some(eq) = rest.find('=') {
                flags.insert(
                    rest[..eq].to_string(),
                    Value::Str(rest[eq + 1..].to_string()),
                );
            } else {
                if i + 1 < src.len() && !src[i + 1].starts_with('-') {
                    flags.insert(rest.to_string(), Value::Str(src[i + 1].clone()));
                    i += 1;
                } else {
                    flags.insert(rest.to_string(), Value::Bool(true));
                }
            }
        } else if s.starts_with('-') && s.len() > 1 {
            let rest = &s[1..];
            if rest.len() > 1 && rest.chars().all(|c: char| c.is_ascii_alphabetic()) {
                for ch in rest.chars() {
                    let key: String = ch.to_string();
                    flags.insert(key, Value::Bool(true));
                }
            } else {
                let ch: char = rest.chars().next().unwrap();
                let tail: String = rest.chars().skip(1).collect();
                if !tail.is_empty() {
                    let key: String = ch.to_string();
                    flags.insert(key, Value::Str(tail));
                } else if i + 1 < src.len() && !src[i + 1].starts_with('-') {
                    let key: String = ch.to_string();
                    flags.insert(key, Value::Str(src[i + 1].clone()));
                    i += 1;
                } else {
                    let key: String = ch.to_string();
                    flags.insert(key, Value::Bool(true));
                }
            }
        } else {
            positionals.push(Value::Str(s.clone()));
        }
        i += 1;
    }
    let mut out: HashMap<String, Value> = HashMap::default();
    out.insert("flags".into(), Value::Object(std::sync::Arc::new(flags)));
    out.insert("positionals".into(), Value::Array(positionals));
    Ok(Value::Object(std::sync::Arc::new(out)))
}

fn builtin_arg_get(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("argGet(name, default?)".to_string());
    }
    let name = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err("name must be string".to_string()),
    };
    let src = crate::execution::runtime::get_program_args();
    let mut flags: HashMap<String, Value> = HashMap::default();
    let mut i = 0usize;
    while i < src.len() {
        let s = &src[i];
        if s.starts_with("--") {
            let rest = &s[2..];
            if let Some(eq) = rest.find('=') {
                flags.insert(
                    rest[..eq].to_string(),
                    Value::Str(rest[eq + 1..].to_string()),
                );
            } else {
                if i + 1 < src.len() && !src[i + 1].starts_with('-') {
                    flags.insert(rest.to_string(), Value::Str(src[i + 1].clone()));
                    i += 1;
                } else {
                    flags.insert(rest.to_string(), Value::Bool(true));
                }
            }
        } else if s.starts_with('-') && s.len() > 1 {
            let rest = &s[1..];
            if rest.len() > 1 && rest.chars().all(|c: char| c.is_ascii_alphabetic()) {
                for ch in rest.chars() {
                    let key: String = ch.to_string();
                    flags.insert(key, Value::Bool(true));
                }
            } else {
                let ch: char = rest.chars().next().unwrap();
                let tail: String = rest.chars().skip(1).collect();
                if !tail.is_empty() {
                    let key: String = ch.to_string();
                    flags.insert(key, Value::Str(tail));
                } else if i + 1 < src.len() && !src[i + 1].starts_with('-') {
                    let key: String = ch.to_string();
                    flags.insert(key, Value::Str(src[i + 1].clone()));
                    i += 1;
                } else {
                    let key: String = ch.to_string();
                    flags.insert(key, Value::Bool(true));
                }
            }
        }
        i += 1;
    }
    if let Some(v) = flags.get(&name) {
        Ok(v.clone())
    } else {
        Ok(args.get(1).cloned().unwrap_or(Value::Null))
    }
}

fn builtin_arg_has(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("argHas(name)".to_string());
    }
    let name = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err("name must be string".to_string()),
    };
    let src = crate::execution::runtime::get_program_args();
    let mut found = false;
    let mut i = 0usize;
    while i < src.len() {
        let s = &src[i];
        if s == &format!("--{}", name)
            || s.starts_with(&format!("--{}=", name))
            || s == &format!("-{}", name)
        {
            found = true;
            break;
        }
        // Handle grouped short flags like -abc: check if char 'a' is in the group
        if s.starts_with('-') && !s.starts_with("--") && s.len() > 2 && name.len() == 1 {
            if s[1..].contains(name.as_str()) {
                found = true;
                break;
            }
        }
        i += 1;
    }
    Ok(Value::Bool(found))
}

fn builtin_env(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 1 {
        return Err("env(name)".to_string());
    }
    let key = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err("name must be string".to_string()),
    };
    if let Some(v) = crate::execution::runtime::runtime_env_get(&key) {
        return Ok(Value::Str(v));
    }
    match std::env::var(&key) {
        Ok(v) => Ok(Value::Str(v)),
        Err(_) => Ok(Value::Null),
    }
}

fn builtin_env_get(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 1 {
        return Err("envGet(name, default?)".to_string());
    }
    let key = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err("name must be string".to_string()),
    };
    match std::env::var(&key) {
        Ok(v) => Ok(Value::Str(v)),
        Err(_) => Ok(args.get(1).cloned().unwrap_or(Value::Null)),
    }
}

fn builtin_env_has(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("envHas(name)".to_string());
    }
    let key = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err("name must be string".to_string()),
    };
    Ok(Value::Bool(
        crate::execution::runtime::runtime_env_has(&key) || std::env::var(&key).is_ok(),
    ))
}

fn builtin_env_all(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let mut m: HashMap<String, Value> = HashMap::default();
    for (k, v) in crate::execution::runtime::runtime_env_all() {
        m.insert(k, Value::Str(v));
    }
    for (k, v) in std::env::vars() {
        if !m.contains_key(&k) {
            m.insert(k, Value::Str(v));
        }
    }
    Ok(Value::Object(std::sync::Arc::new(m)))
}

fn builtin_args_count(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let src = crate::execution::runtime::get_program_args();
    Ok(Value::Number(src.len() as f64))
}

fn builtin_args_slice(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("argsSlice(startIndex)".to_string());
    }
    let start = match &args[0] {
        Value::Number(n) => (*n).max(0.0) as usize,
        _ => return Err("startIndex must be number".to_string()),
    };
    let src = crate::execution::runtime::get_program_args();
    let vals: Vec<Value> = src.into_iter().skip(start).map(Value::Str).collect();
    Ok(Value::Array(vals))
}

fn builtin_args_index_of(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("argsIndexOf(value)".to_string());
    }
    let val = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err("value must be string".to_string()),
    };
    let src = crate::execution::runtime::get_program_args();
    for (i, s) in src.iter().enumerate() {
        if s == &val {
            return Ok(Value::Number(i as f64));
        }
    }
    Ok(Value::Number(-1.0))
}

fn builtin_args_join(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let sep = if args.is_empty() {
        " ".to_string()
    } else {
        match &args[0] {
            Value::Str(s) => s.clone(),
            _ => return Err("separator must be string".to_string()),
        }
    };
    let src = crate::execution::runtime::get_program_args();
    Ok(Value::Str(src.join(&sep)))
}

fn builtin_env_runtime_get(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("envRuntimeGet(name)".to_string());
    }
    let key = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err("name must be string".to_string()),
    };
    if let Some(v) = crate::execution::runtime::runtime_env_get(&key) {
        Ok(Value::Str(v))
    } else {
        Ok(Value::Null)
    }
}

fn builtin_env_runtime_has(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("envRuntimeHas(name)".to_string());
    }
    let key = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err("name must be string".to_string()),
    };
    Ok(Value::Bool(crate::execution::runtime::runtime_env_has(
        &key,
    )))
}

fn builtin_env_runtime_all(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let mut m: HashMap<String, Value> = HashMap::default();
    for (k, v) in crate::execution::runtime::runtime_env_all() {
        m.insert(k, Value::Str(v));
    }
    Ok(Value::Object(std::sync::Arc::new(m)))
}

fn builtin_env_runtime_load(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("envRuntimeLoad(objectOrPath, overwrite?)".to_string());
    }
    let overwrite = if args.len() >= 2 {
        match &args[1] {
            Value::Bool(b) => *b,
            _ => return Err("overwrite must be boolean".to_string()),
        }
    } else {
        false
    };
    match &args[0] {
        Value::Object(o) => {
            let mut map = HashMap::default();
            for (k, v) in o.iter() {
                if let Value::Str(s) = v {
                    map.insert(k.clone(), s.clone());
                }
            }
            let n = crate::execution::runtime::runtime_env_load(map, overwrite);
            Ok(Value::Number(n as f64))
        }
        Value::Str(path) => {
            let m = read_dotenv(path)?;
            let mut map = HashMap::default();
            for (k, v) in m.into_iter() {
                if let Value::Str(s) = v {
                    map.insert(k, s);
                }
            }
            let n = crate::execution::runtime::runtime_env_load(map, overwrite);
            Ok(Value::Number(n as f64))
        }
        _ => Err("envRuntimeLoad expects object or string path".to_string()),
    }
}

fn read_dotenv(path: &str) -> Result<HashMap<String, Value>, String> {
    let s =
        std::fs::read_to_string(path).map_err(|_| format!("failed to read .env at {}", path))?;
    let mut out: HashMap<String, Value> = HashMap::default();
    for raw_line in s.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = if line.starts_with("export ") {
            &line[7..]
        } else {
            line
        };
        if let Some(eq) = line.find('=') {
            let key = line[..eq].trim();
            if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                continue;
            }
            let mut val = line[eq + 1..].trim().to_string();
            if val.starts_with('"') && val.ends_with('"') && val.len() >= 2 {
                val = val[1..val.len() - 1].to_string();
                val = val
                    .replace("\\n", "\n")
                    .replace("\\t", "\t")
                    .replace("\\r", "\r");
            } else if val.starts_with('\'') && val.ends_with('\'') && val.len() >= 2 {
                val = val[1..val.len() - 1].to_string();
            } else {
                if let Some(hash) = val.find('#') {
                    let before = &val[..hash];
                    if before.ends_with(' ') {
                        val = before.trim_end().to_string();
                    }
                }
            }
            out.insert(key.to_string(), Value::Str(val));
        }
    }
    Ok(out)
}

fn builtin_env_from_file(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let path = if args.is_empty() {
        ".env".to_string()
    } else {
        match &args[0] {
            Value::Str(s) => s.clone(),
            _ => return Err("path must be string".to_string()),
        }
    };
    let m = read_dotenv(&path)?;
    Ok(Value::Object(std::sync::Arc::new(m)))
}

fn builtin_env_file_get(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 1 {
        return Err("envFileGet(key, default?, path?)".to_string());
    }
    let key = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err("key must be string".to_string()),
    };
    let default = args.get(1).cloned().unwrap_or(Value::Null);
    let path = if args.len() >= 3 {
        match &args[2] {
            Value::Str(s) => s.clone(),
            _ => return Err("path must be string".to_string()),
        }
    } else {
        ".env".to_string()
    };
    let m = read_dotenv(&path)?;
    Ok(m.get(&key).cloned().unwrap_or(default))
}

fn builtin_env_set(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("envSet(key, value)".to_string());
    }
    let key = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err("key must be string".to_string()),
    };
    let val = match &args[1] {
        Value::Str(s) => s.clone(),
        other => format!("{:?}", other),
    };
    crate::execution::runtime::runtime_env_set(&key, &val);
    Ok(Value::Null)
}

fn builtin_env_remove(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 1 {
        return Err("envRemove(key)".to_string());
    }
    let key = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err("key must be string".to_string()),
    };
    crate::execution::runtime::runtime_env_remove(&key);
    Ok(Value::Null)
}

fn builtin_env_clear(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    crate::execution::runtime::runtime_env_clear();
    Ok(Value::Null)
}

fn builtin_env_set_current_dir(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.len() < 1 {
        return Err("envSetCurrentDir(path)".to_string());
    }
    let path_str = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => return Err("path must be string".to_string()),
    };
    match std::env::set_current_dir(&path_str) {
        Ok(_) => Ok(Value::Bool(true)),
        Err(_) => Ok(Value::Bool(false)),
    }
}

fn builtin_env_user_dir(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let kind = if !args.is_empty() {
        match &args[0] {
            Value::Str(s) => s.to_lowercase(),
            _ => "".to_string(),
        }
    } else {
        "".to_string()
    };

    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| {
            if cfg!(windows) {
                r"C:\Users\Default".to_string()
            } else {
                "/home/user".to_string()
            }
        });

    let path = match kind.as_str() {
        "home" => home.clone(),
        "desktop" => format!("{}/Desktop", home),
        "documents" => format!("{}/Documents", home),
        "downloads" => format!("{}/Downloads", home),
        "pictures" => format!("{}/Pictures", home),
        "music" => format!("{}/Music", home),
        "videos" => format!("{}/Videos", home),
        "public" => {
            if cfg!(windows) {
                r"C:\Users\Public".to_string()
            } else {
                "/home/public".to_string()
            }
        }
        "temp" => std::env::temp_dir().to_string_lossy().to_string(),
        "exe" => std::env::current_exe()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        "exedir" => std::env::current_exe()
            .map(|p| {
                p.parent()
                    .map(|d| d.to_string_lossy().to_string())
                    .unwrap_or_default()
            })
            .unwrap_or_default(),
        _ => home,
    };
    Ok(Value::Str(path))
}

fn builtin_env_platform_info(
    _env: &mut dyn BuiltinEnv,
    _args: Vec<Value>,
) -> Result<Value, String> {
    let mut map = HashMap::default();

    let os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_arch = "wasm32") {
        "wasm"
    } else {
        std::env::consts::OS
    };

    let arch = std::env::consts::ARCH;

    let platform = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_arch = "wasm32") {
        "wasm"
    } else {
        std::env::consts::OS
    };

    let hostname = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "localhost".to_string());

    let username = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "user".to_string());

    let user_id = std::env::var("USERID")
        .or_else(|_| std::env::var("UID"))
        .unwrap_or_else(|_| "1000".to_string());

    let pid = std::process::id() as f64;

    let shell = std::env::var("COMSPEC")
        .or_else(|_| std::env::var("SHELL"))
        .unwrap_or_else(|_| {
            if cfg!(windows) {
                "cmd.exe".to_string()
            } else {
                "/bin/sh".to_string()
            }
        });

    map.insert("os".to_string(), Value::Str(os.to_string()));
    map.insert("arch".to_string(), Value::Str(arch.to_string()));
    map.insert("platform".to_string(), Value::Str(platform.to_string()));
    map.insert("hostname".to_string(), Value::Str(hostname));
    map.insert("username".to_string(), Value::Str(username));
    map.insert("userId".to_string(), Value::Str(user_id));
    map.insert("pid".to_string(), Value::Number(pid));
    map.insert("shell".to_string(), Value::Str(shell));

    Ok(Value::Object(std::sync::Arc::new(map)))
}

// no envLoad to keep environment immutable from scripts

// ============================================================================
// Extended system information helpers
// ============================================================================

fn stdlib_system_os_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_arch = "wasm32") {
        "wasm"
    } else {
        std::env::consts::OS
    }
}

fn stdlib_system_os_type() -> String {
    match stdlib_system_os_name() {
        "windows" => "Windows_NT".to_string(),
        "macos" => "Darwin".to_string(),
        "linux" => "Linux".to_string(),
        "wasm" => "Wasm".to_string(),
        other => other.to_uppercase(),
    }
}

fn stdlib_system_os_version() -> String {
    #[cfg(target_os = "windows")]
    {
        unsafe {
            use windows_sys::Win32::System::SystemInformation::{GetVersionExW, OSVERSIONINFOW};
            let mut info: OSVERSIONINFOW = std::mem::zeroed();
            info.dwOSVersionInfoSize = std::mem::size_of::<OSVERSIONINFOW>() as u32;
            if GetVersionExW(&mut info) != 0 {
                return format!(
                    "{}.{}.{}",
                    info.dwMajorVersion, info.dwMinorVersion, info.dwBuildNumber
                );
            }
        }
    }
    #[cfg(unix)]
    {
        unsafe {
            let mut uts: libc::utsname = std::mem::zeroed();
            if libc::uname(&mut uts) == 0 {
                let release = std::ffi::CStr::from_ptr(uts.release.as_ptr())
                    .to_string_lossy()
                    .to_string();
                if !release.is_empty() {
                    return release;
                }
            }
        }
    }
    String::new()
}

fn stdlib_system_os_family() -> String {
    match stdlib_system_os_name() {
        "windows" => "windows".to_string(),
        "wasm" => "wasm".to_string(),
        _ => "unix".to_string(),
    }
}

fn stdlib_system_memory() -> (u64, u64) {
    #[cfg(target_os = "windows")]
    {
        unsafe {
            use windows_sys::Win32::System::SystemInformation::{
                GlobalMemoryStatusEx, MEMORYSTATUSEX,
            };
            let mut ms: MEMORYSTATUSEX = std::mem::zeroed();
            ms.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
            if GlobalMemoryStatusEx(&mut ms) != 0 {
                return (ms.ullTotalPhys, ms.ullAvailPhys);
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        let parse_kb = |s: &str| -> u64 {
            s.split_whitespace()
                .next()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0)
        };
        if let Ok(contents) = std::fs::read_to_string("/proc/meminfo") {
            let mut total = 0u64;
            let mut avail = 0u64;
            for line in contents.lines() {
                if let Some(rest) = line.strip_prefix("MemTotal:") {
                    total = parse_kb(rest) * 1024;
                } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
                    avail = parse_kb(rest) * 1024;
                }
            }
            if total > 0 {
                return (total, avail);
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        unsafe {
            let mut total: u64 = 0;
            let mut size: libc::size_t = std::mem::size_of::<u64>();
            if libc::sysctlbyname(
                b"hw.memsize\0".as_ptr() as *const libc::c_char,
                &mut total as *mut u64 as *mut libc::c_void,
                &mut size,
                std::ptr::null_mut(),
                0,
            ) == 0
            {
                return (total, 0);
            }
        }
    }
    (0, 0)
}

static STDLIB_PROCESS_START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

fn stdlib_process_start_time() -> std::time::Instant {
    *STDLIB_PROCESS_START.get_or_init(|| std::time::Instant::now())
}

fn stdlib_system_uptime_secs() -> u64 {
    #[cfg(target_os = "windows")]
    {
        unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount64() / 1000 }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(s) = std::fs::read_to_string("/proc/uptime") {
            if let Some(first) = s.split_whitespace().next() {
                if let Ok(secs) = first.parse::<f64>() {
                    return secs as u64;
                }
            }
        }
        0
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        0
    }
}

fn stdlib_system_load_average() -> Vec<Value> {
    #[cfg(all(unix, not(target_os = "android")))]
    {
        let mut loads = [0.0f64; 3];
        let n = unsafe { libc::getloadavg(loads.as_mut_ptr(), 3) };
        if n > 0 {
            return loads
                .iter()
                .take(n as usize)
                .map(|&v| Value::Number(v))
                .collect();
        }
    }
    vec![Value::Number(0.0), Value::Number(0.0), Value::Number(0.0)]
}

fn stdlib_system_page_size() -> u64 {
    #[cfg(unix)]
    {
        let sz = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        if sz > 0 {
            return sz as u64;
        }
    }
    4096
}

fn stdlib_system_parent_pid() -> u32 {
    #[cfg(unix)]
    {
        unsafe { libc::getppid() as u32 }
    }
    #[cfg(not(unix))]
    {
        0
    }
}

fn stdlib_system_process_title() -> String {
    #[cfg(target_os = "linux")]
    {
        if let Ok(comm) = std::fs::read_to_string("/proc/self/comm") {
            let comm = comm.trim().to_string();
            if !comm.is_empty() {
                return comm;
            }
        }
    }
    let name = crate::execution::runtime::get_program_name();
    if !name.is_empty() {
        return name;
    }
    std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
        .unwrap_or_default()
}

fn stdlib_system_machine_id() -> String {
    #[cfg(target_os = "linux")]
    {
        for p in ["/etc/machine-id", "/var/lib/dbus/machine-id"] {
            if let Ok(s) = std::fs::read_to_string(p) {
                let s = s.trim().to_string();
                if !s.is_empty() {
                    return s;
                }
            }
        }
    }
    let hostname = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "localhost".to_string());
    let username = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "user".to_string());
    let mut h = 0x811c9dc5u64;
    for b in format!("{}:{}", hostname, username).bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x01000193);
    }
    format!("{:016x}", h)
}

fn stdlib_system_user_cache_dir() -> String {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    #[cfg(target_os = "windows")]
    {
        std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
            if home.is_empty() {
                r"C:\Users\Default\AppData\Local".to_string()
            } else {
                format!(r"{}\AppData\Local", home)
            }
        })
    }
    #[cfg(target_os = "macos")]
    {
        if home.is_empty() {
            "/tmp".to_string()
        } else {
            format!("{}/Library/Caches", home)
        }
    }
    #[cfg(target_os = "linux")]
    {
        std::env::var("XDG_CACHE_HOME").unwrap_or_else(|_| {
            if home.is_empty() {
                "/tmp".to_string()
            } else {
                format!("{}/.cache", home)
            }
        })
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        home
    }
}

fn stdlib_system_user_config_dir() -> String {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA").unwrap_or_else(|_| {
            if home.is_empty() {
                r"C:\Users\Default\AppData\Roaming".to_string()
            } else {
                format!(r"{}\AppData\Roaming", home)
            }
        })
    }
    #[cfg(target_os = "macos")]
    {
        if home.is_empty() {
            home
        } else {
            format!("{}/Library/Application Support", home)
        }
    }
    #[cfg(target_os = "linux")]
    {
        std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| {
            if home.is_empty() {
                home
            } else {
                format!("{}/.config", home)
            }
        })
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        home
    }
}

fn stdlib_system_command_line() -> String {
    let name = crate::execution::runtime::get_program_name();
    let args = crate::execution::runtime::get_program_args();
    let mut parts = Vec::with_capacity(args.len() + 1);
    if !name.is_empty() {
        parts.push(name);
    }
    parts.extend(args);
    parts.join(" ")
}

fn builtin_env_system_info(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let os = stdlib_system_os_name();
    let os_type = stdlib_system_os_type();
    let os_version = stdlib_system_os_version();
    let os_family = stdlib_system_os_family();
    let arch = std::env::consts::ARCH;
    let platform = os;

    let hostname = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "localhost".to_string());
    let username = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "user".to_string());
    let user_id = std::env::var("USERID")
        .or_else(|_| std::env::var("UID"))
        .unwrap_or_else(|_| "1000".to_string());
    let user_gid = std::env::var("GID").unwrap_or_else(|_| "1000".to_string());
    let shell = std::env::var("COMSPEC")
        .or_else(|_| std::env::var("SHELL"))
        .unwrap_or_else(|_| {
            if cfg!(windows) {
                "cmd.exe".to_string()
            } else {
                "/bin/sh".to_string()
            }
        });

    let (total_memory, free_memory) = stdlib_system_memory();
    let process_uptime = stdlib_process_start_time().elapsed().as_secs();
    let system_uptime = stdlib_system_uptime_secs();
    let endianness = if cfg!(target_endian = "big") {
        "BE"
    } else {
        "LE"
    };
    let pagesize = stdlib_system_page_size();
    let ppid = stdlib_system_parent_pid();

    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    let current_dir = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let current_exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let temp_dir = std::env::temp_dir().to_string_lossy().to_string();

    let mut map: HashMap<String, Value> = HashMap::default();
    map.insert("os".to_string(), Value::Str(os.to_string()));
    map.insert("osType".to_string(), Value::Str(os_type));
    map.insert("osVersion".to_string(), Value::Str(os_version.clone()));
    map.insert("osRelease".to_string(), Value::Str(os_version));
    map.insert("osFamily".to_string(), Value::Str(os_family));
    map.insert(
        "osDescription".to_string(),
        Value::Str(format!(
            "{} {}",
            stdlib_system_os_type(),
            stdlib_system_os_version()
        )),
    );
    map.insert("arch".to_string(), Value::Str(arch.to_string()));
    map.insert("platform".to_string(), Value::Str(platform.to_string()));
    map.insert("hostname".to_string(), Value::Str(hostname));
    map.insert("username".to_string(), Value::Str(username));
    map.insert("userId".to_string(), Value::Str(user_id));
    map.insert("userGid".to_string(), Value::Str(user_gid));
    map.insert("pid".to_string(), Value::Number(std::process::id() as f64));
    map.insert("ppid".to_string(), Value::Number(ppid as f64));
    map.insert("shell".to_string(), Value::Str(shell));
    map.insert(
        "cpuCount".to_string(),
        Value::Number(crate::runtime::thread::logical_cpu_count() as f64),
    );
    map.insert(
        "totalMemory".to_string(),
        Value::Number(total_memory as f64),
    );
    map.insert("freeMemory".to_string(), Value::Number(free_memory as f64));
    map.insert(
        "processUptime".to_string(),
        Value::Number(process_uptime as f64),
    );
    map.insert(
        "systemUptime".to_string(),
        Value::Number(system_uptime as f64),
    );
    map.insert(
        "loadavg".to_string(),
        Value::Array(stdlib_system_load_average()),
    );
    map.insert("endianness".to_string(), Value::Str(endianness.to_string()));
    map.insert("pagesize".to_string(), Value::Number(pagesize as f64));
    map.insert(
        "processTitle".to_string(),
        Value::Str(stdlib_system_process_title()),
    );
    map.insert(
        "machineId".to_string(),
        Value::Str(stdlib_system_machine_id()),
    );
    map.insert("userHome".to_string(), Value::Str(home));
    map.insert(
        "userCache".to_string(),
        Value::Str(stdlib_system_user_cache_dir()),
    );
    map.insert(
        "userConfig".to_string(),
        Value::Str(stdlib_system_user_config_dir()),
    );
    map.insert("tempDir".to_string(), Value::Str(temp_dir));
    map.insert("currentDir".to_string(), Value::Str(current_dir));
    map.insert("currentExe".to_string(), Value::Str(current_exe));

    Ok(Value::Object(std::sync::Arc::new(map)))
}

fn builtin_env_count(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let mut count = crate::execution::runtime::runtime_env_all().len();
    let mut seen: HashMap<String, ()> = HashMap::default();
    for (k, _) in crate::execution::runtime::runtime_env_all() {
        seen.insert(k, ());
    }
    for (k, _) in std::env::vars() {
        if !seen.contains_key(&k) {
            count += 1;
        }
    }
    Ok(Value::Number(count as f64))
}

fn builtin_env_filter(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let prefix = match args.first() {
        Some(Value::Str(s)) => s.clone(),
        _ => return Ok(Value::Object(std::sync::Arc::new(HashMap::default()))),
    };
    let mut m: HashMap<String, Value> = HashMap::default();
    for (k, v) in crate::execution::runtime::runtime_env_all() {
        if k.starts_with(&prefix) {
            m.insert(k, Value::Str(v));
        }
    }
    for (k, v) in std::env::vars() {
        if k.starts_with(&prefix) && !m.contains_key(&k) {
            m.insert(k, Value::Str(v));
        }
    }
    Ok(Value::Object(std::sync::Arc::new(m)))
}

fn builtin_env_get_many(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let mut m: HashMap<String, Value> = HashMap::default();
    let keys: Vec<Value> = match args.first() {
        Some(Value::Array(arr)) => arr.clone(),
        Some(Value::DynArray(da)) => da.data.clone(),
        _ => return Ok(Value::Object(std::sync::Arc::new(m))),
    };
    for k in keys {
        if let Value::Str(key) = k {
            if let Some(v) = crate::execution::runtime::runtime_env_get(&key) {
                m.insert(key, Value::Str(v));
            } else if let Ok(v) = std::env::var(&key) {
                m.insert(key, Value::Str(v));
            }
        }
    }
    Ok(Value::Object(std::sync::Arc::new(m)))
}

fn builtin_env_set_if_absent(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let key = match args.first() {
        Some(Value::Str(s)) => s.clone(),
        _ => return Ok(Value::Bool(false)),
    };
    let val = match args.get(1) {
        Some(Value::Str(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(other) => format!("{:?}", other),
        None => "".to_string(),
    };
    if crate::execution::runtime::runtime_env_has(&key) || std::env::var(&key).is_ok() {
        return Ok(Value::Bool(false));
    }
    crate::execution::runtime::runtime_env_set(&key, &val);
    Ok(Value::Bool(true))
}

fn builtin_env_command_line(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    Ok(Value::Str(stdlib_system_command_line()))
}

fn builtin_env_user_info(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let mut map: HashMap<String, Value> = HashMap::default();
    let username = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "user".to_string());
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    let shell = std::env::var("COMSPEC")
        .or_else(|_| std::env::var("SHELL"))
        .unwrap_or_else(|_| {
            if cfg!(windows) {
                "cmd.exe".to_string()
            } else {
                "/bin/sh".to_string()
            }
        });
    map.insert("username".to_string(), Value::Str(username));
    map.insert(
        "userId".to_string(),
        Value::Str(
            std::env::var("USERID")
                .or_else(|_| std::env::var("UID"))
                .unwrap_or_else(|_| "1000".to_string()),
        ),
    );
    map.insert(
        "userGid".to_string(),
        Value::Str(std::env::var("GID").unwrap_or_else(|_| "1000".to_string())),
    );
    map.insert("home".to_string(), Value::Str(home));
    map.insert("shell".to_string(), Value::Str(shell));
    Ok(Value::Object(std::sync::Arc::new(map)))
}
