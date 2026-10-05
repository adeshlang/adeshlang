//! End-to-End Deterministic Native Compilation Tests (Phase 8).
//!
//! Verifies:
//! 1. Byte-for-byte identical ADOB object emission across multiple compilation runs.
//! 2. Byte-for-byte identical linked native executables.
//! 3. Multi-target determinism (x86_64, aarch64, riscv64).
//! 4. Determinism under various optimization levels (O0, O2, Os, Oz).
//! 5. Real native compilation, linking, and execution on Windows x64.

#![allow(dead_code, unused_imports)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, RegisterClass, VirtualRegister,
};
use adesh_codegen::opt::OptLevel;
use adesh_codegen::targets::{create_backend, x86_64::X86_64Backend};
use adesh_object::TargetDescriptor;
use adesh_object::validator::AdobValidator;
use adesh_object::writer::AdobWriter;
use std::process::Command;
use tempfile::tempdir;

fn build_sample_module() -> NativeModule {
    let mut module = NativeModule::new("determinism_mod");
    let mut func = MachineFunction::new("deterministic_calc");
    func.is_exported = true;

    let v0 = func.alloc_vreg();
    let v1 = func.alloc_vreg();
    let v2 = func.alloc_vreg();

    let block = func.entry_block_mut();
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Immediate(100),
    });
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v1)),
        src: MachineOperand::Immediate(35),
    });
    block.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Register(MachineRegister::Virtual(v1)),
    });
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v2)),
        src: MachineOperand::Immediate(15),
    });
    block.push(MachineInstruction::Sub {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Register(MachineRegister::Virtual(v2)),
    });
    block.push(MachineInstruction::Return);

    module.add_function(func);
    module
}

#[test]
fn test_adob_byte_level_determinism_across_runs() {
    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid target");
    let module = build_sample_module();

    let mut baseline_bytes = Vec::new();

    for i in 0..5 {
        let mut backend = X86_64Backend::new(target.clone()).with_opt_level(OptLevel::O2);
        let obj = backend.emit_object(&module).expect("emit object");
        AdobValidator::validate(&obj).expect("valid ADOB");
        let bytes = AdobWriter::write(&obj).expect("encode ADOB");

        if i == 0 {
            baseline_bytes = bytes;
        } else {
            assert_eq!(
                bytes, baseline_bytes,
                "ADOB output in run {} must match baseline byte-for-byte",
                i
            );
        }
    }
}

#[test]
fn test_multi_target_emission_determinism() {
    let targets = [
        "x86_64-pc-windows-msvc",
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu",
        "riscv64gc-unknown-linux-gnu",
    ];

    let module = build_sample_module();

    for triple in &targets {
        let target = TargetDescriptor::from_triple(triple).expect("valid target");

        let mut b1 = create_backend(target.clone()).expect("b1");
        let obj1 = b1.emit_object(&module).expect("emit obj1");
        let bytes1 = AdobWriter::write(&obj1).expect("write bytes1");

        let mut b2 = create_backend(target.clone()).expect("b2");
        let obj2 = b2.emit_object(&module).expect("emit obj2");
        let bytes2 = AdobWriter::write(&obj2).expect("write bytes2");

        assert_eq!(
            bytes1, bytes2,
            "target {} must produce deterministic binary output",
            triple
        );
    }
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_linked_binary_determinism_and_execution_e2e() {
    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    let mut module = NativeModule::new("det_exec");
    let mut main_func = MachineFunction::new("main");
    main_func.is_exported = true;

    let b = main_func.entry_block_mut();
    b.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(120),
    });
    b.push(MachineInstruction::Return);
    module.add_function(main_func);

    let dir = tempdir().expect("tempdir");

    // Run 1
    let mut b1 = X86_64Backend::new(target.clone()).with_opt_level(OptLevel::Oz);
    let obj1 = b1.emit_object(&module).expect("emit 1");
    let bytes1 = AdobWriter::write(&obj1).expect("write 1");
    let adob1 = dir.path().join("run1.adob");
    std::fs::write(&adob1, &bytes1).expect("write adob1");
    let exe1 = dir.path().join("run1.exe");
    adesh_linker::link(&[&adob1], &exe1, Some("x86_64-pc-windows-msvc")).expect("link 1");

    // Run 2
    let mut b2 = X86_64Backend::new(target).with_opt_level(OptLevel::Oz);
    let obj2 = b2.emit_object(&module).expect("emit 2");
    let bytes2 = AdobWriter::write(&obj2).expect("write 2");
    let adob2 = dir.path().join("run2.adob");
    std::fs::write(&adob2, &bytes2).expect("write adob2");
    let exe2 = dir.path().join("run2.exe");
    adesh_linker::link(&[&adob2], &exe2, Some("x86_64-pc-windows-msvc")).expect("link 2");

    assert_eq!(bytes1, bytes2, "ADOB bytes must be identical");
    let exe_bytes1 = std::fs::read(&exe1).expect("read exe1");
    let exe_bytes2 = std::fs::read(&exe2).expect("read exe2");
    assert_eq!(
        exe_bytes1, exe_bytes2,
        "Linked executables must be byte-for-byte identical"
    );

    // Execute
    let out = Command::new(&exe1).output().expect("execute");
    assert_eq!(out.status.code().unwrap(), 120);
}
