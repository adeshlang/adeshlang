//! Linking functions for combining object files into executables

use std::path::Path;
use std::process::Command;
use target_lexicon::Triple;

use crate::backends::aot::cranelift::{AotOptions, OutputFormat};

/// Link object files using LLVM toolchain
pub(crate) fn link_with_linker_driver(
    options: &AotOptions,
    obj_path: &Path,
    runtime_obj: &Path,
    output: &Path,
    target_triple: &Triple,
) -> Result<(), String> {
    // Use new LLVM-only linking (bypassing old LinkerDriver)
    link_with_llvm_toolchain(
        options,
        obj_path,
        runtime_obj,
        output,
        target_triple,
        &options.output_format,
    )
}

/// Link pure library only (without runtime)
pub(crate) fn link_library_only(
    options: &AotOptions,
    obj_path: &Path,
    output: &Path,
    target_triple: &Triple,
) -> Result<(), String> {
    match options.output_format {
        OutputFormat::StaticLib => {
            let clang_path = find_clang()?;
            let llvm_bin = clang_path.parent().ok_or("Invalid clang path")?;
            let ar_name = if cfg!(windows) {
                "llvm-ar.exe"
            } else {
                "llvm-ar"
            };
            let ar_path = llvm_bin.join(ar_name);
            let mut cmd = Command::new(if ar_path.exists() {
                ar_path
            } else {
                std::path::PathBuf::from("ar")
            });
            cmd.arg("rcs").arg(output).arg(obj_path);
            let result = cmd
                .output()
                .map_err(|e| format!("Failed to run ar: {}", e))?;
            if result.status.success() {
                Ok(())
            } else {
                Err(format!(
                    "Static library archiving failed: {}",
                    String::from_utf8_lossy(&result.stderr)
                ))
            }
        }
        OutputFormat::SharedLib => {
            let clang_path = find_clang()?;
            let mut cmd = Command::new(&clang_path);
            cmd.arg("-shared").arg("-fuse-ld=lld");
            cmd.arg(format!("--target={}", target_triple));
            cmd.arg("-o").arg(output).arg(obj_path);
            let result = cmd
                .output()
                .map_err(|e| format!("Failed to run clang: {}", e))?;
            if result.status.success() {
                Ok(())
            } else {
                Err(format!(
                    "Shared library linking failed: {}",
                    String::from_utf8_lossy(&result.stderr)
                ))
            }
        }
        _ => Err("link_library_only called for non-library format".to_string()),
    }
}

/// In-memory cache for discovered toolchain components
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct CachedToolchain {
    pub clang_path: std::path::PathBuf,
    pub lld_path: std::path::PathBuf,
    pub windows_sdk: Option<(std::path::PathBuf, std::path::PathBuf)>,
    pub msvc_lib: Option<std::path::PathBuf>,
}

static TOOLCHAIN_CACHE: std::sync::OnceLock<CachedToolchain> = std::sync::OnceLock::new();

fn get_cached_toolchain_file() -> std::path::PathBuf {
    if let Ok(home) = std::env::var("ADESH_HOME").or_else(|_| std::env::var("ADESHLANG_HOME")) {
        return std::path::PathBuf::from(home).join(".toolchain_cache.json");
    }
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        return std::path::PathBuf::from(local_app_data)
            .join("AdeshLang")
            .join("toolchain_cache.json");
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    cwd.join(".adesh_cache").join("toolchain.json")
}

fn try_load_toolchain_cache() -> Option<CachedToolchain> {
    let cache_file = get_cached_toolchain_file();
    if let Ok(content) = std::fs::read_to_string(&cache_file) {
        if let Ok(cached) = serde_json::from_str::<CachedToolchain>(&content) {
            if cached.clang_path.exists() && cached.lld_path.exists() {
                return Some(cached);
            }
        }
    }
    None
}

fn save_toolchain_cache(tc: &CachedToolchain) {
    let cache_file = get_cached_toolchain_file();
    if let Some(parent) = cache_file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json_str) = serde_json::to_string_pretty(tc) {
        let _ = std::fs::write(&cache_file, json_str);
    }
}

/// Link using LLVM toolchain (clang + lld) exclusively
fn link_with_llvm_toolchain(
    options: &AotOptions,
    obj_path: &Path,
    runtime_obj: &Path,
    output: &Path,
    target_triple: &Triple,
    output_format: &OutputFormat,
) -> Result<(), String> {
    // Validate clang is available
    let clang_path = find_clang()?;

    // Validate lld is available from same LLVM installation
    let lld_path = validate_lld_available(&clang_path)?;

    // Cache toolchain paths if not cached yet
    if TOOLCHAIN_CACHE.get().is_none() {
        let tc = CachedToolchain {
            clang_path: clang_path.clone(),
            lld_path: lld_path.clone(),
            windows_sdk: find_windows_sdk_paths(),
            msvc_lib: find_msvc_lib_paths(),
        };
        save_toolchain_cache(&tc);
        let _ = TOOLCHAIN_CACHE.set(tc);
    }

    // For Windows MSVC target, use lld-link directly (MSVC-compatible mode)
    // For other targets, use clang with -fuse-ld=lld
    match target_triple.operating_system {
        target_lexicon::OperatingSystem::Windows => {
            // Check if this is MSVC target (x86_64-pc-windows-msvc)
            if target_triple.vendor == target_lexicon::Vendor::Pc
                && target_triple.environment == target_lexicon::Environment::Msvc
            {
                // Use lld-link directly for MSVC compatibility
                link_with_lld_link(
                    options,
                    &lld_path,
                    obj_path,
                    runtime_obj,
                    output,
                    output_format,
                )
            } else {
                // MinGW target - use clang with lld
                link_with_clang_lld(
                    options,
                    &clang_path,
                    &lld_path,
                    obj_path,
                    runtime_obj,
                    output,
                    target_triple,
                    output_format,
                )
            }
        }
        _ => {
            // Unix-like systems - use clang with lld
            link_with_clang_lld(
                options,
                &clang_path,
                &lld_path,
                obj_path,
                runtime_obj,
                output,
                target_triple,
                output_format,
            )
        }
    }
}

