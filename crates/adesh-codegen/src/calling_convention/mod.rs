pub mod parallel_move;
pub use crate::abi::{AbiType, ArgumentLocation, ReturnLocation, StackArgument};
pub use crate::machine_ir::{MoveLocation, MoveOperation, RegisterClass};
pub use parallel_move::{
    ParallelMoveResolver, resolve_abi_call_arguments, resolve_call_arguments,
    resolve_call_arguments_gpr,
};

use crate::machine_ir::{PhysicalRegister, VirtualRegister};

pub trait CallingConvention: Send + Sync {
    fn name(&self) -> &'static str;
    fn arg_registers(&self) -> &[PhysicalRegister];
    fn fp_arg_registers(&self) -> &[PhysicalRegister] {
        &[]
    }
    fn return_registers(&self) -> &[PhysicalRegister];
    fn fp_return_registers(&self) -> &[PhysicalRegister] {
        &[]
    }
    fn caller_saved_registers(&self) -> &[PhysicalRegister];
    fn callee_saved_registers(&self) -> &[PhysicalRegister];
    fn stack_alignment(&self) -> u32;
    fn shadow_space(&self) -> u32 {
        0
    }

    /// Classifies return type according to full ABI rules.
    fn classify_return_location(&self, ret_type: &AbiType) -> ReturnLocation {
        match ret_type {
            AbiType::Void => ReturnLocation::Void,
            AbiType::Float { .. } => ReturnLocation::FloatRegister(PhysicalRegister::xmm(0)),
            AbiType::Vector { .. } => ReturnLocation::VectorRegister(PhysicalRegister::xmm(0)),
            _ => ReturnLocation::Register(PhysicalRegister(0)),
        }
    }

    /// Classify incoming arguments according to full ABI types.
    fn classify_incoming_abi_args(&self, param_types: &[AbiType]) -> Vec<ArgumentLocation> {
        let classes: Vec<RegisterClass> = param_types
            .iter()
            .map(|t| match t {
                AbiType::Float { .. } => RegisterClass::Float,
                _ => RegisterClass::Gpr,
            })
            .collect();
        self.classify_incoming_args(&classes)
    }

    /// Classify incoming arguments into register or stack locations on function entry.
    fn classify_incoming_args(&self, param_classes: &[RegisterClass]) -> Vec<ArgumentLocation> {
        let param_regs = self.arg_registers();
        let shadow = self.shadow_space() as i32;
        param_classes
            .iter()
            .enumerate()
            .map(|(i, _)| {
                if i < param_regs.len() {
                    ArgumentLocation::Register(param_regs[i])
                } else {
                    let offset = 16 + shadow + (i - param_regs.len()) as i32 * 8;
                    ArgumentLocation::Stack(StackArgument {
                        offset,
                        size: 8,
                        align: 8,
                    })
                }
            })
            .collect()
    }

    /// Classify arguments into ABI locations (physical registers or stack slots) using full ABI types.
    fn classify_abi_args(
        &self,
        args: &[(VirtualRegister, AbiType)],
        outgoing_stack_base: PhysicalRegister,
    ) -> (Vec<MoveOperation>, i32) {
        let classes: Vec<(VirtualRegister, RegisterClass)> = args
            .iter()
            .map(|(v, t)| {
                let c = match t {
                    AbiType::Float { .. } => RegisterClass::Float,
                    _ => RegisterClass::Gpr,
                };
                (*v, c)
            })
            .collect();
        self.classify_args(&classes, outgoing_stack_base)
    }

    /// Classify arguments into ABI locations (physical registers or stack slots).
    fn classify_args(
        &self,
        args: &[(VirtualRegister, RegisterClass)],
        outgoing_stack_base: PhysicalRegister,
    ) -> (Vec<MoveOperation>, i32) {
        let shadow = self.shadow_space() as i32;
        let mut moves = Vec::new();
        let param_regs = self.arg_registers();
        let num_reg_args = args.len().min(param_regs.len());
        let stack_arg_bytes = ((args.len() - num_reg_args) * 8) as i32;
        let total_outgoing = ((shadow + stack_arg_bytes) + 15) & !15;

        for (i, &(vreg, _)) in args.iter().take(num_reg_args).enumerate() {
            let dst = MoveLocation::PhysicalRegister(param_regs[i]);
            let src = MoveLocation::VirtualRegister(vreg);
            moves.push(MoveOperation::new(dst, src, 8));
        }

        for (i, &(vreg, _)) in args.iter().skip(num_reg_args).enumerate() {
            let offset = shadow + (i as i32 * 8);
            let dst = MoveLocation::StackSlot {
                base: outgoing_stack_base,
                offset,
            };
            let src = MoveLocation::VirtualRegister(vreg);
            moves.push(MoveOperation::new(dst, src, 8));
        }

        (moves, total_outgoing)
    }
}

