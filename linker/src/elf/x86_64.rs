//! ELF x86_64 Architecture and Entry Point Synthesis.

use crate::relocation::{Relocation, RelocationKind};
use crate::symbol::{Symbol, SymbolBinding, SymbolType};

/// Synthesize standard Linux ELF x86_64 startup code (`_start` -> `main` -> `sys_exit`).
///
/// System V AMD64 ABI & Linux Kernel Execution Requirements:
/// - Clear `%rbp` (`xor ebp, ebp`) so debuggers and unwinders know this is the outermost frame.
/// - Read `argc` from `[rsp]`, pass in `%rdi`.
/// - Compute `argv` as `rsp + 8`, pass in `%rsi`.
/// - Compute `envp` as `argv + argc*8 + 8`, pass in `%rdx`.
/// - 16-byte align `%rsp` before the `call` instruction.
/// - Call user entry function (`main` / `__user_main`).
/// - Move return value from `%eax` into `%edi` for exit status.
/// - Issue Linux `sys_exit` (syscall 60: `mov eax, 60; syscall`).
pub fn synthesize_elf_x86_64_entry(
    program_entry_symbol: &str,
    file_index: usize,
) -> (Vec<u8>, Vec<Relocation>, Vec<Symbol>) {
    let mut code_bytes = Vec::new();
    let mut relocs = Vec::new();
    let mut symbols = Vec::new();

    let start_off = code_bytes.len() as u64;
    let start_bytes = vec![
        0x31, 0xed, // 0..1: xor ebp, ebp (clear frame pointer for GDB/unwinders)
        0x48, 0x8b, 0x3c, 0x24, // 2..5: mov rdi, [rsp] (argc)
        0x48, 0x8d, 0x74, 0x24, 0x08, // 6..10: lea rsi, [rsp + 8] (argv)
        0x48, 0x8d, 0x54, 0xfe, 0x08, // 11..15: lea rdx, [rsi + rdi*8 + 8] (envp)
        0x48, 0x83, 0xe4, 0xf0, // 16..19: and rsp, -16 (align stack before call)
        0xe8, 0x00, 0x00, 0x00, 0x00, // 20..24: call <program_entry> (disp32 @ 21)
        0x89, 0xc7, // 25..26: mov edi, eax (exit code)
        0xb8, 0x3c, 0x00, 0x00, 0x00, // 27..31: mov eax, 60 (sys_exit syscall)
        0x0f, 0x05, // 32..33: syscall
        0xf4, // 34: hlt (trap if syscall returns)
    ];
    let start_sz = start_bytes.len() as u64;
    code_bytes.extend_from_slice(&start_bytes);

    relocs.push(Relocation {
        offset: start_off + 21,
        symbol_name: program_entry_symbol.to_string(),
        symbol_index: None,
        file_index: None,
        kind: RelocationKind::PcRelative32,
        addend: -4,
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
        code_bytes.push(0x90);
    }

    (code_bytes, relocs, symbols)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_elf_x86_64_entry_synthesis() {
        let (bytes, relocs, symbols) = synthesize_elf_x86_64_entry("main", 0);

        assert!(!bytes.is_empty());
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "_start");
        assert_eq!(symbols[0].binding, SymbolBinding::Global);
        assert_eq!(symbols[0].sym_type, SymbolType::Function);

        // Verify call relocation points to main
        let main_reloc = relocs.iter().find(|r| r.offset == 21).unwrap();
        assert_eq!(main_reloc.symbol_name, "main");
        assert_eq!(main_reloc.kind, RelocationKind::PcRelative32);
        assert_eq!(main_reloc.addend, -4);

        // Verify syscall 60 (sys_exit) is present
        assert_eq!(bytes[27..32], [0xb8, 0x3c, 0x00, 0x00, 0x00]);
        assert_eq!(bytes[32..34], [0x0f, 0x05]);
    }
}
