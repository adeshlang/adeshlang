//! Phase 16: Mach-O production hardening (structural).
//!
//! Verifies dyld-facing production details that Phase 13 structure alone
//! does not pin:
//! 1. `LC_BUILD_VERSION` / macOS version packing
//! 2. `__DATA_CONST` segment (`SG_READ_ONLY`) for pointer-bearing rodata
//! 3. TLV descriptor + `__tlv_bootstrap` non-lazy bind
//! 4. Imported data symbols (`environ`) as Absolute64 / GOT binds
//! 5. Dylib `LC_ID_DYLIB` / custom `install_name` / `LC_RPATH` / export trie ABI names
//! 6. Ad-hoc code signature (`LC_CODE_SIGNATURE`, CodeDirectory, page slots)
//! 7. Dylibs omit `LC_LOAD_DYLINKER`; executables keep it
//!
//! Execution under real dyld remains the macOS CI job's responsibility.

use adesh_linker::macho::codesign::LC_CODE_SIGNATURE;
use adesh_linker::macho::dynamic::{
    BIND_TYPE_POINTER, decode_bind_stream_for_test, decode_export_trie_for_test,
};
use adesh_linker::macho::header::*;
use adesh_linker::macho::writer::{MachOWriter, macho_abi_export_name, parse_macos_version};
use adesh_linker::object::ObjectWriter;
use adesh_linker::relocation::{Relocation, RelocationKind};
use adesh_linker::section::Section;
use adesh_linker::symbol::{Symbol, SymbolBinding, SymbolType};
use adesh_linker::target::Target;
use adesh_linker::{LinkConfig, Linker, ObjectFile};
use tempfile::tempdir;

fn u32_at(bytes: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap())
}

fn read_cstr(bytes: &[u8], off: usize) -> String {
    let end = bytes[off..]
        .iter()
        .position(|&b| b == 0)
        .map(|i| off + i)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[off..end]).into_owned()
}

fn minimal_x86_main(target: &Target) -> ObjectFile {
    let mut obj = ObjectFile::new("main.o".into(), target.clone(), 0);
    let text = Section::new_code(".text", vec![0xb8, 0x2a, 0x00, 0x00, 0x00, 0xc3], 16);
    obj.add_section(text);
    obj.add_symbol(Symbol::new_defined(
        "main",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        6,
        0,
    ));
    obj
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
    assert_eq!(macho_abi_export_name("puts"), "_puts");
    assert_eq!(macho_abi_export_name("_NSGetArgc"), "_NSGetArgc");
    assert_eq!(macho_abi_export_name("__tlv_bootstrap"), "__tlv_bootstrap");
}

#[test]
fn test_build_version_load_command() {
    let target = Target::from_triple("x86_64-darwin").unwrap();
    let dir = tempdir().unwrap();
    let obj_path = dir.path().join("main.o");
    let out_path = dir.path().join("my_macho_app");
    ObjectWriter::write_to_file(&minimal_x86_main(&target), &obj_path).unwrap();

    let config = LinkConfig::new(out_path.clone(), target);
    Linker::link(&[obj_path], config).unwrap();

    let bytes = std::fs::read(&out_path).unwrap();
    assert_eq!(u32_at(&bytes, 0), MH_MAGIC_64);

    let ncmds = u32_at(&bytes, 16) as usize;
    let mut offset = 32;
    let mut found_build_version = false;
    let mut found_dylinker = false;

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
        if cmd == LC_LOAD_DYLINKER {
            found_dylinker = true;
            let name_off = u32_at(&bytes, offset + 8) as usize;
            let path = read_cstr(&bytes[offset..offset + cmdsize], name_off);
            assert_eq!(path, "/usr/lib/dyld");
        }
        offset += cmdsize;
    }

    assert!(found_build_version, "binary must contain LC_BUILD_VERSION");
    assert!(found_dylinker, "executables must contain LC_LOAD_DYLINKER");
}

// ─── Test 2: __DATA_CONST Segment Protection ────────────────────────────────