/// Windows x64 ABI (RCX/XMM0, RDX/XMM1, R8/XMM2, R9/XMM3, 32-byte shadow space).
pub struct WindowsX64CallingConvention;

// Physical register IDs for x86_64:
// 0: RAX, 1: RCX, 2: RDX, 3: RBX, 4: RSP, 5: RBP, 6: RSI, 7: RDI
// 8: R8,  9: R9,  10: R10, 11: R11, 12: R12, 13: R13, 14: R14, 15: R15
// 16..31: XMM0..XMM15
const WIN64_ARGS: [PhysicalRegister; 4] = [
    PhysicalRegister(1), // RCX
    PhysicalRegister(2), // RDX
    PhysicalRegister(8), // R8
    PhysicalRegister(9), // R9
];

const WIN64_FP_ARGS: [PhysicalRegister; 4] = [
    PhysicalRegister::xmm(0),
    PhysicalRegister::xmm(1),
    PhysicalRegister::xmm(2),
    PhysicalRegister::xmm(3),
];

const WIN64_RETS: [PhysicalRegister; 1] = [PhysicalRegister(0)]; // RAX
const WIN64_FP_RETS: [PhysicalRegister; 1] = [PhysicalRegister::xmm(0)]; // XMM0

const WIN64_CALLER_SAVED: [PhysicalRegister; 13] = [
    PhysicalRegister(0),  // RAX
    PhysicalRegister(1),  // RCX
    PhysicalRegister(2),  // RDX
    PhysicalRegister(8),  // R8
    PhysicalRegister(9),  // R9
    PhysicalRegister(10), // R10
    PhysicalRegister(11), // R11
    // XMM0..XMM5 are volatile (caller-saved)
    PhysicalRegister::xmm(0),
    PhysicalRegister::xmm(1),
    PhysicalRegister::xmm(2),
    PhysicalRegister::xmm(3),
    PhysicalRegister::xmm(4),
    PhysicalRegister::xmm(5),
];

const WIN64_CALLEE_SAVED: [PhysicalRegister; 18] = [
    PhysicalRegister(3),  // RBX
    PhysicalRegister(5),  // RBP
    PhysicalRegister(6),  // RSI
    PhysicalRegister(7),  // RDI
    PhysicalRegister(12), // R12
    PhysicalRegister(13), // R13
    PhysicalRegister(14), // R14
    PhysicalRegister(15), // R15
    // XMM6..XMM15 are non-volatile (callee-saved)
    PhysicalRegister::xmm(6),
    PhysicalRegister::xmm(7),
    PhysicalRegister::xmm(8),
    PhysicalRegister::xmm(9),
    PhysicalRegister::xmm(10),
    PhysicalRegister::xmm(11),
    PhysicalRegister::xmm(12),
    PhysicalRegister::xmm(13),
    PhysicalRegister::xmm(14),
    PhysicalRegister::xmm(15),
];

