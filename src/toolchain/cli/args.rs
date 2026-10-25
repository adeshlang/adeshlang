//! Command Line Argument Parser
//!
//! Parses CLI arguments into RuntimeConfig.

use crate::toolchain::config::{
    ExecutionBackend, OptLevel, RecursionOptMode, RuntimeConfig, TestOutputFormat,
};
use crate::toolchain::resolver::ToolchainPreference;

/// Parsed command line arguments
#[derive(Debug, Clone)]
pub struct ParsedArgs {
    /// The command to execute (run, repl, compile, etc.)
    pub command: String,
    /// Input file path (if applicable)
    pub input_file: Option<String>,
    /// Output file path (if applicable)
    pub output_file: Option<String>,
    /// Runtime configuration
    pub config: RuntimeConfig,
    /// Additional program arguments
    pub program_args: Vec<String>,
    /// Show help message
    pub show_help: bool,
    /// Show version
    pub show_version: bool,
    /// Select the native compiler toolchain for commands that need one.
    pub toolchain_preference: Option<ToolchainPreference>,
    /// Render environment information as JSON or shell exports.
    pub env_json: bool,
    pub env_shell: bool,
    /// Inline code string to evaluate directly (-e / --eval)
    pub eval_code: Option<String>,
    /// Diagnostic code to explain (--explain <CODE>)
    pub explain_code: Option<String>,
    /// Developer debug backtrace flag (--debug-backtrace)
    pub debug_backtrace: bool,
}

impl Default for ParsedArgs {
    fn default() -> Self {
        Self {
            command: String::new(),
            input_file: None,
            output_file: None,
            config: RuntimeConfig::new(),
            program_args: Vec::new(),
            show_help: false,
            show_version: false,
            toolchain_preference: None,
            env_json: false,
            env_shell: false,
            eval_code: None,
            explain_code: None,
            debug_backtrace: false,
        }
    }
}

