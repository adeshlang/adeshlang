//! IA-32 / i686 Native Codegen Backend.
//!
//! Phase 2 execution subset for 32-bit native execution. This backend emits
//! 32-bit x86 encodings for register moves and returns, enough to execute trivial
//! integer-return functions. Every other `MachineInstruction` variant is refused
//! with a loud `CodegenError` naming the variant and function. It never emits
//! REX prefixes, 64-bit operands, or silently x86-64 semantics.
//!
//! Memory-safe (`#![forbid(unsafe_code)]`), type-safe, no heap in refusal paths.

#![forbid(unsafe_code)]

use crate::backend::CodegenBackend;
use crate::calling_convention::ParallelMoveResolver;
use crate::error::CodegenError;
use crate::machine_ir::{
    MachineFunction, MachineInstruction, MachineOperand, MachineRegister, NativeModule,
    VirtualRegister,
};
use crate::targets::{unsupported_instruction, unsupported_operand_form};
use adesh_object::{
    AdobObject, AdobRelocation, AdobSection, AdobSymbol, MemoryPermissions, MemoryRegion,
    RelocationKind, SectionKind, SymbolBinding, SymbolKind, SymbolVisibility, TargetCapabilities,
    TargetDescriptor, section_flags,
};
use std::collections::{HashMap, HashSet};

const ALLOCATABLE_GPRS: [u8; 5] = [1, 2, 3, 6, 7];
const CDECL_ARGUMENT_GPRS: [u8; 6] = [7, 6, 2, 1, 8, 9];

fn i686_resolver() -> ParallelMoveResolver {
    ParallelMoveResolver::new("i686")
        .with_scratch_pool(vec![crate::machine_ir::PhysicalRegister(0)])
        .with_reserved_registers(
            [
                crate::machine_ir::PhysicalRegister(4),
                crate::machine_ir::PhysicalRegister(5),
            ]
            .into_iter()
            .collect::<HashSet<_>>(),
        )
        .with_default_stack_base(crate::machine_ir::PhysicalRegister(4))
}

fn is_usable_gpr(id: u8) -> bool {
    id < 8 && id != 4 && id != 5
}

fn assign_vreg(
    vreg: VirtualRegister,
    allocations: &mut HashMap<VirtualRegister, u8>,
) -> Result<u8, CodegenError> {
    if let Some(&id) = allocations.get(&vreg) {
        return Ok(id);
    }
    let id = *ALLOCATABLE_GPRS.get(allocations.len()).ok_or_else(|| {
        CodegenError::new(
            "i686",
            "IA-32 backend exhausted its allocatable 32-bit GPR set",
        )
        .with_arch("i686")
    })?;
    allocations.insert(vreg, id);
    Ok(id)
}

fn resolve_gpr(reg: MachineRegister, allocations: &HashMap<VirtualRegister, u8>) -> Option<u8> {
    match reg {
        MachineRegister::Virtual(v) => allocations.get(&v).copied(),
        MachineRegister::Physical(p) if is_usable_gpr(p.0) => Some(p.0),
        _ => None,
    }
}

fn encode_mov_imm32(out: &mut Vec<u8>, dst: u8, imm: i64) -> Result<(), CodegenError> {
    let value = i32::try_from(imm).map_err(|_| {
        CodegenError::new(
            "i686",
            format!("immediate 0x{imm:x} does not fit in a signed 32-bit IA-32 immediate"),
        )
        .with_arch("i686")
        .with_instruction("Move")
    })?;
    out.push(0xB8 + dst);
    out.extend_from_slice(&(value as u32).to_le_bytes());
    Ok(())
}

fn encode_mov_r32_r32(out: &mut Vec<u8>, dst: u8, src: u8) {
    out.push(0x89);
    out.push(0xC0 | (src << 3) | dst);
}

fn encode_call_rel32(out: &mut Vec<u8>, relocations: &mut Vec<AdobRelocation>, symbol: &str) {
    let disp_offset = out.len() + 1;
    out.push(0xE8);
    out.extend_from_slice(&0u32.to_le_bytes());
    relocations.push(AdobRelocation::new(
        disp_offset as u64,
        0,
        symbol,
        RelocationKind::PcRelative32,
        -4,
    ));
}

fn encode_call_r32(out: &mut Vec<u8>, target: u8) {
    out.push(0xFF);
    out.push(0xD0 | target);
}

fn encode_push_r32(out: &mut Vec<u8>, reg: u8) {
    out.push(0x50 + reg);
}

