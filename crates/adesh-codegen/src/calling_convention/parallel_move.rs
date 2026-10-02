//! Production-Grade Parallel Move Resolver for Target Machine IR.
//!
//! Handles simultaneous assignment semantics for function arguments, return values,
//! and register/stack permutations across ABI boundaries.
//!
//! Resolves:
//! - Register-to-register moves and arbitrary dependency chains (in topological order).
//! - Register cycles of any length (2-cycles, 3-cycles, N-cycles) via safely selected temporaries.
//! - Stack-to-register, register-to-stack, and stack-to-stack moves.
//! - Stack slot swaps and mixed register-stack cycles.
//! - Immediate-to-register and immediate-to-stack transfers.
//! - Safe temporary register selection aware of live, reserved, source, and destination registers.

use crate::calling_convention::CallingConvention;
use crate::error::CodegenError;
use crate::machine_ir::{
    MachineInstruction, MachineRegister, MoveLocation, MoveOperation, PhysicalRegister,
    RegisterClass, VirtualRegister,
};
use std::collections::HashSet;

/// Production-Grade Parallel Move Resolver.
#[derive(Debug, Clone)]
pub struct ParallelMoveResolver {
    moves: Vec<MoveOperation>,
    scratch_pool: Vec<PhysicalRegister>,
    reserved_registers: HashSet<PhysicalRegister>,
    live_registers: HashSet<PhysicalRegister>,
    default_stack_base: PhysicalRegister,
    arch: &'static str,
}

impl Default for ParallelMoveResolver {
    fn default() -> Self {
        Self::for_x86_64()
    }
}

impl ParallelMoveResolver {
    pub fn new(arch: &'static str) -> Self {
        Self {
            moves: Vec::new(),
            scratch_pool: Vec::new(),
            reserved_registers: HashSet::new(),
            live_registers: HashSet::new(),
            default_stack_base: PhysicalRegister(4), // RSP
            arch,
        }
    }

    /// Initializes a resolver with x86-64 default scratch and reserved registers.
    ///
    /// Scratch pool: R10 (10), R11 (11), RAX (0).
    /// Reserved: RSP (4), RBP (5).
    pub fn for_x86_64() -> Self {
        let mut resolver = Self::new("x86_64");
        resolver.scratch_pool = vec![
            PhysicalRegister(10), // R10 (caller-saved encoder scratch)
            PhysicalRegister(11), // R11 (caller-saved encoder scratch)
            PhysicalRegister(0),  // RAX (caller-saved)
            PhysicalRegister::xmm(15), // XMM15 (caller-saved FP scratch)
        ];
        resolver.reserved_registers.insert(PhysicalRegister(4)); // RSP
        resolver.reserved_registers.insert(PhysicalRegister(5)); // RBP
        resolver
    }

    /// Sets custom scratch register pool.
    pub fn with_scratch_pool(mut self, pool: Vec<PhysicalRegister>) -> Self {
        self.scratch_pool = pool;
        self
    }

    /// Sets registers that cannot be used as temporaries.
    pub fn with_reserved_registers(mut self, reserved: HashSet<PhysicalRegister>) -> Self {
        self.reserved_registers = reserved;
        self
    }

    /// Sets registers that are live across the move sequence and must not be overwritten as temporaries.
    pub fn with_live_registers(mut self, live: HashSet<PhysicalRegister>) -> Self {
        self.live_registers = live;
        self
    }

    /// Sets the default base register for stack slot operands (e.g. RSP or RBP).
    pub fn with_default_stack_base(mut self, base: PhysicalRegister) -> Self {
        self.default_stack_base = base;
        self
    }

    /// Adds a single parallel move `dst <- src`.
    pub fn add_move(&mut self, dst: MoveLocation, src: MoveLocation, size: u8) {
        self.moves.push(MoveOperation::new(dst, src, size));
    }

    /// Adds a move operation.
    pub fn add_move_op(&mut self, op: MoveOperation) {
        self.moves.push(op);
    }

