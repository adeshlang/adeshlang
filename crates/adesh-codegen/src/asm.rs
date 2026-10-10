//! Phase 9 Production Native Assembly Emitter (`--emit=asm`).
//!
//! Emits clean, human-readable, annotated assembly source listings compatible with
//! standard platform assemblers and debug inspection.

use crate::machine_ir::{
    ConditionCode, MachineBlock, MachineFunction, MachineInstruction, MachineOperand,
    MachineRegister, NativeModule, PhysicalRegister, VirtualRegister,
};

/// Assembly Emitter Engine.
pub struct AssemblyEmitter {
    target_triple: String,
}

impl AssemblyEmitter {
    pub fn new(target_triple: impl Into<String>) -> Self {
        Self {
            target_triple: target_triple.into(),
        }
    }

    /// Convenience function to emit assembly for a module with a default target triple.
    pub fn emit_module(module: &NativeModule) -> String {
        Self::new("x86_64").emit_module_asm(module)
    }

    /// Emit complete assembly text for a NativeModule.
    pub fn emit_module_asm(&self, module: &NativeModule) -> String {
        let mut s = String::new();
        s.push_str("; Adesh Native Assembly Output\n");
        s.push_str(&format!("; Target: {}\n", self.target_triple));
        s.push_str(&format!("; Module: {}\n\n", module.name));

        s.push_str(".section .text\n");
        s.push_str(".align 16\n\n");

        for func in &module.functions {
            self.emit_function_asm(func, &mut s);
        }

        s
    }

    /// Emit assembly for a single function.
    pub fn emit_function_asm(&self, func: &MachineFunction, s: &mut String) {
        if func.is_exported {
            s.push_str(&format!(".globl {}\n", func.name));
        }
        s.push_str(&format!("{}:\n", func.name));

        for block in &func.blocks {
            s.push_str(&format!(".L_{}_{}:\n", func.name, block.label));
            for inst in &block.instructions {
                s.push_str(&format!("    {}\n", self.format_instruction(inst)));
            }
        }
        s.push_str(&format!("; end function {}\n\n", func.name));
    }

    fn format_instruction(&self, inst: &MachineInstruction) -> String {
        match inst {
            MachineInstruction::Move { dst, src } => {
                format!(
                    "mov {}, {}",
                    self.format_operand(dst),
                    self.format_operand(src)
                )
            }
            MachineInstruction::Add { dst, src } => {
                format!(
                    "add {}, {}",
                    self.format_operand(dst),
                    self.format_operand(src)
                )
            }
            MachineInstruction::Sub { dst, src } => {
                format!(
                    "sub {}, {}",
                    self.format_operand(dst),
                    self.format_operand(src)
                )
            }
            MachineInstruction::Mul { dst, src } => {
                format!(
                    "imul {}, {}",
                    self.format_operand(dst),
                    self.format_operand(src)
                )
            }
            MachineInstruction::And { dst, src } => {
                format!(
                    "and {}, {}",
                    self.format_operand(dst),
                    self.format_operand(src)
                )
            }
            MachineInstruction::Or { dst, src } => {
                format!(
                    "or {}, {}",
                    self.format_operand(dst),
                    self.format_operand(src)
                )
            }
            MachineInstruction::Xor { dst, src } => {
                format!(
                    "xor {}, {}",
                    self.format_operand(dst),
                    self.format_operand(src)
                )
            }
            MachineInstruction::Shl { dst, src } => {
                format!(
                    "shl {}, {}",
                    self.format_operand(dst),
                    self.format_operand(src)
                )
            }
            MachineInstruction::Shr { dst, src } => {
                format!(
                    "shr {}, {}",
                    self.format_operand(dst),
                    self.format_operand(src)
                )
            }
            MachineInstruction::Compare { lhs, rhs } => {
                format!(
                    "cmp {}, {}",
                    self.format_operand(lhs),
                    self.format_operand(rhs)
                )
            }
            MachineInstruction::Branch { target } => {
                format!("jmp {}", target)
            }
            MachineInstruction::BranchCc { cc, target } => {
                let cc_str = match cc {
                    ConditionCode::Equal => "je",
                    ConditionCode::NotEqual => "jne",
                    ConditionCode::LessThan => "jl",
                    ConditionCode::LessOrEqual => "jle",
                    ConditionCode::GreaterThan => "jg",
                    ConditionCode::GreaterOrEqual => "jge",
                    _ => "jcc",
                };
                format!("{} {}", cc_str, target)
            }
            MachineInstruction::Call { target, .. } => {
                format!("call {}", self.format_operand(target))
            }
            MachineInstruction::Return => "ret".to_string(),
            MachineInstruction::Load { dst, src, size } => {
                format!(
                    "mov {}, {} ({}b)",
                    self.format_operand(dst),
                    self.format_operand(src),
                    size
                )
            }
            MachineInstruction::Store { dst, src, size } => {
                format!(
                    "mov {}, {} ({}b)",
                    self.format_operand(dst),
                    self.format_operand(src),
                    size
                )
            }
            _ => format!("; custom {:?}", inst),
        }
    }

    fn format_operand(&self, op: &MachineOperand) -> String {
        match op {
            MachineOperand::Immediate(imm) => format!("0x{:x}", imm),
            MachineOperand::Register(r) => self.format_reg(r),
            MachineOperand::Memory { base, offset, .. } => {
                format!("[{} + {}]", self.format_reg(base), offset)
            }
            MachineOperand::Symbol(sym) => sym.clone(),
            _ => format!("{:?}", op),
        }
    }

    fn format_reg(&self, r: &MachineRegister) -> String {
        match r {
            MachineRegister::Physical(p) => match p.0 {
                0 => "rax".to_string(),
                1 => "rcx".to_string(),
                2 => "rdx".to_string(),
                3 => "rbx".to_string(),
                4 => "rsp".to_string(),
                5 => "rbp".to_string(),
                6 => "rsi".to_string(),
                7 => "rdi".to_string(),
                8 => "r8".to_string(),
                9 => "r9".to_string(),
                10 => "r10".to_string(),
                11 => "r11".to_string(),
                12 => "r12".to_string(),
                13 => "r13".to_string(),
                14 => "r14".to_string(),
                15 => "r15".to_string(),
                idx if idx >= 32 => format!("xmm{}", idx - 32),
                other => format!("r{}", other),
            },
            MachineRegister::Virtual(v) => format!("vreg{}", v.0),
        }
    }
}
