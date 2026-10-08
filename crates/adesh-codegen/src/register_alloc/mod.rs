//! Production Linear Scan Register Allocation, Exact Liveness, Spill/Reload Rewriting,
//! ABI Call-Clobber Safety, and Machine IR Verification.

use crate::machine_ir::{
    MachineFunction, MachineInstruction, MachineOperand, MachineRegister, MoveLocation,
    PhysicalRegister, RegisterClass, RegisterConstraint, VirtualRegister,
};
use std::collections::{HashMap, HashSet};

/// Target-specific physical register file and ABI information.
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

    fn call_clobbers_for_class(&self, class: RegisterClass) -> &[PhysicalRegister] {
        self.caller_saved_for_class(class)
    }

    fn scratch_for_class(&self, class: RegisterClass) -> (PhysicalRegister, PhysicalRegister) {
        match class {
            RegisterClass::Gpr => (PhysicalRegister(10), PhysicalRegister(11)),
            RegisterClass::Float => (PhysicalRegister::xmm(15), PhysicalRegister::xmm(14)),
        }
    }
}

/// Segment of a live range defined as a half-open interval `[start, end)`.
/// An instruction at position `start` is the earliest point where the value is defined/live,
/// and `end` is the first instruction index where the value is no longer live.
/// Two intervals `[a, b)` and `[b, c)` do not overlap and can share physical registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LiveSegment {
    pub start: usize,
    pub end: usize,
}

impl LiveSegment {
    pub fn new(start: usize, end: usize) -> Self {
        assert!(start <= end, "invalid live segment start > end");
        Self { start, end }
    }

    /// Checks if this half-open segment overlaps with another `[start, end)`.
    #[inline]
    pub fn overlaps(&self, other: &LiveSegment) -> bool {
        self.start < other.end && other.start < self.end
    }

    /// Checks if this segment covers the instruction position `pos`.
    #[inline]
    pub fn covers(&self, pos: usize) -> bool {
        self.start <= pos && pos < self.end
    }
}

/// Discontinuous Live Range composed of exact half-open segments `[start, end)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveRange {
    pub vreg: VirtualRegister,
    pub class: RegisterClass,
    pub segments: Vec<LiveSegment>,
    pub use_positions: Vec<usize>,
    pub def_positions: Vec<usize>,
    pub assigned_reg: Option<PhysicalRegister>,
    pub spill_slot: Option<i32>,
    pub constraint: RegisterConstraint,
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
            constraint: RegisterConstraint::Any,
        }
    }

    pub fn with_constraint(mut self, constraint: RegisterConstraint) -> Self {
        self.constraint = constraint;
        self
    }

    pub fn start(&self) -> usize {
        self.segments.first().map(|s| s.start).unwrap_or(0)
    }

    pub fn end(&self) -> usize {
        self.segments.last().map(|s| s.end).unwrap_or(0)
    }

    /// Adds a half-open segment `[start, end)`. Merges adjacent/overlapping segments.
    pub fn add_segment(&mut self, start: usize, end: usize) {
        if start >= end {
            return;
        }
        self.segments.push(LiveSegment::new(start, end));
        self.segments.sort_by_key(|s| s.start);
        let mut merged: Vec<LiveSegment> = Vec::new();
        for seg in self.segments.drain(..) {
            if let Some(last) = merged.last_mut()
                && seg.start <= last.end
            {
                last.end = last.end.max(seg.end);
                continue;
            }
            merged.push(seg);
        }
        self.segments = merged;
    }

    /// Tests if this LiveRange overlaps with another LiveRange under half-open interval semantics.
    pub fn overlaps(&self, other: &LiveRange) -> bool {
        for s1 in &self.segments {
            for s2 in &other.segments {
                if s1.overlaps(s2) {
                    return true;
                }
            }
        }
        false
    }

    /// Tests if this LiveRange covers instruction position `pos`.
    pub fn covers(&self, pos: usize) -> bool {
        self.segments.iter().any(|s| s.covers(pos))
    }
}

pub type LiveInterval = LiveRange;

/// Exact instruction-level and block-level liveness analysis.
pub struct LivenessAnalysis {
    pub block_uses: HashMap<u32, HashSet<MachineRegister>>,
    pub block_defs: HashMap<u32, HashSet<MachineRegister>>,
    pub live_in: HashMap<u32, HashSet<MachineRegister>>,
    pub live_out: HashMap<u32, HashSet<MachineRegister>>,
    pub inst_index_map: HashMap<(u32, usize), usize>,
    pub block_ranges: HashMap<u32, (usize, usize)>,
    pub live_before: Vec<HashSet<MachineRegister>>,
    pub live_after: Vec<HashSet<MachineRegister>>,
    pub total_instructions: usize,
}

impl LivenessAnalysis {
    /// Computes CFG, block-level fixed-point liveness, and exact instruction-level
    /// `live_before(I)` / `live_after(I)` for every machine instruction.
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

        let total_instructions = global_inst_idx;

        // Block-level fixed point iteration
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

        // Exact instruction-level transfer function computation:
        // live_after(I) = live_before(I+1) (or live_out for block terminator)
        // live_before(I) = uses(I) ∪ (live_after(I) - defs(I))
        let mut live_before = vec![HashSet::new(); total_instructions];
        let mut live_after = vec![HashSet::new(); total_instructions];

        for block in &func.blocks {
            let &(b_start, b_end) = block_ranges.get(&block.id).unwrap();
            if b_start == b_end {
                continue;
            }

            let mut current_live = live_out.get(&block.id).cloned().unwrap_or_default();

            for local_idx in (0..block.instructions.len()).rev() {
                let inst_idx = *inst_index_map.get(&(block.id, local_idx)).unwrap();
                let inst = &block.instructions[local_idx];

                live_after[inst_idx] = current_live.clone();

                for d in inst.defs() {
                    current_live.remove(&d);
                }
                for u in inst.uses() {
                    current_live.insert(u);
                }

                live_before[inst_idx] = current_live.clone();
            }
        }

        Self {
            block_uses,
            block_defs,
            live_in,
            live_out,
            inst_index_map,
            block_ranges,
            live_before,
            live_after,
            total_instructions,
        }
    }

    /// Builds precise discontinuous `LiveRange`s for all virtual registers.
    pub fn build_live_ranges(&self, func: &MachineFunction) -> Vec<LiveRange> {
        let mut range_map: HashMap<VirtualRegister, LiveRange> = HashMap::new();

        for block in &func.blocks {
            let &(b_start, b_end) = self.block_ranges.get(&block.id).unwrap();
            let block_live_out = self.live_out.get(&block.id).cloned().unwrap_or_default();

            // Track the active live interval end point for each virtual register in this block
            let mut active_end: HashMap<VirtualRegister, usize> = HashMap::new();

            for &reg in &block_live_out {
                if let MachineRegister::Virtual(v) = reg {
                    active_end.insert(v, b_end);
                }
            }

            for local_idx in (0..block.instructions.len()).rev() {
                let inst_idx = *self.inst_index_map.get(&(block.id, local_idx)).unwrap();
                let inst = &block.instructions[local_idx];

                for d in inst.defs() {
                    if let MachineRegister::Virtual(v) = d {
                        let class = func.vreg_class(v);
                        let lr = range_map
                            .entry(v)
                            .or_insert_with(|| LiveRange::new(v, class));
                        lr.def_positions.push(inst_idx);

                        if let Some(end_pos) = active_end.remove(&v) {
                            lr.add_segment(inst_idx, end_pos);
                        } else {
                            // Dead definition: live during instruction inst_idx only
                            lr.add_segment(inst_idx, inst_idx + 1);
                        }
                    }
                }

                for u in inst.uses() {
                    if let MachineRegister::Virtual(v) = u {
                        let class = func.vreg_class(v);
                        let lr = range_map
                            .entry(v)
                            .or_insert_with(|| LiveRange::new(v, class));
                        lr.use_positions.push(inst_idx);

                        active_end.entry(v).or_insert_with(|| inst_idx + 1);
                    }
                }
            }

            // Any variable still live up to the block entry starts at b_start
            for (v, end_pos) in active_end {
                let class = func.vreg_class(v);
                let lr = range_map
                    .entry(v)
                    .or_insert_with(|| LiveRange::new(v, class));
                lr.add_segment(b_start, end_pos);
            }
        }

        let mut ranges: Vec<LiveRange> = range_map.into_values().collect();
        ranges.sort_by_key(|r| r.start());
        ranges
    }

    pub fn live_before_inst(&self, inst_idx: usize) -> Option<&HashSet<MachineRegister>> {
        self.live_before.get(inst_idx)
    }

    pub fn live_after_inst(&self, inst_idx: usize) -> Option<&HashSet<MachineRegister>> {
        self.live_after.get(inst_idx)
    }
}

/// Dedicated Spill Slot Manager tracking slot alignment, width, and non-overlapping lifetime reuse.
pub struct SpillSlotManager {
    slots: Vec<SpillSlotInfo>,
}

#[derive(Debug, Clone)]
pub struct SpillSlotInfo {
    pub offset: i32,
    pub size: u32,
    pub alignment: u32,
    pub class: RegisterClass,
    pub segments: Vec<LiveSegment>,
}

impl Default for SpillSlotManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SpillSlotManager {
    pub fn new() -> Self {
        Self { slots: Vec::new() }
    }

    /// Allocates or reuses a stack spill slot for a value with given class, size, alignment,
    /// and live segments. Reuses slots when all existing segments on that slot do not overlap.
    pub fn allocate_slot(
        &mut self,
        class: RegisterClass,
        size: u32,
        alignment: u32,
        live_segments: &[LiveSegment],
        locals_end: i32,
    ) -> i32 {
        for slot in &mut self.slots {
            if slot.class == class && slot.size >= size && slot.alignment >= alignment {
                let overlaps = slot
                    .segments
                    .iter()
                    .any(|s1| live_segments.iter().any(|s2| s1.overlaps(s2)));
                if !overlaps {
                    slot.segments.extend_from_slice(live_segments);
                    return slot.offset;
                }
            }
        }

        let current_min = self
            .slots
            .iter()
            .map(|s| s.offset)
            .min()
            .unwrap_or(-locals_end);

        let align = (alignment.max(8)) as i32;
        let mut offset = current_min - (size.max(8) as i32);
        while (-offset) % align != 0 {
            offset -= 1;
        }

        self.slots.push(SpillSlotInfo {
            offset,
            size: size.max(8),
            alignment: align as u32,
            class,
            segments: live_segments.to_vec(),
        });

        offset
    }

