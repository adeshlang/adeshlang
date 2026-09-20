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
//!
//! Fixed-width integer operands keep their lane width: results are masked
//! (unsigned) or sign-extended (signed), `Shr` selects the arithmetic or
//! logical native instruction from operand signedness, and out-of-range shift
//! counts trap instead of relying on undefined native shift behavior.

use cranelift::prelude::*;
use std::collections::HashMap;

use crate::backends::aot::cranelift::AotValueType;
use crate::backends::aot::lir::{LirInst, ValueId};

/// Fixed-width lane info for native bitwise lowering: `(bits, is_signed)`.
///
/// `None` for 128-bit and untracked values, which keep the legacy 64-bit path.
fn native_width(ty: Option<&AotValueType>) -> Option<(u32, bool)> {
    match ty? {
        AotValueType::U8 => Some((8, false)),
        AotValueType::U16 => Some((16, false)),
        AotValueType::U32 => Some((32, false)),
        AotValueType::U64 => Some((64, false)),
        AotValueType::I8 => Some((8, true)),
        AotValueType::I16 => Some((16, true)),
        AotValueType::I32 => Some((32, true)),
        AotValueType::I64 | AotValueType::Int => Some((64, true)),
        _ => None,
    }
}

/// Mask a value to its fixed width (no-op on 64-bit lanes).
fn mask_to_width(builder: &mut FunctionBuilder, v: Value, width: u32) -> Value {
    if width >= 64 {
        v
    } else {
        let m = builder
            .ins()
            .iconst(types::I64, ((1u64 << width) - 1) as i64);
        builder.ins().band(v, m)
    }
}

/// Sign-extend the low `width` bits into the full 64-bit lane (no-op at 64).
fn sign_extend(builder: &mut FunctionBuilder, v: Value, width: u32) -> Value {
    if width >= 64 {
        v
    } else {
        let up = builder.ins().iconst(types::I64, (64 - width) as i64);
        let shifted = builder.ins().ishl(v, up);
        builder.ins().sshr(shifted, up)
    }
}

/// Out-of-range shift counts trap instead of producing undefined native shifts.
fn trap_shift_overflow(builder: &mut FunctionBuilder, count: Value, width: u32) {
    let limit = builder.ins().iconst(types::I64, width as i64);
    let overflow = builder.ins().icmp(
        cranelift_codegen::ir::condcodes::IntCC::UnsignedGreaterThanOrEqual,
        count,
        limit,
    );
    builder
        .ins()
        .trapnz(overflow, cranelift_codegen::ir::TrapCode::IntegerOverflow);
}

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

        // Bitwise operations. Lowered directly to native AND/OR/XOR/shift
        // instructions; lanes narrower than 64 bits are masked (unsigned) or
        // sign-extended (signed) so results keep the operand width.
        LirInst::BitAnd(dst, a, b) => {
            let va = value_map
                .get(a)
                .copied()
                .ok_or_else(|| format!("Value {} not found", a))?;
            let vb = value_map
                .get(b)
                .copied()
                .ok_or_else(|| format!("Value {} not found", b))?;
            let (width, signed) = native_width(value_types.get(a)).unwrap_or((64, true));
            let tag = value_types.get(a).cloned().unwrap_or(AotValueType::I64);
            let v = builder.ins().band(va, vb);
            let v = if signed {
                let masked = mask_to_width(builder, v, width);
                sign_extend(builder, masked, width)
            } else {
                mask_to_width(builder, v, width)
            };
            value_map.insert(*dst, v);
            value_types.insert(*dst, tag);
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
            let (width, signed) = native_width(value_types.get(a)).unwrap_or((64, true));
            let tag = value_types.get(a).cloned().unwrap_or(AotValueType::I64);
            let v = builder.ins().bor(va, vb);
            let v = if signed {
                let masked = mask_to_width(builder, v, width);
                sign_extend(builder, masked, width)
            } else {
                mask_to_width(builder, v, width)
            };
            value_map.insert(*dst, v);
            value_types.insert(*dst, tag);
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
            let (width, signed) = native_width(value_types.get(a)).unwrap_or((64, true));
            let tag = value_types.get(a).cloned().unwrap_or(AotValueType::I64);
            let v = builder.ins().bxor(va, vb);
            let v = if signed {
                let masked = mask_to_width(builder, v, width);
                sign_extend(builder, masked, width)
            } else {
                mask_to_width(builder, v, width)
            };
            value_map.insert(*dst, v);
            value_types.insert(*dst, tag);
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
            let (width, signed) = native_width(value_types.get(a)).unwrap_or((64, true));
            let tag = value_types.get(a).cloned().unwrap_or(AotValueType::I64);
            trap_shift_overflow(builder, vb, width);
            let v = builder.ins().ishl(va, vb);
            let v = if signed {
                let masked = mask_to_width(builder, v, width);
                sign_extend(builder, masked, width)
            } else {
                mask_to_width(builder, v, width)
            };
            value_map.insert(*dst, v);
            value_types.insert(*dst, tag);
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
            let (width, signed) = native_width(value_types.get(a)).unwrap_or((64, true));
            let tag = value_types.get(a).cloned().unwrap_or(AotValueType::I64);
            trap_shift_overflow(builder, vb, width);
            let v = if signed {
                // Arithmetic shift: sign-extend narrow lanes so the sign bit
                // sits at bit 63, then shift.
                let sx = sign_extend(builder, va, width);
                builder.ins().sshr(sx, vb)
            } else {
                // Logical shift; narrow lanes mask afterwards to drop any
                // garbage pulled in from above the lane width.
                let shifted = builder.ins().ushr(va, vb);
                mask_to_width(builder, shifted, width)
            };
            value_map.insert(*dst, v);
            value_types.insert(*dst, tag);
            Ok(true)
        }

        // Not an arithmetic instruction
        _ => Ok(false),
    }
}
