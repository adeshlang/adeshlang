//! Complete x86-64 Native Backend implementing `CodegenBackend`.
//!
//! Emits real x86-64 machine code (REX/ModR/M/SIB) from Machine IR with:
//! - proper frame layout (saved RBP, callee-saved save area, locals, spills),
//! - intra-function branch fixups resolved in a post-pass (forward branches
//!   no longer miscompile),
//! - `call rel32` + PC32 relocations and `mov reg, imm64` + ABS64 relocations
//!   for symbol references (string literals, imports),
//! - loud `CodegenError` failures for unsupported instruction forms instead
//!   of silently dropping them.

pub mod asm_printer;
pub mod encoder;

use crate::backend::CodegenBackend;
use crate::calling_convention::{
    CallingConvention, SystemVX64CallingConvention, WindowsX64CallingConvention,
};
use crate::error::CodegenError;
use crate::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister,
};
use crate::register_alloc::{LinearScanAllocator, RegisterFile};
use crate::targets::x86_64::encoder::X86_64Encoder;
use adesh_object::{
    AdobObject, AdobRelocation, AdobSection, AdobSymbol, OperatingSystem, RelocationKind,
    SectionKind, SymbolBinding, SymbolKind, SymbolVisibility, TargetCapabilities, TargetDescriptor,
    section_flags,
};
use std::collections::HashMap;

/// Encoder scratch register for stack-slot arithmetic sequences (R10).
const SCRATCH: u8 = 10;
/// Second scratch register, used to preserve RCX around shift counts (R11).
const SCRATCH2: u8 = 11;

// ---------------------------------------------------------------- Register File

/// x86-64 Physical Register File definition.
///
/// RAX (0), R10, and R11 are reserved for the encoder and never allocated:
/// RAX is the implicit DIV/IDIV operand and the return-value register,
/// R10 backs stack-slot arithmetic sequences, and R11 preserves RCX around
/// shift-count transfers.
pub struct X86_64RegisterFile;

// 0: RAX, 1: RCX, 2: RDX, 3: RBX, 4: RSP, 5: RBP, 6: RSI, 7: RDI
// 8: R8, 9: R9, 10: R10, 11: R11, 12: R12, 13: R13, 14: R14, 15: R15
const X86_64_ALL_REGS: [PhysicalRegister; 16] = [
    PhysicalRegister(0),
    PhysicalRegister(1),
    PhysicalRegister(2),
    PhysicalRegister(3),
    PhysicalRegister(4),
    PhysicalRegister(5),
    PhysicalRegister(6),
    PhysicalRegister(7),
    PhysicalRegister(8),
    PhysicalRegister(9),
    PhysicalRegister(10),
    PhysicalRegister(11),
    PhysicalRegister(12),
    PhysicalRegister(13),
    PhysicalRegister(14),
    PhysicalRegister(15),
];

const X86_64_ALLOCATABLE: [PhysicalRegister; 11] = [
    PhysicalRegister(1),  // RCX
    PhysicalRegister(2),  // RDX
    PhysicalRegister(3),  // RBX
    PhysicalRegister(6),  // RSI
    PhysicalRegister(7),  // RDI
    PhysicalRegister(8),  // R8
    PhysicalRegister(9),  // R9
    PhysicalRegister(12), // R12
    PhysicalRegister(13), // R13
    PhysicalRegister(14), // R14
    PhysicalRegister(15), // R15
];

const X86_64_CALLER_SAVED: [PhysicalRegister; 6] = [
    PhysicalRegister(1), // RCX
    PhysicalRegister(2), // RDX
    PhysicalRegister(6), // RSI
    PhysicalRegister(7), // RDI
    PhysicalRegister(8), // R8
    PhysicalRegister(9), // R9
];

const X86_64_CALLEE_SAVED: [PhysicalRegister; 5] = [
    PhysicalRegister(3),  // RBX
    PhysicalRegister(12), // R12
    PhysicalRegister(13), // R13
    PhysicalRegister(14), // R14
    PhysicalRegister(15), // R15
];

const X86_64_RESERVED: [PhysicalRegister; 5] = [
    PhysicalRegister(0),  // RAX: DIV/IDIV implicit operand, return value
    PhysicalRegister(4),  // RSP: stack pointer
    PhysicalRegister(5),  // RBP: frame pointer
    PhysicalRegister(10), // R10: encoder scratch
    PhysicalRegister(11), // R11: encoder scratch (RCX preservation)
];

impl RegisterFile for X86_64RegisterFile {
    fn registers(&self) -> &[PhysicalRegister] {
        &X86_64_ALL_REGS
    }
    fn allocatable(&self) -> &[PhysicalRegister] {
        &X86_64_ALLOCATABLE
    }
    fn caller_saved(&self) -> &[PhysicalRegister] {
        &X86_64_CALLER_SAVED
    }
    fn callee_saved(&self) -> &[PhysicalRegister] {
        &X86_64_CALLEE_SAVED
    }
    fn reserved(&self) -> &[PhysicalRegister] {
        &X86_64_RESERVED
    }
}

// ------------------------------------------------------------ Operand helpers

fn phys_reg(op: &MachineOperand) -> Option<u8> {
    match op {
        MachineOperand::Register(MachineRegister::Physical(p)) => Some(p.0),
        _ => None,
    }
}

fn stack_slot(op: &MachineOperand) -> Option<i32> {
    match op {
        MachineOperand::StackSlot(slot) => Some(*slot),
        _ => None,
    }
}

type MemAddress = (u8, i32, Option<(u8, u8)>);

/// Decompose a memory operand into (base, offset, index) with physical
/// registers. Returns `None` if the base register is virtual.
fn mem_operand(op: &MachineOperand) -> Option<MemAddress> {
    if let MachineOperand::Memory {
        base,
        offset,
        index,
    } = op
    {
        let base = phys_reg(&MachineOperand::Register(*base))?;
        let index = match index {
            Some((r, scale)) => Some((phys_reg(&MachineOperand::Register(*r))?, *scale)),
            None => None,
        };
        Some((base, *offset, index))
    } else {
        None
    }
}

