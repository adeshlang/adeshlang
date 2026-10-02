//! x86-64 Machine Instruction Encoder.
//!
//! Provides binary instruction encoding with REX prefix, ModR/M, SIB, and displacement computation.

use crate::machine_ir::ConditionCode;

pub struct X86_64Encoder {
    pub buffer: Vec<u8>,
}

impl X86_64Encoder {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn emit_u8(&mut self, byte: u8) {
        self.buffer.push(byte);
    }

    pub fn emit_u16(&mut self, val: u16) {
        self.buffer.extend_from_slice(&val.to_le_bytes());
    }

    pub fn emit_u32(&mut self, val: u32) {
        self.buffer.extend_from_slice(&val.to_le_bytes());
    }

    pub fn emit_i32(&mut self, val: i32) {
        self.buffer.extend_from_slice(&val.to_le_bytes());
    }

    pub fn emit_u64(&mut self, val: u64) {
        self.buffer.extend_from_slice(&val.to_le_bytes());
    }

    pub fn emit_i64(&mut self, val: i64) {
        self.buffer.extend_from_slice(&val.to_le_bytes());
    }

    /// Emit REX prefix: REX.W (64-bit operand), REX.R (reg extension), REX.B (base/rm extension)
    fn emit_rex(&mut self, w: bool, r: u8, b: u8) {
        self.emit_rex_full(w, r, 0, b);
    }

    /// Emit a full REX prefix including the REX.X bit for SIB index registers.
    fn emit_rex_full(&mut self, w: bool, r: u8, x: u8, b: u8) {
        let rex = 0x40
            | if w { 1 << 3 } else { 0 }
            | if (r & 8) != 0 { 1 << 2 } else { 0 }
            | if (x & 8) != 0 { 1 << 1 } else { 0 }
            | if (b & 8) != 0 { 1 } else { 0 };
        if rex != 0x40 {
            self.emit_u8(rex);
        }
    }

    /// Emit ModR/M byte: mod(2 bits), reg(3 bits), rm(3 bits)
    fn emit_modrm(&mut self, mod_bits: u8, reg: u8, rm: u8) {
        let byte = ((mod_bits & 0b11) << 6) | ((reg & 0b111) << 3) | (rm & 0b111);
        self.emit_u8(byte);
    }

    /// Emit the ModR/M (+SIB+displacement) bytes addressing
    /// `[base + index*scale + offset]`.
    ///
    /// Handles the x86-64 addressing corner cases:
    /// - RSP/R12 as base always requires a SIB byte.
    /// - A SIB base field of `100` with mod `00` means "no base", so
    ///   `[rsp]` must use mod `01` with a zero disp8.
    /// - Mod `00` with rm `101` (no SIB) is RIP-relative, so `[rbp]`
    ///   must use mod `01` with a zero disp8.
    fn emit_mem_operand(&mut self, reg_field: u8, base: u8, offset: i32, index: Option<(u8, u8)>) {
        let (index_reg, scale) = match index {
            Some((r, s)) => (r, s),
            None => (4, 0), // SIB index field 100 = no index register
        };
        let disp8 = (-128..=127).contains(&offset);
        // RSP/R12 as base always requires a SIB byte.
        let needs_sib = index.is_some() || (base & 0b111) == 0b100;

        if needs_sib {
            let mod_bits = if offset == 0 && (base & 0b111) != 0b100 {
                0b00
            } else if disp8 {
                0b01
            } else {
                0b10
            };
            self.emit_modrm(mod_bits, reg_field & 0b111, 0b100);
            let scale_bits = scale & 0b11;
            self.emit_u8((scale_bits << 6) | ((index_reg & 0b111) << 3) | (base & 0b111));
            if mod_bits == 0b01 {
                self.emit_u8(offset as i8 as u8);
            } else if mod_bits == 0b10 {
                self.emit_i32(offset);
            }
        } else {
            let mod_bits = if offset == 0 && (base & 0b111) != 0b101 {
                0b00
            } else if disp8 {
                0b01
            } else {
                0b10
            };
            self.emit_modrm(mod_bits, reg_field & 0b111, base & 0b111);
            if mod_bits == 0b01 {
                self.emit_u8(offset as i8 as u8);
            } else if mod_bits == 0b10 {
                self.emit_i32(offset);
            }
        }
    }

