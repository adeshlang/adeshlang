//! Intel 8086 16-bit Real-Mode Native Codegen Backend.
//!
//! Targets real-mode 8086 (`i8086-unknown-dos`) with strict 8086 instruction
//! encodings only (no 80186 `push imm`/`leave`/`imul r,imm`/`shift imm`, no
//! 80386 `setcc`/`movzx`/`jcc rel16` encodings).
//!
//! Register model:
//! - General purpose: AX(0), CX(1), DX(2), BX(3), SP(4), BP(5), SI(6), DI(7)
//! - Allocatable callee-saved registers: BX(3), SI(6), DI(7)
//! - Scratch / mul-div / shift / return registers: AX(0), CX(1), DX(2)
//! - Segment registers: ES(0), CS(1), SS(2), DS(3)

#![forbid(unsafe_code)]

use crate::backend::CodegenBackend;
use crate::calling_convention::ParallelMoveResolver;
use crate::error::CodegenError;
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
use std::collections::{HashMap, HashSet};

/// 8086 16-bit segment registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentRegister {
    Es = 0,
    Cs = 1,
    Ss = 2,
    Ds = 3,
}

impl SegmentRegister {
    pub fn override_prefix(self) -> u8 {
        match self {
            SegmentRegister::Es => 0x26,
            SegmentRegister::Cs => 0x2E,
            SegmentRegister::Ss => 0x36,
            SegmentRegister::Ds => 0x3E,
        }
    }
}

static I8086_GPRS: [PhysicalRegister; 8] = [
    PhysicalRegister::gpr(0), // AX
    PhysicalRegister::gpr(1), // CX
    PhysicalRegister::gpr(2), // DX
    PhysicalRegister::gpr(3), // BX
    PhysicalRegister::gpr(4), // SP
    PhysicalRegister::gpr(5), // BP
    PhysicalRegister::gpr(6), // SI
    PhysicalRegister::gpr(7), // DI
];

// Keep AX, CX, DX as scratch/mul-div/shift/return registers so 16-bit
// `imul`/`idiv`/`shl cl`/calls never clobber live allocated variables.
static I8086_ALLOCATABLE: [PhysicalRegister; 3] = [
    PhysicalRegister::gpr(3), // BX
    PhysicalRegister::gpr(6), // SI
    PhysicalRegister::gpr(7), // DI
];
static I8086_CALLER_SAVED: [PhysicalRegister; 3] = [
    PhysicalRegister::gpr(0), // AX
    PhysicalRegister::gpr(1), // CX
    PhysicalRegister::gpr(2), // DX
];
static I8086_CALLEE_SAVED: [PhysicalRegister; 3] = [
    PhysicalRegister::gpr(3), // BX
    PhysicalRegister::gpr(6), // SI
    PhysicalRegister::gpr(7), // DI
];
static I8086_RESERVED: [PhysicalRegister; 2] = [
    PhysicalRegister::gpr(4), // SP
    PhysicalRegister::gpr(5), // BP
];

struct I8086RegisterFile;

impl RegisterFile for I8086RegisterFile {
    fn registers(&self) -> &[PhysicalRegister] {
        &I8086_GPRS
    }

    fn allocatable(&self) -> &[PhysicalRegister] {
        &I8086_ALLOCATABLE
    }

    fn caller_saved(&self) -> &[PhysicalRegister] {
        &I8086_CALLER_SAVED
    }

    fn callee_saved(&self) -> &[PhysicalRegister] {
        &I8086_CALLEE_SAVED
    }

    fn reserved(&self) -> &[PhysicalRegister] {
        &I8086_RESERVED
    }

    fn scratch_for_class(&self, class: RegisterClass) -> (PhysicalRegister, PhysicalRegister) {
        match class {
            RegisterClass::Gpr => (PhysicalRegister::gpr(0), PhysicalRegister::gpr(1)),
            RegisterClass::Float => (PhysicalRegister::xmm(0), PhysicalRegister::xmm(1)),
        }
    }

    fn scalar_slot_size(&self) -> u32 {
        2
    }
}

fn i8086_resolver() -> ParallelMoveResolver {
    let mut reserved: HashSet<PhysicalRegister> = (8..=u8::MAX).map(PhysicalRegister).collect();
    reserved.insert(PhysicalRegister(4)); // SP
    reserved.insert(PhysicalRegister(5)); // BP
    ParallelMoveResolver::new("i8086")
        .with_scratch_pool(vec![
            PhysicalRegister(0),
            PhysicalRegister(1),
            PhysicalRegister(2),
            PhysicalRegister(3),
            PhysicalRegister(6),
            PhysicalRegister(7),
        ])
        .with_reserved_registers(reserved)
        .with_default_stack_base(PhysicalRegister(5))
}

fn is_usable_gpr(id: u8) -> bool {
    id < 8 && id != 4 && id != 5
}

fn imm16(value: i64) -> Result<u16, CodegenError> {
    u16::try_from(value)
        .or_else(|_| i16::try_from(value).map(|v| v as u16))
        .map_err(|_| {
            CodegenError::new(
                "i8086",
                format!("immediate 0x{value:x} does not fit in a 16-bit 8086 immediate"),
            )
            .with_arch("i8086")
        })
}

fn register_number(op: &MachineOperand) -> Option<u8> {
    match op {
        MachineOperand::Register(MachineRegister::Physical(reg)) if is_usable_gpr(reg.0) => {
            Some(reg.0)
        }
        _ => None,
    }
}

fn any_gpr_number(op: &MachineOperand) -> Option<u8> {
    match op {
        MachineOperand::Register(MachineRegister::Physical(reg)) if reg.0 < 8 => Some(reg.0),
        _ => None,
    }
}

fn byte_register_number(op: &MachineOperand) -> Option<u8> {
    match op {
        // 8-bit low registers on 8086: AL(0), CL(1), DL(2), BL(3).
        MachineOperand::Register(MachineRegister::Physical(reg)) if reg.0 < 4 => Some(reg.0),
        _ => None,
    }
}

#[derive(Clone, Copy)]
struct MemAddr {
    base: u8,
    offset: i16,
    index: Option<(u8, u8)>,
}

impl MemAddr {
    fn uses_reg(&self, reg: u8) -> bool {
        self.base == reg || self.index.is_some_and(|(idx, _)| idx == reg)
    }
}

fn memory_location(op: &MachineOperand) -> Option<MemAddr> {
    match op {
        MachineOperand::StackSlot(offset) => Some(MemAddr {
            base: 5, // BP
            offset: i16::try_from(*offset).ok()?,
            index: None,
        }),
        MachineOperand::Memory {
            base: MachineRegister::Physical(reg),
            offset,
            index,
        } if reg.0 < 8 => {
            let decoded_index = match index {
                None => None,
                Some((MachineRegister::Physical(idx_reg), scale)) if idx_reg.0 < 8 => {
                    let scale_log2 = match scale {
                        0 | 1 => 0u8,
                        2 => 1,
                        4 => 2,
                        8 => 3,
                        _ => return None,
                    };
                    Some((idx_reg.0, scale_log2))
                }
                _ => return None,
            };
            Some(MemAddr {
                base: reg.0,
                offset: i16::try_from(*offset).ok()?,
                index: decoded_index,
            })
        }
        _ => None,
    }
}

