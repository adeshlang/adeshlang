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
    CallingConvention, ParallelMoveResolver, SystemVX64CallingConvention,
    WindowsX64CallingConvention,
};
use crate::error::CodegenError;
use crate::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, RegisterClass,
};
use crate::opt::{OptLevel, OptimizationPipeline};
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
/// Encoder scratch register for floating-point operations (XMM15 = 31).
const FP_SCRATCH: u8 = 15;
/// Second encoder scratch register for floating-point operations (XMM14 = 30).
const FP_SCRATCH2: u8 = 14;

// ---------------------------------------------------------------- Register File

/// x86-64 Physical Register File definition.
///
/// RAX (0), R10, and R11 are reserved for the encoder and never allocated:
/// RAX is the implicit DIV/IDIV operand and the return-value register,
/// R10 backs stack-slot arithmetic sequences, and R11 preserves RCX around
/// shift-count transfers.
///
/// XMM14 (30) and XMM15 (31) are reserved for encoder floating-point scratch operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct X86_64RegisterFile {
    pub is_windows: bool,
}

impl Default for X86_64RegisterFile {
    fn default() -> Self {
        Self::sysv()
    }
}

impl X86_64RegisterFile {
    pub const fn sysv() -> Self {
        Self { is_windows: false }
    }

    pub const fn windows() -> Self {
        Self { is_windows: true }
    }

    pub fn for_os(os: OperatingSystem) -> Self {
        match os {
            OperatingSystem::Windows => Self::windows(),
            _ => Self::sysv(),
        }
    }
}

// 0: RAX, 1: RCX, 2: RDX, 3: RBX, 4: RSP, 5: RBP, 6: RSI, 7: RDI
// 8: R8, 9: R9, 10: R10, 11: R11, 12: R12, 13: R13, 14: R14, 15: R15
// 16: XMM0 .. 31: XMM15
const X86_64_ALL_REGS: [PhysicalRegister; 32] = [
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
    PhysicalRegister(16),
    PhysicalRegister(17),
    PhysicalRegister(18),
    PhysicalRegister(19),
    PhysicalRegister(20),
    PhysicalRegister(21),
    PhysicalRegister(22),
    PhysicalRegister(23),
    PhysicalRegister(24),
    PhysicalRegister(25),
    PhysicalRegister(26),
    PhysicalRegister(27),
    PhysicalRegister(28),
    PhysicalRegister(29),
    PhysicalRegister(30),
    PhysicalRegister(31),
];

