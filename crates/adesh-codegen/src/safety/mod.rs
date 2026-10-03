//! Memory Safety, Stack Protection, and Runtime Safety Framework for Adesh.

use crate::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    VirtualRegister,
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

        // 1. Allocate virtual registers and a stack slot for the canary.
        //
        // The slot must live *below* the frame (negative RBP offset) and the
        // frame must grow to cover it: a positive `[rbp + stack_size + 8]`
        // offset would sit on top of the caller's saved RBP / return address.
        let canary_vreg = func.alloc_vreg();
        let guard_addr_vreg = func.alloc_vreg();
        let canary_slot = -((func.stack_size as i32) + 8);
        func.stack_size += 8;

        // 2. Insert canary initialization in the entry block.
        //
        // `Move reg, Symbol(name)` materialises the *address* of the symbol
        // (mov r64, imm64 + ABS64 relocation), so the guard value is read from
        // that address with an explicit load. Comparing against a `Symbol`
        // operand directly would have no encoding.
        let entry_block = &mut func.blocks[0];
        let mut prologue_insts = Vec::new();
        prologue_insts.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(guard_addr_vreg)),
            src: MachineOperand::Symbol("__stack_chk_guard".to_string()),
        });
        prologue_insts.push(MachineInstruction::Load {
            dst: MachineOperand::Register(MachineRegister::Virtual(canary_vreg)),
            src: guard_memory_operand(guard_addr_vreg),
            size: 8,
        });
        prologue_insts.push(MachineInstruction::Store {
            dst: MachineOperand::StackSlot(canary_slot),
            src: MachineOperand::Register(MachineRegister::Virtual(canary_vreg)),
            size: 8,
        });

        // Prepend to entry block instructions
        prologue_insts.append(&mut entry_block.instructions);
        entry_block.instructions = prologue_insts;

        // 3. Insert canary verification before all Return instructions in every
        //    block. The virtual registers needed for each check are allocated up
        //    front (the block loop already borrows `func`).
        let return_count: usize = func
            .blocks
            .iter()
            .map(|b| {
                b.instructions
                    .iter()
                    .filter(|i| matches!(i, MachineInstruction::Return))
                    .count()
            })
            .sum();
        let mut spare_vregs: Vec<VirtualRegister> =
            (0..return_count * 3).map(|_| func.alloc_vreg()).collect();

        for block in &mut func.blocks {
            let mut new_insts = Vec::with_capacity(block.instructions.len() + 4);
            for inst in block.instructions.drain(..) {
                if let MachineInstruction::Return = inst {
                    let addr_vreg = spare_vregs.pop().expect("preallocated canary vregs");
                    let check_vreg = spare_vregs.pop().expect("preallocated canary vregs");
                    let guard_vreg = spare_vregs.pop().expect("preallocated canary vregs");

                    // Address of the guard, then its value.
                    new_insts.push(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(addr_vreg)),
                        src: MachineOperand::Symbol("__stack_chk_guard".to_string()),
                    });
                    new_insts.push(MachineInstruction::Load {
                        dst: MachineOperand::Register(MachineRegister::Virtual(guard_vreg)),
                        src: guard_memory_operand(addr_vreg),
                        size: 8,
                    });
                    // Load canary from stack slot
                    new_insts.push(MachineInstruction::Load {
                        dst: MachineOperand::Register(MachineRegister::Virtual(check_vreg)),
                        src: MachineOperand::StackSlot(canary_slot),
                        size: 8,
                    });
                    // Compare against the guard value
                    new_insts.push(MachineInstruction::Compare {
                        lhs: MachineOperand::Register(MachineRegister::Virtual(check_vreg)),
                        rhs: MachineOperand::Register(MachineRegister::Virtual(guard_vreg)),
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

/// `[guard_addr]` - the memory location holding the stack guard value.
fn guard_memory_operand(addr_vreg: VirtualRegister) -> MachineOperand {
    MachineOperand::Memory {
        base: MachineRegister::Virtual(addr_vreg),
        offset: 0,
        index: None,
    }
}

/// Control Flow Integrity (CFI) / Intel CET `endbr64` and ARM BTI insertion pass.
///
/// `endbr64` is an *indirect branch landing pad*: it must appear at the target
/// of every indirect branch (function entry points reached through function
/// pointers, and indirect jump targets inside a function). Machine IR does not
/// record which blocks are indirect-branch targets, so this pass only
/// instruments the function entry block, which is always a legal landing pad
/// and the only target the backend can prove. Marking every block (as an
/// earlier revision did) both produced instructions the backend rejected and
/// wasted 4 bytes per block; blocks that are indirect targets need explicit
/// `Custom { endbr64 }` markers from the frontend.
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
        let Some(entry) = func.blocks.first_mut() else {
            return;
        };
        if entry.instructions.first().is_some_and(
            |i| matches!(i, MachineInstruction::Custom { name, .. } if name == "endbr64"),
        ) {
            return;
        }
        entry.instructions.insert(
            0,
            MachineInstruction::Custom {
                name: "endbr64".to_string(),
                operands: Vec::new(),
            },
        );
    }
}