    // --- High-Level Instruction Encoders ---

    /// NOP (0x90)
    pub fn nop(&mut self) {
        self.emit_u8(0x90);
    }

    /// RET (0xC3)
    pub fn ret(&mut self) {
        self.emit_u8(0xC3);
    }

    /// PUSH reg64 (0x50 + (reg & 7))
    pub fn push_reg64(&mut self, reg: u8) {
        if (reg & 8) != 0 {
            self.emit_u8(0x41); // REX.B
        }
        self.emit_u8(0x50 + (reg & 7));
    }

    /// POP reg64 (0x58 + (reg & 7))
    pub fn pop_reg64(&mut self, reg: u8) {
        if (reg & 8) != 0 {
            self.emit_u8(0x41); // REX.B
        }
        self.emit_u8(0x58 + (reg & 7));
    }

    /// PUSH imm32 (0x68 id) - sign-extended to 64-bit.
    pub fn push_imm32(&mut self, imm: i32) {
        self.emit_u8(0x68);
        self.emit_i32(imm);
    }

    /// PUSH qword [base + index*scale + offset] (FF /6)
    pub fn push_mem(&mut self, base: u8, offset: i32, index: Option<(u8, u8)>) {
        let x = index.map(|(r, _)| r).unwrap_or(0);
        self.emit_rex_full(false, 0, x, base);
        self.emit_u8(0xFF);
        self.emit_mem_operand(6, base, offset, index);
    }

    /// POP qword [base + index*scale + offset] (8F /0)
    pub fn pop_mem(&mut self, base: u8, offset: i32, index: Option<(u8, u8)>) {
        let x = index.map(|(r, _)| r).unwrap_or(0);
        self.emit_rex_full(false, 0, x, base);
        self.emit_u8(0x8F);
        self.emit_mem_operand(0, base, offset, index);
    }

    /// MOV reg64, reg64 (0x89 with ModR/M 11)
    pub fn mov_r64_r64(&mut self, dst: u8, src: u8) {
        self.emit_rex(true, src, dst);
        self.emit_u8(0x89);
        self.emit_modrm(0b11, src, dst);
    }

    /// MOV reg64, imm64 (0x48 | REX.B, 0xB8 + (dst & 7), imm64)
    pub fn mov_r64_imm64(&mut self, dst: u8, imm: i64) {
        self.emit_rex(true, 0, dst);
        self.emit_u8(0xB8 + (dst & 7));
        self.emit_i64(imm);
    }

    /// MOV reg64, [rbp + offset]
    pub fn mov_r64_rbp_offset(&mut self, dst: u8, offset: i32) {
        self.emit_rex(true, dst, 5); // 5 is RBP
        self.emit_u8(0x8B);
        if (-128..=127).contains(&offset) {
            self.emit_modrm(0b01, dst, 5);
            self.emit_u8(offset as i8 as u8);
        } else {
            self.emit_modrm(0b10, dst, 5);
            self.emit_i32(offset);
        }
    }

    /// MOV [rbp + offset], reg64
    pub fn mov_rbp_offset_r64(&mut self, offset: i32, src: u8) {
        self.emit_rex(true, src, 5);
        self.emit_u8(0x89);
        if (-128..=127).contains(&offset) {
            self.emit_modrm(0b01, src, 5);
            self.emit_u8(offset as i8 as u8);
        } else {
            self.emit_modrm(0b10, src, 5);
            self.emit_i32(offset);
        }
    }

