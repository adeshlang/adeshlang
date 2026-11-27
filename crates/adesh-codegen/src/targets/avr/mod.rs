//! 8-bit AVR (ATmega328P `avr5`) Native Codegen Backend.
//!
//! Models the real ATmega328P Harvard architecture:
//! - 32 × 8-bit general-purpose registers (`r0..r31`), operated in 16-bit
//!   even-aligned pairs `r(p+1):r(p)` for pointer/16-bit/legalized-32-bit code.
//! - Dedicated register roles (`avr-gcc` compatible):
//!   - `r0` (`__tmp_reg__`): encoder scratch byte
//!   - `r1` (`__zero_reg__`): invariant zero (cleared after `MUL`)
//!   - `r2..r17` (pairs `2,4,6,8,10,12,14,16`): callee-saved allocatable GPR pairs
//!   - `r18..r25` (pairs `18,20,22,24`): caller-saved argument/return/spill-scratch pairs
//!   - `r26:r27` (`X`, id `26`): dedicated address/immediate staging scratch pair
//!   - `r28:r29` (`Y`, id `28`): frame pointer
//!   - `r30:r31` (`Z`, id `30`): dedicated pointer/`LPM`/`ICALL` scratch pair
//!   - `32`: pseudo-id representing I/O stack pointer `SPH:SPL` (`0x3E:0x3D`)
//! - Separate address spaces:
//!   - Program Flash (`0x0000..0x7FFF`, 32 KiB / 16K words) for `.text` and `.rodata`
//!   - Data SRAM (`0x0100..0x08FF`, 2 KiB) for `.data`, `.bss`, and downward stack

#![forbid(unsafe_code)]

use crate::backend::CodegenBackend;
use crate::calling_convention::ParallelMoveResolver;
use crate::error::CodegenError;
use crate::legalize::legalize_32bit_to_16bit_pairs;
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

const SREG_IO: u8 = 0x3F;
const SPL_IO: u8 = 0x3D;
const SPH_IO: u8 = 0x3E;

const REG_TMP: u8 = 0;
const REG_ZERO: u8 = 1;
const REG_XL: u8 = 26;
const REG_XH: u8 = 27;
const REG_YL: u8 = 28;
const REG_YH: u8 = 29;
const REG_ZL: u8 = 30;
const REG_ZH: u8 = 31;
const PSEUDO_SP: u8 = 32;

static AVR_GPRS: [PhysicalRegister; 16] = [
    PhysicalRegister::gpr(0),
    PhysicalRegister::gpr(2),
    PhysicalRegister::gpr(4),
    PhysicalRegister::gpr(6),
    PhysicalRegister::gpr(8),
    PhysicalRegister::gpr(10),
    PhysicalRegister::gpr(12),
    PhysicalRegister::gpr(14),
    PhysicalRegister::gpr(16),
    PhysicalRegister::gpr(18),
    PhysicalRegister::gpr(20),
    PhysicalRegister::gpr(22),
    PhysicalRegister::gpr(24),
    PhysicalRegister::gpr(26),
    PhysicalRegister::gpr(28),
    PhysicalRegister::gpr(30),
];

static AVR_ALLOCATABLE: [PhysicalRegister; 8] = [
    PhysicalRegister::gpr(2),
    PhysicalRegister::gpr(4),
    PhysicalRegister::gpr(6),
    PhysicalRegister::gpr(8),
    PhysicalRegister::gpr(10),
    PhysicalRegister::gpr(12),
    PhysicalRegister::gpr(14),
    PhysicalRegister::gpr(16),
];

static AVR_CALLER_SAVED: [PhysicalRegister; 4] = [
    PhysicalRegister::gpr(18),
    PhysicalRegister::gpr(20),
    PhysicalRegister::gpr(22),
    PhysicalRegister::gpr(24),
];

static AVR_CALLEE_SAVED: [PhysicalRegister; 8] = [
    PhysicalRegister::gpr(2),
    PhysicalRegister::gpr(4),
    PhysicalRegister::gpr(6),
    PhysicalRegister::gpr(8),
    PhysicalRegister::gpr(10),
    PhysicalRegister::gpr(12),
    PhysicalRegister::gpr(14),
    PhysicalRegister::gpr(16),
];

static AVR_RESERVED: [PhysicalRegister; 4] = [
    PhysicalRegister::gpr(0),
    PhysicalRegister::gpr(26), // X
    PhysicalRegister::gpr(28), // Y
    PhysicalRegister::gpr(30), // Z
];

struct AvrRegisterFile;

impl RegisterFile for AvrRegisterFile {
    fn registers(&self) -> &[PhysicalRegister] {
        &AVR_GPRS
    }

    fn allocatable(&self) -> &[PhysicalRegister] {
        &AVR_ALLOCATABLE
    }

    fn caller_saved(&self) -> &[PhysicalRegister] {
        &AVR_CALLER_SAVED
    }

    fn callee_saved(&self) -> &[PhysicalRegister] {
        &AVR_CALLEE_SAVED
    }

    fn reserved(&self) -> &[PhysicalRegister] {
        &AVR_RESERVED
    }

    fn scratch_for_class(&self, class: RegisterClass) -> (PhysicalRegister, PhysicalRegister) {
        match class {
            // Keep r24:r25 (return) and X/Z (encoder staging) free from spill reload clobber.
            RegisterClass::Gpr => (PhysicalRegister::gpr(20), PhysicalRegister::gpr(18)),
            RegisterClass::Float => (PhysicalRegister::gpr(20), PhysicalRegister::gpr(18)),
        }
    }

    fn frame_pointer(&self) -> PhysicalRegister {
        PhysicalRegister::gpr(REG_YL)
    }

    fn scalar_slot_size(&self) -> u32 {
        2
    }
}

fn is_even_pair(id: u8) -> bool {
    id < 32 && (id & 1) == 0 && id != 0 && id != REG_YL
}

fn pair_reg(op: &MachineOperand) -> Option<u8> {
    match op {
        MachineOperand::Register(MachineRegister::Physical(p)) if is_even_pair(p.0) => Some(p.0),
        _ => None,
    }
}

fn imm16(v: i64) -> u16 {
    v as i16 as u16
}

// ============================================================================
// ATmega328P (avr5) 16-bit / 32-bit Instruction Encodings (little-endian words)
// ============================================================================

#[inline]
fn emit_w(out: &mut Vec<u8>, word: u16) {
    out.extend_from_slice(&word.to_le_bytes());
}

#[inline]
fn rd_rr(base: u16, d: u8, r: u8) -> u16 {
    base | (((r as u16) & 0x10) << 5) | (((d as u16) & 0x1F) << 4) | ((r as u16) & 0x0F)
}

#[inline]
fn rd_imm(base: u16, d: u8, k: u8) -> Result<u16, CodegenError> {
    if !(16..=31).contains(&d) {
        return Err(CodegenError::new(
            "avr",
            format!("AVR immediate instruction requires r16..r31, got r{d}"),
        ));
    }
    let d4 = (d - 16) as u16;
    Ok(base | (((k as u16) & 0xF0) << 4) | (d4 << 4) | ((k as u16) & 0x0F))
}

fn enc_nop(out: &mut Vec<u8>) {
    emit_w(out, 0x0000);
}

fn enc_mov(out: &mut Vec<u8>, d: u8, r: u8) {
    emit_w(out, rd_rr(0x2C00, d, r));
}

fn enc_movw(out: &mut Vec<u8>, d_even: u8, r_even: u8) -> Result<(), CodegenError> {
    if (d_even & 1) != 0 || (r_even & 1) != 0 || d_even >= 32 || r_even >= 32 {
        return Err(CodegenError::new(
            "avr",
            format!("MOVW requires even register pairs, got r{d_even}, r{r_even}"),
        ));
    }
    emit_w(
        out,
        0x0100 | (((d_even as u16) >> 1) << 4) | ((r_even as u16) >> 1),
    );
    Ok(())
}

fn enc_ldi(out: &mut Vec<u8>, d: u8, k: u8) -> Result<(), CodegenError> {
    emit_w(out, rd_imm(0xE000, d, k)?);
    Ok(())
}

fn enc_add(out: &mut Vec<u8>, d: u8, r: u8) {
    emit_w(out, rd_rr(0x0C00, d, r));
}

fn enc_adc(out: &mut Vec<u8>, d: u8, r: u8) {
    emit_w(out, rd_rr(0x1C00, d, r));
}

fn enc_sub(out: &mut Vec<u8>, d: u8, r: u8) {
    emit_w(out, rd_rr(0x1800, d, r));
}

fn enc_sbc(out: &mut Vec<u8>, d: u8, r: u8) {
    emit_w(out, rd_rr(0x0800, d, r));
}

fn enc_subi(out: &mut Vec<u8>, d: u8, k: u8) -> Result<(), CodegenError> {
    emit_w(out, rd_imm(0x5000, d, k)?);
    Ok(())
}

fn enc_sbci(out: &mut Vec<u8>, d: u8, k: u8) -> Result<(), CodegenError> {
    emit_w(out, rd_imm(0x4000, d, k)?);
    Ok(())
}

fn enc_and(out: &mut Vec<u8>, d: u8, r: u8) {
    emit_w(out, rd_rr(0x2000, d, r));
}

fn enc_or(out: &mut Vec<u8>, d: u8, r: u8) {
    emit_w(out, rd_rr(0x2800, d, r));
}

fn enc_eor(out: &mut Vec<u8>, d: u8, r: u8) {
    emit_w(out, rd_rr(0x2400, d, r));
}

fn enc_com(out: &mut Vec<u8>, d: u8) {
    emit_w(out, 0x9400 | (((d as u16) & 0x1F) << 4));
}

fn enc_neg(out: &mut Vec<u8>, d: u8) {
    emit_w(out, 0x9401 | (((d as u16) & 0x1F) << 4));
}

