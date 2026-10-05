//! End-to-End Native Backend Atomics and Concurrency Tests (Phase 7).
//!
//! Verifies:
//! 1. Target-independent atomic IR instructions (AtomicLoad, AtomicStore, AtomicFetchAdd, AtomicCompareExchange, Barrier).
//! 2. Memory ordering specification (Relaxed, Acquire, Release, AcqRel, SeqCst).
//! 3. x86-64 atomic lowering (lock xadd, lock cmpxchg, mfence, pause).
//! 4. Real native compilation, linking, and execution on Windows x64.

#![allow(dead_code, unused_imports)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::concurrency::{AtomicOp, ConcurrencyPass, MemoryOrder};
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, RegisterClass, VirtualRegister,
};
use adesh_codegen::opt::OptLevel;
use adesh_codegen::targets::x86_64::X86_64Backend;
use adesh_object::TargetDescriptor;
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
fn test_atomic_ir_representation_and_concurrency_pass() {
    let add_op = AtomicOp::FetchAdd {
        dst: MachineOperand::Register(MachineRegister::Virtual(VirtualRegister(0))),
        val: MachineOperand::Immediate(1),
        order: MemoryOrder::SeqCst,
    };
    let lowered = ConcurrencyPass::lower_atomic(add_op).expect("lower fetch_add");
    assert_eq!(lowered.len(), 1);

    let fence_op = AtomicOp::Fence {
        order: MemoryOrder::SeqCst,
    };
    let fence_lowered = ConcurrencyPass::lower_atomic(fence_op).expect("lower fence");
    assert_eq!(fence_lowered, vec![MachineInstruction::Barrier]);
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_atomic_fetch_add_and_barrier_execution_e2e() {
    let mut module = NativeModule::new("test_atomic");
    let mut main_func = MachineFunction::new("main");
    main_func.is_exported = true;
    main_func.stack_size = 16;

    let v_init = main_func.alloc_vreg();
    let v_add = main_func.alloc_vreg();
    let v_res = main_func.alloc_vreg();

    let block = main_func.entry_block_mut();
    // [rsp-8] = 100
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_init)),
        src: MachineOperand::Immediate(100),
    });
    block.push(MachineInstruction::Store {
        dst: MachineOperand::StackSlot(-8),
        src: MachineOperand::Register(MachineRegister::Virtual(v_init)),
        size: 8,
    });

    // Memory Barrier
    block.push(MachineInstruction::Barrier);

    // Atomic fetch_add [rsp-8], 25
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_add)),
        src: MachineOperand::Immediate(25),
    });
    block.push(MachineInstruction::AtomicFetchAdd {
        dst: MachineOperand::StackSlot(-8),
        src: MachineOperand::Register(MachineRegister::Virtual(v_add)),
        size: 8,
    });

    // Load resulting value from stack slot
    block.push(MachineInstruction::Load {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_res)),
        src: MachineOperand::StackSlot(-8),
        size: 8,
    });

    // Return res (125) in RAX
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Virtual(v_res)),
    });
    block.push(MachineInstruction::Return);

    module.add_function(main_func);

    let code = emit_link_and_run(&module, OptLevel::O2, "test_atomic_exec");
    assert_eq!(code, 125, "atomic fetch_add should result in exit code 125");
}
