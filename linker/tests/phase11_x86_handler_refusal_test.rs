//! An i686 link must not silently run the x86-64 relocation handler.

use adesh_linker::arch::get_handler;
use adesh_linker::relocation::{Relocation, RelocationKind};
use adesh_linker::target::Arch;

#[test]
fn x86_relocations_are_refused_and_leave_the_image_untouched() {
    let handler = get_handler(Arch::X86);
    for kind in [
        RelocationKind::Absolute32,
        RelocationKind::Absolute64,
        RelocationKind::PcRelative32,
    ] {
        let mut image = vec![0u8; 16];
        let reloc = Relocation::new(0, "sym", kind, 0);
        let err = handler
            .apply(&reloc, 0x1000, 0x2000, 0, &mut image)
            .expect_err("i686 relocation must be refused");
        assert!(
            err.to_string().contains("IA-32"),
            "error should name the missing handler: {err}"
        );
        assert!(image.iter().all(|b| *b == 0), "image modified for {kind:?}");
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