/// Link using lld-link (MSVC-compatible mode) for Windows MSVC targets
fn link_with_lld_link(
    options: &AotOptions,
    lld_path: &Path,
    obj_path: &Path,
    runtime_obj: &Path,
    output: &Path,
    output_format: &OutputFormat,
) -> Result<(), String> {
    let is_dll = matches!(output_format, OutputFormat::SharedLib);

    // On Windows, lld-link may create `output.exe.tmpN` replacement files when an
    // existing output is present. Remove stale output first to avoid tmp leakage.
    if output.exists() {
        let _ = std::fs::remove_file(output);
    }

    let mut cmd = Command::new(lld_path);

    // MSVC-style arguments
    cmd.arg("/NOLOGO");
    cmd.arg("/MACHINE:X64");
    cmd.arg(format!("/OUT:{}", output.display()));

    // Fast-linking vs Release optimization flags
    if options.fast_compile || options.opt_level <= 1 {
        cmd.arg("/INCREMENTAL");
        cmd.arg("/OPT:NOREF,NOICF");
        if options.debug_info {
            cmd.arg("/DEBUG:FASTLINK");
        } else {
            cmd.arg("/DEBUG:NONE");
        }
    } else {
        cmd.arg("/OPT:REF,ICF");
        if options.enable_lto {
            cmd.arg("/LTCG");
        }
        if options.debug_info {
            cmd.arg("/DEBUG:FULL");
        } else {
            cmd.arg("/DEBUG:NONE");
        }
    }

    // Add DLL flag if building shared library
    if is_dll {
        cmd.arg("/DLL");
        // Generate import library with same base name
        let import_lib = output.with_extension("lib");
        cmd.arg(format!("/IMPLIB:{}", import_lib.display()));
    }

    // Input object files
    cmd.arg(obj_path);
    cmd.arg(runtime_obj);

    // Windows system libraries (CRITICAL for MSVC runtime)
    cmd.arg("kernel32.lib");
    cmd.arg("ntdll.lib"); // NtReadFile, NtWriteFile
    cmd.arg("ws2_32.lib"); // WSA* functions
    cmd.arg("advapi32.lib");
    cmd.arg("userenv.lib");
    cmd.arg("bcrypt.lib");
    cmd.arg("user32.lib");
    cmd.arg("shell32.lib");
    cmd.arg("ole32.lib");
    cmd.arg("uuid.lib");

    // MSVC runtime (DYNAMIC CRT - matches Rust's default /MD build)
    cmd.arg("ucrt.lib"); // Universal C Runtime (import lib for dynamic ucrt)
    cmd.arg("msvcrt.lib"); // C runtime (import lib for msvcrt.dll)
    cmd.arg("legacy_stdio_definitions.lib"); // For older stdio functions

    // Detect Windows SDK and MSVC installation paths
    let sdk_paths = find_windows_sdk_paths();
    let msvc_paths = find_msvc_lib_paths();

    // Add SDK library paths
    if let Some((um_path, ucrt_path)) = sdk_paths {
        cmd.arg(format!("/LIBPATH:{}", um_path.display()));
        cmd.arg(format!("/LIBPATH:{}", ucrt_path.display()));
    } else if let Ok(sdk_lib) = std::env::var("WindowsSdkDir") {
        // Fallback to environment variables
        let sdk_version = std::env::var("WindowsSDKVersion").unwrap_or_default();
        let lib_path = format!(
            "{}Lib\\{}um\\x64",
            sdk_lib,
            sdk_version.trim_end_matches('\\')
        );
        cmd.arg(format!("/LIBPATH:{}", lib_path));

        let ucrt_path = format!(
            "{}Lib\\{}ucrt\\x64",
            sdk_lib,
            sdk_version.trim_end_matches('\\')
        );
        cmd.arg(format!("/LIBPATH:{}", ucrt_path));
    }

    // Add MSVC library paths
    if let Some(msvc_path) = msvc_paths {
        cmd.arg(format!("/LIBPATH:{}", msvc_path.display()));
    } else if let Ok(vctools) = std::env::var("VCToolsInstallDir") {
        // Fallback to environment variables
        let lib_path = format!("{}lib\\x64", vctools);
        cmd.arg(format!("/LIBPATH:{}", lib_path));
    }

    // Extra linker args from options
    for dir in &options.lib_dirs {
        cmd.arg(format!("/LIBPATH:{}", dir));
    }
    for lib in &options.link_libs {
        if lib.ends_with(".lib") {
            cmd.arg(lib);
        } else {
            cmd.arg(format!("{}.lib", lib));
        }
    }
    for arg in &options.extra_linker_args {
        cmd.arg(arg);
    }

    let output_result = cmd
        .output()
        .map_err(|e| format!("Failed to execute lld-link: {}", e))?;

    if output_result.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output_result.stderr);
        Err(format!(
            "lld-link failed:\n{}\n\n\
            ⚠️  Windows SDK or MSVC runtime libraries not found.\n\n\
            Install Visual Studio Build Tools or Windows SDK:\n\
            https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022",
            stderr
        ))
    }
}

