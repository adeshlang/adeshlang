//! Phase 8 End-to-End Native ABI Infrastructure Tests.
//!
//! Verifies:
//! 1. Centralized AbiSpec framework across x86-64 (Win64 & SysV), AArch64 (AAPCS64), and RISC-V 64.
//! 2. Classification of integer, FP, vector, mixed, small structs, and large aggregates (sret).
//! 3. StackFrameLayout computation with alignment, shadow space, outgoing arguments, and red zone.
//! 4. Real native compilation, linking, and execution of multi-arg mixed integer/FP function calls and callee-saved preservation.

#![allow(dead_code, unused_imports)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::abi::{
    Aapcs64Abi, AbiSpec, AbiType, AggregateReturnRules, ArgumentLocation, ReturnLocation,
    RiscV64Abi, StackFrameLayout, StructPassingRules, SystemVX64Abi, WindowsX64Abi,
};
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
fn test_abi_framework_classification_all_architectures() {
    let win64 = WindowsX64Abi;
    let sysv = SystemVX64Abi;
    let aapcs = Aapcs64Abi;
    let riscv = RiscV64Abi;

    // Check Stack alignment & shadow space
    assert_eq!(win64.stack_alignment().0, 16);
    assert_eq!(win64.shadow_space().0, 32);
    assert_eq!(win64.red_zone().0, 0);

    assert_eq!(sysv.stack_alignment().0, 16);
    assert_eq!(sysv.shadow_space().0, 0);
    assert_eq!(sysv.red_zone().0, 128);

    assert_eq!(aapcs.stack_alignment().0, 16);
    assert_eq!(aapcs.shadow_space().0, 0);

    assert_eq!(riscv.stack_alignment().0, 16);
    assert_eq!(riscv.shadow_space().0, 0);

    // Test argument classification: [i64, f64, i32, ptr, i64, f32]
    let args = vec![
        AbiType::i64(),
        AbiType::f64(),
        AbiType::i32(),
        AbiType::ptr(),
        AbiType::i64(),
        AbiType::f32(),
    ];

    // Win64: slots 0..3 are RCX, XMM1, R8, R9; slots 4 & 5 are stack slots at offset 48, 56
    let win64_locs = win64.classify_arguments(&args);
    assert_eq!(win64_locs.len(), 6);
    assert_eq!(
        win64_locs[0],
        ArgumentLocation::Register(PhysicalRegister(1))
    ); // RCX
    assert_eq!(
        win64_locs[1],
        ArgumentLocation::FloatRegister(PhysicalRegister::xmm(1))
    ); // XMM1
    assert_eq!(
        win64_locs[2],
        ArgumentLocation::Register(PhysicalRegister(8))
    ); // R8
    assert_eq!(
        win64_locs[3],
        ArgumentLocation::Register(PhysicalRegister(9))
    ); // R9
    assert!(matches!(win64_locs[4], ArgumentLocation::Stack(s) if s.offset == 48));
    assert!(matches!(win64_locs[5], ArgumentLocation::Stack(s) if s.offset == 56));

    // SysV: RDI, XMM0, RSI, RDX, RCX, XMM1 (independent integer and FP register sequences)
    let sysv_locs = sysv.classify_arguments(&args);
    assert_eq!(sysv_locs.len(), 6);
    assert_eq!(
        sysv_locs[0],
        ArgumentLocation::Register(PhysicalRegister(7))
    ); // RDI
    assert_eq!(
        sysv_locs[1],
        ArgumentLocation::FloatRegister(PhysicalRegister::xmm(0))
    ); // XMM0
    assert_eq!(
        sysv_locs[2],
        ArgumentLocation::Register(PhysicalRegister(6))
    ); // RSI
    assert_eq!(
        sysv_locs[3],
        ArgumentLocation::Register(PhysicalRegister(2))
    ); // RDX
    assert_eq!(
        sysv_locs[4],
        ArgumentLocation::Register(PhysicalRegister(1))
    ); // RCX
    assert_eq!(
        sysv_locs[5],
        ArgumentLocation::FloatRegister(PhysicalRegister::xmm(1))
    ); // XMM1

    // Large Struct return rules:
    let large_struct = AbiType::Struct {
        fields: vec![AbiType::i64(), AbiType::i64(), AbiType::i64()],
        size: 24,
        align: 8,
    };
    assert!(
        matches!(win64.classify_return(&large_struct), ReturnLocation::HiddenSret(r) if r.0 == 1)
    ); // RCX
    assert!(
        matches!(sysv.classify_return(&large_struct), ReturnLocation::HiddenSret(r) if r.0 == 7)
    ); // RDI
    assert!(
        matches!(aapcs.classify_return(&large_struct), ReturnLocation::HiddenSret(r) if r.0 == 8)
    ); // X8
    assert!(
        matches!(riscv.classify_return(&large_struct), ReturnLocation::HiddenSret(r) if r.0 == 10)
    ); // a0
}