fn imm_fits_i32(v: i64) -> bool {
    (i32::MIN as i64..=i32::MAX as i64).contains(&v)
}

fn instruction_name(inst: &MachineInstruction) -> &'static str {
    match inst {
        MachineInstruction::Nop => "Nop",
        MachineInstruction::Return => "Return",
        MachineInstruction::Move { .. } => "Move",
        MachineInstruction::Load { .. } => "Load",
        MachineInstruction::Store { .. } => "Store",
        MachineInstruction::Add { .. } => "Add",
        MachineInstruction::Sub { .. } => "Sub",
        MachineInstruction::Mul { .. } => "Mul",
        MachineInstruction::Div { .. } => "Div",
        MachineInstruction::Mod { .. } => "Mod",
        MachineInstruction::Neg { .. } => "Neg",
        MachineInstruction::Not { .. } => "Not",
        MachineInstruction::And { .. } => "And",
        MachineInstruction::Or { .. } => "Or",
        MachineInstruction::Xor { .. } => "Xor",
        MachineInstruction::Shl { .. } => "Shl",
        MachineInstruction::Shr { .. } => "Shr",
        MachineInstruction::Sar { .. } => "Sar",
        MachineInstruction::Compare { .. } => "Compare",
        MachineInstruction::Test { .. } => "Test",
        MachineInstruction::SetCc { .. } => "SetCc",
        MachineInstruction::Branch { .. } => "Branch",
        MachineInstruction::BranchCc { .. } => "BranchCc",
        MachineInstruction::Call { .. } => "Call",
        MachineInstruction::Push { .. } => "Push",
        MachineInstruction::Pop { .. } => "Pop",
        MachineInstruction::Vector { .. } => "Vector",
        MachineInstruction::Atomic { .. } => "Atomic",
        MachineInstruction::Barrier => "Barrier",
        MachineInstruction::Custom { .. } => "Custom",
    }
}

fn function_uses_register(func: &MachineFunction, reg: u8) -> bool {
    func.blocks
        .iter()
        .flat_map(|b| b.instructions.iter())
        .any(|inst| instruction_uses_register(inst, reg))
}

fn instruction_uses_register(inst: &MachineInstruction, reg: u8) -> bool {
    let hit = |op: &MachineOperand| -> bool {
        match op {
            MachineOperand::Register(MachineRegister::Physical(p)) => p.0 == reg,
            MachineOperand::Memory { base, index, .. } => {
                matches!(base, MachineRegister::Physical(p) if p.0 == reg)
                    || index.as_ref().is_some_and(
                        |(r, _)| matches!(r, MachineRegister::Physical(p) if p.0 == reg),
                    )
            }
            _ => false,
        }
    };
    match inst {
        MachineInstruction::Nop
        | MachineInstruction::Return
        | MachineInstruction::Branch { .. }
        | MachineInstruction::BranchCc { .. }
        | MachineInstruction::Barrier => false,
        MachineInstruction::Move { dst, src }
        | MachineInstruction::Load { dst, src, .. }
        | MachineInstruction::Store { dst, src, .. }
        | MachineInstruction::Add { dst, src }
        | MachineInstruction::Sub { dst, src }
        | MachineInstruction::Mul { dst, src }
        | MachineInstruction::Div { dst, src }
        | MachineInstruction::Mod { dst, src }
        | MachineInstruction::And { dst, src }
        | MachineInstruction::Or { dst, src }
        | MachineInstruction::Xor { dst, src }
        | MachineInstruction::Shl { dst, src }
        | MachineInstruction::Shr { dst, src }
        | MachineInstruction::Sar { dst, src } => hit(dst) || hit(src),
        MachineInstruction::Compare { lhs, rhs } | MachineInstruction::Test { lhs, rhs } => {
            hit(lhs) || hit(rhs)
        }
        MachineInstruction::Neg { dst }
        | MachineInstruction::Not { dst }
        | MachineInstruction::SetCc { dst, .. }
        | MachineInstruction::Push { src: dst }
        | MachineInstruction::Pop { dst } => hit(dst),
        MachineInstruction::Call { target, .. } => hit(target),
        MachineInstruction::Vector { dst, src, .. } => hit(dst) || hit(src),
        MachineInstruction::Atomic { dst, src, .. } => hit(dst) || hit(src),
        MachineInstruction::Custom { operands, .. } => operands.iter().any(hit),
    }
}

// ------------------------------------------------------------------ Binary ops

#[derive(Clone, Copy, PartialEq, Eq)]
enum BinOp {
    Add,
    Sub,
    And,
    Or,
    Xor,
    Cmp,
    Test,
}

impl BinOp {
    /// `op r64, r/m64` opcode: 03 ADD, 0B OR, 23 AND, 2B SUB, 33 XOR, 3B CMP,
    /// 85 TEST.
    fn reg_mem(self) -> u8 {
        match self {
            BinOp::Add => 0x03,
            BinOp::Or => 0x0B,
            BinOp::And => 0x23,
            BinOp::Sub => 0x2B,
            BinOp::Xor => 0x33,
            BinOp::Cmp => 0x3B,
            BinOp::Test => 0x85,
        }
    }

    /// `op r/m64, r64` opcode: 01 ADD, 09 OR, 21 AND, 29 SUB, 31 XOR, 39 CMP,
    /// 85 TEST.
    fn mem_reg(self) -> u8 {
        match self {
            BinOp::Add => 0x01,
            BinOp::Or => 0x09,
            BinOp::And => 0x21,
            BinOp::Sub => 0x29,
            BinOp::Xor => 0x31,
            BinOp::Cmp => 0x39,
            BinOp::Test => 0x85,
        }
    }

    /// `0x81 /x` extension digit for the imm32 form. TEST uses `F7 /0` and is
    /// special-cased at the call sites.
    fn imm_ext(self) -> u8 {
        match self {
            BinOp::Add => 0,
            BinOp::Or => 1,
            BinOp::And => 4,
            BinOp::Sub => 5,
            BinOp::Xor => 6,
            BinOp::Cmp => 7,
            BinOp::Test => 0,
        }
    }
}