impl ParsedArgs {
    /// Parse command line arguments
    pub fn parse(args: &[String]) -> Result<Self, String> {
        if args.len() < 2 {
            return Ok(Self {
                command: String::new(),
                input_file: None,
                output_file: None,
                config: RuntimeConfig::new(),
                program_args: Vec::new(),
                show_help: true,
                show_version: false,
                toolchain_preference: None,
                env_json: false,
                env_shell: false,
                eval_code: None,
                explain_code: None,
                debug_backtrace: false,
            });
        }

        let mut config = RuntimeConfig::new();
        let mut command = String::new();
        let mut input_file: Option<String> = None;
        let mut output_file: Option<String> = None;
        let mut eval_code: Option<String> = None;
        let mut explain_code: Option<String> = None;
        let mut debug_backtrace = false;
        let mut program_args: Vec<String> = Vec::new();
        let mut show_help = false;
        let mut show_version = false;
        let mut toolchain_preference = None;
        let mut env_json = false;
        let mut env_shell = false;
        let mut collecting_program_args = false;
        let mut positional_args: Vec<String> = Vec::new();

        let is_monitor_cmd = args
            .get(1)
            .map(|s| matches!(s.as_str(), "monitor" | "aro" | "observatory"))
            .unwrap_or(false);

        let mut i = 1; // Skip program name
        while i < args.len() {
            let arg = &args[i];

            if is_monitor_cmd && i > 1 {
                if positional_args.len() < 2 && !arg.starts_with('-') {
                    positional_args.push(arg.clone());
                } else {
                    program_args.push(arg.clone());
                }
                i += 1;
                continue;
            }

            // If we hit "--", start collecting program args
            if arg == "--" {
                collecting_program_args = true;
                i += 1;
                continue;
            }

            if collecting_program_args {
                program_args.push(arg.clone());
                i += 1;
                continue;
            }

            // Parse options
            match arg.as_str() {
                "--help" | "-h" => {
                    show_help = true;
                }
                "--version" | "-v" => {
                    show_version = true;
                }
                "--debug-backtrace" => {
                    debug_backtrace = true;
                }
                "--explain" => {
                    if i + 1 < args.len() {
                        explain_code = Some(args[i + 1].clone());
                        i += 1;
                    } else {
                        explain_code = Some("list".to_string());
                    }
                }
                _ if arg.starts_with("--explain=") => {
                    explain_code = Some(arg["--explain=".len()..].to_string());
                }
                "-e" | "--eval" => {
                    if i + 1 < args.len() {
                        eval_code = Some(args[i + 1].clone());
                        i += 1;
                    } else {
                        return Err("-e / --eval requires a code string".to_string());
                    }
                }
                _ if arg.starts_with("--eval=") => {
                    eval_code = Some(arg["--eval=".len()..].to_string());
                }
                _ if arg.starts_with("-e") && arg.len() > 2 => {
                    eval_code = Some(arg[2..].to_string());
                }
                "--system-toolchain" => {
                    toolchain_preference = Some(ToolchainPreference::System);
                }
                "--system"
                    if command == "toolchain"
                        && !matches!(
                            positional_args.get(1).map(String::as_str),
                            Some("install") | Some("expose")
                        ) =>
                {
                    // `--system` selects the system toolchain for
                    // `adesh toolchain check/list/...`, but is a scope flag
                    // for `adesh toolchain install/expose` — pass it through.
                    toolchain_preference = Some(ToolchainPreference::System);
                }
                "--bundled-toolchain" => {
                    toolchain_preference = Some(ToolchainPreference::Bundled);
                }
                "--toolchain" if i + 1 < args.len() && !args[i + 1].starts_with("-") => {
                    toolchain_preference = Some(match args[i + 1].as_str() {
                        "bundled" => ToolchainPreference::Bundled,
                        "system" => ToolchainPreference::System,
                        value => return Err(format!("Invalid toolchain: {value}")),
                    });
                    i += 1;
                }
                "--json" => {
                    env_json = true;
                }
                "--shell" => {
                    env_shell = true;
                }
                "--jit" => {
                    config.backend = ExecutionBackend::Jit;
                }
                "--jit-native" | "--native-jit" | "--njit" => {
                    config.backend = ExecutionBackend::NativeJit;
                }
                "--interpreter" | "--interp" => {
                    config.backend = ExecutionBackend::Interpreter;
                }
                "--mixed" | "--hybrid" => {
                    config.backend = ExecutionBackend::Mixed;
                }
                "--safe" => {
                    config.backend = ExecutionBackend::Safe;
                }
                "--bytecode" | "--bc" | "--vm" => {
                    eprintln!(
                        "Warning: Bytecode VM backend is deprecated. Native Toolchain and Interpreter are the standard backends."
                    );
                    config.backend = ExecutionBackend::Bytecode;
                }
                "--experimental-vm" => {
                    config.backend = ExecutionBackend::Bytecode;
                }
                "--adaptive" | "--adaptive-jit" | "--ajit" => {
                    config.backend = ExecutionBackend::AdaptiveJit;
                }
                "--tiered" | "--tiered-jit" | "--tjit" => {
                    config.backend = ExecutionBackend::TieredJit;
                }
                "--aot" | "--aot-native" | "--compile" => {
                    config.backend = ExecutionBackend::Aot;
                }
                "--wasm" => {
                    config.backend = ExecutionBackend::Wasm;
                }
                #[cfg(debug_assertions)]
                "--gpu" => {
                    config.backend = ExecutionBackend::Gpu;
                }
                "--gpu-target" => {
                    if i + 1 < args.len() && !args[i + 1].starts_with("-") {
                        let target = crate::toolchain::config::GpuTarget::from_str(&args[i + 1])
                            .ok_or_else(|| format!("Invalid GPU target: {}", args[i + 1]))?;
                        config.gpu_target = target;
                        i += 1;
                    } else {
                        return Err("--gpu-target requires a value".to_string());
                    }
                }
                _ if arg.starts_with("--gpu-target=") => {
                    let target_str = arg.split('=').nth(1).unwrap_or("auto");
                    let target = crate::toolchain::config::GpuTarget::from_str(target_str)
                        .ok_or_else(|| format!("Invalid GPU target: {}", target_str))?;
                    config.gpu_target = target;
                }
                "--gpu-grid" => {
                    if i + 1 < args.len() && !args[i + 1].starts_with("-") {
                        config.gpu_grid = parse_dim3(&args[i + 1])?;
                        i += 1;
                    } else {
                        return Err("--gpu-grid requires a value (x,y,z)".to_string());
                    }
                }
                _ if arg.starts_with("--gpu-grid=") => {
                    let grid_str = arg.split('=').nth(1).unwrap_or("1,1,1");
                    config.gpu_grid = parse_dim3(grid_str)?;
                }
                "--gpu-block" => {
                    if i + 1 < args.len() && !args[i + 1].starts_with("-") {
                        config.gpu_block = parse_dim3(&args[i + 1])?;
                        i += 1;
                    } else {
                        return Err("--gpu-block requires a value (x,y,z)".to_string());
                    }
                }
                _ if arg.starts_with("--gpu-block=") => {
                    let block_str = arg.split('=').nth(1).unwrap_or("256,1,1");
                    config.gpu_block = parse_dim3(block_str)?;
                }
                "--gpu-shared-mem" => {
                    if i + 1 < args.len() && !args[i + 1].starts_with("-") {
                        config.gpu_shared_mem = args[i + 1].parse::<usize>().map_err(|_| {
                            "--gpu-shared-mem requires an integer byte size".to_string()
                        })?;
                        i += 1;
                    } else {
                        return Err("--gpu-shared-mem requires a value".to_string());
                    }
                }
                _ if arg.starts_with("--gpu-shared-mem=") => {
                    let mem_str = arg.split('=').nth(1).unwrap_or("0");
                    config.gpu_shared_mem = mem_str.parse::<usize>().map_err(|_| {
                        "--gpu-shared-mem requires an integer byte size".to_string()
                    })?;
                }
                "--use-lir" => {
                    config.use_lir = true;
                }
                "--no-warnings"
                | "--disable-warnings"
                | "--allow-warnings"
                | "-Wallow"
                | "-Wno-unused"
                | "--no-unused-warnings" => {
                    config.disable_warnings = true;
                }
                "--dump-ast" => {
                    config.dump.dump_ast = true;
                }
                "--dump-hir" | "--dump-ir" => {
                    config.dump.dump_hir = true;
                }
                "--dump-mir" => {
                    config.dump.dump_mir = true;
                }
                "--dump-lir" => {
                    config.dump.dump_lir = true;
                }
                "--dump-vir" => {
                    config.dump.dump_vir = true;
                }
                "--dump-mlir" => {
                    config.dump.dump_mlir = true;
                }
                "--mlir-gpu" => {
                    config.dump.mlir_enable_gpu = true;
                    config.dump.dump_mlir = true;
                }
                "--dump-bytecode" | "--dump-bc" => {
                    config.dump.dump_bytecode = true;
                }
                "--dump-all" => {
                    config.dump.dump_all = true;
                }
                "--buffered-stdout" => {
                    config.io.buffered_stdout = true;
                }
                "--interactive-input" | "--interactive" => {
                    config.io.interactive_input = true;
                }
                "--no-color" => {
                    config.io.no_color = true;
                }
                "--profile" | "--time" => {
                    config.profile = true;
                }
                "--test" | "--tests" => {
                    config.run_tests = true;
                }
                _ if arg.starts_with('-') && arg.trim_start_matches('-') == "exact" => {
                    config.test_exact = true;
                    config.run_tests = true;
                }
                _ if arg.starts_with('-')
                    && arg.trim_start_matches('-') == "skip"
                    && i + 1 < args.len()
                    && !args[i + 1].starts_with('-') =>
                {
                    config.test_skip = Some(args[i + 1].clone());
                    config.run_tests = true;
                    i += 1;
                }
                _ if arg.starts_with('-') && arg.trim_start_matches('-').starts_with("skip=") => {
                    let val = arg.trim_start_matches('-')["skip=".len()..].to_string();
                    config.test_skip = Some(val);
                    config.run_tests = true;
                }
                _ if arg.starts_with('-') && arg.trim_start_matches('-') == "list" => {
                    config.test_list = true;
                    config.run_tests = true;
                }
                _ if arg.starts_with('-') && arg.trim_start_matches('-') == "ignored" => {
                    config.test_ignored = true;
                    config.run_tests = true;
                }
                _ if arg.starts_with('-') && arg.trim_start_matches('-') == "include-ignored" => {
                    config.test_include_ignored = true;
                    config.run_tests = true;
                }
                "--test-threads" | "-j" if i + 1 < args.len() && !args[i + 1].starts_with('-') => {
                    if let Ok(n) = args[i + 1].parse::<usize>() {
                        config.test_threads = Some(n);
                    }
                    config.run_tests = true;
                    i += 1;
                }
                _ if arg.starts_with("--test-threads=") => {
                    if let Ok(n) = arg["--test-threads=".len()..].parse::<usize>() {
                        config.test_threads = Some(n);
                    }
                    config.run_tests = true;
                }
                _ if arg.starts_with("-j=") => {
                    if let Ok(n) = arg["-j=".len()..].parse::<usize>() {
                        config.test_threads = Some(n);
                    }
                    config.run_tests = true;
                }
                "--serial" | "--sequential" | "--one-by-one" => {
                    config.test_serial = true;
                    config.run_tests = true;
                }
                "--parallel" => {
                    config.test_serial = false;
                    config.run_tests = true;
                }
                "--backend-check" => {
                    config.backend_check = true;
                    config.run_tests = true;
                }
                _ if arg.starts_with("--runtimes=") => {
                    // e.g. --runtimes=jit,njit,vm,aot,wasm,gpu
                    let list = arg["--runtimes=".len()..].trim();
                    for token in list.split(',').map(|t| t.trim()).filter(|t| !t.is_empty()) {
                        match ExecutionBackend::from_str(token) {
                            Some(b) => {
                                if !config.test_backends.contains(&b) {
                                    config.test_backends.push(b);
                                }
                            }
                            None => {
                                eprintln!(
                                    "Warning: unknown runtime backend '{}' in --runtimes (ignored)",
                                    token
                                );
                            }
                        }
                    }
                    config.run_tests = true;
                }
                "--runtimes" => {
                    // --runtimes jit,njit (space-separated value)
                    if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                        let list = &args[i + 1];
                        for token in list.split(',').map(|t| t.trim()).filter(|t| !t.is_empty()) {
                            match ExecutionBackend::from_str(token) {
                                Some(b) => {
                                    if !config.test_backends.contains(&b) {
                                        config.test_backends.push(b);
                                    }
                                }
                                None => {
                                    eprintln!(
                                        "Warning: unknown runtime backend '{}' in --runtimes (ignored)",
                                        token
                                    );
                                }
                            }
                        }
                        config.run_tests = true;
                        i += 1;
                    } else {
                        return Err(
                            "--runtimes requires a comma-separated list of backends".to_string()
                        );
                    }
                }
                "--fail-fast" => {
                    config.fail_fast = true;
                }
                "--ffi-debug" => {
                    config.ffi_debug = true;
                }
                "--embedded" => {
                    config.embedded = true;
                }
                "--no-check-ownership" | "--no-ownership" => {
                    config.check_ownership = false;
                }
                "--check-ownership" | "--ownership" => {
                    config.check_ownership = true;
                }
                "--no-check-moves" | "--no-moves" => {
                    config.check_moves = false;
                }
                "--check-moves" => {
                    config.check_moves = true;
                }
                "--type-check" | "--strict-types" | "--show-inferred" => {
                    // Static type checking and inference reporting are always on.
                }
                "--import" => {
                    if i + 1 < args.len() && !args[i + 1].starts_with("-") {
                        config.ffi_imports.push(args[i + 1].clone());
                        i += 1;
                    } else {
                        return Err("--import requires a header path".to_string());
                    }
                }
                "--wasm-module" => {
                    if i + 1 < args.len() && !args[i + 1].starts_with("-") {
                        config.wasm_modules.push(args[i + 1].clone());
                        i += 1;
                    }
                }
                #[allow(unreachable_patterns)]
                "--wasm" => {
                    config.backend = ExecutionBackend::Wasm;
                }
                "--memory" | "--mem" => {
                    config.show_memory = true;
                }
                "--verbose" => {
                    config.verbose = true;
                }
                "--quiet" | "-q" => {
                    config.quiet = true;
                }
                "--nocapture" => {
                    config.test_nocapture = true;
                }
                "--debug" => {
                    config.debug = true;
                }
                // Recursion optimization shortcuts
                "--fast-recursion" | "--recursion-opt-full" => {
                    config.recursion_opt = RecursionOptMode::Full;
                }
                "--no-recursion-opt" | "--recursion-opt-none" => {
                    config.recursion_opt = RecursionOptMode::None;
                }
                "--tco" | "--tail-call" => {
                    config.recursion_opt = RecursionOptMode::Tco;
                }
                "--memo" | "--memoize" => {
                    config.recursion_opt = RecursionOptMode::Memo;
                }
                _ if arg.starts_with("--recursion-opt=") => {
                    let mode_str = arg.split('=').nth(1).unwrap_or("full");
                    config.recursion_opt = RecursionOptMode::from_str(mode_str)
                        .ok_or_else(|| format!("Invalid recursion opt mode: {}", mode_str))?;
                }
                "--release" => {
                    config.opt_level = OptLevel::O3;
                    config.debug = false;
                    program_args.push(arg.clone());
                }
                "--opt-level" if i + 1 < args.len() && !args[i + 1].starts_with("-") => {
                    let level_str = &args[i + 1];
                    config.opt_level = OptLevel::from_str(level_str)
                        .ok_or_else(|| format!("Invalid optimization level: {}", level_str))?;
                    program_args.push(arg.clone());
                    program_args.push(args[i + 1].clone());
                    i += 1;
                }
                _ if arg.starts_with("--opt-level=") => {
                    let level_str = &arg["--opt-level=".len()..];
                    config.opt_level = OptLevel::from_str(level_str)
                        .ok_or_else(|| format!("Invalid optimization level: {}", level_str))?;
                    program_args.push(arg.clone());
                }
                "--jobs" if i + 1 < args.len() && !args[i + 1].starts_with("-") => {
                    program_args.push(arg.clone());
                    program_args.push(args[i + 1].clone());
                    i += 1;
                }
                _ if arg.starts_with("--jobs=") || arg.starts_with("-j=") => {
                    program_args.push(arg.clone());
                }
                "--features" if i + 1 < args.len() && !args[i + 1].starts_with("-") => {
                    program_args.push(arg.clone());
                    program_args.push(args[i + 1].clone());
                    i += 1;
                }
                _ if arg.starts_with("--features=") => {
                    program_args.push(arg.clone());
                }
                "--lto" if i + 1 < args.len() && !args[i + 1].starts_with("-") => {
                    program_args.push(arg.clone());
                    program_args.push(args[i + 1].clone());
                    i += 1;
                }
                "--lto" | "--enable-lto" => {
                    program_args.push(arg.clone());
                }
                "--disable-lto" | "--no-lto" => {
                    program_args.push(arg.clone());
                }
                _ if arg.starts_with("--lto=") => {
                    program_args.push(arg.clone());
                }
                "--pgo" if i + 1 < args.len() && !args[i + 1].starts_with("-") => {
                    program_args.push(arg.clone());
                    program_args.push(args[i + 1].clone());
                    i += 1;
                }
                _ if arg.starts_with("--pgo=") => {
                    program_args.push(arg.clone());
                }
                "--sanitizer" if i + 1 < args.len() && !args[i + 1].starts_with("-") => {
                    program_args.push(arg.clone());
                    program_args.push(args[i + 1].clone());
                    i += 1;
                }
                _ if arg.starts_with("--sanitizer=") => {
                    program_args.push(arg.clone());
                }
                "--hardening" => {
                    program_args.push(arg.clone());
                }
                "--target" if i + 1 < args.len() && !args[i + 1].starts_with("-") => {
                    program_args.push(arg.clone());
                    program_args.push(args[i + 1].clone());
                    i += 1;
                }
                "--emit" if i + 1 < args.len() && !args[i + 1].starts_with("-") => {
                    program_args.push(arg.clone());
                    program_args.push(args[i + 1].clone());
                    i += 1;
                }
                _ if arg.starts_with("--emit=") => {
                    program_args.push(arg.clone());
                }
                _ if arg.starts_with("--opt=")
                    || arg.starts_with("--opt ")
                    || arg.starts_with("-O") =>
                {
                    let level_str = if arg.starts_with("-O") {
                        &arg[2..]
                    } else {
                        arg.split('=').nth(1).unwrap_or("1")
                    };
                    config.opt_level = OptLevel::from_str(level_str)
                        .ok_or_else(|| format!("Invalid optimization level: {}", level_str))?;
                }
                _ if arg.starts_with("--backend=") => {
                    let backend_str = arg.split('=').nth(1).unwrap_or("interpreter");
                    config.backend = ExecutionBackend::from_str(backend_str)
                        .ok_or_else(|| format!("Invalid backend: {}", backend_str))?;
                }
                _ if arg.starts_with("--include-tags=") => {
                    let tags = arg.split('=').nth(1).unwrap_or("");
                    for tag in tags.split(',').filter(|t| !t.is_empty()) {
                        config.include_tags.push(tag.trim().to_string());
                    }
                }
                "--tags" if i + 1 < args.len() && !args[i + 1].starts_with('-') => {
                    for tag in args[i + 1].split(',').filter(|t| !t.is_empty()) {
                        config.include_tags.push(tag.trim().to_string());
                    }
                    i += 1;
                }
                _ if arg.starts_with("--tags=") => {
                    let tags = arg.split('=').nth(1).unwrap_or("");
                    for tag in tags.split(',').filter(|t| !t.is_empty()) {
                        config.include_tags.push(tag.trim().to_string());
                    }
                }
                _ if arg.starts_with("--test-name=") => {
                    let name = arg.split('=').nth(1).unwrap_or("");
                    if !name.is_empty() {
                        config.test_name = Some(name.to_string());
                        config.run_tests = true;
                    }
                }
                _ if arg.starts_with("--format=") => {
                    let fmt = arg.split('=').nth(1).unwrap_or("text");
                    config.test_output_format = match fmt {
                        "json" => TestOutputFormat::Json,
                        "text" => TestOutputFormat::Text,
                        _ => return Err(format!("Invalid format: {}", fmt)),
                    };
                }
                // Formatter options (handled in main.rs, but accepted here)
                "--write" | "-w" | "--check" | "--tabs" => {
                    // These options are handled specifically by the format command
                    // Allow them through without error
                }
                // AOT compilation options (handled in main.rs)
                "--shared" | "--dll" | "--so" | "--static" | "--lib" | "--a" | "--object"
                | "--obj" | "--o" | "-c" | "-S" | "--compile-only" | "--emit-asm" | "--emit-ir"
                | "--emit-header" | "--ffi" | "--fast" | "--incremental" | "--no-incremental"
                | "--no-cache" | "-f" | "--force" | "--clear-cache" => {
                    // These are AOT-specific options, passed through to compile-aot command
                    program_args.push(arg.clone());
                }
                "-L" => {
                    if i + 1 < args.len() && !args[i + 1].starts_with("-") {
                        config.lib_paths.push(args[i + 1].clone());
                        program_args.push(arg.clone());
                        program_args.push(args[i + 1].clone());
                        i += 1;
                    } else {
                        return Err("-L requires a path".to_string());
                    }
                }
                "-l" => {
                    if i + 1 < args.len() && !args[i + 1].starts_with("-") {
                        config.link_libs.push(args[i + 1].clone());
                        program_args.push(arg.clone());
                        program_args.push(args[i + 1].clone());
                        i += 1;
                    } else {
                        return Err("-l requires a library name".to_string());
                    }
                }
                _ if arg.starts_with("-L") && arg.len() > 2 => {
                    config.lib_paths.push(arg[2..].to_string());
                    program_args.push(arg.clone());
                }
                _ if arg.starts_with("-l") && arg.len() > 2 => {
                    config.link_libs.push(arg[2..].to_string());
                    program_args.push(arg.clone());
                }
                _ if arg.starts_with("-I") || arg.starts_with("--target=") => {
                    // Include dirs, target triple or other toolchain flags forwarded
                    program_args.push(arg.clone());
                }
                "--dump-ast-write" => {
                    config.dump.dump_ast = true;
                    if i + 1 < args.len()
                        && !args[i + 1].starts_with("-")
                        && args[i + 1].contains('.')
                    {
                        config.dump.write_ast_file = Some(args[i + 1].clone());
                        i += 1;
                    } else {
                        config.dump.write_ast_file = Some(String::new());
                    }
                }
                "--dump-hir-write" | "--dump-ir-write" => {
                    config.dump.dump_hir = true;
                    if i + 1 < args.len()
                        && !args[i + 1].starts_with("-")
                        && args[i + 1].contains('.')
                    {
                        config.dump.write_hir_file = Some(args[i + 1].clone());
                        i += 1;
                    } else {
                        config.dump.write_hir_file = Some(String::new());
                    }
                }
                "--dump-lir-write" => {
                    config.dump.dump_lir = true;
                    if i + 1 < args.len()
                        && !args[i + 1].starts_with("-")
                        && args[i + 1].contains('.')
                    {
                        config.dump.write_lir_file = Some(args[i + 1].clone());
                        i += 1;
                    } else {
                        config.dump.write_lir_file = Some(String::new());
                    }
                }
                "--dump-vir-write" => {
                    config.dump.dump_vir = true;
                    if i + 1 < args.len()
                        && !args[i + 1].starts_with("-")
                        && args[i + 1].contains('.')
                    {
                        config.dump.write_vir_file = Some(args[i + 1].clone());
                        i += 1;
                    } else {
                        config.dump.write_vir_file = Some(String::new());
                    }
                }
                "--dump-mlir-write" => {
                    config.dump.dump_mlir = true;
                    if i + 1 < args.len()
                        && !args[i + 1].starts_with("-")
                        && args[i + 1].contains('.')
                    {
                        config.dump.write_mlir_file = Some(args[i + 1].clone());
                        i += 1;
                    } else {
                        config.dump.write_mlir_file = Some(String::new());
                    }
                }
                "--dump-cfg" => {
                    config.dump.dump_cfg = true;
                }
                "--dump-cfg-write" => {
                    config.dump.dump_cfg = true;
                    if i + 1 < args.len()
                        && !args[i + 1].starts_with("-")
                        && args[i + 1].contains('.')
                    {
                        config.dump.write_cfg_file = Some(args[i + 1].clone());
                        i += 1;
                    } else {
                        config.dump.write_cfg_file = Some(String::new());
                    }
                }
                _ if arg.starts_with("--indent") => {
                    // --indent is handled by format command
                }
                "--opt" | "--backend" | "--stack-size" | "--stack" | "--heap-size" | "--heap"
                | "--timeout" | "--timeout-ms" => {
                    // Skip here, will be handled in second parsing loop (these take separate arguments)
                    i += 1; // Skip the value as well
                }
                _ if arg.starts_with("-") => {
                    // Unknown option - treat as positional argument
                    // This allows passing flags to the script without explicit '--' separator
                    // e.g. `adesh run script.adesh --port=8080`
                    positional_args.push(arg.clone());
                }
                _ => {
                    // Positional argument
                    positional_args.push(arg.clone());
                }
            }
            i += 1;
        }

