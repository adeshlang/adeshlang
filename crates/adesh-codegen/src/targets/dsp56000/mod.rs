//! 24-bit Motorola DSP56000 / DSP56001 Native Codegen Backend.
//!
//! Models the real Motorola DSP56000 modified Harvard DSP architecture:
//! - Three independent 16-bit word-addressed memories (`P:` program, `X:` data,
//!   `Y:` data), each holding 64K x 24-bit words (big-endian word order).
//! - Data ALU:
//!   - Two 56-bit accumulators `A (A2:A1:A0)` and `B (B2:B1:B0)`
//!   - Four 24-bit input registers `X0, X1, Y0, Y1` (allocatable GPRs)
//!   - Note: `MPY` is fractional (`2 * S1 * S2`); integer multiply emits
//!     `MPY` followed by `ASR` and reads `A0` (low 24 bits).
//! - Address Generation Unit (AGU):
//!   - `R0..R7` (16-bit pointers), `N0..N7` (16-bit offsets), `M0..M7` (`$FFFF` linear)
//!   - `R6` (id `14`) = frame pointer in `X:` space
//!   - `R7` (id `15`) = upward-growing software stack pointer in `X:` space
//!   - `R4, R5` (`10, 11`) + `N4, N6` = reserved AGU scratch registers
//! - Program Control Unit (PCU):
//!   - 15-level hardware call stack `SSH:SSL` used by `JSR`/`RTS`.
//!   - Zero-overhead hardware `REP` loops for multi-bit shifts.

#![forbid(unsafe_code)]

use crate::backend::CodegenBackend;
use crate::calling_convention::ParallelMoveResolver;
use crate::error::CodegenError;
use crate::legalize::{PairLegalizeProfile, legalize_32bit_with_profile};
use crate::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, RegisterClass,
};
use crate::register_alloc::{LinearScanAllocator, LivenessAnalysis, RegisterFile};
use crate::targets::{unsupported_instruction, unsupported_operand_form};
use adesh_object::{
    AdobObject, AdobRelocation, AdobSection, AdobSymbol, MemoryPermissions, MemoryRegion,
    RelocationKind, SectionKind, SymbolBinding, SymbolKind, SymbolVisibility, TargetCapabilities,
    TargetDescriptor, section_flags,
};
use std::collections::HashMap;

// Physical register IDs matching `Dsp56000Abi` & `Dsp56000CallingConvention`:
const PHYS_A1: u8 = 0;
const PHYS_B1: u8 = 1;
const PHYS_X0: u8 = 2;
const PHYS_X1: u8 = 3;
const PHYS_Y0: u8 = 4;
const PHYS_Y1: u8 = 5;
const PHYS_R4: u8 = 10;
const PHYS_R5: u8 = 11;
const PHYS_R6: u8 = 14; // Frame pointer
const PHYS_R7: u8 = 15; // Software stack pointer

// Motorola DSP56000 5-bit register encodings:
const REG5_X0: u8 = 0x04;
const REG5_X1: u8 = 0x05;
const REG5_Y0: u8 = 0x06;
const REG5_Y1: u8 = 0x07;
const REG5_A0: u8 = 0x08;
const REG5_A1: u8 = 0x0C;
const REG5_B1: u8 = 0x0D;
const REG5_A: u8 = 0x0E;
const REG5_B: u8 = 0x0F;
const REG5_R4: u8 = 0x14;
const REG5_R5: u8 = 0x15;
const REG5_R6: u8 = 0x16;
const REG5_R7: u8 = 0x17;
const REG5_N4: u8 = 0x1C;
const REG5_N6: u8 = 0x1E;
const REG5_N7: u8 = 0x1F;

static DSP56K_REGS: [PhysicalRegister; 10] = [
    PhysicalRegister::gpr(PHYS_A1),
    PhysicalRegister::gpr(PHYS_B1),
    PhysicalRegister::gpr(PHYS_X0),
    PhysicalRegister::gpr(PHYS_X1),
    PhysicalRegister::gpr(PHYS_Y0),
    PhysicalRegister::gpr(PHYS_Y1),
    PhysicalRegister::gpr(PHYS_R4),
    PhysicalRegister::gpr(PHYS_R5),
    PhysicalRegister::gpr(PHYS_R6),
    PhysicalRegister::gpr(PHYS_R7),
];

static DSP56K_ALLOCATABLE: [PhysicalRegister; 3] = [
    PhysicalRegister::gpr(PHYS_X0),
    PhysicalRegister::gpr(PHYS_X1),
    PhysicalRegister::gpr(PHYS_Y0),
];

static DSP56K_CALLER_SAVED: [PhysicalRegister; 6] = [
    PhysicalRegister::gpr(PHYS_A1),
    PhysicalRegister::gpr(PHYS_B1),
    PhysicalRegister::gpr(PHYS_X0),
    PhysicalRegister::gpr(PHYS_X1),
    PhysicalRegister::gpr(PHYS_Y0),
    PhysicalRegister::gpr(PHYS_Y1),
];

static DSP56K_CALLEE_SAVED: [PhysicalRegister; 2] = [
    PhysicalRegister::gpr(PHYS_R6),
    PhysicalRegister::gpr(PHYS_R7),
];

static DSP56K_RESERVED: [PhysicalRegister; 7] = [
    PhysicalRegister::gpr(PHYS_A1),
    PhysicalRegister::gpr(PHYS_B1),
    PhysicalRegister::gpr(PHYS_Y1),
    PhysicalRegister::gpr(PHYS_R4),
    PhysicalRegister::gpr(PHYS_R5),
    PhysicalRegister::gpr(PHYS_R6),
    PhysicalRegister::gpr(PHYS_R7),
];

struct Dsp56000RegisterFile;

impl RegisterFile for Dsp56000RegisterFile {
    fn registers(&self) -> &[PhysicalRegister] {
        &DSP56K_REGS
    }
    fn allocatable(&self) -> &[PhysicalRegister] {
        &DSP56K_ALLOCATABLE
    }
    fn caller_saved(&self) -> &[PhysicalRegister] {
        &DSP56K_CALLER_SAVED
    }
    fn callee_saved(&self) -> &[PhysicalRegister] {
        &DSP56K_CALLEE_SAVED
    }
    fn reserved(&self) -> &[PhysicalRegister] {
        &DSP56K_RESERVED
    }
    fn scratch_for_class(&self, _class: RegisterClass) -> (PhysicalRegister, PhysicalRegister) {
        (
            PhysicalRegister::gpr(PHYS_A1),
            PhysicalRegister::gpr(PHYS_B1),
        )
    }
    fn frame_pointer(&self) -> PhysicalRegister {
        PhysicalRegister::gpr(PHYS_R6)
    }
    fn scalar_slot_size(&self) -> u32 {
        1
    }
}

fn phys_to_reg5(id: u8) -> Option<u8> {
    match id {
        PHYS_A1 => Some(REG5_A1),
        PHYS_B1 => Some(REG5_B1),
        PHYS_X0 => Some(REG5_X0),
        PHYS_X1 => Some(REG5_X1),
        PHYS_Y0 => Some(REG5_Y0),
        PHYS_Y1 => Some(REG5_Y1),
        PHYS_R4 => Some(REG5_R4),
        PHYS_R5 => Some(REG5_R5),
        PHYS_R6 => Some(REG5_R6),
        PHYS_R7 => Some(REG5_R7),
        _ => None,
    }
}

// ============================================================================
// Motorola DSP56000 24-bit Big-Endian Instruction Encoder
// ============================================================================

#[inline]
fn emit_word24(out: &mut Vec<u8>, word: u32) {
    let w = word & 0x00FF_FFFF;
    out.push(((w >> 16) & 0xFF) as u8);
    out.push(((w >> 8) & 0xFF) as u8);
    out.push((w & 0xFF) as u8);
}

#[inline]
fn enc_nop(out: &mut Vec<u8>) {
    emit_word24(out, 0x000000);
}

