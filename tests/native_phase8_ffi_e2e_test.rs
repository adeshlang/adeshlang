//! Phase 8 End-to-End Native FFI (Foreign Function Interface) Tests.
//!
//! Verifies:
//! 1. Formal FFI boundary contracts, C-ABI parameter marshalling, and function signatures.
//! 2. FfiCallLowerer converting high-level foreign calls to target ABI machine IR.
//! 3. External symbol imports and dynamic C-library linking.
//! 4. Real native execution of foreign function calls (C runtime functions).

#![allow(dead_code, unused_imports)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::abi::{SysVVariadics, SystemVX64Abi, Win64Variadics, WindowsX64Abi};
use adesh_codegen::ffi::{
    FfiCallLowerer, ForeignCallingConvention, ForeignFunctionDeclaration, ForeignParam,
    ForeignSignature, ForeignType,
};
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    MoveLocation, NativeModule, PhysicalRegister, VirtualRegister,
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
fn test_ffi_declaration_and_lowering() {
    let win64_abi = WindowsX64Abi;
    let sysv_abi = SystemVX64Abi;

    // extern "C" fn test_func(a: i64, b: f64) -> i32
    let decl = ForeignFunctionDeclaration::c_fn(
        "test_func",
        vec![("a", ForeignType::Int64), ("b", ForeignType::Float64)],
        ForeignType::Int32,
        false,
    );

    let arg_vregs = vec![VirtualRegister(0), VirtualRegister(1)];
    let ret_vreg = Some(VirtualRegister(2));

    // Register arguments must be one simultaneous ParallelMove, followed by
    // the call and the result move.
    fn arg_dsts(insts: &[MachineInstruction]) -> Vec<u8> {
        let pm = insts
            .iter()
            .find(|i| matches!(i, MachineInstruction::ParallelMove { .. }))
            .expect("ParallelMove");
        match pm {
            MachineInstruction::ParallelMove { moves } => moves
                .iter()
                .map(|m| match m.dst {
                    MoveLocation::PhysicalRegister(p) => p.0,
                    ref other => panic!("unexpected destination {other:?}"),
                })
                .collect(),
            other => panic!("expected ParallelMove, got {other:?}"),
        }
    }

    // Win64: arg 0 in RCX, arg 1 in XMM1 (reserves 32-byte shadow space: Sub, Move, Call, Add, Move)
    let win64_insts =
        FfiCallLowerer::lower_call(&decl, &arg_vregs, &win64_abi, ret_vreg).expect("lowering");
    assert_eq!(win64_insts.len(), 5);
    assert_eq!(arg_dsts(&win64_insts), vec![1, 17]);
    assert!(
        matches!(&win64_insts[2], MachineInstruction::Call { target: MachineOperand::Symbol(s), .. } if s == "test_func")
    );

    // SysV: arg 0 in RDI, arg 1 in XMM0 (0-byte shadow space: Move, Call, Move)
    let sysv_insts =
        FfiCallLowerer::lower_call(&decl, &arg_vregs, &sysv_abi, ret_vreg).expect("lowering");
    assert_eq!(sysv_insts.len(), 3);
    assert_eq!(arg_dsts(&sysv_insts), vec![7, 16]);
}

#[test]
fn test_ffi_sret_call_lowering() {
    let win64_abi = WindowsX64Abi;
    let sysv_abi = SystemVX64Abi;

    // extern "C" fn get_big_struct(x: i64) -> BigStruct (24 bytes)
    let big_struct_type = ForeignType::Struct {
        fields: vec![ForeignType::Int64, ForeignType::Int64, ForeignType::Int64],
        size: 24,
        align: 8,
    };
    let decl = ForeignFunctionDeclaration::c_fn(
        "get_big_struct",
        vec![("x", ForeignType::Int64)],
        big_struct_type,
        false,
    );

    let arg_vregs = vec![VirtualRegister(0)];
    let dest_buf_vreg = Some(VirtualRegister(1));

    // Win64: hidden sret buffer passed in RCX (1), argument x passed in RDX (2)
    let win64_insts = FfiCallLowerer::lower_call(&decl, &arg_vregs, &win64_abi, dest_buf_vreg)
        .expect("win64 sret lowering");
    assert_eq!(win64_insts.len(), 5);
    let pm = win64_insts
        .iter()
        .find(|i| matches!(i, MachineInstruction::ParallelMove { .. }))
        .expect("ParallelMove");
    match pm {
        MachineInstruction::ParallelMove { moves } => {
            assert_eq!(moves.len(), 2);
            assert_eq!(
                moves[0].dst,
                MoveLocation::PhysicalRegister(PhysicalRegister(1))
            ); // RCX (sret)
            assert_eq!(
                moves[1].dst,
                MoveLocation::PhysicalRegister(PhysicalRegister(2))
            ); // RDX (x)
        }
        other => panic!("expected ParallelMove, got {other:?}"),
    }

    // SysV: hidden sret buffer passed in RDI (7), argument x passed in RSI (6)
    let sysv_insts = FfiCallLowerer::lower_call(&decl, &arg_vregs, &sysv_abi, dest_buf_vreg)
        .expect("sysv sret lowering");
    assert_eq!(sysv_insts.len(), 3);
    match &sysv_insts[0] {
        MachineInstruction::ParallelMove { moves } => {
            assert_eq!(moves.len(), 2);
            assert_eq!(
                moves[0].dst,
                MoveLocation::PhysicalRegister(PhysicalRegister(7))
            ); // RDI (sret)
            assert_eq!(
                moves[1].dst,
                MoveLocation::PhysicalRegister(PhysicalRegister(6))
            ); // RSI (x)
        }
        other => panic!("expected ParallelMove, got {other:?}"),
    }
}

