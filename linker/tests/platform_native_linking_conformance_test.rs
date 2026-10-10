//! Phase 7 — Cross-Platform Native Linking Conformance Suite (Linux & macOS)
//!
//! Verifies:
//! 1. Platform-specific symbol routing (Linux ELF vs macOS Mach-O).
//! 2. External POSIX / libc classification (stdout, memory, strings, filesystem, errors).
//! 3. Preservation of Adesh runtime symbols as required archive definitions.
//! 4. Static Linux ELF executable layout and entry point synthesis.
//! 5. Structural conformance of emitted 64-bit Mach-O binaries and dylibs.

use adesh_linker::config::LinkConfig;
use adesh_linker::linker::Linker;
use adesh_linker::object::ObjectFile;
use adesh_linker::object::writer::ObjectWriter;
use adesh_linker::os_router::{OsApiRouter, SymbolRoute};
use adesh_linker::relocation::{Relocation, RelocationKind};
use adesh_linker::section::Section;
use adesh_linker::symbol::{Symbol, SymbolBinding, SymbolType};
use adesh_linker::target::Target;
use tempfile::tempdir;

#[test]
fn test_posix_libc_symbol_classification_linux_elf() {
    let target = Target::from_triple("x86_64-linux").unwrap();

    // Standard output
    for sym in ["puts", "write", "printf", "fputs"] {
        match OsApiRouter::classify(sym, &target) {
            SymbolRoute::DllImport { dll, name } => {
                assert_eq!(dll, "libc.so.6");
                assert_eq!(name, sym);
            }
            other => panic!("expected libc.so.6 import for {}, got {:?}", sym, other),
        }
    }

    // Memory management
    for sym in ["malloc", "free", "realloc", "calloc", "posix_memalign"] {
        match OsApiRouter::classify(sym, &target) {
            SymbolRoute::DllImport { dll, name } => {
                assert_eq!(dll, "libc.so.6");
                assert_eq!(name, sym);
            }
            other => panic!("expected libc.so.6 import for {}, got {:?}", sym, other),
        }
    }

    // String / Memory operations
    for sym in ["strlen", "strcpy", "strcmp", "memcpy", "memmove", "memset"] {
        match OsApiRouter::classify(sym, &target) {
            SymbolRoute::DllImport { dll, .. } => assert_eq!(dll, "libc.so.6"),
            SymbolRoute::Intrinsic => {} // Compiler intrinsics like memset are also valid
            other => panic!("unexpected route for {}: {:?}", sym, other),
        }
    }

    // Filesystem & Timestamps
    for sym in [
        "open",
        "read",
        "close",
        "stat",
        "fstat",
        "lstat",
        "utimensat",
    ] {
        match OsApiRouter::classify(sym, &target) {
            SymbolRoute::DllImport { dll, name } => {
                assert_eq!(dll, "libc.so.6");
                assert_eq!(name, sym);
            }
            other => panic!("expected libc.so.6 import for {}, got {:?}", sym, other),
        }
    }

    // Unwind & Threading
    match OsApiRouter::classify("_Unwind_Resume", &target) {
        SymbolRoute::DllImport { dll, .. } => assert_eq!(dll, "libgcc_s.so.1"),
        other => panic!(
            "expected libgcc_s.so.1 import for _Unwind_Resume, got {:?}",
            other
        ),
    }

    match OsApiRouter::classify("pthread_create", &target) {
        SymbolRoute::DllImport { dll, .. } => assert_eq!(dll, "libc.so.6"),
        other => panic!(
            "expected libc.so.6 import for pthread_create, got {:?}",
            other
        ),
    }

    // Process & Error
    match OsApiRouter::classify("__errno_location", &target) {
        SymbolRoute::DllImport { dll, .. } => assert_eq!(dll, "libc.so.6"),
        other => panic!(
            "expected libc.so.6 import for __errno_location, got {:?}",
            other
        ),
    }
}

#[test]
fn test_posix_libc_symbol_classification_macos_macho() {
    let target = Target::from_triple("x86_64-darwin").unwrap();

    // Standard output on macOS routes to libSystem with leading underscore
    for sym in ["puts", "write", "printf"] {
        match OsApiRouter::classify(sym, &target) {
            SymbolRoute::DllImport { dll, name } => {
                assert_eq!(dll, "/usr/lib/libSystem.B.dylib");
                assert_eq!(name, format!("_{}", sym));
            }
            other => panic!("expected libSystem import for {}, got {:?}", sym, other),
        }
    }

    // Memory management
    for sym in ["malloc", "free", "realloc"] {
        match OsApiRouter::classify(sym, &target) {
            SymbolRoute::DllImport { dll, name } => {
                assert_eq!(dll, "/usr/lib/libSystem.B.dylib");
                assert_eq!(name, format!("_{}", sym));
            }
            other => panic!("expected libSystem import for {}, got {:?}", sym, other),
        }
    }

    // macOS error handling
    match OsApiRouter::classify("__error", &target) {
        SymbolRoute::DllImport { dll, name } => {
            assert_eq!(dll, "/usr/lib/libSystem.B.dylib");
            assert_eq!(name, "__error");
        }
        other => panic!("expected libSystem import for __error, got {:?}", other),
    }
}

