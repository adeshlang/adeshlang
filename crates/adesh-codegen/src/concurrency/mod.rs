//! Thread Safety, Concurrency, and Memory Consistency Model for Adesh.

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

    /// Lower atomic operation into architecture-neutral Machine IR.
    pub fn lower_atomic(op: AtomicOp) -> Vec<MachineInstruction> {
        match op {
            AtomicOp::FetchAdd { dst, val, .. } => vec![MachineInstruction::Atomic {
                op: "fetch_add".to_string(),
                dst,
                src: val,
            }],
            AtomicOp::FetchSub { dst, val, .. } => vec![MachineInstruction::Atomic {
                op: "fetch_sub".to_string(),
                dst,
                src: val,
            }],
            AtomicOp::Fence { .. } => vec![MachineInstruction::Barrier],
            _ => vec![MachineInstruction::Barrier],
        }
    }
}
