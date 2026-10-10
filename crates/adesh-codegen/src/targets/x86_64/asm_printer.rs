//! x86-64 Assembly Printer (Intel syntax / GNU as compatible).
//!
//! The printed text deliberately mirrors what `X86_64Encoder` emits so that
//! assembling the output with GNU as gives semantics equivalent to
//! `emit_object`:
//! - the same frame (prologue/epilogue save area) via the shared
//!   `compute_frame_layout`,
//! - the same operand-size handling (`movzx` byte/word loads, `dword` moves,
//!   `movss`/`movsd` for XMM operands),
//! - the same RAX/RDX marshalling around DIV/IDIV,
//! - the same RCX-preserving shift-by-register sequences,
//! - the same symbol (`movabs reg, OFFSET sym`) and immediate
//!   (`movabs reg, 0x<bits>`) materialisation,
//! - `setcc` printing the byte register plus the zero-extending `movzx` that
//!   the encoder pairs it with.

use crate::calling_convention::{CallingConvention, SystemVX64CallingConvention};
use crate::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister,
};

use super::{FrameLayout, compute_frame_layout};

/// Encoder scratch registers reproduced in the textual output.
const SCRATCH: u8 = 10;
const SCRATCH2: u8 = 11;
const FP_SCRATCH: u8 = 15;
const FP_SCRATCH2: u8 = 14;

pub struct X86_64AsmPrinter;

impl X86_64AsmPrinter {
    pub fn reg_name(reg: PhysicalRegister) -> &'static str {
        match reg.0 {
            0 => "rax",
            1 => "rcx",
            2 => "rdx",
            3 => "rbx",
            4 => "rsp",
            5 => "rbp",
            6 => "rsi",
            7 => "rdi",
            8 => "r8",
            9 => "r9",
            10 => "r10",
            11 => "r11",
            12 => "r12",
            13 => "r13",
            14 => "r14",
            15 => "r15",
            16 => "xmm0",
            17 => "xmm1",
            18 => "xmm2",
            19 => "xmm3",
            20 => "xmm4",
            21 => "xmm5",
            22 => "xmm6",
            23 => "xmm7",
            24 => "xmm8",
            25 => "xmm9",
            26 => "xmm10",
            27 => "xmm11",
            28 => "xmm12",
            29 => "xmm13",
            30 => "xmm14",
            31 => "xmm15",
            _ => "unknown_reg",
        }
    }

    /// Register name for a given operand width (byte/word/dword/qword).
    fn gpr_name(reg: u8, size: u8) -> &'static str {
        const NAMES_8: [&str; 16] = [
            "al", "cl", "dl", "bl", "spl", "bpl", "sil", "dil", "r8b", "r9b", "r10b", "r11b",
            "r12b", "r13b", "r14b", "r15b",
        ];
        const NAMES_16: [&str; 16] = [
            "ax", "cx", "dx", "bx", "sp", "bp", "si", "di", "r8w", "r9w", "r10w", "r11w", "r12w",
            "r13w", "r14w", "r15w",
        ];
        const NAMES_32: [&str; 16] = [
            "eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi", "r8d", "r9d", "r10d", "r11d",
            "r12d", "r13d", "r14d", "r15d",
        ];
        let idx = (reg & 15) as usize;
        match size {
            1 => NAMES_8[idx],
            2 => NAMES_16[idx],
            4 => NAMES_32[idx],
            _ => Self::reg_name(PhysicalRegister(reg)),
        }
    }

    fn size_name(size: u8) -> &'static str {
        match size {
            1 => "byte",
            2 => "word",
            4 => "dword",
            _ => "qword",
        }
    }

    fn is_mem(op: &MachineOperand) -> bool {
        matches!(
            op,
            MachineOperand::Memory { .. } | MachineOperand::StackSlot(_)
        )
    }

    /// Format an operand with an explicit size prefix when it is memory.
    fn format_sized(op: &MachineOperand, size: u8) -> String {
        if Self::is_mem(op) {
            format!("{} ptr {}", Self::size_name(size), Self::format_operand(op))
        } else {
            Self::format_operand(op)
        }
    }

    pub fn format_operand(op: &MachineOperand) -> String {
        match op {
            MachineOperand::Register(MachineRegister::Physical(p)) => {
                Self::reg_name(*p).to_string()
            }
            MachineOperand::Register(MachineRegister::Virtual(v)) => format!("vreg{}", v.0),
            MachineOperand::Immediate(n) => format!("{}", n),
            MachineOperand::FloatImmediate(f) => format!("{}", f),
            MachineOperand::StackSlot(slot) => {
                if *slot >= 0 {
                    format!("[rbp + {}]", slot)
                } else {
                    format!("[rbp - {}]", -slot)
                }
            }
            MachineOperand::Memory {
                base,
                offset,
                index,
            } => {
                let base_str = match base {
                    MachineRegister::Physical(p) => Self::reg_name(*p),
                    MachineRegister::Virtual(_v) => "vreg",
                };
                if let Some((idx_reg, scale)) = index {
                    let idx_str = match idx_reg {
                        MachineRegister::Physical(p) => Self::reg_name(*p),
                        MachineRegister::Virtual(_) => "vreg",
                    };
                    if *offset >= 0 {
                        format!("[{} + {} * {} + {}]", base_str, idx_str, scale, offset)
                    } else {
                        format!("[{} + {} * {} - {}]", base_str, idx_str, scale, -offset)
                    }
                } else if *offset == 0 {
                    format!("[{}]", base_str)
                } else if *offset > 0 {
                    format!("[{} + {}]", base_str, offset)
                } else {
                    format!("[{} - {}]", base_str, -offset)
                }
            }
            MachineOperand::Label(lbl) => format!(".L_{}", lbl),
            MachineOperand::Symbol(sym) => sym.clone(),
        }
    }

    pub fn cc_suffix(cc: ConditionCode) -> &'static str {
        match cc {
            ConditionCode::Equal | ConditionCode::Zero => "e",
            ConditionCode::NotEqual | ConditionCode::NotZero => "ne",
            ConditionCode::LessThan => "l",
            ConditionCode::LessOrEqual => "le",
            ConditionCode::GreaterThan => "g",
            ConditionCode::GreaterOrEqual => "ge",
            ConditionCode::Below => "b",
            ConditionCode::BelowOrEqual => "be",
            ConditionCode::Above => "a",
            ConditionCode::AboveOrEqual => "ae",
            ConditionCode::Parity => "p",
            ConditionCode::NotParity => "np",
        }
    }

    pub fn print_function(func: &MachineFunction) -> String {
        Self::print_function_with_conv(func, &SystemVX64CallingConvention)
    }

    pub fn print_function_with_conv(
        func: &MachineFunction,
        conv: &dyn CallingConvention,
    ) -> String {
        // Expand parallel moves exactly like the encoder does so the textual
        // output contains no pseudo instruction.
        let mut expanded = func.clone();
        let _ = super::X86_64Backend::expand_parallel_moves(&mut expanded);

        let layout = compute_frame_layout(&expanded, conv);
        let mut printer = FuncPrinter {
            out: String::new(),
            layout,
            label_seq: 0,
        };
        printer.print(&expanded);
        printer.out
    }

    pub fn print_module(module: &NativeModule) -> String {
        Self::print_module_with_conv(module, &SystemVX64CallingConvention)
    }

    pub fn print_module_with_conv(module: &NativeModule, conv: &dyn CallingConvention) -> String {
        let mut out = String::new();
        out.push_str("# ------------------------------------------------------------------\n");
        out.push_str(&format!(
            "# Adesh Generated x86_64 Assembly: {}\n",
            module.name
        ));
        out.push_str("# Zero-dependency Native Machine Code Generator\n");
        out.push_str("# ------------------------------------------------------------------\n");
        out.push_str("    .intel_syntax noprefix\n\n");

        // Read-only data section for strings
        if !module.string_pool.is_empty() {
            out.push_str("    .section .rodata\n");
            out.push_str("    .align 8\n");
            for (idx, s) in module.string_pool.iter().enumerate() {
                out.push_str(&format!("__str_{}:\n", idx));
                out.push_str(&format!("    .asciz \"{}\"\n", s.escape_debug()));
            }
            out.push('\n');
        }

        // Text section
        out.push_str("    .section .text\n");
        out.push_str("    .align 16\n\n");

        for func in &module.functions {
            out.push_str(&Self::print_function_with_conv(func, conv));
        }

        out
    }
}