const X86_64_ALLOCATABLE_GPR: [PhysicalRegister; 11] = [
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

const X86_64_CALLER_SAVED_GPR: [PhysicalRegister; 6] = [
    PhysicalRegister(1), // RCX
    PhysicalRegister(2), // RDX
    PhysicalRegister(6), // RSI
    PhysicalRegister(7), // RDI
    PhysicalRegister(8), // R8
    PhysicalRegister(9), // R9
];

const X86_64_CALLEE_SAVED_GPR: [PhysicalRegister; 5] = [
    PhysicalRegister(3),  // RBX
    PhysicalRegister(12), // R12
    PhysicalRegister(13), // R13
    PhysicalRegister(14), // R14
    PhysicalRegister(15), // R15
];

const X86_64_RESERVED_GPR: [PhysicalRegister; 5] = [
    PhysicalRegister(0),  // RAX: DIV/IDIV implicit operand, return value
    PhysicalRegister(4),  // RSP: stack pointer
    PhysicalRegister(5),  // RBP: frame pointer
    PhysicalRegister(10), // R10: encoder scratch
    PhysicalRegister(11), // R11: encoder scratch (RCX preservation)
];

const X86_64_ALLOCATABLE_FP: [PhysicalRegister; 14] = [
    PhysicalRegister(16), // XMM0
    PhysicalRegister(17), // XMM1
    PhysicalRegister(18), // XMM2
    PhysicalRegister(19), // XMM3
    PhysicalRegister(20), // XMM4
    PhysicalRegister(21), // XMM5
    PhysicalRegister(22), // XMM6
    PhysicalRegister(23), // XMM7
    PhysicalRegister(24), // XMM8
    PhysicalRegister(25), // XMM9
    PhysicalRegister(26), // XMM10
    PhysicalRegister(27), // XMM11
    PhysicalRegister(28), // XMM12
    PhysicalRegister(29), // XMM13
];

const X86_64_CALLER_SAVED_FP_SYSV: [PhysicalRegister; 14] = [
    PhysicalRegister(16), // XMM0
    PhysicalRegister(17), // XMM1
    PhysicalRegister(18), // XMM2
    PhysicalRegister(19), // XMM3
    PhysicalRegister(20), // XMM4
    PhysicalRegister(21), // XMM5
    PhysicalRegister(22), // XMM6
    PhysicalRegister(23), // XMM7
    PhysicalRegister(24), // XMM8
    PhysicalRegister(25), // XMM9
    PhysicalRegister(26), // XMM10
    PhysicalRegister(27), // XMM11
    PhysicalRegister(28), // XMM12
    PhysicalRegister(29), // XMM13
];

const X86_64_CALLEE_SAVED_FP_SYSV: [PhysicalRegister; 0] = [];

const X86_64_CALLER_SAVED_FP_WIN: [PhysicalRegister; 6] = [
    PhysicalRegister(16), // XMM0
    PhysicalRegister(17), // XMM1
    PhysicalRegister(18), // XMM2
    PhysicalRegister(19), // XMM3
    PhysicalRegister(20), // XMM4
    PhysicalRegister(21), // XMM5
];

const X86_64_CALLEE_SAVED_FP_WIN: [PhysicalRegister; 8] = [
    PhysicalRegister(22), // XMM6
    PhysicalRegister(23), // XMM7
    PhysicalRegister(24), // XMM8
    PhysicalRegister(25), // XMM9
    PhysicalRegister(26), // XMM10
    PhysicalRegister(27), // XMM11
    PhysicalRegister(28), // XMM12
    PhysicalRegister(29), // XMM13
];

const X86_64_RESERVED_FP: [PhysicalRegister; 2] = [
    PhysicalRegister(30), // XMM14: FP scratch 2
    PhysicalRegister(31), // XMM15: FP scratch
];

impl RegisterFile for X86_64RegisterFile {
    fn registers(&self) -> &[PhysicalRegister] {
        &X86_64_ALL_REGS
    }
    fn allocatable(&self) -> &[PhysicalRegister] {
        &X86_64_ALLOCATABLE_GPR
    }
    fn caller_saved(&self) -> &[PhysicalRegister] {
        &X86_64_CALLER_SAVED_GPR
    }
    fn callee_saved(&self) -> &[PhysicalRegister] {
        &X86_64_CALLEE_SAVED_GPR
    }
    fn reserved(&self) -> &[PhysicalRegister] {
        &X86_64_RESERVED_GPR
    }
    fn allocatable_for_class(&self, class: RegisterClass) -> &[PhysicalRegister] {
        match class {
            RegisterClass::Gpr => &X86_64_ALLOCATABLE_GPR,
            RegisterClass::Float => &X86_64_ALLOCATABLE_FP,
        }
    }
    fn caller_saved_for_class(&self, class: RegisterClass) -> &[PhysicalRegister] {
        match class {
            RegisterClass::Gpr => &X86_64_CALLER_SAVED_GPR,
            RegisterClass::Float => {
                if self.is_windows {
                    &X86_64_CALLER_SAVED_FP_WIN
                } else {
                    &X86_64_CALLER_SAVED_FP_SYSV
                }
            }
        }
    }
    fn callee_saved_for_class(&self, class: RegisterClass) -> &[PhysicalRegister] {
        match class {
            RegisterClass::Gpr => &X86_64_CALLEE_SAVED_GPR,
            RegisterClass::Float => {
                if self.is_windows {
                    &X86_64_CALLEE_SAVED_FP_WIN
                } else {
                    &X86_64_CALLEE_SAVED_FP_SYSV
                }
            }
        }
    }
    fn reserved_for_class(&self, class: RegisterClass) -> &[PhysicalRegister] {
        match class {
            RegisterClass::Gpr => &X86_64_RESERVED_GPR,
            RegisterClass::Float => &X86_64_RESERVED_FP,
        }
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
        MachineInstruction::VectorAdd { .. } => "VectorAdd",
        MachineInstruction::VectorSub { .. } => "VectorSub",
        MachineInstruction::VectorMul { .. } => "VectorMul",
        MachineInstruction::VectorDiv { .. } => "VectorDiv",
        MachineInstruction::VectorAnd { .. } => "VectorAnd",
        MachineInstruction::VectorOr { .. } => "VectorOr",
        MachineInstruction::VectorXor { .. } => "VectorXor",
        MachineInstruction::VectorLoad { .. } => "VectorLoad",
        MachineInstruction::VectorStore { .. } => "VectorStore",
        MachineInstruction::VectorBroadcast { .. } => "VectorBroadcast",
        MachineInstruction::VectorShuffle { .. } => "VectorShuffle",
        MachineInstruction::VectorReduceAdd { .. } => "VectorReduceAdd",
        MachineInstruction::AtomicLoad { .. } => "AtomicLoad",
        MachineInstruction::AtomicStore { .. } => "AtomicStore",
        MachineInstruction::AtomicFetchAdd { .. } => "AtomicFetchAdd",
        MachineInstruction::AtomicCompareExchange { .. } => "AtomicCompareExchange",
        MachineInstruction::Barrier => "Barrier",
        MachineInstruction::ParallelMove { .. } => "ParallelMove",
        MachineInstruction::FAdd { .. } => "FAdd",
        MachineInstruction::FSub { .. } => "FSub",
        MachineInstruction::FMul { .. } => "FMul",
        MachineInstruction::FDiv { .. } => "FDiv",
        MachineInstruction::FNeg { .. } => "FNeg",
        MachineInstruction::FCmp { .. } => "FCmp",
        MachineInstruction::FCvtIntToFloat { .. } => "FCvtIntToFloat",
        MachineInstruction::FCvtFloatToInt { .. } => "FCvtFloatToInt",
        MachineInstruction::FCvtFloatToFloat { .. } => "FCvtFloatToFloat",
        MachineInstruction::Custom { .. } => "Custom",
    }
}

/// Append every physical register referenced (read or written, including
/// memory base/index registers) by `inst` to `out`.
///
/// This is the single source of truth for "which physical registers does this
/// instruction touch" and backs both the frame layout (callee-saved usage) and
/// the assembly printer.
fn collect_instruction_registers(inst: &MachineInstruction, out: &mut Vec<u8>) {
    fn add_op(op: &MachineOperand, out: &mut Vec<u8>) {
        match op {
            MachineOperand::Register(MachineRegister::Physical(p)) => out.push(p.0),
            MachineOperand::Memory { base, index, .. } => {
                if let MachineRegister::Physical(p) = base {
                    out.push(p.0);
                }
                if let Some((MachineRegister::Physical(p), _)) = index {
                    out.push(p.0);
                }
            }
            _ => {}
        }
    }

    fn add_location(loc: &crate::machine_ir::MoveLocation, out: &mut Vec<u8>) {
        match loc {
            crate::machine_ir::MoveLocation::PhysicalRegister(p) => out.push(p.0),
            crate::machine_ir::MoveLocation::StackSlot { base, .. } => out.push(base.0),
            crate::machine_ir::MoveLocation::Memory { base, index, .. } => {
                if let MachineRegister::Physical(p) = base {
                    out.push(p.0);
                }
                if let Some((MachineRegister::Physical(p), _)) = index {
                    out.push(p.0);
                }
            }
            _ => {}
        }
    }

    match inst {
        MachineInstruction::Nop
        | MachineInstruction::Return
        | MachineInstruction::Branch { .. }
        | MachineInstruction::BranchCc { .. }
        | MachineInstruction::Barrier => {}
        MachineInstruction::ParallelMove { moves } => {
            for m in moves {
                add_location(&m.dst, out);
                add_location(&m.src, out);
            }
        }
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
        | MachineInstruction::Sar { dst, src }
        | MachineInstruction::FAdd { dst, src, .. }
        | MachineInstruction::FSub { dst, src, .. }
        | MachineInstruction::FMul { dst, src, .. }
        | MachineInstruction::FDiv { dst, src, .. }
        | MachineInstruction::FCmp {
            lhs: dst, rhs: src, ..
        }
        | MachineInstruction::FCvtIntToFloat { dst, src, .. }
        | MachineInstruction::FCvtFloatToInt { dst, src, .. }
        | MachineInstruction::FCvtFloatToFloat { dst, src, .. }
        | MachineInstruction::VectorAdd { dst, src, .. }
        | MachineInstruction::VectorSub { dst, src, .. }
        | MachineInstruction::VectorMul { dst, src, .. }
        | MachineInstruction::VectorDiv { dst, src, .. }
        | MachineInstruction::VectorAnd { dst, src, .. }
        | MachineInstruction::VectorOr { dst, src, .. }
        | MachineInstruction::VectorXor { dst, src, .. }
        | MachineInstruction::VectorLoad { dst, src, .. }
        | MachineInstruction::VectorStore { dst, src, .. }
        | MachineInstruction::VectorBroadcast { dst, src, .. }
        | MachineInstruction::VectorShuffle { dst, src, .. }
        | MachineInstruction::VectorReduceAdd { dst, src, .. }
        | MachineInstruction::AtomicLoad { dst, src, .. }
        | MachineInstruction::AtomicStore { dst, src, .. }
        | MachineInstruction::AtomicFetchAdd { dst, src, .. } => {
            add_op(dst, out);
            add_op(src, out);
        }
        MachineInstruction::AtomicCompareExchange {
            dst,
            expected,
            desired,
            ..
        } => {
            add_op(dst, out);
            add_op(expected, out);
            add_op(desired, out);
        }
        MachineInstruction::Compare { lhs, rhs } | MachineInstruction::Test { lhs, rhs } => {
            add_op(lhs, out);
            add_op(rhs, out);
        }
        MachineInstruction::Neg { dst }
        | MachineInstruction::Not { dst }
        | MachineInstruction::FNeg { dst, .. }
        | MachineInstruction::SetCc { dst, .. }
        | MachineInstruction::Push { src: dst }
        | MachineInstruction::Pop { dst } => add_op(dst, out),
        MachineInstruction::Call { target, .. } => add_op(target, out),
        MachineInstruction::Custom { operands, .. } => {
            for op in operands {
                add_op(op, out);
            }
        }
    }
}

/// Every physical register touched by `func`, in a single pass over the
/// instruction stream.
fn collect_function_registers(func: &MachineFunction) -> std::collections::HashSet<u8> {
    let mut touched = Vec::new();
    for block in &func.blocks {
        for inst in &block.instructions {
            collect_instruction_registers(inst, &mut touched);
        }
    }
    touched.into_iter().collect()
}

/// Shared frame layout, computed identically for the binary encoder and the
/// assembly printer so the two paths cannot describe different frames.
pub struct FrameLayout {
    /// Callee-saved registers the function actually touches, ascending.
    pub used_callee_saved: Vec<u8>,
    /// Save-area offset from RBP for each used callee-saved register.
    pub callee_slots: HashMap<u8, i32>,
    /// Bytes of locals + register-spill area (8-byte aligned).
    pub locals_size: i32,
    /// Total frame allocation (16-byte aligned), including the save area.
    pub frame_size: i32,
}

/// Compute the frame layout for `func` under calling convention `conv`.
///
/// A single pass collects the physical registers the function touches; the
/// callee-saved subset determines the save area, which is laid out below the
/// locals/spill area at negative RBP offsets.
pub fn compute_frame_layout(func: &MachineFunction, conv: &dyn CallingConvention) -> FrameLayout {
    let touched = collect_function_registers(func);
    let mut used_callee_saved: Vec<u8> = conv
        .callee_saved_registers()
        .iter()
        .map(|r| r.0)
        .filter(|&r| r != 4 && r != 5)
        .filter(|r| touched.contains(r))
        .collect();
    used_callee_saved.sort_unstable();
    used_callee_saved.dedup();

    let locals_size = (func.stack_size as i32 + 7) & !7;
    let mut cur_offset = locals_size;
    let mut callee_slots = HashMap::new();
    for &r in &used_callee_saved {
        if r >= 16 {
            cur_offset = (cur_offset + 15) & !15;
            cur_offset += 16;
            callee_slots.insert(r, -cur_offset);
        } else {
            cur_offset += 8;
            callee_slots.insert(r, -cur_offset);
        }
    }
    let frame_size = (cur_offset + 15) & !15;

    FrameLayout {
        used_callee_saved,
        callee_slots,
        locals_size,
        frame_size,
    }
}

/// Whether `inst` leaves EFLAGS meaningful for a later condition test, either
/// by writing them or by consuming them without clearing them.
///
/// Used to keep the `xor r, r` zero idiom away from a pending condition test.
fn instruction_writes_flags(inst: &MachineInstruction) -> bool {
    matches!(
        inst,
        MachineInstruction::Add { .. }
            | MachineInstruction::Sub { .. }
            | MachineInstruction::Mul { .. }
            | MachineInstruction::Div { .. }
            | MachineInstruction::Mod { .. }
            | MachineInstruction::Neg { .. }
            | MachineInstruction::And { .. }
            | MachineInstruction::Or { .. }
            | MachineInstruction::Xor { .. }
            | MachineInstruction::Shl { .. }
            | MachineInstruction::Shr { .. }
            | MachineInstruction::Sar { .. }
            | MachineInstruction::Compare { .. }
            | MachineInstruction::Test { .. }
            | MachineInstruction::SetCc { .. }
            | MachineInstruction::FCmp { .. }
            | MachineInstruction::FCvtIntToFloat { .. }
            | MachineInstruction::FCvtFloatToInt { .. }
            | MachineInstruction::ParallelMove { .. }
    )
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

/// Scalar floating-point binary operation kind.
#[derive(Clone, Copy)]
enum FpOp {
    Add,
    Sub,
    Mul,
    Div,
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
    callee_slots: HashMap<u8, i32>,
    /// Bytes of locals + register-spill area (8-byte aligned).
    _locals_size: i32,
    /// Total frame allocation (16-byte aligned), including the callee-saved
    /// save area.
    frame_size: i32,
    saw_return: bool,
    /// True when the most recently emitted instruction wrote EFLAGS. Used to
    /// keep the `xor r, r` zero idiom away from a pending condition test.
    flags_dirty: bool,
    /// Set when the current instruction emitted the `xor r, r` zero idiom,
    /// which itself writes flags.
    zero_idiom_used: bool,
}

impl FunctionEncoding {
    fn new(func: &MachineFunction, conv: &dyn CallingConvention) -> Self {
        // Frame layout (callee-saved save area) is shared with the assembly
        // printer so both describe the identical frame.
        let layout = compute_frame_layout(func, conv);

        Self {
            enc: X86_64Encoder::new(),
            relocations: Vec::new(),
            branch_fixups: Vec::new(),
            block_offsets: HashMap::new(),
            used_callee_saved: layout.used_callee_saved,
            callee_slots: layout.callee_slots,
            _locals_size: layout.locals_size,
            frame_size: layout.frame_size,
            saw_return: false,
            flags_dirty: false,
            zero_idiom_used: false,
        }
    }

    fn callee_slot(&self, reg: u8) -> i32 {
        self.callee_slots[&reg]
    }

    fn emit_prologue(&mut self) {
        self.enc.push_reg64(5); // push rbp
        self.enc.mov_r64_r64(5, 4); // mov rbp, rsp
        if self.frame_size > 0 {
            self.enc.sub_r64_imm32(4, self.frame_size); // sub rsp, frame_size
        }
        let callee_saved = self.used_callee_saved.clone();
        for &reg in &callee_saved {
            let slot = self.callee_slot(reg);
            if reg < 16 {
                self.enc.mov_rbp_offset_r64(slot, reg);
            } else {
                self.enc.movsd_mem_xmm(5, slot, None, reg - 16);
            }
        }
    }

    fn emit_epilogue(&mut self) {
        let callee_saved = self.used_callee_saved.clone();
        for &reg in &callee_saved {
            let slot = self.callee_slot(reg);
            if reg < 16 {
                self.enc.mov_r64_rbp_offset(reg, slot);
            } else {
                self.enc.movsd_xmm_mem(reg - 16, 5, slot, None);
            }
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
            "The native x86-64 backend encodes 64-bit integer GPR forms and SSE2 scalar float forms only.",
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
            // The sign-extended 8-bit immediate form (`0x83 /x ib`) is one byte
            // shorter than `0x81 /x id` and encodes identically for -128..=127.
            _ if (-128..=127).contains(&imm) => self.enc.op_r64_imm8(op.imm_ext(), d, imm as i8),
            _ => self.enc.op_r64_imm32(op.imm_ext(), d, imm),
        }
    }

    /// Materialize a 64-bit integer constant into `reg`.
    ///
    /// Uses the 5-byte `mov r32, imm32` form whenever the zero-extension is
    /// value-preserving (value fits in `u32`); otherwise falls back to the
    /// 10-byte `mov r64, imm64`. Negative values keep the full 64-bit form:
    /// the 32-bit form would zero-extend instead of sign-extending.
    fn load_int_imm(&mut self, reg: u8, value: i64) {
        match u32::try_from(value) {
            Ok(u) => self.enc.mov_r32_imm32(reg, u as i32),
            Err(_) => self.enc.mov_r64_imm64(reg, value),
        }
    }

    /// Materialize an integer zero into a GPR.
    ///
    /// The 3-byte `xor r, r` idiom is preferred over `mov r64, 0` when EFLAGS
    /// are not live (a pending `Compare`/`Test`/`FCmp` may still be consumed by
    /// a later `SetCc`/`BranchCc`); otherwise the flag-preserving 5-byte
    /// `mov r32, 0` is used.
    fn zero_gpr(&mut self, reg: u8) {
        if self.flags_dirty {
            self.enc.mov_r32_imm32(reg, 0);
        } else {
            self.enc.xor_r64_r64(reg, reg);
            self.zero_idiom_used = true;
        }
    }

    /// Materialize a raw 64-bit pattern into a GPR, using the short zero idiom
    /// for an all-zero pattern.
    fn materialize_bits(&mut self, reg: u8, bits: i64) {
        if bits == 0 {
            self.zero_gpr(reg);
        } else {
            self.load_int_imm(reg, bits);
        }
    }

    /// Emit `op dst, cl` for the given shift kind.
    fn emit_shift_by_cl(&mut self, op: ShiftOp, d: u8) {
        match op {
            ShiftOp::Shl => self.enc.shl_r64_cl(d),
            ShiftOp::Shr => self.enc.shr_r64_cl(d),
            ShiftOp::Sar => self.enc.sar_r64_cl(d),
        }
    }

    /// Transfer a shift count from `count` into CL and shift `d`, preserving
    /// RCX and the destination.
    ///
    /// `shl dst, cl` reads its count from CL, so the sequence has to borrow
    /// RCX. Two aliasing cases must be handled explicitly:
    /// - `d == RCX`: the destination value must be shifted *after* RCX is
    ///   overwritten with the count, and the result moved back into RCX (a
    ///   naive save/restore would discard the result).
    /// - `d == SCRATCH2`: the usual RCX save register is the destination, so a
    ///   different save register (RAX) is used.
    fn emit_shift_by_reg(&mut self, op: ShiftOp, d: u8, count: u8) {
        // Save register for RCX: the encoder scratch R11 unless the
        // destination or the count source already occupies it.
        let save = [SCRATCH2, 0, 2, 3]
            .into_iter()
            .find(|c| *c != d && *c != count && *c != 1)
            .unwrap_or(SCRATCH2);
        self.enc.mov_r64_r64(save, 1); // save rcx
        if d == 1 {
            // The destination is RCX: shift the saved copy, then move the
            // result back into RCX.
            self.enc.mov_r64_r64(1, count);
            self.emit_shift_by_cl(op, save);
            self.enc.mov_r64_r64(1, save);
        } else {
            self.enc.mov_r64_r64(1, count);
            self.emit_shift_by_cl(op, d);
            self.enc.mov_r64_r64(1, save);
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
                    self.load_int_imm(SCRATCH, *v);
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
                    self.load_int_imm(SCRATCH2, *v);
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

    fn load_fp_operand_into_xmm(
        &mut self,
        src: &MachineOperand,
        xmm_scratch: u8,
        is_f64: bool,
    ) -> bool {
        if let Some(s) = phys_reg(src)
            && s >= 16
        {
            let s_xmm = s - 16;
            if s_xmm != xmm_scratch {
                if is_f64 {
                    self.enc.movsd_xmm_xmm(xmm_scratch, s_xmm);
                } else {
                    self.enc.movss_xmm_xmm(xmm_scratch, s_xmm);
                }
            }
            return true;
        }
        match src {
            MachineOperand::FloatImmediate(f) => {
                if is_f64 {
                    self.enc.mov_r64_imm64(SCRATCH, f.to_bits() as i64);
                    self.enc.movq_xmm_r64(xmm_scratch, SCRATCH);
                } else {
                    let bits = (*f as f32).to_bits() as i32;
                    self.enc.mov_r32_imm32(SCRATCH, bits);
                    self.enc.movd_xmm_r32(xmm_scratch, SCRATCH);
                }
                true
            }
            MachineOperand::Immediate(v) => {
                self.enc.mov_r64_imm64(SCRATCH, *v);
                if is_f64 {
                    self.enc.movq_xmm_r64(xmm_scratch, SCRATCH);
                } else {
                    self.enc.movd_xmm_r32(xmm_scratch, SCRATCH);
                }
                true
            }
            MachineOperand::StackSlot(slot) => {
                if is_f64 {
                    self.enc.movsd_xmm_mem(xmm_scratch, 5, *slot, None);
                } else {
                    self.enc.movss_xmm_mem(xmm_scratch, 5, *slot, None);
                }
                true
            }
            MachineOperand::Memory { .. } => {
                if let Some((b, off, idx)) = mem_operand(src) {
                    if is_f64 {
                        self.enc.movsd_xmm_mem(xmm_scratch, b, off, idx);
                    } else {
                        self.enc.movss_xmm_mem(xmm_scratch, b, off, idx);
                    }
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn encode_move(&mut self, dst: &MachineOperand, src: &MachineOperand) -> bool {
        // Register destination.
        if let Some(d) = phys_reg(dst) {
            if d >= 16 {
                // FP register destination (XMM d-16)
                let xd = d - 16;
                if let Some(s) = phys_reg(src) {
                    if s >= 16 {
                        // XMM to XMM
                        self.enc.movsd_xmm_xmm(xd, s - 16);
                        return true;
                    } else {
                        // GPR to XMM (bit transfer)
                        self.enc.movq_xmm_r64(xd, s);
                        return true;
                    }
                }
                match src {
                    MachineOperand::FloatImmediate(f) => {
                        self.materialize_bits(SCRATCH, f.to_bits() as i64);
                        self.enc.movq_xmm_r64(xd, SCRATCH);
                        return true;
                    }
                    MachineOperand::Immediate(v) => {
                        self.materialize_bits(SCRATCH, *v);
                        self.enc.movq_xmm_r64(xd, SCRATCH);
                        return true;
                    }
                    MachineOperand::StackSlot(slot) => {
                        self.enc.movsd_xmm_mem(xd, 5, *slot, None);
                        return true;
                    }
                    MachineOperand::Memory { .. } => {
                        if let Some((b, off, idx)) = mem_operand(src) {
                            self.enc.movsd_xmm_mem(xd, b, off, idx);
                            return true;
                        }
                    }
                    _ => {}
                }
                return false;
            }

            // GPR register destination.
            if let Some(s) = phys_reg(src) {
                if s >= 16 {
                    // XMM to GPR (bit transfer)
                    self.enc.movq_r64_xmm(d, s - 16);
                    return true;
                } else {
                    self.enc.mov_r64_r64(d, s);
                    return true;
                }
            }
            match src {
                MachineOperand::Immediate(v) => {
                    self.materialize_bits(d, *v);
                    return true;
                }
                MachineOperand::FloatImmediate(f) => {
                    // Materialize the IEEE-754 double bit pattern into the GPR.
                    self.materialize_bits(d, f.to_bits() as i64);
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
                if s >= 16 {
                    self.enc.movsd_mem_xmm(5, slot, None, s - 16);
                    return true;
                } else {
                    self.enc.mov_rbp_offset_r64(slot, s);
                    return true;
                }
            }
            match src {
                MachineOperand::Immediate(v) => {
                    self.materialize_bits(SCRATCH, *v);
                    self.enc.mov_rbp_offset_r64(slot, SCRATCH);
                    return true;
                }
                MachineOperand::FloatImmediate(f) => {
                    self.materialize_bits(SCRATCH, f.to_bits() as i64);
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
                if s >= 16 {
                    self.enc.movsd_mem_xmm(b, off, idx, s - 16);
                    return true;
                } else {
                    self.enc.mov_mem_r64(b, off, idx, s);
                    return true;
                }
            }
            match src {
                MachineOperand::Immediate(v) => {
                    self.materialize_bits(SCRATCH, *v);
                    self.enc.mov_mem_r64(b, off, idx, SCRATCH);
                    return true;
                }
                MachineOperand::FloatImmediate(f) => {
                    self.materialize_bits(SCRATCH, f.to_bits() as i64);
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
        if let Some(d) = phys_reg(dst) {
            return match src {
                MachineOperand::Register(MachineRegister::Physical(s)) => {
                    self.enc.imul_r64_r64(d, s.0);
                    true
                }
                MachineOperand::Immediate(v) => {
                    self.load_int_imm(SCRATCH, *v);
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
            };
        }

        // Spilled destination: load into scratch, multiply, store back.
        if let Some(slot) = stack_slot(dst) {
            self.enc.mov_r64_rbp_offset(SCRATCH, slot);
            let src_reg = match src {
                MachineOperand::Register(MachineRegister::Physical(s)) => s.0,
                MachineOperand::Immediate(v) => {
                    self.load_int_imm(SCRATCH2, *v);
                    SCRATCH2
                }
                MachineOperand::StackSlot(vs) => {
                    self.enc.mov_r64_rbp_offset(SCRATCH2, *vs);
                    SCRATCH2
                }
                MachineOperand::Memory { .. } => match mem_operand(src) {
                    Some((b, off, idx)) => {
                        self.enc.mov_r64_mem(SCRATCH2, b, off, idx);
                        SCRATCH2
                    }
                    None => return false,
                },
                _ => return false,
            };
            if src_reg != SCRATCH {
                self.enc.imul_r64_r64(SCRATCH, src_reg);
            }
            self.enc.mov_rbp_offset_r64(slot, SCRATCH);
            return true;
        }
        false
    }

    /// Encode DIV (quotient) or MOD (remainder). RAX/RDX are implicit
    /// operands of IDIV; both are reserved scratch registers, so clobbering
    /// them is safe.
    ///
    /// A spilled destination is computed in SCRATCH2 and stored back.
    fn encode_divmod(&mut self, is_mod: bool, dst: &MachineOperand, src: &MachineOperand) -> bool {
        let (d, d_slot) = if let Some(d) = phys_reg(dst) {
            (d, None)
        } else if let Some(slot) = stack_slot(dst) {
            // Load the dividend out of the spill slot first; the result is
            // computed in SCRATCH2 and stored back.
            self.enc.mov_r64_rbp_offset(SCRATCH2, slot);
            (SCRATCH2, Some(slot))
        } else {
            return false;
        };
        let src_reg = match src {
            MachineOperand::Register(MachineRegister::Physical(s)) => s.0,
            MachineOperand::Immediate(v) => {
                self.load_int_imm(SCRATCH, *v);
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
        if let Some(slot) = d_slot {
            self.enc.mov_rbp_offset_r64(slot, d);
        }
        true
    }

    /// Encode a scalar FP binary operation (`addsd`/`addss`/...).
    ///
    /// A spilled destination is computed in FP_SCRATCH and stored back; the
    /// source then uses the *other* FP scratch register so it cannot clobber
    /// the destination.
    fn encode_fp_binop(
        &mut self,
        op: FpOp,
        dst: &MachineOperand,
        src: &MachineOperand,
        size: u8,
    ) -> bool {
        let is_f64 = size == 8;
        let (d_xmm, d_slot) = if let Some(d) = phys_reg(dst) {
            if d >= 16 {
                (d - 16, None)
            } else {
                return false;
            }
        } else if let Some(slot) = stack_slot(dst) {
            if is_f64 {
                self.enc.movsd_xmm_mem(FP_SCRATCH, 5, slot, None);
            } else {
                self.enc.movss_xmm_mem(FP_SCRATCH, 5, slot, None);
            }
            (FP_SCRATCH, Some(slot))
        } else {
            return false;
        };

        let src_scratch = if d_xmm == FP_SCRATCH {
            FP_SCRATCH2
        } else {
            FP_SCRATCH
        };
        let s_xmm = if let Some(s) = phys_reg(src) {
            if s >= 16 {
                s - 16
            } else {
                return false;
            }
        } else {
            if !self.load_fp_operand_into_xmm(src, src_scratch, is_f64) {
                return false;
            }
            src_scratch
        };

        if is_f64 {
            match op {
                FpOp::Add => self.enc.addsd_xmm_xmm(d_xmm, s_xmm),
                FpOp::Sub => self.enc.subsd_xmm_xmm(d_xmm, s_xmm),
                FpOp::Mul => self.enc.mulsd_xmm_xmm(d_xmm, s_xmm),
                FpOp::Div => self.enc.divsd_xmm_xmm(d_xmm, s_xmm),
            }
            if let Some(slot) = d_slot {
                self.enc.movsd_mem_xmm(5, slot, None, d_xmm);
            }
        } else {
            match op {
                FpOp::Add => self.enc.addss_xmm_xmm(d_xmm, s_xmm),
                FpOp::Sub => self.enc.subss_xmm_xmm(d_xmm, s_xmm),
                FpOp::Mul => self.enc.mulss_xmm_xmm(d_xmm, s_xmm),
                FpOp::Div => self.enc.divss_xmm_xmm(d_xmm, s_xmm),
            }
            if let Some(slot) = d_slot {
                self.enc.movss_mem_xmm(5, slot, None, d_xmm);
            }
        }
        true
    }

    /// Emit an unsigned 64-bit integer to scalar float conversion.
    ///
    /// `cvtsi2sd`/`cvtsi2ss` interpret their source as signed, so values with
    /// the top bit set are converted by the standard trick: the value is
    /// halved (arithmetic shift), rounded up with its low bit, converted, and
    /// the result doubled - `f = (x>>1) + (x>>1) + (x&1)`.
    ///
    /// The source is copied into the scratch registers first, so a source that
    /// already lives in a scratch register (spilled or immediate operand) is
    /// not destroyed.
    fn encode_unsigned_int_to_float(&mut self, dst_xmm: u8, src_gpr: u8, is_f64: bool) {
        self.enc.test_r64_r64(src_gpr, src_gpr);
        let jns_instr_offset = self.enc.len();
        self.enc.jcc_rel32(ConditionCode::GreaterOrEqual, 0);
        let jns_disp_offset = self.enc.len() - 4;

        self.enc.mov_r64_r64(SCRATCH, src_gpr);
        self.enc.mov_r64_r64(SCRATCH2, SCRATCH);
        self.enc.shr_r64_imm8(SCRATCH, 1);
        self.enc.op_r64_imm8(4, SCRATCH2, 1); // and scratch2, 1
        self.enc.or_r64_r64(SCRATCH, SCRATCH2);
        if is_f64 {
            self.enc.cvtsi2sd_xmm_r64(dst_xmm, SCRATCH);
            self.enc.addsd_xmm_xmm(dst_xmm, dst_xmm);
        } else {
            self.enc.cvtsi2ss_xmm_r64(dst_xmm, SCRATCH);
            self.enc.addss_xmm_xmm(dst_xmm, dst_xmm);
        }

        let jmp_instr_offset = self.enc.len();
        self.enc.jmp_rel32(0);
        let jmp_disp_offset = self.enc.len() - 4;

        let pos_offset = self.enc.len();
        if is_f64 {
            self.enc.cvtsi2sd_xmm_r64(dst_xmm, src_gpr);
        } else {
            self.enc.cvtsi2ss_xmm_r64(dst_xmm, src_gpr);
        }

        let end_offset = self.enc.len();

        let jns_disp = (pos_offset as i64 - (jns_instr_offset as i64 + 6)) as i32;
        self.enc.buffer[jns_disp_offset..jns_disp_offset + 4]
            .copy_from_slice(&jns_disp.to_le_bytes());

        let jmp_disp = (end_offset as i64 - (jmp_instr_offset as i64 + 5)) as i32;
        self.enc.buffer[jmp_disp_offset..jmp_disp_offset + 4]
            .copy_from_slice(&jmp_disp.to_le_bytes());
    }

    /// Encode a shift. The hardware count operand is either an imm8 (masked
    /// to 6 bits for 64-bit shifts) or the CL register.
    ///
    /// A spilled destination is shifted in SCRATCH and stored back.
    fn encode_shift(&mut self, op: ShiftOp, dst: &MachineOperand, src: &MachineOperand) -> bool {
        let (d, d_slot) = if let Some(d) = phys_reg(dst) {
            (d, None)
        } else if let Some(slot) = stack_slot(dst) {
            self.enc.mov_r64_rbp_offset(SCRATCH, slot);
            (SCRATCH, Some(slot))
        } else {
            return false;
        };
        let encoded = match src {
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
                self.emit_shift_by_cl(op, d);
                true
            }
            MachineOperand::Register(MachineRegister::Physical(s)) => {
                // Shift counts live in CL only: transfer the value while
                // preserving RCX (and the destination, which may itself be
                // RCX). MOV does not touch flags, so a preceding Compare stays
                // valid.
                self.emit_shift_by_reg(op, d, s.0);
                true
            }
            MachineOperand::StackSlot(slot) => {
                // Load the count into a scratch first so RCX can be saved and
                // the destination is not disturbed by the count load.
                self.enc.mov_r64_rbp_offset(SCRATCH2, *slot);
                self.emit_shift_by_reg(op, d, SCRATCH2);
                true
            }
            _ => false,
        };
        if encoded && let Some(slot) = d_slot {
            self.enc.mov_rbp_offset_r64(slot, SCRATCH);
        }
        encoded
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
        // XMM destinations are used by the spill rewriter for spilled floats.
        if dst >= 16 {
            if size == 4 {
                self.enc.movss_xmm_mem(dst - 16, base, offset, index);
            } else {
                self.enc.movsd_xmm_mem(dst - 16, base, offset, index);
            }
            return;
        }
        match size {
            1 => self.enc.movzx_r64_mem8(dst, base, offset, index),
            2 => self.enc.movzx_r64_mem16(dst, base, offset, index),
            4 => self.enc.mov_r32_mem(dst, base, offset, index),
            _ => self.enc.mov_r64_mem(dst, base, offset, index),
        }
    }

    fn store_mem(&mut self, base: u8, offset: i32, index: Option<(u8, u8)>, src: u8, size: u8) {
        // XMM sources are used by the spill rewriter for spilled floats.
        if src >= 16 {
            if size == 4 {
                self.enc.movss_mem_xmm(base, offset, index, src - 16);
            } else {
                self.enc.movsd_mem_xmm(base, offset, index, src - 16);
            }
            return;
        }
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
            self.load_int_imm(SCRATCH, *v);
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
                self.load_int_imm(SCRATCH, *v);
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
                MachineOperand::StackSlot(slot) => {
                    self.enc.mov_r64_rbp_offset(11, *slot);
                    self.enc.call_r64(11);
                }
                MachineOperand::Memory { .. } => {
                    if let Some((b, off, idx)) = mem_operand(target) {
                        self.enc.mov_r64_mem(11, b, off, idx);
                        self.enc.call_r64(11);
                    } else {
                        return Err(self.unsupported(inst, func_name, abi));
                    }
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
            MachineInstruction::FAdd { dst, src, size } => {
                if !self.encode_fp_binop(FpOp::Add, dst, src, *size) {
                    return Err(self.unsupported(inst, func_name, abi));
                }
            }
            MachineInstruction::FSub { dst, src, size } => {
                if !self.encode_fp_binop(FpOp::Sub, dst, src, *size) {
                    return Err(self.unsupported(inst, func_name, abi));
                }
            }
            MachineInstruction::FMul { dst, src, size } => {
                if !self.encode_fp_binop(FpOp::Mul, dst, src, *size) {
                    return Err(self.unsupported(inst, func_name, abi));
                }
            }
            MachineInstruction::FDiv { dst, src, size } => {
                if !self.encode_fp_binop(FpOp::Div, dst, src, *size) {
                    return Err(self.unsupported(inst, func_name, abi));
                }
            }
            MachineInstruction::FNeg { dst, size } => {
                let is_f64 = *size == 8;
                let (d_xmm, d_slot) = if let Some(d) = phys_reg(dst) {
                    if d >= 16 {
                        (d - 16, None)
                    } else {
                        return Err(self.unsupported(inst, func_name, abi));
                    }
                } else if let Some(slot) = stack_slot(dst) {
                    if is_f64 {
                        self.enc.movsd_xmm_mem(FP_SCRATCH, 5, slot, None);
                    } else {
                        self.enc.movss_xmm_mem(FP_SCRATCH, 5, slot, None);
                    }
                    (FP_SCRATCH, Some(slot))
                } else {
                    return Err(self.unsupported(inst, func_name, abi));
                };
                // The sign-bit mask must not overwrite the value being negated
                // (which lives in FP_SCRATCH when the destination is spilled).
                let mask_xmm = if d_xmm == FP_SCRATCH {
                    FP_SCRATCH2
                } else {
                    FP_SCRATCH
                };
                if is_f64 {
                    self.enc.mov_r64_imm64(
                        SCRATCH,
                        i64::from_ne_bytes(0x8000_0000_0000_0000u64.to_ne_bytes()),
                    );
                    self.enc.movq_xmm_r64(mask_xmm, SCRATCH);
                    self.enc.xorpd_xmm_xmm(d_xmm, mask_xmm);
                    if let Some(slot) = d_slot {
                        self.enc.movsd_mem_xmm(5, slot, None, d_xmm);
                    }
                } else {
                    self.enc.mov_r32_imm32(SCRATCH, 0x8000_0000u32 as i32);
                    self.enc.movd_xmm_r32(mask_xmm, SCRATCH);
                    self.enc.xorps_xmm_xmm(d_xmm, mask_xmm);
                    if let Some(slot) = d_slot {
                        self.enc.movss_mem_xmm(5, slot, None, d_xmm);
                    }
                }
            }
            MachineInstruction::FCmp { lhs, rhs, size } => {
                let is_f64 = *size == 8;
                let l_xmm = if let Some(l) = phys_reg(lhs) {
                    if l >= 16 {
                        l - 16
                    } else {
                        return Err(self.unsupported(inst, func_name, abi));
                    }
                } else if let Some(slot) = stack_slot(lhs) {
                    if is_f64 {
                        self.enc.movsd_xmm_mem(FP_SCRATCH, 5, slot, None);
                    } else {
                        self.enc.movss_xmm_mem(FP_SCRATCH, 5, slot, None);
                    }
                    FP_SCRATCH
                } else {
                    return Err(self.unsupported(inst, func_name, abi));
                };
                let r_xmm = if let Some(r) = phys_reg(rhs) {
                    if r >= 16 {
                        r - 16
                    } else {
                        return Err(self.unsupported(inst, func_name, abi));
                    }
                } else {
                    // Load the right-hand side into a scratch that is not the
                    // register already holding the left-hand side.
                    let rhs_scratch = if l_xmm == FP_SCRATCH {
                        FP_SCRATCH2
                    } else {
                        FP_SCRATCH
                    };
                    if !self.load_fp_operand_into_xmm(rhs, rhs_scratch, is_f64) {
                        return Err(self.unsupported(inst, func_name, abi));
                    }
                    rhs_scratch
                };
                if is_f64 {
                    self.enc.ucomisd_xmm_xmm(l_xmm, r_xmm);
                } else {
                    self.enc.ucomiss_xmm_xmm(l_xmm, r_xmm);
                }
            }
            MachineInstruction::FCvtIntToFloat {
                dst,
                src,
                is_f64,
                is_signed,
            } => {
                let (d_xmm, d_slot) = if let Some(d) = phys_reg(dst) {
                    if d >= 16 {
                        (d - 16, None)
                    } else {
                        return Err(self.unsupported(inst, func_name, abi));
                    }
                } else if let Some(slot) = stack_slot(dst) {
                    (FP_SCRATCH, Some(slot))
                } else {
                    return Err(self.unsupported(inst, func_name, abi));
                };
                let s_gpr = if let Some(s) = phys_reg(src) {
                    if s < 16 {
                        s
                    } else {
                        return Err(self.unsupported(inst, func_name, abi));
                    }
                } else if let Some(slot) = stack_slot(src) {
                    self.enc.mov_r64_rbp_offset(SCRATCH, slot);
                    SCRATCH
                } else if let MachineOperand::Immediate(v) = src {
                    self.enc.mov_r64_imm64(SCRATCH, *v);
                    SCRATCH
                } else {
                    return Err(self.unsupported(inst, func_name, abi));
                };

                if *is_signed {
                    if *is_f64 {
                        self.enc.cvtsi2sd_xmm_r64(d_xmm, s_gpr);
                        if let Some(slot) = d_slot {
                            self.enc.movsd_mem_xmm(5, slot, None, d_xmm);
                        }
                    } else {
                        self.enc.cvtsi2ss_xmm_r64(d_xmm, s_gpr);
                        if let Some(slot) = d_slot {
                            self.enc.movss_mem_xmm(5, slot, None, d_xmm);
                        }
                    }
                } else {
                    self.encode_unsigned_int_to_float(d_xmm, s_gpr, *is_f64);
                    if let Some(slot) = d_slot {
                        if *is_f64 {
                            self.enc.movsd_mem_xmm(5, slot, None, d_xmm);
                        } else {
                            self.enc.movss_mem_xmm(5, slot, None, d_xmm);
                        }
                    }
                }
            }
            MachineInstruction::FCvtFloatToInt {
                dst,
                src,
                is_f64,
                is_signed,
            } => {
                let d_gpr = if let Some(d) = phys_reg(dst) {
                    if d < 16 {
                        d
                    } else {
                        return Err(self.unsupported(inst, func_name, abi));
                    }
                } else if let Some(_slot) = stack_slot(dst) {
                    SCRATCH
                } else {
                    return Err(self.unsupported(inst, func_name, abi));
                };
                let s_xmm = if let Some(s) = phys_reg(src) {
                    if s >= 16 {
                        s - 16
                    } else {
                        return Err(self.unsupported(inst, func_name, abi));
                    }
                } else if let Some(slot) = stack_slot(src) {
                    if *is_f64 {
                        self.enc.movsd_xmm_mem(FP_SCRATCH, 5, slot, None);
                    } else {
                        self.enc.movss_xmm_mem(FP_SCRATCH, 5, slot, None);
                    }
                    FP_SCRATCH
                } else {
                    return Err(self.unsupported(inst, func_name, abi));
                };

                if *is_signed {
                    if *is_f64 {
                        self.enc.cvttsd2si_r64_xmm(d_gpr, s_xmm);
                    } else {
                        self.enc.cvttss2si_r64_xmm(d_gpr, s_xmm);
                    }
                } else if *is_f64 {
                    // 2^63 as an f64: values at or above it must be biased by
                    // subtracting 2^63 before a signed conversion and adding
                    // 2^63 (the sign bit) back afterwards.
                    //
                    // Two XMM scratch registers are needed (the magic constant
                    // and the biased value); the source must never be the
                    // register holding the magic constant.
                    let magic_xmm = if s_xmm == FP_SCRATCH {
                        FP_SCRATCH2
                    } else {
                        FP_SCRATCH
                    };
                    let temp_xmm = if magic_xmm == FP_SCRATCH {
                        FP_SCRATCH2
                    } else {
                        FP_SCRATCH
                    };

                    self.enc.mov_r64_imm64(SCRATCH, 0x43E0_0000_0000_0000i64);
                    self.enc.movq_xmm_r64(magic_xmm, SCRATCH);

                    self.enc.ucomisd_xmm_xmm(s_xmm, magic_xmm);

                    let jge_instr_offset = self.enc.len();
                    self.enc.jcc_rel32(ConditionCode::AboveOrEqual, 0);
                    let jge_disp_offset = self.enc.len() - 4;

                    self.enc.cvttsd2si_r64_xmm(d_gpr, s_xmm);

                    let jmp_instr_offset = self.enc.len();
                    self.enc.jmp_rel32(0);
                    let jmp_disp_offset = self.enc.len() - 4;

                    let ge_offset = self.enc.len();
                    self.enc.movsd_xmm_xmm(temp_xmm, s_xmm);
                    self.enc.subsd_xmm_xmm(temp_xmm, magic_xmm);
                    self.enc.cvttsd2si_r64_xmm(d_gpr, temp_xmm);
                    // The biased result needs the sign bit added; the constant
                    // must not land in the destination register itself.
                    let add_temp = if d_gpr != SCRATCH { SCRATCH } else { SCRATCH2 };
                    self.enc.mov_r64_imm64(
                        add_temp,
                        i64::from_ne_bytes(0x8000_0000_0000_0000u64.to_ne_bytes()),
                    );
                    self.enc.add_r64_r64(d_gpr, add_temp);

                    let end_offset = self.enc.len();

                    let jge_disp = (ge_offset as i64 - (jge_instr_offset as i64 + 6)) as i32;
                    self.enc.buffer[jge_disp_offset..jge_disp_offset + 4]
                        .copy_from_slice(&jge_disp.to_le_bytes());

                    let jmp_disp = (end_offset as i64 - (jmp_instr_offset as i64 + 5)) as i32;
                    self.enc.buffer[jmp_disp_offset..jmp_disp_offset + 4]
                        .copy_from_slice(&jmp_disp.to_le_bytes());
                } else {
                    // No SSE2 instruction converts an unsigned 64-bit integer
                    // to a float32 directly; treat the value as signed like the
                    // scalar path above (CVTTSS2SI is signed).
                    self.enc.cvttss2si_r64_xmm(d_gpr, s_xmm);
                }

                if let Some(slot) = stack_slot(dst) {
                    self.enc.mov_rbp_offset_r64(slot, d_gpr);
                }
            }
            MachineInstruction::FCvtFloatToFloat { dst, src, to_f64 } => {
                let (d_xmm, d_slot) = if let Some(d) = phys_reg(dst) {
                    if d >= 16 {
                        (d - 16, None)
                    } else {
                        return Err(self.unsupported(inst, func_name, abi));
                    }
                } else if let Some(slot) = stack_slot(dst) {
                    (FP_SCRATCH, Some(slot))
                } else {
                    return Err(self.unsupported(inst, func_name, abi));
                };
                let src_scratch = if d_xmm == FP_SCRATCH {
                    FP_SCRATCH2
                } else {
                    FP_SCRATCH
                };
                let s_xmm = if let Some(s) = phys_reg(src) {
                    if s >= 16 {
                        s - 16
                    } else {
                        return Err(self.unsupported(inst, func_name, abi));
                    }
                } else if let Some(slot) = stack_slot(src) {
                    if *to_f64 {
                        self.enc.movss_xmm_mem(src_scratch, 5, slot, None);
                    } else {
                        self.enc.movsd_xmm_mem(src_scratch, 5, slot, None);
                    }
                    src_scratch
                } else {
                    return Err(self.unsupported(inst, func_name, abi));
                };
                if *to_f64 {
                    self.enc.cvtss2sd_xmm_xmm(d_xmm, s_xmm);
                    if let Some(slot) = d_slot {
                        self.enc.movsd_mem_xmm(5, slot, None, d_xmm);
                    }
                } else {
                    self.enc.cvtsd2ss_xmm_xmm(d_xmm, s_xmm);
                    if let Some(slot) = d_slot {
                        self.enc.movss_mem_xmm(5, slot, None, d_xmm);
                    }
                }
            }
            MachineInstruction::VectorAdd { dst, src, vec_type } => {
                let d_xmm = phys_reg(dst)
                    .map(|d| if d >= 16 { d - 16 } else { d })
                    .unwrap_or(FP_SCRATCH);
                let s_xmm = phys_reg(src)
                    .map(|s| if s >= 16 { s - 16 } else { s })
                    .unwrap_or(FP_SCRATCH2);
                if vec_type.is_floating_point() {
                    if vec_type.element_type == crate::opt::VectorElementType::F64 {
                        self.enc.addpd_xmm_xmm(d_xmm, s_xmm);
                    } else {
                        self.enc.addps_xmm_xmm(d_xmm, s_xmm);
                    }
                } else {
                    self.enc.paddd_xmm_xmm(d_xmm, s_xmm);
                }
            }
            MachineInstruction::VectorSub { dst, src, vec_type } => {
                let d_xmm = phys_reg(dst)
                    .map(|d| if d >= 16 { d - 16 } else { d })
                    .unwrap_or(FP_SCRATCH);
                let s_xmm = phys_reg(src)
                    .map(|s| if s >= 16 { s - 16 } else { s })
                    .unwrap_or(FP_SCRATCH2);
                if vec_type.is_floating_point() {
                    if vec_type.element_type == crate::opt::VectorElementType::F64 {
                        self.enc.subpd_xmm_xmm(d_xmm, s_xmm);
                    } else {
                        self.enc.subps_xmm_xmm(d_xmm, s_xmm);
                    }
                } else {
                    self.enc.psubd_xmm_xmm(d_xmm, s_xmm);
                }
            }
            MachineInstruction::VectorMul { dst, src, vec_type } => {
                let d_xmm = phys_reg(dst)
                    .map(|d| if d >= 16 { d - 16 } else { d })
                    .unwrap_or(FP_SCRATCH);
                let s_xmm = phys_reg(src)
                    .map(|s| if s >= 16 { s - 16 } else { s })
                    .unwrap_or(FP_SCRATCH2);
                if vec_type.is_floating_point() {
                    if vec_type.element_type == crate::opt::VectorElementType::F64 {
                        self.enc.mulpd_xmm_xmm(d_xmm, s_xmm);
                    } else {
                        self.enc.mulps_xmm_xmm(d_xmm, s_xmm);
                    }
                } else {
                    self.enc.pmulld_xmm_xmm(d_xmm, s_xmm);
                }
            }
            MachineInstruction::VectorDiv { dst, src, vec_type } => {
                let d_xmm = phys_reg(dst)
                    .map(|d| if d >= 16 { d - 16 } else { d })
                    .unwrap_or(FP_SCRATCH);
                let s_xmm = phys_reg(src)
                    .map(|s| if s >= 16 { s - 16 } else { s })
                    .unwrap_or(FP_SCRATCH2);
                if vec_type.element_type == crate::opt::VectorElementType::F64 {
                    self.enc.divpd_xmm_xmm(d_xmm, s_xmm);
                } else {
                    self.enc.divps_xmm_xmm(d_xmm, s_xmm);
                }
            }
            MachineInstruction::VectorAnd { dst, src, .. } => {
                let d_xmm = phys_reg(dst)
                    .map(|d| if d >= 16 { d - 16 } else { d })
                    .unwrap_or(FP_SCRATCH);
                let s_xmm = phys_reg(src)
                    .map(|s| if s >= 16 { s - 16 } else { s })
                    .unwrap_or(FP_SCRATCH2);
                self.enc.andps_xmm_xmm(d_xmm, s_xmm);
            }
            MachineInstruction::VectorOr { dst, src, .. } => {
                let d_xmm = phys_reg(dst)
                    .map(|d| if d >= 16 { d - 16 } else { d })
                    .unwrap_or(FP_SCRATCH);
                let s_xmm = phys_reg(src)
                    .map(|s| if s >= 16 { s - 16 } else { s })
                    .unwrap_or(FP_SCRATCH2);
                self.enc.orps_xmm_xmm(d_xmm, s_xmm);
            }
            MachineInstruction::VectorXor { dst, src, .. } => {
                let d_xmm = phys_reg(dst)
                    .map(|d| if d >= 16 { d - 16 } else { d })
                    .unwrap_or(FP_SCRATCH);
                let s_xmm = phys_reg(src)
                    .map(|s| if s >= 16 { s - 16 } else { s })
                    .unwrap_or(FP_SCRATCH2);
                self.enc.xorps_xmm_xmm(d_xmm, s_xmm);
            }
            MachineInstruction::VectorLoad { dst, src, .. } => {
                let d_xmm = phys_reg(dst)
                    .map(|d| if d >= 16 { d - 16 } else { d })
                    .unwrap_or(FP_SCRATCH);
                if let Some((b, off, idx)) = mem_operand(src) {
                    self.enc.movups_xmm_mem(d_xmm, b, off, idx);
                } else if let Some(slot) = stack_slot(src) {
                    self.enc.movups_xmm_mem(d_xmm, 5, slot, None);
                }
            }
            MachineInstruction::VectorStore { dst, src, .. } => {
                let s_xmm = phys_reg(src)
                    .map(|s| if s >= 16 { s - 16 } else { s })
                    .unwrap_or(FP_SCRATCH);
                if let Some((b, off, idx)) = mem_operand(dst) {
                    self.enc.movups_mem_xmm(b, off, idx, s_xmm);
                } else if let Some(slot) = stack_slot(dst) {
                    self.enc.movups_mem_xmm(5, slot, None, s_xmm);
                }
            }
            MachineInstruction::VectorBroadcast { dst, src, .. } => {
                let d_xmm = phys_reg(dst)
                    .map(|d| if d >= 16 { d - 16 } else { d })
                    .unwrap_or(FP_SCRATCH);
                if let Some(s) = phys_reg(src) {
                    let s_xmm = if s >= 16 { s - 16 } else { s };
                    if d_xmm != s_xmm {
                        self.enc.movaps_xmm_xmm(d_xmm, s_xmm);
                    }
                    self.enc.shufps_xmm_xmm_imm8(d_xmm, d_xmm, 0x00);
                }
            }
            MachineInstruction::VectorShuffle { dst, src, mask, .. } => {
                let d_xmm = phys_reg(dst)
                    .map(|d| if d >= 16 { d - 16 } else { d })
                    .unwrap_or(FP_SCRATCH);
                let s_xmm = phys_reg(src)
                    .map(|s| if s >= 16 { s - 16 } else { s })
                    .unwrap_or(FP_SCRATCH2);
                self.enc.shufps_xmm_xmm_imm8(d_xmm, s_xmm, *mask);
            }
            MachineInstruction::VectorReduceAdd { dst, src, .. } => {
                let d_xmm = phys_reg(dst)
                    .map(|d| if d >= 16 { d - 16 } else { d })
                    .unwrap_or(FP_SCRATCH);
                let s_xmm = phys_reg(src)
                    .map(|s| if s >= 16 { s - 16 } else { s })
                    .unwrap_or(FP_SCRATCH);
                if d_xmm != s_xmm {
                    self.enc.movaps_xmm_xmm(d_xmm, s_xmm);
                }
                self.enc.movaps_xmm_xmm(FP_SCRATCH, d_xmm);
                self.enc.shufps_xmm_xmm_imm8(FP_SCRATCH, FP_SCRATCH, 0x4E);
                self.enc.addps_xmm_xmm(d_xmm, FP_SCRATCH);
                self.enc.movaps_xmm_xmm(FP_SCRATCH, d_xmm);
                self.enc.shufps_xmm_xmm_imm8(FP_SCRATCH, FP_SCRATCH, 0xB1);
                self.enc.addps_xmm_xmm(d_xmm, FP_SCRATCH);
            }
            MachineInstruction::AtomicLoad { dst, src, size } => {
                if !self.encode_load(dst, src, *size) {
                    return Err(self.unsupported(inst, func_name, abi));
                }
            }
            MachineInstruction::AtomicStore { dst, src, size } => {
                if !self.encode_store(dst, src, *size) {
                    return Err(self.unsupported(inst, func_name, abi));
                }
                self.enc.mfence();
            }
            MachineInstruction::AtomicFetchAdd { dst, src, .. } => {
                let reg = phys_reg(src).unwrap_or(SCRATCH);
                if let Some((b, off, idx)) = mem_operand(dst) {
                    self.enc.lock_xadd_mem_r64(b, off, idx, reg);
                } else if let Some(slot) = stack_slot(dst) {
                    self.enc.lock_xadd_mem_r64(5, slot, None, reg);
                }
            }
            MachineInstruction::AtomicCompareExchange {
                dst,
                expected,
                desired,
                ..
            } => {
                if let Some(exp_r) = phys_reg(expected) {
                    if exp_r != 0 {
                        self.enc.mov_r64_r64(0, exp_r);
                    }
                } else if let MachineOperand::Immediate(v) = expected {
                    self.enc.mov_r64_imm64(0, *v);
                }
                let des_r = phys_reg(desired).unwrap_or(SCRATCH);
                if let Some((b, off, idx)) = mem_operand(dst) {
                    self.enc.lock_cmpxchg_mem_r64(b, off, idx, des_r);
                } else if let Some(slot) = stack_slot(dst) {
                    self.enc.lock_cmpxchg_mem_r64(5, slot, None, des_r);
                }
            }
            MachineInstruction::ParallelMove { moves } => {
                let resolver = ParallelMoveResolver::for_x86_64();
                let resolved = resolver.resolve_moves(moves)?;
                for inner_inst in &resolved {
                    self.encode_instruction(inner_inst, func_name, abi)?;
                }
            }
            MachineInstruction::Custom { name, operands } => {
                // ENDBR64 is the only custom instruction with a native
                // encoding; it is a CET indirect-branch landing pad and is
                // emitted only at function entry (see ControlFlowIntegrityPass).
                if name == "endbr64" && operands.is_empty() {
                    self.enc.endbr64();
                } else {
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
        }
        // Track whether the flags are still meaningful for a later condition
        // test, so the `xor r, r` zero idiom can avoid clobbering them.
        self.flags_dirty = instruction_writes_flags(inst) || self.zero_idiom_used;
        self.zero_idiom_used = false;
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
    pub opt_level: OptLevel,
}

impl X86_64Backend {
    pub fn new(target: TargetDescriptor) -> Self {
        let capabilities = TargetCapabilities::for_architecture(&target.architecture);
        Self {
            target,
            capabilities,
            opt_level: OptLevel::O2,
        }
    }

    pub fn with_opt_level(mut self, opt_level: OptLevel) -> Self {
        self.opt_level = opt_level;
        self
    }

    pub fn set_opt_level(&mut self, opt_level: OptLevel) {
        self.opt_level = opt_level;
    }

    pub fn calling_convention(&self) -> Box<dyn CallingConvention> {
        match self.target.operating_system {
            OperatingSystem::Windows => Box::new(WindowsX64CallingConvention),
            _ => Box::new(SystemVX64CallingConvention),
        }
    }

    /// Expand all ParallelMove instructions in a function using the ParallelMoveResolver.
    pub fn expand_parallel_moves(func: &mut MachineFunction) -> Result<(), CodegenError> {
        let resolver = ParallelMoveResolver::for_x86_64();
        for block in &mut func.blocks {
            let mut new_instructions = Vec::with_capacity(block.instructions.len());
            for inst in block.instructions.drain(..) {
                match inst {
                    MachineInstruction::ParallelMove { moves } => {
                        let resolved = resolver.resolve_moves(&moves)?;
                        new_instructions.extend(resolved);
                    }
                    other => new_instructions.push(other),
                }
            }
            block.instructions = new_instructions;
        }
        Ok(())
    }

    /// Encode a single (already register-allocated) function.
    fn encode_function(
        &self,
        func: &MachineFunction,
    ) -> Result<(Vec<u8>, Vec<AdobRelocation>), CodegenError> {
        let mut func_expanded = func.clone();
        Self::expand_parallel_moves(&mut func_expanded)?;

        let conv = self.calling_convention();
        let abi = conv.name();
        let mut ctx = FunctionEncoding::new(&func_expanded, conv.as_ref());
        ctx.emit_prologue();

        for block in &func_expanded.blocks {
            ctx.block_offsets.insert(block.label.clone(), ctx.enc.len());
            for inst in &block.instructions {
                ctx.encode_instruction(inst, &func_expanded.name, abi)?;
            }
        }

        Ok(ctx.finish())
    }
}

/// True when no instruction in `func` mentions a virtual register, i.e. the
/// function has already been through register allocation.
fn function_is_register_allocated(func: &MachineFunction) -> bool {
    func.blocks
        .iter()
        .flat_map(|b| b.instructions.iter())
        .all(|inst| {
            !inst
                .uses()
                .iter()
                .chain(inst.defs().iter())
                .any(|r| matches!(r, MachineRegister::Virtual(_)))
        })
}

impl CodegenBackend for X86_64Backend {
    fn target(&self) -> &TargetDescriptor {
        &self.target
    }

    fn capabilities(&self) -> TargetCapabilities {
        self.capabilities.clone()
    }

    fn set_opt_level(&mut self, opt_level: OptLevel) {
        self.opt_level = opt_level;
    }

    fn lower_module(&mut self, module: &NativeModule) -> Result<NativeModule, CodegenError> {
        let mut lowered = module.clone();
        let mut pipeline = OptimizationPipeline::new(self.opt_level);
        pipeline.optimize_module_pre_alloc(&mut lowered)?;

        let reg_file = X86_64RegisterFile::for_os(self.target.operating_system);
        let allocator = LinearScanAllocator::new(&reg_file);

        for func in &mut lowered.functions {
            allocator.allocate(func);
            Self::expand_parallel_moves(func)?;
            pipeline.optimize_function_post_alloc(func)?;
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
        // Run allocation defensively, but skip the redundant pass when every
        // function has already been register-allocated (no virtual registers
        // remain): re-lowering would only repeat liveness analysis and spill
        // rewriting over physical-register-only code.
        let lowered = if module.functions.iter().all(function_is_register_allocated) {
            module.clone()
        } else {
            self.lower_module(module)?
        };

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

        // 2. Read-only data section (.rodata) for the string pool (with deduplication).
        let mut rodata_symbols: Vec<AdobSymbol> = Vec::new();
        if !lowered.string_pool.is_empty() {
            let mut rodata = Vec::new();
            let mut string_offsets: HashMap<&str, u64> = HashMap::new();
            for (idx, s) in lowered.string_pool.iter().enumerate() {
                let off = if let Some(&existing_off) = string_offsets.get(s.as_str()) {
                    existing_off
                } else {
                    let cur_off = rodata.len() as u64;
                    let s_bytes = s.as_bytes();
                    rodata.extend_from_slice(s_bytes);
                    rodata.push(0); // null terminator
                    string_offsets.insert(s.as_str(), cur_off);
                    cur_off
                };

                let sym = AdobSymbol::new_defined(
                    0,
                    format!("__str_{}", idx),
                    SymbolKind::Object,
                    1, // section 1 (.rodata)
                    off,
                    s.len() as u64 + 1,
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
        let lowered = if module.functions.iter().all(function_is_register_allocated) {
            module.clone()
        } else {
            self.lower_module(module)?
        };
        let conv = self.calling_convention();
        Ok(asm_printer::X86_64AsmPrinter::print_module_with_conv(
            &lowered,
            conv.as_ref(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg(r: u8) -> MachineOperand {
        MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(r)))
    }

    fn slot(offset: i32) -> MachineOperand {
        MachineOperand::StackSlot(offset)
    }

    fn new_ctx(func: &MachineFunction) -> FunctionEncoding {
        FunctionEncoding::new(func, &SystemVX64CallingConvention)
    }

    fn contains_subsequence(haystack: &[u8], needle: &[u8]) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    }

    /// E2: `shl rcx, cl`-style shifts must not destroy the result when the
    /// destination is RCX (the count register). The destination is saved in
    /// R11, shifted there, and moved back into RCX.
    #[test]
    fn shift_by_register_into_rcx_preserves_the_result() {
        let func = MachineFunction::new("shift_rcx");
        let mut ctx = new_ctx(&func);

        // shl rcx, rsi
        assert!(ctx.encode_shift(ShiftOp::Shl, &reg(1), &reg(6)));
        assert_eq!(
            ctx.enc.buffer,
            vec![
                0x49, 0x89, 0xCB, // mov r11, rcx  (save the destination value)
                0x48, 0x89, 0xF1, // mov rcx, rsi  (count -> CL)
                0x49, 0xD3, 0xE3, // shl r11, cl   (shift the saved copy)
                0x4C, 0x89, 0xD9, // mov rcx, r11  (result back into RCX)
            ]
        );
    }

    /// E2: when the destination is the RCX save register (R11) itself, a
    /// different save register (RAX) must be used.
    #[test]
    fn shift_by_register_into_scratch2_uses_another_save_register() {
        let func = MachineFunction::new("shift_r11");
        let mut ctx = new_ctx(&func);

        // shl r11, rsi
        assert!(ctx.encode_shift(ShiftOp::Shl, &reg(SCRATCH2), &reg(6)));
        assert_eq!(
            ctx.enc.buffer,
            vec![
                0x48, 0x89, 0xC8, // mov rax, rcx
                0x48, 0x89, 0xF1, // mov rcx, rsi
                0x49, 0xD3, 0xE3, // shl r11, cl
                0x48, 0x89, 0xC1, // mov rcx, rax
            ]
        );
    }

    /// E2 (regression): an ordinary destination keeps the original
    /// save/shift/restore sequence.
    #[test]
    fn shift_by_register_general_case_unchanged() {
        let func = MachineFunction::new("shift_rdx");
        let mut ctx = new_ctx(&func);

        // shl rdx, rsi
        assert!(ctx.encode_shift(ShiftOp::Shl, &reg(2), &reg(6)));
        assert_eq!(
            ctx.enc.buffer,
            vec![
                0x49, 0x89, 0xCB, // mov r11, rcx
                0x48, 0x89, 0xF1, // mov rcx, rsi
                0x48, 0xD3, 0xE2, // shl rdx, cl
                0x4C, 0x89, 0xD9, // mov rcx, r11
            ]
        );
    }

    /// E5: a spilled (stack-slot) MUL destination round-trips through R10.
    #[test]
    fn spilled_mul_destination_round_trips_through_scratch() {
        let func = MachineFunction::new("spill_mul");
        let mut ctx = new_ctx(&func);

        assert!(ctx.encode_mul(&slot(-8), &reg(1)));
        assert_eq!(
            ctx.enc.buffer,
            vec![
                0x4C, 0x8B, 0x55, 0xF8, // mov r10, [rbp - 8]
                0x4C, 0x0F, 0xAF, 0xD1, // imul r10, rcx
                0x4C, 0x89, 0x55, 0xF8, // mov [rbp - 8], r10
            ]
        );
    }

    /// E5: a spilled DIV destination is loaded first, divided in RAX/RDX, and
    /// the quotient is stored back out of R11.
    #[test]
    fn spilled_div_destination_round_trips_through_scratch() {
        let func = MachineFunction::new("spill_div");
        let mut ctx = new_ctx(&func);

        assert!(ctx.encode_divmod(false, &slot(-16), &reg(7)));
        assert_eq!(
            ctx.enc.buffer,
            vec![
                0x4C, 0x8B, 0x5D, 0xF0, // mov r11, [rbp - 16]  (dividend)
                0x4C, 0x89, 0xD8, // mov rax, r11
                0x48, 0x99, // cqo
                0x48, 0xF7, 0xFF, // idiv rdi
                0x49, 0x89, 0xC3, // mov r11, rax        (quotient)
                0x4C, 0x89, 0x5D, 0xF0, // mov [rbp - 16], r11
            ]
        );
    }

    /// E5: a spilled shift destination is loaded into R10, shifted, stored.
    #[test]
    fn spilled_shift_destination_round_trips_through_scratch() {
        let func = MachineFunction::new("spill_shl");
        let mut ctx = new_ctx(&func);

        assert!(ctx.encode_shift(ShiftOp::Shl, &slot(-8), &reg(6)));
        assert_eq!(
            ctx.enc.buffer,
            vec![
                0x4C, 0x8B, 0x55, 0xF8, // mov r10, [rbp - 8]
                0x49, 0x89, 0xCB, // mov r11, rcx
                0x48, 0x89, 0xF1, // mov rcx, rsi
                0x49, 0xD3, 0xE2, // shl r10, cl
                0x4C, 0x89, 0xD9, // mov rcx, r11
                0x4C, 0x89, 0x55, 0xF8, // mov [rbp - 8], r10
            ]
        );
    }

    /// E5: a spilled FP destination loads into XMM15 and the spilled source
    /// uses XMM14, so the source load cannot clobber the destination.
    #[test]
    fn spilled_fp_binop_uses_distinct_scratch_registers() {
        let func = MachineFunction::new("spill_fadd");
        let mut ctx = new_ctx(&func);

        assert!(ctx.encode_fp_binop(FpOp::Add, &slot(-8), &slot(-16), 8));
        let buf = &ctx.enc.buffer;
        // movsd xmm15, [rbp - 8]
        assert!(contains_subsequence(
            buf,
            &[0xF2, 0x44, 0x0F, 0x10, 0x7D, 0xF8]
        ));
        // movsd xmm14, [rbp - 16]
        assert!(contains_subsequence(
            buf,
            &[0xF2, 0x44, 0x0F, 0x10, 0x75, 0xF0]
        ));
        // addsd xmm15, xmm14
        assert!(contains_subsequence(buf, &[0xF2, 0x45, 0x0F, 0x58, 0xFE]));
        // movsd [rbp - 8], xmm15
        assert!(contains_subsequence(
            buf,
            &[0xF2, 0x44, 0x0F, 0x11, 0x7D, 0xF8]
        ));
    }

    /// E5 end-to-end: MIR whose arithmetic destinations are spill slots (as
    /// produced under register pressure) must compile and encode.
    #[test]
    fn spilled_destination_forms_are_encodable() {
        let target =
            TargetDescriptor::from_triple("x86_64-unknown-linux-gnu").expect("valid triple");
        let mut backend = X86_64Backend::new(target);

        let mut func = MachineFunction::new("spilled_math");
        func.stack_size = 64;
        let block = func.entry_block_mut();
        block.push(MachineInstruction::Move {
            dst: slot(-8),
            src: MachineOperand::Immediate(100),
        });
        block.push(MachineInstruction::Mul {
            dst: slot(-8),
            src: reg(1),
        });
        block.push(MachineInstruction::Shl {
            dst: slot(-8),
            src: reg(6),
        });
        block.push(MachineInstruction::Div {
            dst: slot(-16),
            src: reg(7),
        });
        block.push(MachineInstruction::Mod {
            dst: slot(-16),
            src: reg(7),
        });
        block.push(MachineInstruction::FAdd {
            dst: slot(-24),
            src: reg(16),
            size: 8,
        });
        block.push(MachineInstruction::Return);

        let code = backend
            .generate_function(&func)
            .expect("spilled destinations must encode");
        assert!(!code.is_empty());
        // Ends with the epilogue `pop rbp; ret`.
        assert_eq!(&code[code.len() - 2..], &[0x5D, 0xC3]);
    }

    /// Item 14: small non-negative immediates use the 5-byte
    /// `mov r32, imm32` form; only values outside u32 keep the 10-byte form.
    #[test]
    fn small_immediates_use_the_32_bit_move_form() {
        let func = MachineFunction::new("imm_forms");
        let mut ctx = new_ctx(&func);

        ctx.encode_move(&reg(0), &MachineOperand::Immediate(7));
        assert_eq!(ctx.enc.buffer, vec![0xB8, 0x07, 0x00, 0x00, 0x00]);
        ctx.enc.buffer.clear();

        // 0xFFFF_FFFF fits in u32 and zero-extends identically.
        ctx.encode_move(&reg(0), &MachineOperand::Immediate(0xFFFF_FFFF));
        assert_eq!(ctx.enc.buffer, vec![0xB8, 0xFF, 0xFF, 0xFF, 0xFF]);
        ctx.enc.buffer.clear();

        // -1 must keep the full sign-extended 64-bit form.
        ctx.encode_move(&reg(0), &MachineOperand::Immediate(-1));
        assert_eq!(
            ctx.enc.buffer,
            vec![0x48, 0xB8, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]
        );
    }

    /// Item 16: a zero materialization uses `xor r, r` (3 bytes) when no
    /// condition test is pending.
    #[test]
    fn zero_immediate_uses_xor_idiom() {
        let func = MachineFunction::new("zero_idiom");
        let mut ctx = new_ctx(&func);

        ctx.encode_move(&reg(2), &MachineOperand::Immediate(0));
        assert_eq!(ctx.enc.buffer, vec![0x48, 0x31, 0xD2]); // xor rdx, rdx
    }

    /// Item 15: `add r, imm8` uses the short 0x83 form.
    #[test]
    fn binop_immediate_uses_imm8_form_when_it_fits() {
        let func = MachineFunction::new("imm8_binop");
        let mut ctx = new_ctx(&func);

        ctx.encode_binop(BinOp::Add, &reg(2), &MachineOperand::Immediate(5));
        assert_eq!(ctx.enc.buffer, vec![0x48, 0x83, 0xC2, 0x05]); // add rdx, 5

        ctx.enc.buffer.clear();
        ctx.encode_binop(BinOp::Add, &reg(2), &MachineOperand::Immediate(1000));
        assert_eq!(
            ctx.enc.buffer,
            vec![0x48, 0x81, 0xC2, 0xE8, 0x03, 0x00, 0x00] // add rdx, 1000
        );
    }
}
