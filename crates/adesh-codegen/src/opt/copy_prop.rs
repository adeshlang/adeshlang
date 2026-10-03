//! Copy Propagation and Redundant Move Elimination Pass.
//!
//! Replaces uses of copied virtual registers directly with their original source operands,
//! reducing register pressure and eliminating useless copy instructions.

use crate::error::CodegenError;
use crate::machine_ir::{
    MachineFunction, MachineInstruction, MachineOperand, MachineRegister, VirtualRegister,
};
use crate::opt::pass::MachinePass;
use std::collections::HashMap;

pub struct CopyPropagationPass;

impl Default for CopyPropagationPass {
    fn default() -> Self {
        Self::new()
    }
}

impl CopyPropagationPass {
    pub fn new() -> Self {
        Self
    }

    fn replace_reg_use(
        op: &mut MachineOperand,
        copies: &HashMap<VirtualRegister, VirtualRegister>,
    ) -> bool {
        match op {
            MachineOperand::Register(MachineRegister::Virtual(v)) => {
                if let Some(&orig) = copies.get(v) {
                    *op = MachineOperand::Register(MachineRegister::Virtual(orig));
                    return true;
                }
            }
            MachineOperand::Memory { base, index, .. } => {
                let mut modified = false;
                if let MachineRegister::Virtual(v) = base
                    && let Some(&orig) = copies.get(v)
                {
                    *base = MachineRegister::Virtual(orig);
                    modified = true;
                }
                if let Some((MachineRegister::Virtual(v), _)) = index
                    && let Some(&orig) = copies.get(v)
                {
                    *index = Some((MachineRegister::Virtual(orig), index.unwrap().1));
                    modified = true;
                }
                return modified;
            }
            _ => {}
        }
        false
    }
}

impl MachinePass for CopyPropagationPass {
    fn name(&self) -> &'static str {
        "CopyPropagation"
    }

    fn run_on_function(&mut self, func: &mut MachineFunction) -> Result<bool, CodegenError> {
        let mut changed = false;

        for block in &mut func.blocks {
            let mut copies: HashMap<VirtualRegister, VirtualRegister> = HashMap::new();
            let mut rewritten = Vec::with_capacity(block.instructions.len());

            for mut inst in block.instructions.drain(..) {
                // 1. Remove identity moves: mov x, x
                if let MachineInstruction::Move { dst, src } = &inst
                    && dst == src
                {
                    changed = true;
                    continue;
                }

                // 2. Substitute uses in inst using current copy map
                match &mut inst {
                    MachineInstruction::Move { dst: _, src } => {
                        if Self::replace_reg_use(src, &copies) {
                            changed = true;
                        }
                    }
                    MachineInstruction::Add { dst: _, src }
                    | MachineInstruction::Sub { dst: _, src }
                    | MachineInstruction::Mul { dst: _, src }
                    | MachineInstruction::Div { dst: _, src }
                    | MachineInstruction::Mod { dst: _, src }
                    | MachineInstruction::And { dst: _, src }
                    | MachineInstruction::Or { dst: _, src }
                    | MachineInstruction::Xor { dst: _, src }
                    | MachineInstruction::Shl { dst: _, src }
                    | MachineInstruction::Shr { dst: _, src }
                    | MachineInstruction::Sar { dst: _, src }
                    | MachineInstruction::FAdd { dst: _, src, .. }
                    | MachineInstruction::FSub { dst: _, src, .. }
                    | MachineInstruction::FMul { dst: _, src, .. }
                    | MachineInstruction::FDiv { dst: _, src, .. } => {
                        if Self::replace_reg_use(src, &copies) {
                            changed = true;
                        }
                    }
                    MachineInstruction::Compare { lhs, rhs }
                    | MachineInstruction::Test { lhs, rhs } => {
                        if Self::replace_reg_use(lhs, &copies) {
                            changed = true;
                        }
                        if Self::replace_reg_use(rhs, &copies) {
                            changed = true;
                        }
                    }
                    MachineInstruction::FCmp { lhs, rhs, .. } => {
                        if Self::replace_reg_use(lhs, &copies) {
                            changed = true;
                        }
                        if Self::replace_reg_use(rhs, &copies) {
                            changed = true;
                        }
                    }
                    MachineInstruction::Push { src } => {
                        if Self::replace_reg_use(src, &copies) {
                            changed = true;
                        }
                    }
                    MachineInstruction::Call { target, .. } => {
                        if Self::replace_reg_use(target, &copies) {
                            changed = true;
                        }
                    }
                    _ => {}
                }

                // 3. Update copy map if this is a vreg-to-vreg Move
                if let MachineInstruction::Move {
                    dst: MachineOperand::Register(MachineRegister::Virtual(dst_v)),
                    src: MachineOperand::Register(MachineRegister::Virtual(src_v)),
                } = &inst
                {
                    // If src_v is itself a copy of root_v, map dst_v to root_v
                    let root_v = copies.get(src_v).copied().unwrap_or(*src_v);
                    if *dst_v != root_v {
                        copies.insert(*dst_v, root_v);
                    }
                } else {
                    // Any definition of a register kills its presence as a copy destination or source
                    for d in inst.defs() {
                        if let MachineRegister::Virtual(v) = d {
                            copies.remove(&v);
                            copies.retain(|_, &mut src| src != v);
                        }
                    }
                }

                rewritten.push(inst);
            }

            block.instructions = rewritten;
        }

        Ok(changed)
    }
}
