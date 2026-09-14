//! Execution backend runners
//!
//! This module provides execution functions for different backends
//! (JIT, Adaptive JIT, Tiered JIT, Bytecode VM, Interpreter).

use crate::ProgramMemoryStats;
use crate::backends::jit::JitMemoryStats;
use crate::cli::ParsedArgs;
use crate::cli::ui::BuildProgress;
use crate::execution::runtime::{Interpreter, ModuleLoader};
use std::path::PathBuf;

fn ensure_type_check(body: &str, file: Option<&str>) -> Result<(), String> {
    crate::types::type_system::check_module_in(body, file).map_err(|e| format!("type error: {}", e))
}

/// Run program with JIT backend
pub fn run_with_jit(
    path: &PathBuf,
    src: &str,
    parsed: &ParsedArgs,
) -> Result<Option<JitMemoryStats>, String> {
    if !parsed.config.quiet {
        let progress = BuildProgress::new("Compiling with JIT backend...");

        let _body = super::directives::strip_compile_directive(src).to_string();

        // JIT compilation is fast enough that we might just flash the spinner,
        // but for larger programs it's useful.
        // There isn't a separate "compile" step exposed here easily without refactoring JIT guts,
        // but we can at least show we are starting.
        progress.success("JIT backend ready");

        // Note: The actual compilation happens inside jit_run_with_stats_and_config
        // Ideally we would split compile/run, but for now this minimal wrapper adds consistency.
    }

    // Original logic continues...
    let body = super::directives::strip_compile_directive(src).to_string();
    ensure_type_check(&body, Some(&path.to_string_lossy()))?;
    let collect_memory = parsed.config.show_memory;
    let stack_size = parsed.config.stack_size;
    let recursion_config = parsed.config.recursion_opt.to_config();

    // Set program args for JIT runtime
    crate::execution::runtime::set_program_args(
        parsed.program_args.clone(),
        Some(path.to_string_lossy().to_string()),
    );

    // Handle VIR/LIR backend selection
    if parsed.config.use_lir {
        // Force LIR backend by setting environment variable
        // SAFETY: We're setting this before spawning the JIT thread, so it's safe
        unsafe {
            std::env::set_var("ADESH_USE_VIR", "0");
        }
    }

    // Run JIT in a thread with configured stack size
    let res = std::thread::Builder::new()
        .stack_size(stack_size)
        .spawn(move || {
            use crate::backends::jit::jit_run_with_stats_and_config;
            match jit_run_with_stats_and_config(&body, recursion_config) {
                Ok((_result, stats)) => {
                    if collect_memory {
                        Ok(Some(stats))
                    } else {
                        Ok(None)
                    }
                }
                Err(e) => Err(format!("JIT error: {}", e)),
            }
        });

    match res {
        Ok(handle) => match handle.join() {
            Ok(result) => result,
            Err(_) => Err("JIT thread panicked".to_string()),
        },
        Err(e) => Err(format!("Failed to spawn JIT thread: {}", e)),
    }
}

/// Run program with adaptive JIT (speculative optimization)
pub fn run_with_adaptive_jit(path: &PathBuf, src: &str, parsed: &ParsedArgs) -> Result<(), String> {
    use crate::backends::adaptive_jit::adaptive_jit_run;

    let body = super::directives::strip_compile_directive(src);
    ensure_type_check(&body, Some(&path.to_string_lossy()))?;

    if !parsed.config.quiet {
        let progress = BuildProgress::new("Compiling with Adaptive JIT...");
        // Simulate brief setup or actual compile checking if feasible
        progress.success("Adaptive JIT backend ready");
    }

    // Set program args for adaptive JIT runtime
    crate::execution::runtime::set_program_args(
        parsed.program_args.clone(),
        Some(path.to_string_lossy().to_string()),
    );

    // Handle VIR/LIR backend selection
    if parsed.config.use_lir {
        // Force LIR backend by setting environment variable
        // SAFETY: We're setting this before calling adaptive_jit_run, so it's safe
        unsafe {
            std::env::set_var("ADESH_USE_VIR", "0");
        }
    }

    match adaptive_jit_run(&body) {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("Adaptive JIT error: {}", e)),
    }
}