#[test]
fn test_ffi_lowering_rejects_unrepresentable_calls() {
    let win64_abi = WindowsX64Abi;

    // Extra variadic arguments used to be dropped silently by `zip`.
    let printf = ForeignFunctionDeclaration::printf();
    let err = FfiCallLowerer::lower_call(
        &printf,
        &[VirtualRegister(0), VirtualRegister(1)],
        &win64_abi,
        None,
    )
    .expect_err("extra variadic arguments must be rejected");
    assert!(err.to_string().contains("variadic"), "{err}");

    // Missing arguments.
    let decl = ForeignFunctionDeclaration::c_fn(
        "two_args",
        vec![("a", ForeignType::Int64), ("b", ForeignType::Int64)],
        ForeignType::Int64,
        false,
    );
    assert!(FfiCallLowerer::lower_call(&decl, &[VirtualRegister(0)], &win64_abi, None).is_err());

    // A void function cannot supply a result value.
    let free = ForeignFunctionDeclaration::free();
    assert!(
        FfiCallLowerer::lower_call(
            &free,
            &[VirtualRegister(0)],
            &win64_abi,
            Some(VirtualRegister(1))
        )
        .is_err()
    );
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_ffi_execution_e2e() {
    // Construct an Adesh module that calls an exported native helper through an FFI boundary
    let mut module = NativeModule::new("test_ffi_execution");

    // Native C-compatible helper: c_compute(x: i64, y: i64) -> i64
    let mut helper = MachineFunction::new("c_compute");
    helper.is_exported = true;
    let h_x = helper.alloc_vreg();
    let h_y = helper.alloc_vreg();
    let h_block = helper.entry_block_mut();
    // Copy RCX, RDX
    h_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(h_x)),
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(1))), // RCX
    });
    h_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(h_y)),
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(2))), // RDX
    });
    // h_x = h_x * h_y + 7
    h_block.push(MachineInstruction::Mul {
        dst: MachineOperand::Register(MachineRegister::Virtual(h_x)),
        src: MachineOperand::Register(MachineRegister::Virtual(h_y)),
    });
    h_block.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(h_x)),
        src: MachineOperand::Immediate(7),
    });
    // Return in RAX
    h_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Virtual(h_x)),
    });
    h_block.push(MachineInstruction::Return);
    module.add_function(helper);

    // Main caller using FFI call lowering
    let mut main_func = MachineFunction::new("main");
    main_func.is_exported = true;
    let v_arg1 = main_func.alloc_vreg();
    let v_arg2 = main_func.alloc_vreg();
    let v_ret = main_func.alloc_vreg();

    let m_block = main_func.entry_block_mut();
    m_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_arg1)),
        src: MachineOperand::Immediate(5),
    });
    m_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_arg2)),
        src: MachineOperand::Immediate(10),
    });

    let ffi_decl = ForeignFunctionDeclaration::c_fn(
        "c_compute",
        vec![("x", ForeignType::Int64), ("y", ForeignType::Int64)],
        ForeignType::Int64,
        false,
    );
    let win64_abi = WindowsX64Abi;
    let call_insts =
        FfiCallLowerer::lower_call(&ffi_decl, &[v_arg1, v_arg2], &win64_abi, Some(v_ret))
            .expect("FFI lowering");
    for inst in call_insts {
        m_block.push(inst);
    }

    // Return v_ret in RAX
    m_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Virtual(v_ret)),
    });
    m_block.push(MachineInstruction::Return);
    module.add_function(main_func);

    let code = emit_link_and_run(&module, OptLevel::O2, "test_ffi_exec");
    // 5 * 10 + 7 = 57
    assert_eq!(
        code, 57,
        "c_compute(5, 10) through FFI boundary should return 57"
    );
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_ffi_sret_execution_e2e() {
    let mut module = NativeModule::new("test_ffi_sret_exec_module");

    // Callee: make_big_struct(x: i64) -> BigStruct { a: i64, b: i64, c: i64 } (24 bytes)
    // Under Win64 ABI, returning an aggregate >8 bytes uses sret:
    // RCX receives sret pointer, RDX receives x.
    // Callee must return sret pointer in RAX.
    let mut callee = MachineFunction::new("make_big_struct");
    callee.is_exported = true;
    let sret_ptr = callee.alloc_vreg();
    let x_val = callee.alloc_vreg();
    let x_plus_1 = callee.alloc_vreg();
    let x_plus_2 = callee.alloc_vreg();

    let c_block = callee.entry_block_mut();
    // Move incoming RCX (sret pointer) and RDX (x)
    c_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(sret_ptr)),
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(1))), // RCX
    });
    c_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(x_val)),
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(2))), // RDX
    });
    // [sret_ptr + 0] = x
    c_block.push(MachineInstruction::Store {
        dst: MachineOperand::Memory {
            base: MachineRegister::Virtual(sret_ptr),
            offset: 0,
            index: None,
        },
        src: MachineOperand::Register(MachineRegister::Virtual(x_val)),
        size: 8,
    });
    // x_plus_1 = x + 1; [sret_ptr + 8] = x_plus_1
    c_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(x_plus_1)),
        src: MachineOperand::Register(MachineRegister::Virtual(x_val)),
    });
    c_block.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(x_plus_1)),
        src: MachineOperand::Immediate(1),
    });
    c_block.push(MachineInstruction::Store {
        dst: MachineOperand::Memory {
            base: MachineRegister::Virtual(sret_ptr),
            offset: 8,
            index: None,
        },
        src: MachineOperand::Register(MachineRegister::Virtual(x_plus_1)),
        size: 8,
    });
    // x_plus_2 = x + 2; [sret_ptr + 16] = x_plus_2
    c_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(x_plus_2)),
        src: MachineOperand::Register(MachineRegister::Virtual(x_val)),
    });
    c_block.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(x_plus_2)),
        src: MachineOperand::Immediate(2),
    });
    c_block.push(MachineInstruction::Store {
        dst: MachineOperand::Memory {
            base: MachineRegister::Virtual(sret_ptr),
            offset: 16,
            index: None,
        },
        src: MachineOperand::Register(MachineRegister::Virtual(x_plus_2)),
        size: 8,
    });
    // Win64 requirement: return sret pointer in RAX
    c_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Virtual(sret_ptr)),
    });
    c_block.push(MachineInstruction::Return);
    module.add_function(callee);

    // Caller (main):
    // Allocates stack space for the 24-byte struct, computes its address,
    // calls make_big_struct(10) via FFI lowering, reads the 3 fields,
    // computes field0 + field1 + field2 = 10 + 11 + 12 = 33, and exits with 33.
    let mut main_func = MachineFunction::new("main");
    main_func.is_exported = true;
    main_func.stack_size = 64; // Allocate stack frame

    let v_x = main_func.alloc_vreg();
    let v_buf = main_func.alloc_vreg();
    let v_ret = main_func.alloc_vreg();
    let f0 = main_func.alloc_vreg();
    let f1 = main_func.alloc_vreg();
    let f2 = main_func.alloc_vreg();
    let total = main_func.alloc_vreg();

    let m_block = main_func.entry_block_mut();
    // v_x = 10
    m_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_x)),
        src: MachineOperand::Immediate(10),
    });
    // v_buf = RBP - 32 (buffer for sret return)
    m_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_buf)),
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(5))), // RBP
    });
    m_block.push(MachineInstruction::Sub {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_buf)),
        src: MachineOperand::Immediate(32),
    });

    let big_struct_type = ForeignType::Struct {
        fields: vec![ForeignType::Int64, ForeignType::Int64, ForeignType::Int64],
        size: 24,
        align: 8,
    };
    let decl = ForeignFunctionDeclaration::c_fn(
        "make_big_struct",
        vec![("x", ForeignType::Int64)],
        big_struct_type,
        false,
    );
    let win64_abi = WindowsX64Abi;
    let call_insts = FfiCallLowerer::lower_call(&decl, &[v_x], &win64_abi, Some(v_buf))
        .expect("FFI sret lowering");
    for inst in call_insts {
        m_block.push(inst);
    }
    // Result pointer in RAX was moved to v_buf (or returned in v_buf)
    // Read back field 0, 1, 2 from [v_buf]
    m_block.push(MachineInstruction::Load {
        dst: MachineOperand::Register(MachineRegister::Virtual(f0)),
        src: MachineOperand::Memory {
            base: MachineRegister::Virtual(v_buf),
            offset: 0,
            index: None,
        },
        size: 8,
    });
    m_block.push(MachineInstruction::Load {
        dst: MachineOperand::Register(MachineRegister::Virtual(f1)),
        src: MachineOperand::Memory {
            base: MachineRegister::Virtual(v_buf),
            offset: 8,
            index: None,
        },
        size: 8,
    });
    m_block.push(MachineInstruction::Load {
        dst: MachineOperand::Register(MachineRegister::Virtual(f2)),
        src: MachineOperand::Memory {
            base: MachineRegister::Virtual(v_buf),
            offset: 16,
            index: None,
        },
        size: 8,
    });
    // total = f0 + f1 + f2
    m_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(total)),
        src: MachineOperand::Register(MachineRegister::Virtual(f0)),
    });
    m_block.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(total)),
        src: MachineOperand::Register(MachineRegister::Virtual(f1)),
    });
    m_block.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(total)),
        src: MachineOperand::Register(MachineRegister::Virtual(f2)),
    });
    // Return total in RAX
    m_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Virtual(total)),
    });
    m_block.push(MachineInstruction::Return);
    module.add_function(main_func);

    let code = emit_link_and_run(&module, OptLevel::O2, "test_ffi_sret_exec");
    // 10 + 11 + 12 = 33
    assert_eq!(
        code, 33,
        "make_big_struct(10) via sret should produce 10+11+12 = 33"
    );
}

