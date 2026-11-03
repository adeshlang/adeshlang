//! Linking functions for combining object files into executables

use std::path::Path;
use std::process::Command;
use target_lexicon::Triple;

use crate::backends::aot::cranelift::{AotOptions, OutputFormat};

/// Adesh-level directives that select the external LLVM linker. They are not
/// real linker options, so they must be stripped before forwarding
/// `extra_linker_args` to clang/lld.
const EXTERNAL_LINKER_TRIGGER_ARGS: &[&str] = &[
    "--external-linker",
    "--external-toolchain",
    "--use-llvm",
    "-fuse-ld=lld",
];

pub(crate) fn is_external_linker_trigger(arg: &str) -> bool {
    EXTERNAL_LINKER_TRIGGER_ARGS.contains(&arg)
}

fn wants_external_linker(options: &AotOptions) -> bool {
    options
        .extra_linker_args
        .iter()
        .any(|a| is_external_linker_trigger(a))
}

/// `extra_linker_args` with the external-linker directives removed.
fn forwarded_linker_args(options: &AotOptions) -> impl Iterator<Item = &String> {
    options
        .extra_linker_args
        .iter()
        .filter(|a| !is_external_linker_trigger(a))
}

/// Link object files using self-contained native Adesh Linker (primary default)
pub(crate) fn link_with_linker_driver(
    options: &AotOptions,
    obj_path: &Path,
    runtime_obj: &Path,
    output: &Path,
    target_triple: &Triple,
) -> Result<(), String> {
    // Check if user explicitly requested external toolchain mode
    if wants_external_linker(options) {
        return link_with_llvm_toolchain(
            options,
            obj_path,
            runtime_obj,
            output,
            target_triple,
            &options.output_format,
        );
    }

    // Default: Use self-contained native Adesh Linker (zero external dependencies).
    // NEVER silently fall back to LLVM/GCC/Clang on failure.
    match link_with_adesh_linker(options, obj_path, Some(runtime_obj), output, target_triple) {
        Ok(()) => {
            eprintln!("   ✓ Linked with Adesh Native Linker (zero LLVM/GCC dependencies)");
            Ok(())
        }
        Err(linker_err) => Err(format!(
            "ERROR:\nNative Adesh linker failed: {}\n\n\
                Use `--external-linker` (or legacy `--external-toolchain`) if you explicitly \
                wish to invoke an external compiler/linker.",
            linker_err
        )),
    }
}