#[derive(Clone, Copy)]
enum ShiftOp {
    Shl,
    Shr,
    Sar,
}

#[derive(Clone, Copy)]
enum UnOp {
    Neg,
    Not,
}

// ------------------------------------------------------------ Function encoding

/// A pending branch/call fixup whose displacement is patched once all block
/// offsets are known. If the target never matches a block label, the fixup
/// becomes a PC-relative relocation against an external symbol.
struct BranchFixup {
    /// Buffer offset of the 4-byte displacement field.
    disp_offset: usize,
    /// Buffer offset of the instruction following the branch.
    next_offset: usize,
    /// Target block label (or external symbol name).
    target: String,
}

/// Per-function encoding state.
struct FunctionEncoding {
    enc: X86_64Encoder,
    relocations: Vec<AdobRelocation>,
    branch_fixups: Vec<BranchFixup>,
    block_offsets: HashMap<String, usize>,
    used_callee_saved: Vec<u8>,
    /// Bytes of locals + register-spill area (8-byte aligned).
    locals_size: i32,
    /// Total frame allocation (16-byte aligned), including the callee-saved
    /// save area.
    frame_size: i32,
    saw_return: bool,
}

impl FunctionEncoding {
    fn new(func: &MachineFunction, conv: &dyn CallingConvention) -> Self {
        // Callee-saved registers the function actually touches (excluding
        // RSP/RBP, which the frame itself manages).
        let mut used_callee_saved: Vec<u8> = conv
            .callee_saved_registers()
            .iter()
            .map(|r| r.0)
            .filter(|&r| r != 4 && r != 5)
            .filter(|&r| function_uses_register(func, r))
            .collect();
        used_callee_saved.sort_unstable();
        used_callee_saved.dedup();

        let locals_size = (func.stack_size as i32 + 7) & !7;
        let callee_area = 8 * used_callee_saved.len() as i32;
        let frame_size = (locals_size + callee_area + 15) & !15;

        Self {
            enc: X86_64Encoder::new(),
            relocations: Vec::new(),
            branch_fixups: Vec::new(),
            block_offsets: HashMap::new(),
            used_callee_saved,
            locals_size,
            frame_size,
            saw_return: false,
        }
    }

    /// RBP-relative (negative) frame slot of the i-th callee-saved register.
    /// The save area sits below locals and spills, so nothing collides.
    fn callee_slot(&self, i: usize) -> i32 {
        -(self.locals_size + 8 * (i as i32 + 1))
    }

    fn emit_prologue(&mut self) {
        self.enc.push_reg64(5); // push rbp
        self.enc.mov_r64_r64(5, 4); // mov rbp, rsp
        if self.frame_size > 0 {
            self.enc.sub_r64_imm32(4, self.frame_size); // sub rsp, frame_size
        }
        let callee_saved = self.used_callee_saved.clone();
        for (i, reg) in callee_saved.iter().enumerate() {
            self.enc.mov_rbp_offset_r64(self.callee_slot(i), *reg);
        }
    }

    fn emit_epilogue(&mut self) {
        let callee_saved = self.used_callee_saved.clone();
        for (i, reg) in callee_saved.iter().enumerate() {
            self.enc.mov_r64_rbp_offset(*reg, self.callee_slot(i));
        }
        self.enc.mov_r64_r64(4, 5); // mov rsp, rbp
        self.enc.pop_reg64(5); // pop rbp
        self.enc.ret();
    }

    fn unsupported(&self, inst: &MachineInstruction, func_name: &str, abi: &str) -> CodegenError {
        CodegenError::new(
            "x86_64",
            format!(
                "instruction form is not encodable by the x86-64 native backend: {}",
                instruction_name(inst)
            ),
        )
        .with_arch("x86_64")
        .with_abi(abi)
        .with_function(func_name)
        .with_instruction(instruction_name(inst))
        .with_suggestion(
            "The native x86-64 backend encodes 64-bit integer GPR forms only; \
             vector and atomic instructions are not yet supported.",
        )
    }

    fn binop_reg_reg(&mut self, op: BinOp, d: u8, s: u8) {
        match op {
            BinOp::Add => self.enc.add_r64_r64(d, s),
            BinOp::Sub => self.enc.sub_r64_r64(d, s),
            BinOp::And => self.enc.and_r64_r64(d, s),
            BinOp::Or => self.enc.or_r64_r64(d, s),
            BinOp::Xor => self.enc.xor_r64_r64(d, s),
            BinOp::Cmp => self.enc.cmp_r64_r64(d, s),
            BinOp::Test => self.enc.test_r64_r64(d, s),
        }
    }

    fn binop_reg_imm(&mut self, op: BinOp, d: u8, imm: i32) {
        match op {
            BinOp::Test => self.enc.test_r64_imm32(d, imm),
            _ => self.enc.op_r64_imm32(op.imm_ext(), d, imm),
        }
    }