impl CallingConvention for WindowsX64CallingConvention {
    fn name(&self) -> &'static str {
        "Windows x64"
    }
    fn arg_registers(&self) -> &[PhysicalRegister] {
        &WIN64_ARGS
    }
    fn fp_arg_registers(&self) -> &[PhysicalRegister] {
        &WIN64_FP_ARGS
    }
    fn return_registers(&self) -> &[PhysicalRegister] {
        &WIN64_RETS
    }
    fn fp_return_registers(&self) -> &[PhysicalRegister] {
        &WIN64_FP_RETS
    }
    fn caller_saved_registers(&self) -> &[PhysicalRegister] {
        &WIN64_CALLER_SAVED
    }
    fn callee_saved_registers(&self) -> &[PhysicalRegister] {
        &WIN64_CALLEE_SAVED
    }
    fn stack_alignment(&self) -> u32 {
        16
    }
    fn shadow_space(&self) -> u32 {
        32
    }

    fn classify_return_location(&self, ret_type: &AbiType) -> ReturnLocation {
        crate::abi::win64::classify_win64_return(ret_type)
    }

    fn classify_incoming_abi_args(&self, param_types: &[AbiType]) -> Vec<ArgumentLocation> {
        crate::abi::win64::classify_win64_arguments(param_types)
    }

    fn classify_incoming_args(&self, param_classes: &[RegisterClass]) -> Vec<ArgumentLocation> {
        let types: Vec<AbiType> = param_classes
            .iter()
            .map(|&c| match c {
                RegisterClass::Float => AbiType::f64(),
                RegisterClass::Gpr => AbiType::i64(),
            })
            .collect();
        self.classify_incoming_abi_args(&types)
    }

    fn classify_abi_args(
        &self,
        args: &[(VirtualRegister, AbiType)],
        outgoing_stack_base: PhysicalRegister,
    ) -> (Vec<MoveOperation>, i32) {
        let mut moves = Vec::new();
        let types: Vec<AbiType> = args.iter().map(|(_, t)| t.clone()).collect();
        let locations = self.classify_incoming_abi_args(&types);
        let num_stack_args = args.len().saturating_sub(4);
        let stack_bytes = (num_stack_args * 8) as i32;
        let total_outgoing = (32 + stack_bytes + 15) & !15;

        for (i, &(vreg, _)) in args.iter().enumerate() {
            let loc = &locations[i];
            match loc {
                ArgumentLocation::Register(phys)
                | ArgumentLocation::FloatRegister(phys)
                | ArgumentLocation::VectorRegister(phys)
                | ArgumentLocation::IndirectByReference(phys) => {
                    moves.push(MoveOperation::new(
                        MoveLocation::PhysicalRegister(*phys),
                        MoveLocation::VirtualRegister(vreg),
                        8,
                    ));
                }
                ArgumentLocation::Stack(stack_arg) | ArgumentLocation::IndirectStack(stack_arg) => {
                    let offset = stack_arg.offset - 16;
                    moves.push(MoveOperation::new(
                        MoveLocation::StackSlot {
                            base: outgoing_stack_base,
                            offset,
                        },
                        MoveLocation::VirtualRegister(vreg),
                        8,
                    ));
                }
                ArgumentLocation::Pair(r1, _) => {
                    moves.push(MoveOperation::new(
                        MoveLocation::PhysicalRegister(*r1),
                        MoveLocation::VirtualRegister(vreg),
                        8,
                    ));
                }
            }
        }

        (moves, total_outgoing)
    }

    fn classify_args(
        &self,
        args: &[(VirtualRegister, RegisterClass)],
        outgoing_stack_base: PhysicalRegister,
    ) -> (Vec<MoveOperation>, i32) {
        let abi_args: Vec<(VirtualRegister, AbiType)> = args
            .iter()
            .map(|&(v, c)| {
                let ty = match c {
                    RegisterClass::Float => AbiType::f64(),
                    RegisterClass::Gpr => AbiType::i64(),
                };
                (v, ty)
            })
            .collect();
        self.classify_abi_args(&abi_args, outgoing_stack_base)
    }
}

/// System V AMD64 ABI (Linux, macOS, BSD - RDI, RSI, RDX, RCX, R8, R9; XMM0..XMM7).
pub struct SystemVX64CallingConvention;

const SYSV64_ARGS: [PhysicalRegister; 6] = [
    PhysicalRegister(7), // RDI
    PhysicalRegister(6), // RSI
    PhysicalRegister(2), // RDX
    PhysicalRegister(1), // RCX
    PhysicalRegister(8), // R8
    PhysicalRegister(9), // R9
];

const SYSV64_FP_ARGS: [PhysicalRegister; 8] = [
    PhysicalRegister::xmm(0),
    PhysicalRegister::xmm(1),
    PhysicalRegister::xmm(2),
    PhysicalRegister::xmm(3),
    PhysicalRegister::xmm(4),
    PhysicalRegister::xmm(5),
    PhysicalRegister::xmm(6),
    PhysicalRegister::xmm(7),
];

const SYSV64_RETS: [PhysicalRegister; 2] = [
    PhysicalRegister(0), // RAX
    PhysicalRegister(2), // RDX
];

const SYSV64_FP_RETS: [PhysicalRegister; 2] = [PhysicalRegister::xmm(0), PhysicalRegister::xmm(1)];

