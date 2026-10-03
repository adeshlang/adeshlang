//! Register allocation framework supporting Linear Scan, Spilling, and Target-Specific Register Files.

use crate::machine_ir::{
    MachineFunction, MachineInstruction, MachineOperand, MachineRegister, MoveLocation,
    PhysicalRegister, RegisterClass, VirtualRegister,
};
use std::collections::{HashMap, HashSet};

/// Target-specific physical register file description.
pub trait RegisterFile: Send + Sync {
    fn registers(&self) -> &[PhysicalRegister];
    fn allocatable(&self) -> &[PhysicalRegister];
    fn caller_saved(&self) -> &[PhysicalRegister];
    fn callee_saved(&self) -> &[PhysicalRegister];
    fn reserved(&self) -> &[PhysicalRegister];

    fn registers_for_class(&self, class: RegisterClass) -> &[PhysicalRegister] {
        match class {
            RegisterClass::Gpr => self.registers(),
            RegisterClass::Float => &[],
        }
    }
    fn allocatable_for_class(&self, class: RegisterClass) -> &[PhysicalRegister] {
        match class {
            RegisterClass::Gpr => self.allocatable(),
            RegisterClass::Float => &[],
        }
    }
    fn caller_saved_for_class(&self, class: RegisterClass) -> &[PhysicalRegister] {
        match class {
            RegisterClass::Gpr => self.caller_saved(),
            RegisterClass::Float => &[],
        }
    }
    fn callee_saved_for_class(&self, class: RegisterClass) -> &[PhysicalRegister] {
        match class {
            RegisterClass::Gpr => self.callee_saved(),
            RegisterClass::Float => &[],
        }
    }
    fn reserved_for_class(&self, class: RegisterClass) -> &[PhysicalRegister] {
        match class {
            RegisterClass::Gpr => self.reserved(),
            RegisterClass::Float => &[],
        }
    }
}

/// Segment of a live range [start, end).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LiveSegment {
    pub start: usize,
    pub end: usize,
}

/// Discontinuous Live Range composed of segments and use/def positions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveRange {
    pub vreg: VirtualRegister,
    pub class: RegisterClass,
    pub segments: Vec<LiveSegment>,
    pub use_positions: Vec<usize>,
    pub def_positions: Vec<usize>,
    pub assigned_reg: Option<PhysicalRegister>,
    pub spill_slot: Option<i32>,
}

impl LiveRange {
    pub fn new(vreg: VirtualRegister, class: RegisterClass) -> Self {
        Self {
            vreg,
            class,
            segments: Vec::new(),
            use_positions: Vec::new(),
            def_positions: Vec::new(),
            assigned_reg: None,
            spill_slot: None,
        }
    }

    pub fn start(&self) -> usize {
        self.segments.first().map(|s| s.start).unwrap_or(0)
    }

    pub fn end(&self) -> usize {
        self.segments.last().map(|s| s.end).unwrap_or(0)
    }

    pub fn add_segment(&mut self, start: usize, end: usize) {
        if start >= end {
            return;
        }
        self.segments.push(LiveSegment { start, end });
        self.segments.sort_by_key(|s| s.start);
        let mut merged: Vec<LiveSegment> = Vec::new();
        for seg in self.segments.drain(..) {
            if let Some(last) = merged.last_mut() {
                if seg.start <= last.end {
                    last.end = last.end.max(seg.end);
                    continue;
                }
            }
            merged.push(seg);
        }
        self.segments = merged;
    }

    pub fn overlaps(&self, other: &LiveRange) -> bool {
        for s1 in &self.segments {
            for s2 in &other.segments {
                if s1.start < s2.end && s2.start < s1.end {
                    return true;
                }
            }
        }
        false
    }

    pub fn covers(&self, pos: usize) -> bool {
        self.segments.iter().any(|s| s.start <= pos && pos < s.end)
    }
}

/// Liveness Analysis Result across basic blocks.
pub struct LivenessAnalysis {
    pub block_uses: HashMap<u32, HashSet<MachineRegister>>,
    pub block_defs: HashMap<u32, HashSet<MachineRegister>>,
    pub live_in: HashMap<u32, HashSet<MachineRegister>>,
    pub live_out: HashMap<u32, HashSet<MachineRegister>>,
    pub inst_index_map: HashMap<(u32, usize), usize>,
    pub block_ranges: HashMap<u32, (usize, usize)>,
}