        // Handle option with next argument (but stop at "--")
        let mut i = 1;
        while i < args.len() {
            let arg = &args[i];

            // Stop processing at "--" separator
            if arg == "--" {
                break;
            }

            // Look ahead for options that have values
            match arg.as_str() {
                "--opt" if i + 1 < args.len() && !args[i + 1].starts_with("-") => {
                    config.opt_level = OptLevel::from_str(&args[i + 1])
                        .ok_or_else(|| format!("Invalid optimization level: {}", args[i + 1]))?;
                    i += 2;
                    continue;
                }
                "--backend" if i + 1 < args.len() && !args[i + 1].starts_with("-") => {
                    config.backend = ExecutionBackend::from_str(&args[i + 1])
                        .ok_or_else(|| format!("Invalid backend: {}", args[i + 1]))?;
                    i += 2;
                    continue;
                }
                "--include-tags" if i + 1 < args.len() && !args[i + 1].starts_with("-") => {
                    for tag in args[i + 1].split(',').filter(|t| !t.is_empty()) {
                        config.include_tags.push(tag.trim().to_string());
                    }
                    i += 2;
                    continue;
                }
                "--format" if i + 1 < args.len() && !args[i + 1].starts_with("-") => {
                    config.test_output_format = match args[i + 1].as_str() {
                        "json" => TestOutputFormat::Json,
                        "text" => TestOutputFormat::Text,
                        _ => return Err(format!("Invalid format: {}", args[i + 1])),
                    };
                    i += 2;
                    continue;
                }
                "--test-name" if i + 1 < args.len() && !args[i + 1].starts_with("-") => {
                    config.test_name = Some(args[i + 1].clone());
                    config.run_tests = true;
                    i += 2;
                    continue;
                }
                "--stack-size" | "--stack"
                    if i + 1 < args.len() && !args[i + 1].starts_with("-") =>
                {
                    config.stack_size = parse_memory_size(&args[i + 1])?;
                    i += 2;
                    continue;
                }
                "--heap-size" | "--heap" if i + 1 < args.len() && !args[i + 1].starts_with("-") => {
                    config.heap_size = parse_memory_size(&args[i + 1])?;
                    i += 2;
                    continue;
                }
                "--timeout" | "--timeout-ms"
                    if i + 1 < args.len() && !args[i + 1].starts_with("-") =>
                {
                    config.run_timeout = Some(parse_timeout_arg(&args[i + 1])?);
                    i += 2;
                    continue;
                }
                _ => {
                    i += 1;
                }
            }
        }

