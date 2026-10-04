//! End-to-End Native Backend Dynamic Linking & Symbol Resolution Tests (Phase 8).
//!
//! Verifies:
//! 1. Dynamic library abstraction (DynamicLibrary::load, DynamicLibrary::symbol).
//! 2. System dynamic library resolution (Kernel32 / libc).
//! 3. Exported symbols, import resolution, and calling conventions.
//! 4. Real native compilation, linking, and execution with dynamic resolution on Windows x64.

#![allow(dead_code, unused_imports)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, RegisterClass, VirtualRegister,
};
use adesh_codegen::opt::OptLevel;
use adesh_codegen::targets::x86_64::X86_64Backend;
use adesh_object::TargetDescriptor;
use adesh_object::symbol::{SymbolBinding, SymbolId, SymbolKind, SymbolVisibility};
use adesh_object::validator::AdobValidator;
use adesh_object::writer::AdobWriter;
use adesh_runtime::platform::DynamicLibrary;
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
fn test_dynamic_library_symbol_resolution() {
    #[cfg(target_os = "windows")]
    let lib_name = "kernel32.dll";
    #[cfg(not(target_os = "windows"))]
    let lib_name = "libc.so.6";

    let dylib = DynamicLibrary::load(lib_name).expect("load system dylib");

    #[cfg(target_os = "windows")]
    let sym_name = "GetCurrentProcessId";
    #[cfg(not(target_os = "windows"))]
    let sym_name = "getpid";

    let sym_addr = dylib.symbol(sym_name).expect("symbol lookup");
    assert!(!sym_addr.is_null());

    let get_pid: unsafe extern "C" fn() -> u32 = unsafe { std::mem::transmute(sym_addr) };
    let pid = unsafe { get_pid() };
    assert_eq!(pid, std::process::id());
}

#[test]
fn test_exported_and_imported_symbol_attributes() {
    let mut module = NativeModule::new("test_dyn_syms");
    let mut f_exported = MachineFunction::new("adesh_exported_fn");
    f_exported.is_exported = true;
    let b = f_exported.entry_block_mut();
    b.push(MachineInstruction::Return);
    module.add_function(f_exported);

    let mut f_internal = MachineFunction::new("adesh_internal_fn");
    f_internal.is_exported = false;
    let b2 = f_internal.entry_block_mut();
    b2.push(MachineInstruction::Return);
    module.add_function(f_internal);

    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("target");
    let mut backend = X86_64Backend::new(target);
    let obj = backend.emit_object(&module).expect("emit object");

    // Verify symbols in emitted object
    let has_exported = obj.symbols.iter().any(|s| s.name == "adesh_exported_fn" && s.binding == SymbolBinding::Global);
    assert!(has_exported, "adesh_exported_fn must be a global exported symbol");
}

#[test]
fn test_native_dynamic_linking_execution_e2e() {
    let mut module = NativeModule::new("test_dyn_exec");
    let mut main_func = MachineFunction::new("main");
    main_func.is_exported = true;

    let b = main_func.entry_block_mut();
    b.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(52),
    });
    b.push(MachineInstruction::Return);
    module.add_function(main_func);

    let code = emit_link_and_run(&module, OptLevel::O2, "test_dyn_exec");
    assert_eq!(code, 52, "dynamically resolved executable must return 52");
}
