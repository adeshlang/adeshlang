//! Environment and command-line argument builtins
//!
//! This module provides runtime functions for accessing environment variables
//! and command-line arguments. It includes:
//!
//! # Command-line Arguments
//! - `argc()` - Get argument count
//! - `argv()` - Get all arguments as array
//! - `arg(index)` - Get specific argument by index
//! - `execName()` - Get executable/script name
//! - `argsCount()` - Get argument count (alias for argc)
//! - `argsSlice(start)` - Get arguments from start index
//! - `argsJoin(sep)` - Join arguments with separator
//! - `argsIndexOf(value)` - Find index of argument
//! - `parseArgs()` - Parse arguments into flags and positionals
//! - `argGet(name, default?)` - Get flag value with optional default
//! - `argHas(name)` - Check if flag is present
//!
//! # Environment Variables
//! - `env(name)` - Get environment variable
//! - `envGet(name, default?)` - Get environment variable with default
//! - `envHas(name)` - Check if environment variable exists
//! - `envAll()` - Get all environment variables
//! - `envFromFile(path?)` - Read .env file to object
//! - `envFileGet(key, default?, path?)` - Get key from .env file
//! - `envRuntimeGet(name)` - Get runtime environment variable
//! - `envRuntimeHas(name)` - Check runtime environment variable presence
//! - `envRuntimeAll()` - Get all runtime environment variables
//! - `envRuntimeLoad(objectOrPath, overwrite?)` - Load runtime environment from object or file

use super::RuntimeValue;
use crate::utils::collections::FastMap;

// ============================================================================
// Command-line arguments builtins
// ============================================================================

/// argc() - Returns the number of command-line arguments
pub(crate) fn runtime_argc(_args: &[RuntimeValue]) -> RuntimeValue {
    let args = crate::execution::runtime::get_program_args();

    RuntimeValue::Int((args.len() + 1) as i64)
}

/// argv() - Returns all command-line arguments as an array
pub(crate) fn runtime_argv(_args: &[RuntimeValue]) -> RuntimeValue {
    let args = crate::execution::runtime::get_program_args();
    let name = crate::execution::runtime::get_program_name();

    let mut clean_args = Vec::with_capacity(args.len() + 1);
    clean_args.push(RuntimeValue::String(name));
    clean_args.extend(args.into_iter().map(RuntimeValue::String));
    RuntimeValue::Array(clean_args)
}

/// arg(index) - Returns a specific argument by index
pub(crate) fn runtime_arg(args: &[RuntimeValue]) -> RuntimeValue {
    let idx = args.first().and_then(|v| v.as_int()).unwrap_or(0) as usize;
    if idx == 0 {
        return RuntimeValue::String(crate::execution::runtime::get_program_name());
    }
    let program_args = crate::execution::runtime::get_program_args();
    program_args
        .get(idx - 1)
        .map(|s: &String| RuntimeValue::String(s.clone()))
        .unwrap_or(RuntimeValue::Null)
}

/// execName() - Returns the executable/script name
pub(crate) fn runtime_exec_name(_args: &[RuntimeValue]) -> RuntimeValue {
    RuntimeValue::String(crate::execution::runtime::get_program_name())
}

/// argsCount() - Returns the number of arguments
pub(crate) fn runtime_args_count(_args: &[RuntimeValue]) -> RuntimeValue {
    let args = crate::execution::runtime::get_program_args();
    RuntimeValue::Int((args.len() + 1) as i64)
}

/// argsSlice(start) - Returns arguments from start index
pub(crate) fn runtime_args_slice(args: &[RuntimeValue]) -> RuntimeValue {
    let start = args.first().and_then(|v| v.as_int()).unwrap_or(0).max(0) as usize;
    let program_args = crate::execution::runtime::get_program_args();
    let name = crate::execution::runtime::get_program_name();

    let mut result = Vec::new();
    if start == 0 {
        result.push(RuntimeValue::String(name));
    }

    let skip_count = if start > 0 { start - 1 } else { 0 };
    result.extend(
        program_args
            .into_iter()
            .skip(skip_count)
            .map(RuntimeValue::String),
    );

    RuntimeValue::Array(result)
}