/// Per-function textual emitter.
struct FuncPrinter {
    out: String,
    layout: FrameLayout,
    label_seq: usize,
}

impl FuncPrinter {
    fn line(&mut self, text: &str) {
        self.out.push_str("    ");
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn comment(&mut self, text: &str) {
        self.out.push_str("    # ");
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn fresh_label(&mut self, prefix: &str) -> String {
        self.label_seq += 1;
        format!(".L_{}_{}", prefix, self.label_seq)
    }

    fn rname(reg: u8) -> &'static str {
        X86_64AsmPrinter::reg_name(PhysicalRegister(reg))
    }

    fn xname(reg: u8) -> &'static str {
        X86_64AsmPrinter::reg_name(PhysicalRegister(reg + 16))
    }

    fn print(&mut self, func: &MachineFunction) {
        if func.is_exported {
            self.out.push_str(&format!("    .globl {}\n", func.name));
            self.out
                .push_str(&format!("    .type {}, @function\n", func.name));
        }
        self.out.push_str(&format!("{}:\n", func.name));

        self.prologue();

        let mut saw_return = false;
        for block in &func.blocks {
            if block.label != "entry" {
                self.out.push_str(&format!(".L_{}:\n", block.label));
            }
            for inst in &block.instructions {
                if matches!(inst, MachineInstruction::Return)
                    || matches!(inst, MachineInstruction::Custom { name, .. } if name == "tail_jmp")
                {
                    saw_return = true;
                }
                self.print_inst(inst);
            }
        }

        if !saw_return {
            self.epilogue();
        }

        self.out
            .push_str(&format!("    .size {}, .-{}\n\n", func.name, func.name));
    }

    fn prologue(&mut self) {
        self.line("push rbp");
        self.line("mov rbp, rsp");
        if self.layout.frame_size > 0 {
            self.line(&format!("sub rsp, {}", self.layout.frame_size));
        }
        let callee_saved = self.layout.used_callee_saved.clone();
        for reg in callee_saved {
            let slot = self.layout.callee_slots[&reg];
            let mem = X86_64AsmPrinter::format_operand(&MachineOperand::StackSlot(slot));
            if reg < 16 {
                self.line(&format!("mov {}, {}", mem, Self::rname(reg)));
            } else {
                self.line(&format!("movsd {}, {}", mem, Self::rname(reg)));
            }
        }
    }

    fn epilogue(&mut self) {
        self.frame_teardown();
        self.line("ret");
    }

    fn frame_teardown(&mut self) {
        let callee_saved = self.layout.used_callee_saved.clone();
        for reg in callee_saved {
            let slot = self.layout.callee_slots[&reg];
            let mem = X86_64AsmPrinter::format_operand(&MachineOperand::StackSlot(slot));
            if reg < 16 {
                self.line(&format!("mov {}, {}", Self::rname(reg), mem));
            } else {
                self.line(&format!("movsd {}, {}", Self::rname(reg), mem));
            }
        }
        self.line("mov rsp, rbp");
        self.line("pop rbp");
    }

    /// Materialize a 64-bit constant into `reg`, mirroring the encoder's
    /// `mov r32, imm32` / `mov r64, imm64` selection.
    fn materialize_imm(&mut self, reg: u8, value: i64) {
        if value >= 0 && value <= u32::MAX as i64 {
            self.line(&format!("mov {}, {}", Self::rname(reg), value));
        } else {
            self.line(&format!(
                "movabs {}, {}  # 0x{:X}",
                Self::rname(reg),
                value,
                value as u64
            ));
        }
    }

    /// Materialize an IEEE-754 bit pattern (or integer value) into a GPR.
    fn materialize_bits(&mut self, reg: u8, bits: i64, decimal: Option<f64>) {
        match decimal {
            Some(d) => self.line(&format!(
                "movabs {}, 0x{:X}  # f64 {}",
                Self::rname(reg),
                bits as u64,
                d
            )),
            None => {
                if bits >= 0 && bits <= u32::MAX as i64 {
                    self.line(&format!("mov {}, {}", Self::rname(reg), bits));
                } else {
                    self.line(&format!(
                        "movabs {}, 0x{:X}  # {}",
                        Self::rname(reg),
                        bits as u64,
                        bits
                    ));
                }
            }
        }
    }