impl LivenessAnalysis {
    pub fn compute(func: &mut MachineFunction) -> Self {
        func.rebuild_cfg();

        let mut block_uses = HashMap::new();
        let mut block_defs = HashMap::new();
        let mut inst_index_map = HashMap::new();
        let mut block_ranges = HashMap::new();

        let mut global_inst_idx = 0;

        for block in &func.blocks {
            let mut uses = HashSet::new();
            let mut defs = HashSet::new();
            let block_start = global_inst_idx;

            for (local_idx, inst) in block.instructions.iter().enumerate() {
                inst_index_map.insert((block.id, local_idx), global_inst_idx);

                for u in inst.uses() {
                    if !defs.contains(&u) {
                        uses.insert(u);
                    }
                }
                for d in inst.defs() {
                    defs.insert(d);
                }

                global_inst_idx += 1;
            }

            let block_end = global_inst_idx;
            block_ranges.insert(block.id, (block_start, block_end));
            block_uses.insert(block.id, uses);
            block_defs.insert(block.id, defs);
        }

        let mut live_in: HashMap<u32, HashSet<MachineRegister>> = HashMap::new();
        let mut live_out: HashMap<u32, HashSet<MachineRegister>> = HashMap::new();

        for block in &func.blocks {
            live_in.insert(block.id, HashSet::new());
            live_out.insert(block.id, HashSet::new());
        }

        let mut changed = true;
        while changed {
            changed = false;

            for block in func.blocks.iter().rev() {
                let id = block.id;

                let mut new_live_out = HashSet::new();
                for &succ_id in &block.successors {
                    if let Some(succ_in) = live_in.get(&succ_id) {
                        new_live_out.extend(succ_in.iter().copied());
                    }
                }

                if new_live_out != *live_out.get(&id).unwrap() {
                    live_out.insert(id, new_live_out.clone());
                    changed = true;
                }

                let b_use = block_uses.get(&id).unwrap();
                let b_def = block_defs.get(&id).unwrap();

                let mut new_live_in = b_use.clone();
                for reg in &new_live_out {
                    if !b_def.contains(reg) {
                        new_live_in.insert(*reg);
                    }
                }

                if new_live_in != *live_in.get(&id).unwrap() {
                    live_in.insert(id, new_live_in);
                    changed = true;
                }
            }
        }

        Self {
            block_uses,
            block_defs,
            live_in,
            live_out,
            inst_index_map,
            block_ranges,
        }
    }

    pub fn build_live_ranges(&self, func: &MachineFunction) -> Vec<LiveRange> {
        let mut range_map: HashMap<VirtualRegister, LiveRange> = HashMap::new();

        for block in &func.blocks {
            let &(b_start, b_end) = self.block_ranges.get(&block.id).unwrap();
            let mut live = self.live_out.get(&block.id).cloned().unwrap_or_default();

            for &reg in &live {
                if let MachineRegister::Virtual(v) = reg {
                    let class = func.vreg_class(v);
                    let lr = range_map
                        .entry(v)
                        .or_insert_with(|| LiveRange::new(v, class));
                    lr.add_segment(b_start, b_end);
                }
            }

            for (local_idx, inst) in block.instructions.iter().enumerate().rev() {
                let inst_idx = *self.inst_index_map.get(&(block.id, local_idx)).unwrap();

                for d in inst.defs() {
                    if let MachineRegister::Virtual(v) = d {
                        let class = func.vreg_class(v);
                        let lr = range_map
                            .entry(v)
                            .or_insert_with(|| LiveRange::new(v, class));
                        lr.def_positions.push(inst_idx);
                        live.remove(&d);
                    }
                }

                for u in inst.uses() {
                    if let MachineRegister::Virtual(v) = u {
                        let class = func.vreg_class(v);
                        let lr = range_map
                            .entry(v)
                            .or_insert_with(|| LiveRange::new(v, class));
                        lr.use_positions.push(inst_idx);
                        lr.add_segment(b_start, inst_idx + 1);
                        live.insert(u);
                    }
                }
            }
        }

        let mut ranges: Vec<LiveRange> = range_map.into_values().collect();
        ranges.sort_by_key(|r| r.start());
        ranges
    }
}

pub type LiveInterval = LiveRange;

