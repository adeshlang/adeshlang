//! Build Command Implementation
//!
//! This module provides the unified `build` command for AOT compilation.
//! It routes to the same AOT pipeline as `compile-aot` but with a more
//! user-friendly interface and automatic output inference.

use std::path::PathBuf;

use crate::cli::ui::{BuildProgress, colors};

/// Output type for build command
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmitType {
    /// Native executable (default)
    #[default]
    Executable,
    /// Object file only
    Object,
    /// Static library
    StaticLib,
    /// Shared/dynamic library
    SharedLib,
    /// Assembly output
    Assembly,
}

impl EmitType {
    /// Parse from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "exe" | "executable" | "bin" => Some(EmitType::Executable),
            "obj" | "object" | "o" => Some(EmitType::Object),
            "lib" | "static" | "staticlib" | "a" => Some(EmitType::StaticLib),
            "dylib" | "shared" | "sharedlib" | "so" | "dll" => Some(EmitType::SharedLib),
            "asm" | "assembly" | "s" => Some(EmitType::Assembly),
            _ => None,
        }
    }

    /// Convert to AOT OutputFormat
    pub fn to_aot_format(&self) -> crate::backends::cranelift_aot::OutputFormat {
        use crate::backends::cranelift_aot::OutputFormat;
        match self {
            EmitType::Executable => OutputFormat::Executable,
            EmitType::Object => OutputFormat::Object,
            EmitType::StaticLib => OutputFormat::StaticLib,
            EmitType::SharedLib => OutputFormat::SharedLib,
            EmitType::Assembly => OutputFormat::Assembly,
        }
    }
}

/// Unified build configuration
#[derive(Debug, Clone)]
pub struct AotBuildConfig {
    /// Input source file
    pub input: PathBuf,
    /// Output path (optional, auto-inferred if not specified)
    pub output: Option<PathBuf>,
    /// Optimization level (0-3)
    pub opt_level: u8,
    /// Output format/type
    pub emit: EmitType,
    /// Target triple for cross-compilation
    pub target: Option<String>,
    /// Include debug information
    pub debug_info: bool,
    /// Fast compilation mode (skip optimizations for rapid iteration)
    pub fast_compile: bool,
    /// Generate C header file (path or auto-generate)
    pub emit_header: Option<PathBuf>,
    /// Include directories (-I)
    pub include_dirs: Vec<PathBuf>,
    /// Library search paths (-L)
    pub lib_dirs: Vec<PathBuf>,
    /// Libraries to link (-l)
    pub link_libs: Vec<String>,
    /// Extra linker arguments
    pub linker_args: Vec<String>,
    /// Verbose output
    pub verbose: bool,
    /// Check only (don't emit output)
    pub check_only: bool,
    /// Dry run (show what would be done)
    pub dry_run: bool,
    /// Library mode (skip main function)
    pub library_mode: bool,
    /// Run after building
    pub run_after_build: bool,
    /// Arguments to pass to the program when running
    pub program_args: Vec<String>,
    /// Quiet mode (no progress spinner)
    pub quiet: bool,
    /// Compile a specific binary from src/bin/
    pub bin: Option<String>,
    /// Compile all binaries in src/bin/
    pub bins: bool,
    /// Incremental compilation caching (default: true)
    pub incremental: bool,
    /// Force rebuild, bypassing compilation cache
    pub force_rebuild: bool,
    /// Clear compilation cache before building
    pub clear_cache: bool,
}

impl Default for AotBuildConfig {
    fn default() -> Self {
        Self {
            input: PathBuf::new(),
            output: None,
            opt_level: 3,
            emit: EmitType::Executable,
            target: None,
            debug_info: false,
            fast_compile: false,
            emit_header: None,
            include_dirs: Vec::new(),
            lib_dirs: Vec::new(),
            link_libs: Vec::new(),
            linker_args: Vec::new(),
            verbose: false,
            check_only: false,
            dry_run: false,
            library_mode: false,
            run_after_build: false,
            program_args: Vec::new(),
            quiet: false,
            bin: None,
            bins: false,
            incremental: true,
            force_rebuild: false,
            clear_cache: false,
        }
    }
}

impl AotBuildConfig {
    /// Create a new build config for the given input file
    pub fn new(input: PathBuf) -> Self {
        Self {
            input,
            ..Default::default()
        }
    }