/// argsJoin(sep) - Joins arguments with separator
pub(crate) fn runtime_args_join(args: &[RuntimeValue]) -> RuntimeValue {
    let sep = args
        .first()
        .and_then(|v| match v {
            RuntimeValue::String(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_else(|| " ".to_string());
    let program_args = crate::execution::runtime::get_program_args();
    let name = crate::execution::runtime::get_program_name();

    let mut parts = Vec::with_capacity(program_args.len() + 1);
    parts.push(name);
    parts.extend(program_args);

    RuntimeValue::String(parts.join(&sep))
}

/// argsIndexOf(value) - Finds index of argument
pub(crate) fn runtime_args_index_of(args: &[RuntimeValue]) -> RuntimeValue {
    let val = match args.first() {
        Some(RuntimeValue::String(s)) => s.clone(),
        _ => return RuntimeValue::Int(-1),
    };

    let name = crate::execution::runtime::get_program_name();
    if val == name {
        return RuntimeValue::Int(0);
    }

    let program_args = crate::execution::runtime::get_program_args();
    for (i, s) in program_args.iter().enumerate() {
        if s == &val {
            return RuntimeValue::Int((i + 1) as i64);
        }
    }
    RuntimeValue::Int(-1)
}

/// parseArgs() - Parse arguments into flags and positionals
pub(crate) fn runtime_parse_args(_args: &[RuntimeValue]) -> RuntimeValue {
    let src = crate::execution::runtime::get_program_args();
    let mut flags: FastMap<String, RuntimeValue> = FastMap::default();
    let mut positionals: Vec<RuntimeValue> = Vec::new();
    let mut i = 0usize;

    while i < src.len() {
        let s = &src[i];
        if s.starts_with("--") {
            let rest = &s[2..];
            if let Some(eq) = rest.find('=') {
                flags.insert(
                    rest[..eq].to_string(),
                    RuntimeValue::String(rest[eq + 1..].to_string()),
                );
            } else if i + 1 < src.len() && !src[i + 1].starts_with('-') {
                flags.insert(rest.to_string(), RuntimeValue::String(src[i + 1].clone()));
                i += 1;
            } else {
                flags.insert(rest.to_string(), RuntimeValue::Bool(true));
            }
        } else if s.starts_with('-') && s.len() > 1 {
            let rest = &s[1..];
            if rest.len() > 1 && rest.chars().all(|c: char| c.is_ascii_alphabetic()) {
                for ch in rest.chars() {
                    let key: String = ch.to_string();
                    flags.insert(key, RuntimeValue::Bool(true));
                }
            } else {
                let ch: char = rest.chars().next().unwrap();
                let tail: String = rest.chars().skip(1).collect();
                if !tail.is_empty() {
                    let key: String = ch.to_string();
                    flags.insert(key, RuntimeValue::String(tail));
                } else if i + 1 < src.len() && !src[i + 1].starts_with('-') {
                    let key: String = ch.to_string();
                    flags.insert(key, RuntimeValue::String(src[i + 1].clone()));
                    i += 1;
                } else {
                    let key: String = ch.to_string();
                    flags.insert(key, RuntimeValue::Bool(true));
                }
            }
        } else {
            positionals.push(RuntimeValue::String(s.clone()));
        }
        i += 1;
    }

    let mut out: FastMap<String, RuntimeValue> = FastMap::default();
    out.insert("flags".into(), RuntimeValue::Object(flags));
    out.insert("positionals".into(), RuntimeValue::Array(positionals));
    RuntimeValue::Object(out)
}

/// argGet(name, default?) - Get flag value with optional default
pub(crate) fn runtime_arg_get(args: &[RuntimeValue]) -> RuntimeValue {
    let name = match args.first() {
        Some(RuntimeValue::String(s)) => s.clone(),
        _ => return RuntimeValue::Null,
    };
    let default_val = args.get(1).cloned().unwrap_or(RuntimeValue::Null);

    let src = crate::execution::runtime::get_program_args();
    let mut flags: FastMap<String, RuntimeValue> = FastMap::default();
    let mut i = 0usize;

    while i < src.len() {
        let s = &src[i];
        if s.starts_with("--") {
            let rest = &s[2..];
            if let Some(eq) = rest.find('=') {
                flags.insert(
                    rest[..eq].to_string(),
                    RuntimeValue::String(rest[eq + 1..].to_string()),
                );
            } else if i + 1 < src.len() && !src[i + 1].starts_with('-') {
                flags.insert(rest.to_string(), RuntimeValue::String(src[i + 1].clone()));
                i += 1;
            } else {
                flags.insert(rest.to_string(), RuntimeValue::Bool(true));
            }
        } else if s.starts_with('-') && s.len() > 1 {
            let rest = &s[1..];
            if rest.len() > 1 && rest.chars().all(|c: char| c.is_ascii_alphabetic()) {
                for ch in rest.chars() {
                    let key: String = ch.to_string();
                    flags.insert(key, RuntimeValue::Bool(true));
                }
            } else {
                let ch: char = rest.chars().next().unwrap();
                let tail: String = rest.chars().skip(1).collect();
                if !tail.is_empty() {
                    let key: String = ch.to_string();
                    flags.insert(key, RuntimeValue::String(tail));
                } else if i + 1 < src.len() && !src[i + 1].starts_with('-') {
                    let key: String = ch.to_string();
                    flags.insert(key, RuntimeValue::String(src[i + 1].clone()));
                    i += 1;
                } else {
                    let key: String = ch.to_string();
                    flags.insert(key, RuntimeValue::Bool(true));
                }
            }
        }
        i += 1;
    }

    flags.get(&name).cloned().unwrap_or(default_val)
}

/// argHas(name) - Check if flag is present
pub(crate) fn runtime_arg_has(args: &[RuntimeValue]) -> RuntimeValue {
    let name = match args.first() {
        Some(RuntimeValue::String(s)) => s.clone(),
        _ => return RuntimeValue::Bool(false),
    };
    let name_ref: &str = name.as_str();

    let src = crate::execution::runtime::get_program_args();
    for s in src.iter() {
        if s == &format!("--{}", name_ref)
            || s.starts_with(&format!("--{}=", name_ref))
            || s == &format!("-{}", name_ref)
        {
            return RuntimeValue::Bool(true);
        }
        // Handle grouped short flags like -abc
        if s.starts_with('-') && !s.starts_with("--") && s.len() > 2 {
            // Check if the flag character is in the grouped flags
            if name.len() == 1 && s[1..].contains(name_ref) {
                return RuntimeValue::Bool(true);
            }
        }
    }
    RuntimeValue::Bool(false)
}

// ============================================================================
// Environment variable builtins
// ============================================================================

/// env(name) - Get environment variable
pub(crate) fn runtime_env(args: &[RuntimeValue]) -> RuntimeValue {
    let key = match args.first() {
        Some(RuntimeValue::String(s)) => s.clone(),
        _ => return RuntimeValue::Null,
    };

    // Check runtime env first
    if let Some(v) = crate::execution::runtime::runtime_env_get(&key) {
        return RuntimeValue::String(v);
    }

    // Then check system environment
    match std::env::var(&key) {
        Ok(v) => RuntimeValue::String(v),
        Err(_) => RuntimeValue::Null,
    }
}

/// envGet(name, default?) - Get environment variable with default
pub(crate) fn runtime_env_get(args: &[RuntimeValue]) -> RuntimeValue {
    let key = match args.first() {
        Some(RuntimeValue::String(s)) => s.clone(),
        _ => return RuntimeValue::Null,
    };
    let default_val = args.get(1).cloned().unwrap_or(RuntimeValue::Null);

    if let Some(v) = crate::execution::runtime::runtime_env_get(&key) {
        return RuntimeValue::String(v);
    }

    match std::env::var(&key) {
        Ok(v) => RuntimeValue::String(v),
        Err(_) => default_val,
    }
}

/// envHas(name) - Check if environment variable exists
pub(crate) fn runtime_env_has(args: &[RuntimeValue]) -> RuntimeValue {
    let key = match args.first() {
        Some(RuntimeValue::String(s)) => s.clone(),
        _ => return RuntimeValue::Bool(false),
    };
    RuntimeValue::Bool(
        crate::execution::runtime::runtime_env_has(&key) || std::env::var(&key).is_ok(),
    )
}

/// envAll() - Get all environment variables
pub(crate) fn runtime_env_all(_args: &[RuntimeValue]) -> RuntimeValue {
    let mut m: FastMap<String, RuntimeValue> = FastMap::default();

    // Runtime env first
    for (k, v) in crate::execution::runtime::runtime_env_all() {
        m.insert(k, RuntimeValue::String(v));
    }

    // System env (don't overwrite runtime env)
    for (k, v) in std::env::vars() {
        if !m.contains_key(&k) {
            m.insert(k, RuntimeValue::String(v));
        }
    }

    RuntimeValue::Object(m)
}

/// Helper to read a .env file
pub(crate) fn read_dotenv_file(path: &str) -> Result<FastMap<String, RuntimeValue>, String> {
    let s =
        std::fs::read_to_string(path).map_err(|_| format!("failed to read .env at {}", path))?;
    let mut out: FastMap<String, RuntimeValue> = FastMap::default();

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
            } else if let Some(hash) = val.find('#') {
                let before = &val[..hash];
                if before.ends_with(' ') {
                    val = before.trim_end().to_string();
                }
            }
            out.insert(key.to_string(), RuntimeValue::String(val));
        }
    }
    Ok(out)
}