    /// MOV reg64, [base + index*scale + offset] (8B /r with REX.W)
    pub fn mov_r64_mem(&mut self, dst: u8, base: u8, offset: i32, index: Option<(u8, u8)>) {
        let x = index.map(|(r, _)| r).unwrap_or(0);
        self.emit_rex_full(true, dst, x, base);
        self.emit_u8(0x8B);
        self.emit_mem_operand(dst, base, offset, index);
    }

    /// MOV [base + index*scale + offset], reg64 (89 /r with REX.W)
    pub fn mov_mem_r64(&mut self, base: u8, offset: i32, index: Option<(u8, u8)>, src: u8) {
        let x = index.map(|(r, _)| r).unwrap_or(0);
        self.emit_rex_full(true, src, x, base);
        self.emit_u8(0x89);
        self.emit_mem_operand(src, base, offset, index);
    }

    /// MOV reg32, [base + index*scale + offset] (8B /r, zero-extends to 64 bits)
    pub fn mov_r32_mem(&mut self, dst: u8, base: u8, offset: i32, index: Option<(u8, u8)>) {
        let x = index.map(|(r, _)| r).unwrap_or(0);
        self.emit_rex_full(false, dst, x, base);
        self.emit_u8(0x8B);
        self.emit_mem_operand(dst, base, offset, index);
    }

    /// MOV [base + index*scale + offset], reg32 (89 /r)
    pub fn mov_mem_r32(&mut self, base: u8, offset: i32, index: Option<(u8, u8)>, src: u8) {
        let x = index.map(|(r, _)| r).unwrap_or(0);
        self.emit_rex_full(false, src, x, base);
        self.emit_u8(0x89);
        self.emit_mem_operand(src, base, offset, index);
    }

    /// MOV [base + index*scale + offset], reg16 (66 89 /r)
    pub fn mov_mem_r16(&mut self, base: u8, offset: i32, index: Option<(u8, u8)>, src: u8) {
        let x = index.map(|(r, _)| r).unwrap_or(0);
        self.emit_u8(0x66);
        self.emit_rex_full(false, src, x, base);
        self.emit_u8(0x89);
        self.emit_mem_operand(src, base, offset, index);
    }

    /// MOV [base + index*scale + offset], reg8 (88 /r)
    pub fn mov_mem_r8(&mut self, base: u8, offset: i32, index: Option<(u8, u8)>, src: u8) {
        let x = index.map(|(r, _)| r).unwrap_or(0);
        self.emit_rex_full(false, src, x, base);
        self.emit_u8(0x88);
        self.emit_mem_operand(src, base, offset, index);
    }

    /// MOVZX reg64, reg8 (0F B6 /r with REX.W) - zero-extend byte to 64 bits.
    pub fn movzx_r64_r8(&mut self, dst: u8, src: u8) {
        self.emit_rex(true, src, dst);
        self.emit_u8(0x0F);
        self.emit_u8(0xB6);
        self.emit_modrm(0b11, src, dst);
    }

    /// MOVZX reg64, byte [base + index*scale + offset] (0F B6 /r with REX.W)
    pub fn movzx_r64_mem8(&mut self, dst: u8, base: u8, offset: i32, index: Option<(u8, u8)>) {
        let x = index.map(|(r, _)| r).unwrap_or(0);
        self.emit_rex_full(true, dst, x, base);
        self.emit_u8(0x0F);
        self.emit_u8(0xB6);
        self.emit_mem_operand(dst, base, offset, index);
    }

    /// MOVZX reg64, word [base + index*scale + offset] (0F B7 /r with REX.W)
    pub fn movzx_r64_mem16(&mut self, dst: u8, base: u8, offset: i32, index: Option<(u8, u8)>) {
        let x = index.map(|(r, _)| r).unwrap_or(0);
        self.emit_rex_full(true, dst, x, base);
        self.emit_u8(0x0F);
        self.emit_u8(0xB7);
        self.emit_mem_operand(dst, base, offset, index);
    }

