use adesh_linker::abi::{
    ADESH_ABI_ENDIAN_BIG, ADESH_ABI_ENDIAN_LITTLE, AdeshAbiHeader, verify_abi_compatibility,
};
use adesh_linker::arch::get_handler;
use adesh_linker::elf::header::EM_S390;
use adesh_linker::elf::reader::ElfReader;
use adesh_linker::elf::writer::ElfWriter;
use adesh_linker::error::ErrorCode;
use adesh_linker::relocation::{
    DefaultRelocationHandler, Relocation, RelocationHandler, RelocationKind,
};
use adesh_linker::section::{MergedSection, SectionKind, flags};
use adesh_linker::symbol::{Symbol, SymbolBinding, SymbolType};
use adesh_linker::target::{Arch, Endianness, Target};
use std::path::Path;

#[test]
fn test_relocation_handler_endianness() {
    let mut image_le = vec![0u8; 16];
    let mut image_be = vec![0u8; 16];

    let reloc64 = Relocation::new(0, "sym64", RelocationKind::Absolute64, 0);

    // Test Absolute64: 0x0102030405060708
    let val: u64 = 0x0102030405060708;
    DefaultRelocationHandler::LITTLE
        .apply(&reloc64, 0x1000, val, 0, &mut image_le)
        .expect("apply LE");
    DefaultRelocationHandler::BIG
        .apply(&reloc64, 0x1000, val, 0, &mut image_be)
        .expect("apply BE");

    assert_eq!(
        &image_le[0..8],
        &[0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]
    );
    assert_eq!(
        &image_be[0..8],
        &[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]
    );

    // Test Absolute32: 0x11223344
    let reloc32 = Relocation::new(0, "sym32", RelocationKind::Absolute32, 0);
    let val32: u64 = 0x11223344;
    DefaultRelocationHandler::LITTLE
        .apply(&reloc32, 0x1000, val32, 0, &mut image_le)
        .expect("apply LE 32");
    DefaultRelocationHandler::BIG
        .apply(&reloc32, 0x1000, val32, 0, &mut image_be)
        .expect("apply BE 32");

    assert_eq!(&image_le[0..4], &[0x44, 0x33, 0x22, 0x11]);
    assert_eq!(&image_be[0..4], &[0x11, 0x22, 0x33, 0x44]);

    // Test PcRelative32: target 0x1100, place 0x1000 -> offset +0x100 = 0x00000100
    let reloc_pc = Relocation::new(0, "sym_pc", RelocationKind::PcRelative32, 0);
    DefaultRelocationHandler::LITTLE
        .apply(&reloc_pc, 0x1000, 0x1100, 0, &mut image_le)
        .expect("apply LE PC32");
    DefaultRelocationHandler::BIG
        .apply(&reloc_pc, 0x1000, 0x1100, 0, &mut image_be)
        .expect("apply BE PC32");

    assert_eq!(&image_le[0..4], &[0x00, 0x01, 0x00, 0x00]);
    assert_eq!(&image_be[0..4], &[0x00, 0x00, 0x01, 0x00]);
}

#[test]
fn test_arch_handler_wiring() {
    let handler_x86 = get_handler(Arch::X86_64);
    let handler_s390x = get_handler(Arch::S390x);

    let mut img_x86 = vec![0u8; 8];
    let mut img_s390x = vec![0u8; 8];

    let reloc = Relocation::new(0, "test", RelocationKind::Absolute64, 0);
    handler_x86
        .apply(&reloc, 0x1000, 0xAABBCCDDEEFF0011, 0, &mut img_x86)
        .unwrap();
    handler_s390x
        .apply(&reloc, 0x1000, 0xAABBCCDDEEFF0011, 0, &mut img_s390x)
        .unwrap();

    // x86_64 is little-endian
    assert_eq!(
        u64::from_le_bytes(img_x86[0..8].try_into().unwrap()),
        0xAABBCCDDEEFF0011
    );
    // s390x is big-endian
    assert_eq!(
        u64::from_be_bytes(img_s390x[0..8].try_into().unwrap()),
        0xAABBCCDDEEFF0011
    );
}

