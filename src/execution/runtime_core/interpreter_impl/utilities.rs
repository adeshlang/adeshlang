//! Standalone Utility Helper Functions
//!
//! This module contains standalone utility functions extracted from interpreter_core.rs
//! as part of Phase 4E refactoring. These functions are pure utilities that don't depend
//! on the Interpreter struct state.
//!
//! # Functions
//!
//! - `coerce_value_to_type`: Type conversion from Number to fixed-width integer types
//! - `find_file_recursive`: Recursive file search helper for module resolution
//! - `resolve_path`: Path resolution for module imports
//! - `value_to_string`: Value to string conversion for display/printing
//! - `err_e`: Simple error helper (legacy/dead code)

use crate::parsing::ast::Value;
use std::path::{Path, PathBuf};

/// Coerce a Value to a specific fixed-width type.
///
/// Converts Number variants to typed integers (i8, u32, f64, etc.) with range checking.
/// Returns the original value if it already matches the target type or no coercion is needed.
pub fn coerce_value_to_type(val: &Value, target_type: &str) -> Result<Value, String> {
    use crate::parsing::ast::Value::*;
    match (val, target_type.to_lowercase().as_str()) {
        (Number(n), "i8") => {
            if *n >= i8::MIN as f64 && *n <= i8::MAX as f64 {
                Ok(I8(*n as i8))
            } else {
                Err(format!("{} out of range for i8", n))
            }
        }
        (Number(n), "i16") => {
            if *n >= i16::MIN as f64 && *n <= i16::MAX as f64 {
                Ok(I16(*n as i16))
            } else {
                Err(format!("{} out of range for i16", n))
            }
        }
        (Number(n), "i32") => {
            if *n >= i32::MIN as f64 && *n <= i32::MAX as f64 {
                Ok(I32(*n as i32))
            } else {
                Err(format!("{} out of range for i32", n))
            }
        }
        (Number(n), "i64") => {
            if *n >= i64::MIN as f64 && *n <= i64::MAX as f64 {
                Ok(I64(*n as i64))
            } else {
                Err(format!("{} out of range for i64", n))
            }
        }
        (Number(n), "i128") => Ok(I128(*n as i128)),
        (Number(n), "u8") => {
            if *n >= 0.0 && *n <= u8::MAX as f64 {
                Ok(U8(*n as u8))
            } else {
                Err(format!("{} out of range for u8", n))
            }
        }
        (Number(n), "u16") => {
            if *n >= 0.0 && *n <= u16::MAX as f64 {
                Ok(U16(*n as u16))
            } else {
                Err(format!("{} out of range for u16", n))
            }
        }
        (Number(n), "u32") => {
            if *n >= 0.0 && *n <= u32::MAX as f64 {
                Ok(U32(*n as u32))
            } else {
                Err(format!("{} out of range for u32", n))
            }
        }
        (Number(n), "u64") => {
            if *n >= 0.0 && *n <= u64::MAX as f64 {
                Ok(U64(*n as u64))
            } else {
                Err(format!("{} out of range for u64", n))
            }
        }
        (Number(n), "u128") => Ok(U128(*n as u128)),
        (Number(n), "f32") => Ok(F32(*n as f32)),
        (Number(n), "f64") => Ok(F64(*n)),
        // If value already matches the target type, return as-is
        (I8(_), "i8")
        | (I16(_), "i16")
        | (I32(_), "i32")
        | (I64(_), "i64")
        | (I128(_), "i128")
        | (U8(_), "u8")
        | (U16(_), "u16")
        | (U32(_), "u32")
        | (U64(_), "u64")
        | (U128(_), "u128")
        | (F32(_), "f32")
        | (F64(_), "f64") => Ok(val.clone()),
        // Default: return value as-is if no coercion needed/possible
        _ => Ok(val.clone()),
    }
}

