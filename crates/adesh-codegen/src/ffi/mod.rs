//! First-Class Native FFI (Foreign Function Interface) System.
//!
//! Models foreign function signatures, C ABI boundaries, external/imported symbols,
//! parameter marshalling, calling convention resolution, and unsafe boundary isolation.

use crate::abi::{AbiSpec, AbiType, ArgumentLocation, ReturnLocation};
use crate::error::CodegenError;
use crate::machine_ir::{
    MachineInstruction, MachineOperand, MachineRegister, MoveLocation, MoveOperation,
    PhysicalRegister, VirtualRegister,
};

/// Foreign calling convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ForeignCallingConvention {
    /// Standard C ABI (`extern "C"`).
    C,
    /// System default ABI (`extern "system"` - Win64 or SysV depending on OS).
    System,
    /// Fastcall convention.
    Fastcall,
    /// Stdcall (x86 32-bit legacy or Win32).
    Stdcall,
}

/// Supported foreign data types for FFI parameter and return types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForeignType {
    Void,
    Bool,
    Int8,
    Int16,
    Int32,
    Int64,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    SizeT,
    Float32,
    Float64,
    RawPointer {
        is_const: bool,
    },
    CString,
    FunctionPointer(Box<ForeignSignature>),
    Struct {
        fields: Vec<ForeignType>,
        size: usize,
        align: usize,
    },
}

impl ForeignType {
    /// Convert foreign type into target-independent `AbiType`.
    pub fn to_abi_type(&self) -> AbiType {
        match self {
            ForeignType::Void => AbiType::Void,
            ForeignType::Bool => AbiType::Integer {
                bits: 8,
                is_signed: false,
            },
            ForeignType::Int8 => AbiType::Integer {
                bits: 8,
                is_signed: true,
            },
            ForeignType::Int16 => AbiType::Integer {
                bits: 16,
                is_signed: true,
            },
            ForeignType::Int32 => AbiType::Integer {
                bits: 32,
                is_signed: true,
            },
            ForeignType::Int64 => AbiType::Integer {
                bits: 64,
                is_signed: true,
            },
            ForeignType::UInt8 => AbiType::Integer {
                bits: 8,
                is_signed: false,
            },
            ForeignType::UInt16 => AbiType::Integer {
                bits: 16,
                is_signed: false,
            },
            ForeignType::UInt32 => AbiType::Integer {
                bits: 32,
                is_signed: false,
            },
            ForeignType::UInt64 | ForeignType::SizeT => AbiType::Integer {
                bits: 64,
                is_signed: false,
            },
            ForeignType::Float32 => AbiType::Float { bits: 32 },
            ForeignType::Float64 => AbiType::Float { bits: 64 },
            ForeignType::RawPointer { .. }
            | ForeignType::CString
            | ForeignType::FunctionPointer(_) => AbiType::Pointer,
            ForeignType::Struct {
                fields,
                size,
                align,
            } => {
                let abi_fields = fields.iter().map(|f| f.to_abi_type()).collect();
                AbiType::Struct {
                    fields: abi_fields,
                    size: *size,
                    align: *align,
                }
            }
        }
    }

    pub fn size_in_bytes(&self) -> usize {
        self.to_abi_type().size_in_bytes()
    }
}

/// Parameter descriptor for foreign function declarations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignParam {
    pub name: String,
    pub param_type: ForeignType,
}

/// Signature of an external foreign function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignSignature {
    pub params: Vec<ForeignParam>,
    pub return_type: ForeignType,
    pub is_variadic: bool,
    pub convention: ForeignCallingConvention,
}

/// Complete foreign function declaration (`extern "C" fn ...`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignFunctionDeclaration {
    pub symbol_name: String,
    pub signature: ForeignSignature,
    pub is_unsafe: bool,
}

