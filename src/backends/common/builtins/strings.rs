//! String manipulation builtin functions
//!
//! This module provides runtime implementations for string operations including
//! concatenation, substring extraction, splitting, trimming, case conversion,
//! and various string search and manipulation functions.

use super::RuntimeValue;

/// Concatenate multiple values into a string
pub(crate) fn runtime_concat(args: &[RuntimeValue]) -> RuntimeValue {
    let parts: Vec<String> = args.iter().map(|v| v.as_string()).collect();
    RuntimeValue::String(parts.concat())
}

/// Extract substring from a string
pub(crate) fn runtime_substr(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::String(String::new());
    }
    let s = args[0].as_string();
    let start = args.get(1).and_then(|v| v.as_int()).unwrap_or(0) as usize;
    let len = args.get(2).and_then(|v| v.as_int()).map(|n| n as usize);

    let chars: Vec<char> = s.chars().collect();
    let end = len
        .map(|l| (start + l).min(chars.len()))
        .unwrap_or(chars.len());

    if start >= chars.len() {
        RuntimeValue::String(String::new())
    } else {
        RuntimeValue::String(chars[start..end].iter().collect())
    }
}

/// Get the length of a string
pub(crate) fn runtime_strlen(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Int(0);
    }
    RuntimeValue::Int(args[0].as_string().len() as i64)
}

/// Split a string by a delimiter
pub(crate) fn runtime_split(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Array(vec![]);
    }
    let s = args[0].as_string();
    let delimiter = if args.len() > 1 {
        args[1].as_string()
    } else {
        ",".to_string()
    };

    let parts: Vec<RuntimeValue> = if delimiter.is_empty() {
        // Empty delimiter means split into individual characters
        s.chars()
            .map(|c| RuntimeValue::String(c.to_string()))
            .collect()
    } else {
        s.split(&delimiter)
            .map(|part| RuntimeValue::String(part.to_string()))
            .collect()
    };

    RuntimeValue::Array(parts)
}

/// Join an array of values into a string with a separator
pub(crate) fn runtime_join(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::String(String::new());
    }

    let array = match &args[0] {
        RuntimeValue::Array(arr) => arr,
        _ => return RuntimeValue::String(args[0].as_string()),
    };

    let separator = if args.len() > 1 {
        args[1].as_string()
    } else {
        ",".to_string()
    };

    let parts: Vec<String> = array.iter().map(|v| v.as_string()).collect();
    RuntimeValue::String(parts.join(&separator))
}

/// Check if a string starts with a prefix
pub(crate) fn runtime_starts_with(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Bool(false);
    }
    let s = args[0].as_string();
    let prefix = args[1].as_string();
    RuntimeValue::Bool(s.starts_with(&prefix))
}

/// Check if a string ends with a suffix
pub(crate) fn runtime_ends_with(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Bool(false);
    }
    let s = args[0].as_string();
    let suffix = args[1].as_string();
    RuntimeValue::Bool(s.ends_with(&suffix))
}

/// Find the first index of a substring in a string
pub(crate) fn runtime_index_of(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Int(-1);
    }
    let s = args[0].as_string();
    let needle = args[1].as_string();

    match s.find(&needle) {
        Some(idx) => RuntimeValue::Int(idx as i64),
        None => RuntimeValue::Int(-1),
    }
}

/// Find the last index of a substring in a string
pub(crate) fn runtime_last_index_of(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Int(-1);
    }
    let s = args[0].as_string();
    let needle = args[1].as_string();

    match s.rfind(&needle) {
        Some(idx) => RuntimeValue::Int(idx as i64),
        None => RuntimeValue::Int(-1),
    }
}

/// String slice method (similar to substring)
pub(crate) fn runtime_slice(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::String(String::new());
    }
    let s = args[0].as_string();
    let start = args.get(1).and_then(|v| v.as_int()).unwrap_or(0);
    let end = args.get(2).and_then(|v| v.as_int());

    let chars: Vec<char> = s.chars().collect();
    let len = chars.len() as i64;

    // Handle negative indices
    let start_idx = (if start < 0 {
        (len + start).max(0)
    } else {
        start.min(len)
    }) as usize;
    let end_idx = match end {
        Some(e) => (if e < 0 { (len + e).max(0) } else { e.min(len) }) as usize,
        None => chars.len(),
    };

    if start_idx >= end_idx {
        RuntimeValue::String(String::new())
    } else {
        RuntimeValue::String(chars[start_idx..end_idx].iter().collect())
    }
}

/// Get character at index
pub(crate) fn runtime_char_at(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::String(String::new());
    }
    let s = args[0].as_string();
    let index = args[1].as_int().unwrap_or(0);

    if index < 0 {
        return RuntimeValue::String(String::new());
    }

    match s.chars().nth(index as usize) {
        Some(ch) => RuntimeValue::String(ch.to_string()),
        None => RuntimeValue::String(String::new()),
    }
}

/// Check if string includes a substring
pub(crate) fn runtime_includes(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Bool(false);
    }
    let s = args[0].as_string();
    let needle = args[1].as_string();
    RuntimeValue::Bool(s.contains(&needle))
}

/// Trim whitespace from both ends
pub(crate) fn runtime_trim(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::String(String::new());
    }
    RuntimeValue::String(args[0].as_string().trim().to_string())
}

/// Trim whitespace from start
pub(crate) fn runtime_trim_start(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::String(String::new());
    }
    RuntimeValue::String(args[0].as_string().trim_start().to_string())
}

/// Trim whitespace from end
pub(crate) fn runtime_trim_end(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::String(String::new());
    }
    RuntimeValue::String(args[0].as_string().trim_end().to_string())
}

/// Convert to lowercase
pub(crate) fn runtime_to_lower_case(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::String(String::new());
    }
    RuntimeValue::String(args[0].as_string().to_lowercase())
}

/// Convert to uppercase
pub(crate) fn runtime_to_upper_case(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::String(String::new());
    }
    RuntimeValue::String(args[0].as_string().to_uppercase())
}

/// Replace first occurrence of a substring
pub(crate) fn runtime_replace(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 3 {
        return if args.is_empty() {
            RuntimeValue::String(String::new())
        } else {
            RuntimeValue::String(args[0].as_string())
        };
    }
    let s = args[0].as_string();
    let from = args[1].as_string();
    let to = args[2].as_string();

    RuntimeValue::String(s.replacen(&from, &to, 1))
}

/// Repeat a string n times
pub(crate) fn runtime_repeat(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::String(String::new());
    }
    let s = args[0].as_string();
    let count = args[1].as_int().unwrap_or(0);

    if count < 0 {
        return RuntimeValue::String(String::new());
    }

    RuntimeValue::String(s.repeat(count as usize))
}
