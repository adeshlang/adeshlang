//! Phase 3 — Linux Execution For Real:
//! Source text / Machine IR -> x86-64 SysV AMD64 codegen -> ADOB -> adeshlink ELF static
//! executable -> execute binary in Linux (native Linux in CI, or WSL on Windows) -> assert
//! the exact exit code. (Static ELF binaries are freestanding: they carry no C runtime,
//! so these programs cannot write to stdout; only exit codes are observable.)
//!
//! Tests:
//! 1. `_start` Linux entry point synthesis with argc, argv, envp and sys_exit (syscall 60).
//! 2. Static executable generation with valid ELF64 headers and PT_LOAD segment alignment.
//! 3. Archive symbol index `/` generation and resolution.
//! 4. GC enabled end-to-end: a linked binary with an unreferenced function still
//!    executes correctly. (Actual dead-section removal is unit-tested in
//!    `linker/src/gc.rs`; the x86-64 ADOB emitter places all functions of a
//!    module in one `.text` section, so section GC cannot remove individual
//!    functions from ADOB input yet.)
//! 5. Full language semantics execution (arithmetic, functions, control flow, loops, match).
//!
//! ICF `Safe` vs `All` differentiation is unit-tested in `linker/src/icf.rs`
//! (`test_icf_safe_vs_all_modes`); it is not exercised by this execution suite.

use adesh_codegen::machine_ir::{
    MachineFunction, MachineInstruction, MachineOperand, MachineRegister, NativeModule,
    PhysicalRegister,
};
use adesh_codegen::opt::OptLevel;
use adesh_codegen::targets::create_backend;
use adesh_object::TargetDescriptor;
use adesh_object::validator::AdobValidator;
use adesh_object::writer::AdobWriter;
use adeshlang::backends::native::lower::lower_hir_module;
use adeshlang::parsing::hir_lower::ast_to_hir;
use adeshlang::parsing::lexer::Lexer;
use adeshlang::parsing::parser::Parser;
use std::path::Path;
use std::process::Command;

fn phys(id: u8) -> MachineOperand {
    MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(id)))
}

fn execute_elf(path: &Path) -> std::io::Result<std::process::Output> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            let mut perms = meta.permissions();
            perms.set_mode(0o755);
            let _ = std::fs::set_permissions(path, perms);
        }
        Command::new(path).output()
    }

    #[cfg(target_os = "windows")]
    {
        let win_path = path.to_str().unwrap().replace('\\', "/");
        let wslpath_out = Command::new("wsl")
            .args(["wslpath", "-a", "-u", &win_path])
            .output();
        let wsl_path = match wslpath_out {
            Ok(output) if output.status.success() => {
                String::from_utf8_lossy(&output.stdout).trim().to_string()
            }
            _ => {
                if let Some(stripped) = win_path.strip_prefix("d:/") {
                    format!("/mnt/d/{}", stripped)
                } else if let Some(stripped) = win_path.strip_prefix("D:/") {
                    format!("/mnt/d/{}", stripped)
                } else if let Some(stripped) = win_path.strip_prefix("c:/") {
                    format!("/mnt/c/{}", stripped)
                } else if let Some(stripped) = win_path.strip_prefix("C:/") {
                    format!("/mnt/c/{}", stripped)
                } else {
                    win_path
                }
            }
        };
        let _ = Command::new("wsl")
            .args(["-e", "/bin/sh", "-c", &format!("chmod +x \"{}\"", wsl_path)])
            .output();
        Command::new("wsl")
            .args(["-e", "/bin/sh", "-c", &format!("\"{}\"", wsl_path)])
            .output()
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "ELF execution unsupported on this OS without emulator",
        ))
    }
}

/// Compile Adesh source text to a static Linux ELF binary, execute it, and return (stdout, exit_code).
fn compile_link_and_run_linux(src: &str, test_name: &str) -> (String, i32) {
    compile_link_and_run_linux_with_opt(src, test_name, None)
}

