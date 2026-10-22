//! Comparison operations for Cranelift backend
//!
//! This module handles all comparison instruction lowering, including:
//! - Integer comparisons (lt, le, gt, ge, eq, ne)
//! - Float comparisons (lt, le, gt, ge, eq, ne)

use cranelift::prelude::*;
use cranelift_codegen::ir::InstBuilder;
use cranelift_module::{Linkage, Module};
use std::collections::HashMap;

use crate::backends::aot::cranelift::AotValueType;
use crate::backends::aot::lir::{LirInst, ValueId};

fn to_f64_typed(
    builder: &mut FunctionBuilder,
    val: Value,
    val_type: Option<&AotValueType>,
) -> Value {
    let ty = builder.func.dfg.value_type(val);
    if ty == types::F64 {
        val
    } else if ty == types::F32 {
        builder.ins().fpromote(types::F64, val)
    } else if ty.is_int() {
        let is_unsigned = matches!(
            val_type,
            Some(
                AotValueType::U8
                    | AotValueType::U16
                    | AotValueType::U32
                    | AotValueType::U64
                    | AotValueType::U128
            )
        );
        if is_unsigned {
            builder.ins().fcvt_from_uint(types::F64, val)
        } else {
            builder.ins().fcvt_from_sint(types::F64, val)
        }
    } else {
        val
    }
}

fn ensure_i64(builder: &mut FunctionBuilder, val: Value) -> Value {
    let ty = builder.func.dfg.value_type(val);
    if ty == types::I64 {
        val
    } else if ty.is_int() {
        if ty.bits() < 64 {
            builder.ins().sextend(types::I64, val)
        } else {
            builder.ins().ireduce(types::I64, val)
        }
    } else {
        val
    }
}

