//! IA-32 / i686 Native Codegen Backend.
//!
//! Phase 2 execution subset for 32-bit native execution. The backend allocates
//! and spills scalar GPR values, emits aligned frames, and encodes 32-bit
//! arithmetic, memory accesses, calls, and basic-block control flow. Unsupported
//! widths and instruction classes are refused loudly; it never emits REX
//! prefixes, 64-bit operands, or silently applies x86-64 semantics.
//!
//! Memory-safe (`#![forbid(unsafe_code)]`), type-safe, no heap in refusal paths.

#![forbid(unsafe_code)]

use crate::backend::CodegenBackend;
use crate::calling_convention::ParallelMoveResolver;
use crate::error::CodegenError;
use crate::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    MoveLocation, NativeModule, PhysicalRegister, RegisterClass, VirtualRegister,
};
use crate::register_alloc::{LinearScanAllocator, LivenessAnalysis, RegisterFile};
use crate::targets::{unsupported_instruction, unsupported_operand_form};
use adesh_object::{
    AdobObject, AdobRelocation, AdobSection, AdobSymbol, MemoryPermissions, MemoryRegion,
    RelocationKind, SectionKind, SymbolBinding, SymbolKind, SymbolVisibility, TargetCapabilities,
    TargetDescriptor, section_flags,
};
use std::collections::{HashMap, HashSet};

mod x87;

static I686_GPRS: [PhysicalRegister; 8] = [
    PhysicalRegister::gpr(0),
    PhysicalRegister::gpr(1),
    PhysicalRegister::gpr(2),
    PhysicalRegister::gpr(3),
    PhysicalRegister::gpr(4),
    PhysicalRegister::gpr(5),
    PhysicalRegister::gpr(6),
    PhysicalRegister::gpr(7),
];
static I686_ALLOCATABLE: [PhysicalRegister; 4] = [
    PhysicalRegister::gpr(2),
    PhysicalRegister::gpr(3),
    PhysicalRegister::gpr(6),
    PhysicalRegister::gpr(7),
];
static I686_CALLER_SAVED: [PhysicalRegister; 3] = [
    PhysicalRegister::gpr(0),
    PhysicalRegister::gpr(1),
    PhysicalRegister::gpr(2),
];
static I686_CALLEE_SAVED: [PhysicalRegister; 3] = [
    PhysicalRegister::gpr(3),
    PhysicalRegister::gpr(6),
    PhysicalRegister::gpr(7),
];
static I686_RESERVED: [PhysicalRegister; 2] = [PhysicalRegister::gpr(4), PhysicalRegister::gpr(5)];

struct I686RegisterFile;

impl RegisterFile for I686RegisterFile {
    fn registers(&self) -> &[PhysicalRegister] {
        &I686_GPRS
    }

    fn allocatable(&self) -> &[PhysicalRegister] {
        &I686_ALLOCATABLE
    }

    fn caller_saved(&self) -> &[PhysicalRegister] {
        &I686_CALLER_SAVED
    }

    fn callee_saved(&self) -> &[PhysicalRegister] {
        &I686_CALLEE_SAVED
    }

    fn reserved(&self) -> &[PhysicalRegister] {
        &I686_RESERVED
    }

    fn scratch_for_class(&self, class: RegisterClass) -> (PhysicalRegister, PhysicalRegister) {
        match class {
            RegisterClass::Gpr => (PhysicalRegister::gpr(0), PhysicalRegister::gpr(1)),
            RegisterClass::Float => (PhysicalRegister::xmm(15), PhysicalRegister::xmm(14)),
        }
    }

    fn scalar_slot_size(&self) -> u32 {
        4
    }
}

fn i686_resolver() -> ParallelMoveResolver {
    let mut reserved: HashSet<PhysicalRegister> = (8..=u8::MAX).map(PhysicalRegister).collect();
    reserved.insert(PhysicalRegister(4)); // ESP
    reserved.insert(PhysicalRegister(5)); // EBP
    ParallelMoveResolver::new("i686")
        .with_scratch_pool(vec![
            crate::machine_ir::PhysicalRegister(0),
            crate::machine_ir::PhysicalRegister(1),
            crate::machine_ir::PhysicalRegister(2),
            crate::machine_ir::PhysicalRegister(3),
            crate::machine_ir::PhysicalRegister(6),
            crate::machine_ir::PhysicalRegister(7),
        ])
        .with_reserved_registers(reserved)
        .with_default_stack_base(crate::machine_ir::PhysicalRegister(5))
}

fn is_usable_gpr(id: u8) -> bool {
    id < 8 && id != 4 && id != 5
}

fn imm32(value: i64) -> Result<u32, CodegenError> {
    u32::try_from(value)
        .or_else(|_| i32::try_from(value).map(|value| value as u32))
        .map_err(|_| {
            CodegenError::new(
                "i686",
                format!("immediate 0x{value:x} does not fit in a 32-bit IA-32 immediate"),
            )
            .with_arch("i686")
        })
}

fn encode_mov_imm32(out: &mut Vec<u8>, dst: u8, imm: i64) -> Result<(), CodegenError> {
    let value = imm32(imm)?;
    out.push(0xB8 + dst);
    out.extend_from_slice(&value.to_le_bytes());
    Ok(())
}

fn encode_push_r32(out: &mut Vec<u8>, reg: u8) {
    out.push(0x50 + reg);
}

fn encode_mov_r32_r32(out: &mut Vec<u8>, dst: u8, src: u8) {
    out.push(0x89);
    out.push(0xC0 | (src << 3) | dst);
}

fn encode_call_rel32(out: &mut Vec<u8>, relocations: &mut Vec<AdobRelocation>, symbol: &str) {
    let disp_offset = out.len() + 1;
    out.push(0xE8);
    out.extend_from_slice(&0u32.to_le_bytes());
    relocations.push(AdobRelocation::new(
        disp_offset as u64,
        0,
        symbol,
        RelocationKind::PcRelative32,
        -4,
    ));
}

fn encode_call_r32(out: &mut Vec<u8>, target: u8) {
    out.push(0xFF);
    out.push(0xD0 | target);
}

fn encode_add_esp_imm8(out: &mut Vec<u8>, bytes: u8) {
    out.extend_from_slice(&[0x83, 0xC4, bytes]);
}

fn encode_sub_esp_imm8(out: &mut Vec<u8>, bytes: u8) {
    out.extend_from_slice(&[0x83, 0xEC, bytes]);
}

fn encode_add_sub_esp(out: &mut Vec<u8>, is_add: bool, imm: i64) -> Result<(), CodegenError> {
    let value = i32::try_from(imm).map_err(|_| {
        CodegenError::new(
            "i686",
            format!("stack adjustment 0x{imm:x} does not fit in a signed 32-bit immediate"),
        )
        .with_arch("i686")
        .with_instruction(if is_add { "Add" } else { "Sub" })
    })?;
    if let Ok(imm8) = i8::try_from(value) {
        if is_add {
            encode_add_esp_imm8(out, imm8 as u8);
        } else {
            encode_sub_esp_imm8(out, imm8 as u8);
        }
        return Ok(());
    }
    if is_add {
        out.extend_from_slice(&[0x81, 0xC4]);
    } else {
        out.extend_from_slice(&[0x81, 0xEC]);
    }
    out.extend_from_slice(&(value as u32).to_le_bytes());
    Ok(())
}

#[derive(Clone, Copy)]
struct MemAddr {
    base: u8,
    offset: i32,
    index: Option<(u8, u8)>, // (index_reg, scale_log2: 0..=3)
}

impl MemAddr {
    fn uses_reg(&self, reg: u8) -> bool {
        self.base == reg || self.index.is_some_and(|(idx, _)| idx == reg)
    }
}