/// Link using clang with lld (GNU-compatible mode)
fn link_with_clang_lld(
    options: &AotOptions,
    clang_path: &Path,
    lld_path: &Path,
    obj_path: &Path,
    runtime_obj: &Path,
    output: &Path,
    target_triple: &Triple,
    output_format: &OutputFormat,
) -> Result<(), String> {
    let is_dll = matches!(output_format, OutputFormat::SharedLib);

    let mut cmd = Command::new(clang_path);

    // Force lld linker if available, otherwise let clang use system default linker
    let lld_name = lld_path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if lld_name.contains("lld") {
        cmd.arg("-fuse-ld=lld");
    }

    // Add target triple
    cmd.arg(format!("--target={}", target_triple));

    // Output file
    cmd.arg("-o").arg(output);

    // Optimization and dead-code flags
    if options.fast_compile || options.opt_level <= 1 {
        cmd.arg("-O0");
        cmd.arg("-Wl,--no-gc-sections");
    } else {
        cmd.arg(format!("-O{}", options.opt_level));
        cmd.arg("-ffunction-sections");
        cmd.arg("-fdata-sections");
        if matches!(
            target_triple.operating_system,
            target_lexicon::OperatingSystem::MacOSX { .. }
                | target_lexicon::OperatingSystem::Darwin
        ) {
            cmd.arg("-Wl,-dead_strip");
        } else {
            cmd.arg("-Wl,--gc-sections");
        }
        if options.enable_lto {
            cmd.arg("-flto");
        }
        if !options.debug_info {
            cmd.arg("-s");
        }
    }

    if options.debug_info {
        cmd.arg("-g");
    }

    // Add shared library flag if needed
    if is_dll {
        cmd.arg("-shared");
    } else if target_triple.operating_system == target_lexicon::OperatingSystem::Linux {
        cmd.arg("-no-pie");
    }

    // Input object files
    cmd.arg(obj_path).arg(runtime_obj);

    // Add library search paths and libraries from options
    for dir in &options.lib_dirs {
        cmd.arg(format!("-L{}", dir));
    }
    for lib in &options.link_libs {
        cmd.arg(format!("-l{}", lib));
    }
    for arg in &options.extra_linker_args {
        cmd.arg(arg);
    }

    // Platform-specific libraries
    match target_triple.operating_system {
        target_lexicon::OperatingSystem::Windows => {
            cmd.arg("-lkernel32")
                .arg("-lmsvcrt")
                .arg("-lws2_32")
                .arg("-ladvapi32");
        }
        target_lexicon::OperatingSystem::Linux => {
            cmd.arg("-lc").arg("-lpthread").arg("-ldl").arg("-lm");
        }
        target_lexicon::OperatingSystem::MacOSX { .. }
        | target_lexicon::OperatingSystem::Darwin => {
            cmd.arg("-lSystem");
        }
        _ => {}
    }

    let output_result = cmd
        .output()
        .map_err(|e| format!("Failed to execute clang: {}", e))?;

    if output_result.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output_result.stderr);
        Err(format!("clang + lld linking failed:\n{}", stderr))
    }
}

/// Find clang executable with fast caching
fn find_clang() -> Result<std::path::PathBuf, String> {
    if let Some(cached) = TOOLCHAIN_CACHE.get() {
        if cached.clang_path.exists() {
            return Ok(cached.clang_path.clone());
        }
    }

    if let Some(cached) = try_load_toolchain_cache() {
        let clang = cached.clang_path.clone();
        let _ = TOOLCHAIN_CACHE.set(cached);
        return Ok(clang);
    }

    let mut candidates = Vec::new();

    let exe_suffix = if cfg!(windows) { ".exe" } else { "" };
    let clang_bin = format!("clang{}", exe_suffix);

    // 1. Explicit toolchain root: ADESH_TOOLCHAIN (ADESHLANG_TOOLCHAIN is a legacy alias)
    for env_var in ["ADESH_TOOLCHAIN", "ADESHLANG_TOOLCHAIN"] {
        if let Ok(tc_path) = std::env::var(env_var) {
            candidates.push(
                std::path::PathBuf::from(tc_path.clone())
                    .join("bin")
                    .join(&clang_bin),
            );
            candidates.push(std::path::PathBuf::from(tc_path).join(&clang_bin));
        }
    }

    // 2. Bundled toolchain under the install home: ADESH_HOME (ADESHLANG_HOME is a legacy alias)
    for env_var in ["ADESH_HOME", "ADESHLANG_HOME"] {
        if let Ok(home) = std::env::var(env_var) {
            candidates.push(
                std::path::PathBuf::from(home)
                    .join("toolchain")
                    .join("llvm")
                    .join("bin")
                    .join(&clang_bin),
            );
        }
    }

    // 3. Executable-relative toolchain directory
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            candidates.push(
                exe_dir
                    .join("toolchain")
                    .join("llvm")
                    .join("bin")
                    .join(&clang_bin),
            );
            if let Some(parent) = exe_dir.parent() {
                candidates.push(
                    parent
                        .join("toolchain")
                        .join("llvm")
                        .join("bin")
                        .join(&clang_bin),
                );
            }
        }
    }

    // 4. Common system LLVM locations (dynamic across all drives & program directories)
    for pf in get_program_files_roots() {
        candidates.push(
            pf.join("AdeshLang")
                .join("toolchain")
                .join("llvm")
                .join("bin")
                .join(&clang_bin),
        );
        candidates.push(pf.join("LLVM").join("bin").join(&clang_bin));
    }
    for drive in get_system_drive_roots() {
        candidates.push(drive.join("LLVM").join("bin").join(&clang_bin));
        candidates.push(
            drive
                .join("AdeshLang")
                .join("toolchain")
                .join("llvm")
                .join("bin")
                .join(&clang_bin),
        );
    }
    candidates.push(std::path::PathBuf::from("/usr/lib/llvm-18/bin/clang"));
    candidates.push(std::path::PathBuf::from("/usr/lib/llvm-19/bin/clang"));
    candidates.push(std::path::PathBuf::from("/usr/bin/clang"));
    candidates.push(std::path::PathBuf::from("/usr/local/bin/clang"));
    candidates.push(std::path::PathBuf::from("/opt/homebrew/opt/llvm/bin/clang"));
    candidates.push(std::path::PathBuf::from("clang")); // System PATH fallback

    for candidate in candidates {
        // Fast path: if file exists on disk, check directly
        if candidate.is_file() {
            return Ok(candidate);
        }

        // PATH fallback
        if candidate.to_string_lossy() == "clang" {
            let result = Command::new(&candidate).arg("--version").output();
            if let Ok(output) = result {
                if output.status.success() {
                    return Ok(candidate);
                }
            }
        }
    }

    Err(
        "❌ AdeshLang native toolchain (LLVM clang + lld) not found!\n\n\
        AdeshLang requires LLVM (clang + lld) for AOT native compilation.\n\n\
        Please run `adl doctor` to diagnose installation issues or run `adl repair` to restore the toolchain.\n\
        Or set ADESH_HOME (or legacy ADESHLANG_HOME) to your AdeshLang installation directory,
        or run `adesh toolchain install` to download the pinned LLVM toolchain.".to_string()
    )
}

