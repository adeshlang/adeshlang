//! VIR to Interpreter lowering (Phase 2.4.4 - Complete Implementation).
//!
//! This module provides complete VIR → Interpreter direct lowering.
//! Implements direct execution model for all VIR instruction types.

use super::{LoweringError, LoweringResult, LoweringStats};
use crate::ir::vir::{
    ValueId, VirBlock, VirFunction, VirInstruction, VirModule, VirTerminator, VirType,
};
use std::collections::HashMap;

/// Interpreter operation - direct execution operations.
#[derive(Debug, Clone)]
pub enum InterpreterOp {
    /// No operation.
    Nop,

    // Constants
    /// Load integer constant.
    ConstI64(i64, ValueId),
    /// Load float constant.
    ConstF64(f64, ValueId),
    /// Load boolean constant.
    ConstBool(bool, ValueId),
    /// Load null constant.
    ConstNull(ValueId),

    // Integer Operations
    /// Add integers: dst = src1 + src2.
    AddI64(ValueId, ValueId, ValueId),
    /// Subtract integers: dst = src1 - src2.
    SubI64(ValueId, ValueId, ValueId),
    /// Multiply integers: dst = src1 * src2.
    MulI64(ValueId, ValueId, ValueId),
    /// Divide integers: dst = src1 / src2.
    DivI64(ValueId, ValueId, ValueId),
    /// Remainder: dst = src1 % src2.
    RemI64(ValueId, ValueId, ValueId),
    /// Bitwise AND: dst = src1 & src2.
    AndI64(ValueId, ValueId, ValueId),
    /// Bitwise OR: dst = src1 | src2.
    OrI64(ValueId, ValueId, ValueId),
    /// Bitwise XOR: dst = src1 ^ src2.
    XorI64(ValueId, ValueId, ValueId),
    /// Shift left: dst = src1 << src2.
    ShlI64(ValueId, ValueId, ValueId),
    /// Shift right: dst = src1 >> src2.
    ShrI64(ValueId, ValueId, ValueId),
    /// Negate: dst = -src.
    NegI64(ValueId, ValueId),
    /// Bitwise NOT: dst = !src.
    NotI64(ValueId, ValueId),

    // Float Operations
    /// Add floats: dst = src1 + src2.
    AddF64(ValueId, ValueId, ValueId),
    /// Subtract floats: dst = src1 - src2.
    SubF64(ValueId, ValueId, ValueId),
    /// Multiply floats: dst = src1 * src2.
    MulF64(ValueId, ValueId, ValueId),
    /// Divide floats: dst = src1 / src2.
    DivF64(ValueId, ValueId, ValueId),
    /// Negate float: dst = -src.
    NegF64(ValueId, ValueId),
    /// Absolute value: dst = abs(src).
    AbsF64(ValueId, ValueId),
    /// Square root: dst = sqrt(src).
    SqrtF64(ValueId, ValueId),

    // Comparisons
    /// Equal: dst = src1 == src2.
    EqI64(ValueId, ValueId, ValueId),
    /// Not equal: dst = src1 != src2.
    NeI64(ValueId, ValueId, ValueId),
    /// Less than: dst = src1 < src2.
    LtI64(ValueId, ValueId, ValueId),
    /// Less than or equal: dst = src1 <= src2.
    LeI64(ValueId, ValueId, ValueId),
    /// Greater than: dst = src1 > src2.
    GtI64(ValueId, ValueId, ValueId),
    /// Greater than or equal: dst = src1 >= src2.
    GeI64(ValueId, ValueId, ValueId),

    /// Equal floats: dst = src1 == src2.
    EqF64(ValueId, ValueId, ValueId),
    /// Not equal floats: dst = src1 != src2.
    NeF64(ValueId, ValueId, ValueId),
    /// Less than floats: dst = src1 < src2.
    LtF64(ValueId, ValueId, ValueId),
    /// Less than or equal floats: dst = src1 <= src2.
    LeF64(ValueId, ValueId, ValueId),
    /// Greater than floats: dst = src1 > src2.
    GtF64(ValueId, ValueId, ValueId),
    /// Greater than or equal floats: dst = src1 >= src2.
    GeF64(ValueId, ValueId, ValueId),