fn encode_mov_r16_imm16(out: &mut Vec<u8>, dst: u8, imm: i64) -> Result<(), CodegenError> {
    let v = imm16(imm)?;
    out.push(0xB8 + dst);
    out.extend_from_slice(&v.to_le_bytes());
    Ok(())
}

fn encode_mov_r16_r16(out: &mut Vec<u8>, dst: u8, src: u8) {
    out.push(0x89);
    out.push(0xC0 | (src << 3) | dst);
}

fn encode_push_r16(out: &mut Vec<u8>, reg: u8) {
    out.push(0x50 + reg);
}

fn encode_pop_r16(out: &mut Vec<u8>, reg: u8) {
    out.push(0x58 + reg);
}

pub fn encode_push_seg(out: &mut Vec<u8>, seg: SegmentRegister) {
    out.push(0x06 | ((seg as u8) << 3));
}

pub fn encode_pop_seg(out: &mut Vec<u8>, seg: SegmentRegister) -> Result<(), CodegenError> {
    if seg == SegmentRegister::Cs {
        return Err(
            CodegenError::new("i8086", "POP CS is invalid on Intel 8086").with_arch("i8086"),
        );
    }
    out.push(0x07 | ((seg as u8) << 3));
    Ok(())
}

pub fn encode_mov_seg_r16(
    out: &mut Vec<u8>,
    seg: SegmentRegister,
    src: u8,
) -> Result<(), CodegenError> {
    if seg == SegmentRegister::Cs || src >= 8 {
        return Err(
            CodegenError::new("i8086", "invalid MOV sreg, r16 operand on 8086").with_arch("i8086"),
        );
    }
    out.push(0x8E);
    out.push(0xC0 | ((seg as u8) << 3) | src);
    Ok(())
}

pub fn encode_mov_r16_seg(
    out: &mut Vec<u8>,
    dst: u8,
    seg: SegmentRegister,
) -> Result<(), CodegenError> {
    if dst >= 8 {
        return Err(
            CodegenError::new("i8086", "invalid MOV r16, sreg operand on 8086").with_arch("i8086"),
        );
    }
    out.push(0x8C);
    out.push(0xC0 | ((seg as u8) << 3) | dst);
    Ok(())
}

pub fn encode_call_far_abs(out: &mut Vec<u8>, relocations: &mut Vec<AdobRelocation>, symbol: &str) {
    let off_pos = out.len() + 1;
    out.push(0x9A);
    out.extend_from_slice(&0u16.to_le_bytes());
    let seg_pos = out.len();
    out.extend_from_slice(&0u16.to_le_bytes());
    relocations.push(AdobRelocation::new(
        off_pos as u64,
        0,
        symbol,
        RelocationKind::Absolute16,
        0,
    ));
    relocations.push(AdobRelocation::new(
        seg_pos as u64,
        0,
        symbol,
        RelocationKind::I8086_Segment16,
        0,
    ));
}

pub fn encode_retf(out: &mut Vec<u8>) {
    out.push(0xCB);
}

pub fn encode_int(out: &mut Vec<u8>, vector: u8) {
    out.push(0xCD);
    out.push(vector);
}

fn encode_call_rel16(out: &mut Vec<u8>, relocations: &mut Vec<AdobRelocation>, symbol: &str) {
    let disp_offset = out.len() + 1;
    out.push(0xE8);
    out.extend_from_slice(&0u16.to_le_bytes());
    relocations.push(AdobRelocation::new(
        disp_offset as u64,
        0,
        symbol,
        RelocationKind::PcRelative16,
        -2,
    ));
}

fn encode_call_r16(out: &mut Vec<u8>, target: u8) {
    out.push(0xFF);
    out.push(0xD0 | target);
}

fn encode_add_sub_sp(out: &mut Vec<u8>, is_add: bool, imm: i64) -> Result<(), CodegenError> {
    let value = i16::try_from(imm).map_err(|_| {
        CodegenError::new(
            "i8086",
            format!("stack adjustment 0x{imm:x} does not fit in a signed 16-bit immediate"),
        )
        .with_arch("i8086")
    })?;
    if value == 0 {
        return Ok(());
    }
    if let Ok(imm8) = i8::try_from(value) {
        out.extend_from_slice(&[0x83, if is_add { 0xC4 } else { 0xEC }, imm8 as u8]);
        return Ok(());
    }
    out.extend_from_slice(&[0x81, if is_add { 0xC4 } else { 0xEC }]);
    out.extend_from_slice(&(value as u16).to_le_bytes());
    Ok(())
}

/// Real 8086 16-bit ModR/M encoding for `[base + disp]` where `base` is BX(3), BP(5), SI(6), or DI(7).
fn modrm_rm_for_base(base: u8) -> Option<u8> {
    match base {
        6 => Some(4), // [SI]
        7 => Some(5), // [DI]
        5 => Some(6), // [BP + disp]
        3 => Some(7), // [BX]
        _ => None,
    }
}

fn encode_modrm_16_base_disp(
    out: &mut Vec<u8>,
    reg_field: u8,
    base: u8,
    offset: i16,
) -> Result<(), CodegenError> {
    let rm = modrm_rm_for_base(base).ok_or_else(|| {
        CodegenError::new(
            "i8086",
            format!("register {base} cannot be used directly as an 8086 ModR/M base"),
        )
        .with_arch("i8086")
    })?;
    if offset == 0 && base != 5 {
        out.push((reg_field << 3) | rm);
    } else if let Ok(disp8) = i8::try_from(offset) {
        out.push(0x40 | (reg_field << 3) | rm);
        out.push(disp8 as u8);
    } else {
        out.push(0x80 | (reg_field << 3) | rm);
        out.extend_from_slice(&offset.to_le_bytes());
    }
    Ok(())
}

fn pick_temp_base(avoid1: u8, avoid2: u8) -> u8 {
    for candidate in [3u8, 6, 7] {
        if candidate != avoid1 && candidate != avoid2 {
            return candidate;
        }
    }
    3
}

fn pick_temp_base_3(avoid1: u8, avoid2: u8, avoid3: u8) -> u8 {
    for candidate in [3u8, 6, 7] {
        if candidate != avoid1 && candidate != avoid2 && candidate != avoid3 {
            return candidate;
        }
    }
    pick_temp_base(avoid1, avoid2)
}

/// Execute an 8086 memory instruction `(opcode, reg_field)` over any `MemAddr`,
/// staging non-ModR/M bases (`SP`, `AX`, `CX`, `DX`) or scaled indices through a
/// temporary base register (`BX`/`SI`/`DI`) with `push`/`pop` preservation.
fn emit_mem_op(
    out: &mut Vec<u8>,
    opcode: u8,
    reg_field: u8,
    addr: MemAddr,
    avoid_reg: u8,
) -> Result<(), CodegenError> {
    if let Some((idx, scale_log2)) = addr.index {
        let scratch = pick_temp_base_3(avoid_reg, addr.base, idx);
        encode_push_r16(out, scratch);
        if scratch != idx {
            encode_mov_r16_r16(out, scratch, idx);
        }
        for _ in 0..scale_log2 {
            out.extend_from_slice(&[0xD1, 0xE0 | scratch]); // shl scratch, 1 (preserves CX)
        }
        let adjusted_off = if addr.base == 4 {
            addr.offset.wrapping_add(2)
        } else {
            addr.offset
        };
        out.extend_from_slice(&[0x03, 0xC0 | (scratch << 3) | addr.base]);
        out.push(opcode);
        encode_modrm_16_base_disp(out, reg_field, scratch, adjusted_off)?;
        encode_pop_r16(out, scratch);
        return Ok(());
    }

    if modrm_rm_for_base(addr.base).is_some() {
        out.push(opcode);
        encode_modrm_16_base_disp(out, reg_field, addr.base, addr.offset)?;
        return Ok(());
    }

    // Non-8086 ModR/M base (SP, AX, CX, DX): stage into preserved BX/SI/DI.
    let tmp = pick_temp_base(avoid_reg, addr.base);
    encode_push_r16(out, tmp);
    encode_mov_r16_r16(out, tmp, addr.base);
    let adjusted_off = if addr.base == 4 {
        addr.offset.wrapping_add(2)
    } else {
        addr.offset
    };
    out.push(opcode);
    encode_modrm_16_base_disp(out, reg_field, tmp, adjusted_off)?;
    encode_pop_r16(out, tmp);
    Ok(())
}