    /// Get the output path, inferring it from input if not specified
    pub fn get_output_path(&self) -> PathBuf {
        if let Some(ref output) = self.output {
            // If output is specified, use it (possibly with extension added)
            self.ensure_extension(output.clone())
        } else {
            // Infer from input file
            self.infer_output_path()
        }
    }

    /// Infer output path from input file
    fn infer_output_path(&self) -> PathBuf {
        let stem = self
            .input
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "output".to_string());

        let base_name = match self.emit {
            EmitType::StaticLib | EmitType::SharedLib => {
                // Add lib prefix on Unix-like systems
                if cfg!(unix) {
                    format!("lib{}", stem)
                } else {
                    stem
                }
            }
            _ => stem,
        };

        let path = self
            .input
            .parent()
            .map(|p| p.join(&base_name))
            .unwrap_or_else(|| PathBuf::from(&base_name));

        self.ensure_extension(path)
    }

    /// Ensure the output path has the correct extension
    fn ensure_extension(&self, mut path: PathBuf) -> PathBuf {
        let extension = self.get_platform_extension();

        // Only add extension if path doesn't already have one
        if path.extension().is_none() {
            if let Some(ext) = extension {
                path.set_extension(ext);
            }
        }

        path
    }

    /// Get the platform-appropriate extension for the output type
    fn get_platform_extension(&self) -> Option<&'static str> {
        match self.emit {
            EmitType::Executable => {
                if cfg!(windows) {
                    Some("exe")
                } else {
                    None // No extension on Unix
                }
            }
            EmitType::Object => {
                if cfg!(windows) {
                    Some("obj")
                } else {
                    Some("o")
                }
            }
            EmitType::StaticLib => {
                if cfg!(windows) {
                    Some("lib")
                } else {
                    Some("a")
                }
            }
            EmitType::SharedLib => {
                if cfg!(windows) {
                    Some("dll")
                } else if cfg!(target_os = "macos") {
                    Some("dylib")
                } else {
                    Some("so")
                }
            }
            EmitType::Assembly => {
                if cfg!(windows) {
                    Some("asm")
                } else {
                    Some("s")
                }
            }
        }
    }

    /// Convert to AotOptions for the existing pipeline
    pub fn to_aot_options(&self) -> crate::backends::cranelift_aot::AotOptions {
        crate::backends::cranelift_aot::AotOptions {
            opt_level: self.opt_level,
            target_triple: self.target.clone(),
            output_format: self.emit.to_aot_format(),
            debug_info: self.debug_info,
            flags: std::collections::HashMap::new(),
            library_mode: self.library_mode,
            include_dirs: self
                .include_dirs
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect(),
            lib_dirs: self
                .lib_dirs
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect(),
            link_libs: self.link_libs.clone(),
            extra_linker_args: self.linker_args.clone(),
            fast_compile: self.fast_compile,
            enable_dead_code_elimination: true,
            enable_lto: self.opt_level >= 3 && !self.fast_compile,
            incremental: self.incremental,
            cache_dir: None,
            force_rebuild: self.force_rebuild,
        }
    }

    /// Display build plan (for --dry-run)
    pub fn display_plan(&self) {
        let output = self.get_output_path();

        println!(
            "{}{}Build Plan:{}",
            colors::BOLD,
            colors::CYAN,
            colors::RESET
        );
        println!(
            "  {}Input:{}        {}",
            colors::GREEN,
            colors::RESET,
            self.input.display()
        );
        println!(
            "  {}Output:{}       {}",
            colors::GREEN,
            colors::RESET,
            output.display()
        );
        println!(
            "  {}Format:{}       {:?}",
            colors::GREEN,
            colors::RESET,
            self.emit
        );
        println!(
            "  {}Optimization:{} O{}{}",
            colors::GREEN,
            colors::RESET,
            self.opt_level,
            if self.fast_compile {
                format!(" {}(fast mode - no optimization)", colors::YELLOW)
            } else {
                String::new()
            }
        );

        if self.fast_compile {
            println!(
                "  {}Mode:{}         🚀 Development (lightning-fast compilation)",
                colors::GREEN,
                colors::RESET
            );
        }

        if let Some(ref target) = self.target {
            println!(
                "  {}Target:{}       {}",
                colors::GREEN,
                colors::RESET,
                target
            );
        } else {
            println!("  {}Target:{}       (host)", colors::GREEN, colors::RESET);
        }

        if self.debug_info {
            println!("  {}Debug info:{}   enabled", colors::GREEN, colors::RESET);
        }

        if let Some(ref header) = self.emit_header {
            println!(
                "  {}Header:{}       {}",
                colors::GREEN,
                colors::RESET,
                header.display()
            );
        }

        if !self.include_dirs.is_empty() {
            println!(
                "  {}Include dirs:{} {:?}",
                colors::GREEN,
                colors::RESET,
                self.include_dirs
            );
        }

        if !self.lib_dirs.is_empty() {
            println!(
                "  {}Lib dirs:{}     {:?}",
                colors::GREEN,
                colors::RESET,
                self.lib_dirs
            );
        }

        if !self.link_libs.is_empty() {
            println!(
                "  {}Link libs:{}    {:?}",
                colors::GREEN,
                colors::RESET,
                self.link_libs
            );
        }

        if self.run_after_build {
            println!("  {}Run after:{}    yes", colors::GREEN, colors::RESET);
            if !self.program_args.is_empty() {
                println!(
                    "  {}Args:{}         {:?}",
                    colors::GREEN,
                    colors::RESET,
                    self.program_args
                );
            }
        }
    }
}