/// Link pure library only (without runtime)
pub(crate) fn link_library_only(
    options: &AotOptions,
    obj_path: &Path,
    output: &Path,
    target_triple: &Triple,
) -> Result<(), String> {
    // Native adeshlink archiver/linker is the default for library outputs.
    // The external LLVM path (llvm-ar, clang + lld) is only used when the
    // user explicitly requested it with `--external-linker`.
    if !wants_external_linker(options) {
        return link_with_adesh_linker(options, obj_path, None, output, target_triple).map_err(
            |linker_err| {
                format!(
                    "ERROR:\nNative Adesh linker failed: {}\n\n\
                     Use `--external-linker` (or legacy `--external-toolchain`) if you explicitly \
                     wish to invoke an external compiler/linker.",
                    linker_err
                )
            },
        );
    }

    // Explicit external LLVM toolchain mode (llvm-ar / clang + lld).
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
            for arg in forwarded_linker_args(options) {
                cmd.arg(arg);
            }
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
            cmd.arg("/DEBUG");
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
    for arg in forwarded_linker_args(options) {
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
    for arg in forwarded_linker_args(options) {
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

static RUNTIME_LIB_CACHE: std::sync::Mutex<Option<(String, std::path::PathBuf)>> =
    std::sync::Mutex::new(None);

/// Archive names a runtime staticlib can take.
///
/// `adesh_runtime.*` is `crates/adesh-runtime`'s Cargo `staticlib` output — the
/// archive a per-target build produces.
///
/// The pre-split installs named `adeshlang.*`, and it is not offered: the root
/// `adeshlang` crate is `crate-type = ["rlib", "cdylib"]`, so no staticlib of
/// that name can exist, and the hashed-name search matches the cdylib's COFF
/// import library instead (`target/debug/deps/adeshlang-<hash>.lib`). An import
/// library is a well-formed archive that defines no `aot_*` symbol, so a link
/// that picks one reports success resolving the runtime, then fails on
/// `undefined reference to aot_print_cstr` — a worse diagnosis than "no runtime
/// archive", which is why the name is dropped rather than kept as a fallback.
pub(crate) fn runtime_archive_names(is_win: bool) -> [&'static str; 1] {
    if is_win {
        ["adesh_runtime.lib"]
    } else {
        ["libadesh_runtime.a"]
    }
}

/// Whether `target` names the machine this process runs on.
///
/// The libc flavour is part of the answer: a glibc and a musl archive are both
/// x86-64 ELF, but a musl image needs `libc.musl-x86_64.so.1` as its `DT_NEEDED`
/// and glibc symbols carry version tags, so trading one for the other fails at
/// load time — after a link that reported success.
fn is_host_triple(target: &Triple) -> bool {
    let host = Triple::host();
    target.architecture == host.architecture
        && target.operating_system == host.operating_system
        && target.environment == host.environment
}

/// Everything resolution needs, passed as data rather than read from inside the
/// search, so the order a triple is looked up in is checkable without holding
/// archives for four targets on one machine.
struct RuntimeArchiveSearch<'a> {
    triple: &'a str,
    is_host: bool,
    is_win: bool,
    profile_order: &'a [&'a str],
    project_roots: &'a [std::path::PathBuf],
    install_roots: &'a [std::path::PathBuf],
    runtime_dir: Option<&'a std::path::Path>,
}

impl RuntimeArchiveSearch<'_> {
    /// The locations the runtime archive may be read from, in search order.
    ///
    /// A foreign triple is offered only paths that name its own triple.
    /// `target/debug/`, an install `lib/` and a flat `ADESH_RUNTIME_DIR` each
    /// hold the archive of whichever machine built them; a cross build that
    /// consumes one links a host-arch runtime into a foreign image, the link
    /// succeeds, and the artifact cannot run.
    fn candidates(&self) -> Vec<std::path::PathBuf> {
        let names = runtime_archive_names(self.is_win);
        let mut candidates = Vec::new();

        // 1. Per-target Cargo output, which puts `release` in the list even for
        //    a debug toolchain: a cross archive is built deliberately.
        for root in self.project_roots {
            let triple_dir = root.join("target").join(self.triple);
            for profile in self.profile_order {
                push_archive_dir(&mut candidates, &triple_dir.join(profile), &names, true);
            }
        }

        // 2. Per-target archives bundled with an installation.
        for root in self.install_roots {
            push_archive_dir(
                &mut candidates,
                &root.join("lib").join(self.triple),
                &names,
                false,
            );
        }

        // 3. An explicit runtime directory, organised per target.
        if let Some(dir) = self.runtime_dir {
            push_archive_dir(&mut candidates, &dir.join(self.triple), &names, false);
        }

        // 4. Host-only locations, reachable only when this is the host triple.
        if self.is_host {
            for root in self.project_roots {
                let target_dir = root.join("target");
                for profile in self.profile_order {
                    push_archive_dir(&mut candidates, &target_dir.join(profile), &names, true);
                }
            }
            for root in self.install_roots {
                push_archive_dir(&mut candidates, &root.join("lib"), &names, false);
                push_archive_dir(&mut candidates, root, &names, false);
            }
            if let Some(dir) = self.runtime_dir {
                push_archive_dir(&mut candidates, dir, &names, false);
            }
        }
        candidates
    }

    /// The command that produces the missing archive, named in the error so a
    /// cross build failure is actionable rather than a search-path dump.
    ///
    /// The profile asked for is the one this toolchain searches first, so a
    /// debug `adesh` is not told to run a slow release build for an archive it
    /// would have taken from `target/<triple>/debug/`. The script, not raw
    /// `cargo build`, is what puts the archive at the path being searched.
    fn build_command(&self) -> String {
        let profile = self.profile_order.first().copied().unwrap_or("debug");
        format!(
            "scripts/build-runtime-archives.sh --profile {profile} --targets {}",
            self.triple
        )
    }
}

/// `dir` in each of the names an archive can appear under, plus — when the
/// directory is a Cargo profile dir — the hashed copies Cargo gives to
/// dependency staticlibs (`adesh_runtime-<hash>.a`).
fn push_archive_dir(
    out: &mut Vec<std::path::PathBuf>,
    dir: &Path,
    names: &[&str],
    with_deps: bool,
) {
    for name in names {
        out.push(dir.join(name));
    }
    if with_deps {
        let deps = dir.join("deps");
        for name in names {
            out.push(deps.join(name));
            out.extend(hashed_runtime_libs(&deps, name));
        }
    }
}

