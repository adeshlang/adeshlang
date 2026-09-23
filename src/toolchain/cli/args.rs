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
            });
        }

        let mut config = RuntimeConfig::new();
        let mut command = String::new();
        let mut input_file: Option<String> = None;
        let mut output_file: Option<String> = None;
        let mut eval_code: Option<String> = None;
        let mut program_args: Vec<String> = Vec::new();
        let mut show_help = false;
        let mut show_version = false;
        let mut toolchain_preference = None;
        let mut env_json = false;
        let mut env_shell = false;
        let mut collecting_program_args = false;
        let mut positional_args: Vec<String> = Vec::new();

        let mut i = 1; // Skip program name
        while i < args.len() {
            let arg = &args[i];

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
                    config.backend = ExecutionBackend::Bytecode;
                }
                "--adaptive" | "--adaptive-jit" | "--ajit" => {
                    config.backend = ExecutionBackend::AdaptiveJit;
                }
                "--tiered" | "--tiered-jit" | "--tjit" => {
                    config.backend = ExecutionBackend::TieredJit;
                }
                "--aot" | "--aot-native" => {
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
                "--backend-check" => {
                    config.backend_check = true;
                    config.run_tests = true;
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
                "--no-check-moves" | "--no-moves" => {
                    config.check_moves = false;
                }
                "--import" => {
                    if i + 1 < args.len() && !args[i + 1].starts_with("-") {
                        config.ffi_imports.push(args[i + 1].clone());
                        i += 1;
                    } else {
                        return Err("--import requires a header path".to_string());
                    }
                }
                "--wasm" => {
                    if command == "run" && (i + 1 >= args.len() || args[i + 1].starts_with("-")) {
                        config.backend = ExecutionBackend::Wasm;
                    } else if i + 1 < args.len() && !args[i + 1].starts_with("-") {
                        config.wasm_modules.push(args[i + 1].clone());
                        i += 1;
                    } else {
                        config.backend = ExecutionBackend::Wasm;
                    }
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
                    config.test_name = Some(positional_args[2].clone());
                    program_args.extend(positional_args[3..].iter().cloned());
                } else {
                    program_args.extend(positional_args[2..].iter().cloned());
                }
            }
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
        })
    }
}

