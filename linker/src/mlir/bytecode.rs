//! Self-contained MLIR Bytecode encoder and decoder.
//!
//! Encodes and decodes MLIR module structures into a compact, relocatable bytecode stream
//! (`ML\xEF\x52` container) ready for binary section embedding (`.mlir.bytecode`).

use super::dialect::{DialectKind, DialectOp, PolyhedralSchedule, TensorShape};
use crate::error::{ErrorCode, LinkError, LinkResult};
use std::collections::HashMap;

/// MLIR Bytecode Magic Header: `MLïR` (0x4D, 0x4C, 0xEF, 0x52)
pub const MLIR_BYTECODE_MAGIC: [u8; 4] = [0x4D, 0x4C, 0xEF, 0x52];
pub const MLIR_BYTECODE_VERSION: u32 = 1;

/// In-memory structured MLIR Module container.
#[derive(Debug, Clone)]
pub struct MlirModule {
    pub name: String,
    pub operations: Vec<DialectOp>,
    pub schedules: HashMap<String, PolyhedralSchedule>,
    pub tensor_constants: HashMap<String, Vec<u8>>,
}

impl MlirModule {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            operations: Vec::new(),
            schedules: HashMap::new(),
            tensor_constants: HashMap::new(),
        }
    }
}

/// Binary encoder for MLIR bytecode streams.
pub struct MlirBytecodeWriter;

impl MlirBytecodeWriter {
    pub fn encode(module: &MlirModule) -> LinkResult<Vec<u8>> {
        let mut buf = Vec::new();

        // 1. Header (8 bytes)
        buf.extend_from_slice(&MLIR_BYTECODE_MAGIC);
        buf.extend_from_slice(&MLIR_BYTECODE_VERSION.to_le_bytes());

        // 2. Module Name
        let name_bytes = module.name.as_bytes();
        buf.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(name_bytes);

        // 3. Operations Count
        buf.extend_from_slice(&(module.operations.len() as u32).to_le_bytes());

        // 4. Operations Body
        for op in &module.operations {
            let dialect_str = op.dialect.name().as_bytes();
            buf.extend_from_slice(&(dialect_str.len() as u32).to_le_bytes());
            buf.extend_from_slice(dialect_str);

            let op_str = op.op_name.as_bytes();
            buf.extend_from_slice(&(op_str.len() as u32).to_le_bytes());
            buf.extend_from_slice(op_str);

            // Operands
            buf.extend_from_slice(&(op.operands.len() as u32).to_le_bytes());
            for opnd in &op.operands {
                let opnd_bytes = opnd.as_bytes();
                buf.extend_from_slice(&(opnd_bytes.len() as u32).to_le_bytes());
                buf.extend_from_slice(opnd_bytes);
            }

            // Results
            buf.extend_from_slice(&(op.results.len() as u32).to_le_bytes());
            for res in &op.results {
                let res_bytes = res.as_bytes();
                buf.extend_from_slice(&(res_bytes.len() as u32).to_le_bytes());
                buf.extend_from_slice(res_bytes);
            }

            // Attributes
            buf.extend_from_slice(&(op.attributes.len() as u32).to_le_bytes());
            for (k, v) in &op.attributes {
                let k_bytes = k.as_bytes();
                buf.extend_from_slice(&(k_bytes.len() as u32).to_le_bytes());
                buf.extend_from_slice(k_bytes);

                let v_bytes = v.as_bytes();
                buf.extend_from_slice(&(v_bytes.len() as u32).to_le_bytes());
                buf.extend_from_slice(v_bytes);
            }

            // Optional Tensor Shape
            if let Some(ref shape) = op.tensor_shape {
                buf.push(1u8); // Has shape
                buf.extend_from_slice(&(shape.dims.len() as u32).to_le_bytes());
                for &dim in &shape.dims {
                    buf.extend_from_slice(&dim.to_le_bytes());
                }
                let elem_bytes = shape.element_type.as_bytes();
                buf.extend_from_slice(&(elem_bytes.len() as u32).to_le_bytes());
                buf.extend_from_slice(elem_bytes);
            } else {
                buf.push(0u8);
            }
        }

        // 5. Schedules Count
        buf.extend_from_slice(&(module.schedules.len() as u32).to_le_bytes());
        for (kernel_name, schedule) in &module.schedules {
            let kn_bytes = kernel_name.as_bytes();
            buf.extend_from_slice(&(kn_bytes.len() as u32).to_le_bytes());
            buf.extend_from_slice(kn_bytes);

            // Loop vars
            buf.extend_from_slice(&(schedule.loop_vars.len() as u32).to_le_bytes());
            for var in &schedule.loop_vars {
                let var_bytes = var.as_bytes();
                buf.extend_from_slice(&(var_bytes.len() as u32).to_le_bytes());
                buf.extend_from_slice(var_bytes);
            }

            // Bounds
            buf.extend_from_slice(&(schedule.bounds.len() as u32).to_le_bytes());
            for &(lo, hi) in &schedule.bounds {
                buf.extend_from_slice(&lo.to_le_bytes());
                buf.extend_from_slice(&hi.to_le_bytes());
            }

            // Tile sizes
            buf.extend_from_slice(&(schedule.tile_sizes.len() as u32).to_le_bytes());
            for &tile in &schedule.tile_sizes {
                buf.extend_from_slice(&(tile as u64).to_le_bytes());
            }
        }

        // 6. Tensor Constants Count
        buf.extend_from_slice(&(module.tensor_constants.len() as u32).to_le_bytes());
        for (const_name, data) in &module.tensor_constants {
            let cn_bytes = const_name.as_bytes();
            buf.extend_from_slice(&(cn_bytes.len() as u32).to_le_bytes());
            buf.extend_from_slice(cn_bytes);
            buf.extend_from_slice(&(data.len() as u64).to_le_bytes());
            buf.extend_from_slice(data);
        }

        Ok(buf)
    }
}

