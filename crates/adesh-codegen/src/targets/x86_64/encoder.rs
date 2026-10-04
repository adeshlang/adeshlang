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

    /// Emit a full REX prefix for a *byte* (`r/m8`) operation.
    ///
    /// Byte-register encodings differ from wider ones: without a REX prefix the
    /// ModR/M reg fields 4..=7 select AH/CH/DH/BH instead of SPL/BPL/SIL/DIL.
    /// A bare `0x40` REX (no extension bits set) is therefore mandatory whenever
    /// any register *field* (reg/index/base, high bits included) falls in
    /// `4..=7`. R8B-R15B already need REX.R/REX.X/REX.B.
    fn emit_rex_full_byte(&mut self, w: bool, r: u8, x: u8, b: u8) {
        let rex = 0x40
            | if w { 1 << 3 } else { 0 }
            | if (r & 8) != 0 { 1 << 2 } else { 0 }
            | if (x & 8) != 0 { 1 << 1 } else { 0 }
            | if (b & 8) != 0 { 1 } else { 0 };
        let needs_bare_rex = (r & 0b111) >= 4 || (x & 0b111) >= 4 || (b & 0b111) >= 4;
        if rex != 0x40 || needs_bare_rex {
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
            // A SIB base field of 101 with mod 00 means "no base register,
            // disp32 follows" (RBP/R13 with mod 00 are not encodable as a
            // plain base), so those bases must use mod 01 with a zero disp8.
            let base_field = base & 0b111;
            let mod_bits = if offset == 0 && base_field != 0b100 && base_field != 0b101 {
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

    /// MOV reg32, imm32 (B8 + (dst & 7), imm32) - zero-extends to 64-bit
    pub fn mov_r32_imm32(&mut self, dst: u8, imm: i32) {
        if (dst & 8) != 0 {
            self.emit_u8(0x41); // REX.B
        }
        self.emit_u8(0xB8 + (dst & 7));
        self.emit_i32(imm);
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
    ///
    /// Uses the byte-aware REX emitter: SPL/BPL/SIL/DIL (fields 4..=7) must be
    /// disambiguated from AH/CH/DH/BH with a bare REX prefix.
    pub fn mov_mem_r8(&mut self, base: u8, offset: i32, index: Option<(u8, u8)>, src: u8) {
        let x = index.map(|(r, _)| r).unwrap_or(0);
        self.emit_rex_full_byte(false, src, x, base);
        self.emit_u8(0x88);
        self.emit_mem_operand(src, base, offset, index);
    }

    /// MOVZX reg64, reg8 (0F B6 /r with REX.W) - zero-extend byte to 64 bits.
    pub fn movzx_r64_r8(&mut self, dst: u8, src: u8) {
        self.emit_rex(true, dst, src);
        self.emit_u8(0x0F);
        self.emit_u8(0xB6);
        self.emit_modrm(0b11, dst, src);
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

    /// Generic `op reg64, imm8` (0x83 /x ib) - the sign-extended 8-bit
    /// immediate form, one byte shorter than 0x81 /x for -128..=127.
    /// Extensions: 0 ADD, 1 OR, 4 AND, 5 SUB, 6 XOR, 7 CMP.
    pub fn op_r64_imm8(&mut self, ext: u8, dst: u8, imm: i8) {
        self.emit_rex(true, 0, dst);
        self.emit_u8(0x83);
        self.emit_modrm(0b11, ext, dst);
        self.emit_u8(imm as u8);
    }

    /// ENDBR64 (F3 0F 1E FA) - Intel CET indirect-branch landing pad.
    pub fn endbr64(&mut self) {
        self.emit_u8(0xF3);
        self.emit_u8(0x0F);
        self.emit_u8(0x1E);
        self.emit_u8(0xFA);
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
            ConditionCode::Parity => 0x9A,                      // SETP / SETPE
            ConditionCode::NotParity => 0x9B,                   // SETNP / SETPO
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
            ConditionCode::Parity => 0x8A,
            ConditionCode::NotParity => 0x8B,
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

    // ---------------------------------------------------------------- SSE / SSE2

    fn emit_sse_reg_reg(&mut self, prefix: Option<u8>, opcode: u8, dst: u8, src: u8) {
        if let Some(p) = prefix {
            self.emit_u8(p);
        }
        self.emit_rex(false, dst, src);
        self.emit_u8(0x0F);
        self.emit_u8(opcode);
        self.emit_modrm(0b11, dst & 7, src & 7);
    }

    fn emit_sse_reg_mem(
        &mut self,
        prefix: Option<u8>,
        opcode: u8,
        dst: u8,
        base: u8,
        offset: i32,
        index: Option<(u8, u8)>,
    ) {
        if let Some(p) = prefix {
            self.emit_u8(p);
        }
        let x = index.map(|(r, _)| r).unwrap_or(0);
        self.emit_rex_full(false, dst, x, base);
        self.emit_u8(0x0F);
        self.emit_u8(opcode);
        self.emit_mem_operand(dst, base, offset, index);
    }

    fn emit_sse_mem_reg(
        &mut self,
        prefix: Option<u8>,
        opcode: u8,
        base: u8,
        offset: i32,
        index: Option<(u8, u8)>,
        src: u8,
    ) {
        if let Some(p) = prefix {
            self.emit_u8(p);
        }
        let x = index.map(|(r, _)| r).unwrap_or(0);
        self.emit_rex_full(false, src, x, base);
        self.emit_u8(0x0F);
        self.emit_u8(opcode);
        self.emit_mem_operand(src, base, offset, index);
    }

    /// MOVSS xmm, xmm (F3 0F 10 /r)
    pub fn movss_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(Some(0xF3), 0x10, dst, src);
    }

    /// MOVSS xmm, [mem] (F3 0F 10 /r)
    pub fn movss_xmm_mem(&mut self, dst: u8, base: u8, offset: i32, index: Option<(u8, u8)>) {
        self.emit_sse_reg_mem(Some(0xF3), 0x10, dst, base, offset, index);
    }

    /// MOVSS [mem], xmm (F3 0F 11 /r)
    pub fn movss_mem_xmm(&mut self, base: u8, offset: i32, index: Option<(u8, u8)>, src: u8) {
        self.emit_sse_mem_reg(Some(0xF3), 0x11, base, offset, index, src);
    }

    /// MOVSS xmm, [rbp + offset]
    pub fn movss_xmm_rbp_offset(&mut self, dst: u8, offset: i32) {
        self.movss_xmm_mem(dst, 5, offset, None);
    }

    /// MOVSS [rbp + offset], xmm
    pub fn movss_rbp_offset_xmm(&mut self, offset: i32, src: u8) {
        self.movss_mem_xmm(5, offset, None, src);
    }

    /// MOVSD xmm, xmm (F2 0F 10 /r)
    pub fn movsd_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(Some(0xF2), 0x10, dst, src);
    }

    /// MOVSD xmm, [mem] (F2 0F 10 /r)
    pub fn movsd_xmm_mem(&mut self, dst: u8, base: u8, offset: i32, index: Option<(u8, u8)>) {
        self.emit_sse_reg_mem(Some(0xF2), 0x10, dst, base, offset, index);
    }

    /// MOVSD [mem], xmm (F2 0F 11 /r)
    pub fn movsd_mem_xmm(&mut self, base: u8, offset: i32, index: Option<(u8, u8)>, src: u8) {
        self.emit_sse_mem_reg(Some(0xF2), 0x11, base, offset, index, src);
    }

    /// MOVSD xmm, [rbp + offset]
    pub fn movsd_xmm_rbp_offset(&mut self, dst: u8, offset: i32) {
        self.movsd_xmm_mem(dst, 5, offset, None);
    }

    /// MOVSD [rbp + offset], xmm
    pub fn movsd_rbp_offset_xmm(&mut self, offset: i32, src: u8) {
        self.movsd_mem_xmm(5, offset, None, src);
    }

    /// ADDSS xmm, xmm (F3 0F 58 /r)
    pub fn addss_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(Some(0xF3), 0x58, dst, src);
    }

    /// ADDSS xmm, [mem] (F3 0F 58 /r)
    pub fn addss_xmm_mem(&mut self, dst: u8, base: u8, offset: i32, index: Option<(u8, u8)>) {
        self.emit_sse_reg_mem(Some(0xF3), 0x58, dst, base, offset, index);
    }

    /// SUBSS xmm, xmm (F3 0F 5C /r)
    pub fn subss_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(Some(0xF3), 0x5C, dst, src);
    }

    /// SUBSS xmm, [mem] (F3 0F 5C /r)
    pub fn subss_xmm_mem(&mut self, dst: u8, base: u8, offset: i32, index: Option<(u8, u8)>) {
        self.emit_sse_reg_mem(Some(0xF3), 0x5C, dst, base, offset, index);
    }

    /// MULSS xmm, xmm (F3 0F 59 /r)
    pub fn mulss_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(Some(0xF3), 0x59, dst, src);
    }

    /// MULSS xmm, [mem] (F3 0F 59 /r)
    pub fn mulss_xmm_mem(&mut self, dst: u8, base: u8, offset: i32, index: Option<(u8, u8)>) {
        self.emit_sse_reg_mem(Some(0xF3), 0x59, dst, base, offset, index);
    }

    /// DIVSS xmm, xmm (F3 0F 5E /r)
    pub fn divss_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(Some(0xF3), 0x5E, dst, src);
    }

    /// DIVSS xmm, [mem] (F3 0F 5E /r)
    pub fn divss_xmm_mem(&mut self, dst: u8, base: u8, offset: i32, index: Option<(u8, u8)>) {
        self.emit_sse_reg_mem(Some(0xF3), 0x5E, dst, base, offset, index);
    }

    /// ADDSD xmm, xmm (F2 0F 58 /r)
    pub fn addsd_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(Some(0xF2), 0x58, dst, src);
    }

    /// ADDSD xmm, [mem] (F2 0F 58 /r)
    pub fn addsd_xmm_mem(&mut self, dst: u8, base: u8, offset: i32, index: Option<(u8, u8)>) {
        self.emit_sse_reg_mem(Some(0xF2), 0x58, dst, base, offset, index);
    }

    /// SUBSD xmm, xmm (F2 0F 5C /r)
    pub fn subsd_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(Some(0xF2), 0x5C, dst, src);
    }

    /// SUBSD xmm, [mem] (F2 0F 5C /r)
    pub fn subsd_xmm_mem(&mut self, dst: u8, base: u8, offset: i32, index: Option<(u8, u8)>) {
        self.emit_sse_reg_mem(Some(0xF2), 0x5C, dst, base, offset, index);
    }

    /// MULSD xmm, xmm (F2 0F 59 /r)
    pub fn mulsd_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(Some(0xF2), 0x59, dst, src);
    }

    /// MULSD xmm, [mem] (F2 0F 59 /r)
    pub fn mulsd_xmm_mem(&mut self, dst: u8, base: u8, offset: i32, index: Option<(u8, u8)>) {
        self.emit_sse_reg_mem(Some(0xF2), 0x59, dst, base, offset, index);
    }

    /// DIVSD xmm, xmm (F2 0F 5E /r)
    pub fn divsd_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(Some(0xF2), 0x5E, dst, src);
    }

    /// DIVSD xmm, [mem] (F2 0F 5E /r)
    pub fn divsd_xmm_mem(&mut self, dst: u8, base: u8, offset: i32, index: Option<(u8, u8)>) {
        self.emit_sse_reg_mem(Some(0xF2), 0x5E, dst, base, offset, index);
    }

    /// UCOMISS xmm, xmm (0F 2E /r)
    pub fn ucomiss_xmm_xmm(&mut self, lhs: u8, rhs: u8) {
        self.emit_sse_reg_reg(None, 0x2E, lhs, rhs);
    }

    /// UCOMISS xmm, [mem] (0F 2E /r)
    pub fn ucomiss_xmm_mem(&mut self, lhs: u8, base: u8, offset: i32, index: Option<(u8, u8)>) {
        self.emit_sse_reg_mem(None, 0x2E, lhs, base, offset, index);
    }

    /// UCOMISD xmm, xmm (66 0F 2E /r)
    pub fn ucomisd_xmm_xmm(&mut self, lhs: u8, rhs: u8) {
        self.emit_sse_reg_reg(Some(0x66), 0x2E, lhs, rhs);
    }

    /// UCOMISD xmm, [mem] (66 0F 2E /r)
    pub fn ucomisd_xmm_mem(&mut self, lhs: u8, base: u8, offset: i32, index: Option<(u8, u8)>) {
        self.emit_sse_reg_mem(Some(0x66), 0x2E, lhs, base, offset, index);
    }

    /// CVTSI2SS xmm, reg64 (F3 REX.W 0F 2A /r)
    pub fn cvtsi2ss_xmm_r64(&mut self, dst: u8, src: u8) {
        self.emit_u8(0xF3);
        self.emit_rex(true, dst, src);
        self.emit_u8(0x0F);
        self.emit_u8(0x2A);
        self.emit_modrm(0b11, dst & 7, src & 7);
    }

    /// CVTSI2SS xmm, reg32 (F3 0F 2A /r)
    pub fn cvtsi2ss_xmm_r32(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(Some(0xF3), 0x2A, dst, src);
    }

    /// CVTSI2SD xmm, reg64 (F2 REX.W 0F 2A /r)
    pub fn cvtsi2sd_xmm_r64(&mut self, dst: u8, src: u8) {
        self.emit_u8(0xF2);
        self.emit_rex(true, dst, src);
        self.emit_u8(0x0F);
        self.emit_u8(0x2A);
        self.emit_modrm(0b11, dst & 7, src & 7);
    }

    /// CVTSI2SD xmm, reg32 (F2 0F 2A /r)
    pub fn cvtsi2sd_xmm_r32(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(Some(0xF2), 0x2A, dst, src);
    }

    /// CVTTSS2SI reg64, xmm (F3 REX.W 0F 2C /r)
    pub fn cvttss2si_r64_xmm(&mut self, dst: u8, src: u8) {
        self.emit_u8(0xF3);
        self.emit_rex(true, dst, src);
        self.emit_u8(0x0F);
        self.emit_u8(0x2C);
        self.emit_modrm(0b11, dst & 7, src & 7);
    }

    /// CVTTSS2SI reg32, xmm (F3 0F 2C /r)
    pub fn cvttss2si_r32_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(Some(0xF3), 0x2C, dst, src);
    }

    /// CVTTSD2SI reg64, xmm (F2 REX.W 0F 2C /r)
    pub fn cvttsd2si_r64_xmm(&mut self, dst: u8, src: u8) {
        self.emit_u8(0xF2);
        self.emit_rex(true, dst, src);
        self.emit_u8(0x0F);
        self.emit_u8(0x2C);
        self.emit_modrm(0b11, dst & 7, src & 7);
    }

    /// CVTTSD2SI reg32, xmm (F2 0F 2C /r)
    pub fn cvttsd2si_r32_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(Some(0xF2), 0x2C, dst, src);
    }

    /// CVTSS2SD xmm, xmm (F3 0F 5A /r)
    pub fn cvtss2sd_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(Some(0xF3), 0x5A, dst, src);
    }

    /// CVTSD2SS xmm, xmm (F2 0F 5A /r)
    pub fn cvtsd2ss_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(Some(0xF2), 0x5A, dst, src);
    }

    /// MOVQ xmm, reg64 (66 REX.W 0F 6E /r)
    pub fn movq_xmm_r64(&mut self, dst: u8, src: u8) {
        self.emit_u8(0x66);
        self.emit_rex(true, dst, src);
        self.emit_u8(0x0F);
        self.emit_u8(0x6E);
        self.emit_modrm(0b11, dst & 7, src & 7);
    }

    /// MOVQ reg64, xmm (66 REX.W 0F 7E /r)
    pub fn movq_r64_xmm(&mut self, dst: u8, src: u8) {
        self.emit_u8(0x66);
        self.emit_rex(true, src, dst);
        self.emit_u8(0x0F);
        self.emit_u8(0x7E);
        self.emit_modrm(0b11, src & 7, dst & 7);
    }

    /// MOVD xmm, reg32 (66 0F 6E /r)
    pub fn movd_xmm_r32(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(Some(0x66), 0x6E, dst, src);
    }

    /// MOVD reg32, xmm (66 0F 7E /r)
    pub fn movd_r32_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(Some(0x66), 0x7E, src, dst);
    }

    /// XORPS xmm, xmm (0F 57 /r)
    pub fn xorps_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(None, 0x57, dst, src);
    }

    /// XORPD xmm, xmm (66 0F 57 /r)
    pub fn xorpd_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(Some(0x66), 0x57, dst, src);
    }

    // ---------------------------------------------------------------- SIMD Packed Instructions

    /// ADDPS xmm, xmm (0F 58 /r)
    pub fn addps_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(None, 0x58, dst, src);
    }

    /// SUBPS xmm, xmm (0F 5C /r)
    pub fn subps_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(None, 0x5C, dst, src);
    }

    /// MULPS xmm, xmm (0F 59 /r)
    pub fn mulps_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(None, 0x59, dst, src);
    }

    /// DIVPS xmm, xmm (0F 5E /r)
    pub fn divps_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(None, 0x5E, dst, src);
    }

    /// ADDPD xmm, xmm (66 0F 58 /r)
    pub fn addpd_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(Some(0x66), 0x58, dst, src);
    }

    /// SUBPD xmm, xmm (66 0F 5C /r)
    pub fn subpd_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(Some(0x66), 0x5C, dst, src);
    }

    /// MULPD xmm, xmm (66 0F 59 /r)
    pub fn mulpd_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(Some(0x66), 0x59, dst, src);
    }

    /// DIVPD xmm, xmm (66 0F 5E /r)
    pub fn divpd_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(Some(0x66), 0x5E, dst, src);
    }

    /// PADDD xmm, xmm (66 0F FE /r)
    pub fn paddd_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(Some(0x66), 0xFE, dst, src);
    }

    /// PSUBD xmm, xmm (66 0F FA /r)
    pub fn psubd_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(Some(0x66), 0xFA, dst, src);
    }

    /// PMULLD xmm, xmm (66 0F 38 40 /r)
    pub fn pmulld_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_u8(0x66);
        self.emit_rex(false, dst, src);
        self.emit_u8(0x0F);
        self.emit_u8(0x38);
        self.emit_u8(0x40);
        self.emit_modrm(0b11, dst & 7, src & 7);
    }

    /// ANDPS xmm, xmm (0F 54 /r)
    pub fn andps_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(None, 0x54, dst, src);
    }

    /// ORPS xmm, xmm (0F 56 /r)
    pub fn orps_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(None, 0x56, dst, src);
    }

    /// MOVAPS xmm, xmm (0F 28 /r)
    pub fn movaps_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(None, 0x28, dst, src);
    }

    /// MOVUPS xmm, [mem] (0F 10 /r)
    pub fn movups_xmm_mem(&mut self, dst: u8, base: u8, offset: i32, index: Option<(u8, u8)>) {
        self.emit_sse_reg_mem(None, 0x10, dst, base, offset, index);
    }

    /// MOVUPS [mem], xmm (0F 11 /r)
    pub fn movups_mem_xmm(&mut self, base: u8, offset: i32, index: Option<(u8, u8)>, src: u8) {
        self.emit_sse_mem_reg(None, 0x11, base, offset, index, src);
    }

    /// SHUFPS xmm, xmm, imm8 (0F C6 /r ib)
    pub fn shufps_xmm_xmm_imm8(&mut self, dst: u8, src: u8, imm: u8) {
        self.emit_sse_reg_reg(None, 0xC6, dst, src);
        self.emit_u8(imm);
    }

    /// MAXPS xmm, xmm (0F 5F /r)
    pub fn maxps_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(None, 0x5F, dst, src);
    }

    /// MINPS xmm, xmm (0F 5D /r)
    pub fn minps_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(None, 0x5D, dst, src);
    }

    /// MAXPD xmm, xmm (66 0F 5F /r)
    pub fn maxpd_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(Some(0x66), 0x5F, dst, src);
    }

    /// MINPD xmm, xmm (66 0F 5D /r)
    pub fn minpd_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(Some(0x66), 0x5D, dst, src);
    }

    /// CMPPS xmm, xmm, imm8 (0F C2 /r ib)
    pub fn cmpps_xmm_xmm_imm8(&mut self, dst: u8, src: u8, imm: u8) {
        self.emit_sse_reg_reg(None, 0xC2, dst, src);
        self.emit_u8(imm);
    }

    /// BLENDPS xmm, xmm, imm8 (66 0F 3A 0C /r ib)
    pub fn blendps_xmm_xmm_imm8(&mut self, dst: u8, src: u8, imm: u8) {
        self.emit_u8(0x66);
        self.emit_rex(false, dst, src);
        self.emit_u8(0x0F);
        self.emit_u8(0x3A);
        self.emit_u8(0x0C);
        self.emit_modrm(0b11, dst & 7, src & 7);
        self.emit_u8(imm);
    }

    /// PSLLD xmm, imm8 (66 0F 72 /6 ib)
    pub fn pslld_xmm_imm8(&mut self, dst: u8, imm: u8) {
        self.emit_u8(0x66);
        self.emit_rex(false, 6, dst);
        self.emit_u8(0x0F);
        self.emit_u8(0x72);
        self.emit_modrm(0b11, 6, dst & 7);
        self.emit_u8(imm);
    }

    /// PSRLD xmm, imm8 (66 0F 72 /2 ib)
    pub fn psrld_xmm_imm8(&mut self, dst: u8, imm: u8) {
        self.emit_u8(0x66);
        self.emit_rex(false, 2, dst);
        self.emit_u8(0x0F);
        self.emit_u8(0x72);
        self.emit_modrm(0b11, 2, dst & 7);
        self.emit_u8(imm);
    }

    /// HADDPS xmm, xmm (F2 0F 7C /r)
    pub fn haddps_xmm_xmm(&mut self, dst: u8, src: u8) {
        self.emit_sse_reg_reg(Some(0xF2), 0x7C, dst, src);
    }

    // ---------------------------------------------------------------- Atomic Instructions

    /// LOCK XADD [mem], reg64 (F0 48 0F C1 /r)
    pub fn lock_xadd_mem_r64(&mut self, base: u8, offset: i32, index: Option<(u8, u8)>, reg: u8) {
        self.emit_u8(0xF0);
        let x = index.map(|(r, _)| r).unwrap_or(0);
        self.emit_rex_full(true, reg, x, base);
        self.emit_u8(0x0F);
        self.emit_u8(0xC1);
        self.emit_mem_operand(reg, base, offset, index);
    }

    /// LOCK CMPXCHG [mem], reg64 (F0 48 0F B1 /r)
    pub fn lock_cmpxchg_mem_r64(
        &mut self,
        base: u8,
        offset: i32,
        index: Option<(u8, u8)>,
        reg: u8,
    ) {
        self.emit_u8(0xF0);
        let x = index.map(|(r, _)| r).unwrap_or(0);
        self.emit_rex_full(true, reg, x, base);
        self.emit_u8(0x0F);
        self.emit_u8(0xB1);
        self.emit_mem_operand(reg, base, offset, index);
    }

    /// PAUSE (F3 90) - spin-loop hint
    pub fn pause(&mut self) {
        self.emit_u8(0xF3);
        self.emit_u8(0x90);
    }
}