    /// ADD reg64, reg64 (0x01)
    pub fn add_r64_r64(&mut self, dst: u8, src: u8) {
        self.emit_rex(true, src, dst);
        self.emit_u8(0x01);
        self.emit_modrm(0b11, src, dst);
    }

    /// ADD reg64, imm32 (0x81 /0)
    pub fn add_r64_imm32(&mut self, dst: u8, imm: i32) {
        self.emit_rex(true, 0, dst);
        self.emit_u8(0x81);
        self.emit_modrm(0b11, 0, dst);
        self.emit_i32(imm);
    }

    /// SUB reg64, reg64 (0x29)
    pub fn sub_r64_r64(&mut self, dst: u8, src: u8) {
        self.emit_rex(true, src, dst);
        self.emit_u8(0x29);
        self.emit_modrm(0b11, src, dst);
    }

    /// SUB reg64, imm32 (0x81 /5)
    pub fn sub_r64_imm32(&mut self, dst: u8, imm: i32) {
        self.emit_rex(true, 0, dst);
        self.emit_u8(0x81);
        self.emit_modrm(0b11, 5, dst);
        self.emit_i32(imm);
    }

    /// IMUL reg64, reg64 (0x0F 0xAF)
    pub fn imul_r64_r64(&mut self, dst: u8, src: u8) {
        self.emit_rex(true, dst, src);
        self.emit_u8(0x0F);
        self.emit_u8(0xAF);
        self.emit_modrm(0b11, dst, src);
    }

    /// IMUL reg64, [base + index*scale + offset] (0F AF /r with REX.W)
    pub fn imul_r64_mem(&mut self, dst: u8, base: u8, offset: i32, index: Option<(u8, u8)>) {
        let x = index.map(|(r, _)| r).unwrap_or(0);
        self.emit_rex_full(true, dst, x, base);
        self.emit_u8(0x0F);
        self.emit_u8(0xAF);
        self.emit_mem_operand(dst, base, offset, index);
    }

    /// CQO (0x48 0x99 - sign extend RAX into RDX:RAX)
    pub fn cqo(&mut self) {
        self.emit_u8(0x48);
        self.emit_u8(0x99);
    }

    /// IDIV reg64 (0x48 0xF7 /7)
    pub fn idiv_r64(&mut self, src: u8) {
        self.emit_rex(true, 0, src);
        self.emit_u8(0xF7);
        self.emit_modrm(0b11, 7, src);
    }

    /// AND reg64, reg64 (0x21)
    pub fn and_r64_r64(&mut self, dst: u8, src: u8) {
        self.emit_rex(true, src, dst);
        self.emit_u8(0x21);
        self.emit_modrm(0b11, src, dst);
    }

    /// OR reg64, reg64 (0x09)
    pub fn or_r64_r64(&mut self, dst: u8, src: u8) {
        self.emit_rex(true, src, dst);
        self.emit_u8(0x09);
        self.emit_modrm(0b11, src, dst);
    }

    /// XOR reg64, reg64 (0x31)
    pub fn xor_r64_r64(&mut self, dst: u8, src: u8) {
        self.emit_rex(true, src, dst);
        self.emit_u8(0x31);
        self.emit_modrm(0b11, src, dst);
    }

    /// NEG reg64 (0xF7 /3)
    pub fn neg_r64(&mut self, dst: u8) {
        self.emit_rex(true, 0, dst);
        self.emit_u8(0xF7);
        self.emit_modrm(0b11, 3, dst);
    }

    /// NOT reg64 (0xF7 /2)
    pub fn not_r64(&mut self, dst: u8) {
        self.emit_rex(true, 0, dst);
        self.emit_u8(0xF7);
        self.emit_modrm(0b11, 2, dst);
    }

    /// CMP reg64, reg64 (0x39)
    pub fn cmp_r64_r64(&mut self, lhs: u8, rhs: u8) {
        self.emit_rex(true, rhs, lhs);
        self.emit_u8(0x39);
        self.emit_modrm(0b11, rhs, lhs);
    }