        // Assign positional arguments based on command
        if !positional_args.is_empty() {
            command = positional_args[0].clone();
        }
        if command.is_empty() && eval_code.is_some() {
            command = "run".to_string();
        }
        if command == "test" {
            config.run_tests = true;
            if positional_args.len() >= 2 {
                let first_arg = positional_args[1].clone();
                if first_arg.ends_with(".adesh") || std::path::Path::new(&first_arg).exists() {
                    input_file = Some(first_arg);
                    if positional_args.len() >= 3
                        && config.test_name.is_none()
                        && !positional_args[2].starts_with('-')
                    {
                        config.test_name = Some(positional_args[2].clone());
                        program_args.extend(positional_args[3..].iter().cloned());
                    } else {
                        program_args.extend(positional_args[2..].iter().cloned());
                    }
                } else {
                    if config.test_name.is_none() && !first_arg.starts_with('-') {
                        config.test_name = Some(first_arg);
                    }
                    program_args.extend(positional_args[2..].iter().cloned());
                }
            }
        } else if matches!(command.as_str(), "monitor" | "aro" | "observatory") {
            // Forward subcommands and flags for `adesh monitor [subcmd...]`
            if positional_args.len() >= 2 {
                input_file = Some(positional_args[1].clone());
                program_args.extend(positional_args[2..].iter().cloned());
            }
        } else if command == "build" && positional_args.get(1).map(String::as_str) == Some("run") {
            // `adesh build run [file] [args...]`
            if positional_args.len() >= 3 {
                input_file = Some(positional_args[2].clone());
                program_args.extend(positional_args[3..].iter().cloned());
            }
        } else {
            if positional_args.len() >= 2 {
                input_file = Some(positional_args[1].clone());
            }
            if positional_args.len() >= 3 {
                // Commands that take output_file as second positional arg
                if matches!(
                    command.as_str(),
                    "compile"
                        | "compile-aot"
                        | "compile-wasm"
                        | "compile-native"
                        | "compile-wasm-js"
                        | "compile-native-rust"
                        | "docs"
                ) {
                    output_file = Some(positional_args[2].clone());
                    program_args.extend(positional_args[3..].iter().cloned());
                } else {
                    // Commands that don't take output_file
                    if config.run_tests && command == "run" {
                        if config.test_name.is_none() && !positional_args[2].starts_with('-') {
                            config.test_name = Some(positional_args[2].clone());
                            program_args.extend(positional_args[3..].iter().cloned());
                        } else {
                            program_args.extend(positional_args[2..].iter().cloned());
                        }
                    } else {
                        program_args.extend(positional_args[2..].iter().cloned());
                    }
                }
            }
        }

