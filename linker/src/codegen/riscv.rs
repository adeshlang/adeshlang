//! Native RISC-V (RV32I / RV64I) machine code instruction encoder.

/// RISC-V 32/64-bit integer registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RReg {
    Zero = 0,
    Ra = 1,   // Return Address
    Sp = 2,   // Stack Pointer
    Gp = 3,   // Global Pointer
    Tp = 4,   // Thread Pointer
    T0 = 5,
    T1 = 6,
    T2 = 7,
    S0 = 8,   // Saved / Frame Pointer (Fp)
    S1 = 9,
    A0 = 10,  // Function Arg / Return Value 0
    A1 = 11,  // Function Arg / Return Value 1
    A2 = 12,
    A3 = 13,
    A4 = 14,
    A5 = 15,
    A6 = 16,
    A7 = 17,
    S2 = 18,
    S3 = 19,
    S4 = 20,
    S5 = 21,
    S6 = 22,
    S7 = 23,
    S8 = 24,
    S9 = 25,
    S10 = 26,
    S11 = 27,
    T3 = 28,
    T4 = 29,
    T5 = 30,
    T6 = 31,
}

/// Dynamic buffer for assembling RISC-V instructions.
#[derive(Debug, Clone, Default)]
pub struct RiscvEncoder {
    pub code: Vec<u8>,
}

impl RiscvEncoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn emit_insn(&mut self, insn: u32) {
        self.code.extend_from_slice(&insn.to_le_bytes());
    }

    // --- Formats ---

    /// Emit R-type: `[funct7:7][rs2:5][rs1:5][funct3:3][rd:5][opcode:7]`
    pub fn r_type(&mut self, opcode: u32, funct3: u32, funct7: u32, rd: RReg, rs1: RReg, rs2: RReg) {
        let insn = (funct7 << 25)
            | ((rs2 as u32) << 20)
            | ((rs1 as u32) << 15)
            | (funct3 << 12)
            | ((rd as u32) << 7)
            | (opcode & 0x7F);
        self.emit_insn(insn);
    }

    /// Emit I-type: `[imm11:0:12][rs1:5][funct3:3][rd:5][opcode:7]`
    pub fn i_type(&mut self, opcode: u32, funct3: u32, rd: RReg, rs1: RReg, imm: i32) {
        let imm12 = (imm as u32) & 0x0FFF;
        let insn = (imm12 << 20)
            | ((rs1 as u32) << 15)
            | (funct3 << 12)
            | ((rd as u32) << 7)
            | (opcode & 0x7F);
        self.emit_insn(insn);
    }

    /// Emit S-type: `[imm11:5:7][rs2:5][rs1:5][funct3:3][imm4:0:5][opcode:7]`
    pub fn s_type(&mut self, opcode: u32, funct3: u32, rs1: RReg, rs2: RReg, imm: i32) {
        let imm_u = imm as u32;
        let imm_11_5 = (imm_u >> 5) & 0x7F;
        let imm_4_0 = imm_u & 0x1F;
        let insn = (imm_11_5 << 25)
            | ((rs2 as u32) << 20)
            | ((rs1 as u32) << 15)
            | (funct3 << 12)
            | (imm_4_0 << 7)
            | (opcode & 0x7F);
        self.emit_insn(insn);
    }

    /// Emit B-type: `[imm12:1|imm10:5:6][rs2:5][rs1:5][funct3:3][imm4:1:4|imm11:1][opcode:7]`
    pub fn b_type(&mut self, funct3: u32, rs1: RReg, rs2: RReg, imm: i32) {
        let imm_u = (imm as u32) >> 1; // imm[0] is 0
        let imm12 = (imm_u >> 11) & 0x1;
        let imm10_5 = (imm_u >> 4) & 0x3F;
        let imm4_1 = imm_u & 0xF;
        let imm11 = (imm_u >> 10) & 0x1;
        let insn = (imm12 << 31)
            | (imm10_5 << 25)
            | ((rs2 as u32) << 20)
            | ((rs1 as u32) << 15)
            | (funct3 << 12)
            | (imm4_1 << 8)
            | (imm11 << 7)
            | 0x63;
        self.emit_insn(insn);
    }

    /// Emit U-type: `[imm31:12:20][rd:5][opcode:7]`
    pub fn u_type(&mut self, opcode: u32, rd: RReg, imm20: u32) {
        let insn = ((imm20 & 0x000F_FFFF) << 12) | ((rd as u32) << 7) | (opcode & 0x7F);
        self.emit_insn(insn);
    }

    /// Emit J-type: `[imm20|imm10:1|imm11|imm19:12][rd:5][opcode:7]` (JAL)
    pub fn j_type(&mut self, rd: RReg, imm: i32) {
        let imm_u = (imm as u32) >> 1;
        let imm20 = (imm_u >> 19) & 0x1;
        let imm10_1 = imm_u & 0x03FF;
        let imm11 = (imm_u >> 10) & 0x1;
        let imm19_12 = (imm_u >> 11) & 0xFF;
        let insn = (imm20 << 31)
            | (imm10_1 << 21)
            | (imm11 << 20)
            | (imm19_12 << 12)
            | ((rd as u32) << 7)
            | 0x6F;
        self.emit_insn(insn);
    }

    // --- Standard Instructions ---

    /// ADD rd, rs1, rs2
    pub fn add(&mut self, rd: RReg, rs1: RReg, rs2: RReg) {
        self.r_type(0x33, 0x0, 0x00, rd, rs1, rs2);
    }

    /// SUB rd, rs1, rs2
    pub fn sub(&mut self, rd: RReg, rs1: RReg, rs2: RReg) {
        self.r_type(0x33, 0x0, 0x20, rd, rs1, rs2);
    }

    /// ADDI rd, rs1, imm12
    pub fn addi(&mut self, rd: RReg, rs1: RReg, imm12: i32) {
        self.i_type(0x13, 0x0, rd, rs1, imm12);
    }

    /// LW rd, offset(rs1)
    pub fn lw(&mut self, rd: RReg, rs1: RReg, offset: i32) {
        self.i_type(0x03, 0x2, rd, rs1, offset);
    }

    /// SW rs2, offset(rs1)
    pub fn sw(&mut self, rs1: RReg, rs2: RReg, offset: i32) {
        self.s_type(0x23, 0x2, rs1, rs2, offset);
    }

    /// LUI rd, imm20
    pub fn lui(&mut self, rd: RReg, imm20: u32) {
        self.u_type(0x37, rd, imm20);
    }

    /// AUIPC rd, imm20
    pub fn auipc(&mut self, rd: RReg, imm20: u32) {
        self.u_type(0x17, rd, imm20);
    }

    /// JAL rd, offset
    pub fn jal(&mut self, rd: RReg, offset: i32) {
        self.j_type(rd, offset);
    }

    /// JALR rd, rs1, offset (RET is `jalr zero, ra, 0`)
    pub fn jalr(&mut self, rd: RReg, rs1: RReg, offset: i32) {
        self.i_type(0x67, 0x0, rd, rs1, offset);
    }

    /// RET pseudo-instruction: `jalr x0, x1, 0`
    pub fn ret(&mut self) {
        self.jalr(RReg::Zero, RReg::Ra, 0);
    }

    /// ECALL (Syscall)
    pub fn ecall(&mut self) {
        self.i_type(0x73, 0x0, RReg::Zero, RReg::Zero, 0);
    }

    /// NOP pseudo-instruction: `addi x0, x0, 0`
    pub fn nop(&mut self) {
        self.addi(RReg::Zero, RReg::Zero, 0);
    }
}