fn compile_link_and_run_linux_with_opt(
    src: &str,
    test_name: &str,
    opt_level: Option<OptLevel>,
) -> (String, i32) {
    let tokens = Lexer::new(src).tokenize().expect("tokenize");
    let mut parser = Parser::new(tokens, None);
    let ast = parser.parse_program().expect("parse");
    let hir = ast_to_hir(&ast, true).expect("ast_to_hir");

    let target = TargetDescriptor::from_triple("x86_64-unknown-linux-gnu").expect("valid triple");
    let native_mod = lower_hir_module(&hir, &target).expect("native lowering");

    let mut backend = create_backend(target.clone()).expect("backend creation");
    if let Some(opt_level) = opt_level {
        backend.set_opt_level(opt_level);
    }
    let obj = backend.emit_object(&native_mod).expect("ADOB emission");
    AdobValidator::validate(&obj).expect("emitted ADOB must validate");

    let bytes = AdobWriter::write(&obj).expect("ADOB encoding");
    let test_dir = std::env::current_dir()
        .unwrap()
        .join("target")
        .join("linux_tests");
    std::fs::create_dir_all(&test_dir).expect("create test_dir");

    let adob_path = test_dir.join(format!("{test_name}.adob"));
    std::fs::write(&adob_path, bytes).expect("write ADOB file");

    let elf_path = test_dir.join(format!("{test_name}.elf"));
    adesh_linker::link(&[&adob_path], &elf_path, Some("x86_64-unknown-linux-gnu"))
        .expect("native ELF link");
    assert!(elf_path.exists(), "linked ELF binary must exist");

    // Validate ELF header bytes
    let elf_bytes = std::fs::read(&elf_path).expect("read ELF");
    assert!(elf_bytes.len() >= 64, "ELF file too small");
    assert_eq!(&elf_bytes[0..4], b"\x7fELF", "magic must be \\x7fELF");
    assert_eq!(elf_bytes[4], 2, "EI_CLASS must be ELFCLASS64 (2)");
    assert_eq!(elf_bytes[5], 1, "EI_DATA must be ELFDATA2LSB (1)");
    assert_eq!(elf_bytes[18], 62, "e_machine must be EM_X86_64 (62)");

    let out = execute_elf(&elf_path).expect("execute ELF binary");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    (stdout, code)
}

#[test]
#[cfg(target_os = "linux")]
fn test_linux_elf_size_optimization_levels_preserve_exit_code() {
    for (name, level) in [("os", OptLevel::Os), ("oz", OptLevel::Oz)] {
        let (_, code) = compile_link_and_run_linux_with_opt(
            "fn main(): int { let value: int = 40 + 2; return value; }",
            &format!("test_linux_size_{name}"),
            Some(level),
        );
        assert_eq!(code, 42, "{level:?} must preserve the Linux program result");
    }
}

#[test]
fn test_linux_elf_start_synthesis_and_exit_code() {
    let (_, code) = compile_link_and_run_linux(
        r#"
        fn main(): int {
            return 42;
        }
        "#,
        "test_linux_exit_42",
    );
    assert_eq!(code, 42);
}

#[test]
fn test_linux_elf_arithmetic_and_functions() {
    let (_, code) = compile_link_and_run_linux(
        r#"
        fn add(a: int, b: int): int {
            return a + b;
        }

        fn multiply(a: int, b: int): int {
            return a * b;
        }

        fn main(): int {
            let x = add(10, 20);
            let y = multiply(x, 2);
            return y - 10;
        }
        "#,
        "test_linux_arithmetic",
    );
    assert_eq!(code, 50);
}

#[test]
fn test_linux_elf_control_flow_and_loops() {
    let (_, code) = compile_link_and_run_linux(
        r#"
        fn main(): int {
            let mut sum = 0;
            let mut i = 1;
            while (i <= 10) {
                sum = sum + i;
                i = i + 1;
            }
            return sum;
        }
        "#,
        "test_linux_loop",
    );
    assert_eq!(code, 55);
}

