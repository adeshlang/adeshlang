//! Value handling and conversion operations
//!
//! This module contains logic for handling constants, type conversions,
//! and value operations in the Cranelift AOT compiler.

use cranelift::prelude::*;
use cranelift_module::Module;

use super::context::FunctionCompileContext;
use super::types::AotValueType;
use crate::backends::common::lir::{LirInst, ValueId};

/// Lower constant value instructions
pub(super) fn lower_const_instruction(
    builder: &mut FunctionBuilder,
    ctx: &mut FunctionCompileContext,
    inst: &LirInst,
) -> Result<bool, String> {
    match inst {
        LirInst::ConstI64(dst, val) => {
            let v = builder.ins().iconst(types::I64, *val);
            ctx.value_map.insert(*dst, v);
            ctx.value_types.insert(*dst, AotValueType::I64);
            Ok(true)
        }

        LirInst::ConstF64(dst, val) => {
            let v = builder.ins().f64const(*val);
            ctx.value_map.insert(*dst, v);
            ctx.value_types.insert(*dst, AotValueType::F64);
            Ok(true)
        }

        LirInst::ConstU8(dst, val) => {
            let v = builder.ins().iconst(types::I8, *val as i64);
            ctx.value_map.insert(*dst, v);
            ctx.value_types.insert(*dst, AotValueType::U8);
            Ok(true)
        }

        LirInst::ConstU16(dst, val) => {
            let v = builder.ins().iconst(types::I16, *val as i64);
            ctx.value_map.insert(*dst, v);
            ctx.value_types.insert(*dst, AotValueType::U16);
            Ok(true)
        }

        LirInst::ConstU32(dst, val) => {
            let v = builder.ins().iconst(types::I32, *val as i64);
            ctx.value_map.insert(*dst, v);
            ctx.value_types.insert(*dst, AotValueType::U32);
            Ok(true)
        }

        LirInst::ConstU64(dst, val) => {
            let v = builder.ins().iconst(types::I64, *val as i64);
            ctx.value_map.insert(*dst, v);
            ctx.value_types.insert(*dst, AotValueType::U64);
            Ok(true)
        }

        LirInst::ConstU128(dst, val) => {
            // Cranelift doesn't have native 128-bit support; truncate to 64-bit
            let v = builder.ins().iconst(types::I64, (*val as u64) as i64);
            ctx.value_map.insert(*dst, v);
            ctx.value_types.insert(*dst, AotValueType::U128);
            Ok(true)
        }

        LirInst::ConstI8(dst, val) => {
            let v = builder.ins().iconst(types::I8, *val as i64);
            ctx.value_map.insert(*dst, v);
            ctx.value_types.insert(*dst, AotValueType::I8);
            Ok(true)
        }

        LirInst::ConstI16(dst, val) => {
            let v = builder.ins().iconst(types::I16, *val as i64);
            ctx.value_map.insert(*dst, v);
            ctx.value_types.insert(*dst, AotValueType::I16);
            Ok(true)
        }

        LirInst::ConstI32(dst, val) => {
            let v = builder.ins().iconst(types::I32, *val as i64);
            ctx.value_map.insert(*dst, v);
            ctx.value_types.insert(*dst, AotValueType::I32);
            Ok(true)
        }

        LirInst::ConstI128(dst, val) => {
            // Cranelift doesn't have native 128-bit support; truncate to 64-bit
            let v = builder.ins().iconst(types::I64, (*val as i64));
            ctx.value_map.insert(*dst, v);
            ctx.value_types.insert(*dst, AotValueType::I128);
            Ok(true)
        }

        LirInst::ConstF32(dst, val) => {
            let v = builder.ins().f32const(*val);
            ctx.value_map.insert(*dst, v);
            ctx.value_types.insert(*dst, AotValueType::F32);
            Ok(true)
        }

        LirInst::ConstBool(dst, val) => {
            let v = builder.ins().iconst(types::I8, if *val { 1 } else { 0 });
            ctx.value_map.insert(*dst, v);
            ctx.value_types.insert(*dst, AotValueType::Bool);
            Ok(true)
        }

        LirInst::ConstNull(dst) => {
            let v = builder.ins().iconst(types::I64, 0);
            ctx.value_map.insert(*dst, v);
            ctx.value_types.insert(*dst, AotValueType::Ptr);
            Ok(true)
        }

        LirInst::ConstBigInt(dst, s) => {
            use num_traits::ToPrimitive;
            if let Some(u) = s.to_u64() {
                let v = builder.ins().iconst(types::I64, u as i64);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::U64);
            } else if let Some(i) = s.to_i64() {
                let v = builder.ins().iconst(types::I64, i);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::I64);
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                ctx.value_map.insert(*dst, v);
                ctx.value_types.insert(*dst, AotValueType::I64);
            }
            Ok(true)
        }

        LirInst::ConstString(dst, string_index) => {
            // String is already in string_data by index
            if let Some(&data_id) = ctx.string_indices.get(string_index) {
                // Will be used later to get the global pointer
                ctx.value_map
                    .insert(*dst, builder.ins().iconst(types::I64, *string_index as i64));
                ctx.value_types.insert(*dst, AotValueType::String);
                ctx.string_data_for_value.insert(*dst, data_id);
            } else {
                return Err(format!("String index {} not found", string_index));
            }
            Ok(true)
        }

        LirInst::ConstFunc(dst, _func_name) => {
            // Function pointers: placeholder for now
            let v = builder.ins().iconst(types::I64, 0);
            ctx.value_map.insert(*dst, v);
            ctx.value_types.insert(*dst, AotValueType::Ptr);
            Ok(true)
        }

        _ => Ok(false), // Not a const instruction
    }
}

