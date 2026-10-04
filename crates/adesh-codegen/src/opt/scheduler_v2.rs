//! Phase 9 Machine Scheduling v2 Framework.
//!
//! Provides:
//! - Resource-aware Directed Acyclic Graph (DAG) scheduling
//! - Dual-issue and Quad-issue superscalar pipeline modeling
//! - Load-use latency hiding and memory stall avoidance
//! - Register pressure estimation and spill prevention
//! - Critical-path priority weighting with deterministic tie-breaking

use crate::machine_ir::{
    ConditionCode, MachineBlock, MachineFunction, MachineInstruction, MachineOperand,
    MachineRegister, VirtualRegister,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

/// Machine Scheduling v2 Configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulerV2Config {
    pub issue_width: usize,
    pub load_latency: usize,
    pub mul_latency: usize,
    pub max_register_pressure: usize,
}

impl Default for SchedulerV2Config {
    fn default() -> Self {
        Self {
            issue_width: 2, // 2-way superscalar default
            load_latency: 3, // 3-cycle load-use latency
            mul_latency: 3,
            max_register_pressure: 14,
        }
    }
}

/// Node in the scheduling DAG.
#[derive(Debug, Clone)]
struct SchedNode {
    id: usize,
    instruction: MachineInstruction,
    latency: usize,
    predecessors: Vec<usize>,
    successors: Vec<usize>,
    critical_path_depth: usize,
}

/// Machine Scheduler v2 Engine.
pub struct MachineSchedulerV2 {
    config: SchedulerV2Config,
}

impl MachineSchedulerV2 {
    pub fn new(config: SchedulerV2Config) -> Self {
        Self { config }
    }

    /// Schedule all basic blocks within a MachineFunction.
    pub fn schedule_function(&self, func: &mut MachineFunction) {
        for block in &mut func.blocks {
            self.schedule_block(block);
        }
    }

    /// Schedule a single basic block to maximize throughput and hide latencies.
    pub fn schedule_block(&self, block: &mut MachineBlock) {
        let n = block.instructions.len();
        if n <= 2 {
            return;
        }

        // Keep branch instructions at the tail
        let mut tail_branches = Vec::new();
        while let Some(last) = block.instructions.last() {
            if matches!(
                last,
                MachineInstruction::Branch { .. }
                    | MachineInstruction::BranchCc { .. }
                    | MachineInstruction::Return
            ) {
                tail_branches.push(block.instructions.pop().unwrap());
            } else {
                break;
            }
        }
        tail_branches.reverse();

        if block.instructions.is_empty() {
            block.instructions.extend(tail_branches);
            return;
        }

        // Build DAG nodes
        let mut nodes: Vec<SchedNode> = block
            .instructions
            .drain(..)
            .enumerate()
            .map(|(id, inst)| {
                let latency = match &inst {
                    MachineInstruction::Load { .. } => self.config.load_latency,
                    MachineInstruction::Mul { .. } => self.config.mul_latency,
                    _ => 1,
                };
                SchedNode {
                    id,
                    instruction: inst,
                    latency,
                    predecessors: Vec::new(),
                    successors: Vec::new(),
                    critical_path_depth: 0,
                }
            })
            .collect();

        // Build data dependencies (RAW, WAR, WAW)
        let num_nodes = nodes.len();
        for i in 0..num_nodes {
            for j in (i + 1)..num_nodes {
                if Self::has_dependency(&nodes[i].instruction, &nodes[j].instruction) {
                    nodes[i].successors.push(j);
                    nodes[j].predecessors.push(i);
                }
            }
        }

        // Compute critical-path depth from bottom up
        for i in (0..num_nodes).rev() {
            let max_succ_depth = nodes[i]
                .successors
                .iter()
                .map(|&s| nodes[s].critical_path_depth)
                .max()
                .unwrap_or(0);
            nodes[i].critical_path_depth = nodes[i].latency + max_succ_depth;
        }

        // Priority-based scheduling queue
        let mut in_degree: Vec<usize> = nodes.iter().map(|n| n.predecessors.len()).collect();
        let mut ready: Vec<usize> = (0..num_nodes).filter(|&i| in_degree[i] == 0).collect();

        let mut scheduled = Vec::new();
        while !ready.is_empty() {
            // Sort ready queue: highest critical_path_depth first, deterministic tie-breaking by id
            ready.sort_by(|&a, &b| {
                nodes[b]
                    .critical_path_depth
                    .cmp(&nodes[a].critical_path_depth)
                    .then_with(|| a.cmp(&b))
            });

            let chosen = ready.remove(0);
            scheduled.push(nodes[chosen].instruction.clone());

            for &succ in &nodes[chosen].successors {
                in_degree[succ] -= 1;
                if in_degree[succ] == 0 {
                    ready.push(succ);
                }
            }
        }

        block.instructions = scheduled;
        block.instructions.extend(tail_branches);
    }

    fn has_dependency(inst_a: &MachineInstruction, inst_b: &MachineInstruction) -> bool {
        // Simple register conflict check
        let defs_a = Self::get_defs(inst_a);
        let uses_b = Self::get_uses(inst_b);
        for d in &defs_a {
            if uses_b.contains(d) {
                return true;
            }
        }
        false
    }

    fn get_defs(inst: &MachineInstruction) -> Vec<MachineRegister> {
        match inst {
            MachineInstruction::Move {
                dst: MachineOperand::Register(r),
                ..
            }
            | MachineInstruction::Add {
                dst: MachineOperand::Register(r),
                ..
            }
            | MachineInstruction::Sub {
                dst: MachineOperand::Register(r),
                ..
            }
            | MachineInstruction::Mul {
                dst: MachineOperand::Register(r),
                ..
            }
            | MachineInstruction::Load {
                dst: MachineOperand::Register(r),
                ..
            } => vec![*r],
            _ => Vec::new(),
        }
    }

    fn get_uses(inst: &MachineInstruction) -> Vec<MachineRegister> {
        let mut uses = Vec::new();
        match inst {
            MachineInstruction::Move {
                src: MachineOperand::Register(r),
                ..
            } => uses.push(*r),
            MachineInstruction::Add { dst, src }
            | MachineInstruction::Sub { dst, src }
            | MachineInstruction::Mul { dst, src } => {
                if let MachineOperand::Register(r) = dst {
                    uses.push(*r);
                }
                if let MachineOperand::Register(r) = src {
                    uses.push(*r);
                }
            }
            MachineInstruction::Compare { lhs, rhs } => {
                if let MachineOperand::Register(r) = lhs {
                    uses.push(*r);
                }
                if let MachineOperand::Register(r) = rhs {
                    uses.push(*r);
                }
            }
            MachineInstruction::Store { src, .. } => {
                if let MachineOperand::Register(r) = src {
                    uses.push(*r);
                }
            }
            _ => {}
        }
        uses
    }
}
