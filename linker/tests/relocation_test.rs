use adesh_linker::arch::get_handler;
use adesh_linker::relocation::{Relocation, RelocationKind};
use adesh_linker::target::Arch;

#[test]
fn test_x86_64_absolute64_relocation() {
    let handler = get_handler(Arch::X86_64);
    let mut image = vec![0u8; 16];
    let reloc = Relocation::new(0, "test_sym", RelocationKind::Absolute64, 8);

    let res = handler.apply(&reloc, 0x1000, 0x400000, 8, &mut image);
    assert!(res.is_ok());

    let val = u64::from_le_bytes(image[0..8].try_into().unwrap());
    assert_eq!(val, 0x400008);
}

#[test]
fn test_x86_64_pc32_relocation() {
    let handler = get_handler(Arch::X86_64);
    let mut image = vec![0u8; 16];
    let reloc = Relocation::new(0, "target_func", RelocationKind::PcRelative32, 0);

    // place = 0x1000, symbol = 0x1100 -> offset = +0x100
    let res = handler.apply(&reloc, 0x1000, 0x1100, 0, &mut image);
    assert!(res.is_ok());

    let val = i32::from_le_bytes(image[0..4].try_into().unwrap());
    assert_eq!(val, 0x100);
}

#[test]
fn test_absolute32_bounds_and_overflow() {
    let handler = get_handler(Arch::X86_64);
    let mut image = vec![0u8; 8];

    // Valid u32 max
    let reloc = Relocation::new(0, "u32_max", RelocationKind::Absolute32, 0);
    assert!(
        handler
            .apply(&reloc, 0x1000, u32::MAX as u64, 0, &mut image)
            .is_ok()
    );
    assert_eq!(
        u32::from_le_bytes(image[0..4].try_into().unwrap()),
        u32::MAX
    );

    // Valid signed negative i32 min (-0x8000_0000)
    let reloc = Relocation::new(0, "i32_min", RelocationKind::Absolute32, i32::MIN as i64);
    assert!(
        handler
            .apply(&reloc, 0x1000, 0, i32::MIN as i64, &mut image)
            .is_ok()
    );
    assert_eq!(
        i32::from_le_bytes(image[0..4].try_into().unwrap()),
        i32::MIN
    );

    // Overflow above Mach-O 64-bit image range (0x2_0000_0000)
    let reloc = Relocation::new(0, "overflow_hi", RelocationKind::Absolute32, 0);
    assert!(
        handler
            .apply(&reloc, 0x1000, 0x2_0000_0000, 0, &mut image)
            .is_err()
    );

    // Underflow below i32::MIN (-0x8000_0001)
    let reloc = Relocation::new(
        0,
        "underflow_lo",
        RelocationKind::Absolute32,
        i32::MIN as i64 - 1,
    );
    assert!(
        handler
            .apply(&reloc, 0x1000, 0, i32::MIN as i64 - 1, &mut image)
            .is_err()
    );
}

#[test]
fn test_aarch64_call26_relocation() {
    let handler = get_handler(Arch::AArch64);
    let mut image = vec![0x00, 0x00, 0x00, 0x94]; // BL opcode with 0 imm
    let reloc = Relocation::new(0, "aarch64_func", RelocationKind::AArch64Call26, 0);

    // place = 0x1000, symbol = 0x1040 (offset = 0x40 -> imm26 = 0x40 >> 2 = 16)
    let res = handler.apply(&reloc, 0x1000, 0x1040, 0, &mut image);
    assert!(res.is_ok());

    let insn = u32::from_le_bytes(image[0..4].try_into().unwrap());
    assert_eq!(insn & 0x03FF_FFFF, 16);
}