impl ForeignFunctionDeclaration {
    pub fn c_fn(
        symbol_name: impl Into<String>,
        params: Vec<(&str, ForeignType)>,
        return_type: ForeignType,
        is_variadic: bool,
    ) -> Self {
        let param_list = params
            .into_iter()
            .map(|(n, t)| ForeignParam {
                name: n.to_string(),
                param_type: t,
            })
            .collect();
        Self {
            symbol_name: symbol_name.into(),
            signature: ForeignSignature {
                params: param_list,
                return_type,
                is_variadic,
                convention: ForeignCallingConvention::C,
            },
            is_unsafe: true,
        }
    }

    /// Common declaration: `extern "C" fn puts(s: *const u8) -> i32`
    pub fn puts() -> Self {
        Self::c_fn(
            "puts",
            vec![("str", ForeignType::RawPointer { is_const: true })],
            ForeignType::Int32,
            false,
        )
    }

    /// Common declaration: `extern "C" fn printf(fmt: *const u8, ...) -> i32`
    pub fn printf() -> Self {
        Self::c_fn(
            "printf",
            vec![("fmt", ForeignType::RawPointer { is_const: true })],
            ForeignType::Int32,
            true,
        )
    }

    /// Common declaration: `extern "C" fn malloc(size: usize) -> *mut u8`
    pub fn malloc() -> Self {
        Self::c_fn(
            "malloc",
            vec![("size", ForeignType::SizeT)],
            ForeignType::RawPointer { is_const: false },
            false,
        )
    }

    /// Common declaration: `extern "C" fn free(ptr: *mut u8) -> ()`
    pub fn free() -> Self {
        Self::c_fn(
            "free",
            vec![("ptr", ForeignType::RawPointer { is_const: false })],
            ForeignType::Void,
            false,
        )
    }
}

/// Lowers an FFI boundary call into target MachineIR instructions.
pub struct FfiCallLowerer;

