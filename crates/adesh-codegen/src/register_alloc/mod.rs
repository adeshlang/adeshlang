//! Register allocation framework supporting Linear Scan, Spilling, and Target-Specific Register Files.

use crate::machine_ir::{
    MachineFunction, MachineInstruction, MachineOperand, MachineRegister, PhysicalRegister,
    VirtualRegister,
};
use std::collections::{HashMap, HashSet};

/// Target-specific physical register file description.
pub trait RegisterFile: Send + Sync {
    fn registers(&self) -> &[PhysicalRegister];
    fn allocatable(&self) -> &[PhysicalRegister];
    fn caller_saved(&self) -> &[PhysicalRegister];
    fn callee_saved(&self) -> &[PhysicalRegister];
    fn reserved(&self) -> &[PhysicalRegister];
}

/// Interval representing a virtual register's live range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveInterval {
    pub vreg: VirtualRegister,
    pub start: usize,
    pub end: usize,
    pub assigned_reg: Option<PhysicalRegister>,
    pub spill_slot: Option<i32>,
}

/// Register Allocation Result.
pub struct AllocationResult {
    pub vreg_map: HashMap<VirtualRegister, PhysicalRegister>,
    pub spill_map: HashMap<VirtualRegister, i32>,
    pub total_spill_bytes: u32,
}

/// Linear Scan Register Allocator.
pub struct LinearScanAllocator<'a> {
    reg_file: &'a dyn RegisterFile,
}

impl<'a> LinearScanAllocator<'a> {
    pub fn new(reg_file: &'a dyn RegisterFile) -> Self {
        Self { reg_file }
    }

    /// Compute live intervals for each virtual register.
    fn compute_live_intervals(&self, func: &MachineFunction) -> Vec<LiveInterval> {
        let mut first_seen: HashMap<VirtualRegister, usize> = HashMap::new();
        let mut last_seen: HashMap<VirtualRegister, usize> = HashMap::new();

        let mut inst_idx = 0;
        for block in &func.blocks {
            for inst in &block.instructions {
                self.record_operands(inst, inst_idx, &mut first_seen, &mut last_seen);
                inst_idx += 1;
            }
        }

        let mut intervals = Vec::new();
        for (vreg, start) in first_seen {
            let end = *last_seen.get(&vreg).unwrap_or(&start);
            intervals.push(LiveInterval {
                vreg,
                start,
                end,
                assigned_reg: None,
                spill_slot: None,
            });
        }

        // Sort intervals by start point
        intervals.sort_by_key(|i| i.start);
        intervals
    }

    fn record_operands(
        &self,
        inst: &MachineInstruction,
        idx: usize,
        first: &mut HashMap<VirtualRegister, usize>,
        last: &mut HashMap<VirtualRegister, usize>,
    ) {
        let mut check_op = |op: &MachineOperand| match op {
            MachineOperand::Register(MachineRegister::Virtual(v)) => {
                first.entry(*v).or_insert(idx);
                last.insert(*v, idx);
            }
            MachineOperand::Memory { base, index, .. } => {
                if let MachineRegister::Virtual(v) = base {
                    first.entry(*v).or_insert(idx);
                    last.insert(*v, idx);
                }
                if let Some((MachineRegister::Virtual(v), _)) = index {
                    first.entry(*v).or_insert(idx);
                    last.insert(*v, idx);
                }
            }
            _ => {}
        };

        match inst {
            MachineInstruction::Move { dst, src }
            | MachineInstruction::Load { dst, src, .. }
            | MachineInstruction::Store { dst, src, .. }
            | MachineInstruction::Add { dst, src }
            | MachineInstruction::Sub { dst, src }
            | MachineInstruction::Mul { dst, src }
            | MachineInstruction::Div { dst, src }
            | MachineInstruction::Mod { dst, src }
            | MachineInstruction::And { dst, src }
            | MachineInstruction::Or { dst, src }
            | MachineInstruction::Xor { dst, src }
            | MachineInstruction::Shl { dst, src }
            | MachineInstruction::Shr { dst, src }
            | MachineInstruction::Sar { dst, src } => {
                check_op(src);
                check_op(dst);
            }
            MachineInstruction::Compare { lhs, rhs } | MachineInstruction::Test { lhs, rhs } => {
                check_op(lhs);
                check_op(rhs);
            }
            MachineInstruction::Neg { dst }
            | MachineInstruction::Not { dst }
            | MachineInstruction::SetCc { dst, .. }
            | MachineInstruction::Push { src: dst }
            | MachineInstruction::Pop { dst } => {
                check_op(dst);
            }
            MachineInstruction::Call { target, .. } => {
                check_op(target);
            }
            MachineInstruction::Custom { operands, .. } => {
                for op in operands {
                    check_op(op);
                }
            }
            _ => {}
        }
    }

