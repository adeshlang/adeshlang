// Allow clippy warnings for style preferences in main binary
#![allow(clippy::collapsible_if)]
#![allow(clippy::needless_borrows_for_generic_args)]
#![allow(clippy::ptr_arg)]
#![allow(clippy::manual_strip)]
#![allow(clippy::len_zero)]

use adeshlang::cli::{ExecutionBackend, MemoryStats, ParsedArgs, help_message, version_message};
use adeshlang::execution::runtime::Interpreter;
/**
 * AdeshLang - A statically-typed, multi-backend programming language
 *
 * This is the main entry point for the AdeshLang CLI (run/repl/compile/tools).
 *
 * Supported commands (see `adesh --help` for full details):
 *  - adesh run <file.adesh>
 *  - adesh repl
 *  - adesh editor [file|dir]
 *  - adesh init <dir>
 *  - adesh compile <in.adesh> <out.bin>
 *  - adesh compile-wasm <in.adesh> <out.wasm>
 *  - adesh compile-native <in.adesh> <out.exe>
 *  - adesh compile-aot <in.adesh> <out.exe>
 *  - adesh disassemble <out.bin> [--write [file]]
 *  - adesh docs <in.adesh> <out_dir>
 *  - adesh ai <subcommand>
 *
 * Execution backends (selected via flags or @compile directive):
 *  - --interpreter / --interp: Interpreter (default)
 *  - --bytecode / --vm: Bytecode VM
 *  - --jit: JIT execution (LIR-based interpreter)
 *  - --jit-native / --native-jit / --njit: Native JIT (10-232x faster!) ⭐ NEW!
 *  - --adaptive-jit: Adaptive JIT shell
 *  - --tiered-jit: Tiered JIT shell
 *  - --wasm: WebAssembly backend (Wasmtime runtime) ⭐ NEW!
 *  - --mixed: Hybrid (JIT with interpreter fallback)
 *  - --safe: Safe mode (no JIT)
 *
 * Native JIT Performance:
 *  - Heavy compute: 232x faster than interpreter
 *  - Medium compute: 1.4x faster than interpreter
 *  - Memory/pointer ops: Native CPU speed (100ns malloc, 1-2ns load/store)
 *  - Compilation time: Sub-10ms per function
 *
 * NOTE: Compile-time memory safety validation is mandatory for all backends.
 *
 * @author: Ajay Tainwala
 * @github: https://github.com/adeshlang/adeshlang
 * @version: 0.3.0
 */
use colored::control;
use std::time::Instant;
use std::{env, fs, path::PathBuf};

// Import modularized CLI functions
mod cli_impl {
    pub use adeshlang::cli::aot_utils::*;
    pub use adeshlang::cli::backends::*;
    pub use adeshlang::cli::commands::*;
    pub use adeshlang::cli::config::*;
    pub use adeshlang::cli::directives::*;
    pub use adeshlang::cli::editor_launcher::*;
    pub use adeshlang::cli::ir_utils::*;
    pub use adeshlang::cli::memory_stats::*;
    pub use adeshlang::cli::parsing::*;
    pub use adeshlang::cli::path_utils::*;
    pub use adeshlang::cli::validation::*;
}

fn main() {
    let args: Vec<String> = env::args().collect();

    // Parse arguments using new CLI system
    let parsed = match ParsedArgs::parse(&args) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: {}", e);
            cli_impl::usage();
            std::process::exit(1);
        }
    };

    // Handle help/version
    if parsed.show_help {
        println!("{}", help_message());
        return;
    }
    if parsed.show_version {
        println!("{}", version_message());
        return;
    }

    // Apply color settings
    if parsed.config.io.no_color {
        control::set_override(false);
    }

    // Also support legacy --no-color anywhere
    if args.iter().any(|a| a == "--no-color") {
        control::set_override(false);
    }

    adeshlang::runtime::thread::boot_cli_watchdog(parsed.config.run_timeout);

    // Spawn thread with configured stack size to avoid stack overflow in debug mode
    let stack_size = parsed.config.stack_size;
    let handle = std::thread::Builder::new()
        .name("adesh-main".to_string())
        .stack_size(stack_size)
        .spawn(move || {
            real_main(parsed, args);
        })
        .expect("Failed to spawn main execution thread");

    let panicked = handle.join().is_err();
    adeshlang::runtime::thread::disarm_watchdog();
    if panicked {
        eprintln!("Error: Main execution thread panicked");
        std::process::exit(101);
    }
    // Do not wait for leftover pool/worker OS threads after the program finished.
    std::process::exit(0);
}

