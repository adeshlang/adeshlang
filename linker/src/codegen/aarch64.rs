//! Native fixed-width 32-bit AArch64 (ARM 64-bit) machine code encoder.

/// AArch64 64-bit registers (X0..X30, SP, XZR).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AReg {
    X0 = 0,
    X1 = 1,
    X2 = 2,
    X3 = 3,
    X4 = 4,
    X5 = 5,
    X6 = 6,
    X7 = 7,
    X8 = 8,
    X9 = 9,
    X10 = 10,
    X11 = 11,
    X12 = 12,
    X13 = 13,
    X14 = 14,
    X15 = 15,
    X16 = 16,
    X17 = 17,
    X18 = 18,
    X19 = 19,
    X20 = 20,
    X21 = 21,
    X22 = 22,
    X23 = 23,
    X24 = 24,
    X25 = 25,
    X26 = 26,
    X27 = 27,
    X28 = 28,
    X29 = 29, // Frame Pointer (FP)
    X30 = 30, // Link Register (LR)
    Sp = 31,  // Stack Pointer
    Xzr = 32, // Zero Register
}

/// AArch64 Condition Codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ACond {
    Eq = 0x0,
    Ne = 0x1,
    Cs = 0x2, // Carry Set / Unsigned Higher or Same (HS)
    Cc = 0x3, // Carry Clear / Unsigned Lower (LO)
    Mi = 0x4, // Minus / Negative
    Pl = 0x5, // Plus / Positive
    Vs = 0x6, // Overflow Set
    Vc = 0x7, // Overflow Clear
    Hi = 0x8, // Unsigned Higher
    Ls = 0x9, // Unsigned Lower or Same
    Ge = 0xA, // Signed Greater or Equal
    Lt = 0xB, // Signed Less Than
    Gt = 0xC, // Signed Greater Than
    Le = 0xD, // Signed Less or Equal
    Al = 0xE, // Always
}

/// Dynamic buffer for assembling AArch64 instructions.
#[derive(Debug, Clone, Default)]
pub struct Aarch64Encoder {
    pub code: Vec<u8>,
}

impl Aarch64Encoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn emit_insn(&mut self, insn: u32) {
        self.code.extend_from_slice(&insn.to_le_bytes());
    }

    fn reg_num(r: AReg) -> u32 {
        let n = r as u32;
        if n >= 31 { 31 } else { n }
    }

    // --- Arithmetic & Logical ---

    /// ADD Xd, Xn, Xm (64-bit)
    pub fn add_reg(&mut self, rd: AReg, rn: AReg, rm: AReg) {
        let insn = 0x8B00_0000 | (Self::reg_num(rm) << 16) | (Self::reg_num(rn) << 5) | Self::reg_num(rd);
        self.emit_insn(insn);
    }

    /// ADD Xd, Xn, #imm12 (64-bit)
    pub fn add_imm(&mut self, rd: AReg, rn: AReg, imm12: u16) {
        let insn = 0x9100_0000 | (((imm12 & 0x0FFF) as u32) << 10) | (Self::reg_num(rn) << 5) | Self::reg_num(rd);
        self.emit_insn(insn);
    }

    /// SUB Xd, Xn, Xm (64-bit)
    pub fn sub_reg(&mut self, rd: AReg, rn: AReg, rm: AReg) {
        let insn = 0xCB00_0000 | (Self::reg_num(rm) << 16) | (Self::reg_num(rn) << 5) | Self::reg_num(rd);
        self.emit_insn(insn);
    }

    /// SUB Xd, Xn, #imm12 (64-bit)
    pub fn sub_imm(&mut self, rd: AReg, rn: AReg, imm12: u16) {
        let insn = 0xD100_0000 | (((imm12 & 0x0FFF) as u32) << 10) | (Self::reg_num(rn) << 5) | Self::reg_num(rd);
        self.emit_insn(insn);
    }

    /// ORR Xd, XZR, Xm (MOV Xd, Xm)
    pub fn mov_reg(&mut self, rd: AReg, rm: AReg) {
        let insn = 0xAA00_03E0 | (Self::reg_num(rm) << 16) | Self::reg_num(rd);
        self.emit_insn(insn);
    }

    /// MOVZ Xd, #imm16, LSL #shift (shift in 0, 16, 32, 48)
    pub fn movz(&mut self, rd: AReg, imm16: u16, shift: u8) {
        let hw = (shift / 16) as u32;
        let insn = 0xD280_0000 | (hw << 21) | ((imm16 as u32) << 5) | Self::reg_num(rd);
        self.emit_insn(insn);
    }

    /// MOVK Xd, #imm16, LSL #shift
    pub fn movk(&mut self, rd: AReg, imm16: u16, shift: u8) {
        let hw = (shift / 16) as u32;
        let insn = 0xF280_0000 | (hw << 21) | ((imm16 as u32) << 5) | Self::reg_num(rd);
        self.emit_insn(insn);
    }

    // --- Memory Load/Store ---

    /// LDR Xt, [Xn, #offset] (64-bit unsigned offset, 8-byte aligned)
    pub fn ldr_imm(&mut self, rt: AReg, rn: AReg, offset: u32) {
        let scale = offset / 8;
        let insn = 0xF940_0000 | ((scale & 0x0FFF) << 10) | (Self::reg_num(rn) << 5) | Self::reg_num(rt);
        self.emit_insn(insn);
    }

    /// STR Xt, [Xn, #offset] (64-bit unsigned offset, 8-byte aligned)
    pub fn str_imm(&mut self, rt: AReg, rn: AReg, offset: u32) {
        let scale = offset / 8;
        let insn = 0xF900_0000 | ((scale & 0x0FFF) << 10) | (Self::reg_num(rn) << 5) | Self::reg_num(rt);
        self.emit_insn(insn);
    }

    // --- Control Flow ---

    /// RET {Xn} (defaults to LR/X30)
    pub fn ret(&mut self) {
        self.emit_insn(0xD65F_03C0); // RET X30
    }

    /// B offset (26-bit signed immediate offset in instructions)
    pub fn b(&mut self, imm26: i32) {
        let insn = 0x1400_0000 | ((imm26 as u32) & 0x03FF_FFFF);
        self.emit_insn(insn);
    }

    /// BL offset (26-bit signed branch with link)
    pub fn bl(&mut self, imm26: i32) {
        let insn = 0x9400_0000 | ((imm26 as u32) & 0x03FF_FFFF);
        self.emit_insn(insn);
    }

    /// B.cond offset (19-bit signed conditional branch)
    pub fn b_cond(&mut self, cond: ACond, imm19: i32) {
        let insn = 0x5400_0000 | (((imm19 as u32) & 0x0007_FFFF) << 5) | (cond as u32);
        self.emit_insn(insn);
    }

    /// SVC #imm16 (Supervisor Call / Syscall)
    pub fn svc(&mut self, imm16: u16) {
        let insn = 0xD400_0001 | ((imm16 as u32) << 5);
        self.emit_insn(insn);
    }

    /// NOP (0xD503201F)
    pub fn nop(&mut self) {
        self.emit_insn(0xD503_201F);
    }
}