fn build_minimal_elf64(machine: u16, data: u8) -> Vec<u8> {
    let mut elf = vec![0u8; 64];
    // Magic: \x7FELF
    elf[0..4].copy_from_slice(&[0x7F, b'E', b'L', b'F']);
    elf[4] = 2; // ELFCLASS64
    elf[5] = data; // 1 = Little, 2 = Big
    elf[6] = 1; // EV_CURRENT

    let is_be = data == 2;
    // e_type = ET_REL (1)
    if is_be {
        elf[16..18].copy_from_slice(&1u16.to_be_bytes());
        elf[18..20].copy_from_slice(&machine.to_be_bytes());
        elf[20..24].copy_from_slice(&1u32.to_be_bytes()); // e_version = 1
        elf[40..48].copy_from_slice(&64u64.to_be_bytes()); // e_shoff = 64
        elf[52..54].copy_from_slice(&64u16.to_be_bytes()); // e_ehsize = 64
        elf[58..60].copy_from_slice(&64u16.to_be_bytes()); // e_shentsize = 64
        elf[60..62].copy_from_slice(&1u16.to_be_bytes()); // e_shnum = 1
        elf[62..64].copy_from_slice(&0u16.to_be_bytes()); // e_shstrndx = 0
    } else {
        elf[16..18].copy_from_slice(&1u16.to_le_bytes());
        elf[18..20].copy_from_slice(&machine.to_le_bytes());
        elf[20..24].copy_from_slice(&1u32.to_le_bytes()); // e_version = 1
        elf[40..48].copy_from_slice(&64u64.to_le_bytes()); // e_shoff = 64
        elf[52..54].copy_from_slice(&64u16.to_le_bytes()); // e_ehsize = 64
        elf[58..60].copy_from_slice(&64u16.to_le_bytes()); // e_shentsize = 64
        elf[60..62].copy_from_slice(&1u16.to_le_bytes()); // e_shnum = 1
        elf[62..64].copy_from_slice(&0u16.to_le_bytes()); // e_shstrndx = 0
    }

    // Append 1 null section header (64 bytes)
    elf.extend_from_slice(&[0u8; 64]);
    elf
}

#[test]
fn test_elf_reader_valid_little_and_big_endian() {
    let dummy_path = Path::new("dummy.o");
    // 1. Valid Little-Endian x86_64 ELF (machine EM_X86_64 = 62)
    let le_elf = build_minimal_elf64(62, 1);
    let obj_le = ElfReader::read(&le_elf, dummy_path, 0).expect("parse LE x86_64 ELF");
    assert_eq!(obj_le.target.endianness, Endianness::Little);
    assert_eq!(obj_le.target.arch, Arch::X86_64);

    // 2. Valid Big-Endian S390x ELF (machine EM_S390 = 22)
    let be_elf = build_minimal_elf64(22, 2);
    let obj_be = ElfReader::read(&be_elf, dummy_path, 1).expect("parse BE s390x ELF");
    assert_eq!(obj_be.target.endianness, Endianness::Big);
    assert_eq!(obj_be.target.arch, Arch::S390x);

    // 3. Valid Big-Endian PPC64 ELF (machine EM_PPC64 = 21)
    let be_ppc = build_minimal_elf64(21, 2);
    let obj_ppc = ElfReader::read(&be_ppc, dummy_path, 2).expect("parse BE PPC64 ELF");
    assert_eq!(obj_ppc.target.endianness, Endianness::Big);
    assert_eq!(obj_ppc.target.arch, Arch::Ppc64);
}

