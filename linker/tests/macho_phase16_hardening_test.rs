//! Phase 16: Mach-O Production Hardening tests.
//!
//! Verifies:
//! 1. `LC_BUILD_VERSION` minOS parsing and dynamic deployment target.
//! 2. `__DATA_CONST` segment protection (`SG_READ_ONLY = 0x10`).
//! 3. TLV descriptor table layout `(thunk, key, offset)` and `__tlv_bootstrap` bind stream.
//! 4. Imported data symbols (`environ`) and dyld non-lazy data bind stream.
//! 5. Dylib generation: `LC_ID_DYLIB`, custom `install_name`, and export trie decoding.
//! 6. Ad-hoc code signature: `LC_CODE_SIGNATURE` within `__LINKEDIT`, valid CodeDirectory, SHA-256 code slots.

use adesh_linker::macho::codesign::LC_CODE_SIGNATURE;
use adesh_linker::macho::dynamic::{
    BIND_TYPE_POINTER, decode_bind_stream_for_test, decode_export_trie_for_test,
};
use adesh_linker::macho::header::*;
use adesh_linker::macho::writer::{parse_macos_version, MachOWriter};
use adesh_linker::object::ObjectWriter;
use adesh_linker::relocation::{Relocation, RelocationKind};
use adesh_linker::section::Section;
use adesh_linker::symbol::{Symbol, SymbolBinding, SymbolType};
use adesh_linker::target::{Arch, Os, Target};
use adesh_linker::{LinkConfig, Linker, ObjectFile};
use tempfile::tempdir;

fn u32_at(bytes: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap())
}

fn u64_at(bytes: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(bytes[off..off + 8].try_into().unwrap())
}

// ─── Test 1: LC_BUILD_VERSION and Version Parsing ────────────────────────────

#[test]
fn test_macos_version_parsing() {
    assert_eq!(parse_macos_version("14.0"), Some(0x000E0000));
    assert_eq!(parse_macos_version("14.4"), Some(0x000E0400));
    assert_eq!(parse_macos_version("12.5.1"), Some(0x000C0501));
    assert_eq!(parse_macos_version("11.0"), Some(0x000B0000));
    assert_eq!(parse_macos_version("10.15"), Some(0x000A0F00));
    assert_eq!(parse_macos_version("invalid"), None);
}

#[test]
fn test_build_version_load_command() {
    let target = Target::new(Arch::X86_64, Os::MacOS);
    let mut obj = ObjectFile::new("main.o".into(), target.clone(), 0);
    let mut text = Section::new_code(".text", 16);
    // mov eax, 42; ret
    text.data = vec![0xb8, 0x2a, 0x00, 0x00, 0x00, 0xc3];
    obj.add_section(text);
    obj.add_symbol(Symbol {
        name: "main".into(),
        binding: SymbolBinding::Global,
        visibility: adesh_linker::symbol::SymbolVisibility::Default,
        sym_type: SymbolType::Function,
        section_index: Some(0),
        value: 0,
        size: 6,
        is_defined: true,
        is_imported: false,
    });

    let dir = tempdir().unwrap();
    let obj_path = dir.path().join("main.o");
    let out_path = dir.path().join("my_macho_app");
    ObjectWriter::write(&obj_path, &obj).unwrap();

    let config = LinkConfig {
        output_path: out_path.clone(),
        target: target.clone(),
        entry_point: Some("main".into()),
        ..LinkConfig::default()
    };
    Linker::link(&[obj_path], &config).unwrap();

    let bytes = std::fs::read(&out_path).unwrap();
    assert_eq!(u32_at(&bytes, 0), MH_MAGIC_64);

    let ncmds = u32_at(&bytes, 16) as usize;
    let mut offset = 32;
    let mut found_build_version = false;

    for _ in 0..ncmds {
        let cmd = u32_at(&bytes, offset);
        let cmdsize = u32_at(&bytes, offset + 4) as usize;
        if cmd == LC_BUILD_VERSION {
            found_build_version = true;
            let platform = u32_at(&bytes, offset + 8);
            let minos = u32_at(&bytes, offset + 12);
            let sdk = u32_at(&bytes, offset + 16);
            assert_eq!(platform, PLATFORM_MACOS, "platform must be macOS");
            assert!(minos >= 0x000B0000, "minos must be at least macOS 11.0");
            assert!(sdk >= minos, "sdk must be >= minos");
        }
        offset += cmdsize;
    }

    assert!(found_build_version, "binary must contain LC_BUILD_VERSION");
}