fn enc_inc(out: &mut Vec<u8>, d: u8) {
    emit_w(out, 0x9403 | (((d as u16) & 0x1F) << 4));
}

fn enc_dec(out: &mut Vec<u8>, d: u8) {
    emit_w(out, 0x940A | (((d as u16) & 0x1F) << 4));
}

fn enc_cp(out: &mut Vec<u8>, d: u8, r: u8) {
    emit_w(out, rd_rr(0x1400, d, r));
}

fn enc_cpc(out: &mut Vec<u8>, d: u8, r: u8) {
    emit_w(out, rd_rr(0x0400, d, r));
}

fn enc_cpi(out: &mut Vec<u8>, d: u8, k: u8) -> Result<(), CodegenError> {
    emit_w(out, rd_imm(0x3000, d, k)?);
    Ok(())
}

fn enc_adiw(out: &mut Vec<u8>, d: u8, k: u8) -> Result<(), CodegenError> {
    if !matches!(d, 24 | 26 | 28 | 30) || k > 63 {
        return Err(CodegenError::new(
            "avr",
            format!("ADIW invalid operands r{d}, {k}"),
        ));
    }
    let dd = ((d - 24) / 2) as u16;
    let ku = k as u16;
    emit_w(out, 0x9600 | ((ku & 0x30) << 2) | (dd << 4) | (ku & 0x0F));
    Ok(())
}

fn enc_sbiw(out: &mut Vec<u8>, d: u8, k: u8) -> Result<(), CodegenError> {
    if !matches!(d, 24 | 26 | 28 | 30) || k > 63 {
        return Err(CodegenError::new(
            "avr",
            format!("SBIW invalid operands r{d}, {k}"),
        ));
    }
    let dd = ((d - 24) / 2) as u16;
    let ku = k as u16;
    emit_w(out, 0x9700 | ((ku & 0x30) << 2) | (dd << 4) | (ku & 0x0F));
    Ok(())
}

fn enc_mul(out: &mut Vec<u8>, d: u8, r: u8) {
    emit_w(out, rd_rr(0x9C00, d, r));
}

fn enc_lsl(out: &mut Vec<u8>, d: u8) {
    enc_add(out, d, d);
}

fn enc_rol(out: &mut Vec<u8>, d: u8) {
    enc_adc(out, d, d);
}

fn enc_lsr(out: &mut Vec<u8>, d: u8) {
    emit_w(out, 0x9406 | (((d as u16) & 0x1F) << 4));
}

fn enc_asr(out: &mut Vec<u8>, d: u8) {
    emit_w(out, 0x9405 | (((d as u16) & 0x1F) << 4));
}

fn enc_ror(out: &mut Vec<u8>, d: u8) {
    emit_w(out, 0x9407 | (((d as u16) & 0x1F) << 4));
}

fn enc_push(out: &mut Vec<u8>, d: u8) {
    emit_w(out, 0x920F | (((d as u16) & 0x1F) << 4));
}

fn enc_pop(out: &mut Vec<u8>, d: u8) {
    emit_w(out, 0x900F | (((d as u16) & 0x1F) << 4));
}

fn enc_in(out: &mut Vec<u8>, d: u8, a: u8) {
    let au = (a & 0x3F) as u16;
    emit_w(
        out,
        0xB000 | ((au & 0x30) << 5) | (((d as u16) & 0x1F) << 4) | (au & 0x0F),
    );
}

fn enc_out(out: &mut Vec<u8>, a: u8, r: u8) {
    let au = (a & 0x3F) as u16;
    emit_w(
        out,
        0xB800 | ((au & 0x30) << 5) | (((r as u16) & 0x1F) << 4) | (au & 0x0F),
    );
}

fn enc_ld_z(out: &mut Vec<u8>, d: u8) {
    emit_w(out, 0x8000 | (((d as u16) & 0x1F) << 4));
}

fn enc_st_z(out: &mut Vec<u8>, r: u8) {
    emit_w(out, 0x8200 | (((r as u16) & 0x1F) << 4));
}

fn enc_ldd(out: &mut Vec<u8>, d: u8, use_y: bool, q: u8) -> Result<(), CodegenError> {
    if q > 63 {
        return Err(CodegenError::new(
            "avr",
            format!("LDD displacement {q} exceeds 6-bit maximum 63"),
        ));
    }
    let qu = q as u16;
    let y_bit = if use_y { 0x0008 } else { 0x0000 };
    emit_w(
        out,
        0x8000
            | y_bit
            | ((qu & 0x20) << 8)
            | ((qu & 0x18) << 7)
            | (((d as u16) & 0x1F) << 4)
            | (qu & 0x07),
    );
    Ok(())
}

fn enc_std(out: &mut Vec<u8>, use_y: bool, q: u8, r: u8) -> Result<(), CodegenError> {
    if q > 63 {
        return Err(CodegenError::new(
            "avr",
            format!("STD displacement {q} exceeds 6-bit maximum 63"),
        ));
    }
    let qu = q as u16;
    let y_bit = if use_y { 0x0008 } else { 0x0000 };
    emit_w(
        out,
        0x8200
            | y_bit
            | ((qu & 0x20) << 8)
            | ((qu & 0x18) << 7)
            | (((r as u16) & 0x1F) << 4)
            | (qu & 0x07),
    );
    Ok(())
}

fn enc_rjmp(out: &mut Vec<u8>, rel_words: i16) -> Result<(), CodegenError> {
    if !(-2048..=2047).contains(&rel_words) {
        return Err(CodegenError::new(
            "avr",
            format!("RJMP offset {rel_words} out of 12-bit range"),
        ));
    }
    emit_w(out, 0xC000 | ((rel_words as u16) & 0x0FFF));
    Ok(())
}

fn enc_brbs(out: &mut Vec<u8>, bit: u8, rel_words: i8) {
    emit_w(
        out,
        0xF000 | ((((rel_words as i16) as u16) & 0x007F) << 3) | ((bit as u16) & 7),
    );
}

fn enc_brbc(out: &mut Vec<u8>, bit: u8, rel_words: i8) {
    emit_w(
        out,
        0xF400 | ((((rel_words as i16) as u16) & 0x007F) << 3) | ((bit as u16) & 7),
    );
}

fn enc_jmp_placeholder(out: &mut Vec<u8>) -> usize {
    let offset = out.len();
    emit_w(out, 0x940C);
    emit_w(out, 0x0000);
    offset
}

fn enc_call_placeholder(out: &mut Vec<u8>) -> usize {
    let offset = out.len();
    emit_w(out, 0x940E);
    emit_w(out, 0x0000);
    offset
}

fn enc_icall(out: &mut Vec<u8>) {
    emit_w(out, 0x9509);
}

fn enc_ret(out: &mut Vec<u8>) {
    emit_w(out, 0x9508);
}

fn enc_cli(out: &mut Vec<u8>) {
    emit_w(out, 0x94F8);
}

/// Load a 16-bit immediate into any even register pair (`r2..r30`).
/// Uses `X (r26:r27)` as intermediate when `dst_pair < 16`.
fn emit_load_imm16(out: &mut Vec<u8>, dst_pair: u8, val: u16) -> Result<(), CodegenError> {
    let lo = (val & 0xFF) as u8;
    let hi = (val >> 8) as u8;
    if dst_pair >= 16 {
        enc_ldi(out, dst_pair, lo)?;
        enc_ldi(out, dst_pair + 1, hi)?;
    } else {
        enc_ldi(out, REG_XL, lo)?;
        enc_ldi(out, REG_XH, hi)?;
        enc_movw(out, dst_pair, REG_XL)?;
    }
    Ok(())
}

/// Move a 16-bit pair (`dst_pair <- src_pair`).
fn emit_mov16(out: &mut Vec<u8>, dst_pair: u8, src_pair: u8) -> Result<(), CodegenError> {
    if dst_pair != src_pair {
        enc_movw(out, dst_pair, src_pair)?;
    }
    Ok(())
}

/// Atomically write `Y (r29:r28)` into `SPH:SPL (0x3E:0x3D)` while preserving `SREG`.
fn emit_write_sp_from_y(out: &mut Vec<u8>) {
    enc_in(out, REG_TMP, SREG_IO);
    enc_cli(out);
    enc_out(out, SPH_IO, REG_YH);
    enc_out(out, SREG_IO, REG_TMP);
    enc_out(out, SPL_IO, REG_YL);
}

/// Adjust `Z` (`r31:r30`) by signed 16-bit constant `delta`.
fn emit_add_imm16_z(out: &mut Vec<u8>, delta: i16) -> Result<(), CodegenError> {
    if delta == 0 {
        return Ok(());
    }
    if (0..=63).contains(&delta) {
        enc_adiw(out, REG_ZL, delta as u8)?;
        return Ok(());
    }
    if (-63..0).contains(&delta) {
        enc_sbiw(out, REG_ZL, (-delta) as u8)?;
        return Ok(());
    }
    let neg = (-delta) as u16;
    enc_subi(out, REG_ZL, (neg & 0xFF) as u8)?;
    enc_sbci(out, REG_ZH, (neg >> 8) as u8)?;
    Ok(())
}

/// Adjust `Y` (`r29:r28`) by signed 16-bit constant `delta`.
fn emit_add_imm16_y(out: &mut Vec<u8>, delta: i16) -> Result<(), CodegenError> {
    if delta == 0 {
        return Ok(());
    }
    if (0..=63).contains(&delta) {
        enc_adiw(out, REG_YL, delta as u8)?;
        return Ok(());
    }
    if (-63..0).contains(&delta) {
        enc_sbiw(out, REG_YL, (-delta) as u8)?;
        return Ok(());
    }
    let neg = (-delta) as u16;
    enc_subi(out, REG_YL, (neg & 0xFF) as u8)?;
    enc_sbci(out, REG_YH, (neg >> 8) as u8)?;
    Ok(())
}