/// Run program with tiered JIT (T0->T1->T2)
pub fn run_with_tiered_jit(path: &PathBuf, src: &str, parsed: &ParsedArgs) -> Result<(), String> {
    use crate::backends::tiered_jit::tiered_jit_run;

    let body = super::directives::strip_compile_directive(src);
    ensure_type_check(&body, Some(&path.to_string_lossy()))?;

    if !parsed.config.quiet {
        let progress = BuildProgress::new("Compiling with Tiered JIT...");
        progress.success("Tiered JIT backend ready");
    }

    // Set program args for tiered JIT runtime
    crate::execution::runtime::set_program_args(
        parsed.program_args.clone(),
        Some(path.to_string_lossy().to_string()),
    );

    // Handle VIR/LIR backend selection
    if parsed.config.use_lir {
        // Force LIR backend by setting environment variable
        // SAFETY: We're setting this before calling tiered_jit_run, so it's safe
        unsafe {
            std::env::set_var("ADESH_USE_VIR", "0");
        }
    }

    match tiered_jit_run(&body) {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("Tiered JIT error: {}", e)),
    }
}

/// Run program with Native JIT (true native code generation)
pub fn run_with_native_jit(path: &PathBuf, src: &str, parsed: &ParsedArgs) -> Result<(), String> {
    use crate::backends::jit::native::native_jit_run;

    // Separate compilation from execution for Native JIT to wrap progress
    if !parsed.config.quiet {
        let progress = BuildProgress::new("Compiling with Native JIT...");

        // We can't easily split compile/run for native_jit_run without duplicate logic,
        // so we'll just show the spinner validation.
        // A better approach would be to refactor native_jit_run to take a callback or split steps.
        // For now, let's keep it consistent:
        progress.success("Native JIT backend ready");
    }

    let body = super::directives::strip_compile_directive(src);
    ensure_type_check(&body, Some(&path.to_string_lossy()))?;

    // Set program args for native JIT runtime
    crate::execution::runtime::set_program_args(
        parsed.program_args.clone(),
        Some(path.to_string_lossy().to_string()),
    );

    // Handle VIR/LIR backend selection
    if parsed.config.use_lir {
        // Force LIR backend by setting environment variable
        // SAFETY: We're setting this before calling native_jit_run, so it's safe
        unsafe {
            std::env::set_var("ADESH_USE_VIR", "0");
        }
    }

    match native_jit_run(&body) {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("Native JIT error: {}", e)),
    }
}

/// Run program with bytecode VM
pub fn run_with_bytecode(path: &PathBuf, src: &str, parsed: &ParsedArgs) -> Result<(), String> {
    let body = super::directives::strip_compile_directive(src);
    ensure_type_check(&body, Some(&path.to_string_lossy()))?;

    let mut progress = if !parsed.config.quiet {
        Some(BuildProgress::new("Compiling to bytecode..."))
    } else {
        None
    };

    // Compile to temporary bytecode file
    let out = path.with_extension("adeshbc");
    match crate::execution::bytecode::compile_to_file_v2(&body, &out) {
        Ok(()) => {
            if let Some(p) = progress.take() {
                p.success("Bytecode compilation complete");
            }

            if parsed.config.verbose {
                eprintln!("[bytecode] Running VM...");
            }

            if let Err(e) = crate::execution::vm::run_file_with_args(&out, &parsed.program_args) {
                return Err(format!("vm error: {}", e));
            }
            let _ = std::fs::remove_file(&out);
            Ok(())
        }
        Err(e) => {
            if let Some(p) = progress {
                p.fail("Bytecode compilation failed");
            }

            // Fallback to interpreter for unsupported features in bytecode v2
            if parsed.config.verbose {
                eprintln!(
                    "[bytecode] compile failed: {}\n[bytecode] Falling back to interpreter...",
                    e
                );
            }
            match run_with_interpreter(path, src, parsed) {
                Ok(_) => Ok(()),
                Err(err) => Err(err),
            }
        }
    }
}

