use crate::parsing::ast::{BuiltinEnv, Value};
use crate::stdlib::registry::BuiltinRegistry;
use std::path::Path;

fn ensure_arity(actual: usize, expected: usize, sig: &str) -> Result<(), String> {
    if actual != expected {
        Err(format!(
            "{} expected {} arguments, got {}",
            sig, expected, actual
        ))
    } else {
        Ok(())
    }
}

fn expect_str(v: &Value, name: &str) -> Result<String, String> {
    match v {
        Value::Str(s) => Ok(s.clone()),
        Value::Instance(inst) => {
            if let Ok(fields) = inst.fields.read() {
                if let Some(val) = fields.get("_raw").or_else(|| fields.get("_path")) {
                    if let Value::Str(s) = val {
                        return Ok(s.clone());
                    }
                }
            }
            Ok(inst.class_name.clone())
        }
        Value::Object(map) => {
            if let Some(val) = map.get("_raw").or_else(|| map.get("_path")) {
                if let Value::Str(s) = val {
                    return Ok(s.clone());
                }
            }
            Err(format!("{} must be a string or Path", name))
        }
        _ => Err(format!("{} must be a string", name)),
    }
}

fn builtin_path_sep(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 0, "path_sep()")?;
    let sep = std::path::MAIN_SEPARATOR.to_string();
    Ok(Value::Str(sep))
}

fn builtin_path_is_windows(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 0, "path_is_windows()")?;
    Ok(Value::Bool(cfg!(windows)))
}

fn builtin_path_current(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 0, "path_current()")?;
    let cwd =
        std::env::current_dir().map_err(|e| format!("failed to get current directory: {}", e))?;
    Ok(Value::Str(cwd.to_string_lossy().to_string()))
}

fn builtin_path_home(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 0, "path_home()")?;
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| {
            if cfg!(windows) {
                r"C:\Users\Default".to_string()
            } else {
                "/home/user".to_string()
            }
        });
    Ok(Value::Str(home))
}

fn builtin_path_temp(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 0, "path_temp()")?;
    let temp = std::env::temp_dir();
    Ok(Value::Str(temp.to_string_lossy().to_string()))
}

fn builtin_path_executable(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 0, "path_executable()")?;
    let exe =
        std::env::current_exe().map_err(|e| format!("failed to get executable path: {}", e))?;
    Ok(Value::Str(exe.to_string_lossy().to_string()))
}

fn builtin_path_canonicalize(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "path_canonicalize(path)")?;
    let p_str = expect_str(&args[0], "path")?;
    let p = Path::new(&p_str);
    let can = p
        .canonicalize()
        .map_err(|e| format!("failed to canonicalize '{}': {}", p_str, e))?;
    Ok(Value::Str(can.to_string_lossy().to_string()))
}

pub fn register_all(registry: &mut BuiltinRegistry) {
    registry.register(
        "path_sep",
        "path",
        "Get main path separator",
        builtin_path_sep,
    );
    registry.register(
        "path_is_windows",
        "path",
        "Check if OS is Windows",
        builtin_path_is_windows,
    );
    registry.register(
        "path_current",
        "path",
        "Get current working directory",
        builtin_path_current,
    );
    registry.register(
        "path_home",
        "path",
        "Get user home directory",
        builtin_path_home,
    );
    registry.register(
        "path_temp",
        "path",
        "Get system temporary directory",
        builtin_path_temp,
    );
    registry.register(
        "path_executable",
        "path",
        "Get current executable path",
        builtin_path_executable,
    );
    registry.register(
        "path_canonicalize",
        "path",
        "Canonicalize path using filesystem",
        builtin_path_canonicalize,
    );
}