/// Load a 16-bit or 8-bit value from `Y + y_off` into `dst_pair`.
fn emit_load_y_rel(
    out: &mut Vec<u8>,
    dst_pair: u8,
    y_off: i32,
    size: u8,
) -> Result<(), CodegenError> {
    if (0..=62).contains(&y_off) {
        let q = y_off as u8;
        enc_ldd(out, dst_pair, true, q)?;
        if size == 1 {
            enc_mov(out, dst_pair + 1, REG_ZERO);
        } else {
            enc_ldd(out, dst_pair + 1, true, q + 1)?;
        }
    } else {
        let d16 = i16::try_from(y_off).map_err(|_| {
            CodegenError::new(
                "avr",
                format!("Y-relative offset {y_off} overflows 16-bit address"),
            )
        })?;
        enc_movw(out, REG_ZL, REG_YL)?;
        emit_add_imm16_z(out, d16)?;
        enc_ld_z(out, dst_pair);
        if size == 1 {
            enc_mov(out, dst_pair + 1, REG_ZERO);
        } else {
            enc_ldd(out, dst_pair + 1, false, 1)?;
        }
    }
    Ok(())
}

/// Store a 16-bit or 8-bit value from `src_pair` into `Y + y_off`.
fn emit_store_y_rel(
    out: &mut Vec<u8>,
    y_off: i32,
    src_pair: u8,
    size: u8,
) -> Result<(), CodegenError> {
    if (0..=62).contains(&y_off) {
        let q = y_off as u8;
        enc_std(out, true, q, src_pair)?;
        if size > 1 {
            enc_std(out, true, q + 1, src_pair + 1)?;
        }
    } else {
        let d16 = i16::try_from(y_off).map_err(|_| {
            CodegenError::new(
                "avr",
                format!("Y-relative offset {y_off} overflows 16-bit address"),
            )
        })?;
        enc_movw(out, REG_ZL, REG_YL)?;
        emit_add_imm16_z(out, d16)?;
        enc_st_z(out, src_pair);
        if size > 1 {
            enc_std(out, false, 1, src_pair + 1)?;
        }
    }
    Ok(())
}

/// Load `Z` (`r31:r30`) with effective address `[base + offset + index]`.
fn emit_compute_mem_addr_z(
    out: &mut Vec<u8>,
    base: MachineRegister,
    offset: i32,
    index: Option<(MachineRegister, u8)>,
    frame_bytes: i32,
) -> Result<(), CodegenError> {
    match base {
        MachineRegister::Physical(p) if p.0 == REG_YL => {
            let y_off = if offset < 0 {
                frame_bytes + 1 + offset
            } else {
                frame_bytes + offset
            };
            let d16 = i16::try_from(y_off).map_err(|_| {
                CodegenError::new(
                    "avr",
                    format!("Y-relative offset {y_off} overflows 16 bits"),
                )
            })?;
            enc_movw(out, REG_ZL, REG_YL)?;
            emit_add_imm16_z(out, d16)?;
        }
        MachineRegister::Physical(p) if p.0 == PSEUDO_SP => {
            enc_in(out, REG_ZL, SPL_IO);
            enc_in(out, REG_ZH, SPH_IO);
            // AVR SP is post-decrement, so caller outgoing slots start at SP + 1.
            let d16 = i16::try_from(1 + offset).map_err(|_| {
                CodegenError::new(
                    "avr",
                    format!("SP-relative offset {offset} overflows 16 bits"),
                )
            })?;
            emit_add_imm16_z(out, d16)?;
        }
        MachineRegister::Physical(p) if is_even_pair(p.0) => {
            enc_movw(out, REG_ZL, p.0)?;
            let d16 = i16::try_from(offset).map_err(|_| {
                CodegenError::new("avr", format!("memory offset {offset} overflows 16 bits"))
            })?;
            emit_add_imm16_z(out, d16)?;
        }
        other => {
            return Err(CodegenError::new(
                "avr",
                format!("unsupported AVR memory base register {other:?}"),
            ));
        }
    }

    if let Some((idx_reg, scale)) = index {
        let MachineRegister::Physical(idx_p) = idx_reg else {
            return Err(CodegenError::new(
                "avr",
                "virtual index register reached AVR encoder",
            ));
        };
        if !is_even_pair(idx_p.0) {
            return Err(CodegenError::new(
                "avr",
                format!("invalid AVR index pair r{}", idx_p.0),
            ));
        }
        if scale == 1 {
            enc_add(out, REG_ZL, idx_p.0);
            enc_adc(out, REG_ZH, idx_p.0 + 1);
        } else {
            enc_movw(out, REG_XL, idx_p.0)?;
            let mut s = scale;
            while s > 1 {
                if s & 1 != 0 {
                    return Err(CodegenError::new(
                        "avr",
                        format!("unsupported non-power-of-2 index scale {scale}"),
                    ));
                }
                enc_lsl(out, REG_XL);
                enc_rol(out, REG_XH);
                s >>= 1;
            }
            enc_add(out, REG_ZL, REG_XL);
            enc_adc(out, REG_ZH, REG_XH);
        }
    }
    Ok(())
}

/// Emit `SetCc` (`dst_pair = 0 or 1`) without disturbing `SREG` before reading flags.
fn emit_setcc(out: &mut Vec<u8>, dst_pair: u8, cc: ConditionCode) -> Result<(), CodegenError> {
    // LDI and MOV do NOT modify SREG!
    enc_ldi(out, REG_XL, 1)?;
    match cc {
        ConditionCode::Equal => {
            enc_brbs(out, 1, 1); // BREQ +1 (skip ldi XL, 0)
            enc_ldi(out, REG_XL, 0)?;
        }
        ConditionCode::NotEqual => {
            enc_brbc(out, 1, 1); // BRNE +1
            enc_ldi(out, REG_XL, 0)?;
        }
        ConditionCode::Below => {
            enc_brbs(out, 0, 1); // BRLO +1
            enc_ldi(out, REG_XL, 0)?;
        }
        ConditionCode::AboveOrEqual => {
            enc_brbc(out, 0, 1); // BRSH +1
            enc_ldi(out, REG_XL, 0)?;
        }
        ConditionCode::LessThan => {
            enc_brbs(out, 4, 1); // BRLT +1 (S=1)
            enc_ldi(out, REG_XL, 0)?;
        }
        ConditionCode::GreaterOrEqual => {
            enc_brbc(out, 4, 1); // BRGE +1 (S=0)
            enc_ldi(out, REG_XL, 0)?;
        }
        ConditionCode::LessOrEqual => {
            // true if Z=1 or S=1:
            //   breq +2 (keep XL=1)
            //   brlt +1 (keep XL=1)
            //   ldi XL, 0
            enc_brbs(out, 1, 2);
            enc_brbs(out, 4, 1);
            enc_ldi(out, REG_XL, 0)?;
        }
        ConditionCode::GreaterThan => {
            // true if Z=0 and S=0:
            //   breq +1 (clear XL=0)
            //   brge +1 (keep XL=1)
            //   ldi XL, 0
            enc_brbs(out, 1, 1);
            enc_brbc(out, 4, 1);
            enc_ldi(out, REG_XL, 0)?;
        }
        ConditionCode::BelowOrEqual => {
            // true if Z=1 or C=1:
            enc_brbs(out, 1, 2);
            enc_brbs(out, 0, 1);
            enc_ldi(out, REG_XL, 0)?;
        }
        ConditionCode::Above => {
            // true if Z=0 and C=0:
            enc_brbs(out, 1, 1);
            enc_brbc(out, 0, 1);
            enc_ldi(out, REG_XL, 0)?;
        }
        _ => {
            return Err(CodegenError::new(
                "avr",
                format!("unsupported AVR condition code {cc:?}"),
            ));
        }
    }
    enc_mov(out, dst_pair, REG_XL);
    enc_mov(out, dst_pair + 1, REG_ZERO);
    Ok(())
}

enum BranchPatchKind {
    /// 12-bit `RJMP` at `offset`, patching word-relative displacement.
    Rjmp { offset: usize, target: String },
}

pub struct AvrBackend {
    target: TargetDescriptor,
    capabilities: TargetCapabilities,
}

impl AvrBackend {
    pub fn new(target: TargetDescriptor) -> Self {
        let capabilities = TargetCapabilities::for_architecture(&target.architecture);
        Self {
            target,
            capabilities,
        }
    }