fn encode_add_esp(out: &mut Vec<u8>, bytes: u32) -> Result<(), CodegenError> {
    let bytes = u8::try_from(bytes).map_err(|_| {
        CodegenError::new("i686", "IA-32 call cleanup exceeds imm8 stack adjustment")
            .with_arch("i686")
            .with_instruction("Call")
    })?;
    out.extend_from_slice(&[0x83, 0xC4, bytes]);
    Ok(())
}

pub struct I686Backend {
    target: TargetDescriptor,
    capabilities: TargetCapabilities,
}

impl I686Backend {
    pub fn new(target: TargetDescriptor) -> Self {
        let capabilities = TargetCapabilities::for_architecture(&target.architecture);
        Self {
            target,
            capabilities,
        }
    }
}

impl CodegenBackend for I686Backend {
    fn target(&self) -> &TargetDescriptor {
        &self.target
    }

    fn capabilities(&self) -> TargetCapabilities {
        self.capabilities.clone()
    }

    fn lower_module(&mut self, module: &NativeModule) -> Result<NativeModule, CodegenError> {
        Ok(module.clone())
    }

    fn generate_function(&mut self, func: &MachineFunction) -> Result<Vec<u8>, CodegenError> {
        self.generate_function_with_relocations(func)
            .map(|(code, _)| code)
    }

    fn generate_function_with_relocations(
        &mut self,
        func: &MachineFunction,
    ) -> Result<(Vec<u8>, Vec<AdobRelocation>), CodegenError> {
        let resolver = i686_resolver();
        let mut expanded = Vec::new();
        for block in &func.blocks {
            for inst in &block.instructions {
                match inst {
                    MachineInstruction::ParallelMove { moves } => {
                        expanded.extend(resolver.resolve_moves(moves)?);
                    }
                    other => expanded.push(other.clone()),
                }
            }
        }

        let mut out = Vec::new();
        let mut relocations = Vec::new();
        let mut allocations = HashMap::new();

        for inst in &expanded {
            match inst {
                MachineInstruction::Nop => out.push(0x90),
                MachineInstruction::Return => out.push(0xC3),
                MachineInstruction::Move { dst, src } => {
                    let dst_gpr = match dst {
                        MachineOperand::Register(MachineRegister::Virtual(v)) => {
                            assign_vreg(*v, &mut allocations)?
                        }
                        MachineOperand::Register(MachineRegister::Physical(p))
                            if is_usable_gpr(p.0) =>
                        {
                            p.0
                        }
                        _ => return Err(unsupported_operand_form("i686", func, inst)),
                    };

                    match src {
                        MachineOperand::Immediate(value) => {
                            encode_mov_imm32(&mut out, dst_gpr, *value)?;
                        }
                        MachineOperand::Register(reg) => {
                            let src_gpr = resolve_gpr(*reg, &allocations)
                                .ok_or_else(|| unsupported_operand_form("i686", func, inst))?;
                            encode_mov_r32_r32(&mut out, dst_gpr, src_gpr);
                        }
                        _ => return Err(unsupported_operand_form("i686", func, inst)),
                    }
                }
                MachineInstruction::Call { target, num_args } => {
                    if *num_args as usize > CDECL_ARGUMENT_GPRS.len() {
                        return Err(CodegenError::new(
                            "i686",
                            format!(
                                "IA-32 cdecl call with {num_args} arguments is not implemented"
                            ),
                        )
                        .with_arch("i686")
                        .with_function(func.name.clone())
                        .with_instruction("Call"));
                    }
                    for &gpr in CDECL_ARGUMENT_GPRS[..*num_args as usize].iter().rev() {
                        encode_push_r32(&mut out, gpr);
                    }
                    match target {
                        MachineOperand::Symbol(name) => {
                            encode_call_rel32(&mut out, &mut relocations, name);
                        }
                        MachineOperand::Register(MachineRegister::Physical(p))
                            if is_usable_gpr(p.0) =>
                        {
                            encode_call_r32(&mut out, p.0);
                        }
                        _ => return Err(unsupported_operand_form("i686", func, inst)),
                    }
                    if *num_args > 0 {
                        let cleanup = u32::try_from(*num_args).map_err(|_| {
                            CodegenError::new("i686", "IA-32 call argument count overflows u32")
                                .with_arch("i686")
                                .with_function(func.name.clone())
                                .with_instruction("Call")
                        })? * 4;
                        encode_add_esp(&mut out, cleanup)?;
                    }
                }
                _ => return Err(unsupported_instruction("i686", func, inst)),
            }
        }

        Ok((out, relocations))
    }

