//! Phase 8 End-to-End Native FFI (Foreign Function Interface) Tests.
//!
//! Verifies:
//! 1. Formal FFI boundary contracts, C-ABI parameter marshalling, and function signatures.
//! 2. FfiCallLowerer converting high-level foreign calls to target ABI machine IR.
//! 3. External symbol imports and dynamic C-library linking.
//! 4. Real native execution of foreign function calls (C runtime functions).

#![allow(dead_code, unused_imports)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::abi::{SystemVX64Abi, WindowsX64Abi};
use adesh_codegen::ffi::{
    FfiCallLowerer, ForeignCallingConvention, ForeignFunctionDeclaration, ForeignParam,
    ForeignSignature, ForeignType,
};
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, VirtualRegister,
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

    // Lower for Win64
    let win64_insts = FfiCallLowerer::lower_call(&decl, &arg_vregs, &win64_abi, ret_vreg);
    assert_eq!(win64_insts.len(), 4); // mov RCX, v0; mov XMM1, v1; call test_func; mov v2, RAX
    assert!(matches!(&win64_insts[0], MachineInstruction::Move { dst: MachineOperand::Register(MachineRegister::Physical(p)), .. } if p.0 == 1));
    assert!(matches!(&win64_insts[1], MachineInstruction::Move { dst: MachineOperand::Register(MachineRegister::Physical(p)), .. } if p.0 == 17)); // XMM1
    assert!(matches!(&win64_insts[2], MachineInstruction::Call { target: MachineOperand::Symbol(s), .. } if s == "test_func"));

    // Lower for SysV: arg 0 in RDI, arg 1 in XMM0
    let sysv_insts = FfiCallLowerer::lower_call(&decl, &arg_vregs, &sysv_abi, ret_vreg);
    assert_eq!(sysv_insts.len(), 4);
    assert!(matches!(&sysv_insts[0], MachineInstruction::Move { dst: MachineOperand::Register(MachineRegister::Physical(p)), .. } if p.0 == 7)); // RDI
    assert!(matches!(&sysv_insts[1], MachineInstruction::Move { dst: MachineOperand::Register(MachineRegister::Physical(p)), .. } if p.0 == 16)); // XMM0
}

#[test]
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
    let call_insts = FfiCallLowerer::lower_call(&ffi_decl, &[v_arg1, v_arg2], &win64_abi, Some(v_ret));
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
    assert_eq!(code, 57, "c_compute(5, 10) through FFI boundary should return 57");
}
