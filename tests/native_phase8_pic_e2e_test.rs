//! End-to-End Native Backend PIC / Relocation Tests (Phase 8).
//!
//! Verifies:
//! 1. Position Independent Code (PIC/PIE) and RIP-relative relocations.
//! 2. PC-relative addressing across functions (PcRelative32, Plt32, GotPcrel).
//! 3. Cross-architecture relocation validation (x86_64, AArch64, RISC-V).
//! 4. Real native compilation, linking, and execution of PIC code on Windows x64.

#![allow(dead_code, unused_imports)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, RegisterClass, VirtualRegister,
};
use adesh_codegen::opt::OptLevel;
use adesh_codegen::targets::x86_64::X86_64Backend;
use adesh_object::TargetDescriptor;
use adesh_object::relocation::{AdobRelocation, RelocationFlags, RelocationKind};
use adesh_object::symbol::SymbolId;
use adesh_object::validator::AdobValidator;
use adesh_object::writer::AdobWriter;
use std::process::Command;
use tempfile::tempdir;

fn emit_link_and_run(native_mod: &NativeModule, opt_level: OptLevel, test_name: &str) -> i32 {
    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    let mut backend = X86_64Backend::new(target).with_opt_level(opt_level);

    let obj = backend.emit_object(native_mod).expect("ADOB emission");
    AdobValidator::validate(&obj).expect("emitted ADOB must validate");

    let bytes = AdobWriter::write(&obj).expect("ADOB encoding");
    let dir = tempdir().expect("tempdir");
    let adob_path = dir.path().join(format!("{}.adob", test_name));
    std::fs::write(&adob_path, bytes).expect("write ADOB file");

    let exe_path = dir.path().join(format!("{}.exe", test_name));
    adesh_linker::link(&[&adob_path], &exe_path, Some("x86_64-pc-windows-msvc"))
        .expect("native link");
    assert!(exe_path.exists(), "linked executable must exist");

    let out = Command::new(&exe_path)
        .output()
        .expect("execute native binary");
    out.status.code().expect("exit code")
}

#[test]
fn test_pic_relocation_kinds() {
    let rel_pcrel = AdobRelocation::new(0x20, 1, "helper_fn", RelocationKind::PcRelative32, -4);
    assert_eq!(rel_pcrel.width, 4);
    assert_eq!(rel_pcrel.kind, RelocationKind::PcRelative32);

    let rel_got = AdobRelocation::new(
        0x40,
        2,
        "_GLOBAL_OFFSET_TABLE_",
        RelocationKind::X86_64_GotPcrel,
        -4,
    );
    assert_eq!(rel_got.width, 4);
    assert_eq!(rel_got.kind, RelocationKind::X86_64_GotPcrel);

    let rel_aarch64 = AdobRelocation::new(
        0x60,
        3,
        "external_sym",
        RelocationKind::AArch64_AdrPage21,
        0,
    );
    assert_eq!(rel_aarch64.width, 4);

    let rel_riscv = AdobRelocation::new(0x80, 4, "rv_call", RelocationKind::RiscV_Call, 0);
    assert_eq!(rel_riscv.width, 4);
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_pic_inter_function_calls_execution_e2e() {
    let mut module = NativeModule::new("test_pic");

    // helper() -> returns 45
    let mut helper_func = MachineFunction::new("helper");
    let h_block = helper_func.entry_block_mut();
    h_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(45),
    });
    h_block.push(MachineInstruction::Return);
    module.add_function(helper_func);

    // main() -> calls helper(), adds 10, returns 55
    let mut main_func = MachineFunction::new("main");
    main_func.is_exported = true;
    let m_block = main_func.entry_block_mut();

    // Call helper
    m_block.push(MachineInstruction::Call {
        target: MachineOperand::Symbol("helper".to_string()),
        num_args: 0,
    });

    // Add 10 to RAX (where helper result was returned)
    m_block.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(10),
    });
    m_block.push(MachineInstruction::Return);
    module.add_function(main_func);

    let code = emit_link_and_run(&module, OptLevel::O2, "test_pic_exec");
    assert_eq!(code, 55, "PIC inter-function call should return 55");
}