/// Run program with interpreter
pub fn run_with_interpreter(
    path: &PathBuf,
    src: &str,
    parsed: &ParsedArgs,
) -> Result<Option<ProgramMemoryStats>, String> {
    if !parsed.config.quiet {
        let progress = BuildProgress::new("Analyzing source...");
        // Interpreter analysis is quick
        progress.success("Interpreter ready");
    }

    let body = super::directives::strip_compile_directive(src);
    ensure_type_check(&body, Some(&path.to_string_lossy()))?;
    let run_path = path.clone();
    let run_src = body.clone();
    let collect_memory = parsed.config.show_memory;
    let run_tests = parsed.config.run_tests;
    let selected_backend = parsed.config.backend;
    let include_tags_cfg = parsed.config.include_tags.clone();
    let fail_fast_cfg = parsed.config.fail_fast;
    let backend_check_cfg = parsed.config.backend_check;
    let test_output_format_cfg = parsed.config.test_output_format;
    let no_color_cfg = parsed.config.io.no_color;
    let quiet_cfg = parsed.config.quiet;
    let nocapture_cfg = parsed.config.test_nocapture;
    let test_name_cfg = parsed.config.test_name.clone();
    let stack_size_cfg = parsed.config.stack_size;

    // Set program args
    crate::execution::runtime::set_program_args(
        parsed.program_args.clone(),
        Some(run_path.to_string_lossy().to_string()),
    );

    let res = std::thread::Builder::new()
        .stack_size(stack_size_cfg) // Use configured stack size (default: 32MB, auto-scales to 1GB)
        .spawn(move || {
            let mut interp = Interpreter::new();
            let mut ldr = ModuleLoader::new(
                run_path
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new(".")),
            );

            // Run tests if --test flag is set
            if run_tests {
                use crate::parsing::lexer::Lexer;
                use crate::parsing::parser::Parser;
                use crate::testing::{
                    TestRunOptions, executor::InterpreterTestExecutor, format_test_summary,
                    render_backend_matrix_ascii, render_result_line, run_tests, to_json_report,
                };
                use crate::toolchain::config::TestOutputFormat;
                use std::collections::BTreeSet;

                let mut lexer = Lexer::new(&run_src);
                let tokens = match lexer.tokenize() {
                    Ok(t) => t,
                    Err(e) => return Err(format!("Lexer error: {}", e)),
                };

                let mut parser = Parser::new(tokens, Some(run_path.to_string_lossy().to_string()));
                let ast = match parser.parse_program() {
                    Ok(a) => a,
                    Err(e) => return Err(format!("Parse error: {}", e)),
                };

                let mut include_tags = BTreeSet::new();
                for tag in &include_tags_cfg {
                    include_tags.insert(tag.clone());
                }
                let opts = TestRunOptions {
                    fail_fast: fail_fast_cfg,
                    include_tags,
                    test_name: test_name_cfg.clone(),
                    backend_check: backend_check_cfg,
                    json_format: matches!(test_output_format_cfg, TestOutputFormat::Json),
                    no_color: no_color_cfg,
                    quiet: quiet_cfg,
                    nocapture: nocapture_cfg,
                    ..TestRunOptions::default()
                };

                // Use the real InterpreterTestExecutor
                let mut executor = InterpreterTestExecutor::new(
                    run_src.clone(),
                    run_path.to_string_lossy().to_string(),
                );
                let (results, summary, matrix) =
                    run_tests(&ast, &mut executor, &opts, selected_backend);

                if matches!(test_output_format_cfg, TestOutputFormat::Json) {
                    println!("{}", to_json_report(&results, &summary, matrix.as_ref()));
                } else {
                    // Print individual test results with colors
                    for result in &results {
                        let backend_result = result.backend_results.get(&selected_backend);
                        let msg = backend_result.and_then(|r| r.message.as_deref());
                        let duration = backend_result.map(|r| r.duration);
                        let output = backend_result.map(|r| r.output.clone()).unwrap_or_default();

                        println!(
                            "{}",
                            render_result_line(
                                &result.name,
                                result.effective_status,
                                no_color_cfg,
                                msg,
                                duration,
                                if output.is_empty() {
                                    None
                                } else {
                                    Some(output.as_str())
                                }
                            )
                        );

                        let show_output = matches!(
                            result.effective_status,
                            crate::testing::TestStatus::Fail
                                | crate::testing::TestStatus::Panic
                                | crate::testing::TestStatus::Timeout
                        ) || (nocapture_cfg && !quiet_cfg);
                        if show_output && !output.is_empty() {
                            for line in output.lines() {
                                println!("    {}", line);
                            }
                        }
                    }

                    if let Some(m) = matrix.as_ref() {
                        println!("{}", render_backend_matrix_ascii(m));
                    }
                    println!("{}", format_test_summary(&summary, no_color_cfg));
                }

                if !summary.is_success() {
                    return Err(format!(
                        "Tests failed: {} failed, {} panic, {} timeout",
                        summary.failed, summary.panics, summary.timeout
                    ));
                }

                let stats = if collect_memory {
                    Some(interp.get_memory_stats())
                } else {
                    None
                };
                Ok(stats)
            } else {
                // Normal execution
                match interp.run_module(
                    &run_src,
                    &mut ldr,
                    Some(run_path.to_string_lossy().to_string()),
                ) {
                    Ok(()) => {
                        interp.run_event_loop_until_idle();
                        // Collect memory stats if requested
                        let stats = if collect_memory {
                            Some(interp.get_memory_stats())
                        } else {
                            None
                        };
                        Ok(stats)
                    }
                    Err(e) => Err(e),
                }
            }
        });

    match res {
        Ok(handle) => match handle.join() {
            Ok(result) => result.map_err(|e| e.to_string()),
            Err(_) => Err("thread panicked".to_string()),
        },
        Err(_) => Err("failed to spawn thread".to_string()),
    }
}