/// Binary decoder for MLIR bytecode streams.
pub struct MlirBytecodeReader;

impl MlirBytecodeReader {
    pub fn decode(bytes: &[u8]) -> LinkResult<MlirModule> {
        if bytes.len() < 8 {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                "MLIR bytecode buffer too small",
            ));
        }

        if &bytes[0..4] != MLIR_BYTECODE_MAGIC {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                "Invalid MLIR bytecode magic header",
            ));
        }

        let mut offset = 8usize;

        // Module Name
        let name_len = read_u32(bytes, &mut offset)? as usize;
        let name_bytes = read_slice(bytes, &mut offset, name_len)?;
        let name = String::from_utf8_lossy(name_bytes).to_string();

        let mut module = MlirModule::new(name);

        // Operations
        let op_count = read_u32(bytes, &mut offset)? as usize;
        for _ in 0..op_count {
            let dialect_len = read_u32(bytes, &mut offset)? as usize;
            let dialect_bytes = read_slice(bytes, &mut offset, dialect_len)?;
            let dialect_str = String::from_utf8_lossy(dialect_bytes);
            let dialect = DialectKind::from_name(&dialect_str).unwrap_or(DialectKind::Adesh);

            let op_name_len = read_u32(bytes, &mut offset)? as usize;
            let op_name_bytes = read_slice(bytes, &mut offset, op_name_len)?;
            let op_name = String::from_utf8_lossy(op_name_bytes).to_string();

            let mut op = DialectOp::new(dialect, op_name);

            // Operands
            let operand_count = read_u32(bytes, &mut offset)? as usize;
            for _ in 0..operand_count {
                let opnd_len = read_u32(bytes, &mut offset)? as usize;
                let opnd_bytes = read_slice(bytes, &mut offset, opnd_len)?;
                op.operands.push(String::from_utf8_lossy(opnd_bytes).to_string());
            }

            // Results
            let result_count = read_u32(bytes, &mut offset)? as usize;
            for _ in 0..result_count {
                let res_len = read_u32(bytes, &mut offset)? as usize;
                let res_bytes = read_slice(bytes, &mut offset, res_len)?;
                op.results.push(String::from_utf8_lossy(res_bytes).to_string());
            }

            // Attributes
            let attr_count = read_u32(bytes, &mut offset)? as usize;
            for _ in 0..attr_count {
                let k_len = read_u32(bytes, &mut offset)? as usize;
                let k_bytes = read_slice(bytes, &mut offset, k_len)?;
                let k = String::from_utf8_lossy(k_bytes).to_string();

                let v_len = read_u32(bytes, &mut offset)? as usize;
                let v_bytes = read_slice(bytes, &mut offset, v_len)?;
                let v = String::from_utf8_lossy(v_bytes).to_string();

                op.attributes.insert(k, v);
            }

            // Tensor shape
            if offset < bytes.len() && bytes[offset] == 1 {
                offset += 1;
                let dims_len = read_u32(bytes, &mut offset)? as usize;
                let mut dims = Vec::with_capacity(dims_len);
                for _ in 0..dims_len {
                    dims.push(read_i64(bytes, &mut offset)?);
                }
                let elem_len = read_u32(bytes, &mut offset)? as usize;
                let elem_bytes = read_slice(bytes, &mut offset, elem_len)?;
                let elem_type = String::from_utf8_lossy(elem_bytes).to_string();
                op.tensor_shape = Some(TensorShape::new(dims, elem_type));
            } else if offset < bytes.len() {
                offset += 1;
            }

            module.operations.push(op);
        }

        // Schedules
        if offset + 4 <= bytes.len() {
            let sched_count = read_u32(bytes, &mut offset)? as usize;
            for _ in 0..sched_count {
                let kn_len = read_u32(bytes, &mut offset)? as usize;
                let kn_bytes = read_slice(bytes, &mut offset, kn_len)?;
                let kernel_name = String::from_utf8_lossy(kn_bytes).to_string();

                let var_count = read_u32(bytes, &mut offset)? as usize;
                let mut loop_vars = Vec::with_capacity(var_count);
                for _ in 0..var_count {
                    let var_len = read_u32(bytes, &mut offset)? as usize;
                    let var_bytes = read_slice(bytes, &mut offset, var_len)?;
                    loop_vars.push(String::from_utf8_lossy(var_bytes).to_string());
                }

                let bounds_count = read_u32(bytes, &mut offset)? as usize;
                let mut bounds = Vec::with_capacity(bounds_count);
                for _ in 0..bounds_count {
                    let lo = read_i64(bytes, &mut offset)?;
                    let hi = read_i64(bytes, &mut offset)?;
                    bounds.push((lo, hi));
                }

                let mut schedule = PolyhedralSchedule::new(loop_vars, bounds);

                let tile_count = read_u32(bytes, &mut offset)? as usize;
                schedule.tile_sizes.clear();
                for _ in 0..tile_count {
                    let tile = read_u64(bytes, &mut offset)? as usize;
                    schedule.tile_sizes.push(tile);
                }

                module.schedules.insert(kernel_name, schedule);
            }
        }

        // Tensor Constants
        if offset + 4 <= bytes.len() {
            let const_count = read_u32(bytes, &mut offset)? as usize;
            for _ in 0..const_count {
                let cn_len = read_u32(bytes, &mut offset)? as usize;
                let cn_bytes = read_slice(bytes, &mut offset, cn_len)?;
                let const_name = String::from_utf8_lossy(cn_bytes).to_string();

                let data_len = read_u64(bytes, &mut offset)? as usize;
                let data_slice = read_slice(bytes, &mut offset, data_len)?;
                module.tensor_constants.insert(const_name, data_slice.to_vec());
            }
        }

        Ok(module)
    }
}

