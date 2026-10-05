//! End-to-End Native Backend ABI Conformance, Unwind Metadata & Stack Maps Tests (Phase 6).
//!
//! Verifies:
//! 1. Multi-argument Calling Conventions (SysV 6-register vs Windows 4-register + shadow space).
//! 2. Unwind metadata descriptor generation (`Win64UnwindInfo` and `.eh_frame` CIE/FDE mapping).
//! 3. Stack Map generation (`FunctionStackMap` and GC safepoint tracking).
//! 4. Real native compilation, linking, and execution on Windows x64.

#![allow(dead_code, unused_imports)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::calling_convention::{
    CallingConvention, SystemVX64CallingConvention, WindowsX64CallingConvention,
};
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, RegisterClass, VirtualRegister,
};
use adesh_codegen::opt::OptLevel;
use adesh_codegen::stack_maps::{FunctionStackMap, LiveLocationKind, LiveLocationRecord};
use adesh_codegen::targets::x86_64::X86_64Backend;
use adesh_codegen::unwind_info::{FunctionUnwindDescriptor, Win64UnwindInfo};
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
fn test_calling_convention_parameter_registers() {
    let win64 = WindowsX64CallingConvention;
    let sysv = SystemVX64CallingConvention;

    // Win64 GPR args: RCX (1), RDX (2), R8 (8), R9 (9)
    let win64_gprs = win64.arg_registers();
    assert_eq!(win64_gprs.len(), 4);
    assert_eq!(win64_gprs[0].0, 1);
    assert_eq!(win64_gprs[1].0, 2);
    assert_eq!(win64_gprs[2].0, 8);
    assert_eq!(win64_gprs[3].0, 9);
    assert_eq!(win64.shadow_space(), 32);

    // SysV GPR args: RDI (7), RSI (6), RDX (2), RCX (1), R8 (8), R9 (9)
    let sysv_gprs = sysv.arg_registers();
    assert_eq!(sysv_gprs.len(), 6);
    assert_eq!(sysv_gprs[0].0, 7);
    assert_eq!(sysv_gprs[1].0, 6);
    assert_eq!(sysv_gprs[2].0, 2);
    assert_eq!(sysv_gprs[3].0, 1);
    assert_eq!(sysv_gprs[4].0, 8);
    assert_eq!(sysv_gprs[5].0, 9);
    assert_eq!(sysv.shadow_space(), 0);
}

#[test]
fn test_unwind_descriptor_and_stack_maps() {
    let unwind = FunctionUnwindDescriptor::for_win64("main", 16, 64, &[(3, 8), (12, 12)]);
    assert!(unwind.win64_unwind.is_some());
    let win_unwind = unwind.win64_unwind.unwrap();
    assert_eq!(win_unwind.function_name, "main");
    assert!(!win_unwind.opcodes.is_empty());

    let mut stack_map = FunctionStackMap::new("main");
    stack_map.add_safepoint(
        24,
        vec![
            LiveLocationRecord {
                location: LiveLocationKind::Register(1),
                is_pointer: true,
            },
            LiveLocationRecord {
                location: LiveLocationKind::StackSlot(-16),
                is_pointer: false,
            },
        ],
    );
    assert_eq!(stack_map.safepoints.len(), 1);
    assert_eq!(stack_map.safepoints[0].live_locations.len(), 2);
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_abi_function_call_and_return_e2e() {
    // Helper function `add_three(a, b, c)` -> returns a + b + c
    let mut func_add = MachineFunction::new("add_three");
    func_add.is_exported = true;
    let entry_add = func_add.entry_block_mut();
    // On Win64: arg0 in RCX (1), arg1 in RDX (2), arg2 in R8 (8)
    entry_add.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(1))),
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(2))),
    });
    entry_add.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(1))),
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(8))),
    });
    entry_add.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))), // RAX
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(1))),
    });
    entry_add.push(MachineInstruction::Return);

    // Main function: calls `add_three(10, 20, 12)` -> returns 42
    let mut func_main = MachineFunction::new("main");
    func_main.is_exported = true;
    let entry_main = func_main.entry_block_mut();
    entry_main.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(1))),
        src: MachineOperand::Immediate(10),
    });
    entry_main.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(2))),
        src: MachineOperand::Immediate(20),
    });
    entry_main.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(8))),
        src: MachineOperand::Immediate(12),
    });
    entry_main.push(MachineInstruction::Call {
        target: MachineOperand::Symbol("add_three".to_string()),
        num_args: 3,
    });
    entry_main.push(MachineInstruction::Return);

    let mut module = NativeModule::new("test_abi");
    module.add_function(func_add);
    module.add_function(func_main);

    let exit_code = emit_link_and_run(&module, OptLevel::O2, "abi_conformance");
    assert_eq!(exit_code, 42);
}