        // Parse any test options from program_args (e.g. passed after -- in `adesh test -- --nocapture`)
        if config.run_tests || command == "test" {
            let mut remaining_prog_args = Vec::new();
            let mut pi = 0;
            while pi < program_args.len() {
                let pa = &program_args[pi];
                let trimmed = pa.trim_start_matches('-');
                match trimmed {
                    "nocapture" => config.test_nocapture = true,
                    "exact" | "e" => config.test_exact = true,
                    "list" => config.test_list = true,
                    "ignored" => config.test_ignored = true,
                    "include-ignored" => config.test_include_ignored = true,
                    "fail-fast" => config.fail_fast = true,
                    "quiet" | "q" => config.quiet = true,
                    "skip"
                        if pi + 1 < program_args.len()
                            && !program_args[pi + 1].starts_with('-') =>
                    {
                        config.test_skip = Some(program_args[pi + 1].clone());
                        pi += 1;
                    }
                    _ if trimmed.starts_with("skip=") => {
                        config.test_skip = Some(trimmed["skip=".len()..].to_string());
                    }
                    "test-threads" | "j"
                        if pi + 1 < program_args.len()
                            && !program_args[pi + 1].starts_with('-') =>
                    {
                        if let Ok(n) = program_args[pi + 1].parse::<usize>() {
                            config.test_threads = Some(n);
                        }
                        pi += 1;
                    }
                    _ if trimmed.starts_with("test-threads=") => {
                        if let Ok(n) = trimmed["test-threads=".len()..].parse::<usize>() {
                            config.test_threads = Some(n);
                        }
                    }
                    _ if trimmed.starts_with("j=") => {
                        if let Ok(n) = trimmed["j=".len()..].parse::<usize>() {
                            config.test_threads = Some(n);
                        }
                    }
                    "serial" | "sequential" | "one-by-one" => {
                        config.test_serial = true;
                    }
                    "parallel" => {
                        config.test_serial = false;
                    }
                    _ if !pa.starts_with('-') && config.test_name.is_none() => {
                        config.test_name = Some(pa.clone());
                    }
                    _ => remaining_prog_args.push(pa.clone()),
                }
                pi += 1;
            }
            program_args = remaining_prog_args;
        }

