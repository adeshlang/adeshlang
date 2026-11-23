//! ELF i686 architecture and entry point synthesis.

use crate::relocation::{Relocation, RelocationKind};
use crate::symbol::{Symbol, SymbolBinding, SymbolType, SymbolVisibility};

/// Synthesize a minimal Linux ELF i386 startup stub: `_start` -> `main` -> `sys_exit`.
///
/// IA-32 System V ABI and Linux kernel execution requirements:
/// - Clear EBP for outermost-frame recognition.
/// - Read argc/argv/envp from the initial process stack.
/// - Align ESP to 16 bytes and pass argc/argv/envp on the stack.
/// - Call the program entry through `PcRelative32`.
/// - Pass main's EAX return value to Linux `sys_exit` in EBX.
pub fn synthesize_elf_i686_entry(
    program_entry_symbol: &str,
    file_index: usize,
) -> (Vec<u8>, Vec<Relocation>, Vec<Symbol>) {
    let code_bytes = vec![
        0x31, 0xed, // xor ebp, ebp
        0x8b, 0x4c, 0x24, 0x08, // mov ecx, [esp+8] (envp)
        0x8b, 0x54, 0x24, 0x04, // mov edx, [esp+4] (argv)
        0x8b, 0x04, 0x24, // mov eax, [esp] (argc)
        0x83, 0xe4, 0xf0, // and esp, -16
        0x83, 0xec, 0x10, // sub esp, 16
        0x89, 0x04, 0x24, // mov [esp], eax
        0x89, 0x54, 0x24, 0x04, // mov [esp+4], edx
        0x89, 0x4c, 0x24, 0x08, // mov [esp+8], ecx
        0xe8, 0x00, 0x00, 0x00, 0x00, // call <program_entry> (disp32 @ 31)
        0x83, 0xc4, 0x10, // add esp, 16
        0x89, 0xc3, // mov ebx, eax
        0xb8, 0x01, 0x00, 0x00, 0x00, // mov eax, 1 (sys_exit)
        0xcd, 0x80, // int 0x80
        0xf4, // hlt
    ];

    let relocs = vec![Relocation {
        offset: 31,
        symbol_name: program_entry_symbol.to_string(),
        symbol_index: None,
        file_index: None,
        kind: RelocationKind::PcRelative32,
        addend: -4,
    }];

    let symbols = vec![Symbol {
        name: "_start".to_string(),
        binding: SymbolBinding::Global,
        visibility: SymbolVisibility::Default,
        sym_type: SymbolType::Function,
        section_index: Some(0),
        value: 0,
        size: code_bytes.len() as u64,
        is_defined: true,
        is_imported: false,
        is_exported: false,
        file_index: Some(file_index),
        alias_of: None,
        comdat_group: None,
        version: None,
    }];

    (code_bytes, relocs, symbols)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_stub_has_pc32_call_relocation_at_disp32() {
        let (bytes, relocs, symbols) = synthesize_elf_i686_entry("main", 7);
        assert_eq!(bytes.len(), 48);
        assert_eq!(bytes[30], 0xE8);
        assert_eq!(bytes[45], 0xCD);
        assert_eq!(bytes[46], 0x80);
        assert_eq!(relocs.len(), 1);
        assert_eq!(relocs[0].offset, 31);
        assert_eq!(relocs[0].kind, RelocationKind::PcRelative32);
        assert_eq!(relocs[0].addend, -4);
        assert_eq!(symbols[0].name, "_start");
        assert_eq!(symbols[0].file_index, Some(7));
    }
}