/// Recursively search for a file starting from a given directory.
///
/// Used by module resolution to find .adesh files in the directory tree.
/// Returns the first matching file path, or None if not found.
pub fn find_file_recursive(start: &Path, target: &str) -> Option<PathBuf> {
    let mut stack = vec![start.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if let Ok(read) = std::fs::read_dir(&dir) {
            for e in read.filter_map(|r| r.ok()) {
                let p = e.path();
                if p.is_file() {
                    if let Some(s) = p.to_str() {
                        if s.ends_with(target) {
                            return Some(p.clone());
                        }
                    }
                    if let Some(fname) = p.file_name().and_then(|n| n.to_str()) {
                        if fname == target {
                            return Some(p.clone());
                        }
                    }
                } else if p.is_dir() {
                    stack.push(p);
                }
            }
        }
    }
    None
}

/// Resolve a module import path relative to the current module.
///
/// Handles relative paths, adds .adesh extension if needed, and searches
/// parent directories recursively to find the module file.
pub fn resolve_path(path: &str, module_id: &str) -> String {
    use std::path::{Path, PathBuf};
    let base = Path::new(module_id).parent().unwrap_or(Path::new("."));

    // Check if the path refers to a package in adl_modules by climbing parents
    let mut probe_dir = Some(base);
    while let Some(dir) = probe_dir {
        let adl_modules_path = dir.join("adl_modules").join(path);
        if adl_modules_path.exists() && adl_modules_path.is_dir() {
            for entry_file in &[
                "src/lib.adesh",
                "src/main.adesh",
                "lib.adesh",
                "main.adesh",
                "lib.adl",
                "main.adl",
                "adesh.adl",
            ] {
                let entry_path = adl_modules_path.join(entry_file);
                if entry_path.exists() {
                    return entry_path.to_string_lossy().to_string();
                }
            }
        }
        probe_dir = dir.parent();
    }

    let p = Path::new(path);
    let joined: PathBuf = if p.extension().and_then(|e| e.to_str()) == Some("ind") {
        base.join(p)
    } else {
        base.join(p).with_extension("ind")
    };
    if joined.exists() {
        return joined.to_string_lossy().to_string();
    }
    let target_name = p.file_name().and_then(|n| n.to_str()).unwrap_or(path);
    if p.extension().is_some() {
        let target_ind = target_name.to_string();
        let mut probe: Option<&Path> = Some(base);
        while let Some(dir) = probe {
            if let Some(found) = find_file_recursive(dir, &target_ind) {
                return found.to_string_lossy().to_string();
            }
            probe = dir.parent();
        }
        if let Some(found) = find_file_recursive(Path::new("."), &target_ind) {
            return found.to_string_lossy().to_string();
        }
    } else {
        // Try primary extension (.adesh) first
        let target_adesh = format!("{}.adesh", target_name);
        let mut probe: Option<&Path> = Some(base);
        while let Some(dir) = probe {
            if let Some(found) = find_file_recursive(dir, &target_adesh) {
                return found.to_string_lossy().to_string();
            }
            probe = dir.parent();
        }
        if let Some(found) = find_file_recursive(Path::new("."), &target_adesh) {
            return found.to_string_lossy().to_string();
        }

        // Try secondary extension (.adl) next
        let target_adl = format!("{}.adl", target_name);
        let mut probe = Some(base);
        while let Some(dir) = probe {
            if let Some(found) = find_file_recursive(dir, &target_adl) {
                return found.to_string_lossy().to_string();
            }
            probe = dir.parent();
        }
        if let Some(found) = find_file_recursive(Path::new("."), &target_adl) {
            return found.to_string_lossy().to_string();
        }
    }
    joined.to_string_lossy().to_string()
}