/// Parse build command arguments
pub fn parse_build_args(
    args: &[String],
    start_index: usize,
) -> Result<(AotBuildConfig, bool), String> {
    let mut config = AotBuildConfig::default();
    let mut i = start_index;
    let mut input_found = false;
    let mut is_run_subcommand = false;

    // Check for "run" subcommand
    if i < args.len() && args[i] == "run" {
        is_run_subcommand = true;
        config.run_after_build = true;
        i += 1;
    }

    while i < args.len() {
        let arg = &args[i];

        // Handle -- separator (args after this go to the program)
        if arg == "--" {
            // Collect remaining args for the program
            config.program_args = args[i + 1..].to_vec();
            break;
        }

        match arg.as_str() {
            // Output specification
            "-o" | "--output" => {
                if i + 1 >= args.len() {
                    return Err("-o requires an output path".to_string());
                }
                config.output = Some(PathBuf::from(&args[i + 1]));
                i += 2;
                continue;
            }

            // Optimization levels
            "-O0" | "--opt=0" => config.opt_level = 0,
            "-O1" | "--opt=1" => config.opt_level = 1,
            "-O2" | "--opt=2" => config.opt_level = 2,
            "-O3" | "--opt=3" | "--release" => config.opt_level = 3,
            "--fast" => {
                config.fast_compile = true;
                config.opt_level = 0;
            }

            // Debug info
            "-g" | "--debug" => config.debug_info = true,

            // Output types
            "-c" | "--compile-only" => config.emit = EmitType::Object,
            "-S" | "--emit-asm" => config.emit = EmitType::Assembly,
            "--lib" | "--static" => {
                config.emit = EmitType::StaticLib;
                config.library_mode = true;
            }
            "--shared" | "--dylib" | "--dll" | "--so" => {
                config.emit = EmitType::SharedLib;
                config.library_mode = true;
            }
            "--object" | "--obj" => config.emit = EmitType::Object,

            // Emit type explicit
            _ if arg.starts_with("--emit=") => {
                let emit_str = &arg[7..];
                config.emit = EmitType::from_str(emit_str)
                    .ok_or_else(|| format!("Unknown emit type: {}", emit_str))?;
                if matches!(config.emit, EmitType::StaticLib | EmitType::SharedLib) {
                    config.library_mode = true;
                }
            }

            // Target triple
            _ if arg.starts_with("--target=") => {
                config.target = Some(arg[9..].to_string());
            }
            "--target" => {
                if i + 1 >= args.len() {
                    return Err("--target requires a target triple".to_string());
                }
                config.target = Some(args[i + 1].clone());
                i += 2;
                continue;
            }

            // Header generation
            "--emit-header" => {
                if i + 1 < args.len() && !args[i + 1].starts_with("-") {
                    config.emit_header = Some(PathBuf::from(&args[i + 1]));
                    i += 2;
                    continue;
                } else {
                    // Auto-generate header path
                    config.emit_header = Some(PathBuf::new());
                }
            }

            // Include directories
            _ if arg.starts_with("-I") => {
                if arg.len() > 2 {
                    config.include_dirs.push(PathBuf::from(&arg[2..]));
                } else if i + 1 < args.len() {
                    config.include_dirs.push(PathBuf::from(&args[i + 1]));
                    i += 2;
                    continue;
                }
            }

            // Library directories
            _ if arg.starts_with("-L") => {
                if arg.len() > 2 {
                    config.lib_dirs.push(PathBuf::from(&arg[2..]));
                } else if i + 1 < args.len() {
                    config.lib_dirs.push(PathBuf::from(&args[i + 1]));
                    i += 2;
                    continue;
                }
            }

            // Link libraries
            _ if arg.starts_with("-l") => {
                if arg.len() > 2 {
                    config.link_libs.push(arg[2..].to_string());
                } else if i + 1 < args.len() {
                    config.link_libs.push(args[i + 1].clone());
                    i += 2;
                    continue;
                }
            }

            // Binary target specification (cargo-like)
            "--bin" => {
                if i + 1 >= args.len() {
                    return Err("--bin requires a binary name".to_string());
                }
                config.bin = Some(args[i + 1].clone());
                i += 2;
                continue;
            }
            "--bins" => config.bins = true,

            // Incremental caching
            "--incremental" => config.incremental = true,
            "--no-incremental" | "--no-cache" => config.incremental = false,
            "-f" | "--force" => config.force_rebuild = true,
            "--clear-cache" => config.clear_cache = true,

            // Verbose
            "-v" | "--verbose" => config.verbose = true,

            // Quiet mode
            "-q" | "--quiet" => config.quiet = true,

            // Check only
            "--check" => config.check_only = true,

            // Dry run
            "--dry-run" => config.dry_run = true,

            // Run after build (explicit flag)
            "--run" => config.run_after_build = true,

            // Skip help/version (handled elsewhere)
            "-h" | "--help" | "--version" => {}

            // Positional argument (input file)
            _ if !arg.starts_with("-") => {
                if !input_found {
                    config.input = PathBuf::from(arg);
                    input_found = true;
                } else {
                    // Second positional could be output (legacy support)
                    config.output = Some(PathBuf::from(arg));
                }
            }

            // Unknown option
            _ => {
                if arg.starts_with("-") {
                    return Err(format!("Unknown option: {}", arg));
                }
            }
        }

        i += 1;
    }

    if !input_found && config.bin.is_none() && !config.bins {
        // If we can discover an adesh.adl project containing src/main.adesh, we can default to it
        let current_dir = std::env::current_dir().unwrap_or_default();
        let project_found =
            crate::ecosystem::project::ProjectLayout::discover(&current_dir).is_some();
        if !project_found {
            return Err("No input file specified".to_string());
        }
    }

    // Auto-generate header path if requested but not specified
    if config.emit_header == Some(PathBuf::new()) {
        let output = config.get_output_path();
        config.emit_header = Some(output.with_extension("h"));
    }

    Ok((config, is_run_subcommand))
}

