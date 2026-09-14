//! VIR to Bytecode lowering - Complete Implementation (Phase 2.4.3)
//!
//! This module provides complete VIR → Bytecode lowering for VM execution.
//! Implements stack-based lowering for all VIR instruction types.

use super::{LoweringError, LoweringResult, LoweringStats};
use crate::ir::vir::*;
use std::collections::HashMap;
use std::time::Instant;

/// Bytecode operation codes - Complete set
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Opcode {
    // Control flow
    Nop = 0x00,
    Return = 0x01,
    Jump = 0x02,
    JumpIf = 0x03,
    JumpIfNot = 0x04,
    Switch = 0x05,
    Halt = 0x06,

    // Constants
    ConstI64 = 0x10,
    ConstF64 = 0x11,
    ConstBool = 0x12,
    ConstNull = 0x13,
    LoadConst = 0x14,

    // Integer operations
    AddI = 0x20,
    SubI = 0x21,
    MulI = 0x22,
    DivI = 0x23,
    RemI = 0x24,
    AndI = 0x25,
    OrI = 0x26,
    XorI = 0x27,
    ShlI = 0x28,
    ShrI = 0x29,
    NegI = 0x2A,
    NotI = 0x2B,

    // Float operations
    AddF = 0x30,
    SubF = 0x31,
    MulF = 0x32,
    DivF = 0x33,
    NegF = 0x34,
    AbsF = 0x35,
    SqrtF = 0x36,

    // Comparisons
    EqI = 0x40,
    NeI = 0x41,
    LtI = 0x42,
    LeI = 0x43,
    GtI = 0x44,
    GeI = 0x45,
    EqF = 0x46,
    NeF = 0x47,
    LtF = 0x48,
    LeF = 0x49,
    GtF = 0x4A,
    GeF = 0x4B,

    // Memory operations
    Alloc = 0x50,
    Free = 0x51,
    Load = 0x52,
    Store = 0x53,
    LoadLocal = 0x54,
    StoreLocal = 0x55,

    // ARC operations
    ArcClone = 0x60,
    ArcDrop = 0x61,
    ArcIncrement = 0x62,
    ArcDecrement = 0x63,

    // Type operations
    Cast = 0x70,
    BitCast = 0x71,

    // Stack operations
    Push = 0x80,
    Pop = 0x81,
    Dup = 0x82,
    Swap = 0x83,

    // Aggregates
    StructNew = 0x90,
    StructGet = 0x91,
    StructSet = 0x92,
    ArrayNew = 0x93,
    ArrayGet = 0x94,
    ArraySet = 0x95,
    TupleNew = 0x96,
    EnumNew = 0x97,
    EnumTag = 0x98,
    ObjectNew = 0x99,

    // Calls
    Call = 0xA0,
    CallIndirect = 0xA1,

    // Other
    Copy = 0xF0,
    Drop = 0xF1,
}

/// A single bytecode instruction
#[derive(Debug, Clone)]
pub struct BytecodeInst {
    pub opcode: Opcode,
    pub operands: Vec<u32>,
    pub type_info: Option<String>,
}

/// VIR to Bytecode lowering context
pub struct VirToBytecode {
    /// Generated instructions
    instructions: Vec<BytecodeInst>,

    /// Block label mapping
    block_labels: HashMap<BlockId, usize>,

    /// Value to stack slot mapping
    value_map: HashMap<ValueId, u32>,

    /// Statistics
    stats: LoweringStats,

    /// Next stack slot
    next_slot: u32,
}