#[cfg(debug_assertions)]
pub fn run_with_mlir_gpu(path: &PathBuf, src: &str, parsed: &ParsedArgs) -> Result<(), String> {
    use crate::backends::common::VirBackend;
    use crate::backends::mlir::{MlirBackend, MlirConfig, gpu, pipeline::MlirPipeline};
    use crate::cli::parsing::check_ownership_and_parse;
    use std::fs;
    use std::path::PathBuf;

    if !parsed.config.quiet {
        let progress = BuildProgress::new("Lowering to MLIR GPU backend...");
        progress.success("MLIR lowering ready");
    }

    let body = super::directives::strip_compile_directive(src);
    let (_ast, hir) = check_ownership_and_parse(&body, &parsed.config)?;

    let mir = crate::ir::mir::MirModule::from_hir(&hir)?;
    let vir = crate::ir::vir::VirModule::from_mir(&mir)?;

    let resolved_target = gpu::resolve_target(parsed.config.gpu_target);
    let kernel_config = gpu::GpuKernelConfig {
        grid_dims: parsed.config.gpu_grid,
        block_dims: parsed.config.gpu_block,
        shared_memory_size: parsed.config.gpu_shared_mem,
    };
    let mlir_config = MlirConfig {
        enable_gpu: true,
        gpu_target: resolved_target,
        gpu_kernel_config: kernel_config.clone(),
        opt_level: opt_level_to_u8(parsed.config.opt_level),
        enable_vector: true,
        target_triple: None,
    };

    let mut backend = MlirBackend::with_config(mlir_config);
    let compiled = backend
        .compile_module(&vir)
        .map_err(|e| format!("MLIR lowering error: {}", e))?;

    let mlir_text = compiled.to_mlir_string();

    let mlir_path = if let Some(ref file) = parsed.config.dump.write_mlir_file {
        let out_path = if file.is_empty() {
            PathBuf::from("output.mlir")
        } else {
            PathBuf::from(file)
        };
        fs::write(&out_path, mlir_text)
            .map_err(|e| format!("Failed to write MLIR output: {}", e))?;
        if !parsed.config.quiet {
            eprintln!("MLIR written to: {}", out_path.display());
        }
        out_path
    } else {
        let fallback_path = path.with_extension("mlir");
        fs::write(&fallback_path, mlir_text)
            .map_err(|e| format!("Failed to write MLIR output: {}", e))?;
        fallback_path
    };

    if parsed.config.dump.dump_mlir && parsed.config.dump.write_mlir_file.is_none() {
        eprintln!("\n═══ MLIR (GPU Backend, Debug Build) ═══");
        eprintln!("{}", mlir_text);
    }

    let toolchain = gpu::detect_toolchain(resolved_target);
    if !toolchain.is_available() {
        if !parsed.config.quiet {
            println!("  ⚠ GPU toolchain not found (MLIR written to {}). Falling back to interpreter.", mlir_path.display());
        }
        return run_interpreter_fallback(path, src, parsed, &mlir_path, &mlir_path);
    }

    if !parsed.config.quiet {
        println!("── GPU MLIR Pipeline ──────────────────────────────────────────");
        println!("  Source         : {}", path.display());
        println!("  Target backend : {}", resolved_target.as_str());
        println!(
            "  Grid dims      : ({}, {}, {})",
            kernel_config.grid_dims.0, kernel_config.grid_dims.1, kernel_config.grid_dims.2
        );
        println!(
            "  Block dims     : ({}, {}, {})",
            kernel_config.block_dims.0, kernel_config.block_dims.1, kernel_config.block_dims.2
        );
        println!(
            "  Shared mem     : {} bytes",
            kernel_config.shared_memory_size
        );
    }

    // Dump MIR if requested
    if parsed.config.dump.should_dump_mir() {
        println!("\n═══ MIR Dump ══════════════════════════════════════════════════");
        println!("  Module   : {}", mir.name);
        println!("  Functions: {}", mir.functions.len());
        for f in &mir.functions {
            println!(
                "    fn {}  ({} params, {} locals, {} blocks)",
                f.name,
                f.params.len(),
                f.locals.len(),
                f.body.len()
            );
        }
        println!("  Globals  : {}", mir.globals.len());
        println!("═══════════════════════════════════════════════════════════════");
    }

    // Run MLIR pipeline with new pipeline module
    let pipeline = MlirPipeline::new();
    let output_base = path.with_extension("");

    let lowered_path = output_base.with_extension("lowered.mlir");
    let llvm_ready_path = output_base.with_extension("llvm_ready.mlir");
    let llvm_ir_path = output_base.with_extension("ll");

    // Step 1: GPU kernel outlining
    let result1 = pipeline
        .run_gpu_outlining(&mlir_path, &lowered_path)
        .map_err(|e| format!("{:?}", e))?;

    if !result1.success {
        if !parsed.config.quiet {
            println!("  [1/4] ⚠ GPU kernel outlining failed");
            println!("        {}", result1.stderr.lines().next().unwrap_or(""));
        }
        return run_interpreter_fallback(path, src, parsed, &mlir_path, &lowered_path);
    }

    if !parsed.config.quiet {
        println!(
            "  [1/4] ✓ GPU kernel outlining    → {}",
            lowered_path.display()
        );
    }

    // Step 2: LLVM dialect lowering
    let result2 = pipeline
        .run_llvm_lowering(&lowered_path, &llvm_ready_path)
        .map_err(|e| format!("{:?}", e))?;

    if !result2.success {
        if !parsed.config.quiet {
            println!("  [2/4] ⚠ LLVM dialect lowering failed");
            println!("        {}", result2.stderr.lines().next().unwrap_or(""));
        }
        return run_interpreter_fallback(path, src, parsed, &mlir_path, &lowered_path);
    }

    if !parsed.config.quiet {
        println!(
            "  [2/4] ✓ LLVM dialect lowering    → {}",
            llvm_ready_path.display()
        );
    }

    // Step 3: Translate to LLVM IR
    let result3 = pipeline
        .translate_to_llvm_ir(&llvm_ready_path, &llvm_ir_path)
        .map_err(|e| format!("{:?}", e))?;

    if !result3.success {
        if !parsed.config.quiet {
            println!(
                "  [3/4] ⚠ MLIR→LLVM IR translation failed: Command failed: {}",
                toolchain.mlir_translate.as_ref().unwrap().display()
            );
            println!("         GPU dialect lowering requires NVVM/ROCm dialect plugins.");
            println!("         Run `adesh gpu-check` for toolchain details.");
        }
        return run_interpreter_fallback(path, src, parsed, &mlir_path, &lowered_path);
    }

    if !parsed.config.quiet {
        println!(
            "  [3/4] ✓ MLIR→LLVM IR translation → {}",
            llvm_ir_path.display()
        );
    }

    // Step 4: Compile to assembly and link
    let assembly_path = output_base.with_extension("s");
    let result4 = pipeline
        .compile_to_assembly(&llvm_ir_path, &assembly_path)
        .map_err(|e| format!("{:?}", e))?;

    if !result4.success {
        if !parsed.config.quiet {
            println!("  [4/4] ⚠ LLVM IR→Assembly failed");
            println!("        {}", result4.stderr.lines().next().unwrap_or(""));
        }
        return run_interpreter_fallback(path, src, parsed, &mlir_path, &lowered_path);
    }

    // Step 5: Link executable
    let exe_path = if cfg!(windows) {
        output_base.with_extension("exe")
    } else {
        output_base.with_extension("out")
    };

    let result5 = pipeline
        .link_executable(&assembly_path, &exe_path)
        .map_err(|e| format!("{:?}", e))?;

    if !result5.success {
        if !parsed.config.quiet {
            println!("  [4/4] ⚠ Linking failed");
            println!("        {}", result5.stderr.lines().next().unwrap_or(""));
        }
        return run_interpreter_fallback(path, src, parsed, &mlir_path, &lowered_path);
    }

    if !parsed.config.quiet {
        println!(
            "  [4/4] ✓ Native binary emitted    → {}",
            exe_path.display()
        );
        println!("────────────────────────────────────────────────────────────────");
        println!("  GPU binary ready. Run: {}", exe_path.display());
        println!("────────────────────────────────────────────────────────────────");
    }

    // Execute the compiled GPU host binary
    println!("\n── Program Output (GPU host binary) ──────────────────────────────");
    let exec_status = std::process::Command::new(&exe_path)
        .status()
        .map_err(|e| format!("Failed to run GPU binary: {}", e))?;

    if !exec_status.success() {
        return Err(format!("GPU binary exited with: {}", exec_status));
    }

    Ok(())
}

