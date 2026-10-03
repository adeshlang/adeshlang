//! Memory Effects Modeling and Instruction Classification.
//!
//! Categorizes machine instructions based on their side-effects on memory (reads, writes, calls, barriers).

use crate::machine_ir::{MachineInstruction, MachineOperand};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemoryEffect {
    NoMemoryEffect,
    ReadOnly,
    WriteOnly,
    ReadWrite,
    Volatile,
    Atomic,
    Unknown,
}

impl MemoryEffect {
    /// Classify the memory effect of a given MachineInstruction.
    pub fn of_instruction(inst: &MachineInstruction) -> Self {
        match inst {
            MachineInstruction::Nop
            | MachineInstruction::Return
            | MachineInstruction::Branch { .. }
            | MachineInstruction::BranchCc { .. } => MemoryEffect::NoMemoryEffect,

            MachineInstruction::Move { dst, src } => {
                let dst_mem = matches!(
                    dst,
                    MachineOperand::Memory { .. } | MachineOperand::StackSlot(_)
                );
                let src_mem = matches!(
                    src,
                    MachineOperand::Memory { .. } | MachineOperand::StackSlot(_)
                );
                match (dst_mem, src_mem) {
                    (true, true) => MemoryEffect::ReadWrite,
                    (true, false) => MemoryEffect::WriteOnly,
                    (false, true) => MemoryEffect::ReadOnly,
                    (false, false) => MemoryEffect::NoMemoryEffect,
                }
            }

            MachineInstruction::Load { .. } => MemoryEffect::ReadOnly,
            MachineInstruction::Store { .. } => MemoryEffect::WriteOnly,

            MachineInstruction::Push { .. } => MemoryEffect::WriteOnly,
            MachineInstruction::Pop { .. } => MemoryEffect::ReadOnly,

            MachineInstruction::VectorLoad { .. } => MemoryEffect::ReadOnly,
            MachineInstruction::VectorStore { .. } => MemoryEffect::WriteOnly,

            MachineInstruction::AtomicLoad { .. }
            | MachineInstruction::AtomicStore { .. }
            | MachineInstruction::AtomicFetchAdd { .. }
            | MachineInstruction::AtomicCompareExchange { .. } => MemoryEffect::Atomic,
            MachineInstruction::Barrier => MemoryEffect::Volatile,

            MachineInstruction::Call { .. } => MemoryEffect::Unknown, // Calls conservatively invalidate/read memory

            _ => MemoryEffect::NoMemoryEffect,
        }
    }

    #[inline]
    pub fn may_read(&self) -> bool {
        matches!(
            self,
            MemoryEffect::ReadOnly
                | MemoryEffect::ReadWrite
                | MemoryEffect::Volatile
                | MemoryEffect::Atomic
                | MemoryEffect::Unknown
        )
    }

    #[inline]
    pub fn may_write(&self) -> bool {
        matches!(
            self,
            MemoryEffect::WriteOnly
                | MemoryEffect::ReadWrite
                | MemoryEffect::Volatile
                | MemoryEffect::Atomic
                | MemoryEffect::Unknown
        )
    }
}