    /// Allocate registers and spills.
    pub fn allocate(&self, func: &mut MachineFunction) -> AllocationResult {
        let intervals = self.compute_live_intervals(func);
        let allocatable = self.reg_file.allocatable();

        let mut vreg_map = HashMap::new();
        let mut spill_map = HashMap::new();
        let mut active: Vec<LiveInterval> = Vec::new();

        // Spill slots must live below the locals area (negative offsets from
        // the frame pointer), otherwise they silently collide with lowered
        // local variables.
        let locals_end = (func.stack_size as i32 + 7) & !7;
        let mut next_spill_offset = -locals_end - 8;

        // Prefer callee-saved registers: without live-range splitting around
        // calls, values that live across a call survive only in callee-saved
        // registers. Backends must still save/restore them in the prologue.
        let callee_saved: Vec<PhysicalRegister> = self
            .reg_file
            .callee_saved()
            .iter()
            .copied()
            .filter(|r| allocatable.contains(r))
            .collect();
        let callee_set: HashSet<PhysicalRegister> = callee_saved.iter().copied().collect();
        let mut call_indices = Vec::new();
        let mut inst_idx = 0;
        for block in &func.blocks {
            for inst in &block.instructions {
                if matches!(inst, MachineInstruction::Call { .. }) {
                    call_indices.push(inst_idx);
                }
                inst_idx += 1;
            }
        }

        for mut current in intervals {
            // Expire old intervals
            active.retain(|act| act.end >= current.start);

            let used_phys: HashSet<PhysicalRegister> =
                active.iter().filter_map(|act| act.assigned_reg).collect();

            let crosses_call = call_indices
                .iter()
                .any(|&c| current.start <= c && current.end >= c);

            let chosen_reg = if crosses_call {
                // If live interval spans a call, we can ONLY use an available callee-saved register
                callee_saved
                    .iter()
                    .copied()
                    .find(|r| !used_phys.contains(r))
            } else {
                // If it does NOT span a call, prefer caller-saved registers first, then callee-saved
                let caller_saved: Vec<PhysicalRegister> = allocatable
                    .iter()
                    .copied()
                    .filter(|r| !callee_set.contains(r))
                    .collect();
                caller_saved
                    .iter()
                    .copied()
                    .find(|r| !used_phys.contains(r))
                    .or_else(|| {
                        callee_saved
                            .iter()
                            .copied()
                            .find(|r| !used_phys.contains(r))
                    })
            };

            if let Some(free_reg) = chosen_reg {
                current.assigned_reg = Some(free_reg);
                vreg_map.insert(current.vreg, free_reg);
                active.push(current);
            } else {
                // Spill to stack slot
                current.spill_slot = Some(next_spill_offset);
                spill_map.insert(current.vreg, next_spill_offset);
                next_spill_offset -= 8;
                active.push(current);
            }
        }

        // Each spill slot is 8 bytes; the frame must cover the deepest one.
        let total_spill_bytes = (spill_map.len() as u32) * 8;
        func.stack_size += total_spill_bytes as u64;

        // Rewrite function operands with allocated physical registers or stack slots
        self.rewrite_operands(func, &vreg_map, &spill_map);

        AllocationResult {
            vreg_map,
            spill_map,
            total_spill_bytes,
        }
    }

    fn rewrite_operands(
        &self,
        func: &mut MachineFunction,
        vreg_map: &HashMap<VirtualRegister, PhysicalRegister>,
        spill_map: &HashMap<VirtualRegister, i32>,
    ) {
        let rewrite_op = |op: &mut MachineOperand| {
            if let MachineOperand::Register(MachineRegister::Virtual(v)) = op {
                if let Some(&p) = vreg_map.get(v) {
                    *op = MachineOperand::Register(MachineRegister::Physical(p));
                } else if let Some(&slot) = spill_map.get(v) {
                    *op = MachineOperand::StackSlot(slot);
                }
            } else if let MachineOperand::Memory { base, index, .. } = op {
                if let MachineRegister::Virtual(v) = base
                    && let Some(&p) = vreg_map.get(v)
                {
                    *base = MachineRegister::Physical(p);
                }
                if let Some((reg, _)) = index
                    && let MachineRegister::Virtual(v) = reg
                    && let Some(&p) = vreg_map.get(v)
                {
                    *reg = MachineRegister::Physical(p);
                }
            }
        };

        for block in &mut func.blocks {
            for inst in &mut block.instructions {
                match inst {
                    MachineInstruction::Move { dst, src }
                    | MachineInstruction::Load { dst, src, .. }
                    | MachineInstruction::Store { dst, src, .. }
                    | MachineInstruction::Add { dst, src }
                    | MachineInstruction::Sub { dst, src }
                    | MachineInstruction::Mul { dst, src }
                    | MachineInstruction::Div { dst, src }
                    | MachineInstruction::Mod { dst, src }
                    | MachineInstruction::And { dst, src }
                    | MachineInstruction::Or { dst, src }
                    | MachineInstruction::Xor { dst, src }
                    | MachineInstruction::Shl { dst, src }
                    | MachineInstruction::Shr { dst, src }
                    | MachineInstruction::Sar { dst, src } => {
                        rewrite_op(src);
                        rewrite_op(dst);
                    }
                    MachineInstruction::Compare { lhs, rhs }
                    | MachineInstruction::Test { lhs, rhs } => {
                        rewrite_op(lhs);
                        rewrite_op(rhs);
                    }
                    MachineInstruction::Neg { dst }
                    | MachineInstruction::Not { dst }
                    | MachineInstruction::SetCc { dst, .. }
                    | MachineInstruction::Push { src: dst }
                    | MachineInstruction::Pop { dst } => {
                        rewrite_op(dst);
                    }
                    MachineInstruction::Call { target, .. } => {
                        rewrite_op(target);
                    }
                    MachineInstruction::Custom { operands, .. } => {
                        for op in operands {
                            rewrite_op(op);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
