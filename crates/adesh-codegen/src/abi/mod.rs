//! Production-Grade Centralized ABI Infrastructure.
//!
//! Provides ABI specification, register classification, argument and return location computation,
//! struct passing rules, aggregate return rules, stack frame layout calculation, variadic rules,
//! and calling convention definitions for x86-64 (Windows & SysV), AArch64 (AAPCS64), and RISC-V (RV64).

use crate::machine_ir::PhysicalRegister;
use crate::target_spec::{TargetAbi, TargetSpec};

/// Primitive or composite data type representation for ABI classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbiType {
    /// Integer of given bit width (e.g. 8, 16, 32, 64, 128) and signedness.
    Integer { bits: u16, is_signed: bool },
    /// Floating-point number (32-bit f32 or 64-bit f64).
    Float { bits: u16 },
    /// SIMD Vector type (e.g. 128-bit <4 x f32> or 256-bit <8 x i32>).
    Vector { total_bytes: u16, lane_size: u8 },
    /// Pointer or reference type (64-bit).
    Pointer,
    /// C-compatible or aggregate struct with field types and total size/alignment.
    Struct {
        fields: Vec<AbiType>,
        size: usize,
        align: usize,
    },
    /// Void / unit type.
    Void,
}

impl AbiType {
    pub fn i32() -> Self {
        AbiType::Integer {
            bits: 32,
            is_signed: true,
        }
    }
    pub fn i64() -> Self {
        AbiType::Integer {
            bits: 64,
            is_signed: true,
        }
    }
    pub fn u64() -> Self {
        AbiType::Integer {
            bits: 64,
            is_signed: false,
        }
    }
    pub fn f32() -> Self {
        AbiType::Float { bits: 32 }
    }
    pub fn f64() -> Self {
        AbiType::Float { bits: 64 }
    }
    pub fn ptr() -> Self {
        AbiType::Pointer
    }
    pub fn v128_f32() -> Self {
        AbiType::Vector {
            total_bytes: 16,
            lane_size: 4,
        }
    }

    pub fn size_in_bytes(&self) -> usize {
        match self {
            AbiType::Integer { bits, .. } => (*bits as usize).div_ceil(8),
            AbiType::Float { bits } => (*bits as usize) / 8,
            AbiType::Vector { total_bytes, .. } => *total_bytes as usize,
            AbiType::Pointer => 8,
            AbiType::Struct { size, .. } => *size,
            AbiType::Void => 0,
        }
    }

    pub fn alignment(&self) -> usize {
        match self {
            AbiType::Integer { bits, .. } => (*bits as usize).div_ceil(8).max(1),
            AbiType::Float { bits } => ((*bits as usize) / 8).max(4),
            AbiType::Vector { total_bytes, .. } => (*total_bytes as usize).min(16),
            AbiType::Pointer => 8,
            AbiType::Struct { align, .. } => *align,
            AbiType::Void => 1,
        }
    }
}

/// Description of stack argument slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackArgument {
    pub offset: i32,
    pub size: usize,
    pub align: usize,
}

/// Precise physical location for passing or receiving an argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgumentLocation {
    /// Passed in a general-purpose register.
    Register(PhysicalRegister),
    /// Passed in a floating-point register.
    FloatRegister(PhysicalRegister),
    /// Passed in a SIMD vector register.
    VectorRegister(PhysicalRegister),
    /// Passed split across two registers (e.g. 128-bit integer or 16-byte struct).
    Pair(PhysicalRegister, PhysicalRegister),
    /// Passed in a stack slot.
    Stack(StackArgument),
    /// Passed by reference (invisible pointer in GPR or stack).
    IndirectByReference(PhysicalRegister),
}

/// Precise physical location for returning a function value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnLocation {
    /// Returned in a general-purpose register (e.g. RAX / X0 / a0).
    Register(PhysicalRegister),
    /// Returned in a floating-point register (e.g. XMM0 / D0 / fa0).
    FloatRegister(PhysicalRegister),
    /// Returned in a vector register (e.g. XMM0 / V0 / v0).
    VectorRegister(PhysicalRegister),
    /// Returned in two registers (e.g. RAX:RDX or X0:X1 or a0:a1).
    Pair(PhysicalRegister, PhysicalRegister),
    /// Large aggregate returned via caller-allocated buffer pointed to by hidden sret register.
    HiddenSret(PhysicalRegister),
    /// Void return (no value).
    Void,
}