    fn print_inst(&mut self, inst: &MachineInstruction) {
        match inst {
            MachineInstruction::Nop => self.line("nop"),
            MachineInstruction::Return => self.epilogue(),
            MachineInstruction::Move { dst, src } => self.print_move(dst, src),
            MachineInstruction::Load { dst, src, size } => self.print_load(dst, src, *size),
            MachineInstruction::Store { dst, src, size } => self.print_store(dst, src, *size),
            MachineInstruction::Add { dst, src } => self.print_int_binop("add", dst, src),
            MachineInstruction::Sub { dst, src } => self.print_int_binop("sub", dst, src),
            MachineInstruction::And { dst, src } => self.print_int_binop("and", dst, src),
            MachineInstruction::Or { dst, src } => self.print_int_binop("or", dst, src),
            MachineInstruction::Xor { dst, src } => self.print_int_binop("xor", dst, src),
            MachineInstruction::Compare { lhs, rhs } => self.print_int_binop("cmp", lhs, rhs),
            MachineInstruction::Test { lhs, rhs } => self.print_int_binop("test", lhs, rhs),
            MachineInstruction::Mul { dst, src } => self.print_int_binop("imul", dst, src),
            MachineInstruction::Div { dst, src } => self.print_divmod(dst, src, false),
            MachineInstruction::Mod { dst, src } => self.print_divmod(dst, src, true),
            MachineInstruction::Neg { dst } => {
                self.line(&format!("neg {}", X86_64AsmPrinter::format_operand(dst)))
            }
            MachineInstruction::Not { dst } => {
                self.line(&format!("not {}", X86_64AsmPrinter::format_operand(dst)))
            }
            MachineInstruction::Shl { dst, src } => self.print_shift("shl", dst, src),
            MachineInstruction::Shr { dst, src } => self.print_shift("shr", dst, src),
            MachineInstruction::Sar { dst, src } => self.print_shift("sar", dst, src),
            MachineInstruction::SetCc { dst, cc } => self.print_setcc(*cc, dst),
            MachineInstruction::Branch { target } => {
                self.line(&format!("jmp .L_{}", target));
            }
            MachineInstruction::BranchCc { cc, target } => {
                self.line(&format!(
                    "j{} .L_{}",
                    X86_64AsmPrinter::cc_suffix(*cc),
                    target
                ));
            }
            MachineInstruction::Call { target, .. } => {
                self.line(&format!(
                    "call {}",
                    X86_64AsmPrinter::format_operand(target)
                ));
            }
            MachineInstruction::Push { src } => {
                self.line(&format!("push {}", X86_64AsmPrinter::format_operand(src)))
            }
            MachineInstruction::Pop { dst } => {
                self.line(&format!("pop {}", X86_64AsmPrinter::format_operand(dst)))
            }
            MachineInstruction::Barrier => self.line("mfence"),
            MachineInstruction::FAdd { dst, src, size } => {
                let op = if *size == 8 { "addsd" } else { "addss" };
                self.print_fp_binop(op, dst, src, *size);
            }
            MachineInstruction::FSub { dst, src, size } => {
                let op = if *size == 8 { "subsd" } else { "subss" };
                self.print_fp_binop(op, dst, src, *size);
            }
            MachineInstruction::FMul { dst, src, size } => {
                let op = if *size == 8 { "mulsd" } else { "mulss" };
                self.print_fp_binop(op, dst, src, *size);
            }
            MachineInstruction::FDiv { dst, src, size } => {
                let op = if *size == 8 { "divsd" } else { "divss" };
                self.print_fp_binop(op, dst, src, *size);
            }
            MachineInstruction::FNeg { dst, size } => self.print_fneg(dst, *size),
            MachineInstruction::FCmp { lhs, rhs, size } => {
                let op = if *size == 8 { "ucomisd" } else { "ucomiss" };
                self.line(&format!(
                    "{} {}, {}",
                    op,
                    X86_64AsmPrinter::format_operand(lhs),
                    X86_64AsmPrinter::format_operand(rhs)
                ));
            }
            MachineInstruction::FCvtIntToFloat {
                dst,
                src,
                is_f64,
                is_signed,
            } => self.print_fcvt_int_to_float(dst, src, *is_f64, *is_signed),
            MachineInstruction::FCvtFloatToInt {
                dst,
                src,
                is_f64,
                is_signed,
            } => self.print_fcvt_float_to_int(dst, src, *is_f64, *is_signed),
            MachineInstruction::FCvtFloatToFloat { dst, src, to_f64 } => {
                let op = if *to_f64 { "cvtss2sd" } else { "cvtsd2ss" };
                self.line(&format!(
                    "{} {}, {}",
                    op,
                    X86_64AsmPrinter::format_operand(dst),
                    X86_64AsmPrinter::format_operand(src)
                ));
            }
            MachineInstruction::VectorAdd { dst, src, vec_type } => {
                let op = if vec_type.element_type.is_floating_point() {
                    "addps"
                } else {
                    "paddd"
                };
                self.line(&format!(
                    "{} {}, {}",
                    op,
                    X86_64AsmPrinter::format_operand(dst),
                    X86_64AsmPrinter::format_operand(src)
                ));
            }
            MachineInstruction::VectorSub { dst, src, vec_type } => {
                let op = if vec_type.element_type.is_floating_point() {
                    "subps"
                } else {
                    "psubd"
                };
                self.line(&format!(
                    "{} {}, {}",
                    op,
                    X86_64AsmPrinter::format_operand(dst),
                    X86_64AsmPrinter::format_operand(src)
                ));
            }
            MachineInstruction::VectorMul { dst, src, vec_type } => {
                let op = if vec_type.element_type.is_floating_point() {
                    "mulps"
                } else {
                    "pmulld"
                };
                self.line(&format!(
                    "{} {}, {}",
                    op,
                    X86_64AsmPrinter::format_operand(dst),
                    X86_64AsmPrinter::format_operand(src)
                ));
            }
            MachineInstruction::VectorDiv { dst, src, .. } => {
                self.line(&format!(
                    "divps {}, {}",
                    X86_64AsmPrinter::format_operand(dst),
                    X86_64AsmPrinter::format_operand(src)
                ));
            }
            MachineInstruction::VectorAnd { dst, src, .. } => {
                self.line(&format!(
                    "andps {}, {}",
                    X86_64AsmPrinter::format_operand(dst),
                    X86_64AsmPrinter::format_operand(src)
                ));
            }
            MachineInstruction::VectorOr { dst, src, .. } => {
                self.line(&format!(
                    "orps {}, {}",
                    X86_64AsmPrinter::format_operand(dst),
                    X86_64AsmPrinter::format_operand(src)
                ));
            }
            MachineInstruction::VectorXor { dst, src, .. } => {
                self.line(&format!(
                    "xorps {}, {}",
                    X86_64AsmPrinter::format_operand(dst),
                    X86_64AsmPrinter::format_operand(src)
                ));
            }
            MachineInstruction::VectorLoad { dst, src, .. } => {
                self.line(&format!(
                    "movups {}, {}",
                    X86_64AsmPrinter::format_operand(dst),
                    X86_64AsmPrinter::format_operand(src)
                ));
            }
            MachineInstruction::VectorStore { dst, src, .. } => {
                self.line(&format!(
                    "movups {}, {}",
                    X86_64AsmPrinter::format_operand(dst),
                    X86_64AsmPrinter::format_operand(src)
                ));
            }
            MachineInstruction::VectorBroadcast { dst, src, .. } => {
                self.line(&format!(
                    "shufps {}, {}, 0",
                    X86_64AsmPrinter::format_operand(dst),
                    X86_64AsmPrinter::format_operand(src)
                ));
            }
            MachineInstruction::VectorShuffle { dst, src, mask, .. } => {
                self.line(&format!(
                    "shufps {}, {}, {}",
                    X86_64AsmPrinter::format_operand(dst),
                    X86_64AsmPrinter::format_operand(src),
                    mask
                ));
            }
            MachineInstruction::VectorReduceAdd { dst, src, .. } => {
                self.line(&format!(
                    "addps {}, {}",
                    X86_64AsmPrinter::format_operand(dst),
                    X86_64AsmPrinter::format_operand(src)
                ));
            }
            MachineInstruction::VectorMin { dst, src, .. } => {
                self.line(&format!(
                    "minps {}, {}",
                    X86_64AsmPrinter::format_operand(dst),
                    X86_64AsmPrinter::format_operand(src)
                ));
            }
            MachineInstruction::VectorMax { dst, src, .. } => {
                self.line(&format!(
                    "maxps {}, {}",
                    X86_64AsmPrinter::format_operand(dst),
                    X86_64AsmPrinter::format_operand(src)
                ));
            }
            MachineInstruction::VectorCmp { dst, src, .. } => {
                self.line(&format!(
                    "cmpps {}, {}",
                    X86_64AsmPrinter::format_operand(dst),
                    X86_64AsmPrinter::format_operand(src)
                ));
            }
            MachineInstruction::VectorBlend { dst, src, mask, .. } => {
                self.line(&format!(
                    "blendps {}, {}, {}",
                    X86_64AsmPrinter::format_operand(dst),
                    X86_64AsmPrinter::format_operand(src),
                    mask
                ));
            }
            MachineInstruction::VectorShiftLeft { dst, count, .. } => {
                self.line(&format!(
                    "pslld {}, {}",
                    X86_64AsmPrinter::format_operand(dst),
                    count
                ));
            }
            MachineInstruction::VectorShiftRight { dst, count, .. } => {
                self.line(&format!(
                    "psrld {}, {}",
                    X86_64AsmPrinter::format_operand(dst),
                    count
                ));
            }
            MachineInstruction::AtomicLoad { dst, src, .. } => {
                self.line(&format!(
                    "mov {}, {}",
                    X86_64AsmPrinter::format_operand(dst),
                    X86_64AsmPrinter::format_operand(src)
                ));
            }
            MachineInstruction::AtomicStore { dst, src, .. } => {
                self.line(&format!(
                    "mov {}, {}",
                    X86_64AsmPrinter::format_operand(dst),
                    X86_64AsmPrinter::format_operand(src)
                ));
            }
            MachineInstruction::AtomicFetchAdd { dst, src, .. } => {
                self.line(&format!(
                    "lock xadd {}, {}",
                    X86_64AsmPrinter::format_operand(dst),
                    X86_64AsmPrinter::format_operand(src)
                ));
            }
            MachineInstruction::AtomicExchange { dst, src, .. } => {
                self.line(&format!(
                    "lock xchg {}, {}",
                    X86_64AsmPrinter::format_operand(dst),
                    X86_64AsmPrinter::format_operand(src)
                ));
            }
            MachineInstruction::AtomicCompareExchange { dst, desired, .. } => {
                self.line(&format!(
                    "lock cmpxchg {}, {}",
                    X86_64AsmPrinter::format_operand(dst),
                    X86_64AsmPrinter::format_operand(desired)
                ));
            }
            MachineInstruction::ParallelMove { .. } => {
                self.comment("unexpanded parallel move (no textual equivalent)");
            }
            MachineInstruction::Custom { name, .. } => {
                if name == "endbr64" {
                    self.line("endbr64");
                } else if name == "tail_jmp" {
                    self.frame_teardown();
                    if let MachineInstruction::Custom { operands, .. } = inst
                        && let Some(MachineOperand::Symbol(target)) = operands.first()
                    {
                        self.line(&format!("jmp {}", target));
                    }
                } else {
                    self.comment(&format!(
                        "custom instruction `{}` is rejected by the native encoder",
                        name
                    ));
                }
            }
            MachineInstruction::TlsAddress { dst, symbol } => {
                self.line(&format!(
                    "# tls address of {} into {}",
                    symbol,
                    X86_64AsmPrinter::format_operand(dst)
                ));
            }
        }
    }

