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
            AbiType::Float { .. } => {
                let regs = self.fp_return_registers();
                let reg = if !regs.is_empty() {
                    regs[0]
                } else {
                    PhysicalRegister::xmm(0)
                };
                ReturnLocation::FloatRegister(reg)
            }
            AbiType::Vector { .. } => {
                let regs = self.fp_return_registers();
                let reg = if !regs.is_empty() {
                    regs[0]
                } else {
                    PhysicalRegister::xmm(0)
                };
                ReturnLocation::VectorRegister(reg)
            }
            _ => {
                let regs = self.return_registers();
                let reg = if !regs.is_empty() {
                    regs[0]
                } else {
                    PhysicalRegister(0)
                };
                ReturnLocation::Register(reg)
            }
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

/// AAPCS64 (ARM 64-bit ABI - X0-X7, V0-V7).
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

const AAPCS64_FP_ARGS: [PhysicalRegister; 8] = [
    PhysicalRegister(32), // V0
    PhysicalRegister(33), // V1
    PhysicalRegister(34), // V2
    PhysicalRegister(35), // V3
    PhysicalRegister(36), // V4
    PhysicalRegister(37), // V5
    PhysicalRegister(38), // V6
    PhysicalRegister(39), // V7
];

const AAPCS64_RETS: [PhysicalRegister; 2] = [PhysicalRegister(0), PhysicalRegister(1)];
const AAPCS64_FP_RETS: [PhysicalRegister; 2] = [PhysicalRegister(32), PhysicalRegister(33)];

const AAPCS64_CALLER_SAVED: [PhysicalRegister; 16] = [
    PhysicalRegister(0),
    PhysicalRegister(1),
    PhysicalRegister(2),
    PhysicalRegister(3),
    PhysicalRegister(4),
    PhysicalRegister(5),
    PhysicalRegister(6),
    PhysicalRegister(7),
    PhysicalRegister(8),
    PhysicalRegister(9),
    PhysicalRegister(10),
    PhysicalRegister(11),
    PhysicalRegister(12),
    PhysicalRegister(13),
    PhysicalRegister(14),
    PhysicalRegister(15),
];

const AAPCS64_CALLEE_SAVED: [PhysicalRegister; 10] = [
    PhysicalRegister(19),
    PhysicalRegister(20),
    PhysicalRegister(21),
    PhysicalRegister(22),
    PhysicalRegister(23),
    PhysicalRegister(24),
    PhysicalRegister(25),
    PhysicalRegister(26),
    PhysicalRegister(27),
    PhysicalRegister(28),
];

impl CallingConvention for Aapcs64CallingConvention {
    fn name(&self) -> &'static str {
        "AAPCS64"
    }
    fn arg_registers(&self) -> &[PhysicalRegister] {
        &AAPCS64_ARGS
    }
    fn fp_arg_registers(&self) -> &[PhysicalRegister] {
        &AAPCS64_FP_ARGS
    }
    fn return_registers(&self) -> &[PhysicalRegister] {
        &AAPCS64_RETS
    }
    fn fp_return_registers(&self) -> &[PhysicalRegister] {
        &AAPCS64_FP_RETS
    }
    fn caller_saved_registers(&self) -> &[PhysicalRegister] {
        &AAPCS64_CALLER_SAVED
    }
    fn callee_saved_registers(&self) -> &[PhysicalRegister] {
        &AAPCS64_CALLEE_SAVED
    }
    fn stack_alignment(&self) -> u32 {
        16
    }
}

/// System V i386 / cdecl (Linux IA-32): all integer args on the stack in
/// 4-byte slots, return in EAX (EDX:EAX for 64-bit), no red zone.
pub struct SystemVI386CallingConvention;

const I386_RETS: [PhysicalRegister; 2] = [
    PhysicalRegister(0), // EAX
    PhysicalRegister(2), // EDX
];

const I386_CALLER_SAVED: [PhysicalRegister; 3] = [
    PhysicalRegister(0), // EAX
    PhysicalRegister(1), // ECX
    PhysicalRegister(2), // EDX
];