    pub fn slot_size(&self, offset: i32) -> Option<u32> {
        self.slots
            .iter()
            .find(|s| s.offset == offset)
            .map(|s| s.size)
    }

    pub fn slots(&self) -> &[SpillSlotInfo] {
        &self.slots
    }
}

/// Register Allocation Result.
pub struct AllocationResult {
    pub vreg_map: HashMap<VirtualRegister, PhysicalRegister>,
    pub spill_map: HashMap<VirtualRegister, i32>,
    pub spill_size_map: HashMap<VirtualRegister, u8>,
    pub total_spill_bytes: u32,
    pub used_callee_saved: HashSet<PhysicalRegister>,
}

/// Detailed diagnostics for post-allocation Machine IR verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationError {
    UnresolvedVirtualRegister {
        vreg: VirtualRegister,
        inst_index: usize,
    },
    RegisterClassMismatch {
        vreg: VirtualRegister,
        expected: RegisterClass,
        found: RegisterClass,
    },
    ReservedRegisterAllocated(PhysicalRegister),
    LiveRangeInterference {
        reg: PhysicalRegister,
        v1: VirtualRegister,
        v2: VirtualRegister,
    },
    CallerSavedLiveAcrossCall {
        reg: PhysicalRegister,
        vreg: VirtualRegister,
        call_idx: usize,
    },
    InvalidSpillOffset(i32),
    InvalidSpillAlignment {
        offset: i32,
        alignment: u32,
    },
    CalleeSavedUnpreserved(PhysicalRegister),
}

impl std::fmt::Display for VerificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerificationError::UnresolvedVirtualRegister { vreg, inst_index } => {
                write!(
                    f,
                    "AllocationVerifier: Unresolved virtual register v{} at instruction {}",
                    vreg.0, inst_index
                )
            }
            VerificationError::RegisterClassMismatch {
                vreg,
                expected,
                found,
            } => {
                write!(
                    f,
                    "AllocationVerifier: Register class mismatch for v{}: expected {:?}, found {:?}",
                    vreg.0, expected, found
                )
            }
            VerificationError::ReservedRegisterAllocated(reg) => {
                write!(
                    f,
                    "AllocationVerifier: Reserved physical register {} was illegally allocated",
                    reg.0
                )
            }
            VerificationError::LiveRangeInterference { reg, v1, v2 } => {
                write!(
                    f,
                    "AllocationVerifier: Overlapping live ranges for v{} and v{} assigned to same register {}",
                    v1.0, v2.0, reg.0
                )
            }
            VerificationError::CallerSavedLiveAcrossCall {
                reg,
                vreg,
                call_idx,
            } => {
                write!(
                    f,
                    "AllocationVerifier: Caller-saved register {} assigned to v{} is live across call at instruction {}",
                    reg.0, vreg.0, call_idx
                )
            }
            VerificationError::InvalidSpillOffset(offset) => {
                write!(f, "AllocationVerifier: Invalid spill offset {}", offset)
            }
            VerificationError::InvalidSpillAlignment { offset, alignment } => {
                write!(
                    f,
                    "AllocationVerifier: Spill offset {} violates required alignment {}",
                    offset, alignment
                )
            }
            VerificationError::CalleeSavedUnpreserved(reg) => {
                write!(
                    f,
                    "AllocationVerifier: Callee-saved register {} used but not recorded for preservation",
                    reg.0
                )
            }
        }
    }
}

/// Comprehensive Post-Allocation Machine IR Verifier.
pub struct AllocationVerifier;

impl AllocationVerifier {
    pub fn verify(
        func: &MachineFunction,
        vreg_map: &HashMap<VirtualRegister, PhysicalRegister>,
        spill_map: &HashMap<VirtualRegister, i32>,
        used_callee_saved: &HashSet<PhysicalRegister>,
        intervals: &[LiveInterval],
        call_indices: &[usize],
        preserved_calls: &HashSet<(VirtualRegister, usize)>,
        reg_file: &dyn RegisterFile,
    ) -> Result<(), VerificationError> {
        let reserved_gpr: HashSet<PhysicalRegister> = reg_file
            .reserved_for_class(RegisterClass::Gpr)
            .iter()
            .copied()
            .collect();
        let reserved_fp: HashSet<PhysicalRegister> = reg_file
            .reserved_for_class(RegisterClass::Float)
            .iter()
            .copied()
            .collect();

        // 1. Verify that reserved registers are never allocated and all vregs are accounted for
        for (&v, &p) in vreg_map {
            if reserved_gpr.contains(&p) || reserved_fp.contains(&p) {
                return Err(VerificationError::ReservedRegisterAllocated(p));
            }
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

        for r in intervals {
            if !vreg_map.contains_key(&r.vreg) && !spill_map.contains_key(&r.vreg) {
                return Err(VerificationError::UnresolvedVirtualRegister {
                    vreg: r.vreg,
                    inst_index: r.start(),
                });
            }
        }

        // 2. Verify interference: overlapping intervals must not share physical registers
        for i in 0..intervals.len() {
            for j in (i + 1)..intervals.len() {
                let r1 = &intervals[i];
                let r2 = &intervals[j];
                if let (Some(p1), Some(p2)) = (r1.assigned_reg, r2.assigned_reg)
                    && p1 == p2
                    && r1.overlaps(r2)
                {
                    return Err(VerificationError::LiveRangeInterference {
                        reg: p1,
                        v1: r1.vreg,
                        v2: r2.vreg,
                    });
                }
            }
        }

        // 3. Verify call safety: caller-saved registers cannot cross calls
        for r in intervals {
            if let Some(p) = r.assigned_reg {
                let is_caller_saved = reg_file.caller_saved_for_class(r.class).contains(&p);
                if is_caller_saved {
                    for &c in call_indices {
                        if r.covers(c) && !preserved_calls.contains(&(r.vreg, c)) {
                            return Err(VerificationError::CallerSavedLiveAcrossCall {
                                reg: p,
                                vreg: r.vreg,
                                call_idx: c,
                            });
                        }
                    }
                }
                let is_callee_saved = reg_file.callee_saved_for_class(r.class).contains(&p);
                if is_callee_saved && !used_callee_saved.contains(&p) {
                    return Err(VerificationError::CalleeSavedUnpreserved(p));
                }
            }
        }

        // 4. Verify that rewritten instructions contain no virtual registers
        let mut inst_idx = 0;
        for block in &func.blocks {
            for inst in &block.instructions {
                for u in inst.uses() {
                    if let MachineRegister::Virtual(v) = u {
                        return Err(VerificationError::UnresolvedVirtualRegister {
                            vreg: v,
                            inst_index: inst_idx,
                        });
                    }
                }
                for d in inst.defs() {
                    if let MachineRegister::Virtual(v) = d {
                        return Err(VerificationError::UnresolvedVirtualRegister {
                            vreg: v,
                            inst_index: inst_idx,
                        });
                    }
                }
                inst_idx += 1;
            }
        }

        Ok(())
    }
}

/// Systematic Spill and Reload Rewriter for Machine IR.
/// Rewrites spilled virtual registers into real physical scratch-register loads and stores.
pub struct SpillRewriter<'a> {
    reg_file: &'a dyn RegisterFile,
    vreg_map: &'a HashMap<VirtualRegister, PhysicalRegister>,
    spill_map: &'a HashMap<VirtualRegister, i32>,
    spill_size_map: &'a HashMap<VirtualRegister, u8>,
}

impl<'a> SpillRewriter<'a> {
    pub fn new(
        reg_file: &'a dyn RegisterFile,
        vreg_map: &'a HashMap<VirtualRegister, PhysicalRegister>,
        spill_map: &'a HashMap<VirtualRegister, i32>,
        spill_size_map: &'a HashMap<VirtualRegister, u8>,
    ) -> Self {
        Self {
            reg_file,
            vreg_map,
            spill_map,
            spill_size_map,
        }
    }

    fn phys_op(reg: PhysicalRegister) -> MachineOperand {
        MachineOperand::Register(MachineRegister::Physical(reg))
    }

    fn stack_op(offset: i32) -> MachineOperand {
        MachineOperand::StackSlot(offset)
    }

    pub fn rewrite_function(&self, func: &mut MachineFunction) {
        let (gpr_s1, gpr_s2) = self.reg_file.scratch_for_class(RegisterClass::Gpr);
        let (fp_s1, fp_s2) = self.reg_file.scratch_for_class(RegisterClass::Float);

        for block in &mut func.blocks {
            let mut rewritten: Vec<MachineInstruction> =
                Vec::with_capacity(block.instructions.len() * 2);

            for inst in block.instructions.drain(..) {
                self.rewrite_instruction(inst, &mut rewritten, gpr_s1, gpr_s2, fp_s1, fp_s2);
            }

            block.instructions = rewritten;
        }
    }

