//! Comparison operations for Cranelift backend
//!
//! This module handles all comparison instruction lowering, including:
//! - Integer comparisons (lt, le, gt, ge, eq, ne)
//! - Float comparisons (lt, le, gt, ge, eq, ne)

use cranelift::prelude::*;
use cranelift_codegen::ir::InstBuilder;
use std::collections::HashMap;

use crate::backends::aot::cranelift::AotValueType;
use crate::backends::aot::lir::{LirInst, ValueId};

/// Lower comparison instructions
///
/// Returns Ok(true) if the instruction was handled, Ok(false) if it wasn't a comparison,
/// or Err if an error occurred.
pub(crate) fn lower_comparison_instruction(
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
            let v = builder.ins().icmp(IntCC::Equal, va, vb);
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
            let v = builder.ins().icmp(IntCC::NotEqual, va, vb);
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
            let v = builder.ins().fcmp(FloatCC::LessThan, va, vb);
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
            let v = builder.ins().fcmp(FloatCC::LessThanOrEqual, va, vb);
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
            let v = builder.ins().fcmp(FloatCC::GreaterThan, va, vb);
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
            let v = builder.ins().fcmp(FloatCC::GreaterThanOrEqual, va, vb);
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
            let v = builder.ins().fcmp(FloatCC::Equal, va, vb);
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
            let v = builder.ins().fcmp(FloatCC::NotEqual, va, vb);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::Bool);
            Ok(true)
        }

        _ => Ok(false), // Not a comparison instruction
    }
}

