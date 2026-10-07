//! Stack Frame Optimization & Allocation Compaction.
//!
//! Compacts stack frame sizes, eliminates unused spill slots, and ensures 16-byte ABI alignment.

use crate::error::CodegenError;
use crate::machine_ir::{
    MachineFunction, MachineInstruction, MachineOperand, MachineRegister, MoveLocation,
    PhysicalRegister,
};
use crate::opt::pass::MachinePass;

pub struct StackFrameOptimizationPass;

impl Default for StackFrameOptimizationPass {
    fn default() -> Self {
        Self::new()
    }
}

impl StackFrameOptimizationPass {
    pub fn new() -> Self {
        Self
    }
}

impl MachinePass for StackFrameOptimizationPass {
    fn name(&self) -> &'static str {
        "StackFrameOptimization"
    }

    fn run_on_function(&mut self, func: &mut MachineFunction) -> Result<bool, CodegenError> {
        let mut usage = FrameUsage::default();
        for block in &func.blocks {
            for inst in &block.instructions {
                usage.visit(inst);
            }
        }

        // The frame pointer copied into a value means a frame address escaped
        // (e.g. a pointer to an on-stack argument array handed to a call).
        // Accesses through it are invisible here, so the frame must not shrink.
        if usage.frame_pointer_escapes {
            return Ok(false);
        }

        let initial_size = func.stack_size;
        let required_bytes = match usage.deepest_offset {
            Some(min) if min < 0 => ((-min) as u64 + 15) & !15,
            _ => 0,
        };
        if required_bytes < initial_size {
            func.stack_size = required_bytes;
            return Ok(true);
        }
        Ok(false)
    }
}

/// x86-64 frame pointer (RBP); frame slots are negative displacements from it.
const FRAME_POINTER: u8 = 5;

/// Every frame access a function makes. The walk is exhaustive over
/// instruction kinds so a new kind cannot silently hide a slot access: an
/// under-counted frame puts live locals below the stack pointer, where the
/// next call overwrites them.
#[derive(Default)]
struct FrameUsage {
    deepest_offset: Option<i32>,
    frame_pointer_escapes: bool,
}

impl FrameUsage {
    fn slot(&mut self, offset: i32) {
        self.deepest_offset = Some(self.deepest_offset.map_or(offset, |d| d.min(offset)));
    }