    pub fn generate_function_with_relocations(
        &mut self,
        func: &MachineFunction,
    ) -> Result<(Vec<u8>, Vec<AdobRelocation>), CodegenError> {
        // 1. Validate that no floating-point or SIMD instructions exist.
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
                        "avr",
                        format!(
                            "ATmega328P (avr-unknown-unknown) has no hardware floating-point unit in `{}`",
                            func.name
                        ),
                    )
                    .with_arch("avr")
                    .with_suggestion("Use integer arithmetic (i8/u8/i16/u16/i32/u32) on AVR."));
                }
            }
        }

        // 2. Expand 32-bit integers into 16-bit register pairs + carry chains.
        let mut allocated = func.clone();
        legalize_32bit_to_16bit_pairs(&mut allocated, "avr")?;

        // 3. Allocate registers over AVR callee-saved pairs r2..r17.
        let alloc_result = LinearScanAllocator::with_local_call_splitting(&AvrRegisterFile, false)
            .allocate(&mut allocated)
            .map_err(|err| {
                CodegenError::new("avr", format!("AVR register allocation failed: {err}"))
            })?;

        // 4. Expand ParallelMove using AVR-safe resolver (ids 16..31 are GPR, not XMM!).
        let resolver = ParallelMoveResolver::for_avr();
        let liveness = LivenessAnalysis::compute(&mut allocated);
        let mut inst_index = 0;
        for block in &mut allocated.blocks {
            let mut expanded = Vec::with_capacity(block.instructions.len());
            for inst in std::mem::take(&mut block.instructions) {
                match inst {
                    MachineInstruction::ParallelMove { moves } => {
                        let live = liveness
                            .live_after_inst(inst_index)
                            .into_iter()
                            .flatten()
                            .filter_map(|reg| match reg {
                                MachineRegister::Physical(reg) => Some(*reg),
                                _ => None,
                            })
                            .collect();
                        let resolved = resolver
                            .clone()
                            .with_live_registers(live)
                            .resolve_moves(&moves)?;
                        expanded.extend(resolved);
                    }
                    other => expanded.push(other),
                }
                inst_index += 1;
            }
            block.instructions = expanded;
        }

        // 5. Determine local/spill frame size from all StackSlot(off < 0) and Memory(Y, off < 0).
        let mut min_neg_off: i32 = -(allocated.stack_size as i32);
        for block in &allocated.blocks {
            for inst in &block.instructions {
                for op in [inst.defs(), inst.uses()].into_iter().flatten() {
                    let _ = op;
                }
                let check_op = |op: &MachineOperand, min_off: &mut i32| match op {
                    MachineOperand::StackSlot(off) if *off < 0 => {
                        *min_off = (*min_off).min(*off);
                    }
                    MachineOperand::Memory {
                        base: MachineRegister::Physical(p),
                        offset,
                        ..
                    } if p.0 == REG_YL && *offset < 0 => {
                        *min_off = (*min_off).min(*offset);
                    }
                    _ => {}
                };
                match inst {
                    MachineInstruction::Move { dst, src }
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
                        check_op(dst, &mut min_neg_off);
                        check_op(src, &mut min_neg_off);
                    }
                    MachineInstruction::Load { dst, src, .. }
                    | MachineInstruction::Store { dst, src, .. } => {
                        check_op(dst, &mut min_neg_off);
                        check_op(src, &mut min_neg_off);
                    }
                    MachineInstruction::Custom { operands, .. } => {
                        for op in operands {
                            check_op(op, &mut min_neg_off);
                        }
                    }
                    MachineInstruction::Compare { lhs, rhs }
                    | MachineInstruction::CompareWithBorrow { lhs, rhs }
                    | MachineInstruction::Test { lhs, rhs } => {
                        check_op(lhs, &mut min_neg_off);
                        check_op(rhs, &mut min_neg_off);
                    }
                    MachineInstruction::Neg { dst }
                    | MachineInstruction::Not { dst }
                    | MachineInstruction::SetCc { dst, .. }
                    | MachineInstruction::Pop { dst } => check_op(dst, &mut min_neg_off),
                    MachineInstruction::Push { src } => check_op(src, &mut min_neg_off),
                    _ => {}
                }
            }
        }
        let frame_bytes = (-min_neg_off).max(0);

        // Collect used callee-saved register pairs (sorted deterministically).
        let mut saved_pairs: Vec<u8> = alloc_result
            .used_callee_saved
            .iter()
            .map(|p| p.0)
            .filter(|&p| is_even_pair(p) && p <= 16)
            .collect();
        saved_pairs.sort_unstable();
        saved_pairs.dedup();

        // 6. Emit Prologue + Blocks + Epilogue.
        let mut out = Vec::new();
        let mut relocations = Vec::new();
        let mut label_offsets: HashMap<String, usize> = HashMap::new();
        let mut patches: Vec<BranchPatchKind> = Vec::new();

        // Prologue:
        //   push r28; push r29
        //   in r28, SPL; in r29, SPH
        //   Y -= frame_bytes; SP = Y
        //   push saved callee pairs (below Y+1, leaving Y-relative offsets invariant!)
        enc_push(&mut out, REG_YL);
        enc_push(&mut out, REG_YH);
        enc_in(&mut out, REG_YL, SPL_IO);
        enc_in(&mut out, REG_YH, SPH_IO);
        if frame_bytes > 0 {
            let fb16 = i16::try_from(frame_bytes).map_err(|_| {
                CodegenError::new(
                    "avr",
                    format!("stack frame {frame_bytes} exceeds 16-bit AVR RAM"),
                )
            })?;
            emit_add_imm16_y(&mut out, -fb16)?;
            emit_write_sp_from_y(&mut out);
        }
        for &p in &saved_pairs {
            enc_push(&mut out, p);
            enc_push(&mut out, p + 1);
        }

        let epilogue_label = format!("__avr_epilogue_{}", allocated.name);

        let resolve_y_off = |op: &MachineOperand| -> Option<i32> {
            match op {
                MachineOperand::StackSlot(off) => {
                    if *off < 0 {
                        Some(frame_bytes + 1 + *off)
                    } else {
                        Some(frame_bytes + *off)
                    }
                }
                MachineOperand::Memory {
                    base: MachineRegister::Physical(p),
                    offset,
                    index: None,
                } if p.0 == REG_YL => {
                    if *offset < 0 {
                        Some(frame_bytes + 1 + *offset)
                    } else {
                        Some(frame_bytes + *offset)
                    }
                }
                _ => None,
            }
        };

        for block in &allocated.blocks {
            label_offsets.insert(block.label.clone(), out.len());
            for inst in &block.instructions {
                match inst {
                    MachineInstruction::Nop => enc_nop(&mut out),

                    MachineInstruction::Return => {
                        let off = out.len();
                        emit_w(&mut out, 0xC000); // RJMP placeholder
                        patches.push(BranchPatchKind::Rjmp {
                            offset: off,
                            target: epilogue_label.clone(),
                        });
                    }

                    MachineInstruction::Move { dst, src } => {
                        if let Some(d_pair) = pair_reg(dst) {
                            match src {
                                MachineOperand::Register(MachineRegister::Physical(s))
                                    if is_even_pair(s.0) =>
                                {
                                    emit_mov16(&mut out, d_pair, s.0)?;
                                }
                                MachineOperand::Register(MachineRegister::Physical(s))
                                    if s.0 == REG_YL =>
                                {
                                    enc_movw(&mut out, d_pair, REG_YL)?;
                                }
                                MachineOperand::Immediate(imm) => {
                                    emit_load_imm16(&mut out, d_pair, imm16(*imm))?;
                                }
                                MachineOperand::Symbol(sym) => {
                                    if d_pair >= 16 {
                                        let lo_off = out.len();
                                        enc_ldi(&mut out, d_pair, 0)?;
                                        relocations.push(AdobRelocation::new(
                                            lo_off as u64,
                                            0,
                                            sym.clone(),
                                            RelocationKind::Avr_Lo8,
                                            0,
                                        ));
                                        let hi_off = out.len();
                                        enc_ldi(&mut out, d_pair + 1, 0)?;
                                        relocations.push(AdobRelocation::new(
                                            hi_off as u64,
                                            0,
                                            sym.clone(),
                                            RelocationKind::Avr_Hi8,
                                            0,
                                        ));
                                    } else {
                                        let lo_off = out.len();
                                        enc_ldi(&mut out, REG_XL, 0)?;
                                        relocations.push(AdobRelocation::new(
                                            lo_off as u64,
                                            0,
                                            sym.clone(),
                                            RelocationKind::Avr_Lo8,
                                            0,
                                        ));
                                        let hi_off = out.len();
                                        enc_ldi(&mut out, REG_XH, 0)?;
                                        relocations.push(AdobRelocation::new(
                                            hi_off as u64,
                                            0,
                                            sym.clone(),
                                            RelocationKind::Avr_Hi8,
                                            0,
                                        ));
                                        enc_movw(&mut out, d_pair, REG_XL)?;
                                    }
                                }
                                _ if resolve_y_off(src).is_some() => {
                                    let y_off = resolve_y_off(src).unwrap();
                                    emit_load_y_rel(&mut out, d_pair, y_off, 2)?;
                                }
                                _ => {
                                    return Err(unsupported_operand_form("avr", &allocated, inst));
                                }
                            }
                        } else if let Some(y_off) = resolve_y_off(dst) {
                            let s_pair = match src {
                                MachineOperand::Register(MachineRegister::Physical(s))
                                    if is_even_pair(s.0) =>
                                {
                                    s.0
                                }
                                MachineOperand::Immediate(imm) => {
                                    emit_load_imm16(&mut out, REG_XL, imm16(*imm))?;
                                    REG_XL
                                }
                                MachineOperand::Symbol(sym) => {
                                    let lo_off = out.len();
                                    enc_ldi(&mut out, REG_XL, 0)?;
                                    relocations.push(AdobRelocation::new(
                                        lo_off as u64,
                                        0,
                                        sym.clone(),
                                        RelocationKind::Avr_Lo8,
                                        0,
                                    ));
                                    let hi_off = out.len();
                                    enc_ldi(&mut out, REG_XH, 0)?;
                                    relocations.push(AdobRelocation::new(
                                        hi_off as u64,
                                        0,
                                        sym.clone(),
                                        RelocationKind::Avr_Hi8,
                                        0,
                                    ));
                                    REG_XL
                                }
                                _ if resolve_y_off(src).is_some() => {
                                    let src_y = resolve_y_off(src).expect("checked");
                                    emit_load_y_rel(&mut out, REG_XL, src_y, 2)?;
                                    REG_XL
                                }
                                _ => {
                                    return Err(unsupported_operand_form("avr", &allocated, inst));
                                }
                            };
                            emit_store_y_rel(&mut out, y_off, s_pair, 2)?;
                        } else {
                            return Err(unsupported_operand_form("avr", &allocated, inst));
                        }
                    }

                    MachineInstruction::Load { dst, src, size } => {
                        if !matches!(size, 1 | 2) {
                            return Err(CodegenError::new(
                                "avr",
                                format!(
                                    "AVR Load size {size} in `{}` must be 1 or 2 bytes",
                                    allocated.name
                                ),
                            ));
                        }
                        let d_pair = pair_reg(dst).unwrap_or(REG_XL);
                        if let Some(y_off) = resolve_y_off(src) {
                            emit_load_y_rel(&mut out, d_pair, y_off, *size)?;
                        } else if let MachineOperand::Memory {
                            base,
                            offset,
                            index,
                        } = src
                        {
                            emit_compute_mem_addr_z(&mut out, *base, *offset, *index, frame_bytes)?;
                            enc_ld_z(&mut out, d_pair);
                            if *size == 1 {
                                enc_mov(&mut out, d_pair + 1, REG_ZERO);
                            } else {
                                enc_ldd(&mut out, d_pair + 1, false, 1)?;
                            }
                        } else {
                            return Err(unsupported_operand_form("avr", &allocated, inst));
                        }
                        if pair_reg(dst).is_none() {
                            if let Some(y_off) = resolve_y_off(dst) {
                                emit_store_y_rel(&mut out, y_off, REG_XL, *size)?;
                            } else {
                                return Err(unsupported_operand_form("avr", &allocated, inst));
                            }
                        }
                    }

                    MachineInstruction::Store { dst, src, size } => {
                        if !matches!(size, 1 | 2) {
                            return Err(CodegenError::new(
                                "avr",
                                format!(
                                    "AVR Store size {size} in `{}` must be 1 or 2 bytes",
                                    allocated.name
                                ),
                            ));
                        }
                        let s_pair = match src {
                            MachineOperand::Register(MachineRegister::Physical(p))
                                if is_even_pair(p.0) =>
                            {
                                p.0
                            }
                            MachineOperand::Immediate(imm) => {
                                emit_load_imm16(&mut out, REG_XL, imm16(*imm))?;
                                REG_XL
                            }
                            _ if resolve_y_off(src).is_some() => {
                                let y_off = resolve_y_off(src).expect("checked");
                                emit_load_y_rel(&mut out, REG_XL, y_off, *size)?;
                                REG_XL
                            }
                            _ => {
                                return Err(unsupported_operand_form("avr", &allocated, inst));
                            }
                        };
                        if let Some(y_off) = resolve_y_off(dst) {
                            emit_store_y_rel(&mut out, y_off, s_pair, *size)?;
                        } else if let MachineOperand::Memory {
                            base,
                            offset,
                            index,
                        } = dst
                        {
                            emit_compute_mem_addr_z(&mut out, *base, *offset, *index, frame_bytes)?;
                            enc_st_z(&mut out, s_pair);
                            if *size > 1 {
                                enc_std(&mut out, false, 1, s_pair + 1)?;
                            }
                        } else {
                            return Err(unsupported_operand_form("avr", &allocated, inst));
                        }
                    }

                    MachineInstruction::Add { dst, src } => {
                        if let MachineOperand::Register(MachineRegister::Physical(p)) = dst
                            && p.0 == PSEUDO_SP
                        {
                            let delta = match src {
                                MachineOperand::Immediate(imm) => {
                                    i16::try_from(*imm).map_err(|_| {
                                        CodegenError::new("avr", "SP add delta overflows 16 bits")
                                    })?
                                }
                                _ => {
                                    return Err(unsupported_operand_form("avr", &allocated, inst));
                                }
                            };
                            enc_in(&mut out, REG_ZL, SPL_IO);
                            enc_in(&mut out, REG_ZH, SPH_IO);
                            emit_add_imm16_z(&mut out, delta)?;
                            enc_in(&mut out, REG_TMP, SREG_IO);
                            enc_cli(&mut out);
                            enc_out(&mut out, SPH_IO, REG_ZH);
                            enc_out(&mut out, SREG_IO, REG_TMP);
                            enc_out(&mut out, SPL_IO, REG_ZL);
                            continue;
                        }
                        let d_pair = pair_reg(dst)
                            .ok_or_else(|| unsupported_operand_form("avr", &allocated, inst))?;
                        let s_pair = match src {
                            MachineOperand::Register(MachineRegister::Physical(s))
                                if is_even_pair(s.0) =>
                            {
                                s.0
                            }
                            MachineOperand::Immediate(imm) => {
                                emit_load_imm16(&mut out, REG_XL, imm16(*imm))?;
                                REG_XL
                            }
                            _ => return Err(unsupported_operand_form("avr", &allocated, inst)),
                        };
                        enc_add(&mut out, d_pair, s_pair);
                        enc_adc(&mut out, d_pair + 1, s_pair + 1);
                    }

                    MachineInstruction::AddWithCarry { dst, src } => {
                        let d_pair = pair_reg(dst)
                            .ok_or_else(|| unsupported_operand_form("avr", &allocated, inst))?;
                        let s_pair = match src {
                            MachineOperand::Register(MachineRegister::Physical(s))
                                if is_even_pair(s.0) =>
                            {
                                s.0
                            }
                            MachineOperand::Immediate(imm) => {
                                // LDI and MOVW do not alter SREG carry flag!
                                emit_load_imm16(&mut out, REG_XL, imm16(*imm))?;
                                REG_XL
                            }
                            _ => return Err(unsupported_operand_form("avr", &allocated, inst)),
                        };
                        enc_adc(&mut out, d_pair, s_pair);
                        enc_adc(&mut out, d_pair + 1, s_pair + 1);
                    }

                    MachineInstruction::Sub { dst, src } => {
                        if let MachineOperand::Register(MachineRegister::Physical(p)) = dst
                            && p.0 == PSEUDO_SP
                        {
                            let delta = match src {
                                MachineOperand::Immediate(imm) => {
                                    i16::try_from(*imm).map_err(|_| {
                                        CodegenError::new("avr", "SP sub delta overflows 16 bits")
                                    })?
                                }
                                _ => {
                                    return Err(unsupported_operand_form("avr", &allocated, inst));
                                }
                            };
                            enc_in(&mut out, REG_ZL, SPL_IO);
                            enc_in(&mut out, REG_ZH, SPH_IO);
                            emit_add_imm16_z(&mut out, -delta)?;
                            enc_in(&mut out, REG_TMP, SREG_IO);
                            enc_cli(&mut out);
                            enc_out(&mut out, SPH_IO, REG_ZH);
                            enc_out(&mut out, SREG_IO, REG_TMP);
                            enc_out(&mut out, SPL_IO, REG_ZL);
                            continue;
                        }
                        let d_pair = pair_reg(dst)
                            .ok_or_else(|| unsupported_operand_form("avr", &allocated, inst))?;
                        let s_pair = match src {
                            MachineOperand::Register(MachineRegister::Physical(s))
                                if is_even_pair(s.0) =>
                            {
                                s.0
                            }
                            MachineOperand::Immediate(imm) => {
                                emit_load_imm16(&mut out, REG_XL, imm16(*imm))?;
                                REG_XL
                            }
                            _ => return Err(unsupported_operand_form("avr", &allocated, inst)),
                        };
                        enc_sub(&mut out, d_pair, s_pair);
                        enc_sbc(&mut out, d_pair + 1, s_pair + 1);
                    }

                    MachineInstruction::SubWithBorrow { dst, src } => {
                        let d_pair = pair_reg(dst)
                            .ok_or_else(|| unsupported_operand_form("avr", &allocated, inst))?;
                        let s_pair = match src {
                            MachineOperand::Register(MachineRegister::Physical(s))
                                if is_even_pair(s.0) =>
                            {
                                s.0
                            }
                            MachineOperand::Immediate(imm) => {
                                emit_load_imm16(&mut out, REG_XL, imm16(*imm))?;
                                REG_XL
                            }
                            _ => return Err(unsupported_operand_form("avr", &allocated, inst)),
                        };
                        enc_sbc(&mut out, d_pair, s_pair);
                        enc_sbc(&mut out, d_pair + 1, s_pair + 1);
                    }

                    MachineInstruction::And { dst, src }
                    | MachineInstruction::Or { dst, src }
                    | MachineInstruction::Xor { dst, src } => {
                        let d_pair = pair_reg(dst)
                            .ok_or_else(|| unsupported_operand_form("avr", &allocated, inst))?;
                        let s_pair = match src {
                            MachineOperand::Register(MachineRegister::Physical(s))
                                if is_even_pair(s.0) =>
                            {
                                s.0
                            }
                            MachineOperand::Immediate(imm) => {
                                emit_load_imm16(&mut out, REG_XL, imm16(*imm))?;
                                REG_XL
                            }
                            _ => return Err(unsupported_operand_form("avr", &allocated, inst)),
                        };
                        match inst {
                            MachineInstruction::And { .. } => {
                                enc_and(&mut out, d_pair, s_pair);
                                enc_and(&mut out, d_pair + 1, s_pair + 1);
                            }
                            MachineInstruction::Or { .. } => {
                                enc_or(&mut out, d_pair, s_pair);
                                enc_or(&mut out, d_pair + 1, s_pair + 1);
                            }
                            MachineInstruction::Xor { .. } => {
                                enc_eor(&mut out, d_pair, s_pair);
                                enc_eor(&mut out, d_pair + 1, s_pair + 1);
                            }
                            _ => unreachable!(),
                        }
                    }

                    MachineInstruction::Not { dst } => {
                        let d_pair = pair_reg(dst)
                            .ok_or_else(|| unsupported_operand_form("avr", &allocated, inst))?;
                        enc_com(&mut out, d_pair);
                        enc_com(&mut out, d_pair + 1);
                    }

                    MachineInstruction::Neg { dst } => {
                        let d_pair = pair_reg(dst)
                            .ok_or_else(|| unsupported_operand_form("avr", &allocated, inst))?;
                        // Two's complement of 16-bit pair: COM hi; NEG lo; SBCI hi, -1?
                        // Actually: COM lo; COM hi; LDI XL, 1; ADD lo, XL; ADC hi, r1
                        // (Wait: simpler: NEG lo; SBC hi, hi? No! Correct AVR 16-bit neg:)
                        // com hi; neg lo; sbci hi, 0xff (if hi>=16) or com hi; com lo; adiw/add 1!
                        enc_com(&mut out, d_pair);
                        enc_com(&mut out, d_pair + 1);
                        enc_ldi(&mut out, REG_XL, 1)?;
                        enc_add(&mut out, d_pair, REG_XL);
                        enc_adc(&mut out, d_pair + 1, REG_ZERO);
                    }

                    MachineInstruction::Mul { dst, src } => {
                        let d_pair = pair_reg(dst)
                            .ok_or_else(|| unsupported_operand_form("avr", &allocated, inst))?;
                        let s_pair = match src {
                            MachineOperand::Register(MachineRegister::Physical(s))
                                if is_even_pair(s.0) =>
                            {
                                s.0
                            }
                            MachineOperand::Immediate(imm) => {
                                emit_load_imm16(&mut out, REG_ZL, imm16(*imm))?;
                                REG_ZL
                            }
                            _ => return Err(unsupported_operand_form("avr", &allocated, inst)),
                        };
                        // 16x16 -> 16 using ATmega328P hardware `MUL`:
                        //   mul d_lo, s_lo -> X = r1:r0
                        //   mul d_hi, s_lo -> XH += r0
                        //   mul d_lo, s_hi -> XH += r0
                        //   eor r1, r1     (restore zero reg!)
                        //   movw d, X
                        enc_mul(&mut out, d_pair, s_pair);
                        enc_movw(&mut out, REG_XL, REG_TMP)?;
                        enc_mul(&mut out, d_pair + 1, s_pair);
                        enc_add(&mut out, REG_XH, REG_TMP);
                        enc_mul(&mut out, d_pair, s_pair + 1);
                        enc_add(&mut out, REG_XH, REG_TMP);
                        enc_eor(&mut out, REG_ZERO, REG_ZERO);
                        enc_movw(&mut out, d_pair, REG_XL)?;
                    }

                    MachineInstruction::Div { dst, src } | MachineInstruction::Mod { dst, src } => {
                        let d_pair = pair_reg(dst)
                            .ok_or_else(|| unsupported_operand_form("avr", &allocated, inst))?;
                        let s_pair = match src {
                            MachineOperand::Register(MachineRegister::Physical(s))
                                if is_even_pair(s.0) =>
                            {
                                s.0
                            }
                            MachineOperand::Immediate(imm) => {
                                emit_load_imm16(&mut out, REG_XL, imm16(*imm))?;
                                REG_XL
                            }
                            _ => return Err(unsupported_operand_form("avr", &allocated, inst)),
                        };
                        // Stage into r25:r24 (dividend) and r23:r22 (divisor), avoiding clobber if d==22 or s==24.
                        if d_pair == 22 && s_pair == 24 {
                            enc_movw(&mut out, REG_XL, 24)?;
                            enc_movw(&mut out, 24, 22)?;
                            enc_movw(&mut out, 22, REG_XL)?;
                        } else if s_pair == 24 {
                            enc_movw(&mut out, 22, s_pair)?;
                            enc_movw(&mut out, 24, d_pair)?;
                        } else {
                            enc_movw(&mut out, 24, d_pair)?;
                            enc_movw(&mut out, 22, s_pair)?;
                        }
                        let sym = if matches!(inst, MachineInstruction::Div { .. }) {
                            "__avr_div16"
                        } else {
                            "__avr_mod16"
                        };
                        let call_off = enc_call_placeholder(&mut out);
                        relocations.push(AdobRelocation::new(
                            call_off as u64,
                            0,
                            sym.to_string(),
                            RelocationKind::Avr_Call22,
                            0,
                        ));
                        enc_movw(&mut out, d_pair, 24)?;
                    }

                    MachineInstruction::Shl { dst, src }
                    | MachineInstruction::Shr { dst, src }
                    | MachineInstruction::Sar { dst, src } => {
                        let d_pair = pair_reg(dst)
                            .ok_or_else(|| unsupported_operand_form("avr", &allocated, inst))?;
                        match src {
                            MachineOperand::Immediate(imm) => {
                                let cnt = (*imm).clamp(0, 16) as u8;
                                if cnt >= 16 {
                                    if matches!(inst, MachineInstruction::Sar { .. }) {
                                        // Fill with sign bit: lsl d_hi; sbc d_lo, d_lo; mov d_hi, d_lo
                                        enc_lsl(&mut out, d_pair + 1);
                                        enc_sbc(&mut out, d_pair, d_pair);
                                        enc_mov(&mut out, d_pair + 1, d_pair);
                                    } else {
                                        enc_mov(&mut out, d_pair, REG_ZERO);
                                        enc_mov(&mut out, d_pair + 1, REG_ZERO);
                                    }
                                } else {
                                    for _ in 0..cnt {
                                        match inst {
                                            MachineInstruction::Shl { .. } => {
                                                enc_lsl(&mut out, d_pair);
                                                enc_rol(&mut out, d_pair + 1);
                                            }
                                            MachineInstruction::Shr { .. } => {
                                                enc_lsr(&mut out, d_pair + 1);
                                                enc_ror(&mut out, d_pair);
                                            }
                                            MachineInstruction::Sar { .. } => {
                                                enc_asr(&mut out, d_pair + 1);
                                                enc_ror(&mut out, d_pair);
                                            }
                                            _ => unreachable!(),
                                        }
                                    }
                                }
                            }
                            MachineOperand::Register(MachineRegister::Physical(s))
                                if is_even_pair(s.0) =>
                            {
                                // Loop `cnt = s_lo & 0x1F`:
                                //   mov XL, s_lo
                                //   andi XL, 0x1F  (no, if >= 16 handle nicely, or loop up to XL)
                                //   rjmp +3 (to test)
                                // loop:
                                //   shift 1 bit (2 words)
                                //   dec XL (1 word)
                                // test:
                                //   tst XL -> cp XL, r1 (1 word)
                                //   brne -4 (3 words back + test) -> loop!
                                enc_mov(&mut out, REG_XL, s.0);
                                enc_rjmp(&mut out, 3)?;
                                match inst {
                                    MachineInstruction::Shl { .. } => {
                                        enc_lsl(&mut out, d_pair);
                                        enc_rol(&mut out, d_pair + 1);
                                    }
                                    MachineInstruction::Shr { .. } => {
                                        enc_lsr(&mut out, d_pair + 1);
                                        enc_ror(&mut out, d_pair);
                                    }
                                    MachineInstruction::Sar { .. } => {
                                        enc_asr(&mut out, d_pair + 1);
                                        enc_ror(&mut out, d_pair);
                                    }
                                    _ => unreachable!(),
                                }
                                enc_dec(&mut out, REG_XL);
                                enc_cp(&mut out, REG_XL, REG_ZERO);
                                enc_brbc(&mut out, 1, -4); // BRNE back to shift (4 words back)
                            }
                            _ => return Err(unsupported_operand_form("avr", &allocated, inst)),
                        }
                    }

                    MachineInstruction::Compare { lhs, rhs } => {
                        let l_pair = match lhs {
                            MachineOperand::Register(MachineRegister::Physical(l))
                                if is_even_pair(l.0) =>
                            {
                                l.0
                            }
                            MachineOperand::Immediate(imm) => {
                                emit_load_imm16(&mut out, REG_ZL, imm16(*imm))?;
                                REG_ZL
                            }
                            _ => return Err(unsupported_operand_form("avr", &allocated, inst)),
                        };
                        let r_pair = match rhs {
                            MachineOperand::Register(MachineRegister::Physical(r))
                                if is_even_pair(r.0) =>
                            {
                                r.0
                            }
                            MachineOperand::Immediate(imm) => {
                                emit_load_imm16(&mut out, REG_XL, imm16(*imm))?;
                                REG_XL
                            }
                            _ => return Err(unsupported_operand_form("avr", &allocated, inst)),
                        };
                        enc_cp(&mut out, l_pair, r_pair);
                        enc_cpc(&mut out, l_pair + 1, r_pair + 1);
                    }

                    MachineInstruction::CompareWithBorrow { lhs, rhs } => {
                        let l_pair = match lhs {
                            MachineOperand::Register(MachineRegister::Physical(l))
                                if is_even_pair(l.0) =>
                            {
                                l.0
                            }
                            MachineOperand::Immediate(imm) => {
                                // LDI and MOVW do not alter SREG flags!
                                emit_load_imm16(&mut out, REG_ZL, imm16(*imm))?;
                                REG_ZL
                            }
                            _ => return Err(unsupported_operand_form("avr", &allocated, inst)),
                        };
                        let r_pair = match rhs {
                            MachineOperand::Register(MachineRegister::Physical(r))
                                if is_even_pair(r.0) =>
                            {
                                r.0
                            }
                            MachineOperand::Immediate(imm) => {
                                emit_load_imm16(&mut out, REG_XL, imm16(*imm))?;
                                REG_XL
                            }
                            _ => return Err(unsupported_operand_form("avr", &allocated, inst)),
                        };
                        enc_cpc(&mut out, l_pair, r_pair);
                        enc_cpc(&mut out, l_pair + 1, r_pair + 1);
                    }

                    MachineInstruction::Test { lhs, rhs } => {
                        let l_pair = pair_reg(lhs)
                            .ok_or_else(|| unsupported_operand_form("avr", &allocated, inst))?;
                        let r_pair = match rhs {
                            MachineOperand::Register(MachineRegister::Physical(r))
                                if is_even_pair(r.0) =>
                            {
                                r.0
                            }
                            MachineOperand::Immediate(imm) => {
                                emit_load_imm16(&mut out, REG_XL, imm16(*imm))?;
                                REG_XL
                            }
                            _ => return Err(unsupported_operand_form("avr", &allocated, inst)),
                        };
                        enc_mov(&mut out, REG_TMP, l_pair);
                        enc_and(&mut out, REG_TMP, r_pair);
                        enc_mov(&mut out, REG_ZL, l_pair + 1);
                        enc_and(&mut out, REG_ZL, r_pair + 1);
                        enc_or(&mut out, REG_TMP, REG_ZL); // Z=1 iff (lhs & rhs) == 0
                    }

                    MachineInstruction::SetCc { dst, cc } => {
                        let d_pair = pair_reg(dst)
                            .ok_or_else(|| unsupported_operand_form("avr", &allocated, inst))?;
                        emit_setcc(&mut out, d_pair, *cc)?;
                    }

                    MachineInstruction::Branch { target } => {
                        let off = out.len();
                        emit_w(&mut out, 0xC000); // RJMP placeholder
                        patches.push(BranchPatchKind::Rjmp {
                            offset: off,
                            target: target.clone(),
                        });
                    }

                    MachineInstruction::BranchCc { cc, target } => {
                        // Use short inverted conditional skip + 12-bit RJMP (safe for arbitrary intra-function distances).
                        match cc {
                            ConditionCode::Equal => {
                                enc_brbc(&mut out, 1, 1); // BRNE +1 (skip RJMP)
                            }
                            ConditionCode::NotEqual => {
                                enc_brbs(&mut out, 1, 1); // BREQ +1
                            }
                            ConditionCode::Below => {
                                enc_brbc(&mut out, 0, 1); // BRSH +1
                            }
                            ConditionCode::AboveOrEqual => {
                                enc_brbs(&mut out, 0, 1); // BRLO +1
                            }
                            ConditionCode::LessThan => {
                                enc_brbc(&mut out, 4, 1); // BRGE +1
                            }
                            ConditionCode::GreaterOrEqual => {
                                enc_brbs(&mut out, 4, 1); // BRLT +1
                            }
                            ConditionCode::LessOrEqual => {
                                // branch if Z=1 or S=1:
                                //   breq +1 (jump to RJMP!)
                                //   brge +1 (skip RJMP)
                                //   rjmp target
                                enc_brbs(&mut out, 1, 1);
                                enc_brbc(&mut out, 4, 1);
                            }
                            ConditionCode::GreaterThan => {
                                // branch if Z=0 and S=0:
                                //   breq +2 (skip BRGE and RJMP -> don't jump)
                                //   brge +1? No! If Z=0 and S=1 (lt): skip RJMP!
                                //   breq +2 (skip BRLT + RJMP)
                                //   brlt +1 (skip RJMP)
                                //   rjmp target
                                enc_brbs(&mut out, 1, 2);
                                enc_brbs(&mut out, 4, 1);
                            }
                            ConditionCode::BelowOrEqual => {
                                enc_brbs(&mut out, 1, 1);
                                enc_brbc(&mut out, 0, 1);
                            }
                            ConditionCode::Above => {
                                enc_brbs(&mut out, 1, 2);
                                enc_brbs(&mut out, 0, 1);
                            }
                            _ => {
                                return Err(CodegenError::new(
                                    "avr",
                                    format!("unsupported AVR branch condition {cc:?}"),
                                ));
                            }
                        }
                        let off = out.len();
                        emit_w(&mut out, 0xC000);
                        patches.push(BranchPatchKind::Rjmp {
                            offset: off,
                            target: target.clone(),
                        });
                    }

                    MachineInstruction::Call { target, .. } => match target {
                        MachineOperand::Symbol(sym) => {
                            let call_off = enc_call_placeholder(&mut out);
                            relocations.push(AdobRelocation::new(
                                call_off as u64,
                                0,
                                sym.clone(),
                                RelocationKind::Avr_Call22,
                                0,
                            ));
                        }
                        MachineOperand::Register(MachineRegister::Physical(p))
                            if is_even_pair(p.0) =>
                        {
                            enc_movw(&mut out, REG_ZL, p.0)?;
                            enc_icall(&mut out);
                        }
                        _ => return Err(unsupported_operand_form("avr", &allocated, inst)),
                    },

                    MachineInstruction::Push { src } => {
                        let s_pair = match src {
                            MachineOperand::Register(MachineRegister::Physical(p))
                                if is_even_pair(p.0) =>
                            {
                                p.0
                            }
                            MachineOperand::Immediate(imm) => {
                                emit_load_imm16(&mut out, REG_XL, imm16(*imm))?;
                                REG_XL
                            }
                            _ => return Err(unsupported_operand_form("avr", &allocated, inst)),
                        };
                        enc_push(&mut out, s_pair + 1);
                        enc_push(&mut out, s_pair);
                    }

                    MachineInstruction::Pop { dst } => {
                        let d_pair = pair_reg(dst)
                            .ok_or_else(|| unsupported_operand_form("avr", &allocated, inst))?;
                        enc_pop(&mut out, d_pair);
                        enc_pop(&mut out, d_pair + 1);
                    }

                    MachineInstruction::Barrier => enc_nop(&mut out),

                    MachineInstruction::Custom { name, operands }
                        if name == "avr_lea_slot" && operands.len() == 2 =>
                    {
                        let d_pair = pair_reg(&operands[0])
                            .ok_or_else(|| unsupported_operand_form("avr", &allocated, inst))?;
                        let y_off = resolve_y_off(&operands[1])
                            .ok_or_else(|| unsupported_operand_form("avr", &allocated, inst))?;
                        let d16 = i16::try_from(y_off).map_err(|_| {
                            CodegenError::new(
                                "avr",
                                format!("avr_lea_slot Y offset {y_off} overflows 16 bits"),
                            )
                        })?;
                        enc_movw(&mut out, REG_ZL, REG_YL)?;
                        emit_add_imm16_z(&mut out, d16)?;
                        enc_movw(&mut out, d_pair, REG_ZL)?;
                    }

                    _ => return Err(unsupported_instruction("avr", &allocated, inst)),
                }
            }
        }

        // Epilogue:
        label_offsets.insert(epilogue_label.clone(), out.len());
        for &p in saved_pairs.iter().rev() {
            enc_pop(&mut out, p + 1);
            enc_pop(&mut out, p);
        }
        if frame_bytes > 0 {
            let fb16 = i16::try_from(frame_bytes).map_err(|_| {
                CodegenError::new(
                    "avr",
                    format!("stack frame {frame_bytes} exceeds 16-bit AVR RAM"),
                )
            })?;
            emit_add_imm16_y(&mut out, fb16)?;
            emit_write_sp_from_y(&mut out);
        }
        enc_pop(&mut out, REG_YH);
        enc_pop(&mut out, REG_YL);
        enc_ret(&mut out);

        // 7. Patch intra-function RJMP displacements (word offset relative to PC+1).
        for patch in patches {
            match patch {
                BranchPatchKind::Rjmp { offset, target } => {
                    let target_off = *label_offsets.get(&target).ok_or_else(|| {
                        CodegenError::new(
                            "avr",
                            format!("missing branch target `{target}` in `{}`", allocated.name),
                        )
                    })?;
                    let pc_after = (offset + 2) as isize;
                    let word_disp = ((target_off as isize) - pc_after) / 2;
                    let disp12 = i16::try_from(word_disp)
                        .ok()
                        .filter(|d| (-2048..=2047).contains(d))
                        .ok_or_else(|| {
                            CodegenError::new(
                                "avr",
                                format!(
                                    "AVR intra-function RJMP to `{target}` exceeds +/-2047 words ({word_disp})"
                                ),
                            )
                        })?;
                    let word = 0xC000u16 | ((disp12 as u16) & 0x0FFF);
                    let bytes = word.to_le_bytes();
                    out[offset] = bytes[0];
                    out[offset + 1] = bytes[1];
                }
            }
        }

        Ok((out, relocations))
    }
}

