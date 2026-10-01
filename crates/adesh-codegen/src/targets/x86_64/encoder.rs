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

    /// Emit REX prefix: REX.W (64-bit operand), REX.R (reg extension), REX.X (index extension), REX.B (base/rm extension)
    fn emit_rex(&mut self, w: bool, r: u8, b: u8) {
        let rex_w = if w { 1 << 3 } else { 0 };
        let rex_r = if (r & 8) != 0 { 1 << 2 } else { 0 };
        let rex_b = if (b & 8) != 0 { 1 } else { 0 };
        let rex = 0x40 | rex_w | rex_r | rex_b;
        if rex != 0x40 || (r & 8) != 0 || (b & 8) != 0 || w {
            self.emit_u8(rex);
        }
    }

    /// Emit ModR/M byte: mod(2 bits), reg(3 bits), rm(3 bits)
    fn emit_modrm(&mut self, mod_bits: u8, reg: u8, rm: u8) {
        let byte = ((mod_bits & 0b11) << 6) | ((reg & 0b111) << 3) | (rm & 0b111);
        self.emit_u8(byte);
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
        if (dst & 8) != 0 || dst >= 4 {
            self.emit_rex(false, 0, dst);
        }
        self.emit_u8(0x0F);
        self.emit_u8(code);
        self.emit_modrm(0b11, 0, dst);
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
}

impl Default for X86_64Encoder {
    fn default() -> Self {
        Self::new()
    }
}
