//! x87 floating-point emission for the IA-32 backend.
//!
//! IA-32 has no allocatable FP registers in this backend. Floating-point
//! virtual registers are always spilled, so after allocation every FP value is
//! a frame slot or memory operand. The allocator's FP scratch pseudo-registers
//! (`xmm15`, `xmm14`) and the FP return transfer register (`xmm0`) are mapped to
//! three dedicated 8-byte frame slots. Each operation is a short
//! load / operate / store x87 sequence, so the x87 register stack is empty at
//! every instruction boundary and across calls.
//!
//! Values are IEEE-754 binary64 only; `f32` is refused because the x87 ST(0)
//! return convention cannot carry a bit-exact binary32 payload through the
//! 8-byte transfer slot.

use super::{MemAddr, encode_modrm_mem, imm32, memory_location};
use crate::error::CodegenError;
use crate::machine_ir::{MachineInstruction, MachineOperand, MachineRegister, PhysicalRegister};

pub(super) const FP_PSEUDO_SLOTS: i32 = 3;
pub(super) const FP_PSEUDO_BYTES: i32 = FP_PSEUDO_SLOTS * 8;

const XMM_RET: u8 = 16;

fn fp_error(message: impl Into<String>) -> CodegenError {
    CodegenError::new("i686", message.into()).with_arch("i686")
}

fn pseudo_slot(reg: PhysicalRegister) -> Option<i32> {
    match reg.0 {
        16 => Some(0),
        31 => Some(1),
        30 => Some(2),
        _ => None,
    }
}

pub(super) fn is_fp_pseudo(op: &MachineOperand) -> bool {
    matches!(
        op,
        MachineOperand::Register(MachineRegister::Physical(reg)) if reg.0 >= 16
    )
}

pub(super) fn is_ret_pseudo(op: &MachineOperand) -> bool {
    matches!(
        op,
        MachineOperand::Register(MachineRegister::Physical(reg)) if reg.0 == XMM_RET
    )
}

pub(super) fn is_fp_instruction(inst: &MachineInstruction) -> bool {
    match inst {
        MachineInstruction::FAdd { .. }
        | MachineInstruction::FSub { .. }
        | MachineInstruction::FMul { .. }
        | MachineInstruction::FDiv { .. }
        | MachineInstruction::FNeg { .. }
        | MachineInstruction::FCmp { .. }
        | MachineInstruction::FCvtIntToFloat { .. }
        | MachineInstruction::FCvtFloatToInt { .. }
        | MachineInstruction::FCvtFloatToFloat { .. } => true,
        MachineInstruction::Move { dst, src } => {
            is_fp_pseudo(dst)
                || is_fp_pseudo(src)
                || matches!(src, MachineOperand::FloatImmediate(_))
        }
        MachineInstruction::Load { dst, .. } => is_fp_pseudo(dst),
        MachineInstruction::Store { src, .. } => is_fp_pseudo(src),
        _ => false,
    }
}

/// True when `inst` consumes the FP return transfer register.
pub(super) fn reads_ret_pseudo(inst: &MachineInstruction) -> bool {
    match inst {
        MachineInstruction::Move { src, .. } | MachineInstruction::Store { src, .. } => {
            is_ret_pseudo(src)
        }
        MachineInstruction::FAdd { dst, src, .. }
        | MachineInstruction::FSub { dst, src, .. }
        | MachineInstruction::FMul { dst, src, .. }
        | MachineInstruction::FDiv { dst, src, .. } => is_ret_pseudo(dst) || is_ret_pseudo(src),
        MachineInstruction::FNeg { dst, .. } => is_ret_pseudo(dst),
        MachineInstruction::FCmp { lhs, rhs, .. } => is_ret_pseudo(lhs) || is_ret_pseudo(rhs),
        MachineInstruction::FCvtFloatToInt { src, .. }
        | MachineInstruction::FCvtFloatToFloat { src, .. } => is_ret_pseudo(src),
        _ => false,
    }
}