/// Directories that may hold a Cargo `target/` tree: a source checkout or a
/// `cargo run` of the CLI. Empty for an installed toolchain.
fn runtime_project_roots() -> Vec<std::path::PathBuf> {
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
            if dir.file_name().is_some_and(|n| n == "target") {
                if let Some(parent) = dir.parent() {
                    roots.push(parent.to_path_buf());
                }
                break;
            }
            curr = dir.parent();
        }
    }
    if Path::new("Cargo.toml").exists()
        && let Ok(cwd) = std::env::current_dir()
    {
        roots.push(cwd);
    }
    roots
}

/// Directories an installed toolchain may ship its archives from: next to the
/// executable, the install root above a `bin/`, and `ADESH_HOME` /
/// `ADESHLANG_HOME` with their `bin/` siblings.
fn runtime_install_roots() -> Vec<std::path::PathBuf> {
    let mut roots = Vec::new();
    if let Ok(exe_path) = std::env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
    {
        roots.push(exe_dir.to_path_buf());
        if exe_dir.file_name().is_some_and(|n| n == "bin")
            && let Some(install_root) = exe_dir.parent()
        {
            roots.push(install_root.to_path_buf());
        }
    }
    for env_var in ["ADESH_HOME", "ADESHLANG_HOME"] {
        if let Ok(home) = std::env::var(env_var) {
            let home = std::path::PathBuf::from(home);
            roots.push(home.join("bin"));
            roots.push(home);
        }
    }
    roots
}

/// A memoised resolution counts only for the triple that produced it.
///
/// The cache is process-wide, so without the triple in the key a host build
/// followed by a cross build in one process hands the cross build the host
/// archive it resolved moments earlier — no search path guards against that.
fn cached_archive_matches(cached: &(String, std::path::PathBuf), triple: &str) -> bool {
    cached.0 == triple && cached.1.is_file()
}

/// Find the static runtime archive that a native link pulls in.
///
/// The archive is target-specific: it holds the C and Rust objects that
/// `aot_*` calls bind to, so an image built for `aarch64-unknown-linux-gnu`
/// needs an `aarch64` archive. Resolution therefore searches every location
/// that names the requested triple before it looks anywhere else, and consults
/// the host's own build tree only when the request *is* the host.
pub(crate) fn get_static_runtime_lib(target_triple: &Triple) -> Result<std::path::PathBuf, String> {
    let target_triple_str = target_triple.to_string();
    let is_win = matches!(
        target_triple.operating_system,
        target_lexicon::OperatingSystem::Windows
    );
    let is_host = is_host_triple(target_triple);

    {
        let cache = RUNTIME_LIB_CACHE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(cached) = cache.as_ref()
            && cached_archive_matches(cached, &target_triple_str)
        {
            return Ok(cached.1.clone());
        }
    }

    let profile_order: Vec<&str> = if cfg!(debug_assertions) {
        vec!["debug", "release"]
    } else {
        vec!["release", "debug"]
    };
    let project_roots = runtime_project_roots();
    let install_roots = runtime_install_roots();
    let runtime_dir = std::env::var_os("ADESH_RUNTIME_DIR").map(std::path::PathBuf::from);

    let search = RuntimeArchiveSearch {
        triple: &target_triple_str,
        is_host,
        is_win,
        profile_order: &profile_order,
        project_roots: &project_roots,
        install_roots: &install_roots,
        runtime_dir: runtime_dir.as_deref(),
    };

    let mut searched_paths = Vec::new();
    for candidate in dedupe(search.candidates()) {
        if candidate.is_file() {
            let mut cache = RUNTIME_LIB_CACHE
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *cache = Some((target_triple_str.clone(), candidate.clone()));
            return Ok(candidate);
        }
        searched_paths.push(candidate);
    }

    Err(runtime_archive_error(&search, &searched_paths))
}

/// Deduplicate candidates, preserving search order.
///
/// An install root and an exe-relative root frequently name the same directory,
/// and a repeated path only lengthens the error a failed cross build prints.
fn dedupe(paths: Vec<std::path::PathBuf>) -> Vec<std::path::PathBuf> {
    let mut seen = std::collections::HashSet::new();
    paths
        .into_iter()
        .filter(|path| seen.insert(path.clone()))
        .collect()
}