fn encode_modrm_mem(out: &mut Vec<u8>, reg_field: u8, addr: MemAddr) -> Result<(), CodegenError> {
    if let Some((idx, scale_log2)) = addr.index {
        if idx == 4 || idx >= 8 || addr.base >= 8 || scale_log2 > 3 {
            return Err(
                CodegenError::new("i686", "invalid IA-32 SIB addressing operand").with_arch("i686"),
            );
        }
        let sib = (scale_log2 << 6) | (idx << 3) | addr.base;
        if addr.offset == 0 && addr.base != 5 {
            out.push((reg_field << 3) | 0x04);
            out.push(sib);
        } else if let Ok(disp8) = i8::try_from(addr.offset) {
            out.push(0x40 | (reg_field << 3) | 0x04);
            out.push(sib);
            out.push(disp8 as u8);
        } else {
            out.push(0x80 | (reg_field << 3) | 0x04);
            out.push(sib);
            out.extend_from_slice(&addr.offset.to_le_bytes());
        }
        return Ok(());
    }
    if addr.base >= 8 {
        return Err(
            CodegenError::new("i686", "invalid IA-32 base register in memory operand")
                .with_arch("i686"),
        );
    }
    if addr.base == 4 {
        // ESP-relative forms need a SIB byte (ss=0, index=4/none, base=4).
        if addr.offset == 0 {
            out.push(0x04 | (reg_field << 3));
            out.push(0x24);
        } else if let Ok(disp8) = i8::try_from(addr.offset) {
            out.push(0x44 | (reg_field << 3));
            out.push(0x24);
            out.push(disp8 as u8);
        } else {
            out.push(0x84 | (reg_field << 3));
            out.push(0x24);
            out.extend_from_slice(&addr.offset.to_le_bytes());
        }
        return Ok(());
    }
    if addr.offset == 0 && addr.base != 5 {
        out.push((reg_field << 3) | addr.base);
    } else if let Ok(disp8) = i8::try_from(addr.offset) {
        out.push(0x40 | (reg_field << 3) | addr.base);
        out.push(disp8 as u8);
    } else {
        out.push(0x80 | (reg_field << 3) | addr.base);
        out.extend_from_slice(&addr.offset.to_le_bytes());
    }
    Ok(())
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
                out.push(0x88);
                encode_modrm_mem(out, src, addr)?;
            } else {
                // ESI/EDI have no low-byte ModR/M encoding in 32-bit mode (6/7 are DH/BH).
                let scratch = if addr.uses_reg(0) {
                    if addr.uses_reg(1) {
                        return Err(CodegenError::new(
                            "i686",
                            "cannot spill 8-bit ESI/EDI store when both EAX and ECX are address registers",
                        )
                        .with_arch("i686"));
                    }
                    1
                } else {
                    0
                };
                out.push(0x50 + scratch); // push scratch
                encode_mov_r32_r32(out, scratch, src);
                out.push(0x88);
                encode_modrm_mem(out, scratch, addr)?;
                out.push(0x58 + scratch); // pop scratch
            }
            Ok(())
        }
        2 => {
            out.push(0x66);
            out.push(0x89);
            encode_modrm_mem(out, src, addr)
        }
        4 => {
            out.push(0x89);
            encode_modrm_mem(out, src, addr)
        }
        _ => Err(CodegenError::new(
            "i686",
            format!("IA-32 memory store of {size} bytes is not implemented"),
        )
        .with_arch("i686")
        .with_instruction("Store")),
    }
}

fn encode_load_mem(
    out: &mut Vec<u8>,
    dst: u8,
    addr: MemAddr,
    size: u8,
) -> Result<(), CodegenError> {
    match size {
        1 => {
            // movzx r32, byte [mem]
            out.extend_from_slice(&[0x0F, 0xB6]);
            encode_modrm_mem(out, dst, addr)
        }
        2 => {
            // movzx r32, word [mem]
            out.extend_from_slice(&[0x0F, 0xB7]);
            encode_modrm_mem(out, dst, addr)
        }
        4 => {
            out.push(0x8B);
            encode_modrm_mem(out, dst, addr)
        }
        _ => Err(CodegenError::new(
            "i686",
            format!("IA-32 memory load of {size} bytes is not implemented"),
        )
        .with_arch("i686")
        .with_instruction("Load")),
    }
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
    // Only AL(0), CL(1), DL(2), BL(3) are low-8-bit registers in 32-bit IA-32.
    register_number(op).and_then(|reg| if reg < 4 { Some(reg) } else { None })
}