const I386_CALLEE_SAVED: [PhysicalRegister; 4] = [
    PhysicalRegister(3), // EBX
    PhysicalRegister(5), // EBP
    PhysicalRegister(6), // ESI
    PhysicalRegister(7), // EDI
];

fn i386_slot_size(ty: &AbiType) -> usize {
    match ty {
        AbiType::Pointer => 4,
        AbiType::Integer { bits, .. } if *bits > 32 => 8,
        AbiType::Float { bits } if *bits > 32 => 8,
        AbiType::Struct { size, .. } => *size,
        _ => 4,
    }
}

impl CallingConvention for SystemVI386CallingConvention {
    fn name(&self) -> &'static str {
        "System V i386 (cdecl)"
    }
    fn arg_registers(&self) -> &[PhysicalRegister] {
        &[]
    }
    fn return_registers(&self) -> &[PhysicalRegister] {
        &I386_RETS
    }
    fn caller_saved_registers(&self) -> &[PhysicalRegister] {
        &I386_CALLER_SAVED
    }
    fn callee_saved_registers(&self) -> &[PhysicalRegister] {
        &I386_CALLEE_SAVED
    }
    fn stack_alignment(&self) -> u32 {
        16
    }

    fn classify_return_location(&self, ret_type: &AbiType) -> ReturnLocation {
        match ret_type {
            AbiType::Void => ReturnLocation::Void,
            AbiType::Float { bits: 32 } | AbiType::Float { bits: 64 } => {
                // Scalar FP returns use x87 ST(0); until the i686 encoder grows
                // an x87 path, treat them as integer EAX so callers fail loud
                // on real FP work rather than inventing XMM semantics.
                ReturnLocation::Register(PhysicalRegister(0))
            }
            AbiType::Integer { bits, .. } if *bits > 32 => {
                ReturnLocation::Pair(PhysicalRegister(0), PhysicalRegister(2))
            }
            AbiType::Struct { size, .. } if *size > 0 => {
                ReturnLocation::HiddenSret(PhysicalRegister(0))
            }
            _ => ReturnLocation::Register(PhysicalRegister(0)),
        }
    }

    fn classify_incoming_args(&self, param_classes: &[RegisterClass]) -> Vec<ArgumentLocation> {
        // After `push ebp; mov ebp, esp`, first arg is at [ebp+8].
        param_classes
            .iter()
            .enumerate()
            .map(|(i, _)| {
                ArgumentLocation::Stack(StackArgument {
                    offset: 8 + (i as i32 * 4),
                    size: 4,
                    align: 4,
                })
            })
            .collect()
    }

    fn classify_incoming_abi_args(&self, param_types: &[AbiType]) -> Vec<ArgumentLocation> {
        let mut offset = 8i32;
        param_types
            .iter()
            .map(|ty| {
                let size = i386_slot_size(ty);
                let loc = ArgumentLocation::Stack(StackArgument {
                    offset,
                    size,
                    align: 4,
                });
                offset += size as i32;
                loc
            })
            .collect()
    }

    fn classify_args(
        &self,
        args: &[(VirtualRegister, RegisterClass)],
        outgoing_stack_base: PhysicalRegister,
    ) -> (Vec<MoveOperation>, i32) {
        let mut moves = Vec::with_capacity(args.len());
        let mut stack_bytes = 0i32;
        for (vreg, class) in args {
            let size = if *class == RegisterClass::Float { 8 } else { 4 };
            moves.push(MoveOperation::new(
                MoveLocation::StackSlot {
                    base: outgoing_stack_base,
                    offset: stack_bytes,
                },
                MoveLocation::VirtualRegister(*vreg),
                size,
            ));
            stack_bytes += i32::from(size);
        }
        (moves, (stack_bytes + 15) & !15)
    }

    fn classify_abi_args(
        &self,
        args: &[(VirtualRegister, AbiType)],
        outgoing_stack_base: PhysicalRegister,
    ) -> (Vec<MoveOperation>, i32) {
        let mut moves = Vec::with_capacity(args.len());
        let mut stack_bytes = 0i32;
        for (vreg, ty) in args {
            let size = i386_slot_size(ty);
            moves.push(MoveOperation::new(
                MoveLocation::StackSlot {
                    base: outgoing_stack_base,
                    offset: stack_bytes,
                },
                MoveLocation::VirtualRegister(*vreg),
                size as u8,
            ));
            stack_bytes += size as i32;
        }
        (moves, (stack_bytes + 15) & !15)
    }
}

