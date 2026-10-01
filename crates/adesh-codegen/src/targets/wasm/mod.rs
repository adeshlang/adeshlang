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

    fn reg_to_local_idx(reg: &MachineRegister) -> u8 {
        match reg {
            MachineRegister::Virtual(v) => (v.0 % 32) as u8,
            MachineRegister::Physical(p) => p.0 % 32,
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
        let mut body = Vec::new();
        // 1 local declaration group: 32 i64 locals
        body.push(0x01); // 1 local entry
        body.push(32); // 32 locals
        body.push(0x7E); // type: i64

        for block in &func.blocks {
            for inst in &block.instructions {
                match inst {
                    MachineInstruction::Nop => {
                        body.push(0x01); // nop
                    }
                    MachineInstruction::Move { dst, src } => {
                        match src {
                            MachineOperand::Immediate(val) => {
                                body.push(0x42); // i64.const
                                Self::encode_sleb128(*val, &mut body);
                            }
                            MachineOperand::Register(r) => {
                                body.push(0x20); // local.get
                                body.push(Self::reg_to_local_idx(r));
                            }
                            _ => {
                                body.push(0x42); // i64.const 0
                                Self::encode_sleb128(0, &mut body);
                            }
                        }
                        if let MachineOperand::Register(r) = dst {
                            body.push(0x21); // local.set
                            body.push(Self::reg_to_local_idx(r));
                        }
                    }
                    MachineInstruction::Add { dst, src } => {
                        if let MachineOperand::Register(r) = dst {
                            body.push(0x20); // local.get
                            body.push(Self::reg_to_local_idx(r));
                        }
                        match src {
                            MachineOperand::Immediate(val) => {
                                body.push(0x42);
                                Self::encode_sleb128(*val, &mut body);
                            }
                            MachineOperand::Register(r) => {
                                body.push(0x20);
                                body.push(Self::reg_to_local_idx(r));
                            }
                            _ => {}
                        }
                        body.push(0x7C); // i64.add
                        if let MachineOperand::Register(r) = dst {
                            body.push(0x21); // local.set
                            body.push(Self::reg_to_local_idx(r));
                        }
                    }
                    MachineInstruction::Sub { dst, src } => {
                        if let MachineOperand::Register(r) = dst {
                            body.push(0x20); // local.get
                            body.push(Self::reg_to_local_idx(r));
                        }
                        match src {
                            MachineOperand::Immediate(val) => {
                                body.push(0x42);
                                Self::encode_sleb128(*val, &mut body);
                            }
                            MachineOperand::Register(r) => {
                                body.push(0x20);
                                body.push(Self::reg_to_local_idx(r));
                            }
                            _ => {}
                        }
                        body.push(0x7D); // i64.sub
                        if let MachineOperand::Register(r) = dst {
                            body.push(0x21); // local.set
                            body.push(Self::reg_to_local_idx(r));
                        }
                    }
                    MachineInstruction::Mul { dst, src } => {
                        if let MachineOperand::Register(r) = dst {
                            body.push(0x20); // local.get
                            body.push(Self::reg_to_local_idx(r));
                        }
                        match src {
                            MachineOperand::Immediate(val) => {
                                body.push(0x42);
                                Self::encode_sleb128(*val, &mut body);
                            }
                            MachineOperand::Register(r) => {
                                body.push(0x20);
                                body.push(Self::reg_to_local_idx(r));
                            }
                            _ => {}
                        }
                        body.push(0x7E); // i64.mul
                        if let MachineOperand::Register(r) = dst {
                            body.push(0x21); // local.set
                            body.push(Self::reg_to_local_idx(r));
                        }
                    }
                    MachineInstruction::Div { dst, src } => {
                        if let MachineOperand::Register(r) = dst {
                            body.push(0x20); // local.get
                            body.push(Self::reg_to_local_idx(r));
                        }
                        match src {
                            MachineOperand::Immediate(val) => {
                                body.push(0x42);
                                Self::encode_sleb128(*val, &mut body);
                            }
                            MachineOperand::Register(r) => {
                                body.push(0x20);
                                body.push(Self::reg_to_local_idx(r));
                            }
                            _ => {}
                        }
                        body.push(0x7F); // i64.div_s
                        if let MachineOperand::Register(r) = dst {
                            body.push(0x21); // local.set
                            body.push(Self::reg_to_local_idx(r));
                        }
                    }
                    _ => {}
                }
            }
        }

        // Return result from local 0
        body.push(0x20); // local.get
        body.push(0x00); // local 0
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