/// Validate lld is available from the same LLVM installation or system and return its path
fn validate_lld_available(clang_path: &Path) -> Result<std::path::PathBuf, String> {
    let mut lld_candidates = Vec::new();

    // Get LLVM bin directory from clang path
    if let Some(llvm_bin) = clang_path.parent() {
        lld_candidates.extend(vec![
            llvm_bin.join("lld-link.exe"),
            llvm_bin.join("lld-link"),
            llvm_bin.join("lld.exe"),
            llvm_bin.join("lld"),
            llvm_bin.join("ld.lld.exe"),
            llvm_bin.join("ld.lld"),
        ]);
    }

    // System PATH fallbacks for lld
    let exe_suffix = if cfg!(windows) { ".exe" } else { "" };
    for name in ["lld-link", "lld", "ld.lld"] {
        let full = format!("{}{}", name, exe_suffix);
        if let Ok(path) = std::env::var("PATH") {
            for dir in std::env::split_paths(&path) {
                let candidate = dir.join(&full);
                if candidate.is_file() {
                    lld_candidates.push(candidate);
                }
            }
        }
    }

    for lld_path in &lld_candidates {
        if lld_path.exists() {
            eprintln!("   ✓ Found lld: {}", lld_path.display());
            return Ok(lld_path.clone());
        }
    }

    // System ld fallback if lld is not installed
    let system_ld = if cfg!(windows) { "ld.exe" } else { "ld" };
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join(system_ld);
            if candidate.is_file() {
                eprintln!("   ✓ Fallback to system linker: {}", candidate.display());
                return Ok(candidate);
            }
        }
    }

    Err("❌ Linker not found!\n\n\
        Adesh requires a linker (lld or system ld) for AOT compilation.\n\n\
        Please install LLVM/lld or binutils."
        .to_string())
}

/// Link using cross-compilation tools
#[allow(dead_code)]
pub(crate) fn link_cross_with_runtime(
    _options: &AotOptions,
    obj_path: &Path,
    runtime_obj: &Path,
    output: &Path,
    target_triple: &Triple,
) -> Result<(), String> {
    // Determine target prefix for cross-compilers
    let target_prefix = get_target_prefix(target_triple);

    // Build a list of potential cross-compilers to try
    let mut compilers_to_try: Vec<(String, Vec<String>)> = Vec::new();

    // Add target-prefixed GCC if we have a prefix
    if let Ok(prefix) = &target_prefix {
        compilers_to_try.push((
            format!("{}-gcc", prefix),
            get_linker_args_for_target_with_runtime(target_triple, obj_path, runtime_obj, output),
        ));
    }

    // Add clang with --target flag (works on most systems with clang installed)
    let mut clang_args = vec![
        format!("--target={}", target_triple),
        "-fuse-ld=lld".to_string(), // Prefer lld linker for cross-compilation
    ];
    clang_args.extend(get_linker_args_for_target_with_runtime(
        target_triple,
        obj_path,
        runtime_obj,
        output,
    ));
    compilers_to_try.push(("clang".to_string(), clang_args));

    // Add clang without lld (fallback)
    let mut clang_args_no_lld = vec![format!("--target={}", target_triple)];
    clang_args_no_lld.extend(get_linker_args_for_target_with_runtime(
        target_triple,
        obj_path,
        runtime_obj,
        output,
    ));
    compilers_to_try.push(("clang".to_string(), clang_args_no_lld));

    // Add zig cc as cross-compiler (Zig bundles cross-compilation support)
    let zig_target = get_zig_target(target_triple);
    if let Some(zig_target_str) = zig_target {
        let mut zig_args = vec!["cc".to_string(), format!("-target={}", zig_target_str)];
        zig_args.extend(get_linker_args_for_target_with_runtime(
            target_triple,
            obj_path,
            runtime_obj,
            output,
        ));
        compilers_to_try.push(("zig".to_string(), zig_args));
    }

    // Try each compiler
    let mut last_error = String::new();
    for (compiler, args) in compilers_to_try {
        match try_compiler(&compiler, &args) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_error = e;
                continue;
            }
        }
    }

    // If all failed, provide helpful error message
    Err(format!(
        "Cross-compilation linking failed for target {}.\n\
         Last error: {}\n\n\
         To fix this, install one of the following:\n\
         1. Cross-GCC: {} (e.g., apt install gcc-{} on Debian/Ubuntu)\n\
         2. Clang with LLD: Install clang and lld packages\n\
         3. Zig: Install zig (includes cross-compilation support)\n\n\
         Alternatively, use --object to generate object file only and link manually.",
        target_triple,
        last_error,
        target_prefix
            .as_ref()
            .map(|p| format!("{}-gcc", p))
            .unwrap_or_else(|_| "target-gcc".to_string()),
        target_prefix.unwrap_or_else(|_| "target".to_string())
    ))
}