/// Intel 8086 16-bit Real-Mode cdecl Calling Convention.
///
/// - 16-bit stack slots; caller cleans up arguments after `call`.
/// - After `push bp; mov bp, sp`, first stack argument is at `[bp+4]`
///   (saved BP = 2 bytes, near return IP = 2 bytes).
/// - Return value in `AX` (16-bit / near pointer / 8-bit) or `DX:AX` (32-bit).
pub struct Dos16CallingConvention;

const DOS16_RETS: [PhysicalRegister; 2] = [PhysicalRegister(0), PhysicalRegister(2)]; // AX, DX
const DOS16_CALLER_SAVED: [PhysicalRegister; 3] = [
    PhysicalRegister(0), // AX
    PhysicalRegister(1), // CX
    PhysicalRegister(2), // DX
];
const DOS16_CALLEE_SAVED: [PhysicalRegister; 3] = [
    PhysicalRegister(3), // BX
    PhysicalRegister(6), // SI
    PhysicalRegister(7), // DI
];

fn dos16_slot_size(ty: &AbiType) -> usize {
    match ty {
        AbiType::Integer { bits, .. } if *bits > 16 => 4,
        AbiType::Struct { size, .. } => ((*size + 1) & !1).max(2),
        _ => 2,
    }
}

impl CallingConvention for Dos16CallingConvention {
    fn name(&self) -> &'static str {
        "Intel 8086 Real-Mode cdecl"
    }
    fn arg_registers(&self) -> &[PhysicalRegister] {
        &[]
    }
    fn return_registers(&self) -> &[PhysicalRegister] {
        &DOS16_RETS
    }
    fn caller_saved_registers(&self) -> &[PhysicalRegister] {
        &DOS16_CALLER_SAVED
    }
    fn callee_saved_registers(&self) -> &[PhysicalRegister] {
        &DOS16_CALLEE_SAVED
    }
    fn stack_alignment(&self) -> u32 {
        2
    }

    fn classify_return_location(&self, ret_type: &AbiType) -> ReturnLocation {
        match ret_type {
            AbiType::Void => ReturnLocation::Void,
            AbiType::Integer { bits, .. } if *bits > 16 => {
                ReturnLocation::Pair(PhysicalRegister(0), PhysicalRegister(2))
            }
            AbiType::Struct { size, .. } if *size > 2 => {
                ReturnLocation::HiddenSret(PhysicalRegister(0))
            }
            _ => ReturnLocation::Register(PhysicalRegister(0)),
        }
    }

    fn classify_incoming_args(&self, param_classes: &[RegisterClass]) -> Vec<ArgumentLocation> {
        param_classes
            .iter()
            .enumerate()
            .map(|(i, _)| {
                ArgumentLocation::Stack(StackArgument {
                    offset: 4 + (i as i32 * 2),
                    size: 2,
                    align: 2,
                })
            })
            .collect()
    }

    fn classify_incoming_abi_args(&self, param_types: &[AbiType]) -> Vec<ArgumentLocation> {
        let mut offset = 4i32;
        param_types
            .iter()
            .map(|ty| {
                let size = dos16_slot_size(ty);
                let loc = ArgumentLocation::Stack(StackArgument {
                    offset,
                    size,
                    align: 2,
                });
                offset += size as i32;
                loc
            })
            .collect()
    }

    fn classify_args(
        &self,
        args: &[(VirtualRegister, RegisterClass)],
        outgoing_stack_base: PhysicalRegister,
    ) -> (Vec<MoveOperation>, i32) {
        let mut moves = Vec::with_capacity(args.len());
        let mut stack_bytes = 0i32;
        for (vreg, _) in args {
            moves.push(MoveOperation::new(
                MoveLocation::StackSlot {
                    base: outgoing_stack_base,
                    offset: stack_bytes,
                },
                MoveLocation::VirtualRegister(*vreg),
                2,
            ));
            stack_bytes += 2;
        }
        (moves, stack_bytes)
    }

    fn classify_abi_args(
        &self,
        args: &[(VirtualRegister, AbiType)],
        outgoing_stack_base: PhysicalRegister,
    ) -> (Vec<MoveOperation>, i32) {
        let mut moves = Vec::with_capacity(args.len());
        let mut stack_bytes = 0i32;
        for (vreg, ty) in args {
            let size = dos16_slot_size(ty);
            moves.push(MoveOperation::new(
                MoveLocation::StackSlot {
                    base: outgoing_stack_base,
                    offset: stack_bytes,
                },
                MoveLocation::VirtualRegister(*vreg),
                size as u8,
            ));
            stack_bytes += size as i32;
        }
        (moves, stack_bytes)
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

/// RISC-V ABI (A0-A7, FA0-FA7).
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

const RISCV_FP_ARGS: [PhysicalRegister; 8] = [
    PhysicalRegister(42), // fa0
    PhysicalRegister(43), // fa1
    PhysicalRegister(44), // fa2
    PhysicalRegister(45), // fa3
    PhysicalRegister(46), // fa4
    PhysicalRegister(47), // fa5
    PhysicalRegister(48), // fa6
    PhysicalRegister(49), // fa7
];

const RISCV_FP_RETS: [PhysicalRegister; 2] = [
    PhysicalRegister(42), // fa0
    PhysicalRegister(43), // fa1
];

impl CallingConvention for RiscVCallingConvention {
    fn name(&self) -> &'static str {
        "RISC-V ABI"
    }
    fn arg_registers(&self) -> &[PhysicalRegister] {
        &RISCV_ARGS
    }
    fn fp_arg_registers(&self) -> &[PhysicalRegister] {
        &RISCV_FP_ARGS
    }
    fn return_registers(&self) -> &[PhysicalRegister] {
        &RISCV_ARGS[0..2]
    }
    fn fp_return_registers(&self) -> &[PhysicalRegister] {
        &RISCV_FP_RETS
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
    fn classify_return_location(&self, ret_type: &AbiType) -> ReturnLocation {
        match ret_type {
            AbiType::Void => ReturnLocation::Void,
            AbiType::Float { .. } => ReturnLocation::FloatRegister(PhysicalRegister(42)),
            _ => ReturnLocation::Register(PhysicalRegister(10)),
        }
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

/// ATmega328P / AVR bare-metal calling convention (`avr-gcc` compatible).
///
/// - 16-bit register pairs (identified by their even low register):
///   `r25:r24` (24), `r23:r22` (22), `r21:r20` (20), `r19:r18` (18),
///   `r17:r16` (16), `r15:r14` (14), `r13:r12` (12), `r11:r10` (10), `r9:r8` (8).
/// - 32-bit scalars consume two adjacent pairs: e.g. `r25:r24:r23:r22` (`Pair(22, 24)`).
/// - After `push r28; push r29; in r28, SPL; in r29, SPH`, stack arguments begin at `Y + 5`
///   (2 bytes saved Y + 2 bytes 16-bit return PC + 1 post-decrement bias).
pub struct AvrCallingConvention;

const AVR_ARG_REGS: [PhysicalRegister; 9] = [
    PhysicalRegister(24),
    PhysicalRegister(22),
    PhysicalRegister(20),
    PhysicalRegister(18),
    PhysicalRegister(16),
    PhysicalRegister(14),
    PhysicalRegister(12),
    PhysicalRegister(10),
    PhysicalRegister(8),
];
const AVR_RETS: [PhysicalRegister; 2] = [PhysicalRegister(24), PhysicalRegister(22)];
const AVR_CALLER_SAVED: [PhysicalRegister; 6] = [
    PhysicalRegister(18),
    PhysicalRegister(20),
    PhysicalRegister(22),
    PhysicalRegister(24),
    PhysicalRegister(26),
    PhysicalRegister(30),
];
const AVR_CALLEE_SAVED: [PhysicalRegister; 8] = [
    PhysicalRegister(2),
    PhysicalRegister(4),
    PhysicalRegister(6),
    PhysicalRegister(8),
    PhysicalRegister(10),
    PhysicalRegister(12),
    PhysicalRegister(14),
    PhysicalRegister(16),
];

fn avr_slot_size(ty: &AbiType) -> usize {
    match ty {
        AbiType::Integer { bits, .. } if *bits > 16 => 4,
        AbiType::Struct { size, .. } => ((*size + 1) & !1).max(2),
        _ => 2,
    }
}

impl CallingConvention for AvrCallingConvention {
    fn name(&self) -> &'static str {
        "AVR bare-metal (avr-gcc ATmega328P)"
    }
    fn arg_registers(&self) -> &[PhysicalRegister] {
        &AVR_ARG_REGS
    }
    fn return_registers(&self) -> &[PhysicalRegister] {
        &AVR_RETS
    }
    fn caller_saved_registers(&self) -> &[PhysicalRegister] {
        &AVR_CALLER_SAVED
    }
    fn callee_saved_registers(&self) -> &[PhysicalRegister] {
        &AVR_CALLEE_SAVED
    }
    fn stack_alignment(&self) -> u32 {
        1
    }

    fn classify_return_location(&self, ret_type: &AbiType) -> ReturnLocation {
        match ret_type {
            AbiType::Void => ReturnLocation::Void,
            AbiType::Integer { bits, .. } if *bits > 16 => {
                ReturnLocation::Pair(PhysicalRegister(22), PhysicalRegister(24))
            }
            AbiType::Struct { size, .. } if *size > 2 => {
                ReturnLocation::HiddenSret(PhysicalRegister(24))
            }
            _ => ReturnLocation::Register(PhysicalRegister(24)),
        }
    }

    fn classify_incoming_args(&self, param_classes: &[RegisterClass]) -> Vec<ArgumentLocation> {
        let mut reg_idx = 0usize;
        let mut stack_offset = 5i32;
        param_classes
            .iter()
            .map(|_| {
                if reg_idx < AVR_ARG_REGS.len() {
                    let r = AVR_ARG_REGS[reg_idx];
                    reg_idx += 1;
                    ArgumentLocation::Register(r)
                } else {
                    let loc = ArgumentLocation::Stack(StackArgument {
                        offset: stack_offset,
                        size: 2,
                        align: 1,
                    });
                    stack_offset += 2;
                    loc
                }
            })
            .collect()
    }

    fn classify_incoming_abi_args(&self, param_types: &[AbiType]) -> Vec<ArgumentLocation> {
        let mut reg_idx = 0usize;
        let mut stack_offset = 5i32;
        param_types
            .iter()
            .map(|ty| {
                let bytes = avr_slot_size(ty);
                if bytes <= 2 && reg_idx < AVR_ARG_REGS.len() {
                    let r = AVR_ARG_REGS[reg_idx];
                    reg_idx += 1;
                    ArgumentLocation::Register(r)
                } else if bytes == 4 && reg_idx + 1 < AVR_ARG_REGS.len() {
                    let r_hi = AVR_ARG_REGS[reg_idx];
                    let r_lo = AVR_ARG_REGS[reg_idx + 1];
                    reg_idx += 2;
                    ArgumentLocation::Pair(r_lo, r_hi)
                } else {
                    let loc = ArgumentLocation::Stack(StackArgument {
                        offset: stack_offset,
                        size: bytes,
                        align: 1,
                    });
                    stack_offset += bytes as i32;
                    loc
                }
            })
            .collect()
    }

    fn classify_args(
        &self,
        args: &[(VirtualRegister, RegisterClass)],
        outgoing_stack_base: PhysicalRegister,
    ) -> (Vec<MoveOperation>, i32) {
        let mut moves = Vec::with_capacity(args.len());
        let mut reg_idx = 0usize;
        let mut stack_bytes = 0i32;
        for (vreg, _) in args {
            if reg_idx < AVR_ARG_REGS.len() {
                moves.push(MoveOperation::new(
                    MoveLocation::PhysicalRegister(AVR_ARG_REGS[reg_idx]),
                    MoveLocation::VirtualRegister(*vreg),
                    2,
                ));
                reg_idx += 1;
            } else {
                moves.push(MoveOperation::new(
                    MoveLocation::StackSlot {
                        base: outgoing_stack_base,
                        offset: stack_bytes,
                    },
                    MoveLocation::VirtualRegister(*vreg),
                    2,
                ));
                stack_bytes += 2;
            }
        }
        (moves, stack_bytes)
    }

    fn classify_abi_args(
        &self,
        args: &[(VirtualRegister, AbiType)],
        outgoing_stack_base: PhysicalRegister,
    ) -> (Vec<MoveOperation>, i32) {
        let mut moves = Vec::with_capacity(args.len());
        let mut reg_idx = 0usize;
        let mut stack_bytes = 0i32;
        for (vreg, ty) in args {
            let bytes = avr_slot_size(ty);
            if bytes <= 2 && reg_idx < AVR_ARG_REGS.len() {
                moves.push(MoveOperation::new(
                    MoveLocation::PhysicalRegister(AVR_ARG_REGS[reg_idx]),
                    MoveLocation::VirtualRegister(*vreg),
                    bytes as u8,
                ));
                reg_idx += 1;
            } else if bytes == 4 && reg_idx + 1 < AVR_ARG_REGS.len() {
                let r_lo = AVR_ARG_REGS[reg_idx + 1];
                moves.push(MoveOperation::new(
                    MoveLocation::PhysicalRegister(r_lo),
                    MoveLocation::VirtualRegister(*vreg),
                    4,
                ));
                reg_idx += 2;
            } else {
                moves.push(MoveOperation::new(
                    MoveLocation::StackSlot {
                        base: outgoing_stack_base,
                        offset: stack_bytes,
                    },
                    MoveLocation::VirtualRegister(*vreg),
                    bytes as u8,
                ));
                stack_bytes += bytes as i32;
            }
        }
        (moves, stack_bytes)
    }
}

/// Motorola DSP56000 24-bit fixed-point DSP calling convention (`dsp56k-reg-stack`).
///
/// - Registers `A1(0), B1(1), X0(2), X1(3), Y0(4), Y1(5)` carry up to 6 scalar arguments
///   (1 word = 24 bits each).
/// - 32-bit integers (`i32`/`u32`) are legalized into 2-word `(lo16, hi16)` pairs across
///   adjacent argument registers or consecutive `X:` stack words.
/// - Return values: `A1(0)` for 1-word scalars, `(A1, B1)` for 2-word values.
/// - Software stack (`R7=15`) grows upward in `X:` memory; frame pointer is `R6=14`.
/// - Hardware `SSH:SSL` holds return `PC:SR` (15 levels deep).
pub struct Dsp56000CallingConvention;

const DSP56K_ARG_REGS: [PhysicalRegister; 6] = [
    PhysicalRegister(0), // A1
    PhysicalRegister(1), // B1
    PhysicalRegister(2), // X0
    PhysicalRegister(3), // X1
    PhysicalRegister(4), // Y0
    PhysicalRegister(5), // Y1
];
const DSP56K_RETS: [PhysicalRegister; 2] = [PhysicalRegister(0), PhysicalRegister(1)];
const DSP56K_CALLER_SAVED: [PhysicalRegister; 6] = [
    PhysicalRegister(0),
    PhysicalRegister(1),
    PhysicalRegister(2),
    PhysicalRegister(3),
    PhysicalRegister(4),
    PhysicalRegister(5),
];
const DSP56K_CALLEE_SAVED: [PhysicalRegister; 2] = [
    PhysicalRegister(14), // R6 (frame pointer)
    PhysicalRegister(15), // R7 (software stack pointer)
];

fn dsp56k_slot_words(ty: &AbiType) -> usize {
    match ty {
        AbiType::Integer { bits, .. } if *bits > 24 => ((*bits as usize).div_ceil(16)).max(2),
        AbiType::Struct { size, .. } => (*size).max(1),
        _ => 1,
    }
}

impl CallingConvention for Dsp56000CallingConvention {
    fn name(&self) -> &'static str {
        "Motorola DSP56000 24-bit DSP (dsp56k-reg-stack)"
    }
    fn arg_registers(&self) -> &[PhysicalRegister] {
        &DSP56K_ARG_REGS
    }
    fn return_registers(&self) -> &[PhysicalRegister] {
        &DSP56K_RETS
    }
    fn caller_saved_registers(&self) -> &[PhysicalRegister] {
        &DSP56K_CALLER_SAVED
    }
    fn callee_saved_registers(&self) -> &[PhysicalRegister] {
        &DSP56K_CALLEE_SAVED
    }
    fn stack_alignment(&self) -> u32 {
        1
    }

    fn classify_return_location(&self, ret_type: &AbiType) -> ReturnLocation {
        match ret_type {
            AbiType::Void => ReturnLocation::Void,
            AbiType::Integer { bits, .. } if *bits > 24 => {
                ReturnLocation::Pair(PhysicalRegister(0), PhysicalRegister(1))
            }
            AbiType::Struct { size, .. } if *size > 1 => {
                ReturnLocation::HiddenSret(PhysicalRegister(0))
            }
            _ => ReturnLocation::Register(PhysicalRegister(0)),
        }
    }

    fn classify_incoming_args(&self, param_classes: &[RegisterClass]) -> Vec<ArgumentLocation> {
        let mut reg_idx = 0usize;
        let mut cum_words = 0i32;
        param_classes
            .iter()
            .map(|_| {
                if reg_idx < DSP56K_ARG_REGS.len() {
                    let r = DSP56K_ARG_REGS[reg_idx];
                    reg_idx += 1;
                    ArgumentLocation::Register(r)
                } else {
                    let offset = -(2 + cum_words);
                    cum_words += 1;
                    ArgumentLocation::Stack(StackArgument {
                        offset,
                        size: 1,
                        align: 1,
                    })
                }
            })
            .collect()
    }

    fn classify_incoming_abi_args(&self, param_types: &[AbiType]) -> Vec<ArgumentLocation> {
        let mut reg_idx = 0usize;
        let mut cum_words = 0i32;
        param_types
            .iter()
            .map(|ty| {
                let words = dsp56k_slot_words(ty);
                if words == 1 && reg_idx < DSP56K_ARG_REGS.len() {
                    let r = DSP56K_ARG_REGS[reg_idx];
                    reg_idx += 1;
                    ArgumentLocation::Register(r)
                } else if words == 2 && reg_idx + 1 < DSP56K_ARG_REGS.len() {
                    let r_lo = DSP56K_ARG_REGS[reg_idx];
                    let r_hi = DSP56K_ARG_REGS[reg_idx + 1];
                    reg_idx += 2;
                    ArgumentLocation::Pair(r_lo, r_hi)
                } else {
                    let offset = -(2 + cum_words + (words as i32) - 1);
                    cum_words += words as i32;
                    ArgumentLocation::Stack(StackArgument {
                        offset,
                        size: words,
                        align: 1,
                    })
                }
            })
            .collect()
    }

    fn classify_args(
        &self,
        args: &[(VirtualRegister, RegisterClass)],
        outgoing_stack_base: PhysicalRegister,
    ) -> (Vec<MoveOperation>, i32) {
        let mut moves = Vec::with_capacity(args.len());
        let reg_args = args.len().min(DSP56K_ARG_REGS.len());
        let total_stack_words = (args.len() - reg_args) as i32;
        for (i, (vreg, _)) in args.iter().enumerate() {
            if i < reg_args {
                moves.push(MoveOperation::new(
                    MoveLocation::PhysicalRegister(DSP56K_ARG_REGS[i]),
                    MoveLocation::VirtualRegister(*vreg),
                    1,
                ));
            } else {
                let cum = (i - reg_args) as i32;
                let offset = -(cum + 1);
                moves.push(MoveOperation::new(
                    MoveLocation::StackSlot {
                        base: outgoing_stack_base,
                        offset,
                    },
                    MoveLocation::VirtualRegister(*vreg),
                    1,
                ));
            }
        }
        (moves, total_stack_words)
    }

    fn classify_abi_args(
        &self,
        args: &[(VirtualRegister, AbiType)],
        outgoing_stack_base: PhysicalRegister,
    ) -> (Vec<MoveOperation>, i32) {
        #[derive(Clone, Copy)]
        enum ArgSlot {
            Reg(PhysicalRegister, usize),
            Stack(usize, usize),
        }

        let mut reg_idx = 0usize;
        let mut cum_words = 0usize;
        let mut slots = Vec::with_capacity(args.len());
        for (_, ty) in args {
            let words = dsp56k_slot_words(ty);
            if words == 1 && reg_idx < DSP56K_ARG_REGS.len() {
                slots.push(ArgSlot::Reg(DSP56K_ARG_REGS[reg_idx], words));
                reg_idx += 1;
            } else if words == 2 && reg_idx + 1 < DSP56K_ARG_REGS.len() {
                slots.push(ArgSlot::Reg(DSP56K_ARG_REGS[reg_idx], 2));
                reg_idx += 2;
            } else {
                slots.push(ArgSlot::Stack(cum_words, words));
                cum_words += words;
            }
        }

        let total_stack_words = cum_words as i32;
        let mut moves = Vec::with_capacity(args.len());
        for ((vreg, _), slot) in args.iter().zip(slots.into_iter()) {
            match slot {
                ArgSlot::Reg(r, w) => {
                    moves.push(MoveOperation::new(
                        MoveLocation::PhysicalRegister(r),
                        MoveLocation::VirtualRegister(*vreg),
                        w as u8,
                    ));
                }
                ArgSlot::Stack(cum, w) => {
                    let offset = -((cum + w) as i32);
                    moves.push(MoveOperation::new(
                        MoveLocation::StackSlot {
                            base: outgoing_stack_base,
                            offset,
                        },
                        MoveLocation::VirtualRegister(*vreg),
                        w as u8,
                    ));
                }
            }
        }
        (moves, total_stack_words)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn i686_arguments_use_stack_slots_and_pointer_width() {
        let conv = SystemVI386CallingConvention;
        let args = [
            (VirtualRegister(0), AbiType::i32()),
            (VirtualRegister(1), AbiType::ptr()),
        ];
        let (moves, total) = conv.classify_abi_args(&args, PhysicalRegister(4));

        assert_eq!(total, 16);
        assert_eq!(moves.len(), 2);
        assert_eq!(moves[0].size, 4);
        assert_eq!(moves[1].size, 4);
        assert!(matches!(
            moves[0].dst,
            MoveLocation::StackSlot {
                base: PhysicalRegister(4),
                offset: 0
            }
        ));
        assert!(matches!(
            moves[1].dst,
            MoveLocation::StackSlot {
                base: PhysicalRegister(4),
                offset: 4
            }
        ));

        let incoming = conv.classify_incoming_abi_args(&[AbiType::i32(), AbiType::ptr()]);
        assert_eq!(incoming[0].stack_offset(), Some(8));
        assert_eq!(incoming[1].stack_offset(), Some(12));
    }

    #[test]
    fn i686_keeps_real_i64_arguments_eight_bytes_wide() {
        let conv = SystemVI386CallingConvention;
        let args = [
            (VirtualRegister(0), AbiType::i64()),
            (VirtualRegister(1), AbiType::i32()),
        ];
        let (moves, total) = conv.classify_abi_args(&args, PhysicalRegister(4));

        assert_eq!(moves[0].size, 8);
        assert_eq!(moves[1].size, 4);
        assert!(matches!(
            moves[1].dst,
            MoveLocation::StackSlot { offset: 8, .. }
        ));
        assert_eq!(total, 16);
        assert!(matches!(
            conv.classify_return_location(&AbiType::i64()),
            ReturnLocation::Pair(PhysicalRegister(0), PhysicalRegister(2))
        ));
    }
}