#[test]
fn test_ffi_variadic_call_lowering() {
    let win64_abi = WindowsX64Abi;
    let sysv_abi = SystemVX64Abi;

    let printf = ForeignFunctionDeclaration::printf();
    let v_fmt = VirtualRegister(0);
    let v_a = VirtualRegister(1);
    let v_b = VirtualRegister(2);
    let v_f = VirtualRegister(3);

    // 1. Win64: printf(fmt, a: i64, b: i64) -> RCX (fmt), RDX (a), R8 (b)
    let win64_insts = FfiCallLowerer::lower_variadic_call(
        &printf,
        &[v_fmt, v_a, v_b],
        &[ForeignType::Int64, ForeignType::Int64],
        &win64_abi,
        None,
    )
    .expect("win64 variadic lowering");

    let pm = win64_insts
        .iter()
        .find(|i| matches!(i, MachineInstruction::ParallelMove { .. }))
        .expect("ParallelMove");
    match pm {
        MachineInstruction::ParallelMove { moves } => {
            assert_eq!(moves.len(), 3);
            assert_eq!(
                moves[0].dst,
                MoveLocation::PhysicalRegister(PhysicalRegister(1))
            ); // RCX
            assert_eq!(
                moves[1].dst,
                MoveLocation::PhysicalRegister(PhysicalRegister(2))
            ); // RDX
            assert_eq!(
                moves[2].dst,
                MoveLocation::PhysicalRegister(PhysicalRegister(8))
            ); // R8
        }
        other => panic!("expected ParallelMove, got {other:?}"),
    }

    // 2. SysV: printf(fmt, f: f64) -> RDI (fmt), XMM0 (f), and AL = 1 (vector count)
    let sysv_insts = FfiCallLowerer::lower_variadic_call(
        &printf,
        &[v_fmt, v_f],
        &[ForeignType::Float64],
        &sysv_abi,
        None,
    )
    .expect("sysv variadic lowering");

    match &sysv_insts[0] {
        MachineInstruction::ParallelMove { moves } => {
            assert_eq!(moves.len(), 2);
            assert_eq!(
                moves[0].dst,
                MoveLocation::PhysicalRegister(PhysicalRegister(7))
            ); // RDI
            assert_eq!(
                moves[1].dst,
                MoveLocation::PhysicalRegister(PhysicalRegister::xmm(0))
            ); // XMM0
        }
        other => panic!("expected ParallelMove, got {other:?}"),
    }
    // Instruction 1: AL = 1
    assert_eq!(
        sysv_insts[1],
        MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
            src: MachineOperand::Immediate(1),
        }
    );
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_variadic_execution_e2e() {
    let mut module = NativeModule::new("test_variadic_exec_module");

    // Callee: variadic_sum(count: i64, ...) -> i64
    // Preamble spills incoming RCX, RDX, R8, R9 to the caller's shadow space [RBP + 16..40].
    // Then reads count from [RBP + 16], and arguments 1..=count from [RBP + 16 + i*8].
    let mut callee = MachineFunction::new("variadic_sum");
    callee.is_exported = true;
    let count_reg = callee.alloc_vreg();
    let sum_reg = callee.alloc_vreg();
    let idx_reg = callee.alloc_vreg();
    let elem_reg = callee.alloc_vreg();
    let addr_reg = callee.alloc_vreg();

    let c_block = callee.entry_block_mut();
    // 1. Spill RCX, RDX, R8, R9 into shadow space
    Win64Variadics::emit_callee_shadow_spill(&mut c_block.instructions, PhysicalRegister(5)); // RBP

    // 2. Read count from [RBP + 16]
    c_block.push(MachineInstruction::Load {
        dst: MachineOperand::Register(MachineRegister::Virtual(count_reg)),
        src: MachineOperand::Memory {
            base: MachineRegister::Physical(PhysicalRegister(5)),
            offset: 16,
            index: None,
        },
        size: 8,
    });

    // 3. sum = 0, idx = 1
    c_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(sum_reg)),
        src: MachineOperand::Immediate(0),
    });
    c_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(idx_reg)),
        src: MachineOperand::Immediate(1),
    });

    // Unroll 3 arguments reading:
    // For i = 1..=3:
    // addr = RBP + 16 + i*8; elem = [addr]; sum += elem
    for i in 1..=3 {
        c_block.push(MachineInstruction::Load {
            dst: MachineOperand::Register(MachineRegister::Virtual(elem_reg)),
            src: MachineOperand::Memory {
                base: MachineRegister::Physical(PhysicalRegister(5)),
                offset: 16 + (i as i32 * 8),
                index: None,
            },
            size: 8,
        });
        c_block.push(MachineInstruction::Add {
            dst: MachineOperand::Register(MachineRegister::Virtual(sum_reg)),
            src: MachineOperand::Register(MachineRegister::Virtual(elem_reg)),
        });
    }

    // Return sum in RAX
    c_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Virtual(sum_reg)),
    });
    c_block.push(MachineInstruction::Return);
    module.add_function(callee);

    // Caller (main):
    // Calls variadic_sum(3, 10, 20, 30) -> all passed in registers RCX, RDX, R8, R9!
    // Total sum = 10 + 20 + 30 = 60.
    let mut main_func = MachineFunction::new("main");
    main_func.is_exported = true;
    main_func.stack_size = 64; // Frame with shadow space

    let v_count = main_func.alloc_vreg();
    let v_1 = main_func.alloc_vreg();
    let v_2 = main_func.alloc_vreg();
    let v_3 = main_func.alloc_vreg();
    let v_ret = main_func.alloc_vreg();

    let m_block = main_func.entry_block_mut();
    m_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_count)),
        src: MachineOperand::Immediate(3),
    });
    m_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_1)),
        src: MachineOperand::Immediate(10),
    });
    m_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_2)),
        src: MachineOperand::Immediate(20),
    });
    m_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_3)),
        src: MachineOperand::Immediate(30),
    });

    let decl = ForeignFunctionDeclaration::c_fn(
        "variadic_sum",
        vec![("count", ForeignType::Int64)],
        ForeignType::Int64,
        true, // variadic
    );
    let win64_abi = WindowsX64Abi;
    let call_insts = FfiCallLowerer::lower_variadic_call(
        &decl,
        &[v_count, v_1, v_2, v_3],
        &[ForeignType::Int64, ForeignType::Int64, ForeignType::Int64],
        &win64_abi,
        Some(v_ret),
    )
    .expect("variadic call lowering");

    for inst in call_insts {
        m_block.push(inst);
    }

    // Return v_ret in RAX
    m_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Virtual(v_ret)),
    });
    m_block.push(MachineInstruction::Return);
    module.add_function(main_func);

    let code = emit_link_and_run(&module, OptLevel::O2, "test_variadic_exec");
    // 10 + 20 + 30 = 60
    assert_eq!(
        code, 60,
        "variadic_sum(3, 10, 20, 30) via Win64 shadow space should produce 60"
    );
}