#[allow(dead_code)]
pub(crate) fn link_cross(
    _options: &AotOptions,
    obj_path: &Path,
    output: &Path,
    target_triple: &Triple,
) -> Result<(), String> {
    // Determine target prefix for cross-compilers
    let target_prefix = get_target_prefix(target_triple);

    // Build a list of potential cross-compilers to try
    let mut compilers_to_try: Vec<(String, Vec<String>)> = Vec::new();

    // Add target-prefixed GCC if we have a prefix
    if let Ok(prefix) = &target_prefix {
        compilers_to_try.push((
            format!("{}-gcc", prefix),
            get_linker_args_for_target(target_triple, obj_path, output),
        ));
    }

    // Add clang with --target flag (works on most systems with clang installed)
    let mut clang_args = vec![
        format!("--target={}", target_triple),
        "-fuse-ld=lld".to_string(), // Prefer lld linker for cross-compilation
    ];
    clang_args.extend(get_linker_args_for_target(target_triple, obj_path, output));
    compilers_to_try.push(("clang".to_string(), clang_args));

    // Add clang without lld (fallback)
    let mut clang_args_no_lld = vec![format!("--target={}", target_triple)];
    clang_args_no_lld.extend(get_linker_args_for_target(target_triple, obj_path, output));
    compilers_to_try.push(("clang".to_string(), clang_args_no_lld));

    // Add zig cc as cross-compiler (Zig bundles cross-compilation support)
    let zig_target = get_zig_target(target_triple);
    if let Some(zig_target_str) = zig_target {
        let mut zig_args = vec!["cc".to_string(), format!("-target={}", zig_target_str)];
        zig_args.extend(get_linker_args_for_target(target_triple, obj_path, output));
        compilers_to_try.push(("zig".to_string(), zig_args));
    }

    // Try each compiler
    let mut last_error = String::new();
    for (compiler, args) in compilers_to_try {
        match try_compiler(&compiler, &args) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_error = e;
                continue;
            }
        }
    }

    // If all failed, provide helpful error message
    Err(format!(
        "Cross-compilation linking failed for target {}.\n\
         Last error: {}\n\n\
         To fix this, install one of the following:\n\
         1. Cross-GCC: {} (e.g., apt install gcc-{} on Debian/Ubuntu)\n\
         2. Clang with LLD: Install clang and lld packages\n\
         3. Zig: Install zig (includes cross-compilation support)\n\n\
         Alternatively, use --object to generate object file only and link manually.",
        target_triple,
        last_error,
        target_prefix
            .as_ref()
            .map(|p| format!("{}-gcc", p))
            .unwrap_or_else(|_| "target-gcc".to_string()),
        target_prefix.unwrap_or_else(|_| "target".to_string())
    ))
}

/// Get the appropriate dynamic linker for Linux targets
fn get_linux_dynamic_linker(target_triple: &Triple) -> Result<String, String> {
    match target_triple.architecture {
        target_lexicon::Architecture::X86_64 => Ok("/lib64/ld-linux-x86-64.so.2".to_string()),
        target_lexicon::Architecture::Aarch64(_) => Ok("/lib/ld-linux-aarch64.so.1".to_string()),
        target_lexicon::Architecture::Arm(_) => Ok("/lib/ld-linux-armhf.so.3".to_string()),
        target_lexicon::Architecture::X86_32(_) => Ok("/lib/ld-linux.so.2".to_string()),
        _ => Err(format!(
            "Unsupported Linux architecture: {}",
            target_triple.architecture
        )),
    }
}

