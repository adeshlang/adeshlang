//! JSON Stringify
//!
//! `json_stringify(value)` converts runtime values into a JSON string using
//! the formatter's `value_to_json` conversion.
use crate::parsing::ast::{BuiltinEnv, Value};
use crate::stdlib::registry::BuiltinRegistry;

pub fn register(registry: &mut BuiltinRegistry) {
    registry.register(
        "json_stringify",
        "json",
        "Stringify value to JSON",
        builtin_json_stringify,
    );
}

fn builtin_json_stringify(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 1 {
        return Err("json_stringify expects 1 arg".to_string());
    }
    let v = crate::execution::runtime::format::value_to_json(&args[0]);
    Ok(Value::Str(v.to_string()))
}