/// Stack alignment in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackAlignment(pub usize);

/// Shadow space (spill area for first 4 arguments on Windows x64).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShadowSpace(pub usize);

/// Red zone (stack area below RSP safe from signal interruption).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RedZone(pub usize);

/// Set of callee-saved (non-volatile) registers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalleeSavedSet {
    pub gpr: Vec<PhysicalRegister>,
    pub fpr: Vec<PhysicalRegister>,
}

/// Set of caller-saved (volatile) registers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallerSavedSet {
    pub gpr: Vec<PhysicalRegister>,
    pub fpr: Vec<PhysicalRegister>,
}

/// Calculated complete layout of a function's stack frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackFrameLayout {
    pub total_frame_size: usize,
    pub outgoing_args_size: usize,
    pub spills_size: usize,
    pub locals_size: usize,
    pub callee_saved_size: usize,
    pub shadow_space_size: usize,
    pub red_zone_size: usize,
    pub alignment: usize,
}

impl StackFrameLayout {
    pub fn compute(
        abi_align: usize,
        shadow_bytes: usize,
        red_zone_bytes: usize,
        locals_bytes: usize,
        spills_bytes: usize,
        outgoing_bytes: usize,
        callee_saved_regs: usize,
    ) -> Self {
        let callee_saved_size = callee_saved_regs * 8;
        let shadow_space_size = shadow_bytes;
        let outgoing_args_size = outgoing_bytes.max(shadow_space_size);

        let unaligned_total = callee_saved_size + locals_bytes + spills_bytes + outgoing_args_size;
        let aligned_total = if abi_align > 0 {
            (unaligned_total + (abi_align - 1)) & !(abi_align - 1)
        } else {
            unaligned_total
        };

        Self {
            total_frame_size: aligned_total,
            outgoing_args_size,
            spills_size: spills_bytes,
            locals_size: locals_bytes,
            callee_saved_size,
            shadow_space_size,
            red_zone_size: red_zone_bytes,
            alignment: abi_align,
        }
    }
}

/// Variadic argument passing rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariadicRules {
    /// Windows x64: Float variadics passed in both XMM and corresponding GPR slot.
    Win64ShadowSlots,
    /// System V AMD64: AL register contains count of SSE vector/float registers used (0..8).
    SysVAlVectorCount,
    /// Standard register / stack sequence without special register counts.
    Standard,
}

/// Rules for passing structs and aggregates as arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructPassingRules {
    /// Only 1, 2, 4, 8 byte aggregates passed by value in GPR; larger passed by reference (Win64).
    Win64SmallScalarOrIndirect,
    /// Aggregates up to 16 bytes flattened and passed in up to two registers (SysV / AAPCS64).
    FlattenUpTo16Bytes,
    /// Standard pass by invisible reference for all structs > pointer size.
    AlwaysIndirectIfLarge,
}

/// Rules for returning aggregates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregateReturnRules {
    /// Structs <= 8 bytes returned in RAX; all others via hidden sret pointer in first arg (RCX).
    Win64Sret,
    /// Structs <= 16 bytes returned in RAX:RDX; larger via hidden sret pointer in RDI.
    SysVSret,
    /// Structs <= 16 bytes returned in X0:X1; larger via hidden sret pointer in X8 (AAPCS64).
    Aapcs64Sret,
    /// Structs <= 16 bytes returned in a0:a1; larger via hidden sret pointer in a0 (RV64).
    RiscVSret,
}

/// Unwind metadata specification format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnwindRules {
    /// Windows x64 PE/COFF `.pdata` and `.xdata` UNWIND_INFO.
    Win64PData,
    /// DWARF Call Frame Information (`.eh_frame` / CIE / FDE).
    DwarfCfi,
    /// Compact unwind format (macOS).
    CompactUnwind,
    /// None (embedded or unmanaged).
    None,
}

/// Central Production ABI Specification trait.
pub trait AbiSpec: Send + Sync {
    fn name(&self) -> &'static str;
    fn target_abi(&self) -> TargetAbi;
    fn stack_alignment(&self) -> StackAlignment;
    fn shadow_space(&self) -> ShadowSpace;
    fn red_zone(&self) -> RedZone;
    fn callee_saved(&self) -> CalleeSavedSet;
    fn caller_saved(&self) -> CallerSavedSet;
    fn variadic_rules(&self) -> VariadicRules;
    fn struct_passing_rules(&self) -> StructPassingRules;
    fn aggregate_return_rules(&self) -> AggregateReturnRules;
    fn unwind_rules(&self) -> UnwindRules;

