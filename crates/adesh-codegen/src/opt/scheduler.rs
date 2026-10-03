//! Machine Instruction Scheduling within Basic Blocks.
//!
//! Models instruction dependencies (RAW, WAR, WAW, Memory, Flags, Call/Barrier boundaries)
//! and applies priority-list scheduling using an architecture-aware latency table to reduce pipeline stalls.

use crate::error::CodegenError;
use crate::machine_ir::{
    MachineBlock, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
};
use std::collections::HashSet;

/// Latency model for machine instructions.
#[derive(Debug, Clone, Copy)]
pub struct LatencyModel;

impl LatencyModel {
    pub fn latency_of(inst: &MachineInstruction) -> usize {
        match inst {
            MachineInstruction::Load { .. } | MachineInstruction::VectorLoad { .. } => 3,
            MachineInstruction::Mul { .. }
            | MachineInstruction::FMul { .. }
            | MachineInstruction::VectorMul { .. } => 3,
            MachineInstruction::Div { .. }
            | MachineInstruction::Mod { .. }
            | MachineInstruction::FDiv { .. }
            | MachineInstruction::VectorDiv { .. } => 15,
            MachineInstruction::FAdd { .. }
            | MachineInstruction::FSub { .. }
            | MachineInstruction::FCvtIntToFloat { .. }
            | MachineInstruction::FCvtFloatToInt { .. }
            | MachineInstruction::FCvtFloatToFloat { .. }
            | MachineInstruction::VectorAdd { .. }
            | MachineInstruction::VectorSub { .. } => 2,
            MachineInstruction::Call { .. }
            | MachineInstruction::AtomicFetchAdd { .. }
            | MachineInstruction::AtomicCompareExchange { .. } => 5,
            MachineInstruction::Barrier
            | MachineInstruction::Branch { .. }
            | MachineInstruction::BranchCc { .. }
            | MachineInstruction::Return => 1,
            _ => 1,
        }
    }
}

/// A node in the local Basic Block scheduling DAG.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct SchedNode {
    id: usize,
    inst: MachineInstruction,
    latency: usize,
    defs: HashSet<MachineRegister>,
    uses: HashSet<MachineRegister>,
    has_memory_read: bool,
    has_memory_write: bool,
    is_barrier: bool,
    predecessors: Vec<usize>, // Nodes that must execute BEFORE this node
    successors: Vec<usize>,   // Nodes that must execute AFTER this node
    depth: usize,
    height: usize,
}

impl SchedNode {
    fn new(id: usize, inst: MachineInstruction) -> Self {
        let defs: HashSet<MachineRegister> = inst.defs().into_iter().collect();
        let uses: HashSet<MachineRegister> = inst.uses().into_iter().collect();
        let latency = LatencyModel::latency_of(&inst);

        let mut has_mem_read = false;
        let mut has_mem_write = false;
        let is_barrier = matches!(
            inst,
            MachineInstruction::Barrier
                | MachineInstruction::Call { .. }
                | MachineInstruction::Return
                | MachineInstruction::Branch { .. }
                | MachineInstruction::BranchCc { .. }
                | MachineInstruction::AtomicLoad { .. }
                | MachineInstruction::AtomicStore { .. }
                | MachineInstruction::AtomicFetchAdd { .. }
                | MachineInstruction::AtomicCompareExchange { .. }
        );

        match &inst {
            MachineInstruction::Load { .. } | MachineInstruction::VectorLoad { .. } => {
                has_mem_read = true;
            }
            MachineInstruction::Store { .. } | MachineInstruction::VectorStore { .. } => {
                has_mem_write = true;
            }
            MachineInstruction::Move { dst, src } => {
                if matches!(
                    src,
                    MachineOperand::Memory { .. } | MachineOperand::StackSlot(_)
                ) {
                    has_mem_read = true;
                }
                if matches!(
                    dst,
                    MachineOperand::Memory { .. } | MachineOperand::StackSlot(_)
                ) {
                    has_mem_write = true;
                }
            }
            _ => {}
        }

        Self {
            id,
            inst,
            latency,
            defs,
            uses,
            has_memory_read: has_mem_read,
            has_memory_write: has_mem_write,
            is_barrier,
            predecessors: Vec::new(),
            successors: Vec::new(),
            depth: 0,
            height: latency,
        }
    }
}