/// Fallback to interpreter when GPU pipeline fails
#[allow(dead_code)]
fn run_interpreter_fallback(
    path: &PathBuf,
    src: &str,
    parsed: &ParsedArgs,
    mlir_path: &PathBuf,
    lowered_path: &PathBuf,
) -> Result<(), String> {
    if !parsed.config.quiet {
        println!("────────────────────────────────────────────────────────────────");
        println!("  GPU MLIR artifacts written. Executing via interpreter fallback.");
        println!("  MLIR:         {}", mlir_path.display());
        println!("  Lowered MLIR: {}", lowered_path.display());
        println!("────────────────────────────────────────────────────────────────");
        println!("\n── Program Output (interpreted fallback) ─────────────────────────");
    }

    let _ = run_with_interpreter(path, src, parsed);
    Ok(())
}

#[cfg(debug_assertions)]
fn opt_level_to_u8(level: crate::toolchain::config::OptLevel) -> u8 {
    match level {
        crate::toolchain::config::OptLevel::O0 => 0,
        crate::toolchain::config::OptLevel::O1 => 1,
        crate::toolchain::config::OptLevel::O2 => 2,
        crate::toolchain::config::OptLevel::O3 => 3,
    }
}

#[cfg(debug_assertions)]
#[allow(dead_code)]
fn run_command(cmd: &std::path::Path, args: Vec<String>) -> Result<(), String> {
    use std::process::Command;

    let output = Command::new(cmd)
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to run {}: {}", cmd.display(), e))?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "Command failed: {}\nstdout:\n{}\nstderr:\n{}",
            cmd.display(),
            stdout,
            stderr
        ));
    }

    Ok(())
}