/// Compile the runtime C library to an object file
pub(crate) fn get_static_runtime_lib(target_triple: &Triple) -> Result<std::path::PathBuf, String> {
    // Determine library name based on the target OS
    let target_triple_str = target_triple.to_string();
    let lib_name = match target_triple.operating_system {
        target_lexicon::OperatingSystem::Windows => "adeshlang.lib",
        _ => "libadeshlang.a",
    };

    let mut searched_paths = Vec::new();

    // Determine if we are running in development / Cargo environment
    let is_dev_env = std::env::var("CARGO_MANIFEST_DIR").is_ok()
        || std::env::current_exe()
            .map_or(false, |p| p.components().any(|c| c.as_os_str() == "target"))
        || std::path::Path::new("Cargo.toml").exists();

    // Helper closure to search project target directories
    let search_project_roots =
        |searched: &mut Vec<std::path::PathBuf>| -> Option<std::path::PathBuf> {
            let mut roots = Vec::new();
            if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
                roots.push(std::path::PathBuf::from(manifest_dir));
            }
            if let Ok(exe_path) = std::env::current_exe() {
                let mut curr = exe_path.parent();
                while let Some(dir) = curr {
                    if dir.join("Cargo.toml").exists() {
                        roots.push(dir.to_path_buf());
                        break;
                    }
                    if dir.file_name().map_or(false, |n| n == "target") {
                        if let Some(parent) = dir.parent() {
                            roots.push(parent.to_path_buf());
                        }
                        break;
                    }
                    curr = dir.parent();
                }
            }
            if std::path::Path::new("Cargo.toml").exists() {
                if let Ok(cwd) = std::env::current_dir() {
                    roots.push(cwd);
                }
            }

            for root in roots {
                let target_dir = root.join("target");
                let profile_order = if cfg!(debug_assertions) {
                    ["debug", "release"]
                } else {
                    ["release", "debug"]
                };
                let mut target_candidates = Vec::new();
                for profile in profile_order {
                    target_candidates.push(
                        target_dir
                            .join(&target_triple_str)
                            .join(profile)
                            .join(lib_name),
                    );
                    target_candidates.push(target_dir.join(profile).join(lib_name));
                }
                for cand in target_candidates {
                    if cand.exists() {
                        return Some(cand);
                    }
                    searched.push(cand);
                }
            }
            None
        };

    // 1. In dev/source checkout mode, check Cargo target/ directory FIRST
    if is_dev_env {
        if let Some(cand) = search_project_roots(&mut searched_paths) {
            return Ok(cand);
        }
    }

    // 2. Check executable-relative paths
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let is_bin_dir = exe_dir.file_name().and_then(|n| n.to_str()) == Some("bin");
            let install_root = if is_bin_dir {
                exe_dir.parent().unwrap_or(exe_dir)
            } else {
                exe_dir
            };

            let candidates = [
                install_root
                    .join("lib")
                    .join(&target_triple_str)
                    .join(lib_name),
                install_root.join("lib").join(lib_name),
                exe_dir.join(lib_name),
                install_root.join(lib_name),
            ];
            for cand in candidates {
                if cand.exists() {
                    return Ok(cand);
                }
                searched_paths.push(cand);
            }
        }
    }

    // 3. Check ADESH_HOME / ADESHLANG_HOME environment variable
    for env_var in ["ADESH_HOME", "ADESHLANG_HOME"] {
        if let Ok(home_str) = std::env::var(env_var) {
            let home = std::path::PathBuf::from(home_str);
            let candidates = [
                home.join("lib").join(&target_triple_str).join(lib_name),
                home.join("lib").join(lib_name),
                home.join("bin").join(lib_name),
                home.join(lib_name),
            ];
            for cand in candidates {
                if cand.exists() {
                    return Ok(cand);
                }
                searched_paths.push(cand);
            }
        }
    }

    // 4. Fallback search for Cargo build / source checkout directories (if not already checked)
    if !is_dev_env {
        if let Some(cand) = search_project_roots(&mut searched_paths) {
            return Ok(cand);
        }
    }

    // Format a helpful error message
    let mut searched_display = String::new();
    for path in &searched_paths {
        searched_display.push_str(&format!("  {}\n", path.display()));
    }

    Err(format!(
        "Static runtime library ({lib_name}) not found for target {target_triple_str}.\n\n\
        Searched in:\n{searched_display}\n\
        To resolve this:\n\
        • If using an installed AdeshLang, run `adl doctor` or `adl repair` to check/repair the installation.\n\
        • If developing AdeshLang, build the library with: `cargo build --release --target {target_triple_str}`"
    ))
}

// Helper functions

/// Try a specific compiler with given arguments
fn try_compiler(compiler: &str, args: &[String]) -> Result<(), String> {
    let result = Command::new(compiler).args(args).output();

    match result {
        Ok(output) => {
            if output.status.success() {
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(format!("{} failed: {}", compiler, stderr))
            }
        }
        Err(e) => Err(format!("{} not found: {}", compiler, e)),
    }
}

/// Get linker arguments for a specific target (with runtime library)
#[allow(dead_code)]
fn get_linker_args_for_target_with_runtime(
    target_triple: &Triple,
    obj_path: &Path,
    runtime_obj: &Path,
    output: &Path,
) -> Vec<String> {
    let mut args = vec![
        "-o".to_string(),
        output.to_string_lossy().to_string(),
        obj_path.to_string_lossy().to_string(),
        runtime_obj.to_string_lossy().to_string(),
    ];

    // Add platform-specific libraries
    match target_triple.operating_system {
        target_lexicon::OperatingSystem::Linux => {
            args.push("-lc".to_string());
            // Add dynamic linker for fully static builds
            if let Ok(linker) = get_linux_dynamic_linker(target_triple) {
                args.push("-Wl,--dynamic-linker".to_string());
                args.push(format!("-Wl,{}", linker));
            }
        }
        target_lexicon::OperatingSystem::Windows => {
            args.push("-lkernel32".to_string());
            args.push("-lmsvcrt".to_string());
        }
        target_lexicon::OperatingSystem::MacOSX { .. }
        | target_lexicon::OperatingSystem::Darwin => {
            args.push("-lSystem".to_string());
        }
        _ => {}
    }

    args
}

/// Get linker arguments for a specific target
#[allow(dead_code)]
fn get_linker_args_for_target(
    target_triple: &Triple,
    obj_path: &Path,
    output: &Path,
) -> Vec<String> {
    let mut args = vec![
        "-o".to_string(),
        output.to_string_lossy().to_string(),
        obj_path.to_string_lossy().to_string(),
    ];

    // Add platform-specific libraries
    match target_triple.operating_system {
        target_lexicon::OperatingSystem::Linux => {
            args.push("-lc".to_string());
            // Add dynamic linker for fully static builds
            if let Ok(linker) = get_linux_dynamic_linker(target_triple) {
                args.push("-Wl,--dynamic-linker".to_string());
                args.push(format!("-Wl,{}", linker));
            }
        }
        target_lexicon::OperatingSystem::Windows => {
            args.push("-lkernel32".to_string());
            args.push("-lmsvcrt".to_string());
        }
        target_lexicon::OperatingSystem::MacOSX { .. }
        | target_lexicon::OperatingSystem::Darwin => {
            args.push("-lSystem".to_string());
        }
        _ => {}
    }

    args
}