    fn rewrite_instruction(
        &self,
        inst: MachineInstruction,
        out: &mut Vec<MachineInstruction>,
        gpr_s1: PhysicalRegister,
        gpr_s2: PhysicalRegister,
        fp_s1: PhysicalRegister,
        fp_s2: PhysicalRegister,
    ) {
        match inst {
            MachineInstruction::Move { mut dst, mut src } => {
                let src_spill = self.get_spill(&src);
                let dst_spill = self.get_spill(&dst);

                match (dst_spill, src_spill) {
                    (Some((dst_slot, dst_size, dst_class)), Some((src_slot, src_size, _))) => {
                        let scratch = if dst_class == RegisterClass::Float {
                            fp_s1
                        } else {
                            gpr_s1
                        };
                        out.push(MachineInstruction::Load {
                            dst: Self::phys_op(scratch),
                            src: Self::stack_op(src_slot),
                            size: src_size,
                        });
                        out.push(MachineInstruction::Store {
                            dst: Self::stack_op(dst_slot),
                            src: Self::phys_op(scratch),
                            size: dst_size,
                        });
                    }
                    (Some((dst_slot, dst_size, dst_class)), None) => {
                        self.rewrite_operand(&mut src, gpr_s2, fp_s2, out);
                        if let MachineOperand::Register(MachineRegister::Physical(p)) = src {
                            out.push(MachineInstruction::Store {
                                dst: Self::stack_op(dst_slot),
                                src: Self::phys_op(p),
                                size: dst_size,
                            });
                        } else {
                            let scratch = if dst_class == RegisterClass::Float {
                                fp_s1
                            } else {
                                gpr_s1
                            };
                            out.push(MachineInstruction::Move {
                                dst: Self::phys_op(scratch),
                                src,
                            });
                            out.push(MachineInstruction::Store {
                                dst: Self::stack_op(dst_slot),
                                src: Self::phys_op(scratch),
                                size: dst_size,
                            });
                        }
                    }
                    (None, Some((src_slot, src_size, _))) => {
                        self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
                        if let MachineOperand::Register(MachineRegister::Physical(p)) = dst {
                            out.push(MachineInstruction::Load {
                                dst: Self::phys_op(p),
                                src: Self::stack_op(src_slot),
                                size: src_size,
                            });
                        } else {
                            out.push(MachineInstruction::Load {
                                dst: Self::phys_op(gpr_s1),
                                src: Self::stack_op(src_slot),
                                size: src_size,
                            });
                            out.push(MachineInstruction::Store {
                                dst,
                                src: Self::phys_op(gpr_s1),
                                size: src_size,
                            });
                        }
                    }
                    (None, None) => {
                        self.rewrite_operand(&mut src, gpr_s2, fp_s2, out);
                        self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
                        out.push(MachineInstruction::Move { dst, src });
                    }
                }
            }

            MachineInstruction::Add { dst, src } => {
                self.rewrite_gpr_binop(dst, src, out, gpr_s1, gpr_s2, fp_s2, |d, s| {
                    MachineInstruction::Add { dst: d, src: s }
                });
            }
            MachineInstruction::Sub { dst, src } => {
                self.rewrite_gpr_binop(dst, src, out, gpr_s1, gpr_s2, fp_s2, |d, s| {
                    MachineInstruction::Sub { dst: d, src: s }
                });
            }
            MachineInstruction::Mul { dst, src } => {
                self.rewrite_gpr_binop(dst, src, out, gpr_s1, gpr_s2, fp_s2, |d, s| {
                    MachineInstruction::Mul { dst: d, src: s }
                });
            }
            MachineInstruction::Div { dst, src } => {
                self.rewrite_gpr_binop(dst, src, out, gpr_s1, gpr_s2, fp_s2, |d, s| {
                    MachineInstruction::Div { dst: d, src: s }
                });
            }
            MachineInstruction::Mod { dst, src } => {
                self.rewrite_gpr_binop(dst, src, out, gpr_s1, gpr_s2, fp_s2, |d, s| {
                    MachineInstruction::Mod { dst: d, src: s }
                });
            }
            MachineInstruction::And { dst, src } => {
                self.rewrite_gpr_binop(dst, src, out, gpr_s1, gpr_s2, fp_s2, |d, s| {
                    MachineInstruction::And { dst: d, src: s }
                });
            }
            MachineInstruction::Or { dst, src } => {
                self.rewrite_gpr_binop(dst, src, out, gpr_s1, gpr_s2, fp_s2, |d, s| {
                    MachineInstruction::Or { dst: d, src: s }
                });
            }
            MachineInstruction::Xor { dst, src } => {
                self.rewrite_gpr_binop(dst, src, out, gpr_s1, gpr_s2, fp_s2, |d, s| {
                    MachineInstruction::Xor { dst: d, src: s }
                });
            }
            MachineInstruction::Shl { dst, src } => {
                self.rewrite_gpr_binop(dst, src, out, gpr_s1, gpr_s2, fp_s2, |d, s| {
                    MachineInstruction::Shl { dst: d, src: s }
                });
            }
            MachineInstruction::Shr { dst, src } => {
                self.rewrite_gpr_binop(dst, src, out, gpr_s1, gpr_s2, fp_s2, |d, s| {
                    MachineInstruction::Shr { dst: d, src: s }
                });
            }
            MachineInstruction::Sar { dst, src } => {
                self.rewrite_gpr_binop(dst, src, out, gpr_s1, gpr_s2, fp_s2, |d, s| {
                    MachineInstruction::Sar { dst: d, src: s }
                });
            }

            MachineInstruction::FAdd { dst, src, size } => {
                self.rewrite_fp_binop(dst, src, size, out, fp_s1, fp_s2, gpr_s2, |d, s, sz| {
                    MachineInstruction::FAdd {
                        dst: d,
                        src: s,
                        size: sz,
                    }
                });
            }
            MachineInstruction::FSub { dst, src, size } => {
                self.rewrite_fp_binop(dst, src, size, out, fp_s1, fp_s2, gpr_s2, |d, s, sz| {
                    MachineInstruction::FSub {
                        dst: d,
                        src: s,
                        size: sz,
                    }
                });
            }
            MachineInstruction::FMul { dst, src, size } => {
                self.rewrite_fp_binop(dst, src, size, out, fp_s1, fp_s2, gpr_s2, |d, s, sz| {
                    MachineInstruction::FMul {
                        dst: d,
                        src: s,
                        size: sz,
                    }
                });
            }
            MachineInstruction::FDiv { dst, src, size } => {
                self.rewrite_fp_binop(dst, src, size, out, fp_s1, fp_s2, gpr_s2, |d, s, sz| {
                    MachineInstruction::FDiv {
                        dst: d,
                        src: s,
                        size: sz,
                    }
                });
            }

            MachineInstruction::Compare { lhs, rhs } => {
                self.rewrite_cmp(lhs, rhs, out, gpr_s1, gpr_s2, fp_s2, |l, r| {
                    MachineInstruction::Compare { lhs: l, rhs: r }
                });
            }
            MachineInstruction::Test { lhs, rhs } => {
                self.rewrite_cmp(lhs, rhs, out, gpr_s1, gpr_s2, fp_s2, |l, r| {
                    MachineInstruction::Test { lhs: l, rhs: r }
                });
            }

            MachineInstruction::FCmp {
                mut lhs,
                mut rhs,
                size,
            } => {
                let lhs_spill = self.get_spill(&lhs);
                let rhs_spill = self.get_spill(&rhs);

                match (lhs_spill, rhs_spill) {
                    (Some((l_slot, l_sz, _)), Some((r_slot, r_sz, _))) => {
                        out.push(MachineInstruction::Load {
                            dst: Self::phys_op(fp_s1),
                            src: Self::stack_op(l_slot),
                            size: l_sz,
                        });
                        out.push(MachineInstruction::Load {
                            dst: Self::phys_op(fp_s2),
                            src: Self::stack_op(r_slot),
                            size: r_sz,
                        });
                        out.push(MachineInstruction::FCmp {
                            lhs: Self::phys_op(fp_s1),
                            rhs: Self::phys_op(fp_s2),
                            size,
                        });
                    }
                    (Some((l_slot, l_sz, _)), None) => {
                        out.push(MachineInstruction::Load {
                            dst: Self::phys_op(fp_s1),
                            src: Self::stack_op(l_slot),
                            size: l_sz,
                        });
                        self.rewrite_operand(&mut rhs, gpr_s2, fp_s2, out);
                        out.push(MachineInstruction::FCmp {
                            lhs: Self::phys_op(fp_s1),
                            rhs,
                            size,
                        });
                    }
                    (None, Some((r_slot, r_sz, _))) => {
                        self.rewrite_operand(&mut lhs, gpr_s1, fp_s1, out);
                        out.push(MachineInstruction::Load {
                            dst: Self::phys_op(fp_s2),
                            src: Self::stack_op(r_slot),
                            size: r_sz,
                        });
                        out.push(MachineInstruction::FCmp {
                            lhs,
                            rhs: Self::phys_op(fp_s2),
                            size,
                        });
                    }
                    (None, None) => {
                        self.rewrite_operand(&mut lhs, gpr_s1, fp_s1, out);
                        self.rewrite_operand(&mut rhs, gpr_s2, fp_s2, out);
                        out.push(MachineInstruction::FCmp { lhs, rhs, size });
                    }
                }
            }

            MachineInstruction::Neg { dst } => {
                self.rewrite_unop(dst, out, gpr_s1, fp_s1, |d| MachineInstruction::Neg {
                    dst: d,
                });
            }
            MachineInstruction::Not { dst } => {
                self.rewrite_unop(dst, out, gpr_s1, fp_s1, |d| MachineInstruction::Not {
                    dst: d,
                });
            }

            MachineInstruction::FNeg { mut dst, size } => {
                if let Some((slot, sz, _)) = self.get_spill(&dst) {
                    out.push(MachineInstruction::Load {
                        dst: Self::phys_op(fp_s1),
                        src: Self::stack_op(slot),
                        size: sz,
                    });
                    out.push(MachineInstruction::FNeg {
                        dst: Self::phys_op(fp_s1),
                        size,
                    });
                    out.push(MachineInstruction::Store {
                        dst: Self::stack_op(slot),
                        src: Self::phys_op(fp_s1),
                        size: sz,
                    });
                } else {
                    self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
                    out.push(MachineInstruction::FNeg { dst, size });
                }
            }

            MachineInstruction::SetCc { mut dst, cc } => {
                if let Some((slot, sz, _)) = self.get_spill(&dst) {
                    out.push(MachineInstruction::SetCc {
                        dst: Self::phys_op(gpr_s1),
                        cc,
                    });
                    out.push(MachineInstruction::Store {
                        dst: Self::stack_op(slot),
                        src: Self::phys_op(gpr_s1),
                        size: sz,
                    });
                } else {
                    self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
                    out.push(MachineInstruction::SetCc { dst, cc });
                }
            }

            MachineInstruction::Load {
                mut dst,
                mut src,
                size,
            } => {
                self.rewrite_operand(&mut src, gpr_s2, fp_s2, out);
                if let Some((dst_slot, dst_size, _)) = self.get_spill(&dst) {
                    out.push(MachineInstruction::Load {
                        dst: Self::phys_op(gpr_s1),
                        src,
                        size,
                    });
                    out.push(MachineInstruction::Store {
                        dst: Self::stack_op(dst_slot),
                        src: Self::phys_op(gpr_s1),
                        size: dst_size,
                    });
                } else {
                    self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
                    out.push(MachineInstruction::Load { dst, src, size });
                }
            }

            MachineInstruction::Store {
                mut dst,
                mut src,
                size,
            } => {
                self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
                if let Some((src_slot, src_size, _)) = self.get_spill(&src) {
                    out.push(MachineInstruction::Load {
                        dst: Self::phys_op(gpr_s2),
                        src: Self::stack_op(src_slot),
                        size: src_size,
                    });
                    out.push(MachineInstruction::Store {
                        dst,
                        src: Self::phys_op(gpr_s2),
                        size,
                    });
                } else {
                    self.rewrite_operand(&mut src, gpr_s2, fp_s2, out);
                    out.push(MachineInstruction::Store { dst, src, size });
                }
            }

            MachineInstruction::FCvtIntToFloat {
                mut dst,
                mut src,
                is_f64,
                is_signed,
            } => {
                let src_op = if let Some((slot, sz, _)) = self.get_spill(&src) {
                    out.push(MachineInstruction::Load {
                        dst: Self::phys_op(gpr_s1),
                        src: Self::stack_op(slot),
                        size: sz,
                    });
                    Self::phys_op(gpr_s1)
                } else {
                    self.rewrite_operand(&mut src, gpr_s1, fp_s1, out);
                    src
                };

                if let Some((slot, sz, _)) = self.get_spill(&dst) {
                    out.push(MachineInstruction::FCvtIntToFloat {
                        dst: Self::phys_op(fp_s1),
                        src: src_op,
                        is_f64,
                        is_signed,
                    });
                    out.push(MachineInstruction::Store {
                        dst: Self::stack_op(slot),
                        src: Self::phys_op(fp_s1),
                        size: sz,
                    });
                } else {
                    self.rewrite_operand(&mut dst, gpr_s2, fp_s2, out);
                    out.push(MachineInstruction::FCvtIntToFloat {
                        dst,
                        src: src_op,
                        is_f64,
                        is_signed,
                    });
                }
            }

            MachineInstruction::FCvtFloatToInt {
                mut dst,
                mut src,
                is_f64,
                is_signed,
            } => {
                let src_op = if let Some((slot, sz, _)) = self.get_spill(&src) {
                    out.push(MachineInstruction::Load {
                        dst: Self::phys_op(fp_s1),
                        src: Self::stack_op(slot),
                        size: sz,
                    });
                    Self::phys_op(fp_s1)
                } else {
                    self.rewrite_operand(&mut src, gpr_s1, fp_s1, out);
                    src
                };

                if let Some((slot, sz, _)) = self.get_spill(&dst) {
                    out.push(MachineInstruction::FCvtFloatToInt {
                        dst: Self::phys_op(gpr_s1),
                        src: src_op,
                        is_f64,
                        is_signed,
                    });
                    out.push(MachineInstruction::Store {
                        dst: Self::stack_op(slot),
                        src: Self::phys_op(gpr_s1),
                        size: sz,
                    });
                } else {
                    self.rewrite_operand(&mut dst, gpr_s2, fp_s2, out);
                    out.push(MachineInstruction::FCvtFloatToInt {
                        dst,
                        src: src_op,
                        is_f64,
                        is_signed,
                    });
                }
            }

            MachineInstruction::FCvtFloatToFloat {
                mut dst,
                mut src,
                to_f64,
            } => {
                let src_op = if let Some((slot, sz, _)) = self.get_spill(&src) {
                    out.push(MachineInstruction::Load {
                        dst: Self::phys_op(fp_s1),
                        src: Self::stack_op(slot),
                        size: sz,
                    });
                    Self::phys_op(fp_s1)
                } else {
                    self.rewrite_operand(&mut src, gpr_s1, fp_s1, out);
                    src
                };

                if let Some((slot, sz, _)) = self.get_spill(&dst) {
                    out.push(MachineInstruction::FCvtFloatToFloat {
                        dst: Self::phys_op(fp_s1),
                        src: src_op,
                        to_f64,
                    });
                    out.push(MachineInstruction::Store {
                        dst: Self::stack_op(slot),
                        src: Self::phys_op(fp_s1),
                        size: sz,
                    });
                } else {
                    self.rewrite_operand(&mut dst, gpr_s2, fp_s2, out);
                    out.push(MachineInstruction::FCvtFloatToFloat {
                        dst,
                        src: src_op,
                        to_f64,
                    });
                }
            }

            MachineInstruction::Call {
                mut target,
                num_args,
            } => {
                if let Some((slot, sz, _)) = self.get_spill(&target) {
                    out.push(MachineInstruction::Load {
                        dst: Self::phys_op(gpr_s2),
                        src: Self::stack_op(slot),
                        size: sz,
                    });
                    out.push(MachineInstruction::Call {
                        target: Self::phys_op(gpr_s2),
                        num_args,
                    });
                } else {
                    self.rewrite_operand(&mut target, gpr_s2, fp_s2, out);
                    out.push(MachineInstruction::Call { target, num_args });
                }
            }

            MachineInstruction::ParallelMove { mut moves } => {
                for m in &mut moves {
                    if let MoveLocation::VirtualRegister(v) = m.dst {
                        if let Some(&p) = self.vreg_map.get(&v) {
                            m.dst = MoveLocation::PhysicalRegister(p);
                        } else if let Some(&slot) = self.spill_map.get(&v) {
                            m.dst = MoveLocation::StackSlot {
                                base: PhysicalRegister(5),
                                offset: slot,
                            };
                        }
                    }
                    if let MoveLocation::VirtualRegister(v) = m.src {
                        if let Some(&p) = self.vreg_map.get(&v) {
                            m.src = MoveLocation::PhysicalRegister(p);
                        } else if let Some(&slot) = self.spill_map.get(&v) {
                            m.src = MoveLocation::StackSlot {
                                base: PhysicalRegister(5),
                                offset: slot,
                            };
                        }
                    }
                }
                out.push(MachineInstruction::ParallelMove { moves });
            }

            MachineInstruction::Push { mut src } => {
                if let Some((slot, sz, _)) = self.get_spill(&src) {
                    out.push(MachineInstruction::Load {
                        dst: Self::phys_op(gpr_s1),
                        src: Self::stack_op(slot),
                        size: sz,
                    });
                    out.push(MachineInstruction::Push {
                        src: Self::phys_op(gpr_s1),
                    });
                } else {
                    self.rewrite_operand(&mut src, gpr_s1, fp_s1, out);
                    out.push(MachineInstruction::Push { src });
                }
            }

            MachineInstruction::Pop { mut dst } => {
                if let Some((slot, sz, _)) = self.get_spill(&dst) {
                    out.push(MachineInstruction::Pop {
                        dst: Self::phys_op(gpr_s1),
                    });
                    out.push(MachineInstruction::Store {
                        dst: Self::stack_op(slot),
                        src: Self::phys_op(gpr_s1),
                        size: sz,
                    });
                } else {
                    self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
                    out.push(MachineInstruction::Pop { dst });
                }
            }

            MachineInstruction::VectorAdd {
                mut dst,
                mut src,
                vec_type,
            } => {
                self.rewrite_operand(&mut src, gpr_s2, fp_s2, out);
                self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
                out.push(MachineInstruction::VectorAdd { dst, src, vec_type });
            }
            MachineInstruction::VectorSub {
                mut dst,
                mut src,
                vec_type,
            } => {
                self.rewrite_operand(&mut src, gpr_s2, fp_s2, out);
                self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
                out.push(MachineInstruction::VectorSub { dst, src, vec_type });
            }
            MachineInstruction::VectorMul {
                mut dst,
                mut src,
                vec_type,
            } => {
                self.rewrite_operand(&mut src, gpr_s2, fp_s2, out);
                self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
                out.push(MachineInstruction::VectorMul { dst, src, vec_type });
            }
            MachineInstruction::VectorDiv {
                mut dst,
                mut src,
                vec_type,
            } => {
                self.rewrite_operand(&mut src, gpr_s2, fp_s2, out);
                self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
                out.push(MachineInstruction::VectorDiv { dst, src, vec_type });
            }
            MachineInstruction::VectorAnd {
                mut dst,
                mut src,
                vec_type,
            } => {
                self.rewrite_operand(&mut src, gpr_s2, fp_s2, out);
                self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
                out.push(MachineInstruction::VectorAnd { dst, src, vec_type });
            }
            MachineInstruction::VectorOr {
                mut dst,
                mut src,
                vec_type,
            } => {
                self.rewrite_operand(&mut src, gpr_s2, fp_s2, out);
                self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
                out.push(MachineInstruction::VectorOr { dst, src, vec_type });
            }
            MachineInstruction::VectorXor {
                mut dst,
                mut src,
                vec_type,
            } => {
                self.rewrite_operand(&mut src, gpr_s2, fp_s2, out);
                self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
                out.push(MachineInstruction::VectorXor { dst, src, vec_type });
            }
            MachineInstruction::VectorLoad {
                mut dst,
                mut src,
                vec_type,
            } => {
                self.rewrite_operand(&mut src, gpr_s2, fp_s2, out);
                self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
                out.push(MachineInstruction::VectorLoad { dst, src, vec_type });
            }
            MachineInstruction::VectorStore {
                mut dst,
                mut src,
                vec_type,
            } => {
                self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
                self.rewrite_operand(&mut src, gpr_s2, fp_s2, out);
                out.push(MachineInstruction::VectorStore { dst, src, vec_type });
            }
            MachineInstruction::VectorBroadcast {
                mut dst,
                mut src,
                vec_type,
            } => {
                self.rewrite_operand(&mut src, gpr_s2, fp_s2, out);
                self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
                out.push(MachineInstruction::VectorBroadcast { dst, src, vec_type });
            }
            MachineInstruction::VectorShuffle {
                mut dst,
                mut src,
                mask,
                vec_type,
            } => {
                self.rewrite_operand(&mut src, gpr_s2, fp_s2, out);
                self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
                out.push(MachineInstruction::VectorShuffle {
                    dst,
                    src,
                    mask,
                    vec_type,
                });
            }
            MachineInstruction::VectorReduceAdd {
                mut dst,
                mut src,
                vec_type,
            } => {
                self.rewrite_operand(&mut src, gpr_s2, fp_s2, out);
                self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
                out.push(MachineInstruction::VectorReduceAdd { dst, src, vec_type });
            }
            MachineInstruction::VectorMin {
                mut dst,
                mut src,
                vec_type,
            } => {
                self.rewrite_operand(&mut src, gpr_s2, fp_s2, out);
                self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
                out.push(MachineInstruction::VectorMin { dst, src, vec_type });
            }
            MachineInstruction::VectorMax {
                mut dst,
                mut src,
                vec_type,
            } => {
                self.rewrite_operand(&mut src, gpr_s2, fp_s2, out);
                self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
                out.push(MachineInstruction::VectorMax { dst, src, vec_type });
            }
            MachineInstruction::VectorCmp {
                mut dst,
                mut src,
                cc,
                vec_type,
            } => {
                self.rewrite_operand(&mut src, gpr_s2, fp_s2, out);
                self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
                out.push(MachineInstruction::VectorCmp {
                    dst,
                    src,
                    cc,
                    vec_type,
                });
            }
            MachineInstruction::VectorBlend {
                mut dst,
                mut src,
                mask,
                vec_type,
            } => {
                self.rewrite_operand(&mut src, gpr_s2, fp_s2, out);
                self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
                out.push(MachineInstruction::VectorBlend {
                    dst,
                    src,
                    mask,
                    vec_type,
                });
            }
            MachineInstruction::VectorShiftLeft {
                mut dst,
                mut src,
                count,
                vec_type,
            } => {
                self.rewrite_operand(&mut src, gpr_s2, fp_s2, out);
                self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
                out.push(MachineInstruction::VectorShiftLeft {
                    dst,
                    src,
                    count,
                    vec_type,
                });
            }
            MachineInstruction::VectorShiftRight {
                mut dst,
                mut src,
                count,
                vec_type,
            } => {
                self.rewrite_operand(&mut src, gpr_s2, fp_s2, out);
                self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
                out.push(MachineInstruction::VectorShiftRight {
                    dst,
                    src,
                    count,
                    vec_type,
                });
            }
            MachineInstruction::AtomicLoad {
                mut dst,
                mut src,
                size,
            } => {
                self.rewrite_operand(&mut src, gpr_s2, fp_s2, out);
                self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
                out.push(MachineInstruction::AtomicLoad { dst, src, size });
            }
            MachineInstruction::AtomicStore {
                mut dst,
                mut src,
                size,
            } => {
                self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
                self.rewrite_operand(&mut src, gpr_s2, fp_s2, out);
                out.push(MachineInstruction::AtomicStore { dst, src, size });
            }
            MachineInstruction::AtomicFetchAdd {
                mut dst,
                mut src,
                size,
            } => {
                self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
                self.rewrite_operand(&mut src, gpr_s2, fp_s2, out);
                out.push(MachineInstruction::AtomicFetchAdd { dst, src, size });
            }
            MachineInstruction::AtomicCompareExchange {
                mut dst,
                mut expected,
                mut desired,
                size,
            } => {
                self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
                self.rewrite_operand(&mut expected, gpr_s2, fp_s2, out);
                self.rewrite_operand(&mut desired, gpr_s1, fp_s1, out);
                out.push(MachineInstruction::AtomicCompareExchange {
                    dst,
                    expected,
                    desired,
                    size,
                });
            }

            MachineInstruction::Custom { name, mut operands } => {
                for op in &mut operands {
                    self.rewrite_operand(op, gpr_s1, fp_s1, out);
                }
                out.push(MachineInstruction::Custom { name, operands });
            }

            other => out.push(other),
        }
    }