/// envFromFile(path?) - Read .env file to object
pub(crate) fn runtime_env_from_file(args: &[RuntimeValue]) -> RuntimeValue {
    let path = match args.first() {
        Some(RuntimeValue::String(s)) => s.clone(),
        _ => ".env".to_string(),
    };

    match read_dotenv_file(&path) {
        Ok(m) => RuntimeValue::Object(m),
        Err(_) => RuntimeValue::Null,
    }
}

/// envFileGet(key, default?, path?) - Get key from .env file
pub(crate) fn runtime_env_file_get(args: &[RuntimeValue]) -> RuntimeValue {
    let key = match args.first() {
        Some(RuntimeValue::String(s)) => s.clone(),
        _ => return RuntimeValue::Null,
    };
    let default_val = args.get(1).cloned().unwrap_or(RuntimeValue::Null);
    let path = match args.get(2) {
        Some(RuntimeValue::String(s)) => s.clone(),
        _ => ".env".to_string(),
    };

    match read_dotenv_file(&path) {
        Ok(m) => m.get(&key).cloned().unwrap_or(default_val),
        Err(_) => default_val,
    }
}

/// envRuntimeGet(name) - Get runtime env variable
pub(crate) fn runtime_env_runtime_get(args: &[RuntimeValue]) -> RuntimeValue {
    let key = match args.first() {
        Some(RuntimeValue::String(s)) => s.clone(),
        _ => return RuntimeValue::Null,
    };

    match crate::execution::runtime::runtime_env_get(&key) {
        Some(v) => RuntimeValue::String(v),
        None => RuntimeValue::Null,
    }
}