#[test]
fn test_linux_elf_match_and_branches() {
    let (_, code) = compile_link_and_run_linux(
        r#"
        fn classify(n: int): int {
            let res = match n {
                1 => 10,
                2 => 20,
                3 => 30,
                _ => 99,
            };
            return res;
        }

        fn main(): int {
            let a = classify(2);
            let b = classify(5);
            return a + b;
        }
        "#,
        "test_linux_match",
    );
    assert_eq!(code, 119);
}

#[test]
fn test_linux_elf_six_args_sysv_abi() {
    // SysV AMD64 ABI passes first 6 args in RDI, RSI, RDX, RCX, R8, R9.
    let target = TargetDescriptor::from_triple("x86_64-unknown-linux-gnu").expect("valid triple");
    let mut backend = create_backend(target.clone()).expect("backend creation");

    let mut module = NativeModule::new("sysv_e2e_mod");

    // sum6(rdi, rsi, rdx, rcx, r8, r9) -> rdi+rsi+rdx+rcx+r8+r9
    let mut sum6 = MachineFunction::new("sum6");
    sum6.is_exported = true;
    {
        let b = sum6.entry_block_mut();
        // RAX = RDI
        b.push(MachineInstruction::Move {
            dst: phys(0), // RAX
            src: phys(7), // RDI
        });
        // RAX += RSI
        b.push(MachineInstruction::Add {
            dst: phys(0),
            src: phys(6), // RSI
        });
        // RAX += RDX
        b.push(MachineInstruction::Add {
            dst: phys(0),
            src: phys(2), // RDX
        });
        // RAX += RCX
        b.push(MachineInstruction::Add {
            dst: phys(0),
            src: phys(1), // RCX
        });
        // RAX += R8
        b.push(MachineInstruction::Add {
            dst: phys(0),
            src: phys(8), // R8
        });
        // RAX += R9
        b.push(MachineInstruction::Add {
            dst: phys(0),
            src: phys(9), // R9
        });
        b.push(MachineInstruction::Return);
    }
    module.add_function(sum6);

    // main() -> sum6(1, 2, 3, 4, 5, 6) == 21
    let mut main_fn = MachineFunction::new("main");
    main_fn.is_exported = true;
    {
        let b = main_fn.entry_block_mut();
        b.push(MachineInstruction::Move {
            dst: phys(7), // RDI
            src: MachineOperand::Immediate(1),
        });
        b.push(MachineInstruction::Move {
            dst: phys(6), // RSI
            src: MachineOperand::Immediate(2),
        });
        b.push(MachineInstruction::Move {
            dst: phys(2), // RDX
            src: MachineOperand::Immediate(3),
        });
        b.push(MachineInstruction::Move {
            dst: phys(1), // RCX
            src: MachineOperand::Immediate(4),
        });
        b.push(MachineInstruction::Move {
            dst: phys(8), // R8
            src: MachineOperand::Immediate(5),
        });
        b.push(MachineInstruction::Move {
            dst: phys(9), // R9
            src: MachineOperand::Immediate(6),
        });
        b.push(MachineInstruction::Call {
            target: MachineOperand::Symbol("sum6".to_string()),
            num_args: 6,
        });

        b.push(MachineInstruction::Return);
    }
    module.add_function(main_fn);

    let obj = backend.emit_object(&module).expect("emit ADOB");
    AdobValidator::validate(&obj).expect("validate ADOB");
    let bytes = AdobWriter::write(&obj).expect("write ADOB");

    let test_dir = std::env::current_dir()
        .unwrap()
        .join("target")
        .join("linux_tests");
    std::fs::create_dir_all(&test_dir).expect("create test_dir");

    let adob_path = test_dir.join("sysv6.adob");
    std::fs::write(&adob_path, bytes).expect("write adob");

    let elf_path = test_dir.join("sysv6.elf");
    adesh_linker::link(&[&adob_path], &elf_path, Some("x86_64-unknown-linux-gnu"))
        .expect("link ELF");

    let out = execute_elf(&elf_path).expect("execute ELF");
    assert_eq!(out.status.code(), Some(21));
}