#[inline]
fn enc_rts(out: &mut Vec<u8>) {
    emit_word24(out, 0x00000C);
}

/// `MOVE S, D` (5-bit register to 5-bit register).
#[inline]
fn enc_move_reg(out: &mut Vec<u8>, s5: u8, d5: u8) {
    if s5 != d5 {
        emit_word24(
            out,
            0x200000 | (((s5 as u32) & 0x1F) << 13) | (((d5 as u32) & 0x1F) << 8),
        );
    }
}

/// `MOVE #imm8, D` (1-word unsigned 8-bit immediate) or `MOVE #imm24, D` (2-word 24-bit immediate).
fn enc_move_imm24(out: &mut Vec<u8>, d5: u8, imm: i64) {
    let u24 = (imm as u32) & 0x00FF_FFFF;
    if u24 <= 0xFF && d5 >= 4 {
        emit_word24(
            out,
            0x200000 | (((d5 as u32) & 0x1F) << 16) | ((u24 & 0xFF) << 8),
        );
    } else {
        let hi2 = ((d5 >> 3) & 0x03) as u32;
        let lo3 = (d5 & 0x07) as u32;
        emit_word24(out, 0x40F400 | (hi2 << 20) | (lo3 << 16));
        emit_word24(out, u24);
    }
}

/// Force a 2-word `MOVE #imm24, D` so the extension word can be relocated by the linker.
fn enc_move_imm24_reloc(out: &mut Vec<u8>, d5: u8, val24: u32) -> usize {
    let hi2 = ((d5 >> 3) & 0x03) as u32;
    let lo3 = (d5 & 0x07) as u32;
    emit_word24(out, 0x40F400 | (hi2 << 20) | (lo3 << 16));
    let ext_off = out.len();
    emit_word24(out, val24 & 0x00FF_FFFF);
    ext_off
}

/// `MOVE X:ea, D` (`read=true`) or `MOVE S, X:ea` (`read=false`) using 6-bit `MMMRRR`.
fn enc_move_x_ea(out: &mut Vec<u8>, reg5: u8, read: bool, mmmrrr: u8) {
    let hi2 = ((reg5 >> 3) & 0x03) as u32;
    let lo3 = (reg5 & 0x07) as u32;
    let w_bit = if read { 0x008000 } else { 0 };
    emit_word24(
        out,
        0x404000 | (hi2 << 20) | (lo3 << 16) | w_bit | (((mmmrrr as u32) & 0x3F) << 8),
    );
}

#[inline]
fn ea_ind(rn: u8) -> u8 {
    0x20 | (rn & 0x07)
}
#[inline]
fn ea_post_inc(rn: u8) -> u8 {
    0x18 | (rn & 0x07)
}
#[inline]
fn ea_pre_dec(rn: u8) -> u8 {
    0x38 | (rn & 0x07)
}
#[inline]
fn ea_indexed_nn(rn: u8) -> u8 {
    0x28 | (rn & 0x07)
}

/// Load/Store `reg5` from/to `X:(R6 + off)`. Inserts a `NOP` after loading `N6`
/// to respect the DSP56000 AGU 1-cycle pipeline hazard.
fn emit_x_r6_rel(out: &mut Vec<u8>, reg5: u8, off: i32, read: bool) {
    if off == 0 {
        enc_move_x_ea(out, reg5, read, ea_ind(6));
    } else {
        enc_move_imm24(out, REG5_N6, off as i64);
        enc_nop(out);
        enc_move_x_ea(out, reg5, read, ea_indexed_nn(6));
    }
}

/// Compute `R4 <- base_r + off` (where `base_r` is `R6` or `R7`), inserting a `NOP`
/// so subsequent `(R4)` accesses avoid the AGU pipeline hazard.
fn emit_compute_addr_r4(out: &mut Vec<u8>, base_phys: u8, off: i32) -> Result<(), CodegenError> {
    let b5 = phys_to_reg5(base_phys).ok_or_else(|| {
        CodegenError::new(
            "dsp56000",
            format!("invalid base AGU register id {base_phys}"),
        )
    })?;
    if off == 0 {
        enc_move_reg(out, b5, REG5_R4);
        enc_nop(out);
    } else {
        // Compute `R4 = base + off` via AGU `(R4)+N4` dummy read into reserved `R5`,
        // preserving all Data ALU registers (A, B, X0, X1, Y0, Y1).
        enc_move_reg(out, b5, REG5_R4);
        enc_move_imm24(out, REG5_N4, off as i64);
        enc_nop(out);
        enc_move_x_ea(out, REG5_R5, true, 0x10 | 4);
        enc_nop(out);
    }
    Ok(())
}

#[inline]
fn enc_rep_imm(out: &mut Vec<u8>, count: u8) {
    emit_word24(out, 0x0600A0 | ((count as u32) << 8));
}

#[inline]
fn enc_jmp_abs16_placeholder(out: &mut Vec<u8>) -> usize {
    emit_word24(out, 0x0AF080);
    let ext_off = out.len();
    emit_word24(out, 0x000000);
    ext_off
}

#[inline]
fn enc_jcc_abs16_placeholder(out: &mut Vec<u8>, cc: u8) -> usize {
    emit_word24(out, 0x0AF0A0 | ((cc as u32) & 0x0F));
    let ext_off = out.len();
    emit_word24(out, 0x000000);
    ext_off
}

#[inline]
fn enc_jsr_abs16_placeholder(out: &mut Vec<u8>) -> usize {
    emit_word24(out, 0x0BF080);
    let ext_off = out.len();
    emit_word24(out, 0x000000);
    ext_off
}

fn alu_jjj_for_reg5(reg5: u8) -> Option<u8> {
    match reg5 {
        REG5_X0 => Some(0x40),
        REG5_Y0 => Some(0x50),
        REG5_X1 => Some(0x60),
        REG5_Y1 => Some(0x70),
        REG5_B | REG5_B1 => Some(0x10),
        _ => None,
    }
}

fn emit_alu_a_op(out: &mut Vec<u8>, base_low_nibble: u8, s_reg5: u8) {
    if let Some(jjj) = alu_jjj_for_reg5(s_reg5) {
        if jjj == 0x10 && matches!(base_low_nibble, 0x02 | 0x03 | 0x06) {
            enc_move_reg(out, REG5_B1, REG5_Y1);
            emit_word24(out, 0x200000 | (0x70 | (base_low_nibble as u32)));
        } else {
            emit_word24(out, 0x200000 | ((jjj | base_low_nibble) as u32));
        }
    } else {
        enc_move_reg(out, s_reg5, REG5_Y1);
        emit_word24(out, 0x200000 | (0x70 | (base_low_nibble as u32)));
    }
}

#[inline]
fn enc_asr_a(out: &mut Vec<u8>) {
    emit_word24(out, 0x200022);
}
#[inline]
fn enc_lsr_a(out: &mut Vec<u8>) {
    emit_word24(out, 0x200023);
}
#[inline]
fn enc_asl_a(out: &mut Vec<u8>) {
    emit_word24(out, 0x200032);
}
#[inline]
fn enc_not_a(out: &mut Vec<u8>) {
    emit_word24(out, 0x200017);
}
#[inline]
fn enc_neg_a(out: &mut Vec<u8>) {
    emit_word24(out, 0x200036);
}
#[inline]
fn enc_clr_b(out: &mut Vec<u8>) {
    emit_word24(out, 0x20001B);
}
#[inline]
fn enc_mpy_x0_y0_a(out: &mut Vec<u8>) {
    emit_word24(out, 0x2000A0);
}