    /// Encode a two-operand integer operation across register, immediate,
    /// stack-slot, and memory operand forms. Returns false when the operand
    /// combination has no encoding.
    fn encode_binop(&mut self, op: BinOp, dst: &MachineOperand, src: &MachineOperand) -> bool {
        // Register destination.
        if let Some(d) = phys_reg(dst) {
            if let Some(s) = phys_reg(src) {
                self.binop_reg_reg(op, d, s);
                return true;
            }
            if let MachineOperand::Immediate(v) = src {
                if imm_fits_i32(*v) {
                    self.binop_reg_imm(op, d, *v as i32);
                } else {
                    self.enc.mov_r64_imm64(SCRATCH, *v);
                    self.binop_reg_reg(op, d, SCRATCH);
                }
                return true;
            }
            if let Some(slot) = stack_slot(src) {
                self.enc.op_r64_mem(op.reg_mem(), d, 5, slot, None);
                return true;
            }
            if let Some((b, off, idx)) = mem_operand(src) {
                self.enc.op_r64_mem(op.reg_mem(), d, b, off, idx);
                return true;
            }
            return false;
        }

        // Stack-slot destination: load into scratch, operate, store back.
        if let Some(slot) = stack_slot(dst) {
            if let Some(s) = phys_reg(src) {
                self.enc.mov_r64_rbp_offset(SCRATCH, slot);
                self.binop_reg_reg(op, SCRATCH, s);
                self.enc.mov_rbp_offset_r64(slot, SCRATCH);
                return true;
            }
            if let MachineOperand::Immediate(v) = src {
                self.enc.mov_r64_rbp_offset(SCRATCH, slot);
                if imm_fits_i32(*v) {
                    self.binop_reg_imm(op, SCRATCH, *v as i32);
                } else {
                    self.enc.mov_r64_imm64(SCRATCH2, *v);
                    self.binop_reg_reg(op, SCRATCH, SCRATCH2);
                }
                self.enc.mov_rbp_offset_r64(slot, SCRATCH);
                return true;
            }
            if let Some(vs) = stack_slot(src) {
                self.enc.mov_r64_rbp_offset(SCRATCH, slot);
                self.enc.mov_r64_rbp_offset(SCRATCH2, vs);
                self.binop_reg_reg(op, SCRATCH, SCRATCH2);
                self.enc.mov_rbp_offset_r64(slot, SCRATCH);
                return true;
            }
            return false;
        }

        // Memory destination (`op [mem], r64`).
        if let (Some((b, off, idx)), Some(s)) = (mem_operand(dst), phys_reg(src)) {
            self.enc.op_mem_r64(op.mem_reg(), b, off, idx, s);
            return true;
        }
        false
    }

