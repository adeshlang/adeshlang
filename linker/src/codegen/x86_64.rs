//! Native bit-level x86_64 machine code encoder.

/// 64-bit general purpose registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reg64 {
    Rax = 0,
    Rcx = 1,
    Rdx = 2,
    Rbx = 3,
    Rsp = 4,
    Rbp = 5,
    Rsi = 6,
    Rdi = 7,
    R8 = 8,
    R9 = 9,
    R10 = 10,
    R11 = 11,
    R12 = 12,
    R13 = 13,
    R14 = 14,
    R15 = 15,
}

/// Conditional branch conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Condition {
    Equal = 0x4,        // JE / JZ
    NotEqual = 0x5,     // JNE / JNZ
    Less = 0xC,         // JL / JNGE
    LessEqual = 0xE,    // JLE / JNG
    Greater = 0xF,      // JG / JNLE
    GreaterEqual = 0xD, // JGE / JNL
    Below = 0x2,        // JB / JC (unsigned)
    BelowEqual = 0x6,   // JBE / JNA
    Above = 0x7,        // JA / JNBE
    AboveEqual = 0x3,   // JAE / JNC
}

/// Dynamic buffer for assembling x86_64 machine code.
#[derive(Debug, Clone, Default)]
pub struct X86_64Encoder {
    pub code: Vec<u8>,
}