    /// Adds multiple move operations.
    pub fn add_moves(&mut self, ops: impl IntoIterator<Item = MoveOperation>) {
        for op in ops {
            self.moves.push(op);
        }
    }

    /// Clears all queued moves.
    pub fn clear(&mut self) {
        self.moves.clear();
    }

    /// Resolves the queued parallel moves into a deterministic, safe sequence of MachineInstructions.
    pub fn resolve(&self) -> Result<Vec<MachineInstruction>, CodegenError> {
        self.resolve_moves(&self.moves)
    }

    /// Formats the parallel moves and their resolution for debugging / tracing.
    pub fn debug_resolution(&self) -> Result<String, CodegenError> {
        let mut out = String::new();
        out.push_str("Parallel Moves:\n");
        for m in &self.moves {
            out.push_str(&format!("    {}\n", m));
        }
        let resolved = self.resolve()?;
        out.push_str("Resolved Instructions:\n");
        for inst in &resolved {
            out.push_str(&format!("    {:?}\n", inst));
        }
        Ok(out)
    }

    /// Resolves an arbitrary slice of moves using this resolver's configuration.
    pub fn resolve_moves(
        &self,
        input_moves: &[MoveOperation],
    ) -> Result<Vec<MachineInstruction>, CodegenError> {
        // 1. Validation & No-Op filtering
        let mut pending: Vec<MoveOperation> = Vec::new();
        let mut seen_dsts: HashSet<MoveLocation> = HashSet::new();

        for m in input_moves {
            if m.dst.is_immediate() {
                return Err(CodegenError::new(
                    self.arch,
                    format!("Destination of a move cannot be an immediate value: {:?}", m.dst),
                ));
            }
            // Filter out no-ops: dst == src
            if m.dst == m.src {
                continue;
            }
            // Check for conflicting destinations in the same parallel move set
            if !seen_dsts.insert(m.dst) {
                return Err(CodegenError::new(
                    self.arch,
                    format!(
                        "Conflicting parallel moves: multiple assignments to destination location {}",
                        m.dst
                    ),
                ));
            }
            pending.push(m.clone());
        }

        if pending.is_empty() {
            return Ok(Vec::new());
        }

        let mut emitted: Vec<MachineInstruction> = Vec::new();

        // 2. Loop until all moves are scheduled
        while !pending.is_empty() {
            // (a) Find ready moves: a move D <- S is ready if D is NOT used as S' in any other pending move.
            let mut ready_idx = None;
            for (idx, candidate) in pending.iter().enumerate() {
                let dst_is_read_elsewhere = pending.iter().enumerate().any(|(other_idx, other)| {
                    other_idx != idx && other.src == candidate.dst
                });

                if !dst_is_read_elsewhere {
                    ready_idx = Some(idx);
                    break;
                }
            }

            if let Some(idx) = ready_idx {
                let ready_move = pending.remove(idx);
                self.emit_single_move(&ready_move, &pending, &mut emitted)?;
                continue;
            }

            // (b) No move is ready -> all remaining pending moves form one or more directed cycles!
            // Pick a move in the cycle to break.
            let cycle_move_idx = 0; // Deterministically pick first
            let cycle_move = &pending[cycle_move_idx];
            let cycle_src = cycle_move.src;
            let cycle_dst = cycle_move.dst;
            let cycle_size = cycle_move.size;

            let cycle_class = match cycle_src {
                MoveLocation::PhysicalRegister(p) => p.class(),
                MoveLocation::FloatImmediate(_) => RegisterClass::Float,
                _ => match cycle_dst {
                    MoveLocation::PhysicalRegister(p) => p.class(),
                    _ => RegisterClass::Gpr,
                },
            };

            // Select a safe temporary scratch register matching the cycle's register class
            let temp_reg = self.select_scratch_register(&pending, cycle_class, None)?;

            // Save the cycle source into the temporary
            let save_to_temp = MoveOperation::new(
                MoveLocation::PhysicalRegister(temp_reg),
                cycle_src,
                cycle_size,
            );
            self.emit_single_move(&save_to_temp, &pending, &mut emitted)?;

            // Update the broken move so it now reads from the temporary
            pending[cycle_move_idx].src = MoveLocation::PhysicalRegister(temp_reg);
            // Now the cycle is broken! Next iteration will find at least one ready move.
        }

        Ok(emitted)
    }