impl CodegenBackend for AvrBackend {
    fn target(&self) -> &TargetDescriptor {
        &self.target
    }

    fn capabilities(&self) -> TargetCapabilities {
        self.capabilities.clone()
    }

    fn lower_module(&mut self, module: &NativeModule) -> Result<NativeModule, CodegenError> {
        let mut lowered = module.clone();
        // Strip desktop scope-tracking calls on bare-metal AVR.
        for func in &mut lowered.functions {
            for block in &mut func.blocks {
                let mut index = 0;
                while index < block.instructions.len() {
                    let pruned_scope_call = matches!(
                        &block.instructions[index],
                        MachineInstruction::Call {
                            target: MachineOperand::Symbol(name),
                            ..
                        } if matches!(
                            name.as_str(),
                            "adesh_rt_scope_enter"
                                | "adesh_rt_scope_exit"
                                | "adesh_rt_scope_enter_i686"
                                | "adesh_rt_scope_exit_i686"
                        )
                    );
                    if !pruned_scope_call {
                        index += 1;
                        continue;
                    }
                    let mut start = index;
                    if index >= 2
                        && matches!(
                            &block.instructions[index - 1],
                            MachineInstruction::ParallelMove { .. }
                        )
                        && matches!(
                            &block.instructions[index - 2],
                            MachineInstruction::Sub {
                                dst: MachineOperand::Register(MachineRegister::Physical(reg)),
                                src: MachineOperand::Immediate(_),
                            } if reg.0 == PSEUDO_SP
                        )
                    {
                        start = index - 2;
                    } else if index >= 1
                        && matches!(
                            &block.instructions[index - 1],
                            MachineInstruction::ParallelMove { .. }
                        )
                    {
                        start = index - 1;
                    }
                    let mut end = index + 1;
                    if end < block.instructions.len()
                        && matches!(
                            &block.instructions[end],
                            MachineInstruction::Add {
                                dst: MachineOperand::Register(MachineRegister::Physical(reg)),
                                src: MachineOperand::Immediate(_),
                            } if reg.0 == PSEUDO_SP
                        )
                    {
                        end += 1;
                    }
                    block.instructions.drain(start..end);
                    index = start;
                }
            }
        }
        let mut referenced = std::collections::HashSet::new();
        for func in &lowered.functions {
            for block in &func.blocks {
                for inst in &block.instructions {
                    if let MachineInstruction::Call {
                        target: MachineOperand::Symbol(name),
                        ..
                    } = inst
                    {
                        referenced.insert(name.clone());
                    }
                }
            }
        }
        lowered.imports.retain(|name| referenced.contains(name));
        Ok(lowered)
    }