/// Resolve a config into one or more concrete binary build targets
pub fn resolve_project_targets(config: &AotBuildConfig) -> Result<Vec<AotBuildConfig>, String> {
    // If input is specified, we build exactly that file
    if !config.input.as_os_str().is_empty() {
        return Ok(vec![config.clone()]);
    }

    // Try to discover project layout
    let current_dir = std::env::current_dir().map_err(|e| e.to_string())?;
    let layout = match crate::ecosystem::project::ProjectLayout::discover(&current_dir) {
        Some(l) => l,
        None => {
            return Err("No input file specified, and no adesh.adl project found in current directory or parents.".to_string());
        }
    };

    let bin_dir = layout.root.join("src").join("bin");

    // 1. If a specific binary name is requested (--bin name)
    if let Some(ref bin_name) = config.bin {
        let bin_file = bin_dir.join(format!("{}.adesh", bin_name));
        if !bin_file.exists() {
            return Err(format!(
                "Binary '{}' not found (expected file at {})",
                bin_name,
                bin_file.display()
            ));
        }
        let mut target_config = config.clone();
        target_config.input = bin_file;
        let output_name = if cfg!(windows) {
            format!("{}.exe", bin_name)
        } else {
            bin_name.clone()
        };
        target_config.output = Some(layout.root.join("target").join("bin").join(output_name));
        return Ok(vec![target_config]);
    }

    // 2. Otherwise, collect default targets: src/main.adesh and/or src/bin/*.adesh
    let mut targets = Vec::new();
    let mut main_file_found = false;

    // Check if default main src/main.adesh exists
    let main_file = layout.root.join("src").join("main.adesh");
    if main_file.exists() {
        let mut main_config = config.clone();
        main_config.input = main_file;
        let proj_name = layout
            .load_manifest()
            .ok()
            .and_then(|m| m.project_name())
            .unwrap_or_else(|| "main".to_string());
        let output_name = if cfg!(windows) {
            format!("{}.exe", proj_name)
        } else {
            proj_name
        };
        main_config.output = Some(layout.root.join("target").join(output_name));
        targets.push(main_config);
        main_file_found = true;
    }

    // Scan src/bin/*.adesh for binaries
    if bin_dir.exists() && bin_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&bin_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("adesh") {
                    let bin_name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap()
                        .to_string();

                    // If --bins was specified, or if we are building default targets (when not running)
                    if config.bins || !config.run_after_build {
                        let mut bin_config = config.clone();
                        bin_config.input = path;
                        let output_name = if cfg!(windows) {
                            format!("{}.exe", bin_name)
                        } else {
                            bin_name
                        };
                        bin_config.output =
                            Some(layout.root.join("target").join("bin").join(output_name));
                        targets.push(bin_config);
                    }
                }
            }
        }
    }

    if targets.is_empty() {
        if !main_file_found {
            return Err(
                "No entry points found. Expected src/main.adesh or files in src/bin/".to_string(),
            );
        }
    }

    Ok(targets)
}