    /// Selects an available scratch register from the scratch pool or allocatable registers
    /// that does not collide with pending move locations, reserved registers, or live registers.
    fn select_scratch_register(
        &self,
        pending: &[MoveOperation],
        class: RegisterClass,
        exclude: Option<PhysicalRegister>,
    ) -> Result<PhysicalRegister, CodegenError> {
        let mut used_in_pending: HashSet<PhysicalRegister> = HashSet::new();

        for m in pending {
            if let Some(p) = m.dst.physical_reg() {
                used_in_pending.insert(p);
            }
            if let Some(p) = m.src.physical_reg() {
                used_in_pending.insert(p);
            }
            // Also collect base/index registers in memory operands
            if let MoveLocation::StackSlot { base, .. } = m.dst {
                used_in_pending.insert(base);
            }
            if let MoveLocation::StackSlot { base, .. } = m.src {
                used_in_pending.insert(base);
            }
            if let MoveLocation::Memory { base, index, .. } = m.dst {
                if let MachineRegister::Physical(p) = base {
                    used_in_pending.insert(p);
                }
                if let Some((MachineRegister::Physical(p), _)) = index {
                    used_in_pending.insert(p);
                }
            }
            if let MoveLocation::Memory { base, index, .. } = m.src {
                if let MachineRegister::Physical(p) = base {
                    used_in_pending.insert(p);
                }
                if let Some((MachineRegister::Physical(p), _)) = index {
                    used_in_pending.insert(p);
                }
            }
        }

        // 1. Try candidates in `scratch_pool` matching requested class
        for &cand in &self.scratch_pool {
            if cand.class() != class {
                continue;
            }
            if Some(cand) == exclude {
                continue;
            }
            if !used_in_pending.contains(&cand)
                && !self.reserved_registers.contains(&cand)
                && !self.live_registers.contains(&cand)
            {
                return Ok(cand);
            }
        }

        // 2. Try fallback registers matching requested class
        if class == RegisterClass::Float {
            let fp_fallback = [
                PhysicalRegister::xmm(15),
                PhysicalRegister::xmm(14),
                PhysicalRegister::xmm(13),
                PhysicalRegister::xmm(12),
                PhysicalRegister::xmm(11),
                PhysicalRegister::xmm(10),
                PhysicalRegister::xmm(9),
                PhysicalRegister::xmm(8),
                PhysicalRegister::xmm(0),
                PhysicalRegister::xmm(1),
                PhysicalRegister::xmm(2),
                PhysicalRegister::xmm(3),
                PhysicalRegister::xmm(4),
                PhysicalRegister::xmm(5),
                PhysicalRegister::xmm(6),
                PhysicalRegister::xmm(7),
            ];
            for &cand in &fp_fallback {
                if Some(cand) == exclude {
                    continue;
                }
                if !used_in_pending.contains(&cand)
                    && !self.reserved_registers.contains(&cand)
                    && !self.live_registers.contains(&cand)
                {
                    return Ok(cand);
                }
            }
        } else {
            let fallback_pool = [
                PhysicalRegister(10),
                PhysicalRegister(11),
                PhysicalRegister(0),
                PhysicalRegister(1),
                PhysicalRegister(2),
                PhysicalRegister(6),
                PhysicalRegister(7),
                PhysicalRegister(8),
                PhysicalRegister(9),
            ];

            for &cand in &fallback_pool {
                if Some(cand) == exclude {
                    continue;
                }
                if !used_in_pending.contains(&cand)
                    && !self.reserved_registers.contains(&cand)
                    && !self.live_registers.contains(&cand)
                {
                    return Ok(cand);
                }
            }
        }

        Err(CodegenError::new(
            self.arch,
            format!(
                "ParallelMoveResolver failed: no available {:?} scratch register to break cycle",
                class
            ),
        ))
    }