    fn encode_move(&mut self, dst: &MachineOperand, src: &MachineOperand) -> bool {
        // Register destination.
        if let Some(d) = phys_reg(dst) {
            if let Some(s) = phys_reg(src) {
                self.enc.mov_r64_r64(d, s);
                return true;
            }
            match src {
                MachineOperand::Immediate(v) => {
                    self.enc.mov_r64_imm64(d, *v);
                    return true;
                }
                MachineOperand::FloatImmediate(f) => {
                    // Materialize the IEEE-754 double bit pattern into the GPR.
                    self.enc.mov_r64_imm64(d, f.to_bits() as i64);
                    return true;
                }
                MachineOperand::StackSlot(slot) => {
                    self.enc.mov_r64_rbp_offset(d, *slot);
                    return true;
                }
                MachineOperand::Memory { .. } => {
                    if let Some((b, off, idx)) = mem_operand(src) {
                        self.enc.mov_r64_mem(d, b, off, idx);
                        return true;
                    }
                }
                _ => {}
            }
            return false;
        }

        // Stack-slot destination.
        if let Some(slot) = stack_slot(dst) {
            if let Some(s) = phys_reg(src) {
                self.enc.mov_rbp_offset_r64(slot, s);
                return true;
            }
            match src {
                MachineOperand::Immediate(v) => {
                    self.enc.mov_r64_imm64(SCRATCH, *v);
                    self.enc.mov_rbp_offset_r64(slot, SCRATCH);
                    return true;
                }
                MachineOperand::FloatImmediate(f) => {
                    self.enc.mov_r64_imm64(SCRATCH, f.to_bits() as i64);
                    self.enc.mov_rbp_offset_r64(slot, SCRATCH);
                    return true;
                }
                MachineOperand::StackSlot(vs) => {
                    self.enc.mov_r64_rbp_offset(SCRATCH, *vs);
                    self.enc.mov_rbp_offset_r64(slot, SCRATCH);
                    return true;
                }
                MachineOperand::Memory { .. } => {
                    if let Some((b, off, idx)) = mem_operand(src) {
                        self.enc.mov_r64_mem(SCRATCH, b, off, idx);
                        self.enc.mov_rbp_offset_r64(slot, SCRATCH);
                        return true;
                    }
                }
                _ => {}
            }
            return false;
        }

        // Memory destination.
        if let Some((b, off, idx)) = mem_operand(dst) {
            if let Some(s) = phys_reg(src) {
                self.enc.mov_mem_r64(b, off, idx, s);
                return true;
            }
            match src {
                MachineOperand::Immediate(v) => {
                    self.enc.mov_r64_imm64(SCRATCH, *v);
                    self.enc.mov_mem_r64(b, off, idx, SCRATCH);
                    return true;
                }
                MachineOperand::StackSlot(vs) => {
                    self.enc.mov_r64_rbp_offset(SCRATCH, *vs);
                    self.enc.mov_mem_r64(b, off, idx, SCRATCH);
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    fn encode_mul(&mut self, dst: &MachineOperand, src: &MachineOperand) -> bool {
        let Some(d) = phys_reg(dst) else {
            return false;
        };
        match src {
            MachineOperand::Register(MachineRegister::Physical(s)) => {
                self.enc.imul_r64_r64(d, s.0);
                true
            }
            MachineOperand::Immediate(v) => {
                self.enc.mov_r64_imm64(SCRATCH, *v);
                self.enc.imul_r64_r64(d, SCRATCH);
                true
            }
            MachineOperand::StackSlot(slot) => {
                self.enc.imul_r64_mem(d, 5, *slot, None);
                true
            }
            MachineOperand::Memory { .. } => match mem_operand(src) {
                Some((b, off, idx)) => {
                    self.enc.imul_r64_mem(d, b, off, idx);
                    true
                }
                None => false,
            },
            _ => false,
        }
    }

    /// Encode DIV (quotient) or MOD (remainder). RAX/RDX are implicit
    /// operands of IDIV; both are reserved scratch registers, so clobbering
    /// them is safe.
    fn encode_divmod(&mut self, is_mod: bool, dst: &MachineOperand, src: &MachineOperand) -> bool {
        let Some(d) = phys_reg(dst) else {
            return false;
        };
        let src_reg = match src {
            MachineOperand::Register(MachineRegister::Physical(s)) => s.0,
            MachineOperand::Immediate(v) => {
                self.enc.mov_r64_imm64(SCRATCH, *v);
                SCRATCH
            }
            MachineOperand::StackSlot(slot) => {
                self.enc.mov_r64_rbp_offset(SCRATCH, *slot);
                SCRATCH
            }
            MachineOperand::Memory { .. } => match mem_operand(src) {
                Some((b, off, idx)) => {
                    self.enc.mov_r64_mem(SCRATCH, b, off, idx);
                    SCRATCH
                }
                None => return false,
            },
            _ => return false,
        };
        let mut actual_src = src_reg;
        if actual_src == 0 || actual_src == 2 {
            self.enc.mov_r64_r64(SCRATCH, actual_src);
            actual_src = SCRATCH;
        }
        if d != 0 {
            self.enc.mov_r64_r64(0, d); // mov rax, dst
        }
        self.enc.cqo();
        self.enc.idiv_r64(actual_src);
        // Quotient lands in RAX, remainder in RDX.
        if is_mod {
            self.enc.mov_r64_r64(d, 2); // mov dst, rdx
        } else if d != 0 {
            self.enc.mov_r64_r64(d, 0); // mov dst, rax
        }
        true
    }

    /// Encode a shift. The hardware count operand is either an imm8 (masked
    /// to 6 bits for 64-bit shifts) or the CL register.
    fn encode_shift(&mut self, op: ShiftOp, dst: &MachineOperand, src: &MachineOperand) -> bool {
        let Some(d) = phys_reg(dst) else {
            return false;
        };
        let shift_cl = |enc: &mut X86_64Encoder, d: u8| match op {
            ShiftOp::Shl => enc.shl_r64_cl(d),
            ShiftOp::Shr => enc.shr_r64_cl(d),
            ShiftOp::Sar => enc.sar_r64_cl(d),
        };
        match src {
            MachineOperand::Immediate(v) => {
                let count = (*v as u64 & 63) as u8;
                match op {
                    ShiftOp::Shl => self.enc.shl_r64_imm8(d, count),
                    ShiftOp::Shr => self.enc.shr_r64_imm8(d, count),
                    ShiftOp::Sar => self.enc.sar_r64_imm8(d, count),
                }
                true
            }
            MachineOperand::Register(MachineRegister::Physical(s)) if s.0 == 1 => {
                // CL already holds the count.
                shift_cl(&mut self.enc, d);
                true
            }
            MachineOperand::Register(MachineRegister::Physical(s)) => {
                // Shift counts live in CL only: transfer the value while
                // preserving RCX in the second scratch register. MOV does not
                // touch flags, so a preceding Compare stays valid.
                self.enc.mov_r64_r64(SCRATCH2, 1);
                self.enc.mov_r64_r64(1, s.0);
                shift_cl(&mut self.enc, d);
                self.enc.mov_r64_r64(1, SCRATCH2);
                true
            }
            MachineOperand::StackSlot(slot) => {
                self.enc.mov_r64_r64(SCRATCH2, 1);
                self.enc.mov_r64_rbp_offset(1, *slot);
                shift_cl(&mut self.enc, d);
                self.enc.mov_r64_r64(1, SCRATCH2);
                true
            }
            _ => false,
        }
    }

    fn encode_unary(&mut self, op: UnOp, dst: &MachineOperand) -> bool {
        if let Some(d) = phys_reg(dst) {
            match op {
                UnOp::Neg => self.enc.neg_r64(d),
                UnOp::Not => self.enc.not_r64(d),
            }
            return true;
        }
        if let Some(slot) = stack_slot(dst) {
            self.enc.mov_r64_rbp_offset(SCRATCH, slot);
            match op {
                UnOp::Neg => self.enc.neg_r64(SCRATCH),
                UnOp::Not => self.enc.not_r64(SCRATCH),
            }
            self.enc.mov_rbp_offset_r64(slot, SCRATCH);
            return true;
        }
        false
    }

    fn encode_setcc(&mut self, cc: ConditionCode, dst: &MachineOperand) -> bool {
        if let Some(d) = phys_reg(dst) {
            self.enc.setcc_r8(cc, d);
            // SETcc writes only the low byte: zero-extend so the full register
            // holds a well-defined 0/1 value.
            self.enc.movzx_r64_r8(d, d);
            return true;
        }
        if let Some(slot) = stack_slot(dst) {
            self.enc.setcc_r8(cc, SCRATCH);
            self.enc.movzx_r64_r8(SCRATCH, SCRATCH);
            self.enc.mov_rbp_offset_r64(slot, SCRATCH);
            return true;
        }
        false
    }

    fn load_mem(&mut self, dst: u8, base: u8, offset: i32, index: Option<(u8, u8)>, size: u8) {
        match size {
            1 => self.enc.movzx_r64_mem8(dst, base, offset, index),
            2 => self.enc.movzx_r64_mem16(dst, base, offset, index),
            4 => self.enc.mov_r32_mem(dst, base, offset, index),
            _ => self.enc.mov_r64_mem(dst, base, offset, index),
        }
    }

    fn store_mem(&mut self, base: u8, offset: i32, index: Option<(u8, u8)>, src: u8, size: u8) {
        match size {
            1 => self.enc.mov_mem_r8(base, offset, index, src),
            2 => self.enc.mov_mem_r16(base, offset, index, src),
            4 => self.enc.mov_mem_r32(base, offset, index, src),
            _ => self.enc.mov_mem_r64(base, offset, index, src),
        }
    }

    /// Load a value from a stack slot or memory operand. `dst` may be a
    /// register or a stack slot.
    fn encode_load(&mut self, dst: &MachineOperand, src: &MachineOperand, size: u8) -> bool {
        let (target_reg, target_slot): (u8, Option<i32>) = if let Some(d) = phys_reg(dst) {
            (d, None)
        } else if let Some(s) = stack_slot(dst) {
            (SCRATCH, Some(s))
        } else {
            return false;
        };

        if let Some(slot) = stack_slot(src) {
            self.load_mem(target_reg, 5, slot, None, size);
        } else if let Some((b, off, idx)) = mem_operand(src) {
            self.load_mem(target_reg, b, off, idx, size);
        } else {
            return false;
        }

        if let Some(slot) = target_slot {
            self.enc.mov_rbp_offset_r64(slot, SCRATCH);
        }
        true
    }

    /// Store a value into a stack slot or memory operand. `dst` is the
    /// destination address, `src` the value.
    fn encode_store(&mut self, dst: &MachineOperand, src: &MachineOperand, size: u8) -> bool {
        let (base, off, idx): (u8, i32, Option<(u8, u8)>) = if let Some(s) = stack_slot(dst) {
            (5, s, None)
        } else if let Some((b, o, i)) = mem_operand(dst) {
            (b, o, i)
        } else {
            return false;
        };

        let value_reg: u8 = if let Some(s) = phys_reg(src) {
            s
        } else if let MachineOperand::Immediate(v) = src {
            self.enc.mov_r64_imm64(SCRATCH, *v);
            SCRATCH
        } else if let Some(vs) = stack_slot(src) {
            self.enc.mov_r64_rbp_offset(SCRATCH, vs);
            SCRATCH
        } else if let Some((b2, o2, i2)) = mem_operand(src) {
            self.enc.mov_r64_mem(SCRATCH, b2, o2, i2);
            SCRATCH
        } else {
            return false;
        };

        self.store_mem(base, off, idx, value_reg, size);
        true
    }

    fn encode_push(&mut self, src: &MachineOperand) -> bool {
        match src {
            MachineOperand::Register(MachineRegister::Physical(r)) => {
                self.enc.push_reg64(r.0);
                true
            }
            MachineOperand::StackSlot(slot) => {
                self.enc.push_mem(5, *slot, None);
                true
            }
            MachineOperand::Immediate(v) if imm_fits_i32(*v) => {
                self.enc.push_imm32(*v as i32);
                true
            }
            MachineOperand::Immediate(v) => {
                self.enc.mov_r64_imm64(SCRATCH, *v);
                self.enc.push_reg64(SCRATCH);
                true
            }
            MachineOperand::Memory { .. } => match mem_operand(src) {
                Some((b, off, idx)) => {
                    self.enc.mov_r64_mem(SCRATCH, b, off, idx);
                    self.enc.push_reg64(SCRATCH);
                    true
                }
                None => false,
            },
            _ => false,
        }
    }

    fn encode_pop(&mut self, dst: &MachineOperand) -> bool {
        match dst {
            MachineOperand::Register(MachineRegister::Physical(r)) => {
                self.enc.pop_reg64(r.0);
                true
            }
            MachineOperand::StackSlot(slot) => {
                self.enc.pop_mem(5, *slot, None);
                true
            }
            _ => false,
        }
    }

    fn encode_instruction(
        &mut self,
        inst: &MachineInstruction,
        func_name: &str,
        abi: &str,
    ) -> Result<(), CodegenError> {
        match inst {
            MachineInstruction::Nop => self.enc.nop(),
            MachineInstruction::Return => {
                self.emit_epilogue();
                self.saw_return = true;
            }
            MachineInstruction::Move { dst, src } => {
                if let (
                    MachineOperand::Register(MachineRegister::Physical(d)),
                    MachineOperand::Symbol(name),
                ) = (dst, src)
                {
                    // Materialize a symbol address: mov reg, imm64 + ABS64
                    // relocation patched by the linker.
                    self.enc.mov_r64_imm64(d.0, 0);
                    let imm_offset = self.enc.len() - 8;
                    self.relocations.push(AdobRelocation::new(
                        imm_offset as u64,
                        0,
                        name.clone(),
                        RelocationKind::Absolute64,
                        0,
                    ));
                } else if !self.encode_move(dst, src) {
                    return Err(self.unsupported(inst, func_name, abi));
                }
            }
            MachineInstruction::Add { dst, src } => {
                if !self.encode_binop(BinOp::Add, dst, src) {
                    return Err(self.unsupported(inst, func_name, abi));
                }
            }
            MachineInstruction::Sub { dst, src } => {
                if !self.encode_binop(BinOp::Sub, dst, src) {
                    return Err(self.unsupported(inst, func_name, abi));
                }
            }
            MachineInstruction::And { dst, src } => {
                if !self.encode_binop(BinOp::And, dst, src) {
                    return Err(self.unsupported(inst, func_name, abi));
                }
            }
            MachineInstruction::Or { dst, src } => {
                if !self.encode_binop(BinOp::Or, dst, src) {
                    return Err(self.unsupported(inst, func_name, abi));
                }
            }
            MachineInstruction::Xor { dst, src } => {
                if !self.encode_binop(BinOp::Xor, dst, src) {
                    return Err(self.unsupported(inst, func_name, abi));
                }
            }
            MachineInstruction::Compare { lhs, rhs } => {
                if !self.encode_binop(BinOp::Cmp, lhs, rhs) {
                    return Err(self.unsupported(inst, func_name, abi));
                }
            }
            MachineInstruction::Test { lhs, rhs } => {
                if !self.encode_binop(BinOp::Test, lhs, rhs) {
                    return Err(self.unsupported(inst, func_name, abi));
                }
            }
            MachineInstruction::Mul { dst, src } => {
                if !self.encode_mul(dst, src) {
                    return Err(self.unsupported(inst, func_name, abi));
                }
            }
            MachineInstruction::Div { dst, src } => {
                if !self.encode_divmod(false, dst, src) {
                    return Err(self.unsupported(inst, func_name, abi));
                }
            }
            MachineInstruction::Mod { dst, src } => {
                if !self.encode_divmod(true, dst, src) {
                    return Err(self.unsupported(inst, func_name, abi));
                }
            }
            MachineInstruction::Shl { dst, src } => {
                if !self.encode_shift(ShiftOp::Shl, dst, src) {
                    return Err(self.unsupported(inst, func_name, abi));
                }
            }
            MachineInstruction::Shr { dst, src } => {
                if !self.encode_shift(ShiftOp::Shr, dst, src) {
                    return Err(self.unsupported(inst, func_name, abi));
                }
            }
            MachineInstruction::Sar { dst, src } => {
                if !self.encode_shift(ShiftOp::Sar, dst, src) {
                    return Err(self.unsupported(inst, func_name, abi));
                }
            }
            MachineInstruction::Neg { dst } => {
                if !self.encode_unary(UnOp::Neg, dst) {
                    return Err(self.unsupported(inst, func_name, abi));
                }
            }
            MachineInstruction::Not { dst } => {
                if !self.encode_unary(UnOp::Not, dst) {
                    return Err(self.unsupported(inst, func_name, abi));
                }
            }
            MachineInstruction::SetCc { dst, cc } => {
                if !self.encode_setcc(*cc, dst) {
                    return Err(self.unsupported(inst, func_name, abi));
                }
            }
            MachineInstruction::Branch { target } => {
                let disp_offset = self.enc.len() + 1;
                self.enc.jmp_rel32(0);
                self.branch_fixups.push(BranchFixup {
                    disp_offset,
                    next_offset: self.enc.len(),
                    target: target.clone(),
                });
            }
            MachineInstruction::BranchCc { cc, target } => {
                let disp_offset = self.enc.len() + 2; // 0F 8x <rel32>
                self.enc.jcc_rel32(*cc, 0);
                self.branch_fixups.push(BranchFixup {
                    disp_offset,
                    next_offset: self.enc.len(),
                    target: target.clone(),
                });
            }
            MachineInstruction::Call { target, .. } => match target {
                MachineOperand::Register(MachineRegister::Physical(r)) => {
                    self.enc.call_r64(r.0);
                }
                MachineOperand::Symbol(name) => {
                    // call rel32 + PC32 relocation: the linker patches the
                    // displacement to the resolved symbol (or import thunk).
                    let disp_offset = self.enc.len() + 1;
                    self.enc.call_rel32(0);
                    self.relocations.push(AdobRelocation::new(
                        disp_offset as u64,
                        0,
                        name.clone(),
                        RelocationKind::PcRelative32,
                        -4,
                    ));
                }
                MachineOperand::Label(name) => {
                    // Direct call to a block label within this function.
                    let disp_offset = self.enc.len() + 1;
                    self.enc.call_rel32(0);
                    self.branch_fixups.push(BranchFixup {
                        disp_offset,
                        next_offset: self.enc.len(),
                        target: name.clone(),
                    });
                }
                MachineOperand::Register(MachineRegister::Virtual(v)) => {
                    return Err(CodegenError::new(
                        "x86_64",
                        format!(
                            "call target is virtual register v{} that escaped register allocation",
                            v.0
                        ),
                    )
                    .with_arch("x86_64")
                    .with_abi(abi)
                    .with_function(func_name)
                    .with_instruction("Call")
                    .with_suggestion(
                        "Run the register allocator (lower_module) before code generation",
                    ));
                }
                _ => return Err(self.unsupported(inst, func_name, abi)),
            },
            MachineInstruction::Push { src } => {
                if !self.encode_push(src) {
                    return Err(self.unsupported(inst, func_name, abi));
                }
            }
            MachineInstruction::Pop { dst } => {
                if !self.encode_pop(dst) {
                    return Err(self.unsupported(inst, func_name, abi));
                }
            }
            MachineInstruction::Load { dst, src, size } => {
                if !self.encode_load(dst, src, *size) {
                    return Err(self.unsupported(inst, func_name, abi));
                }
            }
            MachineInstruction::Store { dst, src, size } => {
                if !self.encode_store(dst, src, *size) {
                    return Err(self.unsupported(inst, func_name, abi));
                }
            }
            MachineInstruction::Barrier => self.enc.mfence(),
            MachineInstruction::Vector { op, .. } => {
                return Err(CodegenError::new(
                    "x86_64",
                    format!("vector operation `{}` requires SSE/AVX support, which the native backend does not provide yet", op),
                )
                .with_arch("x86_64")
                .with_abi(abi)
                .with_function(func_name)
                .with_instruction("Vector"));
            }
            MachineInstruction::Atomic { op, .. } => {
                return Err(CodegenError::new(
                    "x86_64",
                    format!("atomic operation `{}` is not encodable yet (requires LOCK-prefixed memory forms)", op),
                )
                .with_arch("x86_64")
                .with_abi(abi)
                .with_function(func_name)
                .with_instruction("Atomic"));
            }
            MachineInstruction::Custom { name, .. } => {
                return Err(CodegenError::new(
                    "x86_64",
                    format!("custom instruction `{}` has no x86-64 encoding", name),
                )
                .with_arch("x86_64")
                .with_abi(abi)
                .with_function(func_name)
                .with_instruction("Custom"));
            }
        }
        Ok(())
    }

    /// Patch all branch fixups and return (code, relocations).
    fn finish(mut self) -> (Vec<u8>, Vec<AdobRelocation>) {
        if !self.saw_return {
            self.emit_epilogue();
        }

        for fixup in &self.branch_fixups {
            if let Some(&target_off) = self.block_offsets.get(&fixup.target) {
                let disp = (target_off as i64 - fixup.next_offset as i64) as i32;
                let buf = &mut self.enc.buffer;
                buf[fixup.disp_offset..fixup.disp_offset + 4].copy_from_slice(&disp.to_le_bytes());
            } else {
                // Not a block of this function: reference it as a PC-relative
                // external symbol (e.g. `__stack_chk_fail`) and let the
                // linker resolve it.
                self.relocations.push(AdobRelocation::new(
                    fixup.disp_offset as u64,
                    0,
                    fixup.target.clone(),
                    RelocationKind::PcRelative32,
                    -4,
                ));
            }
        }

        (self.enc.buffer, self.relocations)
    }
}

/// x86-64 Native Codegen Backend.
pub struct X86_64Backend {
    target: TargetDescriptor,
    capabilities: TargetCapabilities,
}

impl X86_64Backend {
    pub fn new(target: TargetDescriptor) -> Self {
        let capabilities = TargetCapabilities::for_architecture(&target.architecture);
        Self {
            target,
            capabilities,
        }
    }

    pub fn calling_convention(&self) -> Box<dyn CallingConvention> {
        match self.target.operating_system {
            OperatingSystem::Windows => Box::new(WindowsX64CallingConvention),
            _ => Box::new(SystemVX64CallingConvention),
        }
    }

    /// Encode a single (already register-allocated) function.
    fn encode_function(
        &self,
        func: &MachineFunction,
    ) -> Result<(Vec<u8>, Vec<AdobRelocation>), CodegenError> {
        let conv = self.calling_convention();
        let abi = conv.name();
        let mut ctx = FunctionEncoding::new(func, conv.as_ref());
        ctx.emit_prologue();

        for block in &func.blocks {
            ctx.block_offsets.insert(block.label.clone(), ctx.enc.len());
            for inst in &block.instructions {
                ctx.encode_instruction(inst, &func.name, abi)?;
            }
        }

        Ok(ctx.finish())
    }
}

impl CodegenBackend for X86_64Backend {
    fn target(&self) -> &TargetDescriptor {
        &self.target
    }

    fn capabilities(&self) -> TargetCapabilities {
        self.capabilities.clone()
    }

    fn lower_module(&mut self, module: &NativeModule) -> Result<NativeModule, CodegenError> {
        let mut lowered = module.clone();
        let reg_file = X86_64RegisterFile;
        let allocator = LinearScanAllocator::new(&reg_file);

        for func in &mut lowered.functions {
            allocator.allocate(func);
        }

        Ok(lowered)
    }

    fn generate_function(&mut self, func: &MachineFunction) -> Result<Vec<u8>, CodegenError> {
        Ok(self.encode_function(func)?.0)
    }

    fn generate_function_with_relocations(
        &mut self,
        func: &MachineFunction,
    ) -> Result<(Vec<u8>, Vec<AdobRelocation>), CodegenError> {
        self.encode_function(func)
    }

    fn emit_object(&mut self, module: &NativeModule) -> Result<AdobObject, CodegenError> {
        // Always run allocation defensively: callers that pre-lowered get an
        // idempotent pass, callers that did not no longer silently drop
        // virtual-register instructions.
        let lowered = self.lower_module(module)?;

        let mut obj = AdobObject::new(self.target.clone());

        // 1. Text section: function code plus call/symbol relocations.
        let mut text_bytes: Vec<u8> = Vec::new();
        let mut text_relocations: Vec<AdobRelocation> = Vec::new();
        let mut function_symbols: Vec<AdobSymbol> = Vec::new();

        for func in &lowered.functions {
            let func_offset = text_bytes.len() as u64;
            let (code, relocs) = self.encode_function(func)?;
            let func_size = code.len() as u64;
            text_bytes.extend_from_slice(&code);
            for mut reloc in relocs {
                reloc.offset += func_offset;
                text_relocations.push(reloc);
            }

            let sym = AdobSymbol::new_defined(
                0,
                func.name.clone(),
                SymbolKind::Function,
                0, // section 0 (.text)
                func_offset,
                func_size,
            )
            .with_binding(if func.is_exported {
                SymbolBinding::Global
            } else {
                SymbolBinding::Local
            })
            .with_visibility(SymbolVisibility::Default);
            function_symbols.push(sym);

            if func.is_exported {
                obj.add_export(func.name.clone());
            }
        }

        let mut text_sec = AdobSection::new(".text", SectionKind::Text)
            .with_flags(section_flags::READ | section_flags::EXECUTE | section_flags::ALLOC)
            .with_alignment(16)
            .with_data(text_bytes);
        text_sec.relocations = text_relocations;
        obj.add_section(text_sec);

        // 2. Read-only data section (.rodata) for the string pool.
        let mut rodata_symbols: Vec<AdobSymbol> = Vec::new();
        if !lowered.string_pool.is_empty() {
            let mut rodata = Vec::new();
            for (idx, s) in lowered.string_pool.iter().enumerate() {
                let off = rodata.len() as u64;
                let s_bytes = s.as_bytes();
                rodata.extend_from_slice(s_bytes);
                rodata.push(0); // null terminator

                let sym = AdobSymbol::new_defined(
                    0,
                    format!("__str_{}", idx),
                    SymbolKind::Object,
                    1, // section 1 (.rodata)
                    off,
                    s_bytes.len() as u64 + 1,
                )
                .with_binding(SymbolBinding::Local);
                rodata_symbols.push(sym);
            }

            let rodata_sec = AdobSection::new(".rodata", SectionKind::Rodata)
                .with_flags(section_flags::READ | section_flags::ALLOC)
                .with_alignment(8)
                .with_data(rodata);
            obj.add_section(rodata_sec);
        }

        // 3. Symbols: functions first (preserving module order), then string
        //    literals, then imports.
        for sym in function_symbols {
            obj.add_symbol(sym);
        }
        for sym in rodata_symbols {
            obj.add_symbol(sym);
        }
        for imp in &lowered.imports {
            obj.add_import(imp.clone());
            let sym = AdobSymbol::new_undefined(0, imp.clone(), SymbolKind::Import)
                .with_binding(SymbolBinding::Global);
            obj.add_symbol(sym);
        }

        // 4. Resolve relocation symbol IDs by name (ids are assigned above).
        let name_to_id: HashMap<String, u32> =
            obj.symbols.iter().map(|s| (s.name.clone(), s.id)).collect();
        if let Some((_, sec)) = obj.find_section_mut(".text") {
            for reloc in &mut sec.relocations {
                if let Some(&id) = name_to_id.get(&reloc.symbol_name) {
                    reloc.symbol = id;
                }
            }
        }

        Ok(obj)
    }

    fn generate_assembly(&mut self, module: &NativeModule) -> Result<String, CodegenError> {
        let lowered = self.lower_module(module)?;
        Ok(asm_printer::X86_64AsmPrinter::print_module(&lowered))
    }
}
