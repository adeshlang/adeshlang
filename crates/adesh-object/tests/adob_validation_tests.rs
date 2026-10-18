use adesh_object::{
    ADOB_MAGIC, AdobErrorCode, AdobObject, AdobReader, AdobRelocation, AdobSection, AdobSymbol,
    AdobValidator, AdobWriter, RelocationKind, SectionKind, SymbolBinding, SymbolKind,
    TargetDescriptor,
};

#[test]
fn test_truncated_header() {
    let truncated = [0x41, 0x44, 0x4F]; // 'ADO' - only 3 bytes
    let err = AdobReader::read_object(&truncated).unwrap_err();
    assert_eq!(err.code, AdobErrorCode::BufferUnderflow);
}

#[test]
fn test_invalid_magic() {
    let invalid = b"NOPE\x01\x00\x00\x00\x00\x00\x00\x00";
    let err = AdobReader::read_object(invalid).unwrap_err();
    assert_eq!(err.code, AdobErrorCode::InvalidMagic);
}

#[test]
fn test_unsupported_major_version() {
    let mut header = Vec::new();
    header.extend_from_slice(ADOB_MAGIC);
    header.extend_from_slice(&999u16.to_le_bytes()); // major = 999
    header.extend_from_slice(&0u16.to_le_bytes()); // minor = 0
    header.extend_from_slice(&0u16.to_le_bytes()); // patch = 0
    header.extend_from_slice(&0u32.to_le_bytes()); // flags

    let err = AdobReader::read_object(&header).unwrap_err();
    assert_eq!(err.code, AdobErrorCode::UnsupportedVersion);
}

#[test]
fn test_oversized_string_protection() {
    let mut data = Vec::new();
    data.extend_from_slice(ADOB_MAGIC);
    data.extend_from_slice(&1u16.to_le_bytes()); // major = 1
    data.extend_from_slice(&0u16.to_le_bytes()); // minor = 0
    data.extend_from_slice(&0u16.to_le_bytes()); // patch = 0
    data.extend_from_slice(&0u32.to_le_bytes()); // flags = 0
    data.push(0); // device = Cpu
    data.extend_from_slice(&0u16.to_le_bytes()); // arch = X86_64
    data.push(0); // arch_custom = None
    data.push(1); // os = Windows
    data.push(0); // env = Msvc
    data.push(2); // abi = WindowsX64
    data.push(8); // ptr_width = 8 (U64)
    data.push(0); // endian = Little
    data.extend_from_slice(&0u32.to_le_bytes()); // feature_count = 0

    // Counts: sec=1, sym=0, imp=0, exp=0, mr=0, ext=0
    data.extend_from_slice(&1u32.to_le_bytes()); // sec_count = 1
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());

    // Section 0 name string length = 100 MB (> MAX_STRING_LEN 64 MB)
    data.extend_from_slice(&(100 * 1024 * 1024u32).to_le_bytes());

    let err = AdobReader::read_object(&data).unwrap_err();
    assert_eq!(err.code, AdobErrorCode::InvalidMetadata);
    assert!(err.message.contains("exceeds maximum allowed length"));
}

#[test]
fn test_oversized_section_count_rejection() {
    let mut data = Vec::new();
    data.extend_from_slice(ADOB_MAGIC);
    data.extend_from_slice(&1u16.to_le_bytes()); // major = 1
    data.extend_from_slice(&0u16.to_le_bytes()); // minor = 0
    data.extend_from_slice(&0u16.to_le_bytes()); // patch = 0
    data.extend_from_slice(&0u32.to_le_bytes()); // flags = 0
    data.push(0); // device = Cpu
    data.extend_from_slice(&0u16.to_le_bytes()); // arch = X86_64
    data.push(0); // arch_custom = None
    data.push(1); // os = Windows
    data.push(0); // env = Msvc
    data.push(2); // abi = WindowsX64
    data.push(8); // ptr_width = 8 (U64)
    data.push(0); // endian = Little
    data.extend_from_slice(&0u32.to_le_bytes()); // feature_count = 0

    // sec_count = 100_000 (> MAX_SECTIONS 65_536)
    data.extend_from_slice(&100_000u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());

    let err = AdobReader::read_object(&data).unwrap_err();
    assert_eq!(err.code, AdobErrorCode::InvalidSection);
    assert!(err.message.contains("exceeds MAX_SECTIONS"));
}