    /// Classify function arguments into registers or stack slots.
    fn classify_arguments(&self, args: &[AbiType]) -> Vec<ArgumentLocation>;

    /// Classify return type into register(s) or hidden sret.
    fn classify_return(&self, ret: &AbiType) -> ReturnLocation;

    /// Compute stack frame layout.
    fn compute_layout(
        &self,
        locals_bytes: usize,
        spills_bytes: usize,
        outgoing_bytes: usize,
        callee_saved_regs: usize,
    ) -> StackFrameLayout {
        StackFrameLayout::compute(
            self.stack_alignment().0,
            self.shadow_space().0,
            self.red_zone().0,
            locals_bytes,
            spills_bytes,
            outgoing_bytes,
            callee_saved_regs,
        )
    }
}

// ============================================================================
// Concrete Implementation: Windows x64 ABI
// ============================================================================

pub struct WindowsX64Abi;

const WIN64_GPR_ARGS: [PhysicalRegister; 4] = [
    PhysicalRegister(1), // RCX
    PhysicalRegister(2), // RDX
    PhysicalRegister(8), // R8
    PhysicalRegister(9), // R9
];

const WIN64_FPR_ARGS: [PhysicalRegister; 4] = [
    PhysicalRegister::xmm(0),
    PhysicalRegister::xmm(1),
    PhysicalRegister::xmm(2),
    PhysicalRegister::xmm(3),
];

impl AbiSpec for WindowsX64Abi {
    fn name(&self) -> &'static str {
        "x86_64-windows-msvc (Win64)"
    }
    fn target_abi(&self) -> TargetAbi {
        TargetAbi::Win64
    }
    fn stack_alignment(&self) -> StackAlignment {
        StackAlignment(16)
    }
    fn shadow_space(&self) -> ShadowSpace {
        ShadowSpace(32)
    }
    fn red_zone(&self) -> RedZone {
        RedZone(0)
    }
    fn callee_saved(&self) -> CalleeSavedSet {
        CalleeSavedSet {
            gpr: vec![
                PhysicalRegister(3),  // RBX
                PhysicalRegister(5),  // RBP
                PhysicalRegister(6),  // RSI
                PhysicalRegister(7),  // RDI
                PhysicalRegister(12), // R12
                PhysicalRegister(13), // R13
                PhysicalRegister(14), // R14
                PhysicalRegister(15), // R15
            ],
            fpr: (6..=15).map(PhysicalRegister::xmm).collect(),
        }
    }
    fn caller_saved(&self) -> CallerSavedSet {
        CallerSavedSet {
            gpr: vec![
                PhysicalRegister(0),  // RAX
                PhysicalRegister(1),  // RCX
                PhysicalRegister(2),  // RDX
                PhysicalRegister(8),  // R8
                PhysicalRegister(9),  // R9
                PhysicalRegister(10), // R10
                PhysicalRegister(11), // R11
            ],
            fpr: (0..=5).map(PhysicalRegister::xmm).collect(),
        }
    }
    fn variadic_rules(&self) -> VariadicRules {
        VariadicRules::Win64ShadowSlots
    }
    fn struct_passing_rules(&self) -> StructPassingRules {
        StructPassingRules::Win64SmallScalarOrIndirect
    }
    fn aggregate_return_rules(&self) -> AggregateReturnRules {
        AggregateReturnRules::Win64Sret
    }
    fn unwind_rules(&self) -> UnwindRules {
        UnwindRules::Win64PData
    }

    fn classify_arguments(&self, args: &[AbiType]) -> Vec<ArgumentLocation> {
        let mut locations = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            if i < 4 {
                match arg {
                    AbiType::Float { .. } => {
                        locations.push(ArgumentLocation::FloatRegister(WIN64_FPR_ARGS[i]));
                    }
                    AbiType::Vector { .. } => {
                        locations.push(ArgumentLocation::VectorRegister(WIN64_FPR_ARGS[i]));
                    }
                    AbiType::Struct { size, .. } => {
                        if [1, 2, 4, 8].contains(size) {
                            locations.push(ArgumentLocation::Register(WIN64_GPR_ARGS[i]));
                        } else {
                            locations
                                .push(ArgumentLocation::IndirectByReference(WIN64_GPR_ARGS[i]));
                        }
                    }
                    _ => {
                        locations.push(ArgumentLocation::Register(WIN64_GPR_ARGS[i]));
                    }
                }
            } else {
                // Stack arguments starting after 32 bytes shadow space + 16 bytes return addr/frame
                let offset = 48 + ((i - 4) as i32 * 8);
                locations.push(ArgumentLocation::Stack(StackArgument {
                    offset,
                    size: arg.size_in_bytes().max(8),
                    align: arg.alignment().max(8),
                }));
            }
        }
        locations
    }

    fn classify_return(&self, ret: &AbiType) -> ReturnLocation {
        match ret {
            AbiType::Void => ReturnLocation::Void,
            AbiType::Float { .. } => ReturnLocation::FloatRegister(PhysicalRegister::xmm(0)),
            AbiType::Vector { .. } => ReturnLocation::VectorRegister(PhysicalRegister::xmm(0)),
            AbiType::Struct { size, .. } => {
                if [1, 2, 4, 8].contains(size) {
                    ReturnLocation::Register(PhysicalRegister(0)) // RAX
                } else {
                    ReturnLocation::HiddenSret(PhysicalRegister(1)) // RCX
                }
            }
            _ => ReturnLocation::Register(PhysicalRegister(0)), // RAX
        }
    }
}

