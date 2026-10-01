use adesh_object::bundle::AdobBundle;
use adesh_object::format::AdobObject;
use adesh_object::reader::AdobReader;
use adesh_object::relocation::{AdobRelocation, RelocationKind};
use adesh_object::section::{AdobSection, SectionKind};
use adesh_object::symbol::{AdobSymbol, SymbolKind};
use adesh_object::target::{Architecture, ComputeDevice, TargetDescriptor};
use adesh_object::validator::AdobValidator;
use adesh_object::writer::AdobWriter;

#[test]
fn test_end_to_end_adob_serialization_and_validation() {
    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    let mut obj = AdobObject::new(target);

    let code = vec![0x48, 0x89, 0xd8, 0xc3]; // mov rax, rbx; ret
    let mut text_sec = AdobSection::new(".text", SectionKind::Text).with_data(code);
    text_sec.add_relocation(AdobRelocation::new(
        0,
        0,
        "printf",
        RelocationKind::PcRelative32,
        0,
    ));
    let text_idx = obj.add_section(text_sec);

    let main_sym = AdobSymbol::new_defined(0, "main", SymbolKind::Function, text_idx, 0, 4);
    obj.add_symbol(main_sym);
    obj.add_export("main");

    // Write object
    let encoded = AdobWriter::write(&obj).expect("Failed to write ADOB object");

    // Read object
    let decoded = AdobReader::read_object(&encoded).expect("Failed to read ADOB object");

    assert_eq!(decoded.header.magic, *b"ADOB");
    assert_eq!(decoded.sections.len(), 1);
    assert_eq!(decoded.symbols.len(), 1);
    assert_eq!(decoded.sections[0].name, ".text");
    assert_eq!(decoded.symbols[0].name, "main");
    assert_eq!(decoded.sections[0].relocations.len(), 1);

    // Validate
    AdobValidator::validate(&decoded).expect("Validation should pass");
}

#[test]
fn test_multi_arch_bundle() {
    let mut bundle = AdobBundle::new();

    let target_x86 = TargetDescriptor::from_triple("x86_64-unknown-linux-gnu").unwrap();
    let target_aarch64 = TargetDescriptor::from_triple("aarch64-unknown-linux-gnu").unwrap();

    let obj1 = AdobObject::new(target_x86);
    let obj2 = AdobObject::new(target_aarch64);

    bundle.add_object(obj1);
    bundle.add_object(obj2);

    assert_eq!(bundle.entries.len(), 2);
    assert!(
        bundle
            .find_matching(&Architecture::X86_64, ComputeDevice::Cpu)
            .is_some()
    );
    assert!(
        bundle
            .find_matching(&Architecture::AArch64, ComputeDevice::Cpu)
            .is_some()
    );
}