    fn print_move(&mut self, dst: &MachineOperand, src: &MachineOperand) {
        let dst_reg = match dst {
            MachineOperand::Register(MachineRegister::Physical(p)) => Some(p.0),
            _ => None,
        };

        match dst_reg {
            // XMM destination.
            Some(d) if d >= 16 => {
                let xd = Self::xname(d - 16);
                match src {
                    MachineOperand::Register(MachineRegister::Physical(s)) if s.0 >= 16 => {
                        self.line(&format!("movsd {}, {}", xd, Self::xname(s.0 - 16)));
                    }
                    MachineOperand::Register(MachineRegister::Physical(s)) => {
                        self.line(&format!("movq {}, {}", xd, Self::rname(s.0)));
                    }
                    MachineOperand::FloatImmediate(f) => {
                        self.materialize_bits(SCRATCH, f.to_bits() as i64, Some(*f));
                        self.line(&format!("movq {}, {}", xd, Self::rname(SCRATCH)));
                    }
                    MachineOperand::Immediate(v) => {
                        self.materialize_bits(SCRATCH, *v, None);
                        self.line(&format!("movq {}, {}", xd, Self::rname(SCRATCH)));
                    }
                    other => {
                        self.line(&format!(
                            "movsd {}, {}",
                            xd,
                            X86_64AsmPrinter::format_operand(other)
                        ));
                    }
                }
            }
            // GPR destination.
            Some(d) => match src {
                MachineOperand::Register(MachineRegister::Physical(s)) if s.0 >= 16 => {
                    self.line(&format!(
                        "movq {}, {}",
                        Self::rname(d),
                        Self::xname(s.0 - 16)
                    ));
                }
                MachineOperand::Register(MachineRegister::Physical(s)) => {
                    self.line(&format!("mov {}, {}", Self::rname(d), Self::rname(s.0)));
                }
                MachineOperand::Immediate(v) => {
                    // A plain flag-preserving move; the encoder may use the
                    // `xor r, r` idiom, which is value-equivalent.
                    self.materialize_imm(d, *v);
                }
                MachineOperand::FloatImmediate(f) => {
                    self.materialize_bits(d, f.to_bits() as i64, Some(*f));
                }
                MachineOperand::Symbol(name) => {
                    // Absolute 64-bit symbol reference (encoder: mov imm64 +
                    // ABS64 relocation).
                    self.line(&format!("movabs {}, OFFSET {}", Self::rname(d), name));
                }
                other => {
                    self.line(&format!(
                        "mov {}, {}",
                        Self::rname(d),
                        X86_64AsmPrinter::format_operand(other)
                    ));
                }
            },
            // Memory / stack destination.
            None => {
                let mem = X86_64AsmPrinter::format_operand(dst);
                match src {
                    MachineOperand::Register(MachineRegister::Physical(s)) if s.0 >= 16 => {
                        self.line(&format!("movsd {}, {}", mem, Self::xname(s.0 - 16)));
                    }
                    MachineOperand::Register(MachineRegister::Physical(s)) => {
                        self.line(&format!("mov {}, {}", mem, Self::rname(s.0)));
                    }
                    MachineOperand::FloatImmediate(f) => {
                        self.materialize_bits(SCRATCH, f.to_bits() as i64, Some(*f));
                        self.line(&format!("mov {}, {}", mem, Self::rname(SCRATCH)));
                    }
                    MachineOperand::Immediate(v) => {
                        self.materialize_bits(SCRATCH, *v, None);
                        self.line(&format!("mov {}, {}", mem, Self::rname(SCRATCH)));
                    }
                    MachineOperand::Symbol(name) => {
                        self.line(&format!("movabs {}, OFFSET {}", Self::rname(SCRATCH), name));
                        self.line(&format!("mov {}, {}", mem, Self::rname(SCRATCH)));
                    }
                    other if X86_64AsmPrinter::is_mem(other) => {
                        self.line(&format!(
                            "mov {}, {}",
                            Self::rname(SCRATCH),
                            X86_64AsmPrinter::format_operand(other)
                        ));
                        self.line(&format!("mov {}, {}", mem, Self::rname(SCRATCH)));
                    }
                    other => {
                        self.line(&format!(
                            "mov {}, {}",
                            mem,
                            X86_64AsmPrinter::format_operand(other)
                        ));
                    }
                }
            }
        }
    }