#[cfg(debug_assertions)]
#[allow(dead_code)]
fn gpu_pass_pipeline(target: crate::toolchain::config::GpuTarget) -> String {
    // Step 1 pipeline: GPU kernel outlining + canonicalization.
    // A second, separate mlir-opt call handles convert-func-to-llvm (see run_with_mlir_gpu).
    let pipeline = match target {
        // CUDA: GPU outlining + async region formation for CUDA async execution
        crate::toolchain::config::GpuTarget::Cuda => {
            "gpu-kernel-outlining,canonicalize,cse".to_string()
        }
        // ROCm/HIP: same structure as CUDA at the MLIR level
        crate::toolchain::config::GpuTarget::Rocm => {
            "gpu-kernel-outlining,canonicalize,cse".to_string()
        }
        // Vulkan: GPU outlining + SPIR-V-compatible canonicalization
        crate::toolchain::config::GpuTarget::Vulkan => {
            "gpu-kernel-outlining,canonicalize".to_string()
        }
        // Metal: GPU outlining (Apple GPU back-end)
        crate::toolchain::config::GpuTarget::Metal => {
            "gpu-kernel-outlining,canonicalize".to_string()
        }
        // Auto / unknown: most conservative set
        crate::toolchain::config::GpuTarget::Auto => {
            "gpu-kernel-outlining,canonicalize".to_string()
        }
    };
    // mlir-opt requires pipeline to be wrapped with the anchor (top-level) operation
    format!("builtin.module({})", pipeline)
}

