//! PE x86_64 Architecture and Entry Point Synthesis.

use crate::relocation::{Relocation, RelocationKind};
use crate::symbol::{Symbol, SymbolBinding, SymbolType};

/// Synthesize Windows x86_64 native startup code (`mainCRTStartup` -> `__adesh_windows_start`).
///
/// ABI Requirements for Windows x86_64:
/// - 16-byte stack alignment before calls.
/// - 32-byte shadow space reservation.
/// - Nonvolatile register preservation (`RBP`).
/// - Exit code propagation to `ExitProcess`.
pub fn synthesize_windows_x86_64_entry(
    program_entry_symbol: &str,
    file_index: usize,
) -> (Vec<u8>, Vec<Relocation>, Vec<Symbol>) {
    let mut code_bytes = Vec::new();
    let mut relocs = Vec::new();
    let mut symbols = Vec::new();

    // ------------------------------------------------------------------------
    // 1. mainCRTStartup: PE Entry Point (Offset 0)
    // ------------------------------------------------------------------------
    // sub rsp, 40               ; 48 83 ec 28  (32-byte shadow space + 8-byte alignment)
    // call __adesh_windows_start ; e8 [disp32]
    // add rsp, 40               ; 48 83 c4 28
    // ret                       ; c3
    let main_crt_start_off = code_bytes.len() as u64;
    let main_crt_bytes = vec![
        0x48, 0x83, 0xec, 0x28, // 0..3: sub rsp, 40
        0xe8, 0x00, 0x00, 0x00, 0x00, // 4..8: call __adesh_windows_start (disp32 @ 5)
        0x48, 0x83, 0xc4, 0x28, // 9..12: add rsp, 40
        0xc3, // 13: ret
    ];
    let main_crt_sz = main_crt_bytes.len() as u64;
    code_bytes.extend_from_slice(&main_crt_bytes);

    relocs.push(Relocation {
        offset: main_crt_start_off + 5,
        symbol_name: "__adesh_windows_start".to_string(),
        symbol_index: None,
        file_index: None,
        kind: RelocationKind::PcRelative32,
        addend: -4,
    });

    symbols.push(Symbol {
        name: "mainCRTStartup".to_string(),
        binding: SymbolBinding::Global,
        visibility: crate::symbol::SymbolVisibility::Default,
        sym_type: SymbolType::Function,
        section_index: Some(0),
        value: main_crt_start_off,
        size: main_crt_sz,
        is_defined: true,
        is_imported: false,
        is_exported: false,
        file_index: Some(file_index),
        alias_of: None,
        comdat_group: None,
        version: None,
    });

    // 16-byte align next function
    while (code_bytes.len() & 15) != 0 {
        code_bytes.push(0x90);
    }

    // ------------------------------------------------------------------------
    // 2. __adesh_windows_start: Canonical Adesh Windows Native Startup (Offset 16)
    // ------------------------------------------------------------------------
    // push rbp                  ; 55
    // mov rbp, rsp              ; 48 89 e5
    // sub rsp, 48               ; 48 83 ec 30 (16-byte stack alignment + shadow space)
    // xor ecx, ecx              ; 31 c9 (argc = 0)
    // xor edx, edx              ; 31 d2 (argv = NULL)
    // call <program_entry>      ; e8 [disp32]
    // mov ecx, eax              ; 89 c1 (exit code into ECX)
    // call [__imp_ExitProcess]  ; ff 15 [disp32]
    // mov rsp, rbp              ; 48 89 ec
    // pop rbp                   ; 5d
    // ret                       ; c3
    let start_off = code_bytes.len() as u64;
    let start_bytes = vec![
        0x55, // 0: push rbp
        0x48, 0x89, 0xe5, // 1..3: mov rbp, rsp
        0x48, 0x83, 0xec, 0x30, // 4..7: sub rsp, 48
        0x31, 0xc9, // 8..9: xor ecx, ecx
        0x31, 0xd2, // 10..11: xor edx, edx
        0xe8, 0x00, 0x00, 0x00, 0x00, // 12..16: call <program_entry> (disp32 @ 13)
        0x89, 0xc1, // 17..18: mov ecx, eax
        0xff, 0x15, 0x00, 0x00, 0x00, 0x00, // 19..24: call [__imp_ExitProcess] (disp32 @ 21)
        0x48, 0x89, 0xec, // 25..27: mov rsp, rbp
        0x5d, // 28: pop rbp
        0xc3, // 29: ret
    ];
    let start_sz = start_bytes.len() as u64;
    code_bytes.extend_from_slice(&start_bytes);

    relocs.push(Relocation {
        offset: start_off + 13,
        symbol_name: program_entry_symbol.to_string(),
        symbol_index: None,
        file_index: None,
        kind: RelocationKind::PcRelative32,
        addend: -4,
    });

    relocs.push(Relocation {
        offset: start_off + 21,
        symbol_name: "__imp_ExitProcess".to_string(),
        symbol_index: None,
        file_index: None,
        kind: RelocationKind::PcRelative32,
        addend: -4,
    });

    symbols.push(Symbol {
        name: "__adesh_windows_start".to_string(),
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

    // Pad to 16-byte boundary
    while (code_bytes.len() & 15) != 0 {
        code_bytes.push(0x90);
    }

    (code_bytes, relocs, symbols)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_windows_x86_64_entry_synthesis() {
        let (bytes, relocs, symbols) = synthesize_windows_x86_64_entry("main", 0);

        assert!(!bytes.is_empty());
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "mainCRTStartup");
        assert_eq!(symbols[1].name, "__adesh_windows_start");

        // Verify mainCRTStartup calls __adesh_windows_start
        let crt_reloc = relocs.iter().find(|r| r.offset == 5).unwrap();
        assert_eq!(crt_reloc.symbol_name, "__adesh_windows_start");
        assert_eq!(crt_reloc.kind, RelocationKind::PcRelative32);

        // Verify __adesh_windows_start calls main and __imp_ExitProcess
        let main_reloc = relocs.iter().find(|r| r.offset == 16 + 13).unwrap();
        assert_eq!(main_reloc.symbol_name, "main");
        assert_eq!(main_reloc.kind, RelocationKind::PcRelative32);

        let exit_reloc = relocs.iter().find(|r| r.offset == 16 + 21).unwrap();
        assert_eq!(exit_reloc.symbol_name, "__imp_ExitProcess");
        assert_eq!(exit_reloc.kind, RelocationKind::PcRelative32);
    }
}
