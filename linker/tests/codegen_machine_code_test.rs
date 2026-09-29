//! Comprehensive test suite for Phase 2 bit-level Machine Code Encoders.

use adesh_linker::codegen::aarch64::{ACond, AReg, Aarch64Encoder};
use adesh_linker::codegen::riscv::{RReg, RiscvEncoder};
use adesh_linker::codegen::x86_64::{Condition, Reg64, X86_64Encoder};

#[test]
fn test_x86_64_instruction_encoding() {
    let mut enc = X86_64Encoder::new();

    // 1. Function prologue: push rbp; mov rbp, rsp; sub rsp, 32
    enc.push_reg(Reg64::Rbp);
    enc.mov_reg_reg(Reg64::Rbp, Reg64::Rsp);
    enc.sub_reg_imm32(Reg64::Rsp, 32);

    // 2. Data load: mov rax, 0x1122334455667788
    enc.mov_reg_imm64(Reg64::Rax, 0x1122334455667788);

    // 3. Arithmetic: add rax, rbx; xor rcx, rcx
    enc.add_reg_reg(Reg64::Rax, Reg64::Rbx);
    enc.xor_reg_reg(Reg64::Rcx, Reg64::Rcx);

    // 4. Memory: mov [rbp - 8], rax; mov rdx, [rbp - 8]
    enc.mov_mem_reg(Reg64::Rbp, -8, Reg64::Rax);
    enc.mov_reg_mem(Reg64::Rdx, Reg64::Rbp, -8);

    // 5. Comparison and Branching: cmp rax, rdx; jcc Equal, 16; jmp -10
    enc.cmp_reg_reg(Reg64::Rax, Reg64::Rdx);
    enc.jcc_rel32(Condition::Equal, 16);
    enc.jmp_rel32(-10);

    // 6. Epilogue: mov rsp, rbp; pop rbp; ret
    enc.mov_reg_reg(Reg64::Rsp, Reg64::Rbp);
    enc.pop_reg(Reg64::Rbp);
    enc.ret();

    assert!(!enc.is_empty());
    assert_eq!(enc.code[0], 0x55); // push rbp
    assert_eq!(&enc.code[1..4], &[0x48, 0x89, 0xE5]); // mov rbp, rsp
    assert_eq!(*enc.code.last().unwrap(), 0xC3); // ret
}

#[test]
fn test_aarch64_instruction_encoding() {
    let mut enc = Aarch64Encoder::new();

    // 1. STP x29, x30, [sp, #-16]! (or sub sp, sp, #16)
    enc.sub_imm(AReg::Sp, AReg::Sp, 16);

    // 2. movz x0, #0x1234, LSL #0; movk x0, #0x5678, LSL #16
    enc.movz(AReg::X0, 0x1234, 0);
    enc.movk(AReg::X0, 0x5678, 16);

    // 3. add x1, x0, x2; sub x3, x1, #8
    enc.add_reg(AReg::X1, AReg::X0, AReg::X2);
    enc.sub_imm(AReg::X3, AReg::X1, 8);

    // 4. str x0, [sp, #0]; ldr x4, [sp, #0]
    enc.str_imm(AReg::X0, AReg::Sp, 0);
    enc.ldr_imm(AReg::X4, AReg::Sp, 0);

    // 5. b.eq +8; b -4; ret
    enc.b_cond(ACond::Eq, 2);
    enc.b(-1);
    enc.ret();

    assert_eq!(enc.code.len() % 4, 0); // 32-bit fixed width
    let last_insn = u32::from_le_bytes(enc.code[enc.code.len() - 4..].try_into().unwrap());
    assert_eq!(last_insn, 0xD65F_03C0); // RET
}

#[test]
fn test_riscv_instruction_encoding() {
    let mut enc = RiscvEncoder::new();

    // 1. addi sp, sp, -16
    enc.addi(RReg::Sp, RReg::Sp, -16);

    // 2. lui a0, 0x10000; addi a0, a0, 0x123
    enc.lui(RReg::A0, 0x10000);
    enc.addi(RReg::A0, RReg::A0, 0x123);

    // 3. add a1, a0, a2; sub a3, a1, a4
    enc.add(RReg::A1, RReg::A0, RReg::A2);
    enc.sub(RReg::A3, RReg::A1, RReg::A4);

    // 4. sw a0, 0(sp); lw a5, 0(sp)
    enc.sw(RReg::Sp, RReg::A0, 0);
    enc.lw(RReg::A5, RReg::Sp, 0);

    // 5. jal ra, 8; jalr zero, ra, 0 (ret)
    enc.jal(RReg::Ra, 8);
    enc.ret();

    assert_eq!(enc.code.len() % 4, 0); // 32-bit fixed width
    let last_insn = u32::from_le_bytes(enc.code[enc.code.len() - 4..].try_into().unwrap());
    assert_eq!(last_insn, 0x0000_8067); // jalr x0, x1, 0 (ret)
}
