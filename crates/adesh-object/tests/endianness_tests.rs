use adesh_object::target::{Architecture, Endianness, TargetDescriptor};
use adesh_object::{
    AdobErrorCode, AdobObject, AdobReader, AdobSection, AdobWriter, SectionKind,
};

#[test]
fn test_target_endianness_derivation_and_overrides() {
    // Standard LE targets
    let x86 = TargetDescriptor::from_triple("x86_64-unknown-linux-gnu").unwrap();
    assert_eq!(x86.endianness, Endianness::Little);
    assert!(x86.endianness.is_little());
    assert!(!x86.endianness.is_big());

    let aarch64 = TargetDescriptor::from_triple("aarch64-unknown-linux-gnu").unwrap();
    assert_eq!(aarch64.endianness, Endianness::Little);

    let riscv = TargetDescriptor::from_triple("riscv64gc-unknown-linux-gnu").unwrap();
    assert_eq!(riscv.endianness, Endianness::Little);

    // Big-endian targets by default
    let s390x = TargetDescriptor::from_triple("s390x-unknown-linux-gnu").unwrap();
    assert_eq!(s390x.architecture, Architecture::S390x);
    assert_eq!(s390x.endianness, Endianness::Big);
    assert!(s390x.endianness.is_big());
    assert!(!s390x.endianness.is_little());

    let sparc64 = TargetDescriptor::from_triple("sparc64-unknown-linux-gnu").unwrap();
    assert_eq!(sparc64.architecture, Architecture::Sparc64);
    assert_eq!(sparc64.endianness, Endianness::Big);

    let ppc = TargetDescriptor::from_triple("powerpc-unknown-linux-gnu").unwrap();
    assert_eq!(ppc.architecture, Architecture::PowerPc);
    assert_eq!(ppc.endianness, Endianness::Big);

    let mips = TargetDescriptor::from_triple("mips-unknown-linux-gnu").unwrap();
    assert_eq!(mips.architecture, Architecture::Mips);
    assert_eq!(mips.endianness, Endianness::Big);

    // Little-endian overrides for naturally big-endian architectures
    let ppc64le = TargetDescriptor::from_triple("powerpc64le-unknown-linux-gnu").unwrap();
    assert_eq!(ppc64le.architecture, Architecture::PowerPc64);
    assert_eq!(ppc64le.endianness, Endianness::Little);

    let mipsel = TargetDescriptor::from_triple("mipsel-unknown-linux-gnu").unwrap();
    assert_eq!(mipsel.architecture, Architecture::Mips);
    assert_eq!(mipsel.endianness, Endianness::Little);
}

#[test]
fn test_adob_endianness_roundtrip() {
    let mut obj_le = AdobObject::new(TargetDescriptor::from_triple("x86_64-pc-windows-msvc").unwrap());
    obj_le.add_section(
        AdobSection::new(".text", SectionKind::Text)
            .with_data(vec![0x90, 0x90])
            .with_alignment(16),
    );
    let bytes_le = AdobWriter::write(&obj_le).expect("write LE ADOB");
    let read_le = AdobReader::read_object(&bytes_le).expect("read LE ADOB");
    assert_eq!(read_le.target.endianness, Endianness::Little);

    let mut obj_be = AdobObject::new(TargetDescriptor::from_triple("s390x-unknown-linux-gnu").unwrap());
    obj_be.add_section(
        AdobSection::new(".text", SectionKind::Text)
            .with_data(vec![0x07, 0xFE])
            .with_alignment(16),
    );
    let bytes_be = AdobWriter::write(&obj_be).expect("write BE ADOB");
    let read_be = AdobReader::read_object(&bytes_be).expect("read BE ADOB");
    assert_eq!(read_be.target.endianness, Endianness::Big);
}

#[test]
fn test_adob_rejects_invalid_endian_id() {
    let obj = AdobObject::new(TargetDescriptor::from_triple("x86_64-pc-windows-msvc").unwrap());
    let mut bytes = AdobWriter::write(&obj).expect("write ADOB");

    // offset 22 is endian_id
    bytes[22] = 2; // Invalid (only 0 and 1 are valid)

    let err = AdobReader::read_object(&bytes).unwrap_err();
    assert_eq!(err.code, AdobErrorCode::InvalidMetadata);
    assert!(err.message.contains("Invalid endianness identifier"));
}