    fn print_load(&mut self, dst: &MachineOperand, src: &MachineOperand, size: u8) {
        let dst_reg = match dst {
            MachineOperand::Register(MachineRegister::Physical(p)) => Some(p.0),
            _ => None,
        };
        let src_str = X86_64AsmPrinter::format_sized(src, size);
        match dst_reg {
            Some(d) if d >= 16 => {
                let op = if size == 4 { "movss" } else { "movsd" };
                self.line(&format!("{} {}, {}", op, Self::xname(d - 16), src_str));
            }
            Some(d) => match size {
                1 => self.line(&format!(
                    "movzx {}, byte ptr {}",
                    Self::rname(d),
                    X86_64AsmPrinter::format_operand(src)
                )),
                2 => self.line(&format!(
                    "movzx {}, word ptr {}",
                    Self::rname(d),
                    X86_64AsmPrinter::format_operand(src)
                )),
                4 => self.line(&format!(
                    "mov {}, dword ptr {}",
                    X86_64AsmPrinter::gpr_name(d, 4),
                    X86_64AsmPrinter::format_operand(src)
                )),
                _ => self.line(&format!("mov {}, {}", Self::rname(d), src_str)),
            },
            None => {
                self.line(&format!("mov {}, {}", Self::rname(SCRATCH), src_str));
                self.line(&format!(
                    "mov {}, {}",
                    X86_64AsmPrinter::format_operand(dst),
                    Self::rname(SCRATCH)
                ));
            }
        }
    }