/// envRuntimeHas(name) - Check runtime env presence
pub(crate) fn runtime_env_runtime_has(args: &[RuntimeValue]) -> RuntimeValue {
    let key = match args.first() {
        Some(RuntimeValue::String(s)) => s.clone(),
        _ => return RuntimeValue::Bool(false),
    };
    RuntimeValue::Bool(crate::execution::runtime::runtime_env_has(&key))
}

/// envRuntimeAll() - Get runtime env object
pub(crate) fn runtime_env_runtime_all(_args: &[RuntimeValue]) -> RuntimeValue {
    let mut m: FastMap<String, RuntimeValue> = FastMap::default();
    for (k, v) in crate::execution::runtime::runtime_env_all() {
        m.insert(k, RuntimeValue::String(v));
    }
    RuntimeValue::Object(m)
}

/// envRuntimeLoad(objectOrPath, overwrite?) - Load runtime env from object or file
pub(crate) fn runtime_env_runtime_load(args: &[RuntimeValue]) -> RuntimeValue {
    let overwrite = match args.get(1) {
        Some(RuntimeValue::Bool(b)) => *b,
        _ => false,
    };

    match args.first() {
        Some(RuntimeValue::Object(obj)) => {
            let mut map = rustc_hash::FxHashMap::default();
            for (k, v) in obj.iter() {
                if let RuntimeValue::String(s) = v {
                    map.insert(k.clone(), s.clone());
                }
            }
            let n = crate::execution::runtime::runtime_env_load(map, overwrite);
            RuntimeValue::Int(n as i64)
        }
        Some(RuntimeValue::String(path)) => match read_dotenv_file(path) {
            Ok(m) => {
                let mut map = rustc_hash::FxHashMap::default();
                for (k, v) in m.into_iter() {
                    if let RuntimeValue::String(s) = v {
                        map.insert(k, s);
                    }
                }
                let n = crate::execution::runtime::runtime_env_load(map, overwrite);
                RuntimeValue::Int(n as i64)
            }
            Err(_) => RuntimeValue::Int(0),
        },
        _ => RuntimeValue::Int(0),
    }
}

