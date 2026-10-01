//! Calling convention abstractions for all supported targets.

use crate::machine_ir::PhysicalRegister;

pub trait CallingConvention: Send + Sync {
    fn name(&self) -> &'static str;
    fn arg_registers(&self) -> &[PhysicalRegister];
    fn return_registers(&self) -> &[PhysicalRegister];
    fn caller_saved_registers(&self) -> &[PhysicalRegister];
    fn callee_saved_registers(&self) -> &[PhysicalRegister];
    fn stack_alignment(&self) -> u32;
    fn shadow_space(&self) -> u32 {
        0
    }
}

/// Windows x64 ABI (RCX, RDX, R8, R9, 32-byte shadow space).
pub struct WindowsX64CallingConvention;

// Physical register IDs for x86_64:
// 0: RAX, 1: RCX, 2: RDX, 3: RBX, 4: RSP, 5: RBP, 6: RSI, 7: RDI
// 8: R8,  9: R9,  10: R10, 11: R11, 12: R12, 13: R13, 14: R14, 15: R15
const WIN64_ARGS: [PhysicalRegister; 4] = [
    PhysicalRegister(1), // RCX
    PhysicalRegister(2), // RDX
    PhysicalRegister(8), // R8
    PhysicalRegister(9), // R9
];

const WIN64_RETS: [PhysicalRegister; 1] = [PhysicalRegister(0)]; // RAX

const WIN64_CALLER_SAVED: [PhysicalRegister; 7] = [
    PhysicalRegister(0),  // RAX
    PhysicalRegister(1),  // RCX
    PhysicalRegister(2),  // RDX
    PhysicalRegister(8),  // R8
    PhysicalRegister(9),  // R9
    PhysicalRegister(10), // R10
    PhysicalRegister(11), // R11
];

const WIN64_CALLEE_SAVED: [PhysicalRegister; 8] = [
    PhysicalRegister(3),  // RBX
    PhysicalRegister(5),  // RBP
    PhysicalRegister(6),  // RSI
    PhysicalRegister(7),  // RDI
    PhysicalRegister(12), // R12
    PhysicalRegister(13), // R13
    PhysicalRegister(14), // R14
    PhysicalRegister(15), // R15
];

impl CallingConvention for WindowsX64CallingConvention {
    fn name(&self) -> &'static str {
        "Windows x64"
    }
    fn arg_registers(&self) -> &[PhysicalRegister] {
        &WIN64_ARGS
    }
    fn return_registers(&self) -> &[PhysicalRegister] {
        &WIN64_RETS
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
}

/// System V AMD64 ABI (Linux, macOS, BSD - RDI, RSI, RDX, RCX, R8, R9).
pub struct SystemVX64CallingConvention;

const SYSV64_ARGS: [PhysicalRegister; 6] = [
    PhysicalRegister(7), // RDI
    PhysicalRegister(6), // RSI
    PhysicalRegister(2), // RDX
    PhysicalRegister(1), // RCX
    PhysicalRegister(8), // R8
    PhysicalRegister(9), // R9
];

const SYSV64_RETS: [PhysicalRegister; 2] = [
    PhysicalRegister(0), // RAX
    PhysicalRegister(2), // RDX
];

const SYSV64_CALLER_SAVED: [PhysicalRegister; 9] = [
    PhysicalRegister(0),  // RAX
    PhysicalRegister(1),  // RCX
    PhysicalRegister(2),  // RDX
    PhysicalRegister(6),  // RSI
    PhysicalRegister(7),  // RDI
    PhysicalRegister(8),  // R8
    PhysicalRegister(9),  // R9
    PhysicalRegister(10), // R10
    PhysicalRegister(11), // R11
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
    fn return_registers(&self) -> &[PhysicalRegister] {
        &SYSV64_RETS
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