    fn operand(&mut self, op: &MachineOperand) {
        match op {
            MachineOperand::StackSlot(slot) => self.slot(*slot),
            MachineOperand::Memory {
                base,
                index,
                offset,
            } => {
                if *base == MachineRegister::Physical(PhysicalRegister(FRAME_POINTER)) {
                    self.slot(*offset);
                }
                if let Some((MachineRegister::Physical(PhysicalRegister(FRAME_POINTER)), _)) = index
                {
                    self.frame_pointer_escapes = true;
                }
            }
            MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(
                FRAME_POINTER,
            ))) => self.frame_pointer_escapes = true,
            _ => {}
        }
    }

    fn location(&mut self, loc: &MoveLocation) {
        match loc {
            MoveLocation::PhysicalRegister(PhysicalRegister(FRAME_POINTER)) => {
                self.frame_pointer_escapes = true
            }
            MoveLocation::StackSlot { base, offset } if base.0 == FRAME_POINTER => {
                self.slot(*offset)
            }
            MoveLocation::Memory { base, offset, .. }
                if *base == MachineRegister::Physical(PhysicalRegister(FRAME_POINTER)) =>
            {
                self.slot(*offset)
            }
            _ => {}
        }
    }

    fn visit(&mut self, inst: &MachineInstruction) {
        match inst {
            MachineInstruction::Nop
            | MachineInstruction::Return
            | MachineInstruction::Branch { .. }
            | MachineInstruction::BranchCc { .. }
            | MachineInstruction::Barrier => {}
            MachineInstruction::ParallelMove { moves } => {
                for m in moves {
                    self.location(&m.dst);
                    self.location(&m.src);
                }
            }
            MachineInstruction::Move { dst, src }
            | MachineInstruction::Load { dst, src, .. }
            | MachineInstruction::Store { dst, src, .. }
            | MachineInstruction::Add { dst, src }
            | MachineInstruction::Sub { dst, src }
            | MachineInstruction::Mul { dst, src }
            | MachineInstruction::Div { dst, src }
            | MachineInstruction::Mod { dst, src }
            | MachineInstruction::And { dst, src }
            | MachineInstruction::Or { dst, src }
            | MachineInstruction::Xor { dst, src }
            | MachineInstruction::Shl { dst, src }
            | MachineInstruction::Shr { dst, src }
            | MachineInstruction::Sar { dst, src }
            | MachineInstruction::FAdd { dst, src, .. }
            | MachineInstruction::FSub { dst, src, .. }
            | MachineInstruction::FMul { dst, src, .. }
            | MachineInstruction::FDiv { dst, src, .. }
            | MachineInstruction::FCmp {
                lhs: dst, rhs: src, ..
            }
            | MachineInstruction::FCvtIntToFloat { dst, src, .. }
            | MachineInstruction::FCvtFloatToInt { dst, src, .. }
            | MachineInstruction::FCvtFloatToFloat { dst, src, .. }
            | MachineInstruction::VectorAdd { dst, src, .. }
            | MachineInstruction::VectorSub { dst, src, .. }
            | MachineInstruction::VectorMul { dst, src, .. }
            | MachineInstruction::VectorDiv { dst, src, .. }
            | MachineInstruction::VectorAnd { dst, src, .. }
            | MachineInstruction::VectorOr { dst, src, .. }
            | MachineInstruction::VectorXor { dst, src, .. }
            | MachineInstruction::VectorLoad { dst, src, .. }
            | MachineInstruction::VectorStore { dst, src, .. }
            | MachineInstruction::VectorBroadcast { dst, src, .. }
            | MachineInstruction::VectorShuffle { dst, src, .. }
            | MachineInstruction::VectorReduceAdd { dst, src, .. }
            | MachineInstruction::VectorMin { dst, src, .. }
            | MachineInstruction::VectorMax { dst, src, .. }
            | MachineInstruction::VectorCmp { dst, src, .. }
            | MachineInstruction::VectorBlend { dst, src, .. }
            | MachineInstruction::VectorShiftLeft { dst, src, .. }
            | MachineInstruction::VectorShiftRight { dst, src, .. }
            | MachineInstruction::AtomicLoad { dst, src, .. }
            | MachineInstruction::AtomicStore { dst, src, .. }
            | MachineInstruction::AtomicExchange { dst, src, .. }
            | MachineInstruction::AtomicFetchAdd { dst, src, .. } => {
                self.operand(dst);
                self.operand(src);
            }
            MachineInstruction::AtomicCompareExchange {
                dst,
                expected,
                desired,
                ..
            } => {
                self.operand(dst);
                self.operand(expected);
                self.operand(desired);
            }
            MachineInstruction::Compare { lhs, rhs } | MachineInstruction::Test { lhs, rhs } => {
                self.operand(lhs);
                self.operand(rhs);
            }
            MachineInstruction::Neg { dst }
            | MachineInstruction::Not { dst }
            | MachineInstruction::FNeg { dst, .. }
            | MachineInstruction::SetCc { dst, .. }
            | MachineInstruction::Push { src: dst }
            | MachineInstruction::Pop { dst }
            | MachineInstruction::TlsAddress { dst, .. } => self.operand(dst),
            MachineInstruction::Call { target, .. } => self.operand(target),
            MachineInstruction::Custom { operands, .. } => {
                for op in operands {
                    self.operand(op);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame_store(offset: i32) -> MachineInstruction {
        MachineInstruction::Store {
            dst: MachineOperand::Memory {
                base: MachineRegister::Physical(PhysicalRegister(FRAME_POINTER)),
                offset,
                index: None,
            },
            src: MachineOperand::Immediate(1),
            size: 8,
        }
    }

    /// RBP-relative `Memory` accesses used to be ignored, so the frame shrank
    /// below live locals and calls overwrote them.
    #[test]
    fn keeps_frame_covering_rbp_memory_accesses() {
        let mut func = MachineFunction::new("f");
        func.stack_size = 128;
        func.entry_block_mut().push(frame_store(-64));
        let mut pass = StackFrameOptimizationPass::new();
        pass.run_on_function(&mut func).unwrap();
        assert_eq!(func.stack_size, 64);
    }

    #[test]
    fn escaped_frame_pointer_disables_shrinking() {
        let mut func = MachineFunction::new("f");
        func.stack_size = 64;
        let v = func.alloc_vreg();
        func.entry_block_mut().push(frame_store(-24));
        func.entry_block_mut().push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(v)),
            src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(
                FRAME_POINTER,
            ))),
        });
        let mut pass = StackFrameOptimizationPass::new();
        assert!(!pass.run_on_function(&mut func).unwrap());
        assert_eq!(func.stack_size, 64);
    }

    #[test]
    fn unused_frame_is_removed() {
        let mut func = MachineFunction::new("f");
        func.stack_size = 32;
        func.entry_block_mut().push(MachineInstruction::Return);
        let mut pass = StackFrameOptimizationPass::new();
        assert!(pass.run_on_function(&mut func).unwrap());
        assert_eq!(func.stack_size, 0);
    }
}
