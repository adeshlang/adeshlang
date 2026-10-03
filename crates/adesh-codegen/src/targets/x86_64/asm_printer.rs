//! x86-64 Assembly Printer (Intel syntax / GNU as compatible).

use crate::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister,
};

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
        let mut out = String::new();

        if func.is_exported {
            out.push_str(&format!("    .globl {}\n", func.name));
            out.push_str(&format!("    .type {}, @function\n", func.name));
        }
        out.push_str(&format!("{}:\n", func.name));

        // Prologue
        out.push_str("    push rbp\n");
        out.push_str("    mov rbp, rsp\n");
        let frame_size = func.stack_size.div_ceil(16) * 16;
        if frame_size > 0 {
            out.push_str(&format!("    sub rsp, {}\n", frame_size));
        }

        for block in &func.blocks {
            if block.label != "entry" {
                out.push_str(&format!(".L_{}:\n", block.label));
            }

            for inst in &block.instructions {
                match inst {
                    MachineInstruction::Nop => out.push_str("    nop\n"),
                    MachineInstruction::Move { dst, src } => {
                        let is_str_sym = match src {
                            MachineOperand::Symbol(sym) if sym.starts_with("__str_") => Some(sym),
                            _ => None,
                        };
                        if let Some(sym) = is_str_sym {
                            out.push_str(&format!(
                                "    lea {}, [rip + {}]\n",
                                Self::format_operand(dst),
                                sym
                            ));
                            continue;
                        }
                        out.push_str(&format!(
                            "    mov {}, {}\n",
                            Self::format_operand(dst),
                            Self::format_operand(src)
                        ));
                    }
                    MachineInstruction::Load { dst, src, .. } => {
                        out.push_str(&format!(
                            "    mov {}, {}\n",
                            Self::format_operand(dst),
                            Self::format_operand(src)
                        ));
                    }
                    MachineInstruction::Store { dst, src, .. } => {
                        out.push_str(&format!(
                            "    mov {}, {}\n",
                            Self::format_operand(dst),
                            Self::format_operand(src)
                        ));
                    }
                    MachineInstruction::Add { dst, src } => {
                        out.push_str(&format!(
                            "    add {}, {}\n",
                            Self::format_operand(dst),
                            Self::format_operand(src)
                        ));
                    }
                    MachineInstruction::Sub { dst, src } => {
                        out.push_str(&format!(
                            "    sub {}, {}\n",
                            Self::format_operand(dst),
                            Self::format_operand(src)
                        ));
                    }
                    MachineInstruction::Mul { dst, src } => {
                        out.push_str(&format!(
                            "    imul {}, {}\n",
                            Self::format_operand(dst),
                            Self::format_operand(src)
                        ));
                    }
                    MachineInstruction::Div { src, .. } => {
                        out.push_str("    cqo\n");
                        out.push_str(&format!("    idiv {}\n", Self::format_operand(src)));
                    }
                    MachineInstruction::Mod { src, .. } => {
                        out.push_str("    cqo\n");
                        out.push_str(&format!("    idiv {}\n", Self::format_operand(src)));
                        out.push_str("    mov rax, rdx\n");
                    }
                    MachineInstruction::Neg { dst } => {
                        out.push_str(&format!("    neg {}\n", Self::format_operand(dst)));
                    }
                    MachineInstruction::Not { dst } => {
                        out.push_str(&format!("    not {}\n", Self::format_operand(dst)));
                    }
                    MachineInstruction::And { dst, src } => {
                        out.push_str(&format!(
                            "    and {}, {}\n",
                            Self::format_operand(dst),
                            Self::format_operand(src)
                        ));
                    }
                    MachineInstruction::Or { dst, src } => {
                        out.push_str(&format!(
                            "    or {}, {}\n",
                            Self::format_operand(dst),
                            Self::format_operand(src)
                        ));
                    }
                    MachineInstruction::Xor { dst, src } => {
                        out.push_str(&format!(
                            "    xor {}, {}\n",
                            Self::format_operand(dst),
                            Self::format_operand(src)
                        ));
                    }
                    MachineInstruction::Shl { dst, src } => {
                        out.push_str(&format!(
                            "    shl {}, {}\n",
                            Self::format_operand(dst),
                            Self::format_operand(src)
                        ));
                    }
                    MachineInstruction::Shr { dst, src } => {
                        out.push_str(&format!(
                            "    shr {}, {}\n",
                            Self::format_operand(dst),
                            Self::format_operand(src)
                        ));
                    }
                    MachineInstruction::Sar { dst, src } => {
                        out.push_str(&format!(
                            "    sar {}, {}\n",
                            Self::format_operand(dst),
                            Self::format_operand(src)
                        ));
                    }
                    MachineInstruction::Compare { lhs, rhs } => {
                        out.push_str(&format!(
                            "    cmp {}, {}\n",
                            Self::format_operand(lhs),
                            Self::format_operand(rhs)
                        ));
                    }
                    MachineInstruction::SetCc { dst, cc } => {
                        out.push_str(&format!(
                            "    set{} {}\n",
                            Self::cc_suffix(*cc),
                            Self::format_operand(dst)
                        ));
                    }
                    MachineInstruction::Branch { target } => {
                        out.push_str(&format!("    jmp .L_{}\n", target));
                    }
                    MachineInstruction::BranchCc { cc, target } => {
                        out.push_str(&format!("    j{} .L_{}\n", Self::cc_suffix(*cc), target));
                    }
                    MachineInstruction::Call { target, .. } => {
                        out.push_str(&format!("    call {}\n", Self::format_operand(target)));
                    }
                    MachineInstruction::Return => {
                        if frame_size > 0 {
                            out.push_str("    mov rsp, rbp\n");
                        }
                        out.push_str("    pop rbp\n");
                        out.push_str("    ret\n");
                    }
                    MachineInstruction::Push { src } => {
                        out.push_str(&format!("    push {}\n", Self::format_operand(src)));
                    }
                    MachineInstruction::Pop { dst } => {
                        out.push_str(&format!("    pop {}\n", Self::format_operand(dst)));
                    }
                    MachineInstruction::FAdd { dst, src, size } => {
                        let op = if *size == 8 { "addsd" } else { "addss" };
                        out.push_str(&format!(
                            "    {} {}, {}\n",
                            op,
                            Self::format_operand(dst),
                            Self::format_operand(src)
                        ));
                    }
                    MachineInstruction::FSub { dst, src, size } => {
                        let op = if *size == 8 { "subsd" } else { "subss" };
                        out.push_str(&format!(
                            "    {} {}, {}\n",
                            op,
                            Self::format_operand(dst),
                            Self::format_operand(src)
                        ));
                    }
                    MachineInstruction::FMul { dst, src, size } => {
                        let op = if *size == 8 { "mulsd" } else { "mulss" };
                        out.push_str(&format!(
                            "    {} {}, {}\n",
                            op,
                            Self::format_operand(dst),
                            Self::format_operand(src)
                        ));
                    }
                    MachineInstruction::FDiv { dst, src, size } => {
                        let op = if *size == 8 { "divsd" } else { "divss" };
                        out.push_str(&format!(
                            "    {} {}, {}\n",
                            op,
                            Self::format_operand(dst),
                            Self::format_operand(src)
                        ));
                    }
                    MachineInstruction::FNeg { dst, size } => {
                        let op = if *size == 8 { "xorpd" } else { "xorps" };
                        out.push_str(&format!(
                            "    {} {}, [sign_bit]\n",
                            op,
                            Self::format_operand(dst)
                        ));
                    }
                    MachineInstruction::FCmp { lhs, rhs, size } => {
                        let op = if *size == 8 { "ucomisd" } else { "ucomiss" };
                        out.push_str(&format!(
                            "    {} {}, {}\n",
                            op,
                            Self::format_operand(lhs),
                            Self::format_operand(rhs)
                        ));
                    }
                    MachineInstruction::FCvtIntToFloat {
                        dst,
                        src,
                        is_f64,
                        is_signed: _,
                    } => {
                        let op = if *is_f64 { "cvtsi2sd" } else { "cvtsi2ss" };
                        out.push_str(&format!(
                            "    {} {}, {}\n",
                            op,
                            Self::format_operand(dst),
                            Self::format_operand(src)
                        ));
                    }
                    MachineInstruction::FCvtFloatToInt {
                        dst,
                        src,
                        is_f64,
                        is_signed: _,
                    } => {
                        let op = if *is_f64 { "cvttsd2si" } else { "cvttss2si" };
                        out.push_str(&format!(
                            "    {} {}, {}\n",
                            op,
                            Self::format_operand(dst),
                            Self::format_operand(src)
                        ));
                    }
                    MachineInstruction::FCvtFloatToFloat { dst, src, to_f64 } => {
                        let op = if *to_f64 { "cvtss2sd" } else { "cvtsd2ss" };
                        out.push_str(&format!(
                            "    {} {}, {}\n",
                            op,
                            Self::format_operand(dst),
                            Self::format_operand(src)
                        ));
                    }
                    _ => {}
                }
            }
        }

        out.push_str(&format!("    .size {}, .-{}\n\n", func.name, func.name));
        out
    }

    pub fn print_module(module: &NativeModule) -> String {
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
            out.push_str(&Self::print_function(func));
        }

        out
    }
}
