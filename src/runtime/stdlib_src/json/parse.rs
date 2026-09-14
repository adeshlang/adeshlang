//! JSON Parse
//!
//! `json_parse(str)` parses a JSON string into a runtime `Value`, leveraging
//! `serde_json` and runtime conversions for objects/arrays.
use crate::parsing::ast::{BuiltinEnv, Value};
use crate::stdlib::registry::BuiltinRegistry;

pub fn register(registry: &mut BuiltinRegistry) {
    registry.register(
        "json_parse",
        "json",
        "Parse JSON string into value",
        builtin_json_parse,
    );
}

fn builtin_json_parse(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 1 {
        return Err("json_parse expects 1 arg".to_string());
    }
    let s = match &args[0] {
        Value::Str(x) => x.clone(),
        _ => return Err("json_parse expects a string".to_string()),
    };
    let v: serde_json::Value =
        serde_json::from_str(&s).map_err(|e| format!("invalid JSON: {}", e))?;
    Ok(crate::execution::runtime::json_to_value(&v))
}