    /// CMP reg64, imm32 (0x81 /7)
    pub fn cmp_r64_imm32(&mut self, lhs: u8, imm: i32) {
        self.emit_rex(true, 0, lhs);
        self.emit_u8(0x81);
        self.emit_modrm(0b11, 7, lhs);
        self.emit_i32(imm);
    }

    /// TEST reg64, reg64 (0x85 /r)
    pub fn test_r64_r64(&mut self, lhs: u8, rhs: u8) {
        self.emit_rex(true, rhs, lhs);
        self.emit_u8(0x85);
        self.emit_modrm(0b11, rhs, lhs);
    }

    /// TEST reg64, imm32 (0xF7 /0)
    pub fn test_r64_imm32(&mut self, lhs: u8, imm: i32) {
        self.emit_rex(true, 0, lhs);
        self.emit_u8(0xF7);
        self.emit_modrm(0b11, 0, lhs);
        self.emit_i32(imm);
    }

    /// Generic `op reg64, [mem]` for the reg-from-memory encodings:
    /// 03 ADD, 0B OR, 23 AND, 2B SUB, 33 XOR, 3B CMP.
    pub fn op_r64_mem(
        &mut self,
        opcode: u8,
        dst: u8,
        base: u8,
        offset: i32,
        index: Option<(u8, u8)>,
    ) {
        let x = index.map(|(r, _)| r).unwrap_or(0);
        self.emit_rex_full(true, dst, x, base);
        self.emit_u8(opcode);
        self.emit_mem_operand(dst, base, offset, index);
    }

    /// Generic `op [mem], reg64` for the mem-from-register encodings:
    /// 01 ADD, 09 OR, 21 AND, 29 SUB, 31 XOR, 39 CMP, 85 TEST.
    pub fn op_mem_r64(
        &mut self,
        opcode: u8,
        base: u8,
        offset: i32,
        index: Option<(u8, u8)>,
        src: u8,
    ) {
        let x = index.map(|(r, _)| r).unwrap_or(0);
        self.emit_rex_full(true, src, x, base);
        self.emit_u8(opcode);
        self.emit_mem_operand(src, base, offset, index);
    }

    /// Generic `op reg64, imm32` (0x81 /x).
    /// Extensions: 0 ADD, 1 OR, 4 AND, 5 SUB, 6 XOR, 7 CMP.
    pub fn op_r64_imm32(&mut self, ext: u8, dst: u8, imm: i32) {
        self.emit_rex(true, 0, dst);
        self.emit_u8(0x81);
        self.emit_modrm(0b11, ext, dst);
        self.emit_i32(imm);
    }

    /// SHL reg64, imm8 (C1 /4 ib)
    pub fn shl_r64_imm8(&mut self, dst: u8, imm: u8) {
        self.emit_rex(true, 0, dst);
        self.emit_u8(0xC1);
        self.emit_modrm(0b11, 4, dst);
        self.emit_u8(imm);
    }

    /// SHR reg64, imm8 (C1 /5 ib)
    pub fn shr_r64_imm8(&mut self, dst: u8, imm: u8) {
        self.emit_rex(true, 0, dst);
        self.emit_u8(0xC1);
        self.emit_modrm(0b11, 5, dst);
        self.emit_u8(imm);
    }

    /// SAR reg64, imm8 (C1 /7 ib)
    pub fn sar_r64_imm8(&mut self, dst: u8, imm: u8) {
        self.emit_rex(true, 0, dst);
        self.emit_u8(0xC1);
        self.emit_modrm(0b11, 7, dst);
        self.emit_u8(imm);
    }

    /// SHL reg64, CL (D3 /4)
    pub fn shl_r64_cl(&mut self, dst: u8) {
        self.emit_rex(true, 0, dst);
        self.emit_u8(0xD3);
        self.emit_modrm(0b11, 4, dst);
    }

