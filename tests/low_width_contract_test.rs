//! Phase 0 contract test — low-width architecture truth gate.
//!
//! Pins the *current* refusals so that any later phase adding 32/24/16/8-bit
//! support must intentionally change these assertions. Every refusal below is
//! the documented, audited behavior (see `LOW_WIDTH_ARCHITECTURE_PLAN.md`
//! §2): RV32 is refused at backend selection; the i686 linker handler refuses
//! relocations with `IA-32` and leaves the image untouched; Thumb function
//! bodies are refused by the embedded backend. i686 codegen is now *accepted*
//! at backend selection (Phase 2) but refuses non-empty function bodies
//! loudly.
//!
//! Memory-safe (`#![forbid(unsafe_code)]`), type-safe, no heap in the
//! assertion path, minimal executable size. These tests run once per suite
//! and assert structure, not performance.

#![forbid(unsafe_code)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::machine_ir::{
    MachineFunction, MachineInstruction, MachineOperand, MachineRegister, VirtualRegister,
};
use adesh_codegen::targets::{create_backend, embedded::EmbeddedBackend};
use adesh_linker::{
    arch::get_handler,
    relocation::{Relocation, RelocationKind},
    target::Arch,
};
use adesh_object::TargetDescriptor;

/// IA-32 must be accepted by the native backend factory (Phase 2) — i686 now
/// gets a real backend instead of being silently lowered with x86-64/RV64
/// semantics. Non-empty function bodies are still refused loudly by the
/// backend itself (pinned in `crates/adesh-codegen/src/targets/i686/mod.rs`).
#[test]
fn i686_is_accepted_at_backend_selection() {
    let target =
        TargetDescriptor::from_triple("i686-unknown-linux-gnu").expect("i686 triple must parse");
    let _backend = match create_backend(target) {
        Ok(b) => b,
        Err(e) => panic!("i686 must get a native backend: {}", e.reason),
    };
}

/// RV32 must be refused by the native backend factory — the backend is RV64
/// only and would otherwise emit doubleword instructions RV32 hardware cannot
/// run.
#[test]
fn riscv32_is_refused_at_backend_selection() {
    let target = TargetDescriptor::from_triple("riscv32imac-unknown-none-elf")
        .expect("riscv32 triple must parse");
    let err = match create_backend(target) {
        Err(e) => e,
        Ok(_) => panic!("riscv32 must not get a native backend"),
    };
    assert!(
        err.reason.contains("RV64") || err.reason.contains("RV32"),
        "riscv32 refusal should name the RV64-only backend: {}",
        err.reason
    );
}

/// i686 linker relocations apply 32-bit semantics (little-endian u32 / i32)
/// and refuse 64-bit widths — no silent x86-64 application to a 32-bit image.
#[test]
fn i686_absolute32_applies_le_u32() {
    let handler = get_handler(Arch::X86);
    let mut image = vec![0u8; 16];
    let reloc = Relocation::new(0, "sym", RelocationKind::Absolute32, 0);
    handler
        .apply(&reloc, 0x1000, 0x2000, 0, &mut image)
        .expect("IA-32 Absolute32 must apply");
    assert_eq!(
        u32::from_le_bytes(image[0..4].try_into().unwrap()),
        0x2000,
        "Absolute32 must write symbol_va + addend as LE u32"
    );
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
    assert_eq!(
        i32::from_le_bytes(image[0..4].try_into().unwrap()),
        0x1000,
        "PcRelative32 must write S + A - P as LE i32"
    );
}

/// i686 64-bit relocations are refused on IA-32 images and leave the image
/// untouched — never apply 64-bit semantics to 32-bit code.
#[test]
fn i686_absolute64_refused_on_ia32() {
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

/// Thumb embedded backend must refuse any function body (only `Nop`/`Return`
/// are allowed) — never emit Thumb bytes for logic it cannot encode.
#[test]
fn thumb_function_bodies_are_loud_errors() {
    let target =
        TargetDescriptor::from_triple("thumbv7em-none-eabihf").expect("thumb triple must parse");
    let mut backend = EmbeddedBackend::new(target);

    let mut func = MachineFunction::new("boom");
    func.blocks[0].push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(VirtualRegister(0))),
        src: MachineOperand::Immediate(1),
    });
    func.blocks[0].push(MachineInstruction::Return);

    let err = backend
        .generate_function(&func)
        .expect_err("thumb bodies must be refused");
    assert!(
        err.reason.contains("not implemented") || err.reason.contains("Move"),
        "thumb body refusal should name the instruction: {}",
        err.reason
    );
}

/// x86-64 still applies relocations normally — the refusal is architecture
/// specific, not a global handler breakage.
#[test]
fn x86_64_relocation_still_applies() {
    let handler = get_handler(Arch::X86_64);
    let mut image = vec![0u8; 16];
    let reloc = Relocation::new(0, "sym", RelocationKind::PcRelative32, 0);
    handler
        .apply(&reloc, 0x1000, 0x1100, 0, &mut image)
        .expect("x86-64 relocation must apply");
    assert_eq!(
        i32::from_le_bytes(image[0..4].try_into().unwrap()),
        0x100,
        "x86-64 pc-relative fixup must be computed"
    );
}
