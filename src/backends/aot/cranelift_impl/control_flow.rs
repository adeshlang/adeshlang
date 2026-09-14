//! Control flow operations for Cranelift backend
//!
//! This module handles control flow instruction lowering, including:
//! - Unconditional jumps (Jump)
//! - Conditional branches (JumpIf)
//! - Return statements (Return)
//! - Phi nodes (Phi)

use cranelift::prelude::*;
use cranelift_codegen::ir::InstBuilder;
use std::collections::HashMap;

use crate::backends::aot::lir::{BlockId, LirInst, ValueId};

/// Lower control flow instructions
///
/// Returns Ok(true) if the instruction was handled, Ok(false) if it wasn't a control flow instruction,
/// or Err if an error occurred.
pub(crate) fn lower_control_flow_instruction(
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<ValueId, Value>,
    block_map: &HashMap<BlockId, Block>,
    inst: &LirInst,
) -> Result<bool, String> {
    match inst {
        // Unconditional jump
        LirInst::Jump(target) => {
            let target_block = block_map
                .get(target)
                .copied()
                .ok_or_else(|| format!("Block {} not found", target))?;
            builder.ins().jump(target_block, &[]);
            Ok(true)
        }

        // Conditional branch
        LirInst::JumpIf(cond, then_block, else_block) => {
            let cond_val = value_map
                .get(cond)
                .copied()
                .ok_or_else(|| format!("Value {} not found", cond))?;
            let then_cl = block_map
                .get(then_block)
                .copied()
                .ok_or_else(|| format!("Block {} not found", then_block))?;
            let else_cl = block_map
                .get(else_block)
                .copied()
                .ok_or_else(|| format!("Block {} not found", else_block))?;
            builder.ins().brif(cond_val, then_cl, &[], else_cl, &[]);
            Ok(true)
        }

        // Return statement
        LirInst::Return(val) => {
            // Ensure return type matches function signature
            let expected_ty = builder
                .func
                .signature
                .returns
                .get(0)
                .map(|abi| abi.value_type)
                .unwrap_or(types::I64);

            let ret_val = if let Some(v) = val {
                let rv = value_map
                    .get(v)
                    .copied()
                    .ok_or_else(|| format!("Value {} not found", v))?;
                let actual_ty = builder.func.dfg.value_type(rv);

                // Convert return value to expected type if needed
                if actual_ty == expected_ty {
                    rv
                } else {
                    match (expected_ty, actual_ty) {
                        (types::I64, t) if t.is_int() && t.bits() < 64 => {
                            builder.ins().uextend(types::I64, rv)
                        }
                        (types::I8, t) if t.is_int() && t.bits() > 8 => {
                            builder.ins().ireduce(types::I8, rv)
                        }
                        (types::F64, t) if t.is_int() => {
                            builder.ins().fcvt_from_sint(types::F64, rv)
                        }
                        (types::F32, types::F64) => builder.ins().fdemote(types::F32, rv),
                        (types::I64, t) if t == types::F64 || t == types::F32 => {
                            builder.ins().fcvt_to_uint(types::I64, rv)
                        }
                        _ => rv,
                    }
                }
            } else {
                // No return value provided, use zero/default for expected type
                match expected_ty {
                    t if t == types::I8 => builder.ins().iconst(types::I8, 0),
                    t if t == types::I64 => builder.ins().iconst(types::I64, 0),
                    t if t == types::F64 => builder.ins().f64const(0.0),
                    t if t == types::F32 => builder.ins().f32const(0.0),
                    _ => builder.ins().iconst(types::I64, 0),
                }
            };
            builder.ins().return_(&[ret_val]);
            Ok(true)
        }

        // Phi node
        LirInst::Phi(dst, sources) => {
            // Phi nodes are handled by block parameters in Cranelift
            // For simplicity, use the first available value
            if let Some((_, val)) = sources.first() {
                if let Some(&v) = value_map.get(val) {
                    value_map.insert(*dst, v);
                } else {
                    let zero = builder.ins().iconst(types::I64, 0);
                    value_map.insert(*dst, zero);
                }
            } else {
                let zero = builder.ins().iconst(types::I64, 0);
                value_map.insert(*dst, zero);
            }
            Ok(true)
        }

        _ => Ok(false), // Not a control flow instruction
    }
}