    /// Emits a single move operation, introducing an intermediate scratch register if memory-to-memory
    /// or if immediate-to-stack transfer requires it.
    fn emit_single_move(
        &self,
        op: &MoveOperation,
        pending: &[MoveOperation],
        emitted: &mut Vec<MachineInstruction>,
    ) -> Result<(), CodegenError> {
        let dst = &op.dst;
        let src = &op.src;
        let size = op.size;

        // Determine register class
        let class = match (dst, src) {
            (MoveLocation::PhysicalRegister(p), _) | (_, MoveLocation::PhysicalRegister(p)) => {
                p.class()
            }
            (MoveLocation::FloatImmediate(_), _) | (_, MoveLocation::FloatImmediate(_)) => {
                RegisterClass::Float
            }
            _ => RegisterClass::Gpr,
        };

        // Case 1: Memory to Memory (Stack to Stack or Memory to Stack)
        if dst.is_memory_or_stack() && src.is_memory_or_stack() {
            // Cannot emit a direct [mem] <- [mem] instruction on x86-64.
            // Select a scratch register matching the class.
            let scratch = self.select_scratch_register(pending, class, None)?;
            let scratch_loc = MoveLocation::PhysicalRegister(scratch);

            // 1. Load: scratch <- src
            self.emit_direct_move(&scratch_loc, src, size, emitted)?;
            // 2. Store: dst <- scratch
            self.emit_direct_move(dst, &scratch_loc, size, emitted)?;
            return Ok(());
        }

        // Case 2: 64-bit Immediate to Memory/Stack (if imm doesn't fit in imm32)
        if dst.is_memory_or_stack() {
            if let MoveLocation::Immediate(val) = src {
                if !(*val >= i32::MIN as i64 && *val <= i32::MAX as i64) {
                    let scratch = self.select_scratch_register(pending, RegisterClass::Gpr, None)?;
                    let scratch_loc = MoveLocation::PhysicalRegister(scratch);
                    self.emit_direct_move(&scratch_loc, src, size, emitted)?;
                    self.emit_direct_move(dst, &scratch_loc, size, emitted)?;
                    return Ok(());
                }
            } else if let MoveLocation::FloatImmediate(_) = src {
                let scratch = self.select_scratch_register(pending, RegisterClass::Float, None)?;
                let scratch_loc = MoveLocation::PhysicalRegister(scratch);
                self.emit_direct_move(&scratch_loc, src, size, emitted)?;
                self.emit_direct_move(dst, &scratch_loc, size, emitted)?;
                return Ok(());
            }
        }

        // Standard direct move / load / store
        self.emit_direct_move(dst, src, size, emitted)
    }

    /// Emits a single direct MachineInstruction for `dst <- src`.
    fn emit_direct_move(
        &self,
        dst: &MoveLocation,
        src: &MoveLocation,
        size: u8,
        emitted: &mut Vec<MachineInstruction>,
    ) -> Result<(), CodegenError> {
        let dst_op = dst.to_operand();
        let src_op = src.to_operand();

        if dst.is_memory_or_stack() {
            // Store: dst is memory/stack
            emitted.push(MachineInstruction::Store {
                dst: dst_op,
                src: src_op,
                size,
            });
        } else if src.is_memory_or_stack() {
            // Load: src is memory/stack
            emitted.push(MachineInstruction::Load {
                dst: dst_op,
                src: src_op,
                size,
            });
        } else {
            // Register-to-register or Immediate-to-register
            emitted.push(MachineInstruction::Move {
                dst: dst_op,
                src: src_op,
            });
        }

        Ok(())
    }
}

/// Shuffles function call arguments into their ABI parameter locations (registers and stack)
/// using the CallingConvention classification.
pub fn resolve_call_arguments(
    call_conv: &dyn CallingConvention,
    args: &[(VirtualRegister, RegisterClass)],
    outgoing_stack_base: PhysicalRegister,
) -> Result<(Vec<MachineInstruction>, i32), CodegenError> {
    let (moves, total_outgoing) = call_conv.classify_args(args, outgoing_stack_base);
    Ok((
        vec![MachineInstruction::ParallelMove { moves }],
        total_outgoing,
    ))
}