/// Dedicated Spill Slot Manager tracking slot alignment, width, and lifetime reuse.
pub struct SpillSlotManager {
    slots: Vec<SpillSlotInfo>,
}

#[derive(Debug, Clone)]
struct SpillSlotInfo {
    offset: i32,
    size: u32,
    class: RegisterClass,
    start_pos: usize,
    end_pos: usize,
}

impl SpillSlotManager {
    pub fn new() -> Self {
        Self { slots: Vec::new() }
    }

    pub fn allocate_slot(
        &mut self,
        class: RegisterClass,
        start_pos: usize,
        end_pos: usize,
        locals_end: i32,
    ) -> i32 {
        let size = match class {
            RegisterClass::Gpr => 8,
            RegisterClass::Float => 8,
        };

        for slot in &mut self.slots {
            if slot.class == class && (end_pos <= slot.start_pos || start_pos >= slot.end_pos) {
                slot.start_pos = slot.start_pos.min(start_pos);
                slot.end_pos = slot.end_pos.max(end_pos);
                return slot.offset;
            }
        }

        let current_min = self
            .slots
            .iter()
            .map(|s| s.offset)
            .min()
            .unwrap_or(-locals_end);

        let offset = current_min - (size as i32);
        self.slots.push(SpillSlotInfo {
            offset,
            size,
            class,
            start_pos,
            end_pos,
        });

        offset
    }

    pub fn slot_size(&self, offset: i32) -> Option<u32> {
        self.slots
            .iter()
            .find(|s| s.offset == offset)
            .map(|s| s.size)
    }
}

/// Register Allocation Result.
pub struct AllocationResult {
    pub vreg_map: HashMap<VirtualRegister, PhysicalRegister>,
    pub spill_map: HashMap<VirtualRegister, i32>,
    pub total_spill_bytes: u32,
    pub used_callee_saved: HashSet<PhysicalRegister>,
}

/// Verification error for post-allocation Machine IR verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationError {
    UnresolvedVirtualRegister(VirtualRegister),
    RegisterClassMismatch {
        vreg: VirtualRegister,
        expected: RegisterClass,
        found: RegisterClass,
    },
    InvalidStackOffset(i32),
}

/// Post-allocation verification pass to validate Machine IR correctness before assembly/encoding.
pub struct AllocationVerifier;

