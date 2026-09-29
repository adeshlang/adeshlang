//! Embedded bare-metal microcontroller and custom chip linking infrastructure.
//!
//! Provides:
//! - Custom GNU/LLD compatible linker script engine (`MEMORY` and `SECTIONS` evaluation).
//! - Interrupt Vector Table (IVT) layout for ARM Cortex-M & RISC-V.
//! - Zero-dependency baremetal startup stubs (`crt0` for RAM data copy & BSS zeroing).
//! - Flash firmware image writers (Raw `.bin`, Intel HEX `.hex`, Motorola `.srec`).

pub mod flash;
pub mod ivt;
pub mod script;
pub mod startup;

pub use flash::{FlashBinaryWriter, IntelHexWriter, MotorolaSrecWriter};
pub use ivt::{ArmCortexVectorTable, RiscvTrapVectorTable};
pub use script::{LinkerScript, MemoryRegion, SectionAssignment};
pub use startup::BaremetalStartupGenerator;
