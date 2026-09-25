//! Constant value handling for Cranelift backend
//!
//! This module handles all constant instruction lowering, including:
//! - Integer constants (I8, I16, I32, I64, I128, U8, U16, U32, U64, U128)
//! - Float constants (F32, F64)
//! - Boolean constants
//! - String constants
//! - BigInt constants (with limited support)
//! - Null constants
//! - Function constants

use cranelift::prelude::*;
use cranelift_codegen::ir::InstBuilder;
use cranelift_module::{DataId, Module};
use std::collections::HashMap;

use crate::backends::aot::cranelift::AotValueType;
use crate::backends::aot::lir::{LirInst, ValueId};
use cranelift_module::FuncId;

/// Lower constant instructions
///
/// Returns Ok(true) if the instruction was handled, Ok(false) if it wasn't a constant,
/// or Err if an error occurred.
pub(crate) fn lower_constant_instruction(
    module: &mut impl Module,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<ValueId, Value>,
    value_types: &mut HashMap<ValueId, AotValueType>,
    const_ints: &mut HashMap<ValueId, i64>,
    const_bools: &mut HashMap<ValueId, bool>,
    const_strings: &mut HashMap<ValueId, String>,
    string_data: &HashMap<String, DataId>,
    string_pool: &[String],
    user_funcs: &HashMap<String, FuncId>,
    func_values: &mut HashMap<ValueId, FuncId>,
    inst: &LirInst,
) -> Result<bool, String> {
    match inst {
        // I64 constant
        LirInst::ConstI64(dst, val) => {
            let v = builder.ins().iconst(types::I64, *val);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::I64);
            const_ints.insert(*dst, *val);
            Ok(true)
        }

        // F64 constant
        LirInst::ConstF64(dst, val) => {
            let v = builder.ins().f64const(*val);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::F64);
            Ok(true)
        }

        // Fixed-width unsigned integer constants
        LirInst::ConstU8(dst, val) => {
            let v = builder.ins().iconst(types::I64, *val as i64);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::U8);
            const_ints.insert(*dst, *val as i64);
            Ok(true)
        }
        LirInst::ConstU16(dst, val) => {
            let v = builder.ins().iconst(types::I64, *val as i64);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::U16);
            const_ints.insert(*dst, *val as i64);
            Ok(true)
        }
        LirInst::ConstU32(dst, val) => {
            let v = builder.ins().iconst(types::I64, *val as i64);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::U32);
            const_ints.insert(*dst, *val as i64);
            Ok(true)
        }
        LirInst::ConstU64(dst, val) => {
            let v = builder.ins().iconst(types::I64, *val as i64);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::U64);
            const_ints.insert(*dst, *val as i64);
            Ok(true)
        }
        LirInst::ConstU128(dst, val) => {
            // u128 stored as i64 (precision loss for large values)
            let v = builder.ins().iconst(types::I64, *val as i64);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::U128);
            const_ints.insert(*dst, *val as i64);
            Ok(true)
        }

        // Fixed-width signed integer constants
        LirInst::ConstI8(dst, val) => {
            let v = builder.ins().iconst(types::I64, *val as i64);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::I8);
            const_ints.insert(*dst, *val as i64);
            Ok(true)
        }
        LirInst::ConstI16(dst, val) => {
            let v = builder.ins().iconst(types::I64, *val as i64);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::I16);
            const_ints.insert(*dst, *val as i64);
            Ok(true)
        }
        LirInst::ConstI32(dst, val) => {
            let v = builder.ins().iconst(types::I64, *val as i64);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::I32);
            const_ints.insert(*dst, *val as i64);
            Ok(true)
        }
        LirInst::ConstI128(dst, val) => {
            // i128 stored as i64 (precision loss for large values)
            let v = builder.ins().iconst(types::I64, *val as i64);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::I128);
            const_ints.insert(*dst, *val as i64);
            Ok(true)
        }

        // Fixed-width float constants
        LirInst::ConstF32(dst, val) => {
            let v = builder.ins().f64const(*val as f64);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::F32);
            Ok(true)
        }

        // Boolean constant
        LirInst::ConstBool(dst, val) => {
            let v = builder.ins().iconst(types::I8, if *val { 1 } else { 0 });
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::Bool);
            // Track constant booleans for print options
            const_bools.insert(*dst, *val);
            Ok(true)
        }

        // String constant
        LirInst::ConstString(dst, s) => {
            // Get the data reference for this string
            if let Some(&data_id) = string_data.get(s) {
                let gv = module.declare_data_in_func(data_id, builder.func);
                let v = builder.ins().global_value(types::I64, gv);
                value_map.insert(*dst, v);
                value_types.insert(*dst, AotValueType::String);
            } else {
                use std::sync::atomic::{AtomicUsize, Ordering};
                static ANON_STR_COUNTER: AtomicUsize = AtomicUsize::new(0);
                let count = ANON_STR_COUNTER.fetch_add(1, Ordering::Relaxed);
                let safe_name = format!("__aot_str_anon_{}", count);
                let mut data: Vec<u8> = s.as_bytes().iter().filter(|&&b| b != 0).copied().collect();
                data.push(0);
                if let Ok(data_id) =
                    module.declare_data(&safe_name, cranelift_module::Linkage::Local, false, false)
                {
                    let mut desc = cranelift_module::DataDescription::new();
                    desc.define(data.into_boxed_slice());
                    let _ = module.define_data(data_id, &desc);
                    let gv = module.declare_data_in_func(data_id, builder.func);
                    let v = builder.ins().global_value(types::I64, gv);
                    value_map.insert(*dst, v);
                    value_types.insert(*dst, AotValueType::String);
                } else {
                    let v = builder.ins().iconst(types::I64, 0);
                    value_map.insert(*dst, v);
                    value_types.insert(*dst, AotValueType::String);
                }
            }
            // Track constant strings for print options
            const_strings.insert(*dst, s.clone());
            Ok(true)
        }

        // BigInt constant (limited support)
        LirInst::ConstBigInt(dst, big) => {
            use num_traits::ToPrimitive;
            if let Some(u) = big.to_u64() {
                let v = builder.ins().iconst(types::I64, u as i64);
                value_map.insert(*dst, v);
                value_types.insert(*dst, AotValueType::U64);
            } else if let Some(i) = big.to_i64() {
                let v = builder.ins().iconst(types::I64, i);
                value_map.insert(*dst, v);
                value_types.insert(*dst, AotValueType::I64);
            } else {
                let v = builder.ins().iconst(types::I64, 0);
                value_map.insert(*dst, v);
                value_types.insert(*dst, AotValueType::I64);
            }
            Ok(true)
        }

        // Null constant
        LirInst::ConstNull(dst) => {
            let v = builder.ins().iconst(types::I64, 0);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::Ptr);
            Ok(true)
        }

        // Function constant
        LirInst::ConstFunc(dst, _name, _captures, _is_async) => {
            // Track function reference for callback usage
            // Store func id mapping if known
            if let Some(func_id) = user_funcs.get(_name) {
                func_values.insert(*dst, *func_id);
            }
            let v = builder.ins().iconst(types::I64, 0);
            value_map.insert(*dst, v);
            Ok(true)
        }

        _ => Ok(false), // Not a constant instruction
    }
}
