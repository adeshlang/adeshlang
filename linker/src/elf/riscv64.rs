//! ELF RISC-V 64 Architecture and Entry Point Synthesis.

use crate::relocation::{Relocation, RelocationKind};
use crate::symbol::{Symbol, SymbolBinding, SymbolType};

/// Synthesize standard Linux ELF RISC-V 64 startup code (`_start` -> `main` -> `sys_exit`).
///
/// RISC-V LP64 / Linux RV64 Kernel Execution Requirements:
/// - Clear frame pointer (`s0` / `x8`) and return address (`ra` / `x1`) to signal outermost stack frame.
/// - Linux kernel entry convention:
///   - `[sp]` contains `argc` (64-bit int).
///   - `sp + 8` contains `argv`.
///   - `envp = argv + argc*8 + 8`.
/// - Read `argc` into `a0` (`ld a0, 0(sp)`).
/// - Compute `argv` into `a1` (`addi a1, sp, 8`).
/// - Compute `envp` into `a2`:
///   - `slli t0, a0, 3` (argc * 8)
///   - `add  a2, a1, t0` (argv + argc*8)
///   - `addi a2, a2, 8`  (envp = argv + argc*8 + 8)
/// - Align `sp` to 16 bytes (`andi sp, sp, -16`).
/// - Call user entry (`main`):
///   - `auipc ra, 0`
///   - `jalr  ra, ra, 0` (relocation `RelocationKind::RiscvCall` at the `auipc` offset)
/// - The return value from `main` is in `a0`.
/// - Issue Linux `sys_exit` (syscall 93):
///   - `li a7, 93` (`addi a7, zero, 93`)
///   - `ecall`
///   - `ebreak` (trap if ecall returns)
pub fn synthesize_elf_riscv64_entry(
    program_entry_symbol: &str,
    file_index: usize,
) -> (Vec<u8>, Vec<Relocation>, Vec<Symbol>) {
    let mut code_bytes: Vec<u8> = Vec::new();
    let mut relocs = Vec::new();
    let mut symbols = Vec::new();

    let start_off = code_bytes.len() as u64;

    let insns: &[u32] = &[
        0x00000413, // addi s0, zero, 0 (clear fp / x8)
        0x00000093, // addi ra, zero, 0 (clear ra / x1)
        0x00013503, // ld a0, 0(sp)     (argc)
        0x00810593, // addi a1, sp, 8   (argv)
        0x00351293, // slli t0, a0, 3   (argc * 8)
        0x00558633, // add a2, a1, t0   (argv + argc*8)
        0x00860613, // addi a2, a2, 8   (envp)
        0xFF010113, // andi sp, sp, -16 (align stack to 16 bytes)
        0x00000097, // auipc ra, 0      (CALL reloc offset 32)
        0x000080E7, // jalr ra, ra, 0
        0x05D00893, // addi a7, zero, 93 (sys_exit syscall 93)
        0x00000073, // ecall
        0x00100073, // ebreak
    ];

    for &insn in insns {
        code_bytes.extend_from_slice(&insn.to_le_bytes());
    }
    let start_sz = code_bytes.len() as u64;

    // Relocation for auipc + jalr CALL sequence at byte offset 32 (insn index 8)
    relocs.push(Relocation {
        offset: start_off + 32,
        symbol_name: program_entry_symbol.to_string(),
        symbol_index: None,
        file_index: None,
        kind: RelocationKind::RiscvCall,
        addend: 0,
    });

    symbols.push(Symbol {
        name: "_start".to_string(),
        binding: SymbolBinding::Global,
        visibility: crate::symbol::SymbolVisibility::Default,
        sym_type: SymbolType::Function,
        section_index: Some(0),
        value: start_off,
        size: start_sz,
        is_defined: true,
        is_imported: false,
        is_exported: false,
        file_index: Some(file_index),
        alias_of: None,
        comdat_group: None,
        version: None,
    });

    // 16-byte align
    while (code_bytes.len() & 15) != 0 {
        code_bytes.extend_from_slice(&0x00000013u32.to_le_bytes()); // nop (addi x0, x0, 0)
    }

    (code_bytes, relocs, symbols)
}

/// Synthesize a standard RISC-V 64 ELF PLT stub for an imported dynamic symbol.
///
/// RISC-V 64 PLT stub:
///   auipc t3, %pcrel_hi(got_slot)
///   ld    t3, %pcrel_lo(got_slot)(t3)
///   jalr  t1, t3, 0
///   nop
pub fn synthesize_elf_riscv64_plt_stub(
    symbol_name: &str,
    file_index: usize,
    base_offset: u64,
) -> (Vec<u8>, Vec<Relocation>, Symbol) {
    let got_symbol = format!("__got_{}", symbol_name);
    let mut bytes = Vec::new();

    let insns: &[u32] = &[
        0x00000e17, // auipc t3, 0
        0x000e3e03, // ld t3, 0(t3)
        0x000e0367, // jalr t1, t3, 0
        0x00000013, // nop
    ];
    for &insn in insns {
        bytes.extend_from_slice(&insn.to_le_bytes());
    }

    let relocs = vec![
        Relocation {
            offset: base_offset,
            symbol_name: got_symbol.clone(),
            symbol_index: None,
            file_index: None,
            kind: RelocationKind::RiscvHi20,
            addend: 0,
        },
        Relocation {
            offset: base_offset + 4,
            symbol_name: got_symbol,
            symbol_index: None,
            file_index: None,
            kind: RelocationKind::RiscvLo12I,
            addend: 0,
        },
    ];

    let sym = Symbol {
        name: symbol_name.to_string(),
        binding: SymbolBinding::Global,
        visibility: crate::symbol::SymbolVisibility::Default,
        sym_type: SymbolType::Function,
        section_index: Some(0),
        value: base_offset,
        size: bytes.len() as u64,
        is_defined: true,
        is_imported: false,
        is_exported: false,
        file_index: Some(file_index),
        alias_of: None,
        comdat_group: None,
        version: None,
    };

    (bytes, relocs, sym)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_synthesize_elf_riscv64_entry_structure() {
        let (bytes, relocs, symbols) = synthesize_elf_riscv64_entry("main", 0);
        assert!(!bytes.is_empty());
        assert_eq!(bytes.len() % 16, 0); // 16-byte aligned
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "_start");
        assert_eq!(relocs.len(), 1);
        assert_eq!(relocs[0].symbol_name, "main");
        assert_eq!(relocs[0].kind, RelocationKind::RiscvCall);
        assert_eq!(relocs[0].offset, 32);
    }

    #[test]
    fn test_synthesize_elf_riscv64_plt_stub() {
        let (bytes, relocs, sym) = synthesize_elf_riscv64_plt_stub("puts", 1, 0x100);
        assert_eq!(bytes.len(), 16);
        assert_eq!(sym.name, "puts");
        assert_eq!(relocs.len(), 2);
        assert_eq!(relocs[0].symbol_name, "__got_puts");
        assert_eq!(relocs[0].kind, RelocationKind::RiscvHi20);
        assert_eq!(relocs[1].symbol_name, "__got_puts");
        assert_eq!(relocs[1].kind, RelocationKind::RiscvLo12I);
    }
}