        Ok(Self {
            command,
            input_file,
            output_file,
            config,
            program_args,
            show_help,
            show_version,
            toolchain_preference,
            env_json,
            env_shell,
            eval_code,
            explain_code,
            debug_backtrace,
        })
    }
}

fn parse_timeout_arg(s: &str) -> Result<std::time::Duration, String> {
    let raw = s.trim().to_lowercase();
    if raw.is_empty() {
        return Err("timeout must be a duration such as 15s, 15000ms, or 15000".into());
    }
    let (num, mult) = if let Some(n) = raw.strip_suffix("ms") {
        (n, 1u64)
    } else if let Some(n) = raw.strip_suffix("s") {
        (n, 1000u64)
    } else if let Some(n) = raw.strip_suffix("m") {
        (n, 60_000u64)
    } else {
        (raw.as_str(), 1u64)
    };
    let ms = num
        .trim()
        .parse::<u64>()
        .map_err(|_| format!("Invalid timeout: {s}"))?
        .saturating_mul(mult);
    if ms == 0 {
        return Err("timeout must be greater than zero".into());
    }
    Ok(std::time::Duration::from_millis(ms))
}

/// Parse memory size string to bytes
/// Supports formats like: "16M", "16MB", "128M", "1G", "1GB" or raw bytes
fn parse_memory_size(s: &str) -> Result<usize, String> {
    let s = s.trim().to_uppercase();

    // Try to parse as raw bytes first
    if let Ok(bytes) = s.parse::<usize>() {
        return Ok(bytes);
    }

    // Parse with suffix
    let (num_str, multiplier) = if s.ends_with("GB") || s.ends_with("GIB") {
        (
            &s[..s.len() - if s.ends_with("GB") { 2 } else { 3 }],
            1024 * 1024 * 1024,
        )
    } else if s.ends_with("G") {
        (&s[..s.len() - 1], 1024 * 1024 * 1024)
    } else if s.ends_with("MB") || s.ends_with("MIB") {
        (
            &s[..s.len() - if s.ends_with("MB") { 2 } else { 3 }],
            1024 * 1024,
        )
    } else if s.ends_with("M") {
        (&s[..s.len() - 1], 1024 * 1024)
    } else if s.ends_with("KB") || s.ends_with("KIB") {
        (&s[..s.len() - if s.ends_with("KB") { 2 } else { 3 }], 1024)
    } else if s.ends_with("K") {
        (&s[..s.len() - 1], 1024)
    } else {
        return Err(format!(
            "Invalid memory size format: {}. Use format like '16M', '128MB', '1G', or raw bytes",
            s
        ));
    };

    num_str
        .trim()
        .parse::<usize>()
        .map(|n| n * multiplier)
        .map_err(|_| format!("Invalid memory size number: {}", s))
}