impl Default for X86_64Encoder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// E1: a SIB operand whose base field is 101 (RBP/R13) with a zero
    /// displacement must not use mod=00 - that encoding means "no base
    /// register, disp32 follows" and would consume the next instruction's
    /// bytes. It must use mod=01 with a zero disp8 instead.
    #[test]
    fn test_sib_base_r13_zero_displacement_emits_disp8() {
        let mut enc = X86_64Encoder::new();

        // mov r8, [r13 + rsi*1 + 0]
        //   REX.WRXB = 4D, 89 /r, mod=01 reg=000 rm=100, SIB(scale=1,idx=110,base=101), disp8=00
        enc.mov_mem_r64(13, 0, Some((6, 1)), 8);
        assert_eq!(enc.buffer, vec![0x4D, 0x89, 0x44, 0x75, 0x00]);
        enc.buffer.clear();

        // mov rax, [rbp + rcx*1 + 0]
        //   REX.W = 48, 8B /r, mod=01 reg=000 rm=100, SIB(scale=1,idx=001,base=101), disp8=00
        enc.mov_r64_mem(0, 5, 0, Some((1, 1)));
        assert_eq!(enc.buffer, vec![0x48, 0x8B, 0x44, 0x4D, 0x00]);
        enc.buffer.clear();

        // A SIB base that is not RBP/R13 keeps the shorter mod=00 form:
        // mov rax, [r9 + rcx*1 + 0] -> 49 8B 04 49 (mod=00, no displacement)
        enc.mov_r64_mem(0, 9, 0, Some((1, 1)));
        assert_eq!(enc.buffer, vec![0x49, 0x8B, 0x04, 0x49]);
        enc.buffer.clear();

        // R12 as base always needs a SIB byte and mod=01 (base field 100 with
        // mod=00 would mean "no base"):
        // mov rax, [r12 + rcx*1 + 0] -> 49 8B 44 4C 00
        enc.mov_r64_mem(0, 12, 0, Some((1, 1)));
        assert_eq!(enc.buffer, vec![0x49, 0x8B, 0x44, 0x4C, 0x00]);
    }

    /// E3: byte stores of SPL/BPL/SIL/DIL (register fields 4..=7) require a
    /// bare REX prefix, otherwise they encode AH/CH/DH/BH.
    #[test]
    fn test_mov_mem_r8_bare_rex_for_byte_registers() {
        let mut enc = X86_64Encoder::new();

        // mov [rax], sil -> bare REX 40, 88 /r (reg=110), mod=00 rm=000
        enc.mov_mem_r8(0, 0, None, 6);
        assert_eq!(enc.buffer, vec![0x40, 0x88, 0x30]);
        enc.buffer.clear();

        // mov [rbp + 0], spl -> REX 40, 88 /r (reg=100), mod=01 rm=101, disp8=00
        enc.mov_mem_r8(5, 0, None, 4);
        assert_eq!(enc.buffer, vec![0x40, 0x88, 0x65, 0x00]);
        enc.buffer.clear();

        // mov [rsp + 0], dil -> REX 40, 88 /r (reg=111), mod=01 rm=100, SIB, disp8=00
        enc.mov_mem_r8(4, 0, None, 7);
        assert_eq!(enc.buffer, vec![0x40, 0x88, 0x7C, 0x24, 0x00]);
        enc.buffer.clear();

        // mov [rax], bl (field 3) needs no REX at all
        enc.mov_mem_r8(0, 0, None, 3);
        assert_eq!(enc.buffer, vec![0x88, 0x18]);
        enc.buffer.clear();

        // mov [r9], r10b -> REX.RX+B extension bits, no bare-Rex needed
        enc.mov_mem_r8(9, 0, None, 10);
        assert_eq!(enc.buffer, vec![0x45, 0x88, 0x11]);
    }

    /// Item 15: the sign-extended 8-bit immediate form is one byte shorter.
    #[test]
    fn test_op_r64_imm8_encoding() {
        let mut enc = X86_64Encoder::new();

        // add rax, 1 -> 48 83 C0 01
        enc.op_r64_imm8(0, 0, 1);
        assert_eq!(enc.buffer, vec![0x48, 0x83, 0xC0, 0x01]);
        enc.buffer.clear();

        // sub r13, -1 -> 49 83 ED FF
        enc.op_r64_imm8(5, 13, -1);
        assert_eq!(enc.buffer, vec![0x49, 0x83, 0xED, 0xFF]);
    }

    /// Item 10: ENDBR64 is encodable natively (F3 0F 1E FA).
    #[test]
    fn test_endbr64_encoding() {
        let mut enc = X86_64Encoder::new();
        enc.endbr64();
        assert_eq!(enc.buffer, vec![0xF3, 0x0F, 0x1E, 0xFA]);
    }

    #[test]
    fn test_sse2_scalar_float_encodings() {
        let mut enc = X86_64Encoder::new();

        // movsd xmm0, xmm1 -> F2 0F 10 C1
        enc.movsd_xmm_xmm(0, 1);
        assert_eq!(enc.buffer, vec![0xF2, 0x0F, 0x10, 0xC1]);
        enc.buffer.clear();

        // addsd xmm0, xmm1 -> F2 0F 58 C1
        enc.addsd_xmm_xmm(0, 1);
        assert_eq!(enc.buffer, vec![0xF2, 0x0F, 0x58, 0xC1]);
        enc.buffer.clear();

        // subsd xmm0, xmm1 -> F2 0F 5C C1
        enc.subsd_xmm_xmm(0, 1);
        assert_eq!(enc.buffer, vec![0xF2, 0x0F, 0x5C, 0xC1]);
        enc.buffer.clear();

        // mulsd xmm0, xmm1 -> F2 0F 59 C1
        enc.mulsd_xmm_xmm(0, 1);
        assert_eq!(enc.buffer, vec![0xF2, 0x0F, 0x59, 0xC1]);
        enc.buffer.clear();

        // divsd xmm0, xmm1 -> F2 0F 5E C1
        enc.divsd_xmm_xmm(0, 1);
        assert_eq!(enc.buffer, vec![0xF2, 0x0F, 0x5E, 0xC1]);
        enc.buffer.clear();

        // ucomisd xmm0, xmm1 -> 66 0F 2E C1
        enc.ucomisd_xmm_xmm(0, 1);
        assert_eq!(enc.buffer, vec![0x66, 0x0F, 0x2E, 0xC1]);
        enc.buffer.clear();

        // cvtsi2sd xmm0, rcx (1) -> F2 48 0F 2A C1
        enc.cvtsi2sd_xmm_r64(0, 1);
        assert_eq!(enc.buffer, vec![0xF2, 0x48, 0x0F, 0x2A, 0xC1]);
        enc.buffer.clear();

        // cvttsd2si rcx (1), xmm0 -> F3 48 0F 2C C0
        enc.cvttsd2si_r64_xmm(1, 0);
        assert_eq!(enc.buffer, vec![0xF2, 0x48, 0x0F, 0x2C, 0xC8]);
        enc.buffer.clear();

        // cvtss2sd xmm0, xmm1 -> F3 0F 5A C1
        enc.cvtss2sd_xmm_xmm(0, 1);
        assert_eq!(enc.buffer, vec![0xF3, 0x0F, 0x5A, 0xC1]);
        enc.buffer.clear();

        // cvtsd2ss xmm0, xmm1 -> F2 0F 5A C1
        enc.cvtsd2ss_xmm_xmm(0, 1);
        assert_eq!(enc.buffer, vec![0xF2, 0x0F, 0x5A, 0xC1]);
        enc.buffer.clear();

        // xorpd xmm0, xmm1 -> 66 0F 57 C1
        enc.xorpd_xmm_xmm(0, 1);
        assert_eq!(enc.buffer, vec![0x66, 0x0F, 0x57, 0xC1]);
        enc.buffer.clear();
    }
}
