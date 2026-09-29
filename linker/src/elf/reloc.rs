//! ELF architecture-specific relocation definitions and conversions.

use crate::elf::header::*;
use crate::relocation::RelocationKind;
use crate::target::Arch;

pub fn relocation_kind_to_elf_type(arch: Arch, kind: RelocationKind) -> u32 {
    match (arch, kind) {
        (Arch::X86_64, RelocationKind::Absolute64) => R_X86_64_64,
        (Arch::X86_64, RelocationKind::Absolute32) => R_X86_64_32,
        (Arch::X86_64, RelocationKind::PcRelative32) => R_X86_64_PC32,
        (Arch::X86_64, RelocationKind::PltRelative32) => R_X86_64_PLT32,
        (Arch::X86_64, RelocationKind::GotRelative32) => R_X86_64_GOTPCREL,
        (Arch::X86_64, RelocationKind::PcRelative64) => R_X86_64_PC64,

        (Arch::AArch64, RelocationKind::Absolute64) => R_AARCH64_ABS64,
        (Arch::AArch64, RelocationKind::Absolute32) => R_AARCH64_ABS32,
        (Arch::AArch64, RelocationKind::AArch64Call26) => R_AARCH64_CALL26,
        (Arch::AArch64, RelocationKind::AArch64Adrp) => R_AARCH64_ADR_PREL_PG_HI21,
        (Arch::AArch64, RelocationKind::AArch64AddLo12) => R_AARCH64_ADD_ABS_LO12_NC,

        (Arch::Riscv64 | Arch::Riscv32, RelocationKind::Absolute64) => R_RISCV_64,
        (Arch::Riscv64 | Arch::Riscv32, RelocationKind::Absolute32) => R_RISCV_32,
        (Arch::Riscv64 | Arch::Riscv32, RelocationKind::RiscvCall) => R_RISCV_CALL,
        (Arch::Riscv64 | Arch::Riscv32, RelocationKind::RiscvBranch) => R_RISCV_BRANCH,
        (Arch::Riscv64 | Arch::Riscv32, RelocationKind::RiscvHi20) => R_RISCV_HI20,
        (Arch::Riscv64 | Arch::Riscv32, RelocationKind::RiscvLo12I) => R_RISCV_LO12_I,

        _ => 0,
    }
}