// ============================================================================
// Concrete Implementation: System V AMD64 ABI (Linux, macOS, BSD)
// ============================================================================

pub struct SystemVX64Abi;

const SYSV_GPR_ARGS: [PhysicalRegister; 6] = [
    PhysicalRegister(7), // RDI
    PhysicalRegister(6), // RSI
    PhysicalRegister(2), // RDX
    PhysicalRegister(1), // RCX
    PhysicalRegister(8), // R8
    PhysicalRegister(9), // R9
];

const SYSV_FPR_ARGS: [PhysicalRegister; 8] = [
    PhysicalRegister::xmm(0),
    PhysicalRegister::xmm(1),
    PhysicalRegister::xmm(2),
    PhysicalRegister::xmm(3),
    PhysicalRegister::xmm(4),
    PhysicalRegister::xmm(5),
    PhysicalRegister::xmm(6),
    PhysicalRegister::xmm(7),
];

impl AbiSpec for SystemVX64Abi {
    fn name(&self) -> &'static str {
        "x86_64-unknown-linux-gnu (System V AMD64)"
    }
    fn target_abi(&self) -> TargetAbi {
        TargetAbi::SysV
    }
    fn stack_alignment(&self) -> StackAlignment {
        StackAlignment(16)
    }
    fn shadow_space(&self) -> ShadowSpace {
        ShadowSpace(0)
    }
    fn red_zone(&self) -> RedZone {
        RedZone(128)
    }
    fn callee_saved(&self) -> CalleeSavedSet {
        CalleeSavedSet {
            gpr: vec![
                PhysicalRegister(3),  // RBX
                PhysicalRegister(5),  // RBP
                PhysicalRegister(12), // R12
                PhysicalRegister(13), // R13
                PhysicalRegister(14), // R14
                PhysicalRegister(15), // R15
            ],
            fpr: vec![], // All XMM volatile in SysV AMD64
        }
    }
    fn caller_saved(&self) -> CallerSavedSet {
        CallerSavedSet {
            gpr: vec![
                PhysicalRegister(0),  // RAX
                PhysicalRegister(1),  // RCX
                PhysicalRegister(2),  // RDX
                PhysicalRegister(6),  // RSI
                PhysicalRegister(7),  // RDI
                PhysicalRegister(8),  // R8
                PhysicalRegister(9),  // R9
                PhysicalRegister(10), // R10
                PhysicalRegister(11), // R11
            ],
            fpr: (0..=15).map(PhysicalRegister::xmm).collect(),
        }
    }
    fn variadic_rules(&self) -> VariadicRules {
        VariadicRules::SysVAlVectorCount
    }
    fn struct_passing_rules(&self) -> StructPassingRules {
        StructPassingRules::FlattenUpTo16Bytes
    }
    fn aggregate_return_rules(&self) -> AggregateReturnRules {
        AggregateReturnRules::SysVSret
    }
    fn unwind_rules(&self) -> UnwindRules {
        UnwindRules::DwarfCfi
    }

    fn classify_arguments(&self, args: &[AbiType]) -> Vec<ArgumentLocation> {
        let mut locations = Vec::new();
        let mut gpr_idx = 0;
        let mut fpr_idx = 0;
        let mut stack_offset = 16;

        for arg in args {
            match arg {
                AbiType::Float { .. } => {
                    if fpr_idx < SYSV_FPR_ARGS.len() {
                        locations.push(ArgumentLocation::FloatRegister(SYSV_FPR_ARGS[fpr_idx]));
                        fpr_idx += 1;
                    } else {
                        locations.push(ArgumentLocation::Stack(StackArgument {
                            offset: stack_offset,
                            size: 8,
                            align: 8,
                        }));
                        stack_offset += 8;
                    }
                }
                AbiType::Vector { .. } => {
                    if fpr_idx < SYSV_FPR_ARGS.len() {
                        locations.push(ArgumentLocation::VectorRegister(SYSV_FPR_ARGS[fpr_idx]));
                        fpr_idx += 1;
                    } else {
                        locations.push(ArgumentLocation::Stack(StackArgument {
                            offset: stack_offset,
                            size: 16,
                            align: 16,
                        }));
                        stack_offset += 16;
                    }
                }
                AbiType::Struct { size, .. } => {
                    if *size <= 8 && gpr_idx < SYSV_GPR_ARGS.len() {
                        locations.push(ArgumentLocation::Register(SYSV_GPR_ARGS[gpr_idx]));
                        gpr_idx += 1;
                    } else if *size <= 16 && gpr_idx + 1 < SYSV_GPR_ARGS.len() {
                        locations.push(ArgumentLocation::Pair(
                            SYSV_GPR_ARGS[gpr_idx],
                            SYSV_GPR_ARGS[gpr_idx + 1],
                        ));
                        gpr_idx += 2;
                    } else {
                        let sz = ((*size + 7) & !7) as i32;
                        locations.push(ArgumentLocation::Stack(StackArgument {
                            offset: stack_offset,
                            size: *size,
                            align: 8,
                        }));
                        stack_offset += sz;
                    }
                }
                _ => {
                    if gpr_idx < SYSV_GPR_ARGS.len() {
                        locations.push(ArgumentLocation::Register(SYSV_GPR_ARGS[gpr_idx]));
                        gpr_idx += 1;
                    } else {
                        locations.push(ArgumentLocation::Stack(StackArgument {
                            offset: stack_offset,
                            size: 8,
                            align: 8,
                        }));
                        stack_offset += 8;
                    }
                }
            }
        }
        locations
    }

    fn classify_return(&self, ret: &AbiType) -> ReturnLocation {
        match ret {
            AbiType::Void => ReturnLocation::Void,
            AbiType::Float { .. } => ReturnLocation::FloatRegister(PhysicalRegister::xmm(0)),
            AbiType::Vector { .. } => ReturnLocation::VectorRegister(PhysicalRegister::xmm(0)),
            AbiType::Struct { size, .. } => {
                if *size <= 8 {
                    ReturnLocation::Register(PhysicalRegister(0)) // RAX
                } else if *size <= 16 {
                    ReturnLocation::Pair(PhysicalRegister(0), PhysicalRegister(2)) // RAX:RDX
                } else {
                    ReturnLocation::HiddenSret(PhysicalRegister(7)) // RDI
                }
            }
            _ => ReturnLocation::Register(PhysicalRegister(0)), // RAX
        }
    }
}

