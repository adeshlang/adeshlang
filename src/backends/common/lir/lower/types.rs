//! Type conversion and manipulation
//!
//! This module handles conversion between HIR and LIR types, element size computation,
//! and type-based conversions during lowering.

use super::super::{LirFunction, LirInst, LirType, ValueId};
use super::core::LowerCtx;
use crate::parsing::hir::{ArrayKind, HirType};

/// Convert HirType to a string representation for array type metadata
fn hir_type_to_string(hir_type: &HirType) -> String {
    match hir_type {
        HirType::U8 => "u8".to_string(),
        HirType::U16 => "u16".to_string(),
        HirType::U32 => "u32".to_string(),
        HirType::U64 => "u64".to_string(),
        HirType::U128 => "u128".to_string(),
        HirType::I8 => "i8".to_string(),
        HirType::I16 => "i16".to_string(),
        HirType::I32 => "i32".to_string(),
        HirType::I64 => "i64".to_string(),
        HirType::I128 => "i128".to_string(),
        HirType::F32 => "f32".to_string(),
        HirType::F64 => "f64".to_string(),
        HirType::Simd(elem, lanes) => format!("Simd<{}, {}>", hir_type_to_string(elem), lanes),
        HirType::String => "string".to_string(),
        HirType::Bool => "bool".to_string(),
        _ => "any".to_string(),
    }
}

/// Convert HirType to LirType for type tracking during lowering
pub(super) fn hir_type_to_lir_type(hir_type: &HirType) -> LirType {
    match hir_type {
        HirType::Int => LirType::I64,
        HirType::Float => LirType::F64,
        HirType::F32 => LirType::F32,
        HirType::F64 => LirType::F64,
        HirType::I8 => LirType::I8,
        HirType::I16 => LirType::I16,
        HirType::I32 => LirType::I32,
        HirType::I64 => LirType::I64,
        HirType::I128 => LirType::I128,
        HirType::U8 => LirType::U8,
        HirType::U16 => LirType::U16,
        HirType::U32 => LirType::U32,
        HirType::U64 => LirType::U64,
        HirType::U128 => LirType::U128,
        HirType::Bool => LirType::Bool,
        HirType::BorrowImmut(_)
        | HirType::BorrowMut(_)
        | HirType::Borrow(_, _)
        | HirType::Shared(_)
        | HirType::Weak(_)
        | HirType::Simd(_, _) => LirType::Ptr,
        _ => LirType::I64, // Default to I64 for complex types
    }
}

pub(super) fn hir_type_to_lir(ty: Option<&HirType>) -> LirType {
    match ty {
        Some(HirType::Int) => LirType::I64,
        Some(HirType::Float) => LirType::F64,
        Some(HirType::Bool) => LirType::Bool,
        Some(HirType::String) | Some(HirType::Char) => LirType::Ptr,
        Some(HirType::Null) | Some(HirType::Any) | Some(HirType::Unknown) => LirType::Void,
        Some(HirType::Array(_, _)) | Some(HirType::Object) | Some(HirType::Function(_, _)) => {
            LirType::Ptr
        }
        Some(HirType::Dict(_, _)) | Some(HirType::Set(_)) | Some(HirType::Tuple(_)) => LirType::Ptr,
        Some(HirType::Class(_)) | Some(HirType::Instance(_)) | Some(HirType::Promise(_)) => {
            LirType::Ptr
        }
        Some(HirType::U8) => LirType::U8,
        Some(HirType::U16) => LirType::U16,
        Some(HirType::U32) => LirType::U32,
        Some(HirType::U64) => LirType::U64,
        Some(HirType::U128) => LirType::U128,
        Some(HirType::I8) => LirType::I8,
        Some(HirType::I16) => LirType::I16,
        Some(HirType::I32) => LirType::I32,
        Some(HirType::I64) => LirType::I64,
        Some(HirType::I128) => LirType::I128,
        Some(HirType::F32) => LirType::F32,
        Some(HirType::F64) => LirType::F64,
        // Memory model types - all resolve to Ptr (references)
        Some(HirType::BorrowImmut(_)) => LirType::Ptr,
        Some(HirType::BorrowMut(_)) => LirType::Ptr,
        Some(HirType::Borrow(_, _)) => LirType::Ptr,
        Some(HirType::Shared(_)) => LirType::Ptr,
        Some(HirType::Weak(_)) => LirType::Ptr,
        Some(HirType::Simd(elem, _)) => hir_type_to_lir_type(elem),
        None => LirType::Void,
    }
}

