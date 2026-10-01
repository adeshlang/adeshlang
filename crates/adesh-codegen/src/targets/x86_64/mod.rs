//! Complete x86-64 Native Backend implementing `CodegenBackend`.

pub mod asm_printer;
pub mod encoder;

use crate::backend::CodegenBackend;
use crate::calling_convention::{
    CallingConvention, SystemVX64CallingConvention, WindowsX64CallingConvention,
};
use crate::error::CodegenError;
use crate::machine_ir::{
    MachineFunction, MachineInstruction, MachineOperand, MachineRegister, NativeModule,
    PhysicalRegister,
};
use crate::register_alloc::{LinearScanAllocator, RegisterFile};
use crate::targets::x86_64::encoder::X86_64Encoder;
use adesh_object::{
    AdobObject, AdobSection, AdobSymbol, OperatingSystem, SectionKind, SymbolBinding, SymbolKind,
    SymbolVisibility, TargetCapabilities, TargetDescriptor, section_flags,
};
use std::collections::HashMap;

/// x86-64 Physical Register File definition.
pub struct X86_64RegisterFile;

// Allocatable general purpose registers (excluding RSP(4) and RBP(5))
const X86_64_ALL_REGS: [PhysicalRegister; 16] = [
    PhysicalRegister(0),  // RAX
    PhysicalRegister(1),  // RCX
    PhysicalRegister(2),  // RDX
    PhysicalRegister(3),  // RBX
    PhysicalRegister(4),  // RSP (reserved)
    PhysicalRegister(5),  // RBP (frame pointer)
    PhysicalRegister(6),  // RSI
    PhysicalRegister(7),  // RDI
    PhysicalRegister(8),  // R8
    PhysicalRegister(9),  // R9
    PhysicalRegister(10), // R10
    PhysicalRegister(11), // R11
    PhysicalRegister(12), // R12
    PhysicalRegister(13), // R13
    PhysicalRegister(14), // R14
    PhysicalRegister(15), // R15
];

const X86_64_ALLOCATABLE: [PhysicalRegister; 14] = [
    PhysicalRegister(0),  // RAX
    PhysicalRegister(1),  // RCX
    PhysicalRegister(2),  // RDX
    PhysicalRegister(3),  // RBX
    PhysicalRegister(6),  // RSI
    PhysicalRegister(7),  // RDI
    PhysicalRegister(8),  // R8
    PhysicalRegister(9),  // R9
    PhysicalRegister(10), // R10
    PhysicalRegister(11), // R11
    PhysicalRegister(12), // R12
    PhysicalRegister(13), // R13
    PhysicalRegister(14), // R14
    PhysicalRegister(15), // R15
];

const X86_64_RESERVED: [PhysicalRegister; 2] = [
    PhysicalRegister(4), // RSP
    PhysicalRegister(5), // RBP
];

