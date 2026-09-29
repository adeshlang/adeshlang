//! Architecture handlers and relocation dispatch.

pub mod aarch64;
pub mod arm;
pub mod gpu;
pub mod loong64;
pub mod mips;
pub mod npu;
pub mod ppc64;
pub mod qpu;
pub mod riscv32;
pub mod riscv64;
pub mod s390x;
pub mod sparc64;
pub mod tpu;
pub mod wasm32;
pub mod wasm64;
pub mod x86_64;

use crate::relocation::RelocationHandler;
use crate::target::Arch;

pub use aarch64::AArch64Arch;
pub use arm::ArmArch;
pub use gpu::GpuArch;
pub use loong64::Loong64Arch;
pub use mips::MipsArch;
pub use npu::NpuArch;
pub use ppc64::Ppc64Arch;
pub use qpu::QpuArch;
pub use riscv32::Riscv32Arch;
pub use riscv64::Riscv64Arch;
pub use s390x::S390xArch;
pub use sparc64::Sparc64Arch;
pub use tpu::TpuArch;
pub use wasm32::Wasm32Arch;
pub use wasm64::Wasm64Arch;
pub use x86_64::X86_64Arch;

/// Obtain the appropriate relocation handler for the given architecture.
pub fn get_handler(arch: Arch) -> Box<dyn RelocationHandler> {
    match arch {
        Arch::X86_64 => Box::new(X86_64Arch),
        Arch::X86 => Box::new(X86_64Arch),
        Arch::AArch64 => Box::new(AArch64Arch),
        Arch::Arm => Box::new(ArmArch),
        Arch::Ppc64 | Arch::Ppc64le => Box::new(Ppc64Arch),
        Arch::S390x => Box::new(S390xArch),
        Arch::Mips | Arch::Mipsle | Arch::Mips64 | Arch::Mips64le => Box::new(MipsArch),
        Arch::Riscv64 => Box::new(Riscv64Arch),
        Arch::Riscv32 => Box::new(Riscv32Arch),
        Arch::Loong64 => Box::new(Loong64Arch),
        Arch::Sparc64 => Box::new(Sparc64Arch),
        Arch::Wasm32 => Box::new(Wasm32Arch),
        Arch::Wasm64 => Box::new(Wasm64Arch),
        Arch::NvidiaPtx | Arch::AmdGpuHsa | Arch::SpirV => Box::new(GpuArch),
        Arch::HexagonDsp | Arch::AppleAne | Arch::ArmEthos => Box::new(NpuArch),
        Arch::GoogleTpu => Box::new(TpuArch),
        Arch::QuantumQpu => Box::new(QpuArch),
    }
}