fn dsp56k_cc(cc: ConditionCode) -> Option<u8> {
    match cc {
        ConditionCode::Equal | ConditionCode::Zero => Some(0x0A), // EQ
        ConditionCode::NotEqual | ConditionCode::NotZero => Some(0x02), // NE
        ConditionCode::LessThan => Some(0x09),                    // LT
        ConditionCode::LessOrEqual => Some(0x0F),                 // LE
        ConditionCode::GreaterThan => Some(0x07),                 // GT
        ConditionCode::GreaterOrEqual => Some(0x01),              // GE
        ConditionCode::Below => Some(0x08),                       // CS / LO
        ConditionCode::BelowOrEqual => Some(0x0E),                // LS
        ConditionCode::AboveOrEqual => Some(0x00),                // CC / HS
        ConditionCode::Above | ConditionCode::Parity | ConditionCode::NotParity => None,
    }
}

fn emit_mask16_a(out: &mut Vec<u8>) {
    enc_move_imm24(out, REG5_Y1, 0x00FFFF);
    emit_alu_a_op(out, 0x06, REG5_Y1); // AND Y1, A
}

fn emit_lsr16_a(out: &mut Vec<u8>) {
    enc_rep_imm(out, 16);
    enc_lsr_a(out);
}

/// Mask both `d5` and `s5` to 16 bits and leave `(d & 0xFFFF) + (s & 0xFFFF)` in `A`.
fn emit_mask16_a_pair(out: &mut Vec<u8>, d5: u8, s5: u8) {
    enc_move_reg(out, s5, REG5_A);
    emit_mask16_a(out);
    enc_move_reg(out, REG5_A1, REG5_B1);
    enc_move_reg(out, d5, REG5_A);
    emit_mask16_a(out);
    emit_alu_a_op(out, 0x00, REG5_B1);
}

pub struct Dsp56000Backend {
    target: TargetDescriptor,
    capabilities: TargetCapabilities,
}

impl Dsp56000Backend {
    pub fn new(target: TargetDescriptor) -> Self {
        let capabilities = TargetCapabilities::for_architecture(&target.architecture);
        Self {
            target,
            capabilities,
        }
    }

    fn lower_function_internal(
        &self,
        func: &MachineFunction,
    ) -> Result<MachineFunction, CodegenError> {
        for block in &func.blocks {
            for inst in &block.instructions {
                if matches!(
                    inst,
                    MachineInstruction::FAdd { .. }
                        | MachineInstruction::FSub { .. }
                        | MachineInstruction::FMul { .. }
                        | MachineInstruction::FDiv { .. }
                        | MachineInstruction::FNeg { .. }
                        | MachineInstruction::FCmp { .. }
                        | MachineInstruction::FCvtIntToFloat { .. }
                        | MachineInstruction::FCvtFloatToInt { .. }
                        | MachineInstruction::FCvtFloatToFloat { .. }
                ) {
                    return Err(CodegenError::new(
                        "dsp56000",
                        format!(
                            "Motorola DSP56000 (dsp56000-unknown-unknown) has no hardware floating-point unit; refused `{:?}` in `{}`",
                            inst, func.name
                        ),
                    )
                    .with_arch("dsp56000")
                    .with_function(func.name.clone())
                    .with_suggestion("Use 24-bit/16-bit/32-bit fixed-point or integer arithmetic on DSP56000."));
                }
            }
        }

        let mut allocated = func.clone();
        let profile = PairLegalizeProfile::dsp56000();
        legalize_32bit_with_profile(&mut allocated, &profile, "dsp56000")?;

        let _alloc_result =
            LinearScanAllocator::with_local_call_splitting(&Dsp56000RegisterFile, false)
                .allocate(&mut allocated)
                .map_err(|e| {
                    CodegenError::new(
                        self.target.triple_string(),
                        format!(
                            "DSP56000 register allocation failed in `{}`: {e}",
                            allocated.name
                        ),
                    )
                    .with_arch("dsp56000")
                    .with_function(allocated.name.clone())
                })?;

        let resolver = ParallelMoveResolver::for_dsp56000();
        let liveness = LivenessAnalysis::compute(&mut allocated);
        let mut inst_index = 0usize;
        for block in &mut allocated.blocks {
            let mut resolved = Vec::with_capacity(block.instructions.len());
            for inst in std::mem::take(&mut block.instructions) {
                match inst {
                    MachineInstruction::ParallelMove { moves } => {
                        let mut stack_out_moves = Vec::new();
                        let mut other_moves = Vec::new();
                        for mut m in moves {
                            if let crate::calling_convention::MoveLocation::StackSlot {
                                base,
                                ref mut offset,
                            } = m.dst
                                && base.0 == PHYS_R6
                                && *offset < 0
                            {
                                *offset = -*offset - 1;
                            }
                            if let crate::calling_convention::MoveLocation::StackSlot {
                                base,
                                ref mut offset,
                            } = m.src
                                && base.0 == PHYS_R6
                                && *offset < 0
                            {
                                *offset = -*offset - 1;
                            }
                            if matches!(
                                m.dst,
                                crate::calling_convention::MoveLocation::StackSlot { base, .. }
                                    if base.0 == PHYS_R7
                            ) {
                                stack_out_moves.push(m);
                            } else {
                                other_moves.push(m);
                            }
                        }
                        let mut seq = Vec::new();
                        let live: std::collections::HashSet<PhysicalRegister> = liveness
                            .live_after_inst(inst_index)
                            .into_iter()
                            .flatten()
                            .filter_map(|reg| match reg {
                                MachineRegister::Physical(p) => Some(*p),
                                _ => None,
                            })
                            .collect();
                        if !stack_out_moves.is_empty() {
                            seq.extend(
                                resolver
                                    .clone()
                                    .with_live_registers(live.clone())
                                    .resolve_moves(&stack_out_moves)?,
                            );
                        }
                        if !other_moves.is_empty() {
                            seq.extend(
                                resolver
                                    .clone()
                                    .with_live_registers(live)
                                    .resolve_moves(&other_moves)?,
                            );
                        }
                        resolved.extend(seq);
                    }
                    other => resolved.push(other),
                }
                inst_index += 1;
            }
            block.instructions = resolved;
        }
        Ok(allocated)
    }

