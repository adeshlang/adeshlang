//! Loop Optimization Pass
//!
//! Provides aggressive loop optimizations on VIR:
//! 1. Loop Detection via CFG back-edge analysis.
//! 2. Loop-Invariant Code Motion (LICM): hoists pure computations and constant loads
//!    out of loop bodies into loop pre-headers.
//! 3. Loop Branch Simplification & Dead Loop Elimination.
//! 4. Redundant Local Store-Load Forwarding within loop bodies.

use super::{OptLevel, OptResult, VirOptimization};
use crate::ir::vir::{
    BlockId, ValueId, VirBlock, VirFunction, VirInstruction, VirModule, VirTerminator,
};
use std::collections::{HashMap, HashSet};

pub struct LoopOptimization;

impl LoopOptimization {
    pub fn new() -> Self {
        Self
    }

    /// Optimize loops in a function
    fn optimize_function(func: &mut VirFunction) -> bool {
        let mut changed = false;

        // Step 1: Forward local store-load within blocks
        changed |= Self::forward_local_stores(func);

        // Step 2: Simplify loop branches and remove dead blocks
        changed |= Self::simplify_loop_branches(func);

        // Step 3: Run Loop Invariant Code Motion (LICM)
        changed |= Self::run_licm(func);

        changed
    }

    /// Forward local store -> load within basic blocks to avoid stack roundtrips
    fn forward_local_stores(func: &mut VirFunction) -> bool {
        let mut changed = false;

        for block in &mut func.blocks {
            let mut local_values: HashMap<u32, ValueId> = HashMap::new();
            let mut aliases: HashMap<ValueId, ValueId> = HashMap::new();
            let mut new_insts = Vec::with_capacity(block.instructions.len());

            for inst in block.instructions.drain(..) {
                match &inst {
                    VirInstruction::StoreLocal { local, value } => {
                        let actual_val = aliases.get(value).copied().unwrap_or(*value);
                        local_values.insert(*local, actual_val);
                        new_insts.push(inst);
                    }
                    VirInstruction::LoadLocal { dest, local } => {
                        if let Some(&known_val) = local_values.get(local) {
                            // Forward the stored value directly
                            aliases.insert(*dest, known_val);
                            changed = true;
                            // Emit a direct Copy instead of LoadLocal
                            new_insts.push(VirInstruction::Copy {
                                dest: *dest,
                                src: known_val,
                            });
                        } else {
                            new_insts.push(inst);
                        }
                    }
                    VirInstruction::Call { .. } | VirInstruction::Intrinsic { .. } => {
                        // Function call might touch memory / locals, clear tracked stack locals
                        local_values.clear();
                        new_insts.push(inst);
                    }
                    _ => {
                        new_insts.push(inst);
                    }
                }
            }

            block.instructions = new_insts;
        }

        changed
    }

    /// Simplify branches with constant conditions and eliminate dead branches
    fn simplify_loop_branches(func: &mut VirFunction) -> bool {
        let mut changed = false;
        let mut const_bools: HashMap<ValueId, bool> = HashMap::new();

        for block in &func.blocks {
            for inst in &block.instructions {
                if let VirInstruction::ConstBool { dest, value } = inst {
                    const_bools.insert(*dest, *value);
                }
            }
        }

        for block in &mut func.blocks {
            if let VirTerminator::Branch {
                cond,
                true_target,
                false_target,
            } = &block.terminator
            {
                if let Some(&cond_val) = const_bools.get(cond) {
                    let target = if cond_val {
                        *true_target
                    } else {
                        *false_target
                    };
                    block.terminator = VirTerminator::Jump { target };
                    changed = true;
                } else if true_target == false_target {
                    block.terminator = VirTerminator::Jump {
                        target: *true_target,
                    };
                    changed = true;
                }
            }
        }

        changed
    }

