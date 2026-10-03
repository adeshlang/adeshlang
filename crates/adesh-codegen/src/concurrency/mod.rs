//! Thread Safety, Concurrency, and Memory Consistency Model for Adesh.

use crate::error::CodegenError;
use crate::machine_ir::{MachineInstruction, MachineOperand};

/// Memory Ordering specification for atomic operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemoryOrder {
    Relaxed,
    Acquire,
    Release,
    AcqRel,
    SeqCst,
}

/// Atomic Operations supported across backends.
#[derive(Debug, Clone, PartialEq)]
pub enum AtomicOp {
    Load {
        dst: MachineOperand,
        src: MachineOperand,
        size: u8,
        order: MemoryOrder,
    },
    Store {
        dst: MachineOperand,
        src: MachineOperand,
        size: u8,
        order: MemoryOrder,
    },
    FetchAdd {
        dst: MachineOperand,
        val: MachineOperand,
        order: MemoryOrder,
    },
    FetchSub {
        dst: MachineOperand,
        val: MachineOperand,
        order: MemoryOrder,
    },
    CompareExchange {
        dst: MachineOperand,
        expected: MachineOperand,
        desired: MachineOperand,
        success_order: MemoryOrder,
        failure_order: MemoryOrder,
    },
    Fence {
        order: MemoryOrder,
    },
}

/// Thread Safety & Concurrency Instrumentation Pass.
pub struct ConcurrencyPass;

impl Default for ConcurrencyPass {
    fn default() -> Self {
        Self::new()
    }
}

impl ConcurrencyPass {
    pub fn new() -> Self {
        Self
    }

    /// Lower an atomic operation into architecture-neutral Machine IR.
    pub fn lower_atomic(op: AtomicOp) -> Result<Vec<MachineInstruction>, CodegenError> {
        match op {
            AtomicOp::Load { dst, src, size, .. } => {
                Ok(vec![MachineInstruction::AtomicLoad { dst, src, size }])
            }
            AtomicOp::Store { dst, src, size, .. } => {
                Ok(vec![MachineInstruction::AtomicStore { dst, src, size }])
            }
            AtomicOp::FetchAdd { dst, val, .. } => Ok(vec![MachineInstruction::AtomicFetchAdd {
                dst,
                src: val,
                size: 8,
            }]),
            AtomicOp::FetchSub { dst, val, .. } => Ok(vec![MachineInstruction::AtomicFetchAdd {
                dst,
                src: val,
                size: 8,
            }]),
            AtomicOp::CompareExchange {
                dst,
                expected,
                desired,
                ..
            } => Ok(vec![MachineInstruction::AtomicCompareExchange {
                dst,
                expected,
                desired,
                size: 8,
            }]),
            AtomicOp::Fence { .. } => Ok(vec![MachineInstruction::Barrier]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine_ir::{MachineRegister, VirtualRegister};

    #[test]
    fn fetch_add_and_fence_lower_to_machine_ir() {
        let add = ConcurrencyPass::lower_atomic(AtomicOp::FetchAdd {
            dst: MachineOperand::Register(MachineRegister::Virtual(VirtualRegister(0))),
            val: MachineOperand::Immediate(1),
            order: MemoryOrder::SeqCst,
        })
        .expect("fetch_add lowers");
        assert!(matches!(
            add.as_slice(),
            [MachineInstruction::AtomicFetchAdd { .. }]
        ));

        let fence = ConcurrencyPass::lower_atomic(AtomicOp::Fence {
            order: MemoryOrder::SeqCst,
        })
        .expect("fence lowers");
        assert_eq!(fence, vec![MachineInstruction::Barrier]);

        let load = ConcurrencyPass::lower_atomic(AtomicOp::Load {
            dst: MachineOperand::Register(MachineRegister::Virtual(VirtualRegister(0))),
            src: MachineOperand::Immediate(0),
            size: 8,
            order: MemoryOrder::SeqCst,
        })
        .expect("load lowers");
        assert!(matches!(
            load.as_slice(),
            [MachineInstruction::AtomicLoad { .. }]
        ));

        let cas = ConcurrencyPass::lower_atomic(AtomicOp::CompareExchange {
            dst: MachineOperand::Register(MachineRegister::Virtual(VirtualRegister(0))),
            expected: MachineOperand::Immediate(0),
            desired: MachineOperand::Immediate(1),
            success_order: MemoryOrder::Acquire,
            failure_order: MemoryOrder::Relaxed,
        })
        .expect("cas lowers");
        assert!(matches!(
            cas.as_slice(),
            [MachineInstruction::AtomicCompareExchange { .. }]
        ));
    }
}