/// Execute the build command
pub fn execute_build(config: &AotBuildConfig) -> Result<PathBuf, String> {
    use crate::backends::cranelift_aot::aot_compile_with_options;
    use std::fs;

    if config.clear_cache {
        let cache = crate::backends::aot::cache::AotCompilationCache::new(None, true);
        if let Ok(count) = cache.clear() {
            if !config.quiet {
                println!("🧹 Cleared {} compilation cache artifacts", count);
            }
        }
    }

    // Read source file
    let src = fs::read_to_string(&config.input)
        .map_err(|e| format!("Failed to read input file: {}", e))?;

    // Dry run - just show what would be done
    if config.dry_run {
        config.display_plan();
        return Ok(config.get_output_path());
    }

    // Check only - parse and validate without emitting
    if config.check_only {
        use crate::backends::lir_lower::hir_to_lir;
        use crate::parsing::hir_lower::ast_to_hir;
        use crate::parsing::lexer::Lexer;
        use crate::parsing::parser::Parser;

        let progress = if !config.quiet {
            Some(BuildProgress::new("Checking"))
        } else {
            None
        };

        let mut lexer = Lexer::new(&src);
        let tokens = match lexer.tokenize() {
            Ok(t) => t,
            Err(e) => {
                if let Some(p) = progress {
                    p.fail("Check failed");
                }
                return Err(e.to_string());
            }
        };

        let mut parser = Parser::new(tokens, None);
        let ast = parser.parse_program().map_err(|e| e.to_string())?;
        let hir = ast_to_hir(&ast, false)?;
        let _lir = hir_to_lir(&hir)?;

        if let Some(p) = progress {
            p.success(&format!("{} checked successfully", config.input.display()));
        } else {
            println!("✓ {} checked successfully", config.input.display());
        }
        return Ok(config.get_output_path());
    }

    let output = config.get_output_path();
    if let Some(parent) = output.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "Failed to create output directory {}: {}",
                    parent.display(),
                    e
                )
            })?;
        }
    }
    let aot_options = config.to_aot_options();

    // Get input file name for display
    let input_name = config
        .input
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| config.input.to_string_lossy().to_string());

    // Start progress spinner
    let progress = if !config.quiet {
        Some(BuildProgress::new(&format!("Compiling {}", input_name)))
    } else {
        None
    };

    let start_time = std::time::Instant::now();

    // Compile using existing AOT pipeline
    match aot_compile_with_options(&src, &output, aot_options) {
        Ok(()) => {
            let elapsed = start_time.elapsed();
            let time_str = if elapsed.as_millis() < 1000 {
                format!("{}ms", elapsed.as_millis())
            } else {
                format!("{:.2}s", elapsed.as_secs_f64())
            };

            if let Some(p) = progress {
                p.success(&format!("Built {} in {}", output.display(), time_str));
            } else {
                println!("Successfully built: {} in {}", output.display(), time_str);
            }

            // If inside an ADL project, ensure .adl/bin has a copy of the built binary
            if let Some(layout) = crate::ecosystem::project::ProjectLayout::discover(&output)
                .or_else(|| {
                    std::env::current_dir()
                        .ok()
                        .and_then(|cwd| crate::ecosystem::project::ProjectLayout::discover(cwd))
                })
            {
                let _ = layout.ensure_layout();
                if let Some(fname) = output.file_name() {
                    let adl_bin_dest = layout.bin_dir().join(fname);
                    if adl_bin_dest != output {
                        let _ = fs::copy(&output, &adl_bin_dest);
                    }
                }
            }
        }
        Err(e) => {
            if let Some(p) = progress {
                p.fail("Build failed");
            }
            return Err(e);
        }
    }

    // Generate header if requested
    if let Some(ref header_path) = config.emit_header {
        let header_progress = if !config.quiet {
            Some(BuildProgress::new("Generating C header"))
        } else {
            None
        };

        match crate::cli::aot_utils::generate_aot_header(&src, header_path) {
            Ok(()) => {
                if let Some(p) = header_progress {
                    p.success(&format!("Generated {}", header_path.display()));
                } else {
                    println!("Generated header file: {}", header_path.display());
                }
            }
            Err(e) => {
                if let Some(p) = header_progress {
                    p.fail("Header generation failed");
                }
                return Err(e);
            }
        }
    }

    Ok(output)
}