    fn print_store(&mut self, dst: &MachineOperand, src: &MachineOperand, size: u8) {
        let dst_str = X86_64AsmPrinter::format_sized(dst, size);
        match src {
            MachineOperand::Register(MachineRegister::Physical(s)) if s.0 >= 16 => {
                let op = if size == 4 { "movss" } else { "movsd" };
                self.line(&format!("{} {}, {}", op, dst_str, Self::xname(s.0 - 16)));
            }
            MachineOperand::Register(MachineRegister::Physical(s)) => {
                self.line(&format!(
                    "mov {}, {}",
                    dst_str,
                    X86_64AsmPrinter::gpr_name(s.0, size)
                ));
            }
            MachineOperand::Immediate(v) => {
                self.materialize_bits(SCRATCH, *v, None);
                self.line(&format!("mov {}, {}", dst_str, Self::rname(SCRATCH)));
            }
            MachineOperand::FloatImmediate(f) => {
                self.materialize_bits(SCRATCH, f.to_bits() as i64, Some(*f));
                self.line(&format!("mov {}, {}", dst_str, Self::rname(SCRATCH)));
            }
            other if X86_64AsmPrinter::is_mem(other) => {
                self.line(&format!(
                    "mov {}, {}",
                    Self::rname(SCRATCH),
                    X86_64AsmPrinter::format_sized(other, size)
                ));
                self.line(&format!("mov {}, {}", dst_str, Self::rname(SCRATCH)));
            }
            other => {
                self.line(&format!(
                    "mov {}, {}",
                    dst_str,
                    X86_64AsmPrinter::format_operand(other)
                ));
            }
        }
    }

    /// Integer two-operand operation. A 64-bit immediate that does not fit in
    /// `imm32` is materialized into the scratch register first, exactly like
    /// the encoder.
    fn print_int_binop(&mut self, op: &str, dst: &MachineOperand, src: &MachineOperand) {
        let dst_str = X86_64AsmPrinter::format_operand(dst);
        match src {
            MachineOperand::Immediate(v) if (*v < i32::MIN as i64 || *v > i32::MAX as i64) => {
                self.materialize_imm(SCRATCH, *v);
                self.line(&format!("{} {}, {}", op, dst_str, Self::rname(SCRATCH)));
            }
            MachineOperand::Symbol(name) => {
                self.line(&format!("movabs {}, OFFSET {}", Self::rname(SCRATCH), name));
                self.line(&format!("{} {}, {}", op, dst_str, Self::rname(SCRATCH)));
            }
            other => {
                self.line(&format!(
                    "{} {}, {}",
                    op,
                    dst_str,
                    X86_64AsmPrinter::format_operand(other)
                ));
            }
        }
    }

    /// DIV/IDIV: mirror the encoder's RAX/RDX marshalling.
    fn print_divmod(&mut self, dst: &MachineOperand, src: &MachineOperand, is_mod: bool) {
        let dst_str = X86_64AsmPrinter::format_operand(dst);
        if dst_str != "rax" {
            self.line(&format!("mov rax, {}", dst_str));
        }
        // The divisor must survive CQO (which clobbers RDX) and cannot be RAX,
        // so it is loaded into the scratch register unless it is another
        // ordinary GPR.
        match src {
            MachineOperand::Register(MachineRegister::Physical(s)) if s.0 != 0 && s.0 != 2 => {
                self.line("cqo");
                self.line(&format!("idiv {}", Self::rname(s.0)));
            }
            other => {
                self.line(&format!(
                    "mov {}, {}",
                    Self::rname(SCRATCH),
                    X86_64AsmPrinter::format_operand(other)
                ));
                self.line("cqo");
                self.line(&format!("idiv {}", Self::rname(SCRATCH)));
            }
        }
        if is_mod {
            self.line(&format!("mov {}, rdx", dst_str));
        } else {
            self.line(&format!("mov {}, rax", dst_str));
        }
    }

    /// Shift, reproducing the encoder's RCX-preserving sequences for
    /// register-held counts.
    fn print_shift(&mut self, op: &str, dst: &MachineOperand, src: &MachineOperand) {
        let dst_str = X86_64AsmPrinter::format_operand(dst);
        let dst_reg = match dst {
            MachineOperand::Register(MachineRegister::Physical(p)) => Some(p.0),
            _ => None,
        };
        match src {
            MachineOperand::Immediate(v) => {
                let count = (*v as u64 & 63) as u8;
                self.line(&format!("{} {}, {}", op, dst_str, count));
            }
            MachineOperand::Register(MachineRegister::Physical(s)) if s.0 == 1 => {
                self.line(&format!("{} {}, cl", op, dst_str));
            }
            MachineOperand::Register(MachineRegister::Physical(s)) => {
                let d = dst_reg.unwrap_or(SCRATCH);
                self.emit_shift_by_reg(op, d, s.0, &dst_str);
            }
            MachineOperand::StackSlot(slot) => {
                // Load the count into R11 first (mirrors the encoder).
                self.line(&format!(
                    "mov {}, {}",
                    Self::rname(SCRATCH2),
                    X86_64AsmPrinter::format_operand(&MachineOperand::StackSlot(*slot))
                ));
                let d = dst_reg.unwrap_or(SCRATCH);
                self.emit_shift_by_reg(op, d, SCRATCH2, &dst_str);
            }
            other => {
                self.line(&format!(
                    "{} {}, {}",
                    op,
                    dst_str,
                    X86_64AsmPrinter::format_operand(other)
                ));
            }
        }
    }

    fn emit_shift_by_reg(&mut self, op: &str, d: u8, count: u8, dst_str: &str) {
        let save = [SCRATCH2, 0, 2, 3]
            .into_iter()
            .find(|c| *c != d && *c != count && *c != 1)
            .unwrap_or(SCRATCH2);
        self.line(&format!("mov {}, rcx", Self::rname(save)));
        if d == 1 {
            // The destination is RCX itself: shift the saved copy and move the
            // result back.
            self.line(&format!("mov rcx, {}", Self::rname(count)));
            self.line(&format!("{} {}, cl", op, Self::rname(save)));
            self.line(&format!("mov rcx, {}", Self::rname(save)));
        } else {
            self.line(&format!("mov rcx, {}", Self::rname(count)));
            self.line(&format!("{} {}, cl", op, dst_str));
            self.line(&format!("mov rcx, {}", Self::rname(save)));
        }
    }

    fn print_setcc(&mut self, cc: ConditionCode, dst: &MachineOperand) {
        let suffix = X86_64AsmPrinter::cc_suffix(cc);
        match dst {
            MachineOperand::Register(MachineRegister::Physical(p)) if p.0 < 16 => {
                self.line(&format!(
                    "set{} {}",
                    suffix,
                    X86_64AsmPrinter::gpr_name(p.0, 1)
                ));
                self.line(&format!(
                    "movzx {}, {}",
                    Self::rname(p.0),
                    X86_64AsmPrinter::gpr_name(p.0, 1)
                ));
            }
            _ => {
                self.line(&format!(
                    "set{} {}",
                    suffix,
                    X86_64AsmPrinter::gpr_name(SCRATCH, 1)
                ));
                self.line(&format!(
                    "movzx {}, {}",
                    Self::rname(SCRATCH),
                    X86_64AsmPrinter::gpr_name(SCRATCH, 1)
                ));
                self.line(&format!(
                    "mov {}, {}",
                    X86_64AsmPrinter::format_operand(dst),
                    Self::rname(SCRATCH)
                ));
            }
        }
    }