    fn emit_function_with_relocs(
        &self,
        func: &MachineFunction,
    ) -> Result<(Vec<u8>, Vec<AdobRelocation>), CodegenError> {
        let allocated = self.lower_function_internal(func)?;

        // Compute disjoint local (`>= 0`) and spill (`< 0` from allocator) word counts on `R6`.
        let mut min_neg_off: i32 = -(allocated.stack_size as i32);
        let mut max_pos_end: i32 = 0;
        let mut scan_op = |op: &MachineOperand| {
            match op {
                MachineOperand::StackSlot(off) => {
                    if *off < 0 {
                        min_neg_off = min_neg_off.min(*off);
                    } else {
                        max_pos_end = max_pos_end.max(*off + 1);
                    }
                }
                MachineOperand::Memory {
                    base: MachineRegister::Physical(p),
                    offset,
                    index: None,
                } if p.0 == PHYS_R6 && *offset >= 0 => {
                    max_pos_end = max_pos_end.max(*offset + 1);
                }
                _ => {}
            }
        };
        for block in &allocated.blocks {
            for inst in &block.instructions {
                match inst {
                    MachineInstruction::Move { dst, src }
                    | MachineInstruction::Load { dst, src, .. }
                    | MachineInstruction::Store { dst, src, .. }
                    | MachineInstruction::Add { dst, src }
                    | MachineInstruction::AddWithCarry { dst, src }
                    | MachineInstruction::Sub { dst, src }
                    | MachineInstruction::SubWithBorrow { dst, src }
                    | MachineInstruction::Mul { dst, src }
                    | MachineInstruction::Div { dst, src }
                    | MachineInstruction::Mod { dst, src }
                    | MachineInstruction::And { dst, src }
                    | MachineInstruction::Or { dst, src }
                    | MachineInstruction::Xor { dst, src }
                    | MachineInstruction::Shl { dst, src }
                    | MachineInstruction::Shr { dst, src }
                    | MachineInstruction::Sar { dst, src } => {
                        scan_op(dst);
                        scan_op(src);
                    }
                    MachineInstruction::Compare { lhs, rhs }
                    | MachineInstruction::CompareWithBorrow { lhs, rhs }
                    | MachineInstruction::Test { lhs, rhs } => {
                        scan_op(lhs);
                        scan_op(rhs);
                    }
                    MachineInstruction::Neg { dst }
                    | MachineInstruction::Not { dst }
                    | MachineInstruction::SetCc { dst, .. }
                    | MachineInstruction::Pop { dst } => scan_op(dst),
                    MachineInstruction::Push { src } => scan_op(src),
                    MachineInstruction::Custom { operands, .. } => {
                        for op in operands {
                            scan_op(op);
                        }
                    }
                    _ => {}
                }
            }
        }
        let frame_words = (-min_neg_off).max(max_pos_end).max(0);

        let resolve_r6_off = |op: &MachineOperand| -> Option<i32> {
            match op {
                MachineOperand::StackSlot(off) if *off < 0 => Some(-*off - 1),
                MachineOperand::StackSlot(off) => Some(*off),
                MachineOperand::Memory {
                    base: MachineRegister::Physical(p),
                    offset,
                    index: None,
                } if p.0 == PHYS_R6 => Some(*offset),
                _ => None,
            }
        };

        let mut out = Vec::<u8>::new();
        let mut relocations = Vec::<AdobRelocation>::new();

        let emit_adjust_r7 = |out: &mut Vec<u8>, delta: i64| {
            // Advance R7 by `delta` via `MOVE #delta, N7; NOP; MOVE X:(R7)+N7, R4`
            // so Data ALU registers (A1, B1, X0, X1, Y0, Y1) are never clobbered.
            enc_move_imm24(out, REG5_N7, delta);
            enc_nop(out);
            enc_move_x_ea(out, REG5_R4, true, 0x10 | 7);
        };

        // Prologue:
        //   MOVE R6, X:(R7)+
        //   MOVE R7, R6
        //   R7 += frame_words (if > 0)
        enc_move_x_ea(&mut out, REG5_R6, false, ea_post_inc(7));
        enc_move_reg(&mut out, REG5_R7, REG5_R6);
        if frame_words > 0 {
            emit_adjust_r7(&mut out, frame_words as i64);
        }

        let mut label_word_offsets: HashMap<String, usize> = HashMap::new();
        struct PendingBranch {
            ext_byte_off: usize,
            label: String,
        }
        let mut pending_branches: Vec<PendingBranch> = Vec::new();

        let load_op_to_reg5 = |out: &mut Vec<u8>,
                               relocs: &mut Vec<AdobRelocation>,
                               op: &MachineOperand,
                               dst_reg5: u8|
         -> Result<(), CodegenError> {
            match op {
                MachineOperand::Register(MachineRegister::Physical(p)) => {
                    let s5 = phys_to_reg5(p.0).ok_or_else(|| {
                        CodegenError::new(
                            "dsp56000",
                            format!(
                                "unsupported physical register id {} in `{}`",
                                p.0, allocated.name
                            ),
                        )
                    })?;
                    enc_move_reg(out, s5, dst_reg5);
                    Ok(())
                }
                MachineOperand::Immediate(imm) => {
                    enc_move_imm24(out, dst_reg5, *imm);
                    Ok(())
                }
                MachineOperand::Symbol(sym) => {
                    let ext_off = enc_move_imm24_reloc(out, dst_reg5, 0);
                    relocs.push(AdobRelocation::new(
                        ext_off as u64,
                        0,
                        sym.clone(),
                        RelocationKind::Dsp56k_Abs16,
                        0,
                    ));
                    Ok(())
                }
                _ if resolve_r6_off(op).is_some() => {
                    let off = resolve_r6_off(op).expect("checked");
                    emit_x_r6_rel(out, dst_reg5, off, true);
                    Ok(())
                }
                MachineOperand::Memory {
                    base: MachineRegister::Physical(p),
                    offset,
                    index: None,
                } => {
                    emit_compute_addr_r4(out, p.0, *offset)?;
                    enc_move_x_ea(out, dst_reg5, true, ea_ind(4));
                    Ok(())
                }
                _ => Err(CodegenError::new(
                    "dsp56000",
                    format!(
                        "unsupported DSP56000 source operand `{op:?}` in `{}`",
                        allocated.name
                    ),
                )),
            }
        };

        for block in &allocated.blocks {
            label_word_offsets.insert(block.label.clone(), out.len() / 3);

            for inst in &block.instructions {
                match inst {
                    MachineInstruction::Nop | MachineInstruction::Barrier => enc_nop(&mut out),

                    MachineInstruction::Move { dst, src } => {
                        if let Some(d_off) = resolve_r6_off(dst) {
                            if let MachineOperand::Register(MachineRegister::Physical(sp)) = src
                                && let Some(s5) = phys_to_reg5(sp.0)
                            {
                                emit_x_r6_rel(&mut out, s5, d_off, false);
                            } else {
                                load_op_to_reg5(&mut out, &mut relocations, src, REG5_A1)?;
                                emit_x_r6_rel(&mut out, REG5_A1, d_off, false);
                            }
                        } else if let MachineOperand::Register(MachineRegister::Physical(dp)) = dst
                        {
                            let d5 = phys_to_reg5(dp.0).ok_or_else(|| {
                                unsupported_operand_form("dsp56000", &allocated, inst)
                            })?;
                            load_op_to_reg5(&mut out, &mut relocations, src, d5)?;
                        } else if let MachineOperand::Memory {
                            base: MachineRegister::Physical(p),
                            offset,
                            index: None,
                        } = dst
                        {
                            let s5 = match src {
                                MachineOperand::Register(MachineRegister::Physical(sp)) => {
                                    phys_to_reg5(sp.0).ok_or_else(|| {
                                        unsupported_operand_form("dsp56000", &allocated, inst)
                                    })?
                                }
                                _ => {
                                    load_op_to_reg5(&mut out, &mut relocations, src, REG5_A1)?;
                                    REG5_A1
                                }
                            };
                            let safe_s5 = if s5 == REG5_A1 || s5 == REG5_A || s5 == REG5_Y1 {
                                enc_move_reg(&mut out, s5, REG5_B1);
                                REG5_B1
                            } else {
                                s5
                            };
                            emit_compute_addr_r4(&mut out, p.0, *offset)?;
                            enc_move_x_ea(&mut out, safe_s5, false, ea_ind(4));
                        } else {
                            return Err(unsupported_operand_form("dsp56000", &allocated, inst));
                        }
                    }

                    MachineInstruction::Load { dst, src, size } => {
                        if !matches!(size, 1..=3) {
                            return Err(CodegenError::new(
                                "dsp56000",
                                format!(
                                    "DSP56000 Load size {size} in `{}` is unsupported (expected 1..=3 words/bytes for <=24-bit scalar; 32-bit must be legalized)",
                                    allocated.name
                                ),
                            ));
                        }
                        let d5 = match dst {
                            MachineOperand::Register(MachineRegister::Physical(p)) => {
                                phys_to_reg5(p.0).ok_or_else(|| {
                                    unsupported_operand_form("dsp56000", &allocated, inst)
                                })?
                            }
                            _ => REG5_A1,
                        };
                        match src {
                            _ if resolve_r6_off(src).is_some() => {
                                emit_x_r6_rel(&mut out, d5, resolve_r6_off(src).unwrap(), true);
                            }
                            MachineOperand::Memory {
                                base: MachineRegister::Physical(p),
                                offset,
                                index: None,
                            } => {
                                emit_compute_addr_r4(&mut out, p.0, *offset)?;
                                enc_move_x_ea(&mut out, d5, true, ea_ind(4));
                            }
                            MachineOperand::Symbol(sym) => {
                                let ext_off = enc_move_imm24_reloc(&mut out, REG5_R4, 0);
                                relocations.push(AdobRelocation::new(
                                    ext_off as u64,
                                    0,
                                    sym.clone(),
                                    RelocationKind::Dsp56k_Abs16,
                                    0,
                                ));
                                enc_nop(&mut out);
                                enc_move_x_ea(&mut out, d5, true, ea_ind(4));
                            }
                            _ => {
                                return Err(unsupported_operand_form("dsp56000", &allocated, inst));
                            }
                        }
                        if let Some(off) = resolve_r6_off(dst) {
                            emit_x_r6_rel(&mut out, d5, off, false);
                        }
                    }

                    MachineInstruction::Store { dst, src, size } => {
                        if !matches!(size, 1..=3) {
                            return Err(CodegenError::new(
                                "dsp56000",
                                format!(
                                    "DSP56000 Store size {size} in `{}` is unsupported (expected 1..=3)",
                                    allocated.name
                                ),
                            ));
                        }
                        let s5 = match src {
                            MachineOperand::Register(MachineRegister::Physical(p)) => {
                                phys_to_reg5(p.0).ok_or_else(|| {
                                    unsupported_operand_form("dsp56000", &allocated, inst)
                                })?
                            }
                            _ => {
                                load_op_to_reg5(&mut out, &mut relocations, src, REG5_A1)?;
                                REG5_A1
                            }
                        };
                        match dst {
                            _ if resolve_r6_off(dst).is_some() => {
                                emit_x_r6_rel(&mut out, s5, resolve_r6_off(dst).unwrap(), false);
                            }
                            MachineOperand::Memory {
                                base: MachineRegister::Physical(p),
                                offset,
                                index: None,
                            } => {
                                let safe_s5 = if s5 == REG5_A1 || s5 == REG5_A || s5 == REG5_Y1 {
                                    enc_move_reg(&mut out, s5, REG5_B1);
                                    REG5_B1
                                } else {
                                    s5
                                };
                                emit_compute_addr_r4(&mut out, p.0, *offset)?;
                                enc_move_x_ea(&mut out, safe_s5, false, ea_ind(4));
                            }
                            MachineOperand::Symbol(sym) => {
                                let ext_off = enc_move_imm24_reloc(&mut out, REG5_R4, 0);
                                relocations.push(AdobRelocation::new(
                                    ext_off as u64,
                                    0,
                                    sym.clone(),
                                    RelocationKind::Dsp56k_Abs16,
                                    0,
                                ));
                                enc_nop(&mut out);
                                enc_move_x_ea(&mut out, s5, false, ea_ind(4));
                            }
                            _ => {
                                return Err(unsupported_operand_form("dsp56000", &allocated, inst));
                            }
                        }
                    }

                    MachineInstruction::Add { dst, src } => {
                        let MachineOperand::Register(MachineRegister::Physical(dp)) = dst else {
                            return Err(unsupported_operand_form("dsp56000", &allocated, inst));
                        };
                        let d5 = phys_to_reg5(dp.0).ok_or_else(|| {
                            unsupported_operand_form("dsp56000", &allocated, inst)
                        })?;
                        if dp.0 == PHYS_R7 {
                            let delta = match src {
                                MachineOperand::Immediate(v) => *v,
                                _ => {
                                    return Err(unsupported_operand_form(
                                        "dsp56000", &allocated, inst,
                                    ));
                                }
                            };
                            emit_adjust_r7(&mut out, delta);
                            continue;
                        }
                        load_op_to_reg5(&mut out, &mut relocations, src, REG5_Y1)?;
                        emit_mask16_a_pair(&mut out, d5, REG5_Y1);
                        enc_move_reg(&mut out, REG5_A1, d5);
                        emit_lsr16_a(&mut out);
                        enc_move_reg(&mut out, REG5_A1, REG5_N4);
                    }

                    MachineInstruction::AddWithCarry { dst, src } => {
                        let MachineOperand::Register(MachineRegister::Physical(dp)) = dst else {
                            return Err(unsupported_operand_form("dsp56000", &allocated, inst));
                        };
                        let d5 = phys_to_reg5(dp.0).ok_or_else(|| {
                            unsupported_operand_form("dsp56000", &allocated, inst)
                        })?;
                        load_op_to_reg5(&mut out, &mut relocations, src, REG5_Y1)?;
                        enc_move_reg(&mut out, d5, REG5_A);
                        emit_alu_a_op(&mut out, 0x00, REG5_Y1);
                        enc_move_reg(&mut out, REG5_N4, REG5_Y1);
                        emit_alu_a_op(&mut out, 0x00, REG5_Y1);
                        emit_mask16_a(&mut out);
                        enc_move_reg(&mut out, REG5_A1, d5);
                    }

                    MachineInstruction::Sub { dst, src } => {
                        let MachineOperand::Register(MachineRegister::Physical(dp)) = dst else {
                            return Err(unsupported_operand_form("dsp56000", &allocated, inst));
                        };
                        let d5 = phys_to_reg5(dp.0).ok_or_else(|| {
                            unsupported_operand_form("dsp56000", &allocated, inst)
                        })?;
                        if dp.0 == PHYS_R7 {
                            let delta = match src {
                                MachineOperand::Immediate(v) => *v,
                                _ => {
                                    return Err(unsupported_operand_form(
                                        "dsp56000", &allocated, inst,
                                    ));
                                }
                            };
                            emit_adjust_r7(&mut out, -delta);
                            continue;
                        }
                        load_op_to_reg5(&mut out, &mut relocations, src, REG5_Y1)?;
                        enc_move_reg(&mut out, REG5_Y1, REG5_A);
                        emit_mask16_a(&mut out);
                        enc_move_reg(&mut out, REG5_A1, REG5_B1);
                        enc_move_reg(&mut out, d5, REG5_A);
                        emit_mask16_a(&mut out);
                        emit_alu_a_op(&mut out, 0x04, REG5_B1); // SUB B1, A (preserves Y1=0xFFFF? no, B1 holds src)
                        enc_move_reg(&mut out, REG5_A1, REG5_B1);
                        emit_mask16_a(&mut out);
                        enc_move_reg(&mut out, REG5_A1, d5);
                        enc_move_reg(&mut out, REG5_B1, REG5_A);
                        emit_lsr16_a(&mut out);
                        enc_move_imm24(&mut out, REG5_Y1, 1);
                        emit_alu_a_op(&mut out, 0x06, REG5_Y1);
                        enc_move_reg(&mut out, REG5_A1, REG5_N4);
                    }

                    MachineInstruction::SubWithBorrow { dst, src } => {
                        let MachineOperand::Register(MachineRegister::Physical(dp)) = dst else {
                            return Err(unsupported_operand_form("dsp56000", &allocated, inst));
                        };
                        let d5 = phys_to_reg5(dp.0).ok_or_else(|| {
                            unsupported_operand_form("dsp56000", &allocated, inst)
                        })?;
                        load_op_to_reg5(&mut out, &mut relocations, src, REG5_Y1)?;
                        enc_move_reg(&mut out, d5, REG5_A);
                        emit_alu_a_op(&mut out, 0x04, REG5_Y1);
                        enc_move_reg(&mut out, REG5_N4, REG5_Y1);
                        emit_alu_a_op(&mut out, 0x04, REG5_Y1);
                        emit_mask16_a(&mut out);
                        enc_move_reg(&mut out, REG5_A1, d5);
                    }

                    MachineInstruction::And { dst, src }
                    | MachineInstruction::Or { dst, src }
                    | MachineInstruction::Xor { dst, src } => {
                        let MachineOperand::Register(MachineRegister::Physical(dp)) = dst else {
                            return Err(unsupported_operand_form("dsp56000", &allocated, inst));
                        };
                        let d5 = phys_to_reg5(dp.0).ok_or_else(|| {
                            unsupported_operand_form("dsp56000", &allocated, inst)
                        })?;
                        load_op_to_reg5(&mut out, &mut relocations, src, REG5_Y1)?;
                        enc_move_reg(&mut out, d5, REG5_A);
                        let op_code = match inst {
                            MachineInstruction::And { .. } => 0x06,
                            MachineInstruction::Or { .. } => 0x02,
                            MachineInstruction::Xor { .. } => 0x03,
                            _ => unreachable!(),
                        };
                        emit_alu_a_op(&mut out, op_code, REG5_Y1);
                        enc_move_reg(&mut out, REG5_A1, d5);
                    }

                    MachineInstruction::Not { dst } => {
                        let MachineOperand::Register(MachineRegister::Physical(dp)) = dst else {
                            return Err(unsupported_operand_form("dsp56000", &allocated, inst));
                        };
                        let d5 = phys_to_reg5(dp.0).ok_or_else(|| {
                            unsupported_operand_form("dsp56000", &allocated, inst)
                        })?;
                        enc_move_reg(&mut out, d5, REG5_A);
                        enc_not_a(&mut out);
                        enc_move_reg(&mut out, REG5_A1, d5);
                    }

                    MachineInstruction::Neg { dst } => {
                        let MachineOperand::Register(MachineRegister::Physical(dp)) = dst else {
                            return Err(unsupported_operand_form("dsp56000", &allocated, inst));
                        };
                        let d5 = phys_to_reg5(dp.0).ok_or_else(|| {
                            unsupported_operand_form("dsp56000", &allocated, inst)
                        })?;
                        enc_move_reg(&mut out, d5, REG5_A);
                        enc_neg_a(&mut out);
                        enc_move_reg(&mut out, REG5_A1, d5);
                    }

                    MachineInstruction::Mul { dst, src } => {
                        let MachineOperand::Register(MachineRegister::Physical(dp)) = dst else {
                            return Err(unsupported_operand_form("dsp56000", &allocated, inst));
                        };
                        let d5 = phys_to_reg5(dp.0).ok_or_else(|| {
                            unsupported_operand_form("dsp56000", &allocated, inst)
                        })?;
                        enc_move_reg(&mut out, d5, REG5_A1);
                        load_op_to_reg5(&mut out, &mut relocations, src, REG5_Y0)?;
                        enc_move_reg(&mut out, REG5_A1, REG5_X0);
                        enc_mpy_x0_y0_a(&mut out);
                        enc_asr_a(&mut out);
                        enc_move_reg(&mut out, REG5_A0, d5);
                    }

                    MachineInstruction::Div { dst, src } | MachineInstruction::Mod { dst, src } => {
                        let MachineOperand::Register(MachineRegister::Physical(dp)) = dst else {
                            return Err(unsupported_operand_form("dsp56000", &allocated, inst));
                        };
                        let d5 = phys_to_reg5(dp.0).ok_or_else(|| {
                            unsupported_operand_form("dsp56000", &allocated, inst)
                        })?;
                        load_op_to_reg5(&mut out, &mut relocations, src, REG5_B1)?;
                        enc_move_reg(&mut out, d5, REG5_A1);
                        let sym = if matches!(inst, MachineInstruction::Div { .. }) {
                            "__dsp56k_div24"
                        } else {
                            "__dsp56k_mod24"
                        };
                        let ext_off = enc_jsr_abs16_placeholder(&mut out);
                        relocations.push(AdobRelocation::new(
                            ext_off as u64,
                            0,
                            sym.to_string(),
                            RelocationKind::Dsp56k_Abs16,
                            0,
                        ));
                        enc_move_reg(&mut out, REG5_A1, d5);
                    }

                    MachineInstruction::Shl { dst, src }
                    | MachineInstruction::Shr { dst, src }
                    | MachineInstruction::Sar { dst, src } => {
                        let MachineOperand::Register(MachineRegister::Physical(dp)) = dst else {
                            return Err(unsupported_operand_form("dsp56000", &allocated, inst));
                        };
                        let d5 = phys_to_reg5(dp.0).ok_or_else(|| {
                            unsupported_operand_form("dsp56000", &allocated, inst)
                        })?;
                        if let MachineOperand::Immediate(imm) = src {
                            let cnt = (*imm).clamp(0, 24) as u8;
                            if cnt > 0 {
                                enc_move_reg(&mut out, d5, REG5_A);
                                if matches!(inst, MachineInstruction::Shr { .. }) {
                                    enc_clr_b(&mut out);
                                    enc_move_reg(&mut out, d5, REG5_A1);
                                }
                                if cnt > 1 {
                                    enc_rep_imm(&mut out, cnt);
                                }
                                match inst {
                                    MachineInstruction::Shl { .. } => enc_asl_a(&mut out),
                                    MachineInstruction::Shr { .. } => enc_lsr_a(&mut out),
                                    MachineInstruction::Sar { .. } => enc_asr_a(&mut out),
                                    _ => unreachable!(),
                                }
                                enc_move_reg(&mut out, REG5_A1, d5);
                            }
                        } else {
                            load_op_to_reg5(&mut out, &mut relocations, src, REG5_B1)?;
                            enc_move_reg(&mut out, d5, REG5_A1);
                            let sym = match inst {
                                MachineInstruction::Shl { .. } => "__dsp56k_shl24",
                                MachineInstruction::Shr { .. } => "__dsp56k_shr24",
                                MachineInstruction::Sar { .. } => "__dsp56k_sar24",
                                _ => unreachable!(),
                            };
                            let ext_off = enc_jsr_abs16_placeholder(&mut out);
                            relocations.push(AdobRelocation::new(
                                ext_off as u64,
                                0,
                                sym.to_string(),
                                RelocationKind::Dsp56k_Abs16,
                                0,
                            ));
                            enc_move_reg(&mut out, REG5_A1, d5);
                        }
                    }

                    MachineInstruction::Compare { lhs, rhs }
                    | MachineInstruction::CompareWithBorrow { lhs, rhs } => {
                        load_op_to_reg5(&mut out, &mut relocations, lhs, REG5_A1)?;
                        load_op_to_reg5(&mut out, &mut relocations, rhs, REG5_Y1)?;
                        enc_move_reg(&mut out, REG5_A1, REG5_B);
                        enc_move_reg(&mut out, REG5_B1, REG5_A);
                        emit_word24(&mut out, 0x200075); // CMP Y1, A
                    }

                    MachineInstruction::Test { lhs, rhs } => {
                        load_op_to_reg5(&mut out, &mut relocations, lhs, REG5_A1)?;
                        load_op_to_reg5(&mut out, &mut relocations, rhs, REG5_Y1)?;
                        emit_alu_a_op(&mut out, 0x06, REG5_Y1); // AND Y1, A (sets Z flag)
                    }

                    MachineInstruction::SetCc { dst, cc } => {
                        let MachineOperand::Register(MachineRegister::Physical(dp)) = dst else {
                            return Err(unsupported_operand_form("dsp56000", &allocated, inst));
                        };
                        let d5 = phys_to_reg5(dp.0).ok_or_else(|| {
                            unsupported_operand_form("dsp56000", &allocated, inst)
                        })?;
                        // Default dst = 0; if condition holds, set dst = 1.
                        enc_move_imm24(&mut out, d5, 0);
                        let cur_words = (out.len() / 3) as u32;
                        let skip_words = cur_words + 3; // Jcc (2 words) + MOVE #1, dst (1 word)
                        if let Some(c) = dsp56k_cc(*cc) {
                            // Jump over `MOVE #1, dst` when inverted: simpler to jump TO set_one if true,
                            // or unconditionally branch around! Use: Jcc +3 -> MOVE #0, JMP +2, MOVE #1
                            // Since `d5` is already 0, invert or jump forward:
                            // Jcc >set_one; JMP >done; set_one: MOVE #1, d5; done:
                            emit_word24(&mut out, 0x0AF0A0 | ((c as u32) & 0x0F));
                            let ext_set = out.len();
                            emit_word24(&mut out, 0);
                            relocations.push(AdobRelocation::new(
                                ext_set as u64,
                                0,
                                allocated.name.clone(),
                                RelocationKind::Dsp56k_Abs16,
                                (skip_words + 1) as i64,
                            ));
                            let ext_done = enc_jmp_abs16_placeholder(&mut out);
                            relocations.push(AdobRelocation::new(
                                ext_done as u64,
                                0,
                                allocated.name.clone(),
                                RelocationKind::Dsp56k_Abs16,
                                (skip_words + 2) as i64,
                            ));
                            enc_move_imm24(&mut out, d5, 1);
                        } else if matches!(cc, ConditionCode::Above) {
                            // HI: !EQ && CC
                            let ext_eq = enc_jcc_abs16_placeholder(&mut out, 0x0A); // JEQ >done
                            let ext_cs = enc_jcc_abs16_placeholder(&mut out, 0x08); // JCS >done
                            enc_move_imm24(&mut out, d5, 1);
                            let done_word = (out.len() / 3) as i64;
                            relocations.push(AdobRelocation::new(
                                ext_eq as u64,
                                0,
                                allocated.name.clone(),
                                RelocationKind::Dsp56k_Abs16,
                                done_word,
                            ));
                            relocations.push(AdobRelocation::new(
                                ext_cs as u64,
                                0,
                                allocated.name.clone(),
                                RelocationKind::Dsp56k_Abs16,
                                done_word,
                            ));
                        } else {
                            return Err(unsupported_operand_form("dsp56000", &allocated, inst));
                        }
                    }

                    MachineInstruction::Branch { target } => {
                        let ext_off = enc_jmp_abs16_placeholder(&mut out);
                        pending_branches.push(PendingBranch {
                            ext_byte_off: ext_off,
                            label: target.clone(),
                        });
                    }

                    MachineInstruction::BranchCc { cc, target } => {
                        if let Some(c) = dsp56k_cc(*cc) {
                            let ext_off = enc_jcc_abs16_placeholder(&mut out, c);
                            pending_branches.push(PendingBranch {
                                ext_byte_off: ext_off,
                                label: target.clone(),
                            });
                        } else if matches!(cc, ConditionCode::Above) {
                            // Skip JCC if EQ, then JCC target.
                            let cur_words = (out.len() / 3) as u32;
                            let ext_eq = enc_jcc_abs16_placeholder(&mut out, 0x0A); // JEQ
                            relocations.push(AdobRelocation::new(
                                ext_eq as u64,
                                0,
                                allocated.name.clone(),
                                RelocationKind::Dsp56k_Abs16,
                                (cur_words + 4) as i64,
                            ));
                            let ext_off = enc_jcc_abs16_placeholder(&mut out, 0x00); // JCC
                            pending_branches.push(PendingBranch {
                                ext_byte_off: ext_off,
                                label: target.clone(),
                            });
                        } else {
                            return Err(CodegenError::new(
                                "dsp56000",
                                format!("unsupported DSP56000 branch condition `{cc:?}`"),
                            ));
                        }
                    }

                    MachineInstruction::Call { target, .. } => match target {
                        MachineOperand::Symbol(sym) => {
                            let ext_off = enc_jsr_abs16_placeholder(&mut out);
                            relocations.push(AdobRelocation::new(
                                ext_off as u64,
                                0,
                                sym.clone(),
                                RelocationKind::Dsp56k_Abs16,
                                0,
                            ));
                        }
                        _ => return Err(unsupported_operand_form("dsp56000", &allocated, inst)),
                    },

                    MachineInstruction::Return => {
                        enc_move_reg(&mut out, REG5_R6, REG5_R7);
                        enc_move_x_ea(&mut out, REG5_R6, true, ea_pre_dec(7));
                        enc_rts(&mut out);
                    }

                    MachineInstruction::Push { src } => {
                        load_op_to_reg5(&mut out, &mut relocations, src, REG5_A1)?;
                        enc_move_x_ea(&mut out, REG5_A1, false, ea_post_inc(7));
                    }

                    MachineInstruction::Pop { dst } => {
                        let MachineOperand::Register(MachineRegister::Physical(dp)) = dst else {
                            return Err(unsupported_operand_form("dsp56000", &allocated, inst));
                        };
                        let d5 = phys_to_reg5(dp.0).ok_or_else(|| {
                            unsupported_operand_form("dsp56000", &allocated, inst)
                        })?;
                        enc_move_x_ea(&mut out, d5, true, ea_pre_dec(7));
                    }

                    MachineInstruction::Custom { name, operands }
                        if name == "dsp56k_lea_slot" && operands.len() == 2 =>
                    {
                        let MachineOperand::Register(MachineRegister::Physical(dp)) = &operands[0]
                        else {
                            return Err(unsupported_operand_form("dsp56000", &allocated, inst));
                        };
                        let d5 = phys_to_reg5(dp.0).ok_or_else(|| {
                            unsupported_operand_form("dsp56000", &allocated, inst)
                        })?;
                        if let Some(off) = resolve_r6_off(&operands[1]) {
                            enc_move_reg(&mut out, REG5_R6, REG5_A);
                            enc_move_imm24(&mut out, REG5_Y1, off as i64);
                            emit_alu_a_op(&mut out, 0x00, REG5_Y1);
                            enc_move_reg(&mut out, REG5_A1, d5);
                        } else if let MachineOperand::Symbol(sym) = &operands[1] {
                            let ext_off = enc_move_imm24_reloc(&mut out, d5, 0);
                            relocations.push(AdobRelocation::new(
                                ext_off as u64,
                                0,
                                sym.clone(),
                                RelocationKind::Dsp56k_Abs16,
                                0,
                            ));
                        } else {
                            return Err(unsupported_operand_form("dsp56000", &allocated, inst));
                        }
                    }

                    MachineInstruction::Custom { name, operands }
                        if (name == "dsp56k_mac" || name == "mac") && operands.is_empty() =>
                    {
                        emit_word24(&mut out, 0x2000A2); // MAC +X0, Y0, A
                    }

                    _ => return Err(unsupported_instruction("dsp56000", &allocated, inst)),
                }
            }
        }

        for br in pending_branches {
            let target_word = label_word_offsets.get(&br.label).copied().ok_or_else(|| {
                CodegenError::new(
                    "dsp56000",
                    format!(
                        "unresolved local label `{}` in DSP56000 function `{}`",
                        br.label, allocated.name
                    ),
                )
            })?;
            relocations.push(AdobRelocation::new(
                br.ext_byte_off as u64,
                0,
                allocated.name.clone(),
                RelocationKind::Dsp56k_Abs16,
                target_word as i64,
            ));
        }

        Ok((out, relocations))
    }
}

