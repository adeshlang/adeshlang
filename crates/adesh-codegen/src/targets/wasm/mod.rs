//! WebAssembly (WASM32 / WASM64) Native Codegen Backend.

use crate::backend::CodegenBackend;
use crate::error::CodegenError;
use crate::machine_ir::{
    MachineFunction, MachineInstruction, MachineOperand, MachineRegister, NativeModule,
};
use adesh_object::{
    AdobObject, AdobSection, AdobSymbol, SectionKind, SymbolBinding, SymbolKind, SymbolVisibility,
    TargetCapabilities, TargetDescriptor, section_flags,
};

pub struct WasmBackend {
    target: TargetDescriptor,
    capabilities: TargetCapabilities,
}

impl WasmBackend {
    pub fn new(target: TargetDescriptor) -> Self {
        let capabilities = TargetCapabilities::for_architecture(&target.architecture);
        Self {
            target,
            capabilities,
        }
    }

    fn encode_sleb128(mut value: i64, buf: &mut Vec<u8>) {
        let mut more = true;
        while more {
            let mut byte = (value & 0x7F) as u8;
            value >>= 7;
            let sign_bit = (byte & 0x40) != 0;
            if (value == 0 && !sign_bit) || (value == -1 && sign_bit) {
                more = false;
            } else {
                byte |= 0x80;
            }
            buf.push(byte);
        }
    }

    fn encode_uleb128(mut value: u64, buf: &mut Vec<u8>) {
        loop {
            let byte = (value & 0x7F) as u8;
            value >>= 7;
            if value == 0 {
                buf.push(byte);
                return;
            }
            buf.push(byte | 0x80);
        }
    }

    /// The local index assigned to `op`'s register, if it is a known register.
    fn local_of(op: &MachineOperand, locals: &[MachineRegister]) -> Option<u8> {
        match op {
            MachineOperand::Register(r) => locals
                .iter()
                .position(|x| x == r)
                .and_then(|i| u8::try_from(i).ok()),
            _ => None,
        }
    }