#[test]
fn test_data_const_protection_and_flags() {
    let target = Target::from_triple("x86_64-darwin").unwrap();
    let dir = tempdir().unwrap();
    let obj_path = dir.path().join("main.o");
    let out_path = dir.path().join("test_data_const");

    let mut obj = ObjectFile::new(obj_path.clone(), target.clone(), 0);
    // Keep .rodata live: rip-relative load of the first pointer word.
    let mut text = Section::new_code(
        ".text",
        vec![0x48, 0x8b, 0x05, 0x00, 0x00, 0x00, 0x00, 0xc3],
        16,
    );
    text.relocations.push(Relocation::new(
        3,
        "vtable",
        RelocationKind::PcRelative32,
        -4,
    ));
    obj.add_section(text);

    // Pointer-bearing rodata forces `__DATA_CONST` (dyld must rebase it).
    let mut rodata = Section::new_data(".rodata", vec![0u8; 16], false, 8);
    rodata
        .relocations
        .push(Relocation::new(0, "main", RelocationKind::Absolute64, 0));
    rodata
        .relocations
        .push(Relocation::new(8, "main", RelocationKind::Absolute64, 4));
    obj.add_section(rodata);

    obj.add_symbol(Symbol::new_defined(
        "main",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        8,
        0,
    ));
    obj.add_symbol(Symbol::new_defined(
        "vtable",
        SymbolBinding::Global,
        SymbolType::Object,
        1,
        0,
        16,
        0,
    ));
    ObjectWriter::write_to_file(&obj, &obj_path).unwrap();

    let config = LinkConfig::new(out_path.clone(), target);
    Linker::link(&[obj_path], config).unwrap();

    let bytes = std::fs::read(&out_path).unwrap();
    let ncmds = u32_at(&bytes, 16) as usize;
    let mut offset = 32;

    let mut found_data_const = false;
    for _ in 0..ncmds {
        let cmd = u32_at(&bytes, offset);
        let cmdsize = u32_at(&bytes, offset + 4) as usize;
        if cmd == LC_SEGMENT_64 {
            let segname = read_cstr(&bytes, offset + 8);
            if segname == "__DATA_CONST" {
                found_data_const = true;
                // segment_command_64: maxprot@56, initprot@60, flags@68
                let maxprot = u32_at(&bytes, offset + 56);
                let initprot = u32_at(&bytes, offset + 60);
                let flags = u32_at(&bytes, offset + 68);
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
    let target = Target::from_triple("aarch64-darwin").unwrap();
    let dir = tempdir().unwrap();
    let obj_path = dir.path().join("tlv.o");
    let out_path = dir.path().join("test_tlv");

    let mut obj = ObjectFile::new(obj_path.clone(), target.clone(), 0);
    let mut text = Section::new_code(
        ".text",
        vec![
            0x00, 0x00, 0x00, 0x90, // adrp x0
            0x00, 0x00, 0x40, 0xF9, // ldr → rewritten to add
            0xC0, 0x03, 0x5F, 0xD6, // ret
        ],
        16,
    );
    text.relocations
        .push(Relocation::new(0, "_DESC", RelocationKind::AArch64Adrp, 0));
    text.relocations.push(Relocation::new(
        4,
        "_DESC",
        RelocationKind::AArch64TlvDescriptorLo12,
        0,
    ));
    obj.add_section(text);

    let mut tlv = Section::new_data("__thread_vars", vec![0u8; 24], true, 8);
    tlv.flags |= adesh_linker::section::flags::TLV_TABLE;
    tlv.relocations.push(Relocation::new(
        0,
        "__tlv_bootstrap",
        RelocationKind::Absolute64,
        0,
    ));
    tlv.relocations.push(Relocation::new(
        16,
        "_VAR$tlv$init",
        RelocationKind::Absolute64,
        0,
    ));
    obj.add_section(tlv);

    let mut tbss = Section::new_bss("__thread_bss", 8, 8);
    tbss.kind = adesh_linker::section::SectionKind::TBss;
    tbss.flags |= adesh_linker::section::flags::TLS;
    obj.add_section(tbss);

    obj.add_symbol(Symbol::new_defined(
        "main",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        12,
        0,
    ));
    obj.add_symbol(Symbol::new_defined(
        "_VAR$tlv$init",
        SymbolBinding::Global,
        SymbolType::Tls,
        2,
        0,
        8,
        0,
    ));
    obj.add_symbol(Symbol::new_defined(
        "_DESC",
        SymbolBinding::Global,
        SymbolType::Object,
        1,
        0,
        24,
        0,
    ));
    ObjectWriter::write_to_file(&obj, &obj_path).unwrap();

    let config = LinkConfig::new(out_path.clone(), target);
    Linker::link(&[obj_path], config).unwrap();

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
    let decoded_binds = decode_bind_stream_for_test(&bytes[bind_off..bind_off + bind_size]);
    let bootstrap_bind = decoded_binds
        .iter()
        .find(|b| b.4 == "__tlv_bootstrap")
        .expect("bind stream must contain __tlv_bootstrap");
    assert_eq!(bootstrap_bind.2, BIND_TYPE_POINTER);
    assert_eq!(bootstrap_bind.5, 0);
}

// ─── Test 4: Imported Data Symbols (environ) and Data Bind ───────────────────

#[test]
fn test_macho_environ_data_import() {
    let target = Target::from_triple("x86_64-darwin").unwrap();
    let dir = tempdir().unwrap();
    let obj_path = dir.path().join("environ.o");
    let out_path = dir.path().join("test_environ");

    let mut obj = ObjectFile::new(obj_path.clone(), target.clone(), 0);
    // Keep .data alive via a local rip-relative load (GC is on by default).
    let mut text = Section::new_code(
        ".text",
        vec![0x48, 0x8b, 0x05, 0x00, 0x00, 0x00, 0x00, 0xc3],
        16,
    );
    text.relocations.push(Relocation::new(
        3,
        "env_slot",
        RelocationKind::PcRelative32,
        -4,
    ));
    obj.add_section(text);

    // Absolute64 against imported `environ` → non-lazy dyld data bind.
    let mut data = Section::new_data(".data", vec![0u8; 8], true, 8);
    data.relocations
        .push(Relocation::new(0, "environ", RelocationKind::Absolute64, 0));
    obj.add_section(data);

    obj.add_symbol(Symbol::new_defined(
        "main",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        8,
        0,
    ));
    obj.add_symbol(Symbol::new_defined(
        "env_slot",
        SymbolBinding::Global,
        SymbolType::Object,
        1,
        0,
        8,
        0,
    ));
    obj.add_symbol(Symbol::new_undefined("environ", 0));
    ObjectWriter::write_to_file(&obj, &obj_path).unwrap();

    let config = LinkConfig::new(out_path.clone(), target);
    Linker::link(&[obj_path], config).unwrap();

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
    let decoded_binds = decode_bind_stream_for_test(&bytes[bind_off..bind_off + bind_size]);
    let environ_binds: Vec<_> = decoded_binds.iter().filter(|b| b.4 == "_environ").collect();
    assert!(
        !environ_binds.is_empty(),
        "bind stream must contain _environ Absolute64 data bind; got {:?}",
        decoded_binds
            .iter()
            .map(|b| b.4.as_str())
            .collect::<Vec<_>>()
    );
    for b in &environ_binds {
        assert_eq!(b.2, BIND_TYPE_POINTER);
    }
}

// ─── Test 5: Dylib Generation, install_name, LC_RPATH, Export Trie ───────────

#[test]
fn test_macho_dylib_generation_install_name_rpath_and_export_trie() {
    let target = Target::from_triple("x86_64-darwin").unwrap();
    let dir = tempdir().unwrap();
    let obj_path = dir.path().join("dylib_src.o");
    let out_path = dir.path().join("libmy_lib.dylib");

    let mut obj = ObjectFile::new(obj_path.clone(), target.clone(), 0);
    let text = Section::new_code(".text", vec![0xb8, 0x64, 0x00, 0x00, 0x00, 0xc3], 16);
    obj.add_section(text);
    obj.add_symbol(Symbol::new_defined(
        "my_exported_func",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        6,
        0,
    ));
    ObjectWriter::write_to_file(&obj, &obj_path).unwrap();

    let mut config = LinkConfig::new(out_path.clone(), target);
    config.shared = true;
    config.entry_point = None;
    config.install_name = Some("@rpath/libmy_lib.dylib".into());
    config.rpaths = vec!["@loader_path".into(), "@executable_path/../lib".into()];
    Linker::link(&[obj_path], config).unwrap();

    let bytes = std::fs::read(&out_path).unwrap();
    assert_eq!(u32_at(&bytes, 0), MH_MAGIC_64);
    assert_eq!(u32_at(&bytes, 12), MH_DYLIB, "filetype must be MH_DYLIB");

    let ncmds = u32_at(&bytes, 16) as usize;
    let mut offset = 32;
    let mut found_id_dylib = false;
    let mut found_dylinker = false;
    let mut rpaths = Vec::new();
    let mut export_off = 0usize;
    let mut export_size = 0usize;

    for _ in 0..ncmds {
        let cmd = u32_at(&bytes, offset);
        let cmdsize = u32_at(&bytes, offset + 4) as usize;
        match cmd {
            LC_ID_DYLIB => {
                found_id_dylib = true;
                let name_off = u32_at(&bytes, offset + 8) as usize;
                let name = read_cstr(&bytes[offset..offset + cmdsize], name_off);
                assert_eq!(name, "@rpath/libmy_lib.dylib");
            }
            LC_LOAD_DYLINKER => found_dylinker = true,
            LC_RPATH => {
                let path_off = u32_at(&bytes, offset + 8) as usize;
                rpaths.push(read_cstr(&bytes[offset..offset + cmdsize], path_off));
            }
            LC_DYLD_INFO_ONLY | LC_DYLD_INFO => {
                export_off = u32_at(&bytes, offset + 40) as usize;
                export_size = u32_at(&bytes, offset + 44) as usize;
            }
            _ => {}
        }
        offset += cmdsize;
    }

    assert!(found_id_dylib, "dylib must contain LC_ID_DYLIB");
    assert!(!found_dylinker, "dylibs must not contain LC_LOAD_DYLINKER");
    assert_eq!(
        rpaths,
        vec!["@loader_path", "@executable_path/../lib"],
        "LC_RPATH entries must match LinkConfig.rpaths"
    );
    assert!(export_size > 0, "export trie must not be empty");

    let exports = decode_export_trie_for_test(&bytes[export_off..export_off + export_size]);
    assert!(
        exports
            .iter()
            .any(|(name, _, _)| name == "_my_exported_func"),
        "export trie must contain _my_exported_func; got {exports:?}"
    );
}

#[test]
fn test_macho_dylib_default_install_name_is_loader_path() {
    let target = Target::from_triple("x86_64-darwin").unwrap();
    let dir = tempdir().unwrap();
    let obj_path = dir.path().join("lib.o");
    let out_path = dir.path().join("libsibling.dylib");

    let mut obj = ObjectFile::new(obj_path.clone(), target.clone(), 0);
    obj.add_section(Section::new_code(
        ".text",
        vec![0xb8, 0x01, 0x00, 0x00, 0x00, 0xc3],
        16,
    ));
    obj.add_symbol(Symbol::new_defined(
        "sibling_api",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        6,
        0,
    ));
    ObjectWriter::write_to_file(&obj, &obj_path).unwrap();

    let mut config = LinkConfig::new(out_path.clone(), target);
    config.shared = true;
    Linker::link(&[obj_path], config).unwrap();

    let bytes = std::fs::read(&out_path).unwrap();
    let ncmds = u32_at(&bytes, 16) as usize;
    let mut offset = 32;
    let mut id_name = None;
    for _ in 0..ncmds {
        let cmd = u32_at(&bytes, offset);
        let cmdsize = u32_at(&bytes, offset + 4) as usize;
        if cmd == LC_ID_DYLIB {
            let name_off = u32_at(&bytes, offset + 8) as usize;
            id_name = Some(read_cstr(&bytes[offset..offset + cmdsize], name_off));
        }
        offset += cmdsize;
    }
    assert_eq!(
        id_name.as_deref(),
        Some("@loader_path/libsibling.dylib"),
        "default install_name must be @loader_path/<basename>"
    );
}

// ─── Test 6: Ad-hoc Code Signature Integrity ─────────────────────────────────

#[test]
fn test_macho_adhoc_code_signature_integrity() {
    let target = Target::from_triple("x86_64-darwin").unwrap();
    let dir = tempdir().unwrap();
    let obj_path = dir.path().join("signed.o");
    let out_path = dir.path().join("signed_bin");

    let mut obj = ObjectFile::new(obj_path.clone(), target.clone(), 0);
    let mut text = Section::new_code(".text", vec![0x90; 10000], 16);
    text.data[9999] = 0xc3;
    obj.add_section(text);
    obj.add_symbol(Symbol::new_defined(
        "main",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        10000,
        0,
    ));
    ObjectWriter::write_to_file(&obj, &obj_path).unwrap();

    let config = LinkConfig::new(out_path.clone(), target);
    Linker::link(&[obj_path], config).unwrap();

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

    let ident_off = u32::from_be_bytes(cd_bytes[20..24].try_into().unwrap()) as usize;
    let ident = read_cstr(cd_bytes, ident_off);
    assert_eq!(
        ident, "signed_bin",
        "CodeDirectory identifier must match the output basename"
    );

    let n_code_slots = u32::from_be_bytes(cd_bytes[28..32].try_into().unwrap()) as usize;
    let expected_slots = (sig_off + 4095) / 4096;
    assert_eq!(n_code_slots, expected_slots);

    // Encode path must also accept an empty rpaths slice (writer API surface).
    let _ = MachOWriter::encode_macho(
        &Target::from_triple("x86_64-darwin").unwrap(),
        0x100000000,
        &[],
        &[],
        false,
        &[],
        None,
    );
}

// ─── Test 7: Unsupported non-pointer import relocation still fails loudly ───

#[test]
fn test_macho_non_pointer_import_relocation_rejected() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("prog");
    let target = Target::from_triple("x86_64-darwin").unwrap();
    let obj_path = dir.path().join("main.o");
    let mut obj = ObjectFile::new(obj_path.clone(), target.clone(), 0);

    let mut text = Section::new_code(
        ".text",
        vec![0x48, 0x8B, 0x05, 0x00, 0x00, 0x00, 0x00, 0xC3],
        16,
    );
    text.relocations.push(Relocation::new(
        3,
        "global_value",
        RelocationKind::PcRelative32,
        -4,
    ));
    obj.add_section(text);

    // Absolute32 against an import is not a dyld pointer bind shape.
    let mut data_sec = Section::new_data(".data", vec![0u8; 4], true, 4);
    data_sec
        .relocations
        .push(Relocation::new(0, "stdout", RelocationKind::Absolute32, 0));
    obj.add_section(data_sec);

    obj.add_symbol(Symbol::new_defined(
        "main",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        8,
        0,
    ));
    obj.add_symbol(Symbol::new_defined(
        "global_value",
        SymbolBinding::Global,
        SymbolType::Object,
        1,
        0,
        4,
        0,
    ));
    obj.add_symbol(Symbol::new_undefined("stdout", 0));
    ObjectWriter::write_to_file(&obj, &obj_path).unwrap();

    let config = LinkConfig::new(out.clone(), target);
    let err = Linker::link(&[obj_path], config).expect_err("unsupported import reloc must fail");
    let msg = format!("{err}");
    assert!(
        msg.contains("stdout") && msg.contains("non-call relocation"),
        "unexpected error: {msg}"
    );
    assert!(
        !out.exists(),
        "no binary may be produced for a rejected link"
    );
}