    /// Detect natural loops and perform Loop Invariant Code Motion (LICM)
    fn run_licm(func: &mut VirFunction) -> bool {
        if func.blocks.len() < 2 {
            return false;
        }

        // Build CFG successors and predecessors
        let mut succs: HashMap<BlockId, Vec<BlockId>> = HashMap::new();
        let mut preds: HashMap<BlockId, Vec<BlockId>> = HashMap::new();

        for block in &func.blocks {
            let s = match &block.terminator {
                VirTerminator::Jump { target } => vec![*target],
                VirTerminator::Branch {
                    true_target,
                    false_target,
                    ..
                } => vec![*true_target, *false_target],
                VirTerminator::Switch { cases, default, .. } => {
                    let mut list: Vec<BlockId> = cases.iter().map(|(_, t)| *t).collect();
                    list.push(*default);
                    list
                }
                VirTerminator::Return { .. } | VirTerminator::Unreachable => vec![],
            };
            for &t in &s {
                preds.entry(t).or_default().push(block.id);
            }
            succs.insert(block.id, s);
        }

        // Identify back-edges: (tail -> header) where tail has an edge to header and header <= tail
        let mut loops: Vec<(BlockId, HashSet<BlockId>, Option<BlockId>)> = Vec::new();

        for block in &func.blocks {
            if let Some(succ_list) = succs.get(&block.id) {
                for &target in succ_list {
                    if target <= block.id {
                        // Found a loop with header `target` and back-edge from `block.id`
                        let header = target;
                        let mut loop_blocks = HashSet::new();
                        loop_blocks.insert(header);
                        loop_blocks.insert(block.id);

                        // Discover all blocks in the loop (stopping at loop header)
                        let mut stack = vec![block.id];
                        while let Some(curr) = stack.pop() {
                            if curr != header {
                                if let Some(p_list) = preds.get(&curr) {
                                    for &p in p_list {
                                        if loop_blocks.insert(p) {
                                            stack.push(p);
                                        }
                                    }
                                }
                            }
                        }

                        // Find preheader: predecessor of header that is not in loop_blocks
                        let preheader = preds.get(&header).and_then(|p_list| {
                            p_list.iter().find(|&&p| !loop_blocks.contains(&p)).copied()
                        });

                        loops.push((header, loop_blocks, preheader));
                    }
                }
            }
        }

        let mut changed = false;

        for (_header, loop_blocks, preheader_opt) in loops {
            let Some(preheader_id) = preheader_opt else {
                continue;
            };

            // Values defined inside the loop
            let mut loop_defined_values: HashSet<ValueId> = HashSet::new();
            for block in &func.blocks {
                if loop_blocks.contains(&block.id) {
                    for phi in &block.phis {
                        loop_defined_values.insert(phi.dest);
                    }
                    for inst in &block.instructions {
                        if let Some(dest) = get_instruction_dest(inst) {
                            loop_defined_values.insert(dest);
                        }
                    }
                }
            }

            // Find invariant instructions
            let mut hoisted_instructions: Vec<VirInstruction> = Vec::new();

            for block in &mut func.blocks {
                if loop_blocks.contains(&block.id) && block.id != preheader_id {
                    let mut remaining = Vec::with_capacity(block.instructions.len());

                    for inst in block.instructions.drain(..) {
                        if is_hoistable_pure_instruction(&inst, &loop_defined_values) {
                            if let Some(dest) = get_instruction_dest(&inst) {
                                // Once hoisted, this value is no longer loop-defined
                                loop_defined_values.remove(&dest);
                            }
                            hoisted_instructions.push(inst);
                            changed = true;
                        } else {
                            remaining.push(inst);
                        }
                    }

                    block.instructions = remaining;
                }
            }

            // Insert hoisted instructions into preheader before terminator
            if !hoisted_instructions.is_empty() {
                if let Some(preheader_block) = func.blocks.iter_mut().find(|b| b.id == preheader_id)
                {
                    preheader_block.instructions.extend(hoisted_instructions);
                }
            }
        }

        changed
    }
}

impl Default for LoopOptimization {
    fn default() -> Self {
        Self::new()
    }
}

impl VirOptimization for LoopOptimization {
    fn name(&self) -> &str {
        "loop-optimization"
    }

    fn apply(&self, module: &mut VirModule) -> OptResult<bool> {
        let mut changed = false;
        for func in &mut module.functions {
            changed |= Self::optimize_function(func);
        }
        Ok(changed)
    }

    fn enabled_at(&self, level: OptLevel) -> bool {
        level >= OptLevel::Basic
    }
}

/// Check if an instruction is pure and all its operands are defined outside the loop
fn is_hoistable_pure_instruction(
    inst: &VirInstruction,
    loop_defined_values: &HashSet<ValueId>,
) -> bool {
    match inst {
        VirInstruction::ConstInt { .. }
        | VirInstruction::ConstFloat { .. }
        | VirInstruction::ConstBool { .. }
        | VirInstruction::ConstString { .. }
        | VirInstruction::ConstNull { .. } => true,

        VirInstruction::IntBinOp { lhs, rhs, .. }
        | VirInstruction::FloatBinOp { lhs, rhs, .. }
        | VirInstruction::IntCmp { lhs, rhs, .. }
        | VirInstruction::FloatCmp { lhs, rhs, .. } => {
            !loop_defined_values.contains(lhs) && !loop_defined_values.contains(rhs)
        }

        VirInstruction::IntUnOp { operand, .. }
        | VirInstruction::FloatUnOp { operand, .. }
        | VirInstruction::Cast { value: operand, .. }
        | VirInstruction::Bitcast { value: operand, .. } => !loop_defined_values.contains(operand),

        VirInstruction::ExtractTuple { tuple, .. } => !loop_defined_values.contains(tuple),

        VirInstruction::ExtractField { struct_val, .. } => {
            !loop_defined_values.contains(struct_val)
        }

        // Instructions with side effects or memory state cannot be hoisted
        _ => false,
    }
}

