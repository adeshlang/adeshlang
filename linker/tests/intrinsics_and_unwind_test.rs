use adesh_linker::intrinsics::IntrinsicsEngine;
use adesh_linker::target::Target;
use adesh_linker::unwind::{DropTableEntry, EhFrameHdrGenerator, PdataEntry, RaiiDropTable, WindowsPdataGenerator};

#[test]
fn test_runtime_intrinsics_synthesis_x86_64() {
    let target = Target::x86_64_linux();
    let needed = vec![
        "memcpy".to_string(),
        "memset".to_string(),
        "memcmp".to_string(),
        "__multi3".to_string(),
        "__stack_chk_fail".to_string(),
    ];

    let result = IntrinsicsEngine::synthesize_intrinsics_section(&needed, &target);
    assert!(result.is_some());
    let (section, symbols) = result.unwrap();

    assert_eq!(section.name, ".text.adesh_rt");
    assert!(!section.data.is_empty());
    assert_eq!(symbols.len(), 5);

    let memcpy_sym = symbols.iter().find(|s| s.name == "memcpy").unwrap();
    assert_eq!(memcpy_sym.size, 9); // x86_64 memcpy stub size (3+3+2+1 = 9 bytes)
}

#[test]
fn test_runtime_intrinsics_synthesis_aarch64() {
    let target = Target::aarch64_macos();
    let needed = vec![
        "memcpy".to_string(),
        "memset".to_string(),
        "__stack_chk_fail".to_string(),
    ];

    let result = IntrinsicsEngine::synthesize_intrinsics_section(&needed, &target);
    assert!(result.is_some());
    let (section, symbols) = result.unwrap();

    assert_eq!(symbols.len(), 3);
    assert!(!section.data.is_empty());
}

#[test]
fn test_eh_frame_hdr_table_generation() {
    let hdr_va = 0x400000;
    let eh_frame_va = 0x401000;
    let fdes = vec![
        (0x402000, 0x401020),
        (0x402100, 0x401040),
        (0x402200, 0x401060),
    ];

    let bytes = EhFrameHdrGenerator::build(hdr_va, eh_frame_va, fdes);
    assert!(bytes.len() >= 12);
    // Version byte must be 1
    assert_eq!(bytes[0], 1);
    // DW_EH_PE encodings
    assert_eq!(bytes[1], 0x1b);
    assert_eq!(bytes[2], 0x03);
    assert_eq!(bytes[3], 0x3b);
}

#[test]
fn test_windows_x64_pdata_unwind_table_generation() {
    let entries = vec![
        PdataEntry {
            begin_rva: 0x1000,
            end_rva: 0x1080,
            unwind_info_rva: 0x2000,
        },
        PdataEntry {
            begin_rva: 0x1080,
            end_rva: 0x1100,
            unwind_info_rva: 0x2004,
        },
    ];

    let pdata_bytes = WindowsPdataGenerator::build_pdata(entries);
    assert_eq!(pdata_bytes.len(), 24); // 2 * 12 bytes

    let xdata_bytes = WindowsPdataGenerator::build_default_xdata();
    assert_eq!(xdata_bytes.len(), 4);
    assert_eq!(xdata_bytes[0], 0x01); // Version 1
}

#[test]
fn test_raii_ownership_drop_table_generation() {
    let drops = vec![
        DropTableEntry {
            type_id: 0x1001,
            drop_fn_va: 0x401200,
            type_size: 24,
            type_align: 8,
        },
        DropTableEntry {
            type_id: 0x1002,
            drop_fn_va: 0x401250,
            type_size: 64,
            type_align: 16,
        },
    ];

    let drop_table_bytes = RaiiDropTable::build_drop_table(&drops);
    assert!(drop_table_bytes.starts_with(b"ADROP\x01\x00\x00"));
    assert_eq!(drop_table_bytes.len(), 16 + 2 * 32); // 8 magic + 8 count + 2 entries * 32 bytes
}
