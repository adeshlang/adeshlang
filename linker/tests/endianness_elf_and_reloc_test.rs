use adesh_linker::abi::{
    ADESH_ABI_ENDIAN_BIG, ADESH_ABI_ENDIAN_LITTLE, AdeshAbiHeader, verify_abi_compatibility,
};
use adesh_linker::arch::get_handler;
use adesh_linker::elf::header::{EM_386, EM_S390};
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

#[test]
fn test_elf32_i686_roundtrip() {
    let target = Target::from_triple("i686-unknown-linux-gnu").expect("i686 target");
    assert_eq!(target.arch, Arch::X86);
    assert_eq!(target.pointer_width.bits(), 32);

    let mut text_sec =
        MergedSection::new(".text", SectionKind::Text, flags::EXEC | flags::ALLOC, 16);
    text_sec.data = vec![0xC3];
    text_sec.virtual_address = target.image_base + 0x1000;
    text_sec.size = 1;

    let sym = Symbol::new_defined(
        "_start",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        text_sec.virtual_address,
        1,
        0,
    );

    let bytes =
        ElfWriter::encode_executable(&target, text_sec.virtual_address, &[text_sec], &[sym], None)
            .expect("encode ELF32 i686 image");

    assert_eq!(&bytes[0..4], &[0x7F, b'E', b'L', b'F']);
    assert_eq!(bytes[4], 1, "ELFCLASS32");
    assert_eq!(bytes[5], 1, "ELFDATA2LSB");
    let e_machine = u16::from_le_bytes(bytes[18..20].try_into().unwrap());
    assert_eq!(e_machine, EM_386);

    let parsed = ElfReader::read(&bytes, Path::new("i686_exec"), 0).expect("read ELF32 i686 image");
    assert_eq!(parsed.target.arch, Arch::X86);
    assert_eq!(parsed.target.pointer_width.bits(), 32);
    let start = parsed
        .symbols
        .iter()
        .find(|s| s.name == "_start")
        .expect("_start symbol present");
    assert_eq!(start.value, target.image_base + 0x1000);
    assert_eq!(
        u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
        start.value as u32
    );
    assert_eq!(bytes[0x1000], 0xC3, "entry bytes must match entry VA");
}

fn build_i686_relocatable(reloc_type: u32, addend: i32) -> Vec<u8> {
    let mut elf = vec![0u8; 52];
    elf[..7].copy_from_slice(&[0x7F, b'E', b'L', b'F', 1, 1, 1]);
    elf[16..18].copy_from_slice(&1u16.to_le_bytes());
    elf[18..20].copy_from_slice(&EM_386.to_le_bytes());
    elf[20..24].copy_from_slice(&1u32.to_le_bytes());
    elf[40..42].copy_from_slice(&52u16.to_le_bytes());
    elf[46..48].copy_from_slice(&40u16.to_le_bytes());
    elf[48..50].copy_from_slice(&6u16.to_le_bytes());
    elf[50..52].copy_from_slice(&5u16.to_le_bytes());

    let text_offset = elf.len() as u32;
    elf.extend_from_slice(&[0xE8]);
    elf.extend_from_slice(&addend.to_le_bytes());
    let rel_offset = elf.len() as u32;
    elf.extend_from_slice(&1u32.to_le_bytes());
    elf.extend_from_slice(&((1u32 << 8) | reloc_type).to_le_bytes());
    let symtab_offset = elf.len() as u32;
    elf.extend_from_slice(&[0u8; 16]);
    elf.extend_from_slice(&1u32.to_le_bytes());
    let symbol_info = elf.len() - 12;
    elf[symbol_info] = 0x10;
    let strtab_offset = elf.len() as u32;
    elf.extend_from_slice(b"\0target\0");
    let shstrtab_offset = elf.len() as u32;
    elf.extend_from_slice(b"\0.text\0.rel.text\0.symtab\0.strtab\0.shstrtab\0");
    let section_table = elf.len() as u32;
    elf[32..36].copy_from_slice(&section_table.to_le_bytes());

    let names = [0, 1, 7, 17, 25, 33];
    let sections = [
        [0u32; 10],
        [1, 1, 6, 0, text_offset, 5, 0, 0, 1, 0],
        [7, 9, 0, 3, rel_offset, 8, 3, 1, 4, 8],
        [17, 2, 0, 4, symtab_offset, 32, 3, 1, 4, 16],
        [25, 3, 0, 0, strtab_offset, 8, 0, 0, 1, 0],
        [33, 3, 0, 0, shstrtab_offset, 43, 0, 0, 1, 0],
    ];
    for (index, section) in sections.into_iter().enumerate() {
        let mut header = [0u8; 40];
        for (field, value) in header.chunks_exact_mut(4).zip(section) {
            field.copy_from_slice(&value.to_le_bytes());
        }
        header[..4].copy_from_slice(&(names[index] as u32).to_le_bytes());
        elf.extend_from_slice(&header);
    }
    elf
}