/// Generate help message
pub fn help_message() -> String {
    // ANSI color codes
    let cyan = "\x1b[36m";
    let green = "\x1b[32m";
    let yellow = "\x1b[33m";
    let blue = "\x1b[34m";
    let bold = "\x1b[1m";
    let reset = "\x1b[0m";
    let dim = "\x1b[2m";
    let gpu_backend_line = if cfg!(debug_assertions) {
        format!(
            "    {blue}--gpu{reset}                 MLIR GPU backend (debug builds, compile-only)\n",
            blue = blue,
            reset = reset
        )
    } else {
        String::new()
    };
    let gpu_options_block = if cfg!(debug_assertions) {
        format!(
            r#"{bold}GPU OPTIONS (DEBUG ONLY):{reset}
    {blue}--gpu{reset}                  Enable GPU backend (MLIR → CUDA/ROCm/Vulkan/Metal)
    {blue}--gpu-target{reset} <auto|cuda|rocm|vulkan|metal>  Select GPU target (default: auto)
    {blue}--gpu-grid{reset} <x,y,z>        GPU grid dimensions  — blocks  (default: 1,1,1)
    {blue}--gpu-block{reset} <x,y,z>       GPU block dimensions — threads (default: 256,1,1)
    {blue}--gpu-shared-mem{reset} <bytes>  Shared memory per block (default: 0)
    {blue}--dump-mir{reset}             Show MIR (Memory IR with ownership/borrow annotations)

{bold}GPU PIPELINE (4 steps):{reset}
    [1] gpu-kernel-outlining + canonicalize   → .lowered.mlir
    [2] convert-func-to-llvm + reconcile      → .llvm_ready.mlir
    [3] mlir-translate --mlir-to-llvmir       → .ll
    [4] llc + clang                           → .exe / .out
    If steps 2-4 fail (missing dialect plugins), interpreter fallback runs the program.

{bold}GPU DEVICE CHECK:{reset}
    {green}gpu-check{reset}               Probe GPU devices and MLIR toolchain compatibility
    {green}gpu-check -v{reset}            Verbose: include optional/missing checks
    {green}gpu-check --json{reset}        Machine-readable JSON output for scripts/CI

    {dim}Environment overrides for MLIR tools:{reset}
      ADESH_MLIR_OPT, ADESH_MLIR_TRANSLATE, ADESH_LLC, ADESH_CLANG

"#,
            bold = bold,
            reset = reset,
            blue = blue,
            green = "\x1b[32m",
            dim = "\x1b[2m",
        )
    } else {
        String::new()
    };

    format!(
        r#"{bold}{cyan}AdeshLang{reset} {dim}v0.3.0{reset} - Rust-inspired, high-performance language
Built with strong type inference, memory safety, and zero-cost abstractions

{bold}USAGE:{reset}
    {green}adesh{reset} <COMMAND> [OPTIONS] <FILE> [-- <ARGS>...]
    {green}adesh{reset} run {blue}-e{reset} "<code>"
    {green}adesh{reset} editor [FILE|DIR]

{bold}COMMANDS:{reset}
    {green}run{reset} <file>            Run a program (default: interpreter)
    {green}run{reset} {blue}-e{reset} "<code>"     Directly evaluate and run inline code string {yellow}⭐ NEW!{reset}
    {green}editor{reset} [file|dir]     Launch cross-platform TUI code editor (Adesh Editor) {yellow}⭐ NEW!{reset}
    {green}build{reset} <file>          Build native executable (modern AOT interface) {yellow}⭐ NEW!{reset}
    {green}doctor{reset}                 Inspect AdeshLang toolchain & system health {yellow}⭐ NEW!{reset}
    {green}env{reset}                    Display AdeshLang environment paths & toolchain details {yellow}⭐ NEW!{reset}
    {green}repair{reset}                 Repair AdeshLang toolchain & environment settings {yellow}⭐ NEW!{reset}
    {green}pkg{reset} <subcommand>       Manage AdeshLang packages (init, install, update, list) {yellow}⭐ NEW!{reset}
    {green}ai{reset} <subcommand>        AI assistant (explain, fix, generate, chat) {yellow}⭐ NEW!{reset}
    {green}repl{reset}                  Start interactive REPL
    {green}format{reset} <file>         Format source code (use {blue}--write{reset} to modify in place)
    {green}fmt{reset} <file>            Alias for format command
    {green}compile{reset} <in> <out>    Compile to bytecode
    {green}compile-wasm{reset} <in> <out> Compile to WebAssembly (.wasm + .js loader) {yellow}⭐ NEW!{reset}
    {green}compile-native{reset} <in> <out> Compile to native object/executable
    {green}compile-aot{reset} <in> <out> [options]  Compile to native executable (power-user)
    {green}disassemble{reset} <file>    Disassemble a bytecode file (use {blue}--write{reset} to save to file)
    {green}docs{reset} <in> <out>       Generate documentation
    {green}init{reset} <dir>            Initialize a new project
    {green}check{reset} <file>          Type-check program without execution
    {green}target{reset} [list|info]    Show cross-compilation targets
    {green}gpu-check{reset} [-v] [--json] Check GPU device compatibility & toolchain
    {green}clean{reset}                 Remove build artifacts

{bold}ADESH EDITOR (TUI IDE):{reset} {yellow}⭐ NEW!{reset}
    {green}adesh editor{reset}                 Launch TUI editor with an empty workspace
    {green}adesh editor{reset} <file>          Open file directly in editor (e.g. {dim}adesh editor main.adesh{reset})
    {green}adesh editor{reset} <dir>           Open directory workspace with file explorer (e.g. {dim}adesh editor ./src{reset})
    
    {dim}KEY FEATURES & WORKFLOWS:{reset}
      • {bold}Modal Editing:{reset} Normal ({dim}Esc{reset}), Insert ({dim}i, a, o, O{reset}), Visual ({dim}v{reset}), Command ({dim}:{reset})
      • {bold}Run Code Instantly:{reset} {dim}F5{reset} or click ▶ Run button to execute active buffer across all backends
      • {bold}Backend Switcher:{reset} {dim}Ctrl+F5{reset} / {dim}:backend <name>{reset} to switch between all 11 execution backends
      • {bold}Stop Program:{reset} {dim}F6{reset} to terminate running background execution
      • {bold}File Explorer:{reset} {dim}Ctrl+E{reset} / {dim}F3{reset} Toggle sidebar tree (copy, cut, paste, delete, auto-scroll)
      • {bold}Buffer Management:{reset} {dim}Tab{reset} / {dim}Shift+Tab{reset} Cycle open file tabs | Mouse clickable tabs & close buttons
      • {bold}Search & Replace:{reset} {dim}Ctrl+F{reset} Find | {dim}Ctrl+H{reset} Find & Replace | {dim}Ctrl+Shift+F{reset} Multi-buffer Grep
      • {bold}Integrated Terminal:{reset} {dim}Ctrl+J{reset} Embedded interactive terminal with full shell execution
      • {bold}Output Panel:{reset} {dim}Ctrl+`{reset} Toggle output panel (stdout/stderr, exit code, execution time)
      • {bold}Command Palette:{reset} {dim}Ctrl+P{reset} Quick command search | {dim}Ctrl+G{reset} / {dim}:<N>{reset} Go to line
      • {bold}File Operations:{reset} {dim}Ctrl+S{reset} Save | {dim}Ctrl+O{reset} Open | {dim}Ctrl+Q{reset} / {dim}:q{reset} Quit
      • {bold}Code Formatting:{reset} Auto-formatting via {dim}:format{reset} or Edit menu
      • {bold}Themes & Aesthetics:{reset} Adesh Dark, Adesh Light, Gruvbox, Monokai, Dracula, One Dark ({dim}:theme <name>{reset})
      • {bold}In-Editor Help:{reset} Press {dim}F1{reset} or type {dim}:help{reset} anytime for complete shortcut cheatsheet

{bold}TESTING:{reset} {yellow}⭐ NEW!{reset}
    {green}run{reset} {blue}--test{reset} <file> [test_name]  Run tests in file (optionally one test)
    {blue}--test-name{reset} <name>    Run a single test (or group prefix)
    {blue}--fail-fast{reset}            Stop on first test failure
    {blue}--quiet{reset} {blue}-q{reset}             Hide output from passing tests (still show failures)
    {blue}--nocapture{reset}           Show test output for passing tests (Rust-style)
    {blue}--backend-check{reset}        Test across multiple backends and show comparison matrix
    {blue}--format json{reset}          Output test results in JSON format for CI integration
    {blue}--tags{reset} <tag>           Filter tests by tag
    {dim}Test groups use names like group__test (filter also accepts group::test){reset}
    
    {dim}Examples:{reset}
      {green}adesh run --test{reset} tests/math_test.adesh
      {green}adesh run --test{reset} tests/math_test.adesh test_add
      {green}adesh run --test --test-name{reset} test_add tests/math_test.adesh
      {green}adesh run --test --fail-fast{reset} all_tests.adesh
      {green}adesh run --test --quiet{reset} all_tests.adesh
      {green}adesh run --test --nocapture{reset} all_tests.adesh
      {green}adesh run --test --backend-check{reset} integration_tests.adesh
      {green}adesh run --test --format json{reset} tests.adesh {blue}>{reset} results.json
      {green}test{reset} math {{ {green}test fn add(){{}}{reset} {green}test fn sub(){{}}{reset} }}

{bold}TYPE SYSTEM:{reset}
    AdeshLang features a strong, Rust-inspired type system with:
    • Explicit type annotations: {dim}let x: i32 = 42{reset}
    • Type inference: {dim}let y = 42{reset} (inferred as i32)
    • Generics: {dim}fn max<T: Ord>(a: T, b: T) -> T{reset}
    • Traits: {dim}trait Iterator {{ fn next(&mut self) -> Option<T> }}{reset}
    • Pattern matching with exhaustiveness checking
    • Algebraic data types: {dim}enum Result<T, E> {{ Ok(T), Err(E) }}{reset}
    • Lifetimes: {dim}&'a T{reset} (manage borrowing)
    • Fixed-width types: u8, u16, u32, u64, u128, i8-i128, f32, f64
    • Nullable types: Option<T> (no null pointer exceptions)
    • Error handling: Result<T, E> (no exceptions)
    • Ownership and borrowing (safe memory management)

{bold}EXECUTION BACKENDS:{reset}
    {blue}--interpreter{reset}         Use standard interpreter (default, most compatible)
    {blue}--jit{reset}                 Enable JIT compilation (faster execution, LIR interpreter)
    {blue}--jit-native{reset}          Native JIT: Compile to machine code at runtime {yellow}(10-232x faster!){reset} {yellow}⚡⚡⚡{reset}
    {blue}--native-jit{reset}          Alias for {blue}--jit-native{reset}
    {blue}--njit{reset}                Short alias for {blue}--jit-native{reset}
    {blue}--mixed{reset}               Hybrid mode: interpret first, JIT hot functions
    {blue}--safe{reset}                Safe mode: no JIT, full GC safety
    {blue}--bytecode{reset}, {blue}--vm{reset}       Run using bytecode VM
    {blue}--adaptive{reset}, {blue}--ajit{reset}     Adaptive JIT with speculative optimization
    {blue}--tiered{reset}, {blue}--tjit{reset}       Tiered JIT compilation (T0->T1->T2)
    {blue}--wasm{reset}                 WebAssembly VM execution backend (Wasmtime runtime) {yellow}⭐ NEW!{reset}
    {gpu_backend_line}
    
    {dim}PERFORMANCE COMPARISON (on compute-heavy workloads):{reset}
      Interpreter:      1.0x (baseline)
      JIT ({blue}--jit{reset}):      0.8x (slower due to interpretation overhead)
      {yellow}Native JIT:{reset}       {yellow}10-232x faster!{reset} (true native code execution) {yellow}⚡⚡⚡{reset}
      AOT Compile:      Similar to Native JIT (ahead-of-time compilation)
    
    {dim}NATIVE JIT FEATURES:{reset}
      ✅ Compiles LIR to native machine code at runtime using Cranelift
      ✅ Executes via function pointers (zero interpretation overhead)
      ✅ 71% instruction coverage (46/65 LIR instructions)
      ✅ Full loop support (for, while, do-while)
      ✅ Full string support (print with strings works)
      ✅ Full memory management (malloc, free, pointers)
      ✅ All arithmetic, bitwise, and comparison operations
      ✅ Ownership and borrowing support (Arc operations)
      ✅ Production-ready for most workloads
    
    {dim}WHEN TO USE NATIVE JIT:{reset}
      • Loop-heavy algorithms (100% working)
      • Array processing (100% working)
      • Math-intensive computations (100% working)
      • String operations (100% working)
      • Memory-intensive workloads (full dynamic allocation)
      • Performance-critical code paths
      • Real-time systems (deterministic execution)

{gpu_options_block}
{bold}OPTIMIZATION:{reset}
    {blue}--opt O0{reset}              No optimization (fastest compilation)
    {blue}--opt O1{reset}              Basic optimization (default)
    {blue}--opt O2{reset}              Standard optimization
    {blue}--opt O3{reset}              Maximum optimization (slowest compilation)
    {blue}-O0{reset}, {blue}-O1{reset}, {blue}-O2{reset}, {blue}-O3{reset}  Shorthand optimization levels
    
    {dim}AOT COMPILATION OPTIMIZATION OPTIONS:{reset}
    {blue}--fast{reset}                Enable fast-compile mode (skip optimizations, 0.3-0.5s builds) {yellow}⚡{reset}
    {blue}--enable-lto{reset}         Enable Link-Time Optimization (LTO) for production builds {yellow}20-30% smaller{reset}
    {blue}--enable-dce{reset}         Enable Dead-Code Elimination in linker (10-15% smaller)
    {blue}--disable-lto{reset}        Explicitly disable LTO (for debugging slow optimizations)

{bold}MEMORY (OWNERSHIP-BASED, NO GC):{reset}
    {blue}--embedded{reset}            Embedded mode: heap + ARC disabled, stack + arena only
    {blue}--stack-size{reset} <size>   Set initial stack size (e.g. "64M", "128MB", default: 32MB, auto-scales to 1GB)
    {blue}--heap-size{reset} <size>    Set heap size limit (e.g. "1G", "512MB", default: unlimited)
    {blue}--check-ownership{reset}     Enable compile-time ownership and borrow checking (default: on)
    {blue}--check-moves{reset}         Enable move semantics validation (default: on)
    
    {dim}Note: AdeshLang uses deterministic ownership-based memory (like Rust).
    There is NO garbage collector. Memory is managed via:{reset}
      - Ownership (move semantics, deterministic drop)
      - Borrowing (&T, &mut T)
      - Explicit ARC (via 'share' keyword)
      - Regions (arena allocation)

{bold}IR DEBUGGING:{reset}
    {blue}--dump-ast{reset}            Dump AST representation
    {blue}--dump-hir{reset}            Dump HIR (high-level IR)
    {blue}--dump-mir{reset}            Dump MIR (Memory IR — ownership/borrow/lifetime annotated)
    {blue}--dump-vir{reset}            Dump VIR (Value Intermediate Representation, SSA-based)
    {blue}--dump-mlir{reset}           Dump MLIR (Multi-Level Intermediate Representation)
    {blue}--mlir-gpu{reset}            Emit GPU kernel stubs in MLIR output
    {blue}--dump-lir{reset}            Dump LIR (low-level SSA IR)
    {blue}--dump-bytecode{reset}       Dump bytecode instructions
    {blue}--dump-all{reset}            Dump all intermediate representations
    {blue}--dump-ast-write{reset} [file]     Write AST dump to file (auto-generate if no path)
    {blue}--dump-hir-write{reset} [file]     Write HIR dump to file (auto-generate if no path)
    {blue}--dump-vir-write{reset} [file]     Write VIR dump to file (auto-generate if no path)
    {blue}--dump-mlir-write{reset} [file]    Write MLIR dump to file (auto-generate if no path)
    {blue}--dump-ir-write{reset} [file]      Alias for {blue}--dump-hir-write{reset}
    {blue}--dump-lir-write{reset} [file]     Write LIR dump to file (auto-generate if no path)
    {blue}--dump-cfg{reset}            Dump Control Flow Graph
    {blue}--dump-cfg-write{reset} [file]     Write CFG dump to file (auto-generate if no path)
    
    {dim}VIR OUTPUT FORMAT:
    VIR is an SSA-based representation suitable for all backends:{reset}
      - Function definitions with SSA parameters and return types
      - Basic blocks with phi nodes for SSA merging
      - Explicit memory operations (alloc, load, store)
      - Explicit ARC operations (clone, drop)
      - Backend-neutral: consumed by JIT, AOT, Bytecode, Interpreter
    
    {dim}MLIR OUTPUT FORMAT:
    MLIR represents operations using LLVM dialect:{reset}
      - Standard LLVM arithmetic operations (@llvm.*)
      - Function definitions with MLIR syntax
      - Type information in i64, f64 format
      - Compatible with LLVM optimization pipeline

{bold}FORMATTER OPTIONS:{reset}
    {blue}--write{reset}, {blue}-w{reset}           Write formatted output to file (instead of stdout)
    {blue}--indent{reset} <n>          Set indentation size (default: 4)
    {blue}--tabs{reset}                Use tabs instead of spaces
    {blue}--check{reset}               Check if file is formatted (exit 1 if not)

{bold}DISASSEMBLER OPTIONS:{reset}
    {blue}--write{reset}, {blue}-w{reset} [file]    Write disassembly to file (auto-generate name if not specified)

{bold}BUILD COMMAND OPTIONS{reset} ({green}build{reset} command) {yellow}⭐ RECOMMENDED FOR NEW PROJECTS{reset}:
    The modern {blue}build{reset} command is the recommended way to compile AdeshLang to native executables.
    It provides sensible defaults, automatic output inference, and lightning-fast incremental compilation.
    
    {blue}-o{reset} <path>             Output executable path (default: auto-generate from source)
    {blue}--fast{reset}                Lightning-fast compilation mode (no optimizations, ~0.3-0.5s cold, <5ms cached) {yellow}⚡{reset}
                                    Perfect for development iteration with incremental linking.
    {blue}--incremental{reset}         Enable incremental compilation caching (default: on) {yellow}⚡{reset}
    {blue}--no-cache{reset}, {blue}--no-incremental{reset} Disable caching and compile from scratch
    {blue}-f{reset}, {blue}--force{reset}           Force full rebuild, bypassing cache
    {blue}--clear-cache{reset}         Clear compiler artifact cache before building
    {blue}-O0, -O1, -O2, -O3{reset}   Optimization level (default: -O1 for balance)
                                    -O0 = fastest compile, -O3 = slowest compile, best runtime
    {blue}--opt{reset} [0-3]           Alternate syntax for optimization levels
    {blue}--release{reset}             Alias for -O3 (maximum optimization)
    {blue}--enable-lto{reset}          Enable Link-Time Optimization {dim}(adds 2-5s to compile time, 20-30% smaller binaries){reset}
    {blue}--enable-dce{reset}          Enable Dead-Code Elimination (DCE) in linker {dim}(very fast, 10-15% smaller){reset}
    {blue}--debug{reset}               Include debug symbols (larger binary, enables debugging)
    {blue}--run{reset}                 Execute compiled binary immediately after build
    {blue}--dry-run{reset}             Show what would be compiled (don't actually compile)
    {blue}-I{reset}<dir>               Add include directory (-I/path/to/headers)
    {blue}-L{reset}<dir>               Add library search directory (-L/usr/local/lib)
    {blue}-l{reset}<lib>               Link with library (-lpthread, -lm, etc.)
    {blue}--target{reset}=<triple>     Cross-compile for target (e.g., aarch64-unknown-linux-gnu)
    {blue}--library-mode{reset}        Compile as library (.a/.so) instead of executable
    {blue}--emit{reset} <type>         Emit specific output: exe (default), obj, lib, dylib, asm
    
    {dim}OPTIMIZATION & SPEED GUIDE:{reset}
    
    {dim}For instant development builds (cold ~0.3s, cached <5ms):{reset}
      {green}adesh build{reset} {blue}--fast{reset} program.adesh {blue}-o{reset} app.exe
      {green}adesh build run{reset} {blue}--fast{reset} program.adesh
    
    {dim}For incremental cached development:{reset}
      {green}adesh build{reset} program.adesh                              {dim}# 0ms on unchanged files!{reset}
      {green}adesh build{reset} {blue}--force{reset} program.adesh                      {dim}# Force recompile{reset}
      {green}adesh build{reset} {blue}--clear-cache{reset} program.adesh                {dim}# Clean cache then recompile{reset}
    
    {dim}For production release:{reset}
      {green}adesh build{reset} program.adesh {blue}-O3 --enable-lto{reset} {blue}-o{reset} app.exe  {dim}# best performance, 20-30% smaller{reset}
      {green}adesh build{reset} program.adesh {blue}-O2 --enable-dce{reset} {blue}-o{reset} app.exe  {dim}# balanced size and speed{reset}
    
    {dim}ENVIRONMENT VARIABLES:{reset}
    
      ADESH_RUN_TIMEOUT_MS=15000   Process watchdog timeout in ms for `run` (e.g., 15000)
      ADESH_FAST_COMPILE=1         Default to {blue}--fast{reset} mode {dim}(override with -O0, -O1, -O2, -O3){reset}
      ADESH_ENABLE_LTO=1           Automatically enable LTO for all builds
      ADESH_ENABLE_DCE=1           Automatically enable dead-code elimination
      ADESH_VERBOSE=1              Show compilation details and linker commands

{bold}AOT COMPILATION OPTIONS{reset} ({green}compile-aot{reset} command) {dim}[Legacy power-user command]{reset}:
    {blue}--opt O0-O3{reset}           Optimization level (default: O2)
    {blue}--fast{reset}                Lightning-fast compilation mode {yellow}⚡{reset}
    {blue}--incremental{reset}         Enable incremental compilation caching
    {blue}--no-cache{reset}            Disable compilation cache
    {blue}-f{reset}, {blue}--force{reset}           Force rebuild, ignoring cache
    {blue}--clear-cache{reset}         Clear compiler artifact cache
    {blue}--debug{reset}               Include debug information in the binary
    {blue}--shared{reset}, {blue}--dll{reset}, {blue}--so{reset}  Generate shared/dynamic library (.dll/.so/.dylib)
    {blue}--static{reset}, {blue}--lib{reset}, {blue}--a{reset}   Generate static library (.lib/.a)
    {blue}--object{reset}, {blue}--obj{reset}, {blue}--o{reset}   Generate object file only (.obj/.o)
    {blue}-c{reset}, {blue}--compile-only{reset}    Compile to object file only (don't link)
    {blue}-S{reset}, {blue}--emit-asm{reset}        Emit assembly/IR (not yet fully implemented)
    {blue}--emit-header{reset} [file]  Generate C header file for exported functions
    {blue}--ffi{reset}                 Enable FFI mode (C ABI compatibility)
    {blue}-I{reset}<dir>               Add include directory
    {blue}-L{reset}<dir>               Add library search directory
    {blue}-l{reset}<lib>               Link with library
    {blue}--target{reset}=<triple>     Cross-compile for target platform

    {dim}CROSS-COMPILATION TARGETS:{reset}
    Platform targets use the format: <arch>-<vendor>-<os>-<env>
    
    {dim}Common targets:{reset}
      x86_64-unknown-linux-gnu     Linux x86_64 (glibc)
      x86_64-unknown-linux-musl    Linux x86_64 (musl, static)
      aarch64-unknown-linux-gnu    Linux ARM64
      arm-unknown-linux-gnueabihf  Linux ARM32 (hard float)
      x86_64-pc-windows-gnu        Windows x86_64 (MinGW)
      x86_64-pc-windows-msvc       Windows x86_64 (MSVC)
      x86_64-apple-darwin          macOS x86_64
      aarch64-apple-darwin         macOS ARM64 (Apple Silicon)
      aarch64-linux-android        Android ARM64
      armv7-linux-androideabi      Android ARM32
      aarch64-apple-ios            iOS ARM64
      
    {dim}Note: Cross-compilation requires appropriate toolchain installed:{reset}
      - Linux targets: Install cross-gcc (e.g., x86_64-linux-gnu-gcc)
      - Windows from Linux: Install mingw-w64
      - macOS: Requires osxcross or macOS host
      - Android: Install Android NDK
      - iOS: Requires Xcode on macOS host

    Default: Native host platform

{bold}IO OPTIONS:{reset}
    {blue}--buffered-stdout{reset}     Use buffered stdout for performance
    {blue}--interactive-input{reset}   Enable interactive input mode
    {blue}--no-color{reset}            Disable ANSI color codes
    {blue}--quiet{reset}, {blue}-q{reset}           Suppress progress and diagnostic output

{bold}TYPE CHECKING & ANALYSIS:{reset}
    {blue}--type-check{reset}          Perform strict type checking without execution
    {blue}--strict-types{reset}        Disallow implicit type conversions
    {blue}--show-inferred{reset}       Display inferred types during compilation

{bold}RECURSION OPTIMIZATION:{reset}
    {blue}--tco{reset}, {blue}--tail-call{reset}    Enable tail-call optimization
    {blue}--memo{reset}, {blue}--memoize{reset}     Enable memoization for pure functions
    {blue}--fast-recursion{reset}      Full recursion optimizations (TCO + memoization)
    {blue}--no-recursion-opt{reset}    Disable all recursion optimizations

{bold}OTHER:{reset}
    {blue}--profile{reset}             Enable profiling/timing output
    {blue}--timeout{reset} <dur>       Kill the process if `run` has not finished (e.g. 15s, 15000ms)
    {blue}--memory{reset}, {blue}--mem{reset}       Show detailed memory usage statistics after execution
    {blue}--verbose{reset}             Enable verbose output
    {blue}--debug{reset}               Enable debug output
    {blue}--help{reset}, {blue}-h{reset}           Show this help message
    {blue}--version{reset}, {blue}-v{reset}        Show version information

{bold}EXAMPLES:{reset}
    {dim}# TUI Editor (IDE){reset}
    {green}adesh editor{reset}                          {dim}# Launch interactive TUI code editor{reset}
    {green}adesh editor{reset} main.adesh                   {dim}# Open file in editor{reset}
    {green}adesh editor{reset} ./my_project                 {dim}# Open directory workspace in editor{reset}
    
    {dim}# Basic execution{reset}
    {green}adesh run{reset} program.adesh                    {dim}# Run with interpreter{reset}
    {green}adesh run{reset} {blue}--jit{reset} program.adesh              {dim}# Run with JIT{reset}
    {green}adesh run{reset} {blue}--jit-native{reset} program.adesh       {dim}# Run with Native JIT (fastest!){reset}
    {green}adesh run{reset} {blue}--jit --opt O3{reset} program.adesh     {dim}# Run with JIT + max optimization{reset}
    {green}adesh run{reset} {blue}--safe{reset} program.adesh             {dim}# Run in safe mode{reset}
    {green}adesh run{reset} {blue}--embedded{reset} program.adesh         {dim}# Embedded mode (stack + arena only){reset}
    
    {dim}# Testing{reset}
    {green}adesh run{reset} {blue}--test{reset} test_suite.adesh         {dim}# Run all tests{reset}
    {green}adesh run{reset} {blue}--test --fail-fast{reset} tests.adesh  {dim}# Stop on first failure{reset}
    {green}adesh run{reset} {blue}--test --backend-check{reset} tests.adesh  {dim}# Test all backends{reset}
    
    {dim}# Type checking{reset}
    {green}adesh check{reset} program.adesh                  {dim}# Type-check only{reset}
    {green}adesh run{reset} {blue}--type-check{reset} program.adesh       {dim}# Type-check and run{reset}
    {green}adesh run{reset} {blue}--show-inferred{reset} program.adesh    {dim}# Show inferred types{reset}
    
    {dim}# IR debugging{reset}
    {green}adesh run{reset} {blue}--dump-hir{reset} program.adesh         {dim}# Dump HIR to stderr and run{reset}
    {green}adesh run{reset} {blue}--dump-vir{reset} program.adesh         {dim}# Dump VIR to stderr and run{reset}
    {green}adesh run{reset} {blue}--dump-mlir{reset} program.adesh        {dim}# Dump MLIR to stderr and run{reset}
    {green}adesh run{reset} {blue}--dump-hir-write{reset} program.adesh   {dim}# Write HIR to output.hir{reset}
    {green}adesh run{reset} {blue}--dump-vir-write{reset} program.adesh   {dim}# Write VIR to output.vir{reset}
    {green}adesh run{reset} {blue}--dump-mlir-write{reset} program.adesh  {dim}# Write MLIR to output.mlir{reset}
    {green}adesh run{reset} {blue}--dump-hir-write{reset} ir.hir program.adesh  {dim}# Write HIR to ir.hir{reset}
    {green}adesh run{reset} {blue}--dump-vir-write{reset} ir.vir program.adesh  {dim}# Write VIR to ir.vir{reset}
    {green}adesh run{reset} {blue}--dump-lir-write{reset} program.adesh   {dim}# Write LIR to output.lir{reset}
    {green}adesh run{reset} {blue}--dump-ast-write{reset} program.adesh   {dim}# Write AST to output.ast{reset}
    {green}adesh run{reset} {blue}--dump-cfg-write{reset} program.adesh   {dim}# Write CFG to output.cfg{reset}
    
    {dim}# Profiling and analysis{reset}
    {green}adesh run{reset} program.adesh {blue}--profile{reset}          {dim}# Run with timing info{reset}
    {green}adesh run{reset} program.adesh {blue}--timeout 20s{reset}     {dim}# Abort hung OS threads / deadlocks
    {green}adesh run{reset} {blue}--jit --memory{reset} program.adesh     {dim}# JIT with memory stats{reset}
    {green}adesh run{reset} program.adesh {blue}--{reset} arg1 arg2       {dim}# Pass args to program{reset}
    
    {dim}# Formatting{reset}
    {green}adesh format{reset} program.adesh                 {dim}# Print formatted code to stdout{reset}
    {green}adesh fmt{reset} program.adesh                    {dim}# Same as format (alias){reset}
    {green}adesh format{reset} {blue}--write{reset} program.adesh         {dim}# Format file in place{reset}
    {green}adesh format{reset} {blue}--check{reset} program.adesh         {dim}# Check if file is formatted{reset}
    
    {dim}# Bytecode compilation{reset}
    {green}adesh disassemble{reset} out.adeshbc              {dim}# Print disassembly to stdout{reset}
    {green}adesh disassemble{reset} out.adeshbc {blue}--write{reset}      {dim}# Save as out.adeshbc.bc.adesh{reset}
    {green}adesh disassemble{reset} out.adeshbc {blue}--write{reset} dis.adesh  {dim}# Save as dis.adesh{reset}
    
    {dim}# WebAssembly compilation{reset}
    {green}adesh compile-wasm{reset} program.adesh app.wasm   {dim}# Compile to WebAssembly (.wasm + .js loader){reset}
    {green}adesh run{reset} {blue}--wasm{reset} program.adesh              {dim}# Execute program using WASM runtime{reset}
    
    {dim}# Modern AOT compilation (recommended){reset}
    {green}adesh build{reset} program.adesh                    {dim}# Build with default optimization (-O1){reset}
    {green}adesh build{reset} {blue}--fast{reset} program.adesh                {dim}# Lightning-fast dev build (~0.3-0.5s){reset}
    {green}adesh build{reset} {blue}-O3 --enable-lto{reset} program.adesh     {dim}# Production build with LTO + max opt (~5-7s){reset}
    {green}adesh build{reset} {blue}-O2 --enable-dce{reset} program.adesh     {dim}# Production with DCE, faster compile (~3-4s){reset}
    {green}adesh build{reset} program.adesh {blue}-o{reset} myapp {blue}--run{reset}    {dim}# Build and run immediately{reset}
    {green}adesh build{reset} program.adesh {blue}--debug{reset} {blue}-o{reset} app.exe  {dim}# Debug build with symbols{reset}
    {green}adesh build{reset} program.adesh {blue}--library-mode{reset} {blue}-o{reset} lib.a  {dim}# Build static library{reset}
    
    {dim}# Legacy power-user AOT compilation{reset}
    {green}adesh compile-aot{reset} program.adesh app.exe    {dim}# Compile to native executable{reset}
    {green}adesh compile-aot{reset} program.adesh app        {dim}# Compile (auto-add extension){reset}
    {green}adesh compile-aot{reset} program.adesh lib.dll {blue}--shared{reset}  {dim}# Compile to shared library{reset}
    {green}adesh compile-aot{reset} program.adesh lib.so {blue}--so{reset}       {dim}# Compile to .so (Linux){reset}
    {green}adesh compile-aot{reset} program.adesh lib.a {blue}--static{reset}    {dim}# Compile to static library{reset}
    {green}adesh compile-aot{reset} program.adesh app.o {blue}--object{reset}    {dim}# Compile to object file only{reset}
    {green}adesh compile-aot{reset} program.adesh app.exe {blue}-c{reset}        {dim}# Compile to object file only (-c){reset}
    {green}adesh compile-aot{reset} program.adesh app.exe {blue}-O3{reset}       {dim}# AOT with max optimization{reset}
    {green}adesh compile-aot{reset} program.adesh app.exe {blue}--debug{reset}   {dim}# AOT with debug info{reset}
    {green}adesh compile-aot{reset} program.adesh lib.dll {blue}--shared --emit-header{reset} lib.h  {dim}# Generate header{reset}
    {green}adesh compile-aot{reset} program.adesh lib.dll {blue}--shared -fPIC{reset}  {dim}# Shared lib with PIC{reset}
    
    {dim}# Cross-compilation (requires toolchain for linking){reset}
    {green}adesh compile-aot{reset} {blue}--target{reset}=x86_64-unknown-linux-gnu program.adesh app
    {green}adesh compile-aot{reset} {blue}--target{reset}=aarch64-unknown-linux-gnu program.adesh app
    {green}adesh compile-aot{reset} {blue}--target{reset}=x86_64-pc-windows-gnu program.adesh app.exe
    {green}adesh compile-aot{reset} {blue}--target{reset}=aarch64-apple-darwin program.adesh app {blue}--object{reset}
    
    {dim}# Recursion optimization{reset}
    {green}adesh run{reset} {blue}--tco{reset} program.adesh              {dim}# Tail-call optimization{reset}
    {green}adesh run{reset} {blue}--memo{reset} program.adesh             {dim}# Memoization{reset}
    {green}adesh run{reset} {blue}--fast-recursion{reset} program.adesh   {dim}# Full optimization{reset}



{bold}FFI (FOREIGN FUNCTION INTERFACE):{reset}
    AdeshLang supports seamless C/C++ interoperability through FFI. Export functions 
    that can be called from C, with automatic header generation and ABI compatibility.
    
    {dim}SYNTAX:{reset}
        export fn function_name(param1: type1, param2: type2): return_type {{
            // implementation
            return value;
        }}
    
    {dim}TYPE MAPPINGS:{reset}
        AdeshLang Type    C Type
        i8               int8_t
        i16              int16_t
        i32              int32_t
        i64              int64_t
        u8               uint8_t
        u16              uint16_t
        u32              uint32_t
        u64              uint64_t
        f32              float
        f64              double
        bool             bool
        ptr              void*
    
    {dim}COMPILATION FOR FFI:{reset}
        {green}adesh compile-aot{reset} library.adesh library.o {blue}-c --emit-header{reset} library.h
        
        This generates:
        - library.o: Object file (machine code)
        - library.h: C header with function declarations
    
    {dim}LINKING WITH C:{reset}
        gcc program.c library.o -o program.exe
    
    {dim}FFI EXAMPLES:{reset}
        {dim}# Math library{reset}
        {green}adesh compile-aot{reset} math.adesh math.o {blue}-c --emit-header{reset} math.h
        gcc test.c math.o -o test.exe
        
        {dim}# Create C header automatically{reset}
        {green}adesh compile-aot{reset} lib.adesh lib.o {blue}--emit-header{reset} lib.h
        
        {dim}# Multiple libraries{reset}
        {green}adesh compile-aot{reset} math.adesh math.o {blue}-c --emit-header{reset} math.h
        {green}adesh compile-aot{reset} string.adesh string.o {blue}-c --emit-header{reset} string.h
        gcc app.c math.o string.o -o app.exe


{dim}For more information, visit: https://github.com/ajaytainwala-dev/mylang{reset}
"#,
        cyan = cyan,
        green = green,
        yellow = yellow,
        blue = blue,
        bold = bold,
        reset = reset,
        dim = dim,
        gpu_backend_line = gpu_backend_line,
        gpu_options_block = gpu_options_block
    )
}

/// Generate version message
pub fn version_message() -> String {
    "AdeshLang v0.3.0 - Rust-inspired, high-performance language\nBuilt with strong type inference, memory safety, zero-cost abstractions, and lightning-fast incremental AOT compilation".to_string()
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
