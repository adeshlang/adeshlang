//! Native Machine Code Encoders and Instruction Emitters.
//!
//! Provides self-contained, table-free, bit-accurate instruction generation for:
//! - x86_64 (AMD64 / Intel 64) with REX, ModR/M, SIB, and SSE/AVX vector extensions.
//! - AArch64 (ARM 64-bit) with fixed-width 32-bit instruction encoding.
//! - RISC-V (RV32I / RV64I) with R/I/S/B/U/J instruction formats.

pub mod aarch64;
pub mod riscv;
pub mod x86_64;

pub use aarch64::Aarch64Encoder;
pub use riscv::RiscvEncoder;
pub use x86_64::X86_64Encoder;