/// Get Zig target triple format
fn get_zig_target(target_triple: &Triple) -> Option<String> {
    let arch = match target_triple.architecture {
        target_lexicon::Architecture::X86_64 => "x86_64",
        target_lexicon::Architecture::Aarch64(_) => "aarch64",
        target_lexicon::Architecture::Arm(_) => "arm",
        target_lexicon::Architecture::X86_32(_) => "x86",
        _ => return None,
    };

    let os = match target_triple.operating_system {
        target_lexicon::OperatingSystem::Linux => "linux",
        target_lexicon::OperatingSystem::Windows => "windows",
        target_lexicon::OperatingSystem::MacOSX { .. }
        | target_lexicon::OperatingSystem::Darwin => "macos",
        _ => return None,
    };

    let abi = match target_triple.operating_system {
        target_lexicon::OperatingSystem::Linux => "gnu",
        target_lexicon::OperatingSystem::Windows => "gnu", // MinGW ABI
        _ => "none",
    };

    Some(format!("{}-{}-{}", arch, os, abi))
}

/// Get target prefix for cross-compilers (e.g., "x86_64-linux-gnu")
fn get_target_prefix(target_triple: &Triple) -> Result<String, String> {
    match (target_triple.architecture, target_triple.operating_system) {
        (target_lexicon::Architecture::X86_64, target_lexicon::OperatingSystem::Linux) => {
            Ok("x86_64-linux-gnu".to_string())
        }
        (target_lexicon::Architecture::X86_64, target_lexicon::OperatingSystem::Windows) => {
            Ok("x86_64-w64-mingw32".to_string())
        }
        (target_lexicon::Architecture::Aarch64(_), target_lexicon::OperatingSystem::Linux) => {
            Ok("aarch64-linux-gnu".to_string())
        }
        (target_lexicon::Architecture::Arm(_), target_lexicon::OperatingSystem::Linux) => {
            Ok("arm-linux-gnueabihf".to_string())
        }
        (target_lexicon::Architecture::X86_64, target_lexicon::OperatingSystem::MacOSX { .. }) => {
            // macOS cross-compilation is complex, may need osxcross or similar
            Err("macOS cross-compilation requires specialized toolchain".to_string())
        }
        (
            target_lexicon::Architecture::Aarch64(_),
            target_lexicon::OperatingSystem::MacOSX { .. },
        ) => {
            // iOS on ARM64
            Err("iOS cross-compilation requires specialized toolchain".to_string())
        }
        _ => Err(format!(
            "Unsupported cross-compilation target: {}",
            target_triple
        )),
    }
}

/// Dynamically find active Windows system drive roots (e.g., C:\, current drive)
fn get_system_drive_roots() -> Vec<std::path::PathBuf> {
    let mut roots = Vec::new();
    if let Ok(sys_drive) = std::env::var("SystemDrive") {
        let trimmed = sys_drive.trim_end_matches('\\');
        roots.push(std::path::PathBuf::from(format!("{}\\", trimmed)));
    }
    let c_drive = std::path::PathBuf::from("C:\\");
    if !roots.contains(&c_drive) && c_drive.exists() {
        roots.push(c_drive);
    }
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(prefix) = cwd.components().next() {
            let p = std::path::PathBuf::from(format!(
                "{}\\",
                prefix.as_os_str().to_string_lossy().trim_end_matches('\\')
            ));
            if !roots.contains(&p) && p.exists() {
                roots.push(p);
            }
        }
    }
    roots
}

/// Dynamically find all Program Files roots across all drives and environment variables
fn get_program_files_roots() -> Vec<std::path::PathBuf> {
    let mut roots = Vec::new();
    for var in [
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramW6432",
        "ProgramData",
        "LOCALAPPDATA",
    ] {
        if let Ok(val) = std::env::var(var) {
            let p = std::path::PathBuf::from(val);
            if p.exists() && !roots.contains(&p) {
                roots.push(p);
            }
        }
    }
    for drive in get_system_drive_roots() {
        for name in ["Program Files", "Program Files (x86)", "ProgramData"] {
            let p = drive.join(name);
            if p.exists() && !roots.contains(&p) {
                roots.push(p);
            }
        }
    }
    roots
}

/// Find Windows SDK library paths (returns (um path, ucrt path) if found)
pub fn find_windows_sdk_paths() -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    if let Some(cached) = TOOLCHAIN_CACHE.get() {
        if let Some((ref um, ref ucrt)) = cached.windows_sdk {
            if um.exists() && ucrt.exists() {
                return Some((um.clone(), ucrt.clone()));
            }
        }
    }

    use std::fs;

    // 1. Check environment variables (WindowsSdkDir, UniversalCRTSdkDir)
    for env_var in [
        "WindowsSdkDir",
        "UniversalCRTSdkDir",
        "WindowsSDKLibVersion",
    ] {
        if let Ok(sdk_dir) = std::env::var(env_var) {
            let sdk_version = std::env::var("WindowsSDKVersion").unwrap_or_default();
            let version_trimmed = sdk_version.trim_end_matches('\\');
            let sdk_base = std::path::PathBuf::from(&sdk_dir);
            let lib_dir = if sdk_base.join("Lib").exists() {
                sdk_base.join("Lib")
            } else {
                sdk_base.clone()
            };

            if !version_trimmed.is_empty() {
                let um_path = lib_dir.join(version_trimmed).join("um").join("x64");
                let ucrt_path = lib_dir.join(version_trimmed).join("ucrt").join("x64");
                if um_path.exists() && ucrt_path.exists() {
                    return Some((um_path, ucrt_path));
                }
            }

            // Search any subdirectories under lib_dir
            if let Ok(rd) = fs::read_dir(&lib_dir) {
                let mut versions: Vec<_> = rd
                    .filter_map(|entry| entry.ok())
                    .filter(|entry| entry.path().is_dir())
                    .filter_map(|entry| entry.file_name().to_str().map(|s| s.to_string()))
                    .collect();
                versions.sort();
                versions.reverse();
                for version in versions {
                    let um_path = lib_dir.join(&version).join("um").join("x64");
                    let ucrt_path = lib_dir.join(&version).join("ucrt").join("x64");
                    if um_path.exists() && ucrt_path.exists() {
                        return Some((um_path, ucrt_path));
                    }
                }
            }
        }
    }

    // 2. Search dynamically discovered Program Files & Drive roots
    let mut sdk_bases = Vec::new();
    for pf in get_program_files_roots() {
        let cand = pf.join("Windows Kits").join("10").join("Lib");
        if cand.exists() && !sdk_bases.contains(&cand) {
            sdk_bases.push(cand);
        }
    }
    for drive in get_system_drive_roots() {
        let cand = drive.join("Windows Kits").join("10").join("Lib");
        if cand.exists() && !sdk_bases.contains(&cand) {
            sdk_bases.push(cand);
        }
    }

    for sdk_base in &sdk_bases {
        // Find the latest SDK version
        let mut versions: Vec<_> = match fs::read_dir(sdk_base) {
            Ok(rd) => rd
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.path().is_dir())
                .filter_map(|entry| entry.file_name().to_str().map(|s| s.to_string()))
                .collect(),
            Err(_) => continue,
        };

        versions.sort();
        versions.reverse(); // Get latest version first

        for version in versions {
            let um_path = sdk_base.join(&version).join("um").join("x64");
            let ucrt_path = sdk_base.join(&version).join("ucrt").join("x64");

            if um_path.exists() && ucrt_path.exists() {
                return Some((um_path, ucrt_path));
            }
        }
    }

    None
}

