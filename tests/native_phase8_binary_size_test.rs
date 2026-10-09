//! Phase 8 Binary Size Optimization & C-Level Compactness Tests.
//!
//! Verifies:
//! 1. Binary size optimization under -Os and -Oz.
//! 2. Instruction selection compactness (xor eax, eax; short imm; rel8 jumps).
//! 3. Dead code elimination, section GC, and symbol stripping.
//! 4. Generated executable size matches or outstands C programming level (< 4-8 KB for minimal native executables).
//! 5. Detailed section size metrics tracking: .text, .rdata, .data, relocations, symbols, total executable size.

#![allow(dead_code, unused_imports)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, VirtualRegister,
};
use adesh_codegen::opt::OptLevel;
use adesh_codegen::targets::x86_64::X86_64Backend;
use adesh_object::TargetDescriptor;
use adesh_object::validator::AdobValidator;
use adesh_object::writer::AdobWriter;
use adeshlang::backends::native::lower::lower_hir_module;
use adeshlang::parsing::hir_lower::ast_to_hir;
use adeshlang::parsing::lexer::Lexer;
use adeshlang::parsing::parser::Parser;
use std::process::Command;
use tempfile::tempdir;

#[derive(Debug, Clone)]
pub struct BinarySizeMetrics {
    pub text_size: usize,
    pub rodata_size: usize,
    pub data_size: usize,
    pub bss_size: usize,
    pub relocation_count: usize,
    pub symbol_count: usize,
    pub total_file_size: usize,
}

