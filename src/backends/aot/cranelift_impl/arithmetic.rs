//! Arithmetic instruction lowering for Cranelift AOT backend
//!
//! This module handles translation of LIR arithmetic instructions into Cranelift IR.
//! Supports integer arithmetic, floating-point operations, logical operations,
//! and bitwise operations.
//!
//! # Supported Operations
//!
//! ## Integer Arithmetic (I64)
//! - `AddI64`, `SubI64`, `MulI64`, `DivI64`, `ModI64`
//! - `NegI64`
//!
//! ## Floating-Point Arithmetic (F64)
//! - `AddF64`, `SubF64`, `MulF64`, `DivF64`
//! - `NegF64`
//!
//! ## Logical Operations (Boolean)
//! - `And`, `Or`, `Not`
//!
//! ## Bitwise Operations
//! - `BitAnd`, `BitOr`, `BitXor`
//! - `Shl` (Shift Left), `Shr` (Shift Right)

use cranelift::prelude::*;
use std::collections::HashMap;

use crate::backends::aot::cranelift::AotValueType;
use crate::backends::aot::lir::{LirInst, ValueId};

/// Lower arithmetic instructions to Cranelift IR
///
/// # Arguments
///
/// * `builder` - Cranelift function builder for emitting instructions
/// * `value_map` - Map from LIR ValueId to Cranelift Value
/// * `value_types` - Map from LIR ValueId to runtime type information
/// * `inst` - The LIR instruction to lower
///
/// # Returns
///
/// * `Ok(true)` - Instruction was an arithmetic operation and was successfully handled
/// * `Ok(false)` - Instruction is not an arithmetic operation
/// * `Err(String)` - An error occurred during lowering
pub fn lower_arithmetic_instruction(
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<ValueId, Value>,
    value_types: &mut HashMap<ValueId, AotValueType>,
    inst: &LirInst,
) -> Result<bool, String> {
    match inst {
        // Integer arithmetic
        LirInst::AddI64(dst, a, b) => {
            let va = value_map
                .get(a)
                .copied()
                .ok_or_else(|| format!("Value {} not found", a))?;
            let vb = value_map
                .get(b)
                .copied()
                .ok_or_else(|| format!("Value {} not found", b))?;
            let v = builder.ins().iadd(va, vb);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::I64);
            Ok(true)
        }

        LirInst::SubI64(dst, a, b) => {
            let va = value_map
                .get(a)
                .copied()
                .ok_or_else(|| format!("Value {} not found", a))?;
            let vb = value_map
                .get(b)
                .copied()
                .ok_or_else(|| format!("Value {} not found", b))?;
            let v = builder.ins().isub(va, vb);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::I64);
            Ok(true)
        }

        LirInst::MulI64(dst, a, b) => {
            let va = value_map
                .get(a)
                .copied()
                .ok_or_else(|| format!("Value {} not found", a))?;
            let vb = value_map
                .get(b)
                .copied()
                .ok_or_else(|| format!("Value {} not found", b))?;
            let v = builder.ins().imul(va, vb);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::I64);
            Ok(true)
        }

        LirInst::DivI64(dst, a, b) => {
            let va = value_map
                .get(a)
                .copied()
                .ok_or_else(|| format!("Value {} not found", a))?;
            let vb = value_map
                .get(b)
                .copied()
                .ok_or_else(|| format!("Value {} not found", b))?;
            let v = builder.ins().sdiv(va, vb);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::I64);
            Ok(true)
        }

        LirInst::ModI64(dst, a, b) => {
            let va = value_map
                .get(a)
                .copied()
                .ok_or_else(|| format!("Value {} not found", a))?;
            let vb = value_map
                .get(b)
                .copied()
                .ok_or_else(|| format!("Value {} not found", b))?;
            let v = builder.ins().srem(va, vb);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::I64);
            Ok(true)
        }

        LirInst::NegI64(dst, src) => {
            let vs = value_map
                .get(src)
                .copied()
                .ok_or_else(|| format!("Value {} not found", src))?;
            let v = builder.ins().ineg(vs);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::I64);
            Ok(true)
        }

        // Float arithmetic
        LirInst::AddF64(dst, a, b) => {
            let va = value_map
                .get(a)
                .copied()
                .ok_or_else(|| format!("Value {} not found", a))?;
            let vb = value_map
                .get(b)
                .copied()
                .ok_or_else(|| format!("Value {} not found", b))?;
            let v = builder.ins().fadd(va, vb);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::F64);
            Ok(true)
        }

        LirInst::SubF64(dst, a, b) => {
            let va = value_map
                .get(a)
                .copied()
                .ok_or_else(|| format!("Value {} not found", a))?;
            let vb = value_map
                .get(b)
                .copied()
                .ok_or_else(|| format!("Value {} not found", b))?;
            let v = builder.ins().fsub(va, vb);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::F64);
            Ok(true)
        }

        LirInst::MulF64(dst, a, b) => {
            let va = value_map
                .get(a)
                .copied()
                .ok_or_else(|| format!("Value {} not found", a))?;
            let vb = value_map
                .get(b)
                .copied()
                .ok_or_else(|| format!("Value {} not found", b))?;
            let v = builder.ins().fmul(va, vb);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::F64);
            Ok(true)
        }

        LirInst::DivF64(dst, a, b) => {
            let va = value_map
                .get(a)
                .copied()
                .ok_or_else(|| format!("Value {} not found", a))?;
            let vb = value_map
                .get(b)
                .copied()
                .ok_or_else(|| format!("Value {} not found", b))?;
            let v = builder.ins().fdiv(va, vb);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::F64);
            Ok(true)
        }

        LirInst::NegF64(dst, src) => {
            let vs = value_map
                .get(src)
                .copied()
                .ok_or_else(|| format!("Value {} not found", src))?;
            let v = builder.ins().fneg(vs);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::F64);
            Ok(true)
        }

        // Boolean operations
        LirInst::Not(dst, src) => {
            let vs = value_map
                .get(src)
                .copied()
                .ok_or_else(|| format!("Value {} not found", src))?;
            let one = builder.ins().iconst(types::I8, 1);
            let v = builder.ins().bxor(vs, one);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::Bool);
            Ok(true)
        }

        LirInst::And(dst, a, b) => {
            let va = value_map
                .get(a)
                .copied()
                .ok_or_else(|| format!("Value {} not found", a))?;
            let vb = value_map
                .get(b)
                .copied()
                .ok_or_else(|| format!("Value {} not found", b))?;
            let v = builder.ins().band(va, vb);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::Bool);
            Ok(true)
        }

        LirInst::Or(dst, a, b) => {
            let va = value_map
                .get(a)
                .copied()
                .ok_or_else(|| format!("Value {} not found", a))?;
            let vb = value_map
                .get(b)
                .copied()
                .ok_or_else(|| format!("Value {} not found", b))?;
            let v = builder.ins().bor(va, vb);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::Bool);
            Ok(true)
        }

        // Bitwise operations
        LirInst::BitAnd(dst, a, b) => {
            let va = value_map
                .get(a)
                .copied()
                .ok_or_else(|| format!("Value {} not found", a))?;
            let vb = value_map
                .get(b)
                .copied()
                .ok_or_else(|| format!("Value {} not found", b))?;
            let v = builder.ins().band(va, vb);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::I64);
            Ok(true)
        }

        LirInst::BitOr(dst, a, b) => {
            let va = value_map
                .get(a)
                .copied()
                .ok_or_else(|| format!("Value {} not found", a))?;
            let vb = value_map
                .get(b)
                .copied()
                .ok_or_else(|| format!("Value {} not found", b))?;
            let v = builder.ins().bor(va, vb);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::I64);
            Ok(true)
        }

        LirInst::BitXor(dst, a, b) => {
            let va = value_map
                .get(a)
                .copied()
                .ok_or_else(|| format!("Value {} not found", a))?;
            let vb = value_map
                .get(b)
                .copied()
                .ok_or_else(|| format!("Value {} not found", b))?;
            let v = builder.ins().bxor(va, vb);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::I64);
            Ok(true)
        }

        LirInst::Shl(dst, a, b) => {
            let va = value_map
                .get(a)
                .copied()
                .ok_or_else(|| format!("Value {} not found", a))?;
            let vb = value_map
                .get(b)
                .copied()
                .ok_or_else(|| format!("Value {} not found", b))?;
            let v = builder.ins().ishl(va, vb);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::I64);
            Ok(true)
        }

        LirInst::Shr(dst, a, b) => {
            let va = value_map
                .get(a)
                .copied()
                .ok_or_else(|| format!("Value {} not found", a))?;
            let vb = value_map
                .get(b)
                .copied()
                .ok_or_else(|| format!("Value {} not found", b))?;
            let v = builder.ins().sshr(va, vb);
            value_map.insert(*dst, v);
            value_types.insert(*dst, AotValueType::I64);
            Ok(true)
        }

        // Not an arithmetic instruction
        _ => Ok(false),
    }
}