/// Helper to extract the latest MSVC lib\x64 path from a VC\Tools\MSVC directory
fn find_latest_msvc_in_tools_dir(vc_tools_base: &std::path::Path) -> Option<std::path::PathBuf> {
    if !vc_tools_base.exists() {
        return None;
    }
    let mut versions: Vec<_> = match std::fs::read_dir(vc_tools_base) {
        Ok(rd) => rd
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .filter_map(|entry| entry.file_name().to_str().map(|s| s.to_string()))
            .collect(),
        Err(_) => return None,
    };

    versions.sort();
    versions.reverse();

    for version in versions {
        let lib_path = vc_tools_base.join(&version).join("lib").join("x64");
        if lib_path.exists() {
            return Some(lib_path);
        }
    }
    None
}

/// Find MSVC library paths
pub fn find_msvc_lib_paths() -> Option<std::path::PathBuf> {
    if let Some(cached) = TOOLCHAIN_CACHE.get() {
        if let Some(ref msvc) = cached.msvc_lib {
            if msvc.exists() {
                return Some(msvc.clone());
            }
        }
    }

    // 1. Check VCToolsInstallDir environment variable
    if let Ok(vctools) = std::env::var("VCToolsInstallDir") {
        let base = std::path::PathBuf::from(vctools.trim_end_matches('\\'));
        let lib_path = base.join("lib").join("x64");
        if lib_path.exists() {
            return Some(lib_path);
        }
        if base.ends_with("x64") && base.exists() {
            return Some(base);
        }
    }

    // 2. Check VSINSTALLDIR environment variable
    if let Ok(vsdir) = std::env::var("VSINSTALLDIR") {
        let vc_tools_base = std::path::PathBuf::from(vsdir.trim_end_matches('\\'))
            .join("VC")
            .join("Tools")
            .join("MSVC");
        if let Some(lib_path) = find_latest_msvc_in_tools_dir(&vc_tools_base) {
            return Some(lib_path);
        }
    }

    // 3. Query Microsoft's vswhere.exe if available (searched dynamically across all roots and PATH)
    let mut vswhere_candidates = Vec::new();
    for pf in get_program_files_roots() {
        vswhere_candidates.push(
            pf.join("Microsoft Visual Studio")
                .join("Installer")
                .join("vswhere.exe"),
        );
    }
    vswhere_candidates.push(std::path::PathBuf::from("vswhere.exe"));

    for vswhere in &vswhere_candidates {
        let res = Command::new(vswhere)
            .args([
                "-latest",
                "-products",
                "*",
                "-requires",
                "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
                "-property",
                "installationPath",
            ])
            .output();
        if let Ok(out) = res {
            if out.status.success() {
                let install_path = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !install_path.is_empty() {
                    let vc_tools_base = std::path::PathBuf::from(install_path)
                        .join("VC")
                        .join("Tools")
                        .join("MSVC");
                    if let Some(lib_path) = find_latest_msvc_in_tools_dir(&vc_tools_base) {
                        return Some(lib_path);
                    }
                }
            }
        }
    }

    // 4. Check dynamically discovered filesystem roots across all drives and versions
    let vs_years = ["2022", "2019", "2017", "2026", "2025"];
    let editions = [
        "BuildTools",
        "Community",
        "Professional",
        "Enterprise",
        "Preview",
    ];

    let mut vs_parent_dirs = Vec::new();
    for pf in get_program_files_roots() {
        let cand = pf.join("Microsoft Visual Studio");
        if cand.exists() && !vs_parent_dirs.contains(&cand) {
            vs_parent_dirs.push(cand);
        }
    }
    for drive in get_system_drive_roots() {
        let cand = drive.join("Microsoft Visual Studio");
        if cand.exists() && !vs_parent_dirs.contains(&cand) {
            vs_parent_dirs.push(cand);
        }
    }

    for vs_parent in vs_parent_dirs {
        for year in &vs_years {
            for edition in &editions {
                let vc_tools_base = vs_parent
                    .join(year)
                    .join(edition)
                    .join("VC")
                    .join("Tools")
                    .join("MSVC");

                if let Some(lib_path) = find_latest_msvc_in_tools_dir(&vc_tools_base) {
                    return Some(lib_path);
                }
            }
        }
    }

    None
}