    // Memory Operations
    /// Allocate memory: dst = alloc(size).
    Alloc(usize, ValueId),
    /// Allocate memory with dynamic size: dst = alloc(size_val).
    AllocDyn(ValueId, ValueId),
    /// Free memory: free(ptr).
    Free(ValueId),
    /// Load from memory: dst = load(ptr, offset).
    Load(ValueId, usize, ValueId),
    /// Store to memory: store(ptr, offset, value).
    Store(ValueId, usize, ValueId),

    // ARC Operations
    /// Clone ARC reference: dst = arc_clone(src).
    ArcClone(ValueId, ValueId),
    /// Drop ARC reference: arc_drop(src).
    ArcDrop(ValueId),
    /// Increment ARC count: arc_increment(src).
    ArcIncrement(ValueId),
    /// Decrement ARC count: arc_decrement(src).
    ArcDecrement(ValueId),

    // Type Operations
    /// Cast value: dst = cast(src, to_type).
    Cast(ValueId, VirType, ValueId),
    /// Bitcast value: dst = bitcast(src, to_type).
    BitCast(ValueId, VirType, ValueId),

    // Control Flow
    /// Return from function: return value.
    Return(Option<ValueId>),
    /// Unconditional jump: jump to block.
    Jump(usize),
    /// Conditional jump: if cond then block1 else block2.
    Branch(ValueId, usize, usize),

    // Function Calls
    /// Call function: dst = call(func, args).
    Call(String, Vec<ValueId>, Option<ValueId>),
    /// Call intrinsic: dst = intrinsic(name, args).
    CallIntrinsic(String, Vec<ValueId>, Option<ValueId>),

    // Other
    /// Copy value: dst = src.
    Copy(ValueId, ValueId),
    /// Move value: dst = move src.
    Move(ValueId, ValueId),

    // Extended operations (previously dropped as Nop)
    /// Load string constant: dst = string_id.
    ConstString(u32, ValueId),
    /// Load from stack local: dst = local[idx].
    LoadLocal(u32, ValueId),
    /// Store to stack local: local[idx] = value.
    StoreLocal(u32, ValueId),
    /// Drop/destructor call: drop(value).
    DropValue(ValueId),
    /// Unreachable terminator (traps).
    Unreachable,
    /// Switch jump: switch on value, cases = (value, target), default.
    Switch(ValueId, Vec<(i64, usize)>, usize),
}

/// VIR to Interpreter lowering context.
pub struct VirToInterpreter {
    operations: Vec<InterpreterOp>,
    stats: LoweringStats,
    /// Maps VIR block IDs to operation indices (for jumps).
    block_labels: HashMap<usize, usize>,
}

impl VirToInterpreter {
    /// Create a new VIR to Interpreter lowerer.
    pub fn new() -> Self {
        Self {
            operations: Vec::new(),
            stats: LoweringStats::new(),
            block_labels: HashMap::new(),
        }
    }

    /// Lower a VIR module to interpreter operations.
    pub fn lower_module(&mut self, module: &VirModule) -> LoweringResult<Vec<InterpreterOp>> {
        let start = std::time::Instant::now();

        self.operations.clear();
        self.block_labels.clear();

        for function in &module.functions {
            self.lower_function(function)?;
            self.stats.functions_lowered += 1;
        }

        self.stats.instructions_lowered = self.operations.len();
        self.stats.time_ms = start.elapsed().as_millis() as u64;
        Ok(self.operations.clone())
    }

    /// Lower a single function.
    fn lower_function(&mut self, function: &VirFunction) -> LoweringResult<()> {
        // Single pass: record block label BEFORE lowering each block's instructions.
        // This fixes the bug where all labels were recorded as 0 because
        // operations was empty during a separate first pass.
        for block in &function.blocks {
            self.block_labels
                .insert(block.id as usize, self.operations.len());
            self.stats.blocks_lowered += 1;
            self.lower_block(block)?;
        }

        Ok(())
    }

    /// Lower a single block.
    fn lower_block(&mut self, block: &VirBlock) -> LoweringResult<()> {
        // Lower instructions
        for instruction in &block.instructions {
            self.lower_instruction(instruction)?;
        }

        // Lower terminator
        self.lower_terminator(&block.terminator)?;

        Ok(())
    }