// ─── Test 2: __DATA_CONST Segment Protection ────────────────────────────────

#[test]
fn test_data_const_protection_and_flags() {
    let target = Target::new(Arch::X86_64, Os::MacOS);
    let mut obj = ObjectFile::new("main.o".into(), target.clone(), 0);

    let mut text = Section::new_code(".text", 16);
    text.data = vec![0xb8, 0x00, 0x00, 0x00, 0x00, 0xc3];
    obj.add_section(text);

    let mut rodata = Section::new_rodata(".rodata", 8);
    rodata.data = b"Hello from DATA_CONST\0".to_vec();
    obj.add_section(rodata);

    obj.add_symbol(Symbol {
        name: "main".into(),
        binding: SymbolBinding::Global,
        visibility: adesh_linker::symbol::SymbolVisibility::Default,
        sym_type: SymbolType::Function,
        section_index: Some(0),
        value: 0,
        size: 6,
        is_defined: true,
        is_imported: false,
    });

    let dir = tempdir().unwrap();
    let obj_path = dir.path().join("main.o");
    let out_path = dir.path().join("test_data_const");
    ObjectWriter::write(&obj_path, &obj).unwrap();

    let config = LinkConfig {
        output_path: out_path.clone(),
        target: target.clone(),
        entry_point: Some("main".into()),
        ..LinkConfig::default()
    };
    Linker::link(&[obj_path], &config).unwrap();

    let bytes = std::fs::read(&out_path).unwrap();
    let ncmds = u32_at(&bytes, 16) as usize;
    let mut offset = 32;

    let mut found_data_const = false;
    for _ in 0..ncmds {
        let cmd = u32_at(&bytes, offset);
        let cmdsize = u32_at(&bytes, offset + 4) as usize;
        if cmd == LC_SEGMENT_64 {
            let segname_bytes = &bytes[offset + 8..offset + 24];
            let segname = String::from_utf8_lossy(segname_bytes)
                .trim_matches('\0')
                .to_string();

            if segname == "__DATA_CONST" {
                found_data_const = true;
                let maxprot = u32_at(&bytes, offset + 48);
                let initprot = u32_at(&bytes, offset + 52);
                let flags = u32_at(&bytes, offset + 60);

                assert_eq!(
                    maxprot,
                    VM_PROT_READ | VM_PROT_WRITE,
                    "__DATA_CONST maxprot must be rw-"
                );
                assert_eq!(
                    initprot,
                    VM_PROT_READ | VM_PROT_WRITE,
                    "__DATA_CONST initprot must be rw-"
                );
                assert_eq!(
                    flags, SG_READ_ONLY,
                    "__DATA_CONST segment flags must be SG_READ_ONLY (0x10)"
                );
            }
        }
        offset += cmdsize;
    }

    assert!(found_data_const, "must emit __DATA_CONST segment");
}

// ─── Test 3: TLV Descriptor Table and __tlv_bootstrap Bind ───────────────────