/// Why no archive was found, with every path that was asked and the one command
/// that would have made it exist.
fn runtime_archive_error(
    search: &RuntimeArchiveSearch<'_>,
    searched_paths: &[std::path::PathBuf],
) -> String {
    let mut message = format!(
        "No runtime archive for target {}.\n\n\
         A native link needs the `adesh-runtime` staticlib built for the target \
         it is linking. Searched:\n",
        search.triple
    );
    for path in searched_paths {
        message.push_str(&format!("  {}\n", path.display()));
    }
    message.push_str(&format!("\nBuild it with:\n  {}\n", search.build_command()));

    if !search.is_host {
        message.push_str(&format!(
            "\n{} is a cross build, so nothing built for this machine ({}) was \
             considered: a runtime archive for one architecture cannot be linked into \
             an image for another.\n",
            search.triple,
            Triple::host()
        ));
    }
    message.push_str(
        "\nAn installed toolchain may already ship per-target archives; point \
         ADESH_RUNTIME_DIR at that directory or run `adl repair`.",
    );
    message
}

fn hashed_runtime_libs(deps_dir: &Path, primary_lib_name: &str) -> Vec<std::path::PathBuf> {
    let primary_path = Path::new(primary_lib_name);
    let Some(stem) = primary_path.file_stem().and_then(|stem| stem.to_str()) else {
        return Vec::new();
    };
    let prefix = format!("{stem}-");
    let extension = primary_path.extension();
    let mut libraries: Vec<_> = std::fs::read_dir(deps_dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            let name_matches = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .is_some_and(|stem| stem.starts_with(&prefix));
            name_matches && path.extension() == extension
        })
        .collect();
    libraries.sort_by_key(|path| {
        std::cmp::Reverse(
            path.metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
        )
    });
    libraries
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

static WIN_SDK_CACHE: std::sync::OnceLock<Option<(std::path::PathBuf, std::path::PathBuf)>> =
    std::sync::OnceLock::new();
static MSVC_LIB_CACHE: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();

/// Find Windows SDK library paths (returns (um path, ucrt path) if found)
pub fn find_windows_sdk_paths() -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    WIN_SDK_CACHE
        .get_or_init(|| {
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
                let mut versions: Vec<_> = match fs::read_dir(sdk_base) {
                    Ok(rd) => rd
                        .filter_map(|entry| entry.ok())
                        .filter(|entry| entry.path().is_dir())
                        .filter_map(|entry| entry.file_name().to_str().map(|s| s.to_string()))
                        .collect(),
                    Err(_) => continue,
                };

                versions.sort();
                versions.reverse();

                for version in versions {
                    let um_path = sdk_base.join(&version).join("um").join("x64");
                    let ucrt_path = sdk_base.join(&version).join("ucrt").join("x64");

                    if um_path.exists() && ucrt_path.exists() {
                        return Some((um_path, ucrt_path));
                    }
                }
            }

            None
        })
        .clone()
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
    MSVC_LIB_CACHE
        .get_or_init(|| {
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

            // 3. Query Microsoft's vswhere.exe if available
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

            // 4. Check filesystem roots
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
        })
        .clone()
}