const SYSV64_CALLER_SAVED: [PhysicalRegister; 25] = [
    PhysicalRegister(0),  // RAX
    PhysicalRegister(1),  // RCX
    PhysicalRegister(2),  // RDX
    PhysicalRegister(6),  // RSI
    PhysicalRegister(7),  // RDI
    PhysicalRegister(8),  // R8
    PhysicalRegister(9),  // R9
    PhysicalRegister(10), // R10
    PhysicalRegister(11), // R11
    // XMM0..XMM15 are all caller-saved in SysV
    PhysicalRegister::xmm(0),
    PhysicalRegister::xmm(1),
    PhysicalRegister::xmm(2),
    PhysicalRegister::xmm(3),
    PhysicalRegister::xmm(4),
    PhysicalRegister::xmm(5),
    PhysicalRegister::xmm(6),
    PhysicalRegister::xmm(7),
    PhysicalRegister::xmm(8),
    PhysicalRegister::xmm(9),
    PhysicalRegister::xmm(10),
    PhysicalRegister::xmm(11),
    PhysicalRegister::xmm(12),
    PhysicalRegister::xmm(13),
    PhysicalRegister::xmm(14),
    PhysicalRegister::xmm(15),
];

const SYSV64_CALLEE_SAVED: [PhysicalRegister; 6] = [
    PhysicalRegister(3),  // RBX
    PhysicalRegister(5),  // RBP
    PhysicalRegister(12), // R12
    PhysicalRegister(13), // R13
    PhysicalRegister(14), // R14
    PhysicalRegister(15), // R15
];

impl CallingConvention for SystemVX64CallingConvention {
    fn name(&self) -> &'static str {
        "System V AMD64"
    }
    fn arg_registers(&self) -> &[PhysicalRegister] {
        &SYSV64_ARGS
    }
    fn fp_arg_registers(&self) -> &[PhysicalRegister] {
        &SYSV64_FP_ARGS
    }
    fn return_registers(&self) -> &[PhysicalRegister] {
        &SYSV64_RETS
    }
    fn fp_return_registers(&self) -> &[PhysicalRegister] {
        &SYSV64_FP_RETS
    }
    fn caller_saved_registers(&self) -> &[PhysicalRegister] {
        &SYSV64_CALLER_SAVED
    }
    fn callee_saved_registers(&self) -> &[PhysicalRegister] {
        &SYSV64_CALLEE_SAVED
    }
    fn stack_alignment(&self) -> u32 {
        16
    }

    fn classify_return_location(&self, ret_type: &AbiType) -> ReturnLocation {
        crate::abi::sysv64::classify_sysv_return(ret_type)
    }

    fn classify_incoming_abi_args(&self, param_types: &[AbiType]) -> Vec<ArgumentLocation> {
        crate::abi::sysv64::classify_sysv_arguments(param_types)
    }

    fn classify_incoming_args(&self, param_classes: &[RegisterClass]) -> Vec<ArgumentLocation> {
        let types: Vec<AbiType> = param_classes
            .iter()
            .map(|&c| match c {
                RegisterClass::Float => AbiType::f64(),
                RegisterClass::Gpr => AbiType::i64(),
            })
            .collect();
        self.classify_incoming_abi_args(&types)
    }

    fn classify_abi_args(
        &self,
        args: &[(VirtualRegister, AbiType)],
        outgoing_stack_base: PhysicalRegister,
    ) -> (Vec<MoveOperation>, i32) {
        let mut moves = Vec::new();
        let types: Vec<AbiType> = args.iter().map(|(_, t)| t.clone()).collect();
        let locations = self.classify_incoming_abi_args(&types);
        let mut max_stack_offset = 0i32;

        for (i, &(vreg, _)) in args.iter().enumerate() {
            let loc = &locations[i];
            match loc {
                ArgumentLocation::Register(phys)
                | ArgumentLocation::FloatRegister(phys)
                | ArgumentLocation::VectorRegister(phys)
                | ArgumentLocation::IndirectByReference(phys) => {
                    moves.push(MoveOperation::new(
                        MoveLocation::PhysicalRegister(*phys),
                        MoveLocation::VirtualRegister(vreg),
                        8,
                    ));
                }
                ArgumentLocation::Stack(stack_arg) | ArgumentLocation::IndirectStack(stack_arg) => {
                    let offset = stack_arg.offset - 16;
                    let sz = stack_arg.size.max(8);
                    moves.push(MoveOperation::new(
                        MoveLocation::StackSlot {
                            base: outgoing_stack_base,
                            offset,
                        },
                        MoveLocation::VirtualRegister(vreg),
                        sz as u8,
                    ));
                    max_stack_offset = max_stack_offset.max(offset + sz as i32);
                }
                ArgumentLocation::Pair(r1, _) => {
                    moves.push(MoveOperation::new(
                        MoveLocation::PhysicalRegister(*r1),
                        MoveLocation::VirtualRegister(vreg),
                        8,
                    ));
                }
            }
        }

        let total_outgoing = (max_stack_offset + 15) & !15;
        (moves, total_outgoing)
    }

    fn classify_args(
        &self,
        args: &[(VirtualRegister, RegisterClass)],
        outgoing_stack_base: PhysicalRegister,
    ) -> (Vec<MoveOperation>, i32) {
        let abi_args: Vec<(VirtualRegister, AbiType)> = args
            .iter()
            .map(|&(v, c)| {
                let ty = match c {
                    RegisterClass::Float => AbiType::f64(),
                    RegisterClass::Gpr => AbiType::i64(),
                };
                (v, ty)
            })
            .collect();
        self.classify_abi_args(&abi_args, outgoing_stack_base)
    }
}