/// True when `inst` produces a value into the FP return transfer register.
pub(super) fn writes_ret_pseudo(inst: &MachineInstruction) -> bool {
    match inst {
        MachineInstruction::Move { dst, .. } | MachineInstruction::Load { dst, .. } => {
            is_ret_pseudo(dst)
        }
        MachineInstruction::FAdd { dst, .. }
        | MachineInstruction::FSub { dst, .. }
        | MachineInstruction::FMul { dst, .. }
        | MachineInstruction::FDiv { dst, .. }
        | MachineInstruction::FNeg { dst, .. }
        | MachineInstruction::FCvtIntToFloat { dst, .. } => is_ret_pseudo(dst),
        _ => false,
    }
}

pub(super) fn ret_pseudo_addr(fp_base: i32) -> MemAddr {
    MemAddr {
        base: 5,
        offset: -(fp_base + 8),
        index: None,
    }
}

fn fp_addr(op: &MachineOperand, fp_base: i32) -> Result<MemAddr, CodegenError> {
    if let MachineOperand::Register(MachineRegister::Physical(reg)) = op {
        let slot = pseudo_slot(*reg)
            .ok_or_else(|| fp_error(format!("unsupported IA-32 FP register id {}", reg.0)))?;
        return Ok(MemAddr {
            base: 5,
            offset: -(fp_base + 8 * (slot + 1)),
            index: None,
        });
    }
    memory_location(op).ok_or_else(|| fp_error("unsupported IA-32 floating-point operand"))
}

fn high_half(addr: MemAddr) -> MemAddr {
    MemAddr {
        offset: addr.offset + 4,
        ..addr
    }
}

fn mem_op(out: &mut Vec<u8>, opcode: u8, ext: u8, addr: MemAddr) -> Result<(), CodegenError> {
    out.push(opcode);
    encode_modrm_mem(out, ext, addr)
}

fn fld_m64(out: &mut Vec<u8>, addr: MemAddr) -> Result<(), CodegenError> {
    mem_op(out, 0xDD, 0, addr)
}

fn fstp_m64(out: &mut Vec<u8>, addr: MemAddr) -> Result<(), CodegenError> {
    mem_op(out, 0xDD, 3, addr)
}

fn fild_m64(out: &mut Vec<u8>, addr: MemAddr) -> Result<(), CodegenError> {
    mem_op(out, 0xDF, 5, addr)
}

fn fistp_m64(out: &mut Vec<u8>, addr: MemAddr) -> Result<(), CodegenError> {
    mem_op(out, 0xDF, 7, addr)
}

/// Bit-exact 8-byte memory copy. `fild`/`fistp` of a 64-bit integer round-trips
/// every bit pattern (including signalling NaNs) without needing a GPR.
fn copy8(out: &mut Vec<u8>, dst: MemAddr, src: MemAddr) -> Result<(), CodegenError> {
    fild_m64(out, src)?;
    fistp_m64(out, dst)
}

fn store_bits(out: &mut Vec<u8>, dst: MemAddr, bits: u64) -> Result<(), CodegenError> {
    for (addr, word) in [(dst, bits as u32), (high_half(dst), (bits >> 32) as u32)] {
        out.push(0xC7);
        encode_modrm_mem(out, 0, addr)?;
        out.extend_from_slice(&word.to_le_bytes());
    }
    Ok(())
}

fn require_f64(size: u8, what: &str) -> Result<(), CodegenError> {
    if size == 8 {
        Ok(())
    } else {
        Err(fp_error(format!(
            "IA-32 {what} of {size} bytes is not implemented (only binary64 is supported on i686)"
        )))
    }
}

fn lea_esp(out: &mut Vec<u8>, delta: i8) {
    out.extend_from_slice(&[0x8D, 0x64, 0x24, delta as u8]);
}