    fn print_fp_binop(&mut self, op: &str, dst: &MachineOperand, src: &MachineOperand, size: u8) {
        let mov = if size == 8 { "movsd" } else { "movss" };
        let dst_reg = match dst {
            MachineOperand::Register(MachineRegister::Physical(p)) => Some(p.0),
            _ => None,
        };
        match dst_reg {
            Some(d) if d >= 16 => {
                let xd = Self::xname(d - 16);
                match src {
                    MachineOperand::FloatImmediate(f) => {
                        self.materialize_bits(SCRATCH, f.to_bits() as i64, Some(*f));
                        self.line(&format!(
                            "movq {}, {}",
                            Self::xname(FP_SCRATCH2),
                            Self::rname(SCRATCH)
                        ));
                        self.line(&format!("{} {}, {}", op, xd, Self::xname(FP_SCRATCH2)));
                    }
                    MachineOperand::Immediate(v) => {
                        self.materialize_bits(SCRATCH, *v, None);
                        self.line(&format!(
                            "movq {}, {}",
                            Self::xname(FP_SCRATCH2),
                            Self::rname(SCRATCH)
                        ));
                        self.line(&format!("{} {}, {}", op, xd, Self::xname(FP_SCRATCH2)));
                    }
                    other => {
                        self.line(&format!(
                            "{} {}, {}",
                            op,
                            xd,
                            X86_64AsmPrinter::format_operand(other)
                        ));
                    }
                }
            }
            _ => {
                // Spilled destination: load, operate, store (as the encoder does).
                let mem = X86_64AsmPrinter::format_operand(dst);
                self.line(&format!("{} {}, {}", mov, Self::xname(FP_SCRATCH), mem));
                self.line(&format!(
                    "{} {}, {}",
                    op,
                    Self::xname(FP_SCRATCH),
                    X86_64AsmPrinter::format_operand(src)
                ));
                self.line(&format!("{} {}, {}", mov, mem, Self::xname(FP_SCRATCH)));
            }
        }
    }

    fn print_fneg(&mut self, dst: &MachineOperand, size: u8) {
        let dst_reg = match dst {
            MachineOperand::Register(MachineRegister::Physical(p)) => Some(p.0),
            _ => None,
        };
        let (xor_op, bits, movq_op) = if size == 8 {
            ("xorpd", 0x8000_0000_0000_0000u64, "movq")
        } else {
            ("xorps", 0x8000_0000u64, "movd")
        };
        match dst_reg {
            Some(d) if d >= 16 => {
                self.materialize_bits(SCRATCH, bits as i64, None);
                self.line(&format!(
                    "{} {}, {}",
                    movq_op,
                    Self::xname(FP_SCRATCH),
                    Self::rname(SCRATCH)
                ));
                self.line(&format!(
                    "{} {}, {}",
                    xor_op,
                    Self::xname(d - 16),
                    Self::xname(FP_SCRATCH)
                ));
            }
            _ => {
                let mem = X86_64AsmPrinter::format_operand(dst);
                let mov = if size == 8 { "movsd" } else { "movss" };
                self.line(&format!("{} {}, {}", mov, Self::xname(FP_SCRATCH), mem));
                self.materialize_bits(SCRATCH, bits as i64, None);
                self.line(&format!(
                    "{} {}, {}",
                    movq_op,
                    Self::xname(FP_SCRATCH2),
                    Self::rname(SCRATCH)
                ));
                self.line(&format!(
                    "{} {}, {}",
                    xor_op,
                    Self::xname(FP_SCRATCH),
                    Self::xname(FP_SCRATCH2)
                ));
                self.line(&format!("{} {}, {}", mov, mem, Self::xname(FP_SCRATCH)));
            }
        }
    }

    fn print_fcvt_int_to_float(
        &mut self,
        dst: &MachineOperand,
        src: &MachineOperand,
        is_f64: bool,
        is_signed: bool,
    ) {
        let op = if is_f64 { "cvtsi2sd" } else { "cvtsi2ss" };
        let dst_str = X86_64AsmPrinter::format_operand(dst);
        let src_reg = match src {
            MachineOperand::Register(MachineRegister::Physical(s)) => Some(s.0),
            _ => None,
        };
        let src_str = match src_reg {
            Some(s) => Self::rname(s).to_string(),
            None => {
                self.line(&format!(
                    "mov {}, {}",
                    Self::rname(SCRATCH),
                    X86_64AsmPrinter::format_operand(src)
                ));
                Self::rname(SCRATCH).to_string()
            }
        };
        if is_signed {
            self.line(&format!("{} {}, {}", op, dst_str, src_str));
            return;
        }
        // Unsigned: the encoder halves, rounds up, converts and doubles.
        let positive = self.fresh_label("fcvt_pos");
        let done = self.fresh_label("fcvt_end");
        self.line(&format!("test {}, {}", src_str, src_str));
        self.line(&format!("jns {}", positive));
        self.line(&format!("mov {}, {}", Self::rname(SCRATCH), src_str));
        self.line(&format!(
            "mov {}, {}",
            Self::rname(SCRATCH2),
            Self::rname(SCRATCH)
        ));
        self.line(&format!("shr {}, 1", Self::rname(SCRATCH)));
        self.line(&format!("and {}, 1", Self::rname(SCRATCH2)));
        self.line(&format!(
            "or {}, {}",
            Self::rname(SCRATCH),
            Self::rname(SCRATCH2)
        ));
        self.line(&format!("{} {}, {}", op, dst_str, Self::rname(SCRATCH)));
        let dbl = if is_f64 { "addsd" } else { "addss" };
        self.line(&format!("{} {}, {}", dbl, dst_str, dst_str));
        self.line(&format!("jmp {}", done));
        self.out.push_str(&format!("{}:\n", positive));
        self.line(&format!("{} {}, {}", op, dst_str, src_str));
        self.out.push_str(&format!("{}:\n", done));
    }

