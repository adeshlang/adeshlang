//! String Builtin Methods
//!
//! This module contains all built-in string manipulation methods for the language.
//! Includes operations for string splitting, substring extraction, searching,
//! case conversion, trimming, and more.

use super::super::super::format::fmt;
use super::super::super::interpreter::err;
use crate::parsing::ast::Value;

/// Implements all string builtin methods
pub fn call_string_method(obj: &Value, method_name: &str, args: &[Value]) -> Result<Value, String> {
    let mut method_args = vec![obj.clone()];
    method_args.extend_from_slice(args);

    match method_name {
        "length" | "len" => {
            let s = match obj {
                Value::Str(s) => s,
                _ => return Err(err("length requires string".to_string())),
            };
            Ok(Value::Number(s.chars().count() as f64))
        }
        "isEmpty" | "is_empty" => {
            let s = match obj {
                Value::Str(s) => s,
                _ => return Err(err("isEmpty requires string".to_string())),
            };
            Ok(Value::Bool(s.is_empty()))
        }
        "trimStart" => {
            let s = match obj {
                Value::Str(s) => s,
                _ => return Err(err("trimStart requires string".to_string())),
            };
            Ok(Value::Str(s.trim_start().to_string()))
        }
        "trimEnd" => {
            let s = match obj {
                Value::Str(s) => s,
                _ => return Err(err("trimEnd requires string".to_string())),
            };
            Ok(Value::Str(s.trim_end().to_string()))
        }
        "lines" => {
            let s = match obj {
                Value::Str(s) => s,
                _ => return Err(err("lines requires string".to_string())),
            };
            Ok(Value::Array(
                s.lines().map(|v| Value::Str(v.to_string())).collect(),
            ))
        }
        "chars" => {
            let s = match obj {
                Value::Str(s) => s,
                _ => return Err(err("chars requires string".to_string())),
            };
            Ok(Value::Array(s.chars().map(Value::Char).collect()))
        }
        "reverse" => {
            let s = match obj {
                Value::Str(s) => s,
                _ => return Err(err("reverse requires string".to_string())),
            };
            Ok(Value::Str(s.chars().rev().collect()))
        }
        "capitalize" => {
            let s = match obj {
                Value::Str(s) => s,
                _ => return Err(err("capitalize requires string".to_string())),
            };
            let mut chars = s.chars();
            Ok(Value::Str(
                chars
                    .next()
                    .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
                    .unwrap_or_default(),
            ))
        }
        "isAscii" | "is_ascii" => {
            let s = match obj {
                Value::Str(s) => s,
                _ => return Err(err("isAscii requires string".to_string())),
            };
            Ok(Value::Bool(s.is_ascii()))
        }
        "isNumeric" | "is_numeric" => {
            let s = match obj {
                Value::Str(s) => s,
                _ => return Err(err("isNumeric requires string".to_string())),
            };
            Ok(Value::Bool(
                !s.is_empty() && s.chars().all(|c| c.is_numeric()),
            ))
        }
        "isAlphabetic" | "is_alphabetic" => {
            let s = match obj {
                Value::Str(s) => s,
                _ => return Err(err("isAlphabetic requires string".to_string())),
            };
            Ok(Value::Bool(
                !s.is_empty() && s.chars().all(|c| c.is_alphabetic()),
            ))
        }
        "isAlphanumeric" | "is_alphanumeric" => {
            let s = match obj {
                Value::Str(s) => s,
                _ => return Err(err("isAlphanumeric requires string".to_string())),
            };
            Ok(Value::Bool(
                !s.is_empty() && s.chars().all(|c| c.is_alphanumeric()),
            ))
        }
        "padStart" | "padEnd" => {
            let s = match obj {
                Value::Str(s) => s,
                _ => return Err(err("padding requires string".to_string())),
            };
            let width = args
                .first()
                .and_then(Value::as_f64)
                .ok_or_else(|| err("padding width must be numeric".to_string()))?
                as usize;
            let fill = match args.get(1) {
                Some(Value::Str(v)) if !v.is_empty() => v,
                _ => " ",
            };
            let current = s.chars().count();
            let count = width.saturating_sub(current);
            let padding: String = fill.chars().cycle().take(count).collect();
            if method_name == "padStart" {
                Ok(Value::Str(format!("{}{}", padding, s)))
            } else {
                Ok(Value::Str(format!("{}{}", s, padding)))
            }
        }
        "split" => {
            if method_args.len() < 2 {
                return Err(err("split(delimiter)".to_string()));
            }
            let s = match &method_args[0] {
                Value::Str(s) => s.clone(),
                _ => return Err(err("split requires string".to_string())),
            };
            let delim = match &method_args[1] {
                Value::Str(d) => d.clone(),
                _ => return Err(err("split delimiter must be string".to_string())),
            };

            let parts: Vec<Value> = s.split(&delim).map(|p| Value::Str(p.to_string())).collect();
            Ok(Value::Array(parts))
        }
        "substring" | "substr" => {
            if method_args.len() < 2 {
                return Err(err("substring(start[, end])".to_string()));
            }
            let s = match &method_args[0] {
                Value::Str(s) => s.clone(),
                _ => return Err(err("substring requires string".to_string())),
            };
            let start = match &method_args[1] {
                Value::Number(n) => *n as usize,
                _ => return Err(err("substring indices must be numbers".to_string())),
            };
            let end = if method_args.len() > 2 {
                match &method_args[2] {
                    Value::Number(n) => Some(*n as usize),
                    _ => None,
                }
            } else {
                None
            };

            let chars: Vec<char> = s.chars().collect();
            let end = end.unwrap_or(chars.len());
            let substring: String = chars
                .iter()
                .skip(start)
                .take(end.saturating_sub(start))
                .collect();
            Ok(Value::Str(substring))
        }
        "charAt" => {
            if method_args.len() < 2 {
                return Err(err("charAt(index)".to_string()));
            }
            let s = match &method_args[0] {
                Value::Str(s) => s.clone(),
                _ => return Err(err("charAt requires string".to_string())),
            };
            let idx = match &method_args[1] {
                Value::Number(n) => *n as usize,
                Value::I32(n) => *n as usize,
                Value::I64(n) => *n as usize,
                Value::U32(n) => *n as usize,
                Value::U64(n) => *n as usize,
                Value::I8(n) => *n as usize,
                Value::U8(n) => *n as usize,
                Value::I16(n) => *n as usize,
                Value::U16(n) => *n as usize,
                _ => return Err(err("charAt index must be number".to_string())),
            };

            if let Some(ch) = s.chars().nth(idx) {
                Ok(Value::Str(ch.to_string()))
            } else {
                Ok(Value::Str(String::new()))
            }
        }
        "indexOf" => {
            if method_args.len() < 2 {
                return Err(err("indexOf(searchString)".to_string()));
            }
            let s = match &method_args[0] {
                Value::Str(s) => s.clone(),
                _ => return Err(err("indexOf requires string".to_string())),
            };
            let search = match &method_args[1] {
                Value::Str(search) => search.clone(),
                _ => return Err(err("indexOf search must be string".to_string())),
            };

            match s.find(&search) {
                Some(pos) => Ok(Value::Number(pos as f64)),
                None => Ok(Value::Number(-1.0)),
            }
        }
        "lastIndexOf" => {
            if method_args.len() < 2 {
                return Err(err("lastIndexOf(searchString)".to_string()));
            }
            let s = match &method_args[0] {
                Value::Str(s) => s.clone(),
                _ => return Err(err("lastIndexOf requires string".to_string())),
            };
            let search = match &method_args[1] {
                Value::Str(search) => search.clone(),
                _ => return Err(err("lastIndexOf search must be string".to_string())),
            };

            match s.rfind(&search) {
                Some(pos) => Ok(Value::Number(pos as f64)),
                None => Ok(Value::Number(-1.0)),
            }
        }
        "includes" | "contains" => {
            if method_args.len() < 2 {
                return Err(err("includes(searchString)".to_string()));
            }
            let s = match &method_args[0] {
                Value::Str(s) => s.clone(),
                _ => return Err(err("includes requires string".to_string())),
            };
            let search = match &method_args[1] {
                Value::Str(search) => search.clone(),
                _ => return Err(err("includes search must be string".to_string())),
            };

            Ok(Value::Bool(s.contains(&search)))
        }
        "startsWith" => {
            if method_args.len() < 2 {
                return Err(err("startsWith(searchString)".to_string()));
            }
            let s = match &method_args[0] {
                Value::Str(s) => s.clone(),
                _ => return Err(err("startsWith requires string".to_string())),
            };
            let search = match &method_args[1] {
                Value::Str(search) => search.clone(),
                _ => return Err(err("startsWith search must be string".to_string())),
            };

            Ok(Value::Bool(s.starts_with(&search)))
        }
        "endsWith" => {
            if method_args.len() < 2 {
                return Err(err("endsWith(searchString)".to_string()));
            }
            let s = match &method_args[0] {
                Value::Str(s) => s.clone(),
                _ => return Err(err("endsWith requires string".to_string())),
            };
            let search = match &method_args[1] {
                Value::Str(search) => search.clone(),
                _ => return Err(err("endsWith search must be string".to_string())),
            };

            Ok(Value::Bool(s.ends_with(&search)))
        }
        "trim" => {
            let s = match &method_args[0] {
                Value::Str(s) => s.clone(),
                _ => return Err(err("trim requires string".to_string())),
            };
            Ok(Value::Str(s.trim().to_string()))
        }
        "toLowerCase" => {
            let s = match &method_args[0] {
                Value::Str(s) => s.clone(),
                _ => return Err(err("toLowerCase requires string".to_string())),
            };
            Ok(Value::Str(s.to_lowercase()))
        }
        "toUpperCase" => {
            let s = match &method_args[0] {
                Value::Str(s) => s.clone(),
                _ => return Err(err("toUpperCase requires string".to_string())),
            };
            Ok(Value::Str(s.to_uppercase()))
        }
        "replace" => {
            if method_args.len() < 3 {
                return Err(err("replace(search, replacement)".to_string()));
            }
            let s = match &method_args[0] {
                Value::Str(s) => s.clone(),
                _ => return Err(err("replace requires string".to_string())),
            };
            let search = match &method_args[1] {
                Value::Str(search) => search.clone(),
                _ => return Err(err("replace search must be string".to_string())),
            };
            let replacement = match &method_args[2] {
                Value::Str(r) => r.clone(),
                _ => return Err(err("replace replacement must be string".to_string())),
            };

            Ok(Value::Str(s.replacen(&search, &replacement, 1)))
        }
        "repeat" => {
            if method_args.len() < 2 {
                return Err(err("repeat(count)".to_string()));
            }
            let s = match &method_args[0] {
                Value::Str(s) => s.clone(),
                _ => return Err(err("repeat requires string".to_string())),
            };
            let count = match &method_args[1] {
                Value::Number(n) => *n as usize,
                _ => return Err(err("repeat count must be number".to_string())),
            };

            Ok(Value::Str(s.repeat(count)))
        }
        "slice" => {
            if method_args.len() < 2 {
                return Err(err("slice(start[, end])".to_string()));
            }
            let s = match &method_args[0] {
                Value::Str(s) => s.clone(),
                _ => return Err(err("slice requires string".to_string())),
            };
            let start = match &method_args[1] {
                Value::Number(n) => *n as i32,
                Value::I32(n) => *n as i32,
                Value::I64(n) => *n as i32,
                Value::U32(n) => *n as i32,
                Value::U64(n) => *n as i32,
                Value::I8(n) => *n as i32,
                Value::U8(n) => *n as i32,
                Value::I16(n) => *n as i32,
                Value::U16(n) => *n as i32,
                _ => return Err(err("slice start must be number".to_string())),
            };
            let end = if method_args.len() > 2 {
                match &method_args[2] {
                    Value::Number(n) => Some(*n as i32),
                    Value::I32(n) => Some(*n as i32),
                    Value::I64(n) => Some(*n as i32),
                    Value::U32(n) => Some(*n as i32),
                    Value::U64(n) => Some(*n as i32),
                    Value::I8(n) => Some(*n as i32),
                    Value::U8(n) => Some(*n as i32),
                    Value::I16(n) => Some(*n as i32),
                    Value::U16(n) => Some(*n as i32),
                    _ => None,
                }
            } else {
                None
            };

            let chars: Vec<char> = s.chars().collect();
            let len = chars.len() as i32;
            let start_idx = (if start < 0 {
                (len + start).max(0)
            } else {
                start.min(len)
            }) as usize;
            let end_idx = if let Some(e) = end {
                (if e < 0 { (len + e).max(0) } else { e.min(len) }) as usize
            } else {
                chars.len()
            };

            let sliced: String = chars
                .iter()
                .skip(start_idx)
                .take(end_idx.saturating_sub(start_idx))
                .collect();
            Ok(Value::Str(sliced))
        }
        "join" => {
            if method_args.len() < 2 {
                return Ok(Value::Str(fmt(obj)));
            }
            let sep = match &method_args[1] {
                Value::Str(s) => s.clone(),
                _ => return Err(err("join separator must be string".to_string())),
            };
            Ok(Value::Str(format!("{}{}", fmt(obj), sep)))
        }
        _ => Err(err(format!("Unknown string method: '{}'", method_name))),
    }
}