#[test]
fn i686_relocations_are_decoded_by_type_and_unsupported_forms_fail_loudly() {
    for (reloc_type, expected, addend) in [
        (1, RelocationKind::Absolute32, 0),
        (2, RelocationKind::PcRelative32, -4),
        (3, RelocationKind::X86Got32, -4),
        (4, RelocationKind::PltRelative32, -4),
        (9, RelocationKind::X86GotOff32, 0),
        (10, RelocationKind::X86GotPc32, -4),
        (20, RelocationKind::Absolute16, 0),
        (22, RelocationKind::Absolute8, 0),
        (43, RelocationKind::X86Got32, -4),
    ] {
        let object = ElfReader::read(
            &build_i686_relocatable(reloc_type, addend),
            Path::new("i686_reloc.o"),
            0,
        )
        .expect("supported IA-32 relocation");
        let relocation_section = object
            .sections
            .iter()
            .find(|section| !section.relocations.is_empty())
            .expect("relocation is attached to its target section");
        assert_eq!(relocation_section.relocations[0].kind, expected);
        assert_eq!(relocation_section.relocations[0].addend, i64::from(addend));
    }
    for reloc_type in [14, 99] {
        let error = ElfReader::read(
            &build_i686_relocatable(reloc_type, -4),
            Path::new("i686_reloc.o"),
            0,
        )
        .expect_err("unsupported IA-32 relocation must not become Absolute32");
        assert_eq!(error.code, ErrorCode::UnsupportedRelocation);
    }
}

#[test]
fn test_elf32_i686_with_build_id_roundtrip() {
    let target = Target::from_triple("i686-unknown-linux-gnu").expect("i686 target");

    let mut text_sec =
        MergedSection::new(".text", SectionKind::Text, flags::EXEC | flags::ALLOC, 16);
    text_sec.data = vec![0xC3];
    text_sec.virtual_address = target.image_base + 0x1000;
    text_sec.size = 1;

    let build_id = [0xAAu8; 20];
    let bytes = ElfWriter::encode_executable(
        &target,
        text_sec.virtual_address,
        &[text_sec.clone()],
        &[],
        Some(&build_id),
    )
    .expect("encode ELF32 i686 image with build-id");

    assert_eq!(bytes[4], 1, "ELFCLASS32");
    let e_phnum = u16::from_le_bytes(bytes[44..46].try_into().unwrap());
    assert_eq!(e_phnum, 4, "PT_NOTE adds one program header when no RW PT_LOAD is needed");

    let mut data_sec =
        MergedSection::new(".data", SectionKind::Data, flags::READ | flags::WRITE | flags::ALLOC, 4);
    data_sec.data = vec![1, 2, 3, 4];
    data_sec.virtual_address = target.image_base + 0x2000;
    data_sec.size = 4;
    let bytes_rw = ElfWriter::encode_executable(
        &target,
        text_sec.virtual_address,
        &[text_sec, data_sec],
        &[],
        Some(&build_id),
    )
    .expect("encode ELF32 i686 image with .data and build-id");
    let e_phnum_rw = u16::from_le_bytes(bytes_rw[44..46].try_into().unwrap());
    assert_eq!(e_phnum_rw, 5, "PT_NOTE adds one program header alongside RW PT_LOAD");

    let parsed = ElfReader::read(&bytes, Path::new("i686_buildid"), 0)
        .expect("read ELF32 i686 image with build-id");
    assert_eq!(parsed.target.arch, Arch::X86);
}