#[test]
fn test_elf_reader_rejects_incompatible_endianness() {
    let dummy_path = Path::new("dummy.o");
    // x86_64 with Big-Endian (data = 2) must be rejected
    let bad_x86 = build_minimal_elf64(62, 2);
    let err = ElfReader::read(&bad_x86, dummy_path, 0).unwrap_err();
    assert_eq!(err.code, ErrorCode::ArchitectureMismatch);
    assert!(
        err.message
            .contains("x86-64 does not support big-endian ELF")
    );

    // s390x with Little-Endian (data = 1) must be rejected
    let bad_s390 = build_minimal_elf64(22, 1);
    let err = ElfReader::read(&bad_s390, dummy_path, 0).unwrap_err();
    assert_eq!(err.code, ErrorCode::ArchitectureMismatch);
    assert!(
        err.message
            .contains("s390x does not support little-endian ELF")
    );
}

#[test]
fn test_elf_reader_rejects_invalid_data_encoding() {
    let dummy_path = Path::new("dummy.o");
    let mut elf = build_minimal_elf64(62, 0); // data = 0 (ELFDATANONE)
    let err = ElfReader::read(&elf, dummy_path, 0).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidObject);
    assert!(err.message.contains("unsupported ELF data encoding"));

    elf[5] = 3; // data = 3 (invalid)
    let err = ElfReader::read(&elf, dummy_path, 0).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidObject);
    assert!(
        err.message
            .contains("unsupported ELF data encoding: 0x03 (3)")
    );
}

#[test]
fn test_abi_endianness_verification() {
    let mut hdr1 = AdeshAbiHeader::default();
    let mut hdr2 = AdeshAbiHeader::default();

    // Valid LE (1)
    hdr1.endianness = ADESH_ABI_ENDIAN_LITTLE;
    hdr2.endianness = ADESH_ABI_ENDIAN_LITTLE;
    assert!(verify_abi_compatibility(&hdr1, &hdr2).is_ok());

    // Valid BE (2)
    hdr1.endianness = ADESH_ABI_ENDIAN_BIG;
    hdr2.endianness = ADESH_ABI_ENDIAN_BIG;
    assert!(verify_abi_compatibility(&hdr1, &hdr2).is_ok());

    // Mismatched LE (1) vs BE (2)
    hdr1.endianness = ADESH_ABI_ENDIAN_LITTLE;
    hdr2.endianness = ADESH_ABI_ENDIAN_BIG;
    let err = verify_abi_compatibility(&hdr1, &hdr2).unwrap_err();
    assert_eq!(err.code, ErrorCode::AbiMismatch);
    assert!(err.message.contains("Endianness mismatch"));

    // Invalid endianness identifier (99)
    hdr1.endianness = 99;
    let err = verify_abi_compatibility(&hdr1, &hdr2).unwrap_err();
    assert_eq!(err.code, ErrorCode::AbiMismatch);
    assert!(
        err.message
            .contains("Invalid endianness in object A ABI header: 99")
    );
}

#[test]
fn test_elf_writer_big_endian_s390x_roundtrip() {
    let target = Target::from_triple("s390x-unknown-linux-gnu").expect("s390x target");
    assert_eq!(target.endianness, Endianness::Big);
    assert_eq!(target.arch, Arch::S390x);

    let mut text_sec =
        MergedSection::new(".text", SectionKind::Text, flags::EXEC | flags::ALLOC, 16);
    text_sec.data = vec![0x12, 0x34, 0x56, 0x78]; // dummy machine code
    text_sec.virtual_address = target.image_base + 0x1000;
    text_sec.size = 4;

    let sym = Symbol::new_defined(
        "main",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        text_sec.virtual_address,
        4,
        0,
    );

    let elf_bytes =
        ElfWriter::encode_executable(&target, text_sec.virtual_address, &[text_sec], &[sym], None)
            .expect("encode BE s390x ELF");

    // Verify raw header bytes:
    assert_eq!(&elf_bytes[0..4], &[0x7F, b'E', b'L', b'F']);
    assert_eq!(elf_bytes[4], 2); // ELFCLASS64
    assert_eq!(elf_bytes[5], 2); // ELFDATA2MSB (Big Endian)
    let e_machine = u16::from_be_bytes(elf_bytes[18..20].try_into().unwrap());
    assert_eq!(e_machine, EM_S390);

    // Round-trip parse back via ElfReader
    let parsed = ElfReader::read(&elf_bytes, Path::new("s390x_exec"), 0)
        .expect("ElfReader parses BE s390x image");
    assert_eq!(parsed.target.endianness, Endianness::Big);
    assert_eq!(parsed.target.arch, Arch::S390x);

    // Verify symbol in symbol table
    let main_sym = parsed
        .symbols
        .iter()
        .find(|s| s.name == "main")
        .expect("main symbol present");
    assert_eq!(main_sym.value, target.image_base + 0x1000);
}

