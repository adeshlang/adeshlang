//! File Utilities
//!
//! Provides `read_file(path: string)` to load file contents into a `Value::Str`.
//! Errors include the OS message for easier diagnostics.
use crate::parsing::ast::{BuiltinEnv, Value};
use crate::stdlib::registry::BuiltinRegistry;

pub fn register(registry: &mut BuiltinRegistry) {
    registry.register(
        "read_file",
        "io",
        "Read file contents as string",
        builtin_read_file,
    );
}

fn builtin_read_file(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 1 {
        return Err("read_file expects 1 arg".to_string());
    }
    let path = match &args[0] {
        Value::Str(x) => x.clone(),
        _ => return Err("read_file expects a string path".to_string()),
    };
    let s = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {}", path, e))?;
    Ok(Value::Str(s))
}