/// Lower comparison instructions
///
/// Returns Ok(true) if the instruction was handled, Ok(false) if it wasn't a comparison,
/// or Err if an error occurred.
pub(crate) fn lower_comparison_instruction(
    module: &mut dyn Module,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<ValueId, Value>,
    value_types: &mut HashMap<ValueId, AotValueType>,
    inst: &LirInst,
) -> Result<bool, String> {
    match inst {
        // Integer comparisons
        LirInst::CmpLtI64(dst, a, b) => {
            let va = value_map
                .get(a)
                .copied()
                .ok_or_else(|| format!("Value {} not found", a))?;
            let vb = value_map
                .get(b)
                .copied()
                .ok_or_else(|| format!("Value {} not found", b))?;
            let va = ensure_i64(builder, va);
            let vb = ensure_i64(builder, vb);
            let v = builder.ins().icmp(IntCC::SignedLessThan, va, vb);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::Bool);
            Ok(true)
        }

        LirInst::CmpLeI64(dst, a, b) => {
            let va = value_map
                .get(a)
                .copied()
                .ok_or_else(|| format!("Value {} not found", a))?;
            let vb = value_map
                .get(b)
                .copied()
                .ok_or_else(|| format!("Value {} not found", b))?;
            let va = ensure_i64(builder, va);
            let vb = ensure_i64(builder, vb);
            let v = builder.ins().icmp(IntCC::SignedLessThanOrEqual, va, vb);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::Bool);
            Ok(true)
        }

        LirInst::CmpGtI64(dst, a, b) => {
            let va = value_map
                .get(a)
                .copied()
                .ok_or_else(|| format!("Value {} not found", a))?;
            let vb = value_map
                .get(b)
                .copied()
                .ok_or_else(|| format!("Value {} not found", b))?;
            let va = ensure_i64(builder, va);
            let vb = ensure_i64(builder, vb);
            let v = builder.ins().icmp(IntCC::SignedGreaterThan, va, vb);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::Bool);
            Ok(true)
        }

        LirInst::CmpGeI64(dst, a, b) => {
            let va = value_map
                .get(a)
                .copied()
                .ok_or_else(|| format!("Value {} not found", a))?;
            let vb = value_map
                .get(b)
                .copied()
                .ok_or_else(|| format!("Value {} not found", b))?;
            let va = ensure_i64(builder, va);
            let vb = ensure_i64(builder, vb);
            let v = builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, va, vb);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::Bool);
            Ok(true)
        }

        LirInst::CmpEqI64(dst, a, b) => {
            let va = value_map
                .get(a)
                .copied()
                .ok_or_else(|| format!("Value {} not found", a))?;
            let vb = value_map
                .get(b)
                .copied()
                .ok_or_else(|| format!("Value {} not found", b))?;
            let va = ensure_i64(builder, va);
            let vb = ensure_i64(builder, vb);
            let type_a = value_types.get(a).cloned().unwrap_or(AotValueType::Unknown);
            let type_b = value_types.get(b).cloned().unwrap_or(AotValueType::Unknown);

            let is_str_type = |t: &AotValueType| {
                matches!(
                    t,
                    AotValueType::String
                        | AotValueType::Ptr
                        | AotValueType::Handle
                        | AotValueType::Unknown
                )
            };

            let v = if is_str_type(&type_a) || is_str_type(&type_b) {
                let mut eq_sig = module.make_signature();
                eq_sig.params.push(AbiParam::new(types::I64));
                eq_sig.params.push(AbiParam::new(types::I64));
                eq_sig.returns.push(AbiParam::new(types::I64));
                let eq_func = module
                    .declare_function("aot_value_eq", Linkage::Import, &eq_sig)
                    .map_err(|e| format!("Failed to declare aot_value_eq: {}", e))?;
                let local_callee = module.declare_func_in_func(eq_func, builder.func);
                let call_inst = builder.ins().call(local_callee, &[va, vb]);
                let res = builder.inst_results(call_inst)[0];
                builder.ins().icmp_imm(IntCC::NotEqual, res, 0)
            } else {
                builder.ins().icmp(IntCC::Equal, va, vb)
            };
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::Bool);
            Ok(true)
        }

        LirInst::CmpNeI64(dst, a, b) => {
            let va = value_map
                .get(a)
                .copied()
                .ok_or_else(|| format!("Value {} not found", a))?;
            let vb = value_map
                .get(b)
                .copied()
                .ok_or_else(|| format!("Value {} not found", b))?;
            let va = ensure_i64(builder, va);
            let vb = ensure_i64(builder, vb);
            let type_a = value_types.get(a).cloned().unwrap_or(AotValueType::Unknown);
            let type_b = value_types.get(b).cloned().unwrap_or(AotValueType::Unknown);

            let is_str_type = |t: &AotValueType| {
                matches!(
                    t,
                    AotValueType::String
                        | AotValueType::Ptr
                        | AotValueType::Handle
                        | AotValueType::Unknown
                )
            };

            let v = if is_str_type(&type_a) || is_str_type(&type_b) {
                let mut eq_sig = module.make_signature();
                eq_sig.params.push(AbiParam::new(types::I64));
                eq_sig.params.push(AbiParam::new(types::I64));
                eq_sig.returns.push(AbiParam::new(types::I64));
                let eq_func = module
                    .declare_function("aot_value_eq", Linkage::Import, &eq_sig)
                    .map_err(|e| format!("Failed to declare aot_value_eq: {}", e))?;
                let local_callee = module.declare_func_in_func(eq_func, builder.func);
                let call_inst = builder.ins().call(local_callee, &[va, vb]);
                let res = builder.inst_results(call_inst)[0];
                builder.ins().icmp_imm(IntCC::Equal, res, 0)
            } else {
                builder.ins().icmp(IntCC::NotEqual, va, vb)
            };
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::Bool);
            Ok(true)
        }

        // Float comparisons
        LirInst::CmpLtF64(dst, a, b) => {
            let va = value_map
                .get(a)
                .copied()
                .ok_or_else(|| format!("Value {} not found", a))?;
            let vb = value_map
                .get(b)
                .copied()
                .ok_or_else(|| format!("Value {} not found", b))?;
            let fa = to_f64_typed(builder, va, value_types.get(a));
            let fb = to_f64_typed(builder, vb, value_types.get(b));
            let v = builder.ins().fcmp(FloatCC::LessThan, fa, fb);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::Bool);
            Ok(true)
        }

        LirInst::CmpLeF64(dst, a, b) => {
            let va = value_map
                .get(a)
                .copied()
                .ok_or_else(|| format!("Value {} not found", a))?;
            let vb = value_map
                .get(b)
                .copied()
                .ok_or_else(|| format!("Value {} not found", b))?;
            let fa = to_f64_typed(builder, va, value_types.get(a));
            let fb = to_f64_typed(builder, vb, value_types.get(b));
            let v = builder.ins().fcmp(FloatCC::LessThanOrEqual, fa, fb);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::Bool);
            Ok(true)
        }

        LirInst::CmpGtF64(dst, a, b) => {
            let va = value_map
                .get(a)
                .copied()
                .ok_or_else(|| format!("Value {} not found", a))?;
            let vb = value_map
                .get(b)
                .copied()
                .ok_or_else(|| format!("Value {} not found", b))?;
            let fa = to_f64_typed(builder, va, value_types.get(a));
            let fb = to_f64_typed(builder, vb, value_types.get(b));
            let v = builder.ins().fcmp(FloatCC::GreaterThan, fa, fb);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::Bool);
            Ok(true)
        }

        LirInst::CmpGeF64(dst, a, b) => {
            let va = value_map
                .get(a)
                .copied()
                .ok_or_else(|| format!("Value {} not found", a))?;
            let vb = value_map
                .get(b)
                .copied()
                .ok_or_else(|| format!("Value {} not found", b))?;
            let fa = to_f64_typed(builder, va, value_types.get(a));
            let fb = to_f64_typed(builder, vb, value_types.get(b));
            let v = builder.ins().fcmp(FloatCC::GreaterThanOrEqual, fa, fb);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::Bool);
            Ok(true)
        }

        LirInst::CmpEqF64(dst, a, b) => {
            let va = value_map
                .get(a)
                .copied()
                .ok_or_else(|| format!("Value {} not found", a))?;
            let vb = value_map
                .get(b)
                .copied()
                .ok_or_else(|| format!("Value {} not found", b))?;
            let fa = to_f64_typed(builder, va, value_types.get(a));
            let fb = to_f64_typed(builder, vb, value_types.get(b));
            let v = builder.ins().fcmp(FloatCC::Equal, fa, fb);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::Bool);
            Ok(true)
        }

        LirInst::CmpNeF64(dst, a, b) => {
            let va = value_map
                .get(a)
                .copied()
                .ok_or_else(|| format!("Value {} not found", a))?;
            let vb = value_map
                .get(b)
                .copied()
                .ok_or_else(|| format!("Value {} not found", b))?;
            let fa = to_f64_typed(builder, va, value_types.get(a));
            let fb = to_f64_typed(builder, vb, value_types.get(b));
            let v = builder.ins().fcmp(FloatCC::NotEqual, fa, fb);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::Bool);
            Ok(true)
        }

        _ => Ok(false), // Not a comparison instruction
    }
}