fn memory_location(op: &MachineOperand) -> Option<MemAddr> {
    match op {
        MachineOperand::StackSlot(offset) => Some(MemAddr {
            base: 5,
            offset: *offset,
            index: None,
        }),
        MachineOperand::Memory {
            base: MachineRegister::Physical(reg),
            offset,
            index,
        } if reg.0 < 8 => {
            let decoded_index = match index {
                None => None,
                Some((MachineRegister::Physical(idx_reg), scale))
                    if idx_reg.0 < 8 && idx_reg.0 != 4 =>
                {
                    let scale_log2 = match scale {
                        1 => 0,
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
                offset: *offset,
                index: decoded_index,
            })
        }
        _ => None,
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
        MachineOperand::Register(_) => {
            let src_id = if size == 4 {
                any_gpr_number(src)
            } else {
                register_number(src)
            }
            .ok_or_else(|| {
                CodegenError::new("i686", "unsupported IA-32 register operand").with_arch("i686")
            })?;
            match size {
                1 => {
                    if src_id < 4 {
                        out.extend_from_slice(&[0x0F, 0xB6, 0xC0 | (dst << 3) | src_id]);
                    } else {
                        if dst != src_id {
                            encode_mov_r32_r32(out, dst, src_id);
                        }
                        out.extend_from_slice(&[0x81, 0xE0 | dst]);
                        out.extend_from_slice(&0xFFu32.to_le_bytes());
                    }
                }
                2 => {
                    out.extend_from_slice(&[0x0F, 0xB7, 0xC0 | (dst << 3) | src_id]);
                }
                4 => {
                    // Reading ESP(4) or EBP(5) as a source operand (e.g. frame-address
                    // materialization `mov r32, ebp`) is valid 32-bit IA-32; destination
                    // registers remain restricted via `register_number`.
                    if dst != src_id {
                        encode_mov_r32_r32(out, dst, src_id);
                    }
                }
                _ => {
                    return Err(CodegenError::new(
                        "i686",
                        format!("IA-32 register read of {size} bytes is not implemented"),
                    )
                    .with_arch("i686"));
                }
            }
        }
        MachineOperand::Immediate(value) => encode_mov_imm32(out, dst, *value)?,
        MachineOperand::Symbol(name) => {
            let offset = out.len() as u64;
            encode_mov_imm32(out, dst, 0)?;
            relocations.push(AdobRelocation::new(
                offset + 1,
                0,
                name,
                RelocationKind::Absolute32,
                0,
            ));
        }
        _ => {
            let addr = memory_location(src).ok_or_else(|| {
                CodegenError::new("i686", "unsupported IA-32 memory operand").with_arch("i686")
            })?;
            encode_load_mem(out, dst, addr, size)?;
        }
    }
    Ok(())
}

fn emit_read(
    out: &mut Vec<u8>,
    relocations: &mut Vec<AdobRelocation>,
    dst: u8,
    src: &MachineOperand,
) -> Result<(), CodegenError> {
    emit_read_sized(out, relocations, dst, src, 4)
}

fn emit_write_sized(
    out: &mut Vec<u8>,
    dst: &MachineOperand,
    src: u8,
    size: u8,
) -> Result<(), CodegenError> {
    if let Some(dst) = register_number(dst) {
        match size {
            1 => {
                if src < 4 {
                    out.extend_from_slice(&[0x0F, 0xB6, 0xC0 | (dst << 3) | src]);
                } else {
                    if dst != src {
                        encode_mov_r32_r32(out, dst, src);
                    }
                    out.extend_from_slice(&[0x81, 0xE0 | dst]);
                    out.extend_from_slice(&0xFFu32.to_le_bytes());
                }
            }
            2 => {
                out.extend_from_slice(&[0x0F, 0xB7, 0xC0 | (dst << 3) | src]);
            }
            4 => encode_mov_r32_r32(out, dst, src),
            _ => {
                return Err(CodegenError::new(
                    "i686",
                    format!("IA-32 register write of {size} bytes is not implemented"),
                )
                .with_arch("i686"));
            }
        }
        return Ok(());
    }
    let addr = memory_location(dst).ok_or_else(|| {
        CodegenError::new("i686", "unsupported IA-32 destination operand").with_arch("i686")
    })?;
    encode_store_mem(out, addr, src, size)
}

fn emit_move(
    out: &mut Vec<u8>,
    relocations: &mut Vec<AdobRelocation>,
    dst: &MachineOperand,
    src: &MachineOperand,
) -> Result<(), CodegenError> {
    if let (Some(dst), Some(src)) = (register_number(dst), register_number(src)) {
        encode_mov_r32_r32(out, dst, src);
        return Ok(());
    }
    if let Some(dst) = register_number(dst) {
        return emit_read(out, relocations, dst, src);
    }
    if let Some(addr) = memory_location(dst) {
        if let Some(src_reg) = register_number(src) {
            return encode_store_mem(out, addr, src_reg, 4);
        }
        if let MachineOperand::Immediate(value) = src {
            let imm = imm32(*value)?;
            out.push(0xC7);
            encode_modrm_mem(out, 0, addr)?;
            out.extend_from_slice(&imm.to_le_bytes());
            return Ok(());
        }
        if let MachineOperand::Symbol(name) = src {
            let disp_len_before = out.len();
            out.push(0xC7);
            encode_modrm_mem(out, 0, addr)?;
            let imm_off = out.len() as u64;
            out.extend_from_slice(&0u32.to_le_bytes());
            let _ = disp_len_before;
            relocations.push(AdobRelocation::new(
                imm_off,
                0,
                name,
                RelocationKind::Absolute32,
                0,
            ));
            return Ok(());
        }
        let scratch = if addr.uses_reg(0) { 1 } else { 0 };
        emit_read(out, relocations, scratch, src)?;
        return encode_store_mem(out, addr, scratch, 4);
    }
    Err(CodegenError::new("i686", "unsupported IA-32 move operands").with_arch("i686"))
}

fn encode_alu_imm(out: &mut Vec<u8>, dst: u8, extension: u8, imm: i64) -> Result<(), CodegenError> {
    let value = imm32(imm)?;
    if let Ok(imm8) = i8::try_from(value as i32) {
        out.extend_from_slice(&[0x83, 0xC0 | (extension << 3) | dst, imm8 as u8]);
    } else {
        out.extend_from_slice(&[0x81, 0xC0 | (extension << 3) | dst]);
        out.extend_from_slice(&value.to_le_bytes());
    }
    Ok(())
}

fn encode_alu_reg(out: &mut Vec<u8>, dst: u8, opcode: u8, src: u8) {
    out.extend_from_slice(&[opcode, 0xC0 | (src << 3) | dst]);
}

fn emit_alu(
    out: &mut Vec<u8>,
    dst: &MachineOperand,
    src: &MachineOperand,
    extension: u8,
    opcode_r_m_reg: u8,
    opcode_reg_r_m: u8,
) -> Result<(), CodegenError> {
    let dst_reg = register_number(dst).ok_or_else(|| {
        CodegenError::new("i686", "IA-32 arithmetic requires a register destination")
            .with_arch("i686")
    })?;
    match src {
        MachineOperand::Immediate(value) => encode_alu_imm(out, dst_reg, extension, *value)?,
        MachineOperand::Register(_) => {
            let src_reg = register_number(src).ok_or_else(|| {
                CodegenError::new("i686", "unsupported IA-32 arithmetic source").with_arch("i686")
            })?;
            encode_alu_reg(out, dst_reg, opcode_r_m_reg, src_reg);
        }
        _ => {
            let addr = memory_location(src).ok_or_else(|| {
                CodegenError::new("i686", "unsupported IA-32 arithmetic source").with_arch("i686")
            })?;
            out.push(opcode_reg_r_m);
            encode_modrm_mem(out, dst_reg, addr)?;
        }
    }
    Ok(())
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

pub struct I686Backend {
    target: TargetDescriptor,
    capabilities: TargetCapabilities,
}

impl I686Backend {
    pub fn new(target: TargetDescriptor) -> Self {
        let capabilities = TargetCapabilities::for_architecture(&target.architecture);
        Self {
            target,
            capabilities,
        }
    }
}

impl CodegenBackend for I686Backend {
    fn target(&self) -> &TargetDescriptor {
        &self.target
    }

    fn capabilities(&self) -> TargetCapabilities {
        self.capabilities.clone()
    }

    fn lower_module(&mut self, module: &NativeModule) -> Result<NativeModule, CodegenError> {
        let mut lowered = module.clone();
        for func in &mut lowered.functions {
            let allocates = func.blocks.iter().any(|block| {
                block.instructions.iter().any(|inst| {
                    matches!(
                        inst,
                        MachineInstruction::Call {
                            target: MachineOperand::Symbol(name),
                            ..
                        } if name == "adesh_rt_alloc_tracked"
                    )
                })
            });
            if allocates {
                continue;
            }
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
                            "adesh_rt_scope_enter" | "adesh_rt_scope_exit"
                                | "adesh_rt_scope_enter_i686" | "adesh_rt_scope_exit_i686"
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
        let is_fp_operand = |op: &MachineOperand| match op {
            MachineOperand::Register(MachineRegister::Virtual(v)) => {
                func.vreg_class(*v) == RegisterClass::Float
            }
            other => x87::is_fp_pseudo(other),
        };
        for block in &func.blocks {
            for inst in &block.instructions {
                match inst {
                    MachineInstruction::Load { dst, src, size }
                        if !matches!(*size, 1 | 2 | 4)
                            && !(*size == 8
                                && (is_fp_operand(dst)
                                    || (memory_location(dst).is_some()
                                        && memory_location(src).is_some()))) =>
                    {
                        return Err(CodegenError::new(
                            "i686",
                            format!("IA-32 memory access of {size} bytes is not implemented"),
                        )
                        .with_arch("i686")
                        .with_function(&func.name)
                        .with_instruction("Load"));
                    }
                    MachineInstruction::Store { src, size, .. }
                        if !matches!(*size, 1 | 2 | 4) && !(*size == 8 && is_fp_operand(src)) =>
                    {
                        return Err(CodegenError::new(
                            "i686",
                            format!("IA-32 memory access of {size} bytes is not implemented"),
                        )
                        .with_arch("i686")
                        .with_function(&func.name)
                        .with_instruction("Store"));
                    }
                    MachineInstruction::ParallelMove { moves }
                        if moves.iter().any(|movement| {
                            movement.size != 4
                                && !(movement.size == 8
                                    && matches!(
                                        &movement.src,
                                        MoveLocation::VirtualRegister(v)
                                            if func.vreg_class(*v) == RegisterClass::Float
                                    ))
                        }) =>
                    {
                        return Err(CodegenError::new(
                            "i686",
                            "IA-32 parallel moves wider than 4 bytes are not implemented",
                        )
                        .with_arch("i686")
                        .with_function(&func.name));
                    }
                    _ => {}
                }
            }
        }

        let mut allocated = func.clone();
        let allocation = LinearScanAllocator::with_local_call_splitting(&I686RegisterFile, false)
            .allocate(&mut allocated)
            .map_err(|error| {
                CodegenError::new("i686", format!("IA-32 register allocation failed: {error}"))
                    .with_arch("i686")
                    .with_function(&func.name)
            })?;

        let resolver = i686_resolver();
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
                            CodegenError::new("i686", format!("{error}; moves: {moves:?}"))
                                .with_arch("i686")
                                .with_function(&func.name)
                        })?);
                    }
                    other => expanded.push(other),
                }
                inst_index += 1;
            }
            block.instructions = x87::fuse_wide_copies(expanded);
        }
        let uses_fp = allocated
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .any(x87::is_fp_instruction);
        let returns_fp = uses_fp
            && allocated
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .any(x87::writes_ret_pseudo);

        let mut labels = HashMap::new();
        for block in &allocated.blocks {
            if labels.insert(block.label.clone(), block.id).is_some() {
                return Err(CodegenError::new(
                    "i686",
                    format!("duplicate machine block label `{}`", block.label),
                )
                .with_arch("i686")
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
            || uses_fp
            || allocated.stack_size != 0
            || !allocation.used_callee_saved.is_empty();
        let mut saved_registers: Vec<u8> = allocation
            .used_callee_saved
            .iter()
            .map(|reg| reg.0)
            .collect();
        saved_registers.sort_unstable();
        let mut stack_size = i32::try_from(allocated.stack_size).map_err(|_| {
            CodegenError::new("i686", "IA-32 frame size exceeds signed 32-bit range")
                .with_arch("i686")
                .with_function(&func.name)
        })?;
        let fp_base = (stack_size + 3) & !3;
        if uses_fp {
            stack_size = fp_base + x87::FP_PSEUDO_BYTES;
        }
        let saved_bytes = saved_registers.len() as i32 * 4;
        let frame_bytes = if has_calls {
            // Entry `call` leaves ESP % 16 == 12; `push ebp` leaves ESP % 16 == 8.
            // Ensure (8 + frame_bytes + saved_bytes) % 16 == 0 so call sites see ESP % 16 == 0.
            let min_total = 8 + stack_size + saved_bytes;
            let aligned_total = (min_total + 15) / 16 * 16;
            aligned_total - 8 - saved_bytes
        } else {
            (stack_size + 3) & !3
        };

        let mut out = Vec::new();
        let mut relocations = Vec::new();
        let mut branch_fixups = Vec::new();
        if needs_frame {
            encode_push_r32(&mut out, 5);
            encode_mov_r32_r32(&mut out, 5, 4);
            // Allocate locals/spills BEFORE pushing callee-saved registers so
            // saved registers never overlap [ebp - 4 .. ebp - stack_size].
            if frame_bytes != 0 {
                encode_add_sub_esp(&mut out, false, i64::from(frame_bytes))?;
            }
            for &reg in &saved_registers {
                encode_push_r32(&mut out, reg);
            }
        }

        for (block_index, block) in allocated.blocks.iter().enumerate() {
            labels.insert(block.label.clone(), out.len() as u32);
            for (inst_pos, inst) in block.instructions.iter().enumerate() {
                if x87::emit_fp_instruction(&mut out, inst, fp_base)? {
                    continue;
                }
                match inst {
                    MachineInstruction::Nop => out.push(0x90),
                    MachineInstruction::Return => {
                        if returns_fp {
                            x87::emit_return_load(&mut out, fp_base)?;
                        }
                        if needs_frame {
                            for &reg in saved_registers.iter().rev() {
                                out.push(0x58 + reg);
                            }
                            if frame_bytes != 0 {
                                encode_add_sub_esp(&mut out, true, i64::from(frame_bytes))?;
                            }
                            out.push(0xC9);
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
                            encode_add_sub_esp(&mut out, is_add, *value)?;
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
                        let dst = register_number(dst)
                            .ok_or_else(|| unsupported_operand_form("i686", &allocated, inst))?;
                        match src {
                            MachineOperand::Immediate(value) => {
                                let v = imm32(*value)?;
                                if let Ok(imm8) = i8::try_from(v as i32) {
                                    out.extend_from_slice(&[
                                        0x6B,
                                        0xC0 | (dst << 3) | dst,
                                        imm8 as u8,
                                    ]);
                                } else {
                                    out.push(0x69);
                                    out.push(0xC0 | (dst << 3) | dst);
                                    out.extend_from_slice(&v.to_le_bytes());
                                }
                            }
                            MachineOperand::Register(_) => {
                                let src = register_number(src).ok_or_else(|| {
                                    unsupported_operand_form("i686", &allocated, inst)
                                })?;
                                out.extend_from_slice(&[0x0F, 0xAF, 0xC0 | (dst << 3) | src]);
                            }
                            _ => {
                                let addr = memory_location(src).ok_or_else(|| {
                                    unsupported_operand_form("i686", &allocated, inst)
                                })?;
                                out.extend_from_slice(&[0x0F, 0xAF]);
                                encode_modrm_mem(&mut out, dst, addr)?;
                            }
                        }
                    }
                    MachineInstruction::Div { dst, src } | MachineInstruction::Mod { dst, src } => {
                        let dst = register_number(dst)
                            .ok_or_else(|| unsupported_operand_form("i686", &allocated, inst))?;
                        let is_div = matches!(inst, MachineInstruction::Div { .. });
                        if dst == 1 {
                            encode_mov_r32_r32(&mut out, 0, 1);
                            emit_read(&mut out, &mut relocations, 1, src)?;
                        } else {
                            emit_read(&mut out, &mut relocations, 1, src)?;
                            if dst != 0 {
                                encode_mov_r32_r32(&mut out, 0, dst);
                            }
                        }
                        if dst != 2 {
                            encode_push_r32(&mut out, 2);
                        }
                        out.push(0x99); // cdq
                        out.extend_from_slice(&[0xF7, 0xF9]); // idiv ecx
                        let result_reg = if is_div { 0 } else { 2 };
                        if dst != result_reg {
                            encode_mov_r32_r32(&mut out, dst, result_reg);
                        }
                        if dst != 2 {
                            out.push(0x58 + 2); // pop edx
                        }
                    }
                    MachineInstruction::Neg { dst } | MachineInstruction::Not { dst } => {
                        let dst = register_number(dst)
                            .ok_or_else(|| unsupported_operand_form("i686", &allocated, inst))?;
                        let extension = if matches!(inst, MachineInstruction::Neg { .. }) {
                            3
                        } else {
                            2
                        };
                        out.extend_from_slice(&[0xF7, 0xC0 | (extension << 3) | dst]);
                    }
                    MachineInstruction::Shl { dst, src }
                    | MachineInstruction::Shr { dst, src }
                    | MachineInstruction::Sar { dst, src } => {
                        let dst = register_number(dst)
                            .ok_or_else(|| unsupported_operand_form("i686", &allocated, inst))?;
                        let extension = match inst {
                            MachineInstruction::Shl { .. } => 4,
                            MachineInstruction::Shr { .. } => 5,
                            MachineInstruction::Sar { .. } => 7,
                            _ => unreachable!(),
                        };
                        match src {
                            MachineOperand::Immediate(value) => {
                                let count = u8::try_from(*value).map_err(|_| {
                                    CodegenError::new(
                                        "i686",
                                        "IA-32 shift count must fit in 8 bits",
                                    )
                                    .with_arch("i686")
                                })?;
                                out.extend_from_slice(&[
                                    0xC1,
                                    0xC0 | (extension << 3) | dst,
                                    count,
                                ]);
                            }
                            MachineOperand::Register(_) => {
                                let src = register_number(src).ok_or_else(|| {
                                    unsupported_operand_form("i686", &allocated, inst)
                                })?;
                                if dst == 1 && src != 1 {
                                    return Err(CodegenError::new(
                                        "i686",
                                        "IA-32 variable shift cannot use ECX as destination with a non-ECX count",
                                    )
                                    .with_arch("i686")
                                    .with_function(&func.name));
                                }
                                if src != 1 {
                                    encode_mov_r32_r32(&mut out, 1, src);
                                }
                                out.extend_from_slice(&[0xD3, 0xC0 | (extension << 3) | dst]);
                            }
                            _ => {
                                let addr = memory_location(src).ok_or_else(|| {
                                    unsupported_operand_form("i686", &allocated, inst)
                                })?;
                                if dst == 1 {
                                    return Err(CodegenError::new(
                                        "i686",
                                        "IA-32 variable shift cannot use ECX as destination with a memory count",
                                    )
                                    .with_arch("i686")
                                    .with_function(&func.name));
                                }
                                encode_load_mem(&mut out, 1, addr, 4)?;
                                out.extend_from_slice(&[0xD3, 0xC0 | (extension << 3) | dst]);
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
                                let imm = imm32(*value)?;
                                out.extend_from_slice(&[0xF7, 0xC0 | lhs_reg]);
                                out.extend_from_slice(&imm.to_le_bytes());
                            }
                            MachineOperand::Register(_) => {
                                let rhs_reg = register_number(rhs_op).ok_or_else(|| {
                                    unsupported_operand_form("i686", &allocated, inst)
                                })?;
                                out.extend_from_slice(&[0x85, 0xC0 | (rhs_reg << 3) | lhs_reg]);
                            }
                            _ => {
                                let addr = memory_location(rhs_op).ok_or_else(|| {
                                    unsupported_operand_form("i686", &allocated, inst)
                                })?;
                                out.push(0x85);
                                encode_modrm_mem(&mut out, lhs_reg, addr)?;
                            }
                        }
                    }
                    MachineInstruction::SetCc { dst, cc } => {
                        let cc_byte = condition_code(*cc);
                        if let (Some(byte_dst), Some(dst_reg)) =
                            (byte_register_number(dst), register_number(dst))
                        {
                            out.extend_from_slice(&[0x0F, 0x90 | cc_byte, 0xC0 | byte_dst]);
                            out.extend_from_slice(&[0x0F, 0xB6, 0xC0 | (dst_reg << 3) | byte_dst]);
                        } else if let Some(dst_reg) = register_number(dst) {
                            // ESI(6)/EDI(7) have no low-byte ModR/M encoding; stage through AL
                            // while preserving EAX across flag-preserving push/pop.
                            encode_push_r32(&mut out, 0);
                            out.extend_from_slice(&[0x0F, 0x90 | cc_byte, 0xC0]); // setcc al
                            out.extend_from_slice(&[0x0F, 0xB6, 0xC0 | (dst_reg << 3)]); // movzx dst, al
                            out.push(0x58); // pop eax
                        } else if let Some(addr) = memory_location(dst) {
                            let scratch = if addr.uses_reg(0) { 1 } else { 0 };
                            encode_push_r32(&mut out, scratch);
                            out.extend_from_slice(&[0x0F, 0x90 | cc_byte, 0xC0 | scratch]);
                            out.extend_from_slice(&[0x0F, 0xB6, 0xC0 | (scratch << 3) | scratch]);
                            let adjusted = if addr.base == 4 {
                                MemAddr {
                                    offset: addr.offset + 4,
                                    ..addr
                                }
                            } else {
                                addr
                            };
                            encode_store_mem(&mut out, adjusted, scratch, 4)?;
                            out.push(0x58 + scratch);
                        } else {
                            return Err(unsupported_operand_form("i686", &allocated, inst));
                        }
                    }
                    MachineInstruction::Branch { target } => {
                        let displacement = out.len() + 1;
                        out.extend_from_slice(&[0xE9, 0, 0, 0, 0]);
                        branch_fixups.push((displacement, target.clone()));
                    }
                    MachineInstruction::BranchCc { cc, target } => {
                        let displacement = out.len() + 2;
                        out.extend_from_slice(&[0x0F, 0x80 | condition_code(*cc), 0, 0, 0, 0]);
                        branch_fixups.push((displacement, target.clone()));
                    }
                    MachineInstruction::Call { target, .. } => {
                        match target {
                            MachineOperand::Symbol(name) => {
                                encode_call_rel32(&mut out, &mut relocations, name);
                            }
                            MachineOperand::Register(_) => {
                                let target = register_number(target).ok_or_else(|| {
                                    unsupported_operand_form("i686", &allocated, inst)
                                })?;
                                encode_call_r32(&mut out, target);
                            }
                            _ => return Err(unsupported_operand_form("i686", &allocated, inst)),
                        }
                        if uses_fp
                            && block.instructions[inst_pos + 1..]
                                .iter()
                                .take_while(|next| {
                                    !matches!(
                                        next,
                                        MachineInstruction::Call { .. }
                                            | MachineInstruction::Return
                                            | MachineInstruction::Branch { .. }
                                            | MachineInstruction::BranchCc { .. }
                                    )
                                })
                                .any(x87::reads_ret_pseudo)
                        {
                            x87::emit_call_result_store(&mut out, fp_base)?;
                        }
                    }
                    MachineInstruction::TlsAddress { dst, symbol } => {
                        let dst = register_number(dst)
                            .ok_or_else(|| unsupported_operand_form("i686", &allocated, inst))?;
                        // Local-exec, variant II: the thread pointer is the
                        // TCB self pointer at %gs:0 and a variable lives at a
                        // negative offset resolved by the linker (TlsLe).
                        out.extend_from_slice(&[0x65, 0x8B, (dst << 3) | 0x05, 0, 0, 0, 0]);
                        out.extend_from_slice(&[0x81, 0xC0 | dst, 0, 0, 0, 0]);
                        relocations.push(AdobRelocation::new(
                            out.len() as u64 - 4,
                            0,
                            symbol.clone(),
                            RelocationKind::TlsLe,
                            0,
                        ));
                    }
                    MachineInstruction::Push { src } => {
                        let src = register_number(src)
                            .ok_or_else(|| unsupported_operand_form("i686", &allocated, inst))?;
                        encode_push_r32(&mut out, src);
                    }
                    MachineInstruction::Pop { dst } => {
                        let dst = register_number(dst)
                            .ok_or_else(|| unsupported_operand_form("i686", &allocated, inst))?;
                        out.push(0x58 + dst);
                    }
                    MachineInstruction::ParallelMove { .. } => {
                        return Err(CodegenError::new(
                            "i686",
                            "unresolved IA-32 parallel move after allocation",
                        )
                        .with_arch("i686")
                        .with_function(&func.name));
                    }
                    _ => return Err(unsupported_instruction("i686", &allocated, inst)),
                }
            }
            if block_index + 1 == allocated.blocks.len()
                && !matches!(block.instructions.last(), Some(MachineInstruction::Return))
            {
                return Err(CodegenError::new(
                    "i686",
                    "IA-32 function must end in a return or explicit branch",
                )
                .with_arch("i686")
                .with_function(&func.name));
            }
        }

        for (displacement, target) in branch_fixups {
            let destination = *labels.get(&target).ok_or_else(|| {
                CodegenError::new("i686", format!("unknown IA-32 branch target `{target}`"))
                    .with_arch("i686")
                    .with_function(&func.name)
            })?;
            let relative = i32::try_from(i64::from(destination) - displacement as i64 - 4)
                .map_err(|_| {
                    CodegenError::new("i686", "IA-32 branch displacement exceeds 32-bit range")
                        .with_arch("i686")
                        .with_function(&func.name)
                })?;
            out[displacement..displacement + 4].copy_from_slice(&relative.to_le_bytes());
        }

        Ok((out, relocations))
    }

    fn emit_object(&mut self, module: &NativeModule) -> Result<AdobObject, CodegenError> {
        let lowered = self.lower_module(module)?;
        let module = &lowered;
        let mut obj = AdobObject::new(self.target.clone());

        obj.memory_regions.push(MemoryRegion::new(
            ".text",
            0x08048000,
            512 * 1024,
            MemoryPermissions::Rx,
        ));

        let mut text_bytes = Vec::new();
        let mut text_relocations = Vec::new();
        for func in &module.functions {
            let offset = text_bytes.len() as u64;
            let (code, relocs) = self.generate_function_with_relocations(func)?;
            let size = code.len() as u64;
            text_bytes.extend_from_slice(&code);
            for mut reloc in relocs {
                reloc.offset += offset;
                text_relocations.push(reloc);
            }

            let sym = AdobSymbol::new_defined(
                0,
                func.name.clone(),
                SymbolKind::Function,
                0,
                offset,
                size,
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
        for name in &module.imports {
            obj.add_import(name.clone());
            obj.add_symbol(
                AdobSymbol::new_undefined(0, name.clone(), SymbolKind::Import)
                    .with_binding(SymbolBinding::Global),
            );
        }
        let mut text_sec = AdobSection::new(".text", SectionKind::Text)
            .with_flags(section_flags::READ | section_flags::EXECUTE | section_flags::ALLOC)
            .with_alignment(4)
            .with_memory_region(".text")
            .with_data(text_bytes);
        text_sec.relocations = text_relocations;
        obj.add_section(text_sec);

        if !module.string_pool.is_empty() {
            let section_index = obj.sections.len() as u32;
            let mut rodata = Vec::new();
            let mut offsets: HashMap<&str, u64> = HashMap::new();
            for (index, string) in module.string_pool.iter().enumerate() {
                let offset = *offsets.entry(string.as_str()).or_insert_with(|| {
                    let offset = rodata.len() as u64;
                    rodata.extend_from_slice(string.as_bytes());
                    rodata.push(0);
                    offset
                });
                obj.add_symbol(
                    AdobSymbol::new_defined(
                        0,
                        format!("__str_{index}"),
                        SymbolKind::Object,
                        section_index,
                        offset,
                        string.len() as u64 + 1,
                    )
                    .with_binding(SymbolBinding::Local),
                );
            }
            obj.add_section(
                AdobSection::new(".rodata", SectionKind::Rodata)
                    .with_flags(section_flags::READ | section_flags::ALLOC)
                    .with_alignment(4)
                    .with_memory_region(".rodata")
                    .with_data(rodata),
            );
        }

        if !module.data_sections.is_empty() {
            let section_index = obj.sections.len() as u32;
            let mut data = Vec::new();
            for (name, initial) in &module.data_sections {
                let aligned = (data.len() + 3) & !3;
                data.resize(aligned, 0);
                let offset = data.len() as u64;
                data.extend_from_slice(initial);
                obj.add_symbol(
                    AdobSymbol::new_defined(
                        0,
                        name.clone(),
                        SymbolKind::Object,
                        section_index,
                        offset,
                        initial.len() as u64,
                    )
                    .with_binding(SymbolBinding::Global),
                );
            }
            obj.add_section(
                AdobSection::new(".data", SectionKind::Data)
                    .with_flags(section_flags::READ | section_flags::WRITE | section_flags::ALLOC)
                    .with_alignment(4)
                    .with_memory_region(".data")
                    .with_data(data),
            );
        }

        if !module.tls_sections.is_empty() {
            let section_index = obj.sections.len() as u32;
            let mut tls = Vec::new();
            for (name, initial) in &module.tls_sections {
                let aligned = (tls.len() + 7) & !7;
                tls.resize(aligned, 0);
                let offset = tls.len() as u64;
                tls.extend_from_slice(initial);
                obj.add_symbol(
                    AdobSymbol::new_defined(
                        0,
                        name.clone(),
                        SymbolKind::TLS,
                        section_index,
                        offset,
                        initial.len() as u64,
                    )
                    .with_binding(SymbolBinding::Global),
                );
            }
            obj.add_section(
                AdobSection::new(".tls", SectionKind::Tls)
                    .with_flags(
                        section_flags::READ
                            | section_flags::WRITE
                            | section_flags::TLS
                            | section_flags::ALLOC,
                    )
                    .with_alignment(8)
                    .with_data(tls),
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
                        "i686",
                        format!("unresolved relocation symbol `{}`", relocation.symbol_name),
                    )
                    .with_arch("i686")
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
    use crate::machine_ir::PhysicalRegister;

    fn target() -> TargetDescriptor {
        TargetDescriptor::from_triple("i686-unknown-linux-gnu").expect("triple")
    }

    fn move_imm(dst: MachineOperand, value: i64) -> MachineInstruction {
        MachineInstruction::Move {
            dst,
            src: MachineOperand::Immediate(value),
        }
    }

    #[test]
    fn return_only_function_encodes_ret() {
        let mut backend = I686Backend::new(target());

        let mut func = MachineFunction::new("ret");
        func.blocks[0].push(MachineInstruction::Return);

        let code = backend.generate_function(&func).expect("encodes");
        assert_eq!(code, vec![0xC3]);
    }

    #[test]
    fn object_emission_defines_strings_data_and_relocation_targets() {
        let mut backend = I686Backend::new(target());
        let mut module = NativeModule::new("sections");
        module.string_pool = vec!["hello".into(), "hello".into(), "world".into()];
        module.data_sections = vec![
            ("counter".into(), vec![1]),
            ("payload".into(), vec![2, 3, 4, 5, 6]),
        ];
        let mut func = MachineFunction::new("main");
        func.blocks[0].push(MachineInstruction::Move {
            dst: MachineOperand::phys(0),
            src: MachineOperand::Symbol("__str_1".into()),
        });
        func.blocks[0].push(MachineInstruction::Return);
        module.add_function(func);

        let obj = backend.emit_object(&module).expect("object emits");
        let rodata = obj
            .sections
            .iter()
            .find(|sec| sec.name == ".rodata")
            .unwrap();
        assert_eq!(rodata.data, b"hello\0world\0");
        let data = obj.sections.iter().find(|sec| sec.name == ".data").unwrap();
        assert_eq!(data.data, vec![1, 0, 0, 0, 2, 3, 4, 5, 6]);

        let mut strings: Vec<_> = obj
            .symbols
            .iter()
            .filter(|symbol| symbol.name.starts_with("__str_"))
            .map(|symbol| (symbol.name.as_str(), symbol.value, symbol.section_index))
            .collect();
        strings.sort();
        assert_eq!(
            strings,
            vec![
                ("__str_0", 0, Some(1)),
                ("__str_1", 0, Some(1)),
                ("__str_2", 6, Some(1)),
            ]
        );
        let payload = obj
            .symbols
            .iter()
            .find(|symbol| symbol.name == "payload")
            .unwrap();
        assert_eq!(payload.value, 4);
        let relocation = &obj.sections[0].relocations[0];
        assert_eq!(relocation.symbol_name, "__str_1");
        assert_eq!(obj.symbols[relocation.symbol as usize].name, "__str_1");
    }

    fn xmm(id: u8) -> MachineOperand {
        MachineOperand::Register(MachineRegister::Physical(PhysicalRegister::xmm(id)))
    }

    #[test]
    fn x87_add_uses_frame_slots_for_fp_pseudo_registers() {
        let mut backend = I686Backend::new(target());
        let mut func = MachineFunction::new("fadd");
        func.blocks[0].push(MachineInstruction::FAdd {
            dst: xmm(15),
            src: xmm(14),
            size: 8,
        });
        func.blocks[0].push(MachineInstruction::Return);

        let code = backend.generate_function(&func).expect("x87 add encodes");
        assert_eq!(
            code,
            vec![
                0x55, 0x89, 0xE5, 0x83, 0xEC, 0x18, // push ebp; mov ebp,esp; sub esp,24
                0xDD, 0x45, 0xF0, // fld qword [ebp-16]
                0xDC, 0x45, 0xE8, // fadd qword [ebp-24]
                0xDD, 0x5D, 0xF0, // fstp qword [ebp-16]
                0x83, 0xC4, 0x18, 0xC9, 0xC3, // add esp,24; leave; ret
            ]
        );
    }

    #[test]
    fn x87_compare_uses_fucomip_and_pops_the_stack() {
        let mut backend = I686Backend::new(target());
        let mut func = MachineFunction::new("fcmp");
        func.blocks[0].push(MachineInstruction::FCmp {
            lhs: xmm(15),
            rhs: xmm(14),
            size: 8,
        });
        func.blocks[0].push(MachineInstruction::Return);

        let code = backend
            .generate_function(&func)
            .expect("x87 compare encodes");
        assert_eq!(
            &code[6..],
            &[
                0xDD, 0x45, 0xE8, // fld rhs
                0xDD, 0x45, 0xF0, // fld lhs
                0xDF, 0xE9, // fucomip st, st(1)
                0xDD, 0xD8, // fstp st(0)
                0x83, 0xC4, 0x18, 0xC9, 0xC3,
            ]
        );
    }

    #[test]
    fn x87_return_value_is_loaded_into_st0_before_ret() {
        let mut backend = I686Backend::new(target());
        let mut func = MachineFunction::new("ret_fp");
        func.blocks[0].push(MachineInstruction::Move {
            dst: xmm(0),
            src: MachineOperand::FloatImmediate(1.0),
        });
        func.blocks[0].push(MachineInstruction::Return);

        let code = backend.generate_function(&func).expect("fp return encodes");
        let fld_ret = [0xDD, 0x45, 0xF8]; // fld qword [ebp-8]
        assert!(
            code.windows(3).any(|window| window == fld_ret),
            "missing ST0 return load in {code:02X?}"
        );
        assert!(code.ends_with(&[0x83, 0xC4, 0x18, 0xC9, 0xC3]));
    }

    #[test]
    fn single_precision_fp_is_refused() {
        let mut backend = I686Backend::new(target());
        let mut func = MachineFunction::new("f32");
        func.blocks[0].push(MachineInstruction::FAdd {
            dst: xmm(15),
            src: xmm(14),
            size: 4,
        });
        func.blocks[0].push(MachineInstruction::Return);

        let err = backend
            .generate_function(&func)
            .expect_err("f32 is not implemented on i686");
        assert!(err.reason.contains("binary64"), "{}", err.reason);
    }

    #[test]
    fn fp_operations_on_integer_registers_are_refused() {
        let mut backend = I686Backend::new(target());
        let mut func = MachineFunction::new("boom");
        func.blocks[0].push(MachineInstruction::FAdd {
            dst: MachineOperand::phys(0),
            src: MachineOperand::phys(1),
            size: 8,
        });
        func.blocks[0].push(MachineInstruction::Return);

        assert!(backend.generate_function(&func).is_err());
    }

    #[test]
    fn tls_address_is_gs_relative_local_exec_with_tpoff_relocation() {
        let mut backend = I686Backend::new(target());
        let mut func = MachineFunction::new("tls");
        func.blocks[0].push(MachineInstruction::TlsAddress {
            dst: MachineOperand::phys(2),
            symbol: "__adesh_tls_x".into(),
        });
        func.blocks[0].push(MachineInstruction::Return);

        let (code, relocs) = backend
            .generate_function_with_relocations(&func)
            .expect("TLS address encodes");
        assert_eq!(
            code,
            vec![
                0x65, 0x8B, 0x15, 0, 0, 0, 0, // mov edx, gs:[0]
                0x81, 0xC2, 0, 0, 0, 0, // add edx, tpoff32
                0xC3,
            ]
        );
        assert_eq!(relocs.len(), 1);
        assert_eq!(relocs[0].offset, 9);
        assert_eq!(relocs[0].symbol_name, "__adesh_tls_x");
        assert_eq!(relocs[0].kind, RelocationKind::TlsLe);
    }

    #[test]
    fn nop_and_return_are_accepted() {
        let mut backend = I686Backend::new(target());

        let mut func = MachineFunction::new("nop_ret");
        func.blocks[0].push(MachineInstruction::Nop);
        func.blocks[0].push(MachineInstruction::Return);

        let code = backend.generate_function(&func).expect("encodes nop+ret");
        assert_eq!(code, vec![0x90, 0xC3]);
    }

    #[test]
    fn move_immediate_to_eax_encodes_32_bit() {
        let mut backend = I686Backend::new(target());

        let mut func = MachineFunction::new("answer");
        func.blocks[0].push(move_imm(
            MachineOperand::Register(MachineRegister::Physical(PhysicalRegister::gpr(0))),
            42,
        ));
        func.blocks[0].push(MachineInstruction::Return);

        let code = backend.generate_function(&func).expect("encodes");
        assert_eq!(code, vec![0xB8, 0x2A, 0, 0, 0, 0xC3]);
    }

    #[test]
    fn virtual_move_sequence_allocates_32_bit_gpr() {
        let mut backend = I686Backend::new(target());

        let mut func = MachineFunction::new("answer_vreg");
        func.vreg_count = 1;
        func.blocks[0].push(move_imm(MachineOperand::reg(0), 42));
        func.blocks[0].push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister::gpr(0))),
            src: MachineOperand::reg(0),
        });
        func.blocks[0].push(MachineInstruction::Return);

        let code = backend.generate_function(&func).expect("encodes");
        assert_eq!(code, vec![0xBA, 0x2A, 0, 0, 0, 0x89, 0xD0, 0xC3]);
    }

    #[test]
    fn load_from_incoming_stack_argument_to_local_slot() {
        let mut backend = I686Backend::new(target());
        let mut func = MachineFunction::new("parameter");
        func.stack_size = 4;
        func.blocks[0].push(MachineInstruction::Load {
            dst: MachineOperand::StackSlot(-4),
            src: MachineOperand::Memory {
                base: MachineRegister::Physical(PhysicalRegister::gpr(5)),
                offset: 8,
                index: None,
            },
            size: 4,
        });
        func.blocks[0].push(MachineInstruction::Move {
            dst: MachineOperand::phys(0),
            src: MachineOperand::StackSlot(-4),
        });
        func.blocks[0].push(MachineInstruction::Return);

        let code = backend
            .generate_function(&func)
            .expect("parameter load encodes");
        assert_eq!(
            code,
            vec![
                0x55, 0x89, 0xE5, 0x83, 0xEC, 4, 0x8B, 0x45, 8, 0x89, 0x45, 0xFC, 0x8B, 0x45, 0xFC,
                0x83, 0xC4, 4, 0xC9, 0xC3
            ]
        );
    }

    #[test]
    fn allocated_parallel_moves_are_expanded_before_emission() {
        let mut backend = I686Backend::new(target());
        let mut func = MachineFunction::new("parallel");
        func.blocks[0].push(MachineInstruction::ParallelMove {
            moves: vec![
                crate::machine_ir::MoveOperation::new(
                    crate::machine_ir::MoveLocation::PhysicalRegister(PhysicalRegister(0)),
                    crate::machine_ir::MoveLocation::PhysicalRegister(PhysicalRegister(1)),
                    4,
                ),
                crate::machine_ir::MoveOperation::new(
                    crate::machine_ir::MoveLocation::PhysicalRegister(PhysicalRegister(1)),
                    crate::machine_ir::MoveLocation::PhysicalRegister(PhysicalRegister(0)),
                    4,
                ),
            ],
        });
        func.blocks[0].push(MachineInstruction::Return);

        let code = backend
            .generate_function(&func)
            .expect("parallel moves encode");
        assert_eq!(code, vec![0x89, 0xCA, 0x89, 0xC1, 0x89, 0xD0, 0xC3]);
    }

    #[test]
    fn virtual_multiply_returns_its_two_address_destination() {
        let mut backend = I686Backend::new(target());
        let mut func = MachineFunction::new("multiply");
        let left = func.alloc_vreg();
        let right = func.alloc_vreg();
        func.blocks[0].push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(left)),
            src: MachineOperand::imm(6),
        });
        func.blocks[0].push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(right)),
            src: MachineOperand::imm(7),
        });
        func.blocks[0].push(MachineInstruction::Mul {
            dst: MachineOperand::Register(MachineRegister::Virtual(left)),
            src: MachineOperand::Register(MachineRegister::Virtual(right)),
        });
        func.blocks[0].push(MachineInstruction::Move {
            dst: MachineOperand::phys(0),
            src: MachineOperand::Register(MachineRegister::Virtual(left)),
        });
        func.blocks[0].push(MachineInstruction::Return);

        let code = backend.generate_function(&func).expect("multiply encodes");
        assert!(
            code.windows(3).any(|window| window == [0x0F, 0xAF, 0xD3]),
            "{code:02X?}"
        );
        assert!(
            code.windows(2).any(|window| window == [0x89, 0xD0]),
            "{code:02X?}"
        );
        assert_eq!(&code[code.len() - 3..], &[0x5B, 0xC9, 0xC3]);
    }

    #[test]
    fn arithmetic_compare_and_setcc_use_32_bit_encodings() {
        let mut backend = I686Backend::new(target());
        let mut func = MachineFunction::new("arithmetic");
        let block = &mut func.blocks[0];
        block.push(MachineInstruction::Move {
            dst: MachineOperand::phys(0),
            src: MachineOperand::imm(7),
        });
        block.push(MachineInstruction::Add {
            dst: MachineOperand::phys(0),
            src: MachineOperand::imm(5),
        });
        block.push(MachineInstruction::Sub {
            dst: MachineOperand::phys(0),
            src: MachineOperand::imm(2),
        });
        block.push(MachineInstruction::Compare {
            lhs: MachineOperand::phys(0),
            rhs: MachineOperand::imm(10),
        });
        block.push(MachineInstruction::SetCc {
            dst: MachineOperand::phys(0),
            cc: ConditionCode::Equal,
        });
        block.push(MachineInstruction::Return);

        let code = backend
            .generate_function(&func)
            .expect("arithmetic encodes");
        assert_eq!(
            code,
            vec![
                0xB8, 7, 0, 0, 0, 0x83, 0xC0, 5, 0x83, 0xE8, 2, 0x83, 0xF8, 10, 0x0F, 0x94, 0xC0,
                0x0F, 0xB6, 0xC0, 0xC3,
            ]
        );
    }

    #[test]
    fn setcc_uses_the_low_byte_of_its_allocated_register() {
        let mut backend = I686Backend::new(target());
        let mut func = MachineFunction::new("condition");
        let value = func.alloc_vreg();
        func.blocks[0].push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(value)),
            src: MachineOperand::imm(3),
        });
        func.blocks[0].push(MachineInstruction::Compare {
            lhs: MachineOperand::Register(MachineRegister::Virtual(value)),
            rhs: MachineOperand::imm(10),
        });
        func.blocks[0].push(MachineInstruction::SetCc {
            dst: MachineOperand::Register(MachineRegister::Virtual(value)),
            cc: ConditionCode::LessThan,
        });
        func.blocks[0].push(MachineInstruction::Return);

        let code = backend.generate_function(&func).expect("condition encodes");
        assert_eq!(
            code,
            vec![
                0xBA, 3, 0, 0, 0, 0x83, 0xFA, 10, 0x0F, 0x9C, 0xC2, 0x0F, 0xB6, 0xD2, 0xC3,
            ]
        );
    }

    #[test]
    fn conditional_branch_fixup_preserves_block_labels() {
        let mut backend = I686Backend::new(target());
        let mut func = MachineFunction::new("branch");
        func.blocks[0].push(MachineInstruction::Move {
            dst: MachineOperand::phys(0),
            src: MachineOperand::imm(0),
        });
        func.blocks[0].push(MachineInstruction::Compare {
            lhs: MachineOperand::phys(0),
            rhs: MachineOperand::imm(0),
        });
        func.blocks[0].push(MachineInstruction::BranchCc {
            cc: ConditionCode::Equal,
            target: "done".into(),
        });
        let done = func.create_block("done");
        func.blocks[done as usize].push(MachineInstruction::Return);

        let code = backend.generate_function(&func).expect("branch encodes");
        assert_eq!(&code[..3], &[0x55, 0x89, 0xE5]);
        assert_eq!(&code[code.len() - 2..], &[0xC9, 0xC3]);
        let branch = code
            .windows(2)
            .position(|window| window == [0x0F, 0x84])
            .expect("near JE");
        assert_eq!(&code[branch + 2..branch + 6], &[0, 0, 0, 0]);
    }

    #[test]
    fn register_pressure_spills_and_preserves_callee_saved_registers() {
        let mut backend = I686Backend::new(target());
        let mut func = MachineFunction::new("pressure");
        let values: Vec<_> = (0..5).map(|_| func.alloc_vreg()).collect();
        let block = &mut func.blocks[0];
        block.push(MachineInstruction::Move {
            dst: MachineOperand::phys(0),
            src: MachineOperand::imm(0),
        });
        for value in &values {
            block.push(MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Virtual(*value)),
                src: MachineOperand::imm(1),
            });
        }
        for value in &values {
            block.push(MachineInstruction::Add {
                dst: MachineOperand::phys(0),
                src: MachineOperand::Register(MachineRegister::Virtual(*value)),
            });
        }
        block.push(MachineInstruction::Return);

        let code = backend
            .generate_function(&func)
            .expect("pressure encodes with spills");
        // Locals/spills are allocated BEFORE callee-saved pushes so [ebp-4] never overlaps saved regs.
        assert_eq!(&code[..6], &[0x55, 0x89, 0xE5, 0x83, 0xEC, 0x04]);
        assert_eq!(&code[6..9], &[0x53, 0x56, 0x57]);
        assert!(
            code.windows(2).any(|window| window == [0x8B, 0x4D]),
            "{code:02X?}"
        );
        assert!(
            code.windows(3).any(|window| window == [0x5F, 0x5E, 0x5B]),
            "{code:02X?}"
        );
    }

    #[test]
    fn wide_immediate_is_refused() {
        let mut backend = I686Backend::new(target());

        let mut func = MachineFunction::new("wide");
        func.blocks[0].push(move_imm(
            MachineOperand::Register(MachineRegister::Physical(PhysicalRegister::gpr(0))),
            i64::from(u32::MAX) + 1,
        ));

        let err = backend
            .generate_function(&func)
            .expect_err("wide immediate refused");
        assert!(err.reason.contains("32-bit"), "{}", err.reason);
    }

    #[test]
    fn call_emits_rel32_without_invented_pushes() {
        let mut backend = I686Backend::new(target());

        let mut func = MachineFunction::new("caller");
        func.blocks[0].push(MachineInstruction::Call {
            target: MachineOperand::Symbol("callee".into()),
            num_args: 2,
        });
        func.blocks[0].push(MachineInstruction::Return);

        let (code, relocs) = backend
            .generate_function_with_relocations(&func)
            .expect("call encodes");
        // Frame setup aligns ESP to 16 bytes before the `call` instruction.
        assert_eq!(
            code,
            vec![
                0x55, 0x89, 0xE5, 0x83, 0xEC, 8, 0xE8, 0, 0, 0, 0, 0x83, 0xC4, 8, 0xC9, 0xC3
            ]
        );
        assert_eq!(relocs.len(), 1);
        assert_eq!(relocs[0].symbol_name, "callee");
        assert_eq!(relocs[0].kind, RelocationKind::PcRelative32);
    }

    #[test]
    fn esp_adjust_and_stack_store_encode_ia32() {
        let mut backend = I686Backend::new(target());

        let mut func = MachineFunction::new("stack_arg");
        func.blocks[0].push(MachineInstruction::Sub {
            dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister::gpr(4))),
            src: MachineOperand::Immediate(16),
        });
        func.blocks[0].push(MachineInstruction::Store {
            dst: MachineOperand::Memory {
                base: MachineRegister::Physical(PhysicalRegister::gpr(4)),
                offset: 0,
                index: None,
            },
            src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister::gpr(0))),
            size: 4,
        });
        func.blocks[0].push(MachineInstruction::Call {
            target: MachineOperand::Symbol("f".into()),
            num_args: 1,
        });
        func.blocks[0].push(MachineInstruction::Add {
            dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister::gpr(4))),
            src: MachineOperand::Immediate(16),
        });
        func.blocks[0].push(MachineInstruction::Return);

        let code = backend.generate_function(&func).expect("encodes");
        assert_eq!(
            code,
            vec![
                0x55, 0x89, 0xE5, // push ebp; mov ebp, esp
                0x83, 0xEC, 0x08, // sub esp, 8 (16-byte align for call)
                0x83, 0xEC, 0x10, // sub esp, 16
                0x89, 0x04, 0x24, // mov [esp], eax
                0xE8, 0, 0, 0, 0, // call f
                0x83, 0xC4, 0x10, // add esp, 16
                0x83, 0xC4, 0x08, // add esp, 8
                0xC9, // leave
                0xC3, // ret
            ]
        );
    }

    #[test]
    fn wide_store_is_refused_without_truncation() {
        let mut backend = I686Backend::new(target());
        let mut func = MachineFunction::new("wide_store");
        func.blocks[0].push(MachineInstruction::Store {
            dst: MachineOperand::Memory {
                base: MachineRegister::Physical(PhysicalRegister::gpr(4)),
                offset: 0,
                index: None,
            },
            src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister::gpr(0))),
            size: 8,
        });

        let err = backend
            .generate_function(&func)
            .expect_err("wide stores must not be narrowed");
        assert!(err.reason.contains("8 bytes"), "{}", err.reason);
    }

    #[test]
    fn empty_tracked_scopes_do_not_require_runtime_calls() {
        let mut backend = I686Backend::new(target());
        let mut module = NativeModule::new("empty_scope");
        let mut func = MachineFunction::new("answer");
        func.blocks[0].push(MachineInstruction::Sub {
            dst: MachineOperand::phys(4),
            src: MachineOperand::imm(16),
        });
        func.blocks[0].push(MachineInstruction::ParallelMove { moves: vec![] });
        func.blocks[0].push(MachineInstruction::Call {
            target: MachineOperand::Symbol("adesh_rt_scope_enter_i686".into()),
            num_args: 1,
        });
        func.blocks[0].push(MachineInstruction::Add {
            dst: MachineOperand::phys(4),
            src: MachineOperand::imm(16),
        });
        func.blocks[0].push(MachineInstruction::Return);
        module.add_function(func);
        module.imports.push("adesh_rt_scope_enter_i686".into());

        let lowered = backend.lower_module(&module).expect("lower");
        assert!(lowered.imports.is_empty());
        assert!(matches!(
            lowered.functions[0].blocks[0].instructions.as_slice(),
            [MachineInstruction::Return]
        ));
    }

    #[test]
    fn object_preserves_import_and_resolves_call_symbol() {
        let mut backend = I686Backend::new(target());
        let mut module = NativeModule::new("imports");
        let mut func = MachineFunction::new("caller");
        func.blocks[0].push(MachineInstruction::Call {
            target: MachineOperand::Symbol("callee".into()),
            num_args: 0,
        });
        func.blocks[0].push(MachineInstruction::Return);
        module.add_function(func);
        module.imports.push("callee".into());

        let object = backend.emit_object(&module).expect("object");
        assert_eq!(object.imports, vec!["callee"]);
        let import = object
            .symbols
            .iter()
            .find(|symbol| symbol.name == "callee")
            .expect("import symbol");
        let relocation = &object.sections[0].relocations[0];
        assert_eq!(relocation.symbol, import.id);
    }

    #[test]
    fn setcc_on_esi_uses_al_without_corrupting_dh() {
        let mut backend = I686Backend::new(target());
        let mut func = MachineFunction::new("setcc_esi");
        func.blocks[0].push(MachineInstruction::Compare {
            lhs: MachineOperand::phys(6), // ESI
            rhs: MachineOperand::imm(5),
        });
        func.blocks[0].push(MachineInstruction::SetCc {
            dst: MachineOperand::phys(6),
            cc: ConditionCode::Equal,
        });
        func.blocks[0].push(MachineInstruction::Return);

        let code = backend.generate_function(&func).expect("encodes");
        // push eax; sete al; movzx esi, al; pop eax
        assert!(
            code.windows(8)
                .any(|w| w == [0x50, 0x0F, 0x94, 0xC0, 0x0F, 0xB6, 0xF0, 0x58]),
            "expected safe AL staging for ESI SetCc, got {code:02X?}"
        );
    }

    #[test]
    fn div_and_mod_preserve_live_edx() {
        let mut backend = I686Backend::new(target());
        let mut func = MachineFunction::new("div_save_edx");
        func.blocks[0].push(MachineInstruction::Div {
            dst: MachineOperand::phys(3), // EBX
            src: MachineOperand::phys(2), // EDX
        });
        func.blocks[0].push(MachineInstruction::Return);

        let code = backend.generate_function(&func).expect("encodes");
        // mov ecx, edx; mov eax, ebx; push edx; cdq; idiv ecx; mov ebx, eax; pop edx; ret
        assert_eq!(
            code,
            vec![
                0x89, 0xD1, 0x89, 0xD8, 0x52, 0x99, 0xF7, 0xF9, 0x89, 0xC3, 0x5A, 0xC3
            ]
        );
    }

    #[test]
    fn mul_immediate_uses_imul_without_clobbering_ecx() {
        let mut backend = I686Backend::new(target());
        let mut func = MachineFunction::new("mul_imm");
        func.blocks[0].push(MachineInstruction::Mul {
            dst: MachineOperand::phys(3), // EBX
            src: MachineOperand::imm(10),
        });
        func.blocks[0].push(MachineInstruction::Return);

        let code = backend.generate_function(&func).expect("encodes");
        // imul ebx, ebx, 10 => 6B DB 0A; ret
        assert_eq!(code, vec![0x6B, 0xDB, 0x0A, 0xC3]);
    }

    #[test]
    fn test_immediate_uses_f7_not_add() {
        let mut backend = I686Backend::new(target());
        let mut func = MachineFunction::new("test_imm");
        func.blocks[0].push(MachineInstruction::Test {
            lhs: MachineOperand::phys(2), // EDX
            rhs: MachineOperand::imm(1),
        });
        func.blocks[0].push(MachineInstruction::Return);

        let code = backend.generate_function(&func).expect("encodes");
        // test edx, 1 => F7 C2 01 00 00 00; ret
        assert_eq!(code, vec![0xF7, 0xC2, 1, 0, 0, 0, 0xC3]);
    }

    #[test]
    fn load_store_8bit_16bit_and_sib_indexed_encode_correctly() {
        let mut backend = I686Backend::new(target());
        let mut func = MachineFunction::new("mem_widths");
        func.blocks[0].push(MachineInstruction::Load {
            dst: MachineOperand::phys(0), // EAX
            src: MachineOperand::Memory {
                base: MachineRegister::Physical(PhysicalRegister::gpr(3)), // EBX
                offset: 4,
                index: Some((MachineRegister::Physical(PhysicalRegister::gpr(6)), 4)), // ESI*4
            },
            size: 1,
        });
        func.blocks[0].push(MachineInstruction::Load {
            dst: MachineOperand::phys(2), // EDX
            src: MachineOperand::Memory {
                base: MachineRegister::Physical(PhysicalRegister::gpr(3)),
                offset: 0,
                index: None,
            },
            size: 2,
        });
        func.blocks[0].push(MachineInstruction::Store {
            dst: MachineOperand::Memory {
                base: MachineRegister::Physical(PhysicalRegister::gpr(3)),
                offset: 8,
                index: None,
            },
            src: MachineOperand::phys(6), // ESI 8-bit store must stage through AL/CL
            size: 1,
        });
        func.blocks[0].push(MachineInstruction::Store {
            dst: MachineOperand::Memory {
                base: MachineRegister::Physical(PhysicalRegister::gpr(3)),
                offset: 12,
                index: None,
            },
            src: MachineOperand::phys(2),
            size: 2,
        });
        func.blocks[0].push(MachineInstruction::Return);

        let code = backend.generate_function(&func).expect("encodes");
        assert_eq!(
            code,
            vec![
                0x0F, 0xB6, 0x44, 0xB3, 0x04, // movzx eax, byte [ebx + esi*4 + 4]
                0x0F, 0xB7, 0x13, // movzx edx, word [ebx]
                0x50, 0x89, 0xF0, 0x88, 0x43, 0x08,
                0x58, // push eax; mov eax, esi; mov [ebx+8], al; pop eax
                0x66, 0x89, 0x53, 0x0C, // mov word [ebx+12], dx
                0xC3, // ret
            ]
        );
    }

    #[test]
    fn resolver_allows_esi_and_edi_registers() {
        let resolver = i686_resolver();
        let moves = vec![
            crate::machine_ir::MoveOperation::new(
                crate::machine_ir::MoveLocation::PhysicalRegister(PhysicalRegister(6)),
                crate::machine_ir::MoveLocation::PhysicalRegister(PhysicalRegister(7)),
                4,
            ),
            crate::machine_ir::MoveOperation::new(
                crate::machine_ir::MoveLocation::PhysicalRegister(PhysicalRegister(7)),
                crate::machine_ir::MoveLocation::PhysicalRegister(PhysicalRegister(6)),
                4,
            ),
        ];
        let expanded = resolver
            .resolve_moves(&moves)
            .expect("ESI/EDI cycle should resolve using scratch");
        assert!(!expanded.is_empty());
    }

    #[test]
    fn frame_pointer_read_and_callee_pops_ret_encode_correctly() {
        let mut backend = I686Backend::new(target());
        let mut func = MachineFunction::new("sret_ret4");
        func.callee_pops_ret_bytes = 4;
        func.blocks[0].push(MachineInstruction::Move {
            dst: MachineOperand::phys(0), // EAX
            src: MachineOperand::phys(5), // EBP
        });
        func.blocks[0].push(MachineInstruction::Return);

        let code = backend.generate_function(&func).expect("encodes");
        // mov eax, ebp (89 E8); ret 4 (C2 04 00)
        assert_eq!(code, vec![0x89, 0xE8, 0xC2, 0x04, 0x00]);
    }

    #[test]
    fn unsupported_8bit_ebp_read_fails_loudly() {
        let mut backend = I686Backend::new(target());
        let mut func = MachineFunction::new("bad_bp8");
        func.blocks[0].push(MachineInstruction::Load {
            dst: MachineOperand::phys(0),
            src: MachineOperand::phys(5),
            size: 1,
        });
        func.blocks[0].push(MachineInstruction::Return);

        let err = backend
            .generate_function(&func)
            .expect_err("8-bit read from EBP must be refused");
        assert!(
            err.to_string()
                .contains("unsupported IA-32 register operand")
        );
    }
}