impl AllocationVerifier {
    pub fn verify(
        func: &MachineFunction,
        vreg_map: &HashMap<VirtualRegister, PhysicalRegister>,
        spill_map: &HashMap<VirtualRegister, i32>,
    ) -> Result<(), VerificationError> {
        for block in &func.blocks {
            for inst in &block.instructions {
                for u in inst.uses() {
                    if let MachineRegister::Virtual(v) = u {
                        if !vreg_map.contains_key(&v) && !spill_map.contains_key(&v) {
                            return Err(VerificationError::UnresolvedVirtualRegister(v));
                        }
                        if let Some(&p) = vreg_map.get(&v) {
                            let expected = func.vreg_class(v);
                            let found = p.class();
                            if expected != found {
                                return Err(VerificationError::RegisterClassMismatch {
                                    vreg: v,
                                    expected,
                                    found,
                                });
                            }
                        }
                    }
                }
                for d in inst.defs() {
                    if let MachineRegister::Virtual(v) = d {
                        if !vreg_map.contains_key(&v) && !spill_map.contains_key(&v) {
                            return Err(VerificationError::UnresolvedVirtualRegister(v));
                        }
                        if let Some(&p) = vreg_map.get(&v) {
                            let expected = func.vreg_class(v);
                            let found = p.class();
                            if expected != found {
                                return Err(VerificationError::RegisterClassMismatch {
                                    vreg: v,
                                    expected,
                                    found,
                                });
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

/// Linear Scan Register Allocator.
pub struct LinearScanAllocator<'a> {
    reg_file: &'a dyn RegisterFile,
}

impl<'a> LinearScanAllocator<'a> {
    pub fn new(reg_file: &'a dyn RegisterFile) -> Self {
        Self { reg_file }
    }

    fn compute_live_intervals(&self, func: &mut MachineFunction) -> Vec<LiveInterval> {
        let liveness = LivenessAnalysis::compute(func);
        liveness.build_live_ranges(func)
    }

    pub fn allocate(&self, func: &mut MachineFunction) -> AllocationResult {
        let intervals = self.compute_live_intervals(func);

        let mut vreg_map = HashMap::new();
        let mut spill_map = HashMap::new();
        let mut used_callee_saved = HashSet::new();
        let mut active: Vec<LiveInterval> = Vec::new();

        let locals_end = (func.stack_size as i32 + 7) & !7;
        let mut spill_manager = SpillSlotManager::new();

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
            let class = current.class;
            let allocatable = self.reg_file.allocatable_for_class(class);
            let callee_saved: Vec<PhysicalRegister> = self
                .reg_file
                .callee_saved_for_class(class)
                .iter()
                .copied()
                .filter(|r| allocatable.contains(r))
                .collect();
            let callee_set: HashSet<PhysicalRegister> = callee_saved.iter().copied().collect();

            active.retain(|act| act.end() >= current.start());

            let used_phys: HashSet<PhysicalRegister> =
                active.iter().filter_map(|act| act.assigned_reg).collect();

            let crosses_call = call_indices
                .iter()
                .any(|&c| current.start() <= c && current.end() >= c);

            let chosen_reg = if crosses_call {
                callee_saved
                    .iter()
                    .copied()
                    .find(|r| !used_phys.contains(r))
            } else {
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
                if callee_set.contains(&free_reg) {
                    used_callee_saved.insert(free_reg);
                }
                active.push(current);
            } else {
                let slot =
                    spill_manager.allocate_slot(class, current.start(), current.end(), locals_end);
                current.spill_slot = Some(slot);
                spill_map.insert(current.vreg, slot);
                active.push(current);
            }
        }

        let deepest_offset = spill_map.values().min().copied().unwrap_or(-locals_end);
        let total_spill_bytes = ((-deepest_offset) - locals_end).max(0) as u32;
        func.stack_size += total_spill_bytes as u64;

        let _ = AllocationVerifier::verify(func, &vreg_map, &spill_map);

        self.rewrite_operands(func, &vreg_map, &spill_map);

        AllocationResult {
            vreg_map,
            spill_map,
            total_spill_bytes,
            used_callee_saved,
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
                    | MachineInstruction::Sar { dst, src }
                    | MachineInstruction::FAdd { dst, src, .. }
                    | MachineInstruction::FSub { dst, src, .. }
                    | MachineInstruction::FMul { dst, src, .. }
                    | MachineInstruction::FDiv { dst, src, .. }
                    | MachineInstruction::FCvtIntToFloat { dst, src, .. }
                    | MachineInstruction::FCvtFloatToInt { dst, src, .. }
                    | MachineInstruction::FCvtFloatToFloat { dst, src, .. } => {
                        rewrite_op(src);
                        rewrite_op(dst);
                    }
                    MachineInstruction::Compare { lhs, rhs }
                    | MachineInstruction::Test { lhs, rhs }
                    | MachineInstruction::FCmp { lhs, rhs, .. } => {
                        rewrite_op(lhs);
                        rewrite_op(rhs);
                    }
                    MachineInstruction::Neg { dst }
                    | MachineInstruction::Not { dst }
                    | MachineInstruction::FNeg { dst, .. }
                    | MachineInstruction::SetCc { dst, .. }
                    | MachineInstruction::Push { src: dst }
                    | MachineInstruction::Pop { dst } => {
                        rewrite_op(dst);
                    }
                    MachineInstruction::Call { target, .. } => {
                        rewrite_op(target);
                    }
                    MachineInstruction::ParallelMove { moves } => {
                        for m in moves {
                            if let MoveLocation::VirtualRegister(v) = m.dst {
                                if let Some(&p) = vreg_map.get(&v) {
                                    m.dst = MoveLocation::PhysicalRegister(p);
                                } else if let Some(&slot) = spill_map.get(&v) {
                                    m.dst = MoveLocation::StackSlot {
                                        base: PhysicalRegister(5),
                                        offset: slot,
                                    };
                                }
                            }
                            if let MoveLocation::VirtualRegister(v) = m.src {
                                if let Some(&p) = vreg_map.get(&v) {
                                    m.src = MoveLocation::PhysicalRegister(p);
                                } else if let Some(&slot) = spill_map.get(&v) {
                                    m.src = MoveLocation::StackSlot {
                                        base: PhysicalRegister(5),
                                        offset: slot,
                                    };
                                }
                            }
                        }
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