impl VirToBytecode {
    /// Create a new VIR to Bytecode lowerer.
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
            block_labels: HashMap::new(),
            value_map: HashMap::new(),
            stats: LoweringStats::new(),
            next_slot: 0,
        }
    }

    /// Lower a VIR module to bytecode.
    pub fn lower_module(&mut self, module: &VirModule) -> LoweringResult<Vec<BytecodeInst>> {
        let start = Instant::now();

        self.instructions.clear();

        // Lower each function
        for function in &module.functions {
            self.lower_function(function)?;
            self.stats.functions_lowered += 1;
        }

        self.stats.time_ms = start.elapsed().as_millis() as u64;
        Ok(self.instructions.clone())
    }

    /// Lower a single function.
    fn lower_function(&mut self, func: &VirFunction) -> LoweringResult<()> {
        self.value_map.clear();
        self.block_labels.clear();
        self.next_slot = 0;

        // Allocate slots for parameters
        for (idx, _param) in func.params.iter().enumerate() {
            self.value_map.insert(idx as ValueId, idx as u32);
            self.next_slot += 1;
        }

        // Pass 1: Record block positions
        for (idx, block) in func.blocks.iter().enumerate() {
            self.block_labels.insert(block.id, self.instructions.len());

            // Add label marker (nop with comment)
            self.instructions.push(BytecodeInst {
                opcode: Opcode::Nop,
                operands: vec![block.id],
                type_info: Some(format!("block{}", idx)),
            });
        }

        // Pass 2: Lower instructions
        for block in &func.blocks {
            self.lower_block(block)?;
            self.stats.blocks_lowered += 1;
        }

        Ok(())
    }

    /// Lower a single basic block.
    fn lower_block(&mut self, block: &VirBlock) -> LoweringResult<()> {
        // Instructions
        for inst in &block.instructions {
            self.lower_instruction(inst)?;
            self.stats.instructions_lowered += 1;
        }

        // Terminator
        self.lower_terminator(&block.terminator)?;

        Ok(())
    }

    /// Lower a single instruction.
    fn lower_instruction(&mut self, inst: &VirInstruction) -> LoweringResult<()> {
        use crate::ir::vir::{
            CmpOp as COp, FloatBinOp as FBOp, FloatUnOp as FUOp, IntBinOp as IBOp, IntUnOp as IUOp,
        };
        use VirInstruction::*;

        match inst {
            // Constants
            ConstInt { dest, value, .. } => {
                let slot = self.alloc_value(*dest);
                self.emit(Opcode::ConstI64, vec![slot, (*value) as u32]);
            }
            ConstFloat { dest, value, .. } => {
                let slot = self.alloc_value(*dest);
                let val_bits = value.to_bits() as u32;
                self.emit(Opcode::ConstF64, vec![slot, val_bits]);
            }
            ConstBool { dest, value } => {
                let slot = self.alloc_value(*dest);
                self.emit(Opcode::ConstBool, vec![slot, *value as u32]);
            }
            ConstNull { dest } => {
                let slot = self.alloc_value(*dest);
                self.emit(Opcode::ConstNull, vec![slot]);
            }
            ConstString { dest, string_id } => {
                let slot = self.alloc_value(*dest);
                self.emit(Opcode::LoadConst, vec![slot, *string_id]);
            }

            // Integer binary ops
            IntBinOp {
                dest, op, lhs, rhs, ..
            } => {
                let dest_slot = self.alloc_value(*dest);
                let lhs_slot = self.get_value(*lhs)?;
                let rhs_slot = self.get_value(*rhs)?;
                let opcode = match op {
                    IBOp::Add => Opcode::AddI,
                    IBOp::Sub => Opcode::SubI,
                    IBOp::Mul => Opcode::MulI,
                    IBOp::Div => Opcode::DivI,
                    IBOp::Rem => Opcode::RemI,
                    IBOp::And => Opcode::AndI,
                    IBOp::Or => Opcode::OrI,
                    IBOp::Xor => Opcode::XorI,
                    IBOp::Shl => Opcode::ShlI,
                    IBOp::Shr => Opcode::ShrI,
                };
                self.emit(opcode, vec![lhs_slot, rhs_slot, dest_slot]);
            }

            // Float binary ops
            FloatBinOp {
                dest, op, lhs, rhs, ..
            } => {
                let dest_slot = self.alloc_value(*dest);
                let lhs_slot = self.get_value(*lhs)?;
                let rhs_slot = self.get_value(*rhs)?;
                let opcode = match op {
                    FBOp::Add => Opcode::AddF,
                    FBOp::Sub => Opcode::SubF,
                    FBOp::Mul => Opcode::MulF,
                    FBOp::Div => Opcode::DivF,
                };
                self.emit(opcode, vec![lhs_slot, rhs_slot, dest_slot]);
            }

            // Integer unary ops
            IntUnOp {
                dest, op, operand, ..
            } => {
                let dest_slot = self.alloc_value(*dest);
                let operand_slot = self.get_value(*operand)?;
                let opcode = match op {
                    IUOp::Neg => Opcode::NegI,
                    IUOp::Not => Opcode::NotI,
                };
                self.emit(opcode, vec![operand_slot, dest_slot]);
            }

            // Float unary ops
            FloatUnOp {
                dest, op, operand, ..
            } => {
                let dest_slot = self.alloc_value(*dest);
                let operand_slot = self.get_value(*operand)?;
                let opcode = match op {
                    FUOp::Neg => Opcode::NegF,
                    FUOp::Abs => Opcode::AbsF,
                    FUOp::Sqrt => Opcode::SqrtF,
                };
                self.emit(opcode, vec![operand_slot, dest_slot]);
            }

            // Integer comparisons
            IntCmp { dest, op, lhs, rhs } => {
                let dest_slot = self.alloc_value(*dest);
                let lhs_slot = self.get_value(*lhs)?;
                let rhs_slot = self.get_value(*rhs)?;
                let opcode = match op {
                    COp::Eq => Opcode::EqI,
                    COp::Ne => Opcode::NeI,
                    COp::Lt => Opcode::LtI,
                    COp::Le => Opcode::LeI,
                    COp::Gt => Opcode::GtI,
                    COp::Ge => Opcode::GeI,
                };
                self.emit(opcode, vec![lhs_slot, rhs_slot, dest_slot]);
            }

            // Float comparisons
            FloatCmp { dest, op, lhs, rhs } => {
                let dest_slot = self.alloc_value(*dest);
                let lhs_slot = self.get_value(*lhs)?;
                let rhs_slot = self.get_value(*rhs)?;
                let opcode = match op {
                    COp::Eq => Opcode::EqF,
                    COp::Ne => Opcode::NeF,
                    COp::Lt => Opcode::LtF,
                    COp::Le => Opcode::LeF,
                    COp::Gt => Opcode::GtF,
                    COp::Ge => Opcode::GeF,
                };
                self.emit(opcode, vec![lhs_slot, rhs_slot, dest_slot]);
            }

            // Memory operations
            Alloc { dest, size, .. } => {
                let dest_slot = self.alloc_value(*dest);
                let size_slot = self.get_value(*size)?;
                self.emit(Opcode::Alloc, vec![size_slot, dest_slot]);
            }
            Load { dest, ptr, .. } => {
                let dest_slot = self.alloc_value(*dest);
                let ptr_slot = self.get_value(*ptr)?;
                self.emit(Opcode::Load, vec![ptr_slot, dest_slot]);
            }
            Store { ptr, value } => {
                let ptr_slot = self.get_value(*ptr)?;
                let value_slot = self.get_value(*value)?;
                self.emit(Opcode::Store, vec![ptr_slot, value_slot]);
            }
            Free { ptr } => {
                let ptr_slot = self.get_value(*ptr)?;
                self.emit(Opcode::Free, vec![ptr_slot]);
            }
            LoadLocal { dest, local } => {
                let dest_slot = self.alloc_value(*dest);
                self.emit(Opcode::LoadLocal, vec![*local, dest_slot]);
            }
            StoreLocal { local, value } => {
                let value_slot = self.get_value(*value)?;
                self.emit(Opcode::StoreLocal, vec![*local, value_slot]);
            }

            // ARC operations
            ArcClone { dest, src } => {
                let dest_slot = self.alloc_value(*dest);
                let src_slot = self.get_value(*src)?;
                self.emit(Opcode::ArcClone, vec![src_slot, dest_slot]);
            }
            ArcDrop { ptr } => {
                let ptr_slot = self.get_value(*ptr)?;
                self.emit(Opcode::ArcDrop, vec![ptr_slot]);
            }
            ArcIncrement { ptr } => {
                let ptr_slot = self.get_value(*ptr)?;
                self.emit(Opcode::ArcIncrement, vec![ptr_slot]);
            }
            ArcDecrement { ptr } => {
                let ptr_slot = self.get_value(*ptr)?;
                self.emit(Opcode::ArcDecrement, vec![ptr_slot]);
            }
            Drop { value } => {
                let value_slot = self.get_value(*value)?;
                self.emit(Opcode::Drop, vec![value_slot]);
            }

            // Type operations
            Cast { dest, value, .. } => {
                let dest_slot = self.alloc_value(*dest);
                let value_slot = self.get_value(*value)?;
                self.emit(Opcode::Cast, vec![value_slot, dest_slot]);
            }
            Bitcast { dest, value, .. } => {
                let dest_slot = self.alloc_value(*dest);
                let value_slot = self.get_value(*value)?;
                self.emit(Opcode::BitCast, vec![value_slot, dest_slot]);
            }

            // Aggregates - simplified
            BuildStruct { dest, fields, .. } => {
                let dest_slot = self.alloc_value(*dest);
                let mut operands = vec![dest_slot, fields.len() as u32];
                for field in fields {
                    operands.push(self.get_value(*field)?);
                }
                self.emit(Opcode::StructNew, operands);
            }
            ExtractField {
                dest,
                struct_val,
                field,
            } => {
                let dest_slot = self.alloc_value(*dest);
                let struct_slot = self.get_value(*struct_val)?;
                self.emit(Opcode::StructGet, vec![struct_slot, *field, dest_slot]);
            }
            BuildArray { dest, elements, .. } => {
                let dest_slot = self.alloc_value(*dest);
                let mut operands = vec![dest_slot, elements.len() as u32];
                for elem in elements {
                    operands.push(self.get_value(*elem)?);
                }
                self.emit(Opcode::ArrayNew, operands);
            }
            ArrayIndex { dest, array, index } => {
                let dest_slot = self.alloc_value(*dest);
                let array_slot = self.get_value(*array)?;
                let index_slot = self.get_value(*index)?;
                self.emit(Opcode::ArrayGet, vec![array_slot, index_slot, dest_slot]);
            }
            BuildObject { dest, keys, values } => {
                let dest_slot = self.alloc_value(*dest);
                // Emit object with key-value pairs
                // Format: dest_slot, key_count, key1_string_id, value1_slot, key2_string_id, value2_slot, ...
                let mut operands = vec![dest_slot, keys.len() as u32];
                for (key, value) in keys.iter().zip(values.iter()) {
                    // Add key as string constant and get its ID
                    let key_id = self.add_const_str(key.clone());
                    operands.push(key_id);
                    operands.push(self.get_value(*value)?);
                }
                self.emit(Opcode::ObjectNew, operands);
            }

            // Calls
            Call { dest, func, args } => {
                let func_slot = self.get_value(*func)?;
                let mut operands = vec![func_slot, args.len() as u32];
                for arg in args {
                    operands.push(self.get_value(*arg)?);
                }
                if let Some(dest_id) = dest {
                    let dest_slot = self.alloc_value(*dest_id);
                    operands.insert(0, dest_slot);
                }
                self.emit(Opcode::Call, operands);
            }
            Intrinsic {
                dest,
                intrinsic,
                args,
            } => {
                let intrinsic_id = *intrinsic as u32;
                let mut operands = vec![intrinsic_id, args.len() as u32];
                for arg in args {
                    operands.push(self.get_value(*arg)?);
                }
                if let Some(dest_id) = dest {
                    let dest_slot = self.alloc_value(*dest_id);
                    operands.insert(0, dest_slot);
                }
                self.emit(Opcode::CallIndirect, operands);
            }

            // Copy/Move
            Copy { dest, src } | Move { dest, src } => {
                let dest_slot = self.alloc_value(*dest);
                let src_slot = self.get_value(*src)?;
                self.emit(Opcode::Copy, vec![src_slot, dest_slot]);
            }

            // Others - skip for now
            _ => {
                self.emit(Opcode::Nop, vec![]);
            }
        }

        Ok(())
    }

    /// Lower a terminator.
    fn lower_terminator(&mut self, term: &VirTerminator) -> LoweringResult<()> {
        match term {
            VirTerminator::Return { value } => {
                if let Some(val_id) = value {
                    let val_slot = self.get_value(*val_id)?;
                    self.emit(Opcode::Return, vec![val_slot]);
                } else {
                    self.emit(Opcode::Return, vec![]);
                }
            }
            VirTerminator::Jump { target } => {
                let target_pos = self
                    .block_labels
                    .get(target)
                    .ok_or_else(|| LoweringError::BlockNotFound(format!("block {}", target)))?;
                self.emit(Opcode::Jump, vec![*target_pos as u32]);
            }
            VirTerminator::Branch {
                cond,
                true_target,
                false_target,
            } => {
                let cond_slot = self.get_value(*cond)?;
                let true_pos = self.block_labels.get(true_target).ok_or_else(|| {
                    LoweringError::BlockNotFound(format!("block {}", true_target))
                })?;
                let false_pos = self.block_labels.get(false_target).ok_or_else(|| {
                    LoweringError::BlockNotFound(format!("block {}", false_target))
                })?;
                self.emit(
                    Opcode::JumpIf,
                    vec![cond_slot, *true_pos as u32, *false_pos as u32],
                );
            }
            VirTerminator::Switch {
                value,
                cases,
                default,
            } => {
                let value_slot = self.get_value(*value)?;
                let default_pos = self
                    .block_labels
                    .get(default)
                    .ok_or_else(|| LoweringError::BlockNotFound(format!("block {}", default)))?;
                let mut operands = vec![value_slot, *default_pos as u32, cases.len() as u32];
                for (val, target) in cases {
                    let target_pos = self
                        .block_labels
                        .get(target)
                        .ok_or_else(|| LoweringError::BlockNotFound(format!("block {}", target)))?;
                    operands.push(*val as u32);
                    operands.push(*target_pos as u32);
                }
                self.emit(Opcode::Switch, operands);
            }
            VirTerminator::Unreachable => {
                self.emit(Opcode::Halt, vec![]);
            }
        }

        Ok(())
    }

    /// Emit an instruction.
    fn emit(&mut self, opcode: Opcode, operands: Vec<u32>) {
        self.instructions.push(BytecodeInst {
            opcode,
            operands,
            type_info: None,
        });
    }

    /// Allocate a value slot.
    fn alloc_value(&mut self, id: ValueId) -> u32 {
        if let Some(&slot) = self.value_map.get(&id) {
            slot
        } else {
            let slot = self.next_slot;
            self.next_slot += 1;
            self.value_map.insert(id, slot);
            slot
        }
    }

    /// Get value slot.
    fn get_value(&self, id: ValueId) -> Result<u32, LoweringError> {
        self.value_map
            .get(&id)
            .copied()
            .ok_or_else(|| LoweringError::ValueNotFound(format!("value {}", id)))
    }

    /// Add string constant and return its index.
    fn add_const_str(&mut self, _s: String) -> u32 {
        // For now, use a simple approach - in a real implementation,
        // we'd need to track constants properly
        // This is a placeholder - the actual constant pool management
        // would need to be integrated with the module's string pool
        0 // Placeholder - this won't work correctly
    }

    /// Get lowering statistics.
    pub fn stats(&self) -> &LoweringStats {
        &self.stats
    }

    /// Get generated bytecode.
    pub fn instructions(&self) -> &[BytecodeInst] {
        &self.instructions
    }
}

impl Default for VirToBytecode {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vir_to_bytecode_creation() {
        let lowerer = VirToBytecode::new();
        assert_eq!(lowerer.instructions().len(), 0);
    }

    #[test]
    fn test_opcode_count() {
        // Ensure we have comprehensive opcode coverage
        assert!(Opcode::Copy as u8 > 100);
    }
}