impl FfiCallLowerer {
    /// Lower an external native function call using the target ABI specification.
    ///
    /// Register arguments are emitted as a single `ParallelMove` so that a source
    /// allocated to another argument's register is never clobbered before it is read.
    /// Stack arguments are stored first, while every source is still intact. Shapes
    /// this lowering cannot represent (split `Pair` arguments, sret returns, extra
    /// variadic arguments) are rejected instead of being miscompiled.
    pub fn lower_call(
        decl: &ForeignFunctionDeclaration,
        arg_vregs: &[VirtualRegister],
        abi: &dyn AbiSpec,
        ret_vreg: Option<VirtualRegister>,
    ) -> Result<Vec<MachineInstruction>, CodegenError> {
        let unsupported = |reason: String| {
            CodegenError::new("ffi", reason).with_function(decl.symbol_name.clone())
        };

        let params = &decl.signature.params;
        if arg_vregs.len() != params.len() {
            return Err(unsupported(if arg_vregs.len() > params.len() {
                format!(
                    "call to `{}` passes {} arguments but declares {}; extra variadic arguments are not supported by FFI lowering yet",
                    decl.symbol_name,
                    arg_vregs.len(),
                    params.len()
                )
            } else {
                format!(
                    "call to `{}` passes {} arguments but requires {}",
                    decl.symbol_name,
                    arg_vregs.len(),
                    params.len()
                )
            }));
        }

        let ret_abi = decl.signature.return_type.to_abi_type();
        let ret_loc = abi.classify_return(&ret_abi);
        let is_sret = matches!(ret_loc, ReturnLocation::HiddenSret(_));

        let (effective_vregs, effective_types): (Vec<VirtualRegister>, Vec<AbiType>) = if is_sret {
            if let Some(buf_vreg) = ret_vreg {
                let mut vregs = vec![buf_vreg];
                vregs.extend_from_slice(arg_vregs);
                let mut types = vec![AbiType::Pointer];
                types.extend(params.iter().map(|p| p.param_type.to_abi_type()));
                (vregs, types)
            } else {
                return Err(unsupported(format!(
                    "`{}` returns a large aggregate through a hidden sret pointer; a destination buffer is required",
                    decl.symbol_name
                )));
            }
        } else {
            (
                arg_vregs.to_vec(),
                params.iter().map(|p| p.param_type.to_abi_type()).collect(),
            )
        };

        let locations = abi.classify_arguments(&effective_types);
        if locations.len() != effective_vregs.len() {
            return Err(unsupported(format!(
                "ABI classified {} argument locations for {} arguments of `{}`",
                locations.len(),
                effective_vregs.len(),
                decl.symbol_name
            )));
        }

        let shadow_space = abi.shadow_space().0 as i32;
        let mut max_stack_offset = 0i32;
        for loc in &locations {
            match loc {
                ArgumentLocation::Stack(stack_slot)
                | ArgumentLocation::IndirectStack(stack_slot) => {
                    let offset_from_rsp = stack_slot.offset - 16;
                    let end_offset = offset_from_rsp + stack_slot.size.max(8) as i32;
                    max_stack_offset = max_stack_offset.max(end_offset);
                }
                _ => {}
            }
        }
        let max_outgoing = shadow_space.max(max_stack_offset);
        let total_outgoing = if max_outgoing > 0 {
            (max_outgoing + 15) & !15
        } else {
            0
        };

        let mut instructions = Vec::new();
        if total_outgoing > 0 {
            instructions.push(MachineInstruction::Sub {
                dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(4))), // RSP
                src: MachineOperand::Immediate(total_outgoing as i64),
            });
        }

        let mut reg_moves: Vec<MoveOperation> = Vec::new();
        let mut fp_reg_count = 0u8;

        for (&vreg, loc) in effective_vregs.iter().zip(locations.iter()) {
            let src = MoveLocation::VirtualRegister(vreg);
            match loc {
                ArgumentLocation::Register(phys) | ArgumentLocation::IndirectByReference(phys) => {
                    reg_moves.push(MoveOperation::new_qword(
                        MoveLocation::PhysicalRegister(*phys),
                        src,
                    ));
                }
                ArgumentLocation::FloatRegister(phys) | ArgumentLocation::VectorRegister(phys) => {
                    fp_reg_count += 1;
                    reg_moves.push(MoveOperation::new_qword(
                        MoveLocation::PhysicalRegister(*phys),
                        src,
                    ));
                }
                ArgumentLocation::Stack(stack_slot)
                | ArgumentLocation::IndirectStack(stack_slot) => {
                    instructions.push(MachineInstruction::Store {
                        dst: MachineOperand::Memory {
                            base: MachineRegister::Physical(PhysicalRegister(4)), // RSP
                            offset: stack_slot.offset - 16,
                            index: None,
                        },
                        src: MachineOperand::Register(MachineRegister::Virtual(vreg)),
                        size: stack_slot.size as u8,
                    });
                }
                ArgumentLocation::Pair(..) => {
                    return Err(unsupported(format!(
                        "argument of `{}` must be split across two registers; one virtual register cannot supply both halves",
                        decl.symbol_name
                    )));
                }
            }
        }

        if !reg_moves.is_empty() {
            instructions.push(MachineInstruction::ParallelMove { moves: reg_moves });
        }

        // 3. Handle SysV variadic AL register count
        if decl.signature.is_variadic
            && abi.variadic_rules() == crate::abi::VariadicRules::SysVAlVectorCount
        {
            instructions.push(MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))), // RAX / AL
                src: MachineOperand::Immediate(fp_reg_count as i64),
            });
        }

        // 4. Emit the actual Call instruction
        instructions.push(MachineInstruction::Call {
            target: MachineOperand::Symbol(decl.symbol_name.clone()),
            num_args: effective_vregs.len(),
        });

        if total_outgoing > 0 {
            instructions.push(MachineInstruction::Add {
                dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(4))), // RSP
                src: MachineOperand::Immediate(total_outgoing as i64),
            });
        }

        // 5. Retrieve return value into destination virtual register
        if let Some(dest) = ret_vreg {
            match ret_loc {
                ReturnLocation::Register(phys)
                | ReturnLocation::FloatRegister(phys)
                | ReturnLocation::VectorRegister(phys) => {
                    instructions.push(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(dest)),
                        src: MachineOperand::Register(MachineRegister::Physical(phys)),
                    });
                }
                ReturnLocation::HiddenSret(_) => {
                    // Callee returns sret pointer in RAX per Win64 & SysV ABI
                    instructions.push(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(dest)),
                        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(
                            0,
                        ))),
                    });
                }
                ReturnLocation::Pair(..) => {
                    return Err(unsupported(format!(
                        "`{}` returns its value in two registers; capturing a register-pair return is not supported yet",
                        decl.symbol_name
                    )));
                }
                ReturnLocation::Void => {
                    return Err(unsupported(format!(
                        "`{}` returns void but the call expects a result value",
                        decl.symbol_name
                    )));
                }
            }
        }

        Ok(instructions)
    }

    /// Lower an external variadic function call, providing explicit types for the extra arguments.
    pub fn lower_variadic_call(
        decl: &ForeignFunctionDeclaration,
        arg_vregs: &[VirtualRegister],
        extra_types: &[ForeignType],
        abi: &dyn AbiSpec,
        ret_vreg: Option<VirtualRegister>,
    ) -> Result<Vec<MachineInstruction>, CodegenError> {
        let unsupported = |reason: String| {
            CodegenError::new("ffi", reason).with_function(decl.symbol_name.clone())
        };

        if !decl.signature.is_variadic {
            return Err(unsupported(format!(
                "function `{}` is not declared as variadic",
                decl.symbol_name
            )));
        }

        let params = &decl.signature.params;
        if arg_vregs.len() != params.len() + extra_types.len() {
            return Err(unsupported(format!(
                "call to `{}` passes {} arguments but {} types provided ({} fixed + {} extra)",
                decl.symbol_name,
                arg_vregs.len(),
                params.len() + extra_types.len(),
                params.len(),
                extra_types.len(),
            )));
        }

        let ret_abi = decl.signature.return_type.to_abi_type();
        let ret_loc = abi.classify_return(&ret_abi);
        let is_sret = matches!(ret_loc, ReturnLocation::HiddenSret(_));

        let (effective_vregs, effective_types): (Vec<VirtualRegister>, Vec<AbiType>) = if is_sret {
            if let Some(buf_vreg) = ret_vreg {
                let mut vregs = vec![buf_vreg];
                vregs.extend_from_slice(arg_vregs);
                let mut types = vec![AbiType::Pointer];
                types.extend(params.iter().map(|p| p.param_type.to_abi_type()));
                types.extend(extra_types.iter().map(|t| t.to_abi_type()));
                (vregs, types)
            } else {
                return Err(unsupported(format!(
                    "`{}` returns a large aggregate through a hidden sret pointer; a destination buffer is required",
                    decl.symbol_name
                )));
            }
        } else {
            let mut types: Vec<AbiType> =
                params.iter().map(|p| p.param_type.to_abi_type()).collect();
            types.extend(extra_types.iter().map(|t| t.to_abi_type()));
            (arg_vregs.to_vec(), types)
        };

        let locations = abi.classify_arguments(&effective_types);
        if locations.len() != effective_vregs.len() {
            return Err(unsupported(format!(
                "ABI classified {} argument locations for {} arguments of `{}`",
                locations.len(),
                effective_vregs.len(),
                decl.symbol_name
            )));
        }

        let shadow_space = abi.shadow_space().0 as i32;
        let mut max_stack_offset = 0i32;
        for loc in &locations {
            match loc {
                ArgumentLocation::Stack(stack_slot)
                | ArgumentLocation::IndirectStack(stack_slot) => {
                    let offset_from_rsp = stack_slot.offset - 16;
                    let end_offset = offset_from_rsp + stack_slot.size.max(8) as i32;
                    max_stack_offset = max_stack_offset.max(end_offset);
                }
                _ => {}
            }
        }
        let max_outgoing = shadow_space.max(max_stack_offset);
        let total_outgoing = if max_outgoing > 0 {
            (max_outgoing + 15) & !15
        } else {
            0
        };

        let mut instructions = Vec::new();
        if total_outgoing > 0 {
            instructions.push(MachineInstruction::Sub {
                dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(4))), // RSP
                src: MachineOperand::Immediate(total_outgoing as i64),
            });
        }

        let mut reg_moves: Vec<MoveOperation> = Vec::new();
        let mut fp_reg_count = 0u8;

        for (&vreg, loc) in effective_vregs.iter().zip(locations.iter()) {
            let src = MoveLocation::VirtualRegister(vreg);
            match loc {
                ArgumentLocation::Register(phys) | ArgumentLocation::IndirectByReference(phys) => {
                    reg_moves.push(MoveOperation::new_qword(
                        MoveLocation::PhysicalRegister(*phys),
                        src,
                    ));
                }
                ArgumentLocation::FloatRegister(phys) | ArgumentLocation::VectorRegister(phys) => {
                    fp_reg_count += 1;
                    reg_moves.push(MoveOperation::new_qword(
                        MoveLocation::PhysicalRegister(*phys),
                        src,
                    ));
                }
                ArgumentLocation::Stack(stack_slot)
                | ArgumentLocation::IndirectStack(stack_slot) => {
                    instructions.push(MachineInstruction::Store {
                        dst: MachineOperand::Memory {
                            base: MachineRegister::Physical(PhysicalRegister(4)), // RSP
                            offset: stack_slot.offset - 16,
                            index: None,
                        },
                        src: MachineOperand::Register(MachineRegister::Virtual(vreg)),
                        size: stack_slot.size as u8,
                    });
                }
                ArgumentLocation::Pair(..) => {
                    return Err(unsupported(format!(
                        "argument of `{}` must be split across two registers; one virtual register cannot supply both halves",
                        decl.symbol_name
                    )));
                }
            }
        }

        if !reg_moves.is_empty() {
            instructions.push(MachineInstruction::ParallelMove { moves: reg_moves });
        }

        // 3. Handle SysV variadic AL register count
        if abi.variadic_rules() == crate::abi::VariadicRules::SysVAlVectorCount {
            instructions.push(MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))), // RAX / AL
                src: MachineOperand::Immediate(fp_reg_count as i64),
            });
        }

        // 4. Emit Call instruction
        instructions.push(MachineInstruction::Call {
            target: MachineOperand::Symbol(decl.symbol_name.clone()),
            num_args: effective_vregs.len(),
        });

        if total_outgoing > 0 {
            instructions.push(MachineInstruction::Add {
                dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(4))), // RSP
                src: MachineOperand::Immediate(total_outgoing as i64),
            });
        }

        // 5. Retrieve return value
        if let Some(dest) = ret_vreg {
            match ret_loc {
                ReturnLocation::Register(phys)
                | ReturnLocation::FloatRegister(phys)
                | ReturnLocation::VectorRegister(phys) => {
                    instructions.push(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(dest)),
                        src: MachineOperand::Register(MachineRegister::Physical(phys)),
                    });
                }
                ReturnLocation::HiddenSret(_) => {
                    instructions.push(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(dest)),
                        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(
                            0,
                        ))),
                    });
                }
                ReturnLocation::Pair(..) => {
                    return Err(unsupported(format!(
                        "`{}` returns its value in two registers; capturing a register-pair return is not supported yet",
                        decl.symbol_name
                    )));
                }
                ReturnLocation::Void => {
                    return Err(unsupported(format!(
                        "`{}` returns void but the call expects a result value",
                        decl.symbol_name
                    )));
                }
            }
        }

        Ok(instructions)
    }
}