/// Convenience function for resolving integer/pointer-only function call arguments.
pub fn resolve_call_arguments_gpr(
    call_conv: &dyn CallingConvention,
    args: &[VirtualRegister],
    outgoing_stack_base: PhysicalRegister,
) -> Result<(Vec<MachineInstruction>, i32), CodegenError> {
    let typed_args: Vec<(VirtualRegister, RegisterClass)> =
        args.iter().map(|&v| (v, RegisterClass::Gpr)).collect();
    resolve_call_arguments(call_conv, &typed_args, outgoing_stack_base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine_ir::MachineOperand;

    fn phys(id: u8) -> MoveLocation {
        MoveLocation::PhysicalRegister(PhysicalRegister(id))
    }

    fn rsp_slot(off: i32) -> MoveLocation {
        MoveLocation::StackSlot {
            base: PhysicalRegister(4),
            offset: off,
        }
    }

    #[test]
    fn test_basic_moves_and_no_ops() {
        let mut resolver = ParallelMoveResolver::for_x86_64();
        resolver.add_move(phys(0), phys(0), 8); // RAX -> RAX (no-op)
        resolver.add_move(phys(1), phys(0), 8); // RAX -> RCX
        let insts = resolver.resolve().expect("resolves");
        assert_eq!(insts.len(), 1);
        assert_eq!(
            insts[0],
            MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(1))),
                src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
            }
        );
    }

    #[test]
    fn test_register_chain_dependency() {
        // RAX -> RBX, RBX -> RCX, RCX -> RDX
        // dsts: 3 (RBX), 1 (RCX), 2 (RDX)
        // srcs: 0 (RAX), 3 (RBX), 1 (RCX)
        // Expected order: RDX <- RCX, RCX <- RBX, RBX <- RAX
        let mut resolver = ParallelMoveResolver::for_x86_64();
        resolver.add_move(phys(3), phys(0), 8); // RBX <- RAX
        resolver.add_move(phys(1), phys(3), 8); // RCX <- RBX
        resolver.add_move(phys(2), phys(1), 8); // RDX <- RCX

        let insts = resolver.resolve().expect("resolves");
        assert_eq!(insts.len(), 3);
        assert_eq!(
            insts[0],
            MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(2))),
                src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(1))),
            }
        );
        assert_eq!(
            insts[1],
            MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(1))),
                src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(3))),
            }
        );
        assert_eq!(
            insts[2],
            MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(3))),
                src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
            }
        );
    }

    #[test]
    fn test_two_register_swap_cycle() {
        // RAX <-> RBX
        // RBX <- RAX, RAX <- RBX
        let mut resolver = ParallelMoveResolver::for_x86_64();
        resolver.add_move(phys(3), phys(0), 8); // RBX <- RAX
        resolver.add_move(phys(0), phys(3), 8); // RAX <- RBX

        let insts = resolver.resolve().expect("resolves");
        assert_eq!(insts.len(), 3);
        // Step 1: scratch (R10) <- RAX
        assert_eq!(
            insts[0],
            MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(10))),
                src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
            }
        );
        // Step 2: RAX <- RBX
        assert_eq!(
            insts[1],
            MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(3))),
            }
        );
        // Step 3: RBX <- scratch (R10)
        assert_eq!(
            insts[2],
            MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(3))),
                src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(10))),
            }
        );
    }

    #[test]
    fn test_three_register_cycle() {
        // RAX -> RBX, RBX -> RCX, RCX -> RAX
        // RBX (3) <- RAX (0)
        // RCX (1) <- RBX (3)
        // RAX (0) <- RCX (1)
        let mut resolver = ParallelMoveResolver::for_x86_64();
        resolver.add_move(phys(3), phys(0), 8);
        resolver.add_move(phys(1), phys(3), 8);
        resolver.add_move(phys(0), phys(1), 8);

        let insts = resolver.resolve().expect("resolves");
        assert_eq!(insts.len(), 4);
        // 1: scratch (R10) <- RAX
        assert_eq!(
            insts[0],
            MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(10))),
                src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
            }
        );
        // 2: RAX <- RCX
        assert_eq!(
            insts[1],
            MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(1))),
            }
        );
        // 3: RCX <- RBX
        assert_eq!(
            insts[2],
            MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(1))),
                src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(3))),
            }
        );
        // 4: RBX <- scratch (R10)
        assert_eq!(
            insts[3],
            MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(3))),
                src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(10))),
            }
        );
    }

    #[test]
    fn test_register_and_stack_moves() {
        // RAX -> [rsp + 16], [rsp + 16] -> RBX
        // Since dst of first move is [rsp+16] which is src of second move,
        // RBX <- [rsp+16] must execute before [rsp+16] <- RAX.
        let mut resolver = ParallelMoveResolver::for_x86_64();
        resolver.add_move(rsp_slot(16), phys(0), 8);
        resolver.add_move(phys(3), rsp_slot(16), 8);

        let insts = resolver.resolve().expect("resolves");
        assert_eq!(insts.len(), 2);
        // First: RBX <- [rsp + 16] (Load)
        assert_eq!(
            insts[0],
            MachineInstruction::Load {
                dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(3))),
                src: MachineOperand::Memory {
                    base: MachineRegister::Physical(PhysicalRegister(4)),
                    offset: 16,
                    index: None,
                },
                size: 8,
            }
        );
        // Second: [rsp + 16] <- RAX (Store)
        assert_eq!(
            insts[1],
            MachineInstruction::Store {
                dst: MachineOperand::Memory {
                    base: MachineRegister::Physical(PhysicalRegister(4)),
                    offset: 16,
                    index: None,
                },
                src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                size: 8,
            }
        );
    }

    #[test]
    fn test_stack_to_stack_cycle() {
        // [rsp + 8] <-> [rsp + 16]
        let mut resolver = ParallelMoveResolver::for_x86_64();
        resolver.add_move(rsp_slot(16), rsp_slot(8), 8);
        resolver.add_move(rsp_slot(8), rsp_slot(16), 8);

        let insts = resolver.resolve().expect("resolves");
        // Memory cycle breaking:
        // 1: scratch R10 <- [rsp + 8] (Load)
        // 2: [rsp + 8] <- [rsp + 16] (via scratch R11: Load R11, [rsp+16]; Store [rsp+8], R11)
        // 3: [rsp + 16] <- R10 (Store)
        assert_eq!(insts.len(), 4);
    }

    #[test]
    fn test_mixed_register_stack_cycle() {
        // RAX (0) -> RBX (3)
        // RBX (3) -> [rsp + 8]
        // [rsp + 8] -> RCX (1)
        // RCX (1) -> RAX (0)
        let mut resolver = ParallelMoveResolver::for_x86_64();
        resolver.add_move(phys(3), phys(0), 8);
        resolver.add_move(rsp_slot(8), phys(3), 8);
        resolver.add_move(phys(1), rsp_slot(8), 8);
        resolver.add_move(phys(0), phys(1), 8);

        let insts = resolver.resolve().expect("resolves");
        assert_eq!(insts.len(), 5);
    }

    #[test]
    fn test_conflicting_destinations_error() {
        let mut resolver = ParallelMoveResolver::for_x86_64();
        resolver.add_move(phys(1), phys(0), 8);
        resolver.add_move(phys(1), phys(2), 8); // conflict: writing RCX twice

        let res = resolver.resolve();
        assert!(res.is_err());
    }

    #[test]
    fn test_immediate_source_moves() {
        let mut resolver = ParallelMoveResolver::for_x86_64();
        resolver.add_move(phys(1), MoveLocation::imm(42), 8);
        resolver.add_move(rsp_slot(16), MoveLocation::imm(100), 8);

        let insts = resolver.resolve().expect("resolves");
        assert_eq!(insts.len(), 2);
    }
}