/// Convert a Value to a human-readable string representation.
///
/// Used by print statements and debugging. Handles all Value variants including
/// collections, objects, functions, classes, and complex types.
pub fn value_to_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => {
            if n.fract() == 0.0 && n.abs() < 1e15 {
                format!("{}", *n as i64)
            } else {
                n.to_string()
            }
        }
        Value::BigInt(n) => n.to_string(),
        Value::Char(c) => c.to_string(),
        Value::Str(s) => s.clone(),
        // Fixed-width integer types - display without suffix
        Value::U8(n) => n.to_string(),
        Value::U16(n) => n.to_string(),
        Value::U32(n) => n.to_string(),
        Value::U64(n) => n.to_string(),
        Value::U128(n) => n.to_string(),
        Value::I8(n) => n.to_string(),
        Value::I16(n) => n.to_string(),
        Value::I32(n) => n.to_string(),
        Value::I64(n) => n.to_string(),
        Value::I128(n) => n.to_string(),
        Value::F32(n) => n.to_string(),
        Value::F64(n) => n.to_string(),
        Value::Array(arr) => {
            let elements: Vec<String> = arr.iter().map(value_to_string).collect();
            format!("[{}]", elements.join(", "))
        }
        Value::RawArray(elem_type, arr) => {
            let elements: Vec<String> = arr.iter().map(value_to_string).collect();
            format!("[{}] (raw {})", elements.join(", "), elem_type)
        }
        Value::DynArray(da) => {
            let elements: Vec<String> = da.data.iter().map(value_to_string).collect();
            format!(
                "[{}] (len={}, cap={})",
                elements.join(", "),
                da.len(),
                da.capacity()
            )
        }
        Value::Tuple(t) => {
            let elements: Vec<String> = t.iter().map(value_to_string).collect();
            format!("({})", elements.join(", "))
        }
        Value::Set(s) => {
            let elements: Vec<String> = s.iter().map(value_to_string).collect();
            format!("{{{}}}", elements.join(", "))
        }
        Value::Object(obj) => {
            let pairs: Vec<String> = obj
                .iter()
                .map(|(k, v)| format!("{}: {}", k, value_to_string(v)))
                .collect();
            format!("{{{}}}", pairs.join(", "))
        }
        Value::Class(c) => format!("<class {}>", c.name),
        Value::Instance(i) => format!("<{} instance>", i.class.name),
        Value::UserFunction(f) => format!("<fn {}>", f.name),
        Value::Function(_) => "<native fn>".to_string(),
        Value::BoundMethod(f, _) => format!("<method {}>", f.name),
        Value::Promise(id) => format!("<Promise {}>", id),
        Value::Enum(e) => format!("<enum {}>", e.name),
        Value::EnumCtor(e, v) => format!("<enum ctor {}::{}>", e.name, v),
        Value::Super(_, _) => "<super>".to_string(),
        Value::Complex(re, im) => format!("{}+{}i", re, im),
        Value::LazyRange(s, e, step) => format!("range({}, {}, {})", s, e, step),
        Value::Struct(s) => format!("<struct {}>", s.name),
        Value::Interface(i) => format!("<interface {}>", i.name),
        Value::BoundNative(_, _) => "<bound native>".to_string(),
        Value::Ref(inner, _) => format!("&{}", value_to_string(inner)),
        Value::Error(e) => format!("Error: {}", e.message),
        Value::Share(strong_ref) => unsafe {
            let obj = &*strong_ref.ptr;
            format!(
                "<share strong_count={} weak_count={} value={}>",
                obj.strong_count.load(std::sync::atomic::Ordering::SeqCst),
                obj.weak_count.load(std::sync::atomic::Ordering::SeqCst),
                value_to_string(&obj.value)
            )
        },
        Value::Weak(weak_ref) => unsafe {
            let obj = &*weak_ref.ptr;
            format!(
                "<weak strong_count={} weak_count={}>",
                obj.strong_count.load(std::sync::atomic::Ordering::SeqCst),
                obj.weak_count.load(std::sync::atomic::Ordering::SeqCst)
            )
        },
    }
}

/// Simple error helper function (legacy/dead code).
///
/// Converts a message into a String. May be unused in current codebase.
#[allow(dead_code)]
pub fn err_e<T: Into<String>>(msg: T) -> String {
    msg.into()
}