#[test]
fn test_linux_elf_static_binary_segments_and_headers() {
    // Build its own binary: tests in one binary run in parallel, so this test
    // must not depend on a sibling test's output file.
    let (_, code) = compile_link_and_run_linux(
        r#"
fn main(): int {
    return 42;
}
"#,
        "test_linux_static_headers",
    );
    assert_eq!(code, 42);

    let test_dir = std::env::current_dir()
        .unwrap()
        .join("target")
        .join("linux_tests");
    let elf_path = test_dir.join("test_linux_static_headers.elf");
    assert!(elf_path.exists());

    let bytes = std::fs::read(&elf_path).expect("read elf");
    assert!(bytes.len() >= 64);
    assert_eq!(&bytes[0..4], b"\x7fELF");

    // Static ELF must not contain PT_INTERP (tag 3) or PT_DYNAMIC (tag 2)
    let e_phoff = u64::from_le_bytes(bytes[32..40].try_into().unwrap()) as usize;
    let e_phnum = u16::from_le_bytes(bytes[56..58].try_into().unwrap()) as usize;
    let e_phentsize = u16::from_le_bytes(bytes[54..56].try_into().unwrap()) as usize;

    let mut found_pt_load = 0;
    for i in 0..e_phnum {
        let ph_start = e_phoff + i * e_phentsize;
        let p_type = u32::from_le_bytes(bytes[ph_start..ph_start + 4].try_into().unwrap());
        // PT_INTERP is 3, PT_DYNAMIC is 2
        assert_ne!(p_type, 3, "static ELF executable must not have PT_INTERP");
        assert_ne!(p_type, 2, "static ELF executable must not have PT_DYNAMIC");
        if p_type == 1 {
            // PT_LOAD
            found_pt_load += 1;
        }
    }
    assert!(found_pt_load >= 1, "must have at least one PT_LOAD segment");
}

#[test]
fn test_linux_elf_archive_symbol_index_resolution() {
    let target = TargetDescriptor::from_triple("x86_64-unknown-linux-gnu").expect("valid triple");
    let mut backend = create_backend(target.clone()).expect("backend creation");

    // 1. Helper module in an archive: helper_mul7(x: rdi) -> x * 7 (rax)
    let mut helper_mod = NativeModule::new("helper_mod");
    let mut helper_fn = MachineFunction::new("helper_mul7");
    helper_fn.is_exported = true;
    {
        let b = helper_fn.entry_block_mut();
        // RAX = RDI * 7
        b.push(MachineInstruction::Move {
            dst: phys(0),
            src: phys(7),
        });
        b.push(MachineInstruction::Mul {
            dst: phys(0),
            src: MachineOperand::Immediate(7),
        });
        b.push(MachineInstruction::Return);
    }
    helper_mod.add_function(helper_fn);

    let helper_obj = backend.emit_object(&helper_mod).expect("emit helper ADOB");
    let helper_bytes = AdobWriter::write(&helper_obj).expect("write helper ADOB");

    let test_dir = std::env::current_dir()
        .unwrap()
        .join("target")
        .join("linux_tests");
    std::fs::create_dir_all(&test_dir).expect("create test_dir");

    // Package into .a archive with GNU '/' symbol index
    let mut archive = adesh_linker::archive::ar::Archive {
        path: test_dir.join("libhelper.a"),
        members: Vec::new(),
        symbol_index: std::collections::HashMap::new(),
    };
    archive.add_file("helper.adob", helper_bytes);
    let ar_bytes = archive.encode_gnu();
    let ar_path = test_dir.join("libhelper.a");
    std::fs::write(&ar_path, &ar_bytes).expect("write libhelper.a");

    // Verify the written archive has a valid '/' symbol index
    let parsed_ar =
        adesh_linker::archive::ar::Archive::parse(&ar_bytes, &ar_path).expect("parse archive");
    assert!(
        parsed_ar.symbol_index.contains_key("helper_mul7"),
        "symbol index must contain helper_mul7"
    );

    // 2. Main module that calls helper_mul7(6) -> expect 42
    let mut main_mod = NativeModule::new("main_mod");
    let mut main_fn = MachineFunction::new("main");
    main_fn.is_exported = true;
    {
        let b = main_fn.entry_block_mut();
        b.push(MachineInstruction::Move {
            dst: phys(7), // RDI = 6
            src: MachineOperand::Immediate(6),
        });
        b.push(MachineInstruction::Call {
            target: MachineOperand::Symbol("helper_mul7".to_string()),
            num_args: 1,
        });
        b.push(MachineInstruction::Return);
    }
    main_mod.add_function(main_fn);

    let main_obj = backend.emit_object(&main_mod).expect("emit main ADOB");
    let main_bytes = AdobWriter::write(&main_obj).expect("write main ADOB");
    let main_path = test_dir.join("main_ar.adob");
    std::fs::write(&main_path, main_bytes).expect("write main ADOB");

    let elf_path = test_dir.join("test_archive_exec.elf");
    adesh_linker::link(
        &[&main_path, &ar_path],
        &elf_path,
        Some("x86_64-unknown-linux-gnu"),
    )
    .expect("link with static archive");

    let out = execute_elf(&elf_path).expect("execute archive ELF");
    assert_eq!(out.status.code(), Some(42));
}