#[test]
fn test_runtime_symbols_are_never_routed_as_dynamic_libc_imports() {
    let linux = Target::from_triple("x86_64-linux").unwrap();
    let macos = Target::from_triple("x86_64-darwin").unwrap();

    let runtime_symbols = [
        "aot_alloc",
        "aot_free",
        "aot_print",
        "aot_println",
        "aot_runtime_init",
        "aot_vec_new",
        "aot_string_concat",
        "adesh_entry",
        "adesh_panic",
    ];

    for sym in runtime_symbols {
        // Must never be silently routed as dynamic C library imports
        if let SymbolRoute::DllImport { dll, .. } = OsApiRouter::classify(sym, &linux) {
            panic!("{} on Linux was routed as dynamic import from {}", sym, dll);
        }
        if let SymbolRoute::DllImport { dll, .. } = OsApiRouter::classify(sym, &macos) {
            panic!("{} on macOS was routed as dynamic import from {}", sym, dll);
        }
    }
}

#[test]
fn test_static_linux_elf_executable_generation_and_headers() {
    let dir = tempdir().unwrap();
    let out_elf = dir.path().join("test_static_elf");

    let target = Target::from_triple("x86_64-linux").unwrap();
    let obj_path = dir.path().join("main.o");
    let mut obj = ObjectFile::new(obj_path.clone(), target.clone(), 0);

    let mut code_sec = Section::new_code(".text", vec![0x48, 0x31, 0xc0, 0xc3], 16);
    let mut reloc = Relocation::new(0, "main", RelocationKind::PcRelative32, -4);
    reloc.symbol_index = Some(1);
    reloc.file_index = Some(0);
    code_sec.relocations.push(reloc);
    obj.add_section(code_sec);

    obj.add_symbol(Symbol::new_defined(
        "main",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        4,
        0,
    ));

    ObjectWriter::write_to_file(&obj, &obj_path).unwrap();

    let mut link_config = LinkConfig::new(out_elf.clone(), target);
    link_config.shared = false;

    let res = Linker::link(&[obj_path], link_config);
    assert!(res.is_ok(), "linking static Linux ELF failed: {:?}", res);

    // Verify binary exists and begins with valid ELF64 header
    let bytes = std::fs::read(&out_elf).expect("failed to read generated ELF");
    assert!(bytes.len() >= 64, "ELF header too small");
    assert_eq!(&bytes[0..4], b"\x7fELF", "invalid ELF magic");
    assert_eq!(bytes[4], 2, "must be 64-bit ELF (ELFCLASS64)");
    assert_eq!(bytes[5], 1, "must be little-endian (ELFDATA2LSB)");
    assert!(
        bytes[7] == 0 || bytes[7] == 3,
        "ELF OSABI must be SYSV (0) or GNU/Linux (3), got {}",
        bytes[7]
    );

    let e_type = u16::from_le_bytes(bytes[16..18].try_into().unwrap());
    assert_eq!(e_type, 2, "e_type must be ET_EXEC (static executable)");

    let e_machine = u16::from_le_bytes(bytes[18..20].try_into().unwrap());
    assert_eq!(e_machine, 0x3e, "e_machine must be AMD x86-64 (0x3e)");
}

#[test]
fn test_macho_64bit_executable_structural_conformance() {
    let dir = tempdir().unwrap();
    let out_macho = dir.path().join("test_macho");

    let target = Target::from_triple("x86_64-darwin").unwrap();
    let obj_path = dir.path().join("main.o");
    let mut obj = ObjectFile::new(obj_path.clone(), target.clone(), 0);

    obj.add_section(Section::new_code(".text", vec![0x48, 0x31, 0xc0, 0xc3], 16));
    obj.add_symbol(Symbol::new_defined(
        "main",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        4,
        0,
    ));

    ObjectWriter::write_to_file(&obj, &obj_path).unwrap();

    let link_config = LinkConfig::new(out_macho.clone(), target);
    let res = Linker::link(&[obj_path], link_config);
    assert!(res.is_ok(), "linking Mach-O failed: {:?}", res);

    let bytes = std::fs::read(&out_macho).expect("failed to read generated Mach-O");
    assert!(bytes.len() >= 32, "Mach-O header too small");

    // 0xFEEDFACF (64-bit Mach-O magic in little endian)
    let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    assert_eq!(magic, 0xfeedfacf, "must have MH_MAGIC_64");

    let filetype = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    assert_eq!(filetype, 2, "filetype must be MH_EXECUTE (2)");
}