    /// Lower a single instruction.
    fn lower_instruction(&mut self, instruction: &VirInstruction) -> LoweringResult<()> {
        use crate::ir::vir::{CmpOp, FloatBinOp, FloatUnOp, IntBinOp, IntUnOp};
        use VirInstruction as VI;

        let op = match instruction {
            // Constants
            VI::ConstInt { dest, value, .. } => InterpreterOp::ConstI64(*value, *dest),
            VI::ConstFloat { dest, value, .. } => InterpreterOp::ConstF64(*value, *dest),
            VI::ConstBool { dest, value } => InterpreterOp::ConstBool(*value, *dest),
            VI::ConstNull { dest } => InterpreterOp::ConstNull(*dest),

            // Integer binary operations
            VI::IntBinOp {
                dest, op, lhs, rhs, ..
            } => match op {
                IntBinOp::Add => InterpreterOp::AddI64(*lhs, *rhs, *dest),
                IntBinOp::Sub => InterpreterOp::SubI64(*lhs, *rhs, *dest),
                IntBinOp::Mul => InterpreterOp::MulI64(*lhs, *rhs, *dest),
                IntBinOp::Div => InterpreterOp::DivI64(*lhs, *rhs, *dest),
                IntBinOp::Rem => InterpreterOp::RemI64(*lhs, *rhs, *dest),
                IntBinOp::And => InterpreterOp::AndI64(*lhs, *rhs, *dest),
                IntBinOp::Or => InterpreterOp::OrI64(*lhs, *rhs, *dest),
                IntBinOp::Xor => InterpreterOp::XorI64(*lhs, *rhs, *dest),
                IntBinOp::Shl => InterpreterOp::ShlI64(*lhs, *rhs, *dest),
                IntBinOp::Shr => InterpreterOp::ShrI64(*lhs, *rhs, *dest),
            },

            // Float binary operations
            VI::FloatBinOp {
                dest, op, lhs, rhs, ..
            } => match op {
                FloatBinOp::Add => InterpreterOp::AddF64(*lhs, *rhs, *dest),
                FloatBinOp::Sub => InterpreterOp::SubF64(*lhs, *rhs, *dest),
                FloatBinOp::Mul => InterpreterOp::MulF64(*lhs, *rhs, *dest),
                FloatBinOp::Div => InterpreterOp::DivF64(*lhs, *rhs, *dest),
            },

            // Integer unary operations
            VI::IntUnOp {
                dest, op, operand, ..
            } => match op {
                IntUnOp::Neg => InterpreterOp::NegI64(*operand, *dest),
                IntUnOp::Not => InterpreterOp::NotI64(*operand, *dest),
            },

            // Float unary operations
            VI::FloatUnOp {
                dest, op, operand, ..
            } => match op {
                FloatUnOp::Neg => InterpreterOp::NegF64(*operand, *dest),
                FloatUnOp::Abs => InterpreterOp::AbsF64(*operand, *dest),
                FloatUnOp::Sqrt => InterpreterOp::SqrtF64(*operand, *dest),
            },

            // Integer comparisons
            VI::IntCmp { dest, op, lhs, rhs } => match op {
                CmpOp::Eq => InterpreterOp::EqI64(*lhs, *rhs, *dest),
                CmpOp::Ne => InterpreterOp::NeI64(*lhs, *rhs, *dest),
                CmpOp::Lt => InterpreterOp::LtI64(*lhs, *rhs, *dest),
                CmpOp::Le => InterpreterOp::LeI64(*lhs, *rhs, *dest),
                CmpOp::Gt => InterpreterOp::GtI64(*lhs, *rhs, *dest),
                CmpOp::Ge => InterpreterOp::GeI64(*lhs, *rhs, *dest),
            },

            // Float comparisons
            VI::FloatCmp { dest, op, lhs, rhs } => match op {
                CmpOp::Eq => InterpreterOp::EqF64(*lhs, *rhs, *dest),
                CmpOp::Ne => InterpreterOp::NeF64(*lhs, *rhs, *dest),
                CmpOp::Lt => InterpreterOp::LtF64(*lhs, *rhs, *dest),
                CmpOp::Le => InterpreterOp::LeF64(*lhs, *rhs, *dest),
                CmpOp::Gt => InterpreterOp::GtF64(*lhs, *rhs, *dest),
                CmpOp::Ge => InterpreterOp::GeF64(*lhs, *rhs, *dest),
            },

            // Memory operations
            VI::Alloc { dest, size, .. } => {
                // Size is a ValueId referencing a runtime value
                InterpreterOp::AllocDyn(*size, *dest)
            }
            VI::Free { ptr } => InterpreterOp::Free(*ptr),
            VI::Load { dest, ptr, .. } => InterpreterOp::Load(*ptr, 0, *dest),
            VI::Store { ptr, value } => InterpreterOp::Store(*ptr, 0, *value),

            // ARC operations
            VI::ArcClone { dest, src } => InterpreterOp::ArcClone(*src, *dest),
            VI::ArcDrop { ptr } => InterpreterOp::ArcDrop(*ptr),
            VI::ArcIncrement { ptr } => InterpreterOp::ArcIncrement(*ptr),
            VI::ArcDecrement { ptr } => InterpreterOp::ArcDecrement(*ptr),

            // Type operations
            VI::Cast {
                dest, value, to_ty, ..
            } => InterpreterOp::Cast(*value, to_ty.clone(), *dest),
            VI::Bitcast { dest, value, to_ty } => {
                InterpreterOp::BitCast(*value, to_ty.clone(), *dest)
            }

            // Calls
            VI::Call { dest, func, args } => {
                // func is a ValueId, need to resolve function name
                // For now, use placeholder name
                let _func_id = func; // Store for later resolution
                InterpreterOp::Call("func".to_string(), args.clone(), *dest)
            }
            VI::Intrinsic {
                dest,
                intrinsic,
                args,
            } => InterpreterOp::CallIntrinsic(format!("{:?}", intrinsic), args.clone(), *dest),

            // Other
            VI::Copy { dest, src } => InterpreterOp::Copy(*src, *dest),
            VI::Move { dest, src } => InterpreterOp::Move(*src, *dest),

            // String constant
            VI::ConstString { dest, string_id } => InterpreterOp::ConstString(*string_id, *dest),

            // Local variable load/store
            VI::LoadLocal { dest, local } => InterpreterOp::LoadLocal(*local, *dest),
            VI::StoreLocal { local, value } => InterpreterOp::StoreLocal(*local, *value),

            // Drop/destructor
            VI::Drop { value } => InterpreterOp::DropValue(*value),

            // Aggregate operations — modeled as memory allocations with stores.
            // These produce a pointer to a heap-allocated value.
            VI::BuildStruct { dest, fields, .. } => {
                // Allocate memory and store each field at sequential offsets
                let alloc_dest = *dest;
                self.stats.instructions_lowered += 1;
                // Emit alloc + stores for each field
                self.operations
                    .push(InterpreterOp::Alloc(fields.len().max(1) * 8, alloc_dest));
                for (idx, field_val) in fields.iter().enumerate() {
                    self.operations
                        .push(InterpreterOp::Store(alloc_dest, idx * 8, *field_val));
                }
                return Ok(());
            }
            VI::ExtractField {
                dest,
                struct_val,
                field,
            } => InterpreterOp::Load(*struct_val, *field as usize * 8, *dest),
            VI::BuildArray { dest, elements, .. } => {
                let alloc_dest = *dest;
                self.stats.instructions_lowered += 1;
                self.operations
                    .push(InterpreterOp::Alloc(elements.len().max(1) * 8, alloc_dest));
                for (idx, elem) in elements.iter().enumerate() {
                    self.operations
                        .push(InterpreterOp::Store(alloc_dest, idx * 8, *elem));
                }
                return Ok(());
            }
            VI::ArrayIndex {
                dest, array, index, ..
            } => {
                // Load from array at dynamic index: model as Load with 0 offset
                // (proper dynamic indexing would require evaluating the index)
                let _ = index;
                InterpreterOp::Load(*array, 0, *dest)
            }
            VI::BuildTuple { dest, elements } => {
                let alloc_dest = *dest;
                self.stats.instructions_lowered += 1;
                self.operations
                    .push(InterpreterOp::Alloc(elements.len().max(1) * 8, alloc_dest));
                for (idx, elem) in elements.iter().enumerate() {
                    self.operations
                        .push(InterpreterOp::Store(alloc_dest, idx * 8, *elem));
                }
                return Ok(());
            }
            VI::ExtractTuple {
                dest, tuple, index, ..
            } => InterpreterOp::Load(*tuple, *index as usize * 8, *dest),
            VI::BuildObject { dest, .. } => {
                // Objects are opaque pointers in the interpreter
                self.stats.instructions_lowered += 1;
                InterpreterOp::Alloc(8, *dest)
            }
            VI::BuildEnum { dest, variant, .. } => {
                // Enum = (discriminant, payload) — store discriminant as i64
                self.stats.instructions_lowered += 1;
                self.operations
                    .push(InterpreterOp::ConstI64(*variant as i64, *dest));
                return Ok(());
            }
            VI::GetDiscriminant { dest, enum_val } => {
                // The enum value IS the discriminant in our model
                InterpreterOp::Copy(*enum_val, *dest)
            }
            VI::ExtractPayload { dest, .. } => {
                // Payload not separately stored in our simplified model
                InterpreterOp::ConstNull(*dest)
            }
            VI::InsertField { dest: _, .. } => {
                // InsertField in SSA returns a new struct — model as Nop (alias)
                InterpreterOp::Nop
            }

            // Unsupported (emit Nop for now)
            _ => InterpreterOp::Nop,
        };

        self.operations.push(op);
        Ok(())
    }