/// Execute the built binary
pub fn run_executable(executable: &PathBuf, args: &[String]) -> Result<i32, String> {
    use std::process::Command;
    // use std::path::Path;

    // Convert to absolute path to ensure it can be found on all platforms
    let abs_exe = if executable.is_absolute() {
        executable.clone()
    } else {
        std::env::current_dir()
            .map_err(|e| format!("Failed to get current directory: {}", e))?
            .join(executable)
    };

    // Verify the executable exists
    if !abs_exe.exists() {
        return Err(format!("Executable not found: {}", abs_exe.display()));
    }

    println!(
        "\n{}{}▶ Running {}{}{}",
        colors::BOLD,
        colors::CYAN,
        executable.display(),
        colors::RESET,
        if !args.is_empty() {
            format!(" with args: {:?}", args)
        } else {
            String::new()
        }
    );
    println!("{}", "─".repeat(60));

    #[cfg(windows)]
    {
        // Best-effort ANSI enablement so AOT pretty-print colors render on Windows consoles.
        let _ = enable_windows_vt_mode();
    }

    let status = Command::new(&abs_exe).args(args).status().map_err(|e| {
        format!(
            "Failed to run executable: {} (tried: {})",
            e,
            abs_exe.display()
        )
    })?;

    println!("{}", "─".repeat(60));

    let exit_code = status.code().unwrap_or(-1);
    if status.success() {
        println!(
            "{}{}✓ Process exited with code {}{}",
            colors::GREEN,
            colors::BOLD,
            exit_code,
            colors::RESET
        );
    } else {
        println!(
            "{}{}✗ Process exited with code {}{}",
            colors::RED,
            colors::BOLD,
            exit_code,
            colors::RESET
        );
    }

    Ok(exit_code)
}

#[cfg(windows)]
fn enable_windows_vt_mode() -> Result<(), String> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{
        ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, GetStdHandle, STD_OUTPUT_HANDLE,
        SetConsoleMode,
    };

    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        if handle == 0 || handle == INVALID_HANDLE_VALUE {
            return Err("invalid stdout handle".to_string());
        }

        let mut mode: u32 = 0;
        if GetConsoleMode(handle, &mut mode) == 0 {
            return Err("GetConsoleMode failed".to_string());
        }

        let new_mode = mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING;
        if SetConsoleMode(handle, new_mode) == 0 {
            return Err("SetConsoleMode failed".to_string());
        }
    }

    Ok(())
}

