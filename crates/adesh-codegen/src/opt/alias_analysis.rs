//! Alias Analysis, Dead-Store Elimination (DSE), and Load-Store Forwarding.
//!
//! Provides progressive alias queries (stack slots, immutable globals, base+offset disjoint ranges)
//! and optimizations to eliminate redundant memory operations.

use crate::error::CodegenError;
use crate::machine_ir::{MachineFunction, MachineInstruction, MachineOperand, MachineRegister};
use crate::opt::memory_effects::MemoryEffect;
use crate::opt::pass::MachinePass;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AliasResult {
    NoAlias,
    MayAlias,
    MustAlias,
}

pub struct AliasAnalysis;

impl AliasAnalysis {
    /// Determine aliasing between two memory operands.
    pub fn alias(op1: &MachineOperand, op2: &MachineOperand) -> AliasResult {
        match (op1, op2) {
            // Level 1: Distinct stack slots never alias
            (MachineOperand::StackSlot(s1), MachineOperand::StackSlot(s2)) => {
                if s1 == s2 {
                    AliasResult::MustAlias
                } else {
                    AliasResult::NoAlias
                }
            }

            // Level 2 & 3: Memory operands with base and offset
            (
                MachineOperand::Memory {
                    base: b1,
                    offset: off1,
                    index: idx1,
                    ..
                },
                MachineOperand::Memory {
                    base: b2,
                    offset: off2,
                    index: idx2,
                    ..
                },
            ) => {
                if b1 == b2 && idx1 == idx2 {
                    if off1 == off2 {
                        AliasResult::MustAlias
                    } else if (off1 - off2).abs() >= 8 {
                        // 8-byte aligned disjoint offsets from same base
                        AliasResult::NoAlias
                    } else {
                        AliasResult::MayAlias
                    }
                } else {
                    AliasResult::MayAlias
                }
            }

            // Stack slot vs non-stack memory
            (MachineOperand::StackSlot(_), MachineOperand::Memory { base, .. })
            | (MachineOperand::Memory { base, .. }, MachineOperand::StackSlot(_)) => {
                // If base is stack pointer (RSP/RBP), they might alias, otherwise they are independent heaps/globals
                if let MachineRegister::Physical(p) = base {
                    if p.0 == 4 || p.0 == 5 {
                        AliasResult::MayAlias
                    } else {
                        AliasResult::NoAlias
                    }
                } else {
                    AliasResult::MayAlias
                }
            }

            _ => AliasResult::MayAlias,
        }
    }
}

/// Dead-Store Elimination Pass (DSE).
/// Removes stores that are immediately overwritten without an intervening read or aliasing call.
pub struct DeadStoreEliminationPass;

impl Default for DeadStoreEliminationPass {
    fn default() -> Self {
        Self::new()
    }
}

impl DeadStoreEliminationPass {
    pub fn new() -> Self {
        Self
    }
}

impl MachinePass for DeadStoreEliminationPass {
    fn name(&self) -> &'static str {
        "DeadStoreElimination"
    }

    fn run_on_function(&mut self, func: &mut MachineFunction) -> Result<bool, CodegenError> {
        let mut changed = false;

        for block in &mut func.blocks {
            let mut live_stores: HashMap<i32, usize> = HashMap::new(); // slot -> instruction index in block
            let mut dead_indices = Vec::new();

            for (idx, inst) in block.instructions.iter().enumerate() {
                let effect = MemoryEffect::of_instruction(inst);

                if effect.may_read() {
                    // Any read or unknown call clears active live stores that might be read
                    live_stores.clear();
                }

                if let MachineInstruction::Store {
                    dst: MachineOperand::StackSlot(slot),
                    ..
                } = inst
                    && let Some(prev_idx) = live_stores.insert(*slot, idx)
                {
                    // Previous store to this same slot was never read before this overwrite!
                    dead_indices.push(prev_idx);
                    changed = true;
                }
            }

            if !dead_indices.is_empty() {
                dead_indices.sort_unstable();
                for &dead_idx in dead_indices.iter().rev() {
                    block.instructions.remove(dead_idx);
                }
            }
        }

        Ok(changed)
    }
}

/// Load-Store Forwarding Pass.
/// If a value is stored to a stack slot and subsequently loaded with no intervening write/call,
/// forward the register directly instead of performing the redundant memory load.
pub struct LoadStoreForwardingPass;

impl Default for LoadStoreForwardingPass {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadStoreForwardingPass {
    pub fn new() -> Self {
        Self
    }
}

impl MachinePass for LoadStoreForwardingPass {
    fn name(&self) -> &'static str {
        "LoadStoreForwarding"
    }

    fn run_on_function(&mut self, func: &mut MachineFunction) -> Result<bool, CodegenError> {
        let mut changed = false;

        for block in &mut func.blocks {
            let mut store_map: HashMap<i32, MachineOperand> = HashMap::new(); // slot -> stored register value

            for inst in &mut block.instructions {
                let effect = MemoryEffect::of_instruction(inst);

                match inst {
                    MachineInstruction::Store {
                        dst: MachineOperand::StackSlot(slot),
                        src,
                        ..
                    } => {
                        store_map.insert(*slot, src.clone());
                    }

                    MachineInstruction::Load {
                        dst,
                        src: MachineOperand::StackSlot(slot),
                        ..
                    } => {
                        if let Some(forwarded_src) = store_map.get(slot) {
                            // Forward the store source directly!
                            *inst = MachineInstruction::Move {
                                dst: dst.clone(),
                                src: forwarded_src.clone(),
                            };
                            changed = true;
                        }
                    }

                    _ => {
                        if effect.may_write() {
                            store_map.clear();
                        }
                    }
                }
            }
        }

        Ok(changed)
    }
}
