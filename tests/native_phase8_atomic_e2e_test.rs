//! End-to-End Native Backend Atomics & Memory Ordering Tests (Phase 8).
//!
//! Verifies:
//! 1. Memory orders (Relaxed, Acquire, Release, AcqRel, SeqCst).
//! 2. Native atomic operations (AtomicLoad, AtomicStore, AtomicFetchAdd, AtomicCompareExchange, Barrier).
//! 3. Multithreaded concurrent contention & linearizability.
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
use adesh_runtime::threading::{AdeshAtomicI64, AtomicOrdering, Thread};
use std::process::Command;
use std::sync::Arc;
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
fn test_atomic_memory_ordering_lowering() {
    for order in [
        MemoryOrder::Relaxed,
        MemoryOrder::Acquire,
        MemoryOrder::Release,
        MemoryOrder::AcqRel,
        MemoryOrder::SeqCst,
    ] {
        let op = AtomicOp::FetchAdd {
            dst: MachineOperand::Register(MachineRegister::Virtual(VirtualRegister(0))),
            val: MachineOperand::Immediate(5),
            order,
        };
        let lowered = ConcurrencyPass::lower_atomic(op).expect("lower atomic op");
        assert!(
            !lowered.is_empty(),
            "lowering should produce machine instructions"
        );
    }
}

#[test]
fn test_multithreaded_atomic_contention_e2e() {
    let atomic_val = Arc::new(AdeshAtomicI64::new(0));
    let num_threads = 8;
    let ops_per_thread = 5000;

    let mut handles: Vec<Thread> = Vec::new();
    for _ in 0..num_threads {
        let a = Arc::clone(&atomic_val);
        handles.push(Thread::spawn(move || {
            for _ in 0..ops_per_thread {
                a.fetch_add(1, AtomicOrdering::AcqRel);
            }
        }));
    }

    for h in handles {
        h.join().expect("join");
    }

    assert_eq!(
        atomic_val.load(AtomicOrdering::SeqCst),
        (num_threads * ops_per_thread) as i64,
        "concurrent atomic fetch_add must be linearizable"
    );
}

#[test]
fn test_atomic_compare_exchange_e2e() {
    let atomic_val = AdeshAtomicI64::new(10);
    // Successful CAS
    let res1 = atomic_val.compare_exchange(10, 20, AtomicOrdering::SeqCst, AtomicOrdering::Relaxed);
    assert_eq!(res1, Ok(10));
    assert_eq!(atomic_val.load(AtomicOrdering::Relaxed), 20);

    // Failed CAS
    let res2 = atomic_val.compare_exchange(10, 30, AtomicOrdering::SeqCst, AtomicOrdering::Relaxed);
    assert_eq!(res2, Err(20));
    assert_eq!(atomic_val.load(AtomicOrdering::Relaxed), 20);
}

#[test]
fn test_native_atomic_execution_on_windows_x64() {
    let mut module = NativeModule::new("test_atomic_native");
    let mut main_func = MachineFunction::new("main");
    main_func.is_exported = true;
    main_func.stack_size = 16;

    let v_init = main_func.alloc_vreg();
    let v_add = main_func.alloc_vreg();
    let v_res = main_func.alloc_vreg();

    let block = main_func.entry_block_mut();
    // [rsp-8] = 50
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_init)),
        src: MachineOperand::Immediate(50),
    });
    block.push(MachineInstruction::Store {
        dst: MachineOperand::StackSlot(-8),
        src: MachineOperand::Register(MachineRegister::Virtual(v_init)),
        size: 8,
    });

    // Memory Barrier
    block.push(MachineInstruction::Barrier);

    // Atomic fetch_add [rsp-8], 33
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_add)),
        src: MachineOperand::Immediate(33),
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

    // Return res (83) in RAX
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Virtual(v_res)),
    });
    block.push(MachineInstruction::Return);

    module.add_function(main_func);

    let code = emit_link_and_run(&module, OptLevel::O2, "test_atomic_p8_exec");
    assert_eq!(code, 83, "atomic fetch_add should result in exit code 83");
}
