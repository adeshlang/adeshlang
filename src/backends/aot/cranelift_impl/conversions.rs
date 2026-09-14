//! Type conversion operations for Cranelift backend
//!
//! This module handles all type conversion instruction lowering, including:
//! - Integer to float conversions (I64ToF64)
//! - Float to integer conversions (F64ToI64)

use cranelift::prelude::*;
use cranelift_codegen::ir::InstBuilder;
use std::collections::HashMap;

use crate::backends::aot::lir::{LirInst, ValueId};

/// Lower type conversion instructions
///
/// Returns Ok(true) if the instruction was handled, Ok(false) if it wasn't a conversion,
/// or Err if an error occurred.
pub(crate) fn lower_conversion_instruction(
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<ValueId, Value>,
    inst: &LirInst,
) -> Result<bool, String> {
    match inst {
        // Integer to float conversion
        LirInst::I64ToF64(dst, src) => {
            let vs = value_map
                .get(src)
                .copied()
                .ok_or_else(|| format!("Value {} not found", src))?;
            let v = builder.ins().fcvt_from_sint(types::F64, vs);
            value_map.insert(*dst, v);
            Ok(true)
        }

        // Float to integer conversion
        LirInst::F64ToI64(dst, src) => {
            let vs = value_map
                .get(src)
                .copied()
                .ok_or_else(|| format!("Value {} not found", src))?;
            let v = builder.ins().fcvt_to_sint(types::I64, vs);
            value_map.insert(*dst, v);
            Ok(true)
        }

        _ => Ok(false), // Not a conversion instruction
    }
}
