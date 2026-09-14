//! Set Builtin Methods
//!
//! This module contains all built-in set manipulation methods for the language.
//! Includes operations for union, intersection, add, delete, and membership testing.

use super::super::super::interpreter::err;
use super::super::super::ops::equals;
use crate::parsing::ast::Value;

/// Implements all set builtin methods
pub fn call_set_method(obj: &Value, method_name: &str, args: &[Value]) -> Result<Value, String> {
    match obj {
        Value::Set(s) => match method_name {
            "length" | "len" | "size" => Ok(Value::Number(s.len() as f64)),
            "isEmpty" | "is_empty" => Ok(Value::Bool(s.is_empty())),
            "union" => {
                if args.len() != 1 {
                    return Err(err("union(set)"));
                }
                let other_vals = match &args[0] {
                    Value::Set(o) => o.clone(),
                    Value::Array(o) => o.clone(),
                    _ => return Err(err("union expects set or array")),
                };
                let mut result = s.clone();
                for v in &other_vals {
                    if !result.iter().any(|x| equals(x, v)) {
                        result.push(v.clone());
                    }
                }
                Ok(Value::Set(result))
            }
            "intersection" => {
                if args.len() != 1 {
                    return Err(err("intersection(set)"));
                }
                let other_vals = match &args[0] {
                    Value::Set(o) => o.clone(),
                    Value::Array(o) => o.clone(),
                    _ => return Err(err("intersection expects set or array")),
                };
                let result: Vec<Value> = s
                    .iter()
                    .filter(|v| other_vals.iter().any(|x| equals(x, v)))
                    .cloned()
                    .collect();
                Ok(Value::Set(result))
            }
            "add" | "insert" => {
                if args.len() != 1 {
                    return Err(err("insert(value)"));
                }
                let mut result = s.clone();
                if !result.iter().any(|x| equals(x, &args[0])) {
                    result.push(args[0].clone());
                }
                Ok(Value::Set(result))
            }
            "contains" | "has" => {
                if args.len() != 1 {
                    return Err(err("contains(value)"));
                }
                Ok(Value::Bool(s.iter().any(|x| equals(x, &args[0]))))
            }
            "delete" => {
                if args.len() != 1 {
                    return Err(err("delete(value)"));
                }
                let result: Vec<Value> =
                    s.iter().filter(|v| !equals(v, &args[0])).cloned().collect();
                Ok(Value::Set(result))
            }
            "remove" => {
                if args.len() != 1 { return Err(err("remove(value)")); }
                let result: Vec<Value> = s.iter().filter(|v| !equals(v, &args[0])).cloned().collect();
                Ok(Value::Set(result))
            }
            "clear" => Ok(Value::Set(Vec::new())),
            "difference" => {
                if args.len() != 1 { return Err(err("difference(set)")); }
                let other = match &args[0] { Value::Set(v) | Value::Array(v) => v, _ => return Err(err("difference expects set or array")) };
                Ok(Value::Set(s.iter().filter(|v| !other.iter().any(|x| equals(x, v))).cloned().collect()))
            }
            "symmetricDifference" | "symmetric_difference" => {
                if args.len() != 1 { return Err(err("symmetricDifference(set)")); }
                let other = match &args[0] { Value::Set(v) | Value::Array(v) => v, _ => return Err(err("symmetricDifference expects set or array")) };
                let mut result = s.iter().filter(|v| !other.iter().any(|x| equals(x, v))).cloned().collect::<Vec<_>>();
                result.extend(other.iter().filter(|v| !s.iter().any(|x| equals(x, v))).cloned());
                Ok(Value::Set(result))
            }
            "isSubsetOf" | "is_subset_of" => {
                let other = match args.first() { Some(Value::Set(v)) | Some(Value::Array(v)) => v, _ => return Err(err("isSubsetOf(set)")) };
                Ok(Value::Bool(s.iter().all(|v| other.iter().any(|x| equals(x, v)))))
            }
            "isSupersetOf" | "is_superset_of" => {
                let other = match args.first() { Some(Value::Set(v)) | Some(Value::Array(v)) => v, _ => return Err(err("isSupersetOf(set)")) };
                Ok(Value::Bool(other.iter().all(|v| s.iter().any(|x| equals(x, v)))))
            }
            "toArray" => Ok(Value::Array(s.clone())),
            _ => Err(err(format!("Unknown set method: '{}'", method_name))),
        },
        Value::Object(_) => super::associated::call_object_method(obj, method_name, args),
        _ => Err(err(format!("'{}' not a set", method_name))),
    }
}