fn encode_load_mem(
    out: &mut Vec<u8>,
    dst: u8,
    addr: MemAddr,
    size: u8,
) -> Result<(), CodegenError> {
    match size {
        1 => {
            if dst < 4 {
                emit_mem_op(out, 0x8A, dst, addr, dst)?; // mov r8, [mem]
                // Zero-extend low byte -> high byte (`xor r8h, r8h`, reg = dst + 4)
                let hi = dst + 4;
                out.extend_from_slice(&[0x30, 0xC0 | (hi << 3) | hi]);
            } else {
                // SI/DI have no 8-bit register form on 8086; load into AL then copy.
                encode_push_r16(out, 0);
                let adjusted = if addr.base == 4 {
                    MemAddr {
                        offset: addr.offset.wrapping_add(2),
                        ..addr
                    }
                } else {
                    addr
                };
                emit_mem_op(out, 0x8A, 0, adjusted, dst)?;
                out.extend_from_slice(&[0x30, 0xE4]); // xor ah, ah
                encode_mov_r16_r16(out, dst, 0);
                encode_pop_r16(out, 0);
            }
            Ok(())
        }
        2 => emit_mem_op(out, 0x8B, dst, addr, dst),
        other => Err(CodegenError::new(
            "i8086",
            format!("8086 native load of {other} bytes is not supported without legalization"),
        )
        .with_arch("i8086")),
    }
}

fn encode_store_mem(
    out: &mut Vec<u8>,
    addr: MemAddr,
    src: u8,
    size: u8,
) -> Result<(), CodegenError> {
    match size {
        1 => {
            if src < 4 {
                emit_mem_op(out, 0x88, src, addr, src)
            } else {
                encode_push_r16(out, 0);
                encode_mov_r16_r16(out, 0, src);
                let adjusted = if addr.base == 4 {
                    MemAddr {
                        offset: addr.offset.wrapping_add(2),
                        ..addr
                    }
                } else {
                    addr
                };
                emit_mem_op(out, 0x88, 0, adjusted, src)?;
                encode_pop_r16(out, 0);
                Ok(())
            }
        }
        2 => emit_mem_op(out, 0x89, src, addr, src),
        other => Err(CodegenError::new(
            "i8086",
            format!("8086 native store of {other} bytes is not supported without legalization"),
        )
        .with_arch("i8086")),
    }
}

fn emit_read_sized(
    out: &mut Vec<u8>,
    relocations: &mut Vec<AdobRelocation>,
    dst: u8,
    src: &MachineOperand,
    size: u8,
) -> Result<(), CodegenError> {
    match src {
        MachineOperand::Immediate(value) => encode_mov_r16_imm16(out, dst, *value),
        MachineOperand::Register(MachineRegister::Physical(reg)) => {
            if reg.0 >= 8 {
                return Err(CodegenError::new(
                    "i8086",
                    "only 16-bit GPRs (AX..DI) are supported on 8086",
                )
                .with_arch("i8086"));
            }
            if reg.0 != dst {
                encode_mov_r16_r16(out, dst, reg.0);
            }
            if size == 1 {
                if dst < 4 {
                    let hi = dst + 4;
                    out.extend_from_slice(&[0x30, 0xC0 | (hi << 3) | hi]);
                } else {
                    out.extend_from_slice(&[0x81, 0xE0 | dst, 0xFF, 0x00]); // and dst, 0x00FF
                }
            }
            Ok(())
        }
        MachineOperand::Symbol(name) => {
            let off = out.len() + 1;
            out.push(0xB8 + dst);
            out.extend_from_slice(&0u16.to_le_bytes());
            relocations.push(AdobRelocation::new(
                off as u64,
                0,
                name,
                RelocationKind::Absolute16,
                0,
            ));
            Ok(())
        }
        _ => {
            let addr = memory_location(src).ok_or_else(|| {
                CodegenError::new("i8086", "unsupported source operand for 8086 load")
                    .with_arch("i8086")
            })?;
            encode_load_mem(out, dst, addr, size)
        }
    }
}

fn emit_read(
    out: &mut Vec<u8>,
    relocations: &mut Vec<AdobRelocation>,
    dst: u8,
    src: &MachineOperand,
) -> Result<(), CodegenError> {
    emit_read_sized(out, relocations, dst, src, 2)
}

fn emit_write_sized(
    out: &mut Vec<u8>,
    dst: &MachineOperand,
    src: u8,
    size: u8,
) -> Result<(), CodegenError> {
    if let Some(dst_reg) = register_number(dst) {
        if dst_reg != src {
            encode_mov_r16_r16(out, dst_reg, src);
        }
        if size == 1 {
            if dst_reg < 4 {
                let hi = dst_reg + 4;
                out.extend_from_slice(&[0x30, 0xC0 | (hi << 3) | hi]);
            } else {
                out.extend_from_slice(&[0x81, 0xE0 | dst_reg, 0xFF, 0x00]);
            }
        }
        return Ok(());
    }
    let addr = memory_location(dst).ok_or_else(|| {
        CodegenError::new("i8086", "unsupported destination operand for 8086 store")
            .with_arch("i8086")
    })?;
    encode_store_mem(out, addr, src, size)
}

fn emit_move(
    out: &mut Vec<u8>,
    relocations: &mut Vec<AdobRelocation>,
    dst: &MachineOperand,
    src: &MachineOperand,
) -> Result<(), CodegenError> {
    if let Some(dst_reg) = any_gpr_number(dst) {
        return emit_read(out, relocations, dst_reg, src);
    }
    let addr = memory_location(dst).ok_or_else(|| {
        CodegenError::new("i8086", "unsupported destination operand for 8086 Move")
            .with_arch("i8086")
    })?;
    if let Some(src_reg) = any_gpr_number(src) {
        return encode_store_mem(out, addr, src_reg, 2);
    }
    let scratch = if addr.uses_reg(0) { 1 } else { 0 };
    emit_read(out, relocations, scratch, src)?;
    let adjusted = if addr.base == 4 { addr } else { addr };
    encode_store_mem(out, adjusted, scratch, 2)
}