    fn print_fcvt_float_to_int(
        &mut self,
        dst: &MachineOperand,
        src: &MachineOperand,
        is_f64: bool,
        is_signed: bool,
    ) {
        let op = if is_f64 { "cvttsd2si" } else { "cvttss2si" };
        let dst_str = X86_64AsmPrinter::format_operand(dst);
        let src_str = X86_64AsmPrinter::format_operand(src);
        if is_signed || !is_f64 {
            self.line(&format!("{} {}, {}", op, dst_str, src_str));
            return;
        }
        // Unsigned f64 -> u64: bias by 2^63 and add the sign bit back.
        let above = self.fresh_label("fcvt_ge");
        let done = self.fresh_label("fcvt_end");
        self.line(&format!(
            "movabs {}, 0x43E0000000000000  # 2^63 as f64",
            Self::rname(SCRATCH)
        ));
        self.line(&format!(
            "movq {}, {}",
            Self::xname(FP_SCRATCH),
            Self::rname(SCRATCH)
        ));
        self.line(&format!("ucomisd {}, {}", src_str, Self::xname(FP_SCRATCH)));
        self.line(&format!("jae {}", above));
        self.line(&format!("{} {}, {}", op, dst_str, src_str));
        self.line(&format!("jmp {}", done));
        self.out.push_str(&format!("{}:\n", above));
        self.line(&format!("movsd {}, {}", Self::xname(FP_SCRATCH2), src_str));
        self.line(&format!(
            "subsd {}, {}",
            Self::xname(FP_SCRATCH2),
            Self::xname(FP_SCRATCH)
        ));
        self.line(&format!("{} {}, {}", op, dst_str, Self::xname(FP_SCRATCH2)));
        self.line(&format!(
            "movabs {}, 0x8000000000000000",
            Self::rname(SCRATCH2)
        ));
        self.line(&format!("add {}, {}", dst_str, Self::rname(SCRATCH2)));
        self.out.push_str(&format!("{}:\n", done));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine_ir::MachineInstruction;

    fn trimmed_lines(text: &str) -> Vec<&str> {
        text.lines().map(str::trim).collect()
    }

    #[test]
    fn printed_frame_matches_the_encoder_frame_layout() {
        // A function that touches the callee-saved RBX and has one 8-byte
        // local: locals_size = 8, RBX is saved at [rbp - 16], frame_size 16.
        let mut func = MachineFunction::new("frame_check");
        func.stack_size = 8;
        func.blocks[0].push(MachineInstruction::Move {
            dst: MachineOperand::phys(3), // rbx
            src: MachineOperand::Immediate(7),
        });
        func.blocks[0].push(MachineInstruction::Store {
            dst: MachineOperand::StackSlot(-8),
            src: MachineOperand::phys(3),
            size: 8,
        });
        func.blocks[0].push(MachineInstruction::Return);

        let text = X86_64AsmPrinter::print_function(&func);
        let lines = trimmed_lines(&text);
        for expected in [
            "push rbp",
            "mov rbp, rsp",
            "sub rsp, 16",
            // The local store, with an explicit size on the memory operand.
            "mov qword ptr [rbp - 8], rbx",
            // Callee-saved save (prologue) and restore (epilogue).
            "mov [rbp - 16], rbx",
            "mov rbx, [rbp - 16]",
            "mov rsp, rbp",
            "pop rbp",
            "ret",
        ] {
            assert!(
                lines.contains(&expected),
                "missing `{expected}` in:\n{}",
                lines.join("\n")
            );
        }
        // The save must precede the local store, and the restore must follow it.
        let save = lines
            .iter()
            .position(|l| *l == "mov [rbp - 16], rbx")
            .unwrap();
        let local = lines
            .iter()
            .position(|l| *l == "mov qword ptr [rbp - 8], rbx")
            .unwrap();
        let restore = lines
            .iter()
            .position(|l| *l == "mov rbx, [rbp - 16]")
            .unwrap();
        assert!(save < local && local < restore);
    }

    #[test]
    fn printed_instructions_mirror_the_encoder_sequences() {
        let mut func = MachineFunction::new("seq_check");
        func.blocks[0].push(MachineInstruction::Custom {
            name: "endbr64".to_string(),
            operands: Vec::new(),
        });
        // Spilled dividend divided by an ordinary GPR: RAX marshalling with
        // CQO sign extension before the IDIV.
        func.blocks[0].push(MachineInstruction::Div {
            dst: MachineOperand::StackSlot(-8),
            src: MachineOperand::phys(6), // rsi
        });
        // Shift-by-register whose destination is RCX itself: the count must
        // reach CL without destroying the destination's value.
        func.blocks[0].push(MachineInstruction::Shl {
            dst: MachineOperand::phys(1), // rcx
            src: MachineOperand::phys(9), // r9
        });
        func.blocks[0].push(MachineInstruction::Return);

        let text = X86_64AsmPrinter::print_function(&func);
        let lines = trimmed_lines(&text);

        assert!(lines.contains(&"endbr64"));

        let load = lines
            .iter()
            .position(|l| *l == "mov rax, [rbp - 8]")
            .expect("dividend load into rax");
        let cqo = lines.iter().position(|l| *l == "cqo").expect("cqo");
        let idiv = lines.iter().position(|l| *l == "idiv rsi").expect("idiv");
        let store = lines
            .iter()
            .position(|l| *l == "mov [rbp - 8], rax")
            .expect("quotient store");
        assert!(load < cqo && cqo < idiv && idiv < store);

        let save_rcx = lines
            .iter()
            .position(|l| *l == "mov r11, rcx")
            .expect("rcx saved");
        let count = lines
            .iter()
            .position(|l| *l == "mov rcx, r9")
            .expect("count into rcx");
        let shl = lines
            .iter()
            .position(|l| *l == "shl r11, cl")
            .expect("shift of the saved copy");
        let restore = lines
            .iter()
            .position(|l| *l == "mov rcx, r11")
            .expect("result moved back into rcx");
        assert!(save_rcx < count && count < shl && shl < restore);
    }
}
