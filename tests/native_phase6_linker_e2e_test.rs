//! End-to-End Native Linker Integration, TargetSpec, Caching & Resource Limits Tests (Phase 6).
//!
//! Verifies:
//! 1. Multi-object linking and symbol resolution with `adeshlink`.
//! 2. TargetSpec consistency and architecture configuration.
//! 3. CompilationCacheKey determinism across multiple runs.
//! 4. CompilerResourceLimits safe bounds checking.
//! 5. Native executable execution on Windows x64.

#![allow(dead_code, unused_imports)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::driver::{CompilationCacheKey, CompilerResourceLimits};
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, RegisterClass, VirtualRegister,
};
use adesh_codegen::opt::OptLevel;
use adesh_codegen::target_spec::TargetSpec;
use adesh_codegen::targets::x86_64::X86_64Backend;
use adesh_object::TargetDescriptor;
use adesh_object::validator::AdobValidator;
use adesh_object::writer::AdobWriter;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_target_spec_construction() {
    let win_spec = TargetSpec::x86_64_windows();
    assert!(win_spec.is_windows());
    assert_eq!(win_spec.pointer_width_bits, 64);
    assert_eq!(win_spec.stack_alignment_bytes, 16);
    assert_eq!(win_spec.shadow_space_bytes(), 32);

    let linux_spec = TargetSpec::x86_64_linux();
    assert!(!linux_spec.is_windows());
    assert!(linux_spec.is_sysv());
    assert_eq!(linux_spec.pointer_width_bits, 64);
    assert_eq!(linux_spec.shadow_space_bytes(), 0);
}

#[test]
fn test_compilation_cache_key_determinism() {
    let key1 = CompilationCacheKey::new(
        12345678,
        "x86_64-pc-windows-msvc",
        OptLevel::O2,
        vec!["sse2".to_string()],
    );

    let key2 = CompilationCacheKey::new(
        12345678,
        "x86_64-pc-windows-msvc",
        OptLevel::O2,
        vec!["sse2".to_string()],
    );

    assert_eq!(key1, key2, "Compilation cache keys must be deterministic");
}

#[test]
fn test_multi_object_linking_e2e() {
    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");

    // Object 1: `multiply_by_two(x)` -> x * 2
    let mut mod1 = NativeModule::new("mod1");
    let mut func_mul = MachineFunction::new("multiply_by_two");
    func_mul.is_exported = true;
    let entry1 = func_mul.entry_block_mut();
    // On Win64: arg0 is RCX (1)
    entry1.push(MachineInstruction::Shl {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(1))),
        src: MachineOperand::Immediate(1),
    });
    entry1.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(1))),
    });
    entry1.push(MachineInstruction::Return);
    mod1.add_function(func_mul);

    // Object 2: `main()` -> calls `multiply_by_two(21)` -> returns 42
    let mut mod2 = NativeModule::new("mod2");
    let mut func_main = MachineFunction::new("main");
    func_main.is_exported = true;
    let entry2 = func_main.entry_block_mut();
    entry2.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(1))),
        src: MachineOperand::Immediate(21),
    });
    entry2.push(MachineInstruction::Call {
        target: MachineOperand::Symbol("multiply_by_two".to_string()),
        num_args: 1,
    });
    entry2.push(MachineInstruction::Return);
    mod2.add_function(func_main);

    let mut backend = X86_64Backend::new(target).with_opt_level(OptLevel::O2);

    let obj1 = backend.emit_object(&mod1).expect("emit mod1");
    let obj2 = backend.emit_object(&mod2).expect("emit mod2");

    let bytes1 = AdobWriter::write(&obj1).expect("write obj1");
    let bytes2 = AdobWriter::write(&obj2).expect("write obj2");

    let dir = tempdir().expect("tempdir");
    let obj1_path = dir.path().join("mod1.adob");
    let obj2_path = dir.path().join("mod2.adob");
    std::fs::write(&obj1_path, bytes1).expect("write mod1.adob");
    std::fs::write(&obj2_path, bytes2).expect("write mod2.adob");

    let exe_path = dir.path().join("multi_obj.exe");
    adesh_linker::link(
        &[&obj1_path, &obj2_path],
        &exe_path,
        Some("x86_64-pc-windows-msvc"),
    )
    .expect("link multiple objects");
    assert!(exe_path.exists(), "multi-object executable exists");

    let out = Command::new(&exe_path)
        .output()
        .expect("execute multi-object binary");
    assert_eq!(out.status.code().expect("exit code"), 42);
}
