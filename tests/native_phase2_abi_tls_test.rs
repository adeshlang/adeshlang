//! Phase 2 Task P2-5: Windows Thread-Local Storage (TLS) End-to-End Tests.
//!
//! Verifies:
//! 1. Windows x64 TLS resolution sequence (`gs:[0x58]` / `_tls_index` / SECREL addend).
//! 2. Linker synthesis of PE TLS directory (`IMAGE_TLS_DIRECTORY64`) and `.tls` section placement.
//! 3. Linker resolution of `_tls_index` and TLS section-relative symbols (`RelocationKind::TlsLe`).
//! 4. Non-Windows targets reject TLS with a structured compile error until Phase 3.
//! 5. Execution-driven tests verifying real PE binaries executing thread-local storage loads and stores.

#![allow(dead_code, unused_imports)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::calling_convention::{SystemVX64CallingConvention, WindowsX64CallingConvention};
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
fn test_tls_non_windows_target_is_rejected_with_structured_error() {
    let target = TargetDescriptor::from_triple("x86_64-unknown-linux-gnu").expect("valid triple");
    let mut backend = X86_64Backend::new(target);

    let mut native_mod = NativeModule::new("test_tls_sysv");
    native_mod
        .tls_sections
        .push(("g_counter".to_string(), 42i64.to_le_bytes().to_vec()));

    let mut main_fn = MachineFunction::new("main");
    main_fn.is_exported = true;
    let entry = main_fn.entry_block_mut();
    // Try to emit TLS address resolution on a non-Windows target
    entry.push(MachineInstruction::TlsAddress {
        dst: MachineOperand::phys(0),
        symbol: "g_counter".to_string(),
    });
    entry.push(MachineInstruction::Return);
    native_mod.add_function(main_fn);

    let err = backend
        .emit_object(&native_mod)
        .expect_err("SysV TLS must be rejected in Phase 2");
    assert!(
        err.to_string().contains("non-Windows TLS is not supported"),
        "expected structured non-Windows TLS error, got: {}",
        err
    );
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_windows_tls_read_execution_e2e() {
    let mut native_mod = NativeModule::new("test_win_tls_read");
    // Define a 64-bit thread-local variable initialized to 42
    native_mod
        .tls_sections
        .push(("g_thread_counter".to_string(), 42i64.to_le_bytes().to_vec()));

    let mut main_fn = MachineFunction::new("main");
    main_fn.is_exported = true;
    let entry = main_fn.entry_block_mut();

    // 1. Resolve address of g_thread_counter into RAX (phys 0)
    entry.push(MachineInstruction::TlsAddress {
        dst: MachineOperand::phys(0),
        symbol: "g_thread_counter".to_string(),
    });

    // 2. Dereference RAX to read the 64-bit value: RAX = [RAX]
    entry.push(MachineInstruction::Load {
        dst: MachineOperand::phys(0),
        src: MachineOperand::Memory {
            base: MachineRegister::Physical(PhysicalRegister(0)),
            offset: 0,
            index: None,
        },
        size: 8,
    });

    // 3. Add 5 to RAX: 42 + 5 = 47
    entry.push(MachineInstruction::Add {
        dst: MachineOperand::phys(0),
        src: MachineOperand::Immediate(5),
    });

    // 4. Return RAX (process exit code 47)
    entry.push(MachineInstruction::Return);
    native_mod.add_function(main_fn);

    let code = emit_link_and_run(&native_mod, OptLevel::O0, "test_win_tls_read");
    assert_eq!(
        code, 47,
        "exit code must be 42 + 5 = 47 from thread-local storage read"
    );
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_windows_tls_read_modify_write_execution_e2e() {
    let mut native_mod = NativeModule::new("test_win_tls_rmw");
    // Define two TLS variables: initial 10 and step 7
    native_mod
        .tls_sections
        .push(("g_tls_val".to_string(), 10i64.to_le_bytes().to_vec()));
    native_mod
        .tls_sections
        .push(("g_tls_step".to_string(), 7i64.to_le_bytes().to_vec()));

    let mut main_fn = MachineFunction::new("main");
    main_fn.is_exported = true;
    let entry = main_fn.entry_block_mut();

    // 1. Resolve address of g_tls_val into RCX (phys 1)
    entry.push(MachineInstruction::TlsAddress {
        dst: MachineOperand::phys(1),
        symbol: "g_tls_val".to_string(),
    });

    // 2. Resolve address of g_tls_step into RDX (phys 2)
    entry.push(MachineInstruction::TlsAddress {
        dst: MachineOperand::phys(2),
        symbol: "g_tls_step".to_string(),
    });

    // 3. Load g_tls_val into RAX (phys 0)
    entry.push(MachineInstruction::Load {
        dst: MachineOperand::phys(0),
        src: MachineOperand::Memory {
            base: MachineRegister::Physical(PhysicalRegister(1)),
            offset: 0,
            index: None,
        },
        size: 8,
    });

    // 4. Load g_tls_step into R8 (phys 8)
    entry.push(MachineInstruction::Load {
        dst: MachineOperand::phys(8),
        src: MachineOperand::Memory {
            base: MachineRegister::Physical(PhysicalRegister(2)),
            offset: 0,
            index: None,
        },
        size: 8,
    });

    // 5. Compute: RAX = (RAX * 2) + R8 = (10 * 2) + 7 = 27
    entry.push(MachineInstruction::Add {
        dst: MachineOperand::phys(0),
        src: MachineOperand::phys(0),
    });
    entry.push(MachineInstruction::Add {
        dst: MachineOperand::phys(0),
        src: MachineOperand::phys(8),
    });

    // 6. Store 27 back into g_tls_val [RCX]: [RCX] = RAX
    entry.push(MachineInstruction::Store {
        dst: MachineOperand::Memory {
            base: MachineRegister::Physical(PhysicalRegister(1)),
            offset: 0,
            index: None,
        },
        src: MachineOperand::phys(0),
        size: 8,
    });

    // 7. Clear RAX to 0, then re-load from g_tls_val [RCX] to verify persistence
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::phys(0),
        src: MachineOperand::Immediate(0),
    });
    entry.push(MachineInstruction::Load {
        dst: MachineOperand::phys(0),
        src: MachineOperand::Memory {
            base: MachineRegister::Physical(PhysicalRegister(1)),
            offset: 0,
            index: None,
        },
        size: 8,
    });

    // 8. Return RAX (process exit code 27)
    entry.push(MachineInstruction::Return);
    native_mod.add_function(main_fn);

    let code = emit_link_and_run(&native_mod, OptLevel::O0, "test_win_tls_rmw");
    assert_eq!(
        code, 27,
        "exit code must be 27 from thread-local storage read-modify-write"
    );
}
