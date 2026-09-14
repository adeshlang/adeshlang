//! Type Checking Helper Functions
//!
//! This module contains type checking and validation utilities for the interpreter.
//! Extracted from interpreter_core.rs as part of Phase 4G refactoring.
//!
//! # Functions
//!
//! - **Type Validation**: `ann_matches_value` - runtime type annotation checking
//! - **Type Resolution**: Support for builtin types, function types, array types, and user-defined aliases
//!
//! # Design
//!
//! The type checking system supports:
//! - Builtin primitive types (int, string, bool, etc.)
//! - Fixed-width integer types (u8, u16, u32, u64, u128, i8-i128)
//! - Fixed-width float types (f32, f64)
//! - Function types with parameter and return type checking
//! - Array types with element type and size/capacity constraints
//! - User-defined type aliases (objects with __type_alias__ marker)
//! - Nullable types (suffix with ?)
//!
//! Type checking is performed at runtime to support dynamic features while
//! maintaining type safety through annotations.

use crate::execution::runtime_core::interpreter::{Env, element_matches_type};
use crate::parsing::ast::Value;
use crate::typesystem::checker::{Ty, is_subtype};

/// Runtime annotation check that can resolve named type aliases found in the
/// active environment chain starting from `start_env` (or global if None).
///
/// This function performs comprehensive type checking including:
/// - Function type compatibility (parameters and return types)
/// - Array type checking with element type and size constraints
/// - Nullable type support
/// - User-defined type alias resolution
/// - Builtin type validation
///
/// Returns true if the value matches the annotation, false otherwise.
fn get_numeric_value(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => Some(*n),
        Value::U8(n) => Some(*n as f64),
        Value::U16(n) => Some(*n as f64),
        Value::U32(n) => Some(*n as f64),
        Value::U64(n) => Some(*n as f64),
        Value::U128(n) => Some(*n as f64),
        Value::I8(n) => Some(*n as f64),
        Value::I16(n) => Some(*n as f64),
        Value::I32(n) => Some(*n as f64),
        Value::I64(n) => Some(*n as f64),
        Value::I128(n) => Some(*n as f64),
        Value::F32(n) => Some(*n as f64),
        Value::F64(n) => Some(*n),
        _ => None,
    }
}