    fn get_spill(&self, op: &MachineOperand) -> Option<(i32, u8, RegisterClass)> {
        if let MachineOperand::Register(MachineRegister::Virtual(v)) = op
            && let Some(&slot) = self.spill_map.get(v)
        {
            let size = self.spill_size_map.get(v).copied().unwrap_or(8);
            let class = if size == 4 {
                RegisterClass::Float
            } else {
                RegisterClass::Gpr
            };
            return Some((slot, size, class));
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn rewrite_gpr_binop<F>(
        &self,
        mut dst: MachineOperand,
        mut src: MachineOperand,
        out: &mut Vec<MachineInstruction>,
        gpr_s1: PhysicalRegister,
        gpr_s2: PhysicalRegister,
        fp_s2: PhysicalRegister,
        make_inst: F,
    ) where
        F: FnOnce(MachineOperand, MachineOperand) -> MachineInstruction,
    {
        let dst_spill = self.get_spill(&dst);
        let src_spill = self.get_spill(&src);

        match (dst_spill, src_spill) {
            (Some((dst_slot, dst_size, _)), Some((src_slot, src_size, _))) => {
                out.push(MachineInstruction::Load {
                    dst: Self::phys_op(gpr_s1),
                    src: Self::stack_op(dst_slot),
                    size: dst_size,
                });
                out.push(MachineInstruction::Load {
                    dst: Self::phys_op(gpr_s2),
                    src: Self::stack_op(src_slot),
                    size: src_size,
                });
                out.push(make_inst(Self::phys_op(gpr_s1), Self::phys_op(gpr_s2)));
                out.push(MachineInstruction::Store {
                    dst: Self::stack_op(dst_slot),
                    src: Self::phys_op(gpr_s1),
                    size: dst_size,
                });
            }
            (Some((dst_slot, dst_size, _)), None) => {
                out.push(MachineInstruction::Load {
                    dst: Self::phys_op(gpr_s1),
                    src: Self::stack_op(dst_slot),
                    size: dst_size,
                });
                self.rewrite_operand(&mut src, gpr_s2, fp_s2, out);
                out.push(make_inst(Self::phys_op(gpr_s1), src));
                out.push(MachineInstruction::Store {
                    dst: Self::stack_op(dst_slot),
                    src: Self::phys_op(gpr_s1),
                    size: dst_size,
                });
            }
            (None, Some((src_slot, src_size, _))) => {
                self.rewrite_operand(&mut dst, gpr_s1, fp_s2, out);
                out.push(MachineInstruction::Load {
                    dst: Self::phys_op(gpr_s2),
                    src: Self::stack_op(src_slot),
                    size: src_size,
                });
                out.push(make_inst(dst, Self::phys_op(gpr_s2)));
            }
            (None, None) => {
                self.rewrite_operand(&mut dst, gpr_s1, fp_s2, out);
                self.rewrite_operand(&mut src, gpr_s2, fp_s2, out);
                out.push(make_inst(dst, src));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn rewrite_fp_binop<F>(
        &self,
        mut dst: MachineOperand,
        mut src: MachineOperand,
        size: u8,
        out: &mut Vec<MachineInstruction>,
        fp_s1: PhysicalRegister,
        fp_s2: PhysicalRegister,
        gpr_s2: PhysicalRegister,
        make_inst: F,
    ) where
        F: FnOnce(MachineOperand, MachineOperand, u8) -> MachineInstruction,
    {
        let dst_spill = self.get_spill(&dst);
        let src_spill = self.get_spill(&src);

        match (dst_spill, src_spill) {
            (Some((dst_slot, dst_size, _)), Some((src_slot, src_size, _))) => {
                out.push(MachineInstruction::Load {
                    dst: Self::phys_op(fp_s1),
                    src: Self::stack_op(dst_slot),
                    size: dst_size,
                });
                out.push(MachineInstruction::Load {
                    dst: Self::phys_op(fp_s2),
                    src: Self::stack_op(src_slot),
                    size: src_size,
                });
                out.push(make_inst(Self::phys_op(fp_s1), Self::phys_op(fp_s2), size));
                out.push(MachineInstruction::Store {
                    dst: Self::stack_op(dst_slot),
                    src: Self::phys_op(fp_s1),
                    size: dst_size,
                });
            }
            (Some((dst_slot, dst_size, _)), None) => {
                out.push(MachineInstruction::Load {
                    dst: Self::phys_op(fp_s1),
                    src: Self::stack_op(dst_slot),
                    size: dst_size,
                });
                self.rewrite_operand(&mut src, gpr_s2, fp_s2, out);
                out.push(make_inst(Self::phys_op(fp_s1), src, size));
                out.push(MachineInstruction::Store {
                    dst: Self::stack_op(dst_slot),
                    src: Self::phys_op(fp_s1),
                    size: dst_size,
                });
            }
            (None, Some((src_slot, src_size, _))) => {
                self.rewrite_operand(&mut dst, gpr_s2, fp_s1, out);
                out.push(MachineInstruction::Load {
                    dst: Self::phys_op(fp_s2),
                    src: Self::stack_op(src_slot),
                    size: src_size,
                });
                out.push(make_inst(dst, Self::phys_op(fp_s2), size));
            }
            (None, None) => {
                self.rewrite_operand(&mut dst, gpr_s2, fp_s1, out);
                self.rewrite_operand(&mut src, gpr_s2, fp_s2, out);
                out.push(make_inst(dst, src, size));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn rewrite_cmp<F>(
        &self,
        mut lhs: MachineOperand,
        mut rhs: MachineOperand,
        out: &mut Vec<MachineInstruction>,
        gpr_s1: PhysicalRegister,
        gpr_s2: PhysicalRegister,
        fp_s2: PhysicalRegister,
        make_inst: F,
    ) where
        F: FnOnce(MachineOperand, MachineOperand) -> MachineInstruction,
    {
        let lhs_spill = self.get_spill(&lhs);
        let rhs_spill = self.get_spill(&rhs);

        match (lhs_spill, rhs_spill) {
            (Some((l_slot, l_sz, _)), Some((r_slot, r_sz, _))) => {
                out.push(MachineInstruction::Load {
                    dst: Self::phys_op(gpr_s1),
                    src: Self::stack_op(l_slot),
                    size: l_sz,
                });
                out.push(MachineInstruction::Load {
                    dst: Self::phys_op(gpr_s2),
                    src: Self::stack_op(r_slot),
                    size: r_sz,
                });
                out.push(make_inst(Self::phys_op(gpr_s1), Self::phys_op(gpr_s2)));
            }
            (Some((l_slot, l_sz, _)), None) => {
                out.push(MachineInstruction::Load {
                    dst: Self::phys_op(gpr_s1),
                    src: Self::stack_op(l_slot),
                    size: l_sz,
                });
                self.rewrite_operand(&mut rhs, gpr_s2, fp_s2, out);
                out.push(make_inst(Self::phys_op(gpr_s1), rhs));
            }
            (None, Some((r_slot, r_sz, _))) => {
                self.rewrite_operand(&mut lhs, gpr_s1, fp_s2, out);
                out.push(MachineInstruction::Load {
                    dst: Self::phys_op(gpr_s2),
                    src: Self::stack_op(r_slot),
                    size: r_sz,
                });
                out.push(make_inst(lhs, Self::phys_op(gpr_s2)));
            }
            (None, None) => {
                self.rewrite_operand(&mut lhs, gpr_s1, fp_s2, out);
                self.rewrite_operand(&mut rhs, gpr_s2, fp_s2, out);
                out.push(make_inst(lhs, rhs));
            }
        }
    }

    fn rewrite_unop<F>(
        &self,
        mut dst: MachineOperand,
        out: &mut Vec<MachineInstruction>,
        gpr_s1: PhysicalRegister,
        fp_s1: PhysicalRegister,
        make_inst: F,
    ) where
        F: FnOnce(MachineOperand) -> MachineInstruction,
    {
        if let Some((slot, sz, _)) = self.get_spill(&dst) {
            out.push(MachineInstruction::Load {
                dst: Self::phys_op(gpr_s1),
                src: Self::stack_op(slot),
                size: sz,
            });
            out.push(make_inst(Self::phys_op(gpr_s1)));
            out.push(MachineInstruction::Store {
                dst: Self::stack_op(slot),
                src: Self::phys_op(gpr_s1),
                size: sz,
            });
        } else {
            self.rewrite_operand(&mut dst, gpr_s1, fp_s1, out);
            out.push(make_inst(dst));
        }
    }

    fn rewrite_operand(
        &self,
        op: &mut MachineOperand,
        gpr_scratch: PhysicalRegister,
        _fp_scratch: PhysicalRegister,
        out: &mut Vec<MachineInstruction>,
    ) {
        match op {
            MachineOperand::Register(MachineRegister::Virtual(v)) => {
                if let Some(&p) = self.vreg_map.get(v) {
                    *op = MachineOperand::Register(MachineRegister::Physical(p));
                } else if let Some(&slot) = self.spill_map.get(v) {
                    *op = MachineOperand::StackSlot(slot);
                }
            }
            MachineOperand::Memory { base, index, .. } => {
                if let MachineRegister::Virtual(v) = base {
                    if let Some(&p) = self.vreg_map.get(v) {
                        *base = MachineRegister::Physical(p);
                    } else if let Some(&slot) = self.spill_map.get(v) {
                        out.push(MachineInstruction::Load {
                            dst: Self::phys_op(gpr_scratch),
                            src: Self::stack_op(slot),
                            size: 8,
                        });
                        *base = MachineRegister::Physical(gpr_scratch);
                    }
                }
                if let Some((idx_reg, _)) = index
                    && let MachineRegister::Virtual(v) = idx_reg
                {
                    if let Some(&p) = self.vreg_map.get(v) {
                        *idx_reg = MachineRegister::Physical(p);
                    } else if let Some(&slot) = self.spill_map.get(v) {
                        out.push(MachineInstruction::Load {
                            dst: Self::phys_op(gpr_scratch),
                            src: Self::stack_op(slot),
                            size: 8,
                        });
                        *idx_reg = MachineRegister::Physical(gpr_scratch);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Production Linear Scan Register Allocator.
pub struct LinearScanAllocator<'a> {
    reg_file: &'a dyn RegisterFile,
}

impl<'a> LinearScanAllocator<'a> {
    pub fn new(reg_file: &'a dyn RegisterFile) -> Self {
        Self { reg_file }
    }

    pub fn allocate(
        &self,
        func: &mut MachineFunction,
    ) -> Result<AllocationResult, VerificationError> {
        let original = func.clone();
        match self.allocate_attempt(func, false) {
            Ok(result) => Ok(result),
            Err(_) => {
                *func = original.clone();
                match self.allocate_attempt(func, true) {
                    Ok(result) => Ok(result),
                    Err(error) => {
                        *func = original;
                        Err(error)
                    }
                }
            }
        }
    }

    fn allocate_attempt(
        &self,
        func: &mut MachineFunction,
        force_spill: bool,
    ) -> Result<AllocationResult, VerificationError> {
        let liveness = LivenessAnalysis::compute(func);
        let intervals = liveness.build_live_ranges(func);
        let position_weights = Self::instruction_loop_weights(func, &liveness);
        // The loop below assigns registers on clones; the verifier must see
        // those decisions, not the unassigned originals.
        let mut assigned_intervals: Vec<LiveInterval> = Vec::with_capacity(intervals.len());

        let mut vreg_map = HashMap::new();
        let mut spill_map = HashMap::new();
        let mut spill_size_map = HashMap::new();
        let mut used_callee_saved = HashSet::new();
        let mut active: Vec<LiveInterval> = Vec::new();

        let locals_end = (func.stack_size as i32 + 7) & !7;
        let mut spill_manager = SpillSlotManager::new();

        // Call positions are collected once per function and used with a binary
        // search below (they are naturally sorted by instruction index).
        let mut call_indices = Vec::new();
        let mut call_sites = Vec::new();
        let mut inst_idx = 0;
        for block in &func.blocks {
            for (local_idx, inst) in block.instructions.iter().enumerate() {
                if matches!(inst, MachineInstruction::Call { .. }) {
                    call_indices.push(inst_idx);
                    call_sites.push((inst_idx, block.id, local_idx));
                }
                inst_idx += 1;
            }
        }

        let mut preserved_calls: HashSet<(VirtualRegister, usize)> = HashSet::new();
        let mut preservation_slots: HashMap<VirtualRegister, i32> = HashMap::new();

        for mut current in intervals.clone() {
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

            // Expire intervals that ended at or before current.start(). `active`
            // is kept sorted by end position, so the expired prefix is drained
            // in O(expired) instead of rescanning every active interval.
            let cutoff = current.start();
            let expired = active.partition_point(|act| act.end() <= cutoff);
            active.drain(..expired);

            let used_phys: HashSet<PhysicalRegister> =
                active.iter().filter_map(|act| act.assigned_reg).collect();

            // Exact instruction-level liveness distinguishes arguments that
            // die at the call from values that must survive its clobbers.
            let crossed_calls: Vec<usize> = call_indices
                .iter()
                .copied()
                .filter(|&call_idx| {
                    liveness
                        .live_after_inst(call_idx)
                        .is_some_and(|live| live.contains(&MachineRegister::Virtual(current.vreg)))
                })
                .collect();
            let crosses_call = !crossed_calls.is_empty();

            let call_result_registers: HashSet<PhysicalRegister> = call_sites
                .iter()
                .filter(|(call_idx, _, _)| crossed_calls.contains(call_idx))
                .flat_map(|(_, block_id, local_idx)| {
                    func.blocks[*block_id as usize].instructions[*local_idx]
                        .defs()
                        .into_iter()
                        .filter_map(|reg| match reg {
                            MachineRegister::Physical(p) => Some(p),
                            MachineRegister::Virtual(_) => None,
                        })
                })
                .collect();
            let can_preserve_in_caller_saved = |reg: PhysicalRegister| {
                class == RegisterClass::Gpr
                    && self.reg_file.caller_saved_for_class(class).contains(&reg)
                    && !call_result_registers.contains(&reg)
            };
            let allowed_for_interval = |reg: PhysicalRegister| {
                let constraint_allows = match current.constraint {
                    RegisterConstraint::Fixed(fixed) if fixed != reg => false,
                    RegisterConstraint::DifferentFrom(forbidden) if forbidden == reg => false,
                    _ => true,
                };
                constraint_allows
                    && (!crosses_call
                        || callee_set.contains(&reg)
                        || can_preserve_in_caller_saved(reg))
            };

            let chosen_reg = if force_spill
                && !matches!(current.constraint, RegisterConstraint::Fixed(_))
            {
                None
            } else {
                match current.constraint {
                    RegisterConstraint::Fixed(p) => {
                        if !used_phys.contains(&p) && allowed_for_interval(p) {
                            Some(p)
                        } else {
                            None
                        }
                    }
                    RegisterConstraint::DifferentFrom(forbidden) => {
                        allocatable.iter().copied().find(|r| {
                            *r != forbidden && !used_phys.contains(r) && allowed_for_interval(*r)
                        })
                    }
                    _ => {
                        if crosses_call {
                            // Prefer a free callee-saved register. If none exists,
                            // a caller-saved GPR can be split around the call using
                            // stack copies, except when the call returns in it.
                            callee_saved
                                .iter()
                                .copied()
                                .find(|r| !used_phys.contains(r))
                                .or_else(|| {
                                    allocatable.iter().copied().find(|r| {
                                        !used_phys.contains(r) && can_preserve_in_caller_saved(*r)
                                    })
                                })
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
                        }
                    }
                }
            };

            if let Some(free_reg) = chosen_reg {
                current.assigned_reg = Some(free_reg);
                vreg_map.insert(current.vreg, free_reg);
                if callee_set.contains(&free_reg) {
                    used_callee_saved.insert(free_reg);
                } else if crosses_call && can_preserve_in_caller_saved(free_reg) {
                    preservation_slots.entry(current.vreg).or_insert_with(|| {
                        spill_manager.allocate_slot(class, 8, 8, &current.segments, locals_end)
                    });
                    for call_idx in crossed_calls {
                        preserved_calls.insert((current.vreg, call_idx));
                    }
                }
                assigned_intervals.push(current.clone());
                Self::insert_active(&mut active, current);
            } else {
                let current_priority =
                    Self::spill_priority(&current, current.start(), &position_weights);
                let current_reg_allowed = |reg: PhysicalRegister| match current.constraint {
                    RegisterConstraint::Fixed(fixed) => reg == fixed,
                    RegisterConstraint::DifferentFrom(forbidden) => {
                        reg != forbidden && allocatable.contains(&reg)
                    }
                    RegisterConstraint::Any | RegisterConstraint::SameAs(_) => {
                        allocatable.contains(&reg)
                    }
                };
                let victim = if force_spill {
                    None
                } else {
                    active
                        .iter()
                        .enumerate()
                        .filter(|(_, active_range)| current.overlaps(active_range))
                        .filter_map(|(idx, active_range)| {
                            let reg = active_range.assigned_reg?;
                            if !current_reg_allowed(reg)
                                || !allowed_for_interval(reg)
                                || matches!(active_range.constraint, RegisterConstraint::Fixed(_))
                            {
                                return None;
                            }
                            let priority = Self::spill_priority(
                                active_range,
                                current.start(),
                                &position_weights,
                            );
                            (priority.0 < current_priority.0)
                                .then_some((idx, priority.0, priority.1))
                        })
                        .min_by_key(|(_, score, next_use)| (*score, std::cmp::Reverse(*next_use)))
                };

                if let Some((victim_idx, _, _)) = victim {
                    // Spill the lower-cost value and give its register to the
                    // higher-cost interval. Verification gates the rewritten IR.
                    let mut evicted = active.remove(victim_idx);
                    let reg = evicted
                        .assigned_reg
                        .take()
                        .expect("selected victim has a register");
                    let slot = spill_manager.allocate_slot(
                        evicted.class,
                        8,
                        8,
                        &evicted.segments,
                        locals_end,
                    );
                    evicted.spill_slot = Some(slot);
                    vreg_map.remove(&evicted.vreg);
                    spill_map.insert(evicted.vreg, slot);
                    spill_size_map.insert(evicted.vreg, 8);
                    if let Some(assigned) = assigned_intervals
                        .iter_mut()
                        .find(|assigned| assigned.vreg == evicted.vreg)
                    {
                        assigned.assigned_reg = None;
                        assigned.spill_slot = Some(slot);
                    }

                    current.assigned_reg = Some(reg);
                    vreg_map.insert(current.vreg, reg);
                    if callee_set.contains(&reg) {
                        used_callee_saved.insert(reg);
                    }
                    assigned_intervals.push(current.clone());
                    Self::insert_active(&mut active, current);
                } else {
                    let slot =
                        spill_manager.allocate_slot(class, 8, 8, &current.segments, locals_end);
                    current.spill_slot = Some(slot);
                    spill_map.insert(current.vreg, slot);
                    spill_size_map.insert(current.vreg, 8);
                    assigned_intervals.push(current.clone());
                    Self::insert_active(&mut active, current);
                }
            }
        }

        // Keep an up-to-date stack copy after every definition for values
        // assigned to caller-saved registers across calls. This remains valid
        // even when argument setup overwrites those registers before a call.
        if !preservation_slots.is_empty() {
            let mut global_idx = 0usize;
            for block in &mut func.blocks {
                let mut rewritten = Vec::with_capacity(block.instructions.len());
                for inst in block.instructions.drain(..) {
                    let is_call = matches!(inst, MachineInstruction::Call { .. });
                    let defined: Vec<VirtualRegister> = inst
                        .defs()
                        .into_iter()
                        .filter_map(|reg| match reg {
                            MachineRegister::Virtual(v) if preservation_slots.contains_key(&v) => {
                                Some(v)
                            }
                            _ => None,
                        })
                        .collect();
                    rewritten.push(inst);
                    for vreg in defined {
                        let slot = preservation_slots[&vreg];
                        rewritten.push(MachineInstruction::Store {
                            dst: MachineOperand::StackSlot(slot),
                            src: MachineOperand::Register(MachineRegister::Virtual(vreg)),
                            size: 8,
                        });
                    }
                    if is_call {
                        for &(vreg, call_idx) in &preserved_calls {
                            if call_idx == global_idx {
                                let slot = preservation_slots[&vreg];
                                rewritten.push(MachineInstruction::Load {
                                    dst: MachineOperand::Register(MachineRegister::Virtual(vreg)),
                                    src: MachineOperand::StackSlot(slot),
                                    size: 8,
                                });
                            }
                        }
                    }
                    global_idx += 1;
                }
                block.instructions = rewritten;
            }
        }

        let deepest_offset = spill_map
            .values()
            .chain(preservation_slots.values())
            .min()
            .copied()
            .unwrap_or(-locals_end);
        let total_spill_bytes = ((-deepest_offset) - locals_end).max(0) as u32;
        func.stack_size += total_spill_bytes as u64;

        // Perform systematic spill rewriting
        let rewriter = SpillRewriter::new(self.reg_file, &vreg_map, &spill_map, &spill_size_map);
        rewriter.rewrite_function(func);

        AllocationVerifier::verify(
            func,
            &vreg_map,
            &spill_map,
            &used_callee_saved,
            &assigned_intervals,
            &call_indices,
            &preserved_calls,
            self.reg_file,
        )?;

        Ok(AllocationResult {
            vreg_map,
            spill_map,
            spill_size_map,
            total_spill_bytes,
            used_callee_saved,
        })
    }

    /// Insert an interval into the expiry-sorted `active` list.
    fn insert_active(active: &mut Vec<LiveInterval>, interval: LiveInterval) {
        let pos = active.partition_point(|act| act.end() <= interval.end());
        active.insert(pos, interval);
    }

    /// Assign exponential weights to instructions in natural loops. Nested
    /// loops receive higher weights, capped to keep calculations bounded.
    fn instruction_loop_weights(func: &MachineFunction, liveness: &LivenessAnalysis) -> Vec<u32> {
        let dominators = crate::opt::DominatorTree::compute(func);
        let mut loop_depth: HashMap<u32, u32> = HashMap::new();

        for latch in &func.blocks {
            for &header in &latch.successors {
                if !dominators.dominates(header, latch.id) {
                    continue;
                }
                let mut loop_blocks = HashSet::from([header, latch.id]);
                let mut worklist = vec![latch.id];
                while let Some(block_id) = worklist.pop() {
                    if let Some(block) = func.blocks.iter().find(|block| block.id == block_id) {
                        for &pred in &block.predecessors {
                            if loop_blocks.insert(pred) && pred != header {
                                worklist.push(pred);
                            }
                        }
                    }
                }
                for block_id in loop_blocks {
                    let depth = loop_depth.entry(block_id).or_default();
                    *depth = depth.saturating_add(1).min(4);
                }
            }
        }

        let mut weights = vec![1; liveness.total_instructions];
        for block in &func.blocks {
            let Some(&(start, end)) = liveness.block_ranges.get(&block.id) else {
                continue;
            };
            let multiplier = 10u32.pow(loop_depth.get(&block.id).copied().unwrap_or(0));
            for weight in weights.iter_mut().take(end).skip(start) {
                *weight = multiplier;
            }
        }
        weights
    }

    /// Approximate spill cost from future-use density, loop frequency, and
    /// distance. High near-term weighted use protects the interval.
    fn spill_priority(
        interval: &LiveInterval,
        position: usize,
        position_weights: &[u32],
    ) -> (u128, usize) {
        let next_use = interval
            .use_positions
            .iter()
            .copied()
            .filter(|&use_pos| use_pos >= position)
            .min()
            .unwrap_or(usize::MAX);
        if next_use == usize::MAX {
            return (0, next_use);
        }
        let weighted_uses: u128 = interval
            .use_positions
            .iter()
            .copied()
            .filter(|&use_pos| use_pos >= position)
            .map(|use_pos| position_weights.get(use_pos).copied().unwrap_or(1) as u128)
            .sum();
        let distance = next_use.saturating_sub(position).max(1) as u128;
        (weighted_uses.saturating_mul(1_000_000) / distance, next_use)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine_ir::{MachineFunction, MachineInstruction, MachineOperand};
    use crate::targets::x86_64::X86_64RegisterFile;

    #[test]
    fn test_half_open_interval_overlap_semantics() {
        let s_0_3 = LiveSegment::new(0, 3);
        let s_3_6 = LiveSegment::new(3, 6);
        let s_4_7 = LiveSegment::new(4, 7);
        let s_2_5 = LiveSegment::new(2, 5);

        // [0, 3) vs [3, 6) -> touching boundary, no overlap
        assert!(!s_0_3.overlaps(&s_3_6));
        assert!(!s_3_6.overlaps(&s_0_3));

        // [0, 3) vs [4, 7) -> completely disjoint, no overlap
        assert!(!s_0_3.overlaps(&s_4_7));

        // [0, 3) vs [2, 5) -> overlapping at 2, overlap true
        assert!(s_0_3.overlaps(&s_2_5));
        assert!(s_2_5.overlaps(&s_0_3));

        // [2, 5) vs [3, 6) -> overlapping on [3, 5), overlap true
        assert!(s_2_5.overlaps(&s_3_6));
    }

    #[test]
    fn test_exact_liveness_and_range_expiration() {
        let mut func = MachineFunction::new("test_linear");
        let v0 = func.alloc_vreg();
        let v1 = func.alloc_vreg();
        let v2 = func.alloc_vreg();

        let entry = func.entry_block_mut();
        // 0: v0 = 10
        entry.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
            src: MachineOperand::Immediate(10),
        });
        // 1: v1 = 20
        entry.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(v1)),
            src: MachineOperand::Immediate(20),
        });
        // 2: Add v0, v1 (v1 dies here at pos 3)
        entry.push(MachineInstruction::Add {
            dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
            src: MachineOperand::Register(MachineRegister::Virtual(v1)),
        });
        // 3: v2 = 30 (starts at pos 3)
        entry.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(v2)),
            src: MachineOperand::Immediate(30),
        });
        // 4: Add v0, v2
        entry.push(MachineInstruction::Add {
            dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
            src: MachineOperand::Register(MachineRegister::Virtual(v2)),
        });
        // 5: Return v0
        entry.push(MachineInstruction::Return);

        let liveness = LivenessAnalysis::compute(&mut func);
        let ranges = liveness.build_live_ranges(&func);

        let r_v1 = ranges.iter().find(|r| r.vreg == v1).expect("v1 range");
        let r_v2 = ranges.iter().find(|r| r.vreg == v2).expect("v2 range");

        // v1 is live [1, 3), v2 is live [3, 5) -> they must NOT overlap!
        assert_eq!(r_v1.segments, vec![LiveSegment::new(1, 3)]);
        assert_eq!(r_v2.segments, vec![LiveSegment::new(3, 5)]);
        assert!(!r_v1.overlaps(r_v2));
    }

    #[test]
    fn test_spill_slot_lifetime_reuse() {
        let mut manager = SpillSlotManager::new();
        let seg1 = vec![LiveSegment::new(0, 5)];
        let seg2 = vec![LiveSegment::new(5, 10)];
        let seg3 = vec![LiveSegment::new(3, 8)];

        let slot1 = manager.allocate_slot(RegisterClass::Gpr, 8, 8, &seg1, 0);
        let slot2 = manager.allocate_slot(RegisterClass::Gpr, 8, 8, &seg2, 0);
        // seg1 and seg2 do not overlap, so slot2 must reuse slot1
        assert_eq!(slot1, slot2);

        // seg3 overlaps with seg2, so it cannot reuse and gets a distinct offset
        let slot3 = manager.allocate_slot(RegisterClass::Gpr, 8, 8, &seg3, 0);
        assert_ne!(slot1, slot3);
    }

    #[test]
    fn test_verifier_rejects_interfering_assignment() {
        let mut func = MachineFunction::new("test_verifier_interference");
        let v0 = func.alloc_vreg();
        let v1 = func.alloc_vreg();
        let reg_file = X86_64RegisterFile::sysv();
        let p = reg_file.allocatable_for_class(RegisterClass::Gpr)[0];

        let mut r0 = LiveRange::new(v0, RegisterClass::Gpr);
        r0.add_segment(0, 4);
        r0.assigned_reg = Some(p);
        let mut r1 = LiveRange::new(v1, RegisterClass::Gpr);
        r1.add_segment(2, 6);
        r1.assigned_reg = Some(p);

        let vreg_map = HashMap::from([(v0, p), (v1, p)]);
        let err = AllocationVerifier::verify(
            &func,
            &vreg_map,
            &HashMap::new(),
            &HashSet::new(),
            &[r0, r1],
            &[],
            &HashSet::new(),
            &reg_file,
        )
        .expect_err("two overlapping ranges in one register must be rejected");
        assert!(matches!(
            err,
            VerificationError::LiveRangeInterference { .. }
        ));
    }

    #[test]
    fn test_caller_saved_across_call_forces_callee_or_spill() {
        let mut func = MachineFunction::new("test_call_clobber");
        let v0 = func.alloc_vreg();

        let entry = func.entry_block_mut();
        entry.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
            src: MachineOperand::Immediate(42),
        });
        entry.push(MachineInstruction::Call {
            target: MachineOperand::Symbol("dummy_call".to_string()),
            num_args: 0,
        });
        entry.push(MachineInstruction::Add {
            dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
            src: MachineOperand::Immediate(1),
        });
        entry.push(MachineInstruction::Return);

        let reg_file = X86_64RegisterFile::sysv();
        let allocator = LinearScanAllocator::new(&reg_file);
        let res = allocator
            .allocate(&mut func)
            .expect("allocation must pass verification");

        if let Some(&p) = res.vreg_map.get(&v0) {
            assert!(
                reg_file
                    .callee_saved_for_class(RegisterClass::Gpr)
                    .contains(&p),
                "Value live across call must be placed in a callee-saved register"
            );
            assert!(res.used_callee_saved.contains(&p));
        } else {
            assert!(res.spill_map.contains_key(&v0));
        }
    }

    #[test]
    fn test_call_boundary_saves_live_value_in_caller_saved_register() {
        let mut func = MachineFunction::new("test_call_boundary_split");
        let reg_file = X86_64RegisterFile::sysv();
        let allocator = LinearScanAllocator::new(&reg_file);
        let count = reg_file.callee_saved_for_class(RegisterClass::Gpr).len() + 1;
        let vregs: Vec<_> = (0..count).map(|_| func.alloc_vreg()).collect();

        let entry = func.entry_block_mut();
        for &vreg in &vregs {
            entry.push(MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Virtual(vreg)),
                src: MachineOperand::Immediate(7),
            });
        }
        entry.push(MachineInstruction::Call {
            target: MachineOperand::Symbol("dummy_call".to_string()),
            num_args: 0,
        });
        for &vreg in &vregs {
            entry.push(MachineInstruction::Compare {
                lhs: MachineOperand::Register(MachineRegister::Virtual(vreg)),
                rhs: MachineOperand::Immediate(0),
            });
        }
        entry.push(MachineInstruction::Return);

        let allocation = allocator
            .allocate(&mut func)
            .expect("call-boundary preservation must pass allocation verification");
        let caller_saved_value = vregs
            .iter()
            .find(|vreg| {
                allocation.vreg_map.get(vreg).is_some_and(|reg| {
                    reg_file
                        .caller_saved_for_class(RegisterClass::Gpr)
                        .contains(reg)
                })
            })
            .expect("callee-saved pressure should select a caller-saved register");
        assert!(!allocation.spill_map.contains_key(caller_saved_value));

        let instructions = &func.blocks[0].instructions;
        let call_pos = instructions
            .iter()
            .position(|inst| matches!(inst, MachineInstruction::Call { .. }))
            .unwrap();
        assert!(
            instructions[..call_pos].iter().any(|inst| matches!(
                inst,
                MachineInstruction::Store {
                    dst: MachineOperand::StackSlot(_),
                    ..
                }
            )),
            "the live value must be saved before the call"
        );
        assert!(
            instructions[call_pos + 1..].iter().any(|inst| matches!(
                inst,
                MachineInstruction::Load {
                    src: MachineOperand::StackSlot(_),
                    ..
                }
            )),
            "the live value must be restored after the call"
        );
    }

    #[test]
    fn test_value_dying_at_call_does_not_get_preservation_slot() {
        let mut func = MachineFunction::new("test_call_argument_lifetime");
        let value = func.alloc_vreg();
        let entry = func.entry_block_mut();
        entry.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(value)),
            src: MachineOperand::Immediate(7),
        });
        entry.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(1))),
            src: MachineOperand::Register(MachineRegister::Virtual(value)),
        });
        entry.push(MachineInstruction::Call {
            target: MachineOperand::Symbol("dummy_call".to_string()),
            num_args: 1,
        });
        entry.push(MachineInstruction::Return);

        let reg_file = X86_64RegisterFile::sysv();
        let allocator = LinearScanAllocator::new(&reg_file);
        let allocation = allocator
            .allocate(&mut func)
            .expect("call argument allocation should pass verification");
        assert!(
            reg_file
                .caller_saved_for_class(RegisterClass::Gpr)
                .contains(&allocation.vreg_map[&value])
        );
        assert!(
            !func.blocks[0].instructions.iter().any(|inst| matches!(
                inst,
                MachineInstruction::Store {
                    dst: MachineOperand::StackSlot(_),
                    ..
                } | MachineInstruction::Load {
                    src: MachineOperand::StackSlot(_),
                    ..
                }
            )),
            "a value dead after the call should not get a preservation slot"
        );
    }

    #[test]
    fn test_verifier_failure_retries_with_conservative_spilling() {
        struct DeliberatelyInvalidRegisterFile;

        impl RegisterFile for DeliberatelyInvalidRegisterFile {
            fn registers(&self) -> &[PhysicalRegister] {
                static REGISTERS: [PhysicalRegister; 1] = [PhysicalRegister(0)];
                &REGISTERS
            }

            fn allocatable(&self) -> &[PhysicalRegister] {
                self.registers()
            }

            fn caller_saved(&self) -> &[PhysicalRegister] {
                self.registers()
            }

            fn callee_saved(&self) -> &[PhysicalRegister] {
                &[]
            }

            fn reserved(&self) -> &[PhysicalRegister] {
                self.registers()
            }

            fn scratch_for_class(
                &self,
                class: RegisterClass,
            ) -> (PhysicalRegister, PhysicalRegister) {
                match class {
                    RegisterClass::Gpr => (PhysicalRegister(1), PhysicalRegister(2)),
                    RegisterClass::Float => (PhysicalRegister::xmm(15), PhysicalRegister::xmm(14)),
                }
            }
        }

        let mut func = MachineFunction::new("test_verified_retry");
        let value = func.alloc_vreg();
        let entry = func.entry_block_mut();
        entry.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(value)),
            src: MachineOperand::Immediate(42),
        });
        entry.push(MachineInstruction::Add {
            dst: MachineOperand::Register(MachineRegister::Virtual(value)),
            src: MachineOperand::Immediate(1),
        });
        entry.push(MachineInstruction::Return);

        let allocator = LinearScanAllocator::new(&DeliberatelyInvalidRegisterFile);
        let allocation = allocator
            .allocate(&mut func)
            .expect("the conservative retry should spill after reserved-register rejection");
        assert!(allocation.vreg_map.is_empty());
        assert!(allocation.spill_map.contains_key(&value));
    }

    #[test]
    fn test_linear_scan_evicts_farther_next_use() {
        let mut func = MachineFunction::new("test_next_use_eviction");
        let reg_file = X86_64RegisterFile::sysv();
        let allocator = LinearScanAllocator::new(&reg_file);
        let pressure = reg_file.allocatable_for_class(RegisterClass::Gpr).len() + 1;
        let vregs: Vec<_> = (0..pressure).map(|_| func.alloc_vreg()).collect();

        let entry = func.entry_block_mut();
        for &vreg in &vregs {
            entry.push(MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Virtual(vreg)),
                src: MachineOperand::Immediate(1),
            });
        }
        // The newest value is used first, so it has the closest next use.
        for &vreg in vregs.iter().rev() {
            entry.push(MachineInstruction::Compare {
                lhs: MachineOperand::Register(MachineRegister::Virtual(vreg)),
                rhs: MachineOperand::Immediate(0),
            });
        }
        entry.push(MachineInstruction::Return);

        let result = allocator
            .allocate(&mut func)
            .expect("eviction and rewritten spills must pass verification");
        assert!(
            result.spill_map.contains_key(&vregs[0]),
            "the farthest-next-use interval should be evicted"
        );
        assert!(
            result.vreg_map.contains_key(vregs.last().unwrap()),
            "the nearer-next-use interval should keep a register"
        );
    }
}