/// Parse GPU dimension string (x,y,z) into a tuple
fn parse_dim3(s: &str) -> Result<(u32, u32, u32), String> {
    let parts: Vec<&str> = s
        .split(',')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() != 3 {
        return Err(format!(
            "Invalid dimension '{}'. Use format like '1,1,1'",
            s
        ));
    }
    let x = parts[0]
        .parse::<u32>()
        .map_err(|_| format!("Invalid dimension value: {}", parts[0]))?;
    let y = parts[1]
        .parse::<u32>()
        .map_err(|_| format!("Invalid dimension value: {}", parts[1]))?;
    let z = parts[2]
        .parse::<u32>()
        .map_err(|_| format!("Invalid dimension value: {}", parts[2]))?;
    Ok((x, y, z))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_run() {
        let args = vec![
            "adesh".to_string(),
            "run".to_string(),
            "test.adesh".to_string(),
        ];
        let parsed = ParsedArgs::parse(&args).unwrap();
        assert_eq!(parsed.command, "run");
        assert_eq!(parsed.input_file, Some("test.adesh".to_string()));
        assert_eq!(parsed.config.backend, ExecutionBackend::Interpreter);
    }

    #[test]
    fn test_parse_jit_run() {
        let args = vec![
            "adesh".to_string(),
            "run".to_string(),
            "--jit".to_string(),
            "test.adesh".to_string(),
        ];
        let parsed = ParsedArgs::parse(&args).unwrap();
        assert_eq!(parsed.config.backend, ExecutionBackend::Jit);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn test_parse_gpu_run() {
        let args = vec![
            "./target/release/adeshlang.exe".to_string(),
            "run".to_string(),
            "--gpu".to_string(),
            "test.adesh".to_string(),
        ];
        let parsed = ParsedArgs::parse(&args).unwrap();
        assert_eq!(parsed.config.backend, ExecutionBackend::Gpu);
    }

    #[test]
    fn test_parse_optimization() {
        let args = vec![
            "./target/release/adeshlang.exe".to_string(),
            "run".to_string(),
            "-O3".to_string(),
            "test.adesh".to_string(),
        ];
        let parsed = ParsedArgs::parse(&args).unwrap();
        assert_eq!(parsed.config.opt_level, OptLevel::O3);
    }

    #[test]
    fn test_parse_dump_options() {
        let args = vec![
            "./target/release/adeshlang.exe".to_string(),
            "run".to_string(),
            "--dump-hir".to_string(),
            "--dump-lir".to_string(),
            "test.adesh".to_string(),
        ];
        let parsed = ParsedArgs::parse(&args).unwrap();
        assert!(parsed.config.dump.dump_hir);
        assert!(parsed.config.dump.dump_lir);
    }

    #[test]
    fn test_parse_program_args() {
        let args = vec![
            "./target/release/adeshlang.exe".to_string(),
            "run".to_string(),
            "test.adesh".to_string(),
            "--".to_string(),
            "arg1".to_string(),
            "arg2".to_string(),
        ];
        let parsed = ParsedArgs::parse(&args).unwrap();
        assert_eq!(
            parsed.program_args,
            vec!["arg1".to_string(), "arg2".to_string()]
        );
    }

    #[test]
    fn test_parse_dim3_valid() {
        assert_eq!(parse_dim3("2,4,8").unwrap(), (2, 4, 8));
    }

    #[test]
    fn test_parse_dim3_invalid() {
        assert!(parse_dim3("2,4").is_err());
        assert!(parse_dim3("a,b,c").is_err());
    }

    #[cfg(debug_assertions)]
    #[test]
    fn test_parse_gpu_options() {
        let args = vec![
            "./target/release/adeshlang.exe".to_string(),
            "run".to_string(),
            "--gpu".to_string(),
            "--gpu-target=cuda".to_string(),
            "--gpu-grid".to_string(),
            "2,2,1".to_string(),
            "--gpu-block".to_string(),
            "128,1,1".to_string(),
            "--gpu-shared-mem".to_string(),
            "1024".to_string(),
            "test.adesh".to_string(),
        ];
        let parsed = ParsedArgs::parse(&args).unwrap();
        assert_eq!(parsed.config.backend, ExecutionBackend::Gpu);
        assert_eq!(
            parsed.config.gpu_target,
            crate::toolchain::config::GpuTarget::Cuda
        );
        assert_eq!(parsed.config.gpu_grid, (2, 2, 1));
        assert_eq!(parsed.config.gpu_block, (128, 1, 1));
        assert_eq!(parsed.config.gpu_shared_mem, 1024);
    }

    #[test]
    fn test_parse_eval_option() {
        let args = vec![
            "adesh".to_string(),
            "run".to_string(),
            "-e".to_string(),
            "print(\"hello\");".to_string(),
        ];
        let parsed = ParsedArgs::parse(&args).unwrap();
        assert_eq!(parsed.command, "run");
        assert_eq!(parsed.eval_code, Some("print(\"hello\");".to_string()));

        let args2 = vec![
            "adesh".to_string(),
            "-e".to_string(),
            "let x = 10;".to_string(),
        ];
        let parsed2 = ParsedArgs::parse(&args2).unwrap();
        assert_eq!(parsed2.command, "run");
        assert_eq!(parsed2.eval_code, Some("let x = 10;".to_string()));

        let args3 = vec![
            "adesh".to_string(),
            "run".to_string(),
            "--eval=print(42);".to_string(),
        ];
        let parsed3 = ParsedArgs::parse(&args3).unwrap();
        assert_eq!(parsed3.command, "run");
        assert_eq!(parsed3.eval_code, Some("print(42);".to_string()));
    }

    #[test]
    fn test_toolchain_install_flags_pass_through() {
        // `adesh toolchain install --system --force` must deliver every flag
        // to the install command (scope flags are NOT toolchain preferences).
        let args = vec![
            "adesh".to_string(),
            "toolchain".to_string(),
            "install".to_string(),
            "--system".to_string(),
            "--force".to_string(),
        ];
        let parsed = ParsedArgs::parse(&args).unwrap();
        assert_eq!(parsed.command, "toolchain");
        assert_eq!(parsed.input_file.as_deref(), Some("install"));
        assert!(parsed.program_args.iter().any(|a| a == "--system"));
        assert!(parsed.program_args.iter().any(|a| a == "--force"));
    }

    #[test]
    fn test_toolchain_check_system_flag_reaches_command() {
        let args = vec![
            "adesh".to_string(),
            "toolchain".to_string(),
            "check".to_string(),
            "--system".to_string(),
        ];
        let parsed = ParsedArgs::parse(&args).unwrap();
        assert_eq!(parsed.input_file.as_deref(), Some("check"));
        // The flag flows through to the dispatched command, never dropped.
        assert!(parsed.program_args.iter().any(|a| a == "--system"));
    }

    #[test]
    fn test_parse_editor_command() {
        // Plain editor launch
        let args = vec!["adesh".to_string(), "editor".to_string()];
        let parsed = ParsedArgs::parse(&args).unwrap();
        assert_eq!(parsed.command, "editor");
        assert_eq!(parsed.input_file, None);

        // Editor with file
        let args_file = vec![
            "adesh".to_string(),
            "editor".to_string(),
            "main.adesh".to_string(),
        ];
        let parsed_file = ParsedArgs::parse(&args_file).unwrap();
        assert_eq!(parsed_file.command, "editor");
        assert_eq!(parsed_file.input_file, Some("main.adesh".to_string()));

        // Editor with directory path
        let args_dir = vec![
            "adesh".to_string(),
            "editor".to_string(),
            "./src/my_project".to_string(),
        ];
        let parsed_dir = ParsedArgs::parse(&args_dir).unwrap();
        assert_eq!(parsed_dir.command, "editor");
        assert_eq!(parsed_dir.input_file, Some("./src/my_project".to_string()));
    }
}
