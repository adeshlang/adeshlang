//! First-Class Native FFI (Foreign Function Interface) System.
//!
//! Models foreign function signatures, C ABI boundaries, external/imported symbols,
//! parameter marshalling, calling convention resolution, and unsafe boundary isolation.

use crate::abi::{AbiSpec, AbiType, ArgumentLocation, ReturnLocation};
use crate::machine_ir::{
    MachineInstruction, MachineOperand, MachineRegister, PhysicalRegister, VirtualRegister,
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
    RawPointer { is_const: bool },
    CString,
    FunctionPointer(Box<ForeignSignature>),
}

impl ForeignType {
    /// Convert foreign type into target-independent `AbiType`.
    pub fn to_abi_type(&self) -> AbiType {
        match self {
            ForeignType::Void => AbiType::Void,
            ForeignType::Bool => AbiType::Integer { bits: 8, is_signed: false },
            ForeignType::Int8 => AbiType::Integer { bits: 8, is_signed: true },
            ForeignType::Int16 => AbiType::Integer { bits: 16, is_signed: true },
            ForeignType::Int32 => AbiType::Integer { bits: 32, is_signed: true },
            ForeignType::Int64 => AbiType::Integer { bits: 64, is_signed: true },
            ForeignType::UInt8 => AbiType::Integer { bits: 8, is_signed: false },
            ForeignType::UInt16 => AbiType::Integer { bits: 16, is_signed: false },
            ForeignType::UInt32 => AbiType::Integer { bits: 32, is_signed: false },
            ForeignType::UInt64 | ForeignType::SizeT => AbiType::Integer { bits: 64, is_signed: false },
            ForeignType::Float32 => AbiType::Float { bits: 32 },
            ForeignType::Float64 => AbiType::Float { bits: 64 },
            ForeignType::RawPointer { .. } | ForeignType::CString | ForeignType::FunctionPointer(_) => {
                AbiType::Pointer
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
    /// Generates operand moves into physical registers / stack slots, handles shadow space,
    /// sets up AL register for SysV variadic floating-point counts, and places return value into vreg.
    pub fn lower_call(
        decl: &ForeignFunctionDeclaration,
        arg_vregs: &[VirtualRegister],
        abi: &dyn AbiSpec,
        ret_vreg: Option<VirtualRegister>,
    ) -> Vec<MachineInstruction> {
        let mut instructions = Vec::new();

        // 1. Map argument types to AbiTypes
        let arg_abi_types: Vec<AbiType> = decl
            .signature
            .params
            .iter()
            .map(|p| p.param_type.to_abi_type())
            .collect();

        // 2. Classify locations according to ABI
        let locations = abi.classify_arguments(&arg_abi_types);

        let mut fp_reg_count = 0u8;

        for (i, (&vreg, loc)) in arg_vregs.iter().zip(locations.iter()).enumerate() {
            let _ = i;
            match loc {
                ArgumentLocation::Register(phys) => {
                    instructions.push(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Physical(*phys)),
                        src: MachineOperand::Register(MachineRegister::Virtual(vreg)),
                    });
                }
                ArgumentLocation::FloatRegister(phys) => {
                    fp_reg_count += 1;
                    instructions.push(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Physical(*phys)),
                        src: MachineOperand::Register(MachineRegister::Virtual(vreg)),
                    });
                }
                ArgumentLocation::VectorRegister(phys) => {
                    fp_reg_count += 1;
                    instructions.push(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Physical(*phys)),
                        src: MachineOperand::Register(MachineRegister::Virtual(vreg)),
                    });
                }
                ArgumentLocation::Stack(stack_slot) => {
                    instructions.push(MachineInstruction::Store {
                        dst: MachineOperand::StackSlot(stack_slot.offset),
                        src: MachineOperand::Register(MachineRegister::Virtual(vreg)),
                        size: stack_slot.size as u8,
                    });
                }
                ArgumentLocation::IndirectByReference(phys) => {
                    instructions.push(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Physical(*phys)),
                        src: MachineOperand::Register(MachineRegister::Virtual(vreg)),
                    });
                }
                ArgumentLocation::Pair(first, second) => {
                    instructions.push(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Physical(*first)),
                        src: MachineOperand::Register(MachineRegister::Virtual(vreg)),
                    });
                    instructions.push(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Physical(*second)),
                        src: MachineOperand::Register(MachineRegister::Virtual(vreg)),
                    });
                }
            }
        }

        // 3. Handle SysV variadic AL register count
        if decl.signature.is_variadic && abi.variadic_rules() == crate::abi::VariadicRules::SysVAlVectorCount {
            instructions.push(MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))), // RAX / AL
                src: MachineOperand::Immediate(fp_reg_count as i64),
            });
        }

        // 4. Emit the actual Call instruction
        instructions.push(MachineInstruction::Call {
            target: MachineOperand::Symbol(decl.symbol_name.clone()),
            num_args: arg_vregs.len(),
        });

        // 5. Retrieve return value into destination virtual register
        if let Some(dest) = ret_vreg {
            let ret_abi = decl.signature.return_type.to_abi_type();
            match abi.classify_return(&ret_abi) {
                ReturnLocation::Register(phys) => {
                    instructions.push(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(dest)),
                        src: MachineOperand::Register(MachineRegister::Physical(phys)),
                    });
                }
                ReturnLocation::FloatRegister(phys) => {
                    instructions.push(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(dest)),
                        src: MachineOperand::Register(MachineRegister::Physical(phys)),
                    });
                }
                ReturnLocation::VectorRegister(phys) => {
                    instructions.push(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(dest)),
                        src: MachineOperand::Register(MachineRegister::Physical(phys)),
                    });
                }
                ReturnLocation::Pair(first, _) => {
                    instructions.push(MachineInstruction::Move {
                        dst: MachineOperand::Register(MachineRegister::Virtual(dest)),
                        src: MachineOperand::Register(MachineRegister::Physical(first)),
                    });
                }
                _ => {}
            }
        }

        instructions
    }
}
