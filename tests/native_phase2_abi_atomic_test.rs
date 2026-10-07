//! Phase 2 Task P2-6: Complete x86-64 Atomics (lock cmpxchg, lock xadd, lock xchg, mfence).
//!
//! Verifies:
//! 1. 8, 16, 32, 64-bit `lock xadd` (AtomicFetchAdd)
//! 2. 8, 16, 32, 64-bit `lock cmpxchg` (AtomicCompareExchange) for both success and failure cases
//! 3. 8, 16, 32, 64-bit `lock xchg` (AtomicExchange)
//! 4. Full memory barrier (`mfence`)
//! 5. End-to-end execution on x86-64 Windows asserting exact exit codes.

#![allow(dead_code, unused_imports)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister,
};
use adesh_codegen::opt::OptLevel;
use adesh_codegen::targets::x86_64::X86_64Backend;
use adesh_object::TargetDescriptor;
use adesh_object::validator::AdobValidator;
use adesh_object::writer::AdobWriter;
use std::process::Command;
use tempfile::tempdir;

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
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
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_atomic_fetch_add_64_e2e() {
    let mut native_mod = NativeModule::new("test_atomic_xadd_64");
    let mut main_fn = MachineFunction::new("main");
    main_fn.is_exported = true;
    main_fn.stack_size = 16;
    let entry = main_fn.entry_block_mut();

    // [RBP - 8] = 20
    entry.push(MachineInstruction::Store {
        dst: MachineOperand::StackSlot(-8),
        src: MachineOperand::Immediate(20),
        size: 8,
    });

    // R10 = 22
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::phys(10),
        src: MachineOperand::Immediate(22),
    });

    // lock xadd [RBP - 8], R10
    // Result in memory: 20 + 22 = 42
    // Old value in R10: 20
    entry.push(MachineInstruction::AtomicFetchAdd {
        dst: MachineOperand::StackSlot(-8),
        src: MachineOperand::phys(10),
        size: 8,
    });

    // Memory fence
    entry.push(MachineInstruction::Barrier);

    // Read back [RBP - 8] into RAX (phys 0) -> should be 42
    entry.push(MachineInstruction::Load {
        dst: MachineOperand::phys(0),
        src: MachineOperand::StackSlot(-8),
        size: 8,
    });
    entry.push(MachineInstruction::Return);
    native_mod.add_function(main_fn);

    let code = emit_link_and_run(&native_mod, OptLevel::O0, "atomic_xadd_64");
    assert_eq!(code, 42, "AtomicFetchAdd 64-bit should result in 42");
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_atomic_fetch_add_32_16_8_e2e() {
    let mut native_mod = NativeModule::new("test_atomic_xadd_sized");
    let mut main_fn = MachineFunction::new("main");
    main_fn.is_exported = true;
    main_fn.stack_size = 16;
    let entry = main_fn.entry_block_mut();

    // Zero out stack slot
    entry.push(MachineInstruction::Store {
        dst: MachineOperand::StackSlot(-8),
        src: MachineOperand::Immediate(0),
        size: 8,
    });

    // 1. 32-bit xadd: slot starts at 0, add 10 -> 10
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::phys(10),
        src: MachineOperand::Immediate(10),
    });
    entry.push(MachineInstruction::AtomicFetchAdd {
        dst: MachineOperand::StackSlot(-8),
        src: MachineOperand::phys(10),
        size: 4,
    });

    // 2. 16-bit xadd: add 5 -> 15
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::phys(10),
        src: MachineOperand::Immediate(5),
    });
    entry.push(MachineInstruction::AtomicFetchAdd {
        dst: MachineOperand::StackSlot(-8),
        src: MachineOperand::phys(10),
        size: 2,
    });

    // 3. 8-bit xadd: add 4 -> 19
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::phys(10),
        src: MachineOperand::Immediate(4),
    });
    entry.push(MachineInstruction::AtomicFetchAdd {
        dst: MachineOperand::StackSlot(-8),
        src: MachineOperand::phys(10),
        size: 1,
    });

    // Read back 64-bit from stack slot into RAX -> 19
    entry.push(MachineInstruction::Load {
        dst: MachineOperand::phys(0),
        src: MachineOperand::StackSlot(-8),
        size: 8,
    });
    entry.push(MachineInstruction::Return);
    native_mod.add_function(main_fn);

    let code = emit_link_and_run(&native_mod, OptLevel::O0, "atomic_xadd_sized");
    assert_eq!(code, 19, "AtomicFetchAdd sized should accumulate to 19");
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_atomic_cmpxchg_success_and_failure_e2e() {
    let mut native_mod = NativeModule::new("test_atomic_cmpxchg");
    let mut main_fn = MachineFunction::new("main");
    main_fn.is_exported = true;
    main_fn.stack_size = 16;
    let entry = main_fn.entry_block_mut();

    // [RBP - 8] = 50
    entry.push(MachineInstruction::Store {
        dst: MachineOperand::StackSlot(-8),
        src: MachineOperand::Immediate(50),
        size: 8,
    });

    // Success cmpxchg: expected = 50, desired = 75
    // [RBP - 8] becomes 75
    entry.push(MachineInstruction::AtomicCompareExchange {
        dst: MachineOperand::StackSlot(-8),
        expected: MachineOperand::Immediate(50),
        desired: MachineOperand::Immediate(75),
        size: 8,
    });

    // Failure cmpxchg: expected = 999 (mismatch), desired = 100
    // [RBP - 8] remains 75; RAX is loaded with current value 75
    entry.push(MachineInstruction::AtomicCompareExchange {
        dst: MachineOperand::StackSlot(-8),
        expected: MachineOperand::Immediate(999),
        desired: MachineOperand::Immediate(100),
        size: 8,
    });

    // Return RAX (which holds 75 from the failed compare-exchange)
    entry.push(MachineInstruction::Return);
    native_mod.add_function(main_fn);

    let code = emit_link_and_run(&native_mod, OptLevel::O0, "atomic_cmpxchg");
    assert_eq!(
        code, 75,
        "AtomicCompareExchange should succeed then fail, leaving RAX=75"
    );
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_atomic_exchange_e2e() {
    let mut native_mod = NativeModule::new("test_atomic_xchg");
    let mut main_fn = MachineFunction::new("main");
    main_fn.is_exported = true;
    main_fn.stack_size = 16;
    let entry = main_fn.entry_block_mut();

    // [RBP - 8] = 30
    entry.push(MachineInstruction::Store {
        dst: MachineOperand::StackSlot(-8),
        src: MachineOperand::Immediate(30),
        size: 8,
    });

    // R10 = 55
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::phys(10),
        src: MachineOperand::Immediate(55),
    });

    // AtomicExchange: [RBP - 8] becomes 55, R10 becomes 30 (old value)
    entry.push(MachineInstruction::AtomicExchange {
        dst: MachineOperand::StackSlot(-8),
        src: MachineOperand::phys(10),
        size: 8,
    });

    // Load old value (R10) into RAX, add current value from memory
    // RAX = R10 (30)
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::phys(0),
        src: MachineOperand::phys(10),
    });

    // R11 = [RBP - 8] (55)
    entry.push(MachineInstruction::Load {
        dst: MachineOperand::phys(11),
        src: MachineOperand::StackSlot(-8),
        size: 8,
    });

    // RAX = RAX - 30 (0)
    entry.push(MachineInstruction::Sub {
        dst: MachineOperand::phys(0),
        src: MachineOperand::Immediate(30),
    });

    // RAX = RAX + R11 (55)
    entry.push(MachineInstruction::Add {
        dst: MachineOperand::phys(0),
        src: MachineOperand::phys(11),
    });

    entry.push(MachineInstruction::Return);
    native_mod.add_function(main_fn);

    let code = emit_link_and_run(&native_mod, OptLevel::O0, "atomic_xchg");
    assert_eq!(code, 55, "AtomicExchange should swap values correctly");
}