fn emit_alu(
    out: &mut Vec<u8>,
    dst: &MachineOperand,
    src: &MachineOperand,
    imm_ext: u8,
    rm_r_op: u8,
    r_rm_op: u8,
) -> Result<(), CodegenError> {
    let dst_reg = register_number(dst).ok_or_else(|| {
        CodegenError::new("i8086", "8086 ALU destination must be a 16-bit GPR").with_arch("i8086")
    })?;
    match src {
        MachineOperand::Immediate(value) => {
            let v = imm16(*value)?;
            if let Ok(imm8) = i8::try_from(v as i16) {
                out.extend_from_slice(&[0x83, 0xC0 | (imm_ext << 3) | dst_reg, imm8 as u8]);
            } else {
                out.extend_from_slice(&[0x81, 0xC0 | (imm_ext << 3) | dst_reg]);
                out.extend_from_slice(&v.to_le_bytes());
            }
            Ok(())
        }
        MachineOperand::Register(MachineRegister::Physical(reg)) if reg.0 < 8 => {
            out.push(rm_r_op);
            out.push(0xC0 | (reg.0 << 3) | dst_reg);
            Ok(())
        }
        _ => {
            let addr = memory_location(src).ok_or_else(|| {
                CodegenError::new("i8086", "unsupported source operand for 8086 ALU op")
                    .with_arch("i8086")
            })?;
            emit_mem_op(out, r_rm_op, dst_reg, addr, dst_reg)
        }
    }
}

fn condition_code(cc: ConditionCode) -> u8 {
    match cc {
        ConditionCode::Equal | ConditionCode::Zero => 0x4,
        ConditionCode::NotEqual | ConditionCode::NotZero => 0x5,
        ConditionCode::LessThan => 0xC,
        ConditionCode::LessOrEqual => 0xE,
        ConditionCode::GreaterThan => 0xF,
        ConditionCode::GreaterOrEqual => 0xD,
        ConditionCode::Below => 0x2,
        ConditionCode::BelowOrEqual => 0x6,
        ConditionCode::Above => 0x7,
        ConditionCode::AboveOrEqual => 0x3,
        ConditionCode::Parity => 0xA,
        ConditionCode::NotParity => 0xB,
    }
}

/// Legalized 32-bit integer addition on 16-bit 8086 (`(lo_reg, hi_reg) += (rhs_lo, rhs_hi)`).
pub fn emit_legalized_i32_add(out: &mut Vec<u8>, lo_dst: u8, hi_dst: u8, lo_src: u8, hi_src: u8) {
    // ADD lo_dst, lo_src
    out.extend_from_slice(&[0x01, 0xC0 | (lo_src << 3) | lo_dst]);
    // ADC hi_dst, hi_src
    out.extend_from_slice(&[0x11, 0xC0 | (hi_src << 3) | hi_dst]);
}

/// Legalized 32-bit integer subtraction on 16-bit 8086 (`(lo_reg, hi_reg) -= (rhs_lo, rhs_hi)`).
pub fn emit_legalized_i32_sub(out: &mut Vec<u8>, lo_dst: u8, hi_dst: u8, lo_src: u8, hi_src: u8) {
    // SUB lo_dst, lo_src
    out.extend_from_slice(&[0x29, 0xC0 | (lo_src << 3) | lo_dst]);
    // SBB hi_dst, hi_src
    out.extend_from_slice(&[0x19, 0xC0 | (hi_src << 3) | hi_dst]);
}

/// Legalized 32-bit left shift by 1 bit on 16-bit 8086 (`SHL lo, 1; RCL hi, 1`).
pub fn emit_legalized_i32_shl1(out: &mut Vec<u8>, lo: u8, hi: u8) {
    out.extend_from_slice(&[0xD1, 0xE0 | lo]); // shl lo, 1
    out.extend_from_slice(&[0xD1, 0xD0 | hi]); // rcl hi, 1
}

pub struct I8086Backend {
    target: TargetDescriptor,
    capabilities: TargetCapabilities,
}

impl I8086Backend {
    pub fn new(target: TargetDescriptor) -> Self {
        let capabilities = TargetCapabilities::for_architecture(&target.architecture);
        Self {
            target,
            capabilities,
        }
    }
}

impl CodegenBackend for I8086Backend {
    fn target(&self) -> &TargetDescriptor {
        &self.target
    }

    fn capabilities(&self) -> TargetCapabilities {
        self.capabilities.clone()
    }