/// Emits x87 code for `inst` when it is a floating-point operation. Returns
/// `Ok(true)` when the instruction was consumed.
pub(super) fn emit_fp_instruction(
    out: &mut Vec<u8>,
    inst: &MachineInstruction,
    fp_base: i32,
) -> Result<bool, CodegenError> {
    match inst {
        MachineInstruction::Move { dst, src } if is_fp_instruction(inst) => {
            let dst = fp_addr(dst, fp_base)?;
            match src {
                MachineOperand::FloatImmediate(value) => store_bits(out, dst, value.to_bits())?,
                MachineOperand::Immediate(value) => {
                    let low = imm32(*value)? as u32;
                    store_bits(out, dst, u64::from(low))?;
                }
                other => copy8(out, dst, fp_addr(other, fp_base)?)?,
            }
            Ok(true)
        }
        MachineInstruction::Load { dst, src, size: 8 }
            if !is_fp_pseudo(dst)
                && memory_location(dst).is_some()
                && memory_location(src).is_some() =>
        {
            copy8(out, fp_addr(dst, fp_base)?, fp_addr(src, fp_base)?)?;
            Ok(true)
        }
        MachineInstruction::Load { dst, src, size } if is_fp_pseudo(dst) => {
            require_f64(*size, "floating-point load")?;
            copy8(out, fp_addr(dst, fp_base)?, fp_addr(src, fp_base)?)?;
            Ok(true)
        }
        MachineInstruction::Store { dst, src, size } if is_fp_pseudo(src) => {
            require_f64(*size, "floating-point store")?;
            copy8(out, fp_addr(dst, fp_base)?, fp_addr(src, fp_base)?)?;
            Ok(true)
        }
        MachineInstruction::FAdd { dst, src, size }
        | MachineInstruction::FSub { dst, src, size }
        | MachineInstruction::FMul { dst, src, size }
        | MachineInstruction::FDiv { dst, src, size } => {
            require_f64(*size, "floating-point arithmetic")?;
            let ext = match inst {
                MachineInstruction::FAdd { .. } => 0,
                MachineInstruction::FMul { .. } => 1,
                MachineInstruction::FSub { .. } => 4,
                _ => 6,
            };
            let dst = fp_addr(dst, fp_base)?;
            let src = fp_addr(src, fp_base)?;
            fld_m64(out, dst)?;
            mem_op(out, 0xDC, ext, src)?;
            fstp_m64(out, dst)?;
            Ok(true)
        }
        MachineInstruction::FNeg { dst, size } => {
            require_f64(*size, "floating-point negation")?;
            let dst = high_half(fp_addr(dst, fp_base)?);
            // xor byte [dst+7], 0x80 flips the sign without touching NaN payloads.
            let sign = MemAddr {
                offset: dst.offset + 3,
                ..dst
            };
            out.push(0x80);
            encode_modrm_mem(out, 6, sign)?;
            out.push(0x80);
            Ok(true)
        }
        MachineInstruction::FCmp { lhs, rhs, size } => {
            require_f64(*size, "floating-point comparison")?;
            let lhs = fp_addr(lhs, fp_base)?;
            let rhs = fp_addr(rhs, fp_base)?;
            fld_m64(out, rhs)?;
            fld_m64(out, lhs)?;
            out.extend_from_slice(&[0xDF, 0xE9]); // fucomip st, st(1)
            out.extend_from_slice(&[0xDD, 0xD8]); // fstp st(0)
            Ok(true)
        }
        MachineInstruction::FCvtIntToFloat {
            dst,
            src,
            is_f64,
            is_signed,
        } => {
            require_f64(if *is_f64 { 8 } else { 4 }, "integer to float conversion")?;
            let dst = fp_addr(dst, fp_base)?;
            emit_int_to_fp(out, src, *is_signed)?;
            fstp_m64(out, dst)?;
            Ok(true)
        }
        MachineInstruction::FCvtFloatToInt {
            dst, src, is_f64, ..
        } => {
            require_f64(if *is_f64 { 8 } else { 4 }, "float to integer conversion")?;
            let src = fp_addr(src, fp_base)?;
            fld_m64(out, src)?;
            // 12 bytes: [esp] result, [esp+8] saved control word, [esp+10] truncating control word.
            out.extend_from_slice(&[0x8D, 0x64, 0x24, 0xF4]); // lea esp,[esp-12]
            out.extend_from_slice(&[0xD9, 0x7C, 0x24, 0x08]); // fnstcw [esp+8]
            out.extend_from_slice(&[0xD9, 0x7C, 0x24, 0x0A]); // fnstcw [esp+10]
            out.extend_from_slice(&[0x66, 0x81, 0x4C, 0x24, 0x0A, 0x00, 0x0C]); // or word [esp+10], 0x0C00
            out.extend_from_slice(&[0xD9, 0x6C, 0x24, 0x0A]); // fldcw [esp+10]
            out.extend_from_slice(&[0xDF, 0x7C, 0x24, 0x00]); // fistp qword [esp]
            out.extend_from_slice(&[0xD9, 0x6C, 0x24, 0x08]); // fldcw [esp+8]
            emit_pop_result(out, dst)?;
            Ok(true)
        }
        MachineInstruction::FCvtFloatToFloat { .. } => Err(fp_error(
            "IA-32 float width conversion is not implemented (only binary64 is supported on i686)",
        )),
        _ => Ok(false),
    }
}