    /// SHR reg64, CL (D3 /5)
    pub fn shr_r64_cl(&mut self, dst: u8) {
        self.emit_rex(true, 0, dst);
        self.emit_u8(0xD3);
        self.emit_modrm(0b11, 5, dst);
    }

    /// SAR reg64, CL (D3 /7)
    pub fn sar_r64_cl(&mut self, dst: u8) {
        self.emit_rex(true, 0, dst);
        self.emit_u8(0xD3);
        self.emit_modrm(0b11, 7, dst);
    }

    /// SETcc reg8 (0x0F 0x90+cc)
    pub fn setcc_r8(&mut self, cc: ConditionCode, dst: u8) {
        let code = match cc {
            ConditionCode::Equal | ConditionCode::Zero => 0x94, // SETE / SETZ
            ConditionCode::NotEqual | ConditionCode::NotZero => 0x95, // SETNE / SETNZ
            ConditionCode::LessThan => 0x9C,                    // SETL
            ConditionCode::LessOrEqual => 0x9E,                 // SETLE
            ConditionCode::GreaterThan => 0x9F,                 // SETG
            ConditionCode::GreaterOrEqual => 0x9D,              // SETGE
            ConditionCode::Below => 0x92,                       // SETB
            ConditionCode::BelowOrEqual => 0x96,                // SETBE
            ConditionCode::Above => 0x97,                       // SETA
            ConditionCode::AboveOrEqual => 0x93,                // SETAE
        };
        // Byte-register destinations SPL(4)/BPL(5)/SIL(6)/DIL(7) require a REX
        // prefix even with no extension bits set (without REX those encodings
        // address AH/CH/DH/BH); R8B-R15B require REX.B.
        if dst >= 4 || (dst & 8) != 0 {
            self.emit_u8(0x40 | ((dst & 8) >> 3));
        }
        self.emit_u8(0x0F);
        self.emit_u8(code);
        self.emit_modrm(0b11, 0, dst & 7);
    }

    /// JMP rel32 (0xE9 rel32)
    pub fn jmp_rel32(&mut self, rel32: i32) {
        self.emit_u8(0xE9);
        self.emit_i32(rel32);
    }

    /// Jcc rel32 (0x0F 0x80+cc rel32)
    pub fn jcc_rel32(&mut self, cc: ConditionCode, rel32: i32) {
        let code = match cc {
            ConditionCode::Equal | ConditionCode::Zero => 0x84,
            ConditionCode::NotEqual | ConditionCode::NotZero => 0x85,
            ConditionCode::LessThan => 0x8C,
            ConditionCode::LessOrEqual => 0x8E,
            ConditionCode::GreaterThan => 0x8F,
            ConditionCode::GreaterOrEqual => 0x8D,
            ConditionCode::Below => 0x82,
            ConditionCode::BelowOrEqual => 0x86,
            ConditionCode::Above => 0x87,
            ConditionCode::AboveOrEqual => 0x83,
        };
        self.emit_u8(0x0F);
        self.emit_u8(code);
        self.emit_i32(rel32);
    }

    /// CALL rel32 (0xE8 rel32)
    pub fn call_rel32(&mut self, rel32: i32) {
        self.emit_u8(0xE8);
        self.emit_i32(rel32);
    }

    /// CALL reg64 (0xFF /2)
    pub fn call_r64(&mut self, reg: u8) {
        if (reg & 8) != 0 {
            self.emit_u8(0x41);
        }
        self.emit_u8(0xFF);
        self.emit_modrm(0b11, 2, reg);
    }

    /// MFENCE (0F AE F0) - full memory ordering barrier.
    pub fn mfence(&mut self) {
        self.emit_u8(0x0F);
        self.emit_u8(0xAE);
        self.emit_u8(0xF0);
    }
}

impl Default for X86_64Encoder {
    fn default() -> Self {
        Self::new()
    }
}