pub fn ann_matches_value(
    envs: &[Env],
    global: usize,
    start_env: Option<usize>,
    ann_opt: &Option<String>,
    v: &Value,
) -> bool {
    if ann_opt.is_none() {
        return true;
    }
    let s = ann_opt.as_ref().unwrap();

    // Check for top-level union type: T1 | T2 | T3
    let mut depth_bracket = 0;
    let mut depth_paren = 0;
    let mut depth_angle = 0;
    let mut depth_brace = 0;
    let mut union_parts: Vec<&str> = Vec::new();
    let mut last_idx = 0;

    for (i, ch) in s.char_indices() {
        match ch {
            '[' => depth_bracket += 1,
            ']' => depth_bracket = (depth_bracket - 1).max(0),
            '(' => depth_paren += 1,
            ')' => depth_paren = (depth_paren - 1).max(0),
            '<' => depth_angle += 1,
            '>' => depth_angle = (depth_angle - 1).max(0),
            '{' => depth_brace += 1,
            '}' => depth_brace = (depth_brace - 1).max(0),
            '|' if depth_bracket == 0 && depth_paren == 0 && depth_angle == 0 && depth_brace == 0 => {
                union_parts.push(&s[last_idx..i]);
                last_idx = i + 1;
            }
            _ => {}
        }
    }
    if !union_parts.is_empty() {
        union_parts.push(&s[last_idx..]);
        for part in union_parts {
            let part_trimmed = part.trim();
            if !part_trimmed.is_empty() {
                let part_opt = Some(part_trimmed.to_string());
                if ann_matches_value(envs, global, start_env, &part_opt, v) {
                    return true;
                }
            }
        }
        return false;
    }

    // Try to parse annotation into Ty for builtin types/function types
    if let Some(ty) = crate::types::type_system::type_from_name(s) {
        match &ty {
            Ty::Any => return true,
            Ty::Nullable(_) => {
                if matches!(v, Value::Null) {
                    return true;
                }
            }
            Ty::Func {
                params: exp_params,
                ret: _,
            } => match v {
                Value::UserFunction(u) => {
                    let mut actual_params: Vec<Ty> = Vec::new();
                    for p in &u.params {
                        if let Some(tn) = &p.2 {
                            actual_params.push(
                                crate::types::type_system::type_from_name(tn).unwrap_or(Ty::Any),
                            );
                        } else {
                            actual_params.push(Ty::Any);
                        }
                    }
                    let actual_ret = if let Some(rt) = &u.ret_type {
                        crate::types::type_system::type_from_name(rt).unwrap_or(Ty::Any)
                    } else {
                        Ty::Any
                    };
                    let actual = Ty::Func {
                        params: actual_params,
                        ret: Box::new(actual_ret),
                    };
                    return is_subtype(
                        &actual,
                        &Ty::Func {
                            params: exp_params.clone(),
                            ret: Box::new(Ty::Any),
                        },
                    );
                }
                Value::BoundMethod(u, _inst) => {
                    let mut actual_params: Vec<Ty> = Vec::new();
                    for p in &u.params {
                        if let Some(tn) = &p.2 {
                            actual_params.push(
                                crate::types::type_system::type_from_name(tn).unwrap_or(Ty::Any),
                            );
                        } else {
                            actual_params.push(Ty::Any);
                        }
                    }
                    let actual_ret = if let Some(rt) = &u.ret_type {
                        crate::types::type_system::type_from_name(rt).unwrap_or(Ty::Any)
                    } else {
                        Ty::Any
                    };
                    let actual = Ty::Func {
                        params: actual_params,
                        ret: Box::new(actual_ret),
                    };
                    return is_subtype(
                        &actual,
                        &Ty::Func {
                            params: exp_params.clone(),
                            ret: Box::new(Ty::Any),
                        },
                    );
                }
                _ => return true,
            },
            _ => {}
        }
    }

    // Fall back to name-based lookup in environment chain for user-defined aliases
    let nullable = s.ends_with('?');
    let base = if nullable {
        &s[..s.len() - 1]
    } else {
        s.as_str()
    };
    use crate::parsing::ast::Value::*;
    if let Null = v {
        return nullable || base.eq_ignore_ascii_case("null") || base.eq_ignore_ascii_case("any") || base.eq_ignore_ascii_case("undefined");
    }

    // Handle array type annotations: [T], [T;N], [T;N;raw]
    if base.starts_with('[') && base.ends_with(']') {
        let inner = &base[1..base.len() - 1];
        let parts: Vec<&str> = inner.split(';').map(|s| s.trim()).collect();
        let is_raw =
            parts.len() >= 3 && parts.get(2).is_some_and(|s| s.eq_ignore_ascii_case("raw"));

        // Parse element type and optional size/capacity constraint
        let elem_type = parts.get(0).copied().unwrap_or("");
        let size_or_capacity = parts.get(1).and_then(|s| s.parse::<usize>().ok());

        // Handle array-like values depending on whether the annotation expects raw layout
        if is_raw {
            // [T;N;raw] -> RawArray with exact size N
            if let RawArray(_, arr) = v {
                // Check size if specified - must be exact
                if let Some(size) = size_or_capacity {
                    if arr.len() != size {
                        return false;
                    }
                }
                return arr.iter().all(|elem| element_matches_type(elem, elem_type));
            }
            return false;
        } else {
            // Non-raw annotation: accept regular Array, DynArray or RawArray
            match v {
                DynArray(da) => {
                    // For [T;N], N is capacity, not exact size
                    // Check that current size <= capacity
                    if let Some(capacity) = size_or_capacity {
                        if da.data.len() > capacity {
                            return false;
                        }
                        // Also check that tracked_capacity matches if it's set
                        if da.tracked_capacity > 0 && da.tracked_capacity != capacity {
                            return false;
                        }
                    }
                    return da
                        .data
                        .iter()
                        .all(|elem| element_matches_type(elem, elem_type));
                }
                Array(arr) => {
                    // For plain arrays without capacity tracking
                    if let Some(size) = size_or_capacity {
                        if arr.len() != size {
                            return false;
                        }
                    }
                    return arr.iter().all(|elem| element_matches_type(elem, elem_type));
                }
                RawArray(_, arr) => {
                    // RawArray in non-raw context - check exact size
                    if let Some(size) = size_or_capacity {
                        if arr.len() != size {
                            return false;
                        }
                    }
                    return arr.iter().all(|elem| element_matches_type(elem, elem_type));
                }
                _ => return false,
            }
        }
    }

    // builtin simple checks
    let lowered_base = base.to_lowercase();
    if lowered_base.starts_with("set<") || lowered_base == "set" {
        return matches!(v, Set(_));
    }
    if lowered_base.starts_with("dict<") || lowered_base.starts_with("map<") || lowered_base == "dict" || lowered_base == "map" || lowered_base == "object" || lowered_base == "{}" {
        return matches!(v, Object(_));
    }
    if lowered_base.starts_with("array<") || lowered_base.starts_with("list<") || lowered_base == "array" || lowered_base == "list" || lowered_base == "[]" {
        return matches!(v, Array(_) | RawArray(_, _) | DynArray(_));
    }

    match lowered_base.as_str() {
        "any" => return true,
        "char" => return matches!(v, Char(_)) || matches!(v, Str(_)),
        "string" | "str" => return matches!(v, Str(_)),
        "int" => {
            if let Some(n) = get_numeric_value(v) {
                return n.fract().abs() < 1e-12;
            }
            return false;
        }
        "uint" => {
            if let Some(n) = get_numeric_value(v) {
                return n.fract().abs() < 1e-12 && n >= 0.0;
            }
            return false;
        }
        "number" | "float" => return get_numeric_value(v).is_some(),
        "bool" | "boolean" => return matches!(v, Bool(_)),
        "array" => return matches!(v, Array(_) | RawArray(_, _) | DynArray(_)),
        "tuple" => return matches!(v, Tuple(_)),
        "set" => return matches!(v, Set(_)),
        "map" | "object" => return matches!(v, Object(_)),
        "complex" => return matches!(v, Complex(_, _)),
        // Fixed-width integer types (unsigned)
        "u8" => {
            if let Some(n) = get_numeric_value(v) {
                return n.fract().abs() < 1e-12 && n >= 0.0 && n <= u8::MAX as f64;
            }
            return false;
        }
        "u16" => {
            if let Some(n) = get_numeric_value(v) {
                return n.fract().abs() < 1e-12 && n >= 0.0 && n <= u16::MAX as f64;
            }
            return false;
        }
        "u32" => {
            if let Some(n) = get_numeric_value(v) {
                return n.fract().abs() < 1e-12 && n >= 0.0 && n <= u32::MAX as f64;
            }
            return false;
        }
        "u64" => {
            if let Some(n) = get_numeric_value(v) {
                return n.fract().abs() < 1e-12 && n >= 0.0 && n <= u64::MAX as f64;
            }
            return false;
        }
        "u128" => {
            if let Some(n) = get_numeric_value(v) {
                return n.fract().abs() < 1e-12 && n >= 0.0 && n <= u128::MAX as f64;
            }
            return false;
        }
        // Fixed-width integer types (signed)
        "i8" => {
            if let Some(n) = get_numeric_value(v) {
                return n.fract().abs() < 1e-12 && n >= i8::MIN as f64 && n <= i8::MAX as f64;
            }
            return false;
        }
        "i16" => {
            if let Some(n) = get_numeric_value(v) {
                return n.fract().abs() < 1e-12 && n >= i16::MIN as f64 && n <= i16::MAX as f64;
            }
            return false;
        }
        "i32" => {
            if let Some(n) = get_numeric_value(v) {
                return n.fract().abs() < 1e-12 && n >= i32::MIN as f64 && n <= i32::MAX as f64;
            }
            return false;
        }
        "i64" => {
            if let Some(n) = get_numeric_value(v) {
                return n.fract().abs() < 1e-12 && n >= i64::MIN as f64 && n <= i64::MAX as f64;
            }
            return false;
        }
        "i128" => {
            if let Some(n) = get_numeric_value(v) {
                return n.fract().abs() < 1e-12 && n >= i128::MIN as f64 && n <= i128::MAX as f64;
            }
            return false;
        }
        // Fixed-width float types
        "f32" => return get_numeric_value(v).is_some(),
        "f64" => return get_numeric_value(v).is_some(),
        _ => {}
    }

    // Extract potential generic arguments from the type annotation (e.g., Box<Int>)
    let (alias_name, type_args) = if let Some(idx) = base.find('<') {
        if base.ends_with('>') {
            let name = &base[..idx];
            let args_str = &base[idx + 1..base.len() - 1];
            let args: Vec<String> = args_str.split(',').map(|s| s.trim().to_string()).collect();
            (name, args)
        } else {
            (base, Vec::new())
        }
    } else {
        (base, Vec::new())
    };

    // Lookup named alias in the environment chain starting from start_env or global
    let mut c = start_env.or(Some(global));
    while let Some(id) = c {
        if let Some(val) = envs[id].values.get(alias_name) {
            if let Value::Object(map) = val {
                // check marker
                if map.get("__type_alias__").is_some() {
                    // value must be an object
                    if let Value::Object(obj_map) = v {
                        // Extract type params mapping for generics substitution
                        let mut subs = rustc_hash::FxHashMap::default();
                        if let Some(Value::Array(tparams)) = map.get("__type_params__") {
                            for (i, tp) in tparams.iter().enumerate() {
                                if let Value::Str(tp_name) = tp {
                                    if let Some(arg) = type_args.get(i) {
                                        subs.insert(tp_name.clone(), arg.clone());
                                    }
                                }
                            }
                        }

                        for (k, tv) in map.iter() {
                            if k == "__type_alias__"
                                || k == "__type_params__"
                                || k == "__defaults__"
                            {
                                continue;
                            }
                            if let Value::Str(type_str) = tv {
                                let (opt, tname) = if type_str.starts_with('?') {
                                    (true, &type_str[1..])
                                } else {
                                    (false, type_str.as_str())
                                };

                                // Substitute generic type argument if applicable
                                let resolved_tname = subs
                                    .get(tname)
                                    .cloned()
                                    .unwrap_or_else(|| tname.to_string());

                                if let Some(field_val) = obj_map.get(k) {
                                    // field present; must match
                                    if !ann_matches_value(
                                        envs,
                                        global,
                                        Some(id),
                                        &Some(resolved_tname),
                                        field_val,
                                    ) {
                                        return false;
                                    }
                                } else {
                                    if !opt {
                                        return false;
                                    }
                                }
                            }
                        }
                        return true;
                    } else {
                        return false;
                    }
                }
            }
        }
        c = envs[id].enclosing;
    }

    // Unknown annotation: be permissive
    true
}