fn emit_int_to_fp(
    out: &mut Vec<u8>,
    src: &MachineOperand,
    is_signed: bool,
) -> Result<(), CodegenError> {
    match src {
        MachineOperand::Register(MachineRegister::Physical(reg)) if reg.0 < 8 => {
            if !is_signed {
                out.extend_from_slice(&[0x6A, 0x00]); // push 0
            }
            out.push(0x50 + reg.0);
        }
        MachineOperand::Immediate(value) => {
            if !is_signed {
                out.extend_from_slice(&[0x6A, 0x00]);
            }
            out.push(0x68);
            out.extend_from_slice(&imm32(*value)?.to_le_bytes());
        }
        other => {
            let addr = memory_location(other)
                .ok_or_else(|| fp_error("unsupported IA-32 integer-to-float source"))?;
            out.push(0x8B); // mov eax, [src]
            encode_modrm_mem(out, 0, addr)?;
            if !is_signed {
                out.extend_from_slice(&[0x6A, 0x00]);
            }
            out.push(0x50);
        }
    }
    if is_signed {
        out.extend_from_slice(&[0xDB, 0x04, 0x24]); // fild dword [esp]
        lea_esp(out, 4);
    } else {
        out.extend_from_slice(&[0xDF, 0x2C, 0x24]); // fild qword [esp]
        lea_esp(out, 8);
    }
    Ok(())
}

fn emit_pop_result(out: &mut Vec<u8>, dst: &MachineOperand) -> Result<(), CodegenError> {
    let target = match dst {
        MachineOperand::Register(MachineRegister::Physical(reg)) if reg.0 < 8 => Some(reg.0),
        _ => None,
    };
    let reg = target.unwrap_or(0);
    out.extend_from_slice(&[0x8B, 0x04 | (reg << 3), 0x24]); // mov reg, [esp]
    lea_esp(out, 12);
    if target.is_none() {
        let addr = memory_location(dst)
            .ok_or_else(|| fp_error("unsupported IA-32 float-to-integer destination"))?;
        out.push(0x89);
        encode_modrm_mem(out, reg, addr)?;
    }
    Ok(())
}

/// `fld qword [xmm0 slot]` — places the FP return value in ST(0) before `ret`.
pub(super) fn emit_return_load(out: &mut Vec<u8>, fp_base: i32) -> Result<(), CodegenError> {
    fld_m64(out, ret_pseudo_addr(fp_base))
}

/// `fstp qword [xmm0 slot]` — captures an ST(0) call result.
pub(super) fn emit_call_result_store(out: &mut Vec<u8>, fp_base: i32) -> Result<(), CodegenError> {
    fstp_m64(out, ret_pseudo_addr(fp_base))
}

/// Fuses the resolver's `Load gpr <- [A]; Store [B] <- gpr` 8-byte memory copy
/// (emitted for FP call arguments) into FP pseudo-register traffic.
pub(super) fn fuse_wide_copies(instructions: Vec<MachineInstruction>) -> Vec<MachineInstruction> {
    let mut result = Vec::with_capacity(instructions.len());
    let mut iter = instructions.into_iter().peekable();
    while let Some(inst) = iter.next() {
        if let MachineInstruction::Load {
            dst: MachineOperand::Register(MachineRegister::Physical(gpr)),
            src,
            size: 8,
        } = &inst
            && gpr.0 < 8
            && let Some(MachineInstruction::Store {
                dst: store_dst,
                src: MachineOperand::Register(MachineRegister::Physical(store_src)),
                size: 8,
            }) = iter.peek()
            && store_src == gpr
        {
            let scratch =
                MachineOperand::Register(MachineRegister::Physical(PhysicalRegister::xmm(15)));
            result.push(MachineInstruction::Load {
                dst: scratch.clone(),
                src: src.clone(),
                size: 8,
            });
            result.push(MachineInstruction::Store {
                dst: store_dst.clone(),
                src: scratch,
                size: 8,
            });
            iter.next();
            continue;
        }
        result.push(inst);
    }
    result
}