    fn emit_object(&mut self, module: &NativeModule) -> Result<AdobObject, CodegenError> {
        let mut obj = AdobObject::new(self.target.clone());

        obj.memory_regions.push(MemoryRegion::new(
            ".text",
            0x08048000,
            512 * 1024,
            MemoryPermissions::Rx,
        ));

        let mut text_bytes = Vec::new();
        let mut text_relocations = Vec::new();
        for func in &module.functions {
            let offset = text_bytes.len() as u64;
            let (code, relocs) = self.generate_function_with_relocations(func)?;
            let size = code.len() as u64;
            text_bytes.extend_from_slice(&code);
            for mut reloc in relocs {
                reloc.offset += offset;
                text_relocations.push(reloc);
            }

            let sym = AdobSymbol::new_defined(
                0,
                func.name.clone(),
                SymbolKind::Function,
                0,
                offset,
                size,
            )
            .with_binding(SymbolBinding::Global)
            .with_visibility(SymbolVisibility::Default);
            obj.add_symbol(sym);

            if func.is_exported {
                obj.add_export(func.name.clone());
            }
        }

        let mut text_sec = AdobSection::new(".text", SectionKind::Text)
            .with_flags(section_flags::READ | section_flags::EXECUTE | section_flags::ALLOC)
            .with_alignment(4)
            .with_memory_region(".text")
            .with_data(text_bytes);
        text_sec.relocations = text_relocations;
        obj.add_section(text_sec);

        Ok(obj)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine_ir::PhysicalRegister;

    fn target() -> TargetDescriptor {
        TargetDescriptor::from_triple("i686-unknown-linux-gnu").expect("triple")
    }

    fn move_imm(dst: MachineOperand, value: i64) -> MachineInstruction {
        MachineInstruction::Move {
            dst,
            src: MachineOperand::Immediate(value),
        }
    }

    #[test]
    fn return_only_function_encodes_ret() {
        let mut backend = I686Backend::new(target());

        let mut func = MachineFunction::new("ret");
        func.blocks[0].push(MachineInstruction::Return);

        let code = backend.generate_function(&func).expect("encodes");
        assert_eq!(code, vec![0xC3]);
    }

    #[test]
    fn function_bodies_are_loud_errors() {
        let mut backend = I686Backend::new(target());

        let mut func = MachineFunction::new("boom");
        func.blocks[0].push(MachineInstruction::Add {
            dst: MachineOperand::reg(0),
            src: MachineOperand::imm(1),
        });
        func.blocks[0].push(MachineInstruction::Return);

        let err = backend
            .generate_function(&func)
            .expect_err("bodies are unimplemented");
        assert!(err.reason.contains("not implemented"), "{}", err.reason);
        assert_eq!(err.instruction.as_deref(), Some("Add"));
    }

    #[test]
    fn nop_and_return_are_accepted() {
        let mut backend = I686Backend::new(target());

        let mut func = MachineFunction::new("nop_ret");
        func.blocks[0].push(MachineInstruction::Nop);
        func.blocks[0].push(MachineInstruction::Return);

        let code = backend.generate_function(&func).expect("encodes nop+ret");
        assert_eq!(code, vec![0x90, 0xC3]);
    }

    #[test]
    fn move_immediate_to_eax_encodes_32_bit() {
        let mut backend = I686Backend::new(target());

        let mut func = MachineFunction::new("answer");
        func.blocks[0].push(move_imm(
            MachineOperand::Register(MachineRegister::Physical(PhysicalRegister::gpr(0))),
            42,
        ));
        func.blocks[0].push(MachineInstruction::Return);

        let code = backend.generate_function(&func).expect("encodes");
        assert_eq!(code, vec![0xB8, 0x2A, 0, 0, 0, 0xC3]);
    }

    #[test]
    fn virtual_move_sequence_allocates_32_bit_gpr() {
        let mut backend = I686Backend::new(target());

        let mut func = MachineFunction::new("answer_vreg");
        func.vreg_count = 1;
        func.blocks[0].push(move_imm(MachineOperand::reg(0), 42));
        func.blocks[0].push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister::gpr(0))),
            src: MachineOperand::reg(0),
        });
        func.blocks[0].push(MachineInstruction::Return);

        let code = backend.generate_function(&func).expect("encodes");
        assert_eq!(code, vec![0xB9, 0x2A, 0, 0, 0, 0x89, 0xC8, 0xC3]);
    }

    #[test]
    fn wide_immediate_is_refused() {
        let mut backend = I686Backend::new(target());

        let mut func = MachineFunction::new("wide");
        func.blocks[0].push(move_imm(
            MachineOperand::Register(MachineRegister::Physical(PhysicalRegister::gpr(0))),
            i64::from(u32::MAX) + 1,
        ));

        let err = backend
            .generate_function(&func)
            .expect_err("wide immediate refused");
        assert!(err.reason.contains("32-bit"), "{}", err.reason);
    }
}