#[test]
fn test_validator_rejects_non_power_of_two_alignment() {
    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    let mut obj = AdobObject::new(target);
    let mut sec = AdobSection::new(".text", SectionKind::Text).with_data(vec![0x90]);
    sec.alignment = 3; // Invalid non-power of two
    obj.add_section(sec);

    let err = AdobValidator::validate(&obj).unwrap_err();
    assert_eq!(err.code, AdobErrorCode::InvalidAlignment);
}

#[test]
fn test_validator_rejects_duplicate_global_symbols() {
    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    let mut obj = AdobObject::new(target);
    let sec = AdobSection::new(".text", SectionKind::Text).with_data(vec![0x90, 0x90]);
    let sec_idx = obj.add_section(sec);

    let mut sym1 =
        AdobSymbol::new_defined(0, "duplicate_func", SymbolKind::Function, sec_idx, 0, 1);
    sym1.binding = SymbolBinding::Global;
    obj.add_symbol(sym1);

    let mut sym2 =
        AdobSymbol::new_defined(1, "duplicate_func", SymbolKind::Function, sec_idx, 1, 1);
    sym2.binding = SymbolBinding::Global;
    obj.add_symbol(sym2);

    let err = AdobValidator::validate(&obj).unwrap_err();
    assert_eq!(err.code, AdobErrorCode::DuplicateSymbol);
}

#[test]
fn test_validator_rejects_relocation_out_of_bounds() {
    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    let mut obj = AdobObject::new(target);
    let mut sec = AdobSection::new(".text", SectionKind::Text).with_data(vec![0x90; 4]); // 4 bytes

    let sym = AdobSymbol::new_defined(0, "target", SymbolKind::Function, 0, 0, 4);
    obj.add_symbol(sym);

    // Relocation at offset 2 with width 4 would extend to offset 6 > sec.data.len() (4)
    let mut reloc = AdobRelocation::new(2, 0, "target", RelocationKind::PcRelative32, 0);
    reloc.width = 4;
    sec.add_relocation(reloc);
    obj.add_section(sec);

    let err = AdobValidator::validate(&obj).unwrap_err();
    assert_eq!(err.code, AdobErrorCode::RelocationOutOfRange);
}

#[test]
fn test_validator_rejects_relocation_target_not_found() {
    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    let mut obj = AdobObject::new(target);
    let mut sec = AdobSection::new(".text", SectionKind::Text).with_data(vec![0x90; 8]);

    // Relocation references non-existent symbol id 99 and name "missing_sym"
    let reloc = AdobRelocation::new(0, 99, "missing_sym", RelocationKind::PcRelative32, 0);
    sec.add_relocation(reloc);
    obj.add_section(sec);

    let err = AdobValidator::validate(&obj).unwrap_err();
    assert_eq!(err.code, AdobErrorCode::RelocationTargetNotFound);
}

#[test]
fn test_serialization_roundtrip_determinism() {
    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    let mut obj = AdobObject::new(target);
    let mut sec = AdobSection::new(".text", SectionKind::Text)
        .with_data(vec![0x48, 0x31, 0xC0, 0x90, 0x90, 0x90, 0x90, 0xC3]);
    let sym = AdobSymbol::new_defined(0, "main", SymbolKind::Function, 0, 0, 8);
    obj.add_symbol(sym);
    let reloc = AdobRelocation::new(0, 0, "main", RelocationKind::Absolute64, 0);
    sec.add_relocation(reloc);
    obj.add_section(sec);
    obj.add_export("main");

    let bytes1 = AdobWriter::write(&obj).expect("First write");
    let decoded1 = AdobReader::read_object(&bytes1).expect("First decode");
    let bytes2 = AdobWriter::write(&decoded1).expect("Second write");
    let decoded2 = AdobReader::read_object(&bytes2).expect("Second decode");

    assert_eq!(
        bytes1, bytes2,
        "Serialization must be byte-for-byte deterministic across roundtrips"
    );
    assert_eq!(decoded1.sections.len(), decoded2.sections.len());
    assert_eq!(decoded1.symbols.len(), decoded2.symbols.len());
}