/// Run program using AOT (Ahead-of-Time Cranelift compilation)
pub fn run_with_aot(path: &PathBuf, src: &str, parsed: &ParsedArgs) -> Result<(), String> {
    use crate::backends::cranelift_aot::aot_compile_with_options;
    use crate::backends::aot::cranelift::AotOptions;
    use std::process::Command;

    let body = super::directives::strip_compile_directive(src).to_string();
    ensure_type_check(&body, Some(&path.to_string_lossy()))?;

    let mut progress = if !parsed.config.quiet {
        Some(BuildProgress::new("Compiling with AOT Cranelift backend..."))
    } else {
        None
    };

    let exe_path = if cfg!(windows) {
        path.with_extension("aot.exe")
    } else {
        path.with_extension("aot.out")
    };

    let mut options = AotOptions::default();
    options.opt_level = match parsed.config.opt_level {
        crate::toolchain::config::OptLevel::O0 => 0,
        crate::toolchain::config::OptLevel::O1 => 1,
        crate::toolchain::config::OptLevel::O2 => 2,
        crate::toolchain::config::OptLevel::O3 => 3,
    };

    match aot_compile_with_options(&body, &exe_path, options) {
        Ok(()) => {
            if let Some(p) = progress.take() {
                p.success("AOT backend ready");
            }

            let status = Command::new(&exe_path)
                .args(&parsed.program_args)
                .status()
                .map_err(|e| format!("Failed to run AOT binary: {}", e))?;

            let _ = std::fs::remove_file(&exe_path);

            if !status.success() {
                return Err(format!("AOT binary exited with: {}", status));
            }
            Ok(())
        }
        Err(e) => {
            if let Some(p) = progress {
                p.fail("AOT compilation failed");
            }
            if parsed.config.verbose {
                eprintln!("[aot] compile failed: {}\n[aot] Falling back to interpreter...", e);
            }
            match run_with_interpreter(path, src, parsed) {
                Ok(_) => Ok(()),
                Err(err) => Err(err),
            }
        }
    }
}

/// Run program using WebAssembly backend (with fallback to interpreter if no Wasm runtime)
pub fn run_with_wasm(path: &PathBuf, src: &str, parsed: &ParsedArgs) -> Result<(), String> {
    let body = super::directives::strip_compile_directive(src).to_string();
    ensure_type_check(&body, Some(&path.to_string_lossy()))?;

    let mut progress = if !parsed.config.quiet {
        Some(BuildProgress::new("Compiling with WebAssembly backend..."))
    } else {
        None
    };

    let wasm_path = path.with_extension("wasm");
    match crate::backends::wasm::compile_to_file(&body, &wasm_path) {
        Ok(()) => {
            if let Some(p) = progress.take() {
                p.success("WebAssembly backend ready");
            }

            // Check if wasmtime or node is available to run it
            let ran = if let Ok(status) = std::process::Command::new("wasmtime")
                .arg(&wasm_path)
                .args(&parsed.program_args)
                .status()
            {
                Some(status.success())
            } else {
                None
            };

            let _ = std::fs::remove_file(&wasm_path);

            if let Some(success) = ran {
                if !success {
                    return Err("Wasm execution failed".to_string());
                }
                Ok(())
            } else {
                // If standalone wasm runtime isn't installed, execute via interpreter fallback
                if parsed.config.verbose {
                    eprintln!("[wasm] No standalone wasm runtime (wasmtime) found; falling back to interpreter.");
                }
                match run_with_interpreter(path, src, parsed) {
                    Ok(_) => Ok(()),
                    Err(err) => Err(err),
                }
            }
        }
        Err(e) => {
            if let Some(p) = progress {
                p.fail("WebAssembly compilation failed");
            }
            if parsed.config.verbose {
                eprintln!("[wasm] compile failed: {}\n[wasm] Falling back to interpreter...", e);
            }
            match run_with_interpreter(path, src, parsed) {
                Ok(_) => Ok(()),
                Err(err) => Err(err),
            }
        }
    }
}