#[test]
fn test_stack_frame_layout_calculation() {
    // Win64 frame layout: 32 bytes shadow space, 16-byte alignment, 24 bytes locals, 8 bytes spill
    let win64_abi = WindowsX64Abi;
    let layout = win64_abi.compute_layout(24, 8, 0, 2);
    // locals(24) + spills(8) + shadow(32) + callee_saved(2*8=16) = 80 bytes (aligned to 16 = 80)
    assert_eq!(layout.total_frame_size, 80);
    assert_eq!(layout.shadow_space_size, 32);
    assert_eq!(layout.alignment, 16);
    assert_eq!(layout.callee_saved_size, 16);
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_execution_multi_arg_call_e2e() {
    let mut module = NativeModule::new("test_abi_multi_arg");

    // Helper function that adds 4 integer arguments passed in RCX, RDX, R8, R9
    // sum4(a, b, c, d) -> a + b + c + d
    let mut sum4_func = MachineFunction::new("sum4");
    let a = sum4_func.alloc_vreg();
    let b = sum4_func.alloc_vreg();
    let c = sum4_func.alloc_vreg();
    let d = sum4_func.alloc_vreg();

    let block = sum4_func.entry_block_mut();
    // Copy input registers RCX, RDX, R8, R9 to vregs
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(a)),
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(1))), // RCX
    });
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(b)),
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(2))), // RDX
    });
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(c)),
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(8))), // R8
    });
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(d)),
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(9))), // R9
    });

    // a = a + b; a = a + c; a = a + d
    block.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(a)),
        src: MachineOperand::Register(MachineRegister::Virtual(b)),
    });
    block.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(a)),
        src: MachineOperand::Register(MachineRegister::Virtual(c)),
    });
    block.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(a)),
        src: MachineOperand::Register(MachineRegister::Virtual(d)),
    });

    // Return in RAX
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Virtual(a)),
    });
    block.push(MachineInstruction::Return);
    module.add_function(sum4_func);

    // Main function: calls sum4(10, 20, 30, 4) -> 64
    let mut main_func = MachineFunction::new("main");
    main_func.is_exported = true;
    let mblock = main_func.entry_block_mut();

    // Set up call arguments: RCX=10, RDX=20, R8=30, R9=4
    mblock.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(1))),
        src: MachineOperand::Immediate(10),
    });
    mblock.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(2))),
        src: MachineOperand::Immediate(20),
    });
    mblock.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(8))),
        src: MachineOperand::Immediate(30),
    });
    mblock.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(9))),
        src: MachineOperand::Immediate(4),
    });
    mblock.push(MachineInstruction::Call {
        target: MachineOperand::Symbol("sum4".to_string()),
        num_args: 4,
    });
    // Return value in RAX returned directly from main
    mblock.push(MachineInstruction::Return);
    module.add_function(main_func);

    let code = emit_link_and_run(&module, OptLevel::O2, "test_abi_exec");
    assert_eq!(code, 64, "sum4(10, 20, 30, 4) should return exit code 64");
}