/// Conservative Basic Block Instruction Scheduler.
pub struct BasicBlockScheduler;

impl Default for BasicBlockScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl BasicBlockScheduler {
    pub fn new() -> Self {
        Self
    }

    /// Schedule instructions inside all basic blocks of a function.
    pub fn schedule_function(&self, func: &mut MachineFunction) -> Result<usize, CodegenError> {
        let mut total_scheduled = 0;
        for block in &mut func.blocks {
            if block.instructions.len() > 2 {
                let reordered = self.schedule_block(block);
                block.instructions = reordered;
                total_scheduled += 1;
            }
        }
        Ok(total_scheduled)
    }

    /// Schedule instructions within a single basic block.
    pub fn schedule_block(&self, block: &MachineBlock) -> Vec<MachineInstruction> {
        let n = block.instructions.len();
        if n <= 2 {
            return block.instructions.clone();
        }

        // Build nodes
        let mut nodes: Vec<SchedNode> = block
            .instructions
            .iter()
            .enumerate()
            .map(|(i, inst)| SchedNode::new(i, inst.clone()))
            .collect();

        // Build dependency DAG
        for i in 0..n {
            for j in (i + 1)..n {
                let mut has_dep = false;

                // 1. RAW (True dependency): i defines reg, j uses reg
                if !nodes[i].defs.is_empty()
                    && nodes[i].defs.iter().any(|r| nodes[j].uses.contains(r))
                {
                    has_dep = true;
                }
                // 2. WAR (Anti dependency): i uses reg, j defines reg
                if !nodes[i].uses.is_empty()
                    && nodes[i].uses.iter().any(|r| nodes[j].defs.contains(r))
                {
                    has_dep = true;
                }
                // 3. WAW (Output dependency): i defines reg, j defines reg
                if !nodes[i].defs.is_empty()
                    && nodes[i].defs.iter().any(|r| nodes[j].defs.contains(r))
                {
                    has_dep = true;
                }
                // 4. Memory dependencies (Store-Load, Load-Store, Store-Store)
                if (nodes[i].has_memory_write
                    && (nodes[j].has_memory_read || nodes[j].has_memory_write))
                    || (nodes[i].has_memory_read && nodes[j].has_memory_write)
                {
                    has_dep = true;
                }
                // 5. Barriers, control flow terminators, atomics
                if nodes[i].is_barrier || nodes[j].is_barrier {
                    has_dep = true;
                }

                if has_dep {
                    nodes[i].successors.push(j);
                    nodes[j].predecessors.push(i);
                }
            }
        }

        // Compute heights from bottom up (longest path to sink)
        for i in (0..n).rev() {
            let max_succ_height = nodes[i]
                .successors
                .iter()
                .map(|&s| nodes[s].height)
                .max()
                .unwrap_or(0);
            nodes[i].height = nodes[i].latency + max_succ_height;
        }

        // Priority list scheduling
        let mut in_degree: Vec<usize> = nodes.iter().map(|nd| nd.predecessors.len()).collect();
        let mut ready: Vec<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
        let mut scheduled: Vec<MachineInstruction> = Vec::with_capacity(n);

        while !ready.is_empty() {
            // Pick ready node with highest critical-path height, break ties with original program order
            ready.sort_by(|&a, &b| {
                nodes[b]
                    .height
                    .cmp(&nodes[a].height)
                    .then_with(|| a.cmp(&b))
            });

            let best = ready.remove(0);
            scheduled.push(nodes[best].inst.clone());

            for &succ in &nodes[best].successors {
                in_degree[succ] -= 1;
                if in_degree[succ] == 0 {
                    ready.push(succ);
                }
            }
        }

        // Safety fallback: if DAG had an unexpected cycle or leftover node, preserve original order
        if scheduled.len() == n {
            scheduled
        } else {
            block.instructions.clone()
        }
    }
}