/// Generate help message for build command
pub fn build_help_message() -> String {
    format!(
        r#"{}{}Language Build Command{}

{}USAGE:{}
    adesh build [OPTIONS] <FILE>
    adesh build run [OPTIONS] <FILE> [-- <ARGS>...]

{}ARGS:{}
    <FILE>    Source file to compile (.adesh)

{}SUBCOMMANDS:{}
    run                     Build and run the executable

{}OUTPUT OPTIONS:{}
    -o, --output <FILE>     Output file path (auto-inferred if not specified)
    --emit=<TYPE>           Output type: exe, obj, lib, dylib, asm
    --lib, --static         Build as static library
    --shared, --dylib       Build as shared library
    -c, --compile-only      Compile only (output object file)
    -S, --emit-asm          Output assembly

{}OPTIMIZATION:{}
    -O0                     No optimization (fastest compile)
    -O1                     Basic optimization
    -O2                     Standard optimization
    -O3, --release          Maximum optimization (default)
    --fast                  Lightning-fast compile (no optimization, dev mode)

{}DEBUG:{}
    -g, --debug             Include debug information

{}CROSS-COMPILATION:{}
    --target=<TRIPLE>       Target platform (e.g., x86_64-unknown-linux-gnu)
                           Common targets:
                             x86_64-unknown-linux-gnu     Linux x86_64
                             aarch64-unknown-linux-gnu    Linux ARM64
                             x86_64-pc-windows-gnu        Windows x86_64
                             x86_64-apple-darwin          macOS x86_64
                             aarch64-apple-darwin         macOS ARM64 (Apple Silicon)

{}FFI OPTIONS:{}
    --emit-header [FILE]    Generate C header for exported functions
    -I<DIR>                 Add include directory
    -L<DIR>                 Add library search path
    -l<LIB>                 Link with library

{}GENERAL:{}
    -v, --verbose           Verbose output
    -q, --quiet             Suppress progress output
    --dry-run               Show what would be done without building
    --check                 Check compilation without generating output
    -h, --help              Show this help

{}EXAMPLES:{}
    # Basic builds
    {}adesh build program.adesh{}                   Build native executable
    {}adesh build program.adesh -o app{}            Custom output name
    {}adesh build program.adesh --release{}         Optimized release build
    {}adesh build program.adesh --fast{}            Fast dev build (skip optimizations)

    # Build and run
    {}adesh build run program.adesh{}               Build and execute
    {}adesh build run program.adesh -- arg1 arg2{}  Build, run with arguments

    # Library builds
    {}adesh build mylib.adesh --lib{}               Static library
    {}adesh build mylib.adesh --shared{}            Shared library
    {}adesh build mylib.adesh -c --emit-header{}    Object + C header

    # Cross-compilation
    {}adesh build program.adesh --target=aarch64-unknown-linux-gnu{}

{}OUTPUT FILE INFERENCE:{}
    Input: program.adesh
    
    --emit=exe  → program (Unix) / program.exe (Windows)
    --emit=obj  → program.o (Unix) / program.obj (Windows)
    --lib       → libprogram.a (Unix) / program.lib (Windows)
    --shared    → libprogram.so (Linux) / libprogram.dylib (macOS) / program.dll (Windows)

For the legacy AOT command with full control: adesh compile-aot --help
"#,
        colors::BOLD,
        colors::CYAN,
        colors::RESET,
        colors::BOLD,
        colors::RESET,
        colors::BOLD,
        colors::RESET,
        colors::BOLD,
        colors::RESET,
        colors::BOLD,
        colors::RESET,
        colors::BOLD,
        colors::RESET,
        colors::BOLD,
        colors::RESET,
        colors::BOLD,
        colors::RESET,
        colors::BOLD,
        colors::RESET,
        colors::BOLD,
        colors::RESET,
        colors::BOLD,
        colors::RESET,
        colors::CYAN,
        colors::RESET,
        colors::CYAN,
        colors::RESET,
        colors::CYAN,
        colors::RESET,
        colors::CYAN,
        colors::RESET,
        colors::CYAN,
        colors::RESET,
        colors::CYAN,
        colors::RESET,
        colors::CYAN,
        colors::RESET,
        colors::CYAN,
        colors::RESET,
        colors::CYAN,
        colors::RESET,
        colors::CYAN,
        colors::RESET,
        colors::BOLD,
        colors::RESET,
    )
}