impl RegisterFile for X86_64RegisterFile {
    fn registers(&self) -> &[PhysicalRegister] {
        &X86_64_ALL_REGS
    }
    fn allocatable(&self) -> &[PhysicalRegister] {
        &X86_64_ALLOCATABLE
    }
    fn caller_saved(&self) -> &[PhysicalRegister] {
        &X86_64_ALLOCATABLE[0..8]
    }
    fn callee_saved(&self) -> &[PhysicalRegister] {
        &X86_64_ALLOCATABLE[8..]
    }
    fn reserved(&self) -> &[PhysicalRegister] {
        &X86_64_RESERVED
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
        let mut encoder = X86_64Encoder::new();

        // 1. Prologue: push rbp; mov rbp, rsp; sub rsp, aligned_frame
        encoder.push_reg64(5); // push rbp
        encoder.mov_r64_r64(5, 4); // mov rbp, rsp

        let frame_size = func.stack_size.div_ceil(16) * 16;
        if frame_size > 0 {
            encoder.sub_r64_imm32(4, frame_size as i32); // sub rsp, frame_size
        }

        let mut block_offsets = HashMap::new();

        // Pass 1: Record block start offsets and encode instructions
        for block in &func.blocks {
            block_offsets.insert(block.label.clone(), encoder.len());

            for inst in &block.instructions {
                match inst {
                    MachineInstruction::Nop => encoder.nop(),
                    MachineInstruction::Return => {
                        // Epilogue: mov rsp, rbp; pop rbp; ret
                        if frame_size > 0 {
                            encoder.mov_r64_r64(4, 5); // mov rsp, rbp
                        }
                        encoder.pop_reg64(5); // pop rbp
                        encoder.ret();
                    }
                    MachineInstruction::Move { dst, src } => match (dst, src) {
                        (
                            MachineOperand::Register(MachineRegister::Physical(d)),
                            MachineOperand::Register(MachineRegister::Physical(s)),
                        ) => {
                            encoder.mov_r64_r64(d.0, s.0);
                        }
                        (
                            MachineOperand::Register(MachineRegister::Physical(d)),
                            MachineOperand::Immediate(imm),
                        ) => {
                            encoder.mov_r64_imm64(d.0, *imm);
                        }
                        (
                            MachineOperand::Register(MachineRegister::Physical(d)),
                            MachineOperand::StackSlot(slot),
                        ) => {
                            encoder.mov_r64_rbp_offset(d.0, *slot);
                        }
                        (
                            MachineOperand::StackSlot(slot),
                            MachineOperand::Register(MachineRegister::Physical(s)),
                        ) => {
                            encoder.mov_rbp_offset_r64(*slot, s.0);
                        }
                        _ => {}
                    },
                    MachineInstruction::Add { dst, src } => {
                        if let (
                            MachineOperand::Register(MachineRegister::Physical(d)),
                            MachineOperand::Register(MachineRegister::Physical(s)),
                        ) = (dst, src)
                        {
                            encoder.add_r64_r64(d.0, s.0);
                        } else if let (
                            MachineOperand::Register(MachineRegister::Physical(d)),
                            MachineOperand::Immediate(imm),
                        ) = (dst, src)
                        {
                            encoder.add_r64_imm32(d.0, *imm as i32);
                        }
                    }
                    MachineInstruction::Sub { dst, src } => {
                        if let (
                            MachineOperand::Register(MachineRegister::Physical(d)),
                            MachineOperand::Register(MachineRegister::Physical(s)),
                        ) = (dst, src)
                        {
                            encoder.sub_r64_r64(d.0, s.0);
                        } else if let (
                            MachineOperand::Register(MachineRegister::Physical(d)),
                            MachineOperand::Immediate(imm),
                        ) = (dst, src)
                        {
                            encoder.sub_r64_imm32(d.0, *imm as i32);
                        }
                    }
                    MachineInstruction::Mul { dst, src } => {
                        if let (
                            MachineOperand::Register(MachineRegister::Physical(d)),
                            MachineOperand::Register(MachineRegister::Physical(s)),
                        ) = (dst, src)
                        {
                            encoder.imul_r64_r64(d.0, s.0);
                        }
                    }
                    MachineInstruction::Div { dst, src } => {
                        if let (
                            MachineOperand::Register(MachineRegister::Physical(d)),
                            MachineOperand::Register(MachineRegister::Physical(s)),
                        ) = (dst, src)
                        {
                            if d.0 != 0 {
                                encoder.mov_r64_r64(0, d.0); // mov rax, dst
                            }
                            encoder.cqo();
                            encoder.idiv_r64(s.0);
                            if d.0 != 0 {
                                encoder.mov_r64_r64(d.0, 0); // mov dst, rax
                            }
                        }
                    }
                    MachineInstruction::Mod { dst, src } => {
                        if let (
                            MachineOperand::Register(MachineRegister::Physical(d)),
                            MachineOperand::Register(MachineRegister::Physical(s)),
                        ) = (dst, src)
                        {
                            if d.0 != 0 {
                                encoder.mov_r64_r64(0, d.0);
                            }
                            encoder.cqo();
                            encoder.idiv_r64(s.0);
                            encoder.mov_r64_r64(d.0, 2); // mov dst, rdx (remainder)
                        }
                    }
                    MachineInstruction::Neg {
                        dst: MachineOperand::Register(MachineRegister::Physical(d)),
                    } => {
                        encoder.neg_r64(d.0);
                    }
                    MachineInstruction::Not {
                        dst: MachineOperand::Register(MachineRegister::Physical(d)),
                    } => {
                        encoder.not_r64(d.0);
                    }
                    MachineInstruction::And { dst, src } => {
                        if let (
                            MachineOperand::Register(MachineRegister::Physical(d)),
                            MachineOperand::Register(MachineRegister::Physical(s)),
                        ) = (dst, src)
                        {
                            encoder.and_r64_r64(d.0, s.0);
                        }
                    }
                    MachineInstruction::Or { dst, src } => {
                        if let (
                            MachineOperand::Register(MachineRegister::Physical(d)),
                            MachineOperand::Register(MachineRegister::Physical(s)),
                        ) = (dst, src)
                        {
                            encoder.or_r64_r64(d.0, s.0);
                        }
                    }
                    MachineInstruction::Xor { dst, src } => {
                        if let (
                            MachineOperand::Register(MachineRegister::Physical(d)),
                            MachineOperand::Register(MachineRegister::Physical(s)),
                        ) = (dst, src)
                        {
                            encoder.xor_r64_r64(d.0, s.0);
                        }
                    }
                    MachineInstruction::Compare { lhs, rhs } => {
                        if let (
                            MachineOperand::Register(MachineRegister::Physical(l)),
                            MachineOperand::Register(MachineRegister::Physical(r)),
                        ) = (lhs, rhs)
                        {
                            encoder.cmp_r64_r64(l.0, r.0);
                        } else if let (
                            MachineOperand::Register(MachineRegister::Physical(l)),
                            MachineOperand::Immediate(imm),
                        ) = (lhs, rhs)
                        {
                            encoder.cmp_r64_imm32(l.0, *imm as i32);
                        }
                    }
                    MachineInstruction::SetCc {
                        dst: MachineOperand::Register(MachineRegister::Physical(d)),
                        cc,
                    } => {
                        encoder.setcc_r8(*cc, d.0);
                    }
                    MachineInstruction::Branch { target } => {
                        let target_off = block_offsets.get(target).copied().unwrap_or(0) as i32;
                        let curr_off = (encoder.len() + 5) as i32;
                        encoder.jmp_rel32(target_off - curr_off);
                    }
                    MachineInstruction::BranchCc { cc, target } => {
                        let target_off = block_offsets.get(target).copied().unwrap_or(0) as i32;
                        let curr_off = (encoder.len() + 6) as i32;
                        encoder.jcc_rel32(*cc, target_off - curr_off);
                    }
                    MachineInstruction::Call { target, .. } => {
                        if let MachineOperand::Register(MachineRegister::Physical(r)) = target {
                            encoder.call_r64(r.0);
                        } else {
                            encoder.call_rel32(0); // placeholder for relocation
                        }
                    }
                    _ => {}
                }
            }
        }

        // If function doesn't end with RET, add default epilogue
        if encoder.buffer.last() != Some(&0xC3) {
            if frame_size > 0 {
                encoder.mov_r64_r64(4, 5);
            }
            encoder.pop_reg64(5);
            encoder.ret();
        }

        Ok(encoder.buffer)
    }