/// AAPCS64 (ARM 64-bit ABI - X0-X7).
pub struct Aapcs64CallingConvention;

const AAPCS64_ARGS: [PhysicalRegister; 8] = [
    PhysicalRegister(0),
    PhysicalRegister(1),
    PhysicalRegister(2),
    PhysicalRegister(3),
    PhysicalRegister(4),
    PhysicalRegister(5),
    PhysicalRegister(6),
    PhysicalRegister(7),
];

const AAPCS64_RETS: [PhysicalRegister; 2] = [PhysicalRegister(0), PhysicalRegister(1)];

impl CallingConvention for Aapcs64CallingConvention {
    fn name(&self) -> &'static str {
        "AAPCS64"
    }
    fn arg_registers(&self) -> &[PhysicalRegister] {
        &AAPCS64_ARGS
    }
    fn return_registers(&self) -> &[PhysicalRegister] {
        &AAPCS64_RETS
    }
    fn caller_saved_registers(&self) -> &[PhysicalRegister] {
        &AAPCS64_ARGS
    }
    fn callee_saved_registers(&self) -> &[PhysicalRegister] {
        &[]
    }
    fn stack_alignment(&self) -> u32 {
        16
    }
}

/// AAPCS32 (ARM 32-bit ABI - R0-R3).
pub struct Aapcs32CallingConvention;

const AAPCS32_ARGS: [PhysicalRegister; 4] = [
    PhysicalRegister(0),
    PhysicalRegister(1),
    PhysicalRegister(2),
    PhysicalRegister(3),
];

impl CallingConvention for Aapcs32CallingConvention {
    fn name(&self) -> &'static str {
        "AAPCS32"
    }
    fn arg_registers(&self) -> &[PhysicalRegister] {
        &AAPCS32_ARGS
    }
    fn return_registers(&self) -> &[PhysicalRegister] {
        &AAPCS32_ARGS[0..2]
    }
    fn caller_saved_registers(&self) -> &[PhysicalRegister] {
        &AAPCS32_ARGS
    }
    fn callee_saved_registers(&self) -> &[PhysicalRegister] {
        &[]
    }
    fn stack_alignment(&self) -> u32 {
        8
    }
}

/// RISC-V ABI (A0-A7).
pub struct RiscVCallingConvention;

const RISCV_ARGS: [PhysicalRegister; 8] = [
    PhysicalRegister(10), // a0
    PhysicalRegister(11), // a1
    PhysicalRegister(12), // a2
    PhysicalRegister(13), // a3
    PhysicalRegister(14), // a4
    PhysicalRegister(15), // a5
    PhysicalRegister(16), // a6
    PhysicalRegister(17), // a7
];

impl CallingConvention for RiscVCallingConvention {
    fn name(&self) -> &'static str {
        "RISC-V ABI"
    }
    fn arg_registers(&self) -> &[PhysicalRegister] {
        &RISCV_ARGS
    }
    fn return_registers(&self) -> &[PhysicalRegister] {
        &RISCV_ARGS[0..2]
    }
    fn caller_saved_registers(&self) -> &[PhysicalRegister] {
        &RISCV_ARGS
    }
    fn callee_saved_registers(&self) -> &[PhysicalRegister] {
        &[]
    }
    fn stack_alignment(&self) -> u32 {
        16
    }
}

/// WASM internal calling convention.
pub struct WasmCallingConvention;

impl CallingConvention for WasmCallingConvention {
    fn name(&self) -> &'static str {
        "WASM ABI"
    }
    fn arg_registers(&self) -> &[PhysicalRegister] {
        &[]
    }
    fn return_registers(&self) -> &[PhysicalRegister] {
        &[]
    }
    fn caller_saved_registers(&self) -> &[PhysicalRegister] {
        &[]
    }
    fn callee_saved_registers(&self) -> &[PhysicalRegister] {
        &[]
    }
    fn stack_alignment(&self) -> u32 {
        8
    }
}
