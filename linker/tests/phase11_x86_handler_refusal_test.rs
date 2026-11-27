//! IA-32 relocation handling: 32-bit widths apply correctly, 64-bit widths are
//! refused. The x86-64 handler is deliberately not used for 32-bit images.

use adesh_linker::arch::get_handler;
use adesh_linker::relocation::{Relocation, RelocationKind};
use adesh_linker::target::Arch;

#[test]
fn i686_absolute32_applies_le_u32() {
    let handler = get_handler(Arch::X86);
    let mut image = vec![0u8; 16];
    let reloc = Relocation::new(0, "sym", RelocationKind::Absolute32, 0);
    handler
        .apply(&reloc, 0x1000, 0x2000, 0, &mut image)
        .expect("IA-32 Absolute32 must apply");
    assert_eq!(u32::from_le_bytes(image[0..4].try_into().unwrap()), 0x2000);
}

#[test]
fn i686_pc_relative32_applies_le_i32() {
    let handler = get_handler(Arch::X86);
    let mut image = vec![0u8; 16];
    // S = 0x2000, A = 0, P = 0x1000 => S + A - P = 0x1000
    let reloc = Relocation::new(0, "sym", RelocationKind::PcRelative32, 0);
    handler
        .apply(&reloc, 0x1000, 0x2000, 0, &mut image)
        .expect("IA-32 PcRelative32 must apply");
    assert_eq!(i32::from_le_bytes(image[0..4].try_into().unwrap()), 0x1000);
}

#[test]
fn i686_64bit_relocations_are_refused_and_leave_image_untouched() {
    let handler = get_handler(Arch::X86);
    for kind in [RelocationKind::Absolute64, RelocationKind::PcRelative64] {
        let mut image = vec![0u8; 16];
        let reloc = Relocation::new(0, "sym", kind, 0);
        let err = handler
            .apply(&reloc, 0x1000, 0x2000, 0, &mut image)
            .expect_err("64-bit relocation must be refused on IA-32");
        assert!(
            err.to_string().contains("IA-32") || err.to_string().contains("64-bit"),
            "error should name the illegal width: {err}"
        );
        assert!(
            image.iter().all(|b| *b == 0),
            "image must not be modified for {kind:?}"
        );
    }
}

#[test]
fn x86_64_still_applies_the_same_relocation() {
    let handler = get_handler(Arch::X86_64);
    let mut image = vec![0u8; 16];
    let reloc = Relocation::new(0, "sym", RelocationKind::PcRelative32, 0);
    handler
        .apply(&reloc, 0x1000, 0x1100, 0, &mut image)
        .unwrap();
    assert_eq!(i32::from_le_bytes(image[0..4].try_into().unwrap()), 0x100);
}

#[test]
fn pe32_i686_emission_is_refused_loudly() {
    use adesh_linker::error::ErrorCode;
    use adesh_linker::pe::writer::PeWriter;
    use adesh_linker::target::Target;

    let target = Target::from_triple("i686-pc-windows-msvc").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let out_path = dir.path().join("app.exe");
    let err = PeWriter::write_executable(&out_path, &target, 0x0040_1000, &[], &[], &[])
        .expect_err("PE32 emission must be refused until a real 32-bit PE header writer exists");
    assert_eq!(err.code, ErrorCode::UnsupportedFormat, "{err}");
    assert!(
        err.to_string().contains("PE32"),
        "expected explicit PE32 diagnostic: {err}"
    );
}

#[test]
fn elf32_dynamic_and_pie_are_refused_loudly() {
    use adesh_linker::elf::dynamic::ElfLinkInfo;
    use adesh_linker::elf::writer::ElfWriter;
    use adesh_linker::error::ErrorCode;
    use adesh_linker::target::Target;

    let target = Target::from_triple("i686-unknown-linux-gnu").unwrap();

    let err = ElfWriter::encode_elf(&target, 0x0804_9000, &[], &[], None, true, None)
        .expect_err("shared/dynamic ELF32 must fail loudly");
    assert_eq!(err.code, ErrorCode::UnsupportedFormat, "{err}");

    let mut pie_info = ElfLinkInfo::default();
    pie_info.pie = true;
    let err = ElfWriter::encode_elf(&target, 0x0804_9000, &[], &[], None, false, Some(&pie_info))
        .expect_err("PIE ELF32 must fail loudly");
    assert_eq!(err.code, ErrorCode::UnsupportedFormat, "{err}");
}