    fn emit_object(&mut self, module: &NativeModule) -> Result<AdobObject, CodegenError> {
        let mut obj = AdobObject::new(self.target.clone());

        // 1. Text Section
        let mut text_bytes = Vec::new();
        let text_relocations = Vec::new();

        for func in &module.functions {
            let func_offset = text_bytes.len() as u64;
            let code = self.generate_function(func)?;
            let func_size = code.len() as u64;
            text_bytes.extend_from_slice(&code);

            // Add function symbol
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

            obj.add_symbol(sym);

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

        // 2. Read-Only Data Section (.rodata) for String Pool
        if !module.string_pool.is_empty() {
            let mut rodata = Vec::new();
            for (idx, s) in module.string_pool.iter().enumerate() {
                let off = rodata.len() as u64;
                let s_bytes = s.as_bytes();
                rodata.extend_from_slice(s_bytes);
                rodata.push(0); // Null terminator

                let str_sym_name = format!("__str_{}", idx);
                let sym = AdobSymbol::new_defined(
                    0,
                    str_sym_name,
                    SymbolKind::Object,
                    1, // section 1 (.rodata)
                    off,
                    s_bytes.len() as u64 + 1,
                )
                .with_binding(SymbolBinding::Local);
                obj.add_symbol(sym);
            }

            let rodata_sec = AdobSection::new(".rodata", SectionKind::Rodata)
                .with_flags(section_flags::READ | section_flags::ALLOC)
                .with_alignment(8)
                .with_data(rodata);
            obj.add_section(rodata_sec);
        }

        // 3. Imports
        for imp in &module.imports {
            obj.add_import(imp.clone());
            let sym = AdobSymbol::new_undefined(0, imp.clone(), SymbolKind::Import)
                .with_binding(SymbolBinding::Global);
            obj.add_symbol(sym);
        }

        Ok(obj)
    }

    fn generate_assembly(&mut self, module: &NativeModule) -> Result<String, CodegenError> {
        let lowered = self.lower_module(module)?;
        Ok(asm_printer::X86_64AsmPrinter::print_module(&lowered))
    }
}