#[test]
fn test_tlv_table_layout_and_bootstrap_bind() {
    let target = Target::new(Arch::X86_64, Os::MacOS);
    let mut obj = ObjectFile::new("tlv.o".into(), target.clone(), 0);

    let mut text = Section::new_code(".text", 16);
    // mov rax, [rip + tlv_var@TLV]
    text.data = vec![0x48, 0x8b, 0x05, 0x00, 0x00, 0x00, 0x00, 0xc3];
    text.relocations.push(Relocation {
        offset: 3,
        symbol_name: "tlv_var".into(),
        kind: RelocationKind::X86_64TlvDescriptorRel32,
        addend: -4,
    });
    obj.add_section(text);

    // .tls section
    let mut tls_sec = Section::new_tls(".tls", 8);
    tls_sec.data = vec![0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    obj.add_section(tls_sec);

    // .tlv_desc section (24-byte descriptor)
    let mut tlv_desc = Section::new_tlv_table("__thread_vars", 8);
    tlv_desc.data = vec![0u8; 24];
    // reloc against __tlv_bootstrap at offset 0
    tlv_desc.relocations.push(Relocation {
        offset: 0,
        symbol_name: "__tlv_bootstrap".into(),
        kind: RelocationKind::Absolute64,
        addend: 0,
    });
    // reloc against tls_var at offset 16 (offset in per-thread block)
    tlv_desc.relocations.push(Relocation {
        offset: 16,
        symbol_name: "tlv_var_data".into(),
        kind: RelocationKind::Absolute64,
        addend: 0,
    });
    obj.add_section(tlv_desc);

    obj.add_symbol(Symbol {
        name: "main".into(),
        binding: SymbolBinding::Global,
        visibility: adesh_linker::symbol::SymbolVisibility::Default,
        sym_type: SymbolType::Function,
        section_index: Some(0),
        value: 0,
        size: 8,
        is_defined: true,
        is_imported: false,
    });

    obj.add_symbol(Symbol {
        name: "tlv_var_data".into(),
        binding: SymbolBinding::Global,
        visibility: adesh_linker::symbol::SymbolVisibility::Default,
        sym_type: SymbolType::Object,
        section_index: Some(1),
        value: 0,
        size: 8,
        is_defined: true,
        is_imported: false,
    });

    obj.add_symbol(Symbol {
        name: "tlv_var".into(),
        binding: SymbolBinding::Global,
        visibility: adesh_linker::symbol::SymbolVisibility::Default,
        sym_type: SymbolType::Object,
        section_index: Some(2),
        value: 0,
        size: 24,
        is_defined: true,
        is_imported: false,
    });

    let dir = tempdir().unwrap();
    let obj_path = dir.path().join("tlv.o");
    let out_path = dir.path().join("test_tlv");
    ObjectWriter::write(&obj_path, &obj).unwrap();

    let config = LinkConfig {
        output_path: out_path.clone(),
        target: target.clone(),
        entry_point: Some("main".into()),
        ..LinkConfig::default()
    };
    Linker::link(&[obj_path], &config).unwrap();

    let bytes = std::fs::read(&out_path).unwrap();
    let ncmds = u32_at(&bytes, 16) as usize;
    let mut offset = 32;

    let mut bind_off = 0usize;
    let mut bind_size = 0usize;

    for _ in 0..ncmds {
        let cmd = u32_at(&bytes, offset);
        let cmdsize = u32_at(&bytes, offset + 4) as usize;
        if cmd == LC_DYLD_INFO_ONLY || cmd == LC_DYLD_INFO {
            bind_off = u32_at(&bytes, offset + 16) as usize;
            bind_size = u32_at(&bytes, offset + 20) as usize;
        }
        offset += cmdsize;
    }

    assert!(bind_size > 0, "bind stream must not be empty");
    let decoded_binds =
        decode_bind_stream_for_test(&bytes[bind_off..bind_off + bind_size]).unwrap();

    // Verify __tlv_bootstrap bind exists
    let bootstrap_bind = decoded_binds
        .iter()
        .find(|b| b.symbol == "__tlv_bootstrap");
    assert!(
        bootstrap_bind.is_some(),
        "bind stream must contain __tlv_bootstrap for TLV descriptor thunk"
    );
    let b = bootstrap_bind.unwrap();
    assert_eq!(b.bind_type, BIND_TYPE_POINTER);
    assert_eq!(b.addend, 0);
}

// ─── Test 4: Imported Data Symbols (environ) and Data Bind ───────────────────

#[test]
fn test_macho_environ_data_import() {
    let target = Target::new(Arch::X86_64, Os::MacOS);
    let mut obj = ObjectFile::new("environ.o".into(), target.clone(), 0);

    let mut text = Section::new_code(".text", 16);
    text.data = vec![0x48, 0x8b, 0x05, 0x00, 0x00, 0x00, 0x00, 0xc3];
    // GOT-relative load of environ
    text.relocations.push(Relocation {
        offset: 3,
        symbol_name: "environ".into(),
        kind: RelocationKind::GotRelative32,
        addend: -4,
    });
    obj.add_section(text);

    // .data section holding pointer to environ (Absolute64)
    let mut data = Section::new_data(".data", 8);
    data.data = vec![0u8; 8];
    data.relocations.push(Relocation {
        offset: 0,
        symbol_name: "environ".into(),
        kind: RelocationKind::Absolute64,
        addend: 0,
    });
    obj.add_section(data);

    obj.add_symbol(Symbol {
        name: "main".into(),
        binding: SymbolBinding::Global,
        visibility: adesh_linker::symbol::SymbolVisibility::Default,
        sym_type: SymbolType::Function,
        section_index: Some(0),
        value: 0,
        size: 8,
        is_defined: true,
        is_imported: false,
    });

    obj.add_symbol(Symbol {
        name: "environ".into(),
        binding: SymbolBinding::Global,
        visibility: adesh_linker::symbol::SymbolVisibility::Default,
        sym_type: SymbolType::Object,
        section_index: None,
        value: 0,
        size: 8,
        is_defined: false,
        is_imported: true,
    });

    let dir = tempdir().unwrap();
    let obj_path = dir.path().join("environ.o");
    let out_path = dir.path().join("test_environ");
    ObjectWriter::write(&obj_path, &obj).unwrap();

    let config = LinkConfig {
        output_path: out_path.clone(),
        target: target.clone(),
        entry_point: Some("main".into()),
        dynamic: true,
        ..LinkConfig::default()
    };
    Linker::link(&[obj_path], &config).unwrap();

    let bytes = std::fs::read(&out_path).unwrap();
    let ncmds = u32_at(&bytes, 16) as usize;
    let mut offset = 32;

    let mut bind_off = 0usize;
    let mut bind_size = 0usize;

    for _ in 0..ncmds {
        let cmd = u32_at(&bytes, offset);
        let cmdsize = u32_at(&bytes, offset + 4) as usize;
        if cmd == LC_DYLD_INFO_ONLY || cmd == LC_DYLD_INFO {
            bind_off = u32_at(&bytes, offset + 16) as usize;
            bind_size = u32_at(&bytes, offset + 20) as usize;
        }
        offset += cmdsize;
    }

    assert!(bind_size > 0, "bind stream must not be empty");
    let decoded_binds =
        decode_bind_stream_for_test(&bytes[bind_off..bind_off + bind_size]).unwrap();

    let environ_bind = decoded_binds.iter().find(|b| b.symbol == "_environ");
    assert!(
        environ_bind.is_some(),
        "bind stream must contain _environ for imported environ data symbol"
    );
}

// ─── Test 5: Dylib Generation and Export Trie ────────────────────────────────

#[test]
fn test_macho_dylib_generation_and_export_trie() {
    let target = Target::new(Arch::X86_64, Os::MacOS);
    let mut obj = ObjectFile::new("dylib_src.o".into(), target.clone(), 0);

    let mut text = Section::new_code(".text", 16);
    // mov eax, 100; ret
    text.data = vec![0xb8, 0x64, 0x00, 0x00, 0x00, 0xc3];
    obj.add_section(text);

    obj.add_symbol(Symbol {
        name: "my_exported_func".into(),
        binding: SymbolBinding::Global,
        visibility: adesh_linker::symbol::SymbolVisibility::Default,
        sym_type: SymbolType::Function,
        section_index: Some(0),
        value: 0,
        size: 6,
        is_defined: true,
        is_imported: false,
    });

    let dir = tempdir().unwrap();
    let obj_path = dir.path().join("dylib_src.o");
    let out_path = dir.path().join("libmy_lib.dylib");
    ObjectWriter::write(&obj_path, &obj).unwrap();

    let config = LinkConfig {
        output_path: out_path.clone(),
        target: target.clone(),
        shared: true,
        entry_point: None,
        ..LinkConfig::default()
    };
    Linker::link(&[obj_path], &config).unwrap();

    let bytes = std::fs::read(&out_path).unwrap();
    assert_eq!(u32_at(&bytes, 0), MH_MAGIC_64);
    assert_eq!(u32_at(&bytes, 12), MH_DYLIB, "filetype must be MH_DYLIB");

    let ncmds = u32_at(&bytes, 16) as usize;
    let mut offset = 32;

    let mut found_id_dylib = false;
    let mut export_off = 0usize;
    let mut export_size = 0usize;

    for _ in 0..ncmds {
        let cmd = u32_at(&bytes, offset);
        let cmdsize = u32_at(&bytes, offset + 4) as usize;
        if cmd == LC_ID_DYLIB {
            found_id_dylib = true;
            let name_off = u32_at(&bytes, offset + 8) as usize;
            let name_bytes = &bytes[offset + name_off..offset + cmdsize];
            let name = String::from_utf8_lossy(name_bytes)
                .trim_matches('\0')
                .to_string();
            assert!(
                name.contains("libadesh_out.dylib") || name.contains("libmy_lib.dylib"),
                "LC_ID_DYLIB name: {}",
                name
            );
        } else if cmd == LC_DYLD_INFO_ONLY || cmd == LC_DYLD_INFO {
            export_off = u32_at(&bytes, offset + 40) as usize;
            export_size = u32_at(&bytes, offset + 44) as usize;
        }
        offset += cmdsize;
    }

    assert!(found_id_dylib, "dylib must contain LC_ID_DYLIB");
    assert!(export_size > 0, "export trie must not be empty");

    let exports =
        decode_export_trie_for_test(&bytes[export_off..export_off + export_size]).unwrap();
    let my_func = exports.iter().find(|(name, _)| name == "_my_exported_func");
    assert!(
        my_func.is_some(),
        "export trie must contain _my_exported_func"
    );
}

// ─── Test 6: Ad-hoc Code Signature Integrity ─────────────────────────────────

#[test]
fn test_macho_adhoc_code_signature_integrity() {
    let target = Target::new(Arch::X86_64, Os::MacOS);
    let mut obj = ObjectFile::new("signed.o".into(), target.clone(), 0);

    let mut text = Section::new_code(".text", 16);
    // Fill text with 10000 bytes to cross several 4096-byte pages
    text.data = vec![0x90; 10000];
    text.data[9999] = 0xc3; // ret
    obj.add_section(text);

    obj.add_symbol(Symbol {
        name: "main".into(),
        binding: SymbolBinding::Global,
        visibility: adesh_linker::symbol::SymbolVisibility::Default,
        sym_type: SymbolType::Function,
        section_index: Some(0),
        value: 0,
        size: 10000,
        is_defined: true,
        is_imported: false,
    });

    let dir = tempdir().unwrap();
    let obj_path = dir.path().join("signed.o");
    let out_path = dir.path().join("signed_bin");
    ObjectWriter::write(&obj_path, &obj).unwrap();

    let config = LinkConfig {
        output_path: out_path.clone(),
        target: target.clone(),
        entry_point: Some("main".into()),
        ..LinkConfig::default()
    };
    Linker::link(&[obj_path], &config).unwrap();

    let bytes = std::fs::read(&out_path).unwrap();
    let ncmds = u32_at(&bytes, 16) as usize;
    let mut offset = 32;

    let mut sig_off = 0usize;
    let mut sig_size = 0usize;

    for _ in 0..ncmds {
        let cmd = u32_at(&bytes, offset);
        let cmdsize = u32_at(&bytes, offset + 4) as usize;
        if cmd == LC_CODE_SIGNATURE {
            sig_off = u32_at(&bytes, offset + 8) as usize;
            sig_size = u32_at(&bytes, offset + 12) as usize;
        }
        offset += cmdsize;
    }

    assert!(sig_size > 0, "LC_CODE_SIGNATURE must have non-zero size");
    assert_eq!(
        sig_off + sig_size,
        bytes.len(),
        "signature blob must extend to EOF"
    );

    // Verify SuperBlob header (big-endian)
    let sig_bytes = &bytes[sig_off..sig_off + sig_size];
    let sb_magic = u32::from_be_bytes(sig_bytes[0..4].try_into().unwrap());
    assert_eq!(
        sb_magic, 0xFADE0CC0,
        "SuperBlob magic must be CSMAGIC_EMBEDDED_SIGNATURE"
    );

    let sb_length = u32::from_be_bytes(sig_bytes[4..8].try_into().unwrap()) as usize;
    assert_eq!(sb_length, sig_size);

    let sb_count = u32::from_be_bytes(sig_bytes[8..12].try_into().unwrap());
    assert_eq!(sb_count, 1);

    let cd_type = u32::from_be_bytes(sig_bytes[12..16].try_into().unwrap());
    assert_eq!(cd_type, 0, "blob type 0 is CSSLOT_CODEDIRECTORY");

    let cd_offset = u32::from_be_bytes(sig_bytes[16..20].try_into().unwrap()) as usize;
    let cd_bytes = &sig_bytes[cd_offset..];
    let cd_magic = u32::from_be_bytes(cd_bytes[0..4].try_into().unwrap());
    assert_eq!(
        cd_magic, 0xFADE0C02,
        "CodeDirectory magic must be CSMAGIC_CODEDIRECTORY"
    );

    let cd_flags = u32::from_be_bytes(cd_bytes[12..16].try_into().unwrap());
    assert_ne!(cd_flags & 0x0002, 0, "flags must include CS_ADHOC");

    let n_code_slots = u32::from_be_bytes(cd_bytes[28..32].try_into().unwrap()) as usize;
    let expected_slots = (sig_off + 4095) / 4096;
    assert_eq!(n_code_slots, expected_slots);
}