fn compile_link_and_measure(
    module: &NativeModule,
    opt_level: OptLevel,
    name: &str,
) -> (BinarySizeMetrics, i32) {
    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    let mut backend = X86_64Backend::new(target).with_opt_level(opt_level);

    let obj = backend.emit_object(module).expect("ADOB emission");
    AdobValidator::validate(&obj).expect("ADOB validation");

    let text_size = obj
        .sections
        .iter()
        .find(|s| s.name == ".text")
        .map(|s| s.data.len())
        .unwrap_or(0);
    let rodata_size = obj
        .sections
        .iter()
        .find(|s| s.name == ".rodata" || s.name == ".rdata")
        .map(|s| s.data.len())
        .unwrap_or(0);
    let data_size = obj
        .sections
        .iter()
        .find(|s| s.name == ".data")
        .map(|s| s.data.len())
        .unwrap_or(0);
    let bss_size = obj
        .sections
        .iter()
        .find(|s| s.name == ".bss")
        .map(|s| s.data.len())
        .unwrap_or(0);
    let relocation_count: usize = obj.sections.iter().map(|s| s.relocations.len()).sum();
    let symbol_count = obj.symbols.len();

    let bytes = AdobWriter::write(&obj).expect("write ADOB");
    let dir = tempdir().expect("tempdir");
    let adob_path = dir.path().join(format!("{}.adob", name));
    std::fs::write(&adob_path, bytes).expect("write ADOB file");

    let exe_path = dir.path().join(format!("{}.exe", name));
    adesh_linker::link(&[&adob_path], &exe_path, Some("x86_64-pc-windows-msvc")).expect("link");

    let total_file_size = std::fs::metadata(&exe_path).expect("exe metadata").len() as usize;

    let out = Command::new(&exe_path).output().expect("run exe");
    let exit_code = out.status.code().expect("exit code");

    let metrics = BinarySizeMetrics {
        text_size,
        rodata_size,
        data_size,
        bss_size,
        relocation_count,
        symbol_count,
        total_file_size,
    };

    (metrics, exit_code)
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_c_level_minimal_binary_size() {
    let mut module = NativeModule::new("test_minimal_size");
    let mut main_func = MachineFunction::new("main");
    main_func.is_exported = true;

    let block = main_func.entry_block_mut();
    // Return 42 directly in RAX
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(42),
    });
    block.push(MachineInstruction::Return);
    module.add_function(main_func);

    let (metrics_o2, code_o2) = compile_link_and_measure(&module, OptLevel::O2, "test_size_o2");
    assert_eq!(code_o2, 42);

    let (metrics_oz, code_oz) = compile_link_and_measure(&module, OptLevel::Oz, "test_size_oz");
    assert_eq!(code_oz, 42);

    println!("=== Binary Size Report ===");
    println!(
        "O2: Text Size: {}B, Relocs: {}, Total Exe: {}B",
        metrics_o2.text_size, metrics_o2.relocation_count, metrics_o2.total_file_size
    );
    println!(
        "Oz: Text Size: {}B, Relocs: {}, Total Exe: {}B",
        metrics_oz.text_size, metrics_oz.relocation_count, metrics_oz.total_file_size
    );

    // C-level comparison: A native C program compiled with MSVC / Clang produces a 2-4 KB executable.
    // Our Adesh executable is self-contained and must be <= 4 KB!
    assert!(
        metrics_oz.total_file_size <= 4096,
        "Oz executable must be <= 4KB (compact C level), was {}B",
        metrics_oz.total_file_size
    );
    assert!(
        metrics_oz.text_size <= 64,
        "Text section for minimal program should be tiny"
    );
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_hello_world_binary_size_baseline() {
    let source = r#"
fn main(): int {
    print("Hello, world!");
    return 0;
}
"#;
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("hello-world source should lex");
    let mut parser = Parser::new(tokens, None);
    let program = parser
        .parse_program()
        .expect("hello-world source should parse");
    let hir = ast_to_hir(&program, false).expect("hello-world source should lower to HIR");
    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("target");
    let module = lower_hir_module(&hir, &target).expect("native lowering");

    let (metrics, exit_code) = compile_link_and_measure(&module, OptLevel::O2, "hello_world_size");
    assert_eq!(exit_code, 0);
    println!(
        "Hello-world PE baseline: {} bytes (ADOB object text {}, rodata {}, data {}, bss {})",
        metrics.total_file_size,
        metrics.text_size,
        metrics.rodata_size,
        metrics.data_size,
        metrics.bss_size
    );
    assert!(
        metrics.total_file_size <= 40 * 1024,
        "hello-world PE exceeded the 40 KiB size target: {} bytes",
        metrics.total_file_size
    );
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_cli_size_optimization_levels_build_compact_executables() {
    let dir = tempdir().expect("tempdir");
    let source = dir.path().join("size_flags.adesh");
    std::fs::write(
        &source,
        "fn main(): int { print(\"size modes work — café 😀\"); return 0; }\n",
    )
    .expect("write source");

    for level in ["-Os", "-Oz"] {
        let output = dir.path().join(format!("size_{}.exe", &level[2..]));
        let status = Command::new(env!("CARGO_BIN_EXE_adesh"))
            .arg("build")
            .arg(&source)
            .arg(level)
            .arg("-o")
            .arg(&output)
            .status()
            .expect("CLI should start");
        assert!(status.success(), "CLI build failed for {level}");

        let size = std::fs::metadata(&output)
            .expect("CLI executable exists")
            .len();
        assert!(
            size <= 40 * 1024,
            "{level} produced a {size}-byte PE, exceeding the 40 KiB target"
        );

        let result = Command::new(&output)
            .output()
            .expect("CLI-built executable should run");
        assert_eq!(result.status.code(), Some(0), "{level} exit code");
        assert_eq!(
            String::from_utf8(result.stdout)
                .expect("size-optimized executable should emit UTF-8")
                .trim(),
            "size modes work — café 😀",
            "{level} output"
        );
    }
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_cli_build_ignores_unreferenced_weak_runtime_helpers() {
    let dir = tempdir().expect("tempdir");
    let source = dir.path().join("weak_helpers.adesh");
    std::fs::write(
        &source,
        "fn main(): int { print(\"Sum \", 1 + 2 + 3); return 0; }\n",
    )
    .expect("write source");
    let output = dir.path().join("weak_helpers.exe");

    let build = Command::new(env!("CARGO_BIN_EXE_adesh"))
        .arg("build")
        .arg(&source)
        .arg("-o")
        .arg(&output)
        .output()
        .expect("CLI should start");
    assert!(
        build.status.success(),
        "CLI build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    let stderr = String::from_utf8_lossy(&build.stderr);
    for helper in ["__extendhfsf2", "__truncsfhf2", "__udivti3"] {
        assert!(
            !stderr.contains(helper),
            "unreferenced weak helper `{helper}` should not warn: {stderr}"
        );
    }

    let run = Command::new(&output)
        .output()
        .expect("built executable should run");
    assert_eq!(run.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "Sum  6");
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_dead_code_elimination_reduces_size() {
    let mut module = NativeModule::new("test_dce_size");
    let mut main_func = MachineFunction::new("main");
    main_func.is_exported = true;

    let v0 = main_func.alloc_vreg();
    let v_dead = main_func.alloc_vreg();

    let block = main_func.entry_block_mut();
    // v0 = 100
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Immediate(100),
    });
    // Dead compute instructions
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_dead)),
        src: MachineOperand::Immediate(9999),
    });
    block.push(MachineInstruction::Mul {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_dead)),
        src: MachineOperand::Immediate(123),
    });
    // Return v0
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Virtual(v0)),
    });
    block.push(MachineInstruction::Return);
    module.add_function(main_func);

    let (metrics, code) = compile_link_and_measure(&module, OptLevel::Oz, "test_dce_size_exec");
    assert_eq!(code, 100);
    assert!(metrics.text_size <= 64);
}