#[test]
fn test_linux_elf_gc_dead_code_elimination() {
    let target = TargetDescriptor::from_triple("x86_64-unknown-linux-gnu").expect("valid triple");
    let mut backend = create_backend(target.clone()).expect("backend creation");

    let mut module = NativeModule::new("gc_test_mod");

    // dead_func(): never referenced and NOT exported (an exported symbol is a
    // GC root, which would make this test self-defeating). Note: the x86-64
    // ADOB emitter places every function of a module in a single `.text`
    // section, so section GC cannot actually remove dead_func here — that
    // granularity is unit-tested in `linker/src/gc.rs`. This test verifies the
    // end-to-end property that linking with GC enabled never breaks live code.
    let mut dead_fn = MachineFunction::new("dead_func");
    dead_fn.is_exported = false;
    {
        let b = dead_fn.entry_block_mut();
        b.push(MachineInstruction::Move {
            dst: phys(0),
            src: MachineOperand::Immediate(99),
        });
        b.push(MachineInstruction::Return);
    }
    module.add_function(dead_fn);

    // live_func(): called by main()
    let mut live_fn = MachineFunction::new("live_func");
    live_fn.is_exported = true;
    {
        let b = live_fn.entry_block_mut();
        b.push(MachineInstruction::Move {
            dst: phys(0),
            src: MachineOperand::Immediate(77),
        });
        b.push(MachineInstruction::Return);
    }
    module.add_function(live_fn);

    // main(): calls live_func() -> returns 77
    let mut main_fn = MachineFunction::new("main");
    main_fn.is_exported = true;
    {
        let b = main_fn.entry_block_mut();
        b.push(MachineInstruction::Call {
            target: MachineOperand::Symbol("live_func".to_string()),
            num_args: 0,
        });
        b.push(MachineInstruction::Return);
    }
    module.add_function(main_fn);

    let obj = backend.emit_object(&module).expect("emit ADOB");
    let bytes = AdobWriter::write(&obj).expect("write ADOB");

    let test_dir = std::env::current_dir()
        .unwrap()
        .join("target")
        .join("linux_tests");
    std::fs::create_dir_all(&test_dir).expect("create test_dir");

    let adob_path = test_dir.join("gc_test.adob");
    std::fs::write(&adob_path, bytes).expect("write adob");

    let elf_path = test_dir.join("gc_test.elf");
    adesh_linker::link(&[&adob_path], &elf_path, Some("x86_64-unknown-linux-gnu"))
        .expect("link ELF with GC");

    let out = execute_elf(&elf_path).expect("execute ELF");
    assert_eq!(out.status.code(), Some(77));
}