#[test]
fn test_elf_writer_rejects_incompatible_endianness() {
    // 1. x86_64 with Big Endian must be rejected
    let mut x86_target = Target::from_triple("x86_64-unknown-linux-gnu").unwrap();
    x86_target.endianness = Endianness::Big;
    let err = ElfWriter::encode_executable(&x86_target, 0x1000, &[], &[], None).unwrap_err();
    assert_eq!(err.code, ErrorCode::ArchitectureMismatch);
    assert!(err.message.contains("does not support big-endian ELF"));

    // 2. s390x with Little Endian must be rejected
    let mut s390_target = Target::from_triple("s390x-unknown-linux-gnu").unwrap();
    s390_target.endianness = Endianness::Little;
    let err = ElfWriter::encode_executable(&s390_target, 0x1000, &[], &[], None).unwrap_err();
    assert_eq!(err.code, ErrorCode::ArchitectureMismatch);
    assert!(
        err.message
            .contains("s390x does not support little-endian ELF")
    );
}

#[test]
fn test_elf_writer_big_endian_aarch64_and_riscv_roundtrip() {
    // 1. AArch64 Big Endian
    let mut arm_target = Target::from_triple("aarch64-unknown-linux-gnu").unwrap();
    arm_target.endianness = Endianness::Big;
    let mut arm_text =
        MergedSection::new(".text", SectionKind::Text, flags::EXEC | flags::ALLOC, 16);
    arm_text.data = vec![0x1f, 0x20, 0x03, 0xd5]; // nop
    arm_text.virtual_address = arm_target.image_base + 0x1000;
    arm_text.size = 4;
    let arm_elf = ElfWriter::encode_executable(
        &arm_target,
        arm_text.virtual_address,
        &[arm_text],
        &[],
        None,
    )
    .expect("encode BE aarch64 ELF");
    assert_eq!(arm_elf[5], 2); // ELFDATA2MSB
    let arm_parsed =
        ElfReader::read(&arm_elf, Path::new("aarch64_be"), 0).expect("read BE aarch64");
    assert_eq!(arm_parsed.target.endianness, Endianness::Big);
    assert_eq!(arm_parsed.target.arch, Arch::AArch64);

    // 2. RISC-V 64 Big Endian
    let mut riscv_target = Target::from_triple("riscv64gc-unknown-linux-gnu").unwrap();
    riscv_target.endianness = Endianness::Big;
    let mut riscv_text =
        MergedSection::new(".text", SectionKind::Text, flags::EXEC | flags::ALLOC, 16);
    riscv_text.data = vec![0x13, 0x00, 0x00, 0x00]; // nop
    riscv_text.virtual_address = riscv_target.image_base + 0x1000;
    riscv_text.size = 4;
    let riscv_elf = ElfWriter::encode_executable(
        &riscv_target,
        riscv_text.virtual_address,
        &[riscv_text],
        &[],
        None,
    )
    .expect("encode BE riscv64 ELF");
    assert_eq!(riscv_elf[5], 2); // ELFDATA2MSB
    let riscv_parsed =
        ElfReader::read(&riscv_elf, Path::new("riscv64_be"), 0).expect("read BE riscv64");
    assert_eq!(riscv_parsed.target.endianness, Endianness::Big);
    assert_eq!(riscv_parsed.target.arch, Arch::Riscv64);
}