/// Compute element size in bytes for a given HIR type (used for typed pointers)
pub(super) fn elem_size_from_hir_type(hir_type: &HirType) -> Result<usize, String> {
    match hir_type {
        HirType::U8 | HirType::I8 | HirType::Bool => Ok(1),
        HirType::U16 | HirType::I16 => Ok(2),
        HirType::U32 | HirType::I32 | HirType::F32 => Ok(4),
        HirType::U64 | HirType::I64 | HirType::F64 => Ok(8),
        HirType::U128 | HirType::I128 => Ok(16),
        HirType::Char => Ok(4),
        HirType::Int | HirType::Float => Ok(8), // defaults
        // For composite/unknown types, fall back to byte-level until layout is available
        _ => Err(format!(
            "Cannot compute element size for HIR type: {:?}",
            hir_type
        )),
    }
}

/// Apply type conversion to a value based on a type annotation
/// For example, if type is Int, try to convert a string to a number
pub(super) fn apply_type_conversion(
    func: &mut LirFunction,
    _ctx: &mut LowerCtx,
    val: ValueId,
    hir_type: &HirType,
) -> Result<ValueId, String> {
    match hir_type {
        HirType::Int => {
            // Try to convert to int: call the "int" builtin function
            let result = func.alloc_value();
            func.push_to_block(
                _ctx.current_block,
                LirInst::CallBuiltin(result, "int".to_string(), vec![val]),
            );
            Ok(result)
        }
        HirType::Float => {
            // Try to convert to float: call the "float" builtin function
            let result = func.alloc_value();
            func.push_to_block(
                _ctx.current_block,
                LirInst::CallBuiltin(result, "float".to_string(), vec![val]),
            );
            // Track the type for proper operation selection
            _ctx.value_types.insert(result, LirType::F64);
            Ok(result)
        }
        HirType::F32 => {
            // Convert to f32: call the "f32" builtin function
            let result = func.alloc_value();
            func.push_to_block(
                _ctx.current_block,
                LirInst::CallBuiltin(result, "f32".to_string(), vec![val]),
            );
            // Track the type for proper operation selection
            _ctx.value_types.insert(result, LirType::F32);
            Ok(result)
        }
        HirType::F64 => {
            // Convert to f64: call the "f64" builtin function
            let result = func.alloc_value();
            func.push_to_block(
                _ctx.current_block,
                LirInst::CallBuiltin(result, "f64".to_string(), vec![val]),
            );
            // Track the type for proper operation selection
            _ctx.value_types.insert(result, LirType::F64);
            Ok(result)
        }
        HirType::U8 => {
            let result = func.alloc_value();
            func.push_to_block(
                _ctx.current_block,
                LirInst::CallBuiltin(result, "u8".to_string(), vec![val]),
            );
            Ok(result)
        }
        HirType::U16 => {
            let result = func.alloc_value();
            func.push_to_block(
                _ctx.current_block,
                LirInst::CallBuiltin(result, "u16".to_string(), vec![val]),
            );
            Ok(result)
        }
        HirType::U32 => {
            let result = func.alloc_value();
            func.push_to_block(
                _ctx.current_block,
                LirInst::CallBuiltin(result, "u32".to_string(), vec![val]),
            );
            Ok(result)
        }
        HirType::U64 => {
            let result = func.alloc_value();
            func.push_to_block(
                _ctx.current_block,
                LirInst::CallBuiltin(result, "u64".to_string(), vec![val]),
            );
            Ok(result)
        }
        HirType::U128 => {
            let result = func.alloc_value();
            func.push_to_block(
                _ctx.current_block,
                LirInst::CallBuiltin(result, "u128".to_string(), vec![val]),
            );
            Ok(result)
        }
        HirType::I8 => {
            let result = func.alloc_value();
            func.push_to_block(
                _ctx.current_block,
                LirInst::CallBuiltin(result, "i8".to_string(), vec![val]),
            );
            Ok(result)
        }
        HirType::I16 => {
            let result = func.alloc_value();
            func.push_to_block(
                _ctx.current_block,
                LirInst::CallBuiltin(result, "i16".to_string(), vec![val]),
            );
            Ok(result)
        }
        HirType::I32 => {
            let result = func.alloc_value();
            func.push_to_block(
                _ctx.current_block,
                LirInst::CallBuiltin(result, "i32".to_string(), vec![val]),
            );
            Ok(result)
        }
        HirType::I64 => {
            let result = func.alloc_value();
            func.push_to_block(
                _ctx.current_block,
                LirInst::CallBuiltin(result, "i64".to_string(), vec![val]),
            );
            Ok(result)
        }
        HirType::I128 => {
            let result = func.alloc_value();
            func.push_to_block(
                _ctx.current_block,
                LirInst::CallBuiltin(result, "i128".to_string(), vec![val]),
            );
            Ok(result)
        }
        HirType::String => {
            // Try to convert to string: call the "str" builtin function
            let result = func.alloc_value();
            func.push_to_block(
                _ctx.current_block,
                LirInst::CallBuiltin(result, "str".to_string(), vec![val]),
            );
            Ok(result)
        }
        HirType::Char => {
            let result = func.alloc_value();
            func.push_to_block(
                _ctx.current_block,
                LirInst::CallBuiltin(result, "char".to_string(), vec![val]),
            );
            Ok(result)
        }
        HirType::Bool => {
            // Try to convert to bool: call the "bool" builtin function
            let result = func.alloc_value();
            func.push_to_block(
                _ctx.current_block,
                LirInst::CallBuiltin(result, "bool".to_string(), vec![val]),
            );
            Ok(result)
        }
        HirType::Array(_elem_type, kind) => {
            match kind {
                ArrayKind::Raw => {
                    let result = func.alloc_value();
                    func.push_to_block(
                        _ctx.current_block,
                        LirInst::CallBuiltin(result, "array_to_raw".to_string(), vec![val]),
                    );
                    Ok(result)
                }
                ArrayKind::Fixed(n) => {
                    let type_str = hir_type_to_string(_elem_type.as_ref());
                    let type_val = func.alloc_value();
                    func.push_to_block(
                        _ctx.current_block,
                        LirInst::ConstString(type_val, type_str),
                    );
                    let n_val = func.alloc_value();
                    func.push_to_block(_ctx.current_block, LirInst::ConstI64(n_val, *n as i64));
                    let result = func.alloc_value();
                    func.push_to_block(
                        _ctx.current_block,
                        LirInst::CallBuiltin(
                            result,
                            "array_to_fixed".to_string(),
                            vec![val, type_val, n_val],
                        ),
                    );
                    Ok(result)
                }
                ArrayKind::FixedRaw(n) => {
                    let type_str = hir_type_to_string(_elem_type.as_ref());
                    let type_val = func.alloc_value();
                    func.push_to_block(
                        _ctx.current_block,
                        LirInst::ConstString(type_val, type_str),
                    );
                    let n_val = func.alloc_value();
                    func.push_to_block(_ctx.current_block, LirInst::ConstI64(n_val, *n as i64));
                    let result = func.alloc_value();
                    func.push_to_block(
                        _ctx.current_block,
                        LirInst::CallBuiltin(
                            result,
                            "array_to_fixed_raw".to_string(),
                            vec![val, type_val, n_val],
                        ),
                    );
                    Ok(result)
                }
                _ => {
                    // Keep as is for dynamic, fixed, etc.
                    Ok(val)
                }
            }
        }
        _ => {
            // No conversion needed for other types
            Ok(val)
        }
    }
}