/// envSet(name, val) - Set environment variable
pub(crate) fn runtime_env_set(args: &[RuntimeValue]) -> RuntimeValue {
    let key = match args.first() {
        Some(RuntimeValue::String(s)) => s.clone(),
        _ => return RuntimeValue::Null,
    };
    let val = match args.get(1) {
        Some(RuntimeValue::String(s)) => s.clone(),
        Some(RuntimeValue::Int(i)) => i.to_string(),
        Some(RuntimeValue::Float(f)) => f.to_string(),
        Some(RuntimeValue::Bool(b)) => b.to_string(),
        Some(v) => format!("{:?}", v),
        None => "".to_string(),
    };
    crate::execution::runtime::runtime_env_set(&key, &val);
    RuntimeValue::Null
}

/// envRemove(name) - Remove environment variable
pub(crate) fn runtime_env_remove(args: &[RuntimeValue]) -> RuntimeValue {
    let key = match args.first() {
        Some(RuntimeValue::String(s)) => s.clone(),
        _ => return RuntimeValue::Null,
    };
    crate::execution::runtime::runtime_env_remove(&key);
    RuntimeValue::Null
}

/// envClear() - Clear environment variables
pub(crate) fn runtime_env_clear(_args: &[RuntimeValue]) -> RuntimeValue {
    crate::execution::runtime::runtime_env_clear();
    RuntimeValue::Null
}

/// envSetCurrentDir(path) - Set current working directory
pub(crate) fn runtime_env_set_current_dir(args: &[RuntimeValue]) -> RuntimeValue {
    let path_str = match args.first() {
        Some(RuntimeValue::String(s)) => s.clone(),
        _ => return RuntimeValue::Bool(false),
    };
    match std::env::set_current_dir(&path_str) {
        Ok(_) => RuntimeValue::Bool(true),
        Err(_) => RuntimeValue::Bool(false),
    }
}