impl X86_64Encoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.code.len()
    }

    pub fn is_empty(&self) -> bool {
        self.code.is_empty()
    }

    pub fn emit_u8(&mut self, byte: u8) {
        self.code.push(byte);
    }

    pub fn emit_u32(&mut self, val: u32) {
        self.code.extend_from_slice(&val.to_le_bytes());
    }

    pub fn emit_u64(&mut self, val: u64) {
        self.code.extend_from_slice(&val.to_le_bytes());
    }

    /// Emit REX prefix: `0100 W R X B`
    fn emit_rex(&mut self, w: bool, r: u8, x: u8, b: u8) {
        let mut rex = 0x40;
        if w {
            rex |= 0x08;
        }
        if (r & 8) != 0 {
            rex |= 0x04;
        }
        if (x & 8) != 0 {
            rex |= 0x02;
        }
        if (b & 8) != 0 {
            rex |= 0x01;
        }
        if rex != 0x40 || w {
            self.emit_u8(rex);
        }
    }

    /// Emit ModR/M byte: `[mod:2][reg:3][r/m:3]`
    fn emit_modrm(&mut self, mod_bits: u8, reg: u8, rm: u8) {
        let byte = ((mod_bits & 0x3) << 6) | ((reg & 0x7) << 3) | (rm & 0x7);
        self.emit_u8(byte);
    }

    // --- Core Instructions ---

    /// NOP (0x90)
    pub fn nop(&mut self) {
        self.emit_u8(0x90);
    }

    /// RET (0xC3)
    pub fn ret(&mut self) {
        self.emit_u8(0xC3);
    }

    /// PUSH reg64 (0x50 + rd or REX.B + 0x50 + rd)
    pub fn push_reg(&mut self, reg: Reg64) {
        let r = reg as u8;
        if (r & 8) != 0 {
            self.emit_u8(0x41);
        }
        self.emit_u8(0x50 + (r & 7));
    }

    /// POP reg64 (0x58 + rd or REX.B + 0x58 + rd)
    pub fn pop_reg(&mut self, reg: Reg64) {
        let r = reg as u8;
        if (r & 8) != 0 {
            self.emit_u8(0x41);
        }
        self.emit_u8(0x58 + (r & 7));
    }

    /// MOV reg64, reg64 (REX.W + 0x89 ModRM)
    pub fn mov_reg_reg(&mut self, dst: Reg64, src: Reg64) {
        let d = dst as u8;
        let s = src as u8;
        self.emit_rex(true, s, 0, d);
        self.emit_u8(0x89);
        self.emit_modrm(3, s, d);
    }

    /// MOV reg64, imm64 (REX.W + 0xB8 + rd + imm64)
    pub fn mov_reg_imm64(&mut self, dst: Reg64, imm: u64) {
        let d = dst as u8;
        self.emit_rex(true, 0, 0, d);
        self.emit_u8(0xB8 + (d & 7));
        self.emit_u64(imm);
    }

    /// MOV reg64, imm32 (signed/zero-extended)
    pub fn mov_reg_imm32(&mut self, dst: Reg64, imm: u32) {
        let d = dst as u8;
        if (d & 8) != 0 {
            self.emit_u8(0x41);
        }
        self.emit_u8(0xB8 + (d & 7));
        self.emit_u32(imm);
    }

    /// MOV reg64, [base + disp32]
    pub fn mov_reg_mem(&mut self, dst: Reg64, base: Reg64, disp: i32) {
        let d = dst as u8;
        let b = base as u8;
        self.emit_rex(true, d, 0, b);
        self.emit_u8(0x8B);
        if disp == 0 && (b & 7) != 5 && (b & 7) != 4 {
            self.emit_modrm(0, d, b);
        } else if disp >= -128 && disp <= 127 {
            self.emit_modrm(1, d, b);
            if (b & 7) == 4 {
                self.emit_u8(0x24);
            } // SIB for RSP
            self.emit_u8(disp as u8);
        } else {
            self.emit_modrm(2, d, b);
            if (b & 7) == 4 {
                self.emit_u8(0x24);
            }
            self.emit_u32(disp as u32);
        }
    }

    /// MOV [base + disp32], reg64
    pub fn mov_mem_reg(&mut self, base: Reg64, disp: i32, src: Reg64) {
        let s = src as u8;
        let b = base as u8;
        self.emit_rex(true, s, 0, b);
        self.emit_u8(0x89);
        if disp == 0 && (b & 7) != 5 && (b & 7) != 4 {
            self.emit_modrm(0, s, b);
        } else if disp >= -128 && disp <= 127 {
            self.emit_modrm(1, s, b);
            if (b & 7) == 4 {
                self.emit_u8(0x24);
            }
            self.emit_u8(disp as u8);
        } else {
            self.emit_modrm(2, s, b);
            if (b & 7) == 4 {
                self.emit_u8(0x24);
            }
            self.emit_u32(disp as u32);
        }
    }

    /// ADD reg64, reg64 (REX.W + 0x01)
    pub fn add_reg_reg(&mut self, dst: Reg64, src: Reg64) {
        let d = dst as u8;
        let s = src as u8;
        self.emit_rex(true, s, 0, d);
        self.emit_u8(0x01);
        self.emit_modrm(3, s, d);
    }

    /// ADD reg64, imm32 (REX.W + 0x81 /0)
    pub fn add_reg_imm32(&mut self, dst: Reg64, imm: u32) {
        let d = dst as u8;
        self.emit_rex(true, 0, 0, d);
        self.emit_u8(0x81);
        self.emit_modrm(3, 0, d);
        self.emit_u32(imm);
    }

    /// SUB reg64, reg64 (REX.W + 0x29)
    pub fn sub_reg_reg(&mut self, dst: Reg64, src: Reg64) {
        let d = dst as u8;
        let s = src as u8;
        self.emit_rex(true, s, 0, d);
        self.emit_u8(0x29);
        self.emit_modrm(3, s, d);
    }

    /// SUB reg64, imm32 (REX.W + 0x81 /5)
    pub fn sub_reg_imm32(&mut self, dst: Reg64, imm: u32) {
        let d = dst as u8;
        self.emit_rex(true, 0, 0, d);
        self.emit_u8(0x81);
        self.emit_modrm(3, 5, d);
        self.emit_u32(imm);
    }

    /// XOR reg64, reg64 (REX.W + 0x31) - zeroing register
    pub fn xor_reg_reg(&mut self, dst: Reg64, src: Reg64) {
        let d = dst as u8;
        let s = src as u8;
        self.emit_rex(true, s, 0, d);
        self.emit_u8(0x31);
        self.emit_modrm(3, s, d);
    }

    /// CMP reg64, reg64 (REX.W + 0x39)
    pub fn cmp_reg_reg(&mut self, dst: Reg64, src: Reg64) {
        let d = dst as u8;
        let s = src as u8;
        self.emit_rex(true, s, 0, d);
        self.emit_u8(0x39);
        self.emit_modrm(3, s, d);
    }

    /// JMP rel32 (0xE9 + rel32)
    pub fn jmp_rel32(&mut self, rel32: i32) {
        self.emit_u8(0xE9);
        self.emit_u32(rel32 as u32);
    }

    /// Jcc rel32 (0x0F + 0x80+cond + rel32)
    pub fn jcc_rel32(&mut self, cond: Condition, rel32: i32) {
        self.emit_u8(0x0F);
        self.emit_u8(0x80 | (cond as u8));
        self.emit_u32(rel32 as u32);
    }

    /// CALL rel32 (0xE8 + rel32)
    pub fn call_rel32(&mut self, rel32: i32) {
        self.emit_u8(0xE8);
        self.emit_u32(rel32 as u32);
    }

    /// SYSCALL (0x0F 0x05)
    pub fn syscall(&mut self) {
        self.emit_u8(0x0F);
        self.emit_u8(0x05);
    }
}