/// Link object files using self-contained native Adesh Linker (zero external dependencies).
pub fn link_with_adesh_linker(
    options: &AotOptions,
    obj_path: &Path,
    runtime_obj: Option<&Path>,
    output: &Path,
    target_triple: &Triple,
) -> Result<(), String> {
    let target = adesh_linker::target::Target::from_triple(&target_triple.to_string())
        .map_err(|e| format!("Invalid target triple `{target_triple}`: {e}"))?;

    if options.output_format == OutputFormat::StaticLib {
        let mut archive = adesh_linker::archive::Archive::new();
        let data = std::fs::read(obj_path)
            .map_err(|e| format!("Failed to read object file `{}`: {e}", obj_path.display()))?;
        let filename = obj_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("obj.o");
        archive.add_file(filename, data);
        if let Some(rt) = runtime_obj {
            if rt.exists() {
                let rt_data = std::fs::read(rt).map_err(|e| {
                    format!("Failed to read runtime object `{}`: {e}", rt.display())
                })?;
                let rt_name = rt
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("runtime.o");
                archive.add_file(rt_name, rt_data);
            }
        }
        let ar_bytes = archive.encode_gnu();
        std::fs::write(output, ar_bytes)
            .map_err(|e| format!("Failed to write static library `{}`: {e}", output.display()))?;
        return Ok(());
    }

    let mut config = adesh_linker::config::LinkConfig::new(output.to_path_buf(), target);
    let opt_level = match options.opt_level {
        0 => adesh_linker::config::OptLevel::O0,
        1 => adesh_linker::config::OptLevel::O1,
        2 => adesh_linker::config::OptLevel::O2,
        _ => adesh_linker::config::OptLevel::O3,
    };
    config.apply_optimization_level(opt_level);
    config.gc_sections &= options.enable_dead_code_elimination;
    config.lto = if options.enable_lto {
        adesh_linker::config::LtoMode::Thin
    } else {
        adesh_linker::config::LtoMode::Off
    };
    config.strip_debug = !options.debug_info;
    config.shared = options.output_format == OutputFormat::SharedLib;
    config.incremental = options.incremental;
    if let Some(ref cache) = options.cache_dir {
        config.cache_dir = Some(cache.clone());
    }
    for dir in &options.lib_dirs {
        config
            .library_search_paths
            .push(std::path::PathBuf::from(dir));
    }
    for lib in &options.link_libs {
        config.libraries.push(lib.clone());
    }

    let mut inputs = vec![obj_path.to_path_buf()];
    if let Some(rt) = runtime_obj {
        if rt.exists() {
            inputs.push(rt.to_path_buf());
        }
    }

    adesh_linker::link_with_config(&inputs, config).map_err(|e| e.to_string())
}

#[cfg(test)]
mod runtime_archive_tests {
    use super::*;

    const PROFILE_ORDER: [&str; 2] = ["debug", "release"];

    fn parse_triple(triple: &str) -> Triple {
        triple.parse().expect("test triple must parse")
    }