// ============================================================================
// Concrete Implementation: AArch64 AAPCS64
// ============================================================================

pub struct Aapcs64Abi;

const AAPCS64_GPR_ARGS: [PhysicalRegister; 8] = [
    PhysicalRegister(0),
    PhysicalRegister(1),
    PhysicalRegister(2),
    PhysicalRegister(3),
    PhysicalRegister(4),
    PhysicalRegister(5),
    PhysicalRegister(6),
    PhysicalRegister(7),
];

const AAPCS64_FPR_ARGS: [PhysicalRegister; 8] = [
    PhysicalRegister(32), // V0
    PhysicalRegister(33), // V1
    PhysicalRegister(34), // V2
    PhysicalRegister(35), // V3
    PhysicalRegister(36), // V4
    PhysicalRegister(37), // V5
    PhysicalRegister(38), // V6
    PhysicalRegister(39), // V7
];

impl AbiSpec for Aapcs64Abi {
    fn name(&self) -> &'static str {
        "aarch64-unknown-linux-gnu (AAPCS64)"
    }
    fn target_abi(&self) -> TargetAbi {
        TargetAbi::Aapcs64
    }
    fn stack_alignment(&self) -> StackAlignment {
        StackAlignment(16)
    }
    fn shadow_space(&self) -> ShadowSpace {
        ShadowSpace(0)
    }
    fn red_zone(&self) -> RedZone {
        RedZone(0)
    }
    fn callee_saved(&self) -> CalleeSavedSet {
        CalleeSavedSet {
            gpr: (19..=29).map(PhysicalRegister).collect(), // X19..X29
            fpr: (40..=47).map(PhysicalRegister).collect(), // V8..V15 (bottom 64 bits)
        }
    }
    fn caller_saved(&self) -> CallerSavedSet {
        CallerSavedSet {
            gpr: (0..=18).map(PhysicalRegister).collect(),
            fpr: (32..=63)
                .filter(|&r| !(40..=47).contains(&r))
                .map(PhysicalRegister)
                .collect(),
        }
    }
    fn variadic_rules(&self) -> VariadicRules {
        VariadicRules::Standard
    }
    fn struct_passing_rules(&self) -> StructPassingRules {
        StructPassingRules::FlattenUpTo16Bytes
    }
    fn aggregate_return_rules(&self) -> AggregateReturnRules {
        AggregateReturnRules::Aapcs64Sret
    }
    fn unwind_rules(&self) -> UnwindRules {
        UnwindRules::DwarfCfi
    }

    fn classify_arguments(&self, args: &[AbiType]) -> Vec<ArgumentLocation> {
        let mut locations = Vec::new();
        let mut gpr_idx = 0;
        let mut fpr_idx = 0;
        let mut stack_offset = 0;

        for arg in args {
            match arg {
                AbiType::Float { .. } => {
                    if fpr_idx < AAPCS64_FPR_ARGS.len() {
                        locations.push(ArgumentLocation::FloatRegister(AAPCS64_FPR_ARGS[fpr_idx]));
                        fpr_idx += 1;
                    } else {
                        locations.push(ArgumentLocation::Stack(StackArgument {
                            offset: stack_offset,
                            size: 8,
                            align: 8,
                        }));
                        stack_offset += 8;
                    }
                }
                AbiType::Vector { .. } => {
                    if fpr_idx < AAPCS64_FPR_ARGS.len() {
                        locations.push(ArgumentLocation::VectorRegister(AAPCS64_FPR_ARGS[fpr_idx]));
                        fpr_idx += 1;
                    } else {
                        locations.push(ArgumentLocation::Stack(StackArgument {
                            offset: stack_offset,
                            size: 16,
                            align: 16,
                        }));
                        stack_offset += 16;
                    }
                }
                AbiType::Struct { size, .. } => {
                    if *size <= 8 && gpr_idx < AAPCS64_GPR_ARGS.len() {
                        locations.push(ArgumentLocation::Register(AAPCS64_GPR_ARGS[gpr_idx]));
                        gpr_idx += 1;
                    } else if *size <= 16 && gpr_idx + 1 < AAPCS64_GPR_ARGS.len() {
                        locations.push(ArgumentLocation::Pair(
                            AAPCS64_GPR_ARGS[gpr_idx],
                            AAPCS64_GPR_ARGS[gpr_idx + 1],
                        ));
                        gpr_idx += 2;
                    } else {
                        let sz = ((*size + 7) & !7) as i32;
                        locations.push(ArgumentLocation::Stack(StackArgument {
                            offset: stack_offset,
                            size: *size,
                            align: 8,
                        }));
                        stack_offset += sz;
                    }
                }
                _ => {
                    if gpr_idx < AAPCS64_GPR_ARGS.len() {
                        locations.push(ArgumentLocation::Register(AAPCS64_GPR_ARGS[gpr_idx]));
                        gpr_idx += 1;
                    } else {
                        locations.push(ArgumentLocation::Stack(StackArgument {
                            offset: stack_offset,
                            size: 8,
                            align: 8,
                        }));
                        stack_offset += 8;
                    }
                }
            }
        }
        locations
    }

    fn classify_return(&self, ret: &AbiType) -> ReturnLocation {
        match ret {
            AbiType::Void => ReturnLocation::Void,
            AbiType::Float { .. } => ReturnLocation::FloatRegister(PhysicalRegister(32)), // V0
            AbiType::Vector { .. } => ReturnLocation::VectorRegister(PhysicalRegister(32)), // V0
            AbiType::Struct { size, .. } => {
                if *size <= 8 {
                    ReturnLocation::Register(PhysicalRegister(0)) // X0
                } else if *size <= 16 {
                    ReturnLocation::Pair(PhysicalRegister(0), PhysicalRegister(1)) // X0:X1
                } else {
                    ReturnLocation::HiddenSret(PhysicalRegister(8)) // X8
                }
            }
            _ => ReturnLocation::Register(PhysicalRegister(0)), // X0
        }
    }
}

