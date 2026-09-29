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