    /// `is_host` is a parameter, not a call to `is_host_triple`: an assertion
    /// about cross builds must hold on a host that *is* the target it is
    /// cross-building for, which CI runners on aarch64 Linux are.
    fn search<'a>(
        triple: &'a str,
        is_host: bool,
        project_roots: &'a [std::path::PathBuf],
        install_roots: &'a [std::path::PathBuf],
        runtime_dir: Option<&'a std::path::Path>,
    ) -> RuntimeArchiveSearch<'a> {
        let target = parse_triple(triple);
        RuntimeArchiveSearch {
            triple,
            is_host,
            is_win: matches!(
                target.operating_system,
                target_lexicon::OperatingSystem::Windows
            ),
            profile_order: &PROFILE_ORDER,
            project_roots,
            install_roots,
            runtime_dir,
        }
    }

    fn roots(dir: &str) -> Vec<std::path::PathBuf> {
        vec![std::path::PathBuf::from(dir)]
    }

    /// The rule the whole phase rests on: a cross build is never offered a
    /// directory that does not name the target it is building.
    #[test]
    fn cross_builds_are_offered_only_paths_that_name_their_own_triple() {
        let project = roots("/src/adesh");
        let install = roots("/opt/adesh");
        let runtime_dir = std::path::Path::new("/runtime");
        let cross = search(
            "aarch64-unknown-linux-gnu",
            false,
            &project,
            &install,
            Some(runtime_dir),
        );
        let candidates = cross.candidates();

        assert!(!candidates.is_empty(), "a cross build must still search");
        for candidate in &candidates {
            let path = candidate.to_string_lossy();
            assert!(
                path.contains("aarch64-unknown-linux-gnu"),
                "{path} is not scoped to the target triple; a cross build would link \
                 whatever host archive it found there"
            );
        }
        let host_tree = std::path::PathBuf::from("/src/adesh")
            .join("target")
            .join("debug");
        assert!(
            !candidates.iter().any(|c| c.starts_with(&host_tree)),
            "the host build tree must stay out of a cross build's search"
        );
    }

    #[test]
    fn a_host_build_still_finds_its_own_build_tree_and_install_lib() {
        let project = roots("/src/adesh");
        let install = roots("/opt/adesh");
        let runtime_dir = std::path::Path::new("/runtime");
        let host = search(
            "x86_64-pc-windows-msvc",
            true,
            &project,
            &install,
            Some(runtime_dir),
        );
        let candidates = host
            .candidates()
            .iter()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect::<Vec<_>>();

        for expected in [
            "/src/adesh/target/x86_64-pc-windows-msvc/debug/adesh_runtime.lib",
            "/opt/adesh/lib/x86_64-pc-windows-msvc/adesh_runtime.lib",
            "/runtime/x86_64-pc-windows-msvc/adesh_runtime.lib",
            "/src/adesh/target/debug/adesh_runtime.lib",
            "/opt/adesh/lib/adesh_runtime.lib",
            "/runtime/adesh_runtime.lib",
        ] {
            assert!(
                candidates.iter().any(|c| c == expected),
                "{expected} missing"
            );
        }
    }

    /// Ordering is part of the contract: a per-target archive must win over the
    /// host archive that sits in the same `target/` tree.
    #[test]
    fn target_specific_paths_are_searched_before_host_paths() {
        let project = roots("/src/adesh");
        let candidates = search("x86_64-unknown-linux-gnu", true, &project, &[], None)
            .candidates()
            .iter()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect::<Vec<_>>();
        let scoped = candidates
            .iter()
            .position(|c| c.contains("target/x86_64-unknown-linux-gnu/"))
            .expect("triple-scoped candidate");
        let unscoped = candidates
            .iter()
            .position(|c| c.ends_with("target/debug/libadesh_runtime.a"))
            .expect("host candidate");
        assert!(scoped < unscoped, "target-scoped must be searched first");
    }

    /// A cross archive is built deliberately, so `release` is in the list even
    /// when the toolchain running the link is a debug build.
    #[test]
    fn the_release_directory_is_searched_for_every_triple() {
        let project = roots("/src/adesh");
        for (target_triple, archive) in [
            ("aarch64-unknown-linux-gnu", "libadesh_runtime.a"),
            ("x86_64-pc-windows-msvc", "adesh_runtime.lib"),
        ] {
            let found = search(target_triple, false, &project, &[], None)
                .candidates()
                .iter()
                .any(|c| c.ends_with(format!("target/{target_triple}/release/{archive}")));
            assert!(
                found,
                "{target_triple} must be looked for under target/<triple>/release/"
            );
        }
    }

    #[test]
    fn missing_archive_errors_name_the_triple_every_path_and_the_command() {
        let project = roots("/src/adesh");
        let install = roots("/opt/adesh");
        let cross = search(
            "riscv64gc-unknown-linux-gnu",
            false,
            &project,
            &install,
            None,
        );
        let candidates = cross.candidates();
        let error = runtime_archive_error(&cross, &candidates);

        assert!(
            error.contains("riscv64gc-unknown-linux-gnu"),
            "the error must name the triple: {error}"
        );
        assert!(
            error
                .contains("scripts/build-runtime-archives.sh --profile debug --targets riscv64gc-unknown-linux-gnu"),
            "the error must give the command that builds it: {error}"
        );
        for candidate in &candidates {
            assert!(
                error.contains(&candidate.to_string_lossy().to_string()),
                "the error must list every searched path: {candidate:?}"
            );
        }
        assert!(
            error.contains("cross build"),
            "a cross failure must say so: {error}"
        );
    }

    #[test]
    fn a_host_archive_answer_never_satisfies_a_cross_build() {
        let dir = tempfile::tempdir().expect("tempdir");
        let host_archive = dir.path().join("libadesh_runtime.a");
        std::fs::write(&host_archive, "!<arch>\n").expect("write host archive");
        let host = Triple::host().to_string();
        let cached = (host.clone(), host_archive);

        assert!(
            cached_archive_matches(&cached, &host),
            "the host triple must hit the cached archive"
        );
        // Wasm32 is never the machine a process runs on, so this stays true on
        // every CI host.
        assert!(
            !cached_archive_matches(&cached, "wasm32-unknown-unknown"),
            "a different triple must not inherit the cached archive"
        );
    }

    #[test]
    fn the_host_triple_recognises_itself_and_nothing_else() {
        assert!(is_host_triple(&Triple::host()));
        // Wasm32 and MIPS are never the machine a process runs on, so these
        // stay false on every CI host.
        assert!(!is_host_triple(&parse_triple("wasm32-unknown-unknown")));
        assert!(!is_host_triple(&parse_triple(
            "mips64-unknown-linux-gnuabi64"
        )));
    }
}
