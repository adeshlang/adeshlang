//! Memory Safety, Stack Protection, and Runtime Safety Framework for Adesh.

use crate::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
};

/// Stack Protection Pass: Inserts canary generation at prologue and canary verification at epilogue.
pub struct StackCanaryPass;

impl Default for StackCanaryPass {
    fn default() -> Self {
        Self::new()
    }
}

impl StackCanaryPass {
    pub fn new() -> Self {
        Self
    }

    pub fn instrument_function(&self, func: &mut MachineFunction) {
        if func.blocks.is_empty() {
            return;
        }

        // 1. Allocate virtual registers and a stack slot for the canary
        let canary_vreg = func.alloc_vreg();
        let check_vreg = func.alloc_vreg();
        let canary_slot = (func.stack_size + 8) as i32;
        func.stack_size += 8;

        // 2. Insert canary initialization in entry block
        let entry_block = &mut func.blocks[0];
        let mut prologue_insts = Vec::new();
        // Load canary guard: mov canary_vreg, [__stack_chk_guard] (represented as symbol load)
        prologue_insts.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(canary_vreg)),
            src: MachineOperand::Symbol("__stack_chk_guard".to_string()),
        });
        // Store canary into stack slot
        prologue_insts.push(MachineInstruction::Store {
            dst: MachineOperand::StackSlot(canary_slot),
            src: MachineOperand::Register(MachineRegister::Virtual(canary_vreg)),
            size: 8,
        });

        // Prepend to entry block instructions
        prologue_insts.append(&mut entry_block.instructions);
        entry_block.instructions = prologue_insts;

        // 3. Insert canary verification before all Return instructions in every block
        for block in &mut func.blocks {
            let mut new_insts = Vec::with_capacity(block.instructions.len() + 4);
            for inst in block.instructions.drain(..) {
                if let MachineInstruction::Return = inst {
                    // Load canary from stack slot
                    new_insts.push(MachineInstruction::Load {
                        dst: MachineOperand::Register(MachineRegister::Virtual(check_vreg)),
                        src: MachineOperand::StackSlot(canary_slot),
                        size: 8,
                    });
                    // Compare against __stack_chk_guard
                    new_insts.push(MachineInstruction::Compare {
                        lhs: MachineOperand::Register(MachineRegister::Virtual(check_vreg)),
                        rhs: MachineOperand::Symbol("__stack_chk_guard".to_string()),
                    });
                    // If not equal, trap / call __stack_chk_fail
                    new_insts.push(MachineInstruction::BranchCc {
                        cc: ConditionCode::NotEqual,
                        target: "__stack_chk_fail".to_string(),
                    });
                    new_insts.push(MachineInstruction::Return);
                } else {
                    new_insts.push(inst);
                }
            }
            block.instructions = new_insts;
        }
    }
}

/// Control Flow Integrity (CFI) / Intel CET `endbr64` and ARM BTI insertion pass.
pub struct ControlFlowIntegrityPass;

impl Default for ControlFlowIntegrityPass {
    fn default() -> Self {
        Self::new()
    }
}

impl ControlFlowIntegrityPass {
    pub fn new() -> Self {
        Self
    }

    pub fn instrument_function(&self, func: &mut MachineFunction) {
        for block in &mut func.blocks {
            // Prepend landing pad instruction if function entry or indirect branch target
            let mut new_insts = vec![MachineInstruction::Custom {
                name: "endbr64".to_string(),
                operands: Vec::new(),
            }];
            new_insts.append(&mut block.instructions);
            block.instructions = new_insts;
        }
    }
}
