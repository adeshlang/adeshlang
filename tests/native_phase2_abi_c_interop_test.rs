//! Phase 2 Task P2-7: C-Interop Proof with Real UCRT / libc.
//!
//! Verifies:
//! 1. Calling real C runtime functions from native Adesh code:
//!    - `strlen` (string measurement via CRT import)
//!    - `malloc` and `free` (dynamic memory allocation, read/write, and release)
//!    - `memcpy` (buffer copying)
//!    - `sqrt` (scalar double floating-point math)
//!    - `printf` (variadic formatted console output)
//! 2. Proper Win64 calling convention handling (32-byte shadow space, RCX/RDX/R8/R9 parameter passing, XMM floating-point registers).
//! 3. Execution of real linked PE binaries on Windows x86-64 asserting exact exit codes and stdout.

#![allow(dead_code, unused_imports)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::WindowsX64Abi;
use adesh_codegen::ffi::{FfiCallLowerer, ForeignFunctionDeclaration, ForeignParam, ForeignType};
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
fn emit_link_and_run(
    native_mod: &NativeModule,
    opt_level: OptLevel,
    test_name: &str,
) -> (i32, String) {
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
    let code = out.status.code().expect("exit code");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    (code, stdout)
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_c_interop_strlen_e2e() {
    let mut native_mod = NativeModule::new("test_c_strlen");
    // Place null-terminated string literal in data section
    let test_str = b"Adesh C-Interop Rocks!\0"; // length 22 without null
    native_mod
        .data_sections
        .push(("str_literal".to_string(), test_str.to_vec()));

    let mut main_fn = MachineFunction::new("main");
    main_fn.is_exported = true;
    main_fn.stack_size = 48; // shadow space + locals
    let entry = main_fn.entry_block_mut();

    // 1. Materialize string address into RCX (phys 1)
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::phys(1),
        src: MachineOperand::Symbol("str_literal".to_string()),
    });

    // 2. Allocate 32-byte shadow space on RSP for Win64 call
    entry.push(MachineInstruction::Sub {
        dst: MachineOperand::phys(4), // RSP
        src: MachineOperand::Immediate(32),
    });

    // 3. Call strlen
    entry.push(MachineInstruction::Call {
        target: MachineOperand::Symbol("strlen".to_string()),
        num_args: 1,
    });

    // 4. Restore shadow space
    entry.push(MachineInstruction::Add {
        dst: MachineOperand::phys(4), // RSP
        src: MachineOperand::Immediate(32),
    });

    // Return value is in RAX (phys 0)
    entry.push(MachineInstruction::Return);
    native_mod.add_function(main_fn);

    let (code, _) = emit_link_and_run(&native_mod, OptLevel::O0, "c_strlen");
    assert_eq!(
        code, 22,
        "strlen should return 22 for 'Adesh C-Interop Rocks!'"
    );
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_c_interop_malloc_and_free_e2e() {
    let mut native_mod = NativeModule::new("test_c_malloc_free");
    let mut main_fn = MachineFunction::new("main");
    main_fn.is_exported = true;
    main_fn.stack_size = 48;
    let entry = main_fn.entry_block_mut();

    // 1. Allocate 64 bytes via malloc(64)
    // RCX = 64
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::phys(1),
        src: MachineOperand::Immediate(64),
    });
    // Sub RSP, 32
    entry.push(MachineInstruction::Sub {
        dst: MachineOperand::phys(4),
        src: MachineOperand::Immediate(32),
    });
    // Call malloc
    entry.push(MachineInstruction::Call {
        target: MachineOperand::Symbol("malloc".to_string()),
        num_args: 1,
    });
    // Add RSP, 32
    entry.push(MachineInstruction::Add {
        dst: MachineOperand::phys(4),
        src: MachineOperand::Immediate(32),
    });

    // RAX contains the allocated heap pointer.
    // Save ptr to stack slot [RBP - 8]
    entry.push(MachineInstruction::Store {
        dst: MachineOperand::StackSlot(-8),
        src: MachineOperand::phys(0),
        size: 8,
    });

    // 2. Write 33 to [ptr + 0], and 15 to [ptr + 8]
    // Load ptr into RAX
    entry.push(MachineInstruction::Load {
        dst: MachineOperand::phys(0),
        src: MachineOperand::StackSlot(-8),
        size: 8,
    });
    // RDX = 33
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::phys(2),
        src: MachineOperand::Immediate(33),
    });
    entry.push(MachineInstruction::Store {
        dst: MachineOperand::Memory {
            base: MachineRegister::Physical(PhysicalRegister(0)),
            offset: 0,
            index: None,
        },
        src: MachineOperand::phys(2),
        size: 8,
    });

    // RDX = 15
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::phys(2),
        src: MachineOperand::Immediate(15),
    });
    entry.push(MachineInstruction::Store {
        dst: MachineOperand::Memory {
            base: MachineRegister::Physical(PhysicalRegister(0)),
            offset: 8,
            index: None,
        },
        src: MachineOperand::phys(2),
        size: 8,
    });

    // 3. Read back and compute sum: RAX = [ptr + 0] + [ptr + 8] = 48
    entry.push(MachineInstruction::Load {
        dst: MachineOperand::phys(2), // RDX = [ptr + 0]
        src: MachineOperand::Memory {
            base: MachineRegister::Physical(PhysicalRegister(0)),
            offset: 0,
            index: None,
        },
        size: 8,
    });
    entry.push(MachineInstruction::Load {
        dst: MachineOperand::phys(8), // R8 = [ptr + 8]
        src: MachineOperand::Memory {
            base: MachineRegister::Physical(PhysicalRegister(0)),
            offset: 8,
            index: None,
        },
        size: 8,
    });
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::phys(0),
        src: MachineOperand::phys(2),
    });
    entry.push(MachineInstruction::Add {
        dst: MachineOperand::phys(0),
        src: MachineOperand::phys(8),
    });

    // Save sum (48) into stack slot [RBP - 16]
    entry.push(MachineInstruction::Store {
        dst: MachineOperand::StackSlot(-16),
        src: MachineOperand::phys(0),
        size: 8,
    });

    // 4. Free the memory: free(ptr)
    // RCX = ptr from [RBP - 8]
    entry.push(MachineInstruction::Load {
        dst: MachineOperand::phys(1),
        src: MachineOperand::StackSlot(-8),
        size: 8,
    });
    // Sub RSP, 32
    entry.push(MachineInstruction::Sub {
        dst: MachineOperand::phys(4),
        src: MachineOperand::Immediate(32),
    });
    // Call free
    entry.push(MachineInstruction::Call {
        target: MachineOperand::Symbol("free".to_string()),
        num_args: 1,
    });
    // Add RSP, 32
    entry.push(MachineInstruction::Add {
        dst: MachineOperand::phys(4),
        src: MachineOperand::Immediate(32),
    });

    // 5. Restore sum into RAX from [RBP - 16]
    entry.push(MachineInstruction::Load {
        dst: MachineOperand::phys(0),
        src: MachineOperand::StackSlot(-16),
        size: 8,
    });
    entry.push(MachineInstruction::Return);
    native_mod.add_function(main_fn);

    let (code, _) = emit_link_and_run(&native_mod, OptLevel::O0, "c_malloc_free");
    assert_eq!(code, 48, "malloc/free roundtrip with sum should exit 48");
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_c_interop_memcpy_e2e() {
    let mut native_mod = NativeModule::new("test_c_memcpy");
    let mut main_fn = MachineFunction::new("main");
    main_fn.is_exported = true;
    main_fn.stack_size = 64; // Two 16-byte buffers on stack + shadow space
    let entry = main_fn.entry_block_mut();

    // Source buffer at [RBP - 16]: store two 64-bit words: 25, 16
    entry.push(MachineInstruction::Store {
        dst: MachineOperand::StackSlot(-16),
        src: MachineOperand::Immediate(25),
        size: 8,
    });
    entry.push(MachineInstruction::Store {
        dst: MachineOperand::StackSlot(-8),
        src: MachineOperand::Immediate(16),
        size: 8,
    });

    // Zero out destination buffer at [RBP - 32], [RBP - 24]
    entry.push(MachineInstruction::Store {
        dst: MachineOperand::StackSlot(-32),
        src: MachineOperand::Immediate(0),
        size: 8,
    });
    entry.push(MachineInstruction::Store {
        dst: MachineOperand::StackSlot(-24),
        src: MachineOperand::Immediate(0),
        size: 8,
    });

    // Set up memcpy(dst, src, 16):
    // RCX = dst address (&[RBP - 32])
    // lea RCX, [RBP - 32]
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::phys(1), // RCX
        src: MachineOperand::phys(5), // RBP
    });
    entry.push(MachineInstruction::Sub {
        dst: MachineOperand::phys(1),
        src: MachineOperand::Immediate(32),
    });

    // RDX = src address (&[RBP - 16])
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::phys(2), // RDX
        src: MachineOperand::phys(5), // RBP
    });
    entry.push(MachineInstruction::Sub {
        dst: MachineOperand::phys(2),
        src: MachineOperand::Immediate(16),
    });

    // R8 = 16 (count)
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::phys(8), // R8
        src: MachineOperand::Immediate(16),
    });

    // Shadow space
    entry.push(MachineInstruction::Sub {
        dst: MachineOperand::phys(4),
        src: MachineOperand::Immediate(32),
    });
    entry.push(MachineInstruction::Call {
        target: MachineOperand::Symbol("memcpy".to_string()),
        num_args: 3,
    });
    entry.push(MachineInstruction::Add {
        dst: MachineOperand::phys(4),
        src: MachineOperand::Immediate(32),
    });

    // Read back copied values from destination buffer:
    // RAX = [RBP - 32] + [RBP - 24] = 25 + 16 = 41
    entry.push(MachineInstruction::Load {
        dst: MachineOperand::phys(0),
        src: MachineOperand::StackSlot(-32),
        size: 8,
    });
    entry.push(MachineInstruction::Load {
        dst: MachineOperand::phys(10),
        src: MachineOperand::StackSlot(-24),
        size: 8,
    });
    entry.push(MachineInstruction::Add {
        dst: MachineOperand::phys(0),
        src: MachineOperand::phys(10),
    });
    entry.push(MachineInstruction::Return);
    native_mod.add_function(main_fn);

    let (code, _) = emit_link_and_run(&native_mod, OptLevel::O0, "c_memcpy");
    assert_eq!(
        code, 41,
        "memcpy should successfully copy bytes, resulting in 41"
    );
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_c_interop_sqrt_e2e() {
    let mut native_mod = NativeModule::new("test_c_sqrt");
    let mut main_fn = MachineFunction::new("main");
    main_fn.is_exported = true;
    main_fn.stack_size = 48;
    let entry = main_fn.entry_block_mut();

    // 144.0 as f64
    let val_f64 = 144.0f64;
    // Load 144.0 bit pattern into XMM0 (phys 16)
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::phys(16),
        src: MachineOperand::Immediate(val_f64.to_bits() as i64),
    });

    // Shadow space
    entry.push(MachineInstruction::Sub {
        dst: MachineOperand::phys(4),
        src: MachineOperand::Immediate(32),
    });
    // Call sqrt
    entry.push(MachineInstruction::Call {
        target: MachineOperand::Symbol("sqrt".to_string()),
        num_args: 1,
    });
    entry.push(MachineInstruction::Add {
        dst: MachineOperand::phys(4),
        src: MachineOperand::Immediate(32),
    });

    // Result in XMM0 (phys 16) is 12.0
    // Convert XMM0 to int in RAX (phys 0)
    entry.push(MachineInstruction::FCvtFloatToInt {
        dst: MachineOperand::phys(0),
        src: MachineOperand::phys(16),
        is_f64: true,
        is_signed: true,
    });
    entry.push(MachineInstruction::Return);
    native_mod.add_function(main_fn);

    let (code, _) = emit_link_and_run(&native_mod, OptLevel::O0, "c_sqrt");
    assert_eq!(
        code, 12,
        "sqrt(144.0) should return 12.0 converted to int 12"
    );
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_c_interop_printf_variadic_e2e() {
    let mut native_mod = NativeModule::new("test_c_printf");
    // Format string: "Val: %d, Str: %s\n\0"
    let fmt_bytes = b"Val: %d, Str: %s\n\0";
    let str_bytes = b"VERIFIED\0";
    native_mod
        .data_sections
        .push(("fmt_str".to_string(), fmt_bytes.to_vec()));
    native_mod
        .data_sections
        .push(("arg_str".to_string(), str_bytes.to_vec()));

    let mut main_fn = MachineFunction::new("main");
    main_fn.is_exported = true;
    main_fn.stack_size = 48;
    let entry = main_fn.entry_block_mut();

    // 1. RCX = &fmt_str
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::phys(1),
        src: MachineOperand::Symbol("fmt_str".to_string()),
    });

    // 2. RDX = 42
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::phys(2),
        src: MachineOperand::Immediate(42),
    });

    // 3. R8 = &arg_str
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::phys(8),
        src: MachineOperand::Symbol("arg_str".to_string()),
    });

    // 4. Shadow space
    entry.push(MachineInstruction::Sub {
        dst: MachineOperand::phys(4),
        src: MachineOperand::Immediate(32),
    });
    // Call printf
    entry.push(MachineInstruction::Call {
        target: MachineOperand::Symbol("printf".to_string()),
        num_args: 3,
    });
    entry.push(MachineInstruction::Add {
        dst: MachineOperand::phys(4),
        src: MachineOperand::Immediate(32),
    });

    // Return 0 from main
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::phys(0),
        src: MachineOperand::Immediate(0),
    });
    entry.push(MachineInstruction::Return);
    native_mod.add_function(main_fn);

    let (code, stdout) = emit_link_and_run(&native_mod, OptLevel::O0, "c_printf");
    assert_eq!(code, 0, "main should exit 0");
    assert!(
        stdout.contains("Val: 42, Str: VERIFIED"),
        "stdout should contain formatted string, but got: {:?}",
        stdout
    );
}