fn real_main(parsed: ParsedArgs, args: Vec<String>) {
    // Configure global FFI registry based on CLI flags
    cli_impl::apply_ffi_cli_config(&parsed.config);

    let mut interp = Interpreter::new();
    match parsed.command.as_str() {
        "ai" => {
            let mut ai_args = Vec::new();
            if let Some(ref input) = parsed.input_file {
                ai_args.push(input.clone());
            }
            ai_args.extend(parsed.program_args.clone());
            cli_impl::execute_ai_command(&ai_args);
        }
        "editor" => {
            let mut editor_args = Vec::new();
            if let Some(ref input) = parsed.input_file {
                editor_args.push(input.clone());
            }
            editor_args.extend(parsed.program_args.clone());
            cli_impl::execute_editor_command(&editor_args);
        }
        "crypto" => {
            adeshlang::cli::crypto::execute_crypto_cli(&parsed.program_args);
        }
        "doctor" => {
            adeshlang::cli::doctor::execute_doctor_command();
        }
        "env" => {
            adeshlang::cli::env::execute_env_command_with_format(parsed.env_json, parsed.env_shell);
        }
        "repair" => {
            adeshlang::cli::repair::execute_repair_command();
        }
        "pkg" => {
            adeshlang::cli::pkg::execute_pkg_command(&parsed.program_args);
        }
        "toolchain" => {
            let mut toolchain_args = Vec::new();
            if let Some(subcommand) = parsed.input_file.as_ref() {
                toolchain_args.push(subcommand.clone());
            }
            toolchain_args.extend(parsed.program_args.clone());
            adeshlang::cli::toolchain::execute_toolchain_command_with_preference(
                &toolchain_args,
                parsed.toolchain_preference,
            );
        }
        "update" => {
            let mut update_args = Vec::new();
            if let Some(ref input) = parsed.input_file {
                update_args.push(input.clone());
            }
            update_args.extend(parsed.program_args.clone());
            adeshlang::update::execute_update_command(&update_args);
        }
        "rollback" => {
            let mut rollback_args = Vec::new();
            if let Some(ref input) = parsed.input_file {
                rollback_args.push(input.clone());
            }
            rollback_args.extend(parsed.program_args.clone());
            adeshlang::update::execute_rollback_command(&rollback_args);
        }
        "run" | "test" => {
            let (path, src) = if let Some(ref eval_code) = parsed.eval_code {
                (PathBuf::from("<eval>"), eval_code.clone())
            } else {
                let input_path = if let Some(ref ip) = parsed.input_file {
                    ip.clone()
                } else if parsed.config.run_tests {
                    // Auto-detect test file in current project
                    if PathBuf::from("tests/main.adesh").exists() {
                        "tests/main.adesh".to_string()
                    } else if PathBuf::from("tests.adesh").exists() {
                        "tests.adesh".to_string()
                    } else if PathBuf::from("main.test.adesh").exists() {
                        "main.test.adesh".to_string()
                    } else if let Ok(entries) = std::fs::read_dir("tests") {
                        let mut found = None;
                        for entry in entries.flatten() {
                            let p = entry.path();
                            if p.extension().map_or(false, |ext| ext == "adesh") {
                                found = Some(p.to_string_lossy().to_string());
                                break;
                            }
                        }
                        if let Some(f) = found {
                            f
                        } else {
                            eprintln!("Error: No test file specified and none found in tests/");
                            std::process::exit(1);
                        }
                    } else {
                        eprintln!("Error: No input file specified");
                        cli_impl::usage();
                        return;
                    }
                } else {
                    eprintln!("Error: No input file specified");
                    cli_impl::usage();
                    return;
                };

                let orig_path = PathBuf::from(&input_path);
                match cli_impl::resolve_input_path(&orig_path) {
                    Ok(pair) => pair,
                    Err(msg) => {
                        eprintln!("{}", msg);
                        std::process::exit(1);
                    }
                }
            };

            // Handle IR dump options
            if parsed.config.dump.should_dump_ast()
                || parsed.config.dump.should_dump_hir()
                || parsed.config.dump.should_dump_lir()
                || parsed.config.dump.should_dump_vir()
                || parsed.config.dump.should_dump_mlir()
                || parsed.config.dump.should_dump_cfg()
            {
                cli_impl::dump_ir(&src, &parsed.config);
            }

            // Auto-detect backend from @compile directive if present
            let backend = if let Some(directive) = cli_impl::detect_compile_directive(&src) {
                match directive.as_str() {
                    "jit" => ExecutionBackend::Jit,
                    "bytecode" => ExecutionBackend::Bytecode,
                    "mixed" => ExecutionBackend::Mixed,
                    "aot" => ExecutionBackend::Aot,
                    "wasm" => ExecutionBackend::Wasm,
                    #[cfg(debug_assertions)]
                    "gpu" => ExecutionBackend::Gpu,
                    _ => parsed.config.backend,
                }
            } else {
                parsed.config.backend
            };

            if let Err(e) = cli_impl::ensure_embedded_no_heap(&src, &parsed.config) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }

            // ============================================================
            // MANDATORY COMPILE-TIME TYPE SAFETY & MEMORY SAFETY CHECKS
            // ============================================================
            // These checks ALWAYS run post-lexical analysis, regardless of flags.
            // They validate types, ownership, borrowing, lifetimes, and data races
            // BEFORE any backend execution, ensuring all programs are verified.

            if parsed.config.verbose {
                eprintln!(
                    "🔒 Performing mandatory compile-time type & memory safety validation..."
                );
            }

            // Perform mandatory static type checking
            if let Err(e) =
                adeshlang::types::type_system::check_module_in(&src, Some(&path.to_string_lossy()))
            {
                eprintln!("{}", e);
                std::process::exit(1);
            }

            // Parse and run compile-time ownership memory safety checks
            if let Err(e) = cli_impl::check_ownership_and_parse_in(
                &src,
                &parsed.config,
                Some(&path.to_string_lossy()),
            ) {
                if e.starts_with("error[") || e.starts_with("Compile-time") || e.contains('\n') {
                    eprintln!("{}", e);
                } else {
                    eprintln!("error: {}", e);
                }
                std::process::exit(1);
            }

            if parsed.config.verbose {
                eprintln!("compile-time type and memory safety checks passed");
            }

            // Start timing if profiling enabled
            let start_time = if parsed.config.profile {
                Some(Instant::now())
            } else {
                None
            };

            // Execute based on backend selection - async/await now natively supported in JIT/bytecode
            let (result, memory_stats): (Result<(), String>, MemoryStats) = if parsed
                .config
                .run_tests
            {
                // If running tests, dispatch directly through the test runner
                match cli_impl::run_with_interpreter(&path, &src, &parsed) {
                    Ok(Some(stats)) => (Ok(()), MemoryStats::Interpreter(stats)),
                    Ok(None) => (Ok(()), MemoryStats::None),
                    Err(e) => (Err(e), MemoryStats::None),
                }
            } else {
                match backend {
                    ExecutionBackend::Jit => match cli_impl::run_with_jit(&path, &src, &parsed) {
                        Ok(Some(stats)) => (Ok(()), MemoryStats::Jit(stats)),
                        Ok(None) => (Ok(()), MemoryStats::None),
                        Err(e) => (Err(e), MemoryStats::None),
                    },
                    ExecutionBackend::NativeJit => (
                        cli_impl::run_with_native_jit(&path, &src, &parsed),
                        MemoryStats::None,
                    ),
                    ExecutionBackend::Bytecode => (
                        cli_impl::run_with_bytecode(&path, &src, &parsed),
                        MemoryStats::None,
                    ),
                    ExecutionBackend::Mixed => {
                        // Mixed mode: try JIT first, fall back to interpreter
                        if parsed.config.verbose {
                            eprintln!("[mixed] Attempting JIT execution...");
                        }
                        match cli_impl::run_with_jit(&path, &src, &parsed) {
                            Ok(Some(stats)) => (Ok(()), MemoryStats::Jit(stats)),
                            Ok(None) => (Ok(()), MemoryStats::None),
                            Err(e) => {
                                if parsed.config.verbose {
                                    eprintln!(
                                        "[mixed] JIT failed ({}), falling back to interpreter",
                                        e
                                    );
                                }
                                match cli_impl::run_with_interpreter(&path, &src, &parsed) {
                                    Ok(Some(stats)) => (Ok(()), MemoryStats::Interpreter(stats)),
                                    Ok(None) => (Ok(()), MemoryStats::None),
                                    Err(e) => (Err(e), MemoryStats::None),
                                }
                            }
                        }
                    }
                    ExecutionBackend::Safe | ExecutionBackend::Interpreter => {
                        match cli_impl::run_with_interpreter(&path, &src, &parsed) {
                            Ok(Some(stats)) => (Ok(()), MemoryStats::Interpreter(stats)),
                            Ok(None) => (Ok(()), MemoryStats::None),
                            Err(e) => (Err(e), MemoryStats::None),
                        }
                    }
                    ExecutionBackend::AdaptiveJit => (
                        cli_impl::run_with_adaptive_jit(&path, &src, &parsed),
                        MemoryStats::None,
                    ),
                    ExecutionBackend::TieredJit => (
                        cli_impl::run_with_tiered_jit(&path, &src, &parsed),
                        MemoryStats::None,
                    ),
                    ExecutionBackend::Aot => (
                        cli_impl::run_with_aot(&path, &src, &parsed),
                        MemoryStats::None,
                    ),
                    ExecutionBackend::Wasm => (
                        cli_impl::run_with_wasm(&path, &src, &parsed),
                        MemoryStats::None,
                    ),
                    #[cfg(debug_assertions)]
                    ExecutionBackend::Gpu => (
                        cli_impl::run_with_mlir_gpu(&path, &src, &parsed),
                        MemoryStats::None,
                    ),
                }
            };

            // Print timing if profiling enabled
            if let Some(start) = start_time {
                let duration = start.elapsed();
                eprintln!("\n⏱  Execution time: {:?}", duration);
            }

            // Print memory usage if --memory flag is enabled
            if parsed.config.show_memory {
                cli_impl::print_memory_stats(memory_stats);
            }

            // Ensure global stdout buffer is flushed
            adeshlang::execution::runtime_core::stdio::flush_stdout();
            // Flush the fast_print buffer as well
            adeshlang::execution::runtime_core::fast_print::flush_fast_buffer();

            if let Err(e) = result {
                let trimmed = e.trim();
                if !trimmed.is_empty() && !trimmed.starts_with("Tests failed:") {
                    eprintln!("{}", trimmed);
                }
                std::process::exit(1);
            }

            // For all backends, explicitly exit to prevent background threads from hanging
            use std::io::Write;
            std::io::stdout().flush().ok();
            std::io::stderr().flush().ok();
            std::process::exit(0);
        }
        "run-interpret" => {
            // Legacy command - always use interpreter
            if parsed.input_file.is_none() {
                cli_impl::usage();
                return;
            }
            let input_path = parsed.input_file.clone().unwrap();
            let orig_path = PathBuf::from(&input_path);
            let (path, src) = match cli_impl::resolve_input_path(&orig_path) {
                Ok(pair) => pair,
                Err(msg) => {
                    eprintln!("{}", msg);
                    std::process::exit(1);
                }
            };
            // Enforce ownership checks
            if let Err(e) = cli_impl::check_ownership_and_parse_in(
                &src,
                &parsed.config,
                Some(&path.to_string_lossy()),
            ) {
                eprintln!("Ownership Error: {}", e);
                std::process::exit(1);
            }
            match cli_impl::run_with_interpreter(&path, &src, &parsed) {
                Ok(Some(stats)) => {
                    if parsed.config.show_memory {
                        cli_impl::print_memory_stats(MemoryStats::Interpreter(stats));
                    }
                }
                Ok(None) => {
                    if parsed.config.show_memory {
                        cli_impl::print_memory_stats(MemoryStats::None);
                    }
                }
                Err(e) => {
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }
            // Explicitly exit to prevent background threads from hanging
            use std::io::Write;
            std::io::stdout().flush().ok();
            std::io::stderr().flush().ok();
            std::process::exit(0);
        }
        "run-jit" => {
            // Direct JIT command
            if parsed.input_file.is_none() {
                eprintln!("Error: No input file specified");
                cli_impl::usage();
                return;
            }
            let input_path = parsed.input_file.clone().unwrap();
            let orig_path = PathBuf::from(&input_path);
            let (path, src) = match cli_impl::resolve_input_path(&orig_path) {
                Ok(pair) => pair,
                Err(msg) => {
                    eprintln!("{}", msg);
                    std::process::exit(1);
                }
            };

            let start_time = if parsed.config.profile {
                Some(Instant::now())
            } else {
                None
            };

            // Enforce ownership checks
            if let Err(e) = cli_impl::check_ownership_and_parse_in(
                &src,
                &parsed.config,
                Some(&path.to_string_lossy()),
            ) {
                eprintln!("Ownership Error: {}", e);
                std::process::exit(1);
            }

            if let Err(e) = cli_impl::run_with_jit(&path, &src, &parsed) {
                eprintln!("{}", e);
                std::process::exit(1);
            }

            if let Some(start) = start_time {
                let duration = start.elapsed();
                eprintln!("\n⏱  Execution time: {:?}", duration);
            }
        }
        "run-bc" => {
            if parsed.input_file.is_none() {
                cli_impl::usage();
                return;
            }
            let file = PathBuf::from(parsed.input_file.unwrap());
            if let Err(e) = adeshlang::execution::vm::run_file(&file) {
                eprintln!("vm error: {}", e);
                std::process::exit(1);
            }
        }

        "repl" => {
            interp.repl();
        }
        "compile" => {
            if parsed.input_file.is_none() || parsed.output_file.is_none() {
                cli_impl::usage();
                return;
            }
            let in_path = PathBuf::from(parsed.input_file.unwrap());
            let out_path = PathBuf::from(parsed.output_file.unwrap());
            let src = match fs::read_to_string(&in_path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Error reading {}: {}", in_path.display(), e);
                    std::process::exit(1);
                }
            };
            let body = cli_impl::strip_compile_directive(&src);
            // Run type checker and abort on errors
            if let Err(e) = adeshlang::types::type_system::check_module_in(
                &body,
                Some(&in_path.to_string_lossy()),
            ) {
                eprintln!("Type Error: {}", e);
                std::process::exit(1);
            }
            // Enforce ownership checks
            if let Err(e) = cli_impl::check_ownership_and_parse_in(
                &body,
                &parsed.config,
                Some(&in_path.to_string_lossy()),
            ) {
                eprintln!("Ownership Error: {}", e);
                std::process::exit(1);
            }
            match adeshlang::execution::bytecode::compile_to_file_v2(&body, &out_path) {
                Ok(()) => println!("Wrote bytecode (v2) to {}", out_path.display()),
                Err(e) => {
                    eprintln!("compile error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        "compile-wasm" => {
            if parsed.input_file.is_none() || parsed.output_file.is_none() {
                cli_impl::usage();
                return;
            }
            let in_path = PathBuf::from(parsed.input_file.unwrap());
            let out_path = PathBuf::from(parsed.output_file.unwrap());
            let src = match fs::read_to_string(&in_path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Error reading {}: {}", in_path.display(), e);
                    std::process::exit(1);
                }
            };
            let body = cli_impl::strip_compile_directive(&src);
            if let Err(e) = adeshlang::types::type_system::check_module_in(
                &body,
                Some(&in_path.to_string_lossy()),
            ) {
                eprintln!("Type Error: {}", e);
                std::process::exit(1);
            }
            // Enforce ownership checks
            if let Err(e) = cli_impl::check_ownership_and_parse_in(
                &body,
                &parsed.config,
                Some(&in_path.to_string_lossy()),
            ) {
                eprintln!("Ownership Error: {}", e);
                std::process::exit(1);
            }
            match adeshlang::backends::wasm::compile_to_file(&body, &out_path) {
                Ok(()) => {
                    println!("Wrote wasm to {}", out_path.display());
                    let mut js_path = out_path.clone();
                    js_path.set_extension("js");
                    let _ = adeshlang::backends::wasm::write_js_loader(&out_path, &js_path);
                }
                Err(e) => {
                    eprintln!("compile wasm error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        "compile-native" => {
            if parsed.input_file.is_none() || parsed.output_file.is_none() {
                cli_impl::usage();
                return;
            }
            let in_path = PathBuf::from(parsed.input_file.unwrap());
            let out_path = PathBuf::from(parsed.output_file.unwrap());
            let src = match fs::read_to_string(&in_path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Error reading {}: {}", in_path.display(), e);
                    std::process::exit(1);
                }
            };
            if let Err(e) = adeshlang::types::type_system::check_module_in(
                &src,
                Some(&in_path.to_string_lossy()),
            ) {
                eprintln!("Type Error: {}", e);
                std::process::exit(1);
            }
            // Enforce ownership checks
            if let Err(e) = cli_impl::check_ownership_and_parse_in(
                &src,
                &parsed.config,
                Some(&in_path.to_string_lossy()),
            ) {
                eprintln!("Ownership Error: {}", e);
                std::process::exit(1);
            }
            match adeshlang::backends::backend::compile_native(&src, &out_path) {
                Ok(()) => println!("Wrote native to {}", out_path.display()),
                Err(e) => {
                    eprintln!("compile native error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        "compile-aot" => {
            if parsed.input_file.is_none() || parsed.output_file.is_none() {
                eprintln!("Error: compile-aot requires input and output files");
                cli_impl::usage();
                return;
            }
            let in_path = PathBuf::from(parsed.input_file.unwrap());
            let out_path = PathBuf::from(parsed.output_file.unwrap());
            let src = match fs::read_to_string(&in_path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Error reading {}: {}", in_path.display(), e);
                    std::process::exit(1);
                }
            };

            if let Err(e) = adeshlang::types::type_system::check_module_in(
                &src,
                Some(&in_path.to_string_lossy()),
            ) {
                eprintln!("Type Error: {}", e);
                std::process::exit(1);
            }

            // Enforce ownership checks (AOT)
            if let Err(e) = cli_impl::check_ownership_and_parse_in(
                &src,
                &parsed.config,
                Some(&in_path.to_string_lossy()),
            ) {
                eprintln!("Ownership Error: {}", e);
                std::process::exit(1);
            }

            // Parse AOT options from command line
            let mut aot_options = adeshlang::backends::cranelift_aot::AotOptions::default();
            let mut _emit_assembly = false; // -S flag
            let mut emit_header = false;
            let mut _emit_header_path: Option<String> = None;
            let mut _include_dirs: Vec<String> = Vec::new();
            let mut _lib_dirs: Vec<String> = Vec::new();
            let mut _link_libs: Vec<String> = Vec::new();
            let mut _ffi_enabled = false;

            // Parse additional flags (two-pass for value flags like -I, -L, -l)
            let mut i = 0;
            while i < parsed.program_args.len() {
                let arg = &parsed.program_args[i];
                match arg.as_str() {
                    "--opt=0" | "-O0" => aot_options.opt_level = 0,
                    "--opt=1" | "-O1" => aot_options.opt_level = 1,
                    "--opt=2" | "-O2" => aot_options.opt_level = 2,
                    "--opt=3" | "-O3" => aot_options.opt_level = 3,
                    "--fast" => {
                        aot_options.fast_compile = true;
                        aot_options.opt_level = 0;
                    }
                    "--incremental" => aot_options.incremental = true,
                    "--no-incremental" | "--no-cache" => aot_options.incremental = false,
                    "-f" | "--force" => aot_options.force_rebuild = true,
                    "--clear-cache" => {
                        let cache =
                            adeshlang::backends::aot::cache::AotCompilationCache::new(None, true);
                        let _ = cache.clear();
                    }
                    "--debug" => aot_options.debug_info = true,
                    "--shared" | "--dll" | "--so" => {
                        aot_options.output_format =
                            adeshlang::backends::cranelift_aot::OutputFormat::SharedLib;
                        // Enable library mode by default for shared libraries
                        aot_options.library_mode = true;
                    }
                    "--static" | "--lib" | "--a" => {
                        aot_options.output_format =
                            adeshlang::backends::cranelift_aot::OutputFormat::StaticLib;
                        // Enable library mode by default for static libraries
                        aot_options.library_mode = true;
                    }
                    "--object" | "--obj" | "--o" => {
                        aot_options.output_format =
                            adeshlang::backends::cranelift_aot::OutputFormat::Object;
                    }
                    "-c" | "--compile-only" => {
                        aot_options.output_format =
                            adeshlang::backends::cranelift_aot::OutputFormat::Object;
                        aot_options.library_mode = true;
                    }
                    "-S" | "--emit-asm" | "--emit-ir" => {
                        _emit_assembly = true;
                    }
                    "--emit-header" => {
                        emit_header = true;
                        // Next argument might be the header path
                        if i + 1 < parsed.program_args.len()
                            && !parsed.program_args[i + 1].starts_with("-")
                        {
                            _emit_header_path = Some(parsed.program_args[i + 1].clone());
                            i += 1;
                        }
                    }
                    "--ffi" => {
                        _ffi_enabled = true;
                    }
                    "-I" => {
                        if i + 1 < parsed.program_args.len() {
                            _include_dirs.push(parsed.program_args[i + 1].clone());
                            i += 1;
                        }
                    }
                    "-L" => {
                        if i + 1 < parsed.program_args.len() {
                            _lib_dirs.push(parsed.program_args[i + 1].clone());
                            i += 1;
                        }
                    }
                    "-l" => {
                        if i + 1 < parsed.program_args.len() {
                            _link_libs.push(parsed.program_args[i + 1].clone());
                            i += 1;
                        }
                    }
                    _ if arg.starts_with("-I") => {
                        _include_dirs.push(arg[2..].to_string());
                    }
                    _ if arg.starts_with("-L") => {
                        _lib_dirs.push(arg[2..].to_string());
                    }
                    _ if arg.starts_with("-l") => {
                        _link_libs.push(arg[2..].to_string());
                    }
                    _ if arg.starts_with("--target=") => {
                        aot_options.target_triple = Some(arg[9..].to_string());
                    }
                    _ => {
                        if !arg.starts_with("-") {
                            // Positional arguments after options are skipped
                        } else if !arg.starts_with("-O") {
                            // -O flags handled above
                            eprintln!("Warning: Unknown AOT option: {}", arg);
                        }
                    }
                }
                i += 1;
            }

            if _emit_assembly {
                eprintln!(
                    "Warning: -S/--emit-asm not yet fully implemented, compiling to object file instead"
                );
            }

            // Merge CLI-provided lib/include paths and link libs (-L/-l) from both new config and AOT flags
            let mut lib_dirs = _lib_dirs;
            lib_dirs.extend(parsed.config.lib_paths.clone());
            lib_dirs.sort();
            lib_dirs.dedup();

            let mut link_libs = _link_libs;
            link_libs.extend(parsed.config.link_libs.clone());
            link_libs.sort();
            link_libs.dedup();

            let mut include_dirs = _include_dirs;
            include_dirs.sort();
            include_dirs.dedup();

            // Inject merged linker inputs into AOT options
            aot_options.lib_dirs = lib_dirs;
            aot_options.link_libs = link_libs;
            aot_options.include_dirs = include_dirs;

            match adeshlang::backends::cranelift_aot::aot_compile_with_options(
                &src,
                &out_path,
                aot_options,
            ) {
                Ok(()) => {
                    println!("Successfully compiled AOT to {}", out_path.display());

                    // Generate header file if requested
                    if emit_header {
                        // Auto-generate header path from output if not specified
                        let header_path = match _emit_header_path {
                            Some(path) => PathBuf::from(path),
                            None => out_path.with_extension("h"),
                        };

                        // Generate C header from the compiled module
                        match cli_impl::generate_aot_header(&src, &header_path) {
                            Ok(()) => {
                                println!("Generated header file at {}", header_path.display());
                            }
                            Err(e) => {
                                if parsed.config.verbose {
                                    eprintln!("Warning: Failed to generate header file: {}", e);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("AOT compilation error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        "compile-wasm-js" => {
            if parsed.input_file.is_none() || parsed.output_file.is_none() {
                cli_impl::usage();
                return;
            }
            let in_path = PathBuf::from(parsed.input_file.unwrap());
            let out_dir = PathBuf::from(parsed.output_file.unwrap());
            let src = fs::read_to_string(&in_path).expect("failed to read file");
            let wasm_out = out_dir
                .join(in_path.file_stem().unwrap())
                .with_extension("wasm");
            let js_out = out_dir
                .join(in_path.file_stem().unwrap())
                .with_extension("js");
            let body = cli_impl::strip_compile_directive(&src);
            if let Err(e) = adeshlang::types::type_system::check_module_in(
                &body,
                Some(&in_path.to_string_lossy()),
            ) {
                eprintln!("Type Error: {}", e);
                std::process::exit(1);
            }
            // Enforce ownership checks
            if let Err(e) = cli_impl::check_ownership_and_parse_in(
                &body,
                &parsed.config,
                Some(&in_path.to_string_lossy()),
            ) {
                eprintln!("Ownership Error: {}", e);
                std::process::exit(1);
            }
            match adeshlang::backends::wasm::compile_to_file(&body, &wasm_out) {
                Ok(()) => {
                    if let Err(e) = adeshlang::backends::wasm::write_js_loader(&wasm_out, &js_out) {
                        eprintln!("write js loader error: {}", e);
                        std::process::exit(1);
                    }
                    println!(
                        "Wrote wasm to {} and js to {}",
                        wasm_out.display(),
                        js_out.display()
                    );
                }
                Err(e) => {
                    eprintln!("compile wasm error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        "compile-native-rust" => {
            if parsed.input_file.is_none() || parsed.output_file.is_none() {
                cli_impl::usage();
                return;
            }
            let in_path = PathBuf::from(parsed.input_file.unwrap());
            let out_path = PathBuf::from(parsed.output_file.unwrap());
            let src = fs::read_to_string(&in_path).expect("failed to read file");
            if let Err(e) = adeshlang::types::type_system::check_module_in(
                &src,
                Some(&in_path.to_string_lossy()),
            ) {
                eprintln!("Type Error: {}", e);
                std::process::exit(1);
            }
            // Enforce ownership checks
            if let Err(e) = cli_impl::check_ownership_and_parse_in(
                &src,
                &parsed.config,
                Some(&in_path.to_string_lossy()),
            ) {
                eprintln!("Ownership Error: {}", e);
                std::process::exit(1);
            }
            match adeshlang::backends::backend::compile_native_rust(&src, &out_path) {
                Ok(()) => println!("Wrote native Rust to {}", out_path.display()),
                Err(e) => {
                    eprintln!("compile native error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        "disassemble" => {
            if parsed.input_file.is_none() {
                cli_impl::usage();
                return;
            }
            let file = PathBuf::from(parsed.input_file.unwrap());

            // Check if --write flag was in original args
            let write_flag_present = args.iter().any(|a| a == "--write" || a == "-w");

            let out_path = if write_flag_present {
                // Look for output path in output_file or generate default
                if let Some(out) = &parsed.output_file {
                    Some(PathBuf::from(out))
                } else {
                    // Generate default filename: <input>.bc.adesh
                    let mut out = file.clone();
                    if let Some(name) = out.file_name() {
                        let name_str = name.to_string_lossy().to_string();
                        out.set_file_name(format!("{}.bc.adesh", name_str));
                        Some(out)
                    } else {
                        None
                    }
                }
            } else {
                None
            };

            if let Err(e) =
                adeshlang::execution::bytecode::disassemble_file_to_file(&file, out_path.as_deref())
            {
                eprintln!("disassemble error: {}", e);
                std::process::exit(1);
            }

            if let Some(out) = out_path {
                println!("Disassembly written to: {}", out.display());
            }
        }
        "docs" => {
            if parsed.input_file.is_none() || parsed.output_file.is_none() {
                cli_impl::usage();
                return;
            }
            let in_path = PathBuf::from(parsed.input_file.unwrap());
            let out_dir = PathBuf::from(parsed.output_file.unwrap());
            match adeshlang::utils::docgen::generate_docs(&in_path, &out_dir) {
                Ok(()) => println!("Wrote docs to {}", out_dir.display()),
                Err(e) => {
                    eprintln!("docs error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        "format" | "fmt" => {
            if parsed.input_file.is_none() {
                eprintln!("Error: No input file specified");
                cli_impl::usage();
                return;
            }

            let input_path = parsed.input_file.clone().unwrap();
            let orig_path = PathBuf::from(&input_path);
            let (path, src) = match cli_impl::resolve_input_path(&orig_path) {
                Ok(pair) => pair,
                Err(msg) => {
                    eprintln!("{}", msg);
                    std::process::exit(1);
                }
            };

            // Parse formatter options from args
            let write_to_file = args.iter().any(|a| a == "--write" || a == "-w");
            let check_only = args.iter().any(|a| a == "--check");
            let use_tabs = args.iter().any(|a| a == "--tabs");
            let indent_size = args
                .iter()
                .position(|a| a == "--indent")
                .and_then(|i| args.get(i + 1))
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(4);

            let config = adeshlang::utils::formatter::FormatConfig {
                indent_size,
                use_tabs,
                ..Default::default()
            };

            match adeshlang::utils::formatter::format_source(&src, Some(config)) {
                Ok(formatted) => {
                    if check_only {
                        // Check mode: exit 0 if already formatted, 1 otherwise
                        let src_norm = src.replace("\r\n", "\n");
                        let fmt_norm = formatted.replace("\r\n", "\n");
                        if src_norm == fmt_norm {
                            println!("✓ {} is already formatted", path.display());
                            std::process::exit(0);
                        } else {
                            eprintln!("✗ {} needs formatting", path.display());
                            std::process::exit(1);
                        }
                    } else if write_to_file {
                        // Write to file
                        if let Err(e) = fs::write(&path, &formatted) {
                            eprintln!("Error writing file: {}", e);
                            std::process::exit(1);
                        }
                        println!("✓ Formatted {}", path.display());
                    } else {
                        // Print to stdout
                        print!("{}", formatted);
                    }
                }
                Err(e) => {
                    eprintln!("Format error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        "check" => {
            if parsed.input_file.is_none() {
                eprintln!("Error: No input file specified");
                cli_impl::usage();
                return;
            }
            let input_path = parsed.input_file.clone().unwrap();
            let orig_path = PathBuf::from(&input_path);

            // Route .adl manifest files through the manifest parser/validator
            // instead of the Adesh type checker (which expects ':' struct syntax).
            if orig_path.extension().is_some_and(|ext| ext == "adl") {
                // Lock files (*.lock.adl) use a separate parser
                let is_lockfile = orig_path
                    .file_stem()
                    .is_some_and(|stem| stem.to_string_lossy().ends_with(".lock"));
                if is_lockfile {
                    let src = match fs::read_to_string(&orig_path) {
                        Ok(s) => s,
                        Err(e) => {
                            eprintln!("Error reading file: {}", e);
                            std::process::exit(1);
                        }
                    };
                    match adeshlang::ecosystem::LockFile::parse(&src) {
                        Ok(_) => println!("✓ Lockfile validation passed"),
                        Err(e) => {
                            eprintln!("Lockfile Error: {}", e);
                            std::process::exit(1);
                        }
                    }
                    return;
                }
                match adeshlang::ecosystem::Manifest::load(&orig_path) {
                    Ok(manifest) => {
                        if let Err(e) = manifest.validate() {
                            eprintln!("Manifest Error: {}", e);
                            std::process::exit(1);
                        }
                        println!("✓ Manifest validation passed");
                    }
                    Err(e) => {
                        eprintln!("Manifest Error: {}", e);
                        std::process::exit(1);
                    }
                }
                return;
            }

            let (_path, src) = match cli_impl::resolve_input_path(&orig_path) {
                Ok(pair) => pair,
                Err(msg) => {
                    eprintln!("{}", msg);
                    std::process::exit(1);
                }
            };
            // 1. Language syntax and semantic analysis
            let semantic_index =
                adeshlang::semantics::index_source_in(&src, Some(&_path.to_string_lossy()));
            if !semantic_index.errors.is_empty() {
                for err in &semantic_index.errors {
                    eprintln!("{}", err);
                }
                std::process::exit(1);
            }

            // 2. Static type checking
            if let Err(e) =
                adeshlang::types::type_system::check_module_in(&src, Some(&_path.to_string_lossy()))
            {
                eprintln!("{}", e);
                std::process::exit(1);
            }

            // 3. Compile-time memory safety, ownership, borrowing, lifetimes validation
            if let Err(e) = cli_impl::check_ownership_and_parse_in(
                &src,
                &parsed.config,
                Some(&_path.to_string_lossy()),
            ) {
                if e.starts_with("error[") || e.starts_with("Compile-time") || e.contains('\n') {
                    eprintln!("{}", e);
                } else {
                    eprintln!("Safety/Ownership Error: {}", e);
                }
                std::process::exit(1);
            }
            println!("✓ Syntax, semantics, type check, and memory safety validation passed");
        }
        "init" => {
            if parsed.input_file.is_none() {
                cli_impl::usage();
                return;
            }
            if let Err(e) = cli_impl::cmd_init(&parsed.input_file.unwrap()) {
                eprintln!("init error: {}", e);
            }
        }
        "build" => {
            // New unified build command - routes to AOT pipeline
            use adeshlang::cli::build::{
                build_help_message, execute_build, parse_build_args, run_executable,
            };

            // Check for help flag
            if parsed.show_help || args.iter().any(|a| a == "-h" || a == "--help") {
                println!("{}", build_help_message());
                return;
            }

            // Find the "build" command index
            let build_idx = args.iter().position(|a| a == "build").unwrap_or(1);

            // Parse build-specific arguments (returns config and whether it's a run subcommand)
            // Parse build-specific arguments (returns config and whether it's a run subcommand)
            let (build_config, _is_run) = match parse_build_args(&args, build_idx + 1) {
                Ok(result) => result,
                Err(e) => {
                    eprintln!("\x1b[31m\x1b[1mError:\x1b[0m {}", e);
                    println!("{}", build_help_message());
                    std::process::exit(1);
                }
            };

            // Resolve targets (allows multiple binaries build/run)
            let targets = match adeshlang::cli::build::resolve_project_targets(&build_config) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("\x1b[31m\x1b[1mError:\x1b[0m {}", e);
                    std::process::exit(1);
                }
            };

            let mut run_target = None;

            for (idx, target_config) in targets.iter().enumerate() {
                if targets.len() > 1 {
                    println!(
                        "\x1b[32m\x1b[1mBuilding target:\x1b[0m {}",
                        target_config.input.display()
                    );
                }

                // Read source for ownership checks
                let src = match std::fs::read_to_string(&target_config.input) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!(
                            "\x1b[31m\x1b[1mError:\x1b[0m Failed to read {}: {}",
                            target_config.input.display(),
                            e
                        );
                        std::process::exit(1);
                    }
                };

                // Enforce static type checking for build/AOT pipeline.
                if !target_config.dry_run {
                    if let Err(e) = adeshlang::types::type_system::check_module_in(
                        &src,
                        Some(&target_config.input.to_string_lossy()),
                    ) {
                        eprintln!(
                            "\x1b[31m\x1b[1mType Error in {}:\x1b[0m {}",
                            target_config.input.display(),
                            e
                        );
                        std::process::exit(1);
                    }
                }

                // Run mandatory compile-time safety checks (skip for dry-run)
                if !target_config.dry_run {
                    if target_config.verbose {
                        eprintln!(
                            "\x1b[36m🔒 Performing compile-time memory safety validation for {}...\x1b[0m",
                            target_config.input.display()
                        );
                    }

                    if let Err(e) = cli_impl::check_ownership_and_parse_in(
                        &src,
                        &parsed.config,
                        Some(&target_config.input.to_string_lossy()),
                    ) {
                        eprintln!("error in {}: {}", target_config.input.display(), e);
                        std::process::exit(1);
                    }

                    if target_config.verbose {
                        eprintln!(
                            "compile-time memory safety checks passed for {}",
                            target_config.input.display()
                        );
                    }
                }

                // Execute the build
                let output_path = match execute_build(target_config) {
                    Ok(path) => path,
                    Err(e) => {
                        eprintln!(
                            "\x1b[31m\x1b[1mBuild error on {}:\x1b[0m {}",
                            target_config.input.display(),
                            e
                        );
                        std::process::exit(1);
                    }
                };

                if idx == 0 {
                    run_target = Some((output_path, target_config.clone()));
                }
            }

            // Run the executable if requested
            if let Some((output_path, target_config)) = run_target {
                if target_config.run_after_build
                    && !target_config.dry_run
                    && !target_config.check_only
                {
                    match run_executable(&output_path, &target_config.program_args) {
                        Ok(exit_code) => {
                            if exit_code != 0 {
                                std::process::exit(exit_code);
                            }
                        }
                        Err(e) => {
                            eprintln!("\x1b[31m\x1b[1mRun error:\x1b[0m {}", e);
                            std::process::exit(1);
                        }
                    }
                }
            }
        }
        "target" => {
            // Target information subcommand
            let subcommand = args.get(2).map(|s| s.as_str()).unwrap_or("list");
            match subcommand {
                "list" => {
                    println!("Available cross-compilation targets:");
                    println!();
                    println!("  Linux:");
                    println!("    x86_64-unknown-linux-gnu     Linux x86_64 (glibc)");
                    println!("    x86_64-unknown-linux-musl    Linux x86_64 (musl, static)");
                    println!("    aarch64-unknown-linux-gnu    Linux ARM64");
                    println!("    arm-unknown-linux-gnueabihf  Linux ARM32 (hard float)");
                    println!();
                    println!("  Windows:");
                    println!("    x86_64-pc-windows-gnu        Windows x86_64 (MinGW)");
                    println!("    x86_64-pc-windows-msvc       Windows x86_64 (MSVC)");
                    println!("    i686-pc-windows-gnu          Windows x86 (MinGW)");
                    println!();
                    println!("  macOS:");
                    println!("    x86_64-apple-darwin          macOS x86_64 (Intel)");
                    println!("    aarch64-apple-darwin         macOS ARM64 (Apple Silicon)");
                    println!();
                    println!("  Mobile:");
                    println!("    aarch64-linux-android        Android ARM64");
                    println!("    armv7-linux-androideabi      Android ARM32");
                    println!("    aarch64-apple-ios            iOS ARM64");
                    println!();
                    println!("  Embedded:");
                    println!("    thumbv7em-none-eabihf        ARM Cortex-M4/M7");
                    println!("    riscv64gc-unknown-none-elf   RISC-V 64-bit");
                    println!();
                    println!("Use: adesh build --target=<TRIPLE> <file.adesh>");
                }
                "info" => {
                    if let Some(triple) = args.get(3) {
                        println!("Target: {}", triple);
                        // Parse and display target info
                        match triple.parse::<target_lexicon::Triple>() {
                            Ok(t) => {
                                println!("  Architecture: {:?}", t.architecture);
                                println!("  OS:           {:?}", t.operating_system);
                                println!("  Environment:  {:?}", t.environment);
                                println!("  Vendor:       {:?}", t.vendor);
                            }
                            Err(e) => {
                                eprintln!("Invalid target triple: {}", e);
                                std::process::exit(1);
                            }
                        }
                    } else {
                        eprintln!("Usage: adesh target info <TRIPLE>");
                        std::process::exit(1);
                    }
                }
                _ => {
                    eprintln!("Unknown target subcommand: {}", subcommand);
                    eprintln!("Usage: adesh target [list|info <TRIPLE>]");
                    std::process::exit(1);
                }
            }
        }
        "gpu-check" => {
            run_gpu_check(&args);
        }
        "clean" => {
            let verbose = args.iter().any(|a| a == "-v" || a == "--verbose");
            let extensions = ["o", "obj", "a", "lib", "so", "dll", "dylib"];
            let mut removed = 0;

            if let Ok(entries) = std::fs::read_dir(".") {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(ext) = path.extension() {
                            let ext_str = ext.to_string_lossy().to_lowercase();
                            if extensions.contains(&ext_str.as_str()) {
                                if verbose {
                                    println!("Removing: {}", path.display());
                                }
                                if let Err(e) = std::fs::remove_file(&path) {
                                    eprintln!(
                                        "Warning: Failed to remove {}: {}",
                                        path.display(),
                                        e
                                    );
                                } else {
                                    removed += 1;
                                }
                            }
                        }
                    }
                }
            }

            if removed > 0 {
                println!("Cleaned {} build artifact(s)", removed);
            } else {
                println!("No build artifacts to clean");
            }
        }
        "" => {
            cli_impl::usage();
        }
        _ => cli_impl::usage(),
    }
}

/// Run the `gpu-check` command: probe all GPU backends and print a compatibility report.
fn run_gpu_check(args: &[String]) {
    let verbose = args.iter().any(|a| a == "-v" || a == "--verbose");
    let json = args.iter().any(|a| a == "--json");

    use adeshlang::backends::mlir::gpu::{CheckStatus, check_device_compatibility};

    let report = check_device_compatibility();

    if json {
        // Emit machine-readable JSON (no header)
        println!("{{");
        println!("  \"compatible\": {},", report.compatible());
        println!("  \"ok_count\": {},", report.ok_count());
        println!("  \"error_count\": {},", report.error_count());
        println!("  \"entries\": [");
        for (i, entry) in report.entries.iter().enumerate() {
            let comma = if i + 1 < report.entries.len() {
                ","
            } else {
                ""
            };
            println!(
                "    {{\"category\": {:?}, \"label\": {:?}, \"status\": {:?}, \"detail\": {:?}}}{}",
                entry.category,
                entry.label,
                format!("{:?}", entry.status),
                entry.detail,
                comma
            );
        }
        println!("  ]");
        println!("}}");
        return;
    }

    // Human-readable grouped output
    println!("AdeshLang GPU Device Compatibility Check");
    println!("========================================");
    println!();
    let mut current_cat = "";
    for entry in &report.entries {
        // Skip Missing entries unless verbose
        if !verbose && entry.status == CheckStatus::Missing {
            continue;
        }
        if entry.category != current_cat {
            if !current_cat.is_empty() {
                println!();
            }
            println!("  [{category}]", category = entry.category);
            current_cat = entry.category;
        }
        let sym = entry.status.symbol();
        println!("  {} {:<38} {}", sym, entry.label, entry.detail);
    }

    println!();
    println!("────────────────────────────────────────");

    // Derive detected backend from the report itself (not just env vars)
    let detected_backend = {
        let has_cuda = report
            .entries
            .iter()
            .any(|e| e.category == "CUDA/NVIDIA" && e.status == CheckStatus::Ok);
        let has_rocm = report
            .entries
            .iter()
            .any(|e| e.category == "ROCm/HIP" && e.status == CheckStatus::Ok);
        let has_vulkan = report
            .entries
            .iter()
            .any(|e| e.category == "Vulkan" && e.status == CheckStatus::Ok);
        let has_metal = report
            .entries
            .iter()
            .any(|e| e.category == "Metal" && e.status == CheckStatus::Ok);
        if has_cuda {
            "CUDA (NVIDIA)"
        } else if has_rocm {
            "ROCm/HIP (AMD)"
        } else if has_vulkan {
            "Vulkan"
        } else if has_metal {
            "Metal (Apple)"
        } else {
            "none detected"
        }
    };

    // Determine runtime auto-select backend (env-var based, for actual execution)
    let runtime_target = adeshlang::backends::mlir::gpu::resolve_target(
        adeshlang::toolchain::config::GpuTarget::Auto,
    );
    let runtime_backend = match runtime_target {
        adeshlang::toolchain::config::GpuTarget::Cuda => "CUDA (via CUDA_VISIBLE_DEVICES)",
        adeshlang::toolchain::config::GpuTarget::Rocm => "ROCm/HIP (via ROCR_VISIBLE_DEVICES)",
        adeshlang::toolchain::config::GpuTarget::Vulkan => "Vulkan (via VK_ICD_FILENAMES)",
        adeshlang::toolchain::config::GpuTarget::Metal => "Metal (via METAL_DEVICE_WRAPPER_TYPE)",
        adeshlang::toolchain::config::GpuTarget::Auto => {
            "auto (set CUDA_VISIBLE_DEVICES etc. to override)"
        }
    };

    let toolchain = adeshlang::backends::mlir::gpu::detect_toolchain(runtime_target);
    let tc_ok = if toolchain.is_available() {
        "ready ✓"
    } else {
        "incomplete — some tools missing ✗"
    };

    println!();
    println!("  Detected hardware    : {}", detected_backend);
    println!("  Runtime auto-select  : {}", runtime_backend);
    println!("  MLIR toolchain       : {}", tc_ok);
    if report.error_count() == 0 {
        println!("  Overall              : COMPATIBLE ✓");
    } else {
        println!(
            "  Overall              : {} issue(s) found ✗",
            report.error_count()
        );
    }
    println!();

    if !verbose {
        println!(
            "  Tip: run with -v / --verbose to see all checks including missing-but-optional items."
        );
    }
    if !report.compatible() {
        std::process::exit(1);
    }
}