/// envUserDir(kind) - Get platform standard user directories
pub(crate) fn runtime_env_user_dir(args: &[RuntimeValue]) -> RuntimeValue {
    let kind = match args.first() {
        Some(RuntimeValue::String(s)) => s.to_lowercase(),
        _ => "".to_string(),
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
    RuntimeValue::String(path)
}

/// envPlatformInfo() - Introspect OS, architecture, hostname, user, pid, shell
pub(crate) fn runtime_env_platform_info(_args: &[RuntimeValue]) -> RuntimeValue {
    let mut map = FastMap::default();

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

    let pid = std::process::id() as i64;

    let shell = std::env::var("COMSPEC")
        .or_else(|_| std::env::var("SHELL"))
        .unwrap_or_else(|_| {
            if cfg!(windows) {
                "cmd.exe".to_string()
            } else {
                "/bin/sh".to_string()
            }
        });

    map.insert("os".to_string(), RuntimeValue::String(os.to_string()));
    map.insert("arch".to_string(), RuntimeValue::String(arch.to_string()));
    map.insert(
        "platform".to_string(),
        RuntimeValue::String(platform.to_string()),
    );
    map.insert("hostname".to_string(), RuntimeValue::String(hostname));
    map.insert("username".to_string(), RuntimeValue::String(username));
    map.insert("userId".to_string(), RuntimeValue::String(user_id));
    map.insert("pid".to_string(), RuntimeValue::Int(pid));
    map.insert("shell".to_string(), RuntimeValue::String(shell));

    RuntimeValue::Object(map)
}

// ============================================================================
// Extended system information helpers
// ============================================================================

/// OS identifier string ("windows", "linux", "macos", "wasm", ...).
fn system_os_name() -> &'static str {
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

/// Node.js `os.type()`-style OS type name ("Windows_NT", "Linux", "Darwin", ...).
fn system_os_type() -> String {
    match system_os_name() {
        "windows" => "Windows_NT".to_string(),
        "macos" => "Darwin".to_string(),
        "linux" => "Linux".to_string(),
        "wasm" => "Wasm".to_string(),
        other => other.to_uppercase(),
    }
}

/// OS version / release string (best effort across platforms).
fn system_os_version() -> String {
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
    #[cfg(not(any(target_os = "windows", unix)))]
    {
        // best-effort fallback
    }
    String::new()
}

/// OS family ("windows", "unix", "wasm").
fn system_os_family() -> String {
    match system_os_name() {
        "windows" => "windows".to_string(),
        "wasm" => "wasm".to_string(),
        _ => "unix".to_string(),
    }
}

/// Number of logical CPUs.
fn system_cpu_count() -> u32 {
    crate::runtime::thread::logical_cpu_count() as u32
}

/// Total and available physical memory in bytes.
fn system_memory() -> (u64, u64) {
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

/// Process start time for uptime measurements.
static PROCESS_START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

fn process_start_time() -> std::time::Instant {
    *PROCESS_START.get_or_init(|| std::time::Instant::now())
}

/// System uptime in seconds (best effort).
fn system_uptime_secs() -> u64 {
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

/// 1, 5 and 15 minute load averages where available.
fn system_load_average() -> Vec<RuntimeValue> {
    #[cfg(all(unix, not(target_os = "android")))]
    {
        let mut loads = [0.0f64; 3];
        let n = unsafe { libc::getloadavg(loads.as_mut_ptr(), 3) };
        if n > 0 {
            return loads
                .iter()
                .take(n as usize)
                .map(|&v| RuntimeValue::Float(v))
                .collect();
        }
    }
    vec![
        RuntimeValue::Float(0.0),
        RuntimeValue::Float(0.0),
        RuntimeValue::Float(0.0),
    ]
}

/// System memory page size in bytes.
fn system_page_size() -> u64 {
    #[cfg(unix)]
    {
        let sz = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        if sz > 0 {
            return sz as u64;
        }
    }
    4096
}

/// Parent process ID (0 when unavailable).
fn system_parent_pid() -> u32 {
    #[cfg(unix)]
    {
        unsafe { libc::getppid() as u32 }
    }
    #[cfg(not(unix))]
    {
        0
    }
}

/// Current process title.
fn system_process_title() -> String {
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

/// A stable machine identifier (best effort).
fn system_machine_id() -> String {
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

/// User cache directory.
fn system_user_cache_dir() -> String {
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

/// User configuration directory.
fn system_user_config_dir() -> String {
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

/// Full command line string (executable name followed by arguments).
fn system_command_line() -> String {
    let name = crate::execution::runtime::get_program_name();
    let args = crate::execution::runtime::get_program_args();
    let mut parts = Vec::with_capacity(args.len() + 1);
    if !name.is_empty() {
        parts.push(name);
    }
    parts.extend(args);
    parts.join(" ")
}

/// envSystemInfo() - Comprehensive system information object
pub(crate) fn runtime_env_system_info(_args: &[RuntimeValue]) -> RuntimeValue {
    let os = system_os_name();
    let os_type = system_os_type();
    let os_version = system_os_version();
    let os_family = system_os_family();
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

    let (total_memory, free_memory) = system_memory();
    let process_uptime = process_start_time().elapsed().as_secs();
    let system_uptime = system_uptime_secs();
    let endianness = if cfg!(target_endian = "big") {
        "BE"
    } else {
        "LE"
    };
    let pagesize = system_page_size();
    let ppid = system_parent_pid();

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

    let mut map: FastMap<String, RuntimeValue> = FastMap::default();
    map.insert("os".to_string(), RuntimeValue::String(os.to_string()));
    map.insert("osType".to_string(), RuntimeValue::String(os_type));
    map.insert(
        "osVersion".to_string(),
        RuntimeValue::String(os_version.clone()),
    );
    map.insert("osRelease".to_string(), RuntimeValue::String(os_version));
    map.insert("osFamily".to_string(), RuntimeValue::String(os_family));
    map.insert(
        "osDescription".to_string(),
        RuntimeValue::String(format!("{} {}", system_os_type(), system_os_version())),
    );
    map.insert("arch".to_string(), RuntimeValue::String(arch.to_string()));
    map.insert(
        "platform".to_string(),
        RuntimeValue::String(platform.to_string()),
    );
    map.insert("hostname".to_string(), RuntimeValue::String(hostname));
    map.insert("username".to_string(), RuntimeValue::String(username));
    map.insert("userId".to_string(), RuntimeValue::String(user_id));
    map.insert("userGid".to_string(), RuntimeValue::String(user_gid));
    map.insert(
        "pid".to_string(),
        RuntimeValue::Int(std::process::id() as i64),
    );
    map.insert("ppid".to_string(), RuntimeValue::Int(ppid as i64));
    map.insert("shell".to_string(), RuntimeValue::String(shell));
    map.insert(
        "cpuCount".to_string(),
        RuntimeValue::Int(system_cpu_count() as i64),
    );
    map.insert(
        "totalMemory".to_string(),
        RuntimeValue::Int(total_memory as i64),
    );
    map.insert(
        "freeMemory".to_string(),
        RuntimeValue::Int(free_memory as i64),
    );
    map.insert(
        "processUptime".to_string(),
        RuntimeValue::Int(process_uptime as i64),
    );
    map.insert(
        "systemUptime".to_string(),
        RuntimeValue::Int(system_uptime as i64),
    );
    map.insert(
        "loadavg".to_string(),
        RuntimeValue::Array(system_load_average()),
    );
    map.insert(
        "endianness".to_string(),
        RuntimeValue::String(endianness.to_string()),
    );
    map.insert("pagesize".to_string(), RuntimeValue::Int(pagesize as i64));
    map.insert(
        "processTitle".to_string(),
        RuntimeValue::String(system_process_title()),
    );
    map.insert(
        "machineId".to_string(),
        RuntimeValue::String(system_machine_id()),
    );
    map.insert("userHome".to_string(), RuntimeValue::String(home));
    map.insert(
        "userCache".to_string(),
        RuntimeValue::String(system_user_cache_dir()),
    );
    map.insert(
        "userConfig".to_string(),
        RuntimeValue::String(system_user_config_dir()),
    );
    map.insert("tempDir".to_string(), RuntimeValue::String(temp_dir));
    map.insert("currentDir".to_string(), RuntimeValue::String(current_dir));
    map.insert("currentExe".to_string(), RuntimeValue::String(current_exe));

    RuntimeValue::Object(map)
}

/// envCount() - Number of environment variables
pub(crate) fn runtime_env_count(_args: &[RuntimeValue]) -> RuntimeValue {
    let mut count = crate::execution::runtime::runtime_env_all().len();
    let mut seen: rustc_hash::FxHashSet<String> = rustc_hash::FxHashSet::default();
    for (k, _) in crate::execution::runtime::runtime_env_all() {
        seen.insert(k);
    }
    for (k, _) in std::env::vars() {
        if !seen.contains(&k) {
            count += 1;
        }
    }
    RuntimeValue::Int(count as i64)
}

/// envFilter(prefix) - All environment variables whose key starts with `prefix`
pub(crate) fn runtime_env_filter(args: &[RuntimeValue]) -> RuntimeValue {
    let prefix = match args.first() {
        Some(RuntimeValue::String(s)) => s.clone(),
        _ => return RuntimeValue::Object(FastMap::default()),
    };

    let mut m: FastMap<String, RuntimeValue> = FastMap::default();
    for (k, v) in crate::execution::runtime::runtime_env_all() {
        if k.starts_with(&prefix) {
            m.insert(k, RuntimeValue::String(v));
        }
    }
    for (k, v) in std::env::vars() {
        if k.starts_with(&prefix) && !m.contains_key(&k) {
            m.insert(k, RuntimeValue::String(v));
        }
    }
    RuntimeValue::Object(m)
}

/// envGetMany(keys) - Get multiple environment variables as an object
pub(crate) fn runtime_env_get_many(args: &[RuntimeValue]) -> RuntimeValue {
    let mut m: FastMap<String, RuntimeValue> = FastMap::default();
    let keys: Vec<RuntimeValue> = match args.first() {
        Some(RuntimeValue::Array(arr)) => arr.clone(),
        Some(RuntimeValue::DynArray { data, .. }) => data.clone(),
        Some(RuntimeValue::RawArray(_, arr)) => arr.clone(),
        _ => return RuntimeValue::Object(m),
    };
    for k in keys {
        if let RuntimeValue::String(key) = k {
            if let Some(v) = crate::execution::runtime::runtime_env_get(&key) {
                m.insert(key, RuntimeValue::String(v));
            } else if let Ok(v) = std::env::var(&key) {
                m.insert(key, RuntimeValue::String(v));
            }
        }
    }
    RuntimeValue::Object(m)
}

/// envSetIfAbsent(key, value) - Set an environment variable only if not already set
pub(crate) fn runtime_env_set_if_absent(args: &[RuntimeValue]) -> RuntimeValue {
    let key = match args.first() {
        Some(RuntimeValue::String(s)) => s.clone(),
        _ => return RuntimeValue::Bool(false),
    };
    let val = match args.get(1) {
        Some(RuntimeValue::String(s)) => s.clone(),
        Some(RuntimeValue::Int(i)) => i.to_string(),
        Some(RuntimeValue::Float(f)) => f.to_string(),
        Some(RuntimeValue::Bool(b)) => b.to_string(),
        Some(v) => format!("{:?}", v),
        None => "".to_string(),
    };

    if crate::execution::runtime::runtime_env_has(&key) || std::env::var(&key).is_ok() {
        return RuntimeValue::Bool(false);
    }
    crate::execution::runtime::runtime_env_set(&key, &val);
    RuntimeValue::Bool(true)
}

/// envCommandLine() - Full command line as a single string
pub(crate) fn runtime_env_command_line(_args: &[RuntimeValue]) -> RuntimeValue {
    RuntimeValue::String(system_command_line())
}

/// envUserInfo() - Details about the current user
pub(crate) fn runtime_env_user_info(_args: &[RuntimeValue]) -> RuntimeValue {
    let mut map: FastMap<String, RuntimeValue> = FastMap::default();
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
    map.insert("username".to_string(), RuntimeValue::String(username));
    map.insert(
        "userId".to_string(),
        RuntimeValue::String(
            std::env::var("USERID")
                .or_else(|_| std::env::var("UID"))
                .unwrap_or_else(|_| "1000".to_string()),
        ),
    );
    map.insert(
        "userGid".to_string(),
        RuntimeValue::String(std::env::var("GID").unwrap_or_else(|_| "1000".to_string())),
    );
    map.insert("home".to_string(), RuntimeValue::String(home));
    map.insert("shell".to_string(), RuntimeValue::String(shell));
    RuntimeValue::Object(map)
}
