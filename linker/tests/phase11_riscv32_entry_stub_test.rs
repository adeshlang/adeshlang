//! Phase 11 item 4, linker half: the RISC-V `_start` stub is RV64 code, so an
//! RV32 target must be refused instead of receiving a startup sequence its
//! hardware cannot execute.
//!
//! The codegen half of the same refusal lives in
//! `tests/native_phase11_codegen_loudness_test.rs`
//! (`riscv32_is_refused_at_backend_selection`).

use adesh_linker::config::LinkConfig;
use adesh_linker::linker::Linker;
use adesh_linker::object::ObjectFile;
use adesh_linker::object::writer::ObjectWriter;
use adesh_linker::section::Section;
use adesh_linker::symbol::{Symbol, SymbolBinding, SymbolType};
use adesh_linker::target::{Target, TargetTier};

/// An object that defines `main` but no `_start`, so `_start` has to be
/// synthesized by the linker — which is the only place the RV32 hazard lives.
fn written_main_object(dir: &tempfile::TempDir, target: &Target, name: &str) -> std::path::PathBuf {
    let mut obj = ObjectFile::new(dir.path().join(name), target.clone(), 0);
    obj.add_section(Section::new_code(".text", vec![0x13, 0x00, 0x00, 0x00], 4));
    obj.add_symbol(Symbol::new_defined(
        "main",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        4,
        0,
    ));
    let path = dir.path().join(name);
    ObjectWriter::write_to_file(&obj, &path).unwrap();
    path
}

#[test]
fn riscv32_static_executable_is_refused_instead_of_getting_an_rv64_entry_stub() {
    let dir = tempfile::tempdir().unwrap();
    let target = Target::from_triple("riscv32imac-unknown-none-elf").unwrap();
    assert_eq!(target.arch, adesh_linker::target::Arch::Riscv32);

    let obj_path = written_main_object(&dir, &target, "rv32-main.o");
    let out_path = dir.path().join("rv32-app");
    let config = LinkConfig::new(out_path.clone(), target);

    let error = Linker::link(&[obj_path], config)
        .expect_err("an RV32 executable needs an RV64 `addi`-style stub, so the link must fail");
    let message = format!("{error}");
    assert!(
        message.contains("_start"),
        "the refusal must name the startup symbol it cannot synthesize: {message}"
    );
    assert!(
        !out_path.exists(),
        "refusing must happen before any image is written: {:?}",
        out_path
    );
}

#[test]
fn riscv64_static_executable_still_gets_its_synthesized_entry_stub() {
    let dir = tempfile::tempdir().unwrap();
    let target = Target::from_triple("riscv64gc-unknown-none-elf").unwrap();

    let obj_path = written_main_object(&dir, &target, "rv64-main.o");
    let out_path = dir.path().join("rv64-app");
    let config = LinkConfig::new(out_path.clone(), target);

    assert!(
        Linker::link(&[obj_path], config).is_ok(),
        "RV64 entry synthesis is implemented and must keep working"
    );
    assert!(out_path.exists());
}

/// Phase 11 item 6 depends on this: the tier a target reports has to match the
/// verification the docs claim for it. RV32 has no backend and no entry stub, so
/// advertising it as Tier 2 ("Native Codegen & Object Linking Validated") is a
/// documentation contradiction the tier function still carries.
#[test]
fn riscv32_is_not_advertised_as_codegen_validated() {
    let target = Target::from_triple("riscv32imac-unknown-none-elf").unwrap();
    assert_ne!(
        target.tier(),
        TargetTier::Tier2Experimental,
        "riscv32 has no codegen backend and no entry stub; Tier 2 claims both"
    );
}
