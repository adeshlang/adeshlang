use adesh_linker::pe::import::ImportSymbol;
use adesh_linker::pe::writer::PeWriter;
use adesh_linker::section::{MergedSection, SectionKind, flags};
use adesh_linker::symbol::{Symbol, SymbolBinding, SymbolType};
use adesh_linker::target::Target;
use std::path::Path;

fn main() {
    let target = Target::x86_64_windows();
    let exe_path = Path::new("target/adesh_native_app.exe");

    // Native x86_64 machine code for Windows:
    // When BaseThreadInitThunk starts the thread, RSP is aligned.
    // sub rsp, 40      ; 32-byte shadow space + 8-byte alignment (0x48, 0x83, 0xec, 0x28)
    // xor eax, eax     ; return code 0 (0x31, 0xc0)
    // add rsp, 40      ; restore stack (0x48, 0x83, 0xc4, 0x28)
    // ret              ; return to BaseThreadInitThunk which calls ExitThread(0) (0xc3)
    let code_bytes = vec![
        0x48, 0x83, 0xec, 0x28, // sub rsp, 40
        0x31, 0xc0, // xor eax, eax (exit code 0)
        0x48, 0x83, 0xc4, 0x28, // add rsp, 40
        0xc3, // ret
    ];

    let mut merged_sec = MergedSection::new(
        ".text",
        SectionKind::Text,
        flags::READ | flags::EXEC | flags::ALLOC,
        16,
    );
    merged_sec.virtual_address = 0x140001000;
    merged_sec.data = code_bytes;
    merged_sec.size = 10;

    let sym = Symbol::new_defined(
        "main",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0x140001000,
        10,
        0,
    );

    let imports = vec![ImportSymbol {
        dll_name: "kernel32.dll".to_string(),
        symbol_name: "ExitProcess".to_string(),
        ordinal: None,
    }];

    std::fs::create_dir_all("target").unwrap();
    PeWriter::write_executable(
        exe_path,
        &target,
        0x140001000,
        &[merged_sec],
        &[sym],
        &imports,
    )
    .expect("Failed to write Windows PE executable");

    println!(
        "  ✓ Successfully generated native Windows executable: {}",
        exe_path.display()
    );
}