impl CodegenBackend for Dsp56000Backend {
    fn target(&self) -> &TargetDescriptor {
        &self.target
    }

    fn capabilities(&self) -> TargetCapabilities {
        self.capabilities.clone()
    }

    fn lower_module(&mut self, module: &NativeModule) -> Result<NativeModule, CodegenError> {
        let mut out = module.clone();
        out.functions.clear();
        for f in &module.functions {
            out.functions.push(self.lower_function_internal(f)?);
        }
        Ok(out)
    }

    fn generate_function(&mut self, func: &MachineFunction) -> Result<Vec<u8>, CodegenError> {
        let (bytes, _) = self.emit_function_with_relocs(func)?;
        Ok(bytes)
    }

    fn emit_object(&mut self, module: &NativeModule) -> Result<AdobObject, CodegenError> {
        let mut obj = AdobObject::new(self.target.clone());

        // Harvard P:/X:/Y: spaces tagged in disjoint linear ADOB metadata windows
        // (P: 0x0000_0000, X: 0x0100_0000, Y: 0x0200_0000; 64K 24-bit words each).
        obj.memory_regions.push(MemoryRegion::new(
            ".p_prog",
            0x0000_0000,
            65536 * 3,
            MemoryPermissions::Rx,
        ));
        obj.memory_regions.push(MemoryRegion::new(
            ".x_data",
            0x0100_0000,
            65536 * 3,
            MemoryPermissions::Rw,
        ));
        obj.memory_regions.push(MemoryRegion::new(
            ".y_data",
            0x0200_0000,
            65536 * 3,
            MemoryPermissions::Rw,
        ));

        let mut text_bytes = Vec::new();
        let mut text_relocs = Vec::new();

        for func in &module.functions {
            let fn_offset = text_bytes.len() as u64;
            let (code, mut relocs) = self.emit_function_with_relocs(func)?;
            let size = code.len() as u64;
            for r in &mut relocs {
                r.offset += fn_offset;
            }
            text_bytes.extend_from_slice(&code);
            text_relocs.extend(relocs);

            let sym = AdobSymbol::new_defined(
                0,
                func.name.clone(),
                SymbolKind::Function,
                0,
                fn_offset,
                size,
            )
            .with_binding(SymbolBinding::Global)
            .with_visibility(SymbolVisibility::Default);
            obj.add_symbol(sym);

            if func.is_exported {
                obj.add_export(func.name.clone());
            }
        }

        let mut text_sec = AdobSection::new(".text", SectionKind::Text)
            .with_flags(section_flags::READ | section_flags::EXECUTE | section_flags::ALLOC)
            .with_alignment(1)
            .with_memory_region(".p_prog")
            .with_data(text_bytes);
        text_sec.relocations = text_relocs;
        obj.add_section(text_sec);

        if !module.string_pool.is_empty() {
            let rodata_idx = obj.sections.len() as u32;
            let mut rodata = Vec::new();
            let mut offsets: HashMap<&str, u64> = HashMap::new();
            for (idx, s) in module.string_pool.iter().enumerate() {
                let off = *offsets.entry(s.as_str()).or_insert_with(|| {
                    let cur = rodata.len() as u64;
                    for b in s.as_bytes() {
                        emit_word24(&mut rodata, *b as u32);
                    }
                    emit_word24(&mut rodata, 0);
                    cur
                });
                obj.add_symbol(
                    AdobSymbol::new_defined(
                        0,
                        format!("__str_{idx}"),
                        SymbolKind::Object,
                        rodata_idx,
                        off,
                        (s.len() as u64 + 1) * 3,
                    )
                    .with_binding(SymbolBinding::Local),
                );
            }
            obj.add_section(
                AdobSection::new(".rodata", SectionKind::Rodata)
                    .with_flags(section_flags::READ | section_flags::ALLOC)
                    .with_alignment(1)
                    .with_memory_region(".x_data")
                    .with_data(rodata),
            );
        }

        if !module.data_sections.is_empty() {
            let data_idx = obj.sections.len() as u32;
            let mut data_bytes = Vec::new();
            for (name, init_bytes) in &module.data_sections {
                let off = data_bytes.len() as u64;
                data_bytes.extend_from_slice(init_bytes);
                while data_bytes.len() % 3 != 0 {
                    data_bytes.push(0);
                }
                obj.add_symbol(
                    AdobSymbol::new_defined(
                        0,
                        name.clone(),
                        SymbolKind::Object,
                        data_idx,
                        off,
                        (data_bytes.len() as u64) - off,
                    )
                    .with_binding(SymbolBinding::Global),
                );
            }
            obj.add_section(
                AdobSection::new(".data", SectionKind::Data)
                    .with_flags(section_flags::READ | section_flags::WRITE | section_flags::ALLOC)
                    .with_alignment(1)
                    .with_memory_region(".x_data")
                    .with_data(data_bytes),
            );
        }

        if !module.tls_sections.is_empty() {
            return Err(CodegenError::new(
                self.target.triple_string(),
                "thread-local storage (.tls) is unsupported on bare-metal Motorola DSP56000",
            )
            .with_arch("dsp56000")
            .with_suggestion(
                "Remove `thread_local` variables when targeting dsp56000-unknown-unknown.",
            ));
        }

        let mut known: std::collections::HashSet<String> =
            obj.symbols.iter().map(|s| s.name.clone()).collect();
        let missing: Vec<String> = obj.sections[0]
            .relocations
            .iter()
            .filter_map(|r| {
                if known.insert(r.symbol_name.clone()) {
                    Some(r.symbol_name.clone())
                } else {
                    None
                }
            })
            .collect();
        for name in missing {
            obj.add_symbol(
                AdobSymbol::new_undefined(0, name, SymbolKind::Function)
                    .with_binding(SymbolBinding::Global),
            );
        }

        Ok(obj)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine_ir::VirtualRegister;

    #[test]
    fn encodes_24bit_big_endian_arith_and_mpy() {
        let target = TargetDescriptor::from_triple("dsp56000-unknown-unknown").expect("triple");
        let mut backend = Dsp56000Backend::new(target);

        let mut func = MachineFunction::new("dsp_kernel");
        let v0 = func.alloc_vreg();
        let v1 = func.alloc_vreg();
        let bb = &mut func.blocks[0];
        bb.push(MachineInstruction::Move {
            dst: MachineOperand::reg(v0.0),
            src: MachineOperand::Immediate(7),
        });
        bb.push(MachineInstruction::Move {
            dst: MachineOperand::reg(v1.0),
            src: MachineOperand::Immediate(6),
        });
        bb.push(MachineInstruction::Mul {
            dst: MachineOperand::reg(v0.0),
            src: MachineOperand::reg(v1.0),
        });
        bb.push(MachineInstruction::Move {
            dst: MachineOperand::phys(PHYS_A1),
            src: MachineOperand::reg(v0.0),
        });
        bb.push(MachineInstruction::Return);

        let bytes = backend.generate_function(&func).expect("dsp56k codegen");
        assert_eq!(bytes.len() % 3, 0);
        let words: Vec<u32> = bytes
            .chunks_exact(3)
            .map(|c| ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | (c[2] as u32))
            .collect();
        assert!(
            words.contains(&0x2000A0),
            "missing MPY +X0,Y0,A in {words:06X?}"
        );
        assert!(words.contains(&0x200022), "missing ASR A in {words:06X?}");
        assert!(words.contains(&0x00000C), "missing RTS in {words:06X?}");
    }

    #[test]
    fn register_spilling_handles_pressure_above_4_regs() {
        let target = TargetDescriptor::from_triple("dsp56000-unknown-unknown").expect("triple");
        let mut backend = Dsp56000Backend::new(target);

        let mut func = MachineFunction::new("pressure");
        let vregs: Vec<VirtualRegister> = (0..7).map(|_| func.alloc_vreg()).collect();
        let bb = &mut func.blocks[0];
        for (i, &v) in vregs.iter().enumerate() {
            bb.push(MachineInstruction::Move {
                dst: MachineOperand::reg(v.0),
                src: MachineOperand::Immediate((i as i64) + 1),
            });
        }
        for &v in &vregs[1..] {
            bb.push(MachineInstruction::Add {
                dst: MachineOperand::reg(vregs[0].0),
                src: MachineOperand::reg(v.0),
            });
        }
        bb.push(MachineInstruction::Move {
            dst: MachineOperand::phys(PHYS_A1),
            src: MachineOperand::reg(vregs[0].0),
        });
        bb.push(MachineInstruction::Return);

        let bytes = backend
            .generate_function(&func)
            .expect("dsp56k pressure spills cleanly");
        assert_eq!(bytes.len() % 3, 0);
        assert!(bytes.len() > 24);
    }

    #[test]
    fn refuses_unsupported_float_instruction_loudly() {
        let target = TargetDescriptor::from_triple("dsp56000-unknown-unknown").expect("triple");
        let mut backend = Dsp56000Backend::new(target);

        let mut func = MachineFunction::new("fp_bad");
        func.blocks[0].push(MachineInstruction::FAdd {
            dst: MachineOperand::phys(PHYS_A1),
            src: MachineOperand::phys(PHYS_B1),
            size: 8,
        });
        func.blocks[0].push(MachineInstruction::Return);

        let err = backend
            .generate_function(&func)
            .expect_err("must refuse FAdd");
        let msg = err.to_string();
        assert!(msg.contains("dsp56000"), "{msg}");
        assert!(msg.contains("FAdd"), "{msg}");
    }
}