// ============================================================================
// Concrete Implementation: RISC-V RV64 Standard ABI
// ============================================================================

pub struct RiscV64Abi;

const RISCV_GPR_ARGS: [PhysicalRegister; 8] = [
    PhysicalRegister(10), // a0
    PhysicalRegister(11), // a1
    PhysicalRegister(12), // a2
    PhysicalRegister(13), // a3
    PhysicalRegister(14), // a4
    PhysicalRegister(15), // a5
    PhysicalRegister(16), // a6
    PhysicalRegister(17), // a7
];

const RISCV_FPR_ARGS: [PhysicalRegister; 8] = [
    PhysicalRegister(42), // fa0 (F10)
    PhysicalRegister(43), // fa1 (F11)
    PhysicalRegister(44), // fa2 (F12)
    PhysicalRegister(45), // fa3 (F13)
    PhysicalRegister(46), // fa4 (F14)
    PhysicalRegister(47), // fa5 (F15)
    PhysicalRegister(48), // fa6 (F16)
    PhysicalRegister(49), // fa7 (F17)
];

impl AbiSpec for RiscV64Abi {
    fn name(&self) -> &'static str {
        "riscv64gc-unknown-linux-gnu (RV64 Standard ABI)"
    }
    fn target_abi(&self) -> TargetAbi {
        TargetAbi::SysV
    }
    fn stack_alignment(&self) -> StackAlignment {
        StackAlignment(16)
    }
    fn shadow_space(&self) -> ShadowSpace {
        ShadowSpace(0)
    }
    fn red_zone(&self) -> RedZone {
        RedZone(0)
    }
    fn callee_saved(&self) -> CalleeSavedSet {
        CalleeSavedSet {
            gpr: vec![
                PhysicalRegister(2),  // sp
                PhysicalRegister(8),  // s0/fp
                PhysicalRegister(9),  // s1
                PhysicalRegister(18), // s2
                PhysicalRegister(19), // s3
                PhysicalRegister(20), // s4
                PhysicalRegister(21), // s5
                PhysicalRegister(22), // s6
                PhysicalRegister(23), // s7
                PhysicalRegister(24), // s8
                PhysicalRegister(25), // s9
                PhysicalRegister(26), // s10
                PhysicalRegister(27), // s11
            ],
            fpr: (40..=41).chain(50..=63).map(PhysicalRegister).collect(), // fs0-fs1, fs2-fs11
        }
    }
    fn caller_saved(&self) -> CallerSavedSet {
        CallerSavedSet {
            gpr: vec![
                PhysicalRegister(1),  // ra
                PhysicalRegister(5),  // t0
                PhysicalRegister(6),  // t1
                PhysicalRegister(7),  // t2
                PhysicalRegister(10), // a0
                PhysicalRegister(11), // a1
                PhysicalRegister(12), // a2
                PhysicalRegister(13), // a3
                PhysicalRegister(14), // a4
                PhysicalRegister(15), // a5
                PhysicalRegister(16), // a6
                PhysicalRegister(17), // a7
                PhysicalRegister(28), // t3
                PhysicalRegister(29), // t4
                PhysicalRegister(30), // t5
                PhysicalRegister(31), // t6
            ],
            fpr: (32..=39).chain(42..=49).map(PhysicalRegister).collect(),
        }
    }
    fn variadic_rules(&self) -> VariadicRules {
        VariadicRules::Standard
    }
    fn struct_passing_rules(&self) -> StructPassingRules {
        StructPassingRules::FlattenUpTo16Bytes
    }
    fn aggregate_return_rules(&self) -> AggregateReturnRules {
        AggregateReturnRules::RiscVSret
    }
    fn unwind_rules(&self) -> UnwindRules {
        UnwindRules::DwarfCfi
    }

    fn classify_arguments(&self, args: &[AbiType]) -> Vec<ArgumentLocation> {
        let mut locations = Vec::new();
        let mut gpr_idx = 0;
        let mut fpr_idx = 0;
        let mut stack_offset = 0;

        for arg in args {
            match arg {
                AbiType::Float { .. } => {
                    if fpr_idx < RISCV_FPR_ARGS.len() {
                        locations.push(ArgumentLocation::FloatRegister(RISCV_FPR_ARGS[fpr_idx]));
                        fpr_idx += 1;
                    } else {
                        locations.push(ArgumentLocation::Stack(StackArgument {
                            offset: stack_offset,
                            size: 8,
                            align: 8,
                        }));
                        stack_offset += 8;
                    }
                }
                AbiType::Vector { .. } => {
                    if fpr_idx < RISCV_FPR_ARGS.len() {
                        locations.push(ArgumentLocation::VectorRegister(RISCV_FPR_ARGS[fpr_idx]));
                        fpr_idx += 1;
                    } else {
                        locations.push(ArgumentLocation::Stack(StackArgument {
                            offset: stack_offset,
                            size: 16,
                            align: 16,
                        }));
                        stack_offset += 16;
                    }
                }
                AbiType::Struct { size, .. } => {
                    if *size <= 8 && gpr_idx < RISCV_GPR_ARGS.len() {
                        locations.push(ArgumentLocation::Register(RISCV_GPR_ARGS[gpr_idx]));
                        gpr_idx += 1;
                    } else if *size <= 16 && gpr_idx + 1 < RISCV_GPR_ARGS.len() {
                        locations.push(ArgumentLocation::Pair(
                            RISCV_GPR_ARGS[gpr_idx],
                            RISCV_GPR_ARGS[gpr_idx + 1],
                        ));
                        gpr_idx += 2;
                    } else {
                        let sz = ((*size + 7) & !7) as i32;
                        locations.push(ArgumentLocation::Stack(StackArgument {
                            offset: stack_offset,
                            size: *size,
                            align: 8,
                        }));
                        stack_offset += sz;
                    }
                }
                _ => {
                    if gpr_idx < RISCV_GPR_ARGS.len() {
                        locations.push(ArgumentLocation::Register(RISCV_GPR_ARGS[gpr_idx]));
                        gpr_idx += 1;
                    } else {
                        locations.push(ArgumentLocation::Stack(StackArgument {
                            offset: stack_offset,
                            size: 8,
                            align: 8,
                        }));
                        stack_offset += 8;
                    }
                }
            }
        }
        locations
    }

    fn classify_return(&self, ret: &AbiType) -> ReturnLocation {
        match ret {
            AbiType::Void => ReturnLocation::Void,
            AbiType::Float { .. } => ReturnLocation::FloatRegister(PhysicalRegister(42)), // fa0
            AbiType::Vector { .. } => ReturnLocation::VectorRegister(PhysicalRegister(42)),
            AbiType::Struct { size, .. } => {
                if *size <= 8 {
                    ReturnLocation::Register(PhysicalRegister(10)) // a0
                } else if *size <= 16 {
                    ReturnLocation::Pair(PhysicalRegister(10), PhysicalRegister(11)) // a0:a1
                } else {
                    ReturnLocation::HiddenSret(PhysicalRegister(10)) // a0
                }
            }
            _ => ReturnLocation::Register(PhysicalRegister(10)), // a0
        }
    }
}

/// Factory function to create the appropriate `AbiSpec` for a `TargetSpec`.
pub fn create_abi_spec(spec: &TargetSpec) -> Box<dyn AbiSpec> {
    use adesh_object::Architecture;
    match (&spec.architecture, spec.abi) {
        (Architecture::X86_64, TargetAbi::Win64) => Box::new(WindowsX64Abi),
        (Architecture::X86_64, _) => Box::new(SystemVX64Abi),
        (Architecture::AArch64, _) => Box::new(Aapcs64Abi),
        (Architecture::RiscV64, _) => Box::new(RiscV64Abi),
        _ => Box::new(SystemVX64Abi),
    }
}