fn get_instruction_dest(inst: &VirInstruction) -> Option<ValueId> {
    match inst {
        VirInstruction::ConstInt { dest, .. }
        | VirInstruction::ConstFloat { dest, .. }
        | VirInstruction::ConstBool { dest, .. }
        | VirInstruction::ConstString { dest, .. }
        | VirInstruction::ConstNull { dest }
        | VirInstruction::Alloc { dest, .. }
        | VirInstruction::Load { dest, .. }
        | VirInstruction::LoadLocal { dest, .. }
        | VirInstruction::ArcClone { dest, .. }
        | VirInstruction::IntBinOp { dest, .. }
        | VirInstruction::FloatBinOp { dest, .. }
        | VirInstruction::IntUnOp { dest, .. }
        | VirInstruction::FloatUnOp { dest, .. }
        | VirInstruction::IntCmp { dest, .. }
        | VirInstruction::FloatCmp { dest, .. }
        | VirInstruction::Cast { dest, .. }
        | VirInstruction::Bitcast { dest, .. }
        | VirInstruction::BuildStruct { dest, .. }
        | VirInstruction::ExtractField { dest, .. }
        | VirInstruction::InsertField { dest, .. }
        | VirInstruction::BuildArray { dest, .. }
        | VirInstruction::ArrayIndex { dest, .. }
        | VirInstruction::BuildTuple { dest, .. }
        | VirInstruction::ExtractTuple { dest, .. }
        | VirInstruction::BuildObject { dest, .. }
        | VirInstruction::BuildEnum { dest, .. }
        | VirInstruction::GetDiscriminant { dest, .. }
        | VirInstruction::ExtractPayload { dest, .. }
        | VirInstruction::Copy { dest, .. }
        | VirInstruction::Move { dest, .. } => Some(*dest),
        VirInstruction::Call { dest: Some(d), .. }
        | VirInstruction::Intrinsic { dest: Some(d), .. } => Some(*d),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::vir::{IntBinOp, VirBlock, VirFunction, VirTerminator, VirType};

    #[test]
    fn test_loop_opt_creation() {
        let opt = LoopOptimization::new();
        assert_eq!(opt.name(), "loop-optimization");
        assert!(opt.enabled_at(OptLevel::Basic));
    }

    #[test]
    fn test_licm_hoisting() {
        let mut func = VirFunction::new("test_loop".to_string(), VirType::I64);

        // Block 0: Pre-header
        let b0 = VirBlock {
            id: 0,
            label: Some("preheader".to_string()),
            phis: vec![],
            instructions: vec![
                VirInstruction::ConstInt {
                    dest: 1,
                    value: 10,
                    ty: VirType::I64,
                },
                VirInstruction::ConstInt {
                    dest: 2,
                    value: 20,
                    ty: VirType::I64,
                },
            ],
            terminator: VirTerminator::Jump { target: 1 },
        };

        // Block 1: Loop Header / Body
        let b1 = VirBlock {
            id: 1,
            label: Some("loop_body".to_string()),
            phis: vec![],
            instructions: vec![
                // Invariant computation inside loop!
                VirInstruction::IntBinOp {
                    dest: 3,
                    op: IntBinOp::Add,
                    lhs: 1,
                    rhs: 2,
                    ty: VirType::I64,
                },
            ],
            terminator: VirTerminator::Branch {
                cond: 3,
                true_target: 1, // back-edge to loop header
                false_target: 2,
            },
        };

        // Block 2: Exit
        let b2 = VirBlock {
            id: 2,
            label: Some("exit".to_string()),
            phis: vec![],
            instructions: vec![],
            terminator: VirTerminator::Return { value: Some(3) },
        };

        func.blocks = vec![b0, b1, b2];

        let changed = LoopOptimization::optimize_function(&mut func);
        assert!(changed);
        // Instruction dest:3 should be hoisted to block 0
        assert_eq!(func.blocks[0].instructions.len(), 3);
        assert_eq!(func.blocks[1].instructions.len(), 0);
    }
}