fn read_u32(bytes: &[u8], offset: &mut usize) -> LinkResult<u32> {
    if *offset + 4 > bytes.len() {
        return Err(LinkError::new(ErrorCode::InvalidObject, "Unexpected EOF reading u32"));
    }
    let val = u32::from_le_bytes(bytes[*offset..*offset + 4].try_into().unwrap());
    *offset += 4;
    Ok(val)
}

fn read_u64(bytes: &[u8], offset: &mut usize) -> LinkResult<u64> {
    if *offset + 8 > bytes.len() {
        return Err(LinkError::new(ErrorCode::InvalidObject, "Unexpected EOF reading u64"));
    }
    let val = u64::from_le_bytes(bytes[*offset..*offset + 8].try_into().unwrap());
    *offset += 8;
    Ok(val)
}

fn read_i64(bytes: &[u8], offset: &mut usize) -> LinkResult<i64> {
    if *offset + 8 > bytes.len() {
        return Err(LinkError::new(ErrorCode::InvalidObject, "Unexpected EOF reading i64"));
    }
    let val = i64::from_le_bytes(bytes[*offset..*offset + 8].try_into().unwrap());
    *offset += 8;
    Ok(val)
}

fn read_slice<'a>(bytes: &'a [u8], offset: &mut usize, len: usize) -> LinkResult<&'a [u8]> {
    if *offset + len > bytes.len() {
        return Err(LinkError::new(ErrorCode::InvalidObject, "Unexpected EOF reading slice"));
    }
    let s = &bytes[*offset..*offset + len];
    *offset += len;
    Ok(s)
}