/// Lower type conversion instructions
pub(super) fn lower_conversion_instruction(
    builder: &mut FunctionBuilder,
    ctx: &mut FunctionCompileContext,
    inst: &LirInst,
) -> Result<bool, String> {
    match inst {
        LirInst::I64ToF64(dst, src) => {
            let src_val = ctx
                .value_map
                .get(src)
                .copied()
                .ok_or_else(|| format!("Value {} not found", src))?;
            let v = builder.ins().fcvt_from_sint(types::F64, src_val);
            ctx.value_map.insert(*dst, v);
            ctx.value_types.insert(*dst, AotValueType::F64);
            Ok(true)
        }

        LirInst::F64ToI64(dst, src) => {
            let src_val = ctx
                .value_map
                .get(src)
                .copied()
                .ok_or_else(|| format!("Value {} not found", src))?;
            let v = builder.ins().fcvt_to_sint(types::I64, src_val);
            ctx.value_map.insert(*dst, v);
            ctx.value_types.insert(*dst, AotValueType::I64);
            Ok(true)
        }

        _ => Ok(false), // Not a conversion instruction
    }
}

/// Lower copy instruction
pub(super) fn lower_copy_instruction(
    _builder: &mut FunctionBuilder,
    ctx: &mut FunctionCompileContext,
    dst: &ValueId,
    src: &ValueId,
) -> Result<(), String> {
    if let Some(&v) = ctx.value_map.get(src) {
        ctx.value_map.insert(*dst, v);
        if let Some(ty) = ctx.value_types.get(src).cloned() {
            ctx.value_types.insert(*dst, ty);
        }
        // Copy string data reference if applicable
        if let Some(&data_id) = ctx.string_data_for_value.get(src) {
            ctx.string_data_for_value.insert(*dst, data_id);
        }
        // Copy object properties if applicable
        if let Some(props) = ctx.object_properties.get(src).cloned() {
            ctx.object_properties.insert(*dst, props);
        }
        // Copy constant information for print options
        // Copy const_bools if source is a boolean constant
        if let Some(bool_val) = ctx.const_bools.get(src).copied() {
            ctx.const_bools.insert(*dst, bool_val);
        }
        // Copy const_strings if source is a string constant
        if let Some(str_val) = ctx.const_strings.get(src).cloned() {
            ctx.const_strings.insert(*dst, str_val);
        }
    }
    Ok(())
}

/// Lower variable load/store instructions
pub(super) fn lower_var_instruction(
    builder: &mut FunctionBuilder,
    ctx: &mut FunctionCompileContext,
    inst: &LirInst,
) -> Result<bool, String> {
    match inst {
        LirInst::LoadVar(dst, var_name) => {
            let var = *ctx
                .var_map
                .get(var_name)
                .ok_or_else(|| format!("Variable {} not found", var_name))?;
            let val = builder.use_var(var);
            ctx.value_map.insert(*dst, val);
            if let Some(ty) = ctx.var_types.get(var_name).cloned() {
                ctx.value_types.insert(*dst, ty);
            }
            // Propagate object properties if stored for this variable
            if let Some(props) = ctx.var_object_properties.get(var_name).cloned() {
                ctx.object_properties.insert(*dst, props);
            }
            Ok(true)
        }

        LirInst::StoreVar(var_name, src) => {
            let val = ctx
                .value_map
                .get(src)
                .copied()
                .ok_or_else(|| format!("Value {} not found", src))?;
            let var = ctx.get_or_create_var(var_name.clone());
            builder.def_var(var, val);
            if let Some(ty) = ctx.value_types.get(src).cloned() {
                ctx.var_types.insert(var_name.clone(), ty);
            }
            // Propagate object properties to variable tracking
            if let Some(props) = ctx.object_properties.get(src).cloned() {
                ctx.var_object_properties.insert(var_name.clone(), props);
            }
            Ok(true)
        }

        _ => Ok(false), // Not a var instruction
    }
}

/// Lower module load instruction (placeholder)
pub(super) fn lower_load_module_instruction(
    builder: &mut FunctionBuilder,
    ctx: &mut FunctionCompileContext,
    dst: &ValueId,
    _module_name: &str,
) -> Result<(), String> {
    // Module loading not yet implemented; return null for now
    let v = builder.ins().iconst(types::I64, 0);
    ctx.value_map.insert(*dst, v);
    ctx.value_types.insert(*dst, AotValueType::Ptr);
    Ok(())
}