    fn generate_function(&mut self, func: &MachineFunction) -> Result<Vec<u8>, CodegenError> {
        let (code, _) = self.generate_function_with_relocations(func)?;
        Ok(code)
    }

    fn emit_object(&mut self, module: &NativeModule) -> Result<AdobObject, CodegenError> {
        let mut obj = AdobObject::new(self.target.clone());

        // ATmega328P Harvard memory regions: 32 KiB Program Flash + 2 KiB SRAM.
        obj.memory_regions.push(MemoryRegion::new(
            ".flash",
            0x0000_0000,
            32 * 1024,
            MemoryPermissions::Rx,
        ));
        obj.memory_regions.push(MemoryRegion::new(
            ".sram",
            0x0080_0100,
            2 * 1024,
            MemoryPermissions::Rw,
        ));

        let mut text_bytes = Vec::new();
        let mut text_relocs = Vec::new();

        for func in &module.functions {
            if !text_bytes.len().is_multiple_of(2) {
                text_bytes.push(0);
            }
            let fn_offset = text_bytes.len() as u64;
            let (code, relocs) = self.generate_function_with_relocations(func)?;
            let fn_size = code.len() as u64;
            text_bytes.extend_from_slice(&code);

            for mut rel in relocs {
                rel.offset += fn_offset;
                text_relocs.push(rel);
            }

            let sym = AdobSymbol::new_defined(
                0,
                func.name.clone(),
                SymbolKind::Function,
                0,
                fn_offset,
                fn_size,
            )
            .with_binding(SymbolBinding::Global)
            .with_visibility(SymbolVisibility::Default);
            obj.add_symbol(sym);

            if func.is_exported {
                obj.add_export(func.name.clone());
            }
        }
        for name in &module.exports {
            obj.add_export(name.clone());
        }

        let mut text_sec = AdobSection::new(".text", SectionKind::Text)
            .with_flags(section_flags::READ | section_flags::EXECUTE | section_flags::ALLOC)
            .with_alignment(2)
            .with_memory_region(".flash")
            .with_data(text_bytes);
        text_sec.relocations = text_relocs;
        obj.add_section(text_sec);

        for imp in &module.imports {
            obj.add_import(imp.clone());
            obj.add_symbol(
                AdobSymbol::new_undefined(0, imp.clone(), SymbolKind::Import)
                    .with_binding(SymbolBinding::Global),
            );
        }

        if !module.string_pool.is_empty() {
            let rodata_index = obj.sections.len() as u32;
            let mut rodata = Vec::new();
            let mut offsets: HashMap<&str, u64> = HashMap::new();
            for (idx, s) in module.string_pool.iter().enumerate() {
                let off = *offsets.entry(s.as_str()).or_insert_with(|| {
                    let cur = rodata.len() as u64;
                    rodata.extend_from_slice(s.as_bytes());
                    rodata.push(0);
                    cur
                });
                obj.add_symbol(
                    AdobSymbol::new_defined(
                        0,
                        format!("__str_{idx}"),
                        SymbolKind::Object,
                        rodata_index,
                        off,
                        s.len() as u64 + 1,
                    )
                    .with_binding(SymbolBinding::Local),
                );
            }
            let rodata_sec = AdobSection::new(".rodata", SectionKind::Rodata)
                .with_flags(section_flags::READ | section_flags::ALLOC)
                .with_alignment(2)
                .with_memory_region(".flash")
                .with_data(rodata);
            obj.add_section(rodata_sec);
        }

        if !module.tls_sections.is_empty() {
            return Err(CodegenError::new(
                "avr",
                "thread-local storage (.tls) is not supported on bare-metal AVR ATmega328P",
            )
            .with_arch("avr"));
        }

        if !module.data_sections.is_empty() {
            let data_index = obj.sections.len() as u32;
            let mut data_bytes = Vec::new();
            for (name, init_bytes) in &module.data_sections {
                if !data_bytes.len().is_multiple_of(2) {
                    data_bytes.push(0);
                }
                let off = data_bytes.len() as u64;
                let size = init_bytes.len() as u64;
                data_bytes.extend_from_slice(init_bytes);
                obj.add_symbol(
                    AdobSymbol::new_defined(
                        0,
                        name.clone(),
                        SymbolKind::Object,
                        data_index,
                        off,
                        size,
                    )
                    .with_binding(SymbolBinding::Global),
                );
            }
            let data_sec = AdobSection::new(".data", SectionKind::Data)
                .with_flags(section_flags::READ | section_flags::WRITE | section_flags::ALLOC)
                .with_alignment(2)
                .with_memory_region(".sram")
                .with_data(data_bytes);
            obj.add_section(data_sec);
        }

        // Ensure compiler-synthesized AVR arithmetic helpers referenced via relocations
        // (e.g. `__avr_div16`, `__avr_mul32`) are declared as undefined symbols.
        let mut name_to_id: HashMap<String, u32> = obj
            .symbols
            .iter()
            .map(|symbol| (symbol.name.clone(), symbol.id))
            .collect();
        let needed_helpers: Vec<String> = obj
            .sections
            .iter()
            .flat_map(|s| s.relocations.iter().map(|r| r.symbol_name.clone()))
            .filter(|name| !name_to_id.contains_key(name))
            .collect();
        for helper in needed_helpers {
            name_to_id.entry(helper).or_insert_with_key(|helper| {
                obj.add_symbol(
                    AdobSymbol::new_undefined(0, helper.clone(), SymbolKind::Import)
                        .with_binding(SymbolBinding::Global),
                )
            });
        }
        for section in &mut obj.sections {
            for relocation in &mut section.relocations {
                if let Some(&id) = name_to_id.get(&relocation.symbol_name) {
                    relocation.symbol = id;
                } else {
                    return Err(CodegenError::new(
                        "avr",
                        format!("unresolved relocation symbol `{}`", relocation.symbol_name),
                    )
                    .with_arch("avr")
                    .with_instruction("Relocation"));
                }
            }
        }

        Ok(obj)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine_ir::VirtualRegister;

    fn target() -> TargetDescriptor {
        TargetDescriptor::from_triple("avr-unknown-unknown").expect("avr triple")
    }

    #[test]
    fn encodes_avr_16bit_add_and_return() {
        let mut backend = AvrBackend::new(target());
        let mut func = MachineFunction::new("add16");
        let a = func.alloc_vreg();
        let b = func.alloc_vreg();
        let bb = &mut func.blocks[0];
        bb.push(MachineInstruction::Move {
            dst: MachineOperand::reg(a.0),
            src: MachineOperand::phys(24),
        });
        bb.push(MachineInstruction::Move {
            dst: MachineOperand::reg(b.0),
            src: MachineOperand::phys(22),
        });
        bb.push(MachineInstruction::Add {
            dst: MachineOperand::reg(a.0),
            src: MachineOperand::reg(b.0),
        });
        bb.push(MachineInstruction::Move {
            dst: MachineOperand::phys(24),
            src: MachineOperand::reg(a.0),
        });
        bb.push(MachineInstruction::Return);

        let bytes = backend.generate_function(&func).expect("avr encode");
        assert!(!bytes.is_empty());
        assert_eq!(bytes.len() % 2, 0);
        // Ends with RET (0x9508 little endian = [0x08, 0x95])
        assert_eq!(&bytes[bytes.len() - 2..], &[0x08, 0x95]);
    }

    #[test]
    fn legalizes_32bit_add_to_carry_chain() {
        let mut backend = AvrBackend::new(target());
        let mut func = MachineFunction::new("add32");
        let a = func.alloc_vreg();
        let b = func.alloc_vreg();
        func.set_vreg_width(a, 4);
        func.set_vreg_width(b, 4);
        let bb = &mut func.blocks[0];
        bb.push(MachineInstruction::Move {
            dst: MachineOperand::reg(a.0),
            src: MachineOperand::Immediate(70000),
        });
        bb.push(MachineInstruction::Move {
            dst: MachineOperand::reg(b.0),
            src: MachineOperand::Immediate(30000),
        });
        bb.push(MachineInstruction::Add {
            dst: MachineOperand::reg(a.0),
            src: MachineOperand::reg(b.0),
        });
        bb.push(MachineInstruction::Move {
            dst: MachineOperand::phys(24),
            src: MachineOperand::reg(a.0),
        });
        bb.push(MachineInstruction::Return);

        let bytes = backend.generate_function(&func).expect("avr 32bit encode");
        assert!(!bytes.is_empty());
        assert_eq!(bytes.len() % 2, 0);
    }

    #[test]
    fn refuses_hardware_float_on_atmega328p() {
        let mut backend = AvrBackend::new(target());
        let mut func = MachineFunction::new("bad_fp");
        let f = VirtualRegister(func.alloc_fp_vreg().0);
        func.blocks[0].push(MachineInstruction::FAdd {
            dst: MachineOperand::reg(f.0),
            src: MachineOperand::reg(f.0),
            size: 8,
        });
        func.blocks[0].push(MachineInstruction::Return);
        let err = backend.generate_function(&func).unwrap_err();
        assert!(err.to_string().contains("floating-point"));
    }
}