    fn lower_module(&mut self, module: &NativeModule) -> Result<NativeModule, CodegenError> {
        let mut lowered = module.clone();
        // Strip desktop scope-tracking calls and remove unused imports on bare-metal/DOS 8086.
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
                            } if reg.0 == 4
                        )
                    {
                        start -= 2;
                    }
                    let mut end = index + 1;
                    if end < block.instructions.len()
                        && matches!(
                            &block.instructions[end],
                            MachineInstruction::Add {
                                dst: MachineOperand::Register(MachineRegister::Physical(reg)),
                                src: MachineOperand::Immediate(_),
                            } if reg.0 == 4
                        )
                    {
                        end += 1;
                    }
                    block.instructions.drain(start..end);
                    index = start;
                }
            }
        }
        let mut referenced = HashSet::new();
        for func in &lowered.functions {
            for block in &func.blocks {
                for inst in &block.instructions {
                    match inst {
                        MachineInstruction::Call {
                            target: MachineOperand::Symbol(name),
                            ..
                        }
                        | MachineInstruction::Move {
                            src: MachineOperand::Symbol(name),
                            ..
                        } => {
                            referenced.insert(name.clone());
                        }
                        _ => {}
                    }
                }
            }
        }
        lowered.imports.retain(|name| referenced.contains(name));
        Ok(lowered)
    }

    fn generate_function(&mut self, func: &MachineFunction) -> Result<Vec<u8>, CodegenError> {
        self.generate_function_with_relocations(func)
            .map(|(code, _)| code)
    }

    fn generate_function_with_relocations(
        &mut self,
        func: &MachineFunction,
    ) -> Result<(Vec<u8>, Vec<AdobRelocation>), CodegenError> {
        if func
            .vreg_classes
            .values()
            .any(|class| *class == RegisterClass::Float)
        {
            return Err(CodegenError::new(
                "i8086",
                "Intel 8086 has no hardware floating-point unit (8087 FPU codegen not implemented)",
            )
            .with_arch("i8086")
            .with_function(&func.name));
        }

        for block in &func.blocks {
            for inst in &block.instructions {
                match inst {
                    MachineInstruction::Load { size, .. }
                    | MachineInstruction::Store { size, .. }
                        if !matches!(*size, 1 | 2) =>
                    {
                        let instruction = if matches!(inst, MachineInstruction::Load { .. }) {
                            "Load"
                        } else {
                            "Store"
                        };
                        return Err(CodegenError::new(
                            "i8086",
                            format!(
                                "8086 native memory access of {size} bytes exceeds 16-bit word width; \
                                 refusing silent truncation"
                            ),
                        )
                        .with_arch("i8086")
                        .with_function(&func.name)
                        .with_instruction(instruction));
                    }
                    MachineInstruction::ParallelMove { moves }
                        if moves.iter().any(|m| !matches!(m.size, 1 | 2)) =>
                    {
                        return Err(CodegenError::new(
                            "i8086",
                            "8086 parallel moves wider than 2 bytes are not implemented",
                        )
                        .with_arch("i8086")
                        .with_function(&func.name));
                    }
                    MachineInstruction::TlsAddress { .. } => {
                        return Err(CodegenError::new(
                            "i8086",
                            "TLS is not supported in 16-bit DOS real mode",
                        )
                        .with_arch("i8086")
                        .with_function(&func.name));
                    }
                    MachineInstruction::AtomicLoad { .. }
                    | MachineInstruction::AtomicStore { .. }
                    | MachineInstruction::AtomicFetchAdd { .. }
                    | MachineInstruction::AtomicExchange { .. }
                    | MachineInstruction::AtomicCompareExchange { .. } => {
                        return Err(CodegenError::new(
                            "i8086",
                            "atomic instructions are not implemented for 8086 real mode",
                        )
                        .with_arch("i8086")
                        .with_function(&func.name));
                    }
                    _ => {}
                }
            }
        }

        let mut allocated = func.clone();
        let allocation = LinearScanAllocator::with_local_call_splitting(&I8086RegisterFile, false)
            .allocate(&mut allocated)
            .map_err(|error| {
                CodegenError::new("i8086", format!("8086 register allocation failed: {error}"))
                    .with_arch("i8086")
                    .with_function(&func.name)
            })?;

        let resolver = i8086_resolver();
        let liveness = LivenessAnalysis::compute(&mut allocated);
        let mut inst_index = 0;
        for block in &mut allocated.blocks {
            let mut expanded = Vec::new();
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
                        let resolver = resolver.clone().with_live_registers(live);
                        expanded.extend(resolver.resolve_moves(&moves).map_err(|error| {
                            CodegenError::new("i8086", format!("{error}; moves: {moves:?}"))
                                .with_arch("i8086")
                                .with_function(&func.name)
                        })?);
                    }
                    other => expanded.push(other),
                }
                inst_index += 1;
            }
            block.instructions = expanded;
        }

        let mut labels = HashMap::new();
        for block in &allocated.blocks {
            if labels.insert(block.label.clone(), block.id).is_some() {
                return Err(CodegenError::new(
                    "i8086",
                    format!("duplicate machine block label `{}`", block.label),
                )
                .with_arch("i8086")
                .with_function(&func.name));
            }
        }

        let has_calls = allocated
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .any(|inst| matches!(inst, MachineInstruction::Call { .. }));
        let has_branches = allocated
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .any(|inst| {
                matches!(
                    inst,
                    MachineInstruction::Branch { .. } | MachineInstruction::BranchCc { .. }
                )
            });
        let needs_frame = has_calls
            || has_branches
            || allocated.stack_size != 0
            || !allocation.used_callee_saved.is_empty();

        let mut saved_registers: Vec<u8> = allocation
            .used_callee_saved
            .iter()
            .map(|reg| reg.0)
            .collect();
        saved_registers.sort_unstable();

        let stack_size = i16::try_from(allocated.stack_size).map_err(|_| {
            CodegenError::new("i8086", "8086 stack frame size exceeds signed 16-bit range")
                .with_arch("i8086")
                .with_function(&func.name)
        })?;
        // 2-byte stack word alignment
        let frame_bytes = (stack_size + 1) & !1;

        let mut out = Vec::new();
        let mut relocations = Vec::new();
        let mut branch_fixups: Vec<(usize, String)> = Vec::new();

        if needs_frame {
            encode_push_r16(&mut out, 5); // push bp
            encode_mov_r16_r16(&mut out, 5, 4); // mov bp, sp
            if frame_bytes != 0 {
                encode_add_sub_sp(&mut out, false, i64::from(frame_bytes))?;
            }
            for &reg in &saved_registers {
                encode_push_r16(&mut out, reg);
            }
        }

        for (block_index, block) in allocated.blocks.iter().enumerate() {
            labels.insert(block.label.clone(), out.len() as u32);
            for inst in &block.instructions {
                match inst {
                    MachineInstruction::Nop => out.push(0x90),
                    MachineInstruction::Return => {
                        if needs_frame {
                            for &reg in saved_registers.iter().rev() {
                                encode_pop_r16(&mut out, reg);
                            }
                            // Pure 8086 epilogue (`mov sp, bp; pop bp`), NOT 80186 `leave` (`0xC9`).
                            encode_mov_r16_r16(&mut out, 4, 5);
                            encode_pop_r16(&mut out, 5);
                        }
                        if allocated.callee_pops_ret_bytes > 0 {
                            out.push(0xC2);
                            out.extend_from_slice(&allocated.callee_pops_ret_bytes.to_le_bytes());
                        } else {
                            out.push(0xC3);
                        }
                    }
                    MachineInstruction::Move { dst, src } => {
                        emit_move(&mut out, &mut relocations, dst, src)?;
                    }
                    MachineInstruction::Load { dst, src, size } => {
                        if let Some(dst_reg) = register_number(dst) {
                            emit_read_sized(&mut out, &mut relocations, dst_reg, src, *size)?;
                        } else {
                            let scratch = if memory_location(dst).is_some_and(|a| a.uses_reg(0))
                                || memory_location(src).is_some_and(|a| a.uses_reg(0))
                            {
                                1
                            } else {
                                0
                            };
                            emit_read_sized(&mut out, &mut relocations, scratch, src, *size)?;
                            emit_write_sized(&mut out, dst, scratch, *size)?;
                        }
                    }
                    MachineInstruction::Store { dst, src, size } => {
                        if let Some(src_reg) = any_gpr_number(src).filter(|&r| is_usable_gpr(r)) {
                            emit_write_sized(&mut out, dst, src_reg, *size)?;
                        } else {
                            let scratch = if memory_location(dst).is_some_and(|a| a.uses_reg(0)) {
                                1
                            } else {
                                0
                            };
                            emit_read_sized(&mut out, &mut relocations, scratch, src, *size)?;
                            emit_write_sized(&mut out, dst, scratch, *size)?;
                        }
                    }
                    MachineInstruction::Add { dst, src } | MachineInstruction::Sub { dst, src } => {
                        let is_add = matches!(inst, MachineInstruction::Add { .. });
                        if matches!(dst, MachineOperand::Register(MachineRegister::Physical(reg)) if reg.0 == 4)
                            && matches!(src, MachineOperand::Immediate(_))
                        {
                            let MachineOperand::Immediate(value) = src else {
                                unreachable!()
                            };
                            encode_add_sub_sp(&mut out, is_add, *value)?;
                        } else if is_add {
                            emit_alu(&mut out, dst, src, 0, 0x01, 0x03)?;
                        } else {
                            emit_alu(&mut out, dst, src, 5, 0x29, 0x2B)?;
                        }
                    }
                    MachineInstruction::And { dst, src } => {
                        emit_alu(&mut out, dst, src, 4, 0x21, 0x23)?;
                    }
                    MachineInstruction::Or { dst, src } => {
                        emit_alu(&mut out, dst, src, 1, 0x09, 0x0B)?;
                    }
                    MachineInstruction::Xor { dst, src } => {
                        emit_alu(&mut out, dst, src, 6, 0x31, 0x33)?;
                    }
                    MachineInstruction::Mul { dst, src } => {
                        // 8086 only has single-operand `IMUL r/m16` (`DX:AX = AX * src`).
                        let dst = register_number(dst)
                            .ok_or_else(|| unsupported_operand_form("i8086", &allocated, inst))?;
                        if dst != 2 {
                            encode_push_r16(&mut out, 2); // preserve DX
                        }
                        if dst != 0 {
                            encode_push_r16(&mut out, 0); // preserve AX if dst is another reg
                        }
                        emit_read(&mut out, &mut relocations, 1, src)?; // cx = src
                        if dst != 0 {
                            encode_mov_r16_r16(&mut out, 0, dst); // ax = dst
                        }
                        out.extend_from_slice(&[0xF7, 0xE9]); // imul cx -> DX:AX
                        if dst != 0 {
                            encode_mov_r16_r16(&mut out, dst, 0);
                            encode_pop_r16(&mut out, 0);
                        }
                        if dst != 2 {
                            encode_pop_r16(&mut out, 2);
                        }
                    }
                    MachineInstruction::Div { dst, src } | MachineInstruction::Mod { dst, src } => {
                        let dst = register_number(dst)
                            .ok_or_else(|| unsupported_operand_form("i8086", &allocated, inst))?;
                        let is_div = matches!(inst, MachineInstruction::Div { .. });
                        if dst == 1 {
                            encode_mov_r16_r16(&mut out, 0, 1);
                            emit_read(&mut out, &mut relocations, 1, src)?;
                        } else {
                            emit_read(&mut out, &mut relocations, 1, src)?;
                            if dst != 0 {
                                encode_mov_r16_r16(&mut out, 0, dst);
                            }
                        }
                        if dst != 2 {
                            encode_push_r16(&mut out, 2);
                        }
                        out.push(0x99); // cwd (AX -> DX:AX)
                        out.extend_from_slice(&[0xF7, 0xF9]); // idiv cx
                        let result_reg = if is_div { 0 } else { 2 };
                        if dst != result_reg {
                            encode_mov_r16_r16(&mut out, dst, result_reg);
                        }
                        if dst != 2 {
                            encode_pop_r16(&mut out, 2);
                        }
                    }
                    MachineInstruction::Neg { dst } | MachineInstruction::Not { dst } => {
                        let dst = register_number(dst)
                            .ok_or_else(|| unsupported_operand_form("i8086", &allocated, inst))?;
                        let ext = if matches!(inst, MachineInstruction::Neg { .. }) {
                            3
                        } else {
                            2
                        };
                        out.extend_from_slice(&[0xF7, 0xC0 | (ext << 3) | dst]);
                    }
                    MachineInstruction::Shl { dst, src }
                    | MachineInstruction::Shr { dst, src }
                    | MachineInstruction::Sar { dst, src } => {
                        // 8086 only supports shift count `1` (`0xD1`) or `CL` (`0xD3`).
                        let dst = register_number(dst)
                            .ok_or_else(|| unsupported_operand_form("i8086", &allocated, inst))?;
                        let ext = match inst {
                            MachineInstruction::Shl { .. } => 4,
                            MachineInstruction::Shr { .. } => 5,
                            MachineInstruction::Sar { .. } => 7,
                            _ => unreachable!(),
                        };
                        match src {
                            MachineOperand::Immediate(value) => {
                                let count = u8::try_from(*value).map_err(|_| {
                                    CodegenError::new(
                                        "i8086",
                                        "8086 shift count must fit in unsigned 8 bits",
                                    )
                                    .with_arch("i8086")
                                })?;
                                if count == 0 {
                                    // shift by 0 is a no-op
                                } else if count == 1 {
                                    out.extend_from_slice(&[0xD1, 0xC0 | (ext << 3) | dst]);
                                } else {
                                    if dst == 1 {
                                        return Err(CodegenError::new(
                                            "i8086",
                                            "8086 multi-bit shift cannot use CX as destination",
                                        )
                                        .with_arch("i8086")
                                        .with_function(&func.name));
                                    }
                                    out.extend_from_slice(&[0xB1, count]); // mov cl, count
                                    out.extend_from_slice(&[0xD3, 0xC0 | (ext << 3) | dst]);
                                }
                            }
                            MachineOperand::Register(_) => {
                                let src = register_number(src).ok_or_else(|| {
                                    unsupported_operand_form("i8086", &allocated, inst)
                                })?;
                                if dst == 1 && src != 1 {
                                    return Err(CodegenError::new(
                                        "i8086",
                                        "8086 variable shift cannot use CX as destination with non-CX count",
                                    )
                                    .with_arch("i8086")
                                    .with_function(&func.name));
                                }
                                if src != 1 {
                                    encode_mov_r16_r16(&mut out, 1, src);
                                }
                                out.extend_from_slice(&[0xD3, 0xC0 | (ext << 3) | dst]);
                            }
                            _ => {
                                let addr = memory_location(src).ok_or_else(|| {
                                    unsupported_operand_form("i8086", &allocated, inst)
                                })?;
                                if dst == 1 {
                                    return Err(CodegenError::new(
                                        "i8086",
                                        "8086 variable shift cannot use CX as destination with memory count",
                                    )
                                    .with_arch("i8086")
                                    .with_function(&func.name));
                                }
                                encode_load_mem(&mut out, 1, addr, 2)?;
                                out.extend_from_slice(&[0xD3, 0xC0 | (ext << 3) | dst]);
                            }
                        }
                    }
                    MachineInstruction::Compare { lhs, rhs } => {
                        if let Some(lhs_reg) = register_number(lhs) {
                            emit_alu(&mut out, &MachineOperand::phys(lhs_reg), rhs, 7, 0x39, 0x3B)?;
                        } else {
                            let scratch = if matches!(rhs, MachineOperand::Register(MachineRegister::Physical(r)) if r.0 == 0)
                                || memory_location(rhs).is_some_and(|a| a.uses_reg(0))
                            {
                                1
                            } else {
                                0
                            };
                            emit_read(&mut out, &mut relocations, scratch, lhs)?;
                            emit_alu(&mut out, &MachineOperand::phys(scratch), rhs, 7, 0x39, 0x3B)?;
                        }
                    }
                    MachineInstruction::Test { lhs, rhs } => {
                        let (lhs_reg, rhs_op) = if let Some(lhs_reg) = register_number(lhs) {
                            (lhs_reg, rhs)
                        } else if let Some(rhs_reg) = register_number(rhs) {
                            (rhs_reg, lhs)
                        } else {
                            let scratch = if memory_location(rhs).is_some_and(|a| a.uses_reg(0)) {
                                1
                            } else {
                                0
                            };
                            emit_read(&mut out, &mut relocations, scratch, lhs)?;
                            (scratch, rhs)
                        };
                        match rhs_op {
                            MachineOperand::Immediate(value) => {
                                let imm = imm16(*value)?;
                                out.extend_from_slice(&[0xF7, 0xC0 | lhs_reg]);
                                out.extend_from_slice(&imm.to_le_bytes());
                            }
                            MachineOperand::Register(_) => {
                                let rhs_reg = register_number(rhs_op).ok_or_else(|| {
                                    unsupported_operand_form("i8086", &allocated, inst)
                                })?;
                                out.extend_from_slice(&[0x85, 0xC0 | (rhs_reg << 3) | lhs_reg]);
                            }
                            _ => {
                                let addr = memory_location(rhs_op).ok_or_else(|| {
                                    unsupported_operand_form("i8086", &allocated, inst)
                                })?;
                                emit_mem_op(&mut out, 0x85, lhs_reg, addr, lhs_reg)?;
                            }
                        }
                    }
                    MachineInstruction::SetCc { dst, cc } => {
                        // 8086 has no `SETcc` instruction (`0x0F 0x90` is 80386+).
                        // `mov reg, 1` does NOT touch FLAGS; `jcc +2` skips `xor reg, reg`.
                        let cc_byte = condition_code(*cc);
                        if let Some(dst_reg) = register_number(dst) {
                            encode_mov_r16_imm16(&mut out, dst_reg, 1)?;
                            out.extend_from_slice(&[0x70 | cc_byte, 0x02]);
                            out.extend_from_slice(&[0x31, 0xC0 | (dst_reg << 3) | dst_reg]);
                        } else if let Some(addr) = memory_location(dst) {
                            let scratch = if addr.uses_reg(0) { 1 } else { 0 };
                            encode_push_r16(&mut out, scratch);
                            encode_mov_r16_imm16(&mut out, scratch, 1)?;
                            out.extend_from_slice(&[0x70 | cc_byte, 0x02]);
                            out.extend_from_slice(&[0x31, 0xC0 | (scratch << 3) | scratch]);
                            let adjusted = if addr.base == 4 {
                                MemAddr {
                                    offset: addr.offset.wrapping_add(2),
                                    ..addr
                                }
                            } else {
                                addr
                            };
                            encode_store_mem(&mut out, adjusted, scratch, 2)?;
                            encode_pop_r16(&mut out, scratch);
                        } else {
                            return Err(unsupported_operand_form("i8086", &allocated, inst));
                        }
                    }
                    MachineInstruction::Branch { target } => {
                        let displacement = out.len() + 1;
                        out.extend_from_slice(&[0xE9, 0x00, 0x00]); // jmp rel16
                        branch_fixups.push((displacement, target.clone()));
                    }
                    MachineInstruction::BranchCc { cc, target } => {
                        // 8086 has only short rel8 `Jcc` (`0x70..0x7F`); emit `j!cc +3; jmp rel16`.
                        let inverted = condition_code(*cc) ^ 1;
                        out.extend_from_slice(&[0x70 | inverted, 0x03]);
                        let displacement = out.len() + 1;
                        out.extend_from_slice(&[0xE9, 0x00, 0x00]);
                        branch_fixups.push((displacement, target.clone()));
                    }
                    MachineInstruction::Call { target, .. } => match target {
                        MachineOperand::Symbol(name) => {
                            encode_call_rel16(&mut out, &mut relocations, name);
                        }
                        MachineOperand::Register(_) => {
                            let target = register_number(target).ok_or_else(|| {
                                unsupported_operand_form("i8086", &allocated, inst)
                            })?;
                            encode_call_r16(&mut out, target);
                        }
                        _ => return Err(unsupported_operand_form("i8086", &allocated, inst)),
                    },
                    MachineInstruction::Push { src } => {
                        if let Some(src_reg) = any_gpr_number(src) {
                            encode_push_r16(&mut out, src_reg);
                        } else if let MachineOperand::Immediate(imm) = src {
                            // 8086 has no `push imm16` (`0x68` is 80186+); stage via AX.
                            encode_mov_r16_imm16(&mut out, 0, *imm)?;
                            encode_push_r16(&mut out, 0);
                        } else {
                            return Err(unsupported_operand_form("i8086", &allocated, inst));
                        }
                    }
                    MachineInstruction::Pop { dst } => {
                        let dst = any_gpr_number(dst)
                            .ok_or_else(|| unsupported_operand_form("i8086", &allocated, inst))?;
                        encode_pop_r16(&mut out, dst);
                    }
                    MachineInstruction::ParallelMove { .. } => {
                        return Err(CodegenError::new(
                            "i8086",
                            "unresolved 8086 parallel move after allocation",
                        )
                        .with_arch("i8086")
                        .with_function(&func.name));
                    }
                    _ => return Err(unsupported_instruction("i8086", &allocated, inst)),
                }
            }
            if block_index + 1 == allocated.blocks.len()
                && !matches!(block.instructions.last(), Some(MachineInstruction::Return))
            {
                return Err(CodegenError::new(
                    "i8086",
                    format!(
                        "function `{}` ends without Return instruction; refusing to fall off code",
                        func.name
                    ),
                )
                .with_arch("i8086")
                .with_function(&func.name));
            }
        }

        for (disp_offset, target) in branch_fixups {
            let target_offset = labels.get(&target).copied().ok_or_else(|| {
                CodegenError::new("i8086", format!("unknown branch target `{target}`"))
                    .with_arch("i8086")
                    .with_function(&func.name)
            })?;
            let rel =
                i16::try_from(target_offset as i32 - (disp_offset as i32 + 2)).map_err(|_| {
                    CodegenError::new(
                        "i8086",
                        format!("branch target `{target}` exceeds 16-bit signed relative range"),
                    )
                    .with_arch("i8086")
                    .with_function(&func.name)
                })?;
            out[disp_offset..disp_offset + 2].copy_from_slice(&rel.to_le_bytes());
        }

        Ok((out, relocations))
    }

    fn emit_object(&mut self, module: &NativeModule) -> Result<AdobObject, CodegenError> {
        let lowered = self.lower_module(module)?;
        let module = &lowered;
        let mut obj = AdobObject::new(self.target.clone());

        obj.memory_regions.push(MemoryRegion::new(
            ".text",
            0x0100,
            64 * 1024,
            MemoryPermissions::Rx,
        ));

        let mut text = Vec::new();
        let mut text_relocs = Vec::new();

        for func in &module.functions {
            let (code, relocs) = self.generate_function_with_relocations(func)?;
            let align_off = (text.len() + 1) & !1;
            text.resize(align_off, 0x90);
            let func_off = text.len() as u64;
            let func_len = code.len() as u64;
            text.extend_from_slice(&code);
            for mut r in relocs {
                r.offset += func_off;
                text_relocs.push(r);
            }
            let sym = AdobSymbol::new_defined(
                0,
                func.name.clone(),
                SymbolKind::Function,
                0,
                func_off,
                func_len,
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
            .with_memory_region(".text")
            .with_data(text);
        text_sec.relocations = text_relocs;
        obj.add_section(text_sec);

        for imp in &module.imports {
            obj.add_import(imp.clone());
            obj.add_symbol(
                AdobSymbol::new_undefined(0, imp.clone(), SymbolKind::Import)
                    .with_binding(SymbolBinding::Global),
            );
        }

        // Emit `.rodata` / `.data` with 2-byte alignment appropriate for 16-bit 8086.
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
            obj.add_section(
                AdobSection::new(".rodata", SectionKind::Rodata)
                    .with_flags(section_flags::READ | section_flags::ALLOC)
                    .with_alignment(2)
                    .with_data(rodata),
            );
        }

        if !module.tls_sections.is_empty() {
            return Err(CodegenError::new(
                "i8086",
                "thread-local storage (.tls) is not supported in 16-bit DOS real mode",
            )
            .with_arch("i8086"));
        }

        if !module.data_sections.is_empty() {
            let data_index = obj.sections.len() as u32;
            let mut data_bytes = Vec::new();
            for (name, init_bytes) in &module.data_sections {
                let aligned_off = (data_bytes.len() + 1) & !1;
                data_bytes.resize(aligned_off, 0);
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
            obj.add_section(
                AdobSection::new(".data", SectionKind::Data)
                    .with_flags(section_flags::READ | section_flags::WRITE | section_flags::ALLOC)
                    .with_alignment(2)
                    .with_data(data_bytes),
            );
        }

        let name_to_id: HashMap<String, u32> = obj
            .symbols
            .iter()
            .map(|symbol| (symbol.name.clone(), symbol.id))
            .collect();
        for section in &mut obj.sections {
            for relocation in &mut section.relocations {
                if let Some(&id) = name_to_id.get(&relocation.symbol_name) {
                    relocation.symbol = id;
                } else {
                    return Err(CodegenError::new(
                        "i8086",
                        format!("unresolved relocation symbol `{}`", relocation.symbol_name),
                    )
                    .with_arch("i8086")
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

    fn target() -> TargetDescriptor {
        TargetDescriptor::from_triple("i8086-unknown-dos").expect("triple")
    }

    #[test]
    fn encodes_pure_8086_prologue_epilogue_and_arithmetic() {
        let mut backend = I8086Backend::new(target());
        let mut func = MachineFunction::new("add_nums");
        func.stack_size = 4;
        let a = func.alloc_vreg();
        let b = func.alloc_vreg();
        let bb = &mut func.blocks[0];
        bb.push(MachineInstruction::Move {
            dst: MachineOperand::reg(a.0),
            src: MachineOperand::StackSlot(4),
        });
        bb.push(MachineInstruction::Move {
            dst: MachineOperand::reg(b.0),
            src: MachineOperand::StackSlot(6),
        });
        bb.push(MachineInstruction::Add {
            dst: MachineOperand::reg(a.0),
            src: MachineOperand::reg(b.0),
        });
        bb.push(MachineInstruction::Move {
            dst: MachineOperand::phys(0),
            src: MachineOperand::reg(a.0),
        });
        bb.push(MachineInstruction::Return);

        let code = backend.generate_function(&func).unwrap();
        // Starts with `push bp; mov bp, sp` (`55 89 E5`), never `leave` (`C9`), ends with `C3`
        assert_eq!(&code[..3], &[0x55, 0x89, 0xE5]);
        assert!(!code.contains(&0xC9));
        assert_eq!(code.last().copied(), Some(0xC3));
    }

    #[test]
    fn encodes_setcc_without_386_opcodes() {
        let mut backend = I8086Backend::new(target());
        let mut func = MachineFunction::new("cmp_lt");
        let x = func.alloc_vreg();
        let bb = &mut func.blocks[0];
        bb.push(MachineInstruction::Move {
            dst: MachineOperand::reg(x.0),
            src: MachineOperand::Immediate(5),
        });
        bb.push(MachineInstruction::Compare {
            lhs: MachineOperand::reg(x.0),
            rhs: MachineOperand::Immediate(10),
        });
        bb.push(MachineInstruction::SetCc {
            dst: MachineOperand::reg(x.0),
            cc: ConditionCode::LessThan,
        });
        bb.push(MachineInstruction::Move {
            dst: MachineOperand::phys(0),
            src: MachineOperand::reg(x.0),
        });
        bb.push(MachineInstruction::Return);

        let code = backend.generate_function(&func).unwrap();
        // Verify no `0x0F` two-byte 386 prefix anywhere in emitted code!
        assert!(!code.contains(&0x0F));
    }

    #[test]
    fn encodes_spills_when_more_than_three_vregs_live() {
        let mut backend = I8086Backend::new(target());
        let mut func = MachineFunction::new("pressure");
        let vregs: Vec<_> = (0..6).map(|_| func.alloc_vreg()).collect();
        let bb = &mut func.blocks[0];
        for (i, &v) in vregs.iter().enumerate() {
            bb.push(MachineInstruction::Move {
                dst: MachineOperand::reg(v.0),
                src: MachineOperand::Immediate((i + 1) as i64),
            });
        }
        for &v in &vregs[1..] {
            bb.push(MachineInstruction::Add {
                dst: MachineOperand::reg(vregs[0].0),
                src: MachineOperand::reg(v.0),
            });
        }
        bb.push(MachineInstruction::Move {
            dst: MachineOperand::phys(0),
            src: MachineOperand::reg(vregs[0].0),
        });
        bb.push(MachineInstruction::Return);

        let code = backend.generate_function(&func).unwrap();
        assert!(!code.is_empty());
        assert!(!code.contains(&0x0F));
    }

    #[test]
    fn encodes_shifts_mul_div_and_segment_ops() {
        let mut backend = I8086Backend::new(target());
        let mut func = MachineFunction::new("ops");
        let v = func.alloc_vreg();
        let bb = &mut func.blocks[0];
        bb.push(MachineInstruction::Move {
            dst: MachineOperand::reg(v.0),
            src: MachineOperand::Immediate(7),
        });
        bb.push(MachineInstruction::Shl {
            dst: MachineOperand::reg(v.0),
            src: MachineOperand::Immediate(3),
        });
        bb.push(MachineInstruction::Mul {
            dst: MachineOperand::reg(v.0),
            src: MachineOperand::Immediate(5),
        });
        bb.push(MachineInstruction::Div {
            dst: MachineOperand::reg(v.0),
            src: MachineOperand::Immediate(2),
        });
        bb.push(MachineInstruction::Move {
            dst: MachineOperand::phys(0),
            src: MachineOperand::reg(v.0),
        });
        bb.push(MachineInstruction::Return);

        let code = backend.generate_function(&func).unwrap();
        // Uses `mov cl, 3; shl ..., cl` (`B1 03 D3`), NEVER 80186 `C1`
        assert!(!code.contains(&0xC1));
        assert!(code.windows(2).any(|w| w == [0xB1, 0x03]));
        // Segment register helpers work
        let mut seg_buf = Vec::new();
        encode_push_seg(&mut seg_buf, SegmentRegister::Ds);
        encode_pop_seg(&mut seg_buf, SegmentRegister::Es).unwrap();
        encode_mov_seg_r16(&mut seg_buf, SegmentRegister::Ds, 0).unwrap();
        encode_mov_r16_seg(&mut seg_buf, 0, SegmentRegister::Cs).unwrap();
        encode_retf(&mut seg_buf);
        assert_eq!(seg_buf, vec![0x1E, 0x07, 0x8E, 0xD8, 0x8C, 0xC8, 0xCB]);
        assert!(encode_pop_seg(&mut seg_buf, SegmentRegister::Cs).is_err());
    }

    #[test]
    fn refuses_float_and_width_exceeding_memory_loudly() {
        let mut backend = I8086Backend::new(target());
        let mut func = MachineFunction::new("bad_load");
        func.blocks[0].push(MachineInstruction::Load {
            dst: MachineOperand::phys(0),
            src: MachineOperand::StackSlot(-4),
            size: 4,
        });
        func.blocks[0].push(MachineInstruction::Return);
        let err = backend.generate_function(&func).unwrap_err();
        assert!(err.to_string().contains("i8086"));

        let mut float_func = MachineFunction::new("bad_float");
        let fv = float_func.alloc_vreg();
        float_func.vreg_classes.insert(fv, RegisterClass::Float);
        float_func.blocks[0].push(MachineInstruction::Return);
        let err = float_func.blocks[0].instructions.len();
        let _ = err;
        let err = backend.generate_function(&float_func).unwrap_err();
        assert!(err.to_string().contains("floating-point"));
    }

    #[test]
    fn emits_i32_legalized_add_sub_shl1() {
        let mut code = Vec::new();
        emit_legalized_i32_add(&mut code, 0, 2, 3, 1); // DX:AX += CX:BX
        emit_legalized_i32_sub(&mut code, 0, 2, 3, 1);
        emit_legalized_i32_shl1(&mut code, 0, 2);
        assert_eq!(
            code,
            vec![
                0x01, 0xD8, 0x11, 0xCA, 0x29, 0xD8, 0x19, 0xCA, 0xD1, 0xE0, 0xD1, 0xD2
            ]
        );
    }
}
