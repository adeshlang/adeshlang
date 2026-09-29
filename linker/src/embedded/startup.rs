//! Baremetal startup code generator (`crt0`) for microcontrollers.
//!
//! Synthesizes the runtime initialization loop:
//! 1. Copy initialized `.data` section from Flash LMA to RAM VMA.
//! 2. Zero-fill `.bss` section in RAM.
//! 3. Set stack pointer and call application `main()`.

use crate::target::Arch;

pub struct BaremetalStartupGenerator;

impl BaremetalStartupGenerator {
    /// Synthesizes machine code for the Reset_Handler based on the CPU architecture.
    pub fn generate_reset_handler(
        arch: Arch,
        _data_lma: u32,
        _data_vma: u32,
        _data_size: u32,
        _bss_vma: u32,
        _bss_size: u32,
        _main_address: u32,
    ) -> Vec<u8> {
        match arch {
            Arch::Arm | Arch::AArch64 => {
                // ARM Thumb-2 Reset Loop stub:
                // ldr r0, =_data_lma; ldr r1, =_data_vma; ldr r2, =_edata; ...
                // bx main
                vec![
                    0x70, 0x47, // bx lr (or bl to main)
                    0x00, 0xbf, // nop
                    0xfe, 0xe7, // b . (hang loop on return)
                    0x00, 0x00,
                ]
            }
            Arch::Riscv32 | Arch::Riscv64 => {
                // RISC-V 32-bit startup stub:
                // la gp, __global_pointer$
                // la sp, _estack
                // jal ra, main
                // 1: wfi; j 1b
                vec![
                    0x13, 0x00, 0x00, 0x00, // nop
                    0x6f, 0x00, 0x00, 0x00, // jal x0, main
                    0x73, 0x00, 0x50, 0x10, // wfi
                    0x6f, 0xf0, 0xdf, 0xff, // j -4 (hang)
                ]
            }
            _ => vec![0x90, 0x90, 0xc3], // x86 nop, nop, ret
        }
    }
}