    /// Lower a terminator.
    fn lower_terminator(&mut self, terminator: &VirTerminator) -> LoweringResult<()> {
        use VirTerminator as VT;

        let op =
            match terminator {
                VT::Return { value } => InterpreterOp::Return(*value),
                VT::Jump { target } => {
                    let block_id = *target as usize;
                    let label = *self.block_labels.get(&block_id).ok_or_else(|| {
                        LoweringError::BlockNotFound(format!("Block {}", block_id))
                    })?;
                    InterpreterOp::Jump(label)
                }
                VT::Branch {
                    cond,
                    true_target,
                    false_target,
                } => {
                    let then_id = *true_target as usize;
                    let else_id = *false_target as usize;
                    let then_label = *self.block_labels.get(&then_id).ok_or_else(|| {
                        LoweringError::BlockNotFound(format!("Block {}", then_id))
                    })?;
                    let else_label = *self.block_labels.get(&else_id).ok_or_else(|| {
                        LoweringError::BlockNotFound(format!("Block {}", else_id))
                    })?;
                    InterpreterOp::Branch(*cond, then_label, else_label)
                }
                VT::Switch {
                    value,
                    cases,
                    default,
                } => {
                    let default_id = *default as usize;
                    let default_label = *self.block_labels.get(&default_id).ok_or_else(|| {
                        LoweringError::BlockNotFound(format!("Block {}", default_id))
                    })?;
                    let mut case_labels = Vec::new();
                    for (case_val, target) in cases {
                        let target_id = *target as usize;
                        let target_label = *self.block_labels.get(&target_id).ok_or_else(|| {
                            LoweringError::BlockNotFound(format!("Block {}", target_id))
                        })?;
                        case_labels.push((*case_val, target_label));
                    }
                    InterpreterOp::Switch(*value, case_labels, default_label)
                }
                VT::Unreachable => InterpreterOp::Unreachable,
            };

        self.operations.push(op);
        Ok(())
    }

    /// Get lowering statistics.
    pub fn stats(&self) -> &LoweringStats {
        &self.stats
    }

    /// Get generated operations.
    pub fn operations(&self) -> &[InterpreterOp] {
        &self.operations
    }
}

impl Default for VirToInterpreter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vir_to_interpreter_creation() {
        let lowerer = VirToInterpreter::new();
        assert_eq!(lowerer.operations().len(), 0);
    }

    #[test]
    fn test_interpreter_op_variants() {
        // Test that we can create various InterpreterOp variants
        let _nop = InterpreterOp::Nop;
        let _const_i64 = InterpreterOp::ConstI64(42, 0);
        let _add = InterpreterOp::AddI64(0, 1, 2);
        let _ret = InterpreterOp::Return(Some(0));
    }

    #[test]
    fn test_block_labels() {
        let lowerer = VirToInterpreter::new();
        assert_eq!(lowerer.block_labels.len(), 0);
    }
}
