//! ELF AArch64 Architecture and Entry Point Synthesis.

use crate::relocation::{Relocation, RelocationKind};
use crate::symbol::{Symbol, SymbolBinding, SymbolType};

/// Synthesize standard Linux ELF AArch64 startup code (`_start` -> `main` -> `sys_exit`).
///
/// AAPCS64 / Linux AArch64 Kernel Execution Requirements:
/// - Clear frame pointer (x29) and link register (x30) to signal bottom of call stack.
/// - Read argc from [sp] into x0.
/// - Compute argv as sp + 8 into x1.
/// - Compute envp as argv + argc*8 + 8 into x2.
/// - 16-byte align sp before the bl instruction (sp already 16-aligned at kernel entry).
/// - BL to main / user entry.
/// - Move return value from w0 into x0 for exit status.
/// - Issue Linux sys_exit (syscall 93) via svc #0.
pub fn synthesize_elf_aarch64_entry(
    program_entry_symbol: &str,
    file_index: usize,
) -> (Vec<u8>, Vec<Relocation>, Vec<Symbol>) {
    let mut code_bytes: Vec<u8> = Vec::new();
    let mut relocs = Vec::new();
    let mut symbols = Vec::new();

    let start_off = code_bytes.len() as u64;
    let insns: &[u32] = &[
        0xAA1F03FD, // mov x29, xzr  (clear frame pointer)
        0xAA1F03FE, // mov x30, xzr  (clear link register)
        0xF9400000, // ldr x0, [sp]  (argc)
        0x910023E1, // add x1, sp, #8  (argv)
        0x8B000422, // add x2, x1, x0, lsl #3  (envp start)
        0x91002042, // add x2, x2, #8  (envp = argv + argc*8 + 8)
        0x94000000, // bl <main> -- relocation applied at byte offset 24 (6*4)
        0xD2800BA8, // mov x8, #93  (sys_exit syscall number)
        0xD4000001, // svc #0
        0xD4200000, // brk #0  (trap if svc returns)
    ];
    for &insn in insns {
        code_bytes.extend_from_slice(&insn.to_le_bytes());
    }
    let start_sz = code_bytes.len() as u64;

    // BL relocation at byte offset 24 (instruction index 6, each insn = 4 bytes)
    relocs.push(Relocation {
        offset: start_off + 24,
        symbol_name: program_entry_symbol.to_string(),
        symbol_index: None,
        file_index: None,
        kind: RelocationKind::AArch64Call26,
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
        code_bytes.extend_from_slice(&0xD503201Fu32.to_le_bytes()); // nop
    }

    (code_bytes, relocs, symbols)
}

/// Synthesize an AArch64 ELF PLT stub for an imported dynamic symbol.
///
/// AArch64 PLT stub:
///   adrp x16, page(got_slot)
///   ldr  x17, [x16, pageoff(got_slot)]
///   add  x16, x16, pageoff(got_slot)
///   br   x17
///
/// Relocations:
///   R_AARCH64_ADR_PREL_PG_HI21 at offset 0 against __got_<symbol>
///   R_AARCH64_LD64_GOT_LO12_NC at offset 4 against __got_<symbol>
///   R_AARCH64_ADD_ABS_LO12_NC  at offset 8 against __got_<symbol>
pub fn synthesize_elf_aarch64_plt_stub(
    symbol_name: &str,
    file_index: usize,
    base_offset: u64,
) -> (Vec<u8>, Vec<Relocation>, Symbol) {
    let bytes: Vec<u8> = [
        0x10u8, 0x00, 0x00, 0x90, // adrp x16, page  (reloc: AArch64Adrp)
        0x11, 0x02, 0x40, 0xF9, // ldr  x17, [x16, off]  (reloc: AArch64AddLo12 on ldr)
        0x10, 0x02, 0x00, 0x91, // add  x16, x16, off12  (reloc: AArch64AddLo12)
        0x20, 0x02, 0x1F, 0xD6, // br   x17
    ]
    .to_vec();

    let got_sym = format!("__got_{}", symbol_name);

    let relocs = vec![
        Relocation {
            offset: base_offset,
            symbol_name: got_sym.clone(),
            symbol_index: None,
            file_index: None,
            kind: RelocationKind::AArch64Adrp,
            addend: 0,
        },
        Relocation {
            offset: base_offset + 4,
            symbol_name: got_sym.clone(),
            symbol_index: None,
            file_index: None,
            kind: RelocationKind::AArch64AddLo12,
            addend: 0,
        },
        Relocation {
            offset: base_offset + 8,
            symbol_name: got_sym,
            symbol_index: None,
            file_index: None,
            kind: RelocationKind::AArch64AddLo12,
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
        size: 16,
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
    fn test_elf_aarch64_entry_synthesis() {
        let (bytes, relocs, symbols) = synthesize_elf_aarch64_entry("main", 0);

        assert!(!bytes.is_empty());
        assert!(bytes.len() % 16 == 0, "must be 16-byte aligned");
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "_start");
        assert_eq!(symbols[0].binding, SymbolBinding::Global);
        assert_eq!(symbols[0].sym_type, SymbolType::Function);

        // BL reloc at byte offset 24
        let bl_reloc = relocs.iter().find(|r| r.offset == 24).unwrap();
        assert_eq!(bl_reloc.symbol_name, "main");
        assert_eq!(bl_reloc.kind, RelocationKind::AArch64Call26);
        assert_eq!(bl_reloc.addend, 0);

        // Verify the BL instruction template (0x94000000 = BL #0, little-endian)
        let bl_bytes = &bytes[24..28];
        assert_eq!(bl_bytes, &[0x00, 0x00, 0x00, 0x94]);

        // Verify sys_exit (x8 = 93 = 0x5D): mov x8, #93 => D2800BA8 LE: A8 0B 80 D2
        assert_eq!(&bytes[28..32], &[0xA8, 0x0B, 0x80, 0xD2]);
    }

    #[test]
    fn test_elf_aarch64_plt_stub_synthesis() {
        let (bytes, relocs, sym) = synthesize_elf_aarch64_plt_stub("printf", 0, 0);
        assert_eq!(bytes.len(), 16);
        assert_eq!(sym.name, "printf");
        assert_eq!(relocs.len(), 3);
        assert_eq!(relocs[0].kind, RelocationKind::AArch64Adrp);
        assert_eq!(relocs[0].offset, 0);
        assert_eq!(relocs[0].symbol_name, "__got_printf");
        assert_eq!(relocs[1].kind, RelocationKind::AArch64AddLo12);
        assert_eq!(relocs[1].offset, 4);
        assert_eq!(relocs[2].kind, RelocationKind::AArch64AddLo12);
        assert_eq!(relocs[2].offset, 8);
    }
}