    /// Push the value of `src` (register or immediate) onto the operand stack.
    fn push_value(src: &MachineOperand, locals: &[MachineRegister], body: &mut Vec<u8>) -> bool {
        match src {
            MachineOperand::Immediate(val) => {
                body.push(0x42); // i64.const
                Self::encode_sleb128(*val, body);
                true
            }
            MachineOperand::Register(r) => {
                if let Some(idx) = Self::local_of(&MachineOperand::Register(*r), locals) {
                    body.push(0x20); // local.get
                    body.push(idx);
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}

impl CodegenBackend for WasmBackend {
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
        // Assign a distinct WASM local to every register the function
        // references. Mapping register ids with `% 32` (as before) aliased
        // distinct registers (e.g. virtual registers 0 and 32) onto the same
        // local, silently miscompiling one of them.
        let mut locals: Vec<MachineRegister> = Vec::new();
        for block in &func.blocks {
            for inst in &block.instructions {
                for reg in inst.uses().into_iter().chain(inst.defs()) {
                    if !locals.contains(&reg) {
                        locals.push(reg);
                    }
                }
            }
        }

        let mut body = Vec::new();
        // 1 local declaration group: N i64 locals
        body.push(0x01); // 1 local entry
        Self::encode_uleb128(locals.len() as u64, &mut body);
        body.push(0x7E); // type: i64

        'blocks: for block in &func.blocks {
            for inst in &block.instructions {
                match inst {
                    MachineInstruction::Nop => {
                        body.push(0x01); // nop
                    }
                    MachineInstruction::Return => {
                        // A WASM function ends implicitly; nothing to encode
                        // for the return, and the rest of the function is
                        // unreachable.
                        break 'blocks;
                    }
                    MachineInstruction::Move { dst, src } => {
                        if !Self::push_value(src, &locals, &mut body) {
                            return Err(crate::targets::unsupported_operand_form(
                                "wasm", func, inst,
                            ));
                        }
                        match Self::local_of(dst, &locals) {
                            Some(idx) => {
                                body.push(0x21); // local.set
                                body.push(idx);
                            }
                            None => {
                                return Err(crate::targets::unsupported_operand_form(
                                    "wasm", func, inst,
                                ));
                            }
                        }
                    }
                    MachineInstruction::Add { dst, src }
                    | MachineInstruction::Sub { dst, src }
                    | MachineInstruction::Mul { dst, src }
                    | MachineInstruction::Div { dst, src } => {
                        let opcode = match inst {
                            MachineInstruction::Add { .. } => 0x7C, // i64.add
                            MachineInstruction::Sub { .. } => 0x7D, // i64.sub
                            MachineInstruction::Mul { .. } => 0x7E, // i64.mul
                            _ => 0x7F,                              // i64.div_s
                        };
                        match Self::local_of(dst, &locals) {
                            Some(idx) => {
                                body.push(0x20); // local.get
                                body.push(idx);
                            }
                            None => {
                                return Err(crate::targets::unsupported_operand_form(
                                    "wasm", func, inst,
                                ));
                            }
                        }
                        if !Self::push_value(src, &locals, &mut body) {
                            return Err(crate::targets::unsupported_operand_form(
                                "wasm", func, inst,
                            ));
                        }
                        body.push(opcode);
                        match Self::local_of(dst, &locals) {
                            Some(idx) => {
                                body.push(0x21); // local.set
                                body.push(idx);
                            }
                            None => {
                                return Err(crate::targets::unsupported_operand_form(
                                    "wasm", func, inst,
                                ));
                            }
                        }
                    }
                    // Only the instruction forms above are implemented.
                    // Silently skipping anything else would emit code that
                    // computes the wrong result, so refuse loudly.
                    unsupported => {
                        return Err(crate::targets::unsupported_instruction(
                            "wasm",
                            func,
                            unsupported,
                        ));
                    }
                }
            }
        }

        // Implicit function result. With no locals there is nothing to
        // return but a constant (an undeclared `local.get 0` would be
        // invalid WASM).
        if locals.is_empty() {
            body.push(0x42); // i64.const
            Self::encode_sleb128(0, &mut body);
        } else {
            body.push(0x20); // local.get
            body.push(0x00); // local 0
        }
        body.push(0x0B); // end
        Ok(body)
    }

    fn emit_object(&mut self, module: &NativeModule) -> Result<AdobObject, CodegenError> {
        let mut obj = AdobObject::new(self.target.clone());
        let mut text_bytes = Vec::new();

        for func in &module.functions {
            let offset = text_bytes.len() as u64;
            let code = self.generate_function(func)?;
            let size = code.len() as u64;
            text_bytes.extend_from_slice(&code);

            let sym = AdobSymbol::new_defined(
                0,
                func.name.clone(),
                SymbolKind::Function,
                0,
                offset,
                size,
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

        let text_sec = AdobSection::new(".text", SectionKind::Text)
            .with_flags(section_flags::READ | section_flags::EXECUTE | section_flags::ALLOC)
            .with_alignment(1)
            .with_data(text_bytes);
        obj.add_section(text_sec);

        Ok(obj)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine_ir::VirtualRegister;
    use adesh_object::TargetDescriptor;

    #[test]
    fn locals_are_assigned_per_register_not_modulo_32() {
        let target = TargetDescriptor::from_triple("wasm32-unknown-unknown").expect("triple");
        let mut backend = WasmBackend::new(target);

        // Two virtual registers whose ids differ by 32 previously aliased
        // onto the same WASM local.
        let mut func = MachineFunction::new("aliased");
        let v0 = VirtualRegister(0);
        let v32 = VirtualRegister(32);
        func.blocks[0].push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
            src: MachineOperand::Immediate(1),
        });
        func.blocks[0].push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(v32)),
            src: MachineOperand::Immediate(2),
        });
        func.blocks[0].push(MachineInstruction::Add {
            dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
            src: MachineOperand::Register(MachineRegister::Virtual(v32)),
        });
        func.blocks[0].push(MachineInstruction::Return);

        let body = backend.generate_function(&func).expect("encodes");
        // Two locals are declared (not 32, and not 1 aliased local).
        assert_eq!(&body[..4], &[0x01, 0x02, 0x7E, 0x42][..]);
        // v0 gets local 0 and v32 gets local 1 (distinct).
        let local_sets: Vec<u8> = body.to_vec();
        assert!(local_sets.windows(2).any(|w| w == [0x21, 0x00]));
        assert!(local_sets.windows(2).any(|w| w == [0x21, 0x01]));
    }

    #[test]
    fn return_only_function_emits_valid_body() {
        let target = TargetDescriptor::from_triple("wasm32-unknown-unknown").expect("triple");
        let mut backend = WasmBackend::new(target);

        let mut func = MachineFunction::new("ret");
        func.blocks[0].push(MachineInstruction::Return);

        let body = backend.generate_function(&func).expect("encodes");
        // Zero locals declared, and the implicit result is a constant (not
        // an undeclared `local.get 0`).
        assert_eq!(&body[..5], &[0x01, 0x00, 0x7E, 0x42, 0x00][..]);
        assert_eq!(*body.last().unwrap(), 0x0B);
    }

    #[test]
    fn unsupported_instructions_are_loud_errors() {
        let target = TargetDescriptor::from_triple("wasm32-unknown-unknown").expect("triple");
        let mut backend = WasmBackend::new(target);

        let mut func = MachineFunction::new("boom");
        func.blocks[0].push(MachineInstruction::Compare {
            lhs: MachineOperand::Register(MachineRegister::Virtual(VirtualRegister(0))),
            rhs: MachineOperand::Immediate(1),
        });

        let err = backend
            .generate_function(&func)
            .expect_err("Compare is unimplemented");
        assert!(err.reason.contains("not implemented"), "{}", err.reason);
        assert_eq!(err.instruction.as_deref(), Some("Compare"));

        // A memory source has no implemented form either.
        let mut func = MachineFunction::new("boom2");
        func.blocks[0].push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(VirtualRegister(0))),
            src: MachineOperand::StackSlot(-8),
        });
        let err = backend
            .generate_function(&func)
            .expect_err("stack-slot move is unimplemented");
        assert!(err.reason.contains("no implemented"), "{}", err.reason);
    }
}
