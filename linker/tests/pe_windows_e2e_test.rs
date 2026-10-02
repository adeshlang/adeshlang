#[cfg(target_os = "windows")]
use adesh_linker::pe::import::ImportSymbol;
#[cfg(target_os = "windows")]
use adesh_linker::pe::writer::PeWriter;
#[cfg(target_os = "windows")]
use adesh_linker::section::{MergedSection, SectionKind, flags};
#[cfg(target_os = "windows")]
use adesh_linker::symbol::{Symbol, SymbolBinding, SymbolType};
#[cfg(target_os = "windows")]
use adesh_linker::target::Target;
#[cfg(target_os = "windows")]
use std::process::Command;
#[cfg(target_os = "windows")]
use tempfile::tempdir;

#[test]
#[cfg(target_os = "windows")]
fn test_windows_pe_native_execution_zero_deps() {
    let dir = tempdir().expect("Failed to create tempdir");
    let target = Target::x86_64_windows();
    let exe_path = dir.path().join("zero_dep_hello.exe");

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

    PeWriter::write_executable(
        &exe_path,
        &target,
        0x140001000,
        &[merged_sec],
        &[sym],
        &imports,
    )
    .expect("Failed to write Windows PE executable");

    assert!(exe_path.exists());

    // Execute the generated .exe directly on Windows!
    let output = Command::new(&exe_path)
        .output()
        .expect("Failed to execute generated Windows PE binary");

    println!("Process exit code: {:?}", output.status.code());
    assert_eq!(output.status.code(), Some(0));
}

#[test]
#[cfg(target_os = "windows")]
fn test_windows_pe_puts_execution() {
    let dir = tempdir().expect("Failed to create tempdir");
    let target = Target::x86_64_windows();
    let exe_path = dir.path().join("pe_puts_test.exe");

    // "Hello from Adesh Native PE!\0"
    let msg = b"Hello from Adesh Native PE!\0";

    // x86_64 machine code:
    // lea rcx, [rip + msg_offset]
    // sub rsp, 40
    // call puts (via import thunk)
    // xor eax, eax
    // add rsp, 40
    // ret
    let mut code = vec![
        0x48, 0x8D, 0x0D, 0x00, 0x00, 0x00,
        0x00, // lea rcx, [rip + disp32] (offset 0..7, disp at 3..7)
        0x48, 0x83, 0xEC, 0x28, // sub rsp, 40
        0xE8, 0x00, 0x00, 0x00, 0x00, // call puts (offset 11..16, disp at 12..16)
        0x31, 0xC0, // xor eax, eax
        0x48, 0x83, 0xC4, 0x28, // add rsp, 40
        0xC3, // ret
    ];

    let mut text_sec = MergedSection::new(
        ".text",
        SectionKind::Text,
        flags::READ | flags::EXEC | flags::ALLOC,
        16,
    );
    text_sec.virtual_address = 0x140001000;

    let mut rdata_sec = MergedSection::new(
        ".rdata",
        SectionKind::Rodata,
        flags::READ | flags::ALLOC,
        16,
    );
    rdata_sec.virtual_address = 0x140002000;
    rdata_sec.data = msg.to_vec();
    rdata_sec.size = msg.len() as u64;

    // Fixup disp for lea rcx, [rip + disp]
    // RIP after lea instruction (7 bytes) is text_sec.virtual_address + 7 = 0x140001007
    // msg address is rdata_sec.virtual_address = 0x140002000
    let msg_disp = (0x140002000i64 - 0x140001007i64) as i32;
    code[3..7].copy_from_slice(&msg_disp.to_le_bytes());

    let imports = vec![
        ImportSymbol {
            dll_name: "msvcrt.dll".to_string(),
            symbol_name: "puts".to_string(),
            ordinal: None,
        },
        ImportSymbol {
            dll_name: "KERNEL32.dll".to_string(),
            symbol_name: "ExitProcess".to_string(),
            ordinal: None,
        },
    ];

    let idata_rva = 0x3000u32;
    let imp_res =
        adesh_linker::pe::import::build_import_table(&imports, target.image_base, idata_rva);
    let puts_iat_rva = imp_res.symbol_iat_rvas.get("puts").copied().unwrap();
    let puts_iat_va = target.image_base + (puts_iat_rva as u64);

    // Append jump thunk for puts at end of text_sec:
    // jmp qword ptr [rip + disp32] (FF 25 <disp32>)
    let thunk_off = code.len() as u64;
    let thunk_va = text_sec.virtual_address + thunk_off;
    let thunk_disp = (puts_iat_va as i64 - (thunk_va as i64 + 6)) as i32;
    code.extend_from_slice(&[
        0xFF,
        0x25,
        (thunk_disp & 0xFF) as u8,
        ((thunk_disp >> 8) & 0xFF) as u8,
        ((thunk_disp >> 16) & 0xFF) as u8,
        ((thunk_disp >> 24) & 0xFF) as u8,
        0x90,
        0x90,
    ]);

    // Fixup call puts displacement:
    // RIP after call puts (at offset 16) is text_sec.virtual_address + 16
    let call_disp = (thunk_va as i64 - (text_sec.virtual_address as i64 + 16)) as i32;
    code[12..16].copy_from_slice(&call_disp.to_le_bytes());

    text_sec.data = code;
    text_sec.size = text_sec.data.len() as u64;

    let mut idata_sec = MergedSection::new(
        ".idata",
        SectionKind::Data,
        flags::READ | flags::WRITE | flags::ALLOC,
        8,
    );
    idata_sec.virtual_address = target.image_base + (idata_rva as u64);
    idata_sec.data = imp_res.data;
    idata_sec.size = idata_sec.data.len() as u64;

    let sym = Symbol::new_defined(
        "main",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0x140001000,
        text_sec.size,
        0,
    );

    PeWriter::write_executable(
        &exe_path,
        &target,
        0x140001000,
        &[text_sec, rdata_sec, idata_sec],
        &[sym],
        &imports,
    )
    .expect("Failed to write PE executable with puts");

    assert!(exe_path.exists());

    let output = Command::new(&exe_path)
        .output()
        .expect("Failed to execute generated Windows PE binary");

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    println!("Output stdout: {}", stdout_str);
    println!("Output exit code: {:?}", output.status.code());

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout_str.contains("Hello from Adesh Native PE!"));
}
