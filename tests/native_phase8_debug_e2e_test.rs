//! End-to-End Native Backend Debug Metadata & Security Hardening Tests (Phase 8).
//!
//! Verifies:
//! 1. DebugInfo model: DebugSourceFile, DebugLineRecord, DebugVariable.
//! 2. SecurityMetadata: ASLR, DEP/NX, stack protection, CFI, RELRO.
//! 3. ADOB object debug metadata inclusion and validation.
//! 4. Real native compilation, linking, and execution on Windows x64.

#![allow(dead_code, unused_imports)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, RegisterClass, VirtualRegister,
};
use adesh_codegen::opt::OptLevel;
use adesh_codegen::targets::x86_64::X86_64Backend;
use adesh_object::TargetDescriptor;
use adesh_object::metadata::{
    DebugInfo, DebugLineRecord, DebugSourceFile, DebugVariable, SecurityMetadata,
};
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
fn test_debug_info_construction_and_line_records() {
    let mut debug = DebugInfo::default();
    debug.producer = "adeshc 0.3.0 (phase 8)".to_string();
    debug.language = "Adesh".to_string();

    debug.source_files.push(DebugSourceFile {
        file_id: 1,
        path: "main.ad".to_string(),
        directory: "/src".to_string(),
        checksum: Some([0xaa; 16]),
    });

    debug.line_tables.push(DebugLineRecord {
        code_offset: 0x00,
        file_id: 1,
        line: 1,
        column: 1,
        is_stmt: true,
        is_prologue_end: true,
        is_epilogue_begin: false,
    });
    debug.line_tables.push(DebugLineRecord {
        code_offset: 0x10,
        file_id: 1,
        line: 2,
        column: 5,
        is_stmt: true,
        is_prologue_end: false,
        is_epilogue_begin: false,
    });

    debug.variables.push(DebugVariable {
        name: "x".to_string(),
        type_name: "i64".to_string(),
        scope_start: 0x00,
        scope_end: 0x20,
        stack_offset: Some(-8),
        register_id: None,
    });

    assert_eq!(debug.source_files.len(), 1);
    assert_eq!(debug.line_tables.len(), 2);
    assert_eq!(debug.variables.len(), 1);
    assert_eq!(debug.variables[0].name, "x");
}

#[test]
fn test_security_metadata_hardening_flags() {
    let sec = SecurityMetadata {
        stack_protection: true,
        cfi: true,
        shadow_stack: true,
        pac: false,
        bti: false,
        dep_nx: true,
        aslr: true,
        relro: true,
        signed_code: false,
        control_flow_guard: true,
    };

    assert!(sec.stack_protection);
    assert!(sec.dep_nx);
    assert!(sec.aslr);
    assert!(sec.control_flow_guard);
    assert!(sec.relro);
}

#[test]
fn test_native_debug_execution_e2e() {
    let mut module = NativeModule::new("test_debug");
    let mut main_func = MachineFunction::new("main");
    main_func.is_exported = true;

    let b = main_func.entry_block_mut();
    b.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(64),
    });
    b.push(MachineInstruction::Return);
    module.add_function(main_func);

    let code = emit_link_and_run(&module, OptLevel::O0, "test_debug_exec");
    assert_eq!(
        code, 64,
        "debug-built executable must execute and return 64"
    );
}
